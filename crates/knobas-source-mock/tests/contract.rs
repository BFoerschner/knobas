//! The mock adapter proves it honours the SPI by running the shared contract
//! battery against itself, exactly as every real adapter will.
//!
//! The fixture assertions are deliberately literal: `fixtures/tidewater/work.json`
//! is the dataset every mockup was drawn against, so a silent edit to it would
//! quietly desynchronise the app from the design it is being built to match.

use knobas_source::contract::{Fault, VecSink, battery};
use knobas_source::{Capability, Source, SourceError, WriteOp};
use knobas_source_mock::MockSource;

#[tokio::test]
async fn passes_the_contract_battery() {
    battery(|fault| Box::new(MockSource::with_fault(fault)) as Box<dyn knobas_source::Source>)
        .await;
}

#[tokio::test]
async fn fixture_matches_the_brief() {
    let f = knobas_source_mock::fixture();
    assert_eq!(f.people.len(), 5);
    let t = f
        .tickets
        .iter()
        .find(|t| t.key == "PAY-231")
        .expect("PAY-231 present");
    assert_eq!(t.status, "In Progress");
    assert_eq!(t.epic.as_deref(), Some("PAY-200"));
    assert!(f.tickets.iter().any(|t| t.key == "OPS-77"));
    assert!(
        f.builds
            .iter()
            .any(|b| b.num == 1187 && b.status == "failed")
    );
}

/// Every group of the dataset is present, at its full size -- a fixture that
/// silently lost half its tickets would still satisfy the spot checks above.
#[tokio::test]
async fn fixture_carries_the_whole_dataset() {
    let f = knobas_source_mock::fixture();
    assert_eq!(f.today.to_rfc3339(), "2026-08-22T14:32:00+00:00");
    let keys: Vec<&str> = f.tickets.iter().map(|t| t.key.as_str()).collect();
    assert_eq!(
        keys,
        [
            "PAY-200", "PAY-231", "PAY-228", "PAY-240", "PAY-219", "PAY-236", "OPS-77"
        ]
    );
    let nums: Vec<u32> = f.prs.iter().map(|p| p.num).collect();
    assert_eq!(nums, [142, 144, 139]);
    let nums: Vec<u32> = f.builds.iter().map(|b| b.num).collect();
    assert_eq!(nums, [1188, 1187, 412]);
    assert_eq!(f.pages.len(), 5);
    assert_eq!(f.notes.len(), 3);
    let shas: Vec<&str> = f.commits.iter().map(|c| c.sha.as_str()).collect();
    assert_eq!(shas, ["c90d11", "a41f2c", "7be0e4"]);
    assert_eq!(f.branches.len(), 3);
    let repos: Vec<(&str, &str)> = f
        .repos
        .iter()
        .map(|r| (r.name.as_str(), r.lang.as_str()))
        .collect();
    assert_eq!(
        repos,
        [
            ("payout-service", "Rust"),
            ("ledger-api", "Kotlin"),
            ("ops-runbooks", "Markdown"),
        ]
    );
    let people: Vec<&str> = f.people.iter().map(|p| p.username.as_str()).collect();
    assert_eq!(
        people,
        [
            "mara.lindqvist",
            "jonas.becker",
            "priya.nair",
            "tomasz.wojcik",
            "lena.hoffmann",
        ]
    );
}

/// Prose the mockups quote verbatim: if this drifts, screens stop matching the
/// dataset they were designed against.
#[tokio::test]
async fn fixture_keeps_the_prose_verbatim() {
    let f = knobas_source_mock::fixture();
    let t = f.ticket("PAY-231").expect("PAY-231 present");
    assert_eq!(
        t.description.as_deref(),
        Some(
            "When the SEPA batch at the PSP returns a transient error (HTTP 503 or PSP code \
             `TEMP_UNAVAILABLE`), retry the payout with exponential backoff (base 30 s, factor 2, \
             max 5 attempts, jitter). Permanent failures go to the manual review queue."
        )
    );
    assert_eq!(t.estimate_h, Some(16));
    assert_eq!(t.spent_week_m, Some(540));
    assert_eq!(t.comments.len(), 2);
    assert_eq!(t.comments[0].who, "priya");
    assert!(
        t.comments[0]
            .text
            .ends_with("finance wants max 5 attempts.")
    );

    let b = f.builds.iter().find(|b| b.num == 1187).expect("#1187");
    assert!(
        b.log
            .as_deref()
            .expect("#1187 has a log excerpt")
            .contains("assertion failed: attempts == 5 (left: 6, right: 5)")
    );

    let p = f
        .pages
        .iter()
        .find(|p| p.id == "sepa-design")
        .expect("page");
    assert!(
        p.body
            .as_deref()
            .expect("design page has a body")
            .contains("base 30 s, factor 2, max 5 attempts, jitter ±10 %")
    );

    let pr = f.prs.iter().find(|p| p.num == 142).expect("PR #142");
    assert_eq!(pr.approvals.as_deref(), Some("1/2"));
    assert_eq!(pr.checks, [1188, 1187]);
    assert_eq!(pr.files.len(), 3);
    assert_eq!(pr.files[0].added, 84);
    assert_eq!(pr.files[0].removed, 12);

    let n = f.notes.iter().find(|n| n.id == "n2").expect("note n2");
    assert!(n.body_md.starts_with("Test expects 5 attempts"));
    assert_eq!(n.links, ["PAY-231", "#1187", "SEPA payout retry design"]);
}

