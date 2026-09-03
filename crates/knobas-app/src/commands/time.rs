//! The time module's commands (issues #278, #279, #282) -- what the top
//! strip, ⌘T, the launcher's *Start timer* row, the day review and the
//! settings view call.
//!
//! Shims, like every other module here: the decisions are in [`crate::time`],
//! which is testable without a window. The §10.8 exception this module lands
//! under is the backup module's precedent -- a `time` pair on both sides of
//! the bridge -- and it is where **every** time command lives, including the
//! block, worklog, draft, day and week commands #279 and #280 add.
//!
//! `State<'_, Lifecycle>` and `lifecycle.pool()?`, never `State<'_,
//! AppState>`: carry-over §10.6(a), the rule `commands/mod.rs`'s own test
//! enforces for the whole directory.
//!
//! # Why start and stop take an `AppHandle` and the reads do not
//!
//! Because they announce. A timer starting or stopping is a mutation the
//! status bar's latest-change line shows, and the way it shows is the
//! `activity:new` event every other mutation here is announced on
//! (`create_link`, `unlink`, the suggestion answers). **No event of the
//! timer's own** -- issue #278 is explicit that the shell learns through the
//! signal it already watches, and `crate::time`'s module docs give the reason.

use tauri::{Emitter, State};

use crate::time::{self, Block, RunningTimer, TimerTarget};
use crate::{IpcError, Lifecycle};

/// Put one activity line on the wire, the way `commands::entity` does.
///
/// Best-effort by design: `emit` fails only when there is no window to hear
/// it, and a timer that started is started whether or not the strip heard.
fn announce<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    line: knobas_core::activity::ActivityRow,
) {
    if let Err(error) = app.emit(crate::events::ACTIVITY_NEW, &line) {
        tracing::warn!(%error, verb = %line.verb, "an activity line was not announced");
    }
}

/// The timer that is running, or `null`.
///
/// The strip's read: it is called once when the shell is ready and again
/// whenever the activity signal says something happened, and the elapsed
/// reading is ticked from `started_at` in the webview rather than polled.
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) while the database is still
/// coming up, [`Internal`](crate::IpcErrorCode::Internal) if the read fails.
#[tauri::command]
pub async fn current_timer(
    lifecycle: State<'_, Lifecycle>,
) -> Result<Option<RunningTimer>, IpcError> {
    let pool = lifecycle.pool()?;
    time::current(&pool).await
}

/// Start the timer on `target`, in the room the reader is standing in.
///
/// The first argument is the tagged target, not two nullable strings: exactly
/// one of an entity and a label is what the whole feature is about, and a shape
/// that could carry both would put that rule in the caller.
///
/// `inRoom` is the **stored context** the reader was in -- `null` for *All
/// work*, a source room or a project room, none of which is a stored context.
/// It is recorded on the timer and carried onto the block, because it is what
/// the ad-hoc dialog's second rule reads (#281); a dialog that asked which room
/// the reader is in *now* would be answering a different question.
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) before bring-up,
/// [`Invalid`](crate::IpcErrorCode::Invalid) for a target that is not an
/// entity id, is a stored context, or is a blank label, and for a room that is
/// not a stored context, and [`Conflict`](crate::IpcErrorCode::Conflict) when a
/// timer is already running -- stop it first.
#[tauri::command]
pub async fn start_timer<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    lifecycle: State<'_, Lifecycle>,
    target: TimerTarget,
    in_room: Option<String>,
) -> Result<RunningTimer, IpcError> {
    let pool = lifecycle.pool()?;
    let started = time::start(&pool, target, in_room).await?;
    announce(&app, started.activity);
    Ok(started.timer)
}

/// Stop the timer and close its block.
///
/// `null` when nothing was running: stopping a stopped timer is a success,
/// not a message to dismiss.
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) before bring-up,
/// [`Internal`](crate::IpcErrorCode::Internal) if the write fails.
#[tauri::command]
pub async fn stop_timer<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    lifecycle: State<'_, Lifecycle>,
) -> Result<Option<Block>, IpcError> {
    let pool = lifecycle.pool()?;
    let Some(stopped) = time::stop(&pool).await? else {
        return Ok(None);
    };
    announce(&app, stopped.activity);
    Ok(Some(stopped.block))
}

/// Say the window is still alive, and answer with the timer as it now stands.
///
/// Sent every thirty seconds while the window is focused. `foreground` is what
/// the reader has in front of them by the rule *open detail, else room anchor,
/// else none*. It is stored as an observation while passive attribution is on
/// (#282) and dropped otherwise; [`time::heartbeat`] records why a foreground
/// it dislikes costs the attribution and never the beat.
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) before bring-up,
/// [`Internal`](crate::IpcErrorCode::Internal) if the write fails.
#[tauri::command]
pub async fn timer_heartbeat(
    lifecycle: State<'_, Lifecycle>,
    foreground: Option<TimerTarget>,
) -> Result<Option<RunningTimer>, IpcError> {
    let pool = lifecycle.pool()?;
    time::heartbeat(&pool, foreground).await
}

