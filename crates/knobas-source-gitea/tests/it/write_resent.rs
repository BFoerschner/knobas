//! A guarded write whose first attempt landed but whose answer was lost
//! (issue #1129).
//!
//! Each fake here acts on the first request and answers it with a gateway's
//! `504`, so the answer is lost; the repeat -- the transport's retry, or the
//! write queue's later resend -- meets Gitea's refusal of a second one. When
//! Gitea now holds exactly what this write would have made, the write is
//! delivered; otherwise the refusal stands.

use knobas_source::{MergeStyle, Source, SourceError, WriteOp};
use serde_json::{Value, json};
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::support::{TOKEN, instance};

const REPO: &str = "/api/v1/repos/tidewater/payout-service";

fn source(server: &MockServer) -> Box<dyn Source> {
    knobas_source_gitea::build(instance(server.uri(), TOKEN, json!({})))
        .expect("the adapter builds")
}

/// A Gitea that acts on the first `POST` to `at` but whose answer is lost to
/// a gateway's `504`, and that refuses every repeat with `status` and
/// `sentence`, as it does a second one.
async fn lands_then_refuses(at: &str, status: u16, sentence: &str) -> MockServer {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path(format!("{REPO}{at}")))
        .respond_with(ResponseTemplate::new(504))
        .up_to_n_times(1)
        .with_priority(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(format!("{REPO}{at}")))
        .respond_with(ResponseTemplate::new(status).set_body_json(json!({
            "message": sentence,
            "url": "https://gitea.example/api/swagger"
        })))
        .with_priority(2)
        .mount(&server)
        .await;
    server
}

/// Answer `GET <repo><at>` with `body`.
async fn holds(server: &MockServer, at: &str, body: Value) {
    Mock::given(method("GET"))
        .and(path(format!("{REPO}{at}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(server)
        .await;
}

/// How many times the write itself was sent.
async fn posts(server: &MockServer) -> usize {
    server
        .received_requests()
        .await
        .expect("the mock server records requests")
        .iter()
        .filter(|r| r.method.as_str() == "POST")
        .count()
}

fn create_branch() -> WriteOp {
    WriteOp::CreateBranch {
        entity: "gitea:tidewater/payout-service".to_owned(),
        name: "feature/PAY-231-sepa-retry".to_owned(),
        from_ref: "main".to_owned(),
    }
}

/// The branch exists, and at the commit `main` names: it is this write's
/// branch, delivered, not Gitea's 409.
#[tokio::test]
async fn a_branch_created_whose_answer_was_lost_is_delivered() {
    let server = lands_then_refuses("/branches", 409, "The branch already exists.").await;
    holds(
        &server,
        "/branches/main",
        json!({ "name": "main", "commit": { "id": "c90d11aa" } }),
    )
    .await;
    holds(
        &server,
        "/branches/feature%2FPAY-231-sepa-retry",
        json!({ "name": "feature/PAY-231-sepa-retry", "commit": { "id": "c90d11aa" } }),
    )
    .await;

    source(&server)
        .write(create_branch())
        .await
        .expect("the branch this write made is there");
    assert_eq!(
        posts(&server).await,
        2,
        "the lost one and the refused repeat"
    );
}

/// The queue's resend (ADR-0012), not the transport's: every attempt of the
/// first flush lost its answer, so the write waited; the next flush meets
/// Gitea's 409, and the branch is there at `main`'s commit.
#[tokio::test]
async fn a_branch_the_queue_resends_after_its_answers_were_lost_is_delivered() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path(format!("{REPO}/branches")))
        .respond_with(ResponseTemplate::new(504))
        .up_to_n_times(knobas_http::MAX_ATTEMPTS.into())
        .with_priority(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(format!("{REPO}/branches")))
        .respond_with(
            ResponseTemplate::new(409)
                .set_body_json(json!({ "message": "The branch already exists." })),
        )
        .with_priority(2)
        .mount(&server)
        .await;
    holds(
        &server,
        "/branches/main",
        json!({ "name": "main", "commit": { "id": "c90d11aa" } }),
    )
    .await;
    holds(
        &server,
        "/branches/feature%2FPAY-231-sepa-retry",
        json!({ "name": "feature/PAY-231-sepa-retry", "commit": { "id": "c90d11aa" } }),
    )
    .await;

    let gitea = source(&server);
    let first = gitea
        .write(create_branch())
        .await
        .expect_err("no answer came back");
    assert_eq!(first.status(), Some(504), "{first:?}");
    gitea
        .write(create_branch())
        .await
        .expect("the queue's resend finds the branch the first flush made");
}

/// A branch of that name at another commit is somebody else's: Gitea's 409
/// stands.
#[tokio::test]
async fn a_branch_of_that_name_at_another_commit_is_still_refused() {
    let server = lands_then_refuses("/branches", 409, "The branch already exists.").await;
    holds(
        &server,
        "/branches/main",
        json!({ "name": "main", "commit": { "id": "c90d11aa" } }),
    )
    .await;
    holds(
        &server,
        "/branches/feature%2FPAY-231-sepa-retry",
        json!({ "name": "feature/PAY-231-sepa-retry", "commit": { "id": "77ab01ff" } }),
    )
    .await;

    let error = source(&server)
        .write(create_branch())
        .await
        .expect_err("the branch is not this write's");
    assert_eq!(error.status(), Some(409), "{error:?}");
    assert!(
        matches!(&error, SourceError::Protocol { message, .. } if message.contains("already exists")),
        "{error:?}"
    );
}

fn create_pull_request() -> WriteOp {
    WriteOp::CreatePullRequest {
        entity: "gitea:tidewater/payout-service".to_owned(),
        title: "SEPA retry with exponential backoff".to_owned(),
        body: "Retries transient PSP errors.".to_owned(),
        head: "feature/PAY-231-sepa-retry".to_owned(),
        base: "main".to_owned(),
    }
}

/// An open pull request opened by `bjoern`, as Gitea's listing answers it.
fn open_pull(number: u64, title: &str, head: &str, base: &str) -> Value {
    open_pull_by(number, title, head, base, "bjoern")
}

/// An open pull request opened by `by`.
fn open_pull_by(number: u64, title: &str, head: &str, base: &str, by: &str) -> Value {
    json!({
        "number": number,
        "state": "open",
        "title": title,
        "head": { "ref": head, "label": head },
        "base": { "ref": base, "label": base },
        "user": { "login": by },
    })
}

/// This token is `bjoern`'s.
async fn token_is_bjoern(server: &MockServer) {
    Mock::given(method("GET"))
        .and(path("/api/v1/user"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "login": "bjoern" })))
        .mount(server)
        .await;
}

