//! The pull request's source view against the docker-free fake (issue #662,
//! ADR-0020).
//!
//! The mapping and the deferral live here; whether the real server answers
//! these shapes is `tests/live_gitea.rs`'s to say
//! (`a_seeded_pull_requests_view_carries_its_files_commits_and_discussion`),
//! and on disagreement this fake is what is wrong.

use std::sync::Arc;
use std::time::Duration;

use knobas_source::ReviewSide;
use knobas_source::view::{
    ActionField, Block, ChoiceOption, Commenting, DiffChange, Fact, FieldKind, FileThread,
    LineComment, LineComments, LineThread, SourceView, Tone, ViewAction, ViewRow, ViewSection,
};
use knobas_source::{Source, SourceError};
use serde_json::{Value, json};
use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::support::{InFlight, TOKEN, instance, write_probe};

const REPO: &str = "/api/v1/repos/tidewater/payout-service";
/// The owning organisation's labels (#1235).
const ORG: &str = "/api/v1/orgs/tidewater/labels";
const PR: &str = "gitea:tidewater/payout-service#142";
const HEAD_SHA: &str = "c90d11a3f5e2b7c4d9018e6a2b3c4d5e6f708192";
/// The account the token belongs to: not the pull request's author.
///
/// The adapter remembers who a token is for the life of the process, by
/// instance, base URL and token (#705), and wiremock hands one test's port to
/// the next. So every test here whose `/user` answers anything but `VIEWER`
/// to [`TOKEN`] -- another login, a failure -- or counts the `/user` reads,
/// uses a token of its own.
const VIEWER: &str = "knobas";
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
        "comments": 2,
        "labels": [{ "id": 3, "name": "bug" }]
    })
}

