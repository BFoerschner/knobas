//! What the Gitea views share (issue #678): a commit reads the same wherever
//! it is listed, and a list that stopped paging says so in its last row.
//!
//! The shapes are Gitea 1.27.3's, as `view_pr`, `view_branch` and
//! `view_repo` serve them; `tests/live_gitea.rs` re-asserts them against the
//! real server.

use knobas_source::Source;
use knobas_source::view::{Block, SourceView, Tone, ViewRow, ViewSection};
use serde_json::{Value, json};
use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::support::{TOKEN, instance};

const REPO: &str = "/api/v1/repos/tidewater/payout-service";
const WEB: &str = "https://gitea.example/tidewater/payout-service";
const PR: &str = "gitea:tidewater/payout-service#142";
const FEATURE: &str = "gitea:tidewater/payout-service@refs/heads/feature/PAY-231-sepa-retry";
const FEATURE_WIRE: &str = "feature%2FPAY-231-sepa-retry";
const REPO_ENTITY: &str = "gitea:tidewater/payout-service";
const SHA: &str = "c90d11a3f5e2b7c4d9018e6a2b3c4d5e6f708192";
/// Old enough that its age reads as its date, whenever the test runs.
const WHEN: &str = "2026-08-22T11:42:00+02:00";

/// The one commit, as `/commits`, `/pulls/{n}/commits` and the compare all
/// list it. Its message opens on a blank line, which git allows and Gitea
/// keeps.
fn listed() -> Value {
    json!({
        "sha": SHA, "created": WHEN,
        "html_url": format!("{WEB}/commit/{SHA}"),
        "author": { "login": "mara" },
        "commit": {
            "message": "\nPAY-231: jitter in backoff\n\nbody line",
            "author": { "name": "Mara Lindqvist", "email": "mara@tidewater.example", "date": WHEN }
        },
        "files": null
    })
}

/// The same commit as the branch's tip (`PayloadCommit`).
fn tip() -> Value {
    json!({
        "name": "feature/PAY-231-sepa-retry",
        "commit": {
            "id": SHA, "message": "\nPAY-231: jitter in backoff\n\nbody line", "timestamp": WHEN,
            "url": format!("{WEB}/commit/{SHA}"),
            "author": { "name": "Mara Lindqvist", "email": "mara@tidewater.example", "username": "mara" }
        }
    })
}

fn repo_record() -> Value {
    json!({
        "full_name": "tidewater/payout-service", "default_branch": "main",
        "html_url": WEB, "private": true, "empty": false
    })
}

fn branch(name: &str) -> Value {
    json!({ "name": name, "commit": { "id": SHA, "message": "tip", "timestamp": WHEN } })
}

fn pull(number: u64) -> Value {
    json!({
        "number": number, "title": format!("pull {number}"), "state": "open",
        "user": { "login": "jonas" },
        "head": { "ref": format!("head-{number}"), "repo": { "full_name": "tidewater/payout-service" } },
        "base": { "ref": "main", "repo": { "full_name": "tidewater/payout-service" } }
    })
}

fn get(p: String) -> wiremock::MockBuilder {
    Mock::given(method("GET"))
        .and(path(p))
        .and(header("Authorization", format!("token {TOKEN}").as_str()))
}

fn ok(body: Value) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(body)
}

/// A Gitea where `listed()` is the pull request's commit, the branch's tip and
/// only commit, and the default branch's latest commit. Branches and pull
/// requests are `pages` full pages of fifty each, then an empty one.
async fn gitea(pages: u32, compare: Value) -> MockServer {
    let server = MockServer::builder().start().await;
    get(REPO.to_owned())
        .respond_with(ok(repo_record()))
        .mount(&server)
        .await;
    get(format!("{REPO}/pulls/142/commits"))
        .respond_with(ok(json!([listed()])).insert_header("X-Total-Count", "1"))
        .mount(&server)
        .await;
    get(format!("{REPO}/branches/{FEATURE_WIRE}"))
        .respond_with(ok(tip()))
        .mount(&server)
        .await;
    // The branch's tip has no statuses (#710).
    get(format!("{REPO}/commits/{FEATURE_WIRE}/status"))
        .respond_with(ok(json!({ "statuses": [] })))
        .mount(&server)
        .await;
    get(format!("{REPO}/compare/main...{FEATURE_WIRE}"))
        .respond_with(ok(compare))
        .mount(&server)
        .await;
    get(format!("{REPO}/commits"))
        .and(query_param("not", "feature/PAY-231-sepa-retry"))
        .respond_with(ok(json!([])).insert_header("X-Total-Count", "0"))
        .with_priority(1)
        .mount(&server)
        .await;
    get(format!("{REPO}/commits"))
        .and(query_param("sha", "main"))
        .respond_with(ok(json!([listed()])))
        .mount(&server)
        .await;
    for page in 1..=pages {
        let branches: Vec<Value> = (0..50).map(|i| branch(&format!("b{page}-{i}"))).collect();
        get(format!("{REPO}/branches"))
            .and(query_param("page", page.to_string()))
            .respond_with(ok(Value::Array(branches)))
            .mount(&server)
            .await;
        let pulls: Vec<Value> = (0..50).map(|i| pull(u64::from(page) * 100 + i)).collect();
        get(format!("{REPO}/pulls"))
            .and(query_param("page", page.to_string()))
            .respond_with(ok(Value::Array(pulls)))
            .mount(&server)
            .await;
    }
    get(format!("{REPO}/branches"))
        .respond_with(ok(json!([])))
        .with_priority(8)
        .mount(&server)
        .await;
    get(format!("{REPO}/pulls"))
        .respond_with(ok(json!([])))
        .with_priority(8)
        .mount(&server)
        .await;
    server
}

