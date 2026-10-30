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
    let server = MockServer::builder().start().await;
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
    let server = MockServer::builder().start().await;
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

/// The receipt names the pull request Gitea opened by its **key** (#1949),
/// the local part of its entity id, so the app takes `gitea:<key>` as this
/// write's pull request rather than whichever one is open on the head.
#[tokio::test]
async fn a_pull_request_opened_is_named_by_its_key() {
    let server = accepting().await;
    let receipt = source(&server)
        .write(WriteOp::CreatePullRequest {
            entity: "gitea:tidewater/payout-service".to_owned(),
            title: "SEPA retry with exponential backoff".to_owned(),
            body: "Retries transient PSP errors.".to_owned(),
            head: "feature/PAY-231-sepa-retry".to_owned(),
            base: "main".to_owned(),
        })
        .await
        .expect("a declared op is performed");
    assert_eq!(
        receipt.remote_id.as_deref(),
        Some("tidewater/payout-service#145")
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
        // Gitea's refusal of a branch on a mirror (#722): a refusal of the
        // write, not of the token, so it carries its sentence like the rest.
        (403, "Git Repository is a mirror."),
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

/// #1122: a write Gitea would do again if it arrived twice -- a comment, an
/// approval, a review -- is sent once even on a busy answer, since a POST
/// that failed may still have landed. A branch create keeps the transport's
/// retries: Gitea refuses a second one with 409, so a resend cannot double it.
#[tokio::test]
async fn a_write_gitea_would_repeat_is_sent_once_and_a_guarded_one_is_retried() {
    let pr = || "gitea:tidewater/payout-service#142".to_owned();
    let once = [
        WriteOp::Comment {
            entity: pr(),
            body: "bounded".to_owned(),
        },
        WriteOp::Approve {
            entity: pr(),
            body: String::new(),
        },
        WriteOp::SubmitReview {
            entity: pr(),
            verdict: knobas_source::ReviewVerdict::Comment,
            body: "why 30 s?".to_owned(),
            comments: vec![],
        },
    ];
    for op in once {
        let server = refusing(503, "gitea is restarting").await;
        let label = format!("{op:?}");
        source(&server).write(op).await.expect_err("503");
        let sent = server.received_requests().await.unwrap().len();
        assert_eq!(sent, 1, "{label}");
    }

    let server = refusing(503, "gitea is restarting").await;
    source(&server)
        .write(WriteOp::CreateBranch {
            entity: "gitea:tidewater/payout-service".to_owned(),
            name: "feature/x".to_owned(),
            from_ref: "main".to_owned(),
        })
        .await
        .expect_err("503");
    assert_eq!(
        server.received_requests().await.unwrap().len(),
        knobas_http::MAX_ATTEMPTS as usize
    );
}

/// A 403 is two different answers on Gitea (#722), told apart by its body
/// alone (measured on 1.27.3, 2026-10-13): a token lacking the scope a write
/// needs is the credential's fault, and waits for a new one as `Unauthorized`;
/// every other 403 refuses *this write* -- a mirror, a protected branch, the
/// default branch -- and arrives as a refusal carrying Gitea's sentence, so
/// the queue records why instead of asking for a token that is fine. A bad
/// token is a 401 on Gitea, never a 403.
#[tokio::test]
async fn a_403_refusing_the_write_and_a_403_refusing_the_token_are_different_faults() {
    let write = || WriteOp::CreateBranch {
        entity: "gitea:tidewater/payout-service".to_owned(),
        name: "feature/x".to_owned(),
        from_ref: "main".to_owned(),
    };

    for sentence in [
        "Git Repository is a mirror.",
        "branch protected",
        "can not delete default or pull request target branch",
    ] {
        let server = refusing(403, sentence).await;
        let error = source(&server).write(write()).await.unwrap_err();
        assert!(
            matches!(&error, SourceError::Protocol { status: Some(403), message }
                     if message.contains(sentence) && !message.contains("swagger")),
            "{sentence}: {error:?}"
        );
    }

    let server = refusing(
        403,
        "token does not have at least one of required scope(s), required=[write:repository], \
         token scope=read:repository,read:user",
    )
    .await;
    let error = source(&server).write(write()).await.unwrap_err();
    assert!(
        matches!(error, SourceError::Unauthorized { status: Some(403) }),
        "{error:?}"
    );

    // A 403 that is not Gitea's envelope at all -- a proxy's page -- is not
    // read as a refusal: what it refuses is unknown, and the token is the
    // conservative guess.
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(403).set_body_string("<html>Forbidden</html>"))
        .mount(&server)
        .await;
    let error = source(&server).write(write()).await.unwrap_err();
    assert!(
        matches!(error, SourceError::Unauthorized { status: Some(403) }),
        "{error:?}"
    );
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
        let server = MockServer::builder().start().await;
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
                delete_branch: false,
                message: None,
            })
            .await
            .expect("a declared op is performed");

        let (verb, path, body) = only_request(&server).await;
        assert_eq!(verb, "POST");
        assert_eq!(
            path,
            "/api/v1/repos/tidewater/payout-service/pulls/142/merge"
        );
        assert_eq!(body["Do"], json!(word), "{word}");
    }
}

