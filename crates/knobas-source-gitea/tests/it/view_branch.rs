//! A branch's source view against the docker-free fake (issue #663,
//! ADR-0020): how far it is from the repository's default branch, its last
//! commit, its open pull request, its commits and its diff against the
//! default branch.
//!
//! The shapes are Gitea 1.27.3's as measured on the pinned container; whether
//! the real server still answers them is `tests/live_gitea.rs`'s to say
//! (`a_seeded_branchs_view_reads_ahead_with_its_seeded_commits`), and on
//! disagreement this fake is what is wrong.

use knobas_source::view::{Block, DiffChange, Fact, SourceView, Tone, ViewRow, ViewSection};
use knobas_source::{Source, SourceError};
use serde_json::{Value, json};
use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::support::{TOKEN, instance};

const REPO: &str = "/api/v1/repos/tidewater/payout-service";
const FEATURE: &str = "gitea:tidewater/payout-service@refs/heads/feature/PAY-231-sepa-retry";
const MAIN: &str = "gitea:tidewater/payout-service@refs/heads/main";
/// The branch name as it goes on the wire: one path segment, its `/` escaped.
const FEATURE_WIRE: &str = "feature%2FPAY-231-sepa-retry";
const TIP: &str = "c90d11a3f5e2b7c4d9018e6a2b3c4d5e6f708192";
const FIRST: &str = "7be0e4a3f5e2b7c4d9018e6a2b3c4d5e6f708192";
const MAIN_TIP: &str = "1111111111111111111111111111111111111111";

/// The compare's diff: two files, one added.
const DIFF: &str = "diff --git a/src/retry.rs b/src/retry.rs
index 84734ad..6112a3c 100644
--- a/src/retry.rs
+++ b/src/retry.rs
@@ -1,2 +1,3 @@
 let base = 30;
-base * attempts
+let jitter = 3;
+base * attempts + jitter
diff --git a/CHANGELOG.md b/CHANGELOG.md
new file mode 100644
index 0000000..5786b13
--- /dev/null
+++ b/CHANGELOG.md
@@ -0,0 +1 @@
+- jitter in backoff
";

fn repo_record() -> Value {
    json!({
        "full_name": "tidewater/payout-service", "default_branch": "main",
        "html_url": "https://gitea.example/tidewater/payout-service",
        "private": true, "internal": false, "empty": false
    })
}

fn branch_record(name: &str, sha: &str, message: &str, when: &str) -> Value {
    json!({
        "name": name,
        "commit": {
            "id": sha, "message": format!("{message}\n"), "timestamp": when,
            "url": format!("https://gitea.example/tidewater/payout-service/commit/{sha}"),
            "author": { "name": "Mara Lindqvist", "email": "mara@tidewater.example", "username": "mara" }
        }
    })
}

/// One commit as the compare and `/commits` list it, its touched files with it.
fn listed(sha: &str, message: &str, when: &str, files: &[&str]) -> Value {
    json!({
        "sha": sha, "created": when,
        "html_url": format!("https://gitea.example/tidewater/payout-service/commit/{sha}"),
        "commit": {
            "message": format!("{message}\n\nbody line"),
            "author": { "name": "Mara Lindqvist", "email": "mara@tidewater.example", "date": when }
        },
        "files": files.iter().map(|f| json!({ "filename": f, "status": "modified" })).collect::<Vec<_>>()
    })
}

fn pull(number: u64, title: &str, head: &str) -> Value {
    json!({
        "number": number, "title": title, "state": "open", "merged": false,
        "user": { "login": "mara" },
        "html_url": format!("https://gitea.example/tidewater/payout-service/pulls/{number}"),
        "head": { "ref": head, "repo": { "full_name": "tidewater/payout-service" } },
        "base": { "ref": "main", "repo": { "full_name": "tidewater/payout-service" } }
    })
}

/// What one fake serves, so a test can take something away.
struct Served {
    pulls: Vec<Value>,
}

