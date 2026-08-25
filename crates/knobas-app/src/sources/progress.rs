//! `tauri::ipc::Channel<SyncProgress>` as a `knobas_sync::progress::ProgressSink`.
//!
//! The whole reason `knobas-sync` defines a trait instead of taking a Channel:
//! the engine has no business linking Tauri, and a test needs to be able to
//! record progress without a webview.

use knobas_sync::progress::{ProgressSink, SyncProgress};

pub struct ChannelSink {
    channel: tauri::ipc::Channel<SyncProgress>,
}

impl ChannelSink {
    #[must_use]
    pub fn new(channel: tauri::ipc::Channel<SyncProgress>) -> ChannelSink {
        ChannelSink { channel }
    }
}

impl ProgressSink for ChannelSink {
    fn report(&self, progress: SyncProgress) {
        // Best-effort: a closed channel means the window that asked for
        // progress is gone, which must never fail the sync it is watching.
        if let Err(error) = self.channel.send(progress) {
            tracing::debug!(%error, "the progress channel is closed");
        }
    }
}
