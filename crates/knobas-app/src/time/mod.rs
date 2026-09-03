//! The timer and the blocks it makes (spec #272 "the timer is a durable row",
//! issue #278).
//!
//! `commands/time.rs` is a set of shims over this module; every decision lives
//! here, with tests, because a `#[tauri::command]` cannot be called from one.
//! The same arrangement `backup/` and `sources/` use, for the same reason, and
//! the §10.8 exception this module exists under names that precedent.
//!
//! # What a timer is
//!
//! One row, at most, in `knobas.timer` (migration `0013`). It carries a
//! [`TimerTarget`] -- an entity or an ad-hoc label, never both -- the moment it
//! started, and the last moment knobas is known to have been alive. Stopping
//! it deletes the row and writes a [`Block`]: start, end, target, kind. That
//! is the whole state machine, and it has two writers ([`stop`] and
//! [`close_stranded`]) so that "a block appeared" is always traceable to one
//! of two sentences.
//!
//! # Why there is no IPC event
//!
//! The shell learns that a timer started or stopped through **the activity
//! stream it already watches** (`activity:new`, `app/src/lib/shell/
//! latest-change.svelte.ts`). [`start`] and [`stop`] write an activity line
//! with actor `user`, so the status bar's latest-change line and the digest
//! (#282) both see them without a channel of their own, and the strip's
//! *elapsed* is ticked client-side off `started_at` rather than pushed. A
//! second event would be a second thing to keep in step with the first, and
//! it would carry no fact the row does not already hold.
//!
//! # The heartbeat, and the two things that ride on it
//!
//! The frontend sends [`heartbeat`] every thirty seconds while the window is
//! focused, carrying the foreground target by the rule *open detail, else room
//! anchor, else none*. It advances `last_heartbeat`, and that stamp is what
//! [`close_stranded`] closes a block at on relaunch.
//!
//! The foreground it carries is **stored as an observation while passive
//! attribution is switched on** (#282, [`passive`]) and dropped otherwise.
//! #278 took the parameter without a table to put it in and said so; `0015` is
//! that table. Nothing about the switch touches the stamp: a person who never
//! turns passive attribution on keeps the relaunch rule in full.

use chrono::{DateTime, Utc};
use knobas_core::entity::EntityRef;
use sqlx::{PgPool, Row};

use crate::IpcError;

/// The day review's read and its two edits (#279).
///
/// A second file rather than more of this one: the timer is a state machine
/// with two writers, and the day review is a read and two writes over blocks
/// that have already stopped moving. They share the vocabulary above
/// -- [`Block`], [`TimerTarget`], [`vet`], [`block_of`] -- which is why it is
/// a child module and not a sibling.
pub mod day;

/// Passive attribution: the heartbeat's observations, and the blocks they
/// support (#282).
///
/// A third file for the same reason [`day`] is a second: the timer is a state
/// machine, the day review is a read and two writes, and this is one pure
/// function with a store either side of it. It is the only thing here that
/// says something the reader never typed, which is why the spec asks reviewers
/// to look hardest at it.
pub mod passive;

/// The activity actor for everything a person does with the timer.
const ACTOR: &str = "user";

/// The namespace stored contexts live in, and the kind their `knobas.entity`
/// row carries -- the same word, because `knobas_core::context::insert` writes
/// both from it.
///
/// One of `knobas_core::entity::RESERVED_NAMESPACES`, and spelled here rather
/// than indexed out of that array because an index is not a name. Two tests
/// keep the three copies of this word in step, and neither is optional:
/// `the_context_namespace_is_the_one_knobas_core_reserves` pins it downward to
/// `knobas-core`, so a rename there fails here rather than silently making
/// every context a legal target again; and `commands::time`'s
/// `the_shells_context_namespace_is_the_one_the_backend_refuses` pins it
/// sideways to `app/src/lib/shell/timer.ts`, so the picker and the launcher
/// cannot go on filtering for a spelling the backend has stopped refusing.
///
/// `pub` for the second of those: it is read from a test in another module.
pub const CONTEXT_NAMESPACE: &str = "ctx";

