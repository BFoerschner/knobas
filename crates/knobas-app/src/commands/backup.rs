//! Backup export and restore -- what the §14 settings surface ("export now /
//! schedule / restore", plain dialogs) calls: the backup section of the
//! settings view at `#/settings` (issue **#69**, landed in PR #102).
//!
//! Shims, like every other module here: the decisions are in
//! [`crate::backup`], which is testable without a window.
//!
//! Each command takes an `AppHandle` and asks it for the state, rather than
//! declaring `State<'_, Arc<BackupState>>` as an argument. Carry-over
//! §10.6(a): the backup service is managed only once the database is up, and a
//! command declaring its state is rejected by Tauri *before its body runs*,
//! with the bare string `"state not managed"` -- no code, and
//! `IpcErrorCode::NotReady` unreachable. `backup::state(&app)` answers
//! `not_ready` instead.
//!
//! Generic over the runtime for the reason `sync_now` is (§10.2): a bare
//! `tauri::AppHandle` means `AppHandle<Wry>`, and a command taking one cannot
//! be registered on the `tauri::test` mock app at all.

use std::sync::Arc;

use crate::IpcError;
use crate::backup::{self, BackupRecord, BackupSchedule, BackupState, BackupStatus, ShareParts};

/// The state, cloned out of the managed `Arc` so no Tauri guard is held across
/// an await.
fn service<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Result<Arc<BackupState>, IpcError> {
    Ok(Arc::clone(backup::state(app)?.inner()))
}

/// The schedule, the last export, what is on disk and where.
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) before the database is up,
/// [`Internal`](crate::IpcErrorCode::Internal) if the settings cannot be read.
#[tauri::command]
pub async fn backup_status<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
) -> Result<BackupStatus, IpcError> {
    let service = service(&app)?;
    Ok(backup::status(&service).await?)
}

/// *Export now*: take a backup whatever the schedule says.
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) before the database is up,
/// [`Internal`](crate::IpcErrorCode::Internal) if `pg_dump` fails -- its own
/// stderr is the message, because that is the only thing that says *why*.
#[tauri::command]
pub async fn backup_now<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
) -> Result<BackupRecord, IpcError> {
    let service = service(&app)?;
    Ok(backup::export_now(&service).await?)
}

/// *Share export*: an archive restricted to the parts a colleague should get.
///
/// Spec #427 (M4.2). The same custom-format archive `backup_now` writes and
/// `restore_backup` reads, holding only the chosen parts -- so it restores on
/// a clean machine with the existing restore and opens in a standard tool.
/// It is not a backup: it does not reset the nightly schedule's clock and
/// retention never deletes it.
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) before the database is up,
/// [`Invalid`](crate::IpcErrorCode::Invalid) when every part is switched off,
/// [`Internal`](crate::IpcErrorCode::Internal) if `pg_dump` fails.
#[tauri::command]
pub async fn share_export<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    parts: ShareParts,
) -> Result<BackupRecord, IpcError> {
    let service = service(&app)?;
    Ok(backup::share_export(&service, parts).await?)
}

/// Change the nightly schedule. Answers with the whole status, so the dialog
/// redraws its next-run line from the same read that stored the change.
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) before the database is up,
/// [`Internal`](crate::IpcErrorCode::Internal) if the write fails.
#[tauri::command]
pub async fn set_backup_schedule<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    schedule: BackupSchedule,
) -> Result<BackupStatus, IpcError> {
    let service = service(&app)?;
    backup::save_schedule(&service.pool, schedule).await?;
    Ok(backup::status(&service).await?)
}

/// Restore one of this profile's archives into the database.
///
/// Minimal by ratification: it restores into a knobas that holds no data yet,
/// which is the fresh-machine path. Restoring over a populated database means
/// merging, and merge-with-a-preview is M4 -- so that case is a
/// [`Conflict`](crate::IpcErrorCode::Conflict) rather than an overwrite.
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) before the database is up,
/// [`NotFound`](crate::IpcErrorCode::NotFound) for a name that is not an
/// archive in the backup directory,
/// [`Conflict`](crate::IpcErrorCode::Conflict) when the database still holds
/// knobas data.
#[tauri::command]
pub async fn restore_backup<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    file: String,
) -> Result<(), IpcError> {
    let service = service(&app)?;
    Ok(backup::restore(&service, &file).await?)
}

/// *Open the backup folder* (#1704): the running profile's `backups` folder
/// in the file manager, created first if no export has made it yet.
///
/// No argument: the folder is the [`Profile`](crate::Profile)'s, so a demo
/// window cannot name the real profile's archives (P13). Needs no database.
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) before `setup` has managed the
/// profile and the opener,
/// [`Internal`](crate::IpcErrorCode::Internal) naming the folder when it
/// cannot be created or the file manager refuses it.
#[tauri::command]
pub async fn open_backup_folder<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
) -> Result<(), IpcError> {
    use tauri::Manager as _;

    let starting = || IpcError::not_ready("knobas is still starting".to_owned());
    let profile = app.try_state::<crate::Profile>().ok_or_else(starting)?;
    let opener = app.try_state::<backup::FolderOpener>().ok_or_else(starting)?;
    backup::folder::open(&profile, &opener)
}

#[cfg(test)]
mod tests {
    const MIRROR: &str = include_str!("../../../../app/src/lib/ipc/backup.ts");

    /// Every backup command in the handler list is invoked from the mirror.
    ///
    /// `tests/it/ipc.rs` proves they are registered and dispatch; this proves the
    /// frontend calls them by the names they are registered under. A typo on
    /// either side is a call that fails only at run time.
    #[test]
    fn the_mirror_invokes_the_commands_by_their_registered_names() {
        crate::commands::mirror::assert_mirrors("backup", MIRROR, "app/src/lib/ipc/backup.ts", 6);
    }
}
