//! The commit's source view against the docker-free fake (issue #661,
//! ADR-0020).
//!
//! The mapping and the error classification live here; whether the real
//! server answers these shapes is `tests/live_gitea.rs`'s to say
//! (`a_seeded_commits_view_carries_the_diff_the_seed_pushed`), and on
//! disagreement this fake is what is wrong.

use knobas_source::view::{Block, DiffChange, SourceView, Tone, ViewRow};
use knobas_source::{Source, SourceError};
use serde_json::json;
use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::support::{TOKEN, instance, write_probe};

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

/// A Gitea that answers the commit and its diff, `diff` with `status`, and
/// whose combined status for the commit has no statuses in it.
async fn gitea(diff_status: u16) -> MockServer {
    gitea_with_statuses(diff_status, json!([])).await
}

/// [`gitea`], with `statuses` as the commit's combined status (#710).
async fn gitea_with_statuses(diff_status: u16, statuses: serde_json::Value) -> MockServer {
    gitea_full(diff_status, statuses, false).await
}

/// [`gitea`], the repository a pull mirror where `mirror` (#722).
async fn gitea_of(diff_status: u16, mirror: bool) -> MockServer {
    gitea_full(diff_status, json!([]), mirror).await
}

/// [`gitea_with_statuses`] and [`gitea_of`] in one.
async fn gitea_full(diff_status: u16, statuses: serde_json::Value, mirror: bool) -> MockServer {
    let server = MockServer::builder().start().await;
    let token = format!("token {TOKEN}");
    let authed = |m: wiremock::MockBuilder| m.and(header("Authorization", token.as_str()));
    authed(Mock::given(method("GET")).and(path(REPO)))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "name": "payout-service",
            "full_name": "tidewater/payout-service",
            "owner": { "login": "tidewater" },
            "default_branch": "main",
            "mirror": mirror,
        })))
        .mount(&server)
        .await;
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
    authed(Mock::given(method("GET")).and(path(format!("{REPO}/commits/{SHA}/status"))))
        .respond_with(ResponseTemplate::new(200).set_body_json(combined_status(&statuses)))
        .mount(&server)
        .await;
    server
}