/// Reviews, oldest first, as Gitea lists them. Priya asks for changes and
/// then approves; Jonas is asked and has not answered; Olu approves and then
/// comments, which does not take the approval back; a draft review is
/// nobody's verdict yet; Rae's dismissed review takes her earlier approval
/// with it, as Gitea's dismissal leaves her no standing verdict; Lee's request
/// for changes stands.
fn reviews() -> Value {
    json!([
        { "id": 1, "user": { "login": "priya.nair" }, "state": "REQUEST_CHANGES",
          "submitted_at": "2026-08-22T09:00:00Z", "body": "Bound the jitter." },
        { "id": 2, "user": { "login": "jonas.becker" }, "state": "REQUEST_REVIEW", "body": "" },
        { "id": 3, "user": { "login": "priya.nair" }, "state": "APPROVED",
          "submitted_at": "2026-08-22T11:00:00Z", "body": "" },
        { "id": 4, "user": { "login": "olu" }, "state": "APPROVED",
          "submitted_at": "2026-08-22T11:05:00Z" },
        { "id": 5, "user": { "login": "olu" }, "state": "COMMENT",
          "submitted_at": "2026-08-22T11:10:00Z", "body": "nit: name it `jitter_bound`" },
        { "id": 6, "user": { "login": "kai" }, "state": "PENDING", "body": "draft" },
        { "id": 7, "user": { "login": "rae" }, "state": "APPROVED",
          "submitted_at": "2026-08-22T08:00:00Z" },
        { "id": 8, "user": { "login": "rae" }, "state": "REQUEST_CHANGES", "dismissed": true,
          "submitted_at": "2026-08-22T08:30:00Z" },
        { "id": 9, "user": { "login": "sam" }, "state": "COMMENT",
          "submitted_at": "2026-08-22T10:00:00+02:00" },
        { "id": 10, "user": { "login": "lee" }, "state": "REQUEST_CHANGES",
          "submitted_at": "2026-08-22T12:00:00Z" }
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
    slow_gitea(pull, reviews, None, TOKEN).await
}

/// [`gitea`], with `flight` holding every answer and counting the requests in
/// flight.
async fn slow_gitea(
    pull: Value,
    reviews: Value,
    flight: Option<&Arc<InFlight>>,
    token: &str,
) -> MockServer {
    let server = MockServer::builder().start().await;
    if let Some(flight) = flight {
        flight.watch(&server).await;
    }
    let delay = flight.map_or(Duration::ZERO, |f| f.delay());
    let token = format!("token {token}");
    let get = |p: String| {
        Mock::given(method("GET"))
            .and(path(p))
            .and(header("Authorization", token.as_str()))
    };
    get(format!("{REPO}/pulls/142"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(pull)
                .set_delay(delay),
        )
        .mount(&server)
        .await;
    get(format!("{REPO}/pulls/142/reviews"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(reviews)
                .set_delay(delay),
        )
        .mount(&server)
        .await;
    get(format!("{REPO}/pulls/142/commits"))
        // As for `/commits`: the projections Gitea defaults to on are off.
        .and(query_param("files", "false"))
        .and(query_param("verification", "false"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("X-Total-Count", "2")
                .set_body_json(pr_commits())
                .set_delay(delay),
        )
        .mount(&server)
        .await;
    get(format!("{REPO}/issues/142/comments"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(comments())
                .set_delay(delay),
        )
        .mount(&server)
        .await;
    // Who the token is: the review actions are not offered on its own pull
    // requests (#705).
    get("/api/v1/user".to_owned())
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({ "login": VIEWER }))
                .set_delay(delay),
        )
        .mount(&server)
        .await;
    // The repository's labels, which *Add label…* offers (#1194): `bug` is on
    // the pull request already. Gitea counts them (#1235).
    get(format!("{REPO}/labels"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("X-Total-Count", "2")
                .set_body_json(json!([{ "id": 3, "name": "bug" }, { "id": 7, "name": "needs-qa" }]))
                .set_delay(delay),
        )
        .mount(&server)
        .await;
    // The head commit's own statuses (#710): none, unless a test mounts some
    // over this.
    get(format!("{REPO}/commits/{HEAD_SHA}/status"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({ "state": "", "sha": HEAD_SHA, "total_count": 0,
                                       "statuses": [] }))
                .set_delay(delay),
        )
        .mount(&server)
        .await;
    get(format!("{REPO}/pulls/142.diff"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(DIFF)
                .set_delay(delay),
        )
        .mount(&server)
        .await;
    // What landing reads (#708): the base and the head branch, unprotected
    // and asking for no approvals, and whether the base has a commit the
    // head lacks -- none, so the head is level.
    get(format!("{REPO}/branches/main"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(branch("main", false))
                .set_delay(delay),
        )
        .mount(&server)
        .await;
    get(format!("{REPO}/branches/{HEAD_WIRE}"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(branch("feature/PAY-231-sepa-retry", false))
                .set_delay(delay),
        )
        .mount(&server)
        .await;
    get(format!("{REPO}/commits"))
        .and(query_param("sha", "main"))
        .and(query_param("not", HEAD_SHA))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!([]))
                .set_delay(delay),
        )
        .mount(&server)
        .await;
    server
}

/// The head branch's name as one escaped path segment.
const HEAD_WIRE: &str = "feature%2FPAY-231-sepa-retry";

/// Gitea's `Branch` record, as `/branches/{name}` answers it.
fn branch(name: &str, protected: bool) -> Value {
    json!({
        "name": name,
        "commit": { "id": HEAD_SHA },
        "protected": protected,
        "required_approvals": 0,
        "effective_branch_protection_name": if protected { name } else { "" },
    })
}

/// Answer `GET p` with `status` and `body`, over the fake's own answer.
async fn answer(server: &MockServer, p: String, status: u16, body: Value) {
    Mock::given(method("GET"))
        .and(path(p))
        .respond_with(ResponseTemplate::new(status).set_body_json(body))
        .with_priority(2)
        .mount(server)
        .await;
}

/// The base has a commit the head lacks: the head is behind.
async fn behind(server: &MockServer) {
    Mock::given(method("GET"))
        .and(path(format!("{REPO}/commits")))
        .and(query_param("sha", "main"))
        .and(query_param("not", HEAD_SHA))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([{ "sha": FIRST_SHA }])))
        .with_priority(2)
        .mount(server)
        .await;
}

/// `main` protected by a rule that requires `approvals` approvals and,
/// where `outdated`, an up-to-date head.
async fn protect_main(server: &MockServer, approvals: u64, outdated: bool) {
    let mut main = branch("main", true);
    main["required_approvals"] = json!(approvals);
    answer(server, format!("{REPO}/branches/main"), 200, main).await;
    answer(
        server,
        format!("{REPO}/branch_protections/main"),
        200,
        json!({
            "rule_name": "main",
            "required_approvals": approvals,
            "block_on_outdated_branch": outdated,
        }),
    )
    .await;
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
            ("conversation", "Conversation", Some(10), false),
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
            (
                "Mergeable",
                "no: Gitea is still checking whether this can merge, or it conflicts with main",
                Some(Tone::Warn)
            ),
            // #994: what *Ready to merge?* reads without taking the sentence
            // apart. No protection here, so no approval is required.
            ("Approvals", "none required", Some(Tone::Ok)),
            ("Conflicts", "conflicts with main", Some(Tone::Warn)),
            ("Created", "2026-08-21 17:41 +00:00", None),
            ("Updated", "2026-08-22 10:20 +00:00", None),
            ("Review", "approved by priya.nair", Some(Tone::Ok)),
            ("Review", "requested from jonas.becker", Some(Tone::Muted)),
            ("Review", "approved by olu", Some(Tone::Ok)),
            ("Review", "commented by sam", None),
            ("Review", "changes requested by lee", Some(Tone::Warn)),
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

    let commits = &section(&view, "commits").blocks;
    assert_eq!(
        commits.len(),
        1,
        "a whole listing says nothing more: {commits:#?}"
    );
    let Block::Entities { title, rows } = &commits[0] else {
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
                commit: None,
            },
            ViewRow {
                entity_id: Some(format!("gitea:tidewater/payout-service@{HEAD_SHA}")),
                title: "PAY-231: jitter in backoff".into(),
                meta: Some("Mara Lindqvist · 2026-08-22".into()),
                tone: None,
                chip: Some("c90d11a3f5".into()),
                url: None,
                commit: None,
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
            "rae=2026-08-22 08:00 +00:00 Review=approved",
            // 10:00 at +02:00 is 08:00 UTC: ordered by the instant, drawn
            // with the offset the server sent; a tie keeps Gitea's order.
            "sam=2026-08-22 10:00 +02:00 Review=commented",
            "rae=2026-08-22 08:30 +00:00 Review=changes requested, dismissed",
            "priya.nair=2026-08-22 09:00 +00:00 Review=changes requested",
            "text(true): Bound the jitter.",
            "jonas.becker=2026-08-22 09:30 +00:00",
            "text(true): Should the jitter be bounded? <i>+/-20 %</i> feels wide.",
            "mara.lindqvist=2026-08-22 10:20 +00:00",
            "text(true): Bounded to +/-10 % in c90d11.",
            "priya.nair=2026-08-22 11:00 +00:00 Review=approved",
            "olu=2026-08-22 11:05 +00:00 Review=approved",
            "olu=2026-08-22 11:10 +00:00 Review=commented",
            "text(true): nit: name it `jitter_bound`",
            "lee=2026-08-22 12:00 +00:00 Review=changes requested",
        ],
        "reviews among the comments, in time order; a draft and a review request are not \
         part of the conversation"
    );
}

/// Workflow 6 of spec #701 (issue #705): a review's verdict is its tone in
/// *Conversation* -- an approval ok, a request for changes warn, a plain
/// comment none, a dismissed one muted whatever it said.
#[tokio::test]
async fn a_reviews_verdict_is_its_tone_in_the_conversation() {
    let server = gitea(open_pull(), reviews()).await;
    let view = source(&server).view(PR, None).await.unwrap().unwrap();
    let verdicts: Vec<_> = section(&view, "conversation")
        .blocks
        .iter()
        .filter_map(|b| match b {
            Block::Facts { facts } => facts.iter().find(|f| f.label == "Review"),
            _ => None,
        })
        .map(|f| (f.value.as_str(), f.tone))
        .collect();
    assert_eq!(
        verdicts,
        [
            ("approved", Some(Tone::Ok)),
            ("commented", None),
            ("changes requested, dismissed", Some(Tone::Muted)),
            ("changes requested", Some(Tone::Warn)),
            ("approved", Some(Tone::Ok)),
            ("approved", Some(Tone::Ok)),
            ("commented", None),
            ("changes requested", Some(Tone::Warn)),
        ]
    );
    // The tab read on its own reads the reviews too.
    let alone = source(&server)
        .view(PR, Some("conversation"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(alone.sections[0], *section(&view, "conversation"));
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
    let Block::Diff {
        files,
        truncated,
        commentable,
        ..
    } = &files.blocks[0]
    else {
        panic!("the files tab is a diff: {view:#?}");
    };
    assert!(!truncated);
    // An open pull request's diff takes line comments, sent as a review (#709).
    assert_eq!(
        commentable,
        &Some(Commenting {
            write_op: "submit_review".into()
        })
    );
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
    // The diff, and beside it what places the line comments (#709): the pull
    // request's state and its reviews. No review here has line comments, so
    // none is asked for them. And whether the token may send a review at
    // all (#2030): the branch probe, an empty write in the same scope.
    let mut asked = paths(&server).await;
    asked.sort();
    assert_eq!(
        asked,
        [
            format!("{REPO}/branches"),
            format!("{REPO}/pulls/142"),
            format!("{REPO}/pulls/142.diff"),
            format!("{REPO}/pulls/142/reviews"),
        ]
    );
}

// -- line comments on the diff (issue #709) ----------------------------------

/// A pull request that changes one line of one file: old line 2 removed, new
/// line 2 added in its place, lines 1 and 3 kept.
const CHANGED_DIFF: &str = "diff --git a/src/sepa/retry.rs b/src/sepa/retry.rs
index 1111111..2222222 100644
--- a/src/sepa/retry.rs
+++ b/src/sepa/retry.rs
@@ -1,3 +1,3 @@
 pub fn backoff() -> u64 {
-    30
+    30 + jitter()
 }
";

/// One of Gitea's `PullReviewComment`s, as `/reviews/{id}/comments` answers it
/// (shape taken from the real 1.27.3 server, #709): `position` is the new
/// side's line and `original_position` the old side's, the other `0`;
/// `diff_hunk` is the diff up to and including the commented line, as it was
/// when the comment was made.
fn line_comment(who: &str, at: &str, new: u32, old: u32, hunk_tail: &str, body: &str) -> Value {
    json!({
        "user": { "login": who },
        "created_at": at,
        "path": "src/sepa/retry.rs",
        "position": new,
        "original_position": old,
        "diff_hunk": format!("@@ -1,3 +1,3 @@\n pub fn backoff() -> u64 {{\n{hunk_tail}"),
        "body": body,
    })
}

/// A Gitea answering the *Files* tab of `pull`: [`CHANGED_DIFF`], and three
/// reviews -- Priya's request for changes with two line comments, Olu's
/// comment with four (one on a file the diff no longer holds), and Kai's draft (`PENDING`), whose comment is nobody's
/// yet -- whose comments are answered by review id.
async fn gitea_with_line_comments(pull: Value) -> MockServer {
    let server = MockServer::builder().start().await;
    let get = |p: String| {
        Mock::given(method("GET"))
            .and(path(p))
            .and(header("Authorization", format!("token {TOKEN}").as_str()))
    };
    let reviews = json!([
        { "id": 21, "user": { "login": "priya.nair" }, "state": "REQUEST_CHANGES",
          "submitted_at": "2026-08-22T09:00:00Z", "body": "", "comments_count": 2 },
        { "id": 22, "user": { "login": "jonas.becker" }, "state": "APPROVED",
          "submitted_at": "2026-08-22T09:10:00Z", "body": "", "comments_count": 0 },
        { "id": 23, "user": { "login": "olu" }, "state": "COMMENT",
          "submitted_at": "2026-08-22T11:00:00Z", "body": "", "comments_count": 4 },
        { "id": 24, "user": { "login": "kai" }, "state": "PENDING", "body": "",
          "comments_count": 1 }
    ]);
    let mut on_dropped_file = line_comment(
        "olu",
        "2026-08-22T11:00:00Z",
        7,
        0,
        "+    loop {}",
        "A second retry loop?",
    );
    on_dropped_file["path"] = json!("src/sepa/legacy.rs");
    let by_review = [
        (
            21,
            json!([
                line_comment(
                    "priya.nair",
                    "2026-08-22T09:00:00Z",
                    2,
                    0,
                    "-    30\n+    30 + jitter()",
                    "Bound the jitter."
                ),
                line_comment(
                    "priya.nair",
                    "2026-08-22T09:00:00Z",
                    0,
                    2,
                    "-    30",
                    "Why drop the <b>constant</b>?"
                ),
            ]),
        ),
        (
            23,
            json!([
                // A reply on Priya's line, later.
                line_comment(
                    "olu",
                    "2026-08-22T11:00:00Z",
                    2,
                    0,
                    "-    30\n+    30 + jitter()",
                    "Agreed: `±10 %`."
                ),
                // Written on a new line 3 that read `    35`; it reads `}` now.
                line_comment(
                    "olu",
                    "2026-08-22T11:00:00Z",
                    3,
                    0,
                    "-    30\n+    35",
                    "Why 35?"
                ),
                // A line the diff no longer has at all.
                line_comment(
                    "olu",
                    "2026-08-22T11:00:00Z",
                    40,
                    0,
                    "+    gone",
                    "And this?"
                ),
                // A file a later push dropped from the pull request.
                on_dropped_file,
            ]),
        ),
        (
            24,
            json!([line_comment(
                "kai",
                "2026-08-22T12:00:00Z",
                1,
                0,
                "",
                "draft"
            )]),
        ),
    ];
    get(format!("{REPO}/pulls/142"))
        .respond_with(ResponseTemplate::new(200).set_body_json(pull))
        .mount(&server)
        .await;
    get(format!("{REPO}/pulls/142/reviews"))
        .respond_with(ResponseTemplate::new(200).set_body_json(reviews))
        .mount(&server)
        .await;
    get(format!("{REPO}/pulls/142.diff"))
        .respond_with(ResponseTemplate::new(200).set_body_string(CHANGED_DIFF))
        .mount(&server)
        .await;
    for (id, comments) in by_review {
        get(format!("{REPO}/pulls/142/reviews/{id}/comments"))
            .respond_with(ResponseTemplate::new(200).set_body_json(comments))
            .mount(&server)
            .await;
    }
    server
}

fn said(author: &str, at: &str, body: &str) -> LineComment {
    LineComment {
        author: author.into(),
        body: body.into(),
        at: Some(at.into()),
    }
}

/// Workflow 6 of spec #701 (#709): the *Files* tab places each review's line
/// comments on the line they were written on, by side -- a removed line's on
/// the old side, an added line's on the new -- one thread per line with the
/// replies in time order. A comment whose line is gone from the diff, or holds
/// another text since a later push, is outdated: drawn apart, never on a line
/// it was not written about. One on a file the diff no longer holds is
/// outdated too and kept in the diff's own list, never dropped. A draft
/// review's comments are nobody's yet, and a review with none is not asked
/// for them.
#[tokio::test]
async fn line_comments_sit_on_their_lines_and_outdated_ones_apart() {
    let server = gitea_with_line_comments(open_pull()).await;
    let view = source(&server)
        .view(PR, Some("files"))
        .await
        .unwrap()
        .expect("drawn");
    let Block::Diff { files, gone, .. } = &view.sections[0].blocks[0] else {
        panic!("the files tab is a diff: {view:#?}");
    };
    assert_eq!(
        gone,
        &[FileThread {
            path: "src/sepa/legacy.rs".into(),
            thread: LineThread {
                side: ReviewSide::New,
                line: 7,
                outdated: true,
                comments: vec![said("olu", "2026-08-22T11:00:00Z", "A second retry loop?")],
            },
        }],
        "{view:#?}"
    );
    assert_eq!(
        files[0].threads,
        [
            LineThread {
                side: ReviewSide::Old,
                line: 2,
                outdated: false,
                comments: vec![said(
                    "priya.nair",
                    "2026-08-22T09:00:00Z",
                    "Why drop the <b>constant</b>?"
                )],
            },
            LineThread {
                side: ReviewSide::New,
                line: 2,
                outdated: false,
                comments: vec![
                    said("priya.nair", "2026-08-22T09:00:00Z", "Bound the jitter."),
                    said("olu", "2026-08-22T11:00:00Z", "Agreed: `±10 %`."),
                ],
            },
            LineThread {
                side: ReviewSide::New,
                line: 3,
                outdated: true,
                comments: vec![said("olu", "2026-08-22T11:00:00Z", "Why 35?")],
            },
            LineThread {
                side: ReviewSide::New,
                line: 40,
                outdated: true,
                comments: vec![said("olu", "2026-08-22T11:00:00Z", "And this?")],
            },
        ],
        "{view:#?}"
    );
    let asked = paths(&server).await;
    for unasked in [22, 24] {
        assert!(
            !asked.contains(&format!("{REPO}/pulls/142/reviews/{unasked}/comments")),
            "review {unasked} was asked for line comments: {asked:?}"
        );
    }
}

/// Gitea takes no review of a closed or merged pull request (*can't submit
/// review for a closed or merged PR*), so its diff takes no line comments
/// (#709) -- and still shows those it has.
#[tokio::test]
async fn a_closed_pull_requests_diff_shows_its_comments_and_takes_none() {
    let mut closed = open_pull();
    closed["state"] = json!("closed");
    let mut merged = open_pull();
    merged["state"] = json!("closed");
    merged["merged"] = json!(true);
    for pull in [closed, merged] {
        let server = gitea_with_line_comments(pull).await;
        let view = source(&server)
            .view(PR, Some("files"))
            .await
            .unwrap()
            .unwrap();
        let Block::Diff {
            files, commentable, ..
        } = &view.sections[0].blocks[0]
        else {
            panic!("a diff: {view:#?}");
        };
        assert_eq!(commentable, &None, "{view:#?}");
        assert_eq!(files[0].threads.len(), 4, "{view:#?}");
    }
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

/// A pull request longer than the tab's budget of five pages says so in its
/// last row: with the server's count, how many it left out; without one, that
/// there may be more. A page budget that ran out is never drawn as the whole
/// history, and the count is what the tab lists (#678).
#[tokio::test]
async fn a_pull_request_past_the_commit_budget_says_what_it_left_out() {
    for (header, said) in [
        (
            Some("300"),
            "The first 250 of 300 commits; the rest are on Gitea.",
        ),
        (None, "The first 250 commits; there may be more on Gitea."),
    ] {
        let server = MockServer::builder().start().await;
        let page: Vec<Value> = (0..50)
            .map(
                |i| json!({ "sha": format!("{i:040x}"), "commit": { "message": format!("c{i}") } }),
            )
            .collect();
        let mut answer = ResponseTemplate::new(200).set_body_json(Value::Array(page));
        if let Some(total) = header {
            answer = answer.insert_header("X-Total-Count", total);
        }
        Mock::given(method("GET"))
            .and(path(format!("{REPO}/pulls/142/commits")))
            .respond_with(answer)
            .mount(&server)
            .await;
        // Read for the note's link only: the tab was asked for on its own.
        Mock::given(method("GET"))
            .and(path(format!("{REPO}/pulls/142")))
            .respond_with(ResponseTemplate::new(200).set_body_json(open_pull()))
            .mount(&server)
            .await;
        let view = source(&server)
            .view(PR, Some("commits"))
            .await
            .unwrap()
            .unwrap();
        let commits = &view.sections[0];
        let [Block::Entities { rows, .. }] = commits.blocks.as_slice() else {
            panic!("{commits:#?}");
        };
        assert_eq!(rows.len(), 251, "five pages of fifty, then the note");
        assert_eq!(
            rows.last(),
            Some(&ViewRow {
                entity_id: None,
                title: said.into(),
                meta: None,
                tone: Some(Tone::Muted),
                chip: None,
                url: Some(
                    "https://gitea.example/tidewater/payout-service/pulls/142/commits".into()
                ),
                commit: None,
            }),
            "{header:?}"
        );
        assert_eq!(commits.count, Some(250), "the count is what was read");
        assert_eq!(server.received_requests().await.unwrap().len(), 6);
    }
}

/// Whether any section carries an actions block: the pull request's actions
/// are the toolbar's (issue #702), and nothing draws one twice.
fn has_actions_block(view: &SourceView) -> bool {
    view.sections
        .iter()
        .flat_map(|s| &s.blocks)
        .any(|b| matches!(b, Block::Actions { .. }))
}

/// The toolbar's action for `write_op` -- found by op rather than by place,
/// so the order the toolbar grows in (#705-#708) does not move these tests.
fn action(view: &SourceView, write_op: &str) -> ViewAction {
    no_fixed_value_is_a_field(view);
    view.actions
        .iter()
        .find(|a| a.write_op == write_op)
        .cloned()
        .unwrap_or_else(|| panic!("no {write_op} in the toolbar: {view:#?}"))
}

/// The toolbar's action labelled `label`: two actions share `submit_review`.
fn labelled(view: &SourceView, label: &str) -> ViewAction {
    no_fixed_value_is_a_field(view);
    view.actions
        .iter()
        .find(|a| a.label == label)
        .cloned()
        .unwrap_or_else(|| panic!("no {label} in the toolbar: {view:#?}"))
}

/// A fixed value is never also a field (the #705 ruling): the reader could
/// otherwise type over what the source fixed, and which wins would be the
/// shell's guess.
fn no_fixed_value_is_a_field(view: &SourceView) {
    for action in &view.actions {
        for name in action.values.keys() {
            assert!(
                !action.fields.iter().any(|f| &f.name == name),
                "{}: {name:?} is both fixed and asked for",
                action.label
            );
        }
    }
}

/// *Merge…* as the toolbar draws it: the entity's main write, a choice of the
/// SPI's three styles with `merge` (Gitea's own default) first, the sentence
/// naming what goes where; `disabled` is the one part that varies.
fn merge(disabled: Option<&str>) -> ViewAction {
    merge_deleting(disabled, true)
}

/// [`merge`], with *Delete branch after merge* drawn ticked or not (#708).
fn merge_deleting(disabled: Option<&str>, delete: bool) -> ViewAction {
    ViewAction {
        fields: vec![
            ActionField {
                required: true,
                ..ActionField::new(
                    "style",
                    "Style",
                    FieldKind::Choice {
                        options: vec![
                            ChoiceOption::new("merge", "Merge commit"),
                            ChoiceOption::new("rebase", "Rebase"),
                            ChoiceOption::new("squash", "Squash"),
                        ],
                    },
                )
            },
            ActionField {
                value: Some(delete.to_string()),
                ..ActionField::new(
                    "delete_branch",
                    "Delete branch after merge",
                    FieldKind::Flag,
                )
            },
            // #843: a squash's commit message, prefilled as Gitea's own
            // default title for one: the title and the number.
            ActionField {
                value: Some("SEPA retry with exponential backoff (#142)".into()),
                ..ActionField::new(
                    "message",
                    "Squash commit message",
                    FieldKind::Text {
                        suggestions: Vec::new(),
                    },
                )
            },
        ],
        confirm: Some("Merge feature/PAY-231-sepa-retry into main?".into()),
        primary: true,
        disabled: disabled.map(str::to_owned),
        ..ViewAction::new("merge_pull_request", "Merge…")
    }
}

/// Story 10 (issue #667), moved to the toolbar (#702): an open pull request
/// Gitea says it can merge offers *Merge…*, enabled, and no section repeats it.
#[tokio::test]
async fn an_open_mergeable_pull_request_offers_a_merge_in_its_toolbar() {
    let mut pull = open_pull();
    pull["mergeable"] = json!(true);
    let server = gitea(pull, json!([])).await;
    let view = source(&server).view(PR, None).await.unwrap().unwrap();
    assert_eq!(
        action(&view, "merge_pull_request"),
        merge(None),
        "{view:#?}"
    );
    assert!(!has_actions_block(&view), "{view:#?}");
    // The description still follows the facts.
    assert!(matches!(
        section(&view, "overview").blocks[1],
        Block::Text { .. }
    ));
    // Every style offered is one the SPI can carry.
    let FieldKind::Choice { options } = &action(&view, "merge_pull_request").fields[0].kind else {
        unreachable!()
    };
    for option in options {
        serde_json::from_value::<knobas_source::MergeStyle>(json!(option.value))
            .unwrap_or_else(|e| panic!("{option:?} is not a MergeStyle: {e}"));
    }
}

/// Where Gitea would refuse but the pull request can still get there, *Merge…*
/// is still there, disabled, saying why (#702, and the #701 ruling): an open
/// pull request it says it cannot merge (a conflict, a work-in-progress
/// title, a check still running), and one it has not answered for.
#[tokio::test]
async fn a_pull_request_gitea_would_not_merge_offers_a_disabled_merge_saying_why() {
    let not_mergeable = open_pull();
    let mut unknown = open_pull();
    unknown.as_object_mut().unwrap().remove("mergeable");
    for (pull, reason) in [
        (
            not_mergeable,
            "Gitea is still checking whether this can merge, or it conflicts with main",
        ),
        (unknown, "Gitea is still checking whether this can merge"),
    ] {
        let server = gitea(pull, json!([])).await;
        let view = source(&server).view(PR, None).await.unwrap().unwrap();
        // Not yet said is no fact to draw; the toolbar says why it waits.
        if !reason.contains("conflicts") {
            assert_eq!(mergeable_fact(&view), None, "{view:#?}");
        }
        assert_eq!(
            action(&view, "merge_pull_request"),
            merge(Some(reason)),
            "{view:#?}"
        );
        assert!(!has_actions_block(&view), "{view:#?}");
    }
}

// -- Landing: the merge's reasons, its delete flag, Update branch… (#708) ----

/// An open pull request Gitea says it can merge, everything else as `pull`.
fn mergeable_pull() -> Value {
    let mut pull = open_pull();
    pull["mergeable"] = json!(true);
    pull
}

/// The overview's *Mergeable* fact, if it has one.
fn mergeable_fact(view: &SourceView) -> Option<Fact> {
    fact(facts_of(section(view, "overview")), "Mergeable").cloned()
}

/// *Merge…*'s disabled reason names what stands in the way, each thing Gitea
/// would refuse it for that the reader can still change: a conflict, a
/// work-in-progress title, a head behind a base whose protection requires it
/// level, approvals the protection requires and has not got. Several at once
/// are all said. The overview's *Mergeable* says the same, toned warn.
#[tokio::test]
async fn a_merge_says_specifically_what_stands_in_its_way() {
    struct Case {
        what: &'static str,
        pull: Value,
        approvals: Option<(u64, bool)>,
        behind: bool,
        reason: &'static str,
    }
    let mut wip = open_pull();
    wip["title"] = json!("WIP: SEPA retry with exponential backoff");
    let mut bracketed = open_pull();
    bracketed["title"] = json!("[wip] SEPA retry");
    let cases = [
        Case {
            what: "a conflict",
            pull: open_pull(),
            approvals: None,
            behind: false,
            reason: "Gitea is still checking whether this can merge, or it conflicts with main",
        },
        Case {
            what: "a work-in-progress title",
            pull: wip,
            approvals: None,
            behind: false,
            reason: "Marked work in progress in its title",
        },
        Case {
            what: "a work-in-progress title, Gitea's other prefix, any case",
            pull: bracketed,
            approvals: None,
            behind: false,
            reason: "Marked work in progress in its title",
        },
        Case {
            what: "behind a base that must be level",
            pull: mergeable_pull(),
            approvals: Some((0, true)),
            behind: true,
            reason: "Behind main, which its protection forbids: update the branch",
        },
        Case {
            what: "approvals missing",
            pull: mergeable_pull(),
            approvals: Some((2, false)),
            behind: false,
            reason: "1 of 2 required approvals",
        },
        Case {
            what: "all of it",
            pull: open_pull(),
            approvals: Some((2, true)),
            behind: true,
            reason: "Gitea is still checking whether this can merge, or it conflicts with main; \
                     behind main, which its \
                     protection forbids: update the branch; 1 of 2 required approvals",
        },
    ];
    for case in cases {
        let server = gitea(case.pull, official_reviews()).await;
        if let Some((approvals, outdated)) = case.approvals {
            protect_main(&server, approvals, outdated).await;
        }
        if case.behind {
            behind(&server).await;
        }
        let view = source(&server).view(PR, None).await.unwrap().unwrap();
        assert_eq!(
            action(&view, "merge_pull_request").disabled.as_deref(),
            Some(case.reason),
            "{}",
            case.what
        );
        // The toolbar's sentence starts with a capital; the fact's reading
        // does not, unless it starts with a name.
        let mut lower = case.reason.to_owned();
        if !lower.starts_with("Gitea") {
            lower[..1].make_ascii_lowercase();
        }
        assert_eq!(
            mergeable_fact(&view),
            Some(Fact {
                label: "Mergeable".into(),
                value: format!("no: {lower}"),
                tone: Some(Tone::Warn),
            }),
            "{}",
            case.what
        );
    }
}

/// *Ready to merge?*'s facts (#994): *Approvals* as given of required, ok
/// when met and warn when short, and *Conflicts* from Gitea's `mergeable`,
/// none while it has not said and none for a work-in-progress title (Gitea's
/// `false` there is not about a conflict). A closed pull request has neither.
#[tokio::test]
async fn the_overview_declares_its_approvals_and_its_conflicts() {
    let approvals_and_conflicts = |view: &SourceView| {
        let facts = facts_of(section(view, "overview"));
        let read = |label| fact(facts, label).map(|f| (f.value.clone(), f.tone));
        (read("Approvals"), read("Conflicts"))
    };
    let ok = |v: &str| Some((v.to_owned(), Some(Tone::Ok)));
    let warn = |v: &str| Some((v.to_owned(), Some(Tone::Warn)));

    // Two required, one official approval: short; Gitea can merge.
    let server = gitea(mergeable_pull(), official_reviews()).await;
    protect_main(&server, 2, false).await;
    let view = source(&server).view(PR, None).await.unwrap().unwrap();
    assert_eq!(
        approvals_and_conflicts(&view),
        (warn("1 of 2"), ok("none with main"))
    );

    // One required, one given: met; Gitea says it cannot merge.
    let server = gitea(open_pull(), official_reviews()).await;
    protect_main(&server, 1, false).await;
    let view = source(&server).view(PR, None).await.unwrap().unwrap();
    assert_eq!(
        approvals_and_conflicts(&view),
        (ok("1 of 1"), warn("conflicts with main"))
    );

    // Gitea has not said yet: no conflict line.
    let mut checking = open_pull();
    checking["mergeable"] = Value::Null;
    let server = gitea(checking, json!([])).await;
    let view = source(&server).view(PR, None).await.unwrap().unwrap();
    assert_eq!(approvals_and_conflicts(&view), (ok("none required"), None));

    // A work-in-progress title: Gitea's `false` is not a conflict.
    let mut wip = open_pull();
    wip["title"] = json!("WIP: SEPA retry");
    let server = gitea(wip, json!([])).await;
    let view = source(&server).view(PR, None).await.unwrap().unwrap();
    assert_eq!(approvals_and_conflicts(&view).1, None);

    // Closed: neither.
    let mut closed = mergeable_pull();
    closed["state"] = json!("closed");
    let server = gitea(closed, json!([])).await;
    let view = source(&server).view(PR, None).await.unwrap().unwrap();
    assert_eq!(approvals_and_conflicts(&view), (None, None));
}

/// Reviews where one official approval stands: Priya's. Olu's approval is not
/// official (Gitea counts only reviewers its protection names), Rae's was
/// dismissed, and Lee asks for changes.
fn official_reviews() -> Value {
    json!([
        { "id": 1, "user": { "login": "priya.nair" }, "state": "APPROVED", "official": true },
        { "id": 2, "user": { "login": "olu" }, "state": "APPROVED", "official": false },
        { "id": 3, "user": { "login": "rae" }, "state": "APPROVED", "official": true,
          "dismissed": true },
        { "id": 4, "user": { "login": "lee" }, "state": "REQUEST_CHANGES", "official": true }
    ])
}

/// What does not stand in the way does not disable it: a head behind a base
/// that does not require it level, approvals the protection has, a
/// protection the token may not read (Gitea answers its rules to
/// administrators only) -- Gitea's own refusal is the last word there
/// (ADR-0004). The overview reads *yes*, untoned, saying the head is behind
/// where it is.
#[tokio::test]
async fn what_does_not_stand_in_the_way_leaves_the_merge_enabled() {
    // Behind, not required level.
    let server = gitea(mergeable_pull(), json!([])).await;
    behind(&server).await;
    let view = source(&server).view(PR, None).await.unwrap().unwrap();
    assert_eq!(action(&view, "merge_pull_request"), merge(None));
    assert_eq!(
        mergeable_fact(&view),
        Some(Fact {
            label: "Mergeable".into(),
            value: "yes, behind main".into(),
            tone: None,
        })
    );

    // One approval required, one official approval given.
    let server = gitea(mergeable_pull(), official_reviews()).await;
    protect_main(&server, 1, true).await;
    let view = source(&server).view(PR, None).await.unwrap().unwrap();
    assert_eq!(action(&view, "merge_pull_request"), merge(None));
    assert_eq!(mergeable_fact(&view).map(|f| f.value), Some("yes".into()));

    // Protected, its rule unreadable to this token, behind.
    let server = gitea(mergeable_pull(), json!([])).await;
    answer(
        &server,
        format!("{REPO}/branches/main"),
        200,
        branch("main", true),
    )
    .await;
    answer(
        &server,
        format!("{REPO}/branch_protections/main"),
        403,
        json!({ "message": "token does not have at least one of required scope(s)" }),
    )
    .await;
    behind(&server).await;
    let view = source(&server).view(PR, None).await.unwrap().unwrap();
    assert_eq!(action(&view, "merge_pull_request").disabled, None);
}

/// An approval given before the head last moved is `stale`. Gitea counts it
/// unless the base's protection ignores stale approvals, so knobas does too:
/// the same review leaves *Merge…* enabled under one protection and short of
/// its approvals under the other.
#[tokio::test]
async fn a_stale_approval_counts_unless_the_protection_ignores_it() {
    let stale = json!([
        { "id": 1, "user": { "login": "priya.nair" }, "state": "APPROVED", "official": true,
          "stale": true }
    ]);
    for (ignore_stale, reason) in [(false, None), (true, Some("0 of 1 required approvals"))] {
        let server = gitea(mergeable_pull(), stale.clone()).await;
        let mut main = branch("main", true);
        main["required_approvals"] = json!(1);
        answer(&server, format!("{REPO}/branches/main"), 200, main).await;
        answer(
            &server,
            format!("{REPO}/branch_protections/main"),
            200,
            json!({
                "rule_name": "main",
                "required_approvals": 1,
                "ignore_stale_approvals": ignore_stale,
            }),
        )
        .await;
        let view = source(&server).view(PR, None).await.unwrap().unwrap();
        assert_eq!(
            action(&view, "merge_pull_request").disabled.as_deref(),
            reason,
            "ignore_stale_approvals: {ignore_stale}"
        );
    }
}

/// `main` protected by a rule with `rule`'s fields (Gitea's
/// `BranchProtection`), requiring no approvals.
async fn protect_main_with(server: &MockServer, rule: Value) {
    answer(
        server,
        format!("{REPO}/branches/main"),
        200,
        branch("main", true),
    )
    .await;
    let mut protection = json!({ "rule_name": "main", "required_approvals": 0 });
    protection
        .as_object_mut()
        .unwrap()
        .extend(rule.as_object().unwrap().clone());
    answer(
        server,
        format!("{REPO}/branch_protections/main"),
        200,
        protection,
    )
    .await;
}

/// Gitea's `block_on_rejected_reviews` (#721): a standing request for
/// changes from an official reviewer -- not dismissed -- refuses the merge,
/// and *Merge…* names who asked. One from a reviewer the protection does not
/// count, or a dismissed one, does not; nor does any where the rule is off.
#[tokio::test]
async fn a_standing_request_for_changes_blocks_where_the_protection_says_so() {
    let reviews = json!([
        { "id": 1, "user": { "login": "priya.nair" }, "state": "APPROVED", "official": true },
        { "id": 2, "user": { "login": "olu" }, "state": "REQUEST_CHANGES", "official": false },
        { "id": 3, "user": { "login": "rae" }, "state": "REQUEST_CHANGES", "official": true,
          "dismissed": true },
        { "id": 4, "user": { "login": "lee" }, "state": "REQUEST_CHANGES", "official": true }
    ]);
    for (block, reason) in [(true, Some("Changes requested by lee")), (false, None)] {
        let server = gitea(mergeable_pull(), reviews.clone()).await;
        protect_main_with(&server, json!({ "block_on_rejected_reviews": block })).await;
        let view = source(&server).view(PR, None).await.unwrap().unwrap();
        assert_eq!(
            action(&view, "merge_pull_request").disabled.as_deref(),
            reason,
            "block_on_rejected_reviews: {block}"
        );
    }
    // Only the dismissed and the unofficial ones: nothing stands.
    let server = gitea(mergeable_pull(), json!([reviews[1], reviews[2]])).await;
    protect_main_with(&server, json!({ "block_on_rejected_reviews": true })).await;
    let view = source(&server).view(PR, None).await.unwrap().unwrap();
    assert_eq!(action(&view, "merge_pull_request").disabled, None);
}

/// Gitea's `block_on_official_review_requests` (#721): a review requested
/// from an official reviewer and not yet given refuses the merge, and
/// *Merge…* names who it waits for -- a team by its name. A request Gitea
/// does not mark official does not count.
#[tokio::test]
async fn an_open_official_review_request_blocks_where_the_protection_says_so() {
    let reviews = json!([
        { "id": 1, "user": { "login": "jonas.becker" }, "state": "REQUEST_REVIEW",
          "official": true },
        { "id": 2, "team": { "name": "payments" }, "state": "REQUEST_REVIEW",
          "official": true },
        { "id": 3, "user": { "login": "olu" }, "state": "REQUEST_REVIEW", "official": false }
    ]);
    for (block, reason) in [
        (true, Some("Review requested from jonas.becker, payments")),
        (false, None),
    ] {
        let server = gitea(mergeable_pull(), reviews.clone()).await;
        protect_main_with(
            &server,
            json!({ "block_on_official_review_requests": block }),
        )
        .await;
        let view = source(&server).view(PR, None).await.unwrap().unwrap();
        assert_eq!(
            action(&view, "merge_pull_request").disabled.as_deref(),
            reason,
            "block_on_official_review_requests: {block}"
        );
    }
    let server = gitea(mergeable_pull(), json!([reviews[2]])).await;
    protect_main_with(
        &server,
        json!({ "block_on_official_review_requests": true }),
    )
    .await;
    let view = source(&server).view(PR, None).await.unwrap().unwrap();
    assert_eq!(action(&view, "merge_pull_request").disabled, None);
}

/// Answer the head commit's combined status with `statuses` (Gitea's
/// `CommitStatus`, its state in `status`).
async fn head_statuses(server: &MockServer, statuses: Value) {
    answer(
        server,
        format!("{REPO}/commits/{HEAD_SHA}/status"),
        200,
        json!({ "sha": HEAD_SHA, "statuses": statuses }),
    )
    .await;
}

fn status(context: &str, state: &str) -> Value {
    json!({ "context": context, "status": state })
}

/// Gitea's required status checks (`enable_status_check`, its contexts
/// glob patterns), as its `MergeRequiredContextsCommitStatus` decides them
/// (#721): each required context must have reported on the head commit and
/// every one matched must be a success; with no contexts named, every status
/// reported must be, and at least one must have. *Merge…* names what failed,
/// what is pending and what never reported.
#[tokio::test]
async fn required_status_checks_block_until_they_pass() {
    let cases = [
        (
            "required and passed",
            json!(["ci/build"]),
            json!([status("ci/build", "success"), status("ci/lint", "failure")]),
            None,
        ),
        (
            "required and failed",
            json!(["ci/build"]),
            json!([status("ci/build", "failure")]),
            Some("Required check ci/build failed"),
        ),
        (
            "an error and a warning are failures too",
            json!(["ci/*"]),
            json!([status("ci/build", "error"), status("ci/lint", "warning")]),
            Some("Required checks ci/build failed, ci/lint failed"),
        ),
        (
            "never reported",
            json!(["ci/build"]),
            json!([]),
            Some("Required check ci/build has not reported"),
        ),
        (
            "a glob, one of its matches still pending",
            json!(["ci/*"]),
            json!([status("ci/build", "success"), status("ci/lint", "pending")]),
            Some("Required check ci/lint is pending"),
        ),
        (
            "a state Gitea sent null is pending, and a null context reads",
            json!(["ci/build"]),
            json!([{ "context": "ci/build", "status": null }, { "context": null, "status": "success" }]),
            Some("Required check ci/build is pending"),
        ),
        (
            "a skipped check passes",
            json!(["ci/*"]),
            json!([status("ci/build", "success"), status("ci/docs", "skipped")]),
            None,
        ),
        (
            "no contexts named: every status must pass",
            json!([]),
            json!([status("deploy", "success"), status("ci/lint", "error")]),
            Some("Required check ci/lint failed"),
        ),
        (
            "no contexts named and nothing reported",
            json!([]),
            json!([]),
            Some("Required checks have not reported"),
        ),
    ];
    for (what, contexts, statuses, reason) in cases {
        let server = gitea(mergeable_pull(), json!([])).await;
        protect_main_with(
            &server,
            json!({ "enable_status_check": true, "status_check_contexts": contexts }),
        )
        .await;
        head_statuses(&server, statuses).await;
        let view = source(&server).view(PR, None).await.unwrap().unwrap();
        assert_eq!(
            action(&view, "merge_pull_request").disabled.as_deref(),
            reason,
            "{what}"
        );
        // *Checks* and *Merge…* are drawn from one read of the head's
        // statuses, not one each.
        let status_reads = paths(&server)
            .await
            .iter()
            .filter(|p| **p == format!("{REPO}/commits/{HEAD_SHA}/status"))
            .count();
        assert_eq!(status_reads, 1, "{what}: the head's statuses read once");
    }
}

/// Where the status check is off, or its statuses cannot be read, the checks
/// decide nothing: a failing status on a check that is off is no blocker
/// (#710's *Checks* still draws it), and a read Gitea refuses leaves
/// *Merge…* enabled for Gitea's own refusal to say the rest (ADR-0004).
#[tokio::test]
async fn status_checks_off_or_unreadable_decide_nothing() {
    let server = gitea(mergeable_pull(), json!([])).await;
    protect_main_with(
        &server,
        json!({ "enable_status_check": false, "status_check_contexts": ["ci/build"] }),
    )
    .await;
    head_statuses(&server, json!([status("ci/build", "failure")])).await;
    let view = source(&server).view(PR, None).await.unwrap().unwrap();
    assert_eq!(action(&view, "merge_pull_request").disabled, None);

    let server = gitea(mergeable_pull(), json!([])).await;
    protect_main_with(
        &server,
        json!({ "enable_status_check": true, "status_check_contexts": ["ci/build"] }),
    )
    .await;
    answer(
        &server,
        format!("{REPO}/commits/{HEAD_SHA}/status"),
        404,
        json!({ "message": "not found" }),
    )
    .await;
    let view = source(&server).view(PR, None).await.unwrap().unwrap();
    assert_eq!(action(&view, "merge_pull_request").disabled, None);
}

/// *Delete branch after merge* is ticked where Gitea can delete the head: a
/// branch of the same repository that is not protected. A protected head, a
/// head in a fork, and a head branch already gone draw it unticked.
#[tokio::test]
async fn a_merge_deletes_its_branch_by_default_only_where_it_can() {
    // Protected head.
    let server = gitea(mergeable_pull(), json!([])).await;
    answer(
        &server,
        format!("{REPO}/branches/{HEAD_WIRE}"),
        200,
        branch("feature/PAY-231-sepa-retry", true),
    )
    .await;
    let view = source(&server).view(PR, None).await.unwrap().unwrap();
    assert_eq!(
        action(&view, "merge_pull_request"),
        merge_deleting(None, false),
        "a protected head"
    );

    // Head already deleted.
    let server = gitea(mergeable_pull(), json!([])).await;
    answer(
        &server,
        format!("{REPO}/branches/{HEAD_WIRE}"),
        404,
        json!({ "message": "branch does not exist" }),
    )
    .await;
    let view = source(&server).view(PR, None).await.unwrap().unwrap();
    assert_eq!(
        action(&view, "merge_pull_request"),
        merge_deleting(None, false),
        "a head that is gone"
    );

    // Head in a fork: its branch is not this repository's to read or delete.
    let mut forked = mergeable_pull();
    forked["head"]["repo"]["full_name"] = json!("mara/payout-service");
    let server = gitea(forked, json!([])).await;
    let view = source(&server).view(PR, None).await.unwrap().unwrap();
    assert_eq!(
        action(&view, "merge_pull_request"),
        merge_deleting(None, false),
        "a fork's head"
    );
    assert!(
        !paths(&server)
            .await
            .contains(&format!("{REPO}/branches/{HEAD_WIRE}")),
        "a fork's branch is not read in the base repository"
    );
}

/// *Update branch…* as the toolbar draws it (workflow 9 of spec #701): the
/// base merged in or the head rebased onto it, `merge` first, Gitea's own
/// default; maintenance, so neither primary nor a danger.
fn update_action() -> ViewAction {
    ViewAction {
        fields: vec![ActionField {
            required: true,
            ..ActionField::new(
                "style",
                "How",
                FieldKind::Choice {
                    options: vec![
                        ChoiceOption::new("merge", "Merge main in"),
                        ChoiceOption::new("rebase", "Rebase onto main"),
                    ],
                },
            )
        }],
        confirm: Some("Bring feature/PAY-231-sepa-retry up to date with main?".into()),
        ..ViewAction::new("update_pull_request_branch", "Update branch…")
    }
}

/// *Update branch…* is offered where the head is behind its base, after the
/// maintenance and before *Merge…* (the #701 ruling's order); level, there is
/// nothing to update; closed or merged, nothing ever will be (absent, the
/// #701 ruling).
#[tokio::test]
async fn update_branch_is_offered_where_the_head_is_behind() {
    let server = gitea(mergeable_pull(), json!([])).await;
    behind(&server).await;
    let view = source(&server).view(PR, None).await.unwrap().unwrap();
    assert_eq!(action(&view, "update_pull_request_branch"), update_action());
    let labels: Vec<_> = view.actions.iter().map(|a| a.label.as_str()).collect();
    assert_eq!(
        labels,
        [
            "Submit review…",
            "Comment…",
            "Approve…",
            "Request changes…",
            "Edit…",
            "Close",
            "Add label…",
            "Update branch…",
            "Merge…"
        ],
        "{view:#?}"
    );

    let level = gitea(mergeable_pull(), json!([])).await;
    let view = source(&level).view(PR, None).await.unwrap().unwrap();
    assert!(!offers(&view, "Update branch…"), "a level head: {view:#?}");

    let mut closed = mergeable_pull();
    closed["state"] = json!("closed");
    let mut merged = closed.clone();
    merged["merged"] = json!(true);
    for pull in [closed, merged] {
        let server = gitea(pull, json!([])).await;
        behind(&server).await;
        let view = source(&server).view(PR, None).await.unwrap().unwrap();
        assert!(!offers(&view, "Update branch…"), "{view:#?}");
    }
}

/// The overview read on its own says what the whole view's says: one place
/// decides mergeability, the toolbar and the fact alike.
#[tokio::test]
async fn the_overview_read_alone_says_the_same_mergeability() {
    let server = gitea(mergeable_pull(), official_reviews()).await;
    protect_main(&server, 2, false).await;
    let whole = source(&server).view(PR, None).await.unwrap().unwrap();
    let alone = source(&server)
        .view(PR, Some("overview"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        mergeable_fact(&alone),
        Some(Fact {
            label: "Mergeable".into(),
            value: "no: 1 of 2 required approvals".into(),
            tone: Some(Tone::Warn),
        })
    );
    assert_eq!(mergeable_fact(&alone), mergeable_fact(&whole));
}

/// A section read answers that section alone: the toolbar comes with the
/// whole view.
#[tokio::test]
async fn a_section_read_carries_no_toolbar() {
    let mut pull = open_pull();
    pull["mergeable"] = json!(true);
    let server = gitea(pull, json!([])).await;
    let view = source(&server)
        .view(PR, Some("overview"))
        .await
        .unwrap()
        .unwrap();
    assert!(view.actions.is_empty(), "{view:#?}");
}

/// A pull request's tabs are read side by side over its one record (#703):
/// once the record is in, the reviews, the commits, the comments, who the
/// token is (#705), landing's three -- the base branch, the head branch,
/// whether the head is behind (#708) -- the head commit's statuses (#710) and
/// the repository's labels (#1194) and its organisation's (#1235) are in
/// flight together. Read in turn, one is.
#[tokio::test]
async fn a_pull_requests_tabs_are_read_at_the_same_time() {
    // A token of its own, so who it is has not been read before (see
    // `VIEWER`) and that read is in flight with the tabs'.
    const FRESH: &str = "a-token-read-beside-the-tabs";
    let flight = InFlight::new(Duration::from_millis(300));
    let server = slow_gitea(open_pull(), reviews(), Some(&flight), FRESH).await;
    let view = built(&server, FRESH)
        .view(PR, None)
        .await
        .expect("reads")
        .expect("a pull request draws a view");
    assert_eq!(view.sections.len(), 4);
    assert_eq!(flight.most(), 10, "the tabs' reads in flight at once");
}

// -- Comment… and the review actions (issue #705) ----------------------------

/// *Comment…* as the toolbar draws it: a reply on the discussion, through the
/// existing `comment` op, one required long text and nothing to confirm.
fn comment_action() -> ViewAction {
    ViewAction {
        fields: vec![ActionField {
            required: true,
            ..ActionField::new("body", "Comment", FieldKind::LongText)
        }],
        ..ViewAction::new("comment", "Comment…")
    }
}

/// Workflow 7 of spec #701: a pull request offers *Comment…* in its toolbar,
/// whatever its state -- Gitea takes a reply on a closed or merged one too.
#[tokio::test]
async fn a_pull_request_offers_a_comment_in_its_toolbar_in_any_state() {
    let mut merged = open_pull();
    merged["state"] = json!("closed");
    merged["merged"] = json!(true);
    let mut closed = open_pull();
    closed["state"] = json!("closed");
    for pull in [open_pull(), closed, merged] {
        let server = gitea(pull, json!([])).await;
        let view = source(&server).view(PR, None).await.unwrap().unwrap();
        assert_eq!(action(&view, "comment"), comment_action(), "{view:#?}");
    }
}

/// *Approve…* and *Request changes…* as the toolbar draws them (the #705
/// ruling: one action per verdict, the verdict a fixed value): an approval's
/// words optional, a request for changes' required, as Gitea's own rule is.
fn review_action(verdict: &str) -> ViewAction {
    let (label, required) = match verdict {
        "approve" => ("Approve…", false),
        _ => ("Request changes…", true),
    };
    ViewAction {
        fields: vec![ActionField {
            required,
            ..ActionField::new("body", "Comment", FieldKind::LongText)
        }],
        values: [("verdict".to_owned(), verdict.to_owned())].into(),
        // The pending line comments go with a verdict (#709).
        line_comments: LineComments::Sends,
        ..ViewAction::new("submit_review", label)
    }
}

/// *Submit review…* as the toolbar draws it (#709): the reader's pending line
/// comments as a review that only comments, its words optional, drawn only
/// while there are comments to send, and then first and filled.
fn submit_review_action() -> ViewAction {
    ViewAction {
        fields: vec![ActionField::new("body", "Comment", FieldKind::LongText)],
        values: [("verdict".to_owned(), "comment".to_owned())].into(),
        line_comments: LineComments::Requires,
        primary: true,
        ..ViewAction::new("submit_review", "Submit review…")
    }
}

/// Workflow 6 of spec #701: an open pull request somebody else opened offers
/// *Approve…* and *Request changes…*: the toolbar reads *Submit review…*
/// (#709, first, which the shell draws only with pending line comments),
/// *Comment…*, the two verdicts, and *Merge…* last (the #701 ruling).
#[tokio::test]
async fn an_open_pull_request_offers_an_approval_and_a_request_for_changes() {
    let server = gitea(open_pull(), json!([])).await;
    let view = source(&server).view(PR, None).await.unwrap().unwrap();
    assert_eq!(labelled(&view, "Approve…"), review_action("approve"));
    assert_eq!(
        labelled(&view, "Request changes…"),
        review_action("request_changes")
    );
    assert_eq!(labelled(&view, "Submit review…"), submit_review_action());
    let ops: Vec<_> = view.actions.iter().map(|a| a.label.as_str()).collect();
    assert_eq!(
        ops,
        [
            "Submit review…",
            "Comment…",
            "Approve…",
            "Request changes…",
            "Edit…",
            "Close",
            "Add label…",
            "Merge…"
        ],
        "{view:#?}"
    );
}

/// Gitea refuses its author's approval and request for changes (*approve your
/// own pull is not allowed*, *reject your own pull is not allowed*) and any
/// review of a closed or merged pull request (*can't submit review for a
/// closed or merged PR*), all 422s against the real server. None of those can
/// change for this reader, so the verdicts are absent, not disabled (the #701
/// ruling), and on a merged or closed pull request *Merge…* is too. *Comment…*
/// stays: Gitea takes a reply in any state. *Submit review…* (#709) stays on
/// the author's own open pull request -- Gitea takes a review that only
/// comments from anyone -- and goes with the state.
#[tokio::test]
async fn an_action_that_can_never_apply_is_not_offered() {
    let mut own = open_pull();
    own["user"]["login"] = json!(VIEWER);
    let mut closed = open_pull();
    closed["state"] = json!("closed");
    closed["mergeable"] = json!(true);
    let mut merged = open_pull();
    merged["state"] = json!("closed");
    merged["merged"] = json!(true);
    merged["mergeable"] = json!(true);
    for (pull, offered) in [
        (
            own,
            &[
                "Submit review…",
                "Comment…",
                "Edit…",
                "Close",
                "Add label…",
                "Merge…",
            ][..],
        ),
        (closed, &["Comment…", "Edit…", "Reopen", "Add label…"][..]),
        (merged, &["Comment…", "Edit…", "Add label…"][..]),
    ] {
        let server = gitea(pull, json!([])).await;
        let view = source(&server).view(PR, None).await.unwrap().unwrap();
        let labels: Vec<_> = view.actions.iter().map(|a| a.label.as_str()).collect();
        assert_eq!(labels, offered, "{view:#?}");
        assert_eq!(action(&view, "comment").disabled, None);
    }
}

/// Who the token is, is a read of its own, and a failed one does not cost the
/// view: the review actions are offered and Gitea's refusal, should it
/// come, is the write queue's to show (ADR-0004).
#[tokio::test]
async fn a_view_whose_viewer_cannot_be_read_still_offers_the_review() {
    const UNREAD: &str = "a-token-gitea-will-not-name";
    let server = MockServer::builder().start().await;
    let token = format!("token {UNREAD}");
    let get = |p: String| {
        Mock::given(method("GET"))
            .and(path(p))
            .and(header("Authorization", token.as_str()))
    };
    let mut own = open_pull();
    own["user"]["login"] = json!(VIEWER);
    for (p, body) in [
        (format!("{REPO}/pulls/142"), own),
        (format!("{REPO}/pulls/142/reviews"), json!([])),
        (format!("{REPO}/pulls/142/commits"), json!([])),
        (format!("{REPO}/issues/142/comments"), json!([])),
        (
            format!("{REPO}/commits/{HEAD_SHA}/status"),
            json!({ "statuses": [] }),
        ),
    ] {
        get(p)
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(&server)
            .await;
    }
    get("/api/v1/user".to_owned())
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({ "message": "gone" })))
        .mount(&server)
        .await;
    let view = knobas_source_gitea::build(instance(server.uri(), UNREAD, json!({})))
        .expect("builds")
        .view(PR, None)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(labelled(&view, "Approve…"), review_action("approve"));
}

/// A server answering a pull request's whole view to any token, and `/user`
/// to each `(token, login)` with that login.
async fn gitea_for_viewers(pull: Value, viewers: &[(&str, &str)]) -> MockServer {
    let server = MockServer::builder().start().await;
    for (p, body) in [
        (format!("{REPO}/pulls/142"), pull),
        (format!("{REPO}/pulls/142/reviews"), json!([])),
        (format!("{REPO}/pulls/142/commits"), json!([])),
        (format!("{REPO}/issues/142/comments"), json!([])),
        (
            format!("{REPO}/commits/{HEAD_SHA}/status"),
            json!({ "statuses": [] }),
        ),
    ] {
        Mock::given(method("GET"))
            .and(path(p))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(&server)
            .await;
    }
    for (token, login) in viewers {
        Mock::given(method("GET"))
            .and(path("/api/v1/user"))
            .and(header("Authorization", format!("token {token}").as_str()))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "login": login })))
            .mount(&server)
            .await;
    }
    server
}

/// Whether the toolbar offers an action labelled `label`.
fn offers(view: &SourceView, label: &str) -> bool {
    view.actions.iter().any(|a| a.label == label)
}

/// The adapter against `server`, authenticating with `token`.
fn built(server: &MockServer, token: &str) -> Box<dyn Source> {
    knobas_source_gitea::build(instance(server.uri(), token, json!({}))).expect("builds")
}

async fn asked_who(server: &MockServer) -> usize {
    server
        .received_requests()
        .await
        .expect("wiremock records requests")
        .iter()
        .filter(|r| r.url.path() == "/api/v1/user")
        .count()
}

/// Who the token is does not change between views, and the app builds a
/// source for every view it reads, read-ahead included: so it is asked once
/// per token and server, not once per view (#705). Another token on the same
/// server is another account, and is asked for.
#[tokio::test]
async fn who_the_token_is_is_asked_once_per_token_and_server() {
    let mut own = open_pull();
    own["user"]["login"] = json!(VIEWER);
    const ONCE: &str = "a-token-asked-once";
    let server = gitea_for_viewers(own, &[(ONCE, VIEWER), ("another-token", "olu")]).await;
    for _ in 0..3 {
        let view = built(&server, ONCE).view(PR, None).await.unwrap().unwrap();
        assert!(!offers(&view, "Approve…"), "{view:#?}");
    }
    assert_eq!(asked_who(&server).await, 1, "three views, one GET /user");

    let view = built(&server, "another-token")
        .view(PR, None)
        .await
        .unwrap()
        .unwrap();
    assert!(offers(&view, "Approve…"), "olu did not open it");
    assert_eq!(asked_who(&server).await, 2);
}

/// A `/user` Gitea did not answer is not remembered: the next view asks
/// again, and then knows whose pull request it is.
#[tokio::test]
async fn who_the_token_is_is_asked_again_after_a_failed_read() {
    let mut own = open_pull();
    own["user"]["login"] = json!(VIEWER);
    const AGAIN: &str = "a-token-asked-again";
    let server = gitea_for_viewers(own, &[(AGAIN, VIEWER)]).await;
    Mock::given(method("GET"))
        .and(path("/api/v1/user"))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({ "message": "gone" })))
        .up_to_n_times(1)
        .with_priority(1)
        .mount(&server)
        .await;
    let first = built(&server, AGAIN).view(PR, None).await.unwrap().unwrap();
    assert!(offers(&first, "Approve…"), "{first:#?}");
    let second = built(&server, AGAIN).view(PR, None).await.unwrap().unwrap();
    assert!(!offers(&second, "Approve…"), "{second:#?}");
    assert_eq!(asked_who(&server).await, 2);
}

