//! The week timesheet, and *Log all* (spec #272 "Timesheet and Log all",
//! issue #283).
//!
//! [`super::day`] is one day's blocks, editable. This is the same time added
//! up: Monday to Sunday, a row per target and a column per day, with what has
//! reached a ticket and what has not shown side by side. It is a fourth file
//! in the module pair for the reason [`super::day`] is a second and
//! [`super::passive`] a third -- the timer is a state machine, the day review
//! is a read and three writes, this is an aggregate and one bulk write.
//!
//! # The four numbers, and where each comes from
//!
//! **Tracked** is knobas' own record: the day's *manual* blocks. **Offered**
//! is the day's *passive* ones, counted beside it and never inside it -- the
//! day review's own arrangement, in its own word: "tracked time is manual
//! time, and passive time is counted beside it". A week that added the guess
//! to the tracked total would contradict the strip above it; a week that
//! dropped the guess altogether would lose an afternoon the strip is drawing,
//! and story 41 asks for a total that is honest.
//!
//! **Logged** and **held** come from the local worklog copy joined to the
//! write queue's state, and **never from the mirror** (spec #272, "the
//! timesheet's logged and held numbers come from the local copy joined to the
//! queue's state"). A worklog whose write is `pending` or `sent` is logged --
//! spec story 39: the number is about what the reader did, not about sync
//! timing. Anything else -- `held`, `refused`, or a copy with no queue row at
//! all -- is **held**: one word for "this time has not reached the ticket",
//! and the column is separate from unlogged rather than folded into it
//! because the blocks under it already carry a worklog id and *Log all* will
//! not offer them again. Drawing them as unlogged would be an invitation to
//! log an afternoon twice; drawing them as logged would be false.
//!
//! **`discarded` is not among them, and that is the whole of #328's fix.**
//! `held` and `refused` are open states -- `write_queue::open` selects
//! `state in ('pending','held','refused')` -- so they are in the
//! pending-writes panel and a person can retry or withdraw one. A discarded
//! write is not: that module's own words are "a sent or discarded write is
//! history". Held was the honest cell of the four this read had, but it was a
//! dead end -- the time read as held for good, `unlogged` stayed zero, and
//! neither *Log all* nor the draft would offer the blocks again.
//!
//! The way out was the discard path's rather than this read's, and it is now
//! built there: `knobas_core::write_queue::discard` deletes the worklog copy
//! in the same statement that settles the queue row, and `block_worklog_fk`'s
//! `on delete set null` gives the blocks back. So a discarded worklog reaches
//! this read as **no worklog at all** -- its time is tracked and unlogged
//! again, which is the honest cell, and every surface offers it.
//!
//! [`is_logged`] keeps its answer for `discarded` all the same, and it is
//! still *held*. The release spares a copy the source answered for --
//! `discard`'s `remote_id is null` guard, because an hour Jira holds must
//! never be offered for logging twice -- and such a copy beside a discarded
//! write is the one shape that still reaches this read. The state machine
//! does not produce it today (`sent` writes the id and `state = 'sent'` in
//! one statement, and nothing leaves `sent`), so this is a backstop rather
//! than a column anybody has seen; if a path to it ever appears, *held* is
//! the least wrong of the four and the cell will want revisiting with it.
//! `tests/week_ipc.rs`'s `a_discard_leaves_a_worklog_jira_answered_for_alone`
//! builds that fixture and reads the cell, so the claim is pinned rather than
//! merely written down.
//!
//! **Unlogged** is the difference, floored at zero. The floor is not
//! defensive tidiness: the draft's interval and seconds are the reader's own
//! and may exceed the blocks they were made of, and a negative cell would be
//! knobas telling somebody they had logged more than they worked in a column
//! headed *unlogged*.
//!
//! # Which day a stretch is on
//!
//! **A block belongs to the day it started on**, whole -- the rule
//! [`super::worklog`]'s `UNLOGGED_BLOCKS` already keys on, and the reason it
//! has to be the same rule here is *Log all*: a timesheet that split a block
//! across midnight would show Tuesday time that *Log all* then logged on
//! Monday, and the two columns would never reconcile. A worklog is on the day
//! its `started_at` falls in, for the same reason.
//!
//! **Coverage is the exception, and deliberately so.** The "no target, app
//! open" row asks whether a given *instant* was inside a block, so it uses
//! overlap, clipped to the day. The two rules answer two different questions:
//! one is bookkeeping about a stretch, the other is a fact about a moment.
//!
//! # The days are the reader's, and they arrive as instants
//!
//! Seven [`DayWindow`]s, each carrying the date and the two instants it spans,
//! computed by the webview from its own clock. The reasoning is
//! [`super::day`]'s in full: the machine's timezone is a fact only the webview
//! holds, and a single UTC offset for a week would be wrong for the whole week
//! containing a daylight-saving change -- which is 23 or 25 hours long on one
//! of its days and has two offsets across it. Seven windows get every edge
//! right by construction, in every zone, including that week.