/// The blocks overlapping `[from, to)`, earliest first — the day review's read
/// (#279).
///
/// **The interval is the reader's local day, computed in the webview**, and
/// the argument is two instants rather than a `YYYY-MM-DD` because the
/// machine's timezone is a fact only the webview holds. A UTC offset passed
/// instead would be the wrong shape as well as the wrong owner: a day
/// containing a DST change is 23 or 25 hours long and has two offsets.
/// `crate::time::day`'s module docs carry the whole reasoning.
///
/// Overlap, not containment: a block that ran through midnight is on both days
/// it touched.
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) while the database is still
/// coming up, [`Internal`](crate::IpcErrorCode::Internal) if the read fails.
#[tauri::command]
pub async fn day_blocks(
    lifecycle: State<'_, Lifecycle>,
    from: chrono::DateTime<chrono::Utc>,
    to: chrono::DateTime<chrono::Utc>,
) -> Result<Vec<time::day::DayBlock>, IpcError> {
    let pool = lifecycle.pool()?;
    time::day::list(&pool, from, to).await
}

/// Move a block's start, its end and its target — the day review's edit, and
/// what *Extend to now* is (#279, stories 19 and 13).
///
/// The whole editable shape in one call, not a patch: the reader is saying
/// what the block *is*. That is also what makes clearing `ended_by_relaunch`
/// honest — the marker means *knobas guessed this end*, and once a person has
/// stated the end it is theirs.
///
/// **No `AppHandle` and no activity line.** Editing a block is a correction to
/// knobas' own record of a stretch that has already happened, not something
/// that happened; and the timer store re-reads itself on every `activity:new`,
/// so a line here would make every correction a reason for the top strip to go
/// back to the database for a clock that did not move.
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) before bring-up,
/// [`Invalid`](crate::IpcErrorCode::Invalid) for a target that is not an
/// entity id, is a stored context or is a blank label, for an end before its
/// start, and for a block that has been logged into a worklog — which is
/// read-only (story 20) — and [`NotFound`](crate::IpcErrorCode::NotFound) for
/// a block that is not there.
#[tauri::command]
pub async fn update_block(
    lifecycle: State<'_, Lifecycle>,
    id: i64,
    started_at: chrono::DateTime<chrono::Utc>,
    ended_at: chrono::DateTime<chrono::Utc>,
    target: TimerTarget,
) -> Result<time::day::DayBlock, IpcError> {
    let pool = lifecycle.pool()?;
    time::day::update(&pool, id, started_at, ended_at, target).await
}

/// Delete a block.
///
/// Nothing comes back: the day review re-reads the day, which is the one
/// answer that is true about every other block on the strip as well.
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) before bring-up,
/// [`Invalid`](crate::IpcErrorCode::Invalid) for a block that has been logged,
/// [`NotFound`](crate::IpcErrorCode::NotFound) for a block that is not there.
#[tauri::command]
pub async fn delete_block(lifecycle: State<'_, Lifecycle>, id: i64) -> Result<(), IpcError> {
    let pool = lifecycle.pool()?;
    time::day::remove(&pool, id).await
}

/// The worklog draft for a ticket and one of the reader's days, or `null`
/// (issue #280).
///
/// **Called on every stop, and `null` is the ordinary answer.** The shell asks
/// for a draft whenever a timer that was on an entity stops, and opens the
/// draft only if it got one -- so the decision "is this something a worklog can
/// go to" lives in [`crate::time::worklog`] with the descriptor it is read
/// from, not in the webview as a list of kinds. A stop on a note, on a repo, or
/// on a ticket whose blocks are already logged answers `null`, and the reader
/// sees nothing rather than an apology.
///
/// `day` is the reader's own day (`2026-09-03`) and `offsetMinutes` their own
/// offset from UTC, because their machine is the only thing that knows which
/// day they mean.
///
/// **Takes no `Lifecycle`**, and that is not an oversight: the draft asks the
/// *adapter* whether a worklog can go there, so it needs the sources state --
/// which carries the pool as well, and is managed later than the pool is. Two
/// routes to one pool in one shim would be a second thing that can be
/// `not_ready` for a different reason. The shape `commands::entity`'s
/// `submit_write` uses, for the same reason.
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) before bring-up,
/// [`Invalid`](crate::IpcErrorCode::Invalid) for a target that is not an
/// entity id or an offset that is not an offset.
#[tauri::command]
pub async fn worklog_draft<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    entity_id: String,
    day: chrono::NaiveDate,
    offset_minutes: i32,
) -> Result<Option<time::worklog::Draft>, IpcError> {
    let sources = crate::sources::state(&app)?;
    time::worklog::draft(
        &sources.pool,
        sources.registry.as_ref(),
        &entity_id,
        day,
        offset_minutes,
    )
    .await
}

