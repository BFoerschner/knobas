//! Per-item sync progress.
//!
//! Ruling P3 and roadmap §4: **events carry coarse state, at most a handful
//! per run; per-item progress goes on a `tauri::ipc::Channel` and nowhere
//! else.** A scheduled run emits `sync:state` and nothing per item unless
//! somebody is watching; the first-run wizard watches because it draws a
//! progress bar.
//!
//! Every run holds a [`Watchers`] regardless, and that is ADR-0005: a caller
//! can be handed the id of a run the scheduler started on its own -- the wake
//! after *Add source* starts exactly that run -- and a run whose sinks were
//! decided when it began would have nowhere to put that caller. So the set
//! exists from the start and is usually empty, which costs one message built
//! per throttle tick and delivered to nobody.
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
//! pinned in `crates/knobas-app/tests/it/ipc.rs`. On *this* side of the bridge
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
    /// webview that stopped listening is not a sync error -- which is why this
    /// returns nothing rather than a `Result`. `knobas-app`'s `ChannelSink`
    /// logs a closed channel and swallows it, so a listener that has gone away
    /// is discarded where it is noticed and [`Watchers`] never hears of it at
    /// all.
    ///
    /// An implementation that **panics** has broken this contract, and this
    /// crate catches it rather than letting it stand: every delivery it makes
    /// to a sink goes through [`deliver`], the two [`Watchers`] fan-outs and
    /// the scheduler's synthesised ending alike. That is containment, not
    /// permission -- a panicking sink still loses the message it was being
    /// handed.
    fn report(&self, progress: SyncProgress);
}

/// The sinks watching one run, and the promise that each of them is told how it
/// ended.
///
/// **ADR-0005 -- *a run id always comes with an ending*.** A run holds a *set*
/// of sinks rather than one, so a caller handed the id of a run already in
/// flight is enrolled beside whoever started it instead of displacing them or
/// being dropped. The scheduler hands one of these to every run and reports
/// through it, which is why the invariant is a property of the type rather than
/// of each call site: there is one place a sink can be added and one place the
/// ending is sent, and after the second the first refuses.
///
/// The whole state machine is two states and the transition is one way:
///
/// - **open** -- the run is going. `attach` enrols; `report` fans out.
/// - **closed** -- [`close`](Self::close) has fanned the ending out to
///   everyone enrolled at that moment, and nothing more will ever be sent
///   through here. `attach` refuses with [`Attach::RunHadEnded`], which is the
///   scheduler's cue to serve that caller an ending built from the run's log
///   row instead.
///
/// The refusal is what makes the invariant structural. A sink enrolled a
/// microsecond before the ending still hears it, because both go through the
/// same lock; a sink that arrives a microsecond after is told so, rather than
/// being enrolled into a run that will never speak again.
///
/// A `std` mutex and not tokio's: [`ProgressSink::report`] is synchronous and
/// is called from inside the adapter's own stack, so this is never held across
/// an await. It is recovered from poisoning rather than unwrapped -- the *run*
/// must not lose its ending over a broken caller. A panicking sink can no
/// longer poison it in the first place, because [`deliver`] catches the panic
/// at the sink; the recovery stays as the backstop for a panic anywhere else
/// under this lock, which is the case nobody has enumerated.
///
/// **One sink's misbehaviour costs no other sink anything.** Both fan-outs go
/// through [`deliver`], so a sink that panics on its turn loses only its own
/// message: the loop carries on, and everyone enrolled after it still hears
/// the run out. Without that, the ending -- the one message ADR-0005 is a
/// promise about -- was lost by an arbitrary *suffix* of the watchers,
/// whichever ones happened to be enrolled behind the broken one.
pub struct Watchers {
    run_id: i64,
    state: std::sync::Mutex<State>,
}

enum State {
    /// The run is going; these hear everything it has left to say.
    Open(Vec<std::sync::Arc<dyn ProgressSink>>),
    /// The ending has been sent. Nothing is enrolled or reported again.
    Closed,
}

/// What [`Watchers::attach`] found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Attach {
    /// The run was still going. A sink, if one was offered, is enrolled and
    /// will hear the run's remaining progress and its ending.
    Joined,
    /// The run had already ended, so nothing was enrolled -- and whoever holds
    /// the sink now owes it an ending built from the run's record.
    RunHadEnded,
}

