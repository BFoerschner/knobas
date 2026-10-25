//! The read of one item by id, a **refresh** (#2098, ADR-0031), against the
//! docker-free fake: a pull request and an issue answer as the full sync
//! emits them, an id Gitea answers 404 is *gone*, a fault is an error, and the
//! kinds this adapter does not read by id answer *not offered*.
//! `tests/live_gitea.rs` holds each to the real container (ADR-0013); where
//! the two disagree, the fake is wrong.

use knobas_source::contract::VecSink;
use knobas_source::{Capability, RefreshAnswer, Source, SyncItem};
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use crate::support::{Fake, State, source};

const PR: &str = "gitea:tidewater/payout-service#142";
const TICKET: &str = "gitea:tidewater/payout-service/issues/7";

async fn full(source: &dyn Source) -> Vec<SyncItem> {
    let mut sink = VecSink(Vec::new());
    source.sync(None, &mut sink).await.expect("the sync runs");
    sink.0
}

fn emitted<'a>(items: &'a [SyncItem], id: &str) -> &'a SyncItem {
    items
        .iter()
        .find(|item| item.entity.to_string() == id)
        .unwrap_or_else(|| panic!("the sync emitted no {id}: {items:#?}"))
}

fn item(answer: RefreshAnswer, id: &str) -> SyncItem {
    match answer {
        RefreshAnswer::Item(item) => item,
        other => panic!("{id} answered {other:?}, not the item"),
    }
}

/// The whole item, not the declared paths alone: the discussion is in its
/// `body_text`, which a refresh after a comment exists to bring in.
#[tokio::test]
async fn a_pull_request_and_an_issue_are_read_back_as_the_sync_emits_them() {
    let fake = Fake::start(&State::tidewater()).await;
    let source = source(fake.base_url(), json!({}));
    let items = full(&*source).await;

    for id in [PR, TICKET] {
        let read = item(source.refresh(id).await.expect("the read succeeds"), id);
        let synced = emitted(&items, id);
        assert_eq!(
            (&read.entity, &read.kind, &read.title, &read.body_text),
            (&synced.entity, &synced.kind, &synced.title, &synced.body_text),
            "{id}"
        );
        assert_eq!(read.payload, synced.payload, "{id}");
        assert_eq!(
            (&read.author, read.updated_at, &read.web_url, read.deleted),
            (&synced.author, synced.updated_at, &synced.web_url, false),
            "{id}"
        );
    }
}

#[tokio::test]
async fn the_descriptor_declares_the_refresh() {
    let fake = Fake::start(&State::tidewater()).await;
    let source = source(fake.base_url(), json!({}));
    assert!(source.descriptor().refreshes());
    assert!(
        source
            .descriptor()
            .capabilities
            .contains(&Capability::Refresh)
    );
}

/// Gitea answers a deleted issue, and a pull request in a deleted
/// repository, 404: what the sync's probes read as *gone* (#724).
#[tokio::test]
async fn an_item_gitea_answers_404_is_gone() {
    let fake = Fake::start(&State::tidewater()).await;
    for route in [
        "/api/v1/repos/tidewater/payout-service/issues/7",
        "/api/v1/repos/tidewater/payout-service/pulls/142",
    ] {
        Mock::given(method("GET"))
            .and(path(route))
            .respond_with(ResponseTemplate::new(404).set_body_json(json!({ "message": "not found" })))
            .with_priority(1)
            .mount(fake.server())
            .await;
    }
    let source = source(fake.base_url(), json!({}));
    for id in [PR, TICKET] {
        assert!(
            matches!(source.refresh(id).await, Ok(RefreshAnswer::Gone)),
            "{id}"
        );
    }
}

/// A pull request whose discussion read answers 500 and whose own read then
/// answers 404 was deleted between the two reads: *gone*, as the sync skips
/// it (#724).
#[tokio::test]
async fn a_pull_request_deleted_before_its_discussion_is_read_is_gone() {
    let mut state = State::tidewater();
    state
        .vanished_pulls
        .insert("tidewater/payout-service#142".to_owned());
    let fake = Fake::start(&state).await;
    // The first read finds it; the confirming read after the 500 does not.
    Mock::given(method("GET"))
        .and(path("/api/v1/repos/tidewater/payout-service/pulls/142"))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            &state.pulls["tidewater/payout-service"][1],
        ))
        .up_to_n_times(1)
        .with_priority(1)
        .mount(fake.server())
        .await;
    let source = source(fake.base_url(), json!({}));
    assert!(matches!(
        source.refresh(PR).await,
        Ok(RefreshAnswer::Gone)
    ));
}

