//! The day review: a day's blocks, and the three things a person can do to
//! one of them (issue #279).
//!
//! [`super`] holds the *timer* — the clock that makes blocks. This holds the
//! *record* — the blocks it made, read back for one day and edited. They are
//! one module pair on the bridge (the §10.8 exception #278 landed under) and
//! two files here, because the timer is a state machine with two writers and
//! this is a read and three statements over rows that have already stopped
//! moving.
//!
//! # Why a day is two instants and not a date
//!
//! [`list`] takes `from` and `to`, and the webview computes them from the
//! address's `YYYY-MM-DD` by asking its own clock for that day's local
//! midnight and the next one. **The reader's day is a fact only the webview
//! holds.** The backend does not know the machine's timezone (nothing on this
//! bridge has ever carried one), and a UTC offset passed as a parameter is the
//! wrong shape anyway: a day containing a DST change is 23 or 25 hours long
//! and has two offsets, so an offset would silently move one edge of the strip
//! twice a year. `Date` arithmetic in the webview gets both boundaries right
//! by construction.
//!
//! It is also the shape the week timesheet wants (#283) without a second
//! command: a week is an interval too.
//!
//! # What "in this day" means
//!
//! A block **overlapping** the interval, not a block contained in it. Work
//! that ran through midnight belongs to both days it touched, and a day review
//! that dropped it would be a strip a person cannot edit the one block on
//! their day they most want to fix.
//!
//! # What a logged block is
//!
//! Read-only, in both directions (`CONTEXT.md`'s **block**; #272 story 20).
//! The rule is one `where` clause on each of [`update`] and [`remove`] —
//! `worklog_id is null` — so a logged block cannot be edited even by a caller
//! that never asked whether it was logged. What the reader sees is the
//! sentence [`refusal`] builds, because "nothing happened" is not something a
//! person can act on.
//!
//! The worklog table itself arrives with #280; the column is here now
//! (migration `0013`) precisely so this rule did not have to wait for it.

use chrono::{DateTime, Utc};
use sqlx::{PgPool, Row};

use super::{Block, TimerTarget, block_of, vet};
use crate::IpcError;

/// One block as the day review draws it: the block, and what the mirror calls
/// its target.
///
/// Two values rather than a `Block` with a title field, and the split is the
/// point: a block is knobas' own durable record and the title is the mirror's
/// current opinion of a row that may since have been renamed, purged or never
/// synced at all. Folding the second into the first would make a title look
/// like something the block remembers, and the first purged ticket would turn
/// an afternoon into a row with no name.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct DayBlock {
    pub block: Block,
    /// The entity's title from `knobas.entity`, or `null`.
    ///
    /// `null` for a label block — there is no entity — and for an entity the
    /// mirror has never held or has purged, which migration `0013` allows on
    /// purpose (no foreign key: an afternoon survives a purged mirror). A
    /// blank title is `null` too, so the view has **one** question to ask
    /// rather than two, and the answer to it is always "show the id instead".
    pub title: Option<String>,
}

/// The columns every read here selects.
///
/// Spelled out in each statement rather than concatenated: `sqlx::query` takes
/// a `&'static str`, which is the guard roadmap §4 gotcha 2 asks for, and
/// [`day_block_of`] is the one decoder, so a column added to one statement and
/// not another fails there rather than silently.
const LIST: &str = "select b.id, b.started_at, b.ended_at, b.entity_id, b.label, b.kind,
            b.ended_by_relaunch, b.worklog_id, e.title
       from knobas.block b
       left join knobas.entity e on e.id = b.entity_id
      where b.started_at < $2 and b.ended_at >= $1
      order by b.started_at, b.id";

/// Rewrite one block, in one statement, and hand back what it now is.
///
/// **`ended_by_relaunch` is cleared, always**, and that is the rule *Extend to
/// now* rides on. The flag means *knobas guessed this end* — it was written by
/// the relaunch sweep at the last moment knobas was known to be alive, and it
/// is offered to the reader as a question. This command carries the block's
/// whole editable shape, so every call is a person answering that question:
/// the start, the end and the target are now what they said, and a marker
/// still claiming knobas chose the end would be knobas disowning an edit it
/// was handed.
///
/// The `worklog_id is null` in the `where` is the read-only rule, enforced
/// here rather than checked first: a check followed by an update is two
/// statements with a gap between them, and this is one.
///
/// The join is in the same statement so the answer is a [`DayBlock`] the view
/// can draw without a second read — and so the title it shows is the title as
/// of the write, not as of a moment before it.
const UPDATE: &str = "with edited as (
         update knobas.block
            set started_at = $2, ended_at = $3, entity_id = $4, label = $5,
                ended_by_relaunch = false
          where id = $1 and worklog_id is null
         returning id, started_at, ended_at, entity_id, label, kind,
                   ended_by_relaunch, worklog_id
     )
     select b.id, b.started_at, b.ended_at, b.entity_id, b.label, b.kind,
            b.ended_by_relaunch, b.worklog_id, e.title
       from edited b
       left join knobas.entity e on e.id = b.entity_id";

