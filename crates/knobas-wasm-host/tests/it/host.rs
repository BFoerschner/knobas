//! The WASM host, tested at the SPI: the contract battery over a
//! `dyn Source` that wraps the fixture component, and the version check at
//! load. Nothing here looks at how a call crosses the boundary.

use knobas_source::AuthMethod;
use knobas_source::contract::{Fault, VecSink, battery};
use knobas_source::instance::SourceInstance;
use knobas_wasm_host::{LoadError, Version, WasmAdapter, load_fixture};

fn instance(id: &str, config: serde_json::Value) -> SourceInstance {
    SourceInstance {
        id: id.to_owned(),
        kind: "wasm-fixture".to_owned(),
        display_name: "Harbour surveys".to_owned(),
        base_url: "https://surveys.example".to_owned(),
        auth: Some(AuthMethod::Pat),
        secret: Some("never-crosses".to_owned()),
        account: None,
        config,
    }
}

fn fault_config(fault: Fault) -> serde_json::Value {
    let fault = match fault {
        Fault::None => "none",
        Fault::Unauthorized => "unauthorized",
        Fault::Unreachable => "unreachable",
    };
    serde_json::json!({ "fault": fault })
}

#[tokio::test]
async fn the_host_wrapping_the_fixture_passes_the_contract_battery() {
    let adapter = load_fixture();
    battery(
        |fault| match adapter.build(instance("surveys", fault_config(fault))) {
            Ok(source) => source,
            Err(e) => panic!("the fixture must build: {e:?}"),
        },
    )
    .await;
}

#[tokio::test]
async fn a_sync_emits_the_fixed_corpus_and_resumes_from_its_cursor() {
    let source = match load_fixture().build(instance("surveys", serde_json::json!({}))) {
        Ok(source) => source,
        Err(e) => panic!("the fixture must build: {e:?}"),
    };
    let mut sink = VecSink(Vec::new());
    let cursor = source.sync(None, &mut sink).await.unwrap();
    assert_eq!(cursor, "3");
    let ids: Vec<String> = sink.0.iter().map(|i| i.entity.to_string()).collect();
    assert_eq!(
        ids,
        ["surveys:survey-1", "surveys:survey-2", "surveys:survey-3"]
    );
    assert_eq!(sink.0[1].title, "Berth 7 fender survey");
    assert_eq!(sink.0[1].payload["key"], "survey-2");
    assert_eq!(
        sink.0[1].updated_at.map(|t| t.to_rfc3339()).as_deref(),
        Some("2026-10-18T09:00:00+00:00")
    );

    let mut rest = VecSink(Vec::new());
    let again = source.sync(Some("1".into()), &mut rest).await.unwrap();
    assert_eq!(again, "3");
    let ids: Vec<String> = rest.0.iter().map(|i| i.entity.to_string()).collect();
    assert_eq!(ids, ["surveys:survey-2", "surveys:survey-3"]);
}

#[test]
fn the_descriptor_template_is_the_component_s_own() {
    let adapter = load_fixture();
    assert_eq!(adapter.kind(), "wasm-fixture");
    assert_eq!(adapter.version(), knobas_wasm_host::INTERFACE_VERSION);
    let template = adapter.template();
    assert_eq!(template.id, "wasm-fixture");
    assert_eq!(template.entity_kinds[0].plural, "Surveys");
}

#[test]
fn an_instance_of_another_kind_is_refused() {
    let mut other = instance("surveys", serde_json::json!({}));
    other.kind = "jira".into();
    let refused = load_fixture().build(other).err().map(|e| e.to_string());
    assert!(
        refused.as_deref().is_some_and(|e| e.contains("\"jira\"")),
        "{refused:?}"
    );
}

/// A component that exports `knobas:adapter/source` at `version` and
/// nothing in it: enough for the version check, which runs first.
fn exporting(version: &str) -> Vec<u8> {
    wat::parse_str(format!(
        r#"(component
             (instance $source)
             (export "knobas:adapter/source@{version}" (instance $source)))"#
    ))
    .unwrap()
}

fn load_error(bytes: &[u8]) -> LoadError {
    match WasmAdapter::from_bytes(bytes) {
        Ok(adapter) => panic!("{adapter:?} loaded"),
        Err(e) => e,
    }
}

#[test]
fn a_component_built_against_another_major_version_is_refused_at_load() {
    let error = load_error(&exporting("2.0.0"));
    assert!(
        matches!(
            error,
            LoadError::Unsupported {
                found: Version {
                    major: 2,
                    minor: 0,
                    patch: 0
                }
            }
        ),
        "{error:?}"
    );
    assert_eq!(
        error.to_string(),
        "the component is built against knobas:adapter@2.0.0, and this knobas hosts \
         knobas:adapter@1.x up to 1.0.0"
    );
}

#[test]
fn a_component_built_against_a_newer_minor_version_is_refused_at_load() {
    let error = load_error(&exporting("1.1.0"));
    assert!(matches!(error, LoadError::Unsupported { .. }), "{error:?}");
}

#[test]
fn a_component_that_exports_no_source_is_refused_at_load() {
    let bytes = wat::parse_str("(component)").unwrap();
    assert!(matches!(load_error(&bytes), LoadError::NoSource));
}

#[test]
fn a_core_module_is_not_a_component() {
    let bytes = wat::parse_str("(module)").unwrap();
    assert!(matches!(load_error(&bytes), LoadError::NotAComponent(_)));
}
