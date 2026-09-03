//! Progress is throttled on purpose: a 40,000-item Jira sync that reported per
//! item would push 40,000 messages across the IPC bridge for a progress bar
//! that can show at most a few hundred distinct states.

use std::sync::{Arc, Mutex};
use std::time::Instant;

use knobas_source::{
    Capability, ConnectionInfo, Cursor, KindInfo, Sink, Source, SourceDescriptor, SourceError,
    SyncItem, WriteOp,
};
use knobas_source_mock::MockSource;
use knobas_sync::progress::{Observed, ProgressSink, SyncPhase, SyncProgress, Watchers};

#[derive(Default)]
struct Recorder(Mutex<Vec<SyncProgress>>);

impl Recorder {
    fn seen(&self) -> Vec<SyncProgress> {
        self.0.lock().unwrap().clone()
    }
}

impl ProgressSink for Recorder {
    fn report(&self, progress: SyncProgress) {
        self.0.lock().unwrap().push(progress);
    }
}

/// A sink that counts what actually reached it, so the decorator can be proven
/// not to drop or duplicate an item.
#[derive(Default)]
struct Counting(Mutex<Vec<String>>);

#[async_trait::async_trait]
impl Sink for Counting {
    async fn item(&mut self, item: SyncItem) -> Result<(), SourceError> {
        self.0.lock().unwrap().push(item.entity.to_string());
        Ok(())
    }
}

/// A sink that refuses everything, the way `PgSink` refuses an item outside the
/// source's namespace.
#[derive(Default)]
struct Refusing(Mutex<usize>);

#[async_trait::async_trait]
impl Sink for Refusing {
    async fn item(&mut self, _item: SyncItem) -> Result<(), SourceError> {
        *self.0.lock().unwrap() += 1;
        Err(SourceError::Sink("refused".into()))
    }
}

/// An adapter that pushes `n` items as fast as it can.
struct Flood(usize);

#[async_trait::async_trait]
impl Source for Flood {
    fn descriptor(&self) -> SourceDescriptor {
        SourceDescriptor {
            id: "flood".into(),
            adapter_kind: "flood".into(),
            name: "Flood".into(),
            capabilities: Vec::<Capability>::new(),
            adapter_version: "0.1.0".into(),
            auth_methods: Vec::new(),
            write_ops: Vec::new(),
            entity_kinds: vec![KindInfo {
                id: "ticket".into(),
                label: "T".into(),
                plural: "T".into(),
                monogram: "FL".into(),
                full_sync_exhaustive: true,
            }],
            config_schema: serde_json::json!({ "type": "object", "properties": {} }),
            // Nothing declared: this stand-in has no payload shapes to
            // read, so every path-driven read misses on it (#277).
            payload_paths: Vec::new(),
        }
    }
    async fn test_connection(&self) -> Result<ConnectionInfo, SourceError> {
        Ok(ConnectionInfo::default())
    }
    async fn sync(
        &self,
        _c: Option<Cursor>,
        sink: &mut (dyn Sink + Send),
    ) -> Result<Cursor, SourceError> {
        for n in 0..self.0 {
            sink.item(SyncItem {
                entity: knobas_core::entity::EntityRef::new("flood", &format!("F-{n}")),
                kind: "ticket".into(),
                title: String::new(),
                body_text: String::new(),
                author: None,
                updated_at: None,
                payload: serde_json::json!({}),
                web_url: None,
                deleted: false,
            })
            .await?;
        }
        Ok("done".into())
    }
    async fn write(&self, _op: WriteOp) -> Result<(), SourceError> {
        Err(SourceError::protocol("read-only"))
    }
}

#[tokio::test]
async fn the_decorator_forwards_every_item_untouched_and_returns_the_adapters_cursor() {
    let inner = MockSource::new();
    let recorder: Arc<dyn ProgressSink> = Arc::new(Recorder::default());
    let observed = Observed::new(&inner, 7, Instant::now(), Arc::clone(&recorder));

    let mut direct = Counting::default();
    let direct_cursor: Cursor = inner.sync(None, &mut direct).await.unwrap();

    let mut through = Counting::default();
    let through_cursor: Cursor = observed.sync(None, &mut through).await.unwrap();

    assert_eq!(
        through.0.lock().unwrap().clone(),
        direct.0.lock().unwrap().clone()
    );
    assert_eq!(through_cursor, direct_cursor);
    // And it is transparent in the other direction too: the engine reads the
    // descriptor off it to build the namespace guard, so a decorator that
    // reported a different id would make every item fail the guard.
    assert_eq!(observed.descriptor().id, inner.descriptor().id);
    let sweep_gate = |d: knobas_source::SourceDescriptor| {
        d.entity_kinds
            .into_iter()
            .map(|k| (k.id, k.full_sync_exhaustive))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        sweep_gate(observed.descriptor()),
        sweep_gate(inner.descriptor()),
        "the sweep gate is read off the decorated descriptor"
    );
}

