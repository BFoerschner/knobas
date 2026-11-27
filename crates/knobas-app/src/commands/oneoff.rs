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

/// A run's batches on the window's channel, **sent from the main thread** as
/// every event and the sync's progress are (`sources::progress::ChannelSink`,
/// #747): with Tauri's `tracing` feature a send is a script the webview runs
/// synchronously, and a run's thread waiting on the main thread while a quit
/// waits on the run held both for good. The last drop goes there too.
struct ChannelSink {
    channel: Option<Channel<Batch>>,
    on_main: OnMain,
}

/// What runs a task on the main thread: `AppHandle::run_on_main_thread`.
type OnMain = Box<dyn Fn(Box<dyn FnOnce() + Send>) + Send + Sync>;

impl ChannelSink {
    fn new<R: tauri::Runtime>(app: &tauri::AppHandle<R>, channel: Channel<Batch>) -> Self {
        let app = app.clone();
        Self {
            channel: Some(channel),
            on_main: Box::new(move |task| {
                if let Err(error) = app.run_on_main_thread(task) {
                    tracing::debug!(%error, "the main thread is gone: a run's batch not sent");
                }
            }),
        }
    }
}

impl Sink for ChannelSink {
    fn send(&self, batch: &Batch) -> bool {
        let Some(channel) = self.channel.clone() else {
            return false;
        };
        let batch = batch.clone();
        (self.on_main)(Box::new(move || {
            // A closed channel is a dialog that closed: the run goes on.
            if let Err(error) = channel.send(batch) {
                tracing::debug!(%error, "a run's channel is closed");
            }
        }));
        true
    }
}

