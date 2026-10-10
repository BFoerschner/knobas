//! The mock adapter proves it honours the SPI by running the shared contract
//! battery against itself, exactly as every real adapter does in its own
//! tests.
//!
//! The fixture assertions are deliberately literal: `fixtures/tidewater/work.json`
//! is the dataset every mockup was drawn against, so a silent edit to it would
//! quietly desynchronise the app from the design it is being built to match.

use knobas_source::contract::{Fault, VecSink, battery};
use knobas_source::{Capability, Source, SourceError, WriteOp};
use knobas_source_mock::MockSource;

/// How many items the whole fixture is, counted from the fixture rather than
/// remembered as a number.
///
/// Named once because two tests below need it -- the full sync and the
/// upgrade-path re-sync -- and because a seven-term sum written out twice is a
/// place for the two to drift apart. What it is *not* is an oracle for
/// [`items`](knobas_source_mock): it says which of the fixture's lists the
/// adapter is expected to emit, and a kind quietly dropped from `items` fails
/// the tests that use it. A kind quietly added is caught by the contract
/// battery, which refuses an item whose kind the descriptor does not declare.
fn corpus_size(f: &knobas_source_mock::Fixture) -> usize {
    f.tickets.len()
        + f.prs.len()
        + f.builds.len()
        + f.pages.len()
        + f.commits.len()
        + f.repos.len()
        + f.branches.len()
}

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

/// Who pressed Run, transcribed rather than invented.
///
/// `mockups/shared/dataset.md` says it once, in Mara's worklog draft:
/// "triggered build #1188 (11:45)". Nothing in the dataset names a person for
/// #1187 or #412, so those two record none and stay VCS-triggered downstream.
/// The mixture is the point: a fixture that gave every build the same
/// triggerer could not tell an adapter that reads authorship apart from one
/// that hard-codes it.
#[tokio::test]
async fn the_dataset_records_who_triggered_build_1188() {
    let f = knobas_source_mock::fixture();
    let by: Vec<(u32, Option<&str>)> = f
        .builds
        .iter()
        .map(|b| (b.num, b.triggered_by.as_deref()))
        .collect();
    assert_eq!(by, [(1188, Some("mara")), (1187, None), (412, None)]);
    assert!(
        f.person("mara").is_some(),
        "a triggerer is a Person::id, and a dangling one would index as nobody"
    );
}