#[tokio::test]
async fn progress_is_throttled_rather_than_one_message_per_item() {
    let recorder = Arc::new(Recorder::default());
    let sink: Arc<dyn ProgressSink> = recorder.clone();
    let inner = Flood(5_000);
    let observed = Observed::new(&inner, 9, Instant::now(), sink);

    let mut counting = Counting::default();
    observed.sync(None, &mut counting).await.unwrap();

    let reports = recorder.seen();
    assert_eq!(
        counting.0.lock().unwrap().len(),
        5_000,
        "every item still lands"
    );
    assert!(
        reports.len() < 100,
        "5000 items must not produce 5000 messages, got {}",
        reports.len()
    );
    assert!(!reports.is_empty(), "but some progress must be reported");
    assert!(
        reports.iter().all(|r| r.run_id == 9),
        "every message names its run"
    );
    // Every message but the last is `Fetching`; the last is `Writing`, sent
    // once the adapter has returned and the only work left is the engine's
    // flush, sweep and commit. It is the one place that phase is true.
    let (last, fetching) = reports.split_last().expect("some progress");
    assert!(
        fetching.iter().all(|r| r.phase == SyncPhase::Fetching),
        "the fetch phase is what a decorated adapter reports while it fetches"
    );
    assert_eq!(last.phase, SyncPhase::Writing);
    // Monotonic: a progress bar that goes backwards is a bug report.
    assert!(reports.windows(2).all(|w| w[0].items <= w[1].items));
    assert_eq!(reports.last().unwrap().source_id, "flood");
    // The last message carries the run's true total, so the bar reaches the
    // end instead of stopping at the last throttled sample.
    assert_eq!(
        reports.last().unwrap().items,
        5_000,
        "the final message reports what the run actually pushed"
    );
}

/// A run that pushed nothing still reports once, or a wizard that attached a
/// channel sees no message at all and cannot tell "empty" from "hung".
#[tokio::test]
async fn a_run_that_pushed_nothing_still_reports_once() {
    let recorder = Arc::new(Recorder::default());
    let sink: Arc<dyn ProgressSink> = recorder.clone();
    let inner = Flood(0);
    let observed = Observed::new(&inner, 3, Instant::now(), sink);

    let mut counting = Counting::default();
    observed.sync(None, &mut counting).await.unwrap();

    let reports = recorder.seen();
    assert_eq!(
        reports.iter().map(|r| r.phase).collect::<Vec<_>>(),
        [SyncPhase::Fetching, SyncPhase::Writing],
        "the final count, then the handover to the engine's writes"
    );
    assert!(reports.iter().all(|r| r.items == 0 && r.run_id == 3));
}

/// An item the sink refused is not an item the run pushed. Counting it would
/// make the wizard's bar claim progress over data that never landed.
#[tokio::test]
async fn a_refused_item_does_not_count_as_progress() {
    let recorder = Arc::new(Recorder::default());
    let sink: Arc<dyn ProgressSink> = recorder.clone();
    let inner = Flood(10);
    let observed = Observed::new(&inner, 4, Instant::now(), sink);

    let mut refusing = Refusing::default();
    let error = observed
        .sync(None, &mut refusing)
        .await
        .expect_err("the sink refused");
    assert!(matches!(error, SourceError::Sink(_)), "{error:?}");
    assert_eq!(
        *refusing.0.lock().unwrap(),
        1,
        "the adapter abandons the sync on the first refusal"
    );

    let reports = recorder.seen();
    assert!(
        reports.iter().all(|r| r.items == 0),
        "a refused item must not inflate the count: {:?}",
        reports.iter().map(|r| r.items).collect::<Vec<_>>()
    );
}

#[test]
fn the_phases_cross_the_bridge_as_snake_case() {
    for (phase, name) in [
        (SyncPhase::Started, "started"),
        (SyncPhase::Fetching, "fetching"),
        (SyncPhase::Writing, "writing"),
        (SyncPhase::Finished, "finished"),
        (SyncPhase::Failed, "failed"),
    ] {
        assert_eq!(
            serde_json::to_value(phase).unwrap(),
            serde_json::json!(name)
        );
    }
}