/// The activity actor for the relaunch sweep.
///
/// `knobas`, the spelling `knobas_core::activity::ActivityRow` reserves for
/// "an action knobas took on its own". Closing a stranded block is exactly
/// that: nobody pressed anything, and the line has to say so, because the
/// alternative is a user reading their own name against a stop they did not
/// make.
const SWEEPER: &str = "knobas";

/// What a running timer, and therefore a block, is attributed to.
///
/// `CONTEXT.md`'s **timer target**: one entity or one ad-hoc label. A tagged
/// union rather than two nullable fields, because "exactly one" is then a
/// thing the type says instead of a thing every reader has to check -- the
/// database says it too (`timer_target_chk`), and the two agreeing is what
/// makes the check a backstop rather than the only guard.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TimerTarget {
    /// A ticket, page, note, repo or asset, by its entity id.
    Entity { entity_id: String },
    /// "DB config for the migration" -- work with no entity behind it.
    Label { label: String },
}

impl TimerTarget {
    /// The two columns `knobas.timer` and `knobas.block` store this as.
    fn columns(&self) -> (Option<&str>, Option<&str>) {
        match self {
            Self::Entity { entity_id } => (Some(entity_id), None),
            Self::Label { label } => (None, Some(label)),
        }
    }

    /// The entity half, for an activity line's `entity_id`.
    fn entity(&self) -> Option<EntityRef> {
        match self {
            Self::Entity { entity_id } => EntityRef::parse(entity_id).ok(),
            Self::Label { .. } => None,
        }
    }
}

/// How a block came to exist. Mirrors `block_kind_chk` in migration `0013`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockKind {
    /// The timer made it -- `CONTEXT.md`'s wording, and deliberately not "a
    /// person started and stopped it": [`close_stranded`] writes a `Manual`
    /// block that nobody stopped. [`Block::ended_by_relaunch`] is what tells
    /// those two apart; the kind says only where the block came from.
    Manual,
    /// Passive attribution recorded what was open (#282). Written only by
    /// [`passive::materialize`], and turned into [`Manual`](Self::Manual) the
    /// moment a person assigns it -- nothing else in the crate writes this
    /// word, and nothing at all writes it back.
    Passive,
}

impl BlockKind {
    /// The spelling the schema stores and `block_kind_chk` enumerates.
    ///
    /// Public because the day review and the timesheet (#279) narrow by it,
    /// and because a caller writing the word out again is how the fourth copy
    /// of this vocabulary would start.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::Passive => "passive",
        }
    }

    fn parse(word: &str) -> Result<Self, IpcError> {
        match word {
            "manual" => Ok(Self::Manual),
            "passive" => Ok(Self::Passive),
            other => Err(IpcError::internal(format!(
                "knobas.block holds the kind {other:?}, which `block_kind_chk` \
                 should have refused"
            ))),
        }
    }
}

/// The timer as it is running right now.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct RunningTimer {
    pub target: TimerTarget,
    pub started_at: DateTime<Utc>,
    /// The last moment knobas was known to be alive. The strip does not draw
    /// it; [`close_stranded`] is what it is for.
    pub last_heartbeat: DateTime<Utc>,
}

/// What [`start`] did: the timer, and the line that says so.
///
/// Two values rather than one, and the activity line is **not** on the wire --
/// the shape `knobas_core::link::LinkMutation` records for the same job. The
/// command announces the line on `activity:new` and hands the frontend the
/// timer; a store that only returned the timer would leave the caller reading
/// the newest line back out of the log, which is a different line whenever a
/// sync wrote one in between.
#[derive(Debug, Clone)]
pub struct TimerStarted {
    pub timer: RunningTimer,
    pub activity: knobas_core::activity::ActivityRow,
}

/// What [`stop`] did: the block it closed, and the line that says so.
#[derive(Debug, Clone)]
pub struct TimerStopped {
    pub block: Block,
    pub activity: knobas_core::activity::ActivityRow,
}