/// Log a day's work on a ticket: queue the write, keep the copy, and make the
/// blocks it covers read-only (issue #280).
///
/// `startedAt`, `seconds` and `comment` are what the reader settled on in the
/// draft; **which blocks are covered is not an argument**, and
/// [`crate::time::worklog::log`] records why -- a caller that could name them
/// could name another ticket's.
///
/// The answer is the local copy, read back after the flush: `remote_id` is
/// already Jira's if the write went through, and `null` if it is still owed.
/// The write itself is in the pending-writes panel like any other.
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) before bring-up,
/// [`Invalid`](crate::IpcErrorCode::Invalid) for a target whose source does not
/// take worklogs or a duration that is not positive, and
/// [`Conflict`](crate::IpcErrorCode::Conflict) when the day has no unlogged
/// time left on that ticket.
#[tauri::command]
pub async fn log_work<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    entity_id: String,
    day: chrono::NaiveDate,
    offset_minutes: i32,
    started_at: chrono::DateTime<chrono::Utc>,
    seconds: i64,
    comment: String,
) -> Result<time::worklog::Worklog, IpcError> {
    let sources = crate::sources::state(&app)?;
    time::worklog::log(
        &sources,
        &entity_id,
        day,
        offset_minutes,
        started_at,
        seconds,
        &comment,
    )
    .await
}

/// Write a block over a stretch nobody claimed — *Assign…* on a gap (#282).
///
/// The counterpart of [`update_block`], which is what *Assign…* on a **passive
/// block** calls: the reader's sentence is the same either way ("this
/// half-hour was this ticket"), and the only difference is whether knobas
/// already had a row to put it on. Both end in a manual block.
///
/// **No `AppHandle` and no activity line**, for the reason [`update_block`]
/// carries neither: filling in a stretch that has already passed is a
/// correction to knobas' own record, not something that just happened.
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) before bring-up,
/// [`Invalid`](crate::IpcErrorCode::Invalid) for a target that is not an
/// entity id, is a stored context or is a blank label, and for an end before
/// its start.
#[tauri::command]
pub async fn create_block(
    lifecycle: State<'_, Lifecycle>,
    started_at: chrono::DateTime<chrono::Utc>,
    ended_at: chrono::DateTime<chrono::Utc>,
    target: TimerTarget,
) -> Result<time::day::DayBlock, IpcError> {
    let pool = lifecycle.pool()?;
    time::day::create(&pool, started_at, ended_at, target).await
}

/// Whether passive attribution is switched on. `false` until somebody says
/// otherwise — the setting is opt-in (#282).
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) before bring-up,
/// [`Internal`](crate::IpcErrorCode::Internal) if the read fails.
#[tauri::command]
pub async fn passive_attribution(lifecycle: State<'_, Lifecycle>) -> Result<bool, IpcError> {
    let pool = lifecycle.pool()?;
    time::passive::enabled(&pool).await
}

/// Switch passive attribution on or off, and answer with what is now stored.
///
/// The answer is the stored value rather than nothing, so the settings toggle
/// draws what the database holds instead of what the click asked for — the
/// rule `set_backup_schedule` follows for the same surface.
///
/// **Switching it off stops the recording; it does not delete what has already
/// been offered.** `crate::time::passive::set_enabled` carries the reasoning.
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) before bring-up,
/// [`Internal`](crate::IpcErrorCode::Internal) if the write fails.
#[tauri::command]
pub async fn set_passive_attribution(
    lifecycle: State<'_, Lifecycle>,
    enabled: bool,
) -> Result<bool, IpcError> {
    let pool = lifecycle.pool()?;
    time::passive::set_enabled(&pool, enabled).await
}

/// What *Log an ad-hoc block* should show for a block, or `null` because that
/// block is not an ad-hoc one (issue #281).
///
/// **Asked on every stop, before the worklog draft, and `null` is what says
/// "this was a ticket".** The decision *is this something a worklog goes to*
/// lives in [`crate::time::suggest`] with the descriptor it is read from, not
/// in the webview as a list of kinds -- so the shell asks once and opens
/// whichever dialog it is handed, and a source that starts taking worklogs
/// needs no change here.
///
/// A `Some` whose `suggestion` is `null` is the other absence and a different
/// one: the dialog opens, knobas has nothing to suggest, and *Keep local* is
/// the default.
///
/// `day` is the reader's own day and `offsetMinutes` their own offset from
/// UTC, for the reason [`worklog_draft`] takes them: their machine is the only
/// thing that knows which day they mean. It is the day the **block** started
/// on, not the day the dialog opened on.
///
/// **Takes no `Lifecycle`**, the shape [`worklog_draft`] uses and for the same
/// reason: the rules ask the *adapter* which sources take a worklog, so this
/// needs the sources state, which carries the pool as well.
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) before bring-up,
/// [`NotFound`](crate::IpcErrorCode::NotFound) for a block that is not there,
/// [`Invalid`](crate::IpcErrorCode::Invalid) for an offset that is not an
/// offset.
#[tauri::command]
pub async fn ad_hoc_block<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    block_id: i64,
    day: chrono::NaiveDate,
    offset_minutes: i32,
) -> Result<Option<time::suggest::AdHocBlock>, IpcError> {
    let sources = crate::sources::state(&app)?;
    time::suggest::offer(
        &sources.pool,
        sources.registry.as_ref(),
        block_id,
        day,
        offset_minutes,
    )
    .await
}

