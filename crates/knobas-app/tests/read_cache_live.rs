//! **The read cache against the testenv Gitea** (#2239, ADR-0032): a file
//! opened a second time at the same commit costs no request to Gitea and
//! answers in under 150 ms; the same path at another commit is read again.
//!
//! Then the round trips (#2241): a file opened from the repository's tree
//! costs at most two requests and answers in under 400 ms; a repository's
//! grants are asked once and then behind the answer. And moving reads
//! (#2242): a second branch-list read answers from memory and reads once
//! behind it, and a branch pushed between the two is the next read's.
//!
//! Then the byte read (#2256): a private repository's README and its
//! relative PNG and SVG, read at the README's commit as the Files tab reads
//! them, byte for byte; a second look at each image costs no request.
//!
//! The requests are counted by the HTTP client itself
//! (`knobas_http::counted`), around the app's own read
//! (`sources::crud::view`), so what is counted is what the shell's
//! `entity_view` would cost. `tests/it/live_reads.rs` holds the cache's
//! other rules over a fake adapter.
//!
//! # Read-only at Gitea
//!
//! Every request here is a read but two: the moving-read test pushes a
//! branch named `read-cache-<uuid>` from `main` and deletes it again, also
//! when it fails; the byte-read test creates a private repository
//! `bytes-<uuid>` of the token's user and deletes it again the same way. What else it writes is a scratch database. The grants
//! probe's empty-bodied POSTs create nothing (`knobas-source-gitea`'s
//! `grants.rs`).
//!
//! ```text
//! just gitea-live
//! ```

use std::sync::Arc;
use std::time::{Duration, Instant};

use knobas_app::sources::Registry;
use knobas_app::sources::live::LiveSources;
use knobas_secrets::{MemoryStore, Secret, SecretStore};
use knobas_source::AuthMethod;
use knobas_source::view::Block;
use knobas_sync::scheduler::AdapterRegistry;
use sqlx::PgPool;

/// The repository `seed-gitea.sh` creates first; its `auto_init` README is in
/// every commit.
const SEEDED: &str = "tidewater/payout-service";
/// The file opened.
const FILE: &str = "file:README.md";
/// The spec's budget for a second open (#2234, Seam 1).
const CACHED_BUDGET: Duration = Duration::from_millis(150);
/// The spec's budget for an uncached open (#2234, Seam 1; #2241).
const UNCACHED_BUDGET: Duration = Duration::from_millis(400);

fn need(name: &str) -> String {
    std::env::var(name)
        .unwrap_or_else(|_| {
            panic!("{name} is not set -- run `eval \"$(testenv/seed --env)\"` first")
        })
        .trim_end_matches('/')
        .to_owned()
}

struct App {
    pool: PgPool,
    secrets: Arc<dyn SecretStore>,
    registry: Arc<dyn AdapterRegistry>,
    live: LiveSources,
    source: String,
}

async fn app() -> App {
    let pool = knobas_db::test_util::scratch_database("read_cache_live")
        .await
        .pool(4)
        .await
        .expect("a pool onto the scratch database");
    let source = format!("gitea-{}", &uuid::Uuid::new_v4().simple().to_string()[..8]);
    let secrets = Arc::new(MemoryStore::new());
    secrets
        .put(
            &knobas_secrets::KeychainAccount::source(&source),
            &Secret::just(AuthMethod::Pat, need("KNOBAS_GITEA_TOKEN")),
        )
        .expect("the token is stored");
    knobas_sync::config::insert(
        &pool,
        &knobas_sync::config::InsertConfig {
            id: source.clone(),
            adapter_kind: "gitea".to_owned(),
            display_name: source.clone(),
            base_url: need("KNOBAS_GITEA_URL"),
            auth_kind: knobas_sync::config::AuthKind::Method(AuthMethod::Pat),
            config: serde_json::json!({ "repos": [SEEDED] }),
            sync_interval_secs: 86_400,
            enabled: true,
        },
    )
    .await
    .expect("the source row is written");
    App {
        pool,
        secrets,
        registry: Arc::new(Registry::for_process()),
        live: LiveSources::default(),
        source,
    }
}

