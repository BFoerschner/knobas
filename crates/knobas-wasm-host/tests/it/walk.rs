//! The run's walk crosses the boundary (#1380): a component built against
//! `knobas:adapter@1.3.0` calls `sink.walked`, and its reading reaches the
//! engine's progress sink as a compiled-in adapter's does. A 1.0.0 component
//! still installs and syncs, and reports nothing.

use std::sync::{Arc, Mutex, PoisonError};

use knobas_source::instance::SourceInstance;
use knobas_source::{Sink, Source, SourceError, SyncItem};
use knobas_sync::progress::{Observed, ProgressSink, SyncPhase, SyncProgress};
use knobas_wasm_host::{WasmAdapter, manifest};

fn instance() -> SourceInstance {
    SourceInstance {
        id: "surveys".to_owned(),
        kind: "wasm-fixture".to_owned(),
        display_name: "Harbour surveys".to_owned(),
        base_url: "https://surveys.example".to_owned(),
        auth: None,
        secret: None,
        account: None,
        config: serde_json::json!({}),
    }
}

/// `artifact` through install's static checks, under a manifest that names
/// `interface`: what an installed adapter is.
fn install(artifact: &std::path::Path, interface: &str) -> WasmAdapter {
    let bytes = std::fs::read(artifact).unwrap_or_else(|e| {
        panic!(
            "the fixture at {} is not there ({e}); `just wasm-fixture` builds it",
            artifact.display()
        )
    });
    let manifest = manifest::Manifest::parse(
        &serde_json::json!({
            "adapter_kind": "wasm-fixture",
            "adapter_version": "0.1.0",
            "interface_version": interface,
            "artifact": { "file": "fixture.wasm", "sha256": manifest::sha256(&bytes) },
            "hosts": ["{base_url}"],
        })
        .to_string(),
    )
    .expect("the manifest is well formed");
    match manifest::check(manifest, bytes) {
        Ok(checked) => checked.adapter,
        Err(refusal) => panic!("the {interface} fixture must install: {refusal}"),
    }
}

/// What the adapter handed its sink, in order: an item as its entity, a
/// walk reading as `done/of unit`.
#[derive(Default)]
struct Handed(Vec<String>);

#[async_trait::async_trait]
impl Sink for Handed {
    async fn item(&mut self, item: SyncItem) -> Result<(), SourceError> {
        self.0.push(item.entity.to_string());
        Ok(())
    }

    async fn walked(&mut self, done: u64, of: Option<u64>, unit: &str) {
        let of = of.map_or_else(|| "?".to_owned(), |n| n.to_string());
        self.0.push(format!("{done}/{of} {unit}"));
    }
}

/// The engine's progress channel: every message it was sent.
#[derive(Default)]
struct Reported(Mutex<Vec<SyncProgress>>);

impl ProgressSink for Reported {
    fn report(&self, progress: SyncProgress) {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(progress);
    }
}

/// One sync from `cursor` through the engine's progress decorator: what the
/// adapter handed its sink, and what the engine reported.
async fn observed_sync(
    source: &dyn Source,
    cursor: Option<&str>,
) -> (Vec<String>, Vec<SyncProgress>) {
    let reported = Arc::new(Reported::default());
    let observed = Observed::new(
        source,
        "surveys",
        7,
        std::time::Instant::now(),
        Arc::clone(&reported) as Arc<dyn ProgressSink>,
    );
    let mut handed = Handed::default();
    observed
        .sync(cursor.map(str::to_owned), &mut handed)
        .await
        .expect("the fixture syncs");
    let messages = std::mem::take(&mut *reported.0.lock().unwrap_or_else(PoisonError::into_inner));
    (handed.0, messages)
}

fn reading(progress: &SyncProgress) -> (Option<u64>, Option<u64>, Option<&str>) {
    (progress.walked, progress.of, progress.unit.as_deref())
}