/// Workflow 9 of spec #701 (issue #708): *Delete branch after merge* travels
/// in the merge's own request as Gitea's `delete_branch_after_merge`, and
/// it is always said, both ways. Gitea 1.27 reads an absent field as the
/// repository's default, so a reader who unticked the box must send `false`
/// or Gitea may delete the branch anyway.
#[tokio::test]
async fn a_merge_says_whether_gitea_deletes_the_branch_after_it() {
    for delete_branch in [true, false] {
        let server = MockServer::builder().start().await;
        Mock::given(method("POST"))
            .and(path(
                "/api/v1/repos/tidewater/payout-service/pulls/142/merge",
            ))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;
        source(&server)
            .write(WriteOp::MergePullRequest {
                entity: "gitea:tidewater/payout-service#142".to_owned(),
                style: knobas_source::MergeStyle::Squash,
                delete_branch,
                message: None,
            })
            .await
            .expect("a declared op is performed");
        let (_, _, body) = only_request(&server).await;
        assert_eq!(
            body,
            json!({ "Do": "squash", "delete_branch_after_merge": delete_branch }),
            "delete_branch = {delete_branch}"
        );
    }
}

/// *Squash into one commit* with the reader's message (#843, R-8.19): its
/// first line is Gitea's `MergeTitleField`, the rest its `MergeMessageField`
/// (Gitea joins the two with a blank line); a message of one line sends an
/// empty body, so the commit is that line alone. No message sends neither,
/// and the commit is Gitea's default, as before.
#[tokio::test]
async fn a_merge_carries_the_message_the_reader_wrote() {
    for (message, expected) in [
        (
            Some("Idempotent retry window (#142)\n\nRetries on 503.\n"),
            json!({ "Do": "squash", "delete_branch_after_merge": false,
                    "MergeTitleField": "Idempotent retry window (#142)",
                    "MergeMessageField": "Retries on 503." }),
        ),
        (
            Some("Idempotent retry window (#142)"),
            json!({ "Do": "squash", "delete_branch_after_merge": false,
                    "MergeTitleField": "Idempotent retry window (#142)",
                    "MergeMessageField": "" }),
        ),
        (
            None,
            json!({ "Do": "squash", "delete_branch_after_merge": false }),
        ),
    ] {
        let server = MockServer::builder().start().await;
        Mock::given(method("POST"))
            .and(path(
                "/api/v1/repos/tidewater/payout-service/pulls/142/merge",
            ))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;
        source(&server)
            .write(WriteOp::MergePullRequest {
                entity: "gitea:tidewater/payout-service#142".to_owned(),
                style: knobas_source::MergeStyle::Squash,
                delete_branch: false,
                message: message.map(str::to_owned),
            })
            .await
            .expect("a declared op is performed");
        let (_, _, body) = only_request(&server).await;
        assert_eq!(body, expected, "{message:?}");
    }
}