/// Delete one block, subject to the same read-only rule.
const DELETE: &str = "delete from knobas.block where id = $1 and worklog_id is null returning id";

/// Why a write matched no row. Read *after* the write, never before it.
const DIAGNOSE: &str = "select worklog_id from knobas.block where id = $1";

fn day_block_of(row: &sqlx::postgres::PgRow) -> Result<DayBlock, IpcError> {
    let title: Option<String> = row.try_get("title")?;
    Ok(DayBlock {
        block: block_of(row)?,
        // Blank is nothing. `knobas.entity.title` defaults to `''`, so an
        // entity synced before its adapter could name it is a row with a
        // title that says nothing -- and a strip drawing an empty span where
        // a name belongs is worse than one drawing the id.
        title: title.filter(|title| !title.trim().is_empty()),
    })
}

/// The blocks overlapping `[from, to)`, earliest first.
///
/// See the module docs for why the interval is instants rather than a date,
/// and why overlap rather than containment. Ties are broken by id, so a day
/// with two blocks starting in the same second draws in the order they were
/// written rather than in whatever order the planner returned them.
///
/// # Errors
/// [`IpcError`] if the read fails.
pub async fn list(
    pool: &PgPool,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> Result<Vec<DayBlock>, IpcError> {
    let rows = sqlx::query(LIST)
        .bind(from)
        .bind(to)
        .fetch_all(pool)
        .await?;
    rows.iter().map(day_block_of).collect()
}

/// Say why a write matched no row, in a sentence the day review can show.
///
/// Three answers, and they are three different things a reader can do about
/// it: the block is logged and is therefore read-only, there is no such block,
/// or something moved underneath the write. Reporting all three as one
/// "nothing happened" would leave the commonest of them -- a block already in
/// a worklog -- looking like knobas losing an edit.
async fn refusal(pool: &PgPool, id: i64) -> IpcError {
    let row = match sqlx::query(DIAGNOSE).bind(id).fetch_optional(pool).await {
        Ok(row) => row,
        Err(error) => return error.into(),
    };
    let Some(row) = row else {
        return IpcError::not_found(format!(
            "there is no block {id} -- it may have been deleted in another window"
        ));
    };
    match row.try_get::<Option<i64>, _>("worklog_id") {
        Ok(Some(worklog)) => IpcError::invalid(format!(
            "this block has been logged to a worklog (#{worklog}) and is read-only, so that \
             what knobas shows never disagrees with what the ticket holds. Change the worklog \
             instead."
        )),
        // The row exists and is not logged, so the write's `where` should have
        // matched it. Something wrote between the two statements.
        Ok(None) => IpcError::conflict(
            "this block changed while it was being edited -- read the day again and try once more",
        ),
        Err(error) => error.into(),
    }
}

/// Move a block's start, its end and its target, all three at once.
///
/// The whole editable shape rather than a patch of the fields that changed:
/// the reader is stating what the block *is*, and a partial update would leave
/// "the end is unchanged" and "the end is absent" spelled the same way on the
/// wire. It is also what makes clearing `ended_by_relaunch` honest -- see
/// [`UPDATE`].
///
/// # Errors
/// [`Invalid`](crate::IpcErrorCode::Invalid) for a target [`vet`] refuses, for
/// an end before its start, and for a block that has been logged;
/// [`NotFound`](crate::IpcErrorCode::NotFound) for a block that is not there;
/// [`IpcError`] if the write fails.
pub async fn update(
    pool: &PgPool,
    id: i64,
    started_at: DateTime<Utc>,
    ended_at: DateTime<Utc>,
    target: TimerTarget,
) -> Result<DayBlock, IpcError> {
    let target = vet(target)?;
    if ended_at < started_at {
        // `block_span_chk` refuses this too, and that is the backstop. Said
        // here because a check-constraint violation reaches the reader as an
        // internal error, and "a block cannot end before it starts" is a
        // sentence they can act on.
        return Err(IpcError::invalid(
            "a block cannot end before it starts -- check the two times over",
        ));
    }
    let (entity_id, label) = target.columns();

    let row = sqlx::query(UPDATE)
        .bind(id)
        .bind(started_at)
        .bind(ended_at)
        .bind(entity_id)
        .bind(label)
        .fetch_optional(pool)
        .await?;
    match row {
        Some(row) => day_block_of(&row),
        None => Err(refusal(pool, id).await),
    }
}

/// Delete a block.
///
/// `remove` rather than `delete`, which is a keyword-adjacent name this
/// codebase avoids for functions; the command it is shimmed as is
/// `delete_block`, which is the reader's word.
///
/// # Errors
/// [`Invalid`](crate::IpcErrorCode::Invalid) for a block that has been logged,
/// [`NotFound`](crate::IpcErrorCode::NotFound) for a block that is not there,
/// [`IpcError`] if the write fails.
pub async fn remove(pool: &PgPool, id: i64) -> Result<(), IpcError> {
    let row = sqlx::query(DELETE).bind(id).fetch_optional(pool).await?;
    if row.is_none() {
        return Err(refusal(pool, id).await);
    }
    Ok(())
}
