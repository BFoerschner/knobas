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

use knobas_source::{Source, SourceError, WriteOp};
use serde_json::{Value, json};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::support::{TOKEN, instance};

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
    authed(Mock::given(method("POST")).and(path(
        "/api/v1/repos/tidewater/payout-service/issues/142/comments",
    )))
    .respond_with(ResponseTemplate::new(201).set_body_json(json!({ "id": 9001 })))
    .mount(&server)
    .await;
    authed(Mock::given(method("POST")).and(path(
        "/api/v1/repos/tidewater/payout-service/pulls/142/reviews",
    )))
    .respond_with(
        ResponseTemplate::new(200).set_body_json(json!({ "id": 55, "state": "APPROVED" })),
    )
    .mount(&server)
    .await;
    // An edit (#706): Gitea answers 201 with the pull request, as measured.
    authed(
        Mock::given(method("PATCH")).and(path("/api/v1/repos/tidewater/payout-service/pulls/142")),
    )
    .respond_with(ResponseTemplate::new(201).set_body_json(json!({ "number": 142 })))
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

/// Story 10 (spec #660, issue #667): a merge goes to the pull request's
/// `merge` endpoint, and the style the reader picked is Gitea's `Do`.
///
/// `Do` is the one field Gitea's `MergePullRequestOption` requires; a body
/// without it is a 422, and one spelling the style anywhere else would merge
/// with the server's default instead of the reader's choice.
#[tokio::test]
async fn a_merge_carries_the_style_the_reader_picked_as_gitea_do() {
    for (style, word) in [
        (knobas_source::MergeStyle::Merge, "merge"),
        (knobas_source::MergeStyle::Rebase, "rebase"),
        (knobas_source::MergeStyle::Squash, "squash"),
    ] {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path(
                "/api/v1/repos/tidewater/payout-service/pulls/142/merge",
            ))
            .and(header("Authorization", format!("token {TOKEN}").as_str()))
            // Gitea answers a merge with an empty 200.
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;
        source(&server)
            .write(WriteOp::MergePullRequest {
                entity: "gitea:tidewater/payout-service#142".to_owned(),
                style,
            })
            .await
            .expect("a declared op is performed");

        let (verb, path, body) = only_request(&server).await;
        assert_eq!(verb, "POST");
        assert_eq!(
            path,
            "/api/v1/repos/tidewater/payout-service/pulls/142/merge"
        );
        assert_eq!(body, json!({ "Do": word }), "{word}");
    }
}

/// A pull request Gitea will not merge -- a conflict, a work-in-progress
/// title, a merge already made -- is a **405**, and it reaches the queue as a
/// `Protocol` refusal carrying Gitea's own sentence, never retried (ADR-0004).
#[tokio::test]
async fn a_merge_gitea_refuses_arrives_as_a_refusal_with_its_sentence() {
    let server = refusing(405, "Please try again later").await;
    let error = source(&server)
        .write(WriteOp::MergePullRequest {
            entity: "gitea:tidewater/payout-service#142".to_owned(),
            style: knobas_source::MergeStyle::Merge,
        })
        .await
        .expect_err("the server refused");
    assert_eq!(error.status(), Some(405), "{error:?}");
    assert!(
        matches!(&error, SourceError::Protocol { message: m, .. } if m.contains("Please try again later")),
        "{error:?}"
    );
}

/// A merge is aimed at a **pull request**: a repository's id is refused before
/// a request leaves, as an approval's is.
#[tokio::test]
async fn a_merge_aimed_at_a_repository_is_refused_before_a_request() {
    let server = refusing(500, "must not be reached").await;
    let error = source(&server)
        .write(WriteOp::MergePullRequest {
            entity: "gitea:tidewater/payout-service".to_owned(),
            style: knobas_source::MergeStyle::Merge,
        })
        .await
        .expect_err("a repository is not a pull request");
    assert!(
        matches!(error, SourceError::Protocol { status: None, .. }),
        "{error:?}"
    );
    assert!(server.received_requests().await.unwrap().is_empty());
}

/// Workflow 6 of spec #701 (issue #705): a review goes to the pull request's
/// `reviews` endpoint with the verdict as Gitea's `event` -- `APPROVED`,
/// `REQUEST_CHANGES`, `COMMENT` -- and its body, and never without an event,
/// which would file a `PENDING` draft nobody sees.
#[tokio::test]
async fn a_review_carries_its_verdict_as_gitea_event_and_its_body() {
    use knobas_source::ReviewVerdict;
    for (verdict, event, words) in [
        (ReviewVerdict::Approve, "APPROVED", ""),
        (
            ReviewVerdict::RequestChanges,
            "REQUEST_CHANGES",
            "bound the jitter",
        ),
        (ReviewVerdict::Comment, "COMMENT", "why 30 s?"),
    ] {
        let server = accepting().await;
        source(&server)
            .write(WriteOp::SubmitReview {
                entity: "gitea:tidewater/payout-service#142".to_owned(),
                verdict,
                body: words.to_owned(),
                comments: vec![],
            })
            .await
            .expect("a declared op is performed");

        let (verb, path, body) = only_request(&server).await;
        assert_eq!(verb, "POST");
        assert_eq!(
            path,
            "/api/v1/repos/tidewater/payout-service/pulls/142/reviews"
        );
        assert_eq!(
            body,
            json!({ "event": event, "body": words, "comments": [] }),
            "{event}"
        );
    }
}