use chrono::{DateTime, NaiveDate, Utc};
use sqlx::{PgPool, Row};

use super::{BlockKind, TimerTarget, passive, worklog};
use crate::IpcError;

/// The most days one read may be asked for.
///
/// Seven, because the surface is a week and a bound the caller cannot exceed
/// is cheaper than a read that scans a year because a webview miscounted.
const MOST_DAYS: usize = 7;

/// One of the reader's days: the date they call it, and the interval it is.
///
/// Both halves, because neither can be derived from the other on this side.
/// The instants are the query's; the date is what a cell is keyed and labelled
/// by, and a backend that recomputed it from the instants would be doing the
/// timezone arithmetic this shape exists to avoid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DayWindow {
    /// `2026-09-03`, in the reader's own reckoning.
    pub day: NaiveDate,
    /// The reader's local midnight, in UTC.
    pub from: DateTime<Utc>,
    /// The next local midnight, in UTC. Half-open: `to` is the next day's.
    pub to: DateTime<Utc>,
}

/// One target's numbers on one day, in seconds.
///
/// Seconds on the wire and minutes on the screen: `CONTEXT.md`'s timesheet is
/// minute-granular with no rounding, and rounding is the *view's* to do once,
/// on a number that never lost its precision getting there. A backend that
/// handed over minutes would have rounded seven times per row and the week's
/// total would not be the total of the week.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
pub struct WeekCell {
    /// The day's manual blocks on this target, added up. On the "no target"
    /// row, the focused time no block covered.
    pub tracked_seconds: i64,
    /// The day's **passive** blocks on this target -- what knobas is offering,
    /// which is never tracked and never logged.
    ///
    /// Beside `tracked_seconds` rather than inside it, and present rather than
    /// dropped: the strip above this draws these blocks, and a week in which
    /// they were simply absent would disagree with it about the same
    /// afternoon. Nothing logs one until a person assigns it, which is why it
    /// pays into no other column here.
    pub offered_seconds: i64,
    /// Worklog seconds whose write is `pending` or `sent`.
    pub logged_seconds: i64,
    /// Worklog seconds whose write is waiting on a person.
    pub held_seconds: i64,
    /// `tracked - logged - held`, floored at zero.
    pub unlogged_seconds: i64,
}

/// One row of the timesheet: a target, or the row that has none.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct WeekRow {
    /// The target, or `null` for the **"no target, app open"** row.
    ///
    /// `null` rather than a third `TimerTarget` variant: that enum is what a
    /// timer may be started on and what a block stores, and "nothing, but the
    /// window was in front of you" is neither. It is a row the timesheet
    /// makes, not a target anything can be attributed to -- which is also why
    /// [`plan`] skips it.
    pub target: Option<TimerTarget>,
    /// The entity's title from `knobas.entity`, or `null`.
    ///
    /// `null` for a label row and for the no-target row -- neither has an
    /// entity -- and for an entity the mirror never held or has purged, which
    /// is the split [`super::day::DayBlock`] records: a block is knobas' own
    /// record and a title is the mirror's current opinion of it.
    pub title: Option<String>,
    /// One cell per requested day, in the order they were requested.
    pub cells: Vec<WeekCell>,
}

impl WeekRow {
    /// Whether this row says anything at all.
    fn is_empty(&self) -> bool {
        self.cells.iter().all(|cell| {
            cell.tracked_seconds == 0
                && cell.offered_seconds == 0
                && cell.logged_seconds == 0
                && cell.held_seconds == 0
        })
    }
}

/// The week, as the timesheet draws it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Week {
    /// The dates asked for, in order -- the column headings.
    pub days: Vec<NaiveDate>,
    /// Targets first, in title order, then the no-target row if it has
    /// anything to say.
    pub rows: Vec<WeekRow>,
}