/// Gitea's `CombinedStatus` for [`SHA`] around `statuses`, as
/// `/commits/{ref}/status` answers it.
fn combined_status(statuses: &serde_json::Value) -> serde_json::Value {
    let count = statuses.as_array().map_or(0, Vec::len);
    json!({
        "state": if count == 0 { "" } else { "failure" },
        "sha": SHA,
        "total_count": count,
        "statuses": statuses,
    })
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
    let source = source(&server);
    let view = source
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
            author: None,
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

    // The diff is deferred (#2319): the commit tab reads it local-first, at a
    // checkout that holds the commit, so the whole view does not read it too.
    let diff = section(&view, "diff");
    assert_eq!(diff.title, "Diff");
    assert!(diff.deferred, "{diff:#?}");
    assert!(diff.blocks.is_empty(), "{diff:#?}");
    let paths: Vec<String> = server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .map(|r| r.url.path().to_owned())
        .collect();
    assert!(
        !paths.iter().any(|p| p.ends_with(".diff")),
        "the whole view read the diff: {paths:?}"
    );

    let view = source
        .view(COMMIT, Some("diff"))
        .await
        .expect("the read succeeds")
        .expect("Gitea draws a commit's diff");
    let diff = section(&view, "diff");
    assert_eq!(diff.title, "Diff");
    assert!(!diff.deferred);
    assert_eq!(diff.count, Some(2), "files changed");
    let Block::Diff {
        files, truncated, ..
    } = &diff.blocks[0]
    else {
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
    let failed = source(&server).view(COMMIT, Some("diff")).await;
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

    // `files` is a commit's section since #2239 (its tree at the sha); a
    // name no commit's view has is refused.
    let refused = source.view(COMMIT, Some("branches")).await;
    assert!(
        matches!(refused, Err(SourceError::Protocol { ref message, .. }) if message.contains("branches")),
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

/// *New branch…* on a commit (#707): created in its repository, from this
/// commit, its full sha the ref and its short one the label. A section read
/// carries no toolbar.
#[tokio::test]
async fn a_commit_offers_a_new_branch_from_itself() {
    let server = gitea(200).await;
    let view = source(&server).view(COMMIT, None).await.unwrap().unwrap();
    let ops: Vec<_> = view.actions.iter().map(|a| a.write_op.as_str()).collect();
    assert_eq!(ops, ["create_branch"]);
    let new = &view.actions[0];
    assert_eq!(new.label, "New branch…");
    assert_eq!(
        new.entity.as_deref(),
        Some("gitea:tidewater/payout-service")
    );
    let from = new.fields.iter().find(|f| f.name == "from_ref").unwrap();
    assert_eq!(
        from.kind,
        knobas_source::view::FieldKind::Choice {
            options: vec![knobas_source::view::ChoiceOption::new(SHA, "c90d11a3f5")]
        }
    );
    let one = source(&server)
        .view(COMMIT, Some("diff"))
        .await
        .unwrap()
        .unwrap();
    assert!(one.actions.is_empty());
}

/// The commit's own statuses (#710): Gitea's combined status drawn as a
/// *Checks* list right after the facts, the failing first and the passing
/// last, each its context, its description, its state as a toned chip and
/// the page it points at.
#[tokio::test]
async fn a_commits_statuses_are_its_checks_failing_first() {
    let status = |state: &str, context: &str, description: &str, url: Option<&str>| {
        json!({ "id": 1, "status": state, "context": context, "description": description,
                "target_url": url, "created_at": "2026-08-22T12:00:00Z" })
    };
    let server = gitea_with_statuses(
        200,
        json!([
            status(
                "success",
                "ci/build",
                "Build #88 passed",
                Some("https://ci.example/88")
            ),
            status("pending", "ci/deploy", "Waiting for an agent", None),
            status(
                "failure",
                "ci/tests",
                "Tests failed: 1",
                Some("https://ci.example/89")
            ),
            status("warning", "lint", "", None),
            status("error", "ci/docs", "Could not start", None),
        ]),
    )
    .await;
    let view = source(&server).view(COMMIT, None).await.unwrap().unwrap();
    let overview = section(&view, "overview");
    let Block::Entities { title, rows } = &overview.blocks[1] else {
        panic!("the checks come right after the facts: {overview:#?}");
    };
    assert_eq!(title.as_deref(), Some("Checks"));
    let row =
        |title: &str, meta: Option<&str>, tone: Tone, chip: &str, url: Option<&str>| ViewRow {
            entity_id: None,
            title: title.into(),
            meta: meta.map(Into::into),
            tone: Some(tone),
            chip: Some(chip.into()),
            url: url.map(Into::into),
            commit: None,
            author: None,
        };
    assert_eq!(
        rows,
        &[
            row(
                "ci/tests",
                Some("Tests failed: 1"),
                Tone::Err,
                "failure",
                Some("https://ci.example/89")
            ),
            row("ci/docs", Some("Could not start"), Tone::Err, "error", None),
            row("lint", None, Tone::Warn, "warning", None),
            row(
                "ci/deploy",
                Some("Waiting for an agent"),
                Tone::Info,
                "pending",
                None
            ),
            row(
                "ci/build",
                Some("Build #88 passed"),
                Tone::Ok,
                "success",
                Some("https://ci.example/88")
            ),
        ]
    );
}

/// A commit nobody reported on draws no *Checks* at all: no title over
/// nothing, and the parents stay where they were.
#[tokio::test]
async fn a_commit_without_statuses_draws_no_checks() {
    let server = gitea(200).await;
    let view = source(&server).view(COMMIT, None).await.unwrap().unwrap();
    let overview = section(&view, "overview");
    assert!(
        !overview.blocks.iter().any(|b| matches!(
            b,
            Block::Entities { title: Some(t), .. } if t == "Checks"
        )),
        "{overview:#?}"
    );
}

/// A status read that fails does not fail the commit's view (#710 ruling):
/// checks are secondary, so the view draws without them and says why in one
/// muted line in their place, right after the facts.
#[tokio::test]
async fn a_commit_whose_statuses_cannot_be_read_still_draws_and_says_so() {
    let server = gitea(200).await;
    Mock::given(method("GET"))
        .and(path(format!("{REPO}/commits/{SHA}/status")))
        .respond_with(ResponseTemplate::new(500))
        .with_priority(1)
        .mount(&server)
        .await;
    let view = source(&server).view(COMMIT, None).await.unwrap().unwrap();
    let overview = section(&view, "overview");
    assert!(
        matches!(overview.blocks[0], Block::Facts { .. }),
        "{overview:#?}"
    );
    let Block::Entities { title, rows } = &overview.blocks[1] else {
        panic!("the line sits where the checks would: {overview:#?}");
    };
    assert_eq!(title, &None);
    assert_eq!(rows.len(), 1);
    assert!(
        rows[0].title.starts_with("Checks could not be read: "),
        "{:?}",
        rows[0].title
    );
    assert_eq!(rows[0].tone, Some(Tone::Muted));
    assert_eq!((&rows[0].chip, &rows[0].url), (&None, &None));
}

/// A mirror's commit offers *New branch…* disabled, with the reason its branch
/// and repository views give (#722): Gitea refuses a branch on a mirror, so
/// the view must not offer one it will refuse. A plain repository's commit
/// offers it enabled.
#[tokio::test]
async fn a_mirrors_commit_offers_a_new_branch_disabled() {
    let server = gitea_of(200, true).await;
    let view = source(&server).view(COMMIT, None).await.unwrap().unwrap();
    let new = view
        .actions
        .iter()
        .find(|a| a.write_op == "create_branch")
        .expect("a mirror's commit still shows New branch…");
    assert_eq!(
        new.disabled.as_deref(),
        Some("A mirror: its branches come from upstream")
    );

    let server = gitea_of(200, false).await;
    let view = source(&server).view(COMMIT, None).await.unwrap().unwrap();
    assert_eq!(view.actions[0].disabled, None);
}

/// Issue #2030: a token that may only read draws *New branch…* disabled, with
/// what the token lacks, and the commit's reads are unchanged. Gitea is asked
/// with a branch write whose empty body it refuses whatever the token: for
/// the scope where it lacks it, for the missing name where it has it.
#[tokio::test]
async fn a_read_only_token_offers_a_new_branch_disabled_saying_why() {
    let server = gitea(200).await;
    write_probe(
        &server,
        format!("{REPO}/branches"),
        403,
        "token does not have at least one of required scope(s), required=[write:repository], \
         token scope=read:repository,read:user",
    )
    .await;
    let view = source(&server).view(COMMIT, None).await.unwrap().unwrap();
    assert_eq!(
        view.actions[0].disabled.as_deref(),
        Some("This token cannot write: it lacks the write:repository scope")
    );
    assert_eq!(
        section(&view, "overview").blocks.len(),
        3,
        "the reads are as drawn"
    );

    let server = gitea(200).await;
    write_probe(
        &server,
        format!("{REPO}/branches"),
        422,
        "[BranchName]: Required",
    )
    .await;
    let view = source(&server).view(COMMIT, None).await.unwrap().unwrap();
    assert_eq!(view.actions[0].disabled, None, "a token that may write");
}

/// A section read with no write in it asks Gitea nothing more (#2030).
#[tokio::test]
async fn a_view_without_writes_does_not_ask_what_the_token_may_write() {
    let server = gitea(200).await;
    source(&server)
        .view(COMMIT, Some("diff"))
        .await
        .unwrap()
        .unwrap();
    let posts = server
        .received_requests()
        .await
        .unwrap()
        .into_iter()
        .filter(|r| r.method == wiremock::http::Method::POST)
        .count();
    assert_eq!(posts, 0);
}
