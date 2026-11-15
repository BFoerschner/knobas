//! Each open pull request's *Files*, held by the sync before anyone opens it
//! (the held-files pass of PR #2479, 2026-11-15; migration `0091`):
//! `knobas_sync::held_sections::refresh` with the **real** Gitea adapter
//! against a fake Gitea, then a page's reads through `sources::crud::shown`.
//!
//! The held answer is pinned byte-equal on the wire to what the page draws
//! from the forge's own read, on a diff that would show a difference: a
//! rename with changes and one without, a binary file, a name outside ASCII,
//! two hundred files, and line comments on lines that stayed and on lines a
//! later push moved (outdated, drawn apart with their hunk).

use std::sync::Arc;

use knobas_app::sources::live::{LiveSources, ShownView};
use knobas_app::sources::{self, NewSource, Registry, SecretInput};
use knobas_secrets::{MemoryStore, SecretStore};
use knobas_source::instance::SourceInstance;
use knobas_source::{AuthMethod, Source};
use knobas_sync::held_sections::{self, Held};
use knobas_sync::scheduler::AdapterRegistry;
use serde_json::{Value, json};
use sqlx::PgPool;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const REPO: &str = "/api/v1/repos/tidewater/svc";

fn sha(n: u32) -> String {
    format!("{n:040x}")
}

/// The commit a reviewer's comments were written on, before the last push.
fn first() -> String {
    sha(0xf1)
}

/// The `.diff` of pull request `n`: a file whose line 2 changed (the
/// commented one), a rename with a change, a rename alone, a binary file, a
/// name outside ASCII, and `bulk` more files of thirty lines each, every line
/// `wide` characters longer than its own.
fn diff_text(n: u64, bulk: usize, wide: usize) -> String {
    let pad = "x".repeat(wide);
    let mut out = format!(
        "diff --git a/src/retry.rs b/src/retry.rs
index 1111111..2222222 100644
--- a/src/retry.rs
+++ b/src/retry.rs
@@ -1,5 +1,6 @@
 pub fn backoff() -> u64 {{
-    30
+    30 + jitter()
+    // pull request {n}
 }}

 pub fn jitter() -> u64 {{ 3 }}
diff --git a/src/old_name.rs b/src/new_name.rs
similarity index 91%
rename from src/old_name.rs
rename to src/new_name.rs
index 3333333..4444444 100644
--- a/src/old_name.rs
+++ b/src/new_name.rs
@@ -1,2 +1,2 @@
-pub const NAME: &str = \"old\";
+pub const NAME: &str = \"new\";
 pub const KEEP: u8 = 1;
diff --git a/docs/a.md b/docs/b.md
similarity index 100%
rename from docs/a.md
rename to docs/b.md
diff --git a/assets/logo.png b/assets/logo.png
index 5555555..6666666 100644
Binary files a/assets/logo.png and b/assets/logo.png differ
diff --git a/docs/\u{fc}bersicht.md b/docs/\u{fc}bersicht.md
new file mode 100644
index 0000000..7777777
--- /dev/null
+++ b/docs/\u{fc}bersicht.md
@@ -0,0 +1 @@
+Gr\u{fc}\u{df}e
"
    );
    for f in 0..bulk {
        out.push_str(&format!(
            "diff --git a/gen/f{f}.rs b/gen/f{f}.rs\nnew file mode 100644\nindex 0000000..{f:07x}\n--- /dev/null\n+++ b/gen/f{f}.rs\n@@ -0,0 +1,30 @@\n"
        ));
        for line in 0..30 {
            out.push_str(&format!(
                "+pub const C{line}: u32 = {f} * {line}; // pull request {n}{pad}\n"
            ));
        }
    }
    out
}

/// One of Gitea's `PullReviewComment`s on `src/retry.rs` (the shape
/// `knobas-source-gitea`'s `view_pr.rs` takes from the real server).
fn line_comment(id: u64, who: &str, new: u32, old: u32, hunk_tail: &str, body: &str) -> Value {
    json!({
        "id": id,
        "user": { "login": who },
        "created_at": "2026-11-15T09:00:00Z",
        "path": "src/retry.rs",
        "position": new,
        "original_position": old,
        "diff_hunk": format!("@@ -1,3 +1,3 @@\n pub fn backoff() -> u64 {{\n{hunk_tail}"),
        "commit_id": first(),
        "body": body,
    })
}

