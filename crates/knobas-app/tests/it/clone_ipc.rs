//! *Clone* over a scratch database and a real `git` (#823, ADR-0023).
//!
//! The network half is `tests/clone_live.rs` against the testenv Gitea. What is
//! here needs no Docker: the refusals, the adoption, a clone from a local
//! repository (admitted as `file` through `start_admitting`, the test-only
//! door), a failure, and a stop against a server that never answers. And one
//! test that runs git with the production flags to show git itself refuses
//! what the protocol rule keeps out.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use knobas_app::IpcErrorCode;
use knobas_app::checkout::clone::{
    CloneAnswer, CloneEvent, CloneEvents, CloneState, Clones, start, start_admitting, stop,
};
use knobas_app::checkout::set_clones_root;
use knobas_app::sources::Registry;
use knobas_core::checkout::clone::{PROTOCOLS, clone_args};
use sqlx::PgPool;

async fn pool() -> PgPool {
    knobas_db::test_util::scratch_database("clone-ipc")
        .await
        .pool(4)
        .await
        .expect("a pool onto the scratch db")
}

fn unique() -> String {
    uuid::Uuid::new_v4().simple().to_string()[..12].to_owned()
}

/// The events one test's clones sent, in order.
#[derive(Default)]
struct Seen(Mutex<Vec<CloneEvent>>);

impl CloneEvents for Seen {
    fn clone_progress(&self, event: CloneEvent) {
        self.0.lock().unwrap().push(event);
    }
}

impl Seen {
    fn all(&self) -> Vec<CloneEvent> {
        self.0.lock().unwrap().clone()
    }

    /// Wait for the clone's last event: done, failed or stopped.
    async fn end(&self) -> CloneEvent {
        for _ in 0..600 {
            if let Some(last) = self.all().last()
                && !matches!(last.state, CloneState::Progress { .. })
            {
                return last.clone();
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        panic!("the clone did not end within a minute: {:?}", self.all());
    }
}

/// A source of `kind` at `base_url`, and one repo of it named `full_name`.
async fn repo(pool: &PgPool, kind: &str, base_url: &str, full_name: &str) -> String {
    repo_with(pool, kind, base_url, &serde_json::json!({}), full_name).await
}

/// [`repo`], its source configured with `config`.
async fn repo_with(
    pool: &PgPool,
    kind: &str,
    base_url: &str,
    config: &serde_json::Value,
    full_name: &str,
) -> String {
    let source = format!("{kind}-{}", unique());
    sqlx::query(
        "insert into knobas.source_config (id, kind, display_name, base_url, auth_kind, config)
         values ($1, $2, $1, $3, 'token', $4)",
    )
    .bind(&source)
    .bind(kind)
    .bind(base_url)
    .bind(config)
    .execute(pool)
    .await
    .unwrap();
    let id = format!("{source}:{full_name}");
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1, 'repo', $2)")
        .bind(&id)
        .bind(full_name)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload, web_url)
         values ($1, $2, 'repo', $3, '', '{}'::jsonb, $4)",
    )
    .bind(&id)
    .bind(&source)
    .bind(full_name)
    .bind(format!("{base_url}/{full_name}"))
    .execute(pool)
    .await
    .unwrap();
    id
}

fn git(dir: &Path, args: &[&str]) {
    let status = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("LC_ALL", "C")
        .output()
        .unwrap();
    assert!(
        status.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&status.stderr)
    );
}

/// A server folder holding `owner/repo.git` with two commits, and its
/// `file://` address.
fn upstream(owner: &str, repo: &str) -> (tempfile::TempDir, String) {
    let server = tempfile::tempdir().unwrap();
    let dir = server.path().join(owner).join(format!("{repo}.git"));
    std::fs::create_dir_all(&dir).unwrap();
    git(&dir, &["init", "-q", "-b", "main"]);
    for n in 0..2 {
        std::fs::write(dir.join(format!("f{n}")), format!("{n}\n")).unwrap();
        git(&dir, &["add", "."]);
        git(
            &dir,
            &[
                "-c",
                "user.name=k",
                "-c",
                "user.email=k@example.com",
                "commit",
                "-qm",
                &format!("{n}"),
            ],
        );
    }
    // `localhost`, because a clone is matched by host and `file:///` has none.
    let base = format!("file://localhost{}", server.path().display());
    (server, base)
}

