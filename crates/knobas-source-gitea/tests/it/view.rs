//! The commit's source view against the docker-free fake (issue #661,
//! ADR-0020).
//!
//! The mapping and the error classification live here; whether the real
//! server answers these shapes is `tests/live_gitea.rs`'s to say
//! (`a_seeded_commits_view_carries_the_diff_the_seed_pushed`), and on
//! disagreement this fake is what is wrong.

use knobas_source::view::{Block, DiffChange, SourceView, ViewRow};
use knobas_source::{Source, SourceError};
use serde_json::json;
use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::support::{TOKEN, instance};

const REPO: &str = "/api/v1/repos/tidewater/payout-service";
const SHA: &str = "c90d11a3f5e2b7c4d9018e6a2b3c4d5e6f708192";
const PARENT: &str = "a41f2c8b7d6e5f403192837465a0b1c2d3e4f506";
const COMMIT: &str = "gitea:tidewater/payout-service@c90d11a3f5e2b7c4d9018e6a2b3c4d5e6f708192";

/// What `git diff` printed for this commit: one file modified, one added.
const DIFF: &str = "diff --git a/src/retry.rs b/src/retry.rs
index 84734ad..6112a3c 100644
--- a/src/retry.rs
+++ b/src/retry.rs
@@ -1,3 +1,4 @@ pub fn backoff() {
     let base = 30;
-    base * attempts
+    let jitter = 3;
+    base * attempts + jitter
 }
diff --git a/CHANGELOG.md b/CHANGELOG.md
new file mode 100644
index 0000000..5786b13
--- /dev/null
+++ b/CHANGELOG.md
@@ -0,0 +1 @@
+- jitter in backoff
";

/// Gitea's single-commit record, as `/git/commits/{sha}` answers it.
fn commit_record() -> serde_json::Value {
    json!({
        "sha": SHA,
        "html_url": format!("https://gitea.example/tidewater/payout-service/commit/{SHA}"),
        "author": { "login": "mara" },
        "commit": {
            "message": "PAY-231: jitter in backoff, cap at 5 attempts\n\nBounded to +/-10 %.\n<b>not markup</b>\n",
            "author": { "name": "Mara Lindqvist", "email": "mara@tidewater.example", "date": "2026-08-22T11:42:00+02:00" },
            "committer": { "name": "Gitea", "email": "noreply@gitea.example", "date": "2026-08-22T12:05:30Z" }
        },
        "parents": [{ "sha": PARENT, "url": "ignored", "created": "2026-08-22T10:02:00Z" }]
    })
}

/// A Gitea that answers the commit and its diff, `diff` with `status`.
async fn gitea(diff_status: u16) -> MockServer {
    let server = MockServer::start().await;
    let token = format!("token {TOKEN}");
    let authed = |m: wiremock::MockBuilder| m.and(header("Authorization", token.as_str()));
    authed(Mock::given(method("GET")).and(path(format!("{REPO}/git/commits/{SHA}"))))
        // The listing projections Gitea defaults to on are turned off here as
        // they are for `/commits`: a view reads its diff from `.diff`.
        .and(query_param("files", "false"))
        .respond_with(ResponseTemplate::new(200).set_body_json(commit_record()))
        .mount(&server)
        .await;
    let diff = if diff_status == 200 {
        ResponseTemplate::new(200).set_body_string(DIFF)
    } else {
        ResponseTemplate::new(diff_status).set_body_json(json!({ "message": "not found" }))
    };
    authed(Mock::given(method("GET")).and(path(format!("{REPO}/git/commits/{SHA}.diff"))))
        .respond_with(diff)
        .mount(&server)
        .await;
    server
}

fn source(server: &MockServer) -> Box<dyn Source> {
    knobas_source_gitea::build(instance(server.uri(), TOKEN, json!({}))).expect("builds")
}

fn section<'v>(view: &'v SourceView, id: &str) -> &'v knobas_source::view::ViewSection {
    view.sections
        .iter()
        .find(|s| s.id == id)
        .unwrap_or_else(|| panic!("no section {id:?} in {view:#?}"))
}

