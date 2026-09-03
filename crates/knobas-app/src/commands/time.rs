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
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) before bring-up,
/// [`Invalid`](crate::IpcErrorCode::Invalid) for a target that is not an
/// entity id or an offset that is not an offset.
#[tauri::command]
pub async fn worklog_draft<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    lifecycle: State<'_, Lifecycle>,
    entity_id: String,
    day: chrono::NaiveDate,
    offset_minutes: i32,
) -> Result<Option<time::worklog::Draft>, IpcError> {
    let pool = lifecycle.pool()?;
    let sources = crate::sources::state(&app)?;
    time::worklog::draft(
        &pool,
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
        const ADR: &str =
            include_str!("../../../../docs/adr/0012-writes-are-at-least-once-no-idempotency-key.md");
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
}