/// What the fake answers for one pull request.
#[derive(Clone)]
struct Pull {
    number: u64,
    base: String,
    head: String,
    bulk: usize,
    /// How much longer each generated line is.
    wide: usize,
    /// The record's count of line comments.
    comments: u64,
}

impl Pull {
    fn record(&self) -> Value {
        json!({
            "number": self.number, "title": format!("Change {}", self.number), "body": "",
            "state": "open", "merged": false, "mergeable": true,
            "user": { "login": "mara" },
            "head": { "ref": format!("feat-{}", self.number), "sha": self.head,
                      "repo": { "full_name": "tidewater/svc" } },
            "base": { "ref": "main", "repo": { "full_name": "tidewater/svc", "name": "svc" } },
            "merge_base": self.base,
            "created_at": "2026-11-14T10:00:00Z", "updated_at": "2026-11-15T10:00:00Z",
            "html_url": format!("https://gitea.example/tidewater/svc/pulls/{}", self.number),
            "changed_files": 5 + self.bulk, "comments": 0,
            "review_comments": self.comments
        })
    }
}

/// Mount `pull` on `server` at `priority`, over anything mounted for it
/// before at a lower one (wiremock answers the first of equal priority).
async fn mount(server: &MockServer, pull: &Pull, priority: u8) {
    let n = pull.number;
    let at = |p: String| Mock::given(method("GET")).and(path(p));
    let commits = json!([
        { "sha": pull.head, "created": "2026-11-15T09:30:00Z",
          "commit": { "message": "Jitter", "author": { "name": "Mara", "date": "2026-11-15T09:30:00Z" } } },
        { "sha": first(), "created": "2026-11-14T09:30:00Z",
          "commit": { "message": "Retry", "author": { "name": "Mara", "date": "2026-11-14T09:30:00Z" } } }
    ]);
    let reviews = json!([
        { "id": 21, "user": { "login": "priya" }, "state": "REQUEST_CHANGES", "commit_id": first(),
          "submitted_at": "2026-11-15T09:00:00Z", "body": "", "comments_count": 2 },
        { "id": 22, "user": { "login": "knobas" }, "state": "APPROVED", "commit_id": first(),
          "submitted_at": "2026-11-15T09:10:00Z", "body": "", "comments_count": 0 },
        { "id": 23, "user": { "login": "olu" }, "state": "COMMENT", "commit_id": first(),
          "submitted_at": "2026-11-15T09:20:00Z", "body": "", "comments_count": 2 }
    ]);
    let by_review = [
        (
            21,
            json!([
                // On new line 2, which still reads so: placed on it.
                line_comment(
                    101,
                    "priya",
                    2,
                    0,
                    "-    30\n+    30 + jitter()",
                    "Bound it."
                ),
                // On the old side's line 2.
                line_comment(102, "priya", 0, 2, "-    30", "Why drop the constant?"),
            ]),
        ),
        (
            23,
            json!([
                // Written on a new line 3 that read `    35`; a later push
                // moved it, and it reads a comment now: outdated.
                line_comment(201, "olu", 3, 0, "-    30\n+    35", "Why 35?"),
                // A line the diff no longer has at all.
                line_comment(202, "olu", 40, 0, "+    gone", "And this?"),
            ]),
        ),
    ];
    let p = format!("{REPO}/pulls/{n}");
    for (route, answer) in [
        (
            p.clone(),
            ResponseTemplate::new(200).set_body_json(pull.record()),
        ),
        (
            format!("{p}/commits"),
            ResponseTemplate::new(200)
                .insert_header("X-Total-Count", "2")
                .set_body_json(commits),
        ),
        (
            format!("{p}/reviews"),
            ResponseTemplate::new(200).set_body_json(reviews),
        ),
        (
            format!("{p}.diff"),
            ResponseTemplate::new(200).set_body_string(diff_text(n, pull.bulk, pull.wide)),
        ),
    ] {
        at(route)
            .respond_with(answer)
            .with_priority(priority)
            .mount(server)
            .await;
    }
    for (id, comments) in by_review {
        at(format!("{p}/reviews/{id}/comments"))
            .respond_with(ResponseTemplate::new(200).set_body_json(comments))
            .with_priority(priority)
            .mount(server)
            .await;
    }
}