/// The descriptor is the only thing the UI reads to render this source, so it
/// is asserted rather than assumed.
///
/// P12: `Capability::Search` now means "the source supports server-side
/// search, reserved for a future `Source::search`" -- the mock has no such
/// entry point, so it declares no Search. It keeps `Write` + `"comment"`,
/// which is the battery's exercise vehicle for the write path.
#[tokio::test]
async fn descriptor_declares_write_only_and_five_kinds() {
    let d = MockSource::new().descriptor();
    assert_eq!(d.id, "mock");
    assert_eq!(d.adapter_kind, "mock");
    assert_eq!(d.capabilities, [Capability::Write]);
    assert_eq!(d.write_ops, ["comment"]);
    // The fixture is the whole world, so a full sync is exhaustive and the
    // engine's sweep may tombstone what it stops emitting.
    assert!(d.entity_kinds.iter().all(|k| k.full_sync_exhaustive));
    assert!(d.auth_methods.is_empty(), "the mock authenticates nothing");
    let kinds: Vec<&str> = d.entity_kinds.iter().map(|k| k.id.as_str()).collect();
    assert_eq!(kinds, ["ticket", "pr", "build", "page", "commit"]);
    for k in &d.entity_kinds {
        assert!(!k.label.is_empty() && !k.plural.is_empty());
        assert_eq!(k.monogram.chars().count(), 2, "monogram of {:?}", k.id);
    }
    assert_eq!(
        d.config_schema,
        serde_json::json!({ "type": "object", "properties": {} })
    );
}

/// A full sync must not quietly drop a group of the dataset: the whole point of
/// the mock is that the app can be exercised end to end without a real backend.
#[tokio::test]
async fn full_sync_emits_every_work_item() {
    let f = knobas_source_mock::fixture();
    let s = MockSource::new();
    let mut sink = VecSink(Vec::new());
    let cursor = s.sync(None, &mut sink).await.expect("full sync");
    assert_eq!(cursor, "tidewater-v1");
    assert_eq!(
        sink.0.len(),
        f.tickets.len() + f.prs.len() + f.builds.len() + f.pages.len() + f.commits.len()
    );

    let ticket = sink
        .0
        .iter()
        .find(|i| i.entity.key == "PAY-231")
        .expect("PAY-231 emitted");
    assert_eq!(ticket.entity.to_string(), "mock:PAY-231");
    assert_eq!(ticket.kind, "ticket");
    assert_eq!(ticket.author.as_deref(), Some("mara"));
    assert!(!ticket.deleted);
    // body_text is what FTS indexes, so summary, description and comments all
    // have to be in it -- searching "manual review queue" must find the ticket.
    assert!(ticket.body_text.contains("Retry failed SEPA payouts"));
    assert!(ticket.body_text.contains("manual review queue"));
    assert!(ticket.body_text.contains("finance wants max 5 attempts"));
    assert_eq!(ticket.payload["key"], "PAY-231");
    assert_eq!(ticket.payload["epic"], "PAY-200");

    for (kind, key) in [
        ("pr", "payout-service#142"),
        ("build", "Payout_IntegrationTests#1187"),
        ("page", "sepa-design"),
        ("commit", "a41f2c"),
    ] {
        let it = sink
            .0
            .iter()
            .find(|i| i.entity.key == key)
            .unwrap_or_else(|| panic!("{key} emitted"));
        assert_eq!(it.kind, kind);
    }
    let build = sink
        .0
        .iter()
        .find(|i| i.entity.key == "Payout_IntegrationTests#1187")
        .expect("#1187 emitted");
    assert!(build.body_text.contains("gives_up_after_max_attempts"));
}

