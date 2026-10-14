//! What the reader waits on (#753): every read the shell makes for the common
//! interactions, timed against a mirror of realistic size.
//!
//! A table, not an assertion. The mirror is seeded into a scratch database --
//! three Jira-shaped sources of `RP_TICKETS` tickets each (default 5,000), one
//! Confluence-shaped source of `RP_PAGES` pages (2,000), one TeamCity-shaped
//! source of `RP_BUILDS` builds (5,000) -- and, when `KNOBAS_GITEA_URL` and
//! `KNOBAS_GITEA_TOKEN` are set (`eval "$(testenv/seed --env)"`), the seeded
//! Gitea is synced too and its views are read live. Each read is made once
//! cold and `RP_RUNS` times warm (default 7), against the app's own
//! five-connection UI pool, and printed as one `RP` line: interaction, read,
//! cold, warm median, warm max, and the size of the answer as the webview
//! would receive it.
//!
//! **Ignored**, and run by `just responsiveness-probe`: a wall-clock reading
//! measures the machine as much as the code, so it is a reading to put in a
//! PR beside the load average, never a gate test.

use std::time::Instant;

use knobas_app::commands::entity::{
    EntityFilter, EntityOrder, get_entity_inner, inbox_count_inner, inbox_items_inner,
    list_entities_inner, list_projects_inner, recent_activity_inner, room_suggestions_inner,
};
use knobas_app::commands::search::{
    filter_suggestions_inner, launcher_home_inner, search_inner, searcher, smart_lists_inner,
};
use knobas_app::sources::Registry;
use knobas_secrets::{MemoryStore, Secret, SecretStore};
use knobas_source::{AuthMethod, Cursor, Sink, Source, SourceDescriptor, SourceError, SyncItem};
use knobas_sync::config::{self, AuthKind, InsertConfig};
use knobas_sync::scheduler::AdapterRegistry;
use sqlx::PgPool;

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
    "freight",
    "shipment",
    "carrier",
    "route",
    "warehouse",
    "pallet",
    "label",
    "barcode",
    "customs",
    "tariff",
    "currency",
    "booking",
    "container",
    "vessel",
    "manifest",
    "audit",
];
/// How many synthetic words (`w42`, `w43`, ...) the text draws on beside
/// [`WORDS`]: `just suggest-perf`'s vocabulary of 4,000 (#770).
///
/// Until #770 the text drew on the 42 words alone, so every two documents
/// shared five lexemes and `similar_text` had to propose every pair of the
/// 17,000 prose items -- about 144 million proposals, hours of inserts in
/// either planner state, and a corpus no install has. With 4,000 more words
/// two documents share about half a lexeme on average, and the named words
/// stay in the text for the search readings to find.
///
/// `RP_SYNTHETIC=0` puts the narrow corpus back -- templated tickets, every
/// pair similar -- which is what #771's cap of ten proposals per item is
/// measured on.
fn synthetic() -> u64 {
    static SYNTHETIC: std::sync::OnceLock<u64> = std::sync::OnceLock::new();
    *SYNTHETIC.get_or_init(|| env("RP_SYNTHETIC", 4_000))
}

