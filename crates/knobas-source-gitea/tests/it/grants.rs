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
/// entity, an id it did not issue, and an op it does not declare. A probe
/// that meets anything but the two measured answers is no verdict.
#[tokio::test]
async fn what_this_source_cannot_answer_is_no_verdict() {
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

    // Unmounted, so the probe meets wiremock's 404.
    let refused = source(&server)
        .refused_writes(&[ask(issue(), "comment")])
        .await
        .unwrap();
    assert_eq!(refused, []);
}
