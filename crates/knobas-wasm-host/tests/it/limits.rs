//! Every call has a deadline and a memory cap (ADR-0028, #1204), at the SPI:
//! a fixture that passes one fails its call naming the limit, and a source
//! beside it is not held up.

use std::time::{Duration, Instant};

use knobas_source::contract::VecSink;
use knobas_source::instance::SourceInstance;
use knobas_source::{Source, SourceError};
use knobas_wasm_host::limits::{self, Limits};
use knobas_wasm_host::load_fixture;
use knobas_wasm_host::manifest::{Manifest, Refusal};

fn source(id: &str, fault: &str, limits: Limits) -> Box<dyn Source> {
    let instance = SourceInstance {
        id: id.to_owned(),
        kind: "wasm-fixture".to_owned(),
        display_name: "Harbour surveys".to_owned(),
        base_url: "https://surveys.example".to_owned(),
        auth: None,
        secret: None,
        account: None,
        config: serde_json::json!({ "fault": fault }),
    };
    match load_fixture().with_limits(limits).build(instance) {
        Ok(source) => source,
        Err(e) => panic!("the fixture must build: {e:?}"),
    }
}

fn protocol_message(answer: Result<String, SourceError>) -> String {
    match answer {
        Err(SourceError::Protocol { message, .. }) => message,
        other => panic!("expected a protocol error naming the limit, got {other:?}"),
    }
}

#[tokio::test]
async fn a_fixture_that_spins_forever_is_stopped_at_its_deadline() {
    let limits = Limits {
        deadline: Duration::from_secs(1),
        ..Limits::default()
    };
    let spinner = source("spinner", "spin", limits);
    let started = Instant::now();
    let answer = spinner.sync(None, &mut VecSink(Vec::new())).await;
    let took = started.elapsed();
    assert_eq!(
        protocol_message(answer),
        "the external adapter \"wasm-fixture\" was stopped at its deadline of 1 s"
    );
    assert!(
        took < Duration::from_secs(10),
        "stopped at its deadline, not later: {took:?}"
    );
}

#[tokio::test]
async fn a_fixture_that_allocates_past_its_cap_is_stopped_naming_it() {
    let limits = Limits {
        memory_mib: 16,
        ..Limits::default()
    };
    let hog = source("hog", "hog", limits);
    let answer = hog.sync(None, &mut VecSink(Vec::new())).await;
    assert_eq!(
        protocol_message(answer),
        "the external adapter \"wasm-fixture\" was stopped at its memory cap of 16 MiB"
    );
}

#[tokio::test]
async fn a_healthy_source_beside_a_spinning_one_is_not_held_up() {
    // One thread: the spinner and the healthy source share it, as two
    // sources' runs share a runtime worker, so a spinner that held its
    // thread would hold the other run until its deadline.
    let spinning = Limits {
        deadline: Duration::from_secs(2),
        ..Limits::default()
    };
    let spinner = source("spinner", "spin", spinning);
    let healthy = source("surveys", "none", Limits::default());
    let started = Instant::now();
    let mut items = VecSink(Vec::new());
    let (spun, synced) = tokio::join!(
        async { spinner.sync(None, &mut VecSink(Vec::new())).await },
        async {
            let answer = healthy.sync(None, &mut items).await;
            (answer, started.elapsed())
        },
    );
    let (cursor, healthy_took) = synced;
    assert!(cursor.is_ok(), "the healthy source's sync: {cursor:?}");
    assert!(!items.0.is_empty(), "the healthy source handed its items");
    assert!(
        healthy_took < spinning.deadline,
        "the healthy source finished before the spinner was stopped: {healthy_took:?}"
    );
    assert!(protocol_message(spun).contains("deadline of 2 s"));
}

fn manifest_with(limits: serde_json::Value) -> Result<Manifest, Refusal> {
    Manifest::parse(
        &serde_json::json!({
            "adapter_kind": "wasm-fixture",
            "adapter_version": "0.1.0",
            "interface_version": "1.0.0",
            "artifact": { "file": "fixture.wasm", "sha256": "0".repeat(64) },
            "limits": limits,
        })
        .to_string(),
    )
}

