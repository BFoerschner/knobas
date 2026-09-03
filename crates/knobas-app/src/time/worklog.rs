//! The worklog: a day's blocks on a ticket, drafted, logged and kept a copy of
//! (spec #272 "Worklog draft and ad-hoc block", issue #280).
//!
//! `commands/time.rs` is shims over this, the arrangement [`super`] records.
//!
//! # What this module is for
//!
//! A [`Block`](super::Block) is knobas' own record of time and never leaves the
//! machine. A **worklog** is what one or more blocks *become* when a person
//! decides Jira should know about them: one write-back, through the write queue
//! like any other, with a local copy that outlives the queue row.
//!
//! Three decisions live here and nowhere else.
//!
//! **The interval is the day's blocks concatenated, and its length is their
//! sum.** Not `ended - started`: an afternoon with a lunch in it is two blocks
//! with a gap, and logging the span would bill the lunch. The two are different
//! numbers and both are on the draft, because the reader needs to see the
//! window the work sat in *and* the time that is about to be logged.
//!
//! **The candidates are a proposal, never a record.** They are what knobas can
//! see the reader did in that window -- items in the mirror authored by the
//! username their source is configured with, and their own activity lines --
//! and each one is a checkbox. Nothing is logged because a candidate exists;
//! the comment is generated from what is checked and is editable down to
//! empty.
//!
//! **A worklog's local copy is written before the write lands.** The row is
//! the record that a person logged their day; whether Jira has taken it yet is
//! the write queue's state, which is where it belongs. A copy written only on
//! success would leave a refused worklog with no trace of the hours it was made
//! of, and the blocks it covered would come back up for logging with nothing to
//! say they had been sent once already.
//!
//! # Why `log` queues before it writes the copy
//!
//! Because **nothing may be sent to Jira before knobas' own record of it
//! exists.** The copy is what marks the blocks as spent, so a crash between
//! the send and the copy would leave a worklog on the ticket with no trace of
//! it here and the same afternoon offered for logging again. `submit` has that
//! gap by construction, which is why `knobas_sync::write_queue::queue` exists
//! beside it and why the order here is queue, copy, flush.
//!
//! # And why the id survives the other order anyway
//!
//! The id Jira gives a worklog exists exactly once: in the answer to the POST,
//! inside the flush loop. The scheduler flushes every source on its own tick,
//! so a tick can settle this write in the gap above -- with no copy for the
//! settle's stamp to find. `knobas_core::write_queue::sent` therefore records
//! the id on the **queue row** as well, and [`keep`] takes it from there. Two
//! writers, one value, no ordering required; migration `0014` records the
//! reasoning beside the column.

use chrono::{DateTime, Duration, FixedOffset, NaiveDate, Utc};
use knobas_core::entity::EntityRef;
use knobas_source::WriteOp;
use sqlx::{PgPool, Row};

use crate::IpcError;
use crate::sources::SourcesState;

/// The op identifier a source must declare before knobas offers to log time
/// against its items -- `knobas_source::WriteOp::LogWork`'s.
///
/// Spelled here rather than read off the enum so that this module states the
/// rule in the same words a descriptor does; `the_identifier_is_the_one_the_spi_defines`
/// pins the two together, so a rename in the SPI fails here instead of leaving
/// the draft quietly unavailable on every source.
const LOG_WORK: &str = "log_work";

/// The actor whose activity lines are the reader's own.
const ACTOR: &str = "user";

/// Verbs that are knobas talking about itself, and never a bullet in a
/// worklog comment.
///
/// A list, because an activity line carries no flag saying whether it is work:
/// the timer's own two verbs (`super`) and the write queue's eight
/// (`knobas_sync::write_queue`'s `announce` call sites) are all written with
/// actor `user`, because a person did press the button -- but "queued a write"
/// is not something to tell Jira about the afternoon.
///
/// **The failure direction is deliberate.** A verb that ought to be here and
/// is not shows up as one extra checkbox the reader unchecks; nothing is ever
/// lost by an omission, and no bullet is ever added to a comment the reader
/// did not tick. So a new bookkeeping verb costs a moment's noise rather than
/// a wrong worklog.
const NOT_WORK: &[&str] = &[
    // The timer's own (`super::start`, `super::stop`).
    "started",
    "stopped",
    // The write queue's (`knobas_sync::write_queue::announce`).
    "queued",
    "released",
    "amended",
    "discarded",
    "held",
    "sent",
    "refused",
    "waiting",
];

