//! Passive attribution: what was open, turned into blocks nobody typed
//! (issue #282, spec #272 "Blocks"; `CONTEXT.md`'s *passive attribution*).
//!
//! [`super`] holds the *timer* -- the clock a person starts -- and
//! [`super::day`] holds the *record* read back for one day. This holds the one
//! thing in M3.1 that claims to know something the reader did not type, which
//! is why the spec's own further notes tell reviewers to look hardest here.
//!
//! # Off by default, and off means nothing is written
//!
//! One `knobas.setting` row, [`SETTING_KEY`], default `false` -- the backup
//! module's precedent, and therefore no migration for the setting itself. With
//! it off, [`observe`] writes nothing and [`materialize`] returns before it
//! reads anything: knobas records nothing, rather than recording and declining
//! to look. That is the difference between a setting and a filter, and it is
//! the whole of what "opt in" means here.
//!
//! The heartbeat is untouched by the switch. It still stamps `last_heartbeat`
//! on every beat, because that stamp is a statement about *knobas* being alive
//! and is what a stranded timer's block is closed at (#278) -- a person who
//! never turns passive attribution on must not thereby lose the relaunch rule.
//!
//! # The three rules, and why they are a pure function
//!
//! [`derive`] takes a sequence of observations and answers with spans. It
//! touches no database, no clock and no setting, and every rule the spec
//! states lives inside it:
//!
//! * **A visit shorter than [`FLOOR_SECONDS`] is dropped.** Two minutes, a
//!   named constant, because glancing at a ticket is not working on it.
//! * **Adjacent visits to one target merge.** Six observations on one ticket
//!   are one stretch, not six.
//! * **The total never exceeds focused time.** See [`derive`] for why this is
//!   load-bearing rather than theoretical: it is what makes the last visit in
//!   a run of observations end at its last beat instead of one beat window
//!   later.
//!
//! # What an observation claims, and why the cap follows from it
//!
//! An observation says *this was in the foreground at this instant*. It
//! cannot say what was open in between, so the rule is: **an observation
//! credits its target forward, for one beat window, and no further.** Silence
//! therefore stops being work [`BEAT_WINDOW_SECONDS`] after the last thing
//! knobas heard, whatever happened to the process in between.
//!
//! Focused time is the same walk read a beat later: the gap between one beat
//! and the next, clamped to the same window. So the claims in a run of
//! observations add up to its focused time **plus one window** -- the tail the
//! last beat credits forward and no later beat confirms -- and the cap is what
//! takes that tail back. It binds on every run of observations, and it binds
//! harder whenever beats arrive closer together than they are sent (a second
//! window beating, a retry, an import), which is the case in which claims
//! genuinely overlap.

use chrono::{DateTime, Duration, Utc};
use sqlx::{PgPool, Row};

use super::{TimerTarget, vet};
use crate::IpcError;

/// The `knobas.setting` key holding whether passive attribution is on.
///
/// `knobas.setting`, whose migration (`0002`, comment 6) exists for exactly
/// this, so the switch needs no migration of its own -- the reasoning
/// `backup::SCHEDULE_KEY` records. Namespaced `time.` because #283's timesheet
/// and #280's worklog will want their own.
const SETTING_KEY: &str = "time.passive_attribution";

/// How long one beat credits its target for, in seconds.
///
/// **Pinned to the interval the shell actually beats at**
/// (`app/src/lib/shell/timer.svelte.ts`'s `HEARTBEAT_MS`) by
/// `the_beat_window_is_the_interval_the_shell_beats_at`. Without that pin, a
/// shell that started beating every ten seconds would leave this crediting
/// thirty, and every visit would be quietly extended past the last beat that
/// supported it -- green here, and wrong on screen.
pub const BEAT_WINDOW_SECONDS: i64 = 30;

/// The floor: a visit shorter than this is not a visit (spec #272, "visits
/// shorter than two minutes dropped").
///
/// A constant because the spec says it is one, and because the number is the
/// only thing in this module a person might reasonably want to argue with.
pub const FLOOR_SECONDS: i64 = 120;

/// How many days of observations knobas keeps (issue #315).
///
/// Thirty, and the number is a floor plus margin rather than a preference:
///
/// * **One week timesheet read covers seven days.** [`super::week`] takes at
///   most [`MOST_DAYS`](super::week::MOST_DAYS) windows and they are one
///   week, so a Sunday read starts at the Monday before it.
/// * **The standup digest looks back seven more.** Its *yesterday* is the
///   most recent day with any activity on it, at most seven days back (#288),
///   so Monday reads Friday and a week of silence reads a week ago.
///
/// Fourteen is therefore the floor the two surfaces put under it -- an
/// over-count, since both are look-backs from today and only the longer one
/// truly binds, and taken as the floor anyway because a rule that assumed
/// they would never compose is a rule that would have to be re-derived the
/// day one of them moves. Doubled for the margin, which is what covers a
/// laptop shut for a fortnight and a surface nobody has built yet, and
/// rounded to a month so that what knobas promises can be said in a sentence:
/// **it keeps a month of observations.**
///
/// **Both numbers are the width of one read, not a bound on how far back a
/// read may sit**, and no constant here could be: `#/time/<date>` takes any
/// date, and `week::vet` bounds a timesheet's *column count* and nothing
/// about where its windows are. So this is a promise and not a proof --
/// knobas keeps a month, and a day older than that is a day past the
/// observation horizon. Past it the day review offers no passive blocks
/// and the timesheet's "no target, app open" row reads zero for that day;
/// what a surface *says* about such a day is [`Horizon`]'s, which both reads
/// carry to the strip and the timesheet in words (#337). Reading one safely is
/// [`materialize`]'s -- see [`prune`].
pub const RETENTION_DAYS: i64 = 30;

/// The `knobas.setting` key holding the instant before which observations
/// have actually been thrown away.
///
/// `knobas.setting` again, so the observation horizon needs no migration --
/// the same reasoning [`SETTING_KEY`] and `backup::SCHEDULE_KEY` record.
///
/// **Stored rather than recomputed from the clock**, and that is the whole of
/// what makes the guard in [`materialize`] safe. A guard that asked "is this
/// day older than [`RETENTION_DAYS`]" would start refusing days whose beats
/// are all still there, on any database the sweep has never run in -- every
/// test fixture, every restored archive, every profile whose owner never left
/// the app running long enough. This stamp says what knobas *did*, so a day
/// is refused when its record may actually be incomplete and never otherwise.
const PRUNED_KEY: &str = "time.observations_pruned_before";

