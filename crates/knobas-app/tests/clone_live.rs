//! **Clone against the testenv Gitea** (#823, ADR-0023): clone, refuse an
//! existing folder that is not a clone, refuse a bad name, and stop.
//!
//! `tests/it/clone_ipc.rs` proves the same rules over a local repository; this
//! is the network half, a real Gitea answering git over http.
//!
//! # http, admitted on purpose
//!
//! The testenv Gitea speaks plain http and has ssh off
//! (`testenv/docker-compose.yml`), and ADR-0023 clones over https and ssh only.
//! So the clones here go through `start_admitting(.., &["http"])`, the
//! test-only door, and
//! [`the_app_itself_refuses_the_testenv_address`] shows the production entry
//! point refusing this very source. The production flags are pinned by
//! `knobas_core`'s `the_argument_list_is_the_adrs`, and git's refusal of a
//! protocol outside them by `clone_ipc.rs`'s
//! `git_refuses_any_other_protocol_given_the_production_flags`.
//!
//! # Read-only at Gitea
//!
//! A clone writes nothing to the server, so nothing here spends
//! `live_gitea_capped`'s budget and there is no litter to take back. What it
//! writes is local: a scratch database and temporary folders.
//!
//! ```text
//! just gitea-live
//! ```

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use knobas_app::IpcErrorCode;
use knobas_app::checkout::clone::{
    CloneAnswer, CloneEvent, CloneEvents, CloneState, Clones, start, start_admitting, stop,
};
use knobas_app::checkout::{FoundBy, set_clones_root, view};
use knobas_app::sources::Registry;
use sqlx::PgPool;

fn gitea_url() -> String {
    std::env::var("KNOBAS_GITEA_URL")
        .expect("KNOBAS_GITEA_URL is not set -- run `eval \"$(testenv/seed --env)\"` first")
        .trim_end_matches('/')
        .to_owned()
}

/// A repository `seed-gitea.sh` creates, public, so git needs no credential.
const SEEDED: &str = "tidewater/payout-service";

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

    async fn end(&self) -> CloneEvent {
        for _ in 0..1200 {
            if let Some(last) = self.all().last()
                && !matches!(last.state, CloneState::Progress { .. })
            {
                return last.clone();
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        panic!("the clone did not end within two minutes: {:?}", self.all());
    }
}

/// A scratch database holding a Gitea source at the testenv address and one
/// repo of it, and a clones root under a temporary folder.
async fn fixture(full_name: &str) -> (PgPool, String, tempfile::TempDir, PathBuf) {
    let pool = knobas_db::test_util::scratch_database("clone_live")
        .await
        .pool(4)
        .await
        .expect("a pool onto the scratch database");
    let base = gitea_url();
    let source = format!("gitea-{}", &uuid::Uuid::new_v4().simple().to_string()[..8]);
    sqlx::query(
        "insert into knobas.source_config (id, kind, display_name, base_url, auth_kind)
         values ($1, 'gitea', $1, $2, 'token')",
    )
    .bind(&source)
    .bind(&base)
    .execute(&pool)
    .await
    .unwrap();
    let id = format!("{source}:{full_name}");
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1, 'repo', $2)")
        .bind(&id)
        .bind(full_name)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload, web_url)
         values ($1, $2, 'repo', $3, '', '{}'::jsonb, $4)",
    )
    .bind(&id)
    .bind(&source)
    .bind(full_name)
    .bind(format!("{base}/{full_name}"))
    .execute(&pool)
    .await
    .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("code");
    set_clones_root(&pool, Some(&root.to_string_lossy()))
        .await
        .unwrap();
    (pool, id, dir, root)
}