/// The two newest commits on the default branch, newest first, as Gitea
/// lists them.
async fn two_commits() -> (String, String) {
    let listed: serde_json::Value = reqwest::Client::new()
        .get(format!(
            "{}/api/v1/repos/{SEEDED}/commits?sha=main&limit=2&stat=false&files=false",
            need("KNOBAS_GITEA_URL")
        ))
        .header(
            "Authorization",
            format!("token {}", need("KNOBAS_GITEA_TOKEN")),
        )
        .send()
        .await
        .expect("Gitea answers")
        .json()
        .await
        .expect("a commit list");
    let shas: Vec<String> = listed
        .as_array()
        .expect("a list")
        .iter()
        .filter_map(|c| c["sha"].as_str().map(str::to_owned))
        .collect();
    assert_eq!(shas.len(), 2, "the seeded main has two commits: {listed}");
    (shas[0].clone(), shas[1].clone())
}

impl App {
    /// Open `FILE` at `sha` as the shell would: the answer's text, the
    /// requests it sent to Gitea and how long it took.
    async fn open(&self, sha: &str) -> (String, u64, Duration) {
        let entity = format!("{}:{SEEDED}@{sha}", self.source);
        let started = Instant::now();
        let (view, sent) = knobas_http::counted::counted(knobas_app::sources::crud::view(
            &self.pool,
            &self.secrets,
            &self.registry,
            &self.live,
            &entity,
            Some(FILE),
        ))
        .await;
        let took = started.elapsed();
        let view = view.expect("the file reads").expect("Gitea draws a commit");
        let text = view.sections[0]
            .blocks
            .iter()
            .find_map(|block| match block {
                Block::File { text, .. } => Some(text.clone()),
                _ => None,
            })
            .expect("a file block");
        (text, sent, took)
    }
}

#[tokio::test]
#[ignore = "needs the testenv Gitea: just gitea-live"]
async fn a_file_opened_again_at_its_sha_costs_no_request() {
    let app = app().await;
    let (newest, older) = two_commits().await;

    let (first, first_sent, first_took) = app.open(&newest).await;
    println!("first open:  {first_sent} requests, {first_took:?}");
    assert!(
        first_sent >= 2,
        "the counter sees the file's own reads: {first_sent}"
    );
    assert!(!first.is_empty(), "the README has text");

    let (again, sent, took) = app.open(&newest).await;
    println!("second open: {sent} requests, {took:?}");
    assert_eq!(again, first, "the same file");
    assert_eq!(sent, 0, "a second open at the sha is the cache's");
    assert!(took < CACHED_BUDGET, "a cached open took {took:?}");

    let (_, other_sent, _) = app.open(&older).await;
    println!("other sha:   {other_sent} requests");
    assert!(
        other_sent >= 2,
        "another sha is read from Gitea: {other_sent}"
    );
}

impl App {
    fn repo(&self) -> String {
        format!("{}:{SEEDED}", self.source)
    }

    /// `section` of `entity` as `entity_view` reads it, with the requests
    /// it sent -- the ones behind it too, once they ended -- and how long
    /// the answer took.
    async fn view_of(
        &self,
        entity: &str,
        section: &str,
    ) -> (knobas_source::view::SourceView, u64, Duration) {
        let ((view, took), sent) = knobas_http::counted::counted(async {
            let started = Instant::now();
            let view = knobas_app::sources::crud::view(
                &self.pool,
                &self.secrets,
                &self.registry,
                &self.live,
                entity,
                Some(section),
            )
            .await;
            let took = started.elapsed();
            self.live.settled().await;
            (view, took)
        })
        .await;
        let view = view
            .unwrap_or_else(|e| panic!("{section} reads: {e}"))
            .expect("Gitea draws it");
        (view, sent, took)
    }

