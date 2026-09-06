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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    const MIRROR: &str = include_str!("../../../../app/src/lib/ipc/backup.ts");

    /// The exact key set of every DTO this module puts on the wire, and each
    /// key present in the hand-written mirror.
    ///
    /// A `contains` check over a hardcoded list is blind to a *field* being
    /// added on the Rust side, which is the direction that breaks a frontend
    /// silently -- so the keys are read off the serialized value.
    fn keys(value: &serde_json::Value) -> Vec<String> {
        let mut keys: Vec<String> = value
            .as_object()
            .expect("an object")
            .keys()
            .cloned()
            .collect();
        keys.sort();
        keys
    }

    fn assert_mirrored(value: &serde_json::Value, expected: &[&str]) {
        assert_eq!(keys(value), expected);
        for key in expected {
            assert!(
                MIRROR.contains(&format!("{key}:")),
                "{key} is missing from app/src/lib/ipc/backup.ts"
            );
        }
    }

    #[test]
    fn the_schedule_serialises_the_keys_the_mirror_declares() {
        let json = serde_json::to_value(BackupSchedule::default()).unwrap();
        assert_mirrored(&json, &["enabled", "hour", "keep", "minute"]);
        assert_eq!(json["hour"], 3, "the ratified default is nightly at 03:00");
    }

    /// ...and it decodes from what the dialog sends, which is the direction a
    /// serialize-only check cannot see.
    #[test]
    fn the_schedule_decodes_from_the_dialogs_shape() {
        let sent = serde_json::json!({
            "enabled": false, "hour": 22, "minute": 30, "keep": 3
        });
        let decoded: BackupSchedule = serde_json::from_value(sent).unwrap();
        assert_eq!(
            decoded,
            BackupSchedule {
                enabled: false,
                hour: 22,
                minute: 30,
                keep: 3
            }
        );
    }

    /// The share export's toggles, both directions.
    ///
    /// Serialised, so a field added on the Rust side and not in the mirror
    /// fails here rather than at run time; and decoded from what the dialog
    /// sends, which a serialise-only check cannot see.
    #[test]
    fn the_share_parts_serialise_the_keys_the_mirror_declares() {
        let json = serde_json::to_value(ShareParts::default()).unwrap();
        assert_mirrored(
            &json,
            &["assets", "contexts", "links", "notes", "sources", "time"],
        );
        assert_eq!(json["links"], true, "the ratified default is links on");
        assert_eq!(json["notes"], false, "the ratified default is notes off");
        assert_eq!(json["time"], false, "the ratified default is time off");

        let sent = serde_json::json!({
            "links": true, "assets": false, "contexts": false,
            "notes": true, "time": true, "sources": false
        });
        let decoded: ShareParts = serde_json::from_value(sent).unwrap();
        assert_eq!(
            decoded,
            ShareParts {
                links: true,
                assets: false,
                contexts: false,
                notes: true,
                time: true,
                sources: false,
            }
        );
    }

    #[test]
    fn the_status_and_the_record_serialise_the_keys_the_mirror_declares() {
        let record = BackupRecord {
            taken_at: Utc.with_ymd_and_hms(2026, 8, 28, 1, 0, 0).unwrap(),
            file: "knobas-20260828-030000.knobas".to_owned(),
            bytes: 4096,
        };
        assert_mirrored(
            &serde_json::to_value(&record).unwrap(),
            &["bytes", "file", "taken_at"],
        );

        let status = BackupStatus {
            schedule: BackupSchedule::default(),
            directory: "/Users/x/Library/Application Support/dev.knobas.desktop/backups".to_owned(),
            last: Some(record),
            next_due_at: Some(Utc.with_ymd_and_hms(2026, 8, 29, 1, 0, 0).unwrap()),
            archives: vec![crate::backup::ArchiveFile {
                file: "knobas-20260828-030000.knobas".to_owned(),
                bytes: 4096,
                share: false,
            }],
        };
        let json = serde_json::to_value(&status).unwrap();
        assert_mirrored(
            &json,
            &["archives", "directory", "last", "next_due_at", "schedule"],
        );
        // The rows inside, separately: `assert_mirrored` reads the keys of the
        // object it is handed, and a field added to a *nested* DTO is
        // invisible to it -- which is how `share` would have reached the
        // frontend unmirrored (#455).
        assert_mirrored(&json["archives"][0], &["bytes", "file", "share"]);
    }

    /// The five commands, named in the mirror's `invoke` calls.
    ///
    /// `tests/ipc.rs` proves they are registered and dispatch; this proves the
    /// frontend calls them by the names they are registered under. A typo on
    /// either side is a call that fails only at run time.
    #[test]
    fn the_mirror_invokes_the_commands_by_their_registered_names() {
        for command in [
            "backup_status",
            "backup_now",
            "share_export",
            "set_backup_schedule",
            "restore_backup",
        ] {
            assert!(
                MIRROR.contains(&format!("\"{command}\"")),
                "{command} is not invoked from app/src/lib/ipc/backup.ts"
            );
            let registry = include_str!("../lib.rs");
            assert!(
                registry.contains(&format!("commands::backup::{command}")),
                "{command} is not in the generate_handler! list"
            );
        }
    }
}