// -- Edit…, Close and Reopen (issue #706) ------------------------------------

/// *Edit…* as the toolbar draws it: the title one required line, the
/// description a long text, both prefilled with what Gitea holds today, so
/// the shell can send only what the reader changed (the #706 ruling).
fn edit_action(title: &str, body: Option<&str>) -> ViewAction {
    ViewAction {
        fields: vec![
            ActionField {
                value: Some(title.to_owned()),
                required: true,
                ..ActionField::new(
                    "title",
                    "Title",
                    FieldKind::Text {
                        suggestions: vec![],
                    },
                )
            },
            ActionField {
                value: body.map(str::to_owned),
                ..ActionField::new("body", "Description", FieldKind::LongText)
            },
        ],
        ..ViewAction::new("edit_pull_request", "Edit…")
    }
}

/// *Close*: a danger, through its sentence, the state a fixed value.
fn close_action() -> ViewAction {
    ViewAction {
        confirm: Some("Close feature/PAY-231-sepa-retry → main without merging it?".to_owned()),
        danger: true,
        values: [("state".to_owned(), "closed".to_owned())].into(),
        ..ViewAction::new("edit_pull_request", "Close")
    }
}

/// *Reopen*: nothing to ask and nothing destroyed, so it queues on the button.
fn reopen_action() -> ViewAction {
    ViewAction {
        values: [("state".to_owned(), "open".to_owned())].into(),
        ..ViewAction::new("edit_pull_request", "Reopen")
    }
}