    /// The asks' refusals, and the requests they sent, behind the answer too.
    async fn refused(&self, asks: &[(&str, &str)]) -> (Vec<String>, u64) {
        let asks: Vec<_> = asks
            .iter()
            .map(|(entity, op)| knobas_source::grants::WriteAsk {
                entity: (*entity).to_owned(),
                op: (*op).to_owned(),
                field: None,
            })
            .collect();
        let (refused, sent) = knobas_http::counted::counted(async {
            let refused = knobas_app::sources::crud::refused_writes(
                &self.pool,
                &self.secrets,
                &self.registry,
                &self.live,
                &asks,
            )
            .await;
            self.live.settled().await;
            refused
        })
        .await;
        (refused.into_iter().map(|r| r.reason).collect(), sent)
    }
}

fn branch_names(view: &knobas_source::view::SourceView) -> Vec<String> {
    view.sections[0]
        .blocks
        .iter()
        .flat_map(|block| match block {
            Block::Entities { rows, .. } => rows.iter().map(|r| r.title.clone()).collect(),
            _ => Vec::new(),
        })
        .collect()
}

/// **A file open costs at most two round trips** (#2241): the repository's
/// tree read once, the README opened from it costs at most two requests and
/// answers in under 400 ms, and never reads behind (a sha-keyed read, #2242).
/// A file read with no known sha still opens, reading the tip first.
#[tokio::test]
#[ignore = "needs the testenv Gitea: just gitea-live"]
async fn a_file_opened_from_the_tree_costs_at_most_two_requests() {
    let app = app().await;
    let repo = app.repo();
    let (tree, tree_sent, _) = app.view_of(&repo, "files").await;
    println!("tree:        {tree_sent} requests");
    let Some(Block::Tree { entries, .. }) = tree.sections[0].blocks.first() else {
        panic!("a listing: {tree:?}");
    };
    let readme = entries
        .iter()
        .find(|e| e.name == "README.md")
        .and_then(|e| e.section.clone())
        .expect("the README opens");
    println!("README opens as {readme}");

    // First, so a forge without `contents-ext` (Forgejo) pays its one 404
    // here: the client asks `contents` straight away after.
    let (unknown, unknown_sent, _) = app.view_of(&repo, FILE).await;
    println!("no sha:      {unknown_sent} requests");
    assert!(
        matches!(unknown.sections[0].blocks.as_slice(), [Block::File { text, .. }] if !text.is_empty()),
        "a file read with no sha opens"
    );

    let (file, sent, took) = app.view_of(&repo, &readme).await;
    println!("file open:   {sent} requests, {took:?}");
    assert!(
        matches!(file.sections[0].blocks.as_slice(), [Block::File { .. }]),
        "the file alone, no action beside it"
    );
    assert!(sent >= 1, "the counter sees the file's reads: {sent}");
    assert!(
        sent <= 2,
        "an uncached open costs at most two requests: {sent}"
    );
    assert!(took < UNCACHED_BUDGET, "an uncached open took {took:?}");

    let (_, again, _) = app.view_of(&repo, &readme).await;
    assert_eq!(again, 0, "kept, and never read behind");
}

/// **Grants once per repository** (#2241): *New branch…* and *Commit
/// change…* asked together probe Gitea; asked again they are the cache's,
/// asked once behind the answer, for the same cost again and no more. The
/// seeded token may write, so nothing is refused.
#[tokio::test]
#[ignore = "needs the testenv Gitea: just gitea-live"]
async fn a_repositorys_grants_are_asked_once_and_then_behind() {
    let app = app().await;
    let repo = app.repo();
    let asks = [
        (repo.as_str(), "create_branch"),
        (repo.as_str(), "commit_file"),
    ];
    let (first, first_sent) = app.refused(&asks).await;
    println!("grants:       {first_sent} requests, {first:?}");
    assert!(first_sent >= 1, "the probe is counted: {first_sent}");
    assert!(first.is_empty(), "the seeded token writes: {first:?}");
    let (again, again_sent) = app.refused(&asks).await;
    println!("grants again: {again_sent} requests (behind the answer)");
    assert_eq!(again, first);
    assert_eq!(again_sent, first_sent, "one ask behind, not more");
}