/// One heartbeat, as passive attribution sees it.
///
/// The three facts a beat carries and nothing else: when, what was in front of
/// the reader, and whether the window was focused. Not a database row -- this
/// type is what [`derive`] takes, and the point of it is that the rules can be
/// driven from a literal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Observation {
    pub at: DateTime<Utc>,
    /// The foreground by the rule #278 ratified: open detail, else the room's
    /// anchor, else none. `None` is *the reader had nothing in front of them*,
    /// which is a legal observation and not a missing one -- it is focused
    /// time with nothing to attribute it to, and it ends whatever visit was
    /// running.
    pub foreground: Option<TimerTarget>,
    /// Whether the window was focused.
    ///
    /// Always `true` today: the shell sends no beat at all from an unfocused
    /// window, so losing focus arrives here as an *absence* of observations
    /// and [`derive`] reads that absence as the break it is. The field exists
    /// so the rule "an unfocused observation is neither time nor attribution"
    /// can be stated and tested -- the treatment `0013` gave the `passive`
    /// block kind before anything wrote one.
    pub focused: bool,
}

/// A stretch passive attribution is prepared to say something was open for.
///
/// Not a [`Block`](super::Block): it has no id, no kind and no worklog,
/// because it is what the derivation *says* rather than what the database
/// holds. [`materialize`] is the one thing that turns one into a row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PassiveSpan {
    pub started_at: DateTime<Utc>,
    pub ended_at: DateTime<Utc>,
    pub target: TimerTarget,
}

impl PassiveSpan {
    fn length(&self) -> Duration {
        self.ended_at - self.started_at
    }
}

/// A stretch the window was **focused** for, whatever was in the foreground.
///
/// Not a [`PassiveSpan`]: it carries no target, because it is the answer to a
/// different question. A passive span says *this was open*; this says *knobas
/// was being looked at*. The week timesheet's "no target, app open" row (#283)
/// is the second question minus every block, and it has to be measured the
/// same way [`derive`] measures its cap or the two surfaces would disagree
/// about the same afternoon.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FocusedSpan {
    pub started_at: DateTime<Utc>,
    pub ended_at: DateTime<Utc>,
}

impl FocusedSpan {
    fn length(self) -> Duration {
        self.ended_at - self.started_at
    }
}

/// The stretches the window was focused for, in order and never overlapping.
///
/// **The one measurement of focused time in the crate**, and both readers of
/// it are here: [`derive`] sums these into the cap it spends, and #283's week
/// read subtracts the day's blocks from them to get "app open, nothing
/// claimed". A second walk would be a second opinion about the same beats.
///
/// A beat credits the interval up to the **next** beat, clamped to one beat
/// window: a gap longer than the window is focus that was lost and came back,
/// and only the window counts -- otherwise a laptop shut at lunch would bill
/// the afternoon. The **final** beat credits nothing forward, because there is
/// no next beat to credit toward; that missing window is why a day's focused
/// time is one window shorter than the claims [`derive`] builds from the same
/// beats, and therefore why the cap binds at all.
///
/// An unfocused beat credits nothing: it is neither time nor attribution.
#[must_use]
pub fn focused_spans(observations: &[Observation]) -> Vec<FocusedSpan> {
    let window = Duration::seconds(BEAT_WINDOW_SECONDS);
    let mut beats: Vec<&Observation> = observations.iter().collect();
    beats.sort_by_key(|beat| beat.at);

    let mut spans: Vec<FocusedSpan> = Vec::new();
    for pair in beats.windows(2) {
        if !pair[0].focused {
            continue;
        }
        let span = FocusedSpan {
            started_at: pair[0].at,
            ended_at: pair[0].at + (pair[1].at - pair[0].at).min(window),
        };
        if span.length() <= Duration::zero() {
            // Two beats in the same instant. A zero-length span is not a
            // stretch and would only ever be a row of nothing to subtract.
            continue;
        }
        match spans.last_mut() {
            // Consecutive beats inside the window leave spans that butt onto
            // each other; merged so the output is the *shape* of the focused
            // time rather than one entry per beat, which is what makes
            // subtracting blocks from it readable.
            Some(open) if open.ended_at >= span.started_at => {
                open.ended_at = open.ended_at.max(span.ended_at);
            }
            _ => spans.push(span),
        }
    }
    spans
}

/// Turn a sequence of observations into the passive blocks it supports.
///
/// **Pure**: no clock, no database, no setting. Give it beats, get spans.
///
/// The walk, in the order the rules apply:
///
/// 1. **Focused time is measured first**, as the sum over consecutive beats of
///    the gap between them clamped to one beat window. A gap longer than the
///    window is focus that was lost and came back, and only the window counts
///    -- otherwise a laptop shut at lunch would bill the afternoon.
/// 2. **Each focused beat claims its target for one window forward.** Claims
///    on the same target that touch or overlap merge into one visit; a claim
///    on a *different* target truncates the visit before it at the moment the
///    new claim starts, because two things cannot both have been in the
///    foreground. A focused beat with nothing in the foreground truncates it
///    too, and starts nothing.
/// 3. **The floor drops the short visits**, before the cap sees them.
/// 4. **The cap spends focused time in order.** Each surviving visit takes
///    what is left of the budget, and a visit trimmed below the floor is not
///    offered -- nor is anything after it, since the budget is by then spent.
///
/// The cap is a rule about the **total**, which is the rule the spec states,
/// and that has a consequence worth naming: focused time nobody attributed --
/// a room with nothing open -- pays into the same budget, so a day with idle
/// stretches in it can afford the window its last beat credits forward while a
/// day without them cannot. The cap takes back what a day cannot pay for, and
/// it is not a per-visit trim.
///
/// The output is in time order and its spans never overlap.
#[must_use]
pub fn derive(observations: &[Observation]) -> Vec<PassiveSpan> {
    let window = Duration::seconds(BEAT_WINDOW_SECONDS);
    let floor = Duration::seconds(FLOOR_SECONDS);

    // Sorted here rather than trusted from the caller: `materialize` reads in
    // order, but this function is the one that states the rules and a rule
    // that held only for sorted input would be a rule with a caller in it.
    let mut beats: Vec<&Observation> = observations.iter().collect();
    beats.sort_by_key(|beat| beat.at);

    // The cap's budget, measured by the crate's one walk over focused time
    // (`focused_spans`) rather than by a second loop here: #283's week read
    // subtracts blocks from those same spans, and two walks would be two
    // opinions about one afternoon.
    let budget: Duration = focused_spans(observations)
        .iter()
        .map(|span| span.length())
        .fold(Duration::zero(), |total, length| total + length);

    let mut visits: Vec<PassiveSpan> = Vec::new();
    for beat in &beats {
        if !beat.focused {
            // Not time, and not attribution. It also ends whatever was open:
            // the window stopped being looked at, and the visit stops there.
            close(&mut visits, beat.at);
            continue;
        }
        let Some(target) = beat.foreground.clone() else {
            close(&mut visits, beat.at);
            continue;
        };
        let claim = PassiveSpan {
            started_at: beat.at,
            ended_at: beat.at + window,
            target,
        };
        match visits.last_mut() {
            // The merge rule. `>=` and not `>`: a claim that begins exactly
            // where the last one ended is the next beat of the same visit,
            // which is what an unbroken run of beats looks like.
            Some(open) if open.target == claim.target && open.ended_at >= claim.started_at => {
                open.ended_at = open.ended_at.max(claim.ended_at);
            }
            _ => {
                close(&mut visits, claim.started_at);
                visits.push(claim);
            }
        }
    }

    visits.retain(|visit| visit.length() >= floor);

    let mut offered = Vec::with_capacity(visits.len());
    let mut remaining = budget;
    for visit in visits {
        let length = visit.length().min(remaining);
        if length < floor {
            // The budget is spent. Everything after this visit would be
            // trimmed at least as hard, so there is nothing left to offer.
            break;
        }
        remaining -= length;
        offered.push(PassiveSpan {
            ended_at: visit.started_at + length,
            ..visit
        });
    }
    offered
}

