//! The files pass's place in the app (the everything-search decisions,
//! 2026-11-20, D7): the outermost of the scheduler's events, it hands the
//! scheduler the files index and the checkouts' reader the pass writes
//! through after each run, and tells the window `code:files` as the pass
//! goes. Every other event passes through to `inner`.

use knobas_core::activity::ActivityRow;
use knobas_sync::code_files::{Files, FilesProgress};
use knobas_sync::scheduler::SyncEvents;

/// `inner`'s events, and the files pass's.
pub struct FilesOnSync<E, F> {
    inner: E,
    files: Option<Files>,
    told: F,
}

impl<E, F: Fn(&FilesProgress) + Send + Sync + 'static> FilesOnSync<E, F> {
    /// `inner`'s events; the pass writes through `files` where it is given,
    /// and its progress goes to `told`.
    pub fn new(inner: E, files: Option<Files>, told: F) -> Self {
        Self { inner, files, told }
    }
}

impl<E: SyncEvents, F: Fn(&FilesProgress) + Send + Sync + 'static> SyncEvents
    for FilesOnSync<E, F>
{
    fn sync_state(&self, status: knobas_sync::SourceSyncStatus) {
        self.inner.sync_state(status);
    }
    fn source_health(&self, health: knobas_sync::CredentialHealth) {
        self.inner.source_health(health);
    }
    fn activity_new(&self, row: ActivityRow) {
        self.inner.activity_new(row);
    }
    fn write_landed(&self, write: &knobas_core::write_queue::QueuedWrite) {
        self.inner.write_landed(write);
    }
    fn refresh_ended(&self, ended: &knobas_sync::scheduler::RefreshEnded) {
        self.inner.refresh_ended(ended);
    }
    fn as_any(&self) -> Option<&dyn std::any::Any> {
        self.inner.as_any()
    }
    fn files(&self) -> Option<Files> {
        self.files.clone()
    }
    fn code_files(&self, progress: &FilesProgress) {
        (self.told)(progress);
    }
}