/// Workflow 8 of spec #701 (issue #706), per state. Open: *Edit…* and
/// *Close*. Closed and not merged: *Edit…* and *Reopen*. Merged: *Edit…*
/// alone -- Gitea takes a merged pull request's title and description but
/// refuses its state (412, *it was already merged*), and no wait changes
/// that, so neither *Close* nor *Reopen* is drawn (the #701 ruling).
#[tokio::test]
async fn a_pull_request_offers_edit_and_close_or_reopen_by_its_state() {
    let mut closed = open_pull();
    closed["state"] = json!("closed");
    let mut merged = open_pull();
    merged["state"] = json!("closed");
    merged["merged"] = json!(true);
    for (pull, state_action) in [
        (open_pull(), Some(close_action())),
        (closed, Some(reopen_action())),
        (merged, None),
    ] {
        let server = gitea(pull, json!([])).await;
        let view = source(&server).view(PR, None).await.unwrap().unwrap();
        let edits: Vec<ViewAction> = view
            .actions
            .iter()
            .filter(|a| a.write_op == "edit_pull_request")
            .cloned()
            .collect();
        no_fixed_value_is_a_field(&view);
        let mut expected = vec![edit_action(
            "SEPA retry with exponential backoff",
            Some("Refs PAY-231\n\n<b>not markup</b>"),
        )];
        expected.extend(state_action);
        assert_eq!(edits, expected, "{view:#?}");
    }
}

