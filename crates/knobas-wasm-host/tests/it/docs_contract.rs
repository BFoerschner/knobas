//! The docs contract (#2444, spec #2432 story 50), what `knobas adapter
//! check` runs on a component that claims the wiki role: the projection
//! draws every page the sync emitted and refuses every other kind, and the
//! writes answer and refuse as the projection's sections say. A component
//! that breaks it fails naming the check.

use knobas_source::SyncItem;
use knobas_source::contract::VecSink;
use knobas_wasm_host::docs_contract::{self, Check};
use knobas_wasm_host::{WasmAdapter, load_fixture};
use serde_json::{Value, json};

use crate::docs::instance;

/// The fixture built with `config`, its descriptor and what a full sync
/// emitted.
async fn synced(
    adapter: &WasmAdapter,
    config: Value,
) -> (knobas_source::SourceDescriptor, Vec<SyncItem>) {
    let mut instance = instance();
    instance.config = config;
    let source = adapter.build(instance).expect("the fixture builds");
    let mut sink = VecSink(Vec::new());
    source
        .sync(None, &mut sink)
        .await
        .expect("the fixture syncs");
    (source.descriptor(), sink.0)
}

/// The check the fixture told to break the contract `breach` way fails at.
async fn breached(breach: &str) -> docs_contract::Breach {
    let adapter = load_fixture();
    let (descriptor, items) = synced(&adapter, json!({ "docs_breach": breach })).await;
    docs_contract::check(&adapter, &descriptor, &items)
        .expect_err("a component that breaks the docs contract fails it")
}

#[tokio::test]
async fn the_fixture_passes_the_docs_contract() {
    let adapter = load_fixture();
    let (descriptor, items) = synced(&adapter, json!({})).await;
    assert_eq!(docs_contract::check(&adapter, &descriptor, &items), Ok(()));
}

/// The ticket's example: a projection that draws a page's payload as a kind
/// the descriptor does not declare.
#[tokio::test]
async fn a_projection_that_draws_any_kind_fails_naming_the_check() {
    let breach = breached("draws_any_kind").await;
    assert_eq!(breach.check, Check::OtherKindRefused, "{breach}");
    assert!(
        breach
            .to_string()
            .starts_with("docs contract, other kind refused: "),
        "{breach}"
    );
    assert!(breach.detail.contains("docs_contract_probe"), "{breach}");
}

#[tokio::test]
async fn sections_numbered_from_zero_fail_naming_the_check() {
    let breach = breached("numbers_from_zero").await;
    assert_eq!(breach.check, Check::SectionsNumbered, "{breach}");
}

/// The fixture's handbook refuses its one section for its formatting, and
/// this component edits it anyway.
#[tokio::test]
async fn an_edit_of_a_section_the_projection_refuses_fails_naming_the_check() {
    let breach = breached("edits_refused_sections").await;
    assert_eq!(breach.check, Check::RefusedSectionRefused, "{breach}");
    assert!(breach.detail.contains("an edit"), "{breach}");
}

/// The rules' *Mooring* section is the one the fixture edits.
#[tokio::test]
async fn an_edit_against_another_version_than_the_page_s_fails_naming_the_check() {
    let breach = breached("edits_another_version").await;
    assert_eq!(breach.check, Check::EditableSectionEdited, "{breach}");
    assert!(breach.detail.contains("version 4"), "{breach}");
}

/// A component built without the docs export claims no wiki and passes:
/// there is nothing to hold. With the export renamed away but the claim
/// kept, it fails the first check.
#[tokio::test]
async fn a_wiki_claim_without_the_docs_export_fails_the_export_check() {
    let adapter = load_fixture();
    let (descriptor, items) = synced(&adapter, json!({})).await;
    let without = WasmAdapter::from_bytes(&knobas_wasm_host::patched_fixture(
        "knobas:adapter/docs@1.5.0",
        "knobas:adapter/doxs@1.5.0",
    ))
    .expect("the component without the export loads");
    let breach = docs_contract::check(&without, &descriptor, &items)
        .expect_err("a wiki claim needs the export");
    assert_eq!(breach.check, Check::Export, "{breach}");
    let v1_3 = WasmAdapter::load(&knobas_wasm_host::fixture_component_v1_3()).unwrap();
    let (descriptor, items) = synced(&v1_3, json!({})).await;
    assert_eq!(docs_contract::check(&v1_3, &descriptor, &items), Ok(()));
}

/// The fixture's pages hold every check: a section refused and one edited,
/// so a contract that skipped either would not be witnessed passing.
#[tokio::test]
async fn the_fixture_s_pages_have_a_refused_and_an_editable_section() {
    let adapter = load_fixture();
    let (_, items) = synced(&adapter, json!({})).await;
    let sections: Vec<_> = items
        .iter()
        .filter(|item| item.kind == "page")
        .flat_map(|item| adapter.project("page", &item.payload).unwrap().sections)
        .collect();
    assert!(sections.iter().any(|s| s.refusal.is_some()), "{sections:?}");
    assert!(
        sections
            .iter()
            .any(|s| s.text.as_deref() == Some("Two lines, fore and aft.")),
        "{sections:?}"
    );
}
