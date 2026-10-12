//! The pull request's source view against the docker-free fake (issue #662,
//! ADR-0020).
//!
//! The mapping and the deferral live here; whether the real server answers
//! these shapes is `tests/live_gitea.rs`'s to say
//! (`a_seeded_pull_requests_view_carries_its_files_commits_and_discussion`),
//! and on disagreement this fake is what is wrong.

use knobas_source::view::{Block, DiffChange, Fact, SourceView, Tone, ViewRow, ViewSection};
use knobas_source::{Source, SourceError};
use serde_json::{Value, json};
use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::support::{TOKEN, instance};

const REPO: &str = "/api/v1/repos/tidewater/payout-service";
const PR: &str = "gitea:tidewater/payout-service#142";
const HEAD_SHA: &str = "c90d11a3f5e2b7c4d9018e6a2b3c4d5e6f708192";
const FIRST_SHA: &str = "7be0e4c1d2e3f405162738495a6b7c8d9e0f1a2b";

/// What `git diff` printed for the pull request: two files added.
const DIFF: &str = "diff --git a/src/sepa/retry.rs b/src/sepa/retry.rs
new file mode 100644
index 0000000..6112a3c
--- /dev/null
+++ b/src/sepa/retry.rs
@@ -0,0 +1,2 @@
+pub fn backoff() {}
+pub fn jitter() {}
diff --git a/docs/backoff.md b/docs/backoff.md
new file mode 100644
index 0000000..5786b13
--- /dev/null
+++ b/docs/backoff.md
@@ -0,0 +1 @@
+Bounded to +/-10 %.
";

/// Gitea's pull-request record, as `/pulls/{n}` answers it, open and
/// not mergeable.
fn open_pull() -> Value {
    json!({
        "number": 142,
        "title": "SEPA retry with exponential backoff",
        "body": "Refs PAY-231\n\n<b>not markup</b>",
        "state": "open",
        "merged": false,
        "mergeable": false,
        "user": { "login": "mara.lindqvist" },
        "head": { "ref": "feature/PAY-231-sepa-retry", "sha": HEAD_SHA,
                  "repo": { "full_name": "tidewater/payout-service" } },
        "base": { "ref": "main", "repo": { "full_name": "tidewater/payout-service" } },
        "created_at": "2026-08-21T17:41:00Z",
        "updated_at": "2026-08-22T10:20:00Z",
        "html_url": "https://gitea.example/tidewater/payout-service/pulls/142",
        "changed_files": 2,
        "comments": 2
    })
}

/// Reviews, oldest first, as Gitea lists them. Priya asks for changes and
/// then approves; Jonas is asked and has not answered; Olu approves and then
/// comments, which does not take the approval back; a draft review is
/// nobody's verdict yet; a dismissed one no longer stands.
fn reviews() -> Value {
    json!([
        { "id": 1, "user": { "login": "priya.nair" }, "state": "REQUEST_CHANGES" },
        { "id": 2, "user": { "login": "jonas.becker" }, "state": "REQUEST_REVIEW" },
        { "id": 3, "user": { "login": "priya.nair" }, "state": "APPROVED" },
        { "id": 4, "user": { "login": "olu" }, "state": "APPROVED" },
        { "id": 5, "user": { "login": "olu" }, "state": "COMMENT" },
        { "id": 6, "user": { "login": "kai" }, "state": "PENDING" },
        { "id": 7, "user": { "login": "rae" }, "state": "REQUEST_CHANGES", "dismissed": true },
        { "id": 8, "user": { "login": "sam" }, "state": "COMMENT" }
    ])
}

