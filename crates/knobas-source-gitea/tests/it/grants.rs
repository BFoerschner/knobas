//! What the token may write outside a view (#2054, spec #1192 story 10):
//! `Source::refused_writes` answered from the empty-bodied probes a view
//! sends (#2030, `crate::grants`).

use knobas_source::Source;
use knobas_source::grants::{RefusedWrite, WriteAsk};
use knobas_source_gitea::keys;
use wiremock::MockServer;

use crate::support::{TOKEN, instance, write_probe};

const REPO: &str = "/api/v1/repos/tidewater/payout-service";
const OTHER: &str = "/api/v1/repos/tidewater/ledger";

const SCOPE_ISSUE: &str = "token does not have at least one of required scope(s), \
                           required=[write:issue], token scope=read:issue,read:repository";
const SCOPE_REPOSITORY: &str = "token does not have at least one of required scope(s), \
                                required=[write:repository], token scope=read:repository";

fn source(server: &MockServer) -> Box<dyn Source> {
    knobas_source_gitea::build(instance(server.uri(), TOKEN, serde_json::json!({})))
        .expect("the adapter builds")
}

fn ask(entity: String, op: &str) -> WriteAsk {
    WriteAsk {
        entity,
        op: op.to_owned(),
        field: None,
    }
}

fn id(key: String) -> String {
    format!("gitea:{key}")
}

fn issue() -> String {
    id(keys::issue_key("tidewater", "payout-service", 7))
}

fn pull() -> String {
    id(keys::pr_key("tidewater", "payout-service", 142))
}

fn repository() -> String {
    id(keys::repo_key("tidewater", "payout-service"))
}

/// The writes a shell surface draws for a Gitea ticket and pull request.
fn asks() -> Vec<WriteAsk> {
    vec![
        ask(issue(), "comment"),
        ask(issue(), "transition"),
        ask(repository(), "create_ticket"),
        ask(pull(), "comment"),
        ask(pull(), "approve"),
        ask(pull(), "merge_pull_request"),
    ]
}

async fn posts(server: &MockServer) -> Vec<String> {
    server
        .received_requests()
        .await
        .unwrap()
        .into_iter()
        .filter(|r| r.method == wiremock::http::Method::POST)
        .map(|r| r.url.path().to_owned())
        .collect()
}

/// A token minted with read scopes only is refused each write it is asked
/// about, naming the scope the write's route needs, in the order asked.
#[tokio::test]
async fn a_read_only_token_is_refused_each_write_saying_which_scope() {
    let server = MockServer::builder().start().await;
    write_probe(&server, format!("{REPO}/issues"), 403, SCOPE_ISSUE).await;
    write_probe(&server, format!("{REPO}/branches"), 403, SCOPE_REPOSITORY).await;

    let refused = source(&server).refused_writes(&asks()).await.unwrap();

    let issue_scope = "This token cannot write: it lacks the write:issue scope";
    let repository_scope = "This token cannot write: it lacks the write:repository scope";
    let want = |entity: String, op: &str, reason: &str| RefusedWrite {
        entity,
        op: op.to_owned(),
        field: None,
        reason: reason.to_owned(),
    };
    assert_eq!(
        refused,
        [
            want(issue(), "comment", issue_scope),
            want(issue(), "transition", issue_scope),
            want(repository(), "create_ticket", issue_scope),
            want(pull(), "comment", issue_scope),
            want(pull(), "approve", repository_scope),
            want(pull(), "merge_pull_request", repository_scope),
        ]
    );
    let mut sent = posts(&server).await;
    sent.sort();
    assert_eq!(
        sent,
        [format!("{REPO}/branches"), format!("{REPO}/issues")],
        "one probe per category for the repository, however many asks"
    );
}

/// A token that may write gets the body's 422 from each probe: nothing is
/// refused, and every write is drawn as before.
#[tokio::test]
async fn a_token_that_may_write_is_refused_nothing() {
    let server = MockServer::builder().start().await;
    write_probe(&server, format!("{REPO}/issues"), 422, "[Title]: Required").await;
    write_probe(
        &server,
        format!("{REPO}/branches"),
        422,
        "[BranchName]: Required",
    )
    .await;

    let refused = source(&server).refused_writes(&asks()).await.unwrap();

    assert_eq!(refused, []);
}

/// An account that may not push is refused only the writes that push; a
/// reader may still comment and approve.
#[tokio::test]
async fn an_account_that_may_not_push_is_refused_only_the_merge() {
    let server = MockServer::builder().start().await;
    write_probe(&server, format!("{REPO}/issues"), 422, "[Title]: Required").await;
    write_probe(
        &server,
        format!("{REPO}/branches"),
        403,
        "user should have a permission to write to a repo",
    )
    .await;

    let refused = source(&server).refused_writes(&asks()).await.unwrap();

    assert_eq!(
        refused,
        [RefusedWrite {
            entity: pull(),
            op: "merge_pull_request".to_owned(),
            field: None,
            reason: "This account cannot push to this repository".to_owned(),
        }]
    );
}