struct Fixture {
    pool: PgPool,
    secrets: Arc<dyn SecretStore>,
    server: MockServer,
    id: String,
    repo: String,
    /// The next mount's priority: each mount answers over the ones before.
    priority: std::sync::atomic::AtomicU8,
}

fn registry() -> Arc<dyn AdapterRegistry> {
    crate::support::shared(Registry::for_process())
}

impl Fixture {
    fn pr(&self, n: u64) -> String {
        format!("{}#{n}", self.repo)
    }

    /// The source the sync runs, built as the scheduler builds it.
    fn source(&self) -> Box<dyn Source> {
        Registry::for_process()
            .build(SourceInstance {
                id: self.id.clone(),
                kind: "gitea".to_owned(),
                display_name: "Tidewater Git".to_owned(),
                base_url: self.server.uri(),
                auth: Some(AuthMethod::Pat),
                secret: Some("pat".to_owned()),
                account: None,
                config: json!({ "username": "knobas" }),
            })
            .expect("Gitea builds")
    }

    async fn refresh(&self) -> Held {
        // On the background lane, as the scheduler runs it.
        knobas_http::on_lane(
            knobas_http::Lane::Background,
            held_sections::refresh(&self.pool, &self.id, self.source().as_ref()),
        )
        .await
        .unwrap()
    }

    async fn refresh_within(&self, requests: u32, share: i64) -> Held {
        self.refresh_budget(requests, held_sections::BUDGET, share)
            .await
    }

    async fn refresh_budget(&self, requests: u32, budget: std::time::Duration, share: i64) -> Held {
        knobas_http::on_lane(
            knobas_http::Lane::Background,
            held_sections::refresh_within(
                &self.pool,
                &self.id,
                self.source().as_ref(),
                requests,
                budget,
                share,
            ),
        )
        .await
        .unwrap()
    }

    /// A page's *Files*, through `live`.
    async fn shown(&self, live: &LiveSources, n: u64) -> ShownView {
        sources::crud::shown(
            &self.pool,
            &self.secrets,
            &registry(),
            live,
            &self.pr(n),
            Some("files"),
        )
        .await
        .unwrap()
        .expect("drawn")
    }

    async fn mount(&self, pull: &Pull) {
        let priority = self
            .priority
            .fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
        mount(&self.server, pull, priority).await;
    }

    /// What the fake was asked so far, once a page's first read has had the
    /// time to send the keeping read it leaves behind (Gitea's `keep_behind`).
    async fn requests(&self) -> usize {
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        self.server
            .received_requests()
            .await
            .unwrap_or_default()
            .len()
    }

    async fn item(&self, id: &str, kind: &str, title: &str, payload: Value, hours_ago: i32) {
        sqlx::query("insert into knobas.entity (id, kind, title) values ($1, $2, $3)")
            .bind(id)
            .bind(kind)
            .bind(title)
            .execute(&self.pool)
            .await
            .unwrap();
        sqlx::query(
            "insert into sync.item (entity_id, source_id, kind, title, body_text, payload,
                                    item_updated_at, synced_at)
             values ($1, $2, $3, $4, '', $5, now() - make_interval(hours => $6), now())",
        )
        .bind(id)
        .bind(&self.id)
        .bind(kind)
        .bind(title)
        .bind(payload)
        .bind(hours_ago)
        .execute(&self.pool)
        .await
        .unwrap();
    }

    /// Pull request `pull` open on the forge and in the mirror.
    async fn open(&self, pull: &Pull, hours_ago: i32) {
        self.mount(pull).await;
        self.item(
            &self.pr(pull.number),
            "pr",
            &format!("Change {}", pull.number),
            pull.record(),
            hours_ago,
        )
        .await;
    }

    /// The forge and the mirror move `pull` to its new commits, as a push and
    /// the sync after it do.
    async fn moved(&self, pull: &Pull) {
        self.mount(pull).await;
        sqlx::query("update sync.item set payload = $2 where entity_id = $1")
            .bind(self.pr(pull.number))
            .bind(pull.record())
            .execute(&self.pool)
            .await
            .unwrap();
    }

    /// `(entity, speculative)` of every held row of the source.
    async fn held(&self) -> Vec<(String, bool)> {
        sqlx::query_as(
            "select entity_id, speculative from sync.held_read where source_id = $1 order by 1",
        )
        .bind(&self.id)
        .fetch_all(&self.pool)
        .await
        .unwrap()
    }

    /// Every held row's and note's version: a write moves it.
    async fn versions(&self) -> Vec<String> {
        sqlx::query_scalar(
            "select xmin::text || entity_id from sync.held_read where source_id = $1
             union all
             select xmin::text || entity_id from sync.held_section where source_id = $1
             order by 1",
        )
        .bind(&self.id)
        .fetch_all(&self.pool)
        .await
        .unwrap()
    }
}

