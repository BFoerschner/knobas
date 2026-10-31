//! A branch's source view against the docker-free fake (issue #663,
//! ADR-0020): how far it is from the repository's default branch, its last
//! commit, its open pull request, its commits and its diff against the
//! default branch.
//!
//! The shapes are Gitea 1.27.3's as measured on the pinned container; whether
//! the real server still answers them is `tests/live_gitea.rs`'s to say
//! (`a_seeded_branchs_view_reads_ahead_with_its_seeded_commits`), and on
//! disagreement this fake is what is wrong.

use std::sync::Arc;
use std::time::Duration;

use knobas_source::view::{
    Block, DiffChange, Fact, FieldKind, SourceView, Tone, ViewAction, ViewRow, ViewSection,
};
use knobas_source::{Source, SourceError};
use serde_json::{Value, json};
use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::support::{InFlight, TOKEN, instance};

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
    /// Holds every answer and counts the requests in flight; none by default.
    flight: Option<Arc<InFlight>>,
    /// Whether Gitea says the feature branch is protected.
    protected: bool,
    /// Whether the repository is a pull mirror.
    mirror: bool,
    /// `None` for Gitea; `Some(public)` for Forgejo (#1195), which answers
    /// `?output=diff` with the compare's JSON and serves the diff itself only
    /// on the web route, and that to a token only for a public repository.
    forgejo: Option<bool>,
    /// Forgejo's compare JSON says no commits and no files, and its web route
    /// answers that with the compare page in HTML (measured on 17.0.0).
    forgejo_nothing_ahead: bool,
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
            flight: None,
            protected: false,
            mirror: false,
            forgejo: None,
            forgejo_nothing_ahead: false,
        }
    }
}

