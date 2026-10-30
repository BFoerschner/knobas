//! Progress is throttled on purpose: a 40,000-item Jira sync that reported per
//! item would push 40,000 messages across the IPC bridge for a progress bar
//! that can show at most a few hundred distinct states.

use std::sync::{Arc, Mutex};
use std::time::Instant;

use knobas_source::contract::Walking;
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
    /// Draws nothing: this fake has no source view (#661).
    async fn view(
        &self,
        _entity: &str,
        _section: Option<&str>,
    ) -> Result<Option<knobas_source::view::SourceView>, knobas_source::SourceError> {
        Ok(None)
    }

    fn descriptor(&self) -> SourceDescriptor {
        SourceDescriptor {
            id: "flood".into(),
            adapter_kind: "flood".into(),
            name: "Flood".into(),
            capabilities: Vec::<Capability>::new(),
            adapter_version: "0.1.0".into(),
            auth_methods: Vec::new(),
            accepts_account: false,
            min_sync_interval_secs: 60,
            roles: Vec::new(),
            source_actions: Vec::new(),
            clone_from: Vec::new(),
            url_forms: Vec::new(),
            code_pages: None,
            write_ops: Vec::new(),
            entity_kinds: vec![KindInfo {
                id: "ticket".into(),
                label: "T".into(),
                plural: "T".into(),
                monogram: "FL".into(),
                full_sync_exhaustive: true,
                refreshes: true,
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
    async fn write(&self, _op: WriteOp) -> Result<knobas_source::WriteReceipt, SourceError> {
        Err(SourceError::protocol("read-only"))
    }

    /// No workflow: this fake declares no `transition` write op, which is the
    /// contract battery's rule for when the read is refused (#498).
    async fn reachable_transitions(
        &self,
        entity: &str,
    ) -> Result<Vec<String>, knobas_source::SourceError> {
        Err(knobas_source::SourceError::protocol(format!(
            "this fake has no workflow, so there are no reachable transitions for {entity:?}"
        )))
    }
}

#[tokio::test]
async fn the_decorator_forwards_every_item_untouched_and_returns_the_adapters_cursor() {
    let inner = MockSource::new();
    let recorder: Arc<dyn ProgressSink> = Arc::new(Recorder::default());
    let observed = Observed::new(&inner, "mock", 7, Instant::now(), Arc::clone(&recorder));

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
    let observed = Observed::new(&inner, "flood", 9, Instant::now(), sink);

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
    let observed = Observed::new(&inner, "flood", 3, Instant::now(), sink);

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
    let observed = Observed::new(&inner, "flood", 4, Instant::now(), sink);

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

/// An adapter that walks `n` units and pushes nothing: the run #1378 was
/// about, 300 unchanged repositories and zero items.
struct Walker(u64);

#[async_trait::async_trait]
impl Source for Walker {
    async fn view(
        &self,
        entity: &str,
        section: Option<&str>,
    ) -> Result<Option<knobas_source::view::SourceView>, SourceError> {
        Flood(0).view(entity, section).await
    }
    fn descriptor(&self) -> SourceDescriptor {
        Flood(0).descriptor()
    }
    async fn test_connection(&self) -> Result<ConnectionInfo, SourceError> {
        Ok(ConnectionInfo::default())
    }
    async fn sync(
        &self,
        _c: Option<Cursor>,
        sink: &mut (dyn Sink + Send),
    ) -> Result<Cursor, SourceError> {
        for done in 0..=self.0 {
            sink.walked(done, Some(self.0), "repositories").await;
        }
        Ok("done".into())
    }
    async fn write(&self, op: WriteOp) -> Result<knobas_source::WriteReceipt, SourceError> {
        Flood(0).write(op).await
    }
    async fn reachable_transitions(&self, entity: &str) -> Result<Vec<String>, SourceError> {
        Flood(0).reachable_transitions(entity).await
    }
}

fn reading(r: &SyncProgress) -> (Option<u64>, Option<u64>, Option<&str>) {
    (r.walked, r.of, r.unit.as_deref())
}

/// An adapter that never reports its walk produces messages with the three
/// reading fields absent, on the wire too, so the wizard's bar stays
/// indeterminate as before (#1379).
#[tokio::test]
async fn an_adapter_that_never_reports_its_walk_sends_no_reading() {
    let recorder = Arc::new(Recorder::default());
    let sink: Arc<dyn ProgressSink> = recorder.clone();
    let inner = Flood(3);
    let observed = Observed::new(&inner, "flood", 5, Instant::now(), sink);
    observed.sync(None, &mut Counting::default()).await.unwrap();

    let reports = recorder.seen();
    assert!(!reports.is_empty());
    assert!(reports.iter().all(|r| reading(r) == (None, None, None)));
    let wire = serde_json::to_value(&reports[0]).unwrap();
    for field in ["walked", "of", "unit"] {
        assert!(wire.get(field).is_none(), "{field} is on the wire: {wire}");
    }
}

/// A run that pushes nothing still moves the bar: the first reading goes out
/// at once, later ones on the item throttle rather than one message per call,
/// and the final `Fetching` sample carries the last reading (#1378, #1379).
#[tokio::test]
async fn a_walk_reading_reaches_the_progress_sink_on_the_item_throttle() {
    let recorder = Arc::new(Recorder::default());
    let sink: Arc<dyn ProgressSink> = recorder.clone();
    let inner = Walker(5_000);
    let observed = Observed::new(&inner, "flood", 6, Instant::now(), sink);
    observed.sync(None, &mut Counting::default()).await.unwrap();

    let reports = recorder.seen();
    assert!(
        reports.len() < 100,
        "5001 readings must not produce 5001 messages, got {}",
        reports.len()
    );
    assert_eq!(
        reading(&reports[0]),
        (Some(0), Some(5_000), Some("repositories"))
    );
    let (last, fetching) = reports.split_last().expect("some progress");
    assert_eq!(last.phase, SyncPhase::Writing);
    let final_fetching = fetching.last().expect("a final fetching sample");
    assert_eq!(final_fetching.phase, SyncPhase::Fetching);
    assert_eq!(
        reading(final_fetching),
        (Some(5_000), Some(5_000), Some("repositories"))
    );
    assert!(reports.iter().all(|r| r.items == 0));
}

/// The mock's reading arrives through the decorator, and the decorator hands
/// it on to the sink it wraps rather than swallowing it.
#[tokio::test]
async fn the_mocks_walk_reaches_the_progress_sink_and_the_wrapped_sink() {
    let recorder = Arc::new(Recorder::default());
    let sink: Arc<dyn ProgressSink> = recorder.clone();
    let inner = MockSource::new();
    let observed = Observed::new(&inner, "mock", 8, Instant::now(), sink);
    let mut wrapped = Walking::default();
    observed.sync(None, &mut wrapped).await.unwrap();

    let (done, of, _) = wrapped
        .readings
        .last()
        .cloned()
        .expect("the wrapped sink heard the walk");
    assert_eq!(Some(done), of);
    let reports = recorder.seen();
    assert_eq!(reading(&reports[0]), (Some(0), of, Some("items")));
    let final_fetching = &reports[reports.len() - 2];
    assert_eq!(final_fetching.phase, SyncPhase::Fetching);
    assert_eq!(reading(final_fetching), (Some(done), of, Some("items")));
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
        walked: None,
        of: None,
        unit: None,
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

/// An adapter that says it walked its one kind whole, and pushes nothing.
struct Whole;

#[async_trait::async_trait]
impl Source for Whole {
    async fn view(
        &self,
        entity: &str,
        section: Option<&str>,
    ) -> Result<Option<knobas_source::view::SourceView>, SourceError> {
        Flood(0).view(entity, section).await
    }
    fn descriptor(&self) -> SourceDescriptor {
        Flood(0).descriptor()
    }
    async fn test_connection(&self) -> Result<ConnectionInfo, SourceError> {
        Ok(ConnectionInfo::default())
    }
    async fn sync(
        &self,
        _c: Option<Cursor>,
        sink: &mut (dyn Sink + Send),
    ) -> Result<Cursor, SourceError> {
        sink.walked_whole("ticket").await;
        Ok("done".into())
    }
    async fn write(&self, op: WriteOp) -> Result<knobas_source::WriteReceipt, SourceError> {
        Flood(0).write(op).await
    }
    async fn reachable_transitions(&self, entity: &str) -> Result<Vec<String>, SourceError> {
        Flood(0).reachable_transitions(entity).await
    }
}

/// A sink that keeps the kinds it heard were walked whole.
#[derive(Default)]
struct HearsWhole(Vec<String>);

#[async_trait::async_trait]
impl Sink for HearsWhole {
    async fn item(&mut self, _item: SyncItem) -> Result<(), SourceError> {
        Ok(())
    }
    async fn walked_whole(&mut self, kind: &str) {
        self.0.push(kind.to_owned());
    }
}

/// The engine's sweep reads a kind walked whole off its own sink (#2186), so
/// the progress decorator that the scheduler wraps every run in hands it on.
#[tokio::test]
async fn the_decorator_hands_on_a_kind_walked_whole() {
    let recorder: Arc<dyn ProgressSink> = Arc::new(Recorder::default());
    let observed = Observed::new(&Whole, "flood", 8, Instant::now(), recorder);
    let mut sink = HearsWhole::default();
    observed.sync(Some("at".into()), &mut sink).await.unwrap();
    assert_eq!(sink.0, ["ticket"]);
}