/// A fault is never *gone*: the engine would tombstone an item that is
/// still there.
#[tokio::test]
async fn a_server_fault_on_the_read_is_an_error_not_gone() {
    let fake = Fake::start(&State::tidewater()).await;
    Mock::given(method("GET"))
        .and(path("/api/v1/repos/tidewater/payout-service/issues/7"))
        .respond_with(ResponseTemplate::new(503).set_body_json(json!({ "message": "busy" })))
        .with_priority(1)
        .mount(fake.server())
        .await;
    let source = source(fake.base_url(), json!({}));
    let error = source.refresh(TICKET).await.expect_err("a fault");
    assert_eq!(error.status(), Some(503), "{error:?}");
}

/// An issue number Gitea answers with a pull request is not a ticket: the
/// sync never emits it as one, so the ticket of that id is gone.
#[tokio::test]
async fn an_issue_id_naming_a_pull_request_is_gone() {
    let fake = Fake::start(&State::tidewater()).await;
    let mut record = crate::support::issue(142, "SEPA retry", "open", &[], "2026-08-22T10:20:00Z");
    record["pull_request"] = json!({ "merged": false });
    Mock::given(method("GET"))
        .and(path("/api/v1/repos/tidewater/payout-service/issues/142"))
        .respond_with(ResponseTemplate::new(200).set_body_json(record))
        .with_priority(1)
        .mount(fake.server())
        .await;
    let source = source(fake.base_url(), json!({}));
    assert!(matches!(
        source
            .refresh("gitea:tidewater/payout-service/issues/142")
            .await,
        Ok(RefreshAnswer::Gone)
    ));
}

/// Repositories, branches and commits are not read by id: the app runs the
/// source for them, and asks Gitea nothing first.
#[tokio::test]
async fn a_repository_a_branch_and_a_commit_are_not_offered() {
    let fake = Fake::start(&State::tidewater()).await;
    let source = source(fake.base_url(), json!({}));
    let before = fake.requests().await;
    for id in [
        "gitea:tidewater/payout-service",
        "gitea:tidewater/payout-service@refs/heads/main",
        "gitea:tidewater/payout-service@1111111111111111111111111111111111111111",
    ] {
        assert!(
            matches!(source.refresh(id).await, Ok(RefreshAnswer::NotOffered)),
            "{id}"
        );
    }
    assert_eq!(fake.requests().await, before, "nothing was asked of Gitea");
}

/// For a pull request nobody was asked to review, Gitea 1.27.3's listing
/// answers `requested_reviewers: null` and its read of that one pull request
/// `[]`; Forgejo 17 answers `[]` to both (measured by the live battery's
/// clause 13 on each, #2098). A run and a refresh store `[]` either way, so
/// the stored record does not depend on which read brought it.
#[tokio::test]
async fn a_pull_request_nobody_was_asked_to_review_is_stored_alike_by_a_run_and_a_refresh() {
    for listed_as in [json!(null), json!([])] {
        let mut state = State::tidewater();
        let listed = &mut state.pulls.get_mut("tidewater/payout-service").unwrap()[1];
        listed["requested_reviewers"] = listed_as.clone();
        let mut read_whole = listed.clone();
        read_whole["requested_reviewers"] = json!([]);
        let fake = Fake::start(&state).await;
        Mock::given(method("GET"))
            .and(path("/api/v1/repos/tidewater/payout-service/pulls/142"))
            .respond_with(ResponseTemplate::new(200).set_body_json(read_whole))
            .with_priority(1)
            .mount(fake.server())
            .await;
        let source = source(fake.base_url(), json!({}));
        let items = full(&*source).await;
        let synced = emitted(&items, PR);
        let read = item(source.refresh(PR).await.expect("the read succeeds"), PR);
        assert_eq!(read.payload, synced.payload, "listed as {listed_as}");
        assert_eq!(synced.payload["requested_reviewers"], json!([]), "listed as {listed_as}");
    }
}