/// One worklog *Log all* would create, before it creates any of them.
///
/// The confirmation's line, and the acceptance criterion is that it exists
/// *before* anything is sent: a bulk write to a ticketing system is not
/// something to discover afterwards. [`log_all`] re-derives its own work
/// rather than taking this back, for the reason
/// [`worklog::log`](super::worklog::log) does not take block ids -- a caller
/// that could name the blocks could name another ticket's.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct PlannedWorklog {
    pub day: NaiveDate,
    pub entity_id: String,
    /// The mirror's title for it, or `null`. For the reader to recognise.
    pub title: Option<String>,
    /// The first covered block's start -- what the source is told.
    pub started_at: DateTime<Utc>,
    /// The covered blocks' durations added up, gaps excluded.
    pub seconds: i64,
    /// How many blocks it concatenates, so a plan of one line still says it
    /// is three sittings.
    pub blocks: i64,
}

/// The blocks of a week, one row each, with the entity's title beside it.
///
/// `kind` comes back because two different questions are asked of these rows:
/// *tracked* counts the manual ones, and *coverage* -- what the "no target"
/// row subtracts -- counts every one of them, because a passive block is still
/// a stretch the timesheet has an opinion about.
const BLOCKS: &str = "select b.started_at, b.ended_at, b.entity_id, b.label, b.kind, e.title
       from knobas.block b
       left join knobas.entity e on e.id = b.entity_id
      where b.started_at < $2 and b.ended_at > $1
      order by b.started_at, b.id";

/// The week's worklogs with the state of the write that carries each.
///
/// A `left join`, so a copy whose queue row has been swept away is still a
/// worklog that was made -- it reads as held, which is the answer that asks
/// somebody to look.
const WORKLOGS: &str = "select w.started_at, w.entity_id, w.seconds, q.state
       from knobas.worklog w
       left join knobas.write_queue q on q.id = w.write_queue_id
      where w.started_at >= $1 and w.started_at < $2";

/// The week's heartbeats, in order.
const BEATS: &str = "select at, entity_id, label, focused from knobas.heartbeat
      where at >= $1 and at < $2
      order by at, id";

/// The states in which a worklog counts as **logged** (spec story 39).
fn is_logged(state: Option<&str>) -> bool {
    matches!(state, Some("pending" | "sent"))
}

/// One block, as the aggregate sees it.
struct Stretch {
    started_at: DateTime<Utc>,
    ended_at: DateTime<Utc>,
    target: TimerTarget,
    kind: BlockKind,
    title: Option<String>,
}

/// What a row is keyed by while the week is being added up: its target, or
/// `None` for the row that has none.
///
/// The same shape [`WeekRow::target`] carries, so "which row is this" and
/// "what does this row say it is" cannot come apart.
type Key = Option<TimerTarget>;

/// Refuse a day list that is not a week the reader could have meant.
///
/// Three rules, and each of them is a wrong answer rather than a slow one: no
/// days is a read with nothing to say, more than [`MOST_DAYS`] is a scan
/// nobody asked for, and windows that overlap or run backwards would count one
/// block into two columns.
fn vet(days: &[DayWindow]) -> Result<(), IpcError> {
    if days.is_empty() {
        return Err(IpcError::invalid(
            "a timesheet needs at least one day -- the webview computes the \
             week's seven from its own clock",
        ));
    }
    if days.len() > MOST_DAYS {
        return Err(IpcError::invalid(format!(
            "{} days is more than a week; this read takes at most {MOST_DAYS}",
            days.len()
        )));
    }
    let mut reached: Option<DateTime<Utc>> = None;
    for window in days {
        if window.from >= window.to {
            return Err(IpcError::invalid(format!(
                "{} is not an interval -- its midnight is not before the next one",
                window.day
            )));
        }
        if reached.is_some_and(|end| window.from < end) {
            return Err(IpcError::invalid(format!(
                "{} overlaps the day before it, and a block inside the overlap \
                 would be counted twice",
                window.day
            )));
        }
        reached = Some(window.to);
    }
    Ok(())
}

/// Which of `days` an instant falls in, if any.
fn column(days: &[DayWindow], at: DateTime<Utc>) -> Option<usize> {
    days.iter()
        .position(|window| at >= window.from && at < window.to)
}