/// The fixture never changes, so an incremental sync has nothing to say -- and
/// must say so without re-emitting 21 items on every 5-minute tick.
#[tokio::test]
async fn incremental_sync_is_empty_and_keeps_the_cursor() {
    let s = MockSource::new();
    let mut sink = VecSink(Vec::new());
    let cursor = s
        .sync(Some("tidewater-v1".into()), &mut sink)
        .await
        .expect("incremental sync");
    assert!(sink.0.is_empty());
    assert_eq!(cursor, "tidewater-v1");
}

/// Writes are recorded rather than performed, which is how later milestones
/// assert that the app issued the write it claims to have issued.
#[tokio::test]
async fn write_records_the_comment() {
    let s = MockSource::new();
    assert!(s.written_ops().is_empty());
    s.write(WriteOp::Comment {
        entity: "mock:PAY-231".into(),
        body: "on it".into(),
    })
    .await
    .expect("a declared op is accepted");
    let ops = s.written_ops();
    assert_eq!(ops.len(), 1);
    let WriteOp::Comment { entity, body } = &ops[0];
    assert_eq!((entity.as_str(), body.as_str()), ("mock:PAY-231", "on it"));
}

/// A faulted instance must fail the same way everywhere, `write` included --
/// otherwise a UI built against the mock never sees the failure path.
#[tokio::test]
async fn faults_reach_test_connection_sync_and_write() {
    for fault in [Fault::Unauthorized, Fault::Unreachable] {
        let s = MockSource::with_fault(fault);
        let err = s
            .test_connection()
            .await
            .expect_err("test_connection fails");
        assert!(is_mapped(fault, &err), "test_connection: {err:?}");
        let err = s
            .sync(None, &mut VecSink(Vec::new()))
            .await
            .expect_err("sync fails");
        assert!(is_mapped(fault, &err), "sync: {err:?}");
        let err = s
            .write(WriteOp::Comment {
                entity: "mock:PAY-231".into(),
                body: "on it".into(),
            })
            .await
            .expect_err("write fails");
        assert!(is_mapped(fault, &err), "write: {err:?}");
        assert!(s.written_ops().is_empty(), "a failed write records nothing");
    }

    let s = MockSource::new();
    assert!(s.test_connection().await.is_ok());
}

/// Is `err` the [`SourceError`] the SPI requires for `fault`?
fn is_mapped(fault: Fault, err: &SourceError) -> bool {
    matches!(
        (fault, err),
        (Fault::Unauthorized, SourceError::Unauthorized)
            | (Fault::Unreachable, SourceError::Unreachable(_))
    )
}

/// The opt-in tombstone: one deterministic deletion on a full sync, and none
/// at all from the plain mock.
///
/// This is the only way the deletion channel is reachable from the reference
/// adapter -- a real one reports deletions from its remote system, and until
/// there is one, anything exercising tombstones end to end has to have this to
/// sync.
#[tokio::test]
async fn the_opt_in_tombstone_reports_one_deterministic_deletion() {
    let s = MockSource::with_tombstone();
    let mut sink = VecSink(Vec::new());
    s.sync(None, &mut sink).await.expect("full sync");

    let deleted: Vec<_> = sink.0.iter().filter(|i| i.deleted).collect();
    assert_eq!(
        deleted.len(),
        1,
        "exactly one deletion, or it is not a fixture"
    );
    let gone = deleted[0];
    assert_eq!(
        gone.entity.to_string(),
        format!("mock:{}", knobas_source_mock::TOMBSTONED_KEY)
    );
    // A tombstone still carries its last-known title: that is what the UI has
    // left to render for something that vanished upstream.
    assert!(!gone.title.trim().is_empty());
    let declared = MockSource::new().descriptor().entity_kinds;
    assert!(
        declared.iter().any(|k| k.id == gone.kind),
        "a tombstone is an item like any other, and its kind {:?} must be declared",
        gone.kind
    );

    // Same again, byte for byte: a re-sync must be idempotent.
    let mut again = VecSink(Vec::new());
    s.sync(None, &mut again).await.expect("second full sync");
    assert_eq!(sink.0.len(), again.0.len());
    assert_eq!(
        serde_json::to_value(&again.0[again.0.len() - 1]).unwrap(),
        serde_json::to_value(gone).unwrap()
    );

    // ...and the fixture the mockups were drawn against is untouched.
    let mut plain = VecSink(Vec::new());
    MockSource::new().sync(None, &mut plain).await.unwrap();
    assert!(
        plain.0.iter().all(|i| !i.deleted),
        "the plain mock must report no deletions"
    );
    assert_eq!(plain.0.len() + 1, sink.0.len());
}

