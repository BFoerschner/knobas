//! **Slow sources, measured** (the second smoothness pass on PR #2479): every
//! read a Tickets, Docs, Builds, Monitors or shell page waits on, timed while a
//! proxy holds each request to the testenv Jira, Confluence, TeamCity and
//! Uptime Kuma for a set time, or forever, or refuses it.
//!
//! As `slow_forge_live.rs`, nothing here asserts a budget. It prints one
//! `SLOW` line per read and mode, `cold` (a fresh [`LiveSources`], the first
//! read in a session) and `warm` (the same read again at once). The reads are
//! the app's own, the ones the pages' IPC commands call. A read of the mirror
//! is in the list too: its line says the page did not wait on the source.
//!
//! Gitea and Forgejo are configured beside them at full speed, unproxied, as
//! `just live-sources` configures the six: what a run's commit resolves
//! against.
//!
//! # Writes
//!
//! A write answers once it is queued (`submit_write`, #752) and is sent
//! behind. It is timed only in the `down` and `hang` modes, where the proxy
//! forwards nothing, so nothing reaches a product; the request counter shows
//! the queue asked the source nothing in any mode.
//!
//! ```text
//! just live-up jira; just live-up confluence; just live-up teamcity
//! eval "$(testenv/seed --env)"
//! cargo test -p knobas-app --test slow_sources_live -- --ignored --nocapture
//! ```

mod live_digest;
mod live_under;
mod slow_proxy;

use std::sync::Arc;
use std::time::{Duration, Instant};

use knobas_app::sources::live::LiveSources;
use knobas_app::sources::{Registry, SourcesState};
use knobas_secrets::{MemoryStore, Secret, SecretStore};
use knobas_source::AuthMethod;
use knobas_sync::scheduler::{Scheduler, SchedulerDeps};
use serde_json::json;
use slow_proxy as slow;

/// No read is waited on longer than this; past it the line says so.
const CAP: Duration = Duration::from_secs(70);

fn need(name: &str) -> String {
    std::env::var(name)
        .unwrap_or_else(|_| {
            panic!("{name} is not set -- run `eval \"$(testenv/seed --env)\"` first")
        })
        .trim()
        .trim_end_matches('/')
        .to_owned()
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

/// A source: its id, adapter, address, credential and configuration.
struct Product {
    id: &'static str,
    kind: &'static str,
    url: String,
    secret: Secret,
    auth: AuthMethod,
    config: serde_json::Value,
}

/// The products, each behind its own proxy, and the two forges at full speed.
fn products() -> Vec<Product> {
    let gitea = |id: &'static str, url: String, token: String| Product {
        id,
        kind: "gitea",
        url,
        secret: Secret::just(AuthMethod::Pat, token),
        auth: AuthMethod::Pat,
        config: json!({ "repos": ["tidewater/payout-service", "tidewater/ledger-api"],
                         "clone_urls": ["http://gitea:3000"] }),
    };
    let forgejo_state: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../testenv/seed-state-forgejo.json"),
        )
        .expect("testenv/seed-state-forgejo.json: run `./seed --forgejo`"),
    )
    .expect("seed-state-forgejo.json is JSON");
    vec![
        Product {
            id: "jira",
            kind: "jira",
            url: need("KNOBAS_JIRA_URL"),
            secret: Secret::just(AuthMethod::UserPassword, need("KNOBAS_JIRA_PASSWORD")),
            auth: AuthMethod::UserPassword,
            config: json!({ "username": need("KNOBAS_JIRA_USER") }),
        },
        Product {
            id: "confluence",
            kind: "confluence",
            url: need("KNOBAS_CONFLUENCE_URL"),
            secret: Secret::just(AuthMethod::UserPassword, need("KNOBAS_CONFLUENCE_PASSWORD")),
            auth: AuthMethod::UserPassword,
            config: json!({ "username": need("KNOBAS_CONFLUENCE_USER") }),
        },
        Product {
            id: "teamcity",
            kind: "teamcity",
            url: need("KNOBAS_TEAMCITY_URL"),
            secret: Secret::just(AuthMethod::Pat, need("KNOBAS_TEAMCITY_TOKEN")),
            auth: AuthMethod::Pat,
            config: json!({ "username": "knobas" }),
        },
        Product {
            id: "kuma",
            kind: knobas_source_kuma::ADAPTER_KIND,
            url: need("KNOBAS_KUMA_URL"),
            secret: Secret {
                kind: AuthMethod::ApiToken,
                value: need("KNOBAS_KUMA_API_KEY"),
                account: Some(knobas_source::instance::Account {
                    username: need("KNOBAS_KUMA_USER"),
                    password: need("KNOBAS_KUMA_PASSWORD"),
                }),
            },
            auth: AuthMethod::ApiToken,
            config: json!({}),
        },
        gitea(
            "gitea",
            need("KNOBAS_GITEA_URL"),
            need("KNOBAS_GITEA_TOKEN"),
        ),
        gitea(
            "forgejo",
            forgejo_state["gitea"]["url"]
                .as_str()
                .expect("a Forgejo url")
                .trim_end_matches('/')
                .to_owned(),
            forgejo_state["gitea"]["token"]
                .as_str()
                .expect("a Forgejo token")
                .to_owned(),
        ),
    ]
}