/// ...and the item carries it, so a search for the person who started a build
/// finds the build. `author` is a `Person::id` here, the same as every other
/// kind the mock emits.
#[tokio::test]
async fn a_build_item_names_its_triggerer_as_the_author() {
    let mut sink = VecSink(Vec::new());
    MockSource::default()
        .sync(None, &mut sink)
        .await
        .expect("full sync");
    let author = |key: &str| {
        sink.0
            .iter()
            .find(|i| i.entity.key == key)
            .unwrap_or_else(|| panic!("{key} emitted"))
            .author
            .clone()
    };
    assert_eq!(author("Payout_Build#1188").as_deref(), Some("mara"));
    assert_eq!(
        author("Payout_IntegrationTests#1187"),
        None,
        "the dataset names nobody for #1187, and inventing one would be indexed as if it had"
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
async fn descriptor_declares_write_only_and_seven_kinds() {
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
    assert_eq!(
        kinds,
        ["ticket", "pr", "build", "page", "commit", "repo", "branch"]
    );
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
    assert_eq!(cursor, "tidewater-v3");
    assert_eq!(sink.0.len(), corpus_size(f));

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

/// A caller already at the current version has nothing to fetch -- and must say
/// so without re-emitting the whole corpus on every 5-minute tick.
///
/// Corrected rather than deleted (ADR-0011: verify, then correct): this said
/// "the fixture never changes", which the repo's own history falsifies --
/// `fixtures/tidewater/work.json` was edited in `06cc066` and again in
/// `1d45f74`. The *behaviour* below is still right, because the emptiness
/// turns on the cursor matching [`CURSOR`], not on the fixture being frozen;
/// the pair of it is
/// `a_cursor_from_an_older_fixture_re_syncs_the_whole_corpus`, and an edit
/// that widens the fixture is obliged to bump the suffix so that a stored
/// cursor lands there instead of here.
#[tokio::test]
async fn incremental_sync_is_empty_and_keeps_the_cursor() {
    let s = MockSource::new();
    let mut sink = VecSink(Vec::new());
    let cursor = s
        .sync(Some("tidewater-v3".into()), &mut sink)
        .await
        .expect("incremental sync");
    assert!(sink.0.is_empty());
    assert_eq!(cursor, "tidewater-v3");
}

/// The upgrade path the cursor's versioning exists to provide (#234): a
/// profile still holding the position an *older* fixture handed out is re-sent
/// the whole corpus, new fields and all, and then settles.
///
/// `"tidewater-v1"` is not an invented string -- it is the cursor every demo
/// profile created before the fixture gained projects (#230) actually has
/// stored. `"tidewater-v2"` is the second such position, held by every
/// profile created between #234 and #537; the case below drives the older of
/// the two, because a reader that repairs from `v1` repairs from anything
/// that is not [`CURSOR`] -- the sync branches on equality, not on order.
///
/// Asserting the corpus rather than the constant is the point: that a
/// re-sync *happens* is the behaviour, and a test reading `CURSOR` back would
/// pass just as happily while every such profile refetched nothing for ever.
#[tokio::test]
async fn a_cursor_from_an_older_fixture_re_syncs_the_whole_corpus() {
    let f = knobas_source_mock::fixture();
    let s = MockSource::new();
    let mut sink = VecSink(Vec::new());
    let cursor = s
        .sync(Some("tidewater-v1".into()), &mut sink)
        .await
        .expect("a sync from a stale cursor");
    assert_eq!(
        sink.0.len(),
        corpus_size(f),
        "a profile stored at an older fixture version must be re-sent everything"
    );
    // ...carrying what the older fixture had no way to send. Widening the
    // fixture without moving the cursor is exactly the defect #234 fixes, and
    // it is invisible to a length assertion alone: the corpus was already 21
    // items before the project existed.
    let ticket = sink
        .0
        .iter()
        .find(|i| i.entity.key == "PAY-231")
        .expect("PAY-231 re-emitted");
    assert_eq!(ticket.payload["fields"]["project"]["key"], "PAY");

    assert_ne!(
        cursor, "tidewater-v1",
        "a re-sync that hands the stale position back would repeat for ever"
    );
    let mut settled = VecSink(Vec::new());
    s.sync(Some(cursor), &mut settled)
        .await
        .expect("the next tick");
    assert!(
        settled.0.is_empty(),
        "one full re-sync must be enough -- the upgrade settles, it does not loop"
    );
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
    let WriteOp::Comment { entity, body } = &ops[0] else {
        panic!(
            "the mock recorded {:?} rather than the comment it was handed",
            ops[0]
        );
    };
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
        (Fault::Unauthorized, SourceError::Unauthorized { .. })
            | (Fault::Unreachable, SourceError::Unreachable(_))
    )
}

/// The opt-in tombstone: one deterministic deletion on a full sync, and none
/// at all from the plain mock.
///
/// This is the only way the deletion channel is reachable from the reference
/// adapter. A real one reports deletions from its remote system (Gitea's
/// `branch_tombstone` does), which needs that system running; anything
/// exercising tombstones against the demo profile has only this to sync.
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
        account: None,
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
            account: None,
            config: serde_json::json!({}),
        };
        assert!(
            knobas_source_mock::build(instance).is_err(),
            "{bad:?} must be refused"
        );
    }
}

/// `build` honours `instance.config`, which is what lets a test drive a
/// **compiled-in** adapter that fails.
///
/// The registry hands out adapters by kind and hands `build` the stored config;
/// with the config ignored, the only faulted mock in existence was one a test
/// constructed by hand, so nothing that goes *through* the registry could be
/// made to fail. That is the narrow remainder of PR #24's finding, carried on
/// the M0/M1 ledger (#48).
///
/// Both knobs, because `MockSource` has exactly two and honouring one would be
/// the same gap in a smaller shape. Every `Fault` is walked rather than one:
/// the mapping is a match, and a spelling that fell through to `None` would be
/// a config that silently produces a healthy source.
#[tokio::test]
async fn build_honours_the_fault_and_tombstone_in_its_config() {
    for (spelling, fault) in [
        ("none", Fault::None),
        ("unauthorized", Fault::Unauthorized),
        ("unreachable", Fault::Unreachable),
    ] {
        let source =
            knobas_source_mock::build(configured(serde_json::json!({ "fault": spelling })))
                .expect("a known fault spelling builds");

        let reported = source.test_connection().await;
        match fault {
            Fault::None => assert!(reported.is_ok(), "{spelling} must build a healthy mock"),
            Fault::Unauthorized => assert!(matches!(
                reported,
                Err(knobas_source::SourceError::Unauthorized { .. })
            )),
            Fault::Unreachable => assert!(matches!(
                reported,
                Err(knobas_source::SourceError::Unreachable(_))
            )),
        }
    }

    // The tombstone knob: a full sync that also reports one item as deleted,
    // which is the only way to produce a run whose `deleted` differs from its
    // `swept`.
    let source = knobas_source_mock::build(configured(serde_json::json!({ "tombstone": true })))
        .expect("built");
    let mut sink = VecSink(Vec::new());
    source.sync(None, &mut sink).await.expect("full sync");
    assert!(
        sink.0.iter().any(|item| item.deleted
            && item.entity.to_string() == format!("mock:{}", knobas_source_mock::TOMBSTONED_KEY)),
        "the tombstone knob must reach the items the sync emits"
    );

    // And an absent config is the healthy default it has always been -- **both**
    // knobs. Only the fault half was pinned at first, and flipping
    // `tombstone`'s default to `true` left all twenty tests green: every mock
    // the app's registry builds would then have reported PAY-198 as deleted on
    // every full sync, tombstoning an entity nothing asked to be tombstoned.
    // `full_sync_emits_every_work_item` cannot see it -- that one goes through
    // `MockSource::new()`, not `build`.
    let plain = knobas_source_mock::build(configured(serde_json::json!({}))).expect("built");
    assert!(plain.test_connection().await.is_ok());
    let mut untouched = VecSink(Vec::new());
    plain.sync(None, &mut untouched).await.expect("full sync");
    assert!(
        !untouched.0.iter().any(|item| item.deleted),
        "a source added through the form must not delete anything"
    );
}