/// One stretch of time knobas owns -- `CONTEXT.md`'s **block**.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Block {
    pub id: i64,
    pub started_at: DateTime<Utc>,
    pub ended_at: DateTime<Utc>,
    pub target: TimerTarget,
    pub kind: BlockKind,
    /// *Ended when knobas closed* -- the day review's *Extend to now* (#279).
    pub ended_by_relaunch: bool,
    /// The worklog this block was logged into (#280). Always `None` here:
    /// nothing in #278 writes a worklog.
    pub worklog_id: Option<i64>,
}

/// The target a caller may start a timer on, or the refusal.
///
/// Two rules, and both are about what a target *means* rather than about the
/// shape of the string:
///
/// * **An entity target is an entity id**, so a webview cannot start a timer
///   on a free-form string by choosing the wrong variant -- the two halves
///   would then be interchangeable and the ad-hoc label would be the only one
///   with a spelling rule.
/// * **A stored context is never a target** (`CONTEXT.md`, story 15): a
///   context is a *set*, and time on a set has nowhere to go. The `ctx:`
///   namespace is `knobas_core::entity`'s, read from there rather than spelled
///   again here, so the one list stays one list.
///
/// A label is trimmed and must carry something. An empty label is not an
/// ad-hoc target, it is a target nobody named, and the day review would draw
/// it as a blank row.
///
/// **What is deliberately not checked: whether the entity exists.** No table
/// here has a foreign key on `entity_id`, for the reason migration `0013`
/// records -- an afternoon survives a purged mirror -- and a check on the way
/// in that the read side does not enforce would only mean a timer can be
/// started on something that stops existing a second later.
fn vet(target: TimerTarget) -> Result<TimerTarget, IpcError> {
    match target {
        TimerTarget::Entity { entity_id } => {
            let reference = EntityRef::parse(&entity_id).map_err(IpcError::invalid)?;
            if reference.namespace.eq_ignore_ascii_case(CONTEXT_NAMESPACE) {
                return Err(IpcError::invalid(format!(
                    "{entity_id} is a stored context, and a context is never a timer \
                     target: it is a set, and time on a set has nowhere to go. Use an \
                     ad-hoc label for work across a context."
                )));
            }
            Ok(TimerTarget::Entity { entity_id })
        }
        TimerTarget::Label { label } => {
            let label = label.trim().to_owned();
            if label.is_empty() {
                return Err(IpcError::invalid(
                    "an ad-hoc timer needs a label -- what was the time on?",
                ));
            }
            Ok(TimerTarget::Label { label })
        }
    }
}

/// Rebuild a target from the two columns it was stored as.
///
/// The `internal` arms are unreachable while `timer_target_chk` and
/// `block_target_chk` are in force. They are refusals rather than a
/// `Label { label: String::new() }` fallback because a row that broke the
/// constraint is a schema failure, and the honest thing to do with one is to
/// say so rather than to render a blank target as though it were a real one.
fn target_of(entity_id: Option<String>, label: Option<String>) -> Result<TimerTarget, IpcError> {
    match (entity_id, label) {
        (Some(entity_id), None) => Ok(TimerTarget::Entity { entity_id }),
        (None, Some(label)) => Ok(TimerTarget::Label { label }),
        _ => Err(IpcError::internal(
            "a timer row carries both halves of its target, or neither, which \
             the schema's exactly-one check should have refused",
        )),
    }
}

/// The four columns every timer read selects.
///
/// Spelled out in each statement rather than assembled from a shared constant:
/// `sqlx::query` takes only a `&'static str`, which is the guard roadmap §4
/// gotcha 2 asks for, and `commands/entity.rs` records the same discipline --
/// nothing here concatenates anything into SQL. [`timer_of`] is the one
/// decoder, so a column added to one statement and not the other fails there
/// rather than silently.
const READ_TIMER: &str = "select entity_id, label, started_at, last_heartbeat from knobas.timer";

/// Insert the one timer row, or answer with nothing because one already
/// exists. See [`start`] for why the refusal is the `on conflict`.
const START_TIMER: &str = "insert into knobas.timer (entity_id, label)
     values ($1, $2)
     on conflict (only_one) do nothing
     returning entity_id, label, started_at, last_heartbeat";