#[tokio::test]
async fn a_commit_view_is_its_overview_and_its_diff() {
    let server = gitea(200).await;
    let view = source(&server)
        .view(COMMIT, None)
        .await
        .expect("the read succeeds")
        .expect("Gitea draws a commit");

    let ids: Vec<_> = view.sections.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(
        ids,
        ["overview", "diff"],
        "the overview is the tab it opens on"
    );

    let overview = section(&view, "overview");
    assert_eq!(overview.title, "Overview");
    assert!(!overview.deferred);
    let Block::Facts { facts } = &overview.blocks[0] else {
        panic!("the overview opens on facts: {overview:#?}");
    };
    let facts: Vec<_> = facts
        .iter()
        .map(|f| (f.label.as_str(), f.value.as_str()))
        .collect();
    assert_eq!(
        facts,
        [
            ("Commit", "c90d11a3f5"),
            ("SHA", SHA),
            ("Author", "Mara Lindqvist <mara@tidewater.example>"),
            ("Authored", "2026-08-22 11:42 +02:00"),
            ("Committer", "Gitea <noreply@gitea.example>"),
            ("Committed", "2026-08-22 12:05 +00:00"),
        ]
    );
    let Block::Entities { title, rows } = &overview.blocks[1] else {
        panic!("then the parents: {overview:#?}");
    };
    assert_eq!(title.as_deref(), Some("Parent"), "one parent, one word");
    assert_eq!(
        rows,
        &[ViewRow {
            entity_id: Some(format!("gitea:tidewater/payout-service@{PARENT}")),
            title: "a41f2c8b7d".into(),
            meta: None,
            tone: None,
            chip: None,
            url: Some(format!(
                "https://gitea.example/tidewater/payout-service/commit/{PARENT}"
            )),
            commit: None,
        }]
    );
    assert_eq!(
        overview.blocks[2],
        Block::Text {
            text: "PAY-231: jitter in backoff, cap at 5 attempts\n\nBounded to +/-10 %.\n<b>not markup</b>"
                .into(),
            markdown: false,
        },
        "the whole message, as text"
    );

    let diff = section(&view, "diff");
    assert_eq!(diff.title, "Diff");
    assert_eq!(diff.count, Some(2), "files changed");
    let Block::Diff { files, truncated } = &diff.blocks[0] else {
        panic!("the diff tab is a diff: {diff:#?}");
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
}

/// A diff Gitea will not give is a failed read, never an empty diff: an
/// empty one would draw *no changes* over a commit that has some.
#[tokio::test]
async fn a_diff_gitea_refuses_is_an_error_and_not_an_empty_diff() {
    let server = gitea(404).await;
    let failed = source(&server).view(COMMIT, None).await;
    assert!(
        matches!(
            failed,
            Err(SourceError::Protocol {
                status: Some(404),
                ..
            })
        ),
        "{failed:?}"
    );
}

/// One section at a time, as a deferred read asks: the answer holds exactly
/// that section and costs only its own request.
#[tokio::test]
async fn one_section_is_read_on_its_own() {
    let server = gitea(200).await;
    let source = source(&server);
    let view = source
        .view(COMMIT, Some("diff"))
        .await
        .unwrap()
        .expect("drawn");
    assert_eq!(view.sections.len(), 1);
    assert_eq!(view.sections[0].id, "diff");
    let paths: Vec<String> = server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .map(|r| r.url.path().to_owned())
        .collect();
    assert_eq!(paths, [format!("{REPO}/git/commits/{SHA}.diff")]);

    let refused = source.view(COMMIT, Some("files")).await;
    assert!(
        matches!(refused, Err(SourceError::Protocol { ref message, .. }) if message.contains("files")),
        "{refused:?}"
    );
}

/// An id that is not this source's is refused rather than read. (Every Gitea
/// kind draws a view since #663, so none is left to answer `None`.)
#[tokio::test]
async fn a_foreign_id_is_refused() {
    let server = gitea(200).await;
    let source = source(&server);
    let foreign = source
        .view("gitea-eu:tidewater/payout-service@abc", None)
        .await;
    assert!(
        matches!(foreign, Err(SourceError::Protocol { ref message, .. }) if message.contains("gitea-eu")),
        "{foreign:?}"
    );
    assert!(server.received_requests().await.unwrap().is_empty());
}