/// The description is prefilled exactly as Gitea holds it -- trailing
/// newline included -- or the shell would read an untouched one as changed
/// and send it back; a pull request with none prefills nothing.
#[tokio::test]
async fn the_edit_is_prefilled_with_the_description_as_gitea_holds_it() {
    let mut trailing = open_pull();
    trailing["body"] = json!("Refs PAY-231\r\n");
    let mut none = open_pull();
    none["body"] = Value::Null;
    for (pull, body) in [(trailing, Some("Refs PAY-231\r\n")), (none, None)] {
        let server = gitea(pull, json!([])).await;
        let view = source(&server).view(PR, None).await.unwrap().unwrap();
        assert_eq!(
            labelled(&view, "Edit…"),
            edit_action("SEPA retry with exponential backoff", body)
        );
    }
}

/// The reviews' line comments are read side by side (#709, as #703 reads the
/// tabs): Gitea answers them one review at a time, and four reviewed rounds
/// read in turn would cost four round trips on top of the diff's. The diff,
/// the record and the reviews are three reads at once; four reviews' comments
/// at once are the fourth.
#[tokio::test]
async fn the_reviews_line_comments_are_read_at_the_same_time() {
    let flight = InFlight::new(Duration::from_millis(300));
    let server = MockServer::builder().start().await;
    flight.watch(&server).await;
    let held = |body: ResponseTemplate| body.set_delay(flight.delay());
    let reviews: Vec<Value> = (31..=34)
        .map(|id| {
            json!({ "id": id, "user": { "login": "olu" }, "state": "COMMENT",
                    "submitted_at": "2026-08-22T11:00:00Z", "comments_count": 1 })
        })
        .collect();
    for (p, answer) in [
        (
            format!("{REPO}/pulls/142"),
            ResponseTemplate::new(200).set_body_json(open_pull()),
        ),
        (
            format!("{REPO}/pulls/142/reviews"),
            ResponseTemplate::new(200).set_body_json(json!(reviews)),
        ),
        (
            format!("{REPO}/pulls/142.diff"),
            ResponseTemplate::new(200).set_body_string(CHANGED_DIFF),
        ),
    ] {
        Mock::given(method("GET"))
            .and(path(p))
            .respond_with(held(answer))
            .mount(&server)
            .await;
    }
    for id in 31..=34 {
        Mock::given(method("GET"))
            .and(path(format!("{REPO}/pulls/142/reviews/{id}/comments")))
            .respond_with(held(ResponseTemplate::new(200).set_body_json(json!([
                line_comment(
                    "olu",
                    "2026-08-22T11:00:00Z",
                    2,
                    0,
                    "-    30\n+    30 + jitter()",
                    &format!("round {id}")
                )
            ]))))
            .mount(&server)
            .await;
    }
    let view = source(&server)
        .view(PR, Some("files"))
        .await
        .unwrap()
        .unwrap();
    let Block::Diff { files, .. } = &view.sections[0].blocks[0] else {
        panic!("a diff: {view:#?}");
    };
    assert_eq!(files[0].threads[0].comments.len(), 4, "{view:#?}");
    assert_eq!(
        flight.most(),
        4,
        "the four reviews' comments in flight at once"
    );
}

