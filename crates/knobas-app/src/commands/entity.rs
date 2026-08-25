//! Entity and room reads -- stream D (interfaces §2.5).
//!
//! Its `State<'_, Lifecycle>` is not an accident and is not stream D being
//! tidy: carry-over §10.6(a). `AppState` exists only once PostgreSQL is up,
//! and a `#[tauri::command]` resolves every argument *before* its body runs,
//! so a command declaring `State<'_, AppState>` is rejected by Tauri itself
//! during bring-up with the bare string `"state not managed"` -- no code for
//! the frontend to branch on. `Lifecycle` is managed at build time and is
//! always there; `lifecycle.pool()?` is the single place `not_ready` comes
//! from.

use tauri::State;

use crate::{IpcError, Lifecycle};

/// The `limit` most recent activity-log lines, newest first.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) while the
/// database is still coming up, [`Internal`](crate::IpcErrorCode::Internal)
/// for a query failure.
#[tauri::command]
pub async fn recent_activity(
    lifecycle: State<'_, Lifecycle>,
    limit: u32,
) -> Result<Vec<knobas_core::activity::ActivityRow>, IpcError> {
    let pool = lifecycle.pool()?;
    Ok(knobas_core::activity::recent(&pool, i64::from(limit)).await?)
}