/// Advance the last-alive stamp. At most one row, so no `where`.
const BEAT: &str = "update knobas.timer set last_heartbeat = now()
     returning entity_id, label, started_at, last_heartbeat";

/// Delete the timer and write its block, in one statement, so there is no
/// window in which the timer is gone and its block does not exist yet.
const STOP_TIMER: &str = "with stopped as (
         delete from knobas.timer returning entity_id, label, started_at
     )
     insert into knobas.block (started_at, ended_at, entity_id, label, kind)
     select started_at, now(), entity_id, label, 'manual' from stopped
     returning id, started_at, ended_at, entity_id, label, kind,
               ended_by_relaunch, worklog_id";

/// The same shape as [`STOP_TIMER`], ending at the last heartbeat and flagged.
///
/// **Two statements and not one parameterised statement**, deliberately. The
/// difference between them is which column becomes the end and whether the
/// flag is set, and expressing that as a bind would need either dynamic SQL --
/// which this module does not do, for the reason `commands/entity.rs` records
/// -- or a `case` that made both readings harder than either is now. The two
/// are also not the same event: one is a person stopping a clock and the other
/// is knobas admitting it stopped being alive, and the day review distinguishes
/// them.
const CLOSE_STRANDED: &str = "with stranded as (
         delete from knobas.timer
         returning entity_id, label, started_at, last_heartbeat
     )
     insert into knobas.block
         (started_at, ended_at, entity_id, label, kind, ended_by_relaunch)
     select started_at, last_heartbeat, entity_id, label, 'manual', true from stranded
     returning id, started_at, ended_at, entity_id, label, kind,
               ended_by_relaunch, worklog_id";

fn timer_of(row: &sqlx::postgres::PgRow) -> Result<RunningTimer, IpcError> {
    Ok(RunningTimer {
        target: target_of(row.try_get("entity_id")?, row.try_get("label")?)?,
        started_at: row.try_get("started_at")?,
        last_heartbeat: row.try_get("last_heartbeat")?,
    })
}

fn block_of(row: &sqlx::postgres::PgRow) -> Result<Block, IpcError> {
    Ok(Block {
        id: row.try_get("id")?,
        started_at: row.try_get("started_at")?,
        ended_at: row.try_get("ended_at")?,
        target: target_of(row.try_get("entity_id")?, row.try_get("label")?)?,
        kind: BlockKind::parse(row.try_get::<String, _>("kind")?.as_str())?,
        ended_by_relaunch: row.try_get("ended_by_relaunch")?,
        worklog_id: row.try_get("worklog_id")?,
    })
}

/// The timer that is running, or `None`.
///
/// # Errors
/// [`IpcError`] if the read fails.
pub async fn current(pool: &PgPool) -> Result<Option<RunningTimer>, IpcError> {
    let row = sqlx::query(READ_TIMER).fetch_optional(pool).await?;
    row.as_ref().map(timer_of).transpose()
}

/// Start the timer on `target`, and write the line that says so.
///
/// **Refuses when one is already running** rather than replacing it. Every
/// surface that switches targets -- ⌘T, the launcher's *Start timer* -- stops
/// first and says so on screen, because a stop is what closes a block and a
/// silent replacement would be an afternoon quietly discarded. The refusal is
/// the `on conflict do nothing` below answering with no row, so two starts
/// racing produce one timer and one refusal rather than two timers.
///
/// # Errors
/// `invalid` for a target [`vet`] refuses, `conflict` when a timer is already
/// running, [`IpcError`] if the write fails.
pub async fn start(pool: &PgPool, target: TimerTarget) -> Result<TimerStarted, IpcError> {
    let target = vet(target)?;
    let (entity_id, label) = target.columns();

    let row = sqlx::query(START_TIMER)
        .bind(entity_id)
        .bind(label)
        .fetch_optional(pool)
        .await?;
    let Some(row) = row else {
        return Err(IpcError::conflict(
            "a timer is already running -- stop it before starting another",
        ));
    };
    let timer = timer_of(&row)?;

    let activity = knobas_core::activity::record(
        pool,
        ACTOR,
        "started",
        timer.target.entity().as_ref(),
        serde_json::json!({ "timer": timer.target }),
    )
    .await?;
    Ok(TimerStarted { timer, activity })
}