async fn gitea(served: Served) -> MockServer {
    let server = MockServer::builder().start().await;
    let token = format!("token {TOKEN}");
    let authed = |m: wiremock::MockBuilder| m.and(header("Authorization", token.as_str()));
    if let Some(flight) = &served.flight {
        flight.watch(&server).await;
    }
    let delay = served.flight.as_ref().map_or(Duration::ZERO, |f| f.delay());
    let ok = |body: Value| {
        ResponseTemplate::new(200)
            .set_body_json(body)
            .set_delay(delay)
    };

    let mut record = repo_record();
    record["mirror"] = json!(served.mirror);
    authed(Mock::given(method("GET")).and(path(REPO)))
        .respond_with(ok(record))
        .mount(&server)
        .await;
    let mut feature = branch_record(
        "feature/PAY-231-sepa-retry",
        TIP,
        "PAY-231: jitter in backoff, cap at 5 attempts",
        "2026-08-22T11:42:00Z",
    );
    feature["protected"] = json!(served.protected);
    authed(Mock::given(method("GET")).and(path(format!("{REPO}/branches/{FEATURE_WIRE}"))))
        .respond_with(ok(feature))
        .mount(&server)
        .await;
    // The repository's branches, for a pull request's base: main and two
    // features, the listing in Gitea's own (name) order.
    authed(Mock::given(method("GET")).and(path(format!("{REPO}/branches"))))
        .and(query_param("page", "1"))
        .respond_with(ok(json!([
            branch_record(
                "feature/PAY-231-sepa-retry",
                TIP,
                "PAY-231: jitter",
                "2026-08-22T11:42:00Z"
            ),
            branch_record(
                "feature/PAY-236-payout-csv-export",
                FIRST,
                "PAY-236: csv",
                "2026-08-20T09:00:00Z"
            ),
            branch_record(
                "main",
                MAIN_TIP,
                "PAY-228 partial refund drift",
                "2026-08-21T16:00:00Z"
            ),
        ])))
        .mount(&server)
        .await;
    authed(Mock::given(method("GET")).and(path(format!("{REPO}/branches"))))
        .and(query_param("page", "2"))
        .respond_with(ok(json!([])))
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
    let compare_json = if served.forgejo_nothing_ahead {
        json!({"total_commits": 0, "commits": [], "files": []})
    } else {
        json!({"total_commits": 2, "commits": [], "files": []})
    }
    .to_string();
    let diff_body = if served.forgejo.is_some() {
        compare_json
    } else {
        DIFF.to_owned()
    };
    authed(Mock::given(method("GET")).and(path(format!("{REPO}/compare/main...{FEATURE_WIRE}"))))
        .and(query_param("output", "diff"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(diff_body)
                .set_delay(delay),
        )
        .with_priority(1)
        .mount(&server)
        .await;
    if let Some(public) = served.forgejo {
        authed(Mock::given(method("GET")).and(path(format!(
            "/tidewater/payout-service/compare/main...{FEATURE_WIRE}.diff"
        ))))
        .respond_with(if served.forgejo_nothing_ahead {
            ResponseTemplate::new(200).set_body_string("<!DOCTYPE html><title>Comparing</title>")
        } else if public {
            ResponseTemplate::new(200).set_body_string(DIFF)
        } else {
            ResponseTemplate::new(404).set_body_string("Not found.")
        })
        .mount(&server)
        .await;
    }
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
    // Behind: how many commits main has that the branch does not, as
    // Gitea's own count on a one-commit page (1.27.3 sends it on `/commits`).
    authed(Mock::given(method("GET")).and(path(format!("{REPO}/commits"))))
        .and(query_param("sha", "main"))
        .and(query_param("not", "feature/PAY-231-sepa-retry"))
        .and(query_param("limit", "1"))
        .respond_with(
            ok(json!([listed(
                MAIN_TIP,
                "PAY-228 partial refund drift",
                "2026-08-21T16:00:00Z",
                &[]
            )]))
            .insert_header("X-Total-Count", "1"),
        )
        .with_priority(2)
        .mount(&server)
        .await;
    // The same, as the reverse compare: read only when the count is missing.
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
    // The tip's own statuses (#710), asked by the branch's name so the read
    // goes beside the others: none, unless a test mounts some over this.
    for branch in [FEATURE_WIRE, "main"] {
        authed(Mock::given(method("GET")).and(path(format!("{REPO}/commits/{branch}/status"))))
            .respond_with(ok(json!({ "state": "", "total_count": 0, "statuses": [] })))
            .mount(&server)
            .await;
    }
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
    let Some(facts) = section.blocks.iter().find_map(|b| match b {
        Block::Facts { facts } => Some(facts),
        _ => None,
    }) else {
        panic!("the section has facts: {section:#?}");
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
    assert_eq!(ids, ["overview", "commits", "diff", "files"]);

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
            meta: Some("Mara Lindqvist · 2026-08-22".into()),
            tone: None,
            chip: Some("c90d11a3f5".into()),
            url: Some(format!(
                "https://gitea.example/tidewater/payout-service/commit/{TIP}"
            )),
            commit: None,
            author: Some("Mara Lindqvist".into()),
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
            commit: None,
            author: None,
        }],
        "the open pull request whose head is this branch, and no other"
    );

    let commits = section(&view, "commits");
    assert_eq!(
        commits.count,
        Some(2),
        "the count is what it lists: all it is ahead"
    );
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
    // The diff is the compare with `?output=diff`: its path is the JSON
    // compare's, so only the query tells the two reads apart.
    let diff_reads = server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|r| {
            r.url
                .query_pairs()
                .any(|(k, v)| k == "output" && v == "diff")
        })
        .count();
    assert_eq!(diff_reads, 0, "the whole view reads no diff");
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
    let Block::Diff {
        files, truncated, ..
    } = &diff.blocks[0]
    else {
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

/// Forgejo (#1195) ignores `?output=diff` and answers the compare as JSON;
/// read as a diff that is no files, *no changes* over a branch with some. The
/// diff is then read from the web route, which serves it to a token for a
/// public repository.
#[tokio::test]
async fn a_server_that_answers_the_compare_as_json_is_read_from_the_web_route() {
    let server = gitea(Served {
        forgejo: Some(true),
        ..Served::default()
    })
    .await;
    let view = source(&server)
        .view(FEATURE, Some("diff"))
        .await
        .unwrap()
        .expect("drawn");
    let Block::Diff { files, .. } = &view.sections[0].blocks[0] else {
        panic!("a diff: {view:#?}");
    };
    let paths: Vec<_> = files.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(paths, ["src/retry.rs", "CHANGELOG.md"]);
}

/// Where that web route refuses (Forgejo serves it to no token for a private
/// repository), the read says why; it never draws the JSON as an empty diff.
#[tokio::test]
async fn a_forgejo_private_repository_diff_is_refused_not_drawn_empty() {
    let server = gitea(Served {
        forgejo: Some(false),
        ..Served::default()
    })
    .await;
    let error = source(&server)
        .view(FEATURE, Some("diff"))
        .await
        .expect_err("no diff to draw");
    assert!(
        matches!(&error, SourceError::Protocol { message, .. } if message.contains("Forgejo")),
        "{error:?}"
    );
}

/// A web route that answers 200 with a page that is no diff (a sign-in page
/// reached by redirect on a server that requires sign-in to view) is refused
/// too: drawn, it would be the same silent *no changes*.
#[tokio::test]
async fn a_web_route_page_that_is_no_diff_is_refused_not_drawn_empty() {
    let server = gitea(Served {
        forgejo: Some(true),
        ..Served::default()
    })
    .await;
    Mock::given(method("GET"))
        .and(path(format!(
            "/tidewater/payout-service/compare/main...{FEATURE_WIRE}.diff"
        )))
        .respond_with(
            ResponseTemplate::new(200).set_body_string("<!DOCTYPE html><title>Sign in</title>"),
        )
        .with_priority(1)
        .mount(&server)
        .await;
    let error = source(&server)
        .view(FEATURE, Some("diff"))
        .await
        .expect_err("no diff to draw");
    assert!(
        matches!(&error, SourceError::Protocol { message, .. } if message.contains("not a diff")),
        "{error:?}"
    );
}

/// Nothing ahead on Forgejo is the empty diff, read from the compare's JSON:
/// its web route answers that case with the compare page, which is no diff.
#[tokio::test]
async fn a_forgejo_compare_with_nothing_ahead_is_the_empty_diff() {
    let server = gitea(Served {
        forgejo: Some(true),
        forgejo_nothing_ahead: true,
        ..Served::default()
    })
    .await;
    let view = source(&server)
        .view(FEATURE, Some("diff"))
        .await
        .unwrap()
        .expect("drawn");
    let Block::Diff { files, .. } = &view.sections[0].blocks[0] else {
        panic!("a diff: {view:#?}");
    };
    assert!(files.is_empty(), "{files:#?}");
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
        ..Served::default()
    })
    .await;
    let view = source(&server).view(FEATURE, None).await.unwrap().unwrap();
    let overview = section(&view, "overview");
    let Some(Block::Facts { facts }) = overview.blocks.first() else {
        panic!("with no pull request the overview opens on its facts: {overview:#?}");
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
    assert_eq!(
        ids,
        ["overview", "commits", "files"],
        "no diff against itself"
    );
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

/// *Behind* is a count, and is read as one: the reverse compare would list
/// every commit the default branch gained since the branch left it.
#[tokio::test]
async fn behind_is_read_as_a_count_not_as_the_reverse_compare() {
    let server = gitea(Served::default()).await;
    source(&server)
        .view(FEATURE, None)
        .await
        .expect("the read succeeds")
        .expect("Gitea draws a branch");
    let reverse = format!("{REPO}/compare/{FEATURE_WIRE}...main");
    assert!(
        !paths(&server).await.contains(&reverse),
        "asked for the reverse compare: {:?}",
        paths(&server).await
    );
}

/// A server that sends no count still gets a *Behind*: from the reverse
/// compare, which lists what the count would have counted.
#[tokio::test]
async fn behind_falls_back_to_the_reverse_compare_without_a_count() {
    let server = gitea(Served::default()).await;
    Mock::given(method("GET"))
        .and(path(format!("{REPO}/commits")))
        .and(query_param("not", "feature/PAY-231-sepa-retry"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .with_priority(1)
        .mount(&server)
        .await;
    let view = source(&server)
        .view(FEATURE, None)
        .await
        .expect("the read succeeds")
        .expect("Gitea draws a branch");
    assert!(
        facts(section(&view, "overview")).contains(&("Behind".to_owned(), "1".to_owned())),
        "{view:#?}"
    );
}

/// A branch's reads are made side by side, not one after another (#703):
/// once the record is in, its tip, its open pull requests, what it carries
/// ahead and how far it is behind are in flight together, the tip's statuses
/// (#710) with them. Read in turn, one is; with only ahead and behind side
/// by side, two.
#[tokio::test]
async fn a_branchs_reads_are_made_at_the_same_time() {
    let flight = InFlight::new(Duration::from_millis(300));
    let server = gitea(Served {
        flight: Some(Arc::clone(&flight)),
        ..Served::default()
    })
    .await;
    let view = source(&server)
        .view(FEATURE, None)
        .await
        .expect("reads")
        .expect("a branch draws a view");
    assert_eq!(
        view.sections.len(),
        4,
        "the deferred Files tab reads nothing"
    );
    assert_eq!(
        flight.most(),
        6,
        "the branch's reads in flight at once, its repository's branches among them"
    );
}

// -- a branch's life from its view (#707) ------------------------------------

/// The pull requests the default fake serves, without the feature's own.
fn without_its_pull() -> Served {
    Served {
        pulls: vec![pull(
            144,
            "Add payout CSV export",
            "feature/PAY-236-payout-csv-export",
        )],
        ..Served::default()
    }
}

async fn view_of(served: Served, entity: &str) -> SourceView {
    let server = gitea(served).await;
    source(&server)
        .view(entity, None)
        .await
        .expect("the read succeeds")
        .expect("Gitea draws a branch")
}

fn toolbar(view: &SourceView) -> Vec<&str> {
    view.actions.iter().map(|a| a.write_op.as_str()).collect()
}

fn action<'v>(view: &'v SourceView, op: &str) -> &'v ViewAction {
    view.actions
        .iter()
        .find(|a| a.write_op == op)
        .unwrap_or_else(|| panic!("no {op} in {:#?}", view.actions))
}

fn choice(action: &ViewAction, name: &str) -> Vec<String> {
    let field = action
        .fields
        .iter()
        .find(|f| f.name == name)
        .unwrap_or_else(|| panic!("no field {name} in {action:#?}"));
    let FieldKind::Choice { options } = &field.kind else {
        panic!("{name} is a choice: {field:#?}");
    };
    options.iter().map(|o| o.value.clone()).collect()
}

fn prefill<'a>(action: &'a ViewAction, name: &str) -> Option<&'a str> {
    action
        .fields
        .iter()
        .find(|f| f.name == name)
        .and_then(|f| f.value.as_deref())
}

