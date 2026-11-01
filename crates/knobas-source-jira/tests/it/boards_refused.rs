//! A Jira whose Agile REST refuses the account while core REST serves it
//! (#2374).
//!
//! An expired Jira Software licence, or an account without Jira Software
//! access, answers the board listing with **403**
//! `You currently don't have access to perform this operation.` while
//! `myself`, `search` and the filters answer 200. Measured on the seeded Jira
//! 10.3 on 2026-11-01 (#2373's lapsed timebomb licence). That 403 says nothing
//! about the credential: the run keeps its issues, leaves the boards as they
//! were, and says why.
//!
//! wiremock rather than mockd: mockd serves no Agile REST and is frozen
//! (ADR-0013).

use knobas_source::instance::SourceInstance;
use knobas_source::{AuthMethod, Source, SourceError, SyncItem};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Jira's answer to the board listing with Jira Software unlicensed.
const NO_ACCESS: &str = r#"{"errorMessages":["You currently don't have access to perform this operation."],"errors":{}}"#;

fn one_issue() -> serde_json::Value {
    serde_json::json!({
        "startAt": 0, "maxResults": 50, "total": 1,
        "issues": [{
            "id": "10001", "key": "OPS-1",
            "fields": {
                "summary": "Rotate the on-call pager",
                "issuetype": { "id": "10001", "name": "Task", "subtask": false },
                "status": { "id": "1", "name": "Open" },
                "project": { "id": "10000", "key": "OPS", "name": "Operations" },
                "created": "2026-10-30T09:00:00.000+0000",
                "updated": "2026-10-31T09:00:00.000+0000",
                "comment": { "startAt": 0, "maxResults": 0, "total": 0, "comments": [] },
                "worklog": { "startAt": 0, "maxResults": 0, "total": 0, "worklogs": [] }
            }
        }]
    })
}

/// A Jira that answers core REST and refuses the Agile REST with `board`.
async fn jira(board: ResponseTemplate) -> MockServer {
    let server = MockServer::builder().start().await;
    let ok = |body: serde_json::Value| ResponseTemplate::new(200).set_body_json(body);
    for (route, body) in [
        (
            "/rest/api/2/serverInfo",
            serde_json::json!({
                "version": "10.3.24",
                "serverTime": "2026-11-01T14:50:00.000+0000"
            }),
        ),
        (
            "/rest/api/2/myself",
            serde_json::json!({ "name": "knobas", "displayName": "knobas" }),
        ),
        ("/rest/api/2/search", one_issue()),
        ("/rest/api/2/status", serde_json::json!([])),
        ("/rest/api/2/filter/favourite", serde_json::json!([])),
    ] {
        Mock::given(method("GET"))
            .and(path(route))
            .respond_with(ok(body))
            .mount(&server)
            .await;
    }
    Mock::given(method("GET"))
        .and(path("/rest/agile/1.0/board"))
        .respond_with(board)
        .mount(&server)
        .await;
    server
}

fn source(base_url: &str) -> Box<dyn Source> {
    let instance = SourceInstance {
        id: "live-jira".to_owned(),
        kind: "jira".to_owned(),
        display_name: "Jira".to_owned(),
        base_url: base_url.to_owned(),
        auth: Some(AuthMethod::Pat),
        secret: Some("token".to_owned()),
        account: None,
        config: serde_json::json!({ "projects": ["OPS"], "boards": "synced" }),
    };
    match knobas_source_jira::build(instance) {
        Ok(source) => source,
        Err(error) => panic!("the adapter must build: {error:?}"),
    }
}

/// What a run handed its sink.
#[derive(Default)]
struct Recorded {
    items: Vec<SyncItem>,
    left_out: Vec<String>,
}

#[async_trait::async_trait]
impl knobas_source::Sink for Recorded {
    async fn item(&mut self, item: SyncItem) -> Result<(), SourceError> {
        self.items.push(item);
        Ok(())
    }

    async fn left_out(&mut self, why: &str) {
        self.left_out.push(why.to_owned());
    }
}

#[tokio::test]
async fn a_refused_board_listing_keeps_the_issues_and_is_not_a_sign_in_failure() {
    let jira = jira(ResponseTemplate::new(403).set_body_string(NO_ACCESS)).await;
    let mut sink = Recorded::default();

    let cursor = source(&jira.uri()).sync(None, &mut sink).await;

    assert!(
        cursor.is_ok(),
        "a 403 from the boards API failed the run: {cursor:?}"
    );
    let keys: Vec<&str> = sink.items.iter().map(|i| i.entity.key.as_str()).collect();
    assert_eq!(keys, ["OPS-1"], "the issue lands and no board does");
    assert_eq!(
        sink.left_out,
        ["boards unavailable: Jira Software is unlicensed or refuses this account (403)"],
        "the run says why the boards are missing"
    );
}

/// A Jira whose boards answer is the run's: nothing is left out.
#[tokio::test]
async fn a_listed_board_scope_leaves_nothing_out() {
    let empty = serde_json::json!({ "values": [], "isLast": true });
    let jira = jira(ResponseTemplate::new(200).set_body_json(empty)).await;
    let mut sink = Recorded::default();

    source(&jira.uri()).sync(None, &mut sink).await.unwrap();

    assert!(sink.left_out.is_empty(), "{:?}", sink.left_out);
}

/// A Jira that answers `route` with `status` and serves no other path.
async fn refusing(route: &str, status: u16) -> MockServer {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/rest/api/2/serverInfo"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "version": "10.3.24",
            "serverTime": "2026-11-01T14:50:00.000+0000"
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(route))
        .respond_with(ResponseTemplate::new(status).set_body_string(NO_ACCESS))
        .mount(&server)
        .await;
    server
}

/// The credential's own refusals keep their class: a 401 on `myself` is
/// what *Test connection* reads as "Sign-in failed".
#[tokio::test]
async fn a_401_on_myself_is_still_a_sign_in_failure() {
    let jira = refusing("/rest/api/2/myself", 401).await;
    let got = source(&jira.uri()).test_connection().await;
    assert!(
        matches!(got, Err(SourceError::Unauthorized { .. })),
        "{got:?}"
    );
}

/// ...and a 403 on core REST is the credential's too: only the Agile REST's
/// 403 is Jira Software's.
#[tokio::test]
async fn a_403_on_core_rest_still_fails_the_run_as_unauthorized() {
    let jira = refusing("/rest/api/2/search", 403).await;
    let mut sink = Recorded::default();
    let got = source(&jira.uri()).sync(None, &mut sink).await;
    assert!(
        matches!(got, Err(SourceError::Unauthorized { .. })),
        "{got:?}"
    );
    assert!(sink.left_out.is_empty());
}
