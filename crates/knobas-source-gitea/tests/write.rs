//! M2's Gitea write-back set against the docker-free fake (issue #43).
//!
//! Stream B's contract source is the **real** pinned container (interfaces
//! §4.2) and `tests/live_gitea.rs` certifies these same four ops against it;
//! **on disagreement the fake is wrong.** This file exists because `just check`
//! and CI must stay docker-free (roadmap §3), and because a fake is the only
//! place a *refusal* can be provoked on demand.
//!
//! Every test here asserts **what the far end received** -- the method, the
//! path and the fields Gitea requires in the body -- read back off the mock
//! server's own record of the request. Not what the adapter built: a test that
//! only checked the path would pass for a comment with no text in it, which is
//! a 201 from a lenient server and a lost reply from a real one.
//!
//! A separate file rather than a growth of `support::Fake`: that fake mounts
//! `GET`s and a catch-all 401 for everything else, and the read suites rely on
//! that shape. Writes want a server that records bodies.

mod support;

use knobas_source::{Source, SourceError, WriteOp};
use serde_json::{Value, json};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use support::{TOKEN, instance};

/// A Gitea that accepts the four writes and records what it was sent.
///
/// Mounted per test rather than shared, so `received_requests` is this test's
/// requests and nothing else.
async fn accepting() -> MockServer {
    let server = MockServer::start().await;
    let token = format!("token {TOKEN}");
    let authed = |m: wiremock::MockBuilder| m.and(header("Authorization", token.as_str()));

    // Gitea's real answers: 201 for the three creates, 200 for a review.
    authed(
        Mock::given(method("POST")).and(path("/api/v1/repos/tidewater/payout-service/branches")),
    )
    .respond_with(ResponseTemplate::new(201).set_body_json(json!({ "name": "feature/x" })))
    .mount(&server)
    .await;
    authed(Mock::given(method("POST")).and(path("/api/v1/repos/tidewater/payout-service/pulls")))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({ "number": 145 })))
        .mount(&server)
        .await;
    authed(
        Mock::given(method("POST"))
            .and(path("/api/v1/repos/tidewater/payout-service/issues/142/comments")),
    )
    .respond_with(ResponseTemplate::new(201).set_body_json(json!({ "id": 9001 })))
    .mount(&server)
    .await;
    authed(
        Mock::given(method("POST"))
            .and(path("/api/v1/repos/tidewater/payout-service/pulls/142/reviews")),
    )
    .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "id": 55, "state": "APPROVED" })))
    .mount(&server)
    .await;
    server
}

/// A Gitea that refuses everything with `status` and its own error envelope.
async fn refusing(status: u16, message: &str) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(status).set_body_json(json!({
            "message": message,
            "url": "https://gitea.example/api/swagger"
        })))
        .mount(&server)
        .await;
    server
}

fn source(server: &MockServer) -> Box<dyn Source> {
    knobas_source_gitea::build(instance(server.uri(), TOKEN, json!({})))
        .expect("the adapter builds")
}

/// The one request the fake was sent, as `(method, path, body)`.
async fn only_request(server: &MockServer) -> (String, String, Value) {
    let received = server
        .received_requests()
        .await
        .expect("the mock server records requests");
    assert_eq!(
        received.len(),
        1,
        "one write is one request: {:?}",
        received.iter().map(|r| r.url.path()).collect::<Vec<_>>()
    );
    let r = &received[0];
    (
        r.method.to_string(),
        r.url.path().to_owned(),
        serde_json::from_slice(&r.body).unwrap_or(Value::Null),
    )
}

/// Story 5: a branch is created in the repository, from the ref the caller
/// named.
///
/// Both body fields are asserted: `new_branch_name` alone is a 400, and
/// `old_ref_name` alone would branch from whatever Gitea's default is -- which
/// is a branch off `main` when the user asked for one off a release branch.
#[tokio::test]
async fn a_branch_is_created_from_the_ref_that_was_named() {
    let server = accepting().await;
    source(&server)
        .write(WriteOp::CreateBranch {
            entity: "gitea:tidewater/payout-service".to_owned(),
            name: "feature/PAY-231-sepa-retry".to_owned(),
            from_ref: "release/2026.08".to_owned(),
        })
        .await
        .expect("a declared op is performed");

    let (verb, path, body) = only_request(&server).await;
    assert_eq!(verb, "POST");
    assert_eq!(path, "/api/v1/repos/tidewater/payout-service/branches");
    assert_eq!(
        body,
        json!({
            "new_branch_name": "feature/PAY-231-sepa-retry",
            "old_ref_name": "release/2026.08"
        })
    );
}

