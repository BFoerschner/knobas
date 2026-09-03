//! The timer's four commands (issue #278) -- what the top strip, ⌘T and the
//! launcher's *Start timer* row call.
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
fn announce<R: tauri::Runtime>(app: &tauri::AppHandle<R>, line: knobas_core::activity::ActivityRow) {
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
pub async fn current_timer(lifecycle: State<'_, Lifecycle>) -> Result<Option<RunningTimer>, IpcError>
{
    let pool = lifecycle.pool()?;
    time::current(&pool).await
}

/// Start the timer on `target`.
///
/// The argument is the tagged target, not two nullable strings: exactly one of
/// an entity and a label is what the whole feature is about, and a shape that
/// could carry both would put that rule in the caller.
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) before bring-up,
/// [`Invalid`](crate::IpcErrorCode::Invalid) for a target that is not an
/// entity id, is a stored context, or is a blank label, and
/// [`Conflict`](crate::IpcErrorCode::Conflict) when a timer is already
/// running -- stop it first.
#[tauri::command]
pub async fn start_timer<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    lifecycle: State<'_, Lifecycle>,
    target: TimerTarget,
) -> Result<RunningTimer, IpcError> {
    let pool = lifecycle.pool()?;
    let started = time::start(&pool, target).await?;
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
/// else none*; [`crate::time`]'s module docs record why it is taken and not
/// yet stored.
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) before bring-up,
/// [`Invalid`](crate::IpcErrorCode::Invalid) for a foreground that could never
/// be a target, [`Internal`](crate::IpcErrorCode::Internal) if the write fails.
#[tauri::command]
pub async fn timer_heartbeat(
    lifecycle: State<'_, Lifecycle>,
    foreground: Option<TimerTarget>,
) -> Result<Option<RunningTimer>, IpcError> {
    let pool = lifecycle.pool()?;
    time::heartbeat(&pool, foreground).await
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
        for (call, argument) in [("start_timer", "target"), ("timer_heartbeat", "foreground")] {
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
}