/// The ruling on #707: a branch with an open pull request opens its overview
/// on that pull request, and its toolbar offers no second one.
#[tokio::test]
async fn a_branch_with_an_open_pull_request_opens_on_it_and_offers_none() {
    let view = view_of(Served::default(), FEATURE).await;
    let overview = section(&view, "overview");
    let Some(Block::Entities { title, rows }) = overview.blocks.first() else {
        panic!("the first block is the pull request: {overview:#?}");
    };
    assert_eq!(title.as_deref(), Some("Pull request"));
    let row: Vec<_> = rows
        .iter()
        .map(|r| (r.entity_id.as_deref(), r.chip.as_deref(), r.title.as_str()))
        .collect();
    assert_eq!(
        row,
        [(
            Some("gitea:tidewater/payout-service#142"),
            Some("open"),
            "SEPA retry with exponential backoff"
        )]
    );
    assert_eq!(toolbar(&view), ["create_branch", "delete_branch"]);
}

/// *New branch…* on a branch: created in its repository, from this branch.
#[tokio::test]
async fn a_new_branch_from_a_branch_starts_at_it_in_its_repository() {
    let view = view_of(Served::default(), FEATURE).await;
    let new = action(&view, "create_branch");
    assert_eq!(new.label, "New branch…");
    assert_eq!(
        new.entity.as_deref(),
        Some("gitea:tidewater/payout-service")
    );
    let name = &new.fields[0];
    assert_eq!(
        (name.name.as_str(), name.required, &name.kind),
        (
            "name",
            true,
            &FieldKind::Text {
                suggestions: vec![]
            }
        )
    );
    assert_eq!(choice(new, "from_ref"), ["feature/PAY-231-sepa-retry"]);
    assert!(new.disabled.is_none() && !new.danger);
}