/// A clones root, set, under a temporary directory.
async fn root(pool: &PgPool) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("code");
    set_clones_root(pool, Some(&root.to_string_lossy()))
        .await
        .unwrap();
    (dir, root)
}

fn origin_of(dir: &Path) -> String {
    knobas_core::checkout::clone_at(dir)
        .expect("a clone, with an origin")
        .remote
}

#[tokio::test]
async fn a_clone_lands_in_the_root_under_the_repository_name() {
    let pool = pool().await;
    let (_server, base) = upstream("tidewater", "payout-service");
    let id = repo(&pool, "gitea", &base, "tidewater/payout-service").await;
    let (_dir, root) = root(&pool).await;
    let seen = Arc::new(Seen::default());

    let answer = start_admitting(
        &pool,
        &Registry::for_process(),
        &Clones::global(),
        seen.clone(),
        &id,
        &["file"],
    )
    .await
    .unwrap();
    let target = root.join("payout-service");
    assert_eq!(
        answer,
        CloneAnswer::Cloning {
            path: target.to_string_lossy().into_owned()
        }
    );

    let end = seen.end().await;
    assert_eq!(end.state, CloneState::Done, "{:?}", seen.all());
    assert_eq!(end.entity_id, id);
    assert_eq!(
        origin_of(&target),
        format!("{base}/tidewater/payout-service.git"),
        "the URL is the source's address and the full name"
    );
    assert!(target.join("f1").is_file(), "the files were checked out");
    // Progress came before the end, and every share is a share.
    let progress: Vec<u8> = seen
        .all()
        .iter()
        .filter_map(|event| match &event.state {
            CloneState::Progress { percent, .. } => Some(*percent),
            _ => None,
        })
        .collect();
    assert!(!progress.is_empty(), "no progress: {:?}", seen.all());
    assert!(progress.iter().all(|p| *p <= 100));
}

#[tokio::test]
async fn an_existing_clone_of_the_repository_is_adopted() {
    let pool = pool().await;
    let (_server, base) = upstream("tidewater", "ledger");
    let id = repo(&pool, "gitea", &base, "tidewater/ledger").await;
    let (_dir, root) = root(&pool).await;
    let target = root.join("ledger");
    std::fs::create_dir_all(&root).unwrap();
    // Cloned by hand from the same repository, spelled with a trailing slash.
    git(
        &root,
        &[
            "-c",
            "protocol.file.allow=always",
            "clone",
            "-q",
            &format!("{base}/tidewater/ledger.git"),
            "ledger",
        ],
    );
    let seen = Arc::new(Seen::default());

    let answer = start_admitting(
        &pool,
        &Registry::for_process(),
        &Clones::global(),
        seen.clone(),
        &id,
        &["file"],
    )
    .await
    .unwrap();

    assert_eq!(
        answer,
        CloneAnswer::Adopted {
            path: target.to_string_lossy().into_owned()
        }
    );
    // Adopted, so no run holds the repository: a second start adopts it
    // again rather than answering that a clone is running (#1927). The clone
    // runner is spawned only on the path that answers `Cloning` and is what
    // sends every event, so with no run nothing can send one later. Before, a
    // 200 ms look at the events stood for this.
    assert_eq!(
        start_admitting(
            &pool,
            &Registry::for_process(),
            &Clones::global(),
            seen.clone(),
            &id,
            &["file"],
        )
        .await
        .unwrap(),
        answer,
        "a run holds the adopted repository"
    );
    assert!(seen.all().is_empty(), "nothing ran: {:?}", seen.all());
}

/// A clone killed half-way (knobas quit while it ran) already has its
/// `origin`, so it matches by remote; it is refused, not adopted.
#[tokio::test]
async fn a_clone_that_did_not_finish_is_not_adopted() {
    let pool = pool().await;
    let (_server, base) = upstream("tidewater", "ledger");
    let id = repo(&pool, "gitea", &base, "tidewater/ledger").await;
    let (_dir, root) = root(&pool).await;
    let target = root.join("ledger");
    std::fs::create_dir_all(&target).unwrap();
    git(&target, &["init", "-q", "-b", "main"]);
    git(
        &target,
        &[
            "remote",
            "add",
            "origin",
            &format!("{base}/tidewater/ledger.git"),
        ],
    );

    let error = start_admitting(
        &pool,
        &Registry::for_process(),
        &Clones::global(),
        Arc::new(Seen::default()),
        &id,
        &["file"],
    )
    .await
    .expect_err("a half-made clone");

    assert_eq!(error.code, IpcErrorCode::Conflict);
    assert!(
        error.message.contains("did not finish"),
        "{}",
        error.message
    );
    assert!(target.join(".git").is_dir(), "the folder was left alone");
}