const ALREADY_OPEN: &str = "pull request already exists for these targets [id: 145, issue_id: 160, head_repo_id: 7, \
     base_repo_id: 7, head_branch: feature/PAY-231-sepa-retry, base_branch: main]";

/// An open pull request from this head into this base, with this write's
/// title, is the one this write opened: delivered.
#[tokio::test]
async fn a_pull_request_opened_whose_answer_was_lost_is_delivered() {
    let server = lands_then_refuses("/pulls", 409, ALREADY_OPEN).await;
    holds(
        &server,
        "/pulls",
        json!([
            open_pull(146, "Bump deps", "chore/deps", "main"),
            open_pull(
                145,
                "SEPA retry with exponential backoff",
                "feature/PAY-231-sepa-retry",
                "main"
            ),
        ]),
    )
    .await;
    token_is_bjoern(&server).await;

    let receipt = source(&server)
        .write(create_pull_request())
        .await
        .expect("the pull request this write opened is there");
    assert_eq!(
        posts(&server).await,
        2,
        "the lost one and the refused repeat"
    );
    // The receipt names the one it found, as a first answer would (#1949).
    assert_eq!(
        receipt.remote_id.as_deref(),
        Some("tidewater/payout-service#145")
    );
}

/// The open pull request for the pair carries another title: somebody
/// else's, and Gitea's 409 stands.
#[tokio::test]
async fn an_open_pull_request_with_another_title_is_still_refused() {
    let server = lands_then_refuses("/pulls", 409, ALREADY_OPEN).await;
    holds(
        &server,
        "/pulls",
        json!([open_pull(
            145,
            "WIP retry",
            "feature/PAY-231-sepa-retry",
            "main"
        )]),
    )
    .await;

    let error = source(&server)
        .write(create_pull_request())
        .await
        .expect_err("the pull request is not this write's");
    assert_eq!(error.status(), Some(409), "{error:?}");
}

/// A colleague opened the pull request for the pair under the same title:
/// their body is on it, not this write's, and Gitea's 409 stands.
#[tokio::test]
async fn a_pull_request_somebody_else_opened_under_this_title_is_still_refused() {
    let server = lands_then_refuses("/pulls", 409, ALREADY_OPEN).await;
    holds(
        &server,
        "/pulls",
        json!([open_pull_by(
            145,
            "SEPA retry with exponential backoff",
            "feature/PAY-231-sepa-retry",
            "main",
            "marta"
        )]),
    )
    .await;
    token_is_bjoern(&server).await;

    let error = source(&server)
        .write(create_pull_request())
        .await
        .expect_err("the pull request is marta's");
    assert_eq!(error.status(), Some(409), "{error:?}");
}

fn merge() -> WriteOp {
    WriteOp::MergePullRequest {
        entity: "gitea:tidewater/payout-service#142".to_owned(),
        style: MergeStyle::Squash,
        delete_branch: false,
        message: None,
    }
}

