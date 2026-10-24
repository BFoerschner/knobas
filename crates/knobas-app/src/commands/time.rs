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

use tauri::State;

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
    crate::events::emit(app, None, crate::events::ACTIVITY_NEW, &line);
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
/// The answer is a [`DayRecord`](time::day::DayRecord) rather than the bare
/// list it used to be, because one thing on it is true of the *day* and not of
/// any block: whether knobas still has the observations for it (#337).
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
) -> Result<time::day::DayRecord, IpcError> {
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

/// *Dismiss* an offered (passive) block (#900, R-13.4): it is gone, and the
/// day read does not offer a stretch starting where it did again.
///
/// Not `delete_block`, which deletes the row and nothing else: the day read
/// re-derives passive blocks from the observations before it reads, so a
/// deleted offer comes back. Nothing comes back from this either; the Day tab
/// re-reads the day.
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) before bring-up,
/// [`Invalid`](crate::IpcErrorCode::Invalid) for a block that is not an offer,
/// [`NotFound`](crate::IpcErrorCode::NotFound) for a block that is not there.
#[tauri::command]
pub async fn dismiss_block(lifecycle: State<'_, Lifecycle>, id: i64) -> Result<(), IpcError> {
    let pool = lifecycle.pool()?;
    time::passive::dismiss(&pool, id).await
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
/// Reads the pool and the registry from the [`Lifecycle`] (#1321): the draft
/// only needs the adapters' declarations, which the registry answers, so it
/// has one route to both and one reason to be `not_ready`.
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) before bring-up,
/// [`Invalid`](crate::IpcErrorCode::Invalid) for a target that is not an
/// entity id or an offset that is not an offset.
#[tauri::command]
pub async fn worklog_draft(
    lifecycle: State<'_, Lifecycle>,
    entity_id: String,
    day: chrono::NaiveDate,
    offset_minutes: i32,
) -> Result<Option<time::worklog::Draft>, IpcError> {
    let pool = lifecycle.pool()?;
    time::worklog::draft(&pool, lifecycle.registry(), &entity_id, day, offset_minutes).await
}

/// Log a day's work on a ticket: queue the write, keep the copy, and make the
/// blocks it covers read-only (issue #280).
///
/// `startedAt`, `seconds` and `comment` are what the reader settled on in the
/// draft; **which blocks are covered is not an argument**, and
/// [`crate::time::worklog::log`] records why -- a caller that could name them
/// could name another ticket's.
///
/// The answer is the local copy, once the write is queued and the copy kept
/// (#775): `remote_id` is `null` until the send behind the answer lands and
/// its settle stamps Jira's id on. The write itself is in the pending-writes
/// panel like any other, and `write_queue_id` names it.
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
    // Answers on queue (#775): the draft closes before Jira is asked.
    time::worklog::log_in_background(
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
/// Reads the pool and the registry from the [`Lifecycle`], as [`worklog_draft`]
/// does.
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) before bring-up,
/// [`NotFound`](crate::IpcErrorCode::NotFound) for a block that is not there,
/// [`Invalid`](crate::IpcErrorCode::Invalid) for an offset that is not an
/// offset.
#[tauri::command]
pub async fn ad_hoc_block(
    lifecycle: State<'_, Lifecycle>,
    block_id: i64,
    day: chrono::NaiveDate,
    offset_minutes: i32,
) -> Result<Option<time::suggest::AdHocBlock>, IpcError> {
    let pool = lifecycle.pool()?;
    time::suggest::offer(&pool, lifecycle.registry(), block_id, day, offset_minutes).await
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
/// system other people read. The dialog sends each row it ticks through
/// [`log_work`], which re-derives the blocks rather than being handed them.
///
/// Reads the pool and the registry from the [`Lifecycle`], as [`worklog_draft`]
/// does.
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) before bring-up,
/// [`Invalid`](crate::IpcErrorCode::Invalid) for a day list the timesheet
/// would refuse.
#[tauri::command]
pub async fn log_all_preview(
    lifecycle: State<'_, Lifecycle>,
    days: Vec<time::week::DayWindow>,
) -> Result<Vec<time::week::PlannedWorklog>, IpcError> {
    let pool = lifecycle.pool()?;
    time::week::plan(&pool, lifecycle.registry(), &days).await
}

#[cfg(test)]
mod tests {
    const MIRROR: &str = include_str!("../../../../app/src/lib/ipc/time.ts");

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
    /// A source scan, for the reason `tests/it/wiring.rs` gives for its own: this
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

    /// Every time command in the handler list is invoked from the mirror, and the reverse.
    ///
    /// `tests/it/wiring.rs` proves every declared command is registered; this
    /// proves the frontend calls them by the names they are registered under.
    /// A typo on either side is a call that fails only at run time.
    #[test]
    fn the_mirror_invokes_the_commands_by_their_registered_names() {
        crate::commands::mirror::assert_mirrors("time", MIRROR, "app/src/lib/ipc/time.ts", 16);
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
            ("dismiss_block", "id"),
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
            // One argument on both, and it is a *list of structs*: Tauri
            // renames the argument and leaves the fields inside it alone, so
            // `days` here and `day`, `from`, `to` inside each entry.
            ("week_timesheet", "days"),
            ("log_all_preview", "days"),
        ] {
            // The `invoke<..>(` call site, not the first quoted name: a doc
            // comment naming the command may come before it (#1984).
            let at = MIRROR
                .find(&format!(">(\"{call}\""))
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
        // The ticket-first shell's draft, since the rooms UI's went (#881).
        const DRAFT: &str = include_str!("../../../../app/src/lib/tf/TfWorklogDraft.svelte");

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