impl Default for Served {
    fn default() -> Self {
        Self {
            pulls: vec![
                pull(
                    144,
                    "Add payout CSV export",
                    "feature/PAY-236-payout-csv-export",
                ),
                pull(
                    142,
                    "SEPA retry with exponential backoff",
                    "feature/PAY-231-sepa-retry",
                ),
            ],
        }
    }
}

async fn gitea(served: Served) -> MockServer {
    let server = MockServer::start().await;
    let token = format!("token {TOKEN}");
    let authed = |m: wiremock::MockBuilder| m.and(header("Authorization", token.as_str()));
    let ok = |body: Value| ResponseTemplate::new(200).set_body_json(body);

    authed(Mock::given(method("GET")).and(path(REPO)))
        .respond_with(ok(repo_record()))
        .mount(&server)
        .await;
    authed(Mock::given(method("GET")).and(path(format!("{REPO}/branches/{FEATURE_WIRE}"))))
        .respond_with(ok(branch_record(
            "feature/PAY-231-sepa-retry",
            TIP,
            "PAY-231: jitter in backoff, cap at 5 attempts",
            "2026-08-22T11:42:00Z",
        )))
        .mount(&server)
        .await;
    authed(Mock::given(method("GET")).and(path(format!("{REPO}/branches/main"))))
        .respond_with(ok(branch_record(
            "main",
            MAIN_TIP,
            "PAY-228 partial refund drift",
            "2026-08-21T16:00:00Z",
        )))
        .mount(&server)
        .await;
    // Ahead: what the branch carries over main. Two commits, three touches
    // of two files -- the count is files, not touches.
    authed(Mock::given(method("GET")).and(path(format!("{REPO}/compare/main...{FEATURE_WIRE}"))))
        .and(query_param("output", "diff"))
        .respond_with(ResponseTemplate::new(200).set_body_string(DIFF))
        .with_priority(1)
        .mount(&server)
        .await;
    authed(Mock::given(method("GET")).and(path(format!(
        "{REPO}/compare/main...{FEATURE_WIRE}"
    ))))
    .respond_with(ok(json!({
        "total_commits": 2,
        "commits": [
            listed(TIP, "PAY-231: jitter in backoff, cap at 5 attempts", "2026-08-22T11:42:00Z", &["src/retry.rs", "CHANGELOG.md"]),
            listed(FIRST, "PAY-231: retry SEPA payouts", "2026-08-21T17:30:00Z", &["src/retry.rs"]),
        ]
    })))
    .mount(&server)
    .await;
    // Behind: what main has that the branch does not.
    authed(Mock::given(method("GET")).and(path(format!(
        "{REPO}/compare/{FEATURE_WIRE}...main"
    ))))
    .respond_with(ok(json!({
        "total_commits": 1,
        "commits": [listed(MAIN_TIP, "PAY-228 partial refund drift", "2026-08-21T16:00:00Z", &[])]
    })))
    .mount(&server)
    .await;
    authed(Mock::given(method("GET")).and(path(format!("{REPO}/commits"))))
        .and(query_param("sha", "main"))
        .respond_with(ok(json!([listed(
            MAIN_TIP,
            "PAY-228 partial refund drift",
            "2026-08-21T16:00:00Z",
            &[]
        )])))
        .mount(&server)
        .await;
    authed(Mock::given(method("GET")).and(path(format!("{REPO}/pulls"))))
        .and(query_param("state", "open"))
        .and(query_param("page", "1"))
        .respond_with(ok(json!(served.pulls)))
        .mount(&server)
        .await;
    authed(Mock::given(method("GET")).and(path(format!("{REPO}/pulls"))))
        .and(query_param("state", "open"))
        .and(query_param("page", "2"))
        .respond_with(ok(json!([])))
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

fn facts(section: &ViewSection) -> Vec<(String, String)> {
    let Some(Block::Facts { facts }) = section.blocks.first() else {
        panic!("the section opens on facts: {section:#?}");
    };
    facts
        .iter()
        .map(|f| (f.label.clone(), f.value.clone()))
        .collect()
}

/// The entity list titled `title` in `section`.
fn list<'s>(section: &'s ViewSection, title: Option<&str>) -> &'s [ViewRow] {
    section
        .blocks
        .iter()
        .find_map(|b| match b {
            Block::Entities { title: t, rows } if t.as_deref() == title => Some(rows.as_slice()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no list {title:?} in {section:#?}"))
}

async fn paths(server: &MockServer) -> Vec<String> {
    server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .map(|r| r.url.path().to_owned())
        .collect()
}

#[tokio::test]
async fn a_feature_branch_reads_ahead_and_behind_its_default_branch() {
    let server = gitea(Served::default()).await;
    let view = source(&server)
        .view(FEATURE, None)
        .await
        .expect("the read succeeds")
        .expect("Gitea draws a branch");

    let ids: Vec<_> = view.sections.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(ids, ["overview", "commits", "diff"]);

    let overview = section(&view, "overview");
    assert_eq!(
        facts(overview),
        [
            ("Base".to_owned(), "main".to_owned()),
            ("Ahead".to_owned(), "2".to_owned()),
            ("Behind".to_owned(), "1".to_owned()),
        ]
    );
    assert_eq!(
        list(overview, Some("Last commit")),
        [ViewRow {
            entity_id: Some(format!("gitea:tidewater/payout-service@{TIP}")),
            title: "PAY-231: jitter in backoff, cap at 5 attempts".into(),
            meta: Some("c90d11a3f5 · Mara Lindqvist · 2026-08-22 11:42".into()),
            tone: None,
            chip: None,
            url: Some(format!(
                "https://gitea.example/tidewater/payout-service/commit/{TIP}"
            )),
        }]
    );
    assert_eq!(
        list(overview, Some("Pull request")),
        [ViewRow {
            entity_id: Some("gitea:tidewater/payout-service#142".into()),
            title: "SEPA retry with exponential backoff".into(),
            meta: Some("#142 · feature/PAY-231-sepa-retry → main · mara".into()),
            tone: Some(Tone::Info),
            chip: Some("open".into()),
            url: Some("https://gitea.example/tidewater/payout-service/pulls/142".into()),
        }],
        "the open pull request whose head is this branch, and no other"
    );

    let commits = section(&view, "commits");
    assert_eq!(commits.count, Some(2), "the count is ahead");
    assert!(!commits.deferred);
    let rows = list(commits, None);
    let titles: Vec<_> = rows
        .iter()
        .map(|r| (r.entity_id.as_deref().unwrap(), r.title.as_str()))
        .collect();
    assert_eq!(
        titles,
        [
            (
                "gitea:tidewater/payout-service@c90d11a3f5e2b7c4d9018e6a2b3c4d5e6f708192",
                "PAY-231: jitter in backoff, cap at 5 attempts"
            ),
            (
                "gitea:tidewater/payout-service@7be0e4a3f5e2b7c4d9018e6a2b3c4d5e6f708192",
                "PAY-231: retry SEPA payouts"
            ),
        ],
        "each commit an address, titled by its message's first line"
    );

    let diff = section(&view, "diff");
    assert!(diff.deferred, "a branch's diff is read when its tab opens");
    assert!(diff.blocks.is_empty());
    assert_eq!(diff.count, Some(2), "files changed, not file touches");
    assert!(
        !paths(&server).await.iter().any(|p| p.ends_with(".diff")),
        "the whole view reads no diff"
    );
}

/// The deferred read: the compare's own diff, from the API (`?output=diff`),
/// which a token can read on a private repository -- the web route's
/// `compare/…diff` answers a token 404 there (measured on 1.27.3).
#[tokio::test]
async fn the_diff_tab_reads_the_compares_diff() {
    let server = gitea(Served::default()).await;
    let view = source(&server)
        .view(FEATURE, Some("diff"))
        .await
        .unwrap()
        .expect("drawn");
    assert_eq!(view.sections.len(), 1);
    let diff = &view.sections[0];
    assert_eq!(diff.id, "diff");
    assert!(
        !diff.deferred,
        "the deferred read answers the blocks it deferred"
    );
    let Block::Diff { files, truncated } = &diff.blocks[0] else {
        panic!("a diff: {diff:#?}");
    };
    assert!(!truncated);
    let summary: Vec<_> = files
        .iter()
        .map(|f| (f.path.as_str(), f.change, f.additions, f.deletions))
        .collect();
    assert_eq!(
        summary,
        [
            ("src/retry.rs", DiffChange::Modified, 2, 1),
            ("CHANGELOG.md", DiffChange::Added, 1, 0),
        ]
    );
    assert_eq!(diff.count, Some(2));
}

#[tokio::test]
async fn a_branch_with_no_pull_request_says_so() {
    // A fork's branch of the same name is not this branch, and neither is
    // one whose repository Gitea no longer names (a deleted fork).
    let mut from_fork = pull(150, "From a fork", "feature/PAY-231-sepa-retry");
    from_fork["head"]["repo"] = json!({ "full_name": "jonas/payout-service" });
    let mut from_gone = pull(151, "From a deleted fork", "feature/PAY-231-sepa-retry");
    from_gone["head"]["repo"] = Value::Null;
    let server = gitea(Served {
        pulls: vec![
            pull(
                144,
                "Add payout CSV export",
                "feature/PAY-236-payout-csv-export",
            ),
            from_fork,
            from_gone,
        ],
    })
    .await;
    let view = source(&server).view(FEATURE, None).await.unwrap().unwrap();
    let overview = section(&view, "overview");
    let Some(Block::Facts { facts }) = overview.blocks.first() else {
        panic!("{overview:#?}");
    };
    assert_eq!(
        facts.last(),
        Some(&Fact {
            label: "Pull request".into(),
            value: "none".into(),
            tone: Some(Tone::Muted),
        })
    );
    assert!(
        !overview.blocks.iter().any(
            |b| matches!(b, Block::Entities { title, .. } if title.as_deref() == Some("Pull request"))
        ),
        "no empty list under the fact"
    );
}

/// The default branch is what every other branch is measured against: it
/// says so, and asks for no compare with itself.
#[tokio::test]
async fn the_default_branch_shows_no_compare() {
    let server = gitea(Served::default()).await;
    let view = source(&server).view(MAIN, None).await.unwrap().unwrap();
    let ids: Vec<_> = view.sections.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(ids, ["overview", "commits"], "no diff against itself");
    let overview = section(&view, "overview");
    assert_eq!(
        facts(overview)[0],
        ("Branch".to_owned(), "default branch".to_owned())
    );
    assert!(
        !facts(overview)
            .iter()
            .any(|(l, _)| l == "Ahead" || l == "Behind")
    );
    assert_eq!(
        list(overview, Some("Last commit"))[0].entity_id.as_deref(),
        Some("gitea:tidewater/payout-service@1111111111111111111111111111111111111111")
    );
    let commits = section(&view, "commits");
    assert_eq!(commits.count, None, "nothing to be ahead of");
    assert_eq!(list(commits, None).len(), 1);
    assert!(
        !paths(&server).await.iter().any(|p| p.contains("/compare/")),
        "{:?}",
        paths(&server).await
    );
    assert!(
        source(&server).view(MAIN, Some("diff")).await.is_err(),
        "the default branch has no diff tab to read"
    );
}

/// A branch Gitea no longer has is a failed read, which the detail draws as
/// *Showing the mirror* -- never a view of nothing.
#[tokio::test]
async fn a_branch_gitea_does_not_have_is_an_error() {
    let server = gitea(Served::default()).await;
    let gone = source(&server)
        .view(
            "gitea:tidewater/payout-service@refs/heads/feature/gone",
            None,
        )
        .await;
    assert!(
        matches!(gone, Err(SourceError::Protocol { .. })),
        "{gone:?}"
    );
}