impl Drop for ChannelSink {
    fn drop(&mut self) {
        if let Some(channel) = self.channel.take() {
            (self.on_main)(Box::new(move || drop(channel)));
        }
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
/// A generated folder's target runs in the folder `made_token` names, where
/// knobas kept it (ADR-0060, amended 2026-11-25).
///
/// # Errors
/// As [`crate::oneoff::start`].
#[tauri::command]
pub async fn oneoff_run<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    lifecycle: tauri::State<'_, Lifecycle>,
    entity_id: String,
    command: String,
    target: Target,
    made_token: Option<String>,
    channel: Channel<Batch>,
) -> Result<Started, IpcError> {
    let pool = lifecycle.pool()?;
    crate::oneoff::start(
        &pool,
        &entity_id,
        &command,
        target,
        made_token.as_deref(),
        Arc::new(ChannelSink::new(&app, channel)),
    )
    .await
}

/// Tries saved command `command` on pasted text, a chosen file or a chosen
/// folder (#2562), from its settings page: run as on a checkout, with the
/// chosen folder in its place. Its lines follow on `channel` as a run's do,
/// and the run dialog reads it back by its id; it is no row in the list of
/// runs, and a throwaway file goes when it ends.
///
/// # Errors
/// As [`crate::oneoff::try_start`].
#[tauri::command]
pub async fn oneoff_try<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    lifecycle: tauri::State<'_, Lifecycle>,
    command: String,
    on: crate::try_on::TryOn,
    channel: Channel<Batch>,
) -> Result<Started, IpcError> {
    let pool = lifecycle.pool()?;
    crate::oneoff::try_start(
        &pool,
        &command,
        on,
        Arc::new(ChannelSink::new(&app, channel)),
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
pub fn oneoff_watch<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    run: String,
    channel: Channel<Batch>,
) -> bool {
    Runs::global().watch(&run, Arc::new(ChannelSink::new(&app, channel)))
}

/// *Download*: what run `run` printed, as text (with its colour codes where
/// `colours`), or with `result` what it made (`file`, one of the files it
/// wrote to `{outdir}`), saved to the Downloads folder under `name` (a free
/// name beside any file there). Answers the path; knobas keeps no copy.
///
/// # Errors
/// `not_found` once the run is no longer known; the folder or the write fails.
#[tauri::command]
pub async fn oneoff_save(
    run: String,
    name: String,
    colours: bool,
    result: bool,
    file: Option<String>,
) -> Result<String, IpcError> {
    use tokio::io::AsyncWriteExt as _;
    let gone = || {
        IpcError::new(
            crate::IpcErrorCode::NotFound,
            "What it printed is no longer kept.",
        )
    };
    let bytes = if result {
        let handle = Runs::global().get(&run).ok_or_else(gone)?;
        handle
            .result_bytes(file.as_deref())
            .map_err(|why| IpcError::new(crate::IpcErrorCode::NotFound, why))?
    } else {
        Runs::global()
            .text(&run, colours)
            .ok_or_else(gone)?
            .into_bytes()
    };
    let dir = dirs::download_dir().ok_or_else(|| {
        IpcError::new(
            crate::IpcErrorCode::Internal,
            "This computer has no Downloads folder.",
        )
    })?;
    let (path, mut out) = crate::downloads::create_free(&dir, &crate::downloads::safe_name(&name))
        .await
        .map_err(|(path, error)| IpcError::internal(format!("{}: {error}", path.display())))?;
    out.write_all(&bytes)
        .await
        .map_err(|error| IpcError::internal(format!("{}: {error}", path.display())))?;
    Ok(path.display().to_string())
}

/// The bytes of run `run`'s result: its stdout that was not text, the file it
/// wrote to `{out}`, or `file`, one it wrote to `{outdir}`. Served as raw
/// bytes, as a generated folder's file is.
///
/// # Errors
/// `not_found` for a run no longer known or a result not kept.
#[tauri::command]
pub fn oneoff_result(run: String, file: Option<String>) -> Result<tauri::ipc::Response, IpcError> {
    let handle = Runs::global().get(&run).ok_or_else(|| {
        IpcError::new(crate::IpcErrorCode::NotFound, "The run is no longer kept.")
    })?;
    handle
        .result_bytes(file.as_deref())
        .map(tauri::ipc::Response::new)
        .map_err(|why| IpcError::new(crate::IpcErrorCode::NotFound, why))
}

/// What a repository's `.knobas/commands.toml` offers, read from the checkout
/// on disk now (ADR-0060, section 22). Nothing in it runs.
///
/// # Errors
/// As [`crate::oneoff::repo_commands`].
#[tauri::command]
pub async fn oneoff_repo_commands(
    lifecycle: tauri::State<'_, Lifecycle>,
    entity_id: String,
) -> Result<crate::oneoff::RepoCommands, IpcError> {
    crate::oneoff::repo_commands(&lifecycle.pool()?, &entity_id).await
}

/// Copies the named commands of a repository's file into the person's list,
/// scoped to that repository, after they read them; answers the list.
///
/// # Errors
/// As [`crate::oneoff::copy_repo_commands`].
#[tauri::command]
pub async fn oneoff_copy_repo_commands(
    lifecycle: tauri::State<'_, Lifecycle>,
    entity_id: String,
    names: Vec<String>,
) -> Result<Vec<OneOff>, IpcError> {
    crate::oneoff::copy_repo_commands(&lifecycle.pool()?, &entity_id, &names).await
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
        crate::commands::mirror::assert_mirrors("oneoff", MIRROR, "app/src/lib/ipc/oneoff.ts", 13);
    }

    #[test]
    fn the_mirror_sends_the_arguments_by_their_names() {
        use crate::commands::mirror::{invocation, keys};
        for (command, args) in [
            ("set_oneoff_commands", &["drafts"][..]),
            (
                "oneoff_run",
                &["entityId", "command", "target", "madeToken", "channel"],
            ),
            ("oneoff_stop", &["run"]),
            ("oneoff_runs", &["entityId"]),
            ("oneoff_lines", &["run", "from", "count"]),
            ("oneoff_watch", &["run", "channel"]),
            ("oneoff_forget", &["run"]),
            ("oneoff_save", &["run", "name", "colours", "result", "file"]),
            ("oneoff_result", &["run", "file"]),
            ("oneoff_repo_commands", &["entityId"]),
            ("oneoff_copy_repo_commands", &["entityId", "names"]),
            ("oneoff_shell_path", &["again", "on"]),
        ] {
            assert_eq!(keys(invocation(MIRROR, command)), args, "{command}");
        }
    }
}