/// End the visit in progress at `at`, if it ran past it.
///
/// Called wherever something says the previous target was no longer in the
/// foreground: another target, nothing at all, or an unfocused window. The
/// claim it truncates was always a forward guess bounded by the beat window,
/// and this is knobas hearing otherwise before that window ran out.
fn close(visits: &mut [PassiveSpan], at: DateTime<Utc>) {
    if let Some(open) = visits.last_mut()
        && open.ended_at > at
    {
        open.ended_at = at.max(open.started_at);
    }
}

/// Whether passive attribution is switched on. `false` when nothing is stored.
///
/// A value that no longer decodes reads as `false`, the same answer as absent
/// -- the discipline `backup::read_setting` records, resolved the other way
/// round on purpose: the backup schedule's safe failure is to keep backing up,
/// and this one's is to record nothing about a person who cannot be asked.
///
/// # Errors
/// [`IpcError`] if the read fails.
pub async fn enabled(pool: &PgPool) -> Result<bool, IpcError> {
    let stored: Option<serde_json::Value> =
        sqlx::query_scalar("select value from knobas.setting where key = $1")
            .bind(SETTING_KEY)
            .fetch_optional(pool)
            .await?;
    Ok(stored.and_then(|value| value.as_bool()).unwrap_or(false))
}

/// Switch passive attribution on or off, and answer with what is now stored.
///
/// **Switching it off stops the recording and stops the derivation; it does
/// not delete what has already been offered.** A passive block on a day the
/// reader has not reviewed yet is knobas' answer to "what was I doing", and
/// throwing those away on a settings toggle would lose the very afternoons the
/// feature was turned on for. Nothing passive reaches a source on its own, so
/// there is nothing to withdraw.
///
/// # Errors
/// [`IpcError`] if the write fails.
pub async fn set_enabled(pool: &PgPool, on: bool) -> Result<bool, IpcError> {
    sqlx::query(
        "insert into knobas.setting (key, value) values ($1, $2)
         on conflict (key) do update set value = excluded.value, updated_at = now()",
    )
    .bind(SETTING_KEY)
    .bind(serde_json::Value::Bool(on))
    .execute(pool)
    .await?;
    Ok(on)
}

/// Record this beat, if passive attribution is on.
///
/// The one call [`heartbeat`](super::heartbeat) makes, so the switch is read
/// and the row is written in one place rather than as a sequence the caller
/// has to get in the right order. It runs **after** the stamp has landed: see
/// that function for why nothing about the foreground may cost the beat.
///
/// # Errors
/// [`IpcError`] if the setting cannot be read or the write fails.
pub async fn observe(pool: &PgPool, foreground: Option<&TimerTarget>) -> Result<(), IpcError> {
    if !enabled(pool).await? {
        return Ok(());
    }
    record(pool, foreground).await
}

/// Insert one observation, now.
///
/// A malformed foreground arrives here as `None`: the beat still happened and
/// the window was still focused, so the observation is a real one -- what it
/// cannot say is what was open.
///
/// **The instant is this process's clock, bound, and not the column's
/// `default now()`.** Migration `0015` made `at` default server-side so the
/// instant would be "the server's and not a webview clock that disagrees
/// with it by fractions of a second"; that default is now unused by any
/// production writer (`write` binds `at` so the fixtures can write a past
/// through it, #387), and the guard still holds, because `Utc::now()` here
/// is the app process's clock, on the host the embedded server runs on, and
/// never the webview's. `knobas.timer.last_heartbeat` keeps its server-side
/// `now()` (`BEAT` in `time/mod.rs`); the two stamps of one beat are one
/// host's clock read microseconds apart, against thirty-second windows.
///
/// # Errors
/// [`IpcError`] if the write fails.
async fn record(pool: &PgPool, foreground: Option<&TimerTarget>) -> Result<(), IpcError> {
    write(pool, Utc::now(), foreground).await
}

/// Insert one observation at `at`. **Tests only.**
///
/// [`record`] is this with the clock; the instant is a parameter so that a
/// test can build a morning of beats without waiting for one, through the
/// same insert the shell's heartbeat lands in. Before #387 the fixtures
/// wrote `knobas.heartbeat` with SQL of their own, and the two writers were
/// held to each other by nothing: a `record` that wrote `focused = false`
/// left every passive-block test green while the day review offered a real
/// user nothing. A fixture that goes through here cannot drift from the
/// production row, because there is no second row shape to drift to.
///
/// **Behind `test-util`**, the convention `docs/contract.md` §10.4 records
/// for a seam the app must not reach: this one does not read the switch
/// ([`observe`] is the gate, and a fixture that wants a beat kept has
/// already turned the setting on), so an app caller would record beats
/// with passive attribution off. Under the gate that mistake is a compile
/// error rather than a review catch. The crate's own `tests/` see it through
/// the self-dev-dependency.
///
/// # Errors
/// [`IpcError`] if the write fails.
#[cfg(feature = "test-util")]
pub async fn record_at(
    pool: &PgPool,
    at: DateTime<Utc>,
    foreground: Option<&TimerTarget>,
) -> Result<(), IpcError> {
    write(pool, at, foreground).await
}