/// Stop the timer, closing its block, and write the line that says so.
///
/// `None` when nothing was running, which is a success and not an error --
/// the discipline `unlink` records: a second *Stop* on a timer already stopped
/// must not be a message the reader has to dismiss.
///
/// The delete and the insert are **one statement**, so there is no window in
/// which the timer is gone and its block does not exist yet.
///
/// # Errors
/// [`IpcError`] if the write fails.
pub async fn stop(pool: &PgPool) -> Result<Option<TimerStopped>, IpcError> {
    let Some(row) = sqlx::query(STOP_TIMER).fetch_optional(pool).await? else {
        return Ok(None);
    };
    let block = block_of(&row)?;

    let activity = knobas_core::activity::record(
        pool,
        ACTOR,
        "stopped",
        block.target.entity().as_ref(),
        detail_of(&block),
    )
    .await?;
    Ok(Some(TimerStopped { block, activity }))
}

/// Stamp the timer as alive, and answer with it.
///
/// `foreground` is what the window has in front of the reader right now. It is
/// stored as an observation while passive attribution is on (#282) and dropped
/// otherwise -- **off means nothing is recorded**, not that it is recorded and
/// not looked at.
///
/// **Nothing about the foreground can stop the stamp landing, and that is the
/// whole shape of this function.** The stamp is a statement about *knobas*,
/// not about what the reader was looking at: it is the moment
/// [`close_stranded`] closes a stranded block at. A beat refused because its
/// foreground was malformed would leave `last_heartbeat` frozen at the last
/// beat that happened to be well-formed, and the next relaunch would close the
/// block there -- silently losing every hour since, which is the one failure
/// the relaunch rule exists to prevent. So the foreground is vetted *after*
/// the write and a refusal is logged, never propagated.
///
/// `None` when no timer is running. The heartbeat is sent on a schedule rather
/// than on a state, so "there is nothing to stamp" is its ordinary answer and
/// not a failure -- and it is also the answer that tells a shell holding a
/// stale timer that the clock has stopped.
///
/// # Errors
/// [`IpcError`] if the write fails. **Not** for a foreground [`vet`] refuses.
pub async fn heartbeat(
    pool: &PgPool,
    foreground: Option<TimerTarget>,
) -> Result<Option<RunningTimer>, IpcError> {
    // **The stamp first, and unconditionally.** See the doc comment: a beat
    // that a bad foreground could refuse would freeze the last-alive stamp,
    // and the block would then be closed at whenever the frontend last sent
    // something this function happened to like.
    let row = sqlx::query(BEAT).fetch_optional(pool).await?;

    // The observation is vetted, then stored if there is anywhere to store it.
    // A foreground the timer could never run on is a frontend bug: it is
    // logged and **the observation is still written, with no target**. The
    // beat happened and the window was focused, so dropping the row would put
    // a hole in the timeline the derivation reads focused time off -- losing
    // the attribution is honest, losing the observation is not.
    let foreground = foreground.and_then(|foreground| match vet(foreground) {
        Ok(target) => Some(target),
        Err(error) => {
            tracing::warn!(
                %error,
                "a heartbeat carried a foreground that is not a legal timer target"
            );
            None
        }
    });

    // Last, and after the stamp, for the same reason the vet is: nothing about
    // the foreground may cost the beat. The stamp is already durable by the
    // time this runs, so a failed setting read or a failed insert cannot put
    // `last_heartbeat` at risk.
    //
    // **Logged as well as returned**, and the log is the half that matters: a
    // beat is sent every thirty seconds from a window whose store deliberately
    // keeps what is on screen when one rejects (#278, `timer.svelte.ts`), so a
    // failure that only travelled the wire would be seen by nobody -- and a
    // recording that has silently stopped is a day review that quietly says
    // the reader did nothing.
    if let Err(error) = passive::observe(pool, foreground.as_ref()).await {
        tracing::warn!(%error, "passive attribution did not record this beat");
        return Err(error);
    }

    row.as_ref().map(timer_of).transpose()
}