/// The week timesheet: a row per target and a cell per day (issue #283).
///
/// **The webview computes the seven days**, each as a date and the two
/// instants it spans, for the reason [`day_blocks`] takes instants: the
/// machine's timezone is a fact only that side holds, and one UTC offset for a
/// week would be wrong for every week containing a daylight-saving change.
/// Sending seven windows gets each edge right by construction.
///
/// This read **reconciles each day's passive blocks first**, exactly as
/// `day_blocks` does and for the same reason -- see [`crate::time::week::read`].
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) before bring-up,
/// [`Invalid`](crate::IpcErrorCode::Invalid) for a day list that is empty,
/// longer than a week, or whose windows overlap, and
/// [`Internal`](crate::IpcErrorCode::Internal) if a read fails.
#[tauri::command]
pub async fn week_timesheet(
    lifecycle: State<'_, Lifecycle>,
    days: Vec<time::week::DayWindow>,
) -> Result<time::week::Week, IpcError> {
    let pool = lifecycle.pool()?;
    time::week::read(&pool, &days).await
}

/// What *Log all* would send, before any of it is sent (issue #283).
///
/// The confirmation's list, and it is a command of its own rather than a field
/// on the timesheet because it is a different question: the timesheet is what
/// the week *was*, and this is what a button is about to do to a ticketing
/// system other people read. [`log_all`] re-derives its own work rather than
/// being handed this back, for the reason [`log_work`] does not take block ids.
///
/// **Takes no `Lifecycle`** and reaches the pool through the sources state, the
/// shape [`worklog_draft`] records: the plan asks each target's *adapter*
/// whether a worklog can go there.
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) before bring-up,
/// [`Invalid`](crate::IpcErrorCode::Invalid) for a day list the timesheet
/// would refuse.
#[tauri::command]
pub async fn log_all_preview<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    days: Vec<time::week::DayWindow>,
) -> Result<Vec<time::week::PlannedWorklog>, IpcError> {
    let sources = crate::sources::state(&app)?;
    time::week::plan(&sources.pool, sources.registry.as_ref(), &days).await
}