/// **The one statement that writes what [`OBSERVATIONS`] reads.**
///
/// `focused` is left to the column's default, which is `true`, because that
/// is the only value anything writes (see [`Observation::focused`]): the shell
/// sends no beat from an unfocused window.
async fn write(
    pool: &PgPool,
    at: DateTime<Utc>,
    foreground: Option<&TimerTarget>,
) -> Result<(), IpcError> {
    let (entity_id, label) = foreground.map_or((None, None), TimerTarget::columns);
    sqlx::query("insert into knobas.heartbeat (at, entity_id, label) values ($1, $2, $3)")
        .bind(at)
        .bind(entity_id)
        .bind(label)
        .execute(pool)
        .await?;
    Ok(())
}

/// The beats inside `[from, to)`, in order.
const OBSERVATIONS: &str = "select at, entity_id, label, focused from knobas.heartbeat
      where at >= $1 and at < $2
      order by at, id";

/// The blocks a person owns that overlap the day -- everything that is not a
/// passive row this reconciliation is responsible for.
const OWNED: &str = "select started_at, ended_at from knobas.block
      where kind <> 'passive' and started_at < $2 and ended_at > $1";

/// Drop the day's passive rows that the derivation no longer supports.
///
/// Keyed by the start instant, which is a passive block's identity
/// (`0015`'s unique index). `worklog_id is null` is belt and braces: nothing
/// logs a passive block, and the day this module starts deleting rows that
/// something did log is a day the failure should be a constraint rather than a
/// silent hole in somebody's timesheet.
const FORGET: &str = "delete from knobas.block
      where kind = 'passive' and worklog_id is null
        and started_at >= $1 and started_at < $2
        and started_at <> all($3)";

/// Write one derived span, or move the end of the row already at its start.
///
/// The upsert is what keeps a passive block's **id stable** while the day is
/// still being lived: today's last span grows by a beat every thirty seconds,
/// and a row rewritten under each read would hand the strip a new id to draw
/// and a stale one to assign.
const OFFER: &str = "insert into knobas.block (started_at, ended_at, entity_id, label, kind)
      values ($1, $2, $3, $4, 'passive')
      on conflict (started_at) where kind = 'passive'
      do update set ended_at = excluded.ended_at,
                    entity_id = excluded.entity_id,
                    label = excluded.label";

