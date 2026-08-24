//! Entity and room reads -- stream D (interfaces §2.5).

use tauri::State;

use crate::{AppState, IpcError};

/// The `limit` most recent activity-log lines, newest first.
#[tauri::command]
pub async fn recent_activity(
    state: State<'_, AppState>,
    limit: u32,
) -> Result<Vec<knobas_core::activity::ActivityRow>, IpcError> {
    Ok(knobas_core::activity::recent(&state.pool, i64::from(limit)).await?)
}