// -- the fan-out, and what one misbehaving sink may cost the others -----------

/// A sink that blows up where a bug in a caller's `report` would.
///
/// `ProgressSink`'s documented contract already forbids this -- the trait
/// returns nothing so that a listener which has gone away cannot fail a run --
/// so a sink like this is a *broken caller*. The point of the tests below is
/// that a broken caller costs nobody else anything: "the caller broke its
/// contract" is a poor reason for another caller to lose its ending, and a
/// worse one for the process to abort.
struct Boom;

impl ProgressSink for Boom {
    fn report(&self, _progress: SyncProgress) {
        panic!("a sink that panics on purpose");
    }
}

fn a_message(phase: SyncPhase) -> SyncProgress {
    SyncProgress {
        run_id: 7,
        source_id: "jira".into(),
        phase,
        items: 3,
        elapsed_ms: 12,
        message: None,
    }
}

/// **ADR-0005's fan-out is complete, whatever one sink does with its turn.**
///
/// The ending is where the ADR's promise -- *any caller that receives a run id
/// receives an ending for it* -- is actually kept, so a sink that panics
/// half-way through the fan-out must not take the rest of the vector with it.
/// The bad sink is enrolled **first**, which is the only order in which the
/// question is asked at all.
#[test]
fn the_ending_reaches_every_sink_enrolled_after_one_that_panics() {
    let watchers = Watchers::for_run(7);
    watchers.attach(Some(Arc::new(Boom) as Arc<dyn ProgressSink>));
    let live = Arc::new(Recorder::default());
    watchers.attach(Some(Arc::clone(&live) as Arc<dyn ProgressSink>));

    watchers.close(a_message(SyncPhase::Finished));

    let seen = live.seen();
    assert_eq!(
        seen.len(),
        1,
        "the sink enrolled after the panicking one lost its ending: {seen:?}"
    );
    assert_eq!(seen[0].phase, SyncPhase::Finished);
    assert_eq!(seen[0].run_id, 7);
}

/// The same, for the run's ordinary progress rather than its ending.
///
/// `Watchers::report` is called from inside the adapter's own stack, so a panic
/// escaping it does not merely cost the later sinks their message: it unwinds
/// through the run and kills it. That is the one thing `ProgressSink`'s
/// `-> ()` signature exists to make impossible.
#[test]
fn a_progress_message_reaches_every_sink_enrolled_after_one_that_panics() {
    let watchers = Watchers::for_run(7);
    watchers.attach(Some(Arc::new(Boom) as Arc<dyn ProgressSink>));
    let live = Arc::new(Recorder::default());
    watchers.attach(Some(Arc::clone(&live) as Arc<dyn ProgressSink>));

    watchers.report(a_message(SyncPhase::Fetching));

    assert_eq!(
        live.seen().len(),
        1,
        "one panicking sink silenced the run for everyone behind it"
    );
}

/// **A panicking sink during an unwind does not abort the process.**
///
/// `run_task` closes its watchers from a `Drop` guard, so on the path this
/// models -- a run whose task panicked -- `close` runs *while already
/// panicking*, and Rust aborts the process outright if a second panic escapes
/// a drop there. An abort takes the whole test binary with it, so a regression
/// here does not fail this test politely: it kills the run.
#[test]
fn a_sink_that_panics_while_the_run_is_already_unwinding_does_not_abort() {
    struct Closing(Arc<Watchers>);
    impl Drop for Closing {
        fn drop(&mut self) {
            self.0.close(a_message(SyncPhase::Failed));
        }
    }

    let watchers = Watchers::for_run(7);
    watchers.attach(Some(Arc::new(Boom) as Arc<dyn ProgressSink>));

    let died = std::thread::spawn(move || {
        let _closing = Closing(watchers);
        panic!("the run's task blew up");
    })
    .join();

    assert!(died.is_err(), "the thread was supposed to panic");
}

/// The recorder can tell a fan-out that reached it from one that did not.
///
/// Without this the three tests above would pass just as happily against a
/// `close` that delivered to nobody at all.
#[test]
fn the_recorder_would_notice_a_fan_out_that_reached_nobody() {
    let watchers = Watchers::for_run(7);
    let live = Arc::new(Recorder::default());
    watchers.attach(Some(Arc::clone(&live) as Arc<dyn ProgressSink>));
    assert!(live.seen().is_empty());
    watchers.close(a_message(SyncPhase::Finished));
    assert_eq!(live.seen().len(), 1);
}
