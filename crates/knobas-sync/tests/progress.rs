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
use knobas_sync::progress::{Observed, ProgressSink, SyncPhase, SyncProgress};

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
            }],
            full_sync_exhaustive: true,
            config_schema: serde_json::json!({ "type": "object", "properties": {} }),
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
        Err(SourceError::Protocol("read-only".into()))
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
    assert_eq!(
        observed.descriptor().full_sync_exhaustive,
        inner.descriptor().full_sync_exhaustive,
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
        reports
            .iter()
            .all(|r| r.run_id == 9 && r.phase == SyncPhase::Fetching),
        "every message names its run and the phase it is in"
    );
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
    assert_eq!(reports.len(), 1, "exactly the final message");
    assert_eq!(reports[0].items, 0);
    assert_eq!(reports[0].run_id, 3);
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