/// A line comment is Gitea's `CreatePullReviewComment`: the path, the body,
/// and the line number on the side it is on, the other side `0` (#709 fills
/// these from the *Files* tab; the mapping is the adapter's either way).
#[tokio::test]
async fn a_reviews_line_comments_land_on_the_side_they_name() {
    use knobas_source::{ReviewComment, ReviewSide, ReviewVerdict};
    let server = accepting().await;
    source(&server)
        .write(WriteOp::SubmitReview {
            entity: "gitea:tidewater/payout-service#142".to_owned(),
            verdict: ReviewVerdict::Comment,
            body: String::new(),
            comments: vec![
                ReviewComment {
                    path: "src/retry.rs".to_owned(),
                    line: 12,
                    side: ReviewSide::New,
                    body: "bound this".to_owned(),
                },
                ReviewComment {
                    path: "src/old.rs".to_owned(),
                    line: 4,
                    side: ReviewSide::Old,
                    body: "why drop it?".to_owned(),
                },
            ],
        })
        .await
        .expect("a declared op is performed");
    let (_, _, body) = only_request(&server).await;
    assert_eq!(
        body["comments"],
        json!([
            { "path": "src/retry.rs", "body": "bound this", "new_position": 12, "old_position": 0 },
            { "path": "src/old.rs", "body": "why drop it?", "new_position": 0, "old_position": 4 }
        ])
    );
}

/// Gitea refuses an approval, or a request for changes, from the pull
/// request's own author with a **422** and a sentence that says so. It
/// reaches the queue as a `Protocol` refusal carrying that sentence, never
/// retried (ADR-0004); the view already draws the action disabled, so this is
/// the window where the author changed between offer and flush.
#[tokio::test]
async fn a_review_of_ones_own_pull_request_is_refused_with_gitea_sentence() {
    let server = refusing(422, "approve your own pull is not allowed").await;
    let error = source(&server)
        .write(WriteOp::SubmitReview {
            entity: "gitea:tidewater/payout-service#142".to_owned(),
            verdict: knobas_source::ReviewVerdict::Approve,
            body: String::new(),
            comments: vec![],
        })
        .await
        .expect_err("the server refused");
    assert_eq!(error.status(), Some(422), "{error:?}");
    assert!(
        matches!(&error, SourceError::Protocol { message: m, .. }
            if m.contains("approve your own pull is not allowed")),
        "{error:?}"
    );
}

/// A review is given on a **pull request**: a repository's id is refused
/// before a request leaves.
#[tokio::test]
async fn a_review_aimed_at_a_repository_is_refused_before_a_request() {
    let server = refusing(500, "must not be reached").await;
    let error = source(&server)
        .write(WriteOp::SubmitReview {
            entity: "gitea:tidewater/payout-service".to_owned(),
            verdict: knobas_source::ReviewVerdict::Comment,
            body: "x".to_owned(),
            comments: vec![],
        })
        .await
        .expect_err("a repository is not a pull request");
    assert!(
        matches!(error, SourceError::Protocol { status: None, .. }),
        "{error:?}"
    );
    assert!(server.received_requests().await.unwrap().is_empty());
}

/// A Gitea holding `tidewater/payout-service` (default branch `main`) and one
/// branch `name`, protected or not, that answers a delete with Gitea's 204.
async fn holding_branch(name: &str, protected: bool) -> MockServer {
    let server = MockServer::start().await;
    let token = format!("token {TOKEN}");
    Mock::given(method("GET"))
        .and(path("/api/v1/repos/tidewater/payout-service"))
        .and(header("Authorization", token.as_str()))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": 7,
            "full_name": "tidewater/payout-service",
            "name": "payout-service",
            "owner": { "login": "tidewater" },
            "default_branch": "main"
        })))
        .mount(&server)
        .await;
    let escaped = name.replace('/', "%2F");
    Mock::given(method("GET"))
        .and(path(format!(
            "/api/v1/repos/tidewater/payout-service/branches/{escaped}"
        )))
        .and(header("Authorization", token.as_str()))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "name": name,
            "protected": protected,
            "commit": { "id": "c90d11aa" }
        })))
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(header("Authorization", token.as_str()))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;
    server
}

