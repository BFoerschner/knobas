//! **The read cache against the testenv Gitea** (#2239, ADR-0032): a file
//! opened a second time at the same commit costs no request to Gitea and
//! answers in under 150 ms; the same path at another commit is read again.
//!
//! The requests are counted by the HTTP client itself
//! (`knobas_http::counted`), around the app's own read
//! (`sources::crud::view`), so what is counted is what the shell's
//! `entity_view` would cost. `tests/it/live_reads.rs` holds the cache's
//! other rules over a fake adapter.
//!
//! # Read-only at Gitea
//!
//! Every request here is a read, so nothing spends `live_gitea_capped`'s
//! budget and there is no litter to take back. What it writes is a scratch
//! database.
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

fn need(name: &str) -> String {
    std::env::var(name)
        .unwrap_or_else(|_| panic!("{name} is not set -- run `eval \"$(testenv/seed --env)\"` first"))
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
        .header("Authorization", format!("token {}", need("KNOBAS_GITEA_TOKEN")))
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
    assert!(other_sent >= 2, "another sha is read from Gitea: {other_sent}");
}