/// The products the proxies stand in front of.
const SLOWED: [&str; 4] = ["jira", "confluence", "teamcity", "kuma"];

/// One pass's readings, by what was read.
struct Pass {
    mode: &'static str,
    /// Whether this pass queues the writes (the first only).
    writes: bool,
    took: std::sync::Mutex<Vec<(String, Took)>>,
}

impl Pass {
    fn new(mode: &'static str, writes: bool) -> Self {
        Self {
            mode,
            writes,
            took: std::sync::Mutex::new(Vec::new()),
        }
    }

    fn took(self) -> Vec<(String, Took)> {
        self.took.into_inner().unwrap()
    }
}

struct Rig {
    state: SourcesState,
    proxies: Vec<(&'static str, slow::Proxy)>,
    ticket: String,
    attachment: Option<String>,
    transition: Option<String>,
    page: String,
    space_query: String,
    pipeline: String,
    run: String,
    monitor: String,
}

async fn rig() -> Rig {
    let connector = knobas_db::test_util::scratch_database("slow_sources_live").await;
    let pool = connector
        .pool(4)
        .await
        .expect("a pool onto the scratch database");
    live_under::declare(&pool).await;
    let secrets = Arc::new(MemoryStore::new());
    let mut proxies = Vec::new();
    for product in products() {
        let base_url = if SLOWED.contains(&product.id) {
            let proxy = slow::start(&product.url).await;
            let url = proxy.url.clone();
            proxies.push((product.id, proxy));
            url
        } else {
            product.url.clone()
        };
        secrets
            .put(
                &knobas_secrets::KeychainAccount::source(product.id),
                &product.secret,
            )
            .expect("the credential is stored");
        knobas_sync::config::insert(
            &pool,
            &knobas_sync::config::InsertConfig {
                id: product.id.to_owned(),
                adapter_kind: product.kind.to_owned(),
                display_name: product.id.to_owned(),
                base_url,
                auth_kind: knobas_sync::config::AuthKind::Method(product.auth),
                config: product.config,
                sync_interval_secs: 86_400,
                enabled: true,
            },
        )
        .await
        .expect("the source row is written");
    }
    let scheduler = Scheduler::start(SchedulerDeps {
        pool: pool.clone(),
        connections: Arc::new(live_digest::Connections(connector.clone())),
        registry: Arc::new(Registry::for_process()),
        secrets: secrets.clone(),
        events: Arc::new(live_digest::Quiet),
        timing: knobas_sync::scheduler::SchedulerTiming::default(),
        quit: knobas_sync::scheduler::Quit::default(),
    })
    .await
    .expect("a scheduler over the scratch database");
    let state = SourcesState {
        live: LiveSources::default(),
        pool: pool.clone(),
        scheduler,
        secrets,
        registry: Arc::new(Registry::for_process()),
    };
    for id in ["gitea", "forgejo", "jira", "confluence", "teamcity", "kuma"] {
        let started = Instant::now();
        live_digest::sync(&state, id).await;
        println!("synced {id} in {} ms", started.elapsed().as_millis());
    }

    let one = |sql: &'static str| {
        let pool = pool.clone();
        async move {
            sqlx::query_scalar::<_, String>(sql)
                .fetch_optional(&pool)
                .await
                .unwrap()
        }
    };
    let ticket = one(
        "select entity_id from sync.live_item where source_id = 'jira'
           and jsonb_array_length(coalesce(payload->'fields'->'attachment', '[]')) > 0
         order by entity_id limit 1",
    )
    .await;
    let ticket = match ticket {
        Some(t) => t,
        None => one(
            "select entity_id from sync.live_item where source_id = 'jira'
               and kind = 'ticket' order by entity_id limit 1",
        )
        .await
        .expect("a Jira ticket in the mirror"),
    };
    let detail =
        knobas_app::commands::entity::get_entity_inner(&pool, state.registry.as_ref(), &ticket)
            .await
            .expect("the ticket reads");
    let attachment = detail
        .attachments
        .unwrap_or_default()
        .into_iter()
        .next()
        .map(|a| a.id);
    let transition = one(
        "select distinct payload->'fields'->'status'->>'name' from sync.live_item
          where source_id = 'jira' and kind = 'ticket'
            and payload->'fields'->'status'->>'name' is not null limit 1",
    )
    .await;
    let page = one(
        "select entity_id from sync.live_item where source_id = 'confluence'
           and kind = 'page' order by length(coalesce(body_text, '')) desc limit 1",
    )
    .await
    .expect("a Confluence page in the mirror");
    let run = one(
        "select entity_id from sync.live_item where source_id = 'teamcity'
           and kind = 'build' and payload->>'status' = 'FAILURE' order by entity_id limit 1",
    )
    .await
    .expect("a failed TeamCity build in the mirror");
    let pipeline: Option<String> = None;
    let pipeline = match pipeline {
        Some(p) => p,
        None => {
            let rows =
                knobas_app::commands::entity::pipelines_inner(&pool, state.registry.as_ref())
                    .await
                    .expect("the pipelines read");
            let mut found = None;
            for row in rows {
                let runs = knobas_app::commands::entity::pipeline_runs_inner(
                    &pool,
                    state.registry.as_ref(),
                    &row.entity_id,
                    Some(50),
                )
                .await
                .unwrap_or_default();
                if runs.iter().any(|r| r.entity_id == run) {
                    found = Some(row.entity_id);
                    break;
                }
            }
            found.expect("the failed build's pipeline")
        }
    };
    let monitor = one(
        "select entity_id from sync.live_item where source_id = 'kuma'
           and kind = 'monitor' order by entity_id limit 1",
    )
    .await
    .expect("a Kuma monitor in the mirror");
    println!(
        "ticket {ticket} (attachment {attachment:?}), page {page}, pipeline {pipeline}, \
         run {run}, monitor {monitor}"
    );
    Rig {
        state,
        proxies,
        ticket,
        attachment,
        transition,
        page,
        space_query: "sepa".to_owned(),
        pipeline,
        run,
        monitor,
    }
}