/// The requests that were not reads.
async fn writes_received(server: &MockServer) -> Vec<(String, String)> {
    server
        .received_requests()
        .await
        .expect("the mock server records requests")
        .iter()
        .filter(|r| r.method.as_str() != "GET")
        .map(|r| (r.method.to_string(), r.url.path().to_owned()))
        .collect()
}

/// Workflow 10 (#707): a branch is deleted with Gitea's
/// `DELETE /repos/{o}/{r}/branches/{name}`, its name one escaped segment.
#[tokio::test]
async fn a_branch_is_deleted_by_its_own_path() {
    let server = holding_branch("feature/PAY-231-sepa-retry", false).await;
    source(&server)
        .write(WriteOp::DeleteBranch {
            entity: "gitea:tidewater/payout-service@refs/heads/feature/PAY-231-sepa-retry"
                .to_owned(),
        })
        .await
        .expect("a declared op is performed");
    assert_eq!(
        writes_received(&server).await,
        vec![(
            "DELETE".to_owned(),
            "/api/v1/repos/tidewater/payout-service/branches/feature%2FPAY-231-sepa-retry"
                .to_owned()
        )]
    );
}

/// The default branch is refused by name before anything is deleted: Gitea
/// answers that one 403, which the shared fault mapping reads as a dead
/// credential, so the adapter says why itself.
#[tokio::test]
async fn the_default_branch_is_refused_before_a_delete() {
    let server = holding_branch("main", false).await;
    let error = source(&server)
        .write(WriteOp::DeleteBranch {
            entity: "gitea:tidewater/payout-service@refs/heads/main".to_owned(),
        })
        .await
        .expect_err("the default branch is not deleted");
    assert!(
        matches!(&error, SourceError::Protocol { status: None, message } if message.contains("default branch")),
        "{error:?}"
    );
    assert!(writes_received(&server).await.is_empty());
}

/// A protected branch is refused by name before anything is deleted.
#[tokio::test]
async fn a_protected_branch_is_refused_before_a_delete() {
    let server = holding_branch("release/2026.08", true).await;
    let error = source(&server)
        .write(WriteOp::DeleteBranch {
            entity: "gitea:tidewater/payout-service@refs/heads/release/2026.08".to_owned(),
        })
        .await
        .expect_err("a protected branch is not deleted");
    assert!(
        matches!(&error, SourceError::Protocol { status: None, message } if message.contains("protected")),
        "{error:?}"
    );
    assert!(writes_received(&server).await.is_empty());
}

/// A delete is aimed at a **branch**: a repository's or a pull request's id is
/// refused before a request leaves.
#[tokio::test]
async fn a_delete_aimed_at_anything_but_a_branch_is_refused_before_a_request() {
    let server = refusing(500, "must not be reached").await;
    for entity in [
        "gitea:tidewater/payout-service",
        "gitea:tidewater/payout-service#142",
        "gitea:tidewater/payout-service@c90d11aa",
    ] {
        let error = source(&server)
            .write(WriteOp::DeleteBranch {
                entity: entity.to_owned(),
            })
            .await
            .expect_err("only a branch is deleted");
        assert!(
            matches!(&error, SourceError::Protocol { status: None, message } if message.contains("not a branch")),
            "{entity}: {error:?}"
        );
    }
    assert!(server.received_requests().await.unwrap().is_empty());
}

/// A repository whose record names no default branch is not guessed at: the
/// delete is refused rather than taking `main` on trust, which on a `master`
/// repository would let the real default go.
#[tokio::test]
async fn a_delete_where_gitea_names_no_default_branch_is_refused() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/repos/tidewater/payout-service"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "full_name": "tidewater/payout-service", "default_branch": ""
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(
            "/api/v1/repos/tidewater/payout-service/branches/master",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "name": "master", "protected": false
        })))
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;
    let error = source(&server)
        .write(WriteOp::DeleteBranch {
            entity: "gitea:tidewater/payout-service@refs/heads/master".to_owned(),
        })
        .await
        .expect_err("no default branch named, no delete");
    assert!(
        matches!(&error, SourceError::Protocol { status: None, message } if message.contains("default branch")),
        "{error:?}"
    );
    assert!(writes_received(&server).await.is_empty());
}

