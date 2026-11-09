//! **A slow forge, measured** (the smoothness pass on PR #2479): every read a
//! Code page waits on, timed through a proxy that holds each request to the
//! testenv Gitea for a set time, or forever.
//!
//! Nothing here asserts a budget. It prints one `SLOW` line per read and
//! mode, `cold` (the first read in a session: a fresh [`LiveSources`]) and
//! `warm` (the same read again at once), which is what the pull request's
//! before-and-after table is made of. The reads are the app's own
//! (`sources::crud::view`, `checkout::range_diff`, ...), the ones the shell's
//! IPC commands call, so a reading is what the page would wait for, short of
//! the bridge.
//!
//! # The proxy
//!
//! [`slow::Proxy`] forwards bytes between knobas and Gitea and holds the first
//! chunk of each request for the mode's delay; `hang` holds it forever, so the
//! HTTP client's own timeouts decide (30 s a try, 45 s a call), and `down`
//! closes every connection as it opens. It runs in this process, on loopback:
//! nothing is installed on the WSL box.
//!
//! # Read-only at Gitea
//!
//! Every request is a read. What it writes is local: a scratch database, a
//! clone in a temporary folder.
//!
//! ```text
//! eval "$(testenv/seed --env)"
//! cargo test -p knobas-app --test slow_forge_live -- --ignored --nocapture
//! ```

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use knobas_app::checkout::clone::{CloneEvent, CloneEvents, CloneState, Clones};
use knobas_app::sources::Registry;
use knobas_app::sources::live::LiveSources;
use knobas_secrets::{MemoryStore, Secret, SecretStore};
use knobas_source::AuthMethod;
use knobas_source::view::{Block, SourceView};
use knobas_sync::scheduler::AdapterRegistry;
use sqlx::PgPool;

/// The seeded repository every read is of.
const SEEDED: &str = "tidewater/payout-service";
/// A seeded branch with an open pull request into `main`.
const BRANCH: &str = "feature/PAY-231-sepa-retry";
/// No read is waited on longer than this; past it the line says so.
const CAP: Duration = Duration::from_secs(70);

fn need(name: &str) -> String {
    std::env::var(name)
        .unwrap_or_else(|_| {
            panic!("{name} is not set -- run `eval \"$(testenv/seed --env)\"` first")
        })
        .trim_end_matches('/')
        .to_owned()
}

/// The delaying proxy.
mod slow {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::time::Duration;

    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::{TcpListener, TcpStream};

    /// Hold every request forever.
    pub const HANG: u64 = u64::MAX;
    /// Close every connection as it opens.
    pub const DOWN: u64 = u64::MAX - 1;

    pub struct Proxy {
        pub url: String,
        delay: Arc<AtomicU64>,
    }

    impl Proxy {
        /// Hold each request `ms` milliseconds, or [`HANG`] or [`DOWN`].
        pub fn set(&self, ms: u64) {
            self.delay.store(ms, Ordering::SeqCst);
        }
    }

    /// A proxy on loopback in front of `upstream` (`http://host:port`).
    pub async fn start(upstream: &str) -> Proxy {
        let target = upstream
            .trim_start_matches("http://")
            .trim_end_matches('/')
            .to_owned();
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("a port");
        let port = listener.local_addr().unwrap().port();
        let delay = Arc::new(AtomicU64::new(0));
        let shared = delay.clone();
        tokio::spawn(async move {
            loop {
                let Ok((client, _)) = listener.accept().await else {
                    return;
                };
                let (target, delay) = (target.clone(), shared.clone());
                tokio::spawn(async move {
                    let _ = pipe(client, &target, delay).await;
                });
            }
        });
        Proxy {
            url: format!("http://127.0.0.1:{port}"),
            delay,
        }
    }