/// The seconds of `[from, to)` that no interval in `covers` overlaps.
///
/// Pure, and the one piece of arithmetic here that can be wrong in a way
/// nothing else notices: it is what the "no target, app open" row *is*.
/// `covers` need be neither sorted nor disjoint -- overlapping blocks have
/// been legal since #279 made them editable -- so it is walked as a sorted
/// sweep rather than subtracted one at a time.
fn uncovered_seconds(
    from: DateTime<Utc>,
    to: DateTime<Utc>,
    covers: &[(DateTime<Utc>, DateTime<Utc>)],
) -> i64 {
    let mut spans: Vec<(DateTime<Utc>, DateTime<Utc>)> = covers
        .iter()
        .map(|(began, ended)| (*began.max(&from), *ended.min(&to)))
        .filter(|(began, ended)| began < ended)
        .collect();
    spans.sort_by_key(|(began, _)| *began);

    let mut free = to - from;
    let mut reached = from;
    for (began, ended) in spans {
        let began = began.max(reached);
        if ended > began {
            free -= ended - began;
            reached = ended;
        }
    }
    free.num_seconds().max(0)
}

fn stretch_of(row: &sqlx::postgres::PgRow) -> Result<Stretch, IpcError> {
    let entity_id: Option<String> = row.try_get("entity_id")?;
    let label: Option<String> = row.try_get("label")?;
    let title: Option<String> = row.try_get("title")?;
    Ok(Stretch {
        started_at: row.try_get("started_at")?,
        ended_at: row.try_get("ended_at")?,
        target: super::target_of(entity_id, label)?,
        kind: super::BlockKind::parse(row.try_get::<String, _>("kind")?.as_str())?,
        title: title.filter(|title| !title.trim().is_empty()),
    })
}