fn shown(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

#[tokio::test]
#[ignore = "needs the testenv Gitea: just gitea-live"]
async fn a_seeded_repository_clones_and_the_scan_finds_it() {
    let (pool, id, _dir, root) = fixture(SEEDED).await;
    let seen = Arc::new(Seen::default());

    let answer = start_admitting(
        &pool,
        &Registry::builtin(),
        &Clones::global(),
        seen.clone(),
        &id,
        &["http"],
    )
    .await
    .unwrap();
    let target = root.join("payout-service");
    assert_eq!(
        answer,
        CloneAnswer::Cloning {
            path: shown(&target)
        }
    );
    let end = seen.end().await;
    println!("events: {:?}", seen.all());
    assert_eq!(end.state, CloneState::Done, "{:?}", seen.all());

    // The checkout scan finds it as it finds any clone, and git reads a branch
    // in it.
    let checkout = view(&pool, &id).await.unwrap();
    assert_eq!(checkout.found_by, FoundBy::Scan);
    assert_eq!(checkout.path.as_deref(), Some(shown(&target).as_str()));
    match checkout.tree {
        Some(knobas_app::checkout::working_tree::TreeRead::Read(state)) => {
            assert!(state.branch.is_some(), "{state:?}");
            assert_eq!(state.changed + state.untracked, 0, "{state:?}");
        }
        other => panic!("the clone's tree was not read: {other:?}"),
    }

    // And a second click adopts it rather than cloning again.
    assert_eq!(
        start_admitting(
            &pool,
            &Registry::builtin(),
            &Clones::global(),
            Arc::new(Seen::default()),
            &id,
            &["http"]
        )
        .await
        .unwrap(),
        CloneAnswer::Adopted {
            path: shown(&target)
        }
    );
}

#[tokio::test]
#[ignore = "needs the testenv Gitea: just gitea-live"]
async fn the_app_itself_refuses_the_testenv_address() {
    let (pool, id, _dir, root) = fixture(SEEDED).await;

    let error = start(
        &pool,
        &Registry::builtin(),
        &Clones::global(),
        Arc::new(Seen::default()),
        &id,
    )
    .await
    .expect_err("the testenv Gitea is http");

    assert_eq!(error.code, IpcErrorCode::Invalid);
    assert!(error.message.contains("https or ssh"), "{}", error.message);
    assert!(!root.exists());
}

#[tokio::test]
#[ignore = "needs the testenv Gitea: just gitea-live"]
async fn an_existing_folder_that_is_not_a_clone_is_refused() {
    let (pool, id, _dir, root) = fixture(SEEDED).await;
    let target = root.join("payout-service");
    std::fs::create_dir_all(&target).unwrap();
    std::fs::write(target.join("mine.txt"), "mine\n").unwrap();

    let error = start_admitting(
        &pool,
        &Registry::builtin(),
        &Clones::global(),
        Arc::new(Seen::default()),
        &id,
        &["http"],
    )
    .await
    .expect_err("a folder that is not a clone");

    assert_eq!(error.code, IpcErrorCode::Conflict);
    assert!(
        error
            .message
            .contains("exists and is not a clone of this repository"),
        "{}",
        error.message
    );
    let left: Vec<String> = std::fs::read_dir(&target)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(left, ["mine.txt"], "the folder was touched");
}

#[tokio::test]
#[ignore = "needs the testenv Gitea: just gitea-live"]
async fn a_bad_name_is_refused() {
    let (pool, id, _dir, root) = fixture("tidewater/..").await;

    let error = start_admitting(
        &pool,
        &Registry::builtin(),
        &Clones::global(),
        Arc::new(Seen::default()),
        &id,
        &["http"],
    )
    .await
    .expect_err("`..` is not a repository name");

    assert_eq!(error.code, IpcErrorCode::Invalid);
    assert!(
        error.message.contains("is not a repository name"),
        "{}",
        error.message
    );
    assert!(!root.exists());
}

#[tokio::test]
#[ignore = "needs the testenv Gitea: just gitea-live"]
async fn stopping_a_clone_leaves_nothing_behind() {
    let (pool, id, _dir, root) = fixture(SEEDED).await;
    let seen = Arc::new(Seen::default());
    let clones = Clones::global();

    start_admitting(
        &pool,
        &Registry::builtin(),
        &clones,
        seen.clone(),
        &id,
        &["http"],
    )
    .await
    .unwrap();
    // Stopped once git is talking to the server, not before it has started.
    for _ in 0..200 {
        if !seen.all().is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    println!("stopping after: {:?}", seen.all());
    assert!(
        matches!(
            seen.all().first().map(|e| &e.state),
            Some(CloneState::Progress { .. })
        ),
        "git reported no progress before the stop: {:?}",
        seen.all()
    );
    let answer = stop(&pool, &clones, &id).await.unwrap();

    let target = root.join("payout-service");
    assert_eq!(
        answer,
        CloneAnswer::Stopped {
            path: shown(&target),
            removed: true
        },
        "{:?}",
        seen.all()
    );
    assert_eq!(
        seen.end().await.state,
        CloneState::Stopped { removed: true }
    );
    assert!(!target.exists());
    assert_eq!(
        std::fs::read_dir(&root).unwrap().count(),
        0,
        "nothing was left in the clones root"
    );
}