fn pr_commits() -> Value {
    json!([
        {
            "sha": FIRST_SHA,
            "created": "2026-08-21T17:30:00Z",
            "html_url": format!("https://gitea.example/tidewater/payout-service/commit/{FIRST_SHA}"),
            "author": { "login": "mara.lindqvist" },
            "commit": {
                "message": "PAY-231: retry SEPA payouts on transient PSP errors\n\nlong body",
                "author": { "name": "Mara Lindqvist", "date": "2026-08-21T17:30:00Z" }
            }
        },
        {
            "sha": HEAD_SHA,
            "created": "2026-08-22T11:42:00Z",
            "commit": {
                "message": "PAY-231: jitter in backoff",
                "author": { "name": "Mara Lindqvist", "date": "2026-08-22T11:42:00Z" }
            }
        }
    ])
}

fn comments() -> Value {
    json!([
        { "id": 11, "user": { "login": "jonas.becker" }, "created_at": "2026-08-22T09:30:00Z",
          "body": "Should the jitter be bounded? <i>+/-20 %</i> feels wide." },
        { "id": 12, "user": { "login": "mara.lindqvist" }, "created_at": "2026-08-22T10:20:00Z",
          "body": "Bounded to +/-10 % in c90d11." }
    ])
}

/// A Gitea that answers every read the view makes, with `pull` as the
/// pull request's record and `reviews` as its reviews.
async fn gitea(pull: Value, reviews: Value) -> MockServer {
    let server = MockServer::start().await;
    let token = format!("token {TOKEN}");
    let get = |p: String| {
        Mock::given(method("GET"))
            .and(path(p))
            .and(header("Authorization", token.as_str()))
    };
    get(format!("{REPO}/pulls/142"))
        .respond_with(ResponseTemplate::new(200).set_body_json(pull))
        .mount(&server)
        .await;
    get(format!("{REPO}/pulls/142/reviews"))
        .respond_with(ResponseTemplate::new(200).set_body_json(reviews))
        .mount(&server)
        .await;
    get(format!("{REPO}/pulls/142/commits"))
        // As for `/commits`: the projections Gitea defaults to on are off.
        .and(query_param("files", "false"))
        .and(query_param("verification", "false"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("X-Total-Count", "2")
                .set_body_json(pr_commits()),
        )
        .mount(&server)
        .await;
    get(format!("{REPO}/issues/142/comments"))
        .respond_with(ResponseTemplate::new(200).set_body_json(comments()))
        .mount(&server)
        .await;
    get(format!("{REPO}/pulls/142.diff"))
        .respond_with(ResponseTemplate::new(200).set_body_string(DIFF))
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

fn facts_of(section: &ViewSection) -> &[Fact] {
    let Block::Facts { facts } = &section.blocks[0] else {
        panic!("{} opens on facts: {section:#?}", section.id);
    };
    facts
}

fn fact<'f>(facts: &'f [Fact], label: &str) -> Option<&'f Fact> {
    facts.iter().find(|f| f.label == label)
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
async fn a_pull_request_view_is_overview_files_commits_and_conversation() {
    let server = gitea(open_pull(), reviews()).await;
    let view = source(&server)
        .view(PR, None)
        .await
        .expect("the read succeeds")
        .expect("Gitea draws a pull request");

    let tabs: Vec<_> = view
        .sections
        .iter()
        .map(|s| (s.id.as_str(), s.title.as_str(), s.count, s.deferred))
        .collect();
    assert_eq!(
        tabs,
        [
            ("overview", "Overview", None, false),
            ("files", "Files", Some(2), true),
            ("commits", "Commits", Some(2), false),
            ("conversation", "Conversation", Some(2), false),
        ]
    );

    let overview = section(&view, "overview");
    let facts: Vec<_> = facts_of(overview)
        .iter()
        .map(|f| (f.label.as_str(), f.value.as_str(), f.tone))
        .collect();
    assert_eq!(
        facts,
        [
            ("State", "open", Some(Tone::Info)),
            ("Author", "mara.lindqvist", None),
            ("Branches", "feature/PAY-231-sepa-retry → main", None),
            ("Mergeable", "no", Some(Tone::Warn)),
            ("Created", "2026-08-21 17:41 +00:00", None),
            ("Updated", "2026-08-22 10:20 +00:00", None),
            ("Review", "approved by priya.nair", Some(Tone::Ok)),
            ("Review", "requested from jonas.becker", Some(Tone::Muted)),
            ("Review", "approved by olu", Some(Tone::Ok)),
            ("Review", "commented by sam", None),
        ]
    );
    assert_eq!(
        overview.blocks[1],
        Block::Text {
            text: "Refs PAY-231\n\n<b>not markup</b>".into(),
            markdown: true,
        },
        "the description, as text"
    );

    // Files is deferred: a stub, and the whole-view read never asked for the
    // diff.
    assert!(section(&view, "files").blocks.is_empty());
    let asked = paths(&server).await;
    assert!(
        !asked.iter().any(|p| p.ends_with(".diff")),
        "the first read asked for the diff: {asked:?}"
    );

    let Block::Entities { title, rows } = &section(&view, "commits").blocks[0] else {
        panic!("commits are an entity list: {view:#?}");
    };
    assert_eq!(title, &None);
    assert_eq!(
        rows,
        &[
            ViewRow {
                entity_id: Some(format!("gitea:tidewater/payout-service@{FIRST_SHA}")),
                title: "PAY-231: retry SEPA payouts on transient PSP errors".into(),
                meta: Some("Mara Lindqvist · 2026-08-21".into()),
                tone: None,
                chip: Some("7be0e4c1d2".into()),
                url: Some(format!(
                    "https://gitea.example/tidewater/payout-service/commit/{FIRST_SHA}"
                )),
            },
            ViewRow {
                entity_id: Some(format!("gitea:tidewater/payout-service@{HEAD_SHA}")),
                title: "PAY-231: jitter in backoff".into(),
                meta: Some("Mara Lindqvist · 2026-08-22".into()),
                tone: None,
                chip: Some("c90d11a3f5".into()),
                url: None,
            },
        ]
    );

    let conversation = &section(&view, "conversation").blocks;
    let drawn: Vec<_> = conversation
        .iter()
        .map(|b| match b {
            Block::Facts { facts } => facts
                .iter()
                .map(|f| format!("{}={}", f.label, f.value))
                .collect::<Vec<_>>()
                .join(" "),
            Block::Text { text, markdown } => format!("text({markdown}): {text}"),
            other => panic!("a conversation is facts and text: {other:?}"),
        })
        .collect();
    assert_eq!(
        drawn,
        [
            "jonas.becker=2026-08-22 09:30 +00:00",
            "text(true): Should the jitter be bounded? <i>+/-20 %</i> feels wide.",
            "mara.lindqvist=2026-08-22 10:20 +00:00",
            "text(true): Bounded to +/-10 % in c90d11.",
        ]
    );
}

/// The deferred *Files* tab, read on its own: exactly that section, from the
/// pull request's own `.diff`, and no other request.
#[tokio::test]
async fn the_files_tab_is_read_on_its_own_from_the_pull_requests_diff() {
    let server = gitea(open_pull(), reviews()).await;
    let view = source(&server)
        .view(PR, Some("files"))
        .await
        .unwrap()
        .expect("drawn");
    assert_eq!(view.sections.len(), 1);
    let files = &view.sections[0];
    assert_eq!(
        (files.id.as_str(), files.deferred, files.count),
        ("files", false, Some(2)),
        "the deferred read answers with the blocks, not another stub"
    );
    let Block::Diff { files, truncated } = &files.blocks[0] else {
        panic!("the files tab is a diff: {view:#?}");
    };
    assert!(!truncated);
    let summary: Vec<_> = files
        .iter()
        .map(|f| (f.path.as_str(), f.change, f.additions, f.deletions))
        .collect();
    assert_eq!(
        summary,
        [
            ("src/sepa/retry.rs", DiffChange::Added, 2, 0),
            ("docs/backoff.md", DiffChange::Added, 1, 0),
        ]
    );
    assert_eq!(paths(&server).await, [format!("{REPO}/pulls/142.diff")]);
}

/// A pull request nobody has reviewed draws no review facts at all -- not a
/// line saying *none*.
#[tokio::test]
async fn a_pull_request_with_no_reviews_shows_no_verdicts() {
    let server = gitea(open_pull(), json!([])).await;
    let view = source(&server).view(PR, None).await.unwrap().unwrap();
    let facts = facts_of(section(&view, "overview"));
    assert!(fact(facts, "Review").is_none(), "{facts:#?}");
    assert_eq!(fact(facts, "State").unwrap().value, "open");
}

/// Gitea calls a merged pull request `closed`; its merged flag is what tells
/// the two apart. Neither is asked whether it can be merged any more.
#[tokio::test]
async fn a_closed_pull_request_reads_closed_and_a_merged_one_merged() {
    let mut closed = open_pull();
    closed["state"] = json!("closed");
    closed["mergeable"] = json!(true);
    let server = gitea(closed, json!([])).await;
    let view = source(&server).view(PR, None).await.unwrap().unwrap();
    let facts = facts_of(section(&view, "overview"));
    let state = fact(facts, "State").unwrap();
    assert_eq!(
        (state.value.as_str(), state.tone),
        ("closed", Some(Tone::Muted))
    );
    assert!(fact(facts, "Mergeable").is_none(), "{facts:#?}");

    let mut merged = open_pull();
    merged["state"] = json!("closed");
    merged["merged"] = json!(true);
    merged["merged_at"] = json!("2026-08-23T08:00:00Z");
    merged["merged_by"] = json!({ "login": "priya.nair" });
    let server = gitea(merged, json!([])).await;
    let view = source(&server).view(PR, None).await.unwrap().unwrap();
    let facts = facts_of(section(&view, "overview"));
    let state = fact(facts, "State").unwrap();
    assert_eq!(
        (state.value.as_str(), state.tone),
        ("merged", Some(Tone::Ok))
    );
    assert_eq!(
        fact(facts, "Merged").map(|f| f.value.as_str()),
        Some("2026-08-23 08:00 +00:00 by priya.nair")
    );
    assert!(fact(facts, "Mergeable").is_none(), "{facts:#?}");
}

/// A pull request from a fork names the fork's repository before its branch,
/// since the branch name alone would point into the base repository.
#[tokio::test]
async fn a_head_in_another_repository_is_named_with_it() {
    let mut pull = open_pull();
    pull["head"]["repo"]["full_name"] = json!("olu/payout-service");
    pull["mergeable"] = json!(true);
    let server = gitea(pull, json!([])).await;
    let view = source(&server).view(PR, None).await.unwrap().unwrap();
    let facts = facts_of(section(&view, "overview"));
    assert_eq!(
        fact(facts, "Branches").unwrap().value,
        "olu/payout-service:feature/PAY-231-sepa-retry → main"
    );
    let mergeable = fact(facts, "Mergeable").unwrap();
    assert_eq!((mergeable.value.as_str(), mergeable.tone), ("yes", None));
}

/// A tab name the pull request's view does not have is refused, and so is a
/// pull request Gitea will not give: a failed read, never an empty view.
#[tokio::test]
async fn an_unknown_tab_and_a_refused_read_are_errors() {
    let server = gitea(open_pull(), reviews()).await;
    let source = source(&server);
    let refused = source.view(PR, Some("diff")).await;
    assert!(
        matches!(refused, Err(SourceError::Protocol { ref message, .. }) if message.contains("diff")),
        "{refused:?}"
    );
    let missing = source
        .view("gitea:tidewater/payout-service#999", None)
        .await;
    assert!(
        matches!(
            missing,
            Err(SourceError::Protocol {
                status: Some(404),
                ..
            })
        ),
        "{missing:?}"
    );
}