/// *Open pull request…* on a branch with no pull request that is ahead:
/// prefilled from its commits, its head fixed, the default branch first
/// among the bases and the branch itself not among them.
#[tokio::test]
async fn a_branch_ahead_with_no_pull_request_offers_one_prefilled_from_its_commits() {
    let view = view_of(without_its_pull(), FEATURE).await;
    assert_eq!(
        toolbar(&view),
        ["create_pull_request", "create_branch", "delete_branch"]
    );
    let open = action(&view, "create_pull_request");
    assert_eq!(open.label, "Open pull request…");
    assert!(open.primary && open.disabled.is_none());
    assert_eq!(
        open.entity.as_deref(),
        Some("gitea:tidewater/payout-service")
    );
    // Two commits: the title is the branch's name, made readable.
    assert_eq!(prefill(open, "title"), Some("PAY-231 sepa retry"));
    assert_eq!(
        prefill(open, "body"),
        Some("- PAY-231: jitter in backoff, cap at 5 attempts\n- PAY-231: retry SEPA payouts")
    );
    assert_eq!(choice(open, "head"), ["feature/PAY-231-sepa-retry"]);
    assert_eq!(
        choice(open, "base"),
        ["main", "feature/PAY-236-payout-csv-export"]
    );
    let names: Vec<_> = open.fields.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, ["title", "body", "head", "base"]);
}

