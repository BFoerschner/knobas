//! Sources, secrets, sync and diagnostics -- stream F (interfaces §2.2, §2.3).

use tauri::State;

use crate::{AppState, IpcError};

/// Register the demo source if absent, then sync it in full.
///
/// Safe to call repeatedly: see [`crate::demo::demo_load_inner`], which owns
/// the behaviour and the test for it.
#[tauri::command]
pub async fn demo_load(state: State<'_, AppState>) -> Result<knobas_sync::SyncReport, IpcError> {
    Ok(crate::demo::demo_load_inner(&state.pool).await?)
}

/// Sync one configured source from where it last stopped.
///
/// M0 has exactly one adapter, `"mock"`; anything else is refused rather than
/// silently doing nothing.
#[tauri::command]
pub async fn sync_now(
    state: State<'_, AppState>,
    source_id: String,
) -> Result<knobas_sync::SyncReport, IpcError> {
    Ok(crate::demo::sync_now_inner(&state.pool, &source_id).await?)
}