/// Workflow 9 of spec #701 (issue #708): *Update branch…* is Gitea's
/// `POST .../pulls/{n}/update`, the style the reader picked as `?style=`,
/// and no body.
#[tokio::test]
async fn an_update_brings_the_head_up_to_date_in_the_style_picked() {
    for (style, word) in [
        (knobas_source::UpdateStyle::Merge, "merge"),
        (knobas_source::UpdateStyle::Rebase, "rebase"),
    ] {
        let server = MockServer::builder().start().await;
        Mock::given(method("POST"))
            .and(path(
                "/api/v1/repos/tidewater/payout-service/pulls/142/update",
            ))
            .and(header("Authorization", format!("token {TOKEN}").as_str()))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;
        source(&server)
            .write(WriteOp::UpdatePullRequestBranch {
                entity: "gitea:tidewater/payout-service#142".to_owned(),
                style,
            })
            .await
            .expect("a declared op is performed");

        let received = server.received_requests().await.unwrap();
        assert_eq!(received.len(), 1, "one write is one request");
        let request = &received[0];
        assert_eq!(request.method.to_string(), "POST");
        assert_eq!(
            request.url.path(),
            "/api/v1/repos/tidewater/payout-service/pulls/142/update"
        );
        let query: Vec<(String, String)> = request.url.query_pairs().into_owned().collect();
        assert_eq!(query, [("style".to_owned(), word.to_owned())], "{word}");
    }
}

/// An update Gitea refuses -- a conflict is a **409** -- reaches the queue as
/// a refusal carrying Gitea's sentence (ADR-0004); one aimed at anything but
/// a pull request is refused before a request leaves.
#[tokio::test]
async fn an_update_gitea_refuses_or_aimed_wrong_is_a_refusal() {
    let server = refusing(409, "merge conflict between base and head").await;
    let error = source(&server)
        .write(WriteOp::UpdatePullRequestBranch {
            entity: "gitea:tidewater/payout-service#142".to_owned(),
            style: knobas_source::UpdateStyle::Rebase,
        })
        .await
        .expect_err("the server refused");
    assert_eq!(error.status(), Some(409), "{error:?}");
    assert!(
        matches!(&error, SourceError::Protocol { message: m, .. } if m.contains("merge conflict")),
        "{error:?}"
    );

    let untouched = refusing(500, "must not be reached").await;
    let error = source(&untouched)
        .write(WriteOp::UpdatePullRequestBranch {
            entity: "gitea:tidewater/payout-service".to_owned(),
            style: knobas_source::UpdateStyle::Merge,
        })
        .await
        .expect_err("a repository is not a pull request");
    assert!(
        matches!(error, SourceError::Protocol { status: None, .. }),
        "{error:?}"
    );
    assert!(untouched.received_requests().await.unwrap().is_empty());
}

/// The merge, as the queue's flush makes it.
async fn merge_142(server: &MockServer) -> Result<(), SourceError> {
    source(server)
        .write(WriteOp::MergePullRequest {
            entity: "gitea:tidewater/payout-service#142".to_owned(),
            style: knobas_source::MergeStyle::Merge,
            delete_branch: false,
            message: None,
        })
        .await
        .map(|_| ())
}

/// Answer the merge POST with Gitea's 405 envelope saying `message`, at most
/// `times` times where given.
async fn merge_refused(server: &MockServer, message: &str, times: Option<u64>) {
    let mock = Mock::given(method("POST"))
        .and(path(
            "/api/v1/repos/tidewater/payout-service/pulls/142/merge",
        ))
        .respond_with(ResponseTemplate::new(405).set_body_json(json!({
            "message": message,
            "url": "https://gitea.example/api/swagger"
        })));
    match times {
        Some(n) => mock.up_to_n_times(n).with_priority(1).mount(server).await,
        None => mock.mount(server).await,
    }
}

/// Issue #721: Gitea answers a merge **405** *"Please try again later"* while
/// it is still working out whether the pull request can merge -- for a second
/// or two after it was opened or its head moved, even after it has already
/// answered `mergeable: true` (measured live: 3 merges in 5 sent at once). So
/// that answer is tried again, and the merge lands once Gitea has looked.
#[tokio::test]
async fn a_merge_gitea_is_still_checking_is_tried_again_until_it_lands() {
    let server = MockServer::builder().start().await;
    merge_refused(&server, "Please try again later", Some(2)).await;
    Mock::given(method("POST"))
        .and(path(
            "/api/v1/repos/tidewater/payout-service/pulls/142/merge",
        ))
        .respond_with(ResponseTemplate::new(200))
        .with_priority(2)
        .mount(&server)
        .await;

    merge_142(&server)
        .await
        .expect("the merge lands once Gitea has checked");
    assert_eq!(server.received_requests().await.unwrap().len(), 3);
}