/// A source over a fake Gitea, its repository in the mirror, and pull
/// request 7 open with a diff of two hundred and five files.
async fn fixture() -> (Fixture, Pull) {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    let server = MockServer::builder().start().await;
    let at = |p: &str| Mock::given(method("GET")).and(path(p.to_owned()));
    at("/api/v1/user")
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "login": "knobas" })))
        .mount(&server)
        .await;
    at("/api/v1/version")
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "version": "1.27.3" })))
        .mount(&server)
        .await;
    let id = format!("gh{}", &uuid::Uuid::new_v4().simple().to_string()[..10]);
    let secrets: Arc<dyn SecretStore> = Arc::new(MemoryStore::new());
    sources::crud::add(
        &pool,
        &secrets,
        &Registry::for_process(),
        NewSource {
            id: id.clone(),
            adapter_kind: "gitea".to_owned(),
            display_name: "Tidewater Git".to_owned(),
            base_url: server.uri(),
            auth_kind: AuthMethod::Pat,
            config: json!({ "username": "knobas" }),
            secret: SecretInput::of("pat"),
            sync_interval_secs: 300,
            enabled: true,
        },
    )
    .await
    .unwrap();
    let f = Fixture {
        pool,
        secrets,
        server,
        repo: format!("{id}:tidewater/svc"),
        id,
        priority: std::sync::atomic::AtomicU8::new(200),
    };
    f.item(
        &f.repo,
        "repo",
        "tidewater/svc",
        json!({ "id": 1, "name": "svc", "full_name": "tidewater/svc",
                "owner": { "login": "tidewater" }, "default_branch": "main" }),
        1,
    )
    .await;
    let pull = Pull {
        number: 7,
        base: sha(0xb1),
        head: sha(0xc1),
        bulk: 200,
        wide: 0,
        comments: 4,
    };
    f.open(&pull, 2).await;
    (f, pull)
}

/// The wire a page draws, without when it was read.
fn wire(mut shown: ShownView) -> Vec<u8> {
    shown.held_at = None;
    serde_json::to_vec(&shown).unwrap()
}

/// The pass reads an open pull request's *Files* once, through the page's
/// own read, and a page's first open draws it from the store, said to be as
/// knobas read it, byte for byte what the page draws from the forge's read
/// made now: renames, a binary file, a name outside ASCII, two hundred files,
/// threads on their lines and the outdated ones apart with their hunks.
#[tokio::test]
async fn the_held_files_are_what_the_page_draws_from_the_forge_byte_for_byte() {
    let (f, _) = fixture().await;
    let before = f.requests().await;
    let held = f.refresh().await;
    // Nothing is left reading behind the pass: a pull request's first read
    // leaves a keeping read behind it for a page, never for the sync, whose
    // instance ends with its run (and whose lane a spawned read would leave).
    assert_eq!(
        f.requests().await - before,
        usize::try_from(held.requests).unwrap(),
        "the forge saw what the pass counted"
    );
    assert_eq!(
        (held.read, held.left, held.too_large),
        (1, 0, 0),
        "{held:?}"
    );
    assert!(
        held.requests >= 4,
        "the record, commits, reviews, .diff: {held:?}"
    );
    assert_eq!(
        f.held().await,
        [(f.pr(7), true)],
        "speculative until a page draws it"
    );

    let holding = LiveSources::default();
    holding.hold_in(f.pool.clone());
    let drawn = f.shown(&holding, 7).await;
    assert!(
        drawn.held_at.is_some(),
        "drawn from the store, as knobas read it"
    );
    holding.settled().await;

    // The page's read of the forge, with nothing held.
    let now = f.shown(&LiveSources::default(), 7).await;
    assert!(now.held_at.is_none());
    let (drawn, now) = (wire(drawn), wire(now));
    let text = String::from_utf8_lossy(&now);
    for shape in [
        "\"renamed\"",
        "\"binary\"",
        "\u{fc}bersicht",
        "gen/f199.rs",
        "\"outdated\":true",
        "Why 35?",
    ] {
        assert!(text.contains(shape), "the forge's read holds {shape}");
    }
    assert!(
        drawn == now,
        "the held answer is the forge's own, byte for byte"
    );
    assert_eq!(
        f.held().await,
        [(f.pr(7), false)],
        "a person's once a page drew it"
    );
}