#[tokio::test]
async fn a_folder_that_is_not_a_clone_of_it_is_refused_and_left_alone() {
    let pool = pool().await;
    let (_server, base) = upstream("tidewater", "booking-web");
    let id = repo(&pool, "gitea", &base, "tidewater/booking-web").await;
    let (_dir, root) = root(&pool).await;

    // Two folders: one with a file in it, and one empty, which git itself
    // would clone into.
    for content in [Some("mine\n"), None] {
        let target = root.join("booking-web");
        let _ = std::fs::remove_dir_all(&target);
        std::fs::create_dir_all(&target).unwrap();
        if let Some(content) = content {
            std::fs::write(target.join("notes.txt"), content).unwrap();
        }

        let error = start_admitting(
            &pool,
            &Registry::for_process(),
            &Clones::global(),
            Arc::new(Seen::default()),
            &id,
            &["file"],
        )
        .await
        .expect_err("an existing folder is never written into");

        assert_eq!(error.code, IpcErrorCode::Conflict);
        assert!(
            error
                .message
                .contains("exists and is not a clone of this repository"),
            "{}",
            error.message
        );
        let left: Vec<_> = std::fs::read_dir(&target).unwrap().collect();
        assert_eq!(
            left.len(),
            usize::from(content.is_some()),
            "the folder was touched"
        );
    }
}

#[tokio::test]
async fn a_name_outside_the_pattern_is_refused_before_anything_is_made() {
    let pool = pool().await;
    let (_server, base) = upstream("tidewater", "x");
    let (_dir, root) = root(&pool).await;
    for name in ["tidewater/..", "tidewater/x;touch y", "tidewater/a b"] {
        let id = repo(&pool, "gitea", &base, name).await;

        let error = start_admitting(
            &pool,
            &Registry::for_process(),
            &Clones::global(),
            Arc::new(Seen::default()),
            &id,
            &["file"],
        )
        .await
        .expect_err("a bad name");

        assert_eq!(error.code, IpcErrorCode::Invalid, "{name}");
        assert!(
            error.message.contains("is not a repository name"),
            "{}",
            error.message
        );
    }
    assert!(!root.exists(), "nothing was made");
}

/// The production entry point refuses an address that is not https or ssh,
/// before git and before any folder.
#[tokio::test]
async fn the_app_clones_over_https_and_ssh_only() {
    let pool = pool().await;
    let (_server, base) = upstream("tidewater", "payout-service");
    let id = repo(&pool, "gitea", &base, "tidewater/payout-service").await;
    let (_dir, root) = root(&pool).await;

    let error = start(
        &pool,
        &Registry::for_process(),
        &Clones::global(),
        Arc::new(Seen::default()),
        &id,
    )
    .await
    .expect_err("a file:// source");

    assert_eq!(error.code, IpcErrorCode::Invalid);
    assert!(error.message.contains("https or ssh"), "{}", error.message);
    assert!(!root.exists());
}

/// And git refuses it too, given the production flags: the scheme check is
/// not the only thing between a `file://` (or an `ext::`) and a clone.
#[test]
fn git_refuses_any_other_protocol_given_the_production_flags() {
    let (server, base) = upstream("tidewater", "payout-service");
    let into = server.path().join("into");
    let output = std::process::Command::new("git")
        .args(clone_args(
            &format!("{base}/tidewater/payout-service.git"),
            &into,
            &PROTOCOLS,
        ))
        .env("LC_ALL", "C")
        .env_remove("GIT_ALLOW_PROTOCOL")
        .env_remove("GIT_CONFIG_PARAMETERS")
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success());
    assert!(stderr.contains("transport 'file' not allowed"), "{stderr}");
    assert!(!into.exists());
}

/// The built-in adapters and one more code host, `forge`, that is no Gitea:
/// its repositories are cloned from the address its config holds under
/// `clone_base`, not from its base URL (#1197). It builds nothing; a clone
/// reads only the template.
struct WithForge;