/// Close a timer that outlived the process, at the last moment knobas was
/// alive (#272; #278 acceptance criterion 4).
///
/// Called during bring-up, **before the shell's first read**, so no surface
/// ever draws a clock that has been running all night. The block ends at
/// `last_heartbeat` and not at `now()`, and that is the whole point: `now()`
/// would log the hours knobas spent closed as work, which is the one thing a
/// forgotten timer must not do. It is flagged `ended_by_relaunch`, because a
/// person who really did work through is owed *Extend to now* rather than a
/// silent shortening (#279, story 13).
///
/// A timer that died before its first heartbeat closes at zero length. That
/// block is still written: "a timer was running and knobas stopped" is a fact
/// the day review can act on, and dropping it would be knobas deciding the
/// block did not happen.
///
/// `None` when no timer was stranded, which is every ordinary launch.
///
/// # Errors
/// [`IpcError`] if the write fails.
pub async fn close_stranded(pool: &PgPool) -> Result<Option<Block>, IpcError> {
    let Some(row) = sqlx::query(CLOSE_STRANDED).fetch_optional(pool).await? else {
        return Ok(None);
    };
    let block = block_of(&row)?;

    knobas_core::activity::record(
        pool,
        SWEEPER,
        "closed",
        block.target.entity().as_ref(),
        detail_of(&block),
    )
    .await?;
    tracing::info!(
        block = block.id,
        ended_at = %block.ended_at,
        "a timer outlived the last run of knobas and was closed at its last heartbeat"
    );
    Ok(Some(block))
}