/// A run in which no pull request's commits moved asks the forge nothing and
/// writes nothing; a moved head, or a moved merge base, is read again.
#[tokio::test]
async fn only_a_pull_request_whose_head_or_merge_base_moved_is_read_again() {
    let (f, mut pull) = fixture().await;
    f.refresh().await;
    let versions = f.versions().await;
    let before = f.requests().await;
    let again = f.refresh().await;
    assert_eq!(again, Held::default(), "nothing read, nothing noted");
    assert_eq!(f.requests().await, before, "no request");
    assert_eq!(f.versions().await, versions, "no row written");

    pull.head = sha(0xc2);
    f.moved(&pull).await;
    let pushed = f.refresh().await;
    assert_eq!(pushed.read, 1, "a push: {pushed:?}");

    pull.base = sha(0xb2);
    f.moved(&pull).await;
    let rebased = f.refresh().await;
    assert_eq!(rebased.read, 1, "the base moved under it: {rebased:?}");
    let stands: (String, String) =
        sqlx::query_as("select base, head from sync.held_section where source_id = $1")
            .bind(&f.id)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert_eq!(stands, (sha(0xb2), sha(0xc2)));
}

/// A pull request no longer open drops its speculative row and note at the
/// next pass; one a page drew stays, since a person read it.
#[tokio::test]
async fn a_closed_pull_request_drops_what_only_the_sync_held() {
    let (f, _) = fixture().await;
    let other = Pull {
        number: 8,
        base: sha(0xb8),
        head: sha(0xc8),
        bulk: 2,
        wide: 0,
        comments: 4,
    };
    f.open(&other, 3).await;
    f.refresh().await;
    let holding = LiveSources::default();
    holding.hold_in(f.pool.clone());
    f.shown(&holding, 8).await;
    holding.settled().await;
    sqlx::query(
        "update sync.item set payload = jsonb_set(payload, '{state}', '\"closed\"')
          where source_id = $1 and kind = 'pr'",
    )
    .bind(&f.id)
    .execute(&f.pool)
    .await
    .unwrap();
    let closed = f.refresh().await;
    assert_eq!(closed.dropped, 1, "{closed:?}");
    assert_eq!(
        f.held().await,
        [(f.pr(8), false)],
        "the one a page drew stays"
    );
    let notes: i64 =
        sqlx::query_scalar("select count(*) from sync.held_section where source_id = $1")
            .bind(&f.id)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert_eq!(notes, 1, "the closed one's note went with its row");
}

/// The pass stops at its request cap, the most recently updated first, and
/// leaves the rest for the next run; it fills no more than its share of the
/// store, and drops what lies past it.
#[tokio::test]
async fn the_pass_keeps_to_its_request_cap_and_its_share_of_the_store() {
    let (f, _) = fixture().await;
    for (n, hours) in [(8, 1), (9, 5)] {
        let pull = Pull {
            number: n,
            base: sha(0xb0 + u32::try_from(n).unwrap()),
            head: sha(0xc0 + u32::try_from(n).unwrap()),
            bulk: 3,
            wide: 0,
            comments: 4,
        };
        f.open(&pull, hours).await;
    }
    // One read's worth of requests: #8, the newest, and the rest left.
    let capped = f.refresh_within(1, held_sections::SHARE).await;
    assert_eq!((capped.read, capped.left), (1, 2), "{capped:?}");
    assert_eq!(f.held().await, [(f.pr(8), true)]);
    // The next run reads what was left.
    let rest = f.refresh().await;
    assert_eq!((rest.read, rest.left), (2, 0), "{rest:?}");
    // A share of one byte: the newest is kept, and what lies past it goes.
    let small = f.refresh_within(held_sections::REQUESTS_PER_RUN, 1).await;
    assert_eq!(small.dropped, 2, "{small:?}");
    assert_eq!(f.held().await, [(f.pr(8), true)]);
    let again = f.refresh_within(held_sections::REQUESTS_PER_RUN, 1).await;
    assert_eq!(
        (again.read, again.requests, again.dropped),
        (0, 0, 0),
        "{again:?}"
    );
}

