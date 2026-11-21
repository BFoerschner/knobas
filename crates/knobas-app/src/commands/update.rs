//! knobas updates itself (ADR-0052): the shims over [`crate::update::Updater`].
//!
//! No command takes the pool: the updater's settings are a file, so the page and the start
//! screen's line answer while the database is down. A command in the wrong phase (`update_install`
//! with nothing downloaded, a second download) answers `invalid`, so a double press cannot start
//! two installs.
//!
//! Generic over the runtime for the reason `sync_now` is: a bare `AppHandle` cannot be registered
//! on the `tauri::test` mock app.

use knobas_update::channel::Channel;

use crate::IpcError;
use crate::update::{self, UpdateNotes, UpdateState};

fn refused(error: update::Refused) -> IpcError {
    IpcError::invalid(error)
}

/// What is known, from memory: no request, no pool.
///
/// # Errors
///
/// `not_ready` before `setup` managed the updater.
#[tauri::command]
pub fn update_state<R: tauri::Runtime>(app: tauri::AppHandle<R>) -> Result<UpdateState, IpcError> {
    Ok(update::updater(&app)?.state())
}

/// A check now.
///
/// # Errors
///
/// `invalid` while not set up, downloading or installing.
#[tauri::command]
pub async fn update_check<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
) -> Result<UpdateState, IpcError> {
    update::updater(&app)?.check().await.map_err(refused)
}

/// Store *Check for updates* or the channel; a changed channel checks at once.
///
/// # Errors
///
/// `invalid` for a channel change while downloading or installing.
#[tauri::command]
pub async fn update_set<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    check: Option<bool>,
    channel: Option<Channel>,
) -> Result<UpdateState, IpcError> {
    update::updater(&app)?
        .set(check, channel)
        .await
        .map_err(refused)
}

/// Start the download of the offered version.
///
/// # Errors
///
/// `invalid` with nothing to download, a copy updated by hand, or one running.
#[tauri::command]
pub fn update_download<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
) -> Result<UpdateState, IpcError> {
    update::updater(&app)?.download().map_err(refused)
}

/// Stop the download.
///
/// # Errors
///
/// `not_ready` before `setup` managed the updater.
#[tauri::command]
pub fn update_cancel<R: tauri::Runtime>(app: tauri::AppHandle<R>) -> Result<UpdateState, IpcError> {
    Ok(update::updater(&app)?.cancel())
}

/// *Restart to update*: check the download again, back up, put it in place and quit.
///
/// # Errors
///
/// `invalid` unless a checked download is ready.
#[tauri::command]
pub fn update_install<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
) -> Result<UpdateState, IpcError> {
    update::updater(&app)?.install().map_err(refused)
}

/// *Later*: the dot goes until the next start or the next newer version.
///
/// # Errors
///
/// `not_ready` before `setup` managed the updater.
#[tauri::command]
pub fn update_later<R: tauri::Runtime>(app: tauri::AppHandle<R>) -> Result<UpdateState, IpcError> {
    Ok(update::updater(&app)?.later())
}

/// A release's notes and its page; the markdown is `null` when GitHub could not be asked.
///
/// # Errors
///
/// `not_ready` before `setup` managed the updater.
#[tauri::command]
pub async fn update_notes<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    version: String,
) -> Result<UpdateNotes, IpcError> {
    Ok(update::updater(&app)?.notes(&version).await)
}

#[cfg(test)]
mod tests {
    const MIRROR: &str = include_str!("../../../../app/src/lib/ipc/update.ts");

    #[test]
    fn the_mirror_invokes_the_commands_by_their_registered_names() {
        crate::commands::mirror::assert_mirrors("update", MIRROR, "app/src/lib/ipc/update.ts", 8);
    }

    #[test]
    fn the_mirror_sends_the_arguments_by_their_names() {
        use crate::commands::mirror::{invocation, keys};
        for (command, args) in [
            ("update_set", &["check", "channel"][..]),
            ("update_notes", &["version"]),
        ] {
            assert_eq!(keys(invocation(MIRROR, command)), args, "{command}");
        }
    }
}