impl knobas_sync::scheduler::AdapterRegistry for WithForge {
    fn descriptors(&self) -> Vec<knobas_source::SourceDescriptor> {
        let mut all = Registry::for_process().descriptors();
        let mut forge = all
            .iter()
            .find(|d| d.adapter_kind == "gitea")
            .expect("gitea is built in")
            .clone();
        forge.id = "forge".into();
        forge.adapter_kind = "forge".into();
        forge.clone_from = vec![knobas_source::clone_urls::CloneFrom::Config(
            "clone_base".into(),
        )];
        all.push(forge);
        all
    }

    fn build(
        &self,
        _: knobas_source::instance::SourceInstance,
    ) -> Result<Box<dyn knobas_source::Source>, knobas_source::SourceError> {
        unreachable!("a clone reads the template and builds no adapter")
    }
}

/// Criterion 2 of #1197: no adapter-kind check. A code host that is not
/// Gitea clones, from the address its adapter answers `clone_urls` with.
#[tokio::test]
async fn a_repository_of_any_code_host_source_clones() {
    let pool = pool().await;
    let (_server, base) = upstream("acme", "anvil");
    let id = repo_with(
        &pool,
        "forge",
        "https://api.forge.invalid",
        &serde_json::json!({ "clone_base": base }),
        "acme/anvil",
    )
    .await;
    let (_dir, root) = root(&pool).await;
    let seen = Arc::new(Seen::default());

    start_admitting(
        &pool,
        &WithForge,
        &Clones::global(),
        seen.clone(),
        &id,
        &["file"],
    )
    .await
    .unwrap();

    let end = seen.end().await;
    assert_eq!(end.state, CloneState::Done, "{:?}", seen.all());
    assert_eq!(
        origin_of(&root.join("anvil")),
        format!("{base}/acme/anvil.git"),
        "the address the adapter named, and the full name"
    );
}

#[tokio::test]
async fn a_repository_clones_only_with_a_root_set_and_from_an_address_its_adapter_names() {
    let pool = pool().await;
    let (_server, base) = upstream("tidewater", "x");

    let gitea = repo(&pool, "gitea", &base, "tidewater/x").await;
    let error = start_admitting(
        &pool,
        &Registry::for_process(),
        &Clones::global(),
        Arc::new(Seen::default()),
        &gitea,
        &["file"],
    )
    .await
    .expect_err("no clones root");
    assert_eq!(error.code, IpcErrorCode::Invalid);
    assert!(
        error.message.contains("Settings › Checkouts"),
        "{}",
        error.message
    );

    let (_dir, _root) = root(&pool).await;
    // Jira claims no code host, so it names no address to clone from. (The
    // mock claims one since #1229: its base URL is the address, and the
    // demo's is empty.)
    let mock = repo(&pool, "jira", &base, "tidewater/x").await;
    let error = start_admitting(
        &pool,
        &Registry::for_process(),
        &Clones::global(),
        Arc::new(Seen::default()),
        &mock,
        &["file"],
    )
    .await
    .expect_err("no address to clone from");
    assert_eq!(error.code, IpcErrorCode::Invalid);
    assert!(
        error.message.contains("names no address to clone from"),
        "{}",
        error.message
    );

    // A forge whose config holds no `clone_base` names none either; its base
    // URL is not a fallback.
    let bare = repo(&pool, "forge", &base, "tidewater/x").await;
    let error = start_admitting(
        &pool,
        &WithForge,
        &Clones::global(),
        Arc::new(Seen::default()),
        &bare,
        &["file"],
    )
    .await
    .expect_err("no address to clone from");
    assert_eq!(error.code, IpcErrorCode::Invalid);
    assert!(
        error.message.contains("with no clone_base set") && error.message.contains("Edit source"),
        "{}",
        error.message
    );

    let gone = repo(&pool, "nowhere", &base, "tidewater/x").await;
    let error = start_admitting(
        &pool,
        &Registry::for_process(),
        &Clones::global(),
        Arc::new(Seen::default()),
        &gone,
        &["file"],
    )
    .await
    .expect_err("no adapter of that kind");
    assert_eq!(error.code, IpcErrorCode::Invalid);
    assert!(
        error.message.contains("no nowhere adapter is installed"),
        "{}",
        error.message
    );
}