/// A *Files* over the ordinary 2 MiB cap is held too (up to 8 MiB, Fable's
/// ruling), byte for byte the page's read of the forge; one over 8 MiB is read
/// once and noted, and not read again until its commits move.
#[tokio::test]
async fn a_large_files_is_held_and_one_too_large_is_not_read_again() {
    let (f, mut pull) = fixture().await;
    pull.bulk = 1_500;
    f.moved(&pull).await;
    let first = f.refresh().await;
    assert_eq!((first.read, first.too_large), (1, 0), "{first:?}");
    assert!(
        usize::try_from(first.bytes).unwrap() > knobas_sync::held_read::ROW_CAP,
        "over the ordinary cap: {first:?}"
    );
    let holding = LiveSources::default();
    holding.hold_in(f.pool.clone());
    let drawn = f.shown(&holding, 7).await;
    assert!(drawn.held_at.is_some());
    holding.settled().await;
    let now = f.shown(&LiveSources::default(), 7).await;
    assert!(
        wire(drawn) == wire(now),
        "the large held answer is the forge's own, byte for byte"
    );

    pull.head = sha(0xc5);
    pull.wide = 200;
    f.moved(&pull).await;
    let huge = f.refresh().await;
    assert_eq!((huge.read, huge.too_large), (0, 1), "{huge:?}");
    let before = f.requests().await;
    let again = f.refresh().await;
    assert_eq!(again, Held::default());
    assert_eq!(f.requests().await, before);
}

/// Rows over 2 MiB take at most half the pass's share between them: a second
/// large pull request past that half is noted, not held, so a few large ones
/// never push the ordinary ones out.
#[tokio::test]
async fn large_rows_take_at_most_half_the_share() {
    let (f, _) = fixture().await;
    for (n, hours) in [(8, 0), (9, 1)] {
        let pull = Pull {
            number: n,
            base: sha(0xb0 + u32::try_from(n).unwrap()),
            head: sha(0xc0 + u32::try_from(n).unwrap()),
            bulk: 1_500,
            wide: 0,
            comments: 4,
        };
        f.open(&pull, hours).await;
    }
    // A share whose half holds one large row and not two.
    let share = 16 * 1024 * 1024;
    let held = f
        .refresh_within(held_sections::REQUESTS_PER_RUN, share)
        .await;
    assert_eq!(
        (held.read, held.too_large),
        (2, 1),
        "#8 and #7 held, #9 noted: {held:?}"
    );
    let again = f
        .refresh_within(held_sections::REQUESTS_PER_RUN, share)
        .await;
    assert_eq!((again.read, again.requests), (0, 0), "{again:?}");
}

/// A pull request whose count of line comments moved with no push is read
/// again, whole; a note that names no count (held before counts were kept)
/// takes the record's with no read.
#[tokio::test]
async fn a_new_line_comment_reads_the_files_again() {
    let (f, mut pull) = fixture().await;
    f.refresh().await;
    pull.comments = 5;
    f.moved(&pull).await;
    let commented = f.refresh().await;
    assert_eq!(commented.read, 1, "{commented:?}");
    sqlx::query("update sync.held_section set count = null where source_id = $1")
        .bind(&f.id)
        .execute(&f.pool)
        .await
        .unwrap();
    let before = f.requests().await;
    let adopted = f.refresh().await;
    assert_eq!((adopted.noted, adopted.read), (1, 0), "{adopted:?}");
    assert_eq!(f.requests().await, before);
    let count: Option<String> =
        sqlx::query_scalar("select count from sync.held_section where source_id = $1")
            .bind(&f.id)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert_eq!(count.as_deref(), Some("5"));
}