/// What an activity line says about a closed block, beyond its verb.
///
/// `seconds` rather than the two stamps again: the line is read by the status
/// bar and by the digest (#282), and both want "how long", which neither would
/// otherwise be able to work out without re-reading the block.
fn detail_of(block: &Block) -> serde_json::Value {
    serde_json::json!({
        "timer": block.target,
        "block_id": block.id,
        "seconds": (block.ended_at - block.started_at).num_seconds(),
        "ended_by_relaunch": block.ended_by_relaunch,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The `ctx:` refusal, at the one seam that decides it.
    ///
    /// `tests/time_ipc.rs` witnesses it against a real database and a real
    /// context row; this is the same rule read off the namespace list, so a
    /// namespace renamed in `knobas_core::entity` fails here without a
    /// PostgreSQL to run against.
    #[test]
    fn a_stored_context_is_not_a_timer_target() {
        let refusal = vet(TimerTarget::Entity {
            entity_id: "ctx:5b1c0f1e-0000-4000-8000-000000000000".to_owned(),
        })
        .expect_err("a context is a set, and time on a set has nowhere to go");
        assert_eq!(refusal.code, crate::IpcErrorCode::Invalid);
        assert!(
            refusal.message.contains("stored context"),
            "the refusal has to say why, not just no: {}",
            refusal.message
        );
    }

    /// The refusal above reads a namespace, so the namespace has to be the one
    /// `knobas_core` actually writes context ids in. Without this, renaming it
    /// there would leave the guard matching a spelling nothing produces --
    /// green, and refusing nothing.
    #[test]
    fn the_context_namespace_is_the_one_knobas_core_reserves() {
        assert!(
            knobas_core::entity::RESERVED_NAMESPACES.contains(&CONTEXT_NAMESPACE),
            "`{CONTEXT_NAMESPACE}:` is no longer a namespace knobas keeps for \
             itself, so the timer's context refusal matches nothing. Reserved: {:?}",
            knobas_core::entity::RESERVED_NAMESPACES
        );
    }

    /// ...and the direction that says the guard is not simply refusing
    /// everything: a note is a local namespace too, and a note is a legal
    /// target (story 14).
    #[test]
    fn a_note_and_a_synced_item_are_both_legal_targets() {
        for id in [
            "note:5b1c0f1e",
            "jira:PAY-231",
            "confluence:ENG:SEPA design",
        ] {
            let vetted = vet(TimerTarget::Entity {
                entity_id: id.to_owned(),
            })
            .unwrap_or_else(|error| panic!("{id} should be a legal target: {error}"));
            assert_eq!(
                vetted,
                TimerTarget::Entity {
                    entity_id: id.to_owned()
                }
            );
        }
    }

    #[test]
    fn a_string_that_is_not_an_entity_id_is_not_an_entity_target() {
        let refusal = vet(TimerTarget::Entity {
            entity_id: "DB config for the migration".to_owned(),
        })
        .expect_err("an entity target has to be an entity id");
        assert_eq!(refusal.code, crate::IpcErrorCode::Invalid);
    }

    #[test]
    fn a_label_is_trimmed_and_may_not_be_blank() {
        assert_eq!(
            vet(TimerTarget::Label {
                label: "  DB config for the migration  ".to_owned()
            })
            .expect("a label with content is a target"),
            TimerTarget::Label {
                label: "DB config for the migration".to_owned()
            }
        );
        let refusal = vet(TimerTarget::Label {
            label: "   ".to_owned(),
        })
        .expect_err("whitespace is not a name for what the time was on");
        assert_eq!(refusal.code, crate::IpcErrorCode::Invalid);
    }

    /// The wire spelling of the target, which the mirror and the picker both
    /// depend on. Pinned here as well as by `commands::time`'s mirror tests
    /// -- which check the TypeScript against it -- because this is where the
    /// `#[serde]` attributes that decide the spelling actually live.
    #[test]
    fn a_target_is_tagged_by_which_half_it_is() {
        assert_eq!(
            serde_json::to_value(TimerTarget::Entity {
                entity_id: "jira:PAY-231".to_owned()
            })
            .unwrap(),
            serde_json::json!({"kind": "entity", "entity_id": "jira:PAY-231"})
        );
        assert_eq!(
            serde_json::to_value(TimerTarget::Label {
                label: "DB config".to_owned()
            })
            .unwrap(),
            serde_json::json!({"kind": "label", "label": "DB config"})
        );
    }

    /// Every kind this enum can hold is a kind `block_kind_chk` accepts, and
    /// the kind the two writing statements name is one of them.
    ///
    /// The vocabulary lives in three files -- this enum, the migration's check
    /// constraint, and the `'manual'` literal inside [`STOP_TIMER`] and
    /// [`CLOSE_STRANDED`] -- and PostgreSQL is the only thing that would
    /// otherwise notice a disagreement, at run time, on the first stop. The
    /// same cross-check `knobas_core::start_work` runs against `0008`'s `step`
    /// vocabulary, for the same reason.
    #[test]
    fn every_block_kind_is_one_the_schema_accepts() {
        let migration =
            include_str!("../../../knobas-db/migrations/0013_the_timer_and_its_blocks.sql");
        let check = migration
            .split("block_kind_chk")
            .nth(1)
            .expect("0013 declares block_kind_chk");
        let check = &check[..check.find(')').expect("the check is closed")];
        for kind in [BlockKind::Manual, BlockKind::Passive] {
            assert!(
                check.contains(&format!("'{}'", kind.as_str())),
                "`{}` is a BlockKind the schema refuses: {check}",
                kind.as_str()
            );
        }
        for statement in [STOP_TIMER, CLOSE_STRANDED] {
            assert!(
                statement.contains(&format!("'{}'", BlockKind::Manual.as_str())),
                "a statement that writes a block no longer writes it as \
                 `{}`: {statement}",
                BlockKind::Manual.as_str()
            );
        }
    }

    /// A row that broke the schema's exactly-one check is reported, never
    /// rendered. Reachable only by a hand-written `insert`, which is why it is
    /// asserted on the decoder rather than through a command.
    #[test]
    fn a_target_with_both_halves_or_neither_is_a_failure_not_a_guess() {
        for (entity_id, label) in [
            (Some("jira:PAY-231".to_owned()), Some("both".to_owned())),
            (None, None),
        ] {
            let refusal = target_of(entity_id, label)
                .expect_err("exactly one half of a target, or nothing is knowable about it");
            assert_eq!(refusal.code, crate::IpcErrorCode::Internal);
        }
    }
}