    async fn pipe(client: TcpStream, target: &str, delay: Arc<AtomicU64>) -> std::io::Result<()> {
        if delay.load(Ordering::SeqCst) == DOWN {
            return Ok(());
        }
        let server = TcpStream::connect(target).await?;
        let (mut from_client, mut to_client) = client.into_split();
        let (mut from_server, mut to_server) = server.into_split();
        // The next bytes from the client start a request.
        let fresh = Arc::new(AtomicBool::new(true));
        let seen = fresh.clone();
        let up = async move {
            let mut buf = vec![0u8; 64 * 1024];
            loop {
                let n = from_client.read(&mut buf).await?;
                if n == 0 {
                    let _ = to_server.shutdown().await;
                    return Ok::<_, std::io::Error>(());
                }
                if seen.swap(false, Ordering::SeqCst) {
                    match delay.load(Ordering::SeqCst) {
                        HANG => std::future::pending::<()>().await,
                        0 => {}
                        ms => tokio::time::sleep(Duration::from_millis(ms)).await,
                    }
                }
                to_server.write_all(&buf[..n]).await?;
            }
        };
        let down = async move {
            let mut buf = vec![0u8; 64 * 1024];
            loop {
                let n = from_server.read(&mut buf).await?;
                if n == 0 {
                    let _ = to_client.shutdown().await;
                    return Ok::<_, std::io::Error>(());
                }
                fresh.store(true, Ordering::SeqCst);
                to_client.write_all(&buf[..n]).await?;
            }
        };
        let _ = tokio::join!(up, down);
        Ok(())
    }
}

#[derive(Default)]
struct Seen(std::sync::Mutex<Vec<CloneEvent>>);

impl CloneEvents for Seen {
    fn clone_progress(&self, event: CloneEvent) {
        self.0.lock().unwrap().push(event);
    }
}

struct Rig {
    pool: PgPool,
    secrets: Arc<dyn SecretStore>,
    registry: Arc<dyn AdapterRegistry>,
    proxy: slow::Proxy,
    repo: String,
    branch: String,
    pr: String,
    commit: (String, String),
    _clones: tempfile::TempDir,
}