/// One thing knobas saw the reader do inside a draft's interval.
///
/// A proposal for a bullet, not a record of anything: the draft renders these
/// as checkboxes and the comment is built from the ticked ones.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Candidate {
    /// Stable within a draft, and the key the checkbox list is drawn on --
    /// `"item:jira:PAY-231"`, `"activity:4211"`. Two candidates can share an
    /// entity (a commit and the activity line about it), so the entity id
    /// alone would not do.
    pub id: String,
    /// Where this candidate was seen.
    pub source: CandidateSource,
    /// When, which is what puts the list in order.
    pub at: DateTime<Utc>,
    /// The entity it is about, when there is one.
    pub entity_id: Option<String>,
    /// **The line this candidate contributes to the comment, verbatim.**
    ///
    /// Composed here rather than in the webview so that ticking a box is the
    /// only thing the frontend decides: it joins the bullets of what is
    /// checked, and the wording of a bullet is one rule in one language. A
    /// frontend that formatted its own would be a second spelling of every
    /// comment knobas has ever sent.
    pub bullet: String,
}

/// Where a [`Candidate`] was seen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CandidateSource {
    /// An item in the mirror whose author is the username that item's source
    /// is configured with -- a commit, a pull request, a build, a page.
    Mirror,
    /// A line the reader's own actions wrote in `knobas.activity`.
    Activity,
}

/// What the draft offers: one interval, its candidates, and a comment made
/// from them.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Draft {
    /// The ticket the time goes on.
    pub entity_id: String,
    /// The reader's day, as they named it.
    pub day: NaiveDate,
    /// The first block's start, which is what Jira is told the work began at.
    pub started_at: DateTime<Utc>,
    /// The last block's end. **Not** `started_at + seconds`: it is the far
    /// edge of the window, and the difference between the two is the gaps.
    pub ended_at: DateTime<Utc>,
    /// The time that will be logged -- the blocks' durations added up.
    pub seconds: i64,
    /// The blocks this draft is made of, oldest first.
    pub block_ids: Vec<i64>,
    /// What knobas saw in the interval, for the reader to tick.
    pub candidates: Vec<Candidate>,
    /// The comment as generated from **every** candidate, which is what the
    /// draft opens with. Editable, down to empty.
    pub comment: String,
}

/// knobas' copy of a worklog it has sent, or is still sending.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Worklog {
    pub id: i64,
    pub entity_id: String,
    pub started_at: DateTime<Utc>,
    pub seconds: i64,
    pub comment: String,
    /// The blocks this worklog was made of. They are read-only from now on:
    /// each carries `worklog_id` pointing back here.
    pub block_ids: Vec<i64>,
    /// The write that carries it. Read its state with `pending_writes`.
    pub write_queue_id: Option<i64>,
    /// What Jira called it, once Jira has answered -- `null` while the write
    /// is still owed, and after one that was refused.
    pub remote_id: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// The half-open UTC interval one local day occupies.
///
/// `offset_minutes` is the reader's own offset from UTC, sent by the webview,
/// because **the reader's machine is the only thing that knows which day they
/// mean**. The database's timezone is the server's, and a `date_trunc` here
/// would file a Berlin evening's blocks under the following day for a reader
/// who was never in UTC.
///
/// A fixed offset rather than a named zone, and the cost is stated rather than
/// hidden: on the two days a year a zone changes offset, the window is an hour
/// out at one end. The interval is editable in the draft, which is the remedy;
/// carrying a zone database into the bridge for those two days is not.
fn day_bounds(
    day: NaiveDate,
    offset_minutes: i32,
) -> Result<(DateTime<Utc>, DateTime<Utc>), IpcError> {
    let offset = FixedOffset::east_opt(offset_minutes * 60).ok_or_else(|| {
        IpcError::invalid(format!(
            "{offset_minutes} is not a usable offset from UTC -- the value is \
             the reader's own `getTimezoneOffset`, negated"
        ))
    })?;
    let start = day
        .and_hms_opt(0, 0, 0)
        .and_then(|midnight| midnight.and_local_timezone(offset).single())
        .ok_or_else(|| {
            IpcError::invalid(format!(
                "{day} has no midnight at {offset_minutes:+} minutes"
            ))
        })?
        .with_timezone(&Utc);
    Ok((start, start + Duration::days(1)))
}