#[test]
fn a_manifest_asking_above_the_host_maximum_is_refused() {
    for (asked, says) in [
        (
            serde_json::json!({ "deadline_secs": 3_601 }),
            "limits.deadline_secs 3601 is outside what this knobas allows (1 to 3600)",
        ),
        (
            serde_json::json!({ "memory_mib": 1_025 }),
            "limits.memory_mib 1025 is outside what this knobas allows (1 to 1024)",
        ),
        (
            serde_json::json!({ "deadline_secs": 0 }),
            "limits.deadline_secs 0 is outside",
        ),
    ] {
        let refused = manifest_with(asked.clone()).err().map(|e| e.to_string());
        assert!(
            refused.as_deref().is_some_and(|m| m
                .starts_with("the manifest's limits are refused: ")
                && m.contains(says)),
            "{asked}: {refused:?}"
        );
    }
}

#[test]
fn a_manifest_may_ask_up_to_the_maximum_and_absent_is_the_default() {
    let at_most = manifest_with(serde_json::json!({ "deadline_secs": 3_600, "memory_mib": 1_024 }))
        .and_then(|m| m.limits())
        .unwrap();
    assert_eq!(at_most.deadline, limits::MAX_DEADLINE);
    assert_eq!(at_most.memory_mib, limits::MAX_MEMORY_MIB);
    let default = manifest_with(serde_json::json!({}))
        .and_then(|m| m.limits())
        .unwrap();
    assert_eq!(default, Limits::default());
}

// -- a request inside the call (#1203's two open bounds) -----------------------

/// A server that takes every connection and never answers, keeping each
/// open; answers its address.
async fn silent_server() -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move {
        let mut held = Vec::new();
        while let Ok((socket, _)) = listener.accept().await {
            held.push(socket);
        }
    });
    url
}

fn fetching(base_url: &str, config: serde_json::Value, limits: Limits) -> Box<dyn Source> {
    let instance = SourceInstance {
        id: "surveys".to_owned(),
        kind: "wasm-fixture".to_owned(),
        display_name: "Harbour surveys".to_owned(),
        base_url: base_url.to_owned(),
        auth: None,
        secret: None,
        account: None,
        config,
    };
    let hosts = vec![knobas_wasm_host::allow::HostPattern::parse("{base_url}").unwrap()];
    match load_fixture()
        .with_hosts(hosts)
        .with_limits(limits)
        .build(instance)
    {
        Ok(source) => source,
        Err(e) => panic!("the fixture must build: {e:?}"),
    }
}

#[tokio::test]
async fn a_request_s_first_byte_timeout_is_honoured_inside_the_deadline() {
    let url = silent_server().await;
    let limits = Limits {
        deadline: Duration::from_secs(5),
        ..Limits::default()
    };
    let source = fetching(
        &url,
        serde_json::json!({ "fetch": format!("{url}/never"), "first_byte_ms": 200 }),
        limits,
    );
    let started = Instant::now();
    let answer = source.test_connection().await;
    // The component's own timeout ends the request, and the component
    // answers it: a deadline would have stopped the call instead.
    assert!(
        matches!(&answer, Err(SourceError::Unreachable(text)) if text.contains("ConnectionReadTimeout")),
        "{answer:?}"
    );
    assert!(started.elapsed() < Duration::from_secs(10));
}

#[tokio::test]
async fn an_outgoing_body_past_the_memory_cap_is_refused() {
    let url = silent_server().await;
    let limits = Limits {
        deadline: Duration::from_secs(10),
        memory_mib: 8,
    };
    // Nine MiB, written 4 KiB at a time: the component stays under its cap,
    // and the host, which buffers the body, refuses it at the cap.
    let source = fetching(
        &url,
        serde_json::json!({ "fetch": format!("{url}/upload"), "post_mib": 9 }),
        limits,
    );
    let answer = source.test_connection().await;
    assert!(
        matches!(&answer, Err(SourceError::Unreachable(text)) if text.contains("HttpRequestBodySize")),
        "{answer:?}"
    );
}