/// A scratch database with the seeded Gitea source behind the proxy, synced
/// once at full speed, and the repository cloned into a clones root.
async fn rig() -> Rig {
    let proxy = slow::start(&need("KNOBAS_GITEA_URL")).await;
    let pool = knobas_db::test_util::scratch_database("slow_forge_live")
        .await
        .pool(4)
        .await
        .expect("a pool onto the scratch database");
    let source = format!("gitea-{}", &uuid::Uuid::new_v4().simple().to_string()[..8]);
    let token = need("KNOBAS_GITEA_TOKEN");
    let secrets = Arc::new(MemoryStore::new());
    secrets
        .put(
            &knobas_secrets::KeychainAccount::source(&source),
            &Secret::just(AuthMethod::Pat, token.clone()),
        )
        .expect("the token is stored");
    knobas_sync::config::insert(
        &pool,
        &knobas_sync::config::InsertConfig {
            id: source.clone(),
            adapter_kind: "gitea".to_owned(),
            display_name: source.clone(),
            base_url: proxy.url.clone(),
            auth_kind: knobas_sync::config::AuthKind::Method(AuthMethod::Pat),
            config: serde_json::json!({ "repos": [SEEDED] }),
            sync_interval_secs: 86_400,
            enabled: true,
        },
    )
    .await
    .expect("the source row is written");
    let built = knobas_source_gitea::build(knobas_source::instance::SourceInstance {
        id: source.clone(),
        kind: "gitea".to_owned(),
        display_name: source.clone(),
        base_url: proxy.url.clone(),
        auth: Some(AuthMethod::Pat),
        secret: Some(token),
        account: None,
        config: serde_json::json!({ "repos": [SEEDED] }),
    })
    .expect("the source builds");
    knobas_sync::run_once(&pool, built.as_ref(), None)
        .await
        .expect("the seeded repository syncs");

    let id = |kind: &str, like: String| {
        let pool = pool.clone();
        let kind = kind.to_owned();
        async move {
            sqlx::query_scalar::<_, String>(
                "select entity_id from sync.item where kind = $1 and entity_id like $2
                 order by entity_id limit 1",
            )
            .bind(&kind)
            .bind(&like)
            .fetch_optional(&pool)
            .await
            .unwrap()
            .unwrap_or_else(|| panic!("the mirror holds a {kind} like {like}"))
        }
    };
    let repo = format!("{source}:{SEEDED}");
    let branch = id("branch", format!("%{BRANCH}")).await;
    let pr = id("pr", format!("{source}:%")).await;
    let (sha, _) = two_commits(&proxy.url).await;
    let commit = (format!("{repo}@{sha}"), sha);

    let clones = tempfile::tempdir().unwrap();
    let root: PathBuf = clones.path().join("code");
    knobas_app::checkout::set_clones_root(&pool, Some(&root.to_string_lossy()))
        .await
        .unwrap();
    let seen = Arc::new(Seen::default());
    knobas_app::checkout::clone::start_admitting(
        &pool,
        &Registry::for_process(),
        &Clones::global(),
        seen.clone(),
        &repo,
        &["http"],
    )
    .await
    .expect("the clone starts");
    let ended = loop {
        let last = seen.0.lock().unwrap().last().cloned();
        if let Some(last) = last
            && !matches!(last.state, CloneState::Progress { .. })
        {
            break last;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    };
    assert_eq!(ended.state, CloneState::Done, "{ended:?}");
    // The clone's remote is the proxy's address and the mirror's page is
    // Gitea's own, so the scan would not pair them: name the checkout.
    knobas_app::checkout::set_override(&pool, &repo, Some(&ended.path))
        .await
        .expect("the checkout is named");

    Rig {
        pool,
        secrets,
        registry: Arc::new(Registry::for_process()),
        proxy,
        repo,
        branch,
        pr,
        commit,
        _clones: clones,
    }
}

async fn two_commits(base: &str) -> (String, String) {
    let listed: serde_json::Value = reqwest::Client::new()
        .get(format!(
            "{base}/api/v1/repos/{SEEDED}/commits?sha=main&limit=2&stat=false&files=false"
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
    (shas[0].clone(), shas[1].clone())
}

/// One read's outcome: how long, and whether it answered.
struct Took(Duration, Result<(), String>);

impl std::fmt::Display for Took {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let ms = self.0.as_millis();
        match &self.1 {
            Ok(()) => write!(f, "{ms:>6} ms"),
            Err(e) => {
                let short: String = e.chars().take(40).collect();
                write!(f, "{ms:>6} ms ERR {short}")
            }
        }
    }
}

async fn time<T, E: std::fmt::Display>(
    read: impl std::future::Future<Output = Result<T, E>>,
) -> (Took, Option<T>) {
    let started = Instant::now();
    match tokio::time::timeout(CAP, read).await {
        Ok(Ok(v)) => (Took(started.elapsed(), Ok(())), Some(v)),
        Ok(Err(e)) => (Took(started.elapsed(), Err(e.to_string())), None),
        Err(_) => (
            Took(started.elapsed(), Err("still waiting".to_owned())),
            None,
        ),
    }
}

impl Rig {
    async fn view(
        &self,
        live: &LiveSources,
        entity: &str,
        section: Option<&str>,
    ) -> (Took, Option<SourceView>) {
        let (took, view) = time(knobas_app::sources::crud::view(
            &self.pool,
            &self.secrets,
            &self.registry,
            live,
            entity,
            section,
        ))
        .await;
        (took, view.flatten())
    }

    /// Every read, once cold and once warm, as the pages make them.
    async fn measure(&self, mode: &str) {
        let live = LiveSources::default();
        let line = |what: &str, cold: &Took, warm: &Took| {
            println!("SLOW {mode:>6} | {what:<28} | cold {cold} | warm {warm}");
        };

        // The repository page's header.
        let (cold, _) = self.view(&live, &self.repo, Some("branches")).await;
        let (warm, _) = self.view(&live, &self.repo, Some("branches")).await;
        line("header: branch list", &cold, &warm);
        let asks: Vec<_> = ["create_branch", "commit_file"]
            .iter()
            .map(|op| knobas_source::grants::WriteAsk {
                entity: self.repo.clone(),
                op: (*op).to_owned(),
                field: None,
            })
            .collect();
        let grants = || async {
            Ok::<_, String>(
                knobas_app::sources::crud::refused_writes(
                    &self.pool,
                    &self.secrets,
                    &self.registry,
                    &live,
                    &asks,
                )
                .await,
            )
        };
        let (cold, _) = time(grants()).await;
        let (warm, _) = time(grants()).await;
        line("header: write grants", &cold, &warm);
        let checkout = || knobas_app::checkout::view(&self.pool, &self.repo);
        let (cold, _) = time(checkout()).await;
        let (warm, _) = time(checkout()).await;
        line("header: checkout (local)", &cold, &warm);

        // Files: the top, then the README it lands on, a directory, Go to file.
        let (cold, top) = self.view(&live, &self.repo, Some("files")).await;
        let (warm, _) = self.view(&live, &self.repo, Some("files")).await;
        line("files: top directory", &cold, &warm);
        let entries = top
            .as_ref()
            .and_then(|v| v.sections.first())
            .and_then(|s| s.blocks.first())
            .and_then(|b| match b {
                Block::Tree { entries, paths, .. } => Some((entries.clone(), paths.clone())),
                _ => None,
            });
        let (readme, dir, paths) = match &entries {
            Some((entries, paths)) => (
                entries
                    .iter()
                    .find(|e| e.name == "README.md")
                    .and_then(|e| e.section.clone()),
                entries
                    .iter()
                    .find(|e| e.kind == knobas_source::view::EntryKind::Dir)
                    .and_then(|e| e.section.clone()),
                paths.clone(),
            ),
            None => (None, None, None),
        };
        let readme = readme.unwrap_or_else(|| "file:README.md".to_owned());
        let dir = dir.unwrap_or_else(|| "files:src".to_owned());
        let paths = paths.unwrap_or_else(|| "paths".to_owned());
        let (cold, _) = self.view(&live, &self.repo, Some(&readme)).await;
        let (warm, _) = self.view(&live, &self.repo, Some(&readme)).await;
        line("files: README (landing)", &cold, &warm);
        // What Files draws first since PR #2479: the checkout's copy (ADR-0041).
        let local = || {
            knobas_app::checkout::local_tree(&self.pool, self.registry.as_ref(), None, &self.repo)
        };
        let (cold, tree) = time(local()).await;
        let (warm, _) = time(local()).await;
        line("files: tree at the checkout", &cold, &warm);
        if let Some(Some(tree)) = &tree {
            let file = || {
                knobas_app::checkout::local_file(&self.pool, &self.repo, &tree.commit, "README.md")
            };
            let (cold, _) = time(file()).await;
            let (warm, _) = time(file()).await;
            line("files: README at the checkout", &cold, &warm);
        }
        let (cold, _) = self.view(&live, &self.repo, Some(&dir)).await;
        let (warm, _) = self.view(&live, &self.repo, Some(&dir)).await;
        line("files: open a directory", &cold, &warm);
        let (cold, _) = self.view(&live, &self.repo, Some(&paths)).await;
        let (warm, _) = self.view(&live, &self.repo, Some(&paths)).await;
        line("files: go to file", &cold, &warm);
        let (cold, _) = self.view(&live, &self.repo, Some("commits")).await;
        let (warm, _) = self.view(&live, &self.repo, Some("commits")).await;
        line("commits (and last commit)", &cold, &warm);

        // Branches and Compare: a branch's whole view is its standing.
        let (cold, _) = self.view(&live, &self.branch, None).await;
        let (warm, _) = self.view(&live, &self.branch, None).await;
        line("branch standing (compare)", &cold, &warm);
        let diff = || {
            knobas_app::checkout::range_diff(
                &self.pool,
                &self.secrets,
                &self.registry,
                &live,
                &self.repo,
                "main",
                BRANCH,
                false,
            )
        };
        let (cold, _) = time(diff()).await;
        let (warm, _) = time(diff()).await;
        line("compare / new PR diff", &cold, &warm);
        let check = || {
            knobas_app::checkout::source_range_diff(
                &self.pool,
                &self.secrets,
                &self.registry,
                &live,
                &self.repo,
                "main",
                BRANCH,
            )
        };
        let (cold, _) = time(check()).await;
        let (warm, _) = time(check()).await;
        line("compare: its check behind", &cold, &warm);

        // A pull request: its view, then its files.
        let (cold, whole) = self.view(&live, &self.pr, None).await;
        let (warm, _) = self.view(&live, &self.pr, None).await;
        line("pull request view", &cold, &warm);
        let files = whole
            .as_ref()
            .and_then(|v| v.sections.iter().find(|s| s.deferred))
            .map_or_else(|| "files".to_owned(), |s| s.id.clone());
        let (cold, _) = self.view(&live, &self.pr, Some(&files)).await;
        let (warm, _) = self.view(&live, &self.pr, Some(&files)).await;
        line("pull request diff", &cold, &warm);

        // A commit: its whole view and its diff (local-first since #2319).
        let (cold, _) = self.view(&live, &self.commit.0, None).await;
        let (warm, _) = self.view(&live, &self.commit.0, None).await;
        line("commit view", &cold, &warm);
        let diff = || {
            knobas_app::checkout::commit_diff(
                &self.pool,
                &self.secrets,
                &self.registry,
                &live,
                &self.repo,
                &self.commit.0,
                &self.commit.1,
                false,
            )
        };
        let (cold, _) = time(diff()).await;
        let (warm, _) = time(diff()).await;
        line("commit diff", &cold, &warm);
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs the testenv Gitea: eval \"$(testenv/seed --env)\""]
async fn every_read_a_code_page_waits_on_through_a_slow_forge() {
    let rig = rig().await;
    for (mode, ms) in [
        ("0s", 0),
        ("1s", 1_000),
        ("5s", 5_000),
        ("down", slow::DOWN),
        ("hang", slow::HANG),
    ] {
        rig.proxy.set(ms);
        rig.measure(mode).await;
    }
    rig.proxy.set(0);
}

/// **The checkout answers Files and the compare at the mirror's tips with no
/// request to the source** (ADR-0041): the tree, the README and *New pull
/// request*'s diff between two branch names, each read through the app's
/// own entry point with the request counter around it, and the source's
/// check behind the compare names the same two commits.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs the testenv Gitea: eval \"$(testenv/seed --env)\""]
async fn the_checkout_answers_files_and_the_compare_with_no_request() {
    use knobas_app::checkout::DiffReadAt;
    let rig = rig().await;
    let live = LiveSources::default();

    let (tree, sent) = knobas_http::counted::counted(knobas_app::checkout::local_tree(
        &rig.pool,
        rig.registry.as_ref(),
        None,
        &rig.repo,
    ))
    .await;
    let tree = tree
        .expect("the read runs")
        .expect("the clone holds the mirror's tip");
    assert_eq!(sent, 0, "the tree asked the source");
    assert!(
        tree.entries.iter().any(|e| e.path == "README.md"),
        "{tree:?}"
    );
    assert!(tree.as_of.is_some());
    let (main, _) = two_commits(&need("KNOBAS_GITEA_URL")).await;
    assert_eq!(tree.commit, main, "the mirror's tip of the default branch");

    let (file, sent) = knobas_http::counted::counted(knobas_app::checkout::local_file(
        &rig.pool,
        &rig.repo,
        &tree.commit,
        "README.md",
    ))
    .await;
    assert_eq!(sent, 0, "the README asked the source");
    let Some(Block::File { text, sha, .. }) = file.expect("the read runs") else {
        panic!("a file block");
    };
    assert!(!text.is_empty());
    // The blob is the one the source names for it, so an edit commits against it.
    let source = rig
        .view(&live, &rig.repo, Some("file:README.md"))
        .await
        .1
        .expect("the source reads it");
    let Some(Block::File { sha: theirs, .. }) = source.sections[0].blocks.first() else {
        panic!("a file block");
    };
    assert_eq!(&sha, theirs);

    let (predicted, sent) = knobas_http::counted::counted(knobas_app::checkout::range_diff(
        &rig.pool,
        &rig.secrets,
        &rig.registry,
        &live,
        &rig.repo,
        "main",
        BRANCH,
        false,
    ))
    .await;
    let predicted = predicted.expect("the compare reads");
    assert_eq!(sent, 0, "the predicted compare asked the source");
    assert_eq!(predicted.read_at, DiffReadAt::Checkout);
    assert!(predicted.predicted_at.is_some());
    assert!(!predicted.files.is_empty());
    let checked = knobas_app::checkout::source_range_diff(
        &rig.pool,
        &rig.secrets,
        &rig.registry,
        &live,
        &rig.repo,
        "main",
        BRANCH,
    )
    .await
    .expect("the source's compare reads");
    let (ours, theirs) = (
        predicted.range.expect("settled"),
        checked.range.expect("settled"),
    );
    assert_eq!(
        (ours.base, ours.head),
        (theirs.base, theirs.head),
        "the same two commits"
    );
    let names =
        |d: &[knobas_source::diff::DiffFile]| d.iter().map(|f| f.path.clone()).collect::<Vec<_>>();
    assert_eq!(names(&predicted.files), names(&checked.files));
}