/// Every other 405 is Gitea's refusal of this merge -- a work-in-progress
/// title, a merge already made, an account that may not merge -- and it
/// reaches the queue at once as a `Protocol` refusal carrying Gitea's own
/// sentence, sent once and never again (ADR-0004).
#[tokio::test]
async fn a_merge_gitea_refuses_arrives_as_a_refusal_with_its_sentence() {
    for sentence in [
        "Work in progress PRs cannot be merged",
        "The PR is already merged",
        "User not allowed to merge PR",
    ] {
        let server = MockServer::builder().start().await;
        merge_refused(&server, sentence, None).await;
        let error = merge_142(&server).await.expect_err("the server refused");
        assert_eq!(error.status(), Some(405), "{error:?}");
        assert!(
            matches!(&error, SourceError::Protocol { message: m, .. } if m.contains(sentence)),
            "{error:?}"
        );
        // Counted as merges: a 405 reads the pull request once, to see
        // whether the merge is this write's own (#1129).
        let merges = server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .filter(|r| r.method.as_str() == "POST")
            .count();
        assert_eq!(merges, 1, "{sentence}: a refusal is not asked twice");
    }
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
            delete_branch: false,
            message: None,
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
    let server = MockServer::builder().start().await;
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
    let server = MockServer::builder().start().await;
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
    let server = MockServer::builder().start().await;
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
    let server = MockServer::builder().start().await;
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

// -- commit_file (issue #822) -------------------------------------------------

/// The blob the tests' file is at when it was read, and the one somebody
/// else's commit left it at.
const READ_AT: &str = "986f74b729a686f111e61e07635d01e9bf265198";
const MOVED_TO: &str = "da0c4eb8d9a48d171a33574b380752e183286751";

/// A Gitea holding `main` (protected or not, and whether this token may push
/// to it all the same) and `docs/retry.md` on it at `at`, whose contents `PUT`
/// answers `put_status` -- 200 with the commit it made, or Gitea's sentence.
async fn holding_file(protected: bool, at: &str, put_status: u16, said: &str) -> MockServer {
    // "Retry twice.\n" -- what somebody else committed, wrapped the way a
    // long base64 body may be.
    holding_text(protected, at, "UmV0cnkg\ndHdpY2UuCg==", put_status, said).await
}

/// [`holding_file`] with the file's text given, in base64.
async fn holding_text(
    protected: bool,
    at: &str,
    text: &str,
    put_status: u16,
    said: &str,
) -> MockServer {
    let server = MockServer::builder().start().await;
    let token = format!("token {TOKEN}");
    Mock::given(method("GET"))
        .and(path("/api/v1/repos/tidewater/payout-service/branches/main"))
        .and(header("Authorization", token.as_str()))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "name": "main",
            "protected": protected,
            "user_can_push": !protected,
            "commit": { "id": "c90d11aa" }
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(
            "/api/v1/repos/tidewater/payout-service/contents/docs/retry.md",
        ))
        .and(header("Authorization", token.as_str()))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "name": "retry.md", "path": "docs/retry.md", "type": "file", "sha": at,
            "encoding": "base64", "content": text,
            "last_commit_sha": "657b2a74a98aeb5f75bdb4fb47ba5cf8c25acb1d"
        })))
        .mount(&server)
        .await;
    let answer = if put_status == 200 {
        ResponseTemplate::new(200).set_body_json(json!({
            "content": { "path": "docs/retry.md", "sha": "0e1d2c" },
            "commit": { "sha": "657b2a74a98aeb5f75bdb4fb47ba5cf8c25acb1d" }
        }))
    } else {
        ResponseTemplate::new(put_status).set_body_json(json!({
            "message": said, "url": "https://gitea.example/api/swagger"
        }))
    };
    Mock::given(method("PUT"))
        .and(path(
            "/api/v1/repos/tidewater/payout-service/contents/docs/retry.md",
        ))
        .and(header("Authorization", token.as_str()))
        .respond_with(answer)
        .mount(&server)
        .await;
    server
}

fn commit_file(new_branch: Option<&str>) -> WriteOp {
    WriteOp::CommitFile {
        repo: "gitea:tidewater/payout-service".to_owned(),
        branch: "main".to_owned(),
        new_branch: new_branch.map(str::to_owned),
        path: "docs/retry.md".to_owned(),
        content: "Retry three times.\n".to_owned(),
        message: "PAY-231: say how often a payout is retried".to_owned(),
        base_sha: READ_AT.to_owned(),
    }
}