/// The rows of a week, added up.
///
/// **Passive blocks are reconciled first, one day at a time** -- the same
/// `materialize` call [`super::day::list`] makes, for the same reason and with
/// the same per-day boundary: the derivation's cap is a rule about a day, and
/// these seven windows are the only place the reader's midnights are known. A
/// week read that skipped it would show yesterday's passive blocks (written by
/// the day review) and not today's, which is worse than showing none.
///
/// # Errors
/// [`Invalid`](crate::IpcErrorCode::Invalid) for a day list [`vet`] refuses;
/// [`IpcError`] if a read fails.
pub async fn read(pool: &PgPool, days: &[DayWindow]) -> Result<Week, IpcError> {
    vet(days)?;
    for window in days {
        passive::materialize(pool, window.from, window.to).await?;
    }

    let from = days[0].from;
    let to = days[days.len() - 1].to;

    // The answer, built in place. A `Vec<WeekRow>` and a linear scan rather
    // than a map keyed on the target: a week has a handful of targets in it,
    // and the order they first appear in is not the order the answer is sorted
    // into anyway -- so a map would buy nothing and cost the row its shape.
    let mut rows: Vec<WeekRow> = Vec::new();
    let row_for = |rows: &mut Vec<WeekRow>, target: Key, title: Option<String>| -> usize {
        match rows.iter().position(|row| row.target == target) {
            Some(at) => {
                // The first title that is not blank wins, so a row does not
                // lose its name to a block whose entity the mirror has since
                // purged.
                if rows[at].title.is_none() {
                    rows[at].title = title;
                }
                at
            }
            None => {
                rows.push(WeekRow {
                    target,
                    title,
                    cells: vec![WeekCell::default(); days.len()],
                });
                rows.len() - 1
            }
        }
    };

    // Tracked, and the coverage the no-target row is measured against.
    let mut covers: Vec<Vec<(DateTime<Utc>, DateTime<Utc>)>> = vec![Vec::new(); days.len()];
    for row in sqlx::query(BLOCKS)
        .bind(from)
        .bind(to)
        .fetch_all(pool)
        .await?
    {
        let stretch = stretch_of(&row)?;
        // The **coverage** rule, which is one of the two this file keeps: a
        // block covers every day it *overlaps*, and `uncovered_seconds` clips
        // it to the day. The question here is whether a given instant was
        // inside a block, and a stretch that ran from Monday evening into
        // Tuesday morning was covering both mornings it touched.
        for (index, window) in days.iter().enumerate() {
            if stretch.started_at < window.to && stretch.ended_at > window.from {
                covers[index].push((stretch.started_at, stretch.ended_at));
            }
        }
        // ...and the *bookkeeping* rule, which is the other one: the day it
        // started on, whole.
        let Some(index) = column(days, stretch.started_at) else {
            continue;
        };
        let at = row_for(
            &mut rows,
            Some(stretch.target.clone()),
            stretch.title.clone(),
        );
        let seconds = (stretch.ended_at - stretch.started_at).num_seconds();
        // The one place the two kinds part company, and they part into two
        // columns rather than into "counted" and "gone".
        match stretch.kind {
            BlockKind::Manual => rows[at].cells[index].tracked_seconds += seconds,
            BlockKind::Passive => rows[at].cells[index].offered_seconds += seconds,
        }
    }

    // Logged and held.
    for row in sqlx::query(WORKLOGS)
        .bind(from)
        .bind(to)
        .fetch_all(pool)
        .await?
    {
        let started_at: DateTime<Utc> = row.try_get("started_at")?;
        let entity_id: String = row.try_get("entity_id")?;
        let seconds: i64 = row.try_get("seconds")?;
        let state: Option<String> = row.try_get("state")?;
        let Some(index) = column(days, started_at) else {
            continue;
        };
        let at = row_for(&mut rows, Some(TimerTarget::Entity { entity_id }), None);
        if is_logged(state.as_deref()) {
            rows[at].cells[index].logged_seconds += seconds;
        } else {
            rows[at].cells[index].held_seconds += seconds;
        }
    }

    // The "no target, app open" row: focused time no block covered.
    //
    // Per day rather than over the week, because `focused_spans`' walk is the
    // same walk the derivation's cap is spent from and that cap is a per-day
    // rule -- so a beat on one side of midnight never credits time to the
    // other.
    let beats = sqlx::query(BEATS)
        .bind(from)
        .bind(to)
        .fetch_all(pool)
        .await?;
    let mut open = vec![WeekCell::default(); days.len()];
    for (index, window) in days.iter().enumerate() {
        let observations = beats
            .iter()
            .filter(|row| {
                row.try_get::<DateTime<Utc>, _>("at")
                    .is_ok_and(|at| at >= window.from && at < window.to)
            })
            .map(passive::observation_of)
            .collect::<Result<Vec<_>, _>>()?;
        let seconds: i64 = passive::focused_spans(&observations)
            .iter()
            .map(|span| uncovered_seconds(span.started_at, span.ended_at, &covers[index]))
            .sum();
        open[index] = WeekCell {
            tracked_seconds: seconds,
            offered_seconds: 0,
            logged_seconds: 0,
            held_seconds: 0,
            // Time with nowhere to go is time that has not been logged, and
            // saying so is what makes the week's total honest (story 41).
            // Nothing can log it: there is no target, which is why *Log all*
            // never sees this row.
            unlogged_seconds: seconds,
        };
    }

    // Unlogged last, because it is the difference of three numbers that were
    // still being added to a statement ago. **Offered is not in it**: knobas'
    // own guess is not time a person has failed to log.
    let mut out = rows;
    for row in &mut out {
        for cell in &mut row.cells {
            cell.unlogged_seconds =
                (cell.tracked_seconds - cell.logged_seconds - cell.held_seconds).max(0);
        }
    }
    out.sort_by_key(sort_key);

    let open = WeekRow {
        target: None,
        title: None,
        cells: open,
    };
    if !open.is_empty() {
        // Last, and only when there is something to say. A row of zeros on
        // every week nobody had passive attribution switched on for would be
        // a permanent line saying nothing, and a reader cannot tell that from
        // "the app was shut all week" -- which is the same reading, and the
        // honest one, when the row is simply not there.
        out.push(open);
    }

    Ok(Week {
        days: days.iter().map(|window| window.day).collect(),
        rows: out,
    })
}

/// Entity rows first by title, then label rows, and the no-target row last.
///
/// By the reader's own words -- the mirror's title, else the id, else the
/// label -- because that is what the row is drawn with; sorting by entity id
/// would put the rows in an order the screen cannot explain.
fn sort_key(row: &WeekRow) -> (u8, String) {
    match (&row.target, &row.title) {
        (Some(TimerTarget::Entity { entity_id }), title) => (
            0,
            title
                .clone()
                .unwrap_or_else(|| entity_id.clone())
                .to_lowercase(),
        ),
        (Some(TimerTarget::Label { label }), _) => (1, label.to_lowercase()),
        (None, _) => (2, String::new()),
    }
}

