//! A repository's source view against the docker-free fake (issue #663,
//! ADR-0020): its facts and README, its open pull requests, its branches by
//! recency, and the default branch's latest commits.
//!
//! The shapes are Gitea 1.27.3's as measured on the pinned container (it has
//! no `/readme` route; a README is found in the top directory's listing and
//! read raw); `tests/live_gitea.rs` re-asserts them
//! (`the_seeded_repos_view_lists_its_pull_requests_and_branches`).

use knobas_source::Source;
use knobas_source::view::{Block, SourceView, Tone, ViewRow, ViewSection};
use serde_json::{Value, json};
use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::support::{TOKEN, instance};

const REPO: &str = "/api/v1/repos/tidewater/payout-service";
const ENTITY: &str = "gitea:tidewater/payout-service";

const README: &str =
    "# payout-service\n\nPays out.\n\n- one\n- two\n\n<script>not markup</script>\n";

fn repo_record(over: Value) -> Value {
    let mut record = json!({
        "full_name": "tidewater/payout-service", "default_branch": "main",
        "description": "Payout processing service",
        "html_url": "https://gitea.example/tidewater/payout-service",
        "private": true, "internal": false, "empty": false, "mirror": false,
        "has_pull_requests": true,
        "stars_count": 0, "forks_count": 3, "size": 2560
    });
    for (k, v) in over.as_object().unwrap() {
        record[k] = v.clone();
    }
    record
}

fn branch(name: &str, sha: &str, when: &str) -> Value {
    json!({
        "name": name,
        "commit": {
            "id": sha, "message": format!("tip of {name}\n"), "timestamp": when,
            "url": format!("https://gitea.example/tidewater/payout-service/commit/{sha}"),
            "author": { "name": "Mara Lindqvist", "username": "mara" }
        }
    })
}

fn pull(number: u64, title: &str, head: &str) -> Value {
    json!({
        "number": number, "title": title, "state": "open", "merged": false,
        "user": { "login": "jonas" },
        "html_url": format!("https://gitea.example/tidewater/payout-service/pulls/{number}"),
        "head": { "ref": head, "repo": { "full_name": "tidewater/payout-service" } },
        "base": { "ref": "main", "repo": { "full_name": "tidewater/payout-service" } }
    })
}

fn commit(sha: &str, message: &str, when: &str) -> Value {
    json!({
        "sha": sha, "created": when,
        "html_url": format!("https://gitea.example/tidewater/payout-service/commit/{sha}"),
        "commit": { "message": message, "author": { "name": "Mara Lindqvist", "date": when } },
        // What `files=false` answers on 1.27.3: `null`, not `[]` and not absent.
        "files": null, "stats": null
    })
}

/// What one fake serves.
struct Served {
    record: Value,
    /// The top directory's listing; `None` answers it 404, as an empty
    /// repository's is.
    contents: Option<Value>,
    readme: Option<(&'static str, &'static str)>,
    branches: Value,
    pulls: Option<Value>,
    /// `None` answers `/commits` 409, as an empty repository's is.
    commits: Option<Value>,
}

impl Default for Served {
    fn default() -> Self {
        Self {
            record: repo_record(json!({})),
            contents: Some(json!([
                { "name": "src", "type": "dir", "path": "src", "size": 0 },
                { "name": "README.md", "type": "file", "path": "README.md", "size": 60 },
                { "name": "readme-notes.txt", "type": "file", "path": "readme-notes.txt", "size": 4 }
            ])),
            readme: Some(("README.md", README)),
            // Listing order is Gitea's (by name); the view orders by recency.
            branches: json!([
                branch(
                    "feature/PAY-231-sepa-retry",
                    "c90d11a3f5e2b7c4d9018e6a2b3c4d5e6f708192",
                    "2026-08-22T11:42:00Z"
                ),
                branch(
                    "fix/PAY-228-partial-refund-drift",
                    "dccb3df0dccb3df0dccb3df0dccb3df0dccb3df0",
                    "2026-08-31T20:08:41Z"
                ),
                branch(
                    "main",
                    "09479f8509479f8509479f8509479f8509479f85",
                    "2026-08-01T04:18:42Z"
                ),
            ]),
            pulls: Some(json!([
                pull(
                    144,
                    "Add payout CSV export",
                    "feature/PAY-236-payout-csv-export"
                ),
                pull(
                    142,
                    "SEPA retry with exponential backoff",
                    "feature/PAY-231-sepa-retry"
                ),
            ])),
            commits: Some(json!([
                commit(
                    "09479f8509479f8509479f8509479f8509479f85",
                    "Merge PAY-228\n\nbody",
                    "2026-08-01T04:18:42Z"
                ),
                commit(
                    "1111111111111111111111111111111111111111",
                    "PAY-228 partial refund drift",
                    "2026-07-21T16:00:00Z"
                ),
            ])),
        }
    }
}

