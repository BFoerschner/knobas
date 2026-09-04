//! The scheduler's events, on the Tauri bridge.
//!
//! `knobas-sync` knows nothing about Tauri; this is the whole adapter between
//! the two. Emission is best-effort: a failed `emit` means no window is
//! listening, which is not a reason to fail a sync that already landed.
//!
//! The notifier's click (#339) goes through the same adapter under its own
//! trait, `notify::NotificationEvents`, rather than through `SyncEvents`: the
//! scheduler's trait is `knobas-sync`'s and a desktop notification is not a
//! sync fact. What is shared is the bridge, and the reason for a trait at all
//! -- a test observes the emit without Tauri.

use knobas_core::activity::ActivityRow;
use knobas_sync::config::CredentialHealth;
use knobas_sync::scheduler::{SourceSyncStatus, SyncEvents};
use tauri::Emitter;

pub struct TauriEvents<R: tauri::Runtime> {
    app: tauri::AppHandle<R>,
}

impl<R: tauri::Runtime> TauriEvents<R> {
    pub fn new(app: tauri::AppHandle<R>) -> TauriEvents<R> {
        TauriEvents { app }
    }

    fn emit<T: serde::Serialize + Clone>(&self, name: &str, payload: T) {
        if let Err(error) = self.app.emit(name, payload) {
            tracing::debug!(event = name, %error, "nothing was listening for this event");
        }
    }
}

impl<R: tauri::Runtime> SyncEvents for TauriEvents<R> {
    fn sync_state(&self, status: SourceSyncStatus) {
        self.emit(crate::events::SYNC_STATE, status);
    }
    fn source_health(&self, health: CredentialHealth) {
        self.emit(crate::events::SOURCE_HEALTH, health);
    }
    fn activity_new(&self, row: ActivityRow) {
        self.emit(crate::events::ACTIVITY_NEW, row);
    }
}

impl<R: tauri::Runtime> crate::notify::NotificationEvents for TauriEvents<R> {
    fn notification_clicked(&self, address: String) {
        self.emit(
            crate::events::NOTIFICATION_CLICKED,
            crate::notify::NotificationClicked { address },
        );
    }
}