/// Whether a worklog can go to this entity's source at all.
///
/// **Asked of the adapter, never of a kind list.** A worklog goes where the
/// source says it takes one: `SourceDescriptor::write_ops` is what the action
/// bar is rendered from everywhere else in knobas, and a draft that offered
/// itself on a Gitea commit would be an action the queue refuses after the
/// reader has typed a comment. Spec §3a is the same rule from the other side --
/// a hardcoded "tickets only" table is exactly what the descriptor exists to
/// replace.
///
/// The namespace is an **instance** id and the descriptors are per *kind*
/// (§4.2), so the configuration row is what joins them -- the same hop
/// `sources::write_queue::submittable` makes, for the same reason: a second
/// Jira called `jira-eu` declares what every Jira declares, and matching the
/// namespace against a template's id would find nothing.
///
/// `false` for a source that is not configured, which is what a stop on an
/// entity whose source has since been removed reads as.
async fn takes_a_worklog(
    pool: &PgPool,
    registry: &dyn knobas_sync::scheduler::AdapterRegistry,
    namespace: &str,
) -> Result<bool, IpcError> {
    let Some(source) = knobas_sync::config::get(pool, namespace)
        .await
        .map_err(IpcError::internal)?
    else {
        return Ok(false);
    };
    Ok(registry.descriptors().into_iter().any(|d| {
        d.adapter_kind == source.adapter_kind && d.write_ops.iter().any(|op| op == LOG_WORK)
    }))
}

/// The blocks of one local day on one entity that have not been logged yet.
///
/// **A block belongs to the day it started on**, and a timer left running
/// across midnight is one block on the earlier day rather than two. Splitting
/// it would invent a boundary nobody made, in a table whose whole point is
/// that every row is a stretch somebody actually worked.
///
/// **`kind = 'manual'` only.** `0013`'s vocabulary has a second kind --
/// `passive`, which #282 derives from what was open on screen -- and a passive
/// block is knobas' *guess* at an afternoon rather than a person's account of
/// one. Drafting one would put minutes nobody vouched for into a worklog that
/// bills a client, and the read that offers blocks for logging is the place
/// that distinction has to be made, because [`log`] re-derives the covered
/// blocks from this same query. #282's own surface is where a passive block is
/// assigned and thereby becomes a person's claim; until it has been through
/// that, it is not something to log.
const UNLOGGED_BLOCKS: &str = "select id, started_at, ended_at from knobas.block
     where entity_id = $1 and worklog_id is null and kind = 'manual'
       and started_at >= $2 and started_at < $3
     order by started_at, id";

/// Every column of a worklog row that leaves this module.
macro_rules! worklog_columns {
    () => {
        "id, entity_id, started_at, seconds, comment, block_ids, write_queue_id, \
         remote_id, created_at"
    };
}

fn worklog_of(row: &sqlx::postgres::PgRow) -> Result<Worklog, IpcError> {
    Ok(Worklog {
        id: row.try_get("id")?,
        entity_id: row.try_get("entity_id")?,
        started_at: row.try_get("started_at")?,
        seconds: row.try_get("seconds")?,
        comment: row.try_get("comment")?,
        block_ids: row.try_get("block_ids")?,
        write_queue_id: row.try_get("write_queue_id")?,
        remote_id: row.try_get("remote_id")?,
        created_at: row.try_get("created_at")?,
    })
}

/// One stretch, as the interval maths sees it.
struct Span {
    id: i64,
    started_at: DateTime<Utc>,
    ended_at: DateTime<Utc>,
}