#[tokio::test]
async fn a_failed_clone_says_gits_last_line_and_leaves_nothing() {
    let pool = pool().await;
    let (_server, base) = upstream("tidewater", "x");
    // A repository the server does not have.
    let id = repo(&pool, "gitea", &base, "tidewater/missing").await;
    let (_dir, root) = root(&pool).await;
    let seen = Arc::new(Seen::default());

    start_admitting(
        &pool,
        &Registry::for_process(),
        &Clones::global(),
        seen.clone(),
        &id,
        &["file"],
    )
    .await
    .unwrap();
    let end = seen.end().await;

    let CloneState::Failed {
        reason,
        cause,
        removed,
    } = end.state
    else {
        panic!("not a failure: {:?}", seen.all());
    };
    assert!(reason.starts_with("fatal:"), "{reason}");
    assert_eq!(cause, knobas_app::checkout::clone::FailureCause::Other);
    assert!(removed);
    assert!(!root.join("missing").exists());
    // And the repository can be cloned again: nothing is left registered.
    assert_eq!(
        stop(&pool, &Clones::global(), &id).await.unwrap(),
        CloneAnswer::NotCloning
    );
}

/// A server that accepts the connection and never answers, so the clone runs
/// until it is stopped.
async fn silent_server() -> (String, tokio::task::JoinHandle<()>) {
    let (base, task, _reached) = silent_server_counting().await;
    (base, task)
}

/// [`silent_server`], and how many connections it has accepted: git has
/// reached it, so the clone is under way at the source and not still
/// starting.
async fn silent_server_counting() -> (String, tokio::task::JoinHandle<()>, Arc<AtomicUsize>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let reached = Arc::new(AtomicUsize::new(0));
    let task = tokio::spawn({
        let reached = Arc::clone(&reached);
        async move {
            let mut held = Vec::new();
            while let Ok((socket, _)) = listener.accept().await {
                held.push(socket);
                reached.fetch_add(1, Ordering::SeqCst);
            }
        }
    });
    (format!("http://{address}"), task, reached)
}

