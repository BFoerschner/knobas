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
use wiremock::matchers::{method, path};
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
    let server = MockServer::start().await;
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
    holds(&server, "/branches/main", json!({ "name": "main", "commit": { "id": "c90d11aa" } }))
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
    assert_eq!(posts(&server).await, 2, "the lost one and the refused repeat");
}

/// A branch of that name at another commit is somebody else's: Gitea's 409
/// stands.
#[tokio::test]
async fn a_branch_of_that_name_at_another_commit_is_still_refused() {
    let server = lands_then_refuses("/branches", 409, "The branch already exists.").await;
    holds(&server, "/branches/main", json!({ "name": "main", "commit": { "id": "c90d11aa" } }))
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

/// An open pull request, as Gitea's listing answers it.
fn open_pull(number: u64, title: &str, head: &str, base: &str) -> Value {
    json!({
        "number": number,
        "state": "open",
        "title": title,
        "head": { "ref": head, "label": head },
        "base": { "ref": base, "label": base },
    })
}

const ALREADY_OPEN: &str =
    "pull request already exists for these targets [id: 145, issue_id: 160, head_repo_id: 7, \
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
            open_pull(145, "SEPA retry with exponential backoff", "feature/PAY-231-sepa-retry", "main"),
        ]),
    )
    .await;

    source(&server)
        .write(create_pull_request())
        .await
        .expect("the pull request this write opened is there");
    assert_eq!(posts(&server).await, 2, "the lost one and the refused repeat");
}

/// The open pull request for the pair carries another title: somebody
/// else's, and Gitea's 409 stands.
#[tokio::test]
async fn an_open_pull_request_with_another_title_is_still_refused() {
    let server = lands_then_refuses("/pulls", 409, ALREADY_OPEN).await;
    holds(
        &server,
        "/pulls",
        json!([open_pull(145, "WIP retry", "feature/PAY-231-sepa-retry", "main")]),
    )
    .await;

    let error = source(&server)
        .write(create_pull_request())
        .await
        .expect_err("the pull request is not this write's");
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
    Mock::given(method("GET"))
        .and(path("/api/v1/user"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "login": "bjoern" })))
        .mount(&server)
        .await;
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
    assert_eq!(posts(&server).await, 2, "the lost one and the refused repeat");
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
