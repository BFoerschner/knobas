//! A ticket's fields cross the boundary (ADR-0054): a component built
//! against `knobas:adapter@1.6.0` answers `tickets.ticket-fields` and
//! `tickets.field-options` and performs an `EditTicket`; one built against
//! 1.5.0, 1.3.0 or 1.0.0 loads, offers no edits, and is never handed one;
//! a component that declares the write without the interface is refused at
//! load naming both, and one that exports the interface without the write
//! (every SDK-built wiki) loads and offers no edits.

use std::sync::atomic::Ordering;

use knobas_source::contract::{Fault, battery};
use knobas_source::instance::SourceInstance;
use knobas_source::{Assignee, Source, WriteOp};
use knobas_wasm_host::{LoadError, WasmAdapter, load_fixture, patched_fixture};
use serde_json::json;

use crate::docs::counting_server;

fn instance(base_url: &str, config: serde_json::Value) -> SourceInstance {
    SourceInstance {
        id: "berths".to_owned(),
        kind: "wasm-fixture".to_owned(),
        display_name: "Harbour berths".to_owned(),
        base_url: base_url.to_owned(),
        auth: None,
        secret: None,
        account: None,
        config,
    }
}

/// `adapter` built over a counting stand-in its manifest lets it reach,
/// with its tickets synced.
async fn over_a_stand_in(
    adapter: WasmAdapter,
) -> (
    Box<dyn Source>,
    std::sync::Arc<std::sync::atomic::AtomicUsize>,
) {
    let (address, accepted) = counting_server().await;
    let adapter = adapter.with_hosts(vec![
        knobas_wasm_host::allow::HostPattern::parse(&address).unwrap(),
    ]);
    let source = adapter
        .build(instance(
            &format!("http://{address}"),
            json!({ "tickets": true }),
        ))
        .expect("the fixture builds");
    (source, accepted)
}