impl Rig {
    fn set(&self, ms: u64) {
        for (_, proxy) in &self.proxies {
            proxy.set(ms);
        }
    }

    fn grants(
        &self,
        live: &LiveSources,
        asks: Vec<knobas_source::grants::WriteAsk>,
    ) -> impl std::future::Future<Output = Result<Vec<knobas_source::grants::RefusedWrite>, String>>
    {
        let (pool, secrets, registry, live) = (
            self.state.pool.clone(),
            self.state.secrets.clone(),
            self.state.registry.clone(),
            live.clone(),
        );
        async move {
            Ok(
                knobas_app::sources::crud::refused_writes(&pool, &secrets, &registry, &live, &asks)
                    .await,
            )
        }
    }

    fn ask(entity: &str, op: &str) -> knobas_source::grants::WriteAsk {
        knobas_source::grants::WriteAsk {
            entity: entity.to_owned(),
            op: op.to_owned(),
            field: None,
        }
    }

    /// Time `read` under `what`, in this pass.
    async fn once<T, E: std::fmt::Display>(
        &self,
        pass: &Pass,
        what: &str,
        read: impl std::future::Future<Output = Result<T, E>>,
    ) {
        let (took, _) = time(read).await;
        pass.took.lock().unwrap().push((what.to_owned(), took));
    }

    /// A write queued as `submit_write` queues it, and how many requests the
    /// answer made; only where nothing can reach the product.
    async fn queued(&self, pass: &Pass, what: &str, payload: serde_json::Value) {
        let mode = pass.mode;
        if !pass.writes || !matches!(mode, "down" | "hang") {
            return;
        }
        let (answer, sent) = knobas_http::counted::counted(time(
            knobas_app::sources::write_queue::submit_in_background(&self.state, payload),
        ))
        .await;
        let (took, write) = answer;
        println!("SLOW {mode:>6} | {what:<34} | queued {took} | requests before the answer {sent}");
        if let Some(write) = write {
            let _ = knobas_sync::write_queue::discard(self.state.scheduler.deps(), write.id).await;
        }
    }

