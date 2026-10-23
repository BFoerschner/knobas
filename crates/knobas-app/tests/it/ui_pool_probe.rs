//! The UI pool under a syncing estate (#745): five slow Jira-shaped sources
//! syncing through the real [`Scheduler`] while a "frontend" loop drives the
//! UI's own command bodies against a five-connection pool, as the app's is,
//! and `pg_stat_activity` is sampled every second.
//!
//! The reproduction of Björn's Windows hang, 2026-10-13: "it became
//! unresponsive, searches didn't find anything and it said there were no
//! sources available anymore". Before #745 every finished run made the tray
//! start a detection pass on the UI pool with nothing stopping the next one,
//! and `similar_text` compared every item with every other: the pool had no
//! free connection for minutes and `list_sources` failed with a pool timeout.
//!
//! **Ignored**, and run by `just ui-pool-probe`: three minutes of wall clock
//! and a machine-dependent load are not a gate test. The assertions are the
//! ticket's: the UI pool always has a free connection, no UI command fails,
//! none takes over a second. Sizes come from the environment (`HP_SOURCES`,
//! `HP_ITEMS`, `HP_SECS`, `HP_PAGE_DELAY_MS`, `HP_TRAY`).

use knobas_app::sources::Registry;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use knobas_app::commands::entity::{create_note_inner, get_entity_inner, room_suggestions_inner};
use knobas_app::commands::search::{launcher_home_inner, search_inner};
use knobas_app::suggestion_pass::{PASS_STATEMENT_TIMEOUT, SuggestionPass};
use knobas_secrets::{MemoryStore, Secret, SecretStore};
use knobas_source::instance::SourceInstance;
use knobas_source::{AuthMethod, Cursor, Sink, Source, SourceDescriptor, SourceError, SyncItem};
use knobas_sync::config::{self, AuthKind, InsertConfig};
use knobas_sync::run_log::SourceSyncStatus;
use knobas_sync::scheduler::{
    AdapterRegistry, RunConnections, Scheduler, SchedulerDeps, SchedulerTiming, SyncEvents,
};
use sqlx::Row;

fn env(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

const WORDS: &[&str] = &[
    "payment",
    "retry",
    "backoff",
    "invoice",
    "customer",
    "ledger",
    "export",
    "report",
    "batch",
    "worker",
    "queue",
    "timeout",
    "deploy",
    "release",
    "staging",
    "database",
    "migration",
    "index",
    "search",
    "cache",
    "session",
    "login",
    "token",
    "refresh",
    "upload",
    "storage",
    "bucket",
    "image",
    "resize",
    "thumbnail",
    "notification",
    "email",
    "template",
    "render",
    "layout",
    "mobile",
    "android",
    "ios",
    "crash",
    "memory",
    "leak",
    "latency",
    "dashboard",
    "metric",
    "alert",
    "monitor",
    "webhook",
    "partner",
    "integration",
    "sync",
    "mirror",
    "freight",
    "shipment",
    "carrier",
    "route",
    "warehouse",
    "pallet",
    "label",
    "barcode",
    "scanner",
    "driver",
    "tracking",
    "status",
    "delay",
    "customs",
    "tariff",
    "currency",
    "rate",
    "quote",
    "booking",
    "container",
    "vessel",
    "port",
    "terminal",
    "berth",
    "manifest",
    "audit",
    "compliance",
    "permission",
    "role",
    "admin",
    "settings",
    "profile",
    "avatar",
    "feature",
    "flag",
    "rollout",
    "experiment",
    "billing",
    "subscription",
    "plan",
    "trial",
    "discount",
    "coupon",
    "checkout",
    "cart",
    "order",
    "refund",
    "chargeback",
    "fraud",
    "risk",
    "score",
    "model",
    "training",
];

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn word(&mut self) -> String {
        let vocab = env("HP_VOCAB", 0);
        if vocab == 0 {
            return WORDS[(self.next() % WORDS.len() as u64) as usize].to_owned();
        }
        // Log-uniform (Zipf-like) rank over `vocab` synthetic words.
        let u = (self.next() % 1_000_000) as f64 / 1_000_000.0;
        let skip = env("HP_VOCAB_SKIP", 0) as f64;
        let lo = (skip + 1.0).ln();
        let mut k = ((lo + u * ((vocab as f64).ln() - lo)).exp() as u64).saturating_sub(1);
        const SYL: &[&str] = &[
            "ka", "lo", "mi", "re", "tu", "sa", "ne", "po", "di", "va", "go", "be", "ri", "zu",
            "fe", "ha", "jo", "ki", "mu", "te",
        ];
        let mut w = String::from("q");
        loop {
            w.push_str(SYL[(k % 20) as usize]);
            k /= 20;
            if k == 0 {
                break;
            }
        }
        w
    }
    fn words(&mut self, n: usize) -> String {
        (0..n).map(|_| self.word()).collect::<Vec<_>>().join(" ")
    }
}