/// A file the cap cut (#709): a comment on a line past the cut is not called
/// outdated -- the line may be there, unshown -- while one on a shown line
/// still sits on it.
#[tokio::test]
async fn a_comment_past_a_files_cut_is_not_called_outdated() {
    let lines = knobas_source::diff::MAX_LINES_PER_FILE + 100;
    let mut diff = format!(
        "diff --git a/big.txt b/big.txt\nnew file mode 100644\nindex 0000000..1111111\n\
         --- /dev/null\n+++ b/big.txt\n@@ -0,0 +1,{lines} @@\n"
    );
    for n in 1..=lines {
        diff.push_str(&format!("+line {n}\n"));
    }
    let server = MockServer::builder().start().await;
    let reviews = json!([{ "id": 41, "user": { "login": "olu" }, "state": "COMMENT",
                           "submitted_at": "2026-08-22T11:00:00Z", "comments_count": 2 }]);
    let comment = |line: usize| {
        json!({ "user": { "login": "olu" }, "created_at": "2026-08-22T11:00:00Z",
                "path": "big.txt", "position": line, "original_position": 0,
                "diff_hunk": format!("@@ -0,0 +1,{line} @@\n+line {line}"), "body": format!("on {line}") })
    };
    for (p, answer) in [
        (
            format!("{REPO}/pulls/142"),
            ResponseTemplate::new(200).set_body_json(open_pull()),
        ),
        (
            format!("{REPO}/pulls/142/reviews"),
            ResponseTemplate::new(200).set_body_json(reviews),
        ),
        (
            format!("{REPO}/pulls/142.diff"),
            ResponseTemplate::new(200).set_body_string(diff),
        ),
        (
            format!("{REPO}/pulls/142/reviews/41/comments"),
            ResponseTemplate::new(200).set_body_json(json!([comment(3), comment(lines)])),
        ),
    ] {
        Mock::given(method("GET"))
            .and(path(p))
            .respond_with(answer)
            .mount(&server)
            .await;
    }
    let view = source(&server)
        .view(PR, Some("files"))
        .await
        .unwrap()
        .unwrap();
    let Block::Diff { files, .. } = &view.sections[0].blocks[0] else {
        panic!("a diff: {view:#?}");
    };
    assert!(files[0].truncated, "the fixture is past the cap");
    let threads: Vec<(u32, bool)> = files[0]
        .threads
        .iter()
        .map(|t| (t.line, t.outdated))
        .collect();
    assert_eq!(threads, [(3, false), (lines as u32, false)]);
}