/// Wait for git to reach `server`: the clone is under way (#1927), where a
/// fixed margin stood for that and a slow start of git was stopped before it
/// began.
async fn await_git_at(reached: &AtomicUsize) {
    tokio::time::timeout(Duration::from_secs(30), async {
        while reached.load(Ordering::SeqCst) == 0 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("git never reached the server");
}

#[tokio::test]
async fn stopping_kills_git_and_removes_the_half_made_folder() {
    let pool = pool().await;
    let (base, server, reached) = silent_server_counting().await;
    let id = repo(&pool, "gitea", &base, "tidewater/slow").await;
    let (_dir, root) = root(&pool).await;
    let seen = Arc::new(Seen::default());
    let clones = Clones::global();

    start_admitting(
        &pool,
        &Registry::for_process(),
        &clones,
        seen.clone(),
        &id,
        &["http"],
    )
    .await
    .unwrap();
    let target = root.join("slow");
    assert!(target.is_dir(), "the folder is made before git starts");
    // A second start while it runs starts nothing new.
    assert_eq!(
        start_admitting(
            &pool,
            &Registry::for_process(),
            &clones,
            seen.clone(),
            &id,
            &["http"]
        )
        .await
        .unwrap(),
        CloneAnswer::Cloning {
            path: target.to_string_lossy().into_owned()
        }
    );
    await_git_at(&reached).await;

    let answer = stop(&pool, &clones, &id).await.unwrap();

    assert_eq!(
        answer,
        CloneAnswer::Stopped {
            path: target.to_string_lossy().into_owned(),
            removed: true
        }
    );
    assert!(!target.exists(), "the half-made folder is gone");
    assert_eq!(
        std::fs::read_dir(&root).unwrap().count(),
        0,
        "nothing was left in the root"
    );
    assert_eq!(
        seen.end().await.state,
        CloneState::Stopped { removed: true }
    );
    assert_eq!(
        stop(&pool, &clones, &id).await.unwrap(),
        CloneAnswer::NotCloning
    );
    server.abort();
}

/// IPC-10 (#1633, spec #1619): Cmd-Q during a clone stops git and removes the
/// half-made folder, as *Stop* does. Before, the quit dropped the runner with
/// the runtime: git was killed, the folder stayed, and the next *Clone* of
/// the repository refused it as a clone that did not finish.
#[tokio::test(flavor = "multi_thread")]
async fn a_quit_during_a_clone_removes_its_folder_and_the_next_clone_starts() {
    use tauri::Manager as _;

    let pool = pool().await;
    let (base, server, reached) = silent_server_counting().await;
    let id = repo(&pool, "gitea", &base, "tidewater/quit").await;
    let (_dir, root) = root(&pool).await;
    let seen = Arc::new(Seen::default());
    let clones = Arc::new(Clones::default());
    let app = tauri::test::mock_builder()
        .build(tauri::test::mock_context(tauri::test::noop_assets()))
        .expect("a mock app");
    app.manage(Arc::clone(&clones));

    start_admitting(
        &pool,
        &Registry::for_process(),
        &clones,
        seen.clone(),
        &id,
        &["http"],
    )
    .await
    .unwrap();
    let target = root.join("quit");
    assert!(target.is_dir(), "the folder is made before git starts");
    await_git_at(&reached).await;

    tokio::task::block_in_place(|| knobas_app::shutdown(app.handle()));
    let left = target.exists();

    // The next launch: a registry of its own, as a new process has.
    let next = Arc::new(Clones::default());
    let again = start_admitting(
        &pool,
        &Registry::for_process(),
        &next,
        Arc::new(Seen::default()),
        &id,
        &["http"],
    )
    .await;
    let _ = stop(&pool, &next, &id).await;
    let _ = stop(&pool, &clones, &id).await;
    server.abort();

    assert!(!left, "the quit left the half-made folder");
    assert_eq!(
        seen.end().await.state,
        CloneState::Stopped { removed: true }
    );
    assert_eq!(
        again.map_err(|error| error.code),
        Ok(CloneAnswer::Cloning {
            path: target.to_string_lossy().into_owned()
        }),
        "the next Clone of the repository after the quit"
    );
}

/// Two starts at once: one clone, and both answer `cloning`. Neither finds the
/// other's new folder and refuses it as not a clone.
#[tokio::test]
async fn two_starts_at_once_start_one_clone() {
    let pool = pool().await;
    let (base, server) = silent_server().await;
    let id = repo(&pool, "gitea", &base, "tidewater/twice").await;
    let (_dir, root) = root(&pool).await;
    let seen = Arc::new(Seen::default());
    let clones = Clones::global();
    let registry = Registry::for_process();

    let (a, b) = tokio::join!(
        start_admitting(&pool, &registry, &clones, seen.clone(), &id, &["http"]),
        start_admitting(&pool, &registry, &clones, seen.clone(), &id, &["http"]),
    );

    let cloning = CloneAnswer::Cloning {
        path: root.join("twice").to_string_lossy().into_owned(),
    };
    assert_eq!(a.unwrap(), cloning);
    assert_eq!(b.unwrap(), cloning);
    assert!(matches!(
        stop(&pool, &clones, &id).await.unwrap(),
        CloneAnswer::Stopped { removed: true, .. }
    ));
    server.abort();
}

/// `a/x` and `b/x` both clone into `<root>/x`. While one run holds the folder
/// the other is refused by name, not by what it finds on disk: a run's entry
/// lasts until its folder is removed, so a removal still retrying can never
/// remove a folder the other repository's start made.
#[tokio::test]
async fn a_folder_another_repository_is_cloning_into_is_refused() {
    let pool = pool().await;
    let (base, server) = silent_server().await;
    let first = repo(&pool, "gitea", &base, "tidewater/shared").await;
    let second = repo(&pool, "gitea", &base, "harbour/shared").await;
    let (_dir, root) = root(&pool).await;
    let seen = Arc::new(Seen::default());
    let clones = Clones::global();

    start_admitting(
        &pool,
        &Registry::for_process(),
        &clones,
        seen.clone(),
        &first,
        &["http"],
    )
    .await
    .unwrap();
    let error = start_admitting(
        &pool,
        &Registry::for_process(),
        &clones,
        seen.clone(),
        &second,
        &["http"],
    )
    .await
    .expect_err("the folder is the first run's");

    assert_eq!(error.code, IpcErrorCode::Conflict);
    assert!(
        error
            .message
            .contains("is being cloned from another repository of the same name"),
        "{}",
        error.message
    );
    assert!(matches!(
        stop(&pool, &clones, &first).await.unwrap(),
        CloneAnswer::Stopped { removed: true, .. }
    ));
    assert!(!root.join("shared").exists());
    server.abort();
}