/// **Stale, then fresh** (#2242): a second branch-list read answers from the
/// cache at once and reads once behind it; a branch pushed between the two
/// is in the third read, with no refresh asked.
#[tokio::test]
#[ignore = "needs the testenv Gitea: just gitea-live"]
async fn a_pushed_branch_is_the_read_after_the_read_behind() {
    let app = app().await;
    let repo = app.repo();
    let (first, first_sent, _) = app.view_of(&repo, "branches").await;
    println!(
        "branches:    {first_sent} requests, {:?}",
        branch_names(&first)
    );

    let pushed = format!(
        "read-cache-{}",
        &uuid::Uuid::new_v4().simple().to_string()[..8]
    );
    let http = reqwest::Client::new();
    let api = format!(
        "{}/api/v1/repos/{SEEDED}/branches",
        need("KNOBAS_GITEA_URL")
    );
    let auth = format!("token {}", need("KNOBAS_GITEA_TOKEN"));
    let made = http
        .post(&api)
        .header("Authorization", &auth)
        .json(&serde_json::json!({ "new_branch_name": pushed, "old_branch_name": "main" }))
        .send()
        .await
        .expect("Gitea answers");
    assert!(
        made.status().is_success(),
        "the branch is pushed: {}",
        made.status()
    );
    // Deleted again however the test ends: from a thread of its own, with
    // a client of its own (a pooled connection is the test runtime's).
    struct Delete(String, String);
    impl Drop for Delete {
        fn drop(&mut self) {
            let (url, auth) = (self.0.clone(), self.1.clone());
            let _ = std::thread::spawn(move || {
                tokio::runtime::Runtime::new()
                    .unwrap()
                    .block_on(async move {
                        let _ = reqwest::Client::new()
                            .delete(url)
                            .header("Authorization", auth)
                            .send()
                            .await;
                    });
            })
            .join();
        }
    }
    let _delete = Delete(format!("{api}/{pushed}"), auth);

    let (second, second_sent, took) = app.view_of(&repo, "branches").await;
    println!("again:       {second_sent} requests, {took:?}");
    assert_eq!(
        branch_names(&second),
        branch_names(&first),
        "the kept list, at once"
    );
    assert!(took < CACHED_BUDGET, "a stale answer took {took:?}");
    assert_eq!(second_sent, first_sent, "exactly one read behind it");

    let (third, _, _) = app.view_of(&repo, "branches").await;
    assert!(
        branch_names(&third).contains(&pushed),
        "the pushed branch is the next read's: {:?}",
        branch_names(&third)
    );
}

/// A PNG's signature, header and end: the bytes are what is checked, not a picture.
const PNG: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 0x0d, 0x49, 0x48, 0x44, 0x52, 0, 0, 0,
    0x60, 0, 0, 0, 0x20, 0x08, 0x02, 0, 0, 0, 0x62, 0x8a, 0xea, 0x83, 0, 0, 0, 0, 0x49, 0x45, 0x4e,
    0x44, 0xae, 0x42, 0x60, 0x82,
];
/// An SVG with a script in it, which the shell's sanitizer drops; the byte
/// read hands it over as it is.
const SVG: &str =
    "<svg width=\"8\" height=\"8\"><script>alert(1)</script><rect width=\"8\" height=\"8\"/></svg>";