// -- Checks (issue #710) -----------------------------------------------------

/// The pull request's head commit's statuses are its *Checks*, on *Overview*
/// right after the facts and before the description, failing first. Asked
/// by the head's sha, which the pull request's record names.
#[tokio::test]
async fn a_pull_requests_head_commit_statuses_are_its_checks() {
    let server = gitea(open_pull(), json!([])).await;
    Mock::given(method("GET"))
        .and(path(format!("{REPO}/commits/{HEAD_SHA}/status")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "state": "failure", "sha": HEAD_SHA, "total_count": 2,
            "statuses": [
                { "status": "success", "context": "ci/build", "description": "passed",
                  "target_url": "https://ci.example/1", "updated_at": "2026-08-22T11:44:00Z" },
                { "status": "failure", "context": "ci/tests", "description": "1 failed",
                  "target_url": "https://ci.example/2", "updated_at": "2026-08-22T10:10:00Z" },
                { "status": "pending", "context": "ci/deploy",
                  "updated_at": "2026-08-21T09:00:00+02:00" }
            ]
        })))
        .with_priority(2)
        .mount(&server)
        .await;
    for asked in [None, Some("overview")] {
        let view = source(&server).view(PR, asked).await.unwrap().unwrap();
        let overview = section(&view, "overview");
        let Block::Entities { title, rows } = &overview.blocks[1] else {
            panic!("the checks follow the facts: {overview:#?}");
        };
        assert_eq!(title.as_deref(), Some("Checks"));
        let drawn: Vec<_> = rows
            .iter()
            .map(|r| {
                (
                    r.title.as_str(),
                    r.chip.as_deref(),
                    r.tone,
                    r.meta.as_deref(),
                )
            })
            .collect();
        // Each row's time (#994) after its description, as long ago as the
        // shell's `ago()` says it: these are days old, so their UTC date.
        assert_eq!(
            drawn,
            [
                (
                    "ci/tests",
                    Some("failure"),
                    Some(Tone::Err),
                    Some("1 failed · 2026-08-22")
                ),
                (
                    "ci/deploy",
                    Some("pending"),
                    Some(Tone::Info),
                    Some("2026-08-21")
                ),
                (
                    "ci/build",
                    Some("success"),
                    Some(Tone::Ok),
                    Some("passed · 2026-08-22")
                ),
            ],
            "asked for {asked:?}"
        );
        assert!(
            matches!(overview.blocks[2], Block::Text { .. }),
            "the description after them: {overview:#?}"
        );
    }
}

/// A head commit nobody reported on draws no *Checks*: the description
/// follows the facts as it did before.
#[tokio::test]
async fn a_pull_request_without_statuses_draws_no_checks() {
    let server = gitea(open_pull(), json!([])).await;
    let view = source(&server).view(PR, None).await.unwrap().unwrap();
    let overview = section(&view, "overview");
    assert_eq!(overview.blocks.len(), 2, "{overview:#?}");
    assert!(matches!(overview.blocks[1], Block::Text { .. }));
}

