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
#[tokio::test]
async fn descriptor_declares_search_write_and_five_kinds() {
    let d = MockSource::new().descriptor();
    assert_eq!(d.id, "mock");
    assert_eq!(d.adapter_kind, "mock");
    assert_eq!(d.capabilities, [Capability::Search, Capability::Write]);
    assert_eq!(d.write_ops, ["comment"]);
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