struct Slow {
    id: String,
    items: u64,
    page: u64,
    page_delay: Duration,
    runs: Arc<AtomicU64>,
}

#[async_trait::async_trait]
impl Source for Slow {
    async fn write(
        &self,
        _op: knobas_source::WriteOp,
    ) -> Result<knobas_source::WriteReceipt, SourceError> {
        Err(SourceError::protocol("no writes".to_owned()))
    }
    async fn reachable_transitions(&self, _e: &str) -> Result<Vec<String>, SourceError> {
        Ok(Vec::new())
    }
    async fn view(
        &self,
        _entity: &str,
        _section: Option<&str>,
    ) -> Result<Option<knobas_source::view::SourceView>, SourceError> {
        Ok(None)
    }
    fn descriptor(&self) -> SourceDescriptor {
        SourceDescriptor {
            id: self.id.clone(),
            ..knobas_source_jira::descriptor_template()
        }
    }
    async fn test_connection(&self) -> Result<knobas_source::ConnectionInfo, SourceError> {
        Ok(knobas_source::ConnectionInfo::default())
    }
    async fn sync(
        &self,
        _cursor: Option<Cursor>,
        sink: &mut (dyn Sink + Send),
    ) -> Result<Cursor, SourceError> {
        let run = self.runs.fetch_add(1, Ordering::Relaxed);
        let mut rng = Rng(0x9E37_79B9_7F4A_7C15 ^ run.wrapping_mul(7919) ^ self.id.len() as u64);
        let prefix = self.id.to_uppercase().replace('-', "");
        for i in 0..self.items {
            if i % self.page == 0 {
                tokio::time::sleep(self.page_delay).await;
            }
            let key = format!("{prefix}-{i}");
            let parent = format!("{prefix}-{}", i / 20);
            let other = format!("{prefix}-{}", rng.next() % self.items);
            sink.item(SyncItem {
                entity: knobas_core::entity::EntityRef::new(&self.id, &key),
                kind: "ticket".into(),
                title: format!("{} {}", rng.words(6), run),
                body_text: format!("{} see {other} {}", rng.words(30), rng.words(20)),
                author: Some("mara".into()),
                updated_at: Some(chrono::Utc::now()),
                payload: serde_json::json!({
                    "key": key,
                    "fields": {
                        "summary": "x",
                        "status": {"name": "Open"},
                        "priority": {"name": "High"},
                        "assignee": {"name": "mara", "key": "mara"},
                        "project": {"key": prefix, "name": prefix},
                        "parent": {"key": parent},
                        "issuelinks": [{"type": {"outward": "blocks", "inward": "is blocked by"},
                                        "outwardIssue": {"key": other}}]
                    }
                }),
                web_url: None,
                deleted: false,
            })
            .await?;
        }
        Ok(format!("run-{run}"))
    }
}

struct Reg {
    items: u64,
    page: u64,
    page_delay: Duration,
    runs: Arc<AtomicU64>,
}
impl AdapterRegistry for Reg {
    fn descriptors(&self) -> Vec<SourceDescriptor> {
        knobas_app::sources::Registry::for_process().templates()
    }
    fn build(&self, instance: SourceInstance) -> Result<Box<dyn Source>, SourceError> {
        Ok(Box::new(Slow {
            id: instance.id,
            items: self.items,
            page: self.page,
            page_delay: self.page_delay,
            runs: Arc::clone(&self.runs),
        }))
    }
}

