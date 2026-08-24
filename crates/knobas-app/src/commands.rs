//! The M0 IPC surface, mirrored in `app/src/lib/ipc.ts`.
//!
//! Every command is a thin shim: take the arguments Tauri deserialised, call
//! the crate that owns the behaviour, and flatten the error into a `String` the
//! frontend can show. Nothing here decides anything -- when a command grows a
//! policy, that policy belongs in a crate below it, with its own tests.
//!
//! Errors cross the bridge as `String` rather than a typed enum on purpose:
//! `Err(e)` from a command is what `invoke` rejects with, and M0 has no
//! frontend that branches on error *kind*, only one that displays it. The
//! error types below already fold their source's message into their own
//! `Display`, so `to_string` loses nothing.

use tauri::State;

use crate::AppState;

/// Liveness probe. Answers only once `setup` has managed the state, so a
/// successful `ping` means the database is up and migrated.
#[tauri::command]
pub fn ping() -> &'static str {
    "pong"
}

/// The best `limit` full-text matches for `q`.
///
/// `q` is user text in the `websearch_to_tsquery` dialect and is bound as a
/// parameter all the way down; it is never interpolated into SQL.
///
/// The returned `snippet` carries `<b>` marks around **unescaped** source text.
/// It is the frontend's job to render it as text -- see `ipc.ts`.
#[tauri::command]
pub async fn search(
    state: State<'_, AppState>,
    q: String,
    limit: u32,
) -> Result<Vec<knobas_db::search::SearchHit>, String> {
    knobas_db::search::search(&state.pool, &q, i64::from(limit))
        .await
        .map_err(|error| error.to_string())
}

/// The `limit` most recent activity-log lines, newest first.
#[tauri::command]
pub async fn recent_activity(
    state: State<'_, AppState>,
    limit: u32,
) -> Result<Vec<knobas_core::activity::ActivityRow>, String> {
    knobas_core::activity::recent(&state.pool, i64::from(limit))
        .await
        .map_err(|error| error.to_string())
}