#[tokio::test]
async fn a_1_3_0_component_s_walk_reaches_the_engine_s_progress_sink() {
    let adapter = install(&knobas_wasm_host::fixture_component(), "1.3.0");
    let source = adapter.build(instance()).expect("the fixture builds");

    let (handed, messages) = observed_sync(source.as_ref(), None).await;
    assert_eq!(
        handed,
        [
            "0/5 items",
            "surveys:survey-1",
            "1/5 items",
            "surveys:survey-2",
            "2/5 items",
            "surveys:survey-3",
            "3/5 items",
            "surveys:pipeline-1",
            "4/5 items",
            "surveys:run-1",
            "5/5 items",
        ],
        "a reading before each item and one after the last, handed on in order"
    );
    // The first reading goes out at once; the run's last `fetching` sample
    // and its `writing` one carry the last reading.
    assert_eq!(reading(&messages[0]), (Some(0), Some(5), Some("items")));
    let fetched = messages
        .iter()
        .rfind(|m| m.phase == SyncPhase::Fetching)
        .expect("a final fetching sample");
    assert_eq!(
        (fetched.items, reading(fetched)),
        (5, (Some(5), Some(5), Some("items")))
    );
    let writing = messages.last().expect("a writing sample");
    assert_eq!(writing.phase, SyncPhase::Writing);
    assert_eq!(reading(writing), (Some(5), Some(5), Some("items")));

    // From a cursor the walk is what this run walks: two items, and a run
    // that walks nothing still says so.
    let (handed, _) = observed_sync(source.as_ref(), Some("3")).await;
    assert_eq!(
        handed,
        [
            "0/2 items",
            "surveys:pipeline-1",
            "1/2 items",
            "surveys:run-1",
            "2/2 items"
        ]
    );
    let (handed, messages) = observed_sync(source.as_ref(), Some("5")).await;
    assert_eq!(handed, ["0/0 items"]);
    let last = messages.last().expect("a writing sample");
    assert_eq!(
        (last.items, reading(last)),
        (0, (Some(0), Some(0), Some("items")))
    );
}

#[tokio::test]
async fn a_1_0_0_component_still_installs_syncs_and_reports_no_walk() {
    let adapter = install(&knobas_wasm_host::fixture_component_v1_0(), "1.0.0");
    let source = adapter.build(instance()).expect("the fixture builds");

    let (handed, messages) = observed_sync(source.as_ref(), None).await;
    assert_eq!(
        handed,
        [
            "surveys:survey-1",
            "surveys:survey-2",
            "surveys:survey-3",
            "surveys:pipeline-1",
            "surveys:run-1",
        ],
        "its items and no reading"
    );
    assert!(!messages.is_empty());
    for message in &messages {
        assert_eq!(reading(message), (None, None, None), "{message:?}");
    }
    let last = messages.last().expect("a writing sample");
    assert_eq!((last.phase, last.items), (SyncPhase::Writing, 5));
}

/// Takes the first item, refuses the second, and records what it was handed
/// as [`Handed`] does: the refused item is not recorded.
#[derive(Default)]
struct RefusesTheSecond(Handed, usize);

#[async_trait::async_trait]
impl Sink for RefusesTheSecond {
    async fn item(&mut self, item: SyncItem) -> Result<(), SourceError> {
        self.1 += 1;
        if self.1 == 2 {
            return Err(SourceError::Sink("the mirror is full".into()));
        }
        self.0.item(item).await
    }

    async fn walked(&mut self, done: u64, of: Option<u64>, unit: &str) {
        self.0.walked(done, of, unit).await;
    }
}

/// The §10.8 amendment's "after the sync's sink refused an item, [a reading]
/// is dropped": the `swallow_sink` fixture walks on past the refusal, and the
/// engine's sink hears nothing of it.
#[tokio::test]
async fn a_reading_after_the_sink_refused_an_item_is_dropped() {
    let adapter = install(&knobas_wasm_host::fixture_component(), "1.3.0");
    let mut config = instance();
    config.config = serde_json::json!({ "fault": "swallow_sink" });
    let source = adapter.build(config).expect("the fixture builds");

    let mut sink = RefusesTheSecond::default();
    let answer = source.sync(None, &mut sink).await;
    assert!(
        matches!(&answer, Err(SourceError::Sink(m)) if m == "the mirror is full"),
        "{answer:?}"
    );
    assert_eq!(
        sink.0.0,
        ["0/5 items", "surveys:survey-1", "1/5 items"],
        "nothing after the refused second item"
    );
}