const STATUSES: &[&str] = &["Open", "In Progress", "In Review", "Blocked", "Done"];

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn pick<'a>(&mut self, from: &[&'a str]) -> &'a str {
        from[(self.next() % from.len() as u64) as usize]
    }
    /// `n` words of [`WORDS`] and [`synthetic`] more, drawn evenly.
    fn words(&mut self, n: usize) -> String {
        let vocabulary = WORDS.len() as u64 + synthetic();
        (0..n)
            .map(|_| match self.next() % vocabulary {
                i if i < WORDS.len() as u64 => WORDS[i as usize].to_owned(),
                i => format!("w{i}"),
            })
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// A synthetic source of one adapter's shape: `kind` items, `count` of them.
struct Synthetic {
    descriptor: SourceDescriptor,
    kind: &'static str,
    count: u64,
}

#[async_trait::async_trait]
impl Source for Synthetic {
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
        self.descriptor.clone()
    }
    async fn test_connection(&self) -> Result<knobas_source::ConnectionInfo, SourceError> {
        Ok(knobas_source::ConnectionInfo::default())
    }
    async fn sync(
        &self,
        _cursor: Option<Cursor>,
        sink: &mut (dyn Sink + Send),
    ) -> Result<Cursor, SourceError> {
        let id = &self.descriptor.id;
        let mut rng = Rng(0x9E37_79B9_7F4A_7C15 ^ id.len() as u64 ^ self.count);
        let prefix = id.to_uppercase().replace('-', "");
        let now = chrono::Utc::now();
        for i in 0..self.count {
            let key = format!("{prefix}-{i}");
            let (entity_key, payload) = match self.kind {
                "ticket" => (
                    key.clone(),
                    serde_json::json!({
                        "key": key,
                        "fields": {
                            "summary": rng.words(6),
                            "status": {"name": rng.pick(STATUSES)},
                            "priority": {"name": "High"},
                            "assignee": {"name": "mara", "key": "mara"},
                            "project": {"key": format!("{prefix}{}", i % 4), "name": prefix},
                            "parent": {"key": format!("{prefix}-{}", i / 20)},
                        }
                    }),
                ),
                "build" => (
                    format!("build:{i}"),
                    serde_json::json!({
                        "id": i,
                        "buildType": {
                            "id": format!("Cfg_{}", i % 40),
                            "name": format!("Build {}", i % 40),
                            "projectId": "Payments",
                            "projectName": "Payments",
                        },
                        "number": i.to_string(),
                        "status": if i % 7 == 0 { "FAILURE" } else { "SUCCESS" },
                        "state": "finished",
                        "branchName": "main",
                    }),
                ),
                _ => (
                    format!("page:{i}"),
                    serde_json::json!({"id": i, "title": rng.words(4), "space": {"key": "OPS"}}),
                ),
            };
            sink.item(SyncItem {
                entity: knobas_core::entity::EntityRef::new(id, &entity_key),
                kind: self.kind.into(),
                title: rng.words(6),
                body_text: rng.words(40),
                author: Some("mara".into()),
                updated_at: Some(now - chrono::Duration::minutes(i as i64)),
                payload,
                web_url: None,
                deleted: false,
            })
            .await?;
        }
        Ok("done".to_owned())
    }
}

/// One timed read: cold, then `runs` warm.
struct Reading {
    cold: u128,
    warm: Vec<u128>,
    bytes: usize,
}

async fn time<T: serde::Serialize, E: std::fmt::Debug, F, Fut>(
    interaction: &str,
    read: &str,
    runs: u64,
    mut f: F,
) -> Reading
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
{
    let at = Instant::now();
    let answer = f().await.unwrap_or_else(|e| panic!("{read}: {e:?}"));
    let cold = at.elapsed().as_micros();
    let bytes = serde_json::to_vec(&answer).map_or(0, |b| b.len());
    let mut warm = Vec::new();
    for _ in 0..runs {
        let at = Instant::now();
        f().await.unwrap_or_else(|e| panic!("{read}: {e:?}"));
        warm.push(at.elapsed().as_micros());
    }
    warm.sort_unstable();
    let reading = Reading { cold, warm, bytes };
    println!(
        "RP | {interaction} | {read} | {:.1} | {:.1} | {:.1} | {}",
        reading.cold as f64 / 1000.0,
        reading.warm[reading.warm.len() / 2] as f64 / 1000.0,
        reading.warm[reading.warm.len() - 1] as f64 / 1000.0,
        reading.bytes
    );
    reading
}

fn filter(
    sources: &[&str],
    kinds: &[&str],
    ctx: Option<&str>,
    project: Option<&str>,
    order: EntityOrder,
) -> EntityFilter {
    EntityFilter {
        sources: sources.iter().map(|s| (*s).to_owned()).collect(),
        kinds: kinds.iter().map(|s| (*s).to_owned()).collect(),
        updated_within_days: None,
        context: ctx.map(str::to_owned),
        project: project.map(str::to_owned),
        order,
        include_deleted: false,
    }
}