    async fn tickets(&self, pass: &Pass, live: &LiveSources) {
        let (pool, registry) = (&self.state.pool, self.state.registry.as_ref());
        self.once(pass, "tickets: board", {
            knobas_app::commands::entity::mini_board_inner(
                pool,
                registry,
                None,
                Some(knobas_source::role::TICKET_SYSTEM),
                None,
                None,
                None,
            )
        })
        .await;
        self.once(pass, "tickets: list (search)", {
            knobas_app::commands::search::search_inner(
                pool,
                registry,
                knobas_search::SearchQuery {
                    raw: "PAY".to_owned(),
                    limit: 50,
                    filters: knobas_search::SearchFilters {
                        sources: vec!["jira".to_owned()],
                        ..Default::default()
                    },
                    titles: false,
                },
            )
        })
        .await;
        self.once(pass, "ticket: open, comments, links", {
            knobas_app::commands::entity::get_entity_inner(pool, registry, &self.ticket)
        })
        .await;
        self.once(pass, "ticket: comment and move grants", {
            self.grants(
                live,
                vec![
                    Self::ask(&self.ticket, "comment"),
                    Self::ask(&self.ticket, "transition"),
                ],
            )
        })
        .await;
        if let Some(attachment) = &self.attachment {
            self.once(pass, "ticket: attachment preview", {
                knobas_app::commands::entity::attachment_bytes_inner(
                    pool,
                    &self.state.secrets,
                    &self.state.registry,
                    live,
                    &self.ticket,
                    attachment,
                )
            })
            .await;
        }
        self.queued(
            pass,
            "ticket: comment",
            json!({ "Comment": { "entity": self.ticket, "body": "slow rig" } }),
        )
        .await;
        if let Some(to) = &self.transition {
            self.queued(
                pass,
                "ticket: move",
                json!({ "Transition": { "entity": self.ticket, "status": to } }),
            )
            .await;
        }
    }

    async fn docs(&self, pass: &Pass, live: &LiveSources) {
        let (pool, registry) = (&self.state.pool, self.state.registry.as_ref());
        self.once(pass, "docs: home, spaces, tree", {
            knobas_app::commands::entity::docs_tree_inner(pool, registry)
        })
        .await;
        self.once(pass, "docs: open a page", {
            knobas_app::commands::entity::docs_page_inner(pool, registry, &self.page)
        })
        .await;
        self.once(pass, "docs: search", {
            knobas_app::commands::entity::docs_search_inner(pool, registry, &self.space_query, None)
        })
        .await;
        self.once(pass, "docs: edit, comment, new page grants", {
            self.grants(
                live,
                vec![
                    Self::ask(&self.page, "edit_section"),
                    Self::ask(&self.page, "comment"),
                    Self::ask(&self.page, "create_page"),
                ],
            )
        })
        .await;
        self.queued(
            pass,
            "docs: comment",
            json!({ "Comment": { "entity": self.page, "body": "slow rig" } }),
        )
        .await;
    }

    async fn builds(&self, pass: &Pass, live: &LiveSources) {
        let (pool, secrets, registry) =
            (&self.state.pool, &self.state.secrets, &self.state.registry);
        self.once(pass, "builds: pipelines", {
            knobas_app::commands::entity::pipelines_inner(pool, registry.as_ref())
        })
        .await;
        self.once(pass, "builds: a pipeline's runs", {
            knobas_app::commands::entity::pipeline_runs_live_inner(
                pool,
                secrets,
                registry,
                live,
                &self.pipeline,
                None,
            )
        })
        .await;
        self.once(pass, "run: its item (mirror)", {
            knobas_app::commands::entity::get_entity_inner(pool, registry.as_ref(), &self.run)
        })
        .await;
        self.once(pass, "run: overview (source view)", {
            knobas_app::sources::crud::shown(pool, secrets, registry, live, &self.run, None)
        })
        .await;
        self.once(pass, "run: log", {
            knobas_app::commands::entity::run_log_inner(
                pool, secrets, registry, live, &self.run, None, None,
            )
        })
        .await;
        self.once(pass, "run: tests", {
            knobas_app::commands::entity::run_tests_inner(pool, secrets, registry, live, &self.run)
        })
        .await;
        self.once(pass, "run: artifacts", {
            knobas_app::commands::entity::run_artifacts_inner(
                pool, secrets, registry, live, &self.run, None,
            )
        })
        .await;
        self.once(pass, "run: chain", {
            knobas_app::commands::entity::run_chain_inner(pool, secrets, registry, live, &self.run)
        })
        .await;
        self.once(pass, "Run...: settings (parameters)", {
            knobas_app::commands::entity::pipeline_settings_inner(
                pool,
                secrets,
                registry,
                live,
                &self.pipeline,
            )
        })
        .await;
        self.once(pass, "Run...: grants", {
            self.grants(live, vec![Self::ask(&self.pipeline, "run_build")])
        })
        .await;
    }