fn one_commit_compare() -> Value {
    json!({ "total_commits": 1, "commits": [listed()] })
}

fn source(server: &MockServer) -> Box<dyn Source> {
    knobas_source_gitea::build(instance(server.uri(), TOKEN, json!({}))).expect("builds")
}

async fn section(server: &MockServer, entity: &str, id: &str) -> ViewSection {
    let view: SourceView = source(server)
        .view(entity, Some(id))
        .await
        .unwrap_or_else(|e| panic!("{entity} {id}: {e}"))
        .expect("Gitea draws it");
    view.sections
        .into_iter()
        .find(|s| s.id == id)
        .unwrap_or_else(|| panic!("no section {id:?}"))
}

/// The entity list titled `title` in `section`.
fn list(section: &ViewSection, title: Option<&str>) -> Vec<ViewRow> {
    section
        .blocks
        .iter()
        .find_map(|b| match b {
            Block::Entities { title: t, rows } if t.as_deref() == title => Some(rows.clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no list {title:?} in {section:#?}"))
}

/// A muted row with no address, pointing at Gitea's page for the rest.
fn cut(text: &str, url: &str) -> ViewRow {
    ViewRow {
        entity_id: None,
        title: text.to_owned(),
        meta: None,
        tone: Some(Tone::Muted),
        chip: None,
        url: Some(url.to_owned()),
        commit: None,
        author: None,
    }
}

#[tokio::test]
async fn one_commit_is_one_row_in_every_view_that_lists_it() {
    let server = gitea(1, one_commit_compare()).await;
    let expected = ViewRow {
        entity_id: Some(format!("gitea:tidewater/payout-service@{SHA}")),
        title: "PAY-231: jitter in backoff".into(),
        meta: Some("Mara Lindqvist · 2026-08-22".into()),
        tone: None,
        chip: Some("c90d11a3f5".into()),
        url: Some(format!("{WEB}/commit/{SHA}")),
        commit: None,
        author: Some("Mara Lindqvist".into()),
    };

    let in_pr = list(&section(&server, PR, "commits").await, None);
    let in_branch = list(&section(&server, FEATURE, "commits").await, None);
    let last = list(
        &section(&server, FEATURE, "overview").await,
        Some("Last commit"),
    );
    let in_repo = list(&section(&server, REPO_ENTITY, "commits").await, None);

    for (rows, which) in [
        (in_pr, "the pull request's Commits"),
        (in_branch, "the branch's Commits"),
        (last, "the branch's last commit"),
        (in_repo, "the repository's Commits"),
    ] {
        assert_eq!(rows, std::slice::from_ref(&expected), "{which}");
    }
}

#[tokio::test]
async fn a_list_that_stopped_paging_ends_with_where_the_rest_is() {
    let server = gitea(20, one_commit_compare()).await;

    let branches = section(&server, REPO_ENTITY, "branches").await;
    let rows = list(&branches, None);
    assert_eq!(rows.len(), 1001, "twenty pages of fifty, then the note");
    assert_eq!(
        rows.last(),
        Some(&cut(
            "The first 1000 branches; there may be more on Gitea.",
            &format!("{WEB}/branches")
        ))
    );
    assert_eq!(branches.count, Some(1000), "the count is what was read");

    let pulls = section(&server, REPO_ENTITY, "pulls").await;
    let rows = list(&pulls, None);
    assert_eq!(rows.len(), 1001);
    assert_eq!(
        rows.last(),
        Some(&cut(
            "The first 1000 open pull requests; there may be more on Gitea.",
            &format!("{WEB}/pulls")
        ))
    );
    assert_eq!(pulls.count, Some(1000));
}

#[tokio::test]
async fn a_list_that_ended_on_its_own_has_no_note() {
    let server = gitea(19, one_commit_compare()).await;
    for id in ["branches", "pulls"] {
        let section = section(&server, REPO_ENTITY, id).await;
        let rows = list(&section, None);
        assert_eq!(rows.len(), 950, "{id}");
        assert!(rows.iter().all(|r| r.entity_id.is_some()), "{id}");
        assert_eq!(section.count, Some(950), "{id}");
    }
}

#[tokio::test]
async fn a_branchs_commits_stop_at_250_and_say_so() {
    let commits: Vec<Value> = (0..300)
        .map(|i| json!({ "sha": format!("{i:040x}"), "commit": { "message": format!("c{i}") } }))
        .collect();
    let server = gitea(1, json!({ "total_commits": 300, "commits": commits })).await;

    let section = section(&server, FEATURE, "commits").await;
    let rows = list(&section, None);
    assert_eq!(rows.len(), 251, "250 commits, then the note");
    assert_eq!(rows[249].title, "c249");
    assert_eq!(
        rows.last(),
        Some(&cut(
            "The first 250 of 300 commits; the rest are on Gitea.",
            &format!("{WEB}/compare/main...feature/PAY-231-sepa-retry")
        ))
    );
    assert_eq!(section.count, Some(250), "the count is what is listed");
}