/// The reads a room makes: the room's own scan, one page per tile, the board,
/// the suggestion tray.
async fn room(
    pool: &PgPool,
    name: &str,
    sources: &[&str],
    ctx: Option<&str>,
    project: Option<&str>,
    runs: u64,
) {
    let registry = Registry::builtin();
    let declarations = knobas_app::sources::paths::declared_paths(pool, &registry)
        .await
        .unwrap();
    let scan = filter(sources, &[], ctx, project, EntityOrder::UpdatedDesc);
    time(name, "list_entities (room scan, 200)", runs, || {
        list_entities_inner(pool, &scan, 200, 0, &declarations)
    })
    .await;
    // The tiles `kinds.ts` draws, each with its own order.
    for (tile, kinds, order) in [
        ("code", &["pr", "branch"][..], EntityOrder::Attention),
        ("builds", &["build"][..], EntityOrder::LatestPerParent),
        ("docs", &["page"][..], EntityOrder::UpdatedDesc),
        ("notes", &["note"][..], EntityOrder::UpdatedDesc),
    ] {
        let tile_filter = filter(sources, kinds, ctx, project, order);
        let tile_filter = &tile_filter;
        time(
            name,
            &format!("list_entities (tile {tile}, 50)"),
            runs,
            || list_entities_inner(pool, tile_filter, 50, 0, &declarations),
        )
        .await;
    }
    let owned: Vec<String> = sources.iter().map(|s| (*s).to_owned()).collect();
    // Whole, as the tile read it before #753, and capped as it reads it now.
    time(name, "mini_board (tickets tile, whole)", runs, || {
        knobas_core::mini_board::read(pool, ctx, &owned, project, &declarations)
    })
    .await;
    time(name, "mini_board (tickets tile, 50 a column)", runs, || {
        knobas_core::mini_board::read_capped(pool, ctx, &owned, project, &declarations, Some(50))
    })
    .await;
    time(name, "room_suggestions (20)", runs, || {
        room_suggestions_inner(pool, &owned, ctx, 20)
    })
    .await;
    if let Some(ctx) = ctx {
        time(name, "context_members", runs, || {
            knobas_core::context::member_ids(pool, ctx)
        })
        .await;
        time(name, "context_assets", runs, || {
            knobas_app::assets::in_context(pool, ctx)
        })
        .await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "a wall-clock reading; run by `just responsiveness-probe`"]
async fn what_the_reader_waits_on() {
    let tickets = env("RP_TICKETS", 5_000);
    let pages = env("RP_PAGES", 2_000);
    let builds = env("RP_BUILDS", 5_000);
    let runs = env("RP_RUNS", 7);

    let connector = knobas_db::test_util::scratch_database("responsiveness").await;
    let ui = connector.app_pool().await.unwrap();
    let pool = &ui;
    searcher(pool).await.unwrap();
    // `RP_ANALYZED=1` reads against tables the planner has statistics for;
    // otherwise autovacuum is switched off on the scratch database's tables, so
    // they stay never analyzed -- the state of a fresh install for about a
    // minute after its first sync, which is where a plan that leans on
    // estimates shows (#767, #768).
    let analyzed = env("RP_ANALYZED", 0) == 1;
    if !analyzed {
        let tables: Vec<(String, String)> = sqlx::query_as(
            "select n.nspname::text, c.relname::text from pg_class c
               join pg_namespace n on n.oid = c.relnamespace
              where c.relkind = 'r' and n.nspname in ('knobas', 'sync')",
        )
        .fetch_all(pool)
        .await
        .unwrap();
        for (schema, table) in tables {
            sqlx::query(sqlx::AssertSqlSafe(format!(
                "alter table {schema}.{table} set (autovacuum_enabled = false)"
            )))
            .execute(pool)
            .await
            .unwrap();
        }
    }

    let seeded = Instant::now();
    let mut synthetic = Vec::new();
    for n in 0..3 {
        synthetic.push(Synthetic {
            descriptor: SourceDescriptor {
                id: format!("jira{n}"),
                ..knobas_source_jira::descriptor_template()
            },
            kind: "ticket",
            count: tickets,
        });
    }
    synthetic.push(Synthetic {
        descriptor: SourceDescriptor {
            id: "wiki".to_owned(),
            ..knobas_source_confluence::descriptor_template()
        },
        kind: "page",
        count: pages,
    });
    synthetic.push(Synthetic {
        descriptor: SourceDescriptor {
            id: "ci".to_owned(),
            ..knobas_source_teamcity::descriptor_template()
        },
        kind: "build",
        count: builds,
    });
    for source in &synthetic {
        knobas_app::sources::demo::load_mirror_of(pool, source)
            .await
            .unwrap();
    }

    // The seeded Gitea, synced through its real adapter, when the env says
    // where it is. Its views are the live reads the detail makes.
    let secrets: std::sync::Arc<dyn SecretStore> = std::sync::Arc::new(MemoryStore::new());
    let gitea = match (
        std::env::var("KNOBAS_GITEA_URL"),
        std::env::var("KNOBAS_GITEA_TOKEN"),
    ) {
        (Ok(url), Ok(token)) => {
            config::insert(
                pool,
                &InsertConfig {
                    id: "gitea".into(),
                    adapter_kind: "gitea".into(),
                    display_name: "Gitea".into(),
                    base_url: url.clone(),
                    auth_kind: AuthKind::Method(AuthMethod::Pat),
                    config: serde_json::json!({
                        "repos": ["tidewater/payout-service", "tidewater/ledger-api"]
                    }),
                    sync_interval_secs: 86_400,
                    enabled: true,
                },
            )
            .await
            .unwrap();
            secrets
                .put(
                    &knobas_secrets::KeychainAccount::source("gitea"),
                    &Secret::just(AuthMethod::Pat, token.clone()),
                )
                .unwrap();
            let source = Registry::builtin()
                .build(knobas_source::instance::SourceInstance {
                    id: "gitea".into(),
                    kind: "gitea".into(),
                    display_name: "Gitea".into(),
                    base_url: url,
                    auth: Some(AuthMethod::Pat),
                    secret: Some(token),
                    account: None,
                    config: serde_json::json!({
                        "repos": ["tidewater/payout-service", "tidewater/ledger-api"]
                    }),
                })
                .unwrap();
            knobas_sync::run_once(pool, source.as_ref(), None)
                .await
                .unwrap();
            true
        }
        _ => false,
    };
    let items: i64 = sqlx::query_scalar("select count(*) from sync.item")
        .fetch_one(pool)
        .await
        .unwrap();
    println!(
        "RP-SEED items={items} tickets/source={tickets} pages={pages} builds={builds} gitea={gitea} in {} s",
        seeded.elapsed().as_secs()
    );
    // Analyzed before the pass as well as after it: a real install's
    // autovacuum analyzes within about a minute of the first sync, so the pass
    // that follows a sync plans against statistics (#770). Again after, so the
    // reads plan against the links the pass wrote.
    if analyzed {
        sqlx::query("analyze").execute(pool).await.unwrap();
    }
    // The suggestion pass has run once, as it would have after a sync. Timed,
    // with what it wrote, because whether it finishes in both states is what
    // #770 measured: before it, the pass died in its first exact-key rule on
    // tables never analyzed.
    let pass = knobas_app::suggestion_pass::SuggestionPass::new(
        std::sync::Arc::new(Conns(connector.clone())),
        knobas_app::suggestion_pass::PASS_STATEMENT_TIMEOUT,
    );
    let passed = Instant::now();
    let outcome = knobas_app::commands::entity::detect_suggestions_inner(&pass).await;
    let proposals: i64 = sqlx::query_scalar("select count(*) from knobas.proposed_link")
        .fetch_one(pool)
        .await
        .unwrap();
    println!(
        "RP-PASS analyzed={analyzed} ms={} outcome={:?} proposals={proposals}",
        passed.elapsed().as_millis(),
        outcome.map_err(|error| error.message)
    );
    if analyzed {
        sqlx::query("analyze").execute(pool).await.unwrap();
    }
    println!("RP-STATS analyzed={analyzed}");
    // A stored context: the promoted epic `jira0:JIRA0-3` and its children.
    let promoted = knobas_core::context::promote(
        pool,
        &knobas_core::entity::EntityRef::parse("jira0:JIRA0-3").unwrap(),
    )
    .await
    .unwrap()
    .expect("JIRA0-3 promotes");
    let ctx = promoted.context.id.clone();
    println!("RP | interaction | read | cold ms | warm median ms | warm max ms | bytes");

    // -- opening the app -----------------------------------------------------
    let registry = Registry::builtin();
    let now = chrono::Utc::now();
    let boot = "open app";
    time(boot, "list_sources", runs, || {
        knobas_app::sources::crud::list(pool, &registry)
    })
    .await;
    time(boot, "credential_health", runs, || {
        knobas_app::sources::crud::health(pool)
    })
    .await;
    time(boot, "list_contexts", runs, || {
        knobas_core::context::list(pool)
    })
    .await;
    time(boot, "list_projects", runs, || list_projects_inner(pool)).await;
    time(boot, "inbox_items (stream)", runs, || {
        inbox_items_inner(pool, &registry, now, knobas_core::inbox::Shelf::Stream)
    })
    .await;
    time(boot, "inbox_items (snoozed)", runs, || {
        inbox_items_inner(pool, &registry, now, knobas_core::inbox::Shelf::Snoozed)
    })
    .await;
    time(boot, "inbox_count", runs, || {
        inbox_count_inner(pool, &registry, now)
    })
    .await;
    time(boot, "open_alerts", runs, || {
        knobas_app::assets::open_alerts(pool)
    })
    .await;
    time(boot, "recent_activity (1)", runs, || {
        recent_activity_inner(pool, 1, None)
    })
    .await;
    time(boot, "db_stats", runs, || {
        knobas_sync::stats::db_stats(pool)
    })
    .await;
    time(boot, "sync_status", runs, || {
        knobas_sync::scheduler::status_all(pool)
    })
    .await;
    time(boot, "current_timer", runs, || {
        knobas_app::time::current(pool)
    })
    .await;
    time(boot, "write_queue_counts", runs, || {
        knobas_core::write_queue::counts(pool)
    })
    .await;

    // -- switching rooms -----------------------------------------------------
    room(pool, "room: All", &[], None, None, runs).await;
    room(
        pool,
        "room: project JIRA00",
        &["jira0"],
        None,
        Some("JIRA00"),
        runs,
    )
    .await;
    room(pool, "room: stored context", &[], Some(&ctx), None, runs).await;

    // -- opening a detail ----------------------------------------------------
    let declarations = knobas_app::sources::paths::declared_paths(pool, &registry)
        .await
        .unwrap();
    let _ = declarations;
    for (what, id, kind) in [
        ("detail: ticket", "jira0:JIRA0-42", "ticket"),
        ("detail: build", "ci:build:42", "build"),
        ("detail: page", "wiki:page:42", "page"),
    ] {
        time(what, "get_entity", runs, || get_entity_inner(pool, id)).await;
        let entity = knobas_core::entity::EntityRef::parse(id).unwrap();
        time(what, "recent_activity (entity, 50)", runs, || {
            recent_activity_inner(pool, 50, Some(&entity))
        })
        .await;
        time(what, "entity_checkout", runs, || async {
            Ok::<_, ()>(knobas_app::checkout::view(pool, id).await.ok())
        })
        .await;
        if kind == "ticket" {
            let source = ["jira0".to_owned()];
            let declarations = knobas_app::sources::paths::declared_paths(pool, &registry)
                .await
                .unwrap();
            // Opening a ticket reads no board since #760 (it was 725 KB here):
            // `get_entity` above carries the status, and this is the offer's
            // fallback, asked only when the workflow read fails.
            time(
                what,
                "source_statuses (workflow-failure fallback)",
                runs,
                || knobas_core::mini_board::source_statuses(pool, &source[0], &declarations),
            )
            .await;
        }
    }

    // -- the launcher --------------------------------------------------------
    time("open launcher", "launcher_home", runs, || {
        launcher_home_inner(pool)
    })
    .await;
    time("open launcher", "smart_lists", runs, || {
        smart_lists_inner(pool)
    })
    .await;
    // Opening the box on a filter token reads the kinds, the sources and the
    // parents once (#601); each keystroke after it only the parents (#768).
    time(
        "open a filter token",
        "filter_suggestions (kinds, sources, parents)",
        runs,
        || filter_suggestions_inner(pool, "", false),
    )
    .await;
    // The first `under:` keystroke asks with nothing typed yet, and each one
    // after it with the value so far (#761).
    for under in ["", "j", "jira0-1", "pay"] {
        time(
            "type a filter token",
            &format!("filter_suggestions (under:{under})"),
            runs,
            || filter_suggestions_inner(pool, under, true),
        )
        .await;
    }
    for raw in [
        "p",
        "pa",
        "pay",
        "paym",
        "payme",
        "paymen",
        "payment",
        "payment re",
        "payment retry",
        "JIRA0-42",
        "kind:build fail",
    ] {
        time("search keystroke", &format!("search {raw:?}"), runs, || {
            search_inner(
                pool,
                knobas_search::SearchQuery {
                    raw: raw.into(),
                    limit: 50,
                    filters: Default::default(),
                },
            )
        })
        .await;
    }

    // -- Gitea views, live ---------------------------------------------------
    if gitea {
        let pick = |kind: &'static str| async move {
            sqlx::query_scalar::<_, String>(
                "select entity_id from sync.item where source_id = 'gitea' and kind = $1
                  order by entity_id limit 1",
            )
            .bind(kind)
            .fetch_optional(pool)
            .await
            .unwrap()
        };
        for kind in ["repo", "pr", "branch", "commit"] {
            let Some(id) = pick(kind).await else {
                println!("RP-NOTE no gitea {kind} mirrored");
                continue;
            };
            let what = format!("gitea {kind}");
            time(&what, "get_entity", runs, || get_entity_inner(pool, &id)).await;
            let view = time(&what, "entity_view (whole)", runs, || {
                knobas_app::sources::crud::view(pool, &secrets, &registry, &id, None)
            })
            .await;
            let _ = view;
            let whole = knobas_app::sources::crud::view(pool, &secrets, &registry, &id, None)
                .await
                .unwrap()
                .unwrap();
            for section in &whole.sections {
                time(
                    &what,
                    &format!("entity_view (tab {})", section.id),
                    runs,
                    || {
                        knobas_app::sources::crud::view(
                            pool,
                            &secrets,
                            &registry,
                            &id,
                            Some(&section.id),
                        )
                    },
                )
                .await;
            }
            if kind == "repo" {
                // One directory down, as a click in the Files tab is.
                time(&what, "entity_view (files: a directory)", runs, || async {
                    Ok::<_, ()>(
                        knobas_app::sources::crud::view(
                            pool,
                            &secrets,
                            &registry,
                            &id,
                            Some("files:docs"),
                        )
                        .await
                        .ok(),
                    )
                })
                .await;
            }
        }
    } else {
        println!("RP-NOTE KNOBAS_GITEA_URL/TOKEN unset: no live Gitea views read");
    }
}

struct Conns(knobas_db::embedded::Connector);
#[async_trait::async_trait]
impl knobas_sync::scheduler::RunConnections for Conns {
    async fn open(&self) -> Result<sqlx::PgConnection, sqlx::Error> {
        self.0.connect().await
    }
}