/// Story 6: a pull request carries all four fields Gitea's
/// `CreatePullRequestOption` needs to open one.
#[tokio::test]
async fn a_pull_request_carries_its_title_body_head_and_base() {
    let server = accepting().await;
    source(&server)
        .write(WriteOp::CreatePullRequest {
            entity: "gitea:tidewater/payout-service".to_owned(),
            title: "SEPA retry with exponential backoff".to_owned(),
            body: "Retries transient PSP errors. Closes PAY-231.".to_owned(),
            head: "feature/PAY-231-sepa-retry".to_owned(),
            base: "main".to_owned(),
        })
        .await
        .expect("a declared op is performed");

    let (verb, path, body) = only_request(&server).await;
    assert_eq!(verb, "POST");
    assert_eq!(path, "/api/v1/repos/tidewater/payout-service/pulls");
    assert_eq!(
        body,
        json!({
            "title": "SEPA retry with exponential backoff",
            "body": "Retries transient PSP errors. Closes PAY-231.",
            "head": "feature/PAY-231-sepa-retry",
            "base": "main"
        })
    );
}

/// Story 7: a reply goes on the pull request's discussion, which Gitea keeps on
/// the **issue** of the same index -- the same place `client::issue_comments`
/// reads it from.
#[tokio::test]
async fn a_comment_goes_on_the_issue_the_pull_request_shares_an_index_with() {
    let server = accepting().await;
    source(&server)
        .write(WriteOp::Comment {
            entity: "gitea:tidewater/payout-service#142".to_owned(),
            body: "bounded to +/-10 % in c90d11".to_owned(),
        })
        .await
        .expect("a declared op is performed");

    let (verb, path, body) = only_request(&server).await;
    assert_eq!(verb, "POST");
    assert_eq!(
        path,
        "/api/v1/repos/tidewater/payout-service/issues/142/comments"
    );
    assert_eq!(body, json!({ "body": "bounded to +/-10 % in c90d11" }));
}

/// Story 8: an approval is an approval, not a draft.
///
/// `event: "APPROVED"` is the whole difference: `POST .../reviews` with no
/// event files a `PENDING` review, which nobody is notified of and which
/// unblocks nobody -- a write that looks successful and does nothing.
#[tokio::test]
async fn an_approval_says_approved_rather_than_filing_a_pending_review() {
    let server = accepting().await;
    source(&server)
        .write(WriteOp::Approve {
            entity: "gitea:tidewater/payout-service#142".to_owned(),
            body: "looks right".to_owned(),
        })
        .await
        .expect("a declared op is performed");

    let (verb, path, body) = only_request(&server).await;
    assert_eq!(verb, "POST");
    assert_eq!(
        path,
        "/api/v1/repos/tidewater/payout-service/pulls/142/reviews"
    );
    assert_eq!(body, json!({ "event": "APPROVED", "body": "looks right" }));
}

/// An approval with no words is still an approval, and still says `APPROVED`.
#[tokio::test]
async fn an_approval_with_no_words_still_approves() {
    let server = accepting().await;
    source(&server)
        .write(WriteOp::Approve {
            entity: "gitea:tidewater/payout-service#142".to_owned(),
            body: String::new(),
        })
        .await
        .expect("an approval needs no words");
    let (_, _, body) = only_request(&server).await;
    assert_eq!(body["event"], json!("APPROVED"));
}

/// ADR-0004 on the write path: **the status reaches the queue**, which is what
/// its retry decision rests on -- a refusal read as a blip is retried every
/// five seconds forever, and a blip read as a refusal throws away the user's
/// edit. The rule itself lives in `knobas_sync::write_queue::retryable` and is
/// pinned there; what this asserts is that the status it reads survives the
/// write path at all.
///
/// The message carries Gitea's own sentence and **not** the `url` beside it in
/// the envelope, which is a link to its API docs and of no use to anyone
/// reading a failed write.
#[tokio::test]
async fn a_refusal_and_a_blip_arrive_as_different_faults() {
    for (status, message) in [
        (409, "branch already exists"),
        (404, "the base branch does not exist"),
        (403, "token has no write scope"),
        (503, "gitea is restarting"),
    ] {
        let server = refusing(status, message).await;
        let error = source(&server)
            .write(WriteOp::CreateBranch {
                entity: "gitea:tidewater/payout-service".to_owned(),
                name: "feature/x".to_owned(),
                from_ref: "main".to_owned(),
            })
            .await
            .expect_err("the server refused");

        assert_eq!(error.status(), Some(status), "{status}: {error:?}");
        match (status, &error) {
            // 403 is `Unauthorized` -- both halves of the pair the user is
            // asked to act on -- so it has no message to carry.
            (403, SourceError::Unauthorized { .. }) => {}
            (_, SourceError::Protocol { message: m, .. }) => {
                assert!(m.contains(message), "{status}: {m}");
                assert!(
                    !m.contains("swagger"),
                    "the API-doc link must not reach the message: {m}"
                );
            }
            _ => panic!("{status} arrived as {error:?}"),
        }
    }
}