async fn gitea(served: Served) -> MockServer {
    let server = MockServer::start().await;
    let token = format!("token {TOKEN}");
    let authed = |m: wiremock::MockBuilder| m.and(header("Authorization", token.as_str()));
    let ok = |body: Value| ResponseTemplate::new(200).set_body_json(body);
    let not_found = || ResponseTemplate::new(404).set_body_json(json!({ "message": "not found" }));

    authed(Mock::given(method("GET")).and(path(REPO)))
        .respond_with(ok(served.record))
        .mount(&server)
        .await;
    authed(Mock::given(method("GET")).and(path(format!("{REPO}/contents"))))
        .respond_with(served.contents.map_or_else(not_found, ok))
        .mount(&server)
        .await;
    if let Some((name, text)) = served.readme {
        authed(Mock::given(method("GET")).and(path(format!("{REPO}/raw/{name}"))))
            .respond_with(ResponseTemplate::new(200).set_body_string(text))
            .mount(&server)
            .await;
    }
    authed(Mock::given(method("GET")).and(path(format!("{REPO}/branches"))))
        .and(query_param("page", "1"))
        .respond_with(ok(served.branches))
        .mount(&server)
        .await;
    authed(Mock::given(method("GET")).and(path(format!("{REPO}/branches"))))
        .and(query_param("page", "2"))
        .respond_with(ok(json!([])))
        .mount(&server)
        .await;
    match served.pulls {
        Some(pulls) => {
            authed(Mock::given(method("GET")).and(path(format!("{REPO}/pulls"))))
                .and(query_param("state", "open"))
                .and(query_param("page", "1"))
                .respond_with(ok(pulls))
                .mount(&server)
                .await;
            authed(Mock::given(method("GET")).and(path(format!("{REPO}/pulls"))))
                .and(query_param("page", "2"))
                .respond_with(ok(json!([])))
                .mount(&server)
                .await;
        }
        None => {
            authed(Mock::given(method("GET")).and(path(format!("{REPO}/pulls"))))
                .respond_with(not_found())
                .mount(&server)
                .await;
        }
    }
    let commits = authed(Mock::given(method("GET")).and(path(format!("{REPO}/commits"))))
        .and(query_param("sha", "main"))
        .and(query_param("limit", "20"));
    match served.commits {
        Some(listing) => commits.respond_with(ok(listing)),
        None => commits.respond_with(
            ResponseTemplate::new(409)
                .set_body_json(json!({ "message": "Git Repository is empty." })),
        ),
    }
    .mount(&server)
    .await;
    server
}

fn source(server: &MockServer) -> Box<dyn Source> {
    knobas_source_gitea::build(instance(server.uri(), TOKEN, json!({}))).expect("builds")
}

fn section<'v>(view: &'v SourceView, id: &str) -> &'v ViewSection {
    view.sections
        .iter()
        .find(|s| s.id == id)
        .unwrap_or_else(|| panic!("no section {id:?} in {view:#?}"))
}

fn facts(section: &ViewSection) -> Vec<(&str, &str)> {
    let Some(Block::Facts { facts }) = section.blocks.first() else {
        panic!("the section opens on facts: {section:#?}");
    };
    facts
        .iter()
        .map(|f| (f.label.as_str(), f.value.as_str()))
        .collect()
}