/// A Gitea where pull request 142 was merged by `by`, and this token is
/// `bjoern`'s.
async fn merged_by(by: &str) -> MockServer {
    let server = lands_then_refuses("/pulls/142/merge", 405, "The PR is already merged").await;
    holds(
        &server,
        "/pulls/142",
        json!({ "number": 142, "state": "closed", "merged": true, "merged_by": { "login": by } }),
    )
    .await;
    token_is_bjoern(&server).await;
    server
}

/// The pull request is merged, and by this account: this write's merge,
/// delivered, not Gitea's 405 *already merged*.
#[tokio::test]
async fn a_merge_whose_answer_was_lost_is_delivered() {
    let server = merged_by("bjoern").await;
    source(&server)
        .write(merge())
        .await
        .expect("the merge this write made is there");
    assert_eq!(
        posts(&server).await,
        2,
        "the lost one and the refused repeat"
    );
}

/// Merged by somebody else: not this write's merge, and Gitea's 405 stands.
#[tokio::test]
async fn a_pull_request_somebody_else_merged_is_still_refused() {
    let server = merged_by("marta").await;
    let error = source(&server)
        .write(merge())
        .await
        .expect_err("the merge is not this write's");
    assert_eq!(error.status(), Some(405), "{error:?}");
}

/// `n` open pull requests for pairs this write did not ask about.
fn others(n: u64, from: u64) -> Value {
    json!(
        (from..from + n)
            .map(|number| open_pull(number, "Bump deps", &format!("chore/deps-{number}"), "main"))
            .collect::<Vec<_>>()
    )
}

/// Answer `GET <repo>/pulls?page=<page>` with `body`, and every other page
/// with the empty page that ends Gitea's listings.
async fn pulls_page(server: &MockServer, page: u32, body: Value) {
    Mock::given(method("GET"))
        .and(path(format!("{REPO}/pulls")))
        .and(query_param("page", page.to_string().as_str()))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .with_priority(1)
        .mount(server)
        .await;
}

async fn pulls_pages_end_empty(server: &MockServer) {
    holds(server, "/pulls", json!([])).await;
}

/// How many pages of open pull requests the readback asked for.
async fn pull_pages_read(server: &MockServer) -> usize {
    server
        .received_requests()
        .await
        .expect("the mock server records requests")
        .iter()
        .filter(|r| r.method.as_str() == "GET" && r.url.path() == format!("{REPO}/pulls"))
        .count()
}

/// Issue #1625: a Gitea capped at 30 records a page answers a page shorter
/// than the 50 the readback asks for, with more behind it. The open pull
/// request this write made is on page 2, so a readback that ends on a short
/// page reads it as a refusal although it landed.
#[tokio::test]
async fn a_pull_request_on_the_second_page_of_a_capped_server_is_delivered() {
    let server = lands_then_refuses("/pulls", 409, ALREADY_OPEN).await;
    pulls_page(&server, 1, others(30, 200)).await;
    pulls_page(
        &server,
        2,
        json!([open_pull(
            145,
            "SEPA retry with exponential backoff",
            "feature/PAY-231-sepa-retry",
            "main"
        )]),
    )
    .await;
    pulls_pages_end_empty(&server).await;
    token_is_bjoern(&server).await;

    source(&server)
        .write(create_pull_request())
        .await
        .expect("the pull request this write opened is on page 2");
}

/// The readback ends on an empty page, and reads no further.
#[tokio::test]
async fn the_readback_stops_on_an_empty_page() {
    let server = lands_then_refuses("/pulls", 409, ALREADY_OPEN).await;
    pulls_page(&server, 1, others(30, 200)).await;
    pulls_pages_end_empty(&server).await;

    let error = source(&server)
        .write(create_pull_request())
        .await
        .expect_err("no open pull request is this write's");
    assert_eq!(error.status(), Some(409), "{error:?}");
    assert_eq!(
        pull_pages_read(&server).await,
        2,
        "page 1 and the empty page 2"
    );
}

/// The readback honours its page cap: ten full pages of other pull requests
/// are read three pages deep, and Gitea's refusal stands.
#[tokio::test]
async fn the_readback_stops_at_its_page_cap() {
    let server = lands_then_refuses("/pulls", 409, ALREADY_OPEN).await;
    for page in 1..=10 {
        pulls_page(&server, page, others(30, u64::from(page) * 100)).await;
    }
    pulls_pages_end_empty(&server).await;

    let error = source(&server)
        .write(create_pull_request())
        .await
        .expect_err("no open pull request is this write's");
    assert_eq!(error.status(), Some(409), "{error:?}");
    assert_eq!(pull_pages_read(&server).await, 3, "the cap is three pages");
}