/// One commit ahead: the title is that commit's subject.
#[tokio::test]
async fn a_branch_one_commit_ahead_titles_its_pull_request_by_that_commit() {
    let server = gitea(without_its_pull()).await;
    Mock::given(method("GET"))
        .and(path(format!("{REPO}/compare/main...{FEATURE_WIRE}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "total_commits": 1,
            "commits": [listed(TIP, "PAY-231: jitter in backoff", "2026-08-22T11:42:00Z", &["src/retry.rs"])]
        })))
        .with_priority(2)
        .mount(&server)
        .await;
    let view = source(&server).view(FEATURE, None).await.unwrap().unwrap();
    let open = action(&view, "create_pull_request");
    assert_eq!(prefill(open, "title"), Some("PAY-231: jitter in backoff"));
    assert_eq!(prefill(open, "body"), Some("- PAY-231: jitter in backoff"));
}

/// Nothing ahead: nothing to propose, and the delete says it loses nothing.
#[tokio::test]
async fn a_branch_with_nothing_ahead_has_nothing_to_propose_and_is_merged() {
    let server = gitea(without_its_pull()).await;
    Mock::given(method("GET"))
        .and(path(format!("{REPO}/compare/main...{FEATURE_WIRE}")))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({ "total_commits": 0, "commits": [] })),
        )
        .with_priority(2)
        .mount(&server)
        .await;
    let view = source(&server).view(FEATURE, None).await.unwrap().unwrap();
    assert_eq!(
        action(&view, "create_pull_request").disabled.as_deref(),
        Some("No commits ahead of main")
    );
    let delete = action(&view, "delete_branch");
    assert_eq!(
        delete.confirm.as_deref(),
        Some("Delete feature/PAY-231-sepa-retry? It is merged into main: nothing on it is lost.")
    );
}

/// *Delete branch…*: a danger, its sentence saying what an unmerged branch
/// loses, and targeting the view's own entity.
#[tokio::test]
async fn deleting_an_unmerged_branch_says_what_only_its_sha_will_reach() {
    let view = view_of(Served::default(), FEATURE).await;
    let delete = action(&view, "delete_branch");
    assert_eq!(delete.label, "Delete branch…");
    assert!(delete.danger && delete.fields.is_empty() && delete.entity.is_none());
    assert_eq!(
        delete.confirm.as_deref(),
        Some(
            "Delete feature/PAY-231-sepa-retry? Not merged into main: 2 commits will only be \
             reachable by SHA."
        )
    );
    assert_eq!(delete.disabled, None);
}

/// The default branch never can be deleted, so it offers no delete (the
/// #701 ruling); a protected one can be, once its protection is lifted, so
/// it offers the delete disabled and says how.
#[tokio::test]
async fn the_default_branch_offers_no_delete_and_a_protected_one_says_why_not_yet() {
    let main = view_of(Served::default(), MAIN).await;
    assert_eq!(
        toolbar(&main),
        ["create_branch"],
        "nothing to propose from the default branch, and nothing to delete"
    );
    assert_eq!(choice(action(&main, "create_branch"), "from_ref"), ["main"]);

    let protected = view_of(
        Served {
            protected: true,
            ..Served::default()
        },
        FEATURE,
    )
    .await;
    assert_eq!(
        action(&protected, "delete_branch").disabled.as_deref(),
        Some("Protected branch: lift its protection on Gitea to delete it")
    );
}

