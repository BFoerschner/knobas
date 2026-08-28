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

use knobas_source::{Cursor, Sink, Source, SourceError, SyncItem};

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

/// Report at most this often, whatever the item rate.
const THROTTLE: std::time::Duration = std::time::Duration::from_millis(250);

/// …and at least this often by count, so a burst between two clock ticks does
/// not go entirely unreported on a fast local source.
const EVERY_N_ITEMS: u64 = 250;

/// An adapter that reports what it pushes.
///
/// **A decorator, not a hook in the engine.** Threading a progress callback
/// through `run_once` would put a reporting concern inside the engine's
/// transaction and make it everyone's to maintain. Wrapping the *adapter*
/// instead reaches exactly the same items, needs no change to `run_once` at
/// all, and costs nothing when nobody attached a channel -- the scheduler just
/// does not build one.
///
/// Borrows the real adapter rather than owning it: the registry hands out a
/// `Box<dyn Source>`, and moving it in would force every caller into the same
/// shape whether it wants progress or not.
pub struct Observed<'a> {
    inner: &'a dyn Source,
    run_id: i64,
    started: std::time::Instant,
    sink: std::sync::Arc<dyn ProgressSink>,
}

impl<'a> Observed<'a> {
    /// `started` is the *run's* start, not the decorator's, so `elapsed_ms`
    /// measures what the user has been waiting rather than when the wrapper
    /// happened to be built.
    #[must_use]
    pub fn new(
        inner: &'a dyn Source,
        run_id: i64,
        started: std::time::Instant,
        sink: std::sync::Arc<dyn ProgressSink>,
    ) -> Observed<'a> {
        Observed {
            inner,
            run_id,
            started,
            sink,
        }
    }
}

#[async_trait::async_trait]
impl Source for Observed<'_> {
    /// Verbatim. The engine builds its namespace and kind guards out of this,
    /// and reads each kind's `full_sync_exhaustive` off it to decide which
    /// kinds to sweep, so anything but delegation here changes what the run
    /// *does*.
    fn descriptor(&self) -> knobas_source::SourceDescriptor {
        self.inner.descriptor()
    }

    async fn test_connection(&self) -> Result<knobas_source::ConnectionInfo, SourceError> {
        self.inner.test_connection().await
    }

    async fn sync(
        &self,
        cursor: Option<Cursor>,
        sink: &mut (dyn Sink + Send),
    ) -> Result<Cursor, SourceError> {
        let source_id = self.inner.descriptor().id;
        let mut counting = Counting {
            inner: sink,
            // So the first accepted item reports at once: a wizard that
            // attached a channel should see the bar move, not wait 250 ms to
            // learn the run started.
            reported_at: std::time::Instant::now() - THROTTLE,
            seen: 0,
            run_id: self.run_id,
            source_id,
            started: self.started,
            sink: std::sync::Arc::clone(&self.sink),
        };
        let result = self.inner.sync(cursor, &mut counting).await;
        // One final `Fetching` either way, so the bar reaches the count the run
        // actually pushed instead of stopping at the last throttled sample --
        // and so a run that pushed nothing still says so.
        counting.emit();

        // **`Writing` here, and only on success, because here is where it
        // becomes true.** The adapter has returned, so nothing is left to
        // fetch; what the engine does next is flush the tail of the batch,
        // sweep, and commit. An earlier version sent this from the scheduler
        // *after* the whole run had finished, including on the failure path --
        // a phase that named work already over, or work that never happened.
        if result.is_ok() {
            counting.phase(SyncPhase::Writing);
        }
        result
    }

    async fn write(&self, op: knobas_source::WriteOp) -> Result<(), SourceError> {
        self.inner.write(op).await
    }
}

/// The sink the adapter is really handed.
struct Counting<'s> {
    inner: &'s mut (dyn Sink + Send),
    reported_at: std::time::Instant,
    seen: u64,
    run_id: i64,
    source_id: String,
    started: std::time::Instant,
    sink: std::sync::Arc<dyn ProgressSink>,
}

impl Counting<'_> {
    fn emit(&mut self) {
        self.reported_at = std::time::Instant::now();
        self.phase(SyncPhase::Fetching);
    }

    /// Report `phase` with the count so far.
    fn phase(&self, phase: SyncPhase) {
        self.sink.report(SyncProgress {
            run_id: self.run_id,
            source_id: self.source_id.clone(),
            phase,
            items: self.seen,
            elapsed_ms: u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX),
            message: None,
        });
    }
}

#[async_trait::async_trait]
impl Sink for Counting<'_> {
    async fn item(&mut self, item: SyncItem) -> Result<(), SourceError> {
        // The real sink first, and the count only on success: an item is
        // "seen" once it has been accepted, so a rejected item never inflates
        // the number the wizard shows.
        self.inner.item(item).await?;
        self.seen += 1;
        if self.seen.is_multiple_of(EVERY_N_ITEMS) || self.reported_at.elapsed() >= THROTTLE {
            self.emit();
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The phases are one list in two languages; the mirror is
    /// `app/src/lib/ipc/sources.ts`. A rename on one side is a progress bar
    /// that silently never advances.
    ///
    /// This is the **spelling** half only. That a phase is *ever emitted* is a
    /// different claim and a stronger one -- a variant declared on both sides
    /// and sent by nobody invites a frontend to wait for a state that never
    /// arrives -- and it is asserted by
    /// `knobas-sync/tests/scheduler_run.rs::every_declared_phase_is_actually_emitted`,
    /// which drives real runs and collects what comes out.
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

    /// The union the emission test reads is the one the mirror declares.
    #[test]
    fn the_declared_union_is_read_out_of_the_mirror() {
        assert_eq!(
            crate::mirror::declared_union(
                include_str!("../../../app/src/lib/ipc/sources.ts"),
                "SyncPhase"
            ),
            ["started", "fetching", "writing", "finished", "failed"],
            "the union moved or changed shape; the emission test reads it"
        );
    }
}
