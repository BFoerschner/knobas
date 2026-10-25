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

pub struct TauriEvents<R: tauri::Runtime> {
    app: tauri::AppHandle<R>,
}

impl<R: tauri::Runtime> TauriEvents<R> {
    pub fn new(app: tauri::AppHandle<R>) -> TauriEvents<R> {
        TauriEvents { app }
    }

    fn emit<T: serde::Serialize>(&self, name: &'static str, payload: T) {
        crate::events::emit(&self.app, None, name, payload);
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
    fn refresh_ended(&self, ended: &knobas_sync::scheduler::RefreshEnded) {
        self.emit(crate::events::ITEM_REFRESHED, ended);
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

impl<R: tauri::Runtime> crate::checkout::clone::CloneEvents for TauriEvents<R> {
    fn clone_progress(&self, event: crate::checkout::clone::CloneEvent) {
        self.emit(crate::events::CLONE_PROGRESS, event);
    }
}

#[cfg(test)]
mod tests {
    use tauri::Listener;

    use super::*;

    /// A refresh's end reaches the shell as `item:refreshed`, naming the item,
    /// the write it followed, and how it ended (#2095), in the shape
    /// `RefreshEnded` is mirrored in.
    #[test]
    fn a_refresh_that_ended_reaches_the_shell_with_the_entity_and_the_write() {
        let app = tauri::test::mock_app();
        let heard = std::sync::Arc::new(std::sync::Mutex::new(Vec::<serde_json::Value>::new()));
        let recorder = std::sync::Arc::clone(&heard);
        app.listen(crate::events::ITEM_REFRESHED, move |event| {
            recorder
                .lock()
                .unwrap()
                .push(serde_json::from_str(event.payload()).expect("a JSON payload"));
        });

        TauriEvents::new(app.handle().clone()).refresh_ended(
            &knobas_sync::scheduler::RefreshEnded {
                entity_id: "jira:PAY-1".to_owned(),
                write_id: Some(7),
                ended: knobas_sync::scheduler::RefreshEnd::Gone,
            },
        );

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while heard.lock().unwrap().is_empty() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(
            *heard.lock().unwrap(),
            [serde_json::json!({ "entity_id": "jira:PAY-1", "write_id": 7, "ended": "gone" })]
        );
    }
}