/// Each repository is asked about on its own, and only for the categories
/// its asks fall in: a token's scope holds everywhere, but an account's push
/// does not.
#[tokio::test]
async fn each_repository_is_probed_for_the_categories_it_is_asked_about() {
    let server = MockServer::builder().start().await;
    write_probe(&server, format!("{REPO}/issues"), 403, SCOPE_ISSUE).await;
    write_probe(
        &server,
        format!("{OTHER}/branches"),
        422,
        "[BranchName]: Required",
    )
    .await;
    let ledger = id(keys::pr_key("tidewater", "ledger", 9));

    let refused = source(&server)
        .refused_writes(&[ask(issue(), "comment"), ask(ledger.clone(), "approve")])
        .await
        .unwrap();

    assert_eq!(
        refused
            .iter()
            .map(|r| r.entity.as_str())
            .collect::<Vec<_>>(),
        [issue()]
    );
    let mut sent = posts(&server).await;
    sent.sort();
    assert_eq!(
        sent,
        [format!("{OTHER}/branches"), format!("{REPO}/issues")]
    );
}

/// What this source does not answer is not asked about: another source's
/// entity, an id it did not issue, and an op it does not declare.
#[tokio::test]
async fn what_this_source_cannot_answer_is_not_asked() {
    let server = MockServer::builder().start().await;
    let refused = source(&server)
        .refused_writes(&[
            ask("jira:PAY-1".to_owned(), "comment"),
            ask("gitea:not a key".to_owned(), "comment"),
            ask(issue(), "run_build"),
        ])
        .await
        .unwrap();
    assert_eq!(refused, []);
    assert_eq!(
        posts(&server).await,
        Vec::<String>::new(),
        "nothing is probed"
    );
}

/// **A probe that meets anything but the measured answers is no verdict,
/// and no verdict is an error, never "taken"** (#2306): the app keeps a
/// grants answer and asks it again behind, so a failure answered as taken
/// would be kept, or replace a kept refusal. A 5xx, a 502 and a 401 each
/// fail the call.
#[tokio::test]
async fn a_probe_without_a_verdict_fails_the_ask() {
    for (status, message) in [
        (500, "internal error"),
        (502, "bad gateway"),
        (401, "token is required"),
    ] {
        let server = MockServer::builder().start().await;
        write_probe(&server, format!("{REPO}/issues"), status, message).await;
        let answer = source(&server)
            .refused_writes(&[ask(issue(), "comment")])
            .await;
        assert!(answer.is_err(), "{status}: {answer:?}");
    }
}

/// One category without a verdict fails the whole ask, though the other
/// category answered: `refused_writes` cannot say "taken" for one ask and
/// "unknown" for another, and an ask left out of an `Ok` reads as taken.
#[tokio::test]
async fn one_probe_without_a_verdict_fails_the_asks_beside_it() {
    let server = MockServer::builder().start().await;
    write_probe(&server, format!("{REPO}/issues"), 403, SCOPE_ISSUE).await;
    write_probe(&server, format!("{REPO}/branches"), 503, "maintenance").await;
    let answer = source(&server).refused_writes(&asks()).await;
    assert!(answer.is_err(), "{answer:?}");
}

/// **An archived repository's 423 and a missing route's 404 are verdicts**
/// (merger of #2306, measured on Gitea 1.27 and Forgejo): no write lands
/// behind either, so each refuses its category's writes saying why rather
/// than failing the call. Left as errors, one archived or deleted
/// repository among a surface's asks would draw every other repository's
/// writes "could not ask", on every ask.
#[tokio::test]
async fn an_archived_or_missing_repository_refuses_saying_why() {
    let server = MockServer::builder().start().await;
    write_probe(&server, format!("{REPO}/issues"), 423, "repo is archived").await;
    write_probe(&server, format!("{REPO}/branches"), 423, "repo is archived").await;
    write_probe(&server, format!("{OTHER}/issues"), 404, "not found").await;
    write_probe(&server, format!("{OTHER}/branches"), 422, "[BranchName]: Required").await;
    let elsewhere = id(keys::issue_key("tidewater", "ledger", 3));
    let elsewhere_pull = id(keys::pr_key("tidewater", "ledger", 4));
    let refused = source(&server)
        .refused_writes(&[
            ask(issue(), "comment"),
            ask(pull(), "merge_pull_request"),
            ask(elsewhere.clone(), "comment"),
            ask(elsewhere_pull.clone(), "merge_pull_request"),
        ])
        .await
        .expect("both are verdicts");
    let archived = "This repository is archived, so it takes no writes";
    assert_eq!(
        refused,
        [
            RefusedWrite::of(&ask(issue(), "comment"), archived.to_owned()),
            RefusedWrite::of(&ask(pull(), "merge_pull_request"), archived.to_owned()),
            RefusedWrite::of(
                &ask(elsewhere, "comment"),
                "Gitea does not show this repository's issues to this token: they are switched off, or the repository is gone".to_owned()
            ),
        ]
    );
}
