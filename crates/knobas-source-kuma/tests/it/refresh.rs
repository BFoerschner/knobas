//! The refresh (ADR-0031, #2100): one monitor read out of `/metrics`, as the
//! sync emits it, over the recording.
//!
//! The recording has no socket.io channel, so what is witnessed here is the
//! source with only its API key, whose one observation is the document. A
//! source with an account asks Kuma whether a monitor the document omits is
//! still running (`src/refresh.rs`, its unit tests), and the real container
//! witnesses both through `just kuma-live`.

use knobas_source::RefreshAnswer;
use knobas_source::contract::VecSink;

use crate::support::{Fake, KEY, adapter};

/// **A refreshed monitor is the item a run emits for it**, field for field.
#[tokio::test]
async fn a_refreshed_monitor_is_the_item_a_run_emits_for_it() {
    let fake = Fake::start().await;
    let source = adapter(fake.base_url(), KEY);
    let mut sink = VecSink(Vec::new());
    source.sync(None, &mut sink).await.unwrap();
    let synced = sink.0.iter().find(|i| i.title == "canary").unwrap();
    assert_eq!(synced.entity.to_string(), "kuma:8");

    let RefreshAnswer::Item(item) = source.refresh("kuma:8").await.unwrap() else {
        panic!("the recorded canary is published");
    };
    assert_eq!(item.entity, synced.entity);
    assert_eq!(item.kind, synced.kind);
    assert_eq!(item.title, synced.title);
    assert_eq!(item.body_text, synced.body_text);
    assert_eq!(item.payload, synced.payload);
    assert_eq!(item.web_url, synced.web_url);
    assert!(!item.deleted);
}

/// **A monitor the document does not publish is gone**: paused and deleted
/// are one observation through `/metrics`, and a source with only its key
/// has no other.
#[tokio::test]
async fn a_monitor_the_document_omits_is_gone_to_a_source_with_only_its_key() {
    let fake = Fake::start().await;
    let source = adapter(fake.base_url(), KEY);
    let answer = source.refresh("kuma:9").await.unwrap();
    assert!(matches!(answer, RefreshAnswer::Gone), "{answer:?}");
}

/// **The roster a create targets is not a monitor**, so it is not offered:
/// a create whose receipt named no id is read back by its target, and the
/// run that follows is what finds the monitor.
#[tokio::test]
async fn the_create_target_is_not_offered() {
    let fake = Fake::start().await;
    let source = adapter(fake.base_url(), KEY);
    let answer = source
        .refresh(&knobas_source::monitor_target("kuma"))
        .await
        .unwrap();
    assert!(matches!(answer, RefreshAnswer::NotOffered), "{answer:?}");
}

/// **Another instance's monitor is refused, never read here**: `kuma-eu:8`
/// is not this Kuma's monitor 8.
#[tokio::test]
async fn another_instances_monitor_is_refused() {
    let fake = Fake::start().await;
    let source = adapter(fake.base_url(), KEY);
    let error = source.refresh("kuma-eu:8").await.expect_err("refused");
    assert!(
        matches!(error, knobas_source::SourceError::Protocol { .. }),
        "{error:?}"
    );
    assert!(error.to_string().contains("kuma-eu:8"), "{error}");
}

/// **The descriptor says so**, with or without an account: the read needs
/// only the key.
#[tokio::test]
async fn every_instance_declares_the_refresh() {
    let fake = Fake::start().await;
    assert!(adapter(fake.base_url(), KEY).descriptor().refreshes());
    assert!(
        crate::support::adapter_with_account(fake.base_url(), KEY)
            .descriptor()
            .refreshes()
    );
    assert!(knobas_source_kuma::descriptor_template().refreshes());
}