/// The `PUT`s the fake was sent, as `(path, body)`.
async fn puts(server: &MockServer) -> Vec<(String, Value)> {
    server
        .received_requests()
        .await
        .expect("the mock server records requests")
        .iter()
        .filter(|r| r.method.as_str() == "PUT")
        .map(|r| (r.url.path().to_owned(), r.body_json().expect("a JSON body")))
        .collect()
}

/// R-8.6 (#822): an edit is one `PUT /repos/{o}/{r}/contents/{path}` carrying
/// the whole new text in base64, the message, the branch, and the blob the
/// edit was made against as Gitea's `sha` -- the check that makes a race a
/// hold rather than an overwrite. The receipt names the commit Gitea made.
#[tokio::test]
async fn a_file_is_committed_through_the_contents_api_against_the_blob_it_was_read_at() {
    let server = holding_file(false, READ_AT, 200, "").await;
    let receipt = source(&server)
        .write(commit_file(None))
        .await
        .expect("a declared op is performed");
    assert_eq!(
        receipt.remote_id.as_deref(),
        Some("tidewater/payout-service@657b2a74a98aeb5f75bdb4fb47ba5cf8c25acb1d")
    );
    assert_eq!(
        puts(&server).await,
        vec![(
            "/api/v1/repos/tidewater/payout-service/contents/docs/retry.md".to_owned(),
            json!({
                // "Retry three times.\n"
                "content": "UmV0cnkgdGhyZWUgdGltZXMuCg==",
                "message": "PAY-231: say how often a payout is retried",
                "branch": "main",
                "sha": READ_AT
            })
        )]
    );
}

/// *Commit to a new branch*: Gitea's own `new_branch` makes the branch from
/// `branch` and commits there, in the one request. The base branch's
/// protection is no reason to refuse it -- nothing lands on `main` -- so it
/// is not even read.
#[tokio::test]
async fn a_commit_to_a_new_branch_names_it_and_minds_no_protection_on_the_base() {
    let server = holding_file(true, READ_AT, 200, "").await;
    source(&server)
        .write(commit_file(Some("feature/PAY-231-retry-doc")))
        .await
        .expect("a new branch off a protected one is fine");
    let sent = puts(&server).await;
    assert_eq!(sent.len(), 1, "{sent:?}");
    assert_eq!(sent[0].1["new_branch"], "feature/PAY-231-retry-doc");
    assert_eq!(sent[0].1["branch"], "main");
    let read_branch = server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .any(|r| r.url.path().ends_with("/branches/main"));
    assert!(!read_branch, "a new branch's commit reads no protection");
}

/// A protected branch this token may not push to is refused before the
/// commit, saying why and what to do instead: Gitea's own 403 says only
/// *user cannot commit to repo* (measured on 1.27.3).
#[tokio::test]
async fn a_commit_to_a_protected_branch_is_refused_with_the_reason() {
    let server = holding_file(true, READ_AT, 200, "").await;
    let error = source(&server)
        .write(commit_file(None))
        .await
        .expect_err("a protected branch takes no commit");
    assert!(
        matches!(&error, SourceError::Protocol { status: None, message }
            if message.contains("protected") && message.contains("new branch")),
        "{error:?}"
    );
    assert!(puts(&server).await.is_empty());
}

/// Story 93 (#822): the file changed since it was read. Gitea answers the
/// stale `sha` with a 422; the adapter reads the file as `branch` holds it now
/// and answers [`SourceError::Moved`] with that blob and its text, which the
/// queue holds with the reader's own beside it.
#[tokio::test]
async fn a_stale_base_comes_back_moved_with_what_gitea_holds_now() {
    let server = holding_file(
        false,
        MOVED_TO,
        422,
        &format!("sha does not match [given: {READ_AT}, expected: {MOVED_TO}]"),
    )
    .await;
    let error = source(&server)
        .write(commit_file(None))
        .await
        .expect_err("a moved file is not overwritten");
    assert!(
        matches!(&error, SourceError::Moved { base, current }
            if base == MOVED_TO && current == "Retry twice.\n"),
        "{error:?}"
    );
}