/// What *Log all* would send, before it sends any of it.
///
/// One entry per **(day, ticket)** with unlogged manual blocks on it, which is
/// the whole of the spec's rule: "one worklog per day and ticket from that
/// day's unlogged manual blocks … so that a whole week logs correctly rather
/// than as one lump on Friday".
///
/// Three exclusions, each of them a rule rather than a filter:
///
/// * **Passive blocks**, because a passive block is knobas' guess and no
///   passive block is ever logged without a person saying so (spec story 30).
///   The narrowing is [`worklog`]'s `UNLOGGED_BLOCKS`, reused rather than
///   restated, so *Log all* and the draft cannot come to disagree about what
///   is loggable.
/// * **Label blocks**, because a label is not somewhere a worklog can go.
///   Story 25 gives label time its own path -- *Log to a ticket…* on the day
///   review -- and that path is a person choosing a ticket, which is exactly
///   what a bulk action must not do on their behalf.
/// * **Blocks that already carry a worklog id**, which is the same clause and
///   is what makes *Log all* idempotent: run it twice and the second run has
///   nothing to send.
///
/// A target whose source does not declare `log_work` is skipped too, for the
/// reason the draft asks the descriptor rather than a list of kinds.
///
/// # Errors
/// [`Invalid`](crate::IpcErrorCode::Invalid) for a day list [`vet`] refuses;
/// [`IpcError`] if a read fails.
pub async fn plan(
    pool: &PgPool,
    registry: &dyn knobas_sync::scheduler::AdapterRegistry,
    days: &[DayWindow],
) -> Result<Vec<PlannedWorklog>, IpcError> {
    vet(days)?;
    let mut out = Vec::new();
    for window in days {
        for (entity_id, title) in loggable(pool, registry, window).await? {
            let spans = worklog::unlogged(pool, &entity_id, window.from, window.to).await?;
            let Some((started_at, _, seconds)) = worklog::concatenate(&spans) else {
                continue;
            };
            if seconds <= 0 {
                // A day of blocks that add up to nothing -- a timer started and
                // stopped inside a second, or one the relaunch sweep closed
                // before its first heartbeat. `worklog_seconds_chk` refuses a
                // worklog of no time, so a plan that offered one would be a
                // confirmation whose only outcome is an error.
                continue;
            }
            out.push(PlannedWorklog {
                day: window.day,
                entity_id,
                title,
                started_at,
                seconds,
                blocks: spans.len() as i64,
            });
        }
    }
    Ok(out)
}

/// The entities with unlogged manual blocks on `window`, whose source takes a
/// worklog, with the mirror's title for each.
///
/// The `kind = 'manual'` and `worklog_id is null` narrowing is stated once, in
/// [`worklog`], and this asks the same question of the whole day rather than
/// of one entity: `distinct` over the same predicate.
const LOGGABLE: &str = "select distinct b.entity_id, e.title
       from knobas.block b
       left join knobas.entity e on e.id = b.entity_id
      where b.entity_id is not null and b.worklog_id is null and b.kind = 'manual'
        and b.started_at >= $1 and b.started_at < $2
      order by e.title nulls last, b.entity_id";

async fn loggable(
    pool: &PgPool,
    registry: &dyn knobas_sync::scheduler::AdapterRegistry,
    window: &DayWindow,
) -> Result<Vec<(String, Option<String>)>, IpcError> {
    let rows = sqlx::query(LOGGABLE)
        .bind(window.from)
        .bind(window.to)
        .fetch_all(pool)
        .await?;
    let mut out = Vec::new();
    for row in &rows {
        let entity_id: String = row.try_get("entity_id")?;
        if !worklog::takes_a_worklog_for(pool, registry, &entity_id).await? {
            continue;
        }
        let title: Option<String> = row.try_get("title")?;
        out.push((entity_id, title.filter(|title| !title.trim().is_empty())));
    }
    Ok(out)
}

