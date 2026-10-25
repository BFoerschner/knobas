//! Refresh's read crosses the boundary (#2101, ADR-0031): a component built
//! against `knobas:adapter@1.4.0` answers the read of one item by id as the
//! item its sync emits, and *gone* for one deleted at the source. One built
//! against 1.3.0, the minor before it, still installs and syncs, and
//! answers *not offered*, which is what sends its source to a run after a
//! landed write.

use knobas_source::contract::VecSink;
use knobas_source::instance::SourceInstance;
use knobas_source::{RefreshAnswer, Source};
use knobas_wasm_host::{Version, WasmAdapter, load_fixture, manifest};

fn instance(config: serde_json::Value) -> SourceInstance {
    SourceInstance {
        id: "surveys".to_owned(),
        kind: "wasm-fixture".to_owned(),
        display_name: "Harbour surveys".to_owned(),
        base_url: "https://surveys.example".to_owned(),
        auth: None,
        secret: None,
        account: None,
        config,
    }
}

fn build(adapter: &WasmAdapter, config: serde_json::Value) -> Box<dyn Source> {
    adapter
        .build(instance(config))
        .unwrap_or_else(|e| panic!("the fixture must build: {e:?}"))
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

async fn synced(source: &dyn Source) -> Vec<knobas_source::SyncItem> {
    let mut sink = VecSink(Vec::new());
    source
        .sync(None, &mut sink)
        .await
        .expect("the fixture syncs");
    sink.0
}

fn json(item: &knobas_source::SyncItem) -> serde_json::Value {
    serde_json::to_value(item).expect("an item serializes")
}

/// Every item the sync emitted, read back by its id, is the item the sync
/// emitted, field for field: the read crosses as `sink.item` does.
#[tokio::test]
async fn a_refresh_crosses_as_the_item_the_sync_emits_for_that_id() {
    let source = build(&load_fixture(), serde_json::json!({}));
    assert!(source.descriptor().refreshes());
    let items = synced(source.as_ref()).await;
    assert_eq!(items.len(), 5, "the surveys, the pipeline and the run");
    for item in &items {
        let id = item.entity.to_string();
        match source.refresh(&id).await {
            Ok(RefreshAnswer::Item(read)) => assert_eq!(json(&read), json(item), "{id}"),
            other => panic!("refresh of {id} answered {other:?}"),
        }
    }
}

/// A survey deleted at the source: the sync no longer emits it, and its
/// refresh answers *gone*, while its neighbours still answer.
#[tokio::test]
async fn a_refresh_of_an_item_deleted_at_the_source_answers_gone() {
    let adapter = load_fixture();
    let before = synced(build(&adapter, serde_json::json!({})).as_ref()).await;
    assert!(
        before
            .iter()
            .any(|i| i.entity.to_string() == "surveys:survey-2")
    );

    let source = build(&adapter, serde_json::json!({ "deleted": "survey-2" }));
    let after = synced(source.as_ref()).await;
    assert!(
        after
            .iter()
            .all(|i| i.entity.to_string() != "surveys:survey-2"),
        "the sync no longer emits it"
    );
    let gone = source.refresh("surveys:survey-2").await;
    assert!(matches!(gone, Ok(RefreshAnswer::Gone)), "{gone:?}");
    let live = source.refresh("surveys:survey-1").await;
    assert!(matches!(live, Ok(RefreshAnswer::Item(_))), "{live:?}");
}

/// A read that fails crosses as the sync's fault, never as *gone*.
#[tokio::test]
async fn a_failed_refresh_is_an_error_not_gone() {
    let source = build(
        &load_fixture(),
        serde_json::json!({ "fault": "unauthorized" }),
    );
    let answer = source.refresh("surveys:survey-1").await;
    assert!(
        matches!(
            answer,
            Err(knobas_source::SourceError::Unauthorized { status: Some(401) })
        ),
        "{answer:?}"
    );
}

/// The minor before `refresh`: it installs under a 1.3.0 manifest, syncs
/// its corpus, declares no `Capability::Refresh`, and answers *not offered*
/// for every id, without the host calling into it.
#[tokio::test]
async fn a_1_3_0_component_installs_syncs_and_answers_not_offered() {
    let adapter = install(&knobas_wasm_host::fixture_component_v1_3(), "1.3.0");
    assert_eq!(
        adapter.version(),
        Version {
            major: 1,
            minor: 3,
            patch: 0
        }
    );
    assert!(!adapter.refreshes());
    assert!(load_fixture().refreshes());
    let source = build(&adapter, serde_json::json!({}));
    assert!(!source.descriptor().refreshes());
    let items = synced(source.as_ref()).await;
    assert_eq!(items.len(), 5);
    for item in &items {
        let id = item.entity.to_string();
        let answer = source.refresh(&id).await;
        assert!(
            matches!(answer, Ok(RefreshAnswer::NotOffered)),
            "{id}: {answer:?}"
        );
    }
}

/// A component without `refresh` that declares `Capability::Refresh` anyway
/// is not believed: the host drops the capability from its descriptor, so
/// the app offers *Sync now* and runs the source after a landed write, as
/// for any component of an earlier minor.
#[tokio::test]
async fn a_component_without_refresh_that_declares_it_is_not_believed() {
    let adapter = install(&knobas_wasm_host::fixture_component_v1_3(), "1.3.0");
    let config = serde_json::json!({ "declares_refresh": true });
    let source = build(&adapter, config);
    let descriptor = source.descriptor();
    assert!(!descriptor.refreshes());
    // Nor is any kind its JSON says is refreshed (#2146): the per-kind flag
    // is read only under the capability the host dropped.
    assert!(!descriptor.entity_kinds.is_empty());
    for kind in &descriptor.entity_kinds {
        assert!(
            kind.refreshes,
            "{}: the component's JSON says refreshed",
            kind.id
        );
        assert!(!descriptor.refreshes_kind(&kind.id), "{}", kind.id);
    }
}
