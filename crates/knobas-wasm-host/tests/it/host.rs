//! The WASM host, tested at the SPI: the contract battery over a
//! `dyn Source` that wraps the fixture component, and the version check at
//! load. Nothing here looks at how a call crosses the boundary.

use knobas_source::contract::{Fault, VecSink, battery};
use knobas_source::instance::SourceInstance;
use knobas_source::{AuthMethod, SourceError, WriteOp};
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

/// Battery clause 5 at runtime (#1950): a guest that performs every op it is
/// handed, though it declares one source action, is never handed another.
/// The host refuses an undeclared op, and a source action of an undeclared
/// id, by name before the call crosses; the declared one still reaches the
/// guest, which answers it with its own receipt.
#[tokio::test]
async fn a_guest_is_never_handed_a_write_it_does_not_declare() {
    let config = serde_json::json!({ "performs_undeclared": true });
    let source = match load_fixture().build(instance("surveys", config)) {
        Ok(source) => source,
        Err(e) => panic!("the fixture must build: {e:?}"),
    };
    let fields = std::collections::BTreeMap::from([("day".to_owned(), "Tuesday".to_owned())]);

    let comment = source
        .write(WriteOp::Comment {
            entity: "surveys:survey-1".to_owned(),
            body: "fender cracked".to_owned(),
        })
        .await;
    assert_eq!(
        comment.map_err(|e| e.to_string()),
        Err(
            "protocol: source \"surveys\" does not declare the write \"comment\", so it was not \
             sent"
                .to_owned()
        )
    );
    let scrap = source
        .write(WriteOp::SourceAction {
            entity: "surveys:survey-1".to_owned(),
            id: "scrap_crane".to_owned(),
            fields: fields.clone(),
        })
        .await;
    assert_eq!(
        scrap.map_err(|e| e.to_string()),
        Err(
            "protocol: source \"surveys\" does not declare the source action \"scrap_crane\", so \
             it was not sent"
                .to_owned()
        )
    );

    let booked = source
        .write(WriteOp::SourceAction {
            entity: "surveys:survey-1".to_owned(),
            id: "book_repair".to_owned(),
            fields,
        })
        .await
        .expect("the declared action crosses");
    assert_eq!(
        booked.remote_id.as_deref(),
        Some("performed source_action"),
        "the guest performed it"
    );
}

/// The same guest passes the battery whole: clause 5 and clause 11 probe it
/// with ops it would perform, and the host refuses them first.
#[tokio::test]
async fn a_guest_that_performs_undeclared_writes_passes_the_contract_battery() {
    let adapter = load_fixture();
    battery(|fault| {
        let mut config = fault_config(fault);
        config["performs_undeclared"] = serde_json::Value::Bool(true);
        match adapter.build(instance("surveys", config)) {
            Ok(source) => source,
            Err(e) => panic!("the fixture must build: {e:?}"),
        }
    })
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
    assert_eq!(cursor, "5");
    let ids: Vec<String> = sink.0.iter().map(|i| i.entity.to_string()).collect();
    assert_eq!(
        ids,
        [
            "surveys:survey-1",
            "surveys:survey-2",
            "surveys:survey-3",
            "surveys:pipeline-1",
            "surveys:run-1"
        ]
    );
    assert_eq!(sink.0[1].title, "Berth 7 fender survey");
    assert_eq!(sink.0[1].payload["key"], "survey-2");
    assert_eq!(
        sink.0[1].updated_at.map(|t| t.to_rfc3339()).as_deref(),
        Some("2026-10-18T09:00:00+00:00")
    );

    let mut rest = VecSink(Vec::new());
    let again = source.sync(Some("3".into()), &mut rest).await.unwrap();
    assert_eq!(again, "5");
    let ids: Vec<String> = rest.0.iter().map(|i| i.entity.to_string()).collect();
    assert_eq!(ids, ["surveys:pipeline-1", "surveys:run-1"]);
}