/// Forgejo (#1195) answers the same stale `sha` with a 409 where Gitea says
/// 422, same sentence. The read-again decides, not the status: moved is moved.
#[tokio::test]
async fn a_stale_base_answered_409_comes_back_moved_too() {
    let server = holding_file(
        false,
        MOVED_TO,
        409,
        &format!("sha does not match [given: {READ_AT}, expected: {MOVED_TO}]"),
    )
    .await;
    let error = source(&server)
        .write(commit_file(None))
        .await
        .expect_err("a moved file is not overwritten");
    assert!(
        matches!(&error, SourceError::Moved { base, current }
            if base == MOVED_TO && current == "Retry twice.\n"),
        "{error:?}"
    );
}

/// A 409 over a file that did not move stays the server's refusal, as the 422
/// does: the status alone never makes a commit "moved".
#[tokio::test]
async fn a_409_over_a_file_that_did_not_move_is_a_refusal() {
    let server = holding_file(false, READ_AT, 409, "branch already exists").await;
    let error = source(&server)
        .write(commit_file(Some("feature/PAY-231-retry-doc")))
        .await
        .expect_err("the server refused");
    assert_eq!(error.status(), Some(409), "{error:?}");
}

/// A re-send of a commit that already landed (ADR-0012: its answer was lost,
/// or knobas stopped before the settle) meets Gitea's 422 on the stale
/// `sha`, and the file already holds this commit's text: delivered, with the
/// commit that made it -- not the reader's own text held as *theirs*. The
/// same on a new branch, whose re-send Gitea answers *branch already exists*.
#[tokio::test]
async fn a_resent_commit_that_already_landed_is_delivered_not_held() {
    // "Retry three times.\n", which is what `commit_file` sends.
    for new_branch in [None, Some("feature/PAY-231-retry-doc")] {
        let server = holding_text(
            false,
            MOVED_TO,
            "UmV0cnkgdGhyZWUgdGltZXMuCg==",
            422,
            "sha does not match",
        )
        .await;
        let receipt = source(&server)
            .write(commit_file(new_branch))
            .await
            .unwrap_or_else(|e| panic!("{new_branch:?}: already landed, not {e:?}"));
        assert_eq!(
            receipt.remote_id.as_deref(),
            Some("tidewater/payout-service@657b2a74a98aeb5f75bdb4fb47ba5cf8c25acb1d"),
            "{new_branch:?}"
        );
    }
}

/// A 422 that is not about the file -- here the new branch already exists --
/// stays Gitea's refusal: the file still reads at the blob the edit was made
/// against, so nothing moved.
#[tokio::test]
async fn a_422_over_a_file_that_did_not_move_is_a_refusal() {
    let server = holding_file(
        false,
        READ_AT,
        422,
        "branch already exists [name: feature/PAY-231-retry-doc]",
    )
    .await;
    let error = source(&server)
        .write(commit_file(Some("feature/PAY-231-retry-doc")))
        .await
        .expect_err("the server refused");
    assert_eq!(error.status(), Some(422), "{error:?}");
    assert!(
        matches!(&error, SourceError::Protocol { message, .. } if message.contains("branch already exists")),
        "{error:?}"
    );
}

/// A commit goes into a **repository**: a pull request's or a branch's id is
/// refused before a request leaves, and so is a path that could leave the
/// route it is put in.
#[tokio::test]
async fn a_commit_aimed_at_anything_but_a_repository_is_refused_before_a_request() {
    let server = refusing(500, "must not be reached").await;
    for repo in [
        "gitea:tidewater/payout-service#142",
        "gitea:tidewater/payout-service@refs/heads/main",
    ] {
        let WriteOp::CommitFile {
            branch,
            new_branch,
            path,
            content,
            message,
            base_sha,
            ..
        } = commit_file(None)
        else {
            unreachable!()
        };
        let error = source(&server)
            .write(WriteOp::CommitFile {
                repo: repo.to_owned(),
                branch,
                new_branch,
                path,
                content,
                message,
                base_sha,
            })
            .await
            .expect_err("only a repository takes a commit");
        assert!(
            matches!(&error, SourceError::Protocol { status: None, message } if message.contains("not a repository")),
            "{repo}: {error:?}"
        );
    }
    for path in ["../etc/passwd", "docs//retry.md", ""] {
        let error = source(&server)
            .write(WriteOp::CommitFile {
                repo: "gitea:tidewater/payout-service".to_owned(),
                branch: "main".to_owned(),
                new_branch: Some("x".to_owned()),
                path: path.to_owned(),
                content: "c".to_owned(),
                message: "m".to_owned(),
                base_sha: READ_AT.to_owned(),
            })
            .await
            .expect_err("not a usable path");
        assert!(
            matches!(&error, SourceError::Protocol { status: None, .. }),
            "{path:?}: {error:?}"
        );
    }
    assert!(server.received_requests().await.unwrap().is_empty());
}

