//! The launcher's names read again when the mirror changed (the fuzzy-search
//! decisions, 2026-11-19, D3;
//! `docs/decisions/2026-11-19-names-are-found-as-a-picker-finds-them.md`).
//!
//! [`knobas_search::picker::Picker`] holds every name of the mirror. It is
//! read whole at bring-up and again after every sync run that ended OK and
//! did not say it wrote nothing, after a write that landed, and after a
//! single item's refresh: about 40 ms behind the app, never on a key. A
//! request while a reload runs is kept and runs once after it (the picker's
//! own rule). Whole reloads, no incremental path.

use sqlx::PgPool;

use knobas_core::activity::ActivityRow;
use knobas_search::picker::Picker;

/// Read every name into `picker` in the background, and log what was read.
pub fn reload(picker: Picker, pool: PgPool) {
    tokio::spawn(async move {
        match picker.reload(&pool).await {
            Ok(Some(loaded)) => tracing::info!(
                names = loaded.names,
                words = loaded.words,
                ms = loaded.millis,
                "the launcher's names are read"
            ),
            // Another reload was running, and reads again for this one.
            Ok(None) => {}
            Err(error) => tracing::warn!(%error, "the launcher's names could not be read"),
        }
    });
}

/// The scheduler's events, and the names read again where the mirror
/// changed, as [`super::state_colours::CountOnSync`] counts its words. Every
/// event is passed on unchanged.
pub struct PickOnSync<E> {
    inner: E,
    picker: Picker,
    pool: PgPool,
}

impl<E> PickOnSync<E> {
    /// `inner`'s events, and reloads of `picker` over `pool`.
    pub fn new(inner: E, picker: Picker, pool: PgPool) -> PickOnSync<E> {
        PickOnSync {
            inner,
            picker,
            pool,
        }
    }

    fn reload(&self) {
        reload(self.picker.clone(), self.pool.clone());
    }
}

impl<E: knobas_sync::scheduler::SyncEvents> knobas_sync::scheduler::SyncEvents for PickOnSync<E> {
    fn sync_state(&self, status: knobas_sync::SourceSyncStatus) {
        if !status.running
            && status.last_outcome == Some(knobas_sync::SyncOutcome::Ok)
            && status.last_changed != Some(false)
        {
            self.reload();
        }
        self.inner.sync_state(status);
    }
    fn source_health(&self, health: knobas_sync::CredentialHealth) {
        self.inner.source_health(health);
    }
    fn activity_new(&self, row: ActivityRow) {
        self.inner.activity_new(row);
    }
    fn write_landed(&self, write: &knobas_core::write_queue::QueuedWrite) {
        self.reload();
        self.inner.write_landed(write);
    }
    fn refresh_ended(&self, ended: &knobas_sync::scheduler::RefreshEnded) {
        self.reload();
        self.inner.refresh_ended(ended);
    }
    // The files pass's index and progress, passed on wherever this wrapper
    // sits in the stack (`tests/it/files_on_sync.rs`).
    fn files(&self) -> Option<knobas_sync::code_files::Files> {
        self.inner.files()
    }
    fn code_files(&self, progress: &knobas_sync::code_files::FilesProgress) {
        self.inner.code_files(progress);
    }
}