/// A tombstoning adapter is still an adapter: the contract battery holds for
/// it exactly as it does for the plain mock.
#[tokio::test]
async fn the_tombstoning_mock_passes_the_contract_battery() {
    battery(|fault| match fault {
        Fault::None => Box::new(MockSource::with_tombstone()) as Box<dyn knobas_source::Source>,
        other => Box::new(MockSource::with_fault(other)) as Box<dyn knobas_source::Source>,
    })
    .await;
}

/// P4: a successful connection reports who answered, so the Add-source flow
/// can say more than "Connected". Every field is optional and the mock fills
/// what a compiled-in fixture can honestly claim.
#[tokio::test]
async fn test_connection_reports_what_it_reached() {
    let info = MockSource::new()
        .test_connection()
        .await
        .expect("the mock always connects");
    assert_eq!(info.account.as_deref(), Some("mara.lindqvist"));
    assert!(
        info.server_version
            .as_deref()
            .is_some_and(|v| v.starts_with("knobas-source-mock")),
        "{:?}",
        info.server_version
    );
    // Nothing to expire: the fixture is compiled in.
    assert!(info.secret_expires_at.is_none());
}

/// P5: *Open in browser* needs a URL from the adapter, because deriving it in
/// the frontend would need exactly the per-adapter table §3a forbids.
#[tokio::test]
async fn every_emitted_item_carries_a_web_url() {
    let s = MockSource::new();
    let mut sink = VecSink(Vec::new());
    s.sync(None, &mut sink).await.expect("full sync");

    let ticket = sink
        .0
        .iter()
        .find(|i| i.entity.key == "PAY-231")
        .expect("PAY-231");
    assert_eq!(
        ticket.web_url.as_deref(),
        Some("https://tidewater.example/browse/PAY-231")
    );
    let pr = sink
        .0
        .iter()
        .find(|i| i.entity.key == "payout-service#142")
        .expect("PR #142");
    assert_eq!(
        pr.web_url.as_deref(),
        Some("https://tidewater.example/tidewater/payout-service/pulls/142")
    );
    assert!(
        sink.0.iter().all(|i| i.web_url.is_some()),
        "the fixture's world is fictional but complete: every item has a page"
    );
}

/// §4.2: one descriptor template per adapter kind, with `id == adapter_kind`.
#[tokio::test]
async fn the_descriptor_template_is_the_default_instance() {
    let template = knobas_source_mock::descriptor_template();
    assert_eq!(template.id, template.adapter_kind);
    assert_eq!(template.id, "mock");
}

/// Ruling P10's multi-instance form, proven rather than assumed: a second
/// instance emits into its own namespace, so two of them cannot overwrite each
/// other's rows.
#[tokio::test]
async fn build_honours_the_instance_id() {
    let instance = knobas_source::instance::SourceInstance {
        id: "mock-eu".to_owned(),
        kind: "mock".to_owned(),
        display_name: "Tidewater EU".to_owned(),
        base_url: String::new(),
        auth: None,
        secret: None,
        config: serde_json::json!({}),
    };
    let source = knobas_source_mock::build(instance.clone()).expect("built");
    assert_eq!(source.descriptor().id, "mock-eu");
    assert_eq!(source.descriptor().adapter_kind, "mock");

    let mut sink = VecSink(Vec::new());
    source.sync(None, &mut sink).await.expect("full sync");
    assert!(
        sink.0.iter().all(|item| item.entity.namespace == "mock-eu"),
        "every item must be namespaced to the instance that emitted it"
    );

    // And a second instance is a whole adapter, not a half one.
    battery(move |fault| match fault {
        Fault::None => knobas_source_mock::build(instance.clone()).expect("built"),
        other => Box::new(MockSource::with_fault(other)) as Box<dyn knobas_source::Source>,
    })
    .await;
}

/// An id that cannot be an entity namespace is refused at build time, not at
/// the first sync -- it is baked into every row the source would ever write.
#[tokio::test]
async fn build_refuses_an_unusable_instance_id() {
    for bad in ["Mock", "note", "mock:eu", ""] {
        let instance = knobas_source::instance::SourceInstance {
            id: bad.to_owned(),
            kind: "mock".to_owned(),
            display_name: "bad".to_owned(),
            base_url: String::new(),
            auth: None,
            secret: None,
            config: serde_json::json!({}),
        };
        assert!(
            knobas_source_mock::build(instance).is_err(),
            "{bad:?} must be refused"
        );
    }
}
