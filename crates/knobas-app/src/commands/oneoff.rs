//! Commands' IPC (*Run…*, ADR-0060): the saved list, a run's start, stop,
//! lines and watch, the list of runs, and the login shell's `PATH`. The
//! window names a saved command by its id and a target, never a program or an
//! argument (ADR-0016, #1643).

use std::sync::Arc;

use tauri::ipc::Channel;

use knobas_core::checkout::oneoff::{Draft, OneOff};

use crate::oneoff::history::RunRow;
use crate::oneoff::run::{Batch, Started, Target};
use crate::oneoff::runs::{Runs, Sink};
use crate::oneoff::shell_path::{self, ShellPath};
use crate::{IpcError, Lifecycle};

/// A run's batches on the window's channel.
struct ChannelSink(Channel<Batch>);

impl Sink for ChannelSink {
    fn send(&self, batch: &Batch) -> bool {
        self.0.send(batch.clone()).is_ok()
    }
}

/// The saved commands, in the person's order.
///
/// # Errors
/// [`Internal`](crate::IpcErrorCode::Internal) if the read fails.
#[tauri::command]
pub async fn oneoff_commands(
    lifecycle: tauri::State<'_, Lifecycle>,
) -> Result<Vec<OneOff>, IpcError> {
    crate::oneoff::list(&lifecycle.pool()?).await
}

/// Saves the whole list, checked, and answers it as stored.
///
/// # Errors
/// [`Invalid`](crate::IpcErrorCode::Invalid) naming the entry that cannot be
/// saved, and why.
#[tauri::command]
pub async fn set_oneoff_commands(
    lifecycle: tauri::State<'_, Lifecycle>,
    drafts: Vec<Draft>,
) -> Result<Vec<OneOff>, IpcError> {
    crate::oneoff::save(&lifecycle.pool()?, &drafts).await
}

/// Runs saved command `command` on `target` of `entity_id`'s checkout. Its
/// lines follow on `channel` in batches, the last one carrying how it ended.
///
/// # Errors
/// As [`crate::oneoff::start`].
#[tauri::command]
pub async fn oneoff_run(
    lifecycle: tauri::State<'_, Lifecycle>,
    entity_id: String,
    command: String,
    target: Target,
    channel: Channel<Batch>,
) -> Result<Started, IpcError> {
    let pool = lifecycle.pool()?;
    crate::oneoff::start(
        &pool,
        &entity_id,
        &command,
        target,
        Arc::new(ChannelSink(channel)),
    )
    .await
}

/// Stops a run with everything it started.
#[tauri::command]
pub fn oneoff_stop(run: String) {
    Runs::global().stop(&run);
}

/// Every run, the running ones first, then newest first; of one repository
/// where `entity_id` names it. What a run printed is not in a row.
#[tauri::command]
pub fn oneoff_runs(entity_id: Option<String>) -> Vec<RunRow> {
    Runs::global().rows(entity_id.as_deref())
}

/// The kept lines `from` to `from + count` of a run, at most 20,000, with
/// its counts: what a dialog opened again reads, newest first.
#[tauri::command]
pub fn oneoff_lines(run: String, from: u32, count: u32) -> Option<Batch> {
    Runs::global().lines(&run, from, count)
}

/// A running run's next batches on `channel` too. `false` for a run no
/// longer known.
#[tauri::command]
pub fn oneoff_watch(run: String, channel: Channel<Batch>) -> bool {
    Runs::global().watch(&run, Arc::new(ChannelSink(channel)))
}

/// Forgets one run, or every run that ended.
#[tauri::command]
pub fn oneoff_forget(run: Option<String>) {
    Runs::global().forget(run.as_deref());
}

/// What the login shell answered for `PATH` this session, read now with
/// `again`, and whether *Use my shell's PATH* is on. `None` for the answer
/// where it was never asked and `again` is false.
///
/// # Errors
/// [`Internal`](crate::IpcErrorCode::Internal) if the setting's read fails.
#[tauri::command]
pub async fn oneoff_shell_path(
    lifecycle: tauri::State<'_, Lifecycle>,
    again: bool,
    on: Option<bool>,
) -> Result<OneOffShell, IpcError> {
    let pool = lifecycle.pool()?;
    if let Some(on) = on {
        crate::oneoff::set_shell_path(&pool, on).await?;
    }
    let on = crate::oneoff::shell_path_on(&pool).await?;
    let read = if again {
        Some(shell_path::get(true).await)
    } else {
        shell_path::known()
    };
    Ok(OneOffShell {
        on,
        offered: !cfg!(windows),
        read,
    })
}

/// *Where programs are found* in Settings › Commands.
#[derive(Clone, Debug, serde::Serialize, ts_rs::TS)]
#[ts(export_to = "app.ts")]
pub struct OneOffShell {
    /// *Use my shell's PATH*.
    pub on: bool,
    /// Whether the row is drawn: not on Windows.
    pub offered: bool,
    #[ts(optional = nullable)]
    pub read: Option<ShellPath>,
}

#[cfg(test)]
mod tests {
    const MIRROR: &str = include_str!("../../../../app/src/lib/ipc/oneoff.ts");

    #[test]
    fn the_mirror_invokes_the_commands_by_their_registered_names() {
        crate::commands::mirror::assert_mirrors("oneoff", MIRROR, "app/src/lib/ipc/oneoff.ts", 9);
    }

    #[test]
    fn the_mirror_sends_the_arguments_by_their_names() {
        use crate::commands::mirror::{invocation, keys};
        for (command, args) in [
            ("set_oneoff_commands", &["drafts"][..]),
            ("oneoff_run", &["entityId", "command", "target", "channel"]),
            ("oneoff_stop", &["run"]),
            ("oneoff_runs", &["entityId"]),
            ("oneoff_lines", &["run", "from", "count"]),
            ("oneoff_watch", &["run", "channel"]),
            ("oneoff_forget", &["run"]),
            ("oneoff_shell_path", &["again", "on"]),
        ] {
            assert_eq!(keys(invocation(MIRROR, command)), args, "{command}");
        }
    }
}