// -- thread replies, resolve, review requests (#2277, #2303) -----------------

/// A Gitea answering every POST with `status`, and nothing else.
async fn answering(status: u16) -> MockServer {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(header("Authorization", format!("token {TOKEN}").as_str()))
        .respond_with(ResponseTemplate::new(status))
        .mount(&server)
        .await;
    server
}

/// #2277: a reply is a comment-only review with one line comment on the
/// thread's line, which Gitea joins to the conversation there.
#[tokio::test]
async fn a_reply_is_a_comment_review_on_the_threads_line() {
    let server = accepting().await;
    source(&server)
        .write(WriteOp::ReplyToThread {
            entity: "gitea:tidewater/payout-service#142".to_owned(),
            thread: "4088".to_owned(),
            comment: knobas_source::ReviewComment {
                path: "src/retry.rs".to_owned(),
                line: 27,
                side: knobas_source::ReviewSide::New,
                body: "Done in 7aa01f9.".to_owned(),
            },
        })
        .await
        .expect("a declared op is performed");
    let (verb, at, body) = only_request(&server).await;
    assert_eq!(
        (verb.as_str(), at.as_str()),
        (
            "POST",
            "/api/v1/repos/tidewater/payout-service/pulls/142/reviews"
        )
    );
    assert_eq!(
        body,
        json!({ "event": "COMMENT", "body": "", "comments": [
            { "path": "src/retry.rs", "body": "Done in 7aa01f9.", "new_position": 27, "old_position": 0 }
        ] })
    );
}

/// #2277: resolve and unresolve post to the thread's first comment, no
/// body; Forgejo's 405 is refused with a sentence saying the server cannot.
#[tokio::test]
async fn a_resolve_posts_to_the_threads_comment_and_forgejos_405_says_it_cannot() {
    for (resolved, word) in [(true, "resolve"), (false, "unresolve")] {
        let server = answering(204).await;
        source(&server)
            .write(WriteOp::ResolveThread {
                entity: "gitea:tidewater/payout-service#142".to_owned(),
                thread: "4088".to_owned(),
                resolved,
            })
            .await
            .expect("a declared op is performed");
        let (_, at, _) = only_request(&server).await;
        assert_eq!(
            at,
            format!("/api/v1/repos/tidewater/payout-service/pulls/comments/4088/{word}")
        );
    }
    let forgejo = answering(405).await;
    let error = source(&forgejo)
        .write(WriteOp::ResolveThread {
            entity: "gitea:tidewater/payout-service#142".to_owned(),
            thread: "4088".to_owned(),
            resolved: true,
        })
        .await
        .expect_err("Forgejo cannot resolve");
    assert!(
        matches!(&error, SourceError::Protocol { status: Some(405), message } if message.contains("cannot resolve")),
        "{error:?}"
    );
}

/// #2303: a review request is `POST …/requested_reviewers` with the logins
/// under `reviewers`; one naming nobody is refused before a request leaves.
#[tokio::test]
async fn a_review_request_names_the_reviewers_under_their_key() {
    let server = answering(201).await;
    source(&server)
        .write(WriteOp::RequestReviewers {
            entity: "gitea:tidewater/payout-service#142".to_owned(),
            reviewers: vec!["olu".to_owned(), "kai".to_owned()],
        })
        .await
        .expect("a declared op is performed");
    let (_, at, body) = only_request(&server).await;
    assert_eq!(
        at,
        "/api/v1/repos/tidewater/payout-service/pulls/142/requested_reviewers"
    );
    assert_eq!(body, json!({ "reviewers": ["olu", "kai"] }));

    let untouched = answering(500).await;
    let error = source(&untouched)
        .write(WriteOp::RequestReviewers {
            entity: "gitea:tidewater/payout-service#142".to_owned(),
            reviewers: vec![],
        })
        .await
        .expect_err("nobody to ask");
    assert!(matches!(error, SourceError::Protocol { .. }), "{error:?}");
    assert!(untouched.received_requests().await.unwrap().is_empty());
}