/// A head commit whose statuses cannot be read does not fail the pull
/// request's view (#710 ruling): its *Overview* draws one muted line where
/// the checks would be, and the description still follows, whole or asked
/// for alone.
#[tokio::test]
async fn a_pull_request_whose_checks_cannot_be_read_still_draws_and_says_so() {
    let server = gitea(open_pull(), json!([])).await;
    Mock::given(method("GET"))
        .and(path(format!("{REPO}/commits/{HEAD_SHA}/status")))
        .respond_with(ResponseTemplate::new(403))
        .with_priority(2)
        .mount(&server)
        .await;
    for asked in [None, Some("overview")] {
        let view = source(&server).view(PR, asked).await.unwrap().unwrap();
        let overview = section(&view, "overview");
        let Block::Entities { title: None, rows } = &overview.blocks[1] else {
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
        assert!(
            matches!(overview.blocks[2], Block::Text { .. }),
            "the description after it: {overview:#?}"
        );
    }
}

// -- the source action (#1194, ADR-0029) -------------------------------------

/// *Add label…* is Gitea's one source action: a write no role's vocabulary
/// has. It offers the repository's labels the pull request does not carry,
/// in every state -- Gitea labels a closed or merged pull request too -- with
/// the action's id fixed and the label a required choice.
#[tokio::test]
async fn add_label_offers_the_repositorys_labels_the_pull_request_lacks() {
    let server = gitea(open_pull(), json!([])).await;
    let view = source(&server).view(PR, None).await.unwrap().unwrap();
    assert_eq!(
        labelled(&view, "Add label…"),
        crate::source_action::add_label_action(&[("7", "needs-qa")], None)
    );
}

/// Where nothing can be added the action says why, disabled: the reader can
/// make a label, or take one off, and it is enabled again (the #701 ruling).
/// A labels read that fails costs the action, never the view.
#[tokio::test]
async fn add_label_with_nothing_to_add_or_no_labels_read_says_why() {
    let mut all = open_pull();
    all["labels"] = json!([{ "id": 3, "name": "bug" }, { "id": 7, "name": "needs-qa" }]);
    for (pull, labels, reason) in [
        (
            all,
            Some(json!(null)),
            "Every label of this repository is on this pull request.",
        ),
        (
            open_pull(),
            Some(json!([])),
            "This repository has no labels.",
        ),
        (open_pull(), None, "Could not read this repository's labels"),
    ] {
        let server = gitea(pull, json!([])).await;
        match labels {
            Some(Value::Null) => {}
            Some(body) => answer(&server, format!("{REPO}/labels"), 200, body).await,
            None => {
                answer(
                    &server,
                    format!("{REPO}/labels"),
                    404,
                    json!({ "message": "The target couldn't be found." }),
                )
                .await
            }
        }
        let view = source(&server).view(PR, None).await.unwrap().unwrap();
        let action = labelled(&view, "Add label…");
        let disabled = action.disabled.clone().unwrap_or_default();
        assert!(disabled.starts_with(reason), "{reason}: {action:#?}");
        assert!(action.fields.is_empty(), "{action:#?}");
    }
}

/// One page of a label listing at `p`, answered for `page` only.
async fn label_page(
    server: &MockServer,
    p: String,
    page: u32,
    labels: Value,
    total: Option<usize>,
) {
    let mut answer = ResponseTemplate::new(200).set_body_json(labels);
    if let Some(total) = total {
        answer = answer.insert_header("X-Total-Count", total.to_string().as_str());
    }
    Mock::given(method("GET"))
        .and(path(p))
        .and(query_param("limit", "50"))
        .and(query_param("page", page.to_string()))
        .respond_with(answer)
        .with_priority(1)
        .mount(server)
        .await;
}

/// *Add label…* reads every page of the repository's labels (#1235): where
/// the first fifty are all on the pull request, the fifty-first is offered,
/// not "every label is on this pull request". Gitea's `X-Total-Count` says
/// when the listing is whole; without one, an empty page does (#81's rule).
#[tokio::test]
async fn add_label_offers_the_repositorys_labels_past_the_first_page() {
    let first: Vec<Value> = (100..150)
        .map(|id| json!({ "id": id, "name": format!("area/{id}") }))
        .collect();
    let mut pull = open_pull();
    pull["labels"] = Value::Array(first.clone());
    for total in [Some(51), None] {
        let server = gitea(pull.clone(), json!([])).await;
        let labels = format!("{REPO}/labels");
        label_page(
            &server,
            labels.clone(),
            1,
            Value::Array(first.clone()),
            total,
        )
        .await;
        let last = json!([{ "id": 200, "name": "late" }]);
        label_page(&server, labels.clone(), 2, last, total).await;
        // Past the end: with a count, a page a correct read never asks for;
        // without one, the empty page that ends the listing.
        let beyond = match total {
            Some(_) => json!([{ "id": 300, "name": "beyond" }]),
            None => json!([]),
        };
        label_page(&server, labels, 3, beyond, total).await;
        let view = source(&server).view(PR, None).await.unwrap().unwrap();
        assert_eq!(
            labelled(&view, "Add label…"),
            crate::source_action::add_label_action(&[("200", "late")], None),
            "X-Total-Count {total:?}"
        );
    }
}

/// A short page does not end a label listing (#1235, #81's rule): a Gitea
/// whose `MAX_RESPONSE_ITEMS` is under fifty answers fewer than were asked
/// for on every page, and without a count only the empty page says the
/// listing is whole.
#[tokio::test]
async fn add_label_reads_on_past_a_short_page() {
    let server = gitea(open_pull(), json!([])).await;
    let labels = format!("{REPO}/labels");
    let first = json!([{ "id": 7, "name": "needs-qa" }]);
    label_page(&server, labels.clone(), 1, first, None).await;
    let second = json!([{ "id": 8, "name": "wontfix" }]);
    label_page(&server, labels.clone(), 2, second, None).await;
    label_page(&server, labels, 3, json!([]), None).await;
    let view = source(&server).view(PR, None).await.unwrap().unwrap();
    assert_eq!(
        labelled(&view, "Add label…"),
        crate::source_action::add_label_action(&[("7", "needs-qa"), ("8", "wontfix")], None)
    );
}

/// *Add label…* also offers the owning organisation's labels, every page of
/// them, after the repository's own (#1235): a repository that uses only
/// organisation labels has some to offer. A user owner is Gitea's 404 there
/// and has none (the other tests here); any other failure says so.
#[tokio::test]
async fn add_label_offers_the_organisations_labels_too() {
    let org_first: Vec<Value> = (500..550)
        .map(|id| json!({ "id": id, "name": format!("org/{id}") }))
        .collect();
    let mut pull = open_pull();
    pull["labels"] = Value::Array(org_first.clone());
    for (own, offered) in [
        (
            json!([{ "id": 7, "name": "needs-qa" }]),
            &[("7", "needs-qa"), ("900", "org-late")][..],
        ),
        (json!([]), &[("900", "org-late")][..]),
    ] {
        let server = gitea(pull.clone(), json!([])).await;
        let count = own.as_array().map(Vec::len);
        label_page(&server, format!("{REPO}/labels"), 1, own, count).await;
        label_page(
            &server,
            ORG.to_owned(),
            1,
            Value::Array(org_first.clone()),
            Some(51),
        )
        .await;
        let last = json!([{ "id": 900, "name": "org-late" }]);
        label_page(&server, ORG.to_owned(), 2, last, Some(51)).await;
        let view = source(&server).view(PR, None).await.unwrap().unwrap();
        assert_eq!(
            labelled(&view, "Add label…"),
            crate::source_action::add_label_action(offered, None)
        );
    }

    let server = gitea(open_pull(), json!([])).await;
    answer(&server, ORG.to_owned(), 500, json!({ "message": "boom" })).await;
    let view = source(&server).view(PR, None).await.unwrap().unwrap();
    let action = labelled(&view, "Add label…");
    let disabled = action.disabled.clone().unwrap_or_default();
    assert!(
        disabled.starts_with("Could not read its organisation's labels"),
        "{action:#?}"
    );
}

/// A repository label and an organisation label of one name read as two
/// options (#1271): the organisation's carries the organisation's name, the
/// repository's keeps its plain one, and a name only one list has stays plain.
/// The organisation's keeps its suffix when the repository's is already on the
/// pull request, so it reads apart from the label the pull request shows.
#[tokio::test]
async fn add_label_tells_a_same_named_organisation_label_apart() {
    let mut carrying = open_pull();
    carrying["labels"] = json!([{ "id": 7, "name": "bug" }]);
    for (pull, offered) in [
        (
            open_pull(),
            &[
                ("7", "bug"),
                ("8", "wontfix"),
                ("900", "bug (tidewater)"),
                ("901", "org-late"),
            ][..],
        ),
        (
            carrying,
            &[
                ("8", "wontfix"),
                ("900", "bug (tidewater)"),
                ("901", "org-late"),
            ][..],
        ),
    ] {
        let server = gitea(pull, json!([])).await;
        let own = json!([{ "id": 7, "name": "bug" }, { "id": 8, "name": "wontfix" }]);
        label_page(&server, format!("{REPO}/labels"), 1, own, Some(2)).await;
        let org = json!([{ "id": 900, "name": "bug" }, { "id": 901, "name": "org-late" }]);
        label_page(&server, ORG.to_owned(), 1, org, Some(2)).await;
        let view = source(&server).view(PR, None).await.unwrap().unwrap();
        assert_eq!(
            labelled(&view, "Add label…"),
            crate::source_action::add_label_action(offered, None)
        );
    }
}

// -- what the token may write (issue #2030) ----------------------------------

const SCOPE_REFUSAL: &str = "token does not have at least one of required scope(s)";

/// Each toolbar action's label and reason, the ones in `refused` replaced by
/// `reason` and every other as `drawn` has it.
fn expected(drawn: &SourceView, refused: &[&str], reason: &str) -> Vec<(String, Option<String>)> {
    drawn
        .actions
        .iter()
        .map(|a| {
            let why = if refused.contains(&a.label.as_str()) {
                Some(reason.to_owned())
            } else {
                a.disabled.clone()
            };
            (a.label.clone(), why)
        })
        .collect()
}

fn reasons(view: &SourceView) -> Vec<(String, Option<String>)> {
    view.actions
        .iter()
        .map(|a| (a.label.clone(), a.disabled.clone()))
        .collect()
}

/// Issue #2030, against the measured answers (`view::grants`): a token
/// without `write:issue` cannot comment or label, which go through the
/// issue of the same index, and may still review and merge; one without
/// `write:repository` may do neither, and its diff takes no line comments;
/// an account that may not push cannot merge and may still comment and
/// review. Where Gitea gives no verdict the view is as the entity drew it.
#[tokio::test]
async fn what_the_token_may_not_write_is_drawn_disabled_saying_why() {
    let plain = gitea(open_pull(), reviews()).await;
    let drawn = source(&plain).view(PR, None).await.unwrap().unwrap();
    let labels: Vec<&str> = drawn.actions.iter().map(|a| a.label.as_str()).collect();
    assert!(
        labels.contains(&"Comment…") && labels.contains(&"Merge…"),
        "{labels:?}"
    );

    let no_issue = gitea(open_pull(), reviews()).await;
    write_probe(
        &no_issue,
        format!("{REPO}/branches"),
        422,
        "[BranchName]: Required",
    )
    .await;
    write_probe(&no_issue, format!("{REPO}/issues"), 403, SCOPE_REFUSAL).await;
    let view = source(&no_issue).view(PR, None).await.unwrap().unwrap();
    assert_eq!(
        reasons(&view),
        expected(
            &drawn,
            &["Comment…", "Add label…"],
            "This token cannot write: it lacks the write:issue scope"
        )
    );

    let read_only = gitea(open_pull(), reviews()).await;
    write_probe(&read_only, format!("{REPO}/branches"), 403, SCOPE_REFUSAL).await;
    write_probe(&read_only, format!("{REPO}/issues"), 403, SCOPE_REFUSAL).await;
    let view = source(&read_only).view(PR, None).await.unwrap().unwrap();
    for (label, why) in reasons(&view) {
        assert!(
            why.as_deref()
                .is_some_and(|w| w.starts_with("This token cannot write: ")),
            "{label}: {why:?}"
        );
    }
    let files = source(&read_only)
        .view(PR, Some("files"))
        .await
        .unwrap()
        .unwrap();
    let Block::Diff { commentable, .. } = &files.sections[0].blocks[0] else {
        panic!("{files:#?}");
    };
    assert_eq!(commentable, &None, "no line comments it cannot send");
    assert_eq!(files.sections[0].count, Some(2), "the diff reads as before");

    let reader = gitea(open_pull(), reviews()).await;
    write_probe(
        &reader,
        format!("{REPO}/branches"),
        403,
        "user should have a permission to write to a repo",
    )
    .await;
    write_probe(&reader, format!("{REPO}/issues"), 422, "[Title]: Required").await;
    let view = source(&reader).view(PR, None).await.unwrap().unwrap();
    assert_eq!(
        reasons(&view),
        expected(
            &drawn,
            &["Merge…"],
            "This account cannot push to this repository"
        )
    );
}

// -- what the token may not read (issue #2042) --------------------------------

/// Issue #2042, spec #1192 story 10: a token without `read:issue` reads a
/// pull request's view whole but for what Gitea keeps on the issue of the
/// same index. *Conversation* draws the reviews, which are the pull
/// request's, under a line naming the scope its comments need; *Add label…*
/// names it too. The tab read on its own says the same.
#[tokio::test]
async fn a_token_without_the_issue_scope_reads_the_view_but_its_comments() {
    let server = gitea(open_pull(), reviews()).await;
    let refused = json!({ "message": format!("{SCOPE_REFUSAL}, required=[read:issue]") });
    answer(
        &server,
        format!("{REPO}/issues/142/comments"),
        403,
        refused.clone(),
    )
    .await;
    answer(&server, format!("{REPO}/labels"), 403, refused).await;
    let note = "This token cannot read this pull request's comments: it lacks the read:issue scope";
    let view = source(&server).view(PR, None).await.unwrap().unwrap();
    let ids: Vec<&str> = view.sections.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(ids, ["overview", "files", "commits", "conversation"]);
    assert_eq!(
        labelled(&view, "Add label…").disabled.as_deref(),
        Some("This token cannot read this repository's labels: it lacks the read:issue scope")
    );
    let alone = source(&server)
        .view(PR, Some("conversation"))
        .await
        .unwrap()
        .unwrap();
    for conversation in [section(&view, "conversation"), &alone.sections[0]] {
        let Block::Entities { title: None, rows } = &conversation.blocks[0] else {
            panic!("the tab opens on the line: {conversation:#?}");
        };
        let drawn: Vec<_> = rows.iter().map(|r| (r.title.as_str(), r.tone)).collect();
        assert_eq!(drawn, [(note, Some(Tone::Muted))]);
        // The eight reviews that are something someone said, and no comment.
        assert_eq!(conversation.count, Some(8), "{conversation:#?}");
        let Block::Facts { facts } = &conversation.blocks[1] else {
            panic!("the reviews follow the line: {conversation:#?}");
        };
        assert_eq!(facts[0].label, "rae", "{conversation:#?}");
    }
}

/// Only the scope refusal is drawn as a line: a discussion Gitea fails to
/// serve for any other reason fails the view as before, so a blip is never
/// drawn as a pull request nobody commented on (#2042).
#[tokio::test]
async fn a_discussion_gitea_fails_to_serve_fails_the_view() {
    for (status, body) in [
        (500, json!({ "message": "boom" })),
        (403, json!({ "message": "user does not have access" })),
    ] {
        let server = gitea(open_pull(), reviews()).await;
        answer(&server, format!("{REPO}/issues/142/comments"), status, body).await;
        for asked in [None, Some("conversation")] {
            let view = source(&server).view(PR, asked).await;
            assert_eq!(
                view.as_ref().err().and_then(SourceError::status),
                Some(status),
                "{asked:?}: {view:?}"
            );
        }
    }
}

/// A token with `read:issue` and without `read:organization` reads the
/// repository's labels and is refused its organisation's: *Add label…* names
/// the scope it lacks, and the view draws.
#[tokio::test]
async fn a_token_without_the_organisation_scope_is_told_on_add_label() {
    let server = gitea(open_pull(), reviews()).await;
    let refused = json!({ "message": format!("{SCOPE_REFUSAL}, required=[read:organization]") });
    answer(&server, ORG.to_owned(), 403, refused).await;
    let view = source(&server).view(PR, None).await.unwrap().unwrap();
    assert_eq!(
        labelled(&view, "Add label…").disabled.as_deref(),
        Some(
            "This token cannot read its organisation's labels: \
             it lacks the read:organization scope"
        )
    );
}