/// Make the day's unassigned passive blocks equal to what the observations
/// support.
///
/// Called by [`day::list`](super::day::list) before it reads, and by nothing
/// else. **The day read is where this belongs**, and the reason is the cap: it
/// is a rule about a *day*, and the reader's midnight is a fact only the
/// webview holds, so the read that is handed two instants is the one place the
/// derivation can be applied at all. Making it a command of its own would mean
/// every future reader of blocks had to remember to run it first, and one that
/// forgot would draw a day with no passive time and no way to tell that from a
/// day with none.
///
/// Four things stop it before it writes, and each is a rule rather than an
/// optimisation:
///
/// * **The setting is off.** Nothing was recorded, and nothing already offered
///   is taken back.
/// * **The day has no observations at all.** Every day before this feature
///   existed is such a day, and a reconciliation that spoke about one would
///   delete passive blocks it has no evidence either way about.
/// * **The day reaches back past what [`prune`] has swept.** The same rule
///   one line further out: a day whose observations are past the horizon is a
///   day knobas has no evidence about either, and it reads as **absent**
///   rather than as observed-and-empty. Emptiness would delete the blocks the
///   day was already offered, which is knobas forgetting an afternoon on the
///   strength of a record it threw away itself. The guard is on `from` and not
///   on `to` deliberately: the horizon is an instant and a day is an interval,
///   so one day always straddles it, and it is exactly that day -- half swept,
///   half intact -- a `to` comparison would hand to the derivation. The guard
///   and the reading the two surfaces draw are one comparison, in
///   [`Horizon::passed`], so "the day this refused to reconcile" and "the day
///   the strip calls absent" cannot come apart (#337).
/// * **A span overlaps a block the person owns.** Passive attribution never
///   draws over time a manual block already claims -- including a passive
///   block that has since been assigned, which is what stops an assignment
///   growing a passive twin on the next read. Where they overlap the block
///   wins and the span is dropped whole, the direction that can only lose a
///   suggestion and never invent one.
///
/// # Errors
/// [`IpcError`] if any of the reads or writes fails.
pub(super) async fn materialize(
    pool: &PgPool,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> Result<(), IpcError> {
    if !enabled(pool).await? {
        return Ok(());
    }
    if horizon_of(pool).await?.passed(from) {
        return Ok(());
    }

    let rows = sqlx::query(OBSERVATIONS)
        .bind(from)
        .bind(to)
        .fetch_all(pool)
        .await?;
    if rows.is_empty() {
        return Ok(());
    }
    let observations = rows
        .iter()
        .map(observation_of)
        .collect::<Result<Vec<_>, _>>()?;

    let owned = sqlx::query(OWNED)
        .bind(from)
        .bind(to)
        .fetch_all(pool)
        .await?;
    let owned: Vec<(DateTime<Utc>, DateTime<Utc>)> = owned
        .iter()
        .map(|row| Ok((row.try_get("started_at")?, row.try_get("ended_at")?)))
        .collect::<Result<_, sqlx::Error>>()?;

    let spans: Vec<PassiveSpan> = derive(&observations)
        .into_iter()
        .filter(|span| {
            !owned
                .iter()
                .any(|(began, ended)| span.started_at < *ended && span.ended_at > *began)
        })
        .collect();

    let starts: Vec<DateTime<Utc>> = spans.iter().map(|span| span.started_at).collect();
    let mut tx = pool.begin().await?;
    sqlx::query(FORGET)
        .bind(from)
        .bind(to)
        .bind(&starts)
        .execute(&mut *tx)
        .await?;
    for span in &spans {
        let (entity_id, label) = span.target.columns();
        sqlx::query(OFFER)
            .bind(span.started_at)
            .bind(span.ended_at)
            .bind(entity_id)
            .bind(label)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(())
}

/// Every observation older than the cutoff goes, in one statement.
const SWEEP: &str = "delete from knobas.heartbeat where at < $1";

/// Read the stamp; `None` when knobas has never thrown an observation away.
const STAMP_READ: &str = "select value from knobas.setting where key = $1";

/// Move the stamp. Same upsert `set_enabled` uses, on a different key.
const STAMP_WRITE: &str = "insert into knobas.setting (key, value) values ($1, $2)
      on conflict (key) do update set value = excluded.value, updated_at = now()";

/// The oldest instant an observation may carry at `now` and still be kept.
///
/// A function of the clock and nothing else, so the rule can be read at a
/// glance -- the treatment [`derive`] gets, for the same reason. Private:
/// [`prune`] is the only thing that spends it, and a caller outside this
/// module holding its own copy of the cutoff is the drift the stamp exists
/// to make impossible.
///
/// **Not the observation horizon**, which it sat next to under that name
/// until #344. This is what a sweep at `now` *would* take; the horizon is
/// what a sweep actually took, and [`horizon_of`] is the only thing that
/// knows it. The two coincide right after [`prune`] deletes something and
/// diverge every second after -- and on a database no sweep has run in there
/// is a cutoff every day but no horizon at all, which is the case
/// [`horizon_of`]'s own doc warns not to compute past.
fn sweep_cutoff(now: DateTime<Utc>) -> DateTime<Utc> {
    now - Duration::days(RETENTION_DAYS)
}

/// The instant before which knobas no longer has observations.
///
/// `None` on a database no sweep has taken anything out of, which is the
/// answer that lets [`materialize`] reconcile freely: nothing is missing, so
/// nothing can be missed. A value that no longer decodes reads as `None` too
/// -- the same resolution `backup::read_setting` records, and it errs the
/// same way [`enabled`] does, toward doing the ordinary thing rather than
/// refusing every day on the strength of a row nobody can read.
///
/// # Errors
/// [`IpcError`] if the read fails.
async fn pruned_before<'e, E>(db: E) -> Result<Option<DateTime<Utc>>, IpcError>
where
    E: sqlx::PgExecutor<'e>,
{
    let stored: Option<serde_json::Value> = sqlx::query_scalar(STAMP_READ)
        .bind(PRUNED_KEY)
        .fetch_optional(db)
        .await?;
    Ok(stored.and_then(|value| serde_json::from_value(value).ok()))
}

/// The observation horizon, as one value a caller can ask questions of.
///
/// A newtype over [`pruned_before`]'s answer rather than the `Option` itself,
/// and the reason is #337: **three** callers now have to decide whether a day
/// reaches back past what [`prune`] swept -- [`materialize`], which refuses to
/// reconcile such a day, and the day and week reads, which have to *say* so.
/// Three copies of `from < swept` is three chances for the surfaces to
/// disagree with the guard about which days those are, and the disagreement
/// would be invisible: both readings draw an empty passive column.
///
/// So the comparison is written once, here, and everything else asks
/// [`Horizon::passed`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Horizon(Option<DateTime<Utc>>);

impl Horizon {
    /// Whether a day beginning at `from` reaches back past what [`prune`] has
    /// swept.
    ///
    /// **On `from` and not on `to`**, the rule [`materialize`] records in
    /// full: the horizon is an instant and a day is an interval, so one day
    /// always straddles it, and that day -- half swept, half intact -- is one
    /// knobas cannot speak about either.
    pub(super) fn passed(self, from: DateTime<Utc>) -> bool {
        self.0.is_some_and(|swept| from < swept)
    }
}

/// Read the horizon this database is actually behind.
///
/// The stamp, never `now - RETENTION_DAYS`: [`PRUNED_KEY`] carries the whole
/// argument, and it applies to what a surface *says* exactly as it applies to
/// what [`materialize`] does. A database no sweep has run in -- a fresh
/// fixture, a restored archive, a profile whose owner never left the app
/// running -- has every observation it ever had, however old, and a reader
/// must not be told otherwise.
///
/// # Errors
/// [`IpcError`] if the read fails.
pub(super) async fn horizon_of<'e, E>(db: E) -> Result<Horizon, IpcError>
where
    E: sqlx::PgExecutor<'e>,
{
    Ok(Horizon(pruned_before(db).await?))
}

/// Throw away the observations [`RETENTION_DAYS`] has aged out, and answer
/// with how many went (issue #315).
///
/// `now` is a parameter and not a clock, so the rule is testable at a
/// [`sweep_cutoff`] a fixture chooses rather than only at one thirty days
/// behind the machine.
/// Its one production caller is `backup::tick`.
///
/// # Where this runs, and why it is not the day read
///
/// The obvious home is [`materialize`], which already runs on every day read
/// and already knows about beats. It is the wrong one: the table grows while
/// **passive attribution is on**, not while somebody is reviewing, so a sweep
/// bound to the day read never runs for the reader who switched the feature on
/// and has not opened the strip since -- the one reader whose table nobody is
/// bounding. Worse, that reader is also the one for whom the promise
/// [`RETENTION_DAYS`] makes is a privacy claim: a person who switches passive
/// attribution *off* is asking knobas to stop keeping a record of what they
/// had open, and a sweep that only runs on the day read would keep the old one
/// for as long as they stayed away.
///
/// So it runs on the background task that ticks whatever the reader is doing,
/// which is `backup`'s -- the module that already owns a retention rule (its
/// archives') and the app's only wall-clock loop that is not per-source. The
/// rule and the constant stay here; that module contributes the clock.
///
/// # The stamp, and why it moves only when something was deleted
///
/// The delete and the stamp are **one transaction**, so a day read either sees
/// every beat and no stamp or the surviving beats and the stamp that explains
/// them. There is no instant in between for a reconciliation to run in, which
/// is what makes "what happens to the day the reader is looking at while this
/// runs" a question with a boring answer.
///
/// The stamp moves only when rows actually went, and never backwards. A sweep
/// that found nothing has thrown nothing away, so it has no claim to record --
/// and recording one anyway would refuse [`materialize`] a day whose beats are
/// all still present. The `max` is what keeps a clock that jumped backwards
/// from un-forgetting rows that are already gone.
///
/// # Errors
/// [`IpcError`] if the delete or either half of the stamp fails.
pub async fn prune(pool: &PgPool, now: DateTime<Utc>) -> Result<u64, IpcError> {
    let cut = sweep_cutoff(now);
    let mut tx = pool.begin().await?;
    let taken = sqlx::query(SWEEP)
        .bind(cut)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    if taken > 0 {
        let stamp = pruned_before(&mut *tx)
            .await?
            .map_or(cut, |had| had.max(cut));
        sqlx::query(STAMP_WRITE)
            .bind(PRUNED_KEY)
            // Spelled as a string here rather than serialized, so there is no
            // failure to resolve. `serde_json::to_value` cannot fail for a
            // `DateTime<Utc>`, but the fallback such a call needs would be a
            // value the read decodes as *never swept* -- which disarms the
            // guard in `materialize` on the one write that most needs it.
            .bind(serde_json::Value::String(stamp.to_rfc3339()))
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(taken)
}

/// Rebuild an observation from its row.
///
/// A stored foreground that is no longer a legal target -- a namespace that
/// has since been reserved, a row written by another knobas -- reads as
/// `None`: the beat happened and the window was focused, and the honest thing
/// to lose is the attribution rather than the whole observation. [`vet`] is
/// the same rule the timer refuses on, asked here so there is one answer to
/// "may time be attributed to this" in the crate.
///
/// **This is deliberately not [`target_of`](super::target_of), which decodes
/// the same two columns for the timer and the block.** That function is
/// *exactly one, or the row is a schema failure and says so*; this one is
/// **at most one**, because `heartbeat_target_chk` allows neither half and a
/// beat with nothing in the foreground is a real observation rather than a
/// broken row. Sharing one decoder would mean either an internal error on
/// every empty room or a silent fallback that made a broken timer row look
/// like an empty one. Two rules, two decoders, and the reason written down.
pub(super) fn observation_of(row: &sqlx::postgres::PgRow) -> Result<Observation, IpcError> {
    let entity_id: Option<String> = row.try_get("entity_id")?;
    let label: Option<String> = row.try_get("label")?;
    let foreground = match (entity_id, label) {
        (Some(entity_id), None) => vet(TimerTarget::Entity { entity_id }).ok(),
        (None, Some(label)) => vet(TimerTarget::Label { label }).ok(),
        _ => None,
    };
    Ok(Observation {
        at: row.try_get("at")?,
        foreground,
        focused: row.try_get("focused")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at(seconds: i64) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, 3, 9, 0, 0).unwrap() + Duration::seconds(seconds)
    }

    fn on(entity_id: &str) -> TimerTarget {
        TimerTarget::Entity {
            entity_id: entity_id.to_owned(),
        }
    }

    /// Beats every `every` seconds from `first` to `last` inclusive, all
    /// focused, all on `target`.
    fn beats(target: Option<TimerTarget>, first: i64, last: i64, every: i64) -> Vec<Observation> {
        let mut seconds = first;
        let mut made = Vec::new();
        while seconds <= last {
            made.push(Observation {
                at: at(seconds),
                foreground: target.clone(),
                focused: true,
            });
            seconds += every;
        }
        made
    }

    fn seconds(span: &PassiveSpan) -> i64 {
        span.length().num_seconds()
    }

    /// **A silence is a break in focused time, not more of it**, and the
    /// break costs everything past one beat window.
    ///
    /// Two runs of beats an hour apart. The walk gives two spans, and the hour
    /// between them buys exactly **thirty seconds** -- the window the last
    /// beat of the first run credits forward before knobas stops claiming to
    /// know anything. The other 2970 seconds are not focused time, which is
    /// the rule that stops a laptop shut at lunch billing the afternoon, and
    /// it is the rule #283's "app open" row is read through as well.
    #[test]
    fn focused_time_is_the_beats_shape_and_a_silence_is_not_in_it() {
        let mut observations = beats(Some(on("jira:PAY-231")), 0, 600, 30);
        observations.extend(beats(None, 3600, 3720, 30));

        let spans = focused_spans(&observations);

        assert_eq!(spans.len(), 2, "an hour of silence is a break: {spans:?}");
        assert_eq!(spans[0].started_at, at(0));
        assert_eq!(
            spans[0].ended_at,
            at(630),
            "the run's last beat credits one window toward the silence and no \
             more of it"
        );
        assert_eq!(spans[1].started_at, at(3600));
        assert_eq!(
            spans[1].ended_at,
            at(3720),
            "and the very last beat of all credits nothing forward"
        );
        assert_eq!(
            spans.iter().map(|s| s.length().num_seconds()).sum::<i64>(),
            750,
            "an hour of wall clock is 750 seconds of focused time, and that is \
             what the cap spends"
        );
    }

    /// An unfocused beat is neither time nor attribution, so it credits
    /// nothing -- the direction that keeps a window left open behind another
    /// app out of the week's "app open" row.
    #[test]
    fn an_unfocused_beat_credits_no_focused_time() {
        let observations = vec![
            Observation {
                at: at(0),
                foreground: Some(on("jira:PAY-231")),
                focused: false,
            },
            Observation {
                at: at(30),
                foreground: Some(on("jira:PAY-231")),
                focused: true,
            },
            Observation {
                at: at(60),
                foreground: Some(on("jira:PAY-231")),
                focused: true,
            },
        ];

        let spans = focused_spans(&observations);

        assert_eq!(spans.len(), 1, "only the focused pair counts: {spans:?}");
        assert_eq!(spans[0].started_at, at(30));
        assert_eq!(spans[0].ended_at, at(60));
    }

    /// **The cap is spent in order, and the later visit is the one trimmed.**
    ///
    /// Two dense runs of observations, each claiming a window past its last
    /// beat, and a day that can afford one of those tails and not both. The
    /// first visit is offered whole and the second ends at its last beat --
    /// which is the ordering rule stated as an outcome rather than as a
    /// comment, and the case neither `the_cap_binds_when_heartbeats_overlap`
    /// (one visit) nor the tail tests (no competition) can reach.
    #[test]
    fn the_cap_is_spent_in_order_and_the_later_visit_is_trimmed() {
        let mut observations = beats(Some(on("jira:PAY-231")), 0, 600, 5);
        observations.extend(beats(Some(on("jira:PAY-99")), 3600, 4200, 5));

        let spans = derive(&observations);

        assert_eq!(
            spans.len(),
            2,
            "two runs of observations, two visits: {spans:?}"
        );
        assert_eq!(
            seconds(&spans[0]),
            630,
            "the earlier visit is paid first and keeps the window its last \
             beat credits forward"
        );
        assert_eq!(
            seconds(&spans[1]),
            600,
            "and the later one is trimmed to what the day has left"
        );
    }

    /// ...and when what the day has left is under the floor, the visit is not
    /// offered at all: a stretch knobas cannot pay for is not a stretch it may
    /// suggest, and nothing after it can be paid for either.
    #[test]
    fn a_visit_the_day_cannot_pay_for_is_not_offered() {
        let mut observations = beats(Some(on("jira:PAY-231")), 0, 600, 30);
        observations.extend(beats(Some(on("jira:PAY-99")), 630, 720, 30));

        let spans = derive(&observations);

        assert_eq!(
            spans.len(),
            1,
            "the day had ninety seconds of budget left and the second visit \
             wanted a hundred and twenty: {spans:?}"
        );
        assert_eq!(spans[0].target, on("jira:PAY-231"));
    }

    /// ...and the boundary between those two, which is the floor compared a
    /// second time.
    ///
    /// The cap trims the later visit to **exactly** the floor, and the rule
    /// that decides whether that is a block is the same rule the floor itself
    /// states: shorter than two minutes is dropped, so two minutes is offered.
    /// Neither of the two tests above can tell a `<` here from a `<=` --
    /// theirs are trimmed well clear of the line on either side -- and the two
    /// comparisons are in different statements, so witnessing one does not
    /// witness the other.
    #[test]
    fn a_visit_the_day_can_pay_for_down_to_the_floor_is_offered() {
        let mut observations = beats(Some(on("jira:PAY-231")), 0, 600, 30);
        observations.extend(beats(Some(on("jira:PAY-99")), 3600, 3720, 30));

        let spans = derive(&observations);

        assert_eq!(
            spans.len(),
            2,
            "the day had exactly two minutes left and the visit wanted two \
             and a half: {spans:?}"
        );
        assert_eq!(
            seconds(&spans[1]),
            FLOOR_SECONDS,
            "a visit trimmed to exactly the floor is still a block"
        );
    }

    /// The floor, at the length the ticket names.
    ///
    /// Beats at 0, 30 and 60 on the ticket and one at 90 on something else:
    /// the ticket is credited from its first beat to the moment knobas heard
    /// otherwise, which is ninety seconds, and ninety seconds of having a
    /// ticket open is not an hour's work in the making.
    #[test]
    fn a_visit_of_ninety_seconds_is_not_a_block() {
        let mut observations = beats(Some(on("jira:PAY-231")), 0, 60, 30);
        observations.extend(beats(Some(on("jira:PAY-99")), 90, 300, 30));

        let spans = derive(&observations);

        assert!(
            spans.iter().all(|span| span.target != on("jira:PAY-231")),
            "a ninety-second visit is under the two-minute floor and is not a \
             block: {spans:?}"
        );
    }

    /// ...and the direction that says the floor is a floor rather than a wall:
    /// the same shape, one beat longer than the floor, is offered.
    #[test]
    fn a_visit_past_the_floor_is_a_block() {
        let mut observations = beats(Some(on("jira:PAY-231")), 0, 120, 30);
        observations.extend(beats(Some(on("jira:PAY-99")), 150, 400, 30));

        let spans = derive(&observations);

        let ticket: Vec<&PassiveSpan> = spans
            .iter()
            .filter(|span| span.target == on("jira:PAY-231"))
            .collect();
        assert_eq!(
            ticket.len(),
            1,
            "one visit, not none and not four: {spans:?}"
        );
        assert_eq!(seconds(ticket[0]), 150);
    }

    /// ...and the boundary itself, which is the one place neither of those two
    /// can reach.
    ///
    /// Ninety seconds is short and a hundred and fifty is long, so a `>` where
    /// the rule says `>=` is green against both of them: it drops only the
    /// visit that is *exactly* the floor, and nothing else asks about that
    /// visit. The spec's rule is "shorter than two minutes", so two minutes is
    /// a block, and this is the assertion that says which side of the line the
    /// line itself is on.
    #[test]
    fn a_visit_of_exactly_the_floor_is_a_block() {
        let mut observations = beats(Some(on("jira:PAY-231")), 0, 90, 30);
        observations.extend(beats(Some(on("jira:PAY-99")), 120, 400, 30));

        let spans = derive(&observations);

        let ticket: Vec<&PassiveSpan> = spans
            .iter()
            .filter(|span| span.target == on("jira:PAY-231"))
            .collect();
        assert_eq!(
            ticket.len(),
            1,
            "a visit of exactly the floor is not shorter than the floor: \
             {spans:?}"
        );
        assert_eq!(seconds(ticket[0]), FLOOR_SECONDS);
    }

    /// The merge rule: an unbroken run of beats on one target is **one**
    /// stretch, and it runs from the first beat to the last thing that
    /// supported it.
    #[test]
    fn adjacent_visits_to_one_target_are_one_block() {
        let observations = beats(Some(on("jira:PAY-231")), 0, 600, 30);

        let spans = derive(&observations);

        assert_eq!(spans.len(), 1, "twenty-one beats are one visit: {spans:?}");
        assert_eq!(spans[0].started_at, at(0));
        // The cap: the last beat credits thirty seconds forward and no later
        // beat confirms it, so the visit ends **at** its last beat.
        assert_eq!(spans[0].ended_at, at(600));
    }

    /// ...and the direction that says merging is about the target and not
    /// merely about adjacency: a run interrupted by another target is two
    /// visits, and the time on the interruption is not on either of them.
    #[test]
    fn a_target_interrupted_by_another_does_not_merge_across_it() {
        let mut observations = beats(Some(on("jira:PAY-231")), 0, 300, 30);
        observations.extend(beats(Some(on("jira:PAY-99")), 330, 600, 30));
        observations.extend(beats(Some(on("jira:PAY-231")), 630, 900, 30));

        let spans = derive(&observations);

        let reading: Vec<(&TimerTarget, i64, i64)> = spans
            .iter()
            .map(|span| {
                (
                    &span.target,
                    (span.started_at - at(0)).num_seconds(),
                    (span.ended_at - at(0)).num_seconds(),
                )
            })
            .collect();
        assert_eq!(
            reading,
            vec![
                (&on("jira:PAY-231"), 0, 330),
                (&on("jira:PAY-99"), 330, 630),
                (&on("jira:PAY-231"), 630, 900),
            ],
            "the ticket was open twice with something else in between, and \
             that is three blocks and not one"
        );
    }

    /// The cap, in the one case where it takes back more than the tail of a
    /// run of observations: **beats that overlap**.
    ///
    /// Two windows beating, or a retry, or an import -- whatever the cause,
    /// the beats arrive six times as often as they are sent, so the claims
    /// they make overlap. Merging them is right; believing their sum is not.
    ///
    /// **This test can tell "capped" from "merged"**, which is the whole
    /// reason it is written on these numbers. Without the cap the spans still
    /// merge and the answer is one span of 330 seconds -- the assertion below
    /// is on 300, so the merge being right does not save it. Without the
    /// *merge* the answer is sixty spans of thirty seconds, every one of them
    /// under the floor, so the list is empty and this fails on its length
    /// rather than on its arithmetic. The two mutants fail differently.
    #[test]
    fn the_cap_binds_when_heartbeats_overlap() {
        let observations = beats(Some(on("jira:PAY-231")), 0, 300, 5);

        let spans = derive(&observations);

        assert_eq!(spans.len(), 1, "one visit, however many beats: {spans:?}");
        assert_eq!(
            seconds(&spans[0]),
            300,
            "sixty-one beats five seconds apart are five minutes of focused \
             time, and the claims they make add up to five and a half"
        );
        assert_eq!(
            spans[0].ended_at,
            at(300),
            "the visit ends at its last beat"
        );
    }

    /// Focused time is time, whatever was open. A stretch with nothing in the
    /// foreground attributes nothing -- and it still pays into the budget,
    /// which is what a **total** cap means: the ticket that follows is offered
    /// whole, including the window its last beat credits forward, because the
    /// day has focused time to spare for it.
    #[test]
    fn focused_time_with_nothing_open_attributes_nothing_and_pays_for_itself() {
        let mut observations = beats(None, 0, 300, 30);
        observations.extend(beats(Some(on("jira:PAY-231")), 330, 600, 30));

        let spans = derive(&observations);

        assert_eq!(spans.len(), 1, "an empty room is not a block: {spans:?}");
        assert_eq!(spans[0].started_at, at(330));
        assert_eq!(spans[0].ended_at, at(630));
    }

    /// An unfocused observation is neither time nor attribution.
    ///
    /// Nothing writes one today -- the shell sends no beat from an unfocused
    /// window at all -- and the rule is stated here so that the day a beat is
    /// sent on blur, it lands on a derivation that already knows what to do
    /// with it. The same treatment `0013` gave the `passive` block kind.
    #[test]
    fn an_unfocused_observation_is_neither_time_nor_attribution() {
        let mut observations = beats(Some(on("jira:PAY-231")), 0, 300, 30);
        // The window is blurred, still on the ticket, for a quarter of an hour.
        observations.extend((330..=1200).step_by(30).map(|second| Observation {
            at: at(second),
            foreground: Some(on("jira:PAY-231")),
            focused: false,
        }));

        let spans = derive(&observations);

        assert_eq!(
            spans.len(),
            1,
            "one visit, not one and a quarter hour: {spans:?}"
        );
        assert_eq!(
            spans[0].ended_at,
            at(330),
            "the visit ends where the reader stopped looking -- fifteen minutes \
             of a blurred window on the same ticket is not fifteen minutes of \
             work, and it does not pay into the budget either"
        );
    }

    /// A gap in the beats is a gap: silence stops being work one beat window
    /// after the last thing knobas heard, and the two stretches are two
    /// blocks rather than one long one.
    #[test]
    fn silence_longer_than_a_beat_window_breaks_a_visit() {
        let mut observations = beats(Some(on("jira:PAY-231")), 0, 300, 30);
        observations.extend(beats(Some(on("jira:PAY-231")), 3600, 3900, 30));

        let spans = derive(&observations);

        assert_eq!(spans.len(), 2, "a lunch is not work: {spans:?}");
        assert_eq!(
            spans[0].ended_at,
            at(330),
            "the first visit outlives its last beat by one window"
        );
        assert_eq!(spans[1].started_at, at(3600));
    }

    /// Order is the derivation's to establish, not the caller's to promise.
    #[test]
    fn observations_out_of_order_derive_the_same_blocks() {
        let observations = beats(Some(on("jira:PAY-231")), 0, 600, 30);
        let mut shuffled = observations.clone();
        shuffled.reverse();

        assert_eq!(derive(&shuffled), derive(&observations));
    }

    #[test]
    fn no_observations_are_no_blocks() {
        assert!(derive(&[]).is_empty());
    }

    /// The floor is the spec's two minutes, and the window is the interval the
    /// shell actually beats at.
    ///
    /// A source scan for the second, for the reason `commands/time.rs`'s
    /// context-namespace pin gives for its own: the two numbers live in two
    /// languages and nothing else in the tree compares them. A shell that
    /// started beating every ten seconds while this went on crediting thirty
    /// would extend every visit past the last beat that supported it -- green
    /// here, and wrong on screen.
    #[test]
    fn the_beat_window_is_the_interval_the_shell_beats_at() {
        const SHELL: &str = include_str!("../../../../app/src/lib/shell/timer.svelte.ts");
        let declaration = format!("HEARTBEAT_MS = {}_000;", BEAT_WINDOW_SECONDS);
        assert!(
            SHELL.contains(&declaration),
            "app/src/lib/shell/timer.svelte.ts no longer beats every \
             {BEAT_WINDOW_SECONDS} seconds, so every passive visit is credited \
             past the last beat that supported it"
        );
        assert_eq!(
            FLOOR_SECONDS, 120,
            "spec #272: visits shorter than two minutes are dropped"
        );
    }

    /// The observations outlast every surface that reads a past day.
    ///
    /// [`RETENTION_DAYS`]' floor, spelled where lowering it fails rather than
    /// in prose alone. One half is a constant this crate holds
    /// ([`MOST_DAYS`](super::week::MOST_DAYS)), so a timesheet that grew to a
    /// fortnight would fail here rather than start quietly reading days whose
    /// beats had been swept. The other is a literal, because #288's digest
    /// does not exist yet and a constant invented for it would be this module
    /// guessing at another module's rule.
    ///
    /// **"Outlasts" is about the *width* of a read and not about where a
    /// reader may point one.** No assertion here could be about the latter:
    /// both surfaces take an arbitrary past date, so the only thing a
    /// constant can promise is a window, and what a surface should say once
    /// it is outside one is #337's. This is why the failure message names a
    /// span rather than a date.
    #[test]
    fn the_observations_outlast_the_surfaces_that_read_a_past_day() {
        /// The standup digest's *yesterday* reaches back at most this far
        /// (#288: "at most seven days back, so Monday reads Friday").
        const DIGEST_LOOK_BACK_DAYS: i64 = 7;
        let week = i64::try_from(super::super::week::MOST_DAYS).expect("a week is small");

        assert!(
            RETENTION_DAYS >= week + DIGEST_LOOK_BACK_DAYS,
            "knobas keeps {RETENTION_DAYS} days of observations and a surface reads \
back {week} + {DIGEST_LOOK_BACK_DAYS}: a day the reader can still be shown is a \
day the sweep has taken the beats for"
        );
    }
}
