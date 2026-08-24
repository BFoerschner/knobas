//! Per-item sync progress.
//!
//! Ruling P3 and roadmap §4: **events carry coarse state, at most a handful
//! per run; per-item progress goes on a `tauri::ipc::Channel` and nowhere
//! else.** A scheduled run has no channel and emits `sync:state` only; the
//! first-run wizard attaches one because it draws a progress bar.
//!
//! The transport is a trait rather than the Tauri type so the engine stays
//! free of the app shell (and testable without a webview); `knobas-app` wraps
//! the channel in a local type that implements it.
//!
//! **The channel is not optional on the bridge.** `Option<Channel<_>>` is not
//! a `#[tauri::command]` argument in Tauri 2.11 -- `Channel` has no
//! `Deserialize` impl, so the `Option` has no route to `CommandArg` -- which
//! is why the command is split into `sync_now` and `sync_now_with_progress`
//! rather than taking an omittable channel. The verdict and its evidence are
//! pinned in `crates/knobas-app/tests/ipc.rs`. On *this* side of the bridge
//! the sink is an ordinary `Option<&dyn ProgressSink>`, so the engine has one
//! code path either way.

/// One progress message.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SyncProgress {
    /// `knobas.sync_run.id` -- on every message, so a UI watching two runs can
    /// tell them apart.
    pub run_id: i64,
    pub source_id: String,
    pub phase: SyncPhase,
    /// Items pushed so far this run.
    pub items: u64,
    pub elapsed_ms: u64,
    pub message: Option<String>,
}

/// Where a run is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncPhase {
    Started,
    /// Waiting on the remote system.
    Fetching,
    /// Writing a batch into Postgres.
    Writing,
    Finished,
    Failed,
}

/// Where a run reports itself, when anyone is listening.
pub trait ProgressSink: Send + Sync {
    /// Report one message. Implementations must not block or fail the run: a
    /// webview that stopped listening is not a sync error.
    fn report(&self, progress: SyncProgress);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The phases are one list in two languages; the mirror is
    /// `app/src/lib/ipc/sources.ts`. A rename on one side is a progress bar
    /// that silently never advances.
    #[test]
    fn the_phase_names_match_their_typescript_mirror() {
        let mirror = include_str!("../../../app/src/lib/ipc/sources.ts");
        for phase in [
            SyncPhase::Started,
            SyncPhase::Fetching,
            SyncPhase::Writing,
            SyncPhase::Finished,
            SyncPhase::Failed,
        ] {
            let wire = serde_json::to_string(&phase).unwrap();
            assert!(
                mirror.contains(&wire),
                "{wire} is missing from app/src/lib/ipc/sources.ts"
            );
        }
    }
}