/// Log every planned worklog, one per (day, ticket).
///
/// The worklogs it made, in the order it made them. **The work is re-derived**
/// rather than taken from a plan the caller hands back: a plan is what the
/// confirmation was drawn from, and between drawing it and confirming it a
/// timer may have stopped or another window may have logged the same
/// afternoon. [`worklog`]'s own read is asked again, per day and per ticket,
/// which is also what makes the comment and the interval the same ones a
/// single *Log* would have produced.
///
/// **The comment is empty**, and that is a decision rather than an omission.
/// A worklog with no words is a worklog (story 33), and the alternative --
/// generating the draft's candidate bullets for every ticket of a week without
/// anybody reading one of them -- would send text nobody ticked to a system
/// other people read. A reader who wants the bullets logs that day from the
/// draft, where the boxes are.
///
/// A day and ticket that fails is **not** rolled back and does not stop the
/// rest: each worklog is its own write through the queue, the copy exists
/// before anything is sent, and a failure that undid the copies would be the
/// one thing ADR-0012 forbids -- losing knobas' record of a write that may
/// already have landed. The failure is returned once every other day has been
/// logged.
///
/// # Errors
/// [`Invalid`](crate::IpcErrorCode::Invalid) for a day list [`vet`] refuses;
/// otherwise whatever the queue or the database said, after the rest of the
/// week has been logged.
pub async fn log_all(
    state: &crate::sources::SourcesState,
    days: &[DayWindow],
) -> Result<Vec<worklog::Worklog>, IpcError> {
    vet(days)?;
    let planned = plan(&state.pool, state.registry.as_ref(), days).await?;
    let mut made = Vec::new();
    let mut failure: Option<IpcError> = None;
    for entry in &planned {
        let Some(window) = days.iter().find(|window| window.day == entry.day) else {
            continue;
        };
        match worklog::log_within(state, &entry.entity_id, window.from, window.to, "").await {
            Ok(worklog) => made.push(worklog),
            Err(error) => failure = failure.or(Some(error)),
        }
    }
    match failure {
        Some(error) => Err(error),
        None => Ok(made),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at(day: u32, hour: u32, minute: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, day, hour, minute, 0).unwrap()
    }

    fn window(day: u32) -> DayWindow {
        DayWindow {
            day: NaiveDate::from_ymd_opt(2026, 8, day).unwrap(),
            from: at(day, 0, 0),
            to: at(day + 1, 0, 0),
        }
    }

    /// **The number the "no target" row is**, with the three shapes that can
    /// each be wrong on their own: a block inside the span, two blocks that
    /// overlap each other, and a block that runs off both ends of it.
    #[test]
    fn uncovered_time_is_the_span_minus_the_union_of_what_covers_it() {
        let span = (at(24, 9, 0), at(24, 12, 0));

        assert_eq!(
            uncovered_seconds(span.0, span.1, &[]),
            3 * 3600,
            "nothing covered is the whole span"
        );
        assert_eq!(
            uncovered_seconds(span.0, span.1, &[(at(24, 10, 0), at(24, 11, 0))]),
            2 * 3600,
            "one hour of it is claimed"
        );
        assert_eq!(
            uncovered_seconds(
                span.0,
                span.1,
                &[
                    (at(24, 10, 0), at(24, 11, 0)),
                    (at(24, 10, 30), at(24, 11, 30))
                ],
            ),
            90 * 60,
            "overlapping blocks are legal (#279) and must be counted once, not twice"
        );
        assert_eq!(
            uncovered_seconds(span.0, span.1, &[(at(24, 8, 0), at(24, 13, 0))]),
            0,
            "a block running off both ends is clipped, not counted past them"
        );
    }

    /// A day list that would count one block into two columns is refused, and
    /// so is one that is not a week.
    #[test]
    fn a_day_list_that_would_double_count_is_refused() {
        assert!(vet(&[window(24), window(25)]).is_ok());
        assert_eq!(
            vet(&[]).expect_err("no days is no timesheet").code,
            crate::IpcErrorCode::Invalid
        );
        assert_eq!(
            vet(&[window(24), window(24)])
                .expect_err("the same day twice overlaps itself")
                .code,
            crate::IpcErrorCode::Invalid
        );
        let mut backwards = window(24);
        backwards.to = backwards.from;
        assert_eq!(
            vet(&[backwards])
                .expect_err("an empty day is not a day")
                .code,
            crate::IpcErrorCode::Invalid
        );
    }

    /// **A pending worklog is logged and a held one is not** -- spec story 39,
    /// which is the one rule in this file a reasonable person would write the
    /// other way round.
    #[test]
    fn a_pending_worklog_is_logged_and_everything_else_is_held() {
        assert!(
            is_logged(Some("pending")),
            "sync timing is not the question"
        );
        assert!(is_logged(Some("sent")));
        assert!(!is_logged(Some("held")), "a held worklog shows as held");
        assert!(!is_logged(Some("refused")));
        assert!(!is_logged(Some("discarded")));
        assert!(
            !is_logged(None),
            "a copy with no queue row wants looking at"
        );
    }
}