/// A branch of a pull mirror is refused by name before anything is deleted:
/// Gitea answers that delete 403 (*Git Repository is a mirror.*), which the
/// shared fault mapping reads as a dead credential.
#[tokio::test]
async fn a_branch_of_a_mirror_is_refused_before_a_delete() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/repos/tidewater/payout-service"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "full_name": "tidewater/payout-service", "default_branch": "main", "mirror": true
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(
            "/api/v1/repos/tidewater/payout-service/branches/upstream-topic",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "name": "upstream-topic", "protected": false
        })))
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .respond_with(ResponseTemplate::new(403).set_body_json(json!({
            "message": "Git Repository is a mirror."
        })))
        .mount(&server)
        .await;
    let error = source(&server)
        .write(WriteOp::DeleteBranch {
            entity: "gitea:tidewater/payout-service@refs/heads/upstream-topic".to_owned(),
        })
        .await
        .expect_err("a mirror's branch is not deleted");
    assert!(
        matches!(&error, SourceError::Protocol { status: None, message } if message.contains("mirror")),
        "{error:?}"
    );
    assert!(writes_received(&server).await.is_empty());
}

// -- edit_pull_request (issue #706) ------------------------------------------

/// Workflow 8 of spec #701 (issue #706): an edit is one `PATCH` on the pull
/// request carrying **only the fields given** -- an edit of the title must not
/// send a description, which would overwrite one somebody else changed since
/// the view was read (the deputy's ruling on #706). Close and reopen are the
/// same `PATCH` with only `state`.
#[tokio::test]
async fn an_edit_patches_only_the_fields_it_was_given() {
    use knobas_source::PullRequestState;
    for (title, body, state, sent) in [
        (
            Some("Bound the jitter"),
            None,
            None,
            json!({ "title": "Bound the jitter" }),
        ),
        (None, Some(""), None, json!({ "body": "" })),
        (
            Some("t"),
            Some("b"),
            None,
            json!({ "title": "t", "body": "b" }),
        ),
        (
            None,
            None,
            Some(PullRequestState::Closed),
            json!({ "state": "closed" }),
        ),
        (
            None,
            None,
            Some(PullRequestState::Open),
            json!({ "state": "open" }),
        ),
    ] {
        let server = accepting().await;
        source(&server)
            .write(WriteOp::EditPullRequest {
                entity: "gitea:tidewater/payout-service#142".to_owned(),
                title: title.map(str::to_owned),
                body: body.map(str::to_owned),
                state,
            })
            .await
            .expect("a declared op is performed");

        let (verb, path, received) = only_request(&server).await;
        assert_eq!(verb, "PATCH");
        assert_eq!(path, "/api/v1/repos/tidewater/payout-service/pulls/142");
        assert_eq!(received, sent);
    }
}

/// An edit with nothing in it would be a `PATCH {}` that changes nothing and
/// still bumps nothing worth a request: refused before one leaves.
#[tokio::test]
async fn an_edit_with_nothing_in_it_is_refused_before_a_request() {
    let server = accepting().await;
    let error = source(&server)
        .write(WriteOp::EditPullRequest {
            entity: "gitea:tidewater/payout-service#142".to_owned(),
            title: None,
            body: None,
            state: None,
        })
        .await
        .expect_err("nothing to edit");
    assert!(
        matches!(error, SourceError::Protocol { status: None, .. }),
        "{error:?}"
    );
    assert!(server.received_requests().await.unwrap().is_empty());
}

/// A merged pull request cannot be closed or reopened: Gitea answers **412**
/// *cannot change state of this pull request, it was already merged*
/// (measured on the live 1.27.3), and that reaches the queue as a `Protocol`
/// refusal with the sentence, never retried (ADR-0004).
#[tokio::test]
async fn reopening_a_merged_pull_request_is_refused_with_gitea_sentence() {
    let server = MockServer::start().await;
    Mock::given(method("PATCH"))
        .respond_with(ResponseTemplate::new(412).set_body_json(json!({
            "message": "cannot change state of this pull request, it was already merged",
            "url": "https://gitea.example/api/swagger"
        })))
        .mount(&server)
        .await;
    let error = source(&server)
        .write(WriteOp::EditPullRequest {
            entity: "gitea:tidewater/payout-service#142".to_owned(),
            title: None,
            body: None,
            state: Some(knobas_source::PullRequestState::Open),
        })
        .await
        .expect_err("the server refused");
    assert_eq!(error.status(), Some(412), "{error:?}");
    assert!(
        matches!(&error, SourceError::Protocol { message: m, .. }
            if m.contains("it was already merged")),
        "{error:?}"
    );
}

/// An edit is made to a **pull request**: a repository's id is refused before
/// a request leaves.
#[tokio::test]
async fn an_edit_aimed_at_a_repository_is_refused_before_a_request() {
    let server = refusing(500, "must not be reached").await;
    let error = source(&server)
        .write(WriteOp::EditPullRequest {
            entity: "gitea:tidewater/payout-service".to_owned(),
            title: Some("x".to_owned()),
            body: None,
            state: None,
        })
        .await
        .expect_err("a repository is not a pull request");
    assert!(
        matches!(error, SourceError::Protocol { status: None, .. }),
        "{error:?}"
    );
    assert!(server.received_requests().await.unwrap().is_empty());
}