struct Conns(knobas_db::embedded::Connector);
#[async_trait::async_trait]
impl RunConnections for Conns {
    async fn open(&self) -> Result<sqlx::PgConnection, sqlx::Error> {
        self.0.connect().await
    }
}

struct Events {
    t0: Instant,
    tx: tokio::sync::mpsc::UnboundedSender<SourceSyncStatus>,
}
impl SyncEvents for Events {
    fn sync_state(&self, status: SourceSyncStatus) {
        println!(
            "[{:>6.1}] sync:state {} running={}",
            self.t0.elapsed().as_secs_f32(),
            status.source_id,
            status.running
        );
        let _ = self.tx.send(status);
    }
    fn source_health(&self, _h: knobas_sync::config::CredentialHealth) {}
    fn activity_new(&self, _r: knobas_core::activity::ActivityRow) {}
}

/// UI calls that failed, and UI calls that took over a second.
static FAILED: AtomicUsize = AtomicUsize::new(0);
static SLOW: AtomicUsize = AtomicUsize::new(0);
/// The slowest UI call that succeeded: its time and its name.
static SLOWEST: std::sync::Mutex<(u128, &str)> = std::sync::Mutex::new((0, ""));

/// Time one UI call and print it when slow or failed, counting both.
async fn timed<T, E: std::fmt::Display>(
    t0: Instant,
    name: &'static str,
    f: impl std::future::Future<Output = Result<T, E>>,
) -> Option<T> {
    let start = Instant::now();
    let r = f.await;
    let ms = start.elapsed().as_millis();
    if r.is_ok() {
        let mut slowest = SLOWEST.lock().unwrap();
        if ms > slowest.0 {
            *slowest = (ms, name);
        }
    }
    match r {
        Ok(v) => {
            if ms > 1000 {
                SLOW.fetch_add(1, Ordering::Relaxed);
                println!("[{:>6.1}] SLOW {name} {ms} ms", t0.elapsed().as_secs_f32());
            }
            Some(v)
        }
        Err(e) => {
            FAILED.fetch_add(1, Ordering::Relaxed);
            println!(
                "[{:>6.1}] FAIL {name} after {ms} ms: {e}",
                t0.elapsed().as_secs_f32()
            );
            None
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
#[ignore = "three minutes of wall clock; run by `just ui-pool-probe`"]
async fn the_ui_pool_keeps_a_free_connection_while_five_sources_sync() {
    let n_sources = env("HP_SOURCES", 5);
    let items = env("HP_ITEMS", 1000);
    let secs = env("HP_SECS", 180);
    let page_delay = Duration::from_millis(env("HP_PAGE_DELAY_MS", 200));
    // The app's pool, statement timeout and all: five connections.
    let ui_pool_size = 5;
    let mimic_tray = env("HP_TRAY", 1) == 1;

    let connector = knobas_db::test_util::scratch_database("hangprobe").await;
    let ui = connector.app_pool().await.unwrap();
    let sync_pool = connector
        .pool(knobas_sync::scheduler::SYNC_POOL_SIZE)
        .await
        .unwrap();
    let mut sampler = connector.connect().await.unwrap();
    // What the app's bring-up does before the first sync.
    knobas_app::commands::search::searcher(&ui, &Registry::for_process())
        .await
        .unwrap();

    let secrets = MemoryStore::new();
    let mut ids = Vec::new();
    for n in 0..n_sources {
        let id = format!("jira{n}");
        config::insert(
            &ui,
            &InsertConfig {
                id: id.clone(),
                adapter_kind: "jira".into(),
                display_name: id.clone(),
                base_url: "http://jira.invalid".into(),
                auth_kind: AuthKind::Method(AuthMethod::Pat),
                config: serde_json::json!({}),
                sync_interval_secs: 60,
                enabled: true,
            },
        )
        .await
        .unwrap();
        secrets
            .put(
                &knobas_secrets::KeychainAccount::source(&id),
                &Secret::just(AuthMethod::Pat, "tok"),
            )
            .unwrap();
        ids.push(id);
    }

    let t0 = Instant::now();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let scheduler = Scheduler::start(SchedulerDeps {
        pool: sync_pool,
        connections: Arc::new(Conns(connector.clone())),
        registry: Arc::new(Reg {
            items,
            page: 100,
            page_delay,
            runs: Arc::new(AtomicU64::new(0)),
        }),
        secrets: Arc::new(secrets),
        events: Arc::new(Events { t0, tx }),
        timing: SchedulerTiming {
            startup_delay: Duration::from_millis(500),
            tick: Duration::from_secs(1),
            ..Default::default()
        },
    })
    .await
    .unwrap();
    let scheduler = Arc::new(scheduler);
    // The app's pass: one per process, on connections of its own.
    let pass = SuggestionPass::new(Arc::new(Conns(connector.clone())), PASS_STATEMENT_TIMEOUT);
    // `jira0:JIRA0-5` exists once jira0's first run has landed; opening it
    // before then is a correct not-found, not a failure of the pool.
    let jira0_synced = Arc::new(AtomicBool::new(false));

    if env("HP_RULES_ONLY", 0) == 1 {
        let mut done = std::collections::HashSet::new();
        while done.len() < n_sources as usize {
            let st = rx.recv().await.unwrap();
            if !st.running {
                done.insert(st.source_id);
            }
        }
        scheduler.shutdown().await;
        for pass in 0..2 {
            for rule in knobas_core::suggest::RULES {
                let start = Instant::now();
                let n = knobas_core::suggest::detect_rule(&ui, rule).await.unwrap();
                println!(
                    "RULE pass={pass} {} wrote={n} {} ms",
                    rule.name,
                    start.elapsed().as_millis()
                );
            }
        }
        return;
    }

    // The frontend's reaction to a finished run: the SuggestionTray's
    // `refresh(true)` (detect + room read), the status bar's stats/schedule,
    // the sources view's re-list. Fire-and-forget, like `void refresh()`.
    let react = {
        let ui = ui.clone();
        let pass = pass.clone();
        let jira0_synced = Arc::clone(&jira0_synced);
        tokio::spawn(async move {
            while let Some(status) = rx.recv().await {
                if status.running {
                    continue;
                }
                if status.source_id == "jira0" {
                    jira0_synced.store(true, Ordering::Relaxed);
                }
                let ui1 = ui.clone();
                let pass = pass.clone();
                if mimic_tray {
                    tokio::spawn(async move {
                        // The pass itself is not a UI read: it is timed and
                        // printed, never counted against the one-second line.
                        let start = Instant::now();
                        let pass = pass.run().await;
                        println!(
                            "[{:>6.1}] PASS suggestion pass {} ms: {:?}",
                            t0.elapsed().as_secs_f32(),
                            start.elapsed().as_millis(),
                            pass.map_err(|e| e.to_string())
                        );
                        timed(
                            t0,
                            "room_suggestions",
                            room_suggestions_inner(&ui1, &[], None, 20),
                        )
                        .await;
                    });
                }
                let ui2 = ui.clone();
                tokio::spawn(async move {
                    timed(
                        t0,
                        "list_sources",
                        knobas_app::sources::crud::list(&ui2, &Registry::for_process()),
                    )
                    .await;
                });
            }
        })
    };

    // The user: a search a second, an entity open every two, a note every
    // ten, the launcher board every five.
    let user = {
        let ui = ui.clone();
        let jira0_synced = Arc::clone(&jira0_synced);
        tokio::spawn(async move {
            let mut tick = 0u64;
            loop {
                tick += 1;
                let q = WORDS[(tick as usize * 7) % WORDS.len()];
                if let Some(r) = timed(
                    t0,
                    "search",
                    search_inner(
                        &ui,
                        &Registry::for_process(),
                        knobas_search::SearchQuery {
                            raw: q.into(),
                            limit: 50,
                            filters: Default::default(),
                        },
                    ),
                )
                .await
                    && tick.is_multiple_of(10)
                {
                    println!(
                        "[{:>6.1}] search {q:?} -> {} groups",
                        t0.elapsed().as_secs_f32(),
                        serde_json::to_value(&r)
                            .ok()
                            .and_then(|v| v
                                .get("groups")
                                .and_then(|g| g.as_array().map(|a| a.len())))
                            .unwrap_or(0)
                    );
                }
                if tick.is_multiple_of(2) && jira0_synced.load(Ordering::Relaxed) {
                    timed(
                        t0,
                        "get_entity",
                        get_entity_inner(&ui, &Registry::for_process(), "jira0:JIRA0-5"),
                    )
                    .await;
                }
                if tick.is_multiple_of(5) {
                    timed(
                        t0,
                        "launcher_home",
                        launcher_home_inner(&ui, &Registry::for_process()),
                    )
                    .await;
                    timed(
                        t0,
                        "credential_health",
                        knobas_app::sources::crud::health(&ui),
                    )
                    .await;
                }
                if tick.is_multiple_of(10) {
                    timed(
                        t0,
                        "create_note",
                        create_note_inner(&ui, Some("probe"), Some("x"), &[]),
                    )
                    .await;
                }
                tokio::time::sleep(Duration::from_millis(1000)).await;
            }
        })
    };

    // "Sync all" every 45 s on top of the ticker.
    let syncall = {
        let s = Arc::clone(&scheduler);
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(45)).await;
                let _ = s.trigger_all().await;
            }
        })
    };

    let deadline = Instant::now() + Duration::from_secs(secs);
    let (mut samples, mut starved) = (0usize, 0usize);
    while Instant::now() < deadline {
        tokio::time::sleep(Duration::from_secs(1)).await;
        let rows = sqlx::query(
            "select pid, coalesce(state,'') as state,
                    coalesce(wait_event_type,'') || ':' || coalesce(wait_event,'') as wait,
                    coalesce(extract(epoch from now() - xact_start)::int, -1) as xact_s,
                    coalesce(extract(epoch from now() - query_start)::int, -1) as q_s,
                    left(regexp_replace(query, '\\s+', ' ', 'g'), 70) as q
               from pg_stat_activity
              where datname = current_database() and pid <> pg_backend_pid()
              order by pid",
        )
        .fetch_all(&mut sampler)
        .await
        .unwrap();
        let busy: Vec<String> = rows
            .iter()
            .filter(|r| r.get::<String, _>("state") != "idle")
            .map(|r| {
                format!(
                    "  pid={} {} wait={} xact={}s q={}s :: {}",
                    r.get::<i32, _>("pid"),
                    r.get::<String, _>("state"),
                    r.get::<String, _>("wait"),
                    r.get::<i32, _>("xact_s"),
                    r.get::<i32, _>("q_s"),
                    r.get::<String, _>("q")
                )
            })
            .collect();
        samples += 1;
        // No free connection: every one the pool may open is open, and none
        // is idle -- the next UI acquire waits.
        if ui.size() >= ui_pool_size && ui.num_idle() == 0 {
            starved += 1;
        }
        println!(
            "[{:>6.1}] SAMPLE backends={} busy={} ui_pool size={} idle={}",
            t0.elapsed().as_secs_f32(),
            rows.len(),
            busy.len(),
            ui.size(),
            ui.num_idle()
        );
        for b in busy {
            println!("{b}");
        }
    }
    user.abort();
    react.abort();
    syncall.abort();
    let n_items: i64 = sqlx::query_scalar("select count(*) from sync.item")
        .fetch_one(&mut sampler)
        .await
        .unwrap();
    let n_links: i64 = sqlx::query_scalar("select count(*) from knobas.link")
        .fetch_one(&mut sampler)
        .await
        .unwrap();
    let decl_upd: i64 = sqlx::query_scalar(
        "select n_tup_upd + n_tup_ins from pg_stat_user_tables where relname = 'parent_declaration'",
    )
    .fetch_one(&mut sampler)
    .await
    .unwrap();
    let (failed, slow) = (FAILED.load(Ordering::Relaxed), SLOW.load(Ordering::Relaxed));
    let slowest = *SLOWEST.lock().unwrap();
    println!("END slowest UI call: {} {} ms", slowest.1, slowest.0);
    println!("END items={n_items} links={n_links} parent_declaration writes={decl_upd}");
    println!(
        "END ui_pool starved in {starved} of {samples} samples; {failed} UI calls failed, \
         {slow} took over 1 s"
    );
    scheduler.shutdown().await;
    assert_eq!(
        starved, 0,
        "the UI pool had no free connection in {starved} of {samples} samples"
    );
    assert_eq!(failed, 0, "{failed} UI calls failed");
    assert_eq!(slow, 0, "{slow} UI calls took over a second");
}