    /// The Monitors tab asks no grant: Kuma's writes are drawn from what the
    /// instance offers, read without a request.
    async fn monitors(&self, pass: &Pass, _live: &LiveSources) {
        let (pool, registry) = (&self.state.pool, self.state.registry.as_ref());
        self.once(pass, "monitors: the list", {
            knobas_app::assets::monitor_roster(pool, registry)
        })
        .await;
        self.once(pass, "monitor: its page", {
            knobas_app::commands::entity::get_entity_inner(pool, registry, &self.monitor)
        })
        .await;
    }

    async fn shell(&self, pass: &Pass) {
        let (pool, registry) = (&self.state.pool, self.state.registry.as_ref());
        let now = chrono::Utc::now();
        self.once(pass, "shell: inbox", {
            knobas_app::commands::entity::inbox_items_inner(
                pool,
                registry,
                now,
                knobas_core::inbox::Shelf::Stream,
            )
        })
        .await;
        self.once(pass, "shell: launcher search", {
            knobas_app::commands::search::search_inner(
                pool,
                registry,
                knobas_search::SearchQuery {
                    raw: "sepa".to_owned(),
                    limit: 30,
                    filters: knobas_search::SearchFilters::default(),
                    titles: false,
                },
            )
        })
        .await;
        let today = live_digest::day_window(now.date_naive());
        self.once(pass, "shell: standup", {
            knobas_app::commands::entity::standup_digest_inner(pool, registry, now, today, &[])
        })
        .await;
        self.once(pass, "shell: time, the day", {
            knobas_app::time::day::list(pool, today.from, today.to)
        })
        .await;
        self.once(pass, "shell: asset tree", {
            knobas_app::assets::tree(pool, None)
        })
        .await;
    }

    /// One pass over every read through `live`; writes only on the first.
    async fn pass(&self, pass: &Pass, live: &LiveSources) {
        tokio::join!(
            self.tickets(pass, live),
            self.docs(pass, live),
            self.builds(pass, live),
            self.monitors(pass, live),
            self.shell(pass),
        );
    }

    /// A session as the app makes one: its reads held between sessions
    /// (ADR-0042) in the scratch database.
    fn session(&self) -> LiveSources {
        let live = LiveSources::default();
        live.hold_in(self.state.pool.clone());
        live
    }

    /// Every read three times: the first ever (nothing held), in the next
    /// session (what the first held), and again in that session (warm).
    async fn measure(&self, mode: &'static str) {
        sqlx::query("delete from sync.held_read")
            .execute(&self.state.pool)
            .await
            .expect("the held reads are cleared");
        let first = Pass::new(mode, true);
        self.pass(&first, &self.session()).await;
        let next = self.session();
        let held = Pass::new(mode, false);
        self.pass(&held, &next).await;
        let warm = Pass::new(mode, false);
        self.pass(&warm, &next).await;
        let (first, held, warm) = (first.took(), held.took(), warm.took());
        for (what, took) in &first {
            let of = |list: &[(String, Took)]| {
                list.iter()
                    .find(|(w, _)| w == what)
                    .map_or_else(|| "     -".to_owned(), |(_, t)| t.to_string())
            };
            println!(
                "SLOW {mode:>6} | {what:<34} | first {took} | next session {} | warm {}",
                of(&held),
                of(&warm)
            );
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs the testenv Jira, Confluence, TeamCity and Kuma: eval \"$(testenv/seed --env)\""]
async fn every_read_a_page_waits_on_through_slow_sources() {
    let rig = rig().await;
    let only = std::env::var("KNOBAS_SLOW_MODES").unwrap_or_default();
    for (mode, ms) in [
        ("0s", 0),
        ("1s", 1_000),
        ("5s", 5_000),
        ("down", slow::DOWN),
        ("hang", slow::HANG),
    ] {
        if !only.is_empty() && !only.split(',').any(|m| m == mode) {
            continue;
        }
        rig.set(ms);
        rig.measure(mode).await;
    }
    rig.set(0);
}