async fn unlogged(
    pool: &PgPool,
    entity_id: &str,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> Result<Vec<Span>, IpcError> {
    let rows = sqlx::query(UNLOGGED_BLOCKS)
        .bind(entity_id)
        .bind(from)
        .bind(to)
        .fetch_all(pool)
        .await?;
    rows.iter()
        .map(|row| {
            Ok(Span {
                id: row.try_get("id")?,
                started_at: row.try_get("started_at")?,
                ended_at: row.try_get("ended_at")?,
            })
        })
        .collect()
}

/// The three numbers a set of spans concatenates to: first start, last end,
/// and **the sum of the durations**.
///
/// Pure, and separate from the read, because the one thing about this that can
/// be wrong in a way nobody notices is the third number. `ended - started`
/// agrees with the sum on every unbroken afternoon and disagrees by exactly
/// the lunch on every other one, so a test with a gap in it is the only test
/// that can tell them apart.
fn concatenate(spans: &[Span]) -> Option<(DateTime<Utc>, DateTime<Utc>, i64)> {
    let first = spans.first()?;
    let started_at = first.started_at;
    let ended_at = spans.iter().map(|s| s.ended_at).max()?;
    let seconds = spans
        .iter()
        .map(|s| (s.ended_at - s.started_at).num_seconds())
        .sum();
    Some((started_at, ended_at, seconds))
}

/// Items in the mirror, inside the interval, authored by the username the
/// item's **own source** is configured with.
///
/// The join is what makes "authored by me" mean anything across sources: a
/// person is `mara.lindqvist` in Jira and `mlindqvist` in Gitea, and
/// `knobas.source_config.config->>'username'` is where each of those was
/// entered (§4.2 gives every adapter that key). Comparing an item's author
/// against one global name would silently miss every source but one.
///
/// Case-insensitively, because the three products disagree about the case they
/// echo a username back in and none of them treats it as significant.
const AUTHORED_ITEMS: &str = "select i.entity_id, i.title, i.item_updated_at
       from sync.live_item i
       join knobas.source_config s on s.id = i.source_id
      where i.item_updated_at >= $1 and i.item_updated_at <= $2
        and i.author is not null
        and coalesce(s.config->>'username', '') <> ''
        and lower(i.author) = lower(s.config->>'username')
      order by i.item_updated_at, i.entity_id";

/// The reader's own activity lines inside the interval, minus knobas'
/// housekeeping.
///
/// `entity_id is not null` for the same reason [`NOT_WORK`] exists: a line
/// with nothing behind it has nothing to say about the afternoon.
const MY_ACTIVITY: &str = "select id, at, verb, entity_id from knobas.activity
      where actor = $1 and at >= $2 and at <= $3
        and entity_id is not null
        and verb <> all($4)
      order by at, id";

/// The key half of an entity id -- `PAY-231` out of `jira:PAY-231`.
///
/// The same half `Detail.svelte`'s header and the timer strip read, so a
/// bullet says the word a person would say.
fn key_of(entity_id: &str) -> &str {
    match entity_id.find(':') {
        Some(at) => &entity_id[at + 1..],
        None => entity_id,
    }
}

async fn candidates(
    pool: &PgPool,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> Result<Vec<Candidate>, IpcError> {
    let mut out: Vec<Candidate> = Vec::new();

    for row in sqlx::query(AUTHORED_ITEMS)
        .bind(from)
        .bind(to)
        .fetch_all(pool)
        .await?
    {
        let entity_id: String = row.try_get("entity_id")?;
        let title: String = row.try_get("title")?;
        let at: DateTime<Utc> = row.try_get("item_updated_at")?;
        out.push(Candidate {
            id: format!("item:{entity_id}"),
            source: CandidateSource::Mirror,
            at,
            bullet: format!("- {title}"),
            entity_id: Some(entity_id),
        });
    }

    for row in sqlx::query(MY_ACTIVITY)
        .bind(ACTOR)
        .bind(from)
        .bind(to)
        .bind(NOT_WORK)
        .fetch_all(pool)
        .await?
    {
        let id: i64 = row.try_get("id")?;
        let verb: String = row.try_get("verb")?;
        let entity_id: Option<String> = row.try_get("entity_id")?;
        let at: DateTime<Utc> = row.try_get("at")?;
        let what = entity_id.as_deref().map(key_of).unwrap_or_default();
        out.push(Candidate {
            id: format!("activity:{id}"),
            source: CandidateSource::Activity,
            at,
            bullet: format!("- {verb} {what}").trim_end().to_owned(),
            entity_id,
        });
    }

    out.sort_by(|a, b| a.at.cmp(&b.at).then_with(|| a.id.cmp(&b.id)));
    Ok(out)
}

/// The comment a set of candidates makes: one bullet each, in order.
///
/// The frontend re-runs exactly this when a box is unticked -- it joins the
/// bullets it still has -- which is why a bullet is a whole line and this is a
/// join rather than a format.
fn comment_of(candidates: &[Candidate]) -> String {
    candidates
        .iter()
        .map(|c| c.bullet.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

/// The source a target belongs to, refusing anything that is not a target at
/// all.
fn source_of(entity_id: &str) -> Result<EntityRef, IpcError> {
    EntityRef::parse(entity_id).map_err(IpcError::invalid)
}

/// The draft for one ticket and one of the reader's days, or `None` when there
/// is nothing to draft.
///
/// `None` rather than a refusal, and it is the answer to two different
/// questions on purpose, because the caller does the same thing with both:
/// **the shell asks for a draft on every stop and opens the modal only if it
/// got one.** A stop on a note, on a Gitea commit or on an ad-hoc label is not
/// an error to apologise for, and neither is a stop on a ticket whose blocks
/// were all logged an hour ago.
///
/// # Errors
///
/// `invalid` for a target that is not an entity id or an offset that is not an
/// offset, and [`IpcError`] if a read fails.
pub async fn draft(
    pool: &PgPool,
    registry: &dyn knobas_sync::scheduler::AdapterRegistry,
    entity_id: &str,
    day: NaiveDate,
    offset_minutes: i32,
) -> Result<Option<Draft>, IpcError> {
    let entity = source_of(entity_id)?;
    if !takes_a_worklog(pool, registry, &entity.namespace).await? {
        return Ok(None);
    }
    let (from, to) = day_bounds(day, offset_minutes)?;
    let spans = unlogged(pool, entity_id, from, to).await?;
    let Some((started_at, ended_at, seconds)) = concatenate(&spans) else {
        return Ok(None);
    };

    // The candidates' window is the **interval**, not the day: the question a
    // draft asks is "what were you doing while the clock was on this", and a
    // day-wide window would offer this morning's other ticket as a bullet.
    let candidates = candidates(pool, started_at, ended_at).await?;
    Ok(Some(Draft {
        entity_id: entity_id.to_owned(),
        day,
        started_at,
        ended_at,
        seconds,
        block_ids: spans.iter().map(|s| s.id).collect(),
        comment: comment_of(&candidates),
        candidates,
    }))
}

/// Write the local copy and mark every block it covers, in one statement.
///
/// One statement because the two halves are one fact: a worklog that exists
/// over blocks that are still offered for logging would be logged twice, and a
/// block pointing at a worklog that was never written is a block nothing can
/// give back.
/// The `remote_id` is read **off the queue row this copy names**, rather than
/// left null for the settle to fill in, and that is the fix for an ordering
/// nothing can enforce: the scheduler flushes on its own tick, so the write
/// this copy belongs to may already have settled -- with no copy for the
/// settle's own stamp to have found. `knobas_core::write_queue::sent` keeps
/// the id on the queue row for exactly this reason, and migration `0014`
/// records it. A write that has not landed yet reads `null` here and is
/// stamped by the settle in the ordinary way.
const LOG: &str = "with made as (
       insert into knobas.worklog
         (entity_id, started_at, seconds, comment, block_ids, write_queue_id,
          remote_id)
       select $1, $2, $3, $4, $5, $6, q.remote_id
         from knobas.write_queue q where q.id = $6
       returning id, entity_id, started_at, seconds, comment, block_ids,
                 write_queue_id, remote_id, created_at
     ), marked as (
       update knobas.block b set worklog_id = made.id
         from made where b.id = any(made.block_ids)
     )
     select * from made";

/// Write the local copy of a worklog, and spend the blocks it covers.
///
/// The middle step of [`log`], and public for one reason: it is the seam where
/// **the order of the copy and the flush stops mattering**, and a test has to
/// be able to put the copy second (`tests/worklog_ipc.rs`,
/// `a_settle_that_beat_the_copy_still_gives_it_the_id`). Nothing in production
/// calls it but `log`.
///
/// `write_queue_id` names a row that already exists; the copy takes its
/// `remote_id` from that row, so a write a scheduler tick has already settled
/// is not a worklog knobas can never name.
///
/// # Errors
///
/// [`IpcError`] if the write fails, including when `write_queue_id` names no
/// row -- there is then nothing to copy from and nothing to flush.
pub async fn keep(
    pool: &PgPool,
    entity_id: &str,
    started_at: DateTime<Utc>,
    seconds: i64,
    comment: &str,
    block_ids: &[i64],
    write_queue_id: i64,
) -> Result<Worklog, IpcError> {
    let row = sqlx::query(LOG)
        .bind(entity_id)
        .bind(started_at)
        .bind(seconds)
        .bind(comment)
        .bind(block_ids)
        .bind(write_queue_id)
        .fetch_one(pool)
        .await?;
    worklog_of(&row)
}

/// Log a day's work on a ticket: queue the write, keep the copy, and make the
/// blocks read-only.
///
/// `started_at`, `seconds` and `comment` are **the reader's**, edited in the
/// draft or not; which blocks are covered is not, and is re-derived here from
/// the same rule [`draft`] used. That asymmetry is deliberate: the interval and
/// the words are a person's account of their afternoon, and knobas has no
/// business overruling them, but *which of knobas' own rows are now spoken
/// for* is knobas' to decide -- a caller that could name them could name
/// another ticket's, or the same ones twice.
///
/// # Errors
///
/// `invalid` for a target that is not an entity id, a source that does not take
/// worklogs, or a duration that is not positive; `conflict` when the day has
/// no unlogged blocks left on this ticket; otherwise whatever the queue or the
/// database says.
pub async fn log(
    state: &SourcesState,
    entity_id: &str,
    day: NaiveDate,
    offset_minutes: i32,
    started_at: DateTime<Utc>,
    seconds: i64,
    comment: &str,
) -> Result<Worklog, IpcError> {
    let entity = source_of(entity_id)?;
    if !takes_a_worklog(&state.pool, state.registry.as_ref(), &entity.namespace).await? {
        return Err(IpcError::invalid(format!(
            "{entity_id} belongs to a source that does not take worklogs -- \
             `{LOG_WORK}` is not in what its adapter declares"
        )));
    }
    if seconds <= 0 {
        return Err(IpcError::invalid(
            "a worklog of no time is not a worklog -- edit the interval or leave \
             the blocks unlogged",
        ));
    }
    let (from, to) = day_bounds(day, offset_minutes)?;
    let spans = unlogged(&state.pool, entity_id, from, to).await?;
    if spans.is_empty() {
        return Err(IpcError::conflict(format!(
            "there is no unlogged time on {entity_id} for {day} -- it has been \
             logged already, or the blocks are on another day"
        )));
    }
    let block_ids: Vec<i64> = spans.iter().map(|s| s.id).collect();

    // **Queue first, copy second, flush third** -- see this module's docs. The
    // copy has to carry the queue row's id before anything can settle it,
    // because the settle is what stamps Jira's worklog id onto the copy and it
    // stamps by that id.
    let op = WriteOp::LogWork {
        entity: entity_id.to_owned(),
        started: started_at,
        seconds,
        comment: comment.to_owned(),
    };
    let payload = serde_json::to_value(&op).map_err(IpcError::internal)?;
    let (queued, source_id) = crate::sources::write_queue::queue(state, payload).await?;

    let worklog = keep(
        &state.pool,
        entity_id,
        started_at,
        seconds,
        comment,
        &block_ids,
        queued.id,
    )
    .await?;

    crate::sources::write_queue::flush(state, &source_id, queued.id).await?;

    // Read back rather than answer with what was just written: the flush above
    // may have settled the write, and if it did, the copy carries Jira's id
    // now. The value in hand was correct one statement ago and says `null`.
    let settled = sqlx::query(concat!(
        "select ",
        worklog_columns!(),
        " from knobas.worklog where id = $1"
    ))
    .bind(worklog.id)
    .fetch_optional(&state.pool)
    .await?;
    settled.as_ref().map_or(Ok(worklog), worklog_of)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(hour: u32, minute: u32) -> DateTime<Utc> {
        use chrono::TimeZone;
        Utc.with_ymd_and_hms(2026, 9, 3, hour, minute, 0).unwrap()
    }

    fn span(from: (u32, u32), to: (u32, u32), id: i64) -> Span {
        Span {
            id,
            started_at: at(from.0, from.1),
            ended_at: at(to.0, to.1),
        }
    }

    /// The one number in this module that can be wrong invisibly: an
    /// afternoon with a lunch in it.
    ///
    /// 09:00--10:30 and 13:00--14:00 is a **two and a half hour** worklog
    /// inside a five hour window. `ended - started` would bill the lunch, and
    /// on any unbroken afternoon the two agree -- which is why the gap is the
    /// fixture rather than a detail of it.
    #[test]
    fn a_concatenated_interval_logs_the_time_worked_not_the_window_it_sat_in() {
        let spans = [span((9, 0), (10, 30), 1), span((13, 0), (14, 0), 2)];
        let (started_at, ended_at, seconds) = concatenate(&spans).expect("two blocks concatenate");
        assert_eq!(
            started_at,
            at(9, 0),
            "the interval starts at the first block"
        );
        assert_eq!(ended_at, at(14, 0), "...and ends at the last one");
        assert_eq!(seconds, 150 * 60, "two and a half hours were worked");
        assert_eq!(
            (ended_at - started_at).num_seconds(),
            300 * 60,
            "...inside a five hour window, which is the number this must not log"
        );
    }

    #[test]
    fn no_blocks_is_no_interval() {
        assert!(concatenate(&[]).is_none());
    }

    /// A day is the reader's day, and the reader's machine says which one.
    #[test]
    fn a_day_is_the_readers_day_and_not_the_servers() {
        let day = NaiveDate::from_ymd_opt(2026, 9, 3).unwrap();

        let (from, to) = day_bounds(day, 0).expect("UTC is an offset");
        assert_eq!(from.to_rfc3339(), "2026-09-03T00:00:00+00:00");
        assert_eq!((to - from).num_hours(), 24);

        // Berlin in summer: the reader's day begins at 22:00 the evening
        // before, in UTC. A `date_trunc` on the server would file that
        // evening's blocks under the wrong day.
        let (from, _) = day_bounds(day, 120).expect("+02:00 is an offset");
        assert_eq!(from.to_rfc3339(), "2026-09-02T22:00:00+00:00");

        // ...and the other direction, so the sign is pinned rather than
        // guessed: a Chicago reader's day begins at 05:00 UTC.
        let (from, _) = day_bounds(day, -300).expect("-05:00 is an offset");
        assert_eq!(from.to_rfc3339(), "2026-09-03T05:00:00+00:00");
    }

    #[test]
    fn an_offset_that_is_not_an_offset_is_refused() {
        let refusal = day_bounds(NaiveDate::from_ymd_opt(2026, 9, 3).unwrap(), 100_000)
            .expect_err("a day cannot be 70 hours from UTC");
        assert_eq!(refusal.code, crate::IpcErrorCode::Invalid);
    }

    /// The identifier this module gates the draft on is the one the SPI
    /// actually defines. Without this, a rename in `WriteOp::identifier` would
    /// leave the draft matching a word no descriptor declares -- green here,
    /// and never offering itself anywhere.
    #[test]
    fn the_identifier_is_the_one_the_spi_defines() {
        let op = WriteOp::LogWork {
            entity: "jira:PAY-231".to_owned(),
            started: at(9, 0),
            seconds: 60,
            comment: String::new(),
        };
        assert_eq!(op.identifier(), LOG_WORK);
    }

    fn candidate(id: &str, bullet: &str) -> Candidate {
        Candidate {
            id: id.to_owned(),
            source: CandidateSource::Mirror,
            at: at(9, 0),
            entity_id: None,
            bullet: bullet.to_owned(),
        }
    }

    /// One bullet per candidate, in order, and nothing else -- no heading, no
    /// trailing newline. The frontend joins the same way when a box is
    /// unticked, so anything this added would have to be added there too.
    #[test]
    fn the_comment_is_the_bullets_and_nothing_else() {
        let generated = comment_of(&[
            candidate("item:a", "- Retry SEPA payouts"),
            candidate("activity:1", "- linked PAY-231"),
        ]);
        assert_eq!(generated, "- Retry SEPA payouts\n- linked PAY-231");
        assert_eq!(comment_of(&[]), "", "nothing seen is an empty comment");
    }

    /// A bullet says the key, not the whole id: `PAY-231`, the word a person
    /// would say.
    #[test]
    fn the_key_is_the_half_after_the_first_colon() {
        assert_eq!(key_of("jira:PAY-231"), "PAY-231");
        assert_eq!(
            key_of("confluence:ENG:SEPA design"),
            "ENG:SEPA design",
            "a key may carry further colons"
        );
        assert_eq!(key_of("not-an-id"), "not-an-id");
    }

    /// The timer's own verbs are on the list, read from the module that writes
    /// them rather than typed again here.
    #[test]
    fn the_timers_own_verbs_are_not_work() {
        for verb in ["started", "stopped"] {
            assert!(
                NOT_WORK.contains(&verb),
                "{verb:?} is the timer's own bookkeeping and would become a \
                 bullet in every worklog comment"
            );
        }
    }
}