/// Hand one message to one sink, whatever that sink does with its turn.
///
/// [`ProgressSink::report`] returns nothing precisely so that a listener which
/// has gone away cannot fail a run. A sink that *panics* defeats that by
/// another route and takes two things with it that are not its to spend: every
/// sink enrolled after it in the fan-out, and -- when the fan-out is
/// [`Watchers::close`] -- the ending ADR-0005 promises them. So the panic is
/// contained at the sink that raised it and the loop carries on.
///
/// **Every delivery this crate makes to a sink comes through here**, and that
/// is the point of it being a free function rather than a method: the two
/// [`Watchers`] fan-outs are not the only ones. `Scheduler`'s served-from-record
/// path hands a lone caller its synthesised ending directly, in the caller's own
/// stack and under the scheduler's `runs` lock, so a panic there unwound out of
/// `trigger` and into the command that called it. That is ADR-0005's own
/// delivery, and it should not be the one place a broken sink is uncontained.
///
/// Not a licence to panic: the sink that did loses the message it was handed,
/// and says so in the log. It is the difference between one broken caller
/// hearing nothing and every caller behind it hearing nothing.
///
/// `AssertUnwindSafe` because nothing this crate owns can be left half-updated
/// by the unwind: the closure touches no state of [`Watchers`] or the
/// scheduler, and the message was already cloned for this sink. What the panic
/// *may* leave inconsistent is the sink's own interior, and a sink that panicked
/// stays enrolled and is handed later messages -- deliberately, because nothing
/// here can tell a sink that is broken from one that threw once, and dropping a
/// caller's channel over a single `report` would cost it every message after,
/// its ending included.
pub(crate) fn deliver(run_id: i64, sink: &dyn ProgressSink, progress: SyncProgress) {
    let delivered =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| sink.report(progress)));
    if delivered.is_err() {
        tracing::warn!(
            run_id,
            "a progress sink panicked; its message is lost and the run carries on"
        );
    }
}

impl Watchers {
    #[must_use]
    pub fn for_run(run_id: i64) -> std::sync::Arc<Watchers> {
        std::sync::Arc::new(Watchers {
            run_id,
            state: std::sync::Mutex::new(State::Open(Vec::new())),
        })
    }

    #[must_use]
    pub fn run_id(&self) -> i64 {
        self.run_id
    }

    /// Enrol a sink, and say whether there was still a run to enrol it in.
    ///
    /// `None` enrols nobody and only asks the question -- which is what a
    /// caller with no channel of its own (the ticker, *Sync now* without a
    /// progress bar) needs, and why this is one call and not a `is_open()`
    /// followed by an `attach()`: between those two the run could end, and the
    /// sink would be enrolled into a run that has already said its last word.
    pub fn attach(&self, sink: Option<std::sync::Arc<dyn ProgressSink>>) -> Attach {
        match &mut *self.lock() {
            State::Open(sinks) => {
                if let Some(sink) = sink {
                    sinks.push(sink);
                }
                Attach::Joined
            }
            State::Closed => Attach::RunHadEnded,
        }
    }

    /// Send the run's ending to everyone enrolled, and enrol nobody after.
    ///
    /// Idempotent, and the first call wins: the run closes its own watchers
    /// when it settles, and the scheduler's guard closes them again if the task
    /// died before reaching that -- the second call must not overwrite a true
    /// ending with a fallback one.
    ///
    /// **Every sink is delivered to, whatever the ones before it did.** This is
    /// where ADR-0005's promise is actually kept, so it is the one loop that
    /// must not be abandoned half-way: the state is already `Closed` by the
    /// time the fan-out starts, so a sink that panicked out of here would leave
    /// the rest of the vector with no ending and nothing left that could ever
    /// send them one. Containment per sink ([`deliver`]) rather than a
    /// re-ordered state transition, because the ordering is not the defect: an
    /// escaping panic skips the remaining sinks whatever the state says, and
    /// closing *after* the fan-out would leave a run whose watchers stayed
    /// `Open` for ever -- the source never released, which is the failure
    /// ADR-0005 exists to remove, arriving through a third door.
    ///
    /// Never panics, and that is load-bearing: the scheduler closes a run's
    /// watchers from a `Drop` guard, so on the path where the run's task
    /// panicked this runs *while already unwinding*, where a second panic
    /// escaping a drop aborts the process.
    pub fn close(&self, ending: SyncProgress) {
        let mut state = self.lock();
        let State::Open(sinks) = std::mem::replace(&mut *state, State::Closed) else {
            return;
        };
        // Still under the lock, so the ending really is the last thing every
        // one of these hears: a `report` racing this one either ran before the
        // swap or finds `Closed` and drops its message.
        for sink in sinks {
            deliver(self.run_id, sink.as_ref(), ending.clone());
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

impl ProgressSink for Watchers {
    /// Fan one message out. Nothing at all once the run has ended, and nothing
    /// at all when nobody is watching -- an unwatched run costs one `Vec` scan
    /// per throttled message and no allocation beyond the message itself.
    fn report(&self, progress: SyncProgress) {
        let State::Open(sinks) = &*self.lock() else {
            return;
        };
        for sink in sinks {
            deliver(self.run_id, sink.as_ref(), progress.clone());
        }
    }
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

    async fn write(
        &self,
        op: knobas_source::WriteOp,
    ) -> Result<knobas_source::WriteReceipt, SourceError> {
        self.inner.write(op).await
    }

    /// Delegated, and deliberately unreported: this decorator narrates a *sync
    /// run* -- a run id, a phase, a growing item count -- and the workflow read
    /// belongs to a detail somebody opened, which has no run and no progress
    /// bar to move.
    async fn reachable_transitions(&self, entity: &str) -> Result<Vec<String>, SourceError> {
        self.inner.reachable_transitions(entity).await
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
    /// `knobas-sync/tests/it/scheduler_run.rs::every_declared_phase_is_actually_emitted`,
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