/// A read that had the whole budget and did not end goes last in the runs
/// after, as a failure does, so it does not take every run's budget.
#[tokio::test]
async fn a_read_that_took_the_whole_budget_goes_last() {
    let (f, _) = fixture().await;
    let slow = Pull {
        number: 8,
        base: sha(0xb8),
        head: sha(0xc8),
        bulk: 1,
        wide: 0,
        comments: 4,
    };
    f.open(&slow, 0).await;
    Mock::given(method("GET"))
        .and(path(format!("{REPO}/pulls/8.diff")))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(diff_text(8, 1, 0))
                .set_delay(std::time::Duration::from_secs(3)),
        )
        .with_priority(1)
        .mount(&f.server)
        .await;
    let budget = std::time::Duration::from_millis(800);
    let first = f
        .refresh_budget(
            held_sections::REQUESTS_PER_RUN,
            budget,
            held_sections::SHARE,
        )
        .await;
    assert_eq!(
        (first.read, first.left, first.stopped_by),
        (0, 2, Some("time")),
        "{first:?}"
    );
    let second = f
        .refresh_budget(
            held_sections::REQUESTS_PER_RUN,
            budget,
            held_sections::SHARE,
        )
        .await;
    assert_eq!(second.read, 1, "#7 first now: {second:?}");
    assert_eq!(f.held().await, [(f.pr(7), true)]);
}

/// A *Files* a page read at the record's commits is noted with no request:
/// the pass does not read again what a person's read holds.
#[tokio::test]
async fn a_persons_read_at_the_records_commits_is_not_read_again() {
    let (f, _) = fixture().await;
    let holding = LiveSources::default();
    holding.hold_in(f.pool.clone());
    f.shown(&holding, 7).await;
    holding.settled().await;
    let before = f.requests().await;
    let held = f.refresh().await;
    assert_eq!(
        (held.noted, held.read, held.requests),
        (1, 0, 0),
        "{held:?}"
    );
    assert_eq!(f.requests().await, before);
    assert_eq!(f.held().await, [(f.pr(7), false)]);
}

/// A pull request the forge fails on as a whole (a 5xx after the client's
/// retries) stops the run, and goes last in the runs after it, so it holds up
/// no other pull request; one the forge says it does not have (a 404) is not
/// asked again until its commits move.
#[tokio::test]
async fn a_pull_request_the_forge_fails_on_holds_up_no_other() {
    let (f, _) = fixture().await;
    let failing = Pull {
        number: 8,
        base: sha(0xb8),
        head: sha(0xc8),
        bulk: 1,
        wide: 0,
        comments: 4,
    };
    f.open(&failing, 1).await;
    Mock::given(method("GET"))
        .and(path(format!("{REPO}/pulls/8.diff")))
        .respond_with(ResponseTemplate::new(500))
        .with_priority(1)
        .mount(&f.server)
        .await;
    let first = f.refresh().await;
    assert_eq!(
        (first.read, first.left),
        (0, 2),
        "#8, the newest, failed and stopped the run: {first:?}"
    );
    let second = f.refresh().await;
    assert_eq!(
        (second.read, second.left),
        (1, 1),
        "#7 first, then #8 again: {second:?}"
    );
    assert_eq!(f.held().await, [(f.pr(7), true)]);

    let gone = Pull {
        number: 9,
        base: sha(0xb9),
        head: sha(0xc9),
        bulk: 1,
        wide: 0,
        comments: 4,
    };
    f.item(&f.pr(9), "pr", "Change 9", gone.record(), 0).await;
    let missing = f.refresh().await;
    assert_eq!(missing.refused, 1, "{missing:?}");
    let before = f.requests().await;
    let again = f.refresh().await;
    assert_eq!(again.refused, 0, "#9 is not asked again: {again:?}");
    let asked: Vec<String> = f.server.received_requests().await.unwrap_or_default()[before..]
        .iter()
        .map(|r| r.url.path().to_owned())
        .collect();
    assert!(
        asked.iter().all(|p| !p.contains("/pulls/9")),
        "only #8 is asked again: {asked:?}"
    );
}