fn rows(section: &ViewSection) -> &[ViewRow] {
    section
        .blocks
        .iter()
        .find_map(|b| match b {
            Block::Entities { rows, .. } => Some(rows.as_slice()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no list in {section:#?}"))
}

#[tokio::test]
async fn a_repo_view_is_its_overview_pull_requests_branches_and_commits() {
    let server = gitea(Served::default()).await;
    let view = source(&server)
        .view(ENTITY, None)
        .await
        .expect("the read succeeds")
        .expect("Gitea draws a repository");

    let tabs: Vec<_> = view
        .sections
        .iter()
        .map(|s| (s.id.as_str(), s.title.as_str(), s.count, s.deferred))
        .collect();
    assert_eq!(
        tabs,
        [
            ("overview", "Overview", None, false),
            ("pulls", "Pull requests", Some(2), false),
            ("branches", "Branches", Some(3), false),
            ("commits", "Commits", None, false),
        ]
    );

    let overview = section(&view, "overview");
    assert_eq!(
        facts(overview),
        [
            ("Default branch", "main"),
            ("Description", "Payout processing service"),
            ("Visibility", "private"),
            ("Forks", "3"),
            ("Size", "2.5 MiB"),
        ],
        "no stars line for none"
    );
    assert_eq!(
        overview.blocks[1],
        Block::Text {
            text: README.trim_end().to_owned(),
            markdown: true,
        },
        "the README.md, whole, as Markdown text"
    );

    let pulls = rows(section(&view, "pulls"));
    assert_eq!(pulls.len(), 2);
    assert_eq!(
        pulls[1],
        ViewRow {
            entity_id: Some("gitea:tidewater/payout-service#142".into()),
            title: "SEPA retry with exponential backoff".into(),
            meta: Some("#142 · feature/PAY-231-sepa-retry → main · jonas".into()),
            tone: Some(Tone::Info),
            chip: Some("open".into()),
            url: Some("https://gitea.example/tidewater/payout-service/pulls/142".into()),
            commit: None,
        }
    );

    let branches = rows(section(&view, "branches"));
    let order: Vec<_> = branches
        .iter()
        .map(|r| (r.title.as_str(), r.chip.as_deref()))
        .collect();
    assert_eq!(
        order,
        [
            ("main", Some("default")),
            ("fix/PAY-228-partial-refund-drift", None),
            ("feature/PAY-231-sepa-retry", None),
        ],
        "the default branch first, then by last commit, newest first"
    );
    assert_eq!(
        branches[1].entity_id.as_deref(),
        Some("gitea:tidewater/payout-service@refs/heads/fix/PAY-228-partial-refund-drift")
    );
    assert_eq!(
        branches[1].meta.as_deref(),
        Some("dccb3df0dc · 2026-08-31 20:08")
    );

    let commits = rows(section(&view, "commits"));
    let commits: Vec<_> = commits
        .iter()
        .map(|r| (r.entity_id.as_deref().unwrap(), r.title.as_str()))
        .collect();
    assert_eq!(
        commits,
        [
            (
                "gitea:tidewater/payout-service@09479f8509479f8509479f8509479f8509479f85",
                "Merge PAY-228"
            ),
            (
                "gitea:tidewater/payout-service@1111111111111111111111111111111111111111",
                "PAY-228 partial refund drift"
            ),
        ]
    );
}

#[tokio::test]
async fn a_repo_without_a_readme_draws_no_readme_block() {
    let server = gitea(Served {
        contents: Some(json!([{ "name": "src", "type": "dir", "path": "src", "size": 0 }])),
        readme: None,
        ..Served::default()
    })
    .await;
    let view = source(&server).view(ENTITY, None).await.unwrap().unwrap();
    let overview = section(&view, "overview");
    assert!(
        !overview
            .blocks
            .iter()
            .any(|b| matches!(b, Block::Text { .. })),
        "{overview:#?}"
    );
}

/// A README that is not Markdown is drawn as plain text; one named in
/// another case is still found.
#[tokio::test]
async fn a_plain_readme_is_text_and_not_markdown() {
    let server = gitea(Served {
        contents: Some(json!([{ "name": "readme", "type": "file", "path": "readme", "size": 9 }])),
        readme: Some(("readme", "*literal*\n")),
        ..Served::default()
    })
    .await;
    let view = source(&server).view(ENTITY, None).await.unwrap().unwrap();
    assert_eq!(
        section(&view, "overview").blocks[1],
        Block::Text {
            text: "*literal*".into(),
            markdown: false
        }
    );
}

/// A repository with no commits at all: no contents, `200 null` branches, a
/// 404 for `/pulls` and a 409 for `/commits` (Gitea 1.27.3, measured). It
/// draws its overview, and nothing fails.
#[tokio::test]
async fn an_empty_repo_draws_an_overview_and_no_failure() {
    let server = gitea(Served {
        record: repo_record(
            json!({ "empty": true, "description": "", "forks_count": 0, "size": 0 }),
        ),
        contents: None,
        readme: None,
        branches: Value::Null,
        pulls: None,
        commits: None,
    })
    .await;
    let view = source(&server)
        .view(ENTITY, None)
        .await
        .expect("an empty repository is not a failure")
        .expect("drawn");
    let overview = section(&view, "overview");
    assert_eq!(
        facts(overview),
        [
            ("Default branch", "main"),
            ("Visibility", "private"),
            ("Contents", "empty"),
        ]
    );
    assert_eq!(overview.blocks.len(), 1, "no README block");
    assert_eq!(section(&view, "pulls").count, Some(0));
    assert_eq!(section(&view, "branches").count, Some(0));
    assert!(rows(section(&view, "commits")).is_empty());
    let asked: Vec<String> = server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .map(|r| r.url.path().to_owned())
        .collect();
    assert!(
        !asked.iter().any(|p| p.ends_with("/pulls")),
        "an empty repository's /pulls is a bare 404, so it is not asked: {asked:?}"
    );
}

/// One section on its own, and a section the view does not have refused.
#[tokio::test]
async fn one_section_is_read_on_its_own() {
    let server = gitea(Served::default()).await;
    let source = source(&server);
    let view = source
        .view(ENTITY, Some("branches"))
        .await
        .unwrap()
        .unwrap();
    let ids: Vec<_> = view.sections.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(ids, ["branches"]);
    assert!(source.view(ENTITY, Some("diff")).await.is_err());
}