/// A config knobas cannot read is a configuration mistake, refused at build
/// time like the two `build` already refuses -- not a source that silently
/// builds healthy and syncs.
///
/// **The misspelled keys are the cases with teeth.** A key-by-key reader answers
/// `None` for `faultt` exactly as it does for an absent `fault`, so a test author
/// who typos one gets a green run against a *healthy* adapter -- which is the
/// failure honouring the config exists to prevent, arriving through the door
/// that was meant to close it. Same for a `config` that is not an object at all.
#[tokio::test]
async fn build_refuses_a_config_it_cannot_read() {
    for bad in [
        // Values of the wrong shape.
        serde_json::json!({ "fault": "flaky" }),
        serde_json::json!({ "fault": 7 }),
        serde_json::json!({ "tombstone": "yes" }),
        // Keys this adapter's schema does not describe -- including the two
        // near-misses of the keys it does.
        serde_json::json!({ "faultt": "unauthorized" }),
        serde_json::json!({ "Fault": "unauthorized" }),
        serde_json::json!({ "tombstoned": true }),
        serde_json::json!({ "fault": "unauthorized", "flavor": "datacenter" }),
        // Not an object.
        serde_json::json!("unauthorized"),
        serde_json::json!([{ "fault": "unauthorized" }]),
        serde_json::json!(null),
    ] {
        assert!(
            knobas_source_mock::build(configured(bad.clone())).is_err(),
            "{bad} must be refused"
        );
    }
}

fn configured(config: serde_json::Value) -> knobas_source::instance::SourceInstance {
    knobas_source::instance::SourceInstance {
        id: "mock".to_owned(),
        kind: "mock".to_owned(),
        display_name: "Tidewater".to_owned(),
        base_url: String::new(),
        auth: None,
        secret: None,
        account: None,
        config,
    }
}