/// A private repository of the token's user holding `files` (committed in
/// order; a `README.md` goes over the one `auto_init` made), an [`App`] with
/// a source of its own scoped to it, and the repository's entity id. The
/// repository is deleted when the returned guard drops, however the test ends.
async fn scratch_repo(files: &[(&str, &[u8])]) -> (App, String, Delete) {
    use base64::Engine as _;
    let http = reqwest::Client::new();
    let base = need("KNOBAS_GITEA_URL");
    let auth = format!("token {}", need("KNOBAS_GITEA_TOKEN"));
    let user: serde_json::Value = http
        .get(format!("{base}/api/v1/user"))
        .header("Authorization", &auth)
        .send()
        .await
        .expect("Gitea answers")
        .json()
        .await
        .expect("the token's user");
    let owner = user["login"].as_str().expect("a login").to_owned();
    let name = format!("bytes-{}", &uuid::Uuid::new_v4().simple().to_string()[..8]);
    let made = http
        .post(format!("{base}/api/v1/user/repos"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({ "name": name, "private": true, "auto_init": true, "default_branch": "main" }))
        .send()
        .await
        .expect("Gitea answers");
    assert!(
        made.status().is_success(),
        "the repository is made: {}",
        made.status()
    );
    // Deleted again however the test ends, as the pushed branch is.
    let delete = Delete(format!("{base}/api/v1/repos/{owner}/{name}"), auth.clone());
    let b64 = |bytes: &[u8]| base64::engine::general_purpose::STANDARD.encode(bytes);
    for (path, content) in files {
        let (path, content) = (*path, content.to_vec());
        // README.md exists from auto_init: one PUT over it needs its sha.
        let sha = if path == "README.md" {
            let entry: serde_json::Value = http
                .get(format!(
                    "{base}/api/v1/repos/{owner}/{name}/contents/{path}"
                ))
                .header("Authorization", &auth)
                .send()
                .await
                .expect("Gitea answers")
                .json()
                .await
                .expect("the README's entry");
            entry["sha"].as_str().map(str::to_owned)
        } else {
            None
        };
        let request = if sha.is_some() {
            http.put(format!(
                "{base}/api/v1/repos/{owner}/{name}/contents/{path}"
            ))
        } else {
            http.post(format!(
                "{base}/api/v1/repos/{owner}/{name}/contents/{path}"
            ))
        };
        let committed = request
            .header("Authorization", &auth)
            .json(&serde_json::json!({ "content": b64(&content), "message": format!("add {path}"), "sha": sha }))
            .send()
            .await
            .expect("Gitea answers");
        assert!(
            committed.status().is_success(),
            "{path} is committed: {}",
            committed.status()
        );
    }

    let mut app = app().await;
    // A source of its own whose scope is the new repository.
    app.source = format!("gitea-{}", &uuid::Uuid::new_v4().simple().to_string()[..8]);
    app.secrets
        .put(
            &knobas_secrets::KeychainAccount::source(&app.source),
            &Secret::just(AuthMethod::Pat, need("KNOBAS_GITEA_TOKEN")),
        )
        .expect("the token is stored");
    knobas_sync::config::insert(
        &app.pool,
        &knobas_sync::config::InsertConfig {
            id: app.source.clone(),
            adapter_kind: "gitea".to_owned(),
            display_name: app.source.clone(),
            base_url: base.clone(),
            auth_kind: knobas_sync::config::AuthKind::Method(AuthMethod::Pat),
            config: serde_json::json!({ "repos": [format!("{owner}/{name}")] }),
            sync_interval_secs: 86_400,
            enabled: true,
        },
    )
    .await
    .expect("the source row is written");
    let repo = format!("{}:{owner}/{name}", app.source);
    (app, repo, delete)
}

/// Deletes a scratch repository on drop.
struct Delete(String, String);
impl Drop for Delete {
    fn drop(&mut self) {
        let (url, auth) = (self.0.clone(), self.1.clone());
        let _ = std::thread::spawn(move || {
            tokio::runtime::Runtime::new()
                .unwrap()
                .block_on(async move {
                    let _ = reqwest::Client::new()
                        .delete(url)
                        .header("Authorization", auth)
                        .send()
                        .await;
                });
        })
        .join();
    }
}

/// **The byte read against Gitea** (#2256): a private repository's README
/// names the commit it was read at; its relative PNG and SVG read at that
/// commit are the bytes committed; a second look at each costs no request.
#[tokio::test]
#[ignore = "needs the testenv Gitea: just gitea-live"]
async fn a_private_readmes_images_are_read_at_its_commit_and_kept() {
    let readme = "# bytes\n\n![logo](docs/img/logo.png)\n\n![flow](docs/img/flow.svg)\n";
    let (app, repo, _delete) = scratch_repo(&[
        ("docs/img/logo.png", PNG),
        ("docs/img/flow.svg", SVG.as_bytes()),
        ("README.md", readme.as_bytes()),
    ])
    .await;

    let (view, _, _) = app.view_of(&repo, FILE).await;
    let Some(Block::File { text, commit, .. }) = view.sections[0].blocks.first() else {
        panic!("the README: {view:?}");
    };
    assert_eq!(text, readme);
    let commit = commit.clone().expect("the README names its commit");
    println!("README at    {commit}");

    for (path, want) in [
        ("docs/img/logo.png", PNG),
        ("docs/img/flow.svg", SVG.as_bytes()),
    ] {
        let read = || {
            knobas_http::counted::counted(knobas_app::commands::entity::entity_file_bytes_inner(
                &app.pool,
                &app.secrets,
                &app.registry,
                &app.live,
                &repo,
                path,
                Some(&commit),
            ))
        };
        let (first, first_sent) = read().await;
        println!("{path}: {first_sent} requests");
        assert_eq!(
            first.expect("the image reads"),
            want,
            "{path} byte for byte"
        );
        assert!(first_sent >= 1, "the counter sees the read: {first_sent}");
        let (again, sent) = read().await;
        assert_eq!(again.expect("the image reads"), want);
        assert_eq!(sent, 0, "a second look at {path} is the cache's");
    }

    // A path the commit does not hold, read as a rendered diff reads a side
    // (#2279): at the commit entity, its own sha. A binary diff does not say
    // added or deleted, so `not_found` here is "no file on that side".
    let absent = knobas_app::commands::entity::entity_file_bytes_inner(
        &app.pool,
        &app.secrets,
        &app.registry,
        &app.live,
        &format!("{repo}@{commit}"),
        "docs/img/gone.png",
        None,
    )
    .await
    .expect_err("no file there");
    println!("absent path: {:?} {}", absent.code, absent.message);
    assert_eq!(
        absent.code,
        knobas_app::IpcErrorCode::NotFound,
        "{}",
        absent.message
    );
}

/// The open pull request the seed opens on the seeded repository from the
/// branch `feature/PAY-231-sepa-retry`, by its number.
async fn sepa_pull() -> u64 {
    let listed: serde_json::Value = reqwest::Client::new()
        .get(format!(
            "{}/api/v1/repos/{SEEDED}/pulls?state=open&limit=50",
            need("KNOBAS_GITEA_URL")
        ))
        .header(
            "Authorization",
            format!("token {}", need("KNOBAS_GITEA_TOKEN")),
        )
        .send()
        .await
        .expect("Gitea answers")
        .json()
        .await
        .expect("a pull request list");
    listed
        .as_array()
        .expect("a list")
        .iter()
        .find(|p| p["head"]["ref"] == "feature/PAY-231-sepa-retry")
        .and_then(|p| p["number"].as_u64())
        .unwrap_or_else(|| panic!("the seed opens the SEPA retry pull request: {listed}"))
}

/// The range a view's diff names.
fn range_of(view: &knobas_source::view::SourceView) -> knobas_source::view::DiffRange {
    view.sections[0]
        .blocks
        .iter()
        .find_map(|block| match block {
            Block::Diff { range, .. } => range.clone(),
            _ => None,
        })
        .expect("the diff names its range")
}

/// #2273, #2310: a pull request's diff names its merge base, head and
/// commits, and no last review for a token that never reviewed it; one of
/// its commits picked is a compare that names that commit's parent and the
/// commit; and a file read whole at the head for expanded context, and its
/// bytes for a rendered diff, cost no request the second time.
#[tokio::test]
#[ignore = "needs the testenv Gitea: just gitea-live"]
async fn a_pull_requests_range_is_picked_from_and_read_whole_at_its_shas() {
    let app = app().await;
    let pull = format!("{}:{SEEDED}#{}", app.source, sepa_pull().await);
    let (files, sent, _) = app.view_of(&pull, "files").await;
    println!("files: {sent} requests");
    let range = range_of(&files);
    assert_eq!(range.repository, app.repo());
    assert!(
        range.base.len() == 40 && range.head.len() == 40,
        "whole shas: {range:?}"
    );
    assert!(!range.commits.is_empty(), "its commits: {range:?}");
    assert_eq!(range.commits[0].sha, range.head, "newest first");
    assert_eq!(range.reviewed, None, "the token never reviewed it");

    // The oldest commit alone, as the picker reads it.
    let oldest = range.commits.last().unwrap().sha.clone();
    let (one, _, _) = app
        .view_of(&app.repo(), &format!("compare:{oldest}^...{oldest}"))
        .await;
    let picked = range_of(&one);
    assert_eq!(picked.head, oldest);
    assert_eq!(
        picked.base, range.base,
        "the oldest commit sits on the merge base"
    );

    let path = match &files.sections[0].blocks[0] {
        Block::Diff { files, .. } => files[0].path.clone(),
        other => panic!("a diff: {other:?}"),
    };
    let at = format!("file@{}:{path}", range.head);
    let (_, first, _) = app.view_of(&app.repo(), &at).await;
    let (_, again, took) = app.view_of(&app.repo(), &at).await;
    println!("whole at the head: {first} then {again} requests, {took:?}");
    assert!(first >= 1, "the first read is Gitea's: {first}");
    assert_eq!(again, 0, "a second expansion is the cache's");

    let repo = app.repo();
    let bytes = || {
        knobas_http::counted::counted(knobas_app::sources::crud::file_bytes(
            &app.pool,
            &app.secrets,
            &app.registry,
            &app.live,
            &repo,
            &path,
            Some(&range.head),
        ))
    };
    let (read, first) = bytes().await;
    assert!(read.expect("the bytes read").is_some());
    let (_, again) = bytes().await;
    println!("bytes at the head: {first} then {again} requests");
    assert!(first >= 1 && again == 0, "{first} then {again}");
}

/// Spec #2234 story 16: a text file past Gitea's file view (10,000 lines or
/// 1 MB) is cut in the view, and the byte read the viewer then makes at the
/// commit it was read at answers it whole, byte for byte; a second look is
/// the cache's.
#[tokio::test]
#[ignore = "needs the testenv Gitea: just gitea-live"]
async fn a_file_past_the_views_cut_is_read_whole_through_its_bytes() {
    let long: String = (1..=12_000).map(|n| format!("line {n}\n")).collect();
    let (app, repo, _delete) = scratch_repo(&[("data/ledger.csv", long.as_bytes())]).await;

    let (view, _, _) = app.view_of(&repo, "file:data/ledger.csv").await;
    let Some(Block::File {
        text,
        truncated,
        commit,
        size,
        ..
    }) = view.sections[0].blocks.first()
    else {
        panic!("the file: {view:?}");
    };
    println!(
        "view: {} lines, truncated {truncated}, size {size}",
        text.lines().count()
    );
    assert!(*truncated, "Gitea's view cuts it");
    assert!(text.lines().count() <= 10_000);
    assert_eq!(*size, long.len() as u64, "the size is the whole file's");
    let commit = commit.clone().expect("the file names its commit");

    let read = || {
        knobas_http::counted::counted(knobas_app::commands::entity::entity_file_bytes_inner(
            &app.pool,
            &app.secrets,
            &app.registry,
            &app.live,
            &repo,
            "data/ledger.csv",
            Some(&commit),
        ))
    };
    let (whole, sent) = read().await;
    println!("bytes: {sent} requests");
    assert_eq!(
        whole.expect("the bytes read"),
        long.as_bytes(),
        "whole, byte for byte"
    );
    let (_, again) = read().await;
    assert_eq!(again, 0, "a second look is the cache's");
}
