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
//! it off, [`record`] is never called and [`materialize`] returns before it
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
//! * **Adjacent visits to one target merge.** Six beats on one ticket are one
//!   stretch, not six.
//! * **The total never exceeds focused time.** See [`derive`] for why this is
//!   load-bearing rather than theoretical: it is what makes a session's last
//!   visit end at its last beat instead of one beat window later.
//!
//! # What an observation claims, and why the cap follows from it
//!
//! A beat says *this was in the foreground at this instant*. It cannot say
//! what was open in between, so the rule is: **a beat credits its target
//! forward, for one beat window, and no further.** Silence therefore stops
//! being work [`BEAT_WINDOW_SECONDS`] after the last thing knobas heard,
//! whatever happened to the process in between.
//!
//! Focused time is the same walk read a beat later: the gap between one beat
//! and the next, clamped to the same window. So a session's claims add up to
//! its focused time **plus one window** -- the tail the last beat credits
//! forward and no later beat confirms -- and the cap is what takes that tail
//! back. It binds on every session, and it binds harder whenever beats arrive
//! closer together than they are sent (a second window beating, a retry, an
//! import), which is the case in which claims genuinely overlap.

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

    let mut budget = Duration::zero();
    for pair in beats.windows(2) {
        if pair[0].focused {
            budget += (pair[1].at - pair[0].at).min(window);
        }
    }

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

/// Insert one observation.
///
/// Called by [`heartbeat`](super::heartbeat) **after** the stamp has landed and
/// only while the setting is on. A malformed foreground arrives here as
/// `None`: the beat still happened and the window was still focused, so the
/// observation is a real one -- what it cannot say is what was open.
///
/// # Errors
/// [`IpcError`] if the write fails.
pub async fn record(pool: &PgPool, foreground: Option<&TimerTarget>) -> Result<(), IpcError> {
    let (entity_id, label) = foreground.map_or((None, None), TimerTarget::columns);
    sqlx::query("insert into knobas.heartbeat (entity_id, label) values ($1, $2)")
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

/// Make the day's unassigned passive blocks equal to what the beats support.
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
/// Three things stop it before it writes, and each is a rule rather than an
/// optimisation:
///
/// * **The setting is off.** Nothing was recorded, and nothing already offered
///   is taken back.
/// * **The day has no observations at all.** Every day before this feature
///   existed is such a day, and a reconciliation that spoke about one would
///   delete passive blocks it has no evidence either way about.
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

/// Rebuild an observation from its row.
///
/// A stored foreground that is no longer a legal target -- a namespace that
/// has since been reserved, a row written by another knobas -- reads as
/// `None`: the beat happened and the window was focused, and the honest thing
/// to lose is the attribution rather than the whole observation. [`vet`] is
/// the same rule the timer refuses on, asked here so there is one answer to
/// "may time be attributed to this" in the crate.
fn observation_of(row: &sqlx::postgres::PgRow) -> Result<Observation, IpcError> {
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

    /// The cap, in the one case where it takes back more than a session's
    /// tail: **beats that overlap**.
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
}