/// The repositories and branches the demo profile is asked for (#537).
///
/// The corpus carried neither until this landed: `fixtures/tidewater/work.json`
/// has held three repos and three branches since the transcription, and `items`
/// walked tickets, PRs, builds, pages and commits past them. So the `--demo`
/// profile had no repo entity in it, and everything hanging off one -- the
/// checkout panel, *Open in VS Code*, `open-in-editor`'s desktop witness -- had
/// nothing to open (#501, #525).
///
/// **The key grammar is the thing under test, not the count.** Interfaces §4.2
/// fixes it per source, and `knobas_app::checkout`'s `repo_of` finds a branch's
/// repository as *the longest repo id in the same source that the branch id
/// starts with*. A branch key that did not extend its repo's would leave every
/// branch detail answering "no checkout" -- with the repo, the clone and the
/// setting all correct -- so the prefix is asserted here rather than assumed
/// from the format string that builds it.
#[tokio::test]
async fn full_sync_emits_the_fixtures_repos_and_branches() {
    let f = knobas_source_mock::fixture();
    let s = MockSource::new();
    let mut sink = VecSink(Vec::new());
    s.sync(None, &mut sink).await.expect("full sync");

    let repos: Vec<(&str, &str)> = sink
        .0
        .iter()
        .filter(|i| i.kind == "repo")
        .map(|i| (i.entity.key.as_str(), i.title.as_str()))
        .collect();
    assert_eq!(
        repos,
        [
            ("payout-service", "payout-service"),
            ("ledger-api", "ledger-api"),
            ("ops-runbooks", "ops-runbooks"),
        ],
        "every repo the fixture holds, keyed and titled by its name"
    );

    let payout = sink
        .0
        .iter()
        .find(|i| i.entity.key == "payout-service")
        .expect("payout-service emitted");
    assert_eq!(payout.entity.to_string(), "mock:payout-service");
    // The URL a clone's `origin` is matched against: `knobas_core::checkout`
    // reduces both to host + owner/repo, and the desktop driver writes exactly
    // this remote into the `.git/config` it plants.
    assert_eq!(
        payout.web_url.as_deref(),
        Some("https://tidewater.example/tidewater/payout-service")
    );
    // The transcription verbatim, as every other kind carries it.
    assert_eq!(payout.payload["lang"], "Rust");
    assert_eq!(payout.payload["default_branch"], "main");
    assert!(!payout.deleted);

    let branches: Vec<&str> = sink
        .0
        .iter()
        .filter(|i| i.kind == "branch")
        .map(|i| i.entity.key.as_str())
        .collect();
    assert_eq!(
        branches,
        [
            "payout-service@refs/heads/main",
            "payout-service@refs/heads/feature/PAY-231-sepa-retry",
            "payout-service@refs/heads/fix/PAY-228-partial-refund-drift",
        ]
    );
    let sepa = sink
        .0
        .iter()
        .find(|i| i.entity.key == "payout-service@refs/heads/feature/PAY-231-sepa-retry")
        .expect("the SEPA branch emitted");
    assert_eq!(sepa.title, "feature/PAY-231-sepa-retry");
    assert_eq!(sepa.payload["ticket"], "PAY-231");
    assert_eq!(
        sepa.web_url.as_deref(),
        Some(
            "https://tidewater.example/tidewater/payout-service/src/branch/\
             feature/PAY-231-sepa-retry"
        )
    );
    // The rule `repo_of` walks, stated as a property of every emitted branch
    // rather than of the one above: each has exactly one repo whose id it
    // extends, and that repo is the one the fixture names.
    for branch in sink.0.iter().filter(|i| i.kind == "branch") {
        let repo = f
            .branches
            .iter()
            .find(|b| branch.entity.key.starts_with(&format!("{}@", b.repo)))
            .map(|b| b.repo.as_str())
            .expect("a branch key extends its repository's");
        let owners: Vec<&str> = sink
            .0
            .iter()
            .filter(|i| i.kind == "repo")
            .filter(|i| branch.entity.to_string().starts_with(&i.entity.to_string()))
            .map(|i| i.entity.key.as_str())
            .collect();
        assert_eq!(
            owners,
            [repo],
            "{} must extend exactly one repo id, its own",
            branch.entity
        );
    }
}

/// The demo button's subtitle names the size of the corpus it loads, and
/// nothing made that true until this.
///
/// `app/src/lib/sources/FirstRun.svelte` offers *Load the Tidewater dataset*
/// under `21 fixture items · a 23-asset estate · no network, no credential`.
/// That sentence was written when a full sync was 21 items, is the first thing
/// a person opening a demo build reads, and no test on either side of the
/// bridge could see it: #537 widened the corpus to 27 and found the `21` by
/// grep. The estate's own count is pinned by `demo.rs`'s
/// `the_demo_load_brings_the_real_estate_and_a_second_start_changes_nothing`,
/// which reads `testenv/hetzner/estate.json`; this is the work half's.
///
/// The oracle is a **real sync through the adapter**, not a sum of fixture
/// array lengths: what the subtitle claims is what the button produces, and an
/// `items` that stopped emitting a kind would have to move the sentence too.
/// `include_str!` rather than a path read, so the file being renamed or moved
/// fails the build here rather than passing an assertion over an empty string.
#[tokio::test]
async fn the_first_run_subtitle_names_the_size_of_the_corpus_it_loads() {
    const FIRST_RUN: &str = include_str!("../../../../app/src/lib/sources/FirstRun.svelte");
    let mut sink = VecSink(Vec::new());
    MockSource::new().sync(None, &mut sink).await.expect("sync");

    let claim = format!("{} fixture items", sink.0.len());
    assert!(
        FIRST_RUN.contains(&claim),
        "FirstRun.svelte must offer the demo load as {claim:?}; \
         a full sync emits {} items and the subtitle says otherwise",
        sink.0.len()
    );
}