/// A branch of a pull mirror proposes nothing (the mirror takes no pull
/// requests) and draws its branch writes disabled: Gitea answers both 403,
/// and a mirror can be turned into a plain repository.
#[tokio::test]
async fn a_mirrors_branch_proposes_nothing_and_says_why_it_takes_no_branch_writes() {
    let view = view_of(
        Served {
            pulls: Vec::new(),
            mirror: true,
            ..Served::default()
        },
        FEATURE,
    )
    .await;
    assert_eq!(toolbar(&view), ["create_branch", "delete_branch"]);
    for op in ["create_branch", "delete_branch"] {
        assert_eq!(
            action(&view, op).disabled.as_deref(),
            Some("A mirror: its branches come from upstream"),
            "{op}"
        );
    }
}

/// A section read carries no toolbar: the toolbar is the whole view's.
#[tokio::test]
async fn a_section_read_carries_no_toolbar() {
    let server = gitea(Served::default()).await;
    let view = source(&server)
        .view(FEATURE, Some("commits"))
        .await
        .unwrap()
        .unwrap();
    assert!(view.actions.is_empty(), "{:#?}", view.actions);
}

// -- Checks (issue #710) -----------------------------------------------------

/// The branch's tip's statuses are its *Checks*, on *Overview* right after
/// the facts, failing first; read by the branch's name beside the view's
/// other reads.
#[tokio::test]
async fn a_branchs_tip_statuses_are_its_checks() {
    let server = gitea(Served::default()).await;
    Mock::given(method("GET"))
        .and(path(format!("{REPO}/commits/{FEATURE_WIRE}/status")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "state": "pending", "sha": TIP, "total_count": 2,
            "statuses": [
                { "status": "success", "context": "ci/build", "description": "passed" },
                { "status": "pending", "context": "ci/deploy", "description": null }
            ]
        })))
        .with_priority(2)
        .mount(&server)
        .await;
    let view = source(&server).view(FEATURE, None).await.unwrap().unwrap();
    let overview = section(&view, "overview");
    let at = overview
        .blocks
        .iter()
        .position(|b| matches!(b, Block::Facts { .. }))
        .expect("the overview has its facts");
    let Block::Entities { title, rows } = &overview.blocks[at + 1] else {
        panic!("the checks follow the facts: {overview:#?}");
    };
    assert_eq!(title.as_deref(), Some("Checks"));
    let drawn: Vec<_> = rows
        .iter()
        .map(|r| (r.title.as_str(), r.meta.as_deref(), r.tone))
        .collect();
    assert_eq!(
        drawn,
        [
            ("ci/deploy", None, Some(Tone::Info)),
            ("ci/build", Some("passed"), Some(Tone::Ok)),
        ]
    );
}

/// A tip nobody reported on draws no *Checks*.
#[tokio::test]
async fn a_branch_without_statuses_draws_no_checks() {
    let server = gitea(Served::default()).await;
    let view = source(&server).view(FEATURE, None).await.unwrap().unwrap();
    let overview = section(&view, "overview");
    assert!(
        !overview
            .blocks
            .iter()
            .any(|b| matches!(b, Block::Entities { title: Some(t), .. } if t == "Checks")),
        "{overview:#?}"
    );
}

/// A tip whose statuses cannot be read does not fail the branch's view
/// (#710 ruling): one muted line after the facts says so.
#[tokio::test]
async fn a_branch_whose_checks_cannot_be_read_still_draws_and_says_so() {
    let server = gitea(Served::default()).await;
    Mock::given(method("GET"))
        .and(path(format!("{REPO}/commits/{FEATURE_WIRE}/status")))
        .respond_with(ResponseTemplate::new(404))
        .with_priority(2)
        .mount(&server)
        .await;
    let view = source(&server).view(FEATURE, None).await.unwrap().unwrap();
    let overview = section(&view, "overview");
    let at = overview
        .blocks
        .iter()
        .position(|b| matches!(b, Block::Facts { .. }))
        .expect("the overview has its facts");
    let Block::Entities { title: None, rows } = &overview.blocks[at + 1] else {
        panic!("the line follows the facts: {overview:#?}");
    };
    let drawn: Vec<_> = rows
        .iter()
        .map(|r| {
            (
                r.title.starts_with("Checks could not be read: "),
                r.tone,
                &r.chip,
            )
        })
        .collect();
    assert_eq!(drawn, [(true, Some(Tone::Muted), &None)], "{rows:#?}");
}