fn retitle() -> WriteOp {
    WriteOp::EditTicket {
        entity: "berths:BERTH-1".to_owned(),
        title: Some("Fender panels replaced at berth 7".to_owned()),
        assignee: Some(Assignee::Account("pilot-ana".to_owned())),
        priority: None,
        labels: None,
        fields: std::collections::BTreeMap::new(),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_1_6_component_answers_its_fields_and_options_and_its_edit_reaches_the_source() {
    let (source, accepted) = over_a_stand_in(load_fixture()).await;
    let fields = source
        .ticket_fields("berths:BERTH-1")
        .await
        .expect("the read crosses")
        .expect("a berth ticket edits");
    assert_eq!(knobas_source::ticket::fault(&fields), None);
    let ids: Vec<&str> = fields.fields.iter().map(|f| f.id.as_str()).collect();
    assert_eq!(ids, ["title", "assignee", "priority", "labels", "due"]);
    assert_eq!(
        fields.get("assignee").unwrap().shown.as_deref(),
        Some("Ines Harbourmaster")
    );
    assert!(
        source
            .ticket_fields("berths:survey-1")
            .await
            .unwrap()
            .is_none(),
        "a survey is no ticket"
    );
    let people = source
        .field_options("berths:BERTH-1", "assignee", "ana")
        .await
        .unwrap()
        .expect("the assignee has a list");
    assert_eq!(people.options.len(), 1);
    assert_eq!(people.options[0].value, "pilot-ana");
    assert!(people.complete);
    // Typed words a person's name holds and their account does not still find them.
    let by_name = source
        .field_options("berths:BERTH-1", "assignee", "ines")
        .await
        .unwrap()
        .expect("the assignee has a list");
    assert_eq!(
        by_name
            .options
            .iter()
            .map(|o| o.value.as_str())
            .collect::<Vec<_>>(),
        ["harbourmaster"]
    );
    assert!(
        source
            .field_options("berths:BERTH-1", "due", "")
            .await
            .unwrap()
            .is_none()
    );

    source.write(retitle()).await.expect("the edit lands");
    assert_eq!(
        accepted.load(Ordering::SeqCst),
        1,
        "the edit reached the source"
    );
    source
        .write(WriteOp::Transition {
            entity: "berths:BERTH-1".to_owned(),
            status: "In Progress".to_owned(),
        })
        .await
        .expect("the move lands");
    assert_eq!(accepted.load(Ordering::SeqCst), 2);

    // A label added or removed alone is a change, and is sent; labels that
    // add and remove nothing are not, and nothing leaves (the final suite's
    // mutants in the fixture's own check went unseen here).
    let labels = |add: &[&str], remove: &[&str]| WriteOp::EditTicket {
        entity: "berths:BERTH-1".to_owned(),
        title: None,
        assignee: None,
        priority: None,
        labels: Some(knobas_source::LabelChange {
            add: add.iter().map(|s| (*s).to_owned()).collect(),
            remove: remove.iter().map(|s| (*s).to_owned()).collect(),
        }),
        fields: std::collections::BTreeMap::new(),
    };
    source
        .write(labels(&["quay"], &[]))
        .await
        .expect("a label added lands");
    source
        .write(labels(&[], &["fenders"]))
        .await
        .expect("a label removed lands");
    assert_eq!(accepted.load(Ordering::SeqCst), 4);
    let empty = source.write(labels(&[], &[])).await;
    assert!(
        matches!(&empty, Err(knobas_source::SourceError::Protocol { message, .. }) if message.contains("nothing to change")),
        "{empty:?}"
    );
    assert_eq!(
        accepted.load(Ordering::SeqCst),
        4,
        "nothing left for an empty edit"
    );
}

/// Clause 15 over the 1.6.0 fixture's tickets: answered for each, an empty
/// edit and an unlisted field refused before anything is sent, every list
/// answered.
#[tokio::test]
async fn the_1_6_fixture_with_tickets_passes_the_contract_battery() {
    let adapter = load_fixture();
    battery(|fault| {
        let fault = match fault {
            Fault::None => "none",
            Fault::Unauthorized => "unauthorized",
            Fault::Unreachable => "unreachable",
        };
        adapter
            .build(instance(
                "https://berths.example",
                json!({ "fault": fault, "tickets": true }),
            ))
            .expect("the fixture builds")
    })
    .await;
}

/// An adapter from before ticket fields: it loads, its tickets move (it
/// declares `transition`, in every 1.x world), it answers no fields, and an
/// `EditTicket` is refused by the host by name with nothing crossing.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_1_5_component_moves_its_tickets_offers_no_edits_and_is_never_handed_one() {
    let old = WasmAdapter::load(&knobas_wasm_host::fixture_component_v1_5()).expect("1.5.0 loads");
    assert!(!old.edits_tickets());
    assert!(
        knobas_wasm_host::load_fixture().edits_tickets(),
        "the current fixture exports the fields"
    );
    assert!(
        !old.template()
            .write_ops
            .iter()
            .any(|op| op == "edit_ticket")
    );
    let (source, accepted) = over_a_stand_in(old).await;
    assert!(
        source
            .ticket_fields("berths:BERTH-1")
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        source
            .field_options("berths:BERTH-1", "assignee", "")
            .await
            .unwrap()
            .is_none()
    );
    let refused = source.write(retitle()).await;
    assert_eq!(
        refused.map_err(|e| e.to_string()),
        Err(
            "protocol: source \"berths\" does not declare the write \"edit_ticket\", so it was \
             not sent"
                .to_owned()
        )
    );
    assert_eq!(accepted.load(Ordering::SeqCst), 0, "nothing crossed");
    assert_eq!(
        source
            .reachable_transitions("berths:BERTH-2")
            .await
            .unwrap(),
        ["Done", "To Do"]
    );
    source
        .write(WriteOp::Transition {
            entity: "berths:BERTH-2".to_owned(),
            status: "Done".to_owned(),
        })
        .await
        .expect("an old adapter's move lands");
    assert_eq!(accepted.load(Ordering::SeqCst), 1);
}

/// The 1.3.0 and 1.0.0 builds load as before and answer no fields.
#[tokio::test]
async fn older_components_load_and_answer_no_fields() {
    for path in [
        knobas_wasm_host::fixture_component_v1_3(),
        knobas_wasm_host::fixture_component_v1_0(),
    ] {
        let old = WasmAdapter::load(&path).expect("an older 1.x loads");
        assert!(!old.edits_tickets());
        let source = old
            .build(instance("https://berths.example", json!({})))
            .expect("it builds");
        assert!(
            source
                .ticket_fields("berths:survey-1")
                .await
                .unwrap()
                .is_none()
        );
        let refused = source.write(retitle()).await;
        assert!(refused.is_err(), "{refused:?}");
    }
}

/// The write without the interface is refused at load with a message naming
/// both; the interface without the write (what the SDK's `export!` gives a
/// wiki) loads, offers no edits and answers no fields.
#[tokio::test]
async fn edit_ticket_without_tickets_is_refused_and_tickets_without_edit_ticket_offers_none() {
    let unexported = patched_fixture(
        "knobas:adapter/tickets@1.7.0",
        "knobas:adapter/tickits@1.7.0",
    );
    let error = WasmAdapter::from_bytes(&unexported).expect_err("the write without the export");
    let LoadError::Link(message) = &error else {
        panic!("{error:?}")
    };
    assert!(
        message.contains("edit_ticket") && message.contains("knobas:adapter/tickets"),
        "{message}"
    );

    let undeclared = patched_fixture("edit_ticket", "edit_tickex");
    let adapter = WasmAdapter::from_bytes(&undeclared).expect("the export without the write loads");
    assert!(
        !adapter.edits_tickets(),
        "an export without the write offers no edits"
    );
    let (source, accepted) = over_a_stand_in(adapter).await;
    let fields = source
        .ticket_fields("berths:BERTH-1")
        .await
        .map_err(|e| e.to_string());
    assert!(matches!(fields, Ok(None)), "{fields:?}");
    assert_eq!(accepted.load(Ordering::SeqCst), 0, "nothing crossed");
}