/// Takes the first item and refuses the second, counting every call.
struct RefusesTheSecond(usize);

#[async_trait::async_trait]
impl knobas_source::Sink for RefusesTheSecond {
    async fn item(&mut self, _item: knobas_source::SyncItem) -> Result<(), SourceError> {
        self.0 += 1;
        if self.0 == 2 {
            Err(SourceError::Sink("the mirror is full".into()))
        } else {
            Ok(())
        }
    }
}

#[tokio::test]
async fn a_sink_error_the_component_swallows_still_fails_the_sync() {
    // The `swallow_sink` fixture ignores what `sink.item` answers and hands
    // back cursor "5", which would skip the item the engine never stored.
    let source = match load_fixture().build(instance(
        "surveys",
        serde_json::json!({ "fault": "swallow_sink" }),
    )) {
        Ok(source) => source,
        Err(e) => panic!("the fixture must build: {e:?}"),
    };
    let mut sink = RefusesTheSecond(0);
    let answer = source.sync(None, &mut sink).await;
    assert!(
        matches!(&answer, Err(SourceError::Sink(m)) if m == "the mirror is full"),
        "{answer:?}"
    );
    // Once refused, the host answers every item after it itself.
    assert_eq!(sink.0, 2);
}

/// The SDK's `Sink::item` hands the sink's error to the adapter (#2037).
/// The host fails the sync whatever the component answers, so the
/// fixture's log line is what shows the error reached it.
#[tokio::test]
async fn the_component_hears_the_sink_s_refusal_of_an_item() {
    const ID: &str = "item-refused-2037";
    let lines = crate::limits::logged();
    let source = load_fixture()
        .build(instance(ID, serde_json::json!({})))
        .expect("the fixture builds");
    let answer = source.sync(None, &mut RefusesTheSecond(0)).await;
    assert!(matches!(answer, Err(SourceError::Sink(_))), "{answer:?}");
    let ours: Vec<String> = lines
        .lock()
        .unwrap()
        .iter()
        .filter(|(source, _)| source == ID)
        .map(|(_, m)| m.clone())
        .collect();
    assert_eq!(ours, ["the sink refused: sink: the mirror is full"]);
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
fn the_fixture_claims_its_custom_role_and_ci_with_every_optional_read() {
    let roles = load_fixture().template().roles;
    let claimed: Vec<&str> = roles.iter().map(|c| c.role.as_str()).collect();
    assert_eq!(
        claimed,
        ["wasm-fixture:survey-desk", knobas_source::role::CI]
    );
    let ci = roles
        .iter()
        .find(|c| c.role == knobas_source::role::CI)
        .unwrap();
    assert_eq!(
        ci.reads,
        [
            "run_log",
            "pipeline_settings",
            "older_runs",
            "run_tests",
            "test_history",
            "run_artifacts",
            "artifact_preview",
            "download",
            "run_chain",
        ]
    );
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

#[tokio::test]
async fn a_saved_view_s_query_crosses_to_the_component_and_its_translation_back() {
    let source = load_fixture()
        .build(instance("surveys", fault_config(Fault::None)))
        .unwrap();
    let copy = source
        .translate_query(&serde_json::json!({ "survey_query": "crane 4" }))
        .await
        .unwrap()
        .expect("the fixture translates a survey query");
    assert_eq!(
        serde_json::to_value(copy).unwrap(),
        serde_json::json!({
            "conditions": [{ "key": "label", "negated": false, "values": ["crane 4"] }],
            "kept": 1,
            "left_out": [],
        })
    );
    let none = source
        .translate_query(&serde_json::json!({}))
        .await
        .unwrap();
    assert_eq!(none, None, "a view it does not translate is `none`");
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
         knobas:adapter@1.x up to 1.3.0"
    );
}

#[test]
fn a_component_built_against_a_newer_minor_version_is_refused_at_load() {
    let error = load_error(&exporting("1.4.0"));
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