/// Log the week: one worklog per day and ticket, from that day's unlogged
/// manual blocks (issue #283, spec story 43).
///
/// The worklogs it made. Each is queued, copied and flushed the way a single
/// *Log* is, so they appear in the pending-writes panel like any other write.
/// Passive and label blocks are untouched -- [`crate::time::week::plan`]
/// carries why each exclusion is a rule rather than a filter.
///
/// A day and ticket that fails does **not** roll back the ones that succeeded:
/// the copies are knobas' record of writes that may already have landed, and
/// ADR-0012 forbids losing one. The failure is reported once the rest of the
/// week is logged.
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) before bring-up,
/// [`Invalid`](crate::IpcErrorCode::Invalid) for a day list the timesheet
/// would refuse, and otherwise whatever the queue or the database said.
#[tauri::command]
pub async fn log_all<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    days: Vec<time::week::DayWindow>,
) -> Result<Vec<time::worklog::Worklog>, IpcError> {
    let sources = crate::sources::state(&app)?;
    time::week::log_all(&sources, &days).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use knobas_sync::mirror::{assert_shape, declared_union, interface_body};

    const MIRROR: &str = include_str!("../../../../app/src/lib/ipc/time.ts");

    fn at(hour: u32, minute: u32) -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, 3, hour, minute, 0).unwrap()
    }

    /// Both halves of the target, in the shape the picker and the strip
    /// branch on. Two interfaces plus a union rather than one interface with
    /// two optional fields: `TimerTarget` is exactly-one on the Rust side and
    /// in the schema, and a mirror that allowed both would be the one place
    /// the rule was not stated.
    #[test]
    fn both_halves_of_a_target_match_their_typescript_mirror() {
        assert_shape(
            MIRROR,
            "EntityTarget",
            &serde_json::to_value(TimerTarget::Entity {
                entity_id: "jira:PAY-231".to_owned(),
            })
            .unwrap(),
            &["kind", "entity_id"],
        );
        assert_shape(
            MIRROR,
            "LabelTarget",
            &serde_json::to_value(TimerTarget::Label {
                label: "DB config for the migration".to_owned(),
            })
            .unwrap(),
            &["kind", "label"],
        );
    }

    #[test]
    fn the_running_timer_shape_matches_its_typescript_mirror() {
        let timer = RunningTimer {
            target: TimerTarget::Entity {
                entity_id: "jira:PAY-231".to_owned(),
            },
            started_at: at(9, 30),
            last_heartbeat: at(10, 0),
        };
        assert_shape(
            MIRROR,
            "RunningTimer",
            &serde_json::to_value(&timer).unwrap(),
            &["target", "started_at", "last_heartbeat"],
        );
    }

    /// `worklog_id` is exercised as `None`, the discipline `entity_mirror.rs`
    /// records: an `Option` serializes to a `null` *key*, and the mirror
    /// declares `number | null` on that promise.
    #[test]
    fn the_block_shape_matches_its_typescript_mirror() {
        let block = Block {
            id: 7,
            started_at: at(9, 30),
            ended_at: at(10, 15),
            target: TimerTarget::Label {
                label: "DB config for the migration".to_owned(),
            },
            kind: crate::time::BlockKind::Manual,
            ended_by_relaunch: true,
            worklog_id: None,
        };
        assert_shape(
            MIRROR,
            "Block",
            &serde_json::to_value(&block).unwrap(),
            &[
                "id",
                "started_at",
                "ended_at",
                "target",
                "kind",
                "ended_by_relaunch",
                "worklog_id",
            ],
        );
    }

    /// The kinds, read out of the mirror rather than listed here -- the rule
    /// `entity_mirror.rs` states: a hand-copied list of members passes while
    /// both the union and the copy drift from the Rust enum.
    #[test]
    fn the_block_kinds_match_their_typescript_mirror() {
        let rust: Vec<String> = [
            crate::time::BlockKind::Manual,
            crate::time::BlockKind::Passive,
        ]
        .iter()
        .map(|kind| {
            serde_json::to_value(kind)
                .unwrap()
                .as_str()
                .expect("a kind serializes to a string")
                .to_owned()
        })
        .collect();
        let mut declared = declared_union(MIRROR, "BlockKind");
        declared.sort();
        let mut rust = rust;
        rust.sort();
        assert_eq!(
            rust, declared,
            "the mirror's BlockKind and `time::BlockKind` no longer agree, so \
             a block knobas wrote is a block the day review cannot classify"
        );
    }

    /// The discriminated union has to be declared as one, or the picker's
    /// `switch` has nothing to narrow on.
    #[test]
    fn the_mirror_declares_the_target_as_a_union_of_its_two_halves() {
        let mut declared = declared_union(MIRROR, "TimerTarget");
        declared.sort();
        assert_eq!(
            declared,
            ["EntityTarget", "LabelTarget"],
            "TimerTarget has to be the union of exactly the two halves the Rust \
             enum has -- a third member is a target with no writer, and a \
             missing one is a target the frontend cannot express"
        );
        for half in ["EntityTarget", "LabelTarget"] {
            assert!(
                interface_body(MIRROR, half).contains("kind:"),
                "{half} carries no `kind`, so the union cannot be narrowed"
            );
        }
    }

    /// The shell refuses the same word the backend does (#278, story 15).
    ///
    /// Three copies of `ctx` decide whether a stored context can be timed:
    /// `knobas_core::entity::RESERVED_NAMESPACES`, `time::CONTEXT_NAMESPACE`,
    /// and `app/src/lib/shell/timer.ts`'s own constant. The first two are
    /// pinned in `time`'s own tests; this is the third, and it is the one that
    /// fails **silently** without a pin — a rename in Rust would leave the
    /// picker and the launcher filtering for a spelling nothing produces any
    /// more, so every list would go on looking correct while offering a target
    /// the backend then refuses.
    ///
    /// A source scan, for the reason `tests/wiring.rs` gives for its own: this
    /// is a TypeScript constant, and there is nothing else in the tree that
    /// compares the two.
    #[test]
    fn the_shells_context_namespace_is_the_one_the_backend_refuses() {
        const SHELL: &str = include_str!("../../../../app/src/lib/shell/timer.ts");
        let declaration = format!(
            "const CONTEXT_NAMESPACE = \"{}\";",
            crate::time::CONTEXT_NAMESPACE
        );
        assert!(
            SHELL.contains(&declaration),
            "app/src/lib/shell/timer.ts does not declare `{declaration}`, so the \
             picker and the launcher are filtering for a spelling the backend \
             no longer refuses"
        );
    }

    /// The four commands, named in the mirror's `invoke` calls.
    ///
    /// `tests/wiring.rs` proves every declared command is registered; this
    /// proves the frontend calls them by the names they are registered under.
    /// A typo on either side is a call that fails only at run time.
    #[test]
    fn the_mirror_invokes_the_commands_by_their_registered_names() {
        for command in [
            "current_timer",
            "start_timer",
            "stop_timer",
            "timer_heartbeat",
            "day_blocks",
            "update_block",
            "delete_block",
            "create_block",
            "passive_attribution",
            "set_passive_attribution",
            "worklog_draft",
            "log_work",
            "ad_hoc_block",
            "week_timesheet",
            "log_all_preview",
            "log_all",
        ] {
            assert!(
                MIRROR.contains(&format!("\"{command}\"")),
                "{command} is not invoked from app/src/lib/ipc/time.ts"
            );
            let registry = include_str!("../lib.rs");
            assert!(
                registry.contains(&format!("commands::time::{command}")),
                "{command} is not in the generate_handler! list"
            );
        }
    }

    /// Tauri renames a command's *arguments* to camelCase and leaves struct
    /// fields alone. Both spellings are on this surface at once -- the
    /// `foreground` argument and the `entity_id` field inside it -- and
    /// getting either wrong is a call that arrives with the value missing and
    /// no error anywhere.
    #[test]
    fn the_mirror_sends_the_argument_names_tauri_expects() {
        for (call, argument) in [
            ("start_timer", "target"),
            ("timer_heartbeat", "foreground"),
            ("day_blocks", "from"),
            ("day_blocks", "to"),
            // camelCase, and the whole reason this test exists: the Rust
            // parameter is `started_at` and Tauri renames it. A mirror sending
            // `started_at` arrives with the value missing and no error
            // anywhere -- while `target`, a *field* inside the payload, keeps
            // its snake_case. Both spellings are on this one call.
            ("update_block", "startedAt"),
            ("update_block", "endedAt"),
            ("update_block", "target"),
            ("delete_block", "id"),
            // The same camelCase rename, on the command that carries no id --
            // which is the one place a wrong spelling would arrive as a block
            // starting at the Unix epoch rather than as a refusal.
            ("create_block", "startedAt"),
            ("create_block", "endedAt"),
            ("create_block", "target"),
            ("set_passive_attribution", "enabled"),
            ("worklog_draft", "entityId"),
            ("worklog_draft", "offsetMinutes"),
            ("log_work", "startedAt"),
            ("log_work", "offsetMinutes"),
            // The room, on the command that has taken a target since #278: a
            // start that sent the target and dropped this would record every
            // block as having run nowhere, and rule two would simply stop
            // firing -- with nothing on screen to say so.
            ("start_timer", "inRoom"),
            ("ad_hoc_block", "blockId"),
            ("ad_hoc_block", "offsetMinutes"),
            // One argument on all three, and it is a *list of structs*: Tauri
            // renames the argument and leaves the fields inside it alone, so
            // `days` here and `day`, `from`, `to` inside each entry.
            ("week_timesheet", "days"),
            ("log_all_preview", "days"),
            ("log_all", "days"),
        ] {
            let at = MIRROR
                .find(&format!("\"{call}\""))
                .unwrap_or_else(|| panic!("{call} is not invoked from the mirror"));
            let tail = &MIRROR[at..];
            let invocation = &tail[..tail.find(");").unwrap_or(tail.len())];
            assert!(
                invocation.contains(argument),
                "the mirror calls {call} without passing {argument}, so the \
                 command receives nothing: {invocation}"
            );
        }
    }

    /// The day review's row, both halves.
    ///
    /// `title` is exercised as `Some`, and the nested `block` as a value
    /// rather than a placeholder: `assert_shape` compares top-level keys, so
    /// what this pins is that the mirror declares `block` and `title` and
    /// nothing else -- the block's own fields are pinned by
    /// `the_block_shape_matches_its_typescript_mirror` above, which is the
    /// only place they should be stated.
    #[test]
    fn the_day_review_row_matches_its_typescript_mirror() {
        let row = crate::time::day::DayBlock {
            block: Block {
                id: 7,
                started_at: at(9, 30),
                ended_at: at(10, 15),
                target: TimerTarget::Entity {
                    entity_id: "jira:PAY-231".to_owned(),
                },
                kind: crate::time::BlockKind::Manual,
                ended_by_relaunch: false,
                worklog_id: None,
            },
            title: Some("Retry failed SEPA payouts".to_owned()),
        };
        assert_shape(
            MIRROR,
            "DayBlock",
            &serde_json::to_value(&row).unwrap(),
            &["block", "title"],
        );
    }

    /// ...and with no title, because that is the arm the strip falls back on
    /// and an `Option` serializes to a `null` **key** -- the discipline
    /// `entity_mirror.rs` records, and the reason the mirror declares
    /// `string | null` rather than an optional field.
    #[test]
    fn a_day_review_row_with_no_title_still_carries_the_key() {
        let row = crate::time::day::DayBlock {
            block: Block {
                id: 8,
                started_at: at(11, 0),
                ended_at: at(11, 0),
                target: TimerTarget::Label {
                    label: "DB config for the migration".to_owned(),
                },
                kind: crate::time::BlockKind::Manual,
                ended_by_relaunch: true,
                worklog_id: Some(77),
            },
            title: None,
        };
        let wire = serde_json::to_value(&row).unwrap();
        assert_eq!(wire["title"], serde_json::Value::Null);
        assert_shape(MIRROR, "DayBlock", &wire, &["block", "title"]);
    }

    // -- the worklog (#280) -------------------------------------------------

    fn candidate() -> crate::time::worklog::Candidate {
        crate::time::worklog::Candidate {
            id: "item:jira:PAY-231".to_owned(),
            source: crate::time::worklog::CandidateSource::Mirror,
            at: at(9, 30),
            entity_id: Some("jira:PAY-231".to_owned()),
            bullet: "- Retry SEPA payouts".to_owned(),
        }
    }

    /// `entity_id` is exercised as `Some` **and** the draft's candidate list
    /// as non-empty: an `Option` that is `None` still serialises to a `null`
    /// key, so either would satisfy `assert_shape`, but only a filled one
    /// witnesses the type the mirror declares.
    #[test]
    fn the_candidate_shape_matches_its_typescript_mirror() {
        assert_shape(
            MIRROR,
            "Candidate",
            &serde_json::to_value(candidate()).unwrap(),
            &["id", "source", "at", "entity_id", "bullet"],
        );
    }

    #[test]
    fn the_candidate_sources_match_their_typescript_mirror() {
        let rust: Vec<String> = [
            crate::time::worklog::CandidateSource::Mirror,
            crate::time::worklog::CandidateSource::Activity,
        ]
        .iter()
        .map(|source| {
            serde_json::to_value(source)
                .unwrap()
                .as_str()
                .expect("a candidate source serialises to a string")
                .to_owned()
        })
        .collect();
        let mut declared = declared_union(MIRROR, "CandidateSource");
        declared.sort();
        let mut rust = rust;
        rust.sort();
        assert_eq!(
            rust, declared,
            "the mirror's CandidateSource and the Rust enum no longer agree, so \
             the draft cannot say where a candidate came from"
        );
    }

    #[test]
    fn the_draft_shape_matches_its_typescript_mirror() {
        let draft = crate::time::worklog::Draft {
            entity_id: "jira:PAY-231".to_owned(),
            day: "2026-09-03".parse().expect("a date"),
            started_at: at(9, 0),
            ended_at: at(14, 0),
            seconds: 150 * 60,
            block_ids: vec![7, 8],
            candidates: vec![candidate()],
            comment: "- Retry SEPA payouts".to_owned(),
        };
        let json = serde_json::to_value(&draft).unwrap();
        assert_eq!(
            json["day"], "2026-09-03",
            "the day crosses as the reader's own spelling of it, which is what \
             the mirror declares and what `worklog_draft` takes back"
        );
        assert_shape(
            MIRROR,
            "Draft",
            &json,
            &[
                "entity_id",
                "day",
                "started_at",
                "ended_at",
                "seconds",
                "block_ids",
                "candidates",
                "comment",
            ],
        );
    }

    /// `remote_id` and `write_queue_id` are exercised as `Some`, the rule
    /// `entity_mirror.rs` records: a `None` serialises to a `null` key and
    /// would pass against a mirror declaring any type at all.
    #[test]
    fn the_worklog_shape_matches_its_typescript_mirror() {
        let worklog = crate::time::worklog::Worklog {
            id: 3,
            entity_id: "jira:PAY-231".to_owned(),
            started_at: at(9, 0),
            seconds: 150 * 60,
            comment: "- Retry SEPA payouts".to_owned(),
            block_ids: vec![7, 8],
            write_queue_id: Some(11),
            remote_id: Some("30007".to_owned()),
            created_at: at(14, 1),
        };
        assert_shape(
            MIRROR,
            "Worklog",
            &serde_json::to_value(&worklog).unwrap(),
            &[
                "id",
                "entity_id",
                "started_at",
                "seconds",
                "comment",
                "block_ids",
                "write_queue_id",
                "remote_id",
                "created_at",
            ],
        );
    }

    // -- the ad-hoc block (#281) --------------------------------------------

    /// `title` is exercised as `Some`: an `Option` that is `None` serialises to
    /// a `null` key and would satisfy `assert_shape` against any declared type
    /// at all -- the discipline `entity_mirror.rs` records.
    #[test]
    fn the_suggestion_shape_matches_its_typescript_mirror() {
        assert_shape(
            MIRROR,
            "Suggestion",
            &serde_json::to_value(crate::time::suggest::Suggestion {
                entity_id: "jira:PAY-231".to_owned(),
                title: Some("Retry failed SEPA payouts".to_owned()),
                rule: crate::time::suggest::SuggestionRule::LinkedToTarget,
            })
            .unwrap(),
            &["entity_id", "title", "rule"],
        );
    }

    /// The rules, read out of the mirror rather than listed here -- the rule
    /// `entity_mirror.rs` states: a hand-copied list of members passes while
    /// both the union and the copy drift from the Rust enum. The dialog
    /// switches its *reason* on these three words, so a member that exists on
    /// one side only is a suggestion drawn with no reason beside it.
    #[test]
    fn the_suggestion_rules_match_their_typescript_mirror() {
        let mut rust: Vec<String> = [
            crate::time::suggest::SuggestionRule::LinkedToTarget,
            crate::time::suggest::SuggestionRule::ContextAnchor,
            crate::time::suggest::SuggestionRule::LastLogged,
        ]
        .iter()
        .map(|rule| {
            serde_json::to_value(rule)
                .unwrap()
                .as_str()
                .expect("a rule serialises to a string")
                .to_owned()
        })
        .collect();
        let mut declared = declared_union(MIRROR, "SuggestionRule");
        declared.sort();
        rust.sort();
        assert_eq!(
            rust, declared,
            "the mirror's SuggestionRule and the Rust enum no longer agree, so \
             the dialog cannot say why it suggested what it suggested"
        );
    }

    /// The offer, with its suggestion **present** -- the arm that carries a
    /// type. Its empty arm is a `null` key, which the mirror declares as
    /// `Suggestion | null` and which no fixture can distinguish.
    #[test]
    fn the_ad_hoc_block_shape_matches_its_typescript_mirror() {
        let offer = crate::time::suggest::AdHocBlock {
            suggestion: Some(crate::time::suggest::Suggestion {
                entity_id: "jira:PAY-231".to_owned(),
                title: None,
                rule: crate::time::suggest::SuggestionRule::ContextAnchor,
            }),
        };
        assert_shape(
            MIRROR,
            "AdHocBlock",
            &serde_json::to_value(&offer).unwrap(),
            &["suggestion"],
        );
    }

    /// **ADR-0012's sentence, verbatim, on the draft** (#280's criterion).
    ///
    /// Read out of the ADR and out of the component, and compared with
    /// whitespace collapsed on both sides -- the file is Prettier-formatted, so
    /// the sentence is wrapped across lines there and a literal `contains`
    /// would fail on the formatting rather than on the words.
    ///
    /// A source scan from Rust, for the reason `the_shells_context_namespace_is_the_one_the_backend_refuses`
    /// gives for its own: this is prose in a `.svelte` file, and nothing else
    /// in the tree compares it to the decision record it comes from. The
    /// alternative is a frontend test holding its own copy of the sentence,
    /// which is a second place for it to drift from the ADR.
    #[test]
    fn the_draft_quotes_adr_0012s_sentence_verbatim() {
        const ADR: &str = include_str!(
            "../../../../docs/adr/0012-writes-are-at-least-once-no-idempotency-key.md"
        );
        const DRAFT: &str = include_str!("../../../../app/src/lib/time/WorklogDraft.svelte");

        let flatten = |text: &str| text.split_whitespace().collect::<Vec<_>>().join(" ");
        let sentence = ADR
            .lines()
            .find(|line| line.starts_with("> A write knobas was sending"))
            .map(|line| flatten(line.trim_start_matches("> ")))
            .expect("ADR-0012 states the canonical sentence as a block quote");

        assert!(
            flatten(DRAFT).contains(&sentence),
            "the worklog draft no longer quotes ADR-0012's sentence verbatim. \
             It is the one thing a person can observe about the write queue's \
             guarantee, and the draft is where a re-send is contemplated:\n  {sentence}"
        );
    }

    // -- the week timesheet (#283) ------------------------------------------

    fn window() -> crate::time::week::DayWindow {
        crate::time::week::DayWindow {
            day: "2026-08-24".parse().expect("a date"),
            from: at(0, 0),
            to: at(0, 0) + chrono::Duration::days(1),
        }
    }

    /// The day window the webview builds, in the shape it sends it.
    ///
    /// A struct on the wire and therefore **snake_case fields inside a
    /// camelCase argument** -- the split
    /// `the_mirror_sends_the_argument_names_tauri_expects` exists for. Getting
    /// this one wrong is a week read that arrives with every window at the
    /// Unix epoch rather than an error anybody sees.
    #[test]
    fn the_day_window_matches_its_typescript_mirror() {
        assert_shape(
            MIRROR,
            "DayWindow",
            &serde_json::to_value(window()).unwrap(),
            &["day", "from", "to"],
        );
    }

    /// The cell's four numbers, all four declared.
    #[test]
    fn the_week_cell_matches_its_typescript_mirror() {
        let cell = crate::time::week::WeekCell {
            tracked_seconds: 5_400,
            offered_seconds: 900,
            logged_seconds: 3_600,
            held_seconds: 0,
            unlogged_seconds: 1_800,
        };
        assert_shape(
            MIRROR,
            "WeekCell",
            &serde_json::to_value(cell).unwrap(),
            &[
                "tracked_seconds",
                "offered_seconds",
                "logged_seconds",
                "held_seconds",
                "unlogged_seconds",
            ],
        );
    }

    /// The row, with a target and a title -- and then the **no-target** row,
    /// which is the arm the "app open" line is drawn on and the one where both
    /// `Option`s are `None`.
    ///
    /// Both, because an `Option` serialises to a `null` key either way: only a
    /// filled row witnesses the type the mirror declares, and only the empty
    /// one witnesses that the keys survive being empty.
    #[test]
    fn the_week_row_matches_its_typescript_mirror_with_a_target_and_without() {
        let row = crate::time::week::WeekRow {
            target: Some(TimerTarget::Entity {
                entity_id: "jira:PAY-231".to_owned(),
            }),
            title: Some("Retry failed SEPA payouts".to_owned()),
            cells: vec![crate::time::week::WeekCell::default()],
        };
        assert_shape(
            MIRROR,
            "WeekRow",
            &serde_json::to_value(&row).unwrap(),
            &["target", "title", "cells"],
        );

        let open = crate::time::week::WeekRow {
            target: None,
            title: None,
            cells: vec![crate::time::week::WeekCell::default()],
        };
        let wire = serde_json::to_value(&open).unwrap();
        assert_eq!(wire["target"], serde_json::Value::Null);
        assert_eq!(wire["title"], serde_json::Value::Null);
        assert_shape(MIRROR, "WeekRow", &wire, &["target", "title", "cells"]);
    }

    #[test]
    fn the_week_matches_its_typescript_mirror() {
        let week = crate::time::week::Week {
            days: vec!["2026-08-24".parse().expect("a date")],
            rows: Vec::new(),
        };
        assert_shape(
            MIRROR,
            "Week",
            &serde_json::to_value(&week).unwrap(),
            &["days", "rows"],
        );
    }

    /// The confirmation's line. `title` is exercised as `Some`, the arm the
    /// reader recognises a ticket by.
    #[test]
    fn the_planned_worklog_matches_its_typescript_mirror() {
        let planned = crate::time::week::PlannedWorklog {
            day: "2026-08-24".parse().expect("a date"),
            entity_id: "jira:PAY-231".to_owned(),
            title: Some("Retry failed SEPA payouts".to_owned()),
            started_at: at(9, 0),
            seconds: 5_400,
            blocks: 2,
        };
        assert_shape(
            MIRROR,
            "PlannedWorklog",
            &serde_json::to_value(&planned).unwrap(),
            &[
                "day",
                "entity_id",
                "title",
                "started_at",
                "seconds",
                "blocks",
            ],
        );
    }
}
