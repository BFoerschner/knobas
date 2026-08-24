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

/// The channel `sync_now_with_progress` reports on.
///
/// A local wrapper because the orphan rule forbids implementing
/// `knobas_sync::ProgressSink` for `tauri::ipc::Channel` directly -- neither is
/// this crate's type.
struct ChannelProgress(tauri::ipc::Channel<knobas_sync::SyncProgress>);

impl knobas_sync::ProgressSink for ChannelProgress {
    fn report(&self, progress: knobas_sync::SyncProgress) {
        // A webview that stopped listening is not a sync failure: the run is
        // the point, the progress bar is not.
        if let Err(error) = self.0.send(progress) {
            tracing::debug!(%error, "dropping a progress message: nobody is listening");
        }
    }
}

/// Start a sync of one configured source and return its `sync_run.id`.
///
/// Returns as soon as the run is recorded rather than when it finishes
/// (ruling P3): a UI must never wait on a source. What the run did is read
/// back from `knobas.sync_run`; while it is in flight, `sync:state` says so.
///
/// M0 has exactly one adapter, `"mock"`; anything else is refused rather than
/// silently doing nothing.
///
/// # Why there are two of these
///
/// P3 asked for one command with an omittable `Option<Channel<SyncProgress>>`.
/// Tauri 2.11 cannot express that: `Channel` implements `Serialize` and its own
/// `CommandArg` but no `Deserialize`, and the only route from
/// `Option<Channel<_>>` to `CommandArg` is the blanket impl over
/// `Deserialize`, so the wrapped form does not compile. P3's documented
/// fallback is therefore in force -- this command for callers that want no
/// per-item progress, [`sync_now_with_progress`] for the ones that do. The
/// evidence is pinned in `crates/knobas-app/tests/ipc.rs`.
#[tauri::command]
pub async fn sync_now(state: State<'_, AppState>, source_id: String) -> Result<i64, IpcError> {
    Ok(crate::demo::sync_now_inner(&state.pool, &source_id, None).await?)
}

/// [`sync_now`], reporting per-item progress on `progress`.
///
/// Same return value and same semantics; the channel is the only difference,
/// and it is **required** -- see [`sync_now`] for why the two are separate
/// commands. Ruling P3 and roadmap §4: per-item progress goes on the channel
/// and nowhere else, so a caller that only wants to know a run started should
/// call [`sync_now`] and listen to `sync:state` instead of opening a channel
/// it will not read.
#[tauri::command]
pub async fn sync_now_with_progress(
    state: State<'_, AppState>,
    source_id: String,
    progress: tauri::ipc::Channel<knobas_sync::SyncProgress>,
) -> Result<i64, IpcError> {
    let sink = ChannelProgress(progress);
    Ok(crate::demo::sync_now_inner(&state.pool, &source_id, Some(&sink)).await?)
}
