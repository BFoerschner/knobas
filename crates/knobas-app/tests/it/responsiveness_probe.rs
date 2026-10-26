//! What the reader waits on (#753): every read the shell makes for the common
//! interactions, timed against a mirror of realistic size.
//!
//! A table, not an assertion. The mirror is seeded into a scratch database --
//! three Jira-shaped sources of `RP_TICKETS` tickets each (default 5,000), one
//! Confluence-shaped source of `RP_PAGES` pages (2,000), one TeamCity-shaped
//! source of `RP_BUILDS` builds (5,000) over `RP_PIPELINES` build
//! configurations (130, the last five of them quiet: no run in the mirror),
//! one Uptime-Kuma-shaped source of `RP_MONITORS` monitors (30) with
//! `RP_SAMPLES` samples between them (600,000), and a link set whose members
//! are every configuration (#2185) -- and, when `KNOBAS_GITEA_URL` and
//! `KNOBAS_GITEA_TOKEN` are set (`eval "$(testenv/seed --env)"`), the seeded
//! Gitea is synced too and its views are read live. Each read is made once
//! cold and `RP_RUNS` times warm (default 7), against the app's own
//! five-connection UI pool, and printed as one `RP` line: interaction, read,
//! cold, warm median, warm max, and the size of the answer as the webview
//! would receive it.
//!
//! Last, the write surfaces (#775): what each dialog or editor waited on
//! before it closed, and what it waits on now, through a scheduler whose
//! adapters take `RP_SEND_MS` (default 300) for every send and read nothing
//! back, so the corpus the reads were taken over is untouched.
//!
//! **Ignored**, and run by `just responsiveness-probe`: a wall-clock reading
//! measures the machine as much as the code, so it is a reading to put in a
//! PR beside the load average, never a gate test.

use std::time::Instant;

use crate::probe_snapshot;
use knobas_app::commands::entity::{
    get_entity_inner, inbox_count_inner, inbox_items_inner, list_entities_inner,
    list_projects_inner, room_suggestions_inner,
};
use knobas_app::commands::search::{
    filter_suggestions_inner, launcher_home_inner, search_inner, searcher, smart_lists_inner,
};
use knobas_app::sources::Registry;
use knobas_core::entity_list::{EntityFilter, EntityOrder};
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

/// A word the body text of three items in four carries (#1849), so a search
/// keystroke has a word that matches most of the mirror. Nothing else in the
/// text does: [`WORDS`] and the synthetic words each match about one row in a
/// hundred. A ticket asking for "`payout` on the 22k probe" reads this one.
const COMMON_WORD: &str = "common";

/// Whether item `i` carries [`COMMON_WORD`]: three in four, a share no
/// fixture size changes and `RP_ITEMS` scaling keeps above half.
fn carries_common_word(i: u64) -> bool {
    !i.is_multiple_of(4)
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

/// How many build configurations a `build` source holds besides its builds
/// (#2185): `RP_PIPELINES`, 130 by default, the sweep's size.
pub(crate) fn pipelines() -> u64 {
    env("RP_PIPELINES", 130)
}

/// The configurations at the end of [`pipelines`] that no build ran (#2185):
/// paused, archived or never built, the members whose state walked every run.
pub(crate) const QUIET_PIPELINES: u64 = 5;

/// A synthetic source of one adapter's shape: `kind` items, `count` of them.
/// A `build` source holds [`pipelines`] configurations too, and its builds
/// are spread over all but the [`QUIET_PIPELINES`]. `read_plans.rs` seeds its
/// mirror with these too.
pub(crate) struct Synthetic {
    pub(crate) descriptor: SourceDescriptor,
    pub(crate) kind: &'static str,
    pub(crate) count: u64,
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
        // A build source's configurations (#2185), TeamCity-shaped: the
        // pipelines its builds run under, in one project.
        if self.kind == "build" {
            for n in 0..pipelines() {
                sink.item(SyncItem {
                    entity: knobas_core::entity::EntityRef::new(id, &format!("buildType:Cfg_{n}")),
                    kind: "build_config".into(),
                    title: format!("Build {n}"),
                    body_text: String::new(),
                    author: None,
                    updated_at: Some(now),
                    payload: serde_json::json!({
                        "id": format!("Cfg_{n}"),
                        "name": format!("Build {n}"),
                        "projectId": "Payments",
                        "projectName": "Payments",
                        "projectPath": [{"id": "Payments", "name": "Payments"}],
                    }),
                    web_url: None,
                    deleted: false,
                })
                .await?;
            }
        }
        let active = pipelines().saturating_sub(QUIET_PIPELINES).max(1);
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
                        "buildTypeId": format!("Cfg_{}", i % active),
                        "buildType": {
                            "id": format!("Cfg_{}", i % active),
                            "name": format!("Build {}", i % active),
                            "projectId": "Payments",
                            "projectName": "Payments",
                        },
                        "number": i.to_string(),
                        "status": if i % 7 == 0 { "FAILURE" } else { "SUCCESS" },
                        "state": "finished",
                        "branchName": if i % 10 == 0 { format!("feature/{}", i % 50) } else { "main".to_owned() },
                    }),
                ),
                // An Uptime Kuma monitor (#2185): its samples are seeded
                // beside it ([`seed_samples`]), not read from the payload.
                "monitor" => (
                    format!("monitor:{i}"),
                    serde_json::json!({
                        "id": i,
                        "name": format!("monitor {i}"),
                        "type": "http",
                        "active": true,
                    }),
                ),
                // A pull request (#939), Gitea-shaped: two in three open,
                // and one in five asks `mara` for a review, so the signals
                // read and `has:` have open pull requests to find.
                "pr" => (
                    format!("tidewater/payout#{i}"),
                    serde_json::json!({
                        "number": i,
                        "state": if i % 3 == 0 { "closed" } else { "open" },
                        "merged": i % 3 == 0,
                        "requested_reviewers": if i % 5 == 0 {
                            serde_json::json!([{"login": "mara"}])
                        } else {
                            serde_json::json!([])
                        },
                    }),
                ),
                // A page carries a storage body of two sections, so the Docs
                // reads (#860) project it as Confluence's would.
                _ => (
                    format!("page:{i}"),
                    serde_json::json!({
                        "id": i,
                        "title": rng.words(4),
                        "space": {"key": "OPS"},
                        "version": {"number": 3, "by": {"username": "mara", "displayName": "Mara Lindqvist"}},
                        "body": {"storage": {"value": format!(
                            "<p>{}</p><h2>{}</h2><p>{}</p><h2>{}</h2><p>{}</p>",
                            rng.words(12), rng.words(2), rng.words(14), rng.words(2), rng.words(14)
                        )}},
                    }),
                ),
            };
            let body_text = if self.kind == "page" {
                // The marked text a docs source writes at sync (#860).
                format!(
                    "{}\n{}\n{}",
                    rng.words(12),
                    knobas_source::docs::indexed::section_line(&rng.words(2)),
                    rng.words(28)
                )
            } else {
                rng.words(40)
            };
            // After every draw, so the rest of the text is the seed it was.
            let body_text = if carries_common_word(i) {
                format!("{body_text} {COMMON_WORD}")
            } else {
                body_text
            };
            sink.item(SyncItem {
                entity: knobas_core::entity::EntityRef::new(id, &entity_key),
                kind: self.kind.into(),
                title: rng.words(6),
                body_text,
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

/// The Tickets view's board in lanes by `field`, as the view reads it
/// (#939): its first ten lanes, eight a column, fifty while words filter.
/// Before #939 this was the whole board, uncapped, split in the webview.
async fn laned_board(
    pool: &PgPool,
    field: &str,
    query: Option<&str>,
) -> Result<knobas_core::mini_board::MiniBoard, knobas_app::IpcError> {
    let by = serde_json::from_value(serde_json::json!(field)).expect("a lane field");
    knobas_app::commands::entity::mini_board_laned(
        pool,
        &knobas_app::sources::Registry::for_process(),
        knobas_app::commands::entity::Room::default(),
        Some(if query.is_some() { 50 } else { 8 }),
        query,
        Some(knobas_core::mini_board::Lanes {
            by,
            first: Some(10),
        }),
    )
    .await
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
        reach: None,
        role: None,
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
    let registry = Registry::for_process();
    let declarations = knobas_app::sources::paths::declared_paths(pool, &registry)
        .await
        .unwrap();
    let scan = filter(sources, &[], ctx, project, EntityOrder::UpdatedDesc);
    time(name, "list_entities (room scan, 200)", runs, || {
        list_entities_inner(pool, &registry, &scan, 200, 0, &declarations)
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
            || list_entities_inner(pool, &registry, tile_filter, 50, 0, &declarations),
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
}

/// The mirror this probe reads: the synthetic sources, then the seeded Gitea
/// when the env says where it is. What a snapshot saves (#781).
/// Confirmed links (#939), dense enough that the signals read and `has:`
/// have a graph to walk: each pull request linked from a ticket and to a
/// build, four in five builds linked from a ticket, and one ticket in three
/// blocking the next. At the default size, 12,000 links.
async fn confirm_links(pool: &PgPool, tickets: u64, builds: u64, prs: u64) {
    let statement = "insert into knobas.link (from_id, to_id, relation, origin, created_by,
                                               confirmed_at)
         select f, t, r, 'manual', 'user', now() from (
           select format('jira%s:JIRA%s-%s', n % 3, n % 3, n / 3) as f,
                  format('git:tidewater/payout#%s', n) as t, 'relates' as r
             from generate_series(0, $3 - 1) n
           union all
           select format('git:tidewater/payout#%s', n), format('ci:build:%s', (n * 3) % $2),
                  'relates'
             from generate_series(0, $3 - 1) n
           union all
           select format('jira%s:JIRA%s-%s', n % 3, n % 3, $1 - 1 - n / 3),
                  format('ci:build:%s', n), 'relates'
             from generate_series(0, ($2 * 4 / 5) - 1) n
           union all
           select format('jira%s:JIRA%s-%s', n % 3, n % 3, n / 3),
                  format('jira%s:JIRA%s-%s', n % 3, n % 3, n / 3 + 1), 'blocks'
             from generate_series(0, 3 * $1 - 4) n where (n / 3) % 3 = 0
         ) l(f, t, r)
         on conflict do nothing";
    let made = sqlx::query(statement)
        .bind(i64::try_from(tickets).unwrap())
        .bind(i64::try_from(builds).unwrap())
        .bind(i64::try_from(prs).unwrap())
        .execute(pool)
        .await
        .unwrap();
    println!("RP-LINKS confirmed={}", made.rows_affected());
}

async fn seed(
    pool: &PgPool,
    tickets: u64,
    pages: u64,
    builds: u64,
    prs: u64,
    gitea: Option<&(String, String)>,
) {
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
    synthetic.push(Synthetic {
        descriptor: SourceDescriptor {
            id: "git".to_owned(),
            ..knobas_source_gitea::descriptor_template()
        },
        kind: "pr",
        count: prs,
    });
    synthetic.push(Synthetic {
        descriptor: SourceDescriptor {
            id: "kuma".to_owned(),
            ..knobas_source_kuma::descriptor_template()
        },
        kind: "monitor",
        count: monitors(),
    });
    for source in &synthetic {
        knobas_app::sources::demo::load_mirror_of(pool, source)
            .await
            .unwrap();
    }
    confirm_links(pool, tickets, builds, prs).await;
    seed_samples(pool).await;
    seed_link_set(pool).await;

    // The seeded Gitea, synced through its real adapter. Its views are the
    // live reads the detail makes.
    if let Some((url, token)) = gitea {
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
        let source = Registry::for_process()
            .build(knobas_source::instance::SourceInstance {
                id: "gitea".into(),
                kind: "gitea".into(),
                display_name: "Gitea".into(),
                base_url: url.clone(),
                auth: Some(AuthMethod::Pat),
                secret: Some(token.clone()),
                account: None,
                config: serde_json::json!({
                    "repos": ["tidewater/payout-service", "tidewater/ledger-api"]
                }),
            })
            .unwrap();
        let synced = knobas_sync::run_once(pool, source.as_ref(), None).await;
        if let Err(error) = synced {
            panic!("{}", gitea_sync_failure(&error));
        }
    }
}

/// How many monitors the Kuma-shaped source holds (#2185): `RP_MONITORS`.
fn monitors() -> u64 {
    env("RP_MONITORS", 30)
}

/// How many samples the monitors hold between them (#2185): `RP_SAMPLES`,
/// 600,000 at the full size and scaled with `RP_ITEMS` -- at 30 monitors,
/// two weeks of one a minute each, a sixth of the 90-day retention.
fn samples() -> u64 {
    env("RP_SAMPLES", 600_000 * env("RP_ITEMS", 22_000) / 22_000)
}

/// [`samples`] samples, spread evenly over the monitors, one a minute back
/// from now: one in fifty `down`, the rest `up`. Every monitor's newest
/// sample is `up` but the first's, which is `down`, so the reconcile has an
/// alert to keep open and many monitors to leave alone.
async fn seed_samples(pool: &PgPool) {
    let made = sqlx::query(
        "insert into knobas.monitor_sample (entity_id, taken_at, state, response_time_ms)
         select 'kuma:monitor:' || m, now() - make_interval(mins => n + 1),
                case when n % 50 = 49 or (m = 0 and n = 0) then 'down' else 'up' end, 120
           from generate_series(0, $1 - 1) m
          cross join generate_series(0, $2 / $1 - 1) n",
    )
    .bind(i64::try_from(monitors()).unwrap())
    .bind(i64::try_from(samples()).unwrap())
    .execute(pool)
    .await
    .unwrap();
    println!("RP-SAMPLES monitors={} samples={}", monitors(), made.rows_affected());
}

/// The link set the Link sets reads open (#2185): every configuration a
/// pipeline member, the quiet ones included, one pipeline pattern, and a
/// project rule that puts it on every ticket of `JIRA00`.
async fn seed_link_set(pool: &PgPool) {
    let members = (0..pipelines())
        .map(|n| knobas_app::link_sets::LinkSetMember {
            kind: "pipeline".to_owned(),
            entity_id: format!("ci:buildType:Cfg_{n}"),
        })
        .collect();
    knobas_app::link_sets::save(
        pool,
        &Registry::for_process(),
        knobas_app::link_sets::LinkSetDraft {
            id: None,
            name: "Payments CI".to_owned(),
            description: String::new(),
            members,
            patterns: vec![knobas_app::link_sets::LinkSetPattern {
                kind: "pipeline".to_owned(),
                query: "project:Payments".to_owned(),
            }],
            rules: vec![knobas_app::link_sets::LinkSetRule {
                kind: "project".to_owned(),
                value: "jira0:JIRA00".to_owned(),
            }],
        },
    )
    .await
    .unwrap();
}

/// What the seeding sync's failure says. A token the seeded Gitea no longer
/// accepts is the fixture's refusal, and `./seed` is the remedy (#1377); any
/// other failure is quoted whole.
fn gitea_sync_failure(error: &knobas_sync::SyncError) -> String {
    match error {
        knobas_sync::SyncError::Source(SourceError::Unauthorized { status }) => format!(
            "the fixture's Gitea token is refused (HTTP {}): run ./seed from testenv/ \
             to mint a new one, then retry",
            status.map_or_else(|| "unknown".to_owned(), |s| s.to_string())
        ),
        other => format!("seeding sync of the Gitea fixture failed: {other:?}"),
    }
}

#[test]
fn a_refused_gitea_token_names_the_seed() {
    let refused = knobas_sync::SyncError::Source(SourceError::Unauthorized { status: Some(401) });
    let said = gitea_sync_failure(&refused);
    assert!(said.contains("token is refused"), "{said}");
    assert!(said.contains("401") && said.contains("./seed"), "{said}");
    let other = knobas_sync::SyncError::Source(SourceError::Unreachable("down".into()));
    assert!(!gitea_sync_failure(&other).contains("./seed"));
}

/// `RP_ANALYZED=0` reads against tables the planner has no statistics for:
/// autovacuum is switched off on the scratch database's tables, so they stay
/// never analyzed -- the state of a fresh install for about a minute after its
/// first sync, which is where a plan that leans on estimates shows (#767,
/// #768).
async fn tables_of_the_mirror(pool: &PgPool) -> Vec<String> {
    let tables: Vec<(String, String)> = sqlx::query_as(
        "select n.nspname::text, c.relname::text from pg_class c
           join pg_namespace n on n.oid = c.relnamespace
          where c.relkind = 'r' and n.nspname in ('knobas', 'sync')",
    )
    .fetch_all(pool)
    .await
    .unwrap();
    tables
        .into_iter()
        .map(|(schema, table)| format!("{schema}.{table}"))
        .collect()
}

async fn start_at(root: &std::path::Path) -> knobas_db::embedded::EmbeddedDb {
    knobas_db::embedded::EmbeddedDb::start(knobas_db::embedded::DbConfig {
        root_dir: root.to_path_buf(),
        existing_url: None,
    })
    .await
    .expect("start the probe's postgres")
}

/// Build the mirror from nothing and save both snapshots: never analyzed
/// (autovacuum off on every table, so it stays so), then analyzed (autovacuum
/// back to its default, `analyze` run).
async fn build_snapshots(
    tickets: u64,
    pages: u64,
    builds: u64,
    prs: u64,
    gitea: Option<&(String, String)>,
    never: &std::path::Path,
    analyzed: &std::path::Path,
) {
    let root = probe_snapshot::root().join(format!("build-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let db = start_at(&root).await;
    let pool = db.connector().app_pool().await.unwrap();
    knobas_db::migrate::run(&pool).await.unwrap();
    searcher(&pool, &Registry::for_process()).await.unwrap();
    for table in tables_of_the_mirror(&pool).await {
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "alter table {table} set (autovacuum_enabled = false)"
        )))
        .execute(&pool)
        .await
        .unwrap();
    }
    seed(&pool, tickets, pages, builds, prs, gitea).await;
    pool.close().await;
    db.stop().await.unwrap();
    probe_snapshot::save(&root, never);

    let db = start_at(&root).await;
    let pool = db.connector().app_pool().await.unwrap();
    for table in tables_of_the_mirror(&pool).await {
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "alter table {table} reset (autovacuum_enabled)"
        )))
        .execute(&pool)
        .await
        .unwrap();
    }
    sqlx::query("analyze").execute(&pool).await.unwrap();
    pool.close().await;
    db.stop().await.unwrap();
    probe_snapshot::save(&root, analyzed);
    let _ = std::fs::remove_dir_all(&root);
}

/// The suggestion pass, run once as it would be after a sync, timed, with
/// what it wrote: whether it finishes in both statistics states is what #770
/// measured (before it, the pass died in its first exact-key rule on tables
/// never analyzed). Analyzed before the pass as well as after it: a real
/// install's autovacuum analyzes within about a minute of the first sync, so
/// the pass plans against statistics. Again after, so the reads plan against
/// the links the pass wrote.
async fn run_pass(connector: &knobas_db::embedded::Connector, pool: &PgPool, analyzed: bool) {
    let pass = knobas_app::suggestion_pass::SuggestionPass::new(
        std::sync::Arc::new(Conns(connector.clone())),
        knobas_app::suggestion_pass::PASS_STATEMENT_TIMEOUT,
    );
    let passed = Instant::now();
    let outcome = pass.run().await;
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
}

/// Build the after-pass snapshot `to` from the base snapshot `from` (#793):
/// restore it, run the pass, stop, save.
async fn build_passed(from: &std::path::Path, to: &std::path::Path, analyzed: bool) {
    let root = probe_snapshot::root().join(format!("pass-{}", std::process::id()));
    probe_snapshot::restore(from, &root);
    let db = start_at(&root).await;
    let connector = db.connector();
    let pool = connector.app_pool().await.unwrap();
    searcher(&pool, &Registry::for_process()).await.unwrap();
    run_pass(&connector, &pool, analyzed).await;
    pool.close().await;
    db.stop().await.unwrap();
    probe_snapshot::save(&root, to);
    let _ = std::fs::remove_dir_all(&root);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "a wall-clock reading; run by `just responsiveness-probe`"]
async fn what_the_reader_waits_on() {
    // The full mirror is 22,000 items; `RP_ITEMS` scales the three sizes
    // below it, each still settable on its own. `RP_ITEMS=2500` is the size to
    // iterate on, with a snapshot of its own (#781).
    let all = env("RP_ITEMS", 22_000);
    let scaled = |full: u64| full * all / 22_000;
    let tickets = env("RP_TICKETS", scaled(5_000));
    let pages = env("RP_PAGES", scaled(2_000));
    let builds = env("RP_BUILDS", scaled(5_000));
    // Pull requests and the confirmed links around them (#939): beyond the
    // 22,000, which stays what `RP_ITEMS` names.
    let prs = env("RP_PRS", scaled(1_500));
    let runs = env("RP_RUNS", 7);
    // `RP_ANALYZED=1` reads against tables the planner has statistics for;
    // otherwise they are never analyzed (see `tables_of_the_mirror`).
    let analyzed = env("RP_ANALYZED", 0) == 1;
    let gitea_env = match (
        std::env::var("KNOBAS_GITEA_URL"),
        std::env::var("KNOBAS_GITEA_TOKEN"),
    ) {
        (Ok(url), Ok(token)) => Some((url, token)),
        _ => None,
    };
    let gitea = gitea_env.is_some();

    // The built database is a snapshot: a run restores it instead of seeding
    // again. `RP_FRESH=1` rebuilds it; a changed key rebuilds on its own.
    let snapshots = probe_snapshot::root();
    let migrations = probe_snapshot::migrations_hash(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../knobas-db/migrations"),
    );
    let pass_code = probe_snapshot::pass_code_hash(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../knobas-core/src"),
        &migrations,
    );
    for gone in probe_snapshot::sweep(
        &snapshots,
        &migrations,
        &pass_code,
        std::time::SystemTime::now(),
        probe_snapshot::MAX_AGE,
    ) {
        println!("RP-SNAPSHOT deleted stale {}", gone.display());
    }
    let key = probe_snapshot::seed_hash(&[
        ("tickets", tickets.to_string()),
        ("pages", pages.to_string()),
        ("builds", builds.to_string()),
        // Pull requests and confirmed links (#939): a seed without them is
        // another seed.
        ("prs", prs.to_string()),
        ("links", "939".to_owned()),
        ("synthetic", synthetic().to_string()),
        // The common word in three bodies of four (#1849).
        ("common_word", COMMON_WORD.to_owned()),
        // A page's body and marks (#860): a seed of the shape before them is
        // another seed.
        ("page_shape", "storage-and-marks".to_owned()),
        // Configurations, quiet ones, monitors with samples and a link set
        // (#2185).
        ("pipelines", format!("{}-{QUIET_PIPELINES}", pipelines())),
        ("monitors", format!("{}x{}", monitors(), samples())),
        ("link_set", "2185".to_owned()),
        (
            "gitea",
            gitea_env
                .as_ref()
                .map_or(String::new(), |(url, _)| url.clone()),
        ),
    ]);
    let never = snapshots.join(probe_snapshot::name(
        &migrations,
        &key,
        probe_snapshot::State::NeverAnalyzed,
    ));
    let analyzed_copy = snapshots.join(probe_snapshot::name(
        &migrations,
        &key,
        probe_snapshot::State::Analyzed,
    ));
    let base = if analyzed { &analyzed_copy } else { &never };
    // `RP_PASS=skip` reads against an after-pass snapshot (#793), keyed on the
    // pass's code as well; `RP_PASS=run` (the default) runs and times the pass.
    let skip_pass = match std::env::var("RP_PASS").as_deref() {
        Ok("skip") => true,
        Ok("run") | Err(_) => false,
        Ok(other) => panic!("RP_PASS is `run` or `skip`, not `{other}`"),
    };
    let passed_copy = snapshots.join(probe_snapshot::passed_name(
        &migrations,
        &key,
        &pass_code,
        if analyzed {
            probe_snapshot::State::Analyzed
        } else {
            probe_snapshot::State::NeverAnalyzed
        },
    ));
    let wanted = if skip_pass { &passed_copy } else { base };

    let seeded = Instant::now();
    let fresh = env("RP_FRESH", 0) == 1;
    let how = if fresh || !probe_snapshot::exists(base) {
        build_snapshots(
            tickets,
            pages,
            builds,
            prs,
            gitea_env.as_ref(),
            &never,
            &analyzed_copy,
        )
        .await;
        "built"
    } else {
        "restored"
    };
    let how = if skip_pass && (fresh || !probe_snapshot::exists(wanted)) {
        build_passed(base, wanted, analyzed).await;
        "built"
    } else {
        how
    };
    println!(
        "RP-SNAPSHOT {how} {} size_mb={} age_s={}",
        wanted.file_name().unwrap().to_string_lossy(),
        probe_snapshot::size(wanted) / 1_000_000,
        probe_snapshot::age(wanted, std::time::SystemTime::now()).as_secs()
    );
    let run_root = snapshots.join(format!("run-{}", std::process::id()));
    probe_snapshot::restore(wanted, &run_root);
    let db = start_at(&run_root).await;
    let connector = db.connector();
    let ui = connector.app_pool().await.unwrap();
    let pool = &ui;
    searcher(pool, &Registry::for_process()).await.unwrap();

    // The seeded Gitea's views are read live, through the token.
    let secrets: std::sync::Arc<dyn SecretStore> = std::sync::Arc::new(MemoryStore::new());
    if let Some((_, token)) = &gitea_env {
        secrets
            .put(
                &knobas_secrets::KeychainAccount::source("gitea"),
                &Secret::just(AuthMethod::Pat, token.clone()),
            )
            .unwrap();
    }
    let items: i64 = sqlx::query_scalar("select count(*) from sync.item")
        .fetch_one(pool)
        .await
        .unwrap();
    println!(
        "RP-SEED items={items} tickets/source={tickets} pages={pages} builds={builds} prs={prs} gitea={gitea} snapshot={how} in {} s",
        seeded.elapsed().as_secs()
    );
    if skip_pass {
        println!(
            "RP-PASS skipped (snapshot {})",
            wanted.file_name().unwrap().to_string_lossy()
        );
    } else {
        run_pass(&connector, pool, analyzed).await;
    }
    println!("RP-STATS analyzed={analyzed}");
    // A stored context: the promoted epic `jira0:JIRA0-3` and its children.
    let promoted = knobas_core::context::promote(
        pool,
        &knobas_core::entity::EntityRef::parse("jira0:JIRA0-3").unwrap(),
        &knobas_core::payload::Declarations::empty(),
    )
    .await
    .unwrap()
    .expect("JIRA0-3 promotes");
    let ctx = promoted.context.id.clone();
    println!("RP | interaction | read | cold ms | warm median ms | warm max ms | bytes");

    // -- opening the app -----------------------------------------------------
    let registry = Registry::for_process();
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
    time(boot, "list_projects", runs, || {
        list_projects_inner(pool, &registry)
    })
    .await;
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

    // -- the Tickets view's filter (#810, #897) ------------------------------
    // What a keystroke in the filter reads: the board at 50 a column while the
    // filter has words (R-3.4, #1024), every match still counted. A board in
    // lanes still reads every match, so that row stays beside it. `a` is one
    // broad letter (every key here holds one), `jira0-123` a narrow one, and
    // `zzqx` a word nothing holds, which makes every row's assignee read run.
    // A word left out and an `or` (#1889): `-zzqx` keeps every row and reads
    // every row's assignee and parents to be sure, `-a` leaves out every row
    // on its key, and `a or zzqx` reads the second alternative for no row.
    let tickets_view = "tickets view: filter";
    time(
        tickets_view,
        "mini_board (no words, 8 a column)",
        runs,
        || {
            knobas_app::commands::entity::mini_board_inner(
                pool,
                &registry,
                None,
                None,
                None,
                Some(8),
                None,
            )
        },
    )
    .await;
    for word in ["a", "jira0-123", "zzqx", "-zzqx", "-a", "a or zzqx"] {
        for (cap, label) in [(Some(50), "50 a column"), (None, "every match, lanes")] {
            time(
                tickets_view,
                &format!("mini_board (typed `{word}`, {label})"),
                runs,
                || {
                    knobas_app::commands::entity::mini_board_inner(
                        pool,
                        &registry,
                        None,
                        None,
                        None,
                        cap,
                        Some(word),
                    )
                },
            )
            .await;
        }
    }

    // -- lanes and `has:` over confirmed links (#939, #1098) -----------------
    // A laned board, by three fields: the epic (a lane of about twenty
    // tickets each), the assignee and the priority (one lane each here).
    let lanes = "tickets view: lanes";
    for field in ["epic", "assignee", "priority"] {
        for word in [None, Some("a")] {
            let typed = word.map_or("no words".to_owned(), |word| format!("typed `{word}`"));
            time(
                lanes,
                &format!("mini_board (lanes by {field}, {typed})"),
                runs,
                || laned_board(pool, field, word),
            )
            .await;
        }
    }
    let has = "tickets view: has:";
    for value in ["failing-build", "blocker", "open-pr", "review-for-me"] {
        let raw = format!("has:{value}");
        time(has, &format!("search {raw}"), runs, || {
            search_inner(
                pool,
                &registry,
                knobas_search::SearchQuery {
                    raw: raw.clone(),
                    limit: 50,
                    filters: Default::default(),
                },
            )
        })
        .await;
        time(
            has,
            &format!("mini_board ({raw}, 8 a column)"),
            runs,
            || {
                knobas_app::commands::entity::mini_board_inner(
                    pool,
                    &registry,
                    None,
                    None,
                    None,
                    Some(8),
                    Some(&raw),
                )
            },
        )
        .await;
    }

    // -- opening a detail ----------------------------------------------------
    let declarations = knobas_app::sources::paths::declared_paths(pool, &registry)
        .await
        .unwrap();
    let _ = declarations;
    for (what, id) in [
        ("detail: ticket", "jira0:JIRA0-42"),
        ("detail: build", "ci:build:42"),
        ("detail: page", "wiki:page:42"),
    ] {
        time(what, "get_entity", runs, || {
            get_entity_inner(pool, &registry, id)
        })
        .await;
        time(what, "entity_checkout", runs, || async {
            Ok::<_, ()>(knobas_app::checkout::view(pool, id).await.ok())
        })
        .await;
    }

    // -- the launcher --------------------------------------------------------
    time("open launcher", "launcher_home", runs, || {
        launcher_home_inner(pool, &registry)
    })
    .await;
    time("open launcher", "smart_lists", runs, || {
        smart_lists_inner(pool, &registry)
    })
    .await;
    // Opening the box on a filter token reads the kinds, the sources and the
    // parents once (#601); each keystroke after it only the parents (#768).
    time(
        "open a filter token",
        "filter_suggestions (kinds, sources, parents)",
        runs,
        || filter_suggestions_inner(pool, &registry, "", false),
    )
    .await;
    // The first `under:` keystroke asks with nothing typed yet, and each one
    // after it with the value so far (#761).
    for under in ["", "j", "jira0-1", "pay"] {
        time(
            "type a filter token",
            &format!("filter_suggestions (under:{under})"),
            runs,
            || filter_suggestions_inner(pool, &registry, under, true),
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
                &registry,
                knobas_search::SearchQuery {
                    raw: raw.into(),
                    limit: 50,
                    filters: Default::default(),
                },
            )
        })
        .await;
    }
    // The word that matches most of the mirror (#1849): the ranking and the
    // count run over every match, not the few a prefix of `payment` reaches.
    let common = knobas_search::SearchQuery {
        raw: COMMON_WORD.into(),
        limit: 50,
        filters: Default::default(),
    };
    let matched = search_inner(pool, &registry, common.clone())
        .await
        .unwrap_or_else(|e| panic!("search {COMMON_WORD:?}: {e:?}"))
        .total;
    time(
        "search keystroke",
        &format!("search {COMMON_WORD:?} (matches {matched} of {items})"),
        runs,
        || search_inner(pool, &registry, common.clone()),
    )
    .await;
    println!("RP-MATCHES search {COMMON_WORD:?} total={matched} items={items}");
    assert!(
        i64::from(matched) * 2 >= items,
        "{COMMON_WORD:?} matches {matched} of {items} items, under half"
    );

    // -- Docs (#859 to #862) -------------------------------------------------
    time("open Docs", "docs_tree", runs, || {
        knobas_app::commands::entity::docs_tree_inner(pool, &registry)
    })
    .await;
    time("open Docs", "docs_page", runs, || {
        knobas_app::commands::entity::docs_page_inner(pool, &registry, "wiki:page:42")
    })
    .await;
    let ops = knobas_app::docs::DocsScope {
        source_id: "wiki".into(),
        key: "OPS".into(),
    };
    for raw in [
        "p",
        "pa",
        "pay",
        "payment",
        "payment retry",
        "\"payment retry\" -cron",
    ] {
        time(
            "docs search keystroke",
            &format!("docs_search {raw:?}"),
            runs,
            || knobas_app::commands::entity::docs_search_inner(pool, &registry, raw, None),
        )
        .await;
        time(
            "docs search keystroke",
            &format!("docs_search {raw:?} in OPS"),
            runs,
            || knobas_app::commands::entity::docs_search_inner(pool, &registry, raw, Some(&ops)),
        )
        .await;
    }

    // -- Builds, link sets and the alert reconcile (#2185) --------------------
    // The reads that walked every run of a pipeline's source through the
    // declared `pipeline` path, and the reconcile that sorted every sample.
    let builds_view = "open Builds";
    time(builds_view, "pipelines (Builds list)", runs, || {
        knobas_app::commands::entity::pipelines_inner(pool, &registry)
    })
    .await;
    for (which, id) in [
        ("active", "ci:buildType:Cfg_7".to_owned()),
        (
            "quiet",
            format!("ci:buildType:Cfg_{}", pipelines() - 1),
        ),
    ] {
        time(
            "open a pipeline",
            &format!("pipeline_runs ({which}, 100)"),
            runs,
            || knobas_app::commands::entity::pipeline_runs_inner(pool, &registry, &id, None),
        )
        .await;
    }
    // A ticket of `JIRA00`, so the seeded link set's project rule puts its
    // members and its pattern's hits in the job's reach.
    let job = "jira0:JIRA0-40";
    time("Builds: This job", "job_pipelines", runs, || {
        knobas_app::commands::entity::job_pipelines_inner(pool, &registry, job)
    })
    .await;
    time("job with a pattern set", "reach", runs, || {
        knobas_core::reach::reach(pool, job)
    })
    .await;
    time("open Dependencies", "dependencies", runs, || {
        knobas_app::assets::dependencies::read(pool, &registry)
    })
    .await;
    time("open Link sets", "link_sets (list)", runs, || {
        knobas_app::link_sets::list(pool, &registry)
    })
    .await;
    time("open Link sets", "link_set_hints", runs, || {
        knobas_app::link_sets::hints(pool, &registry)
    })
    .await;
    for query in ["project:Payments", "name:Build_1*"] {
        let pattern = knobas_app::link_sets::LinkSetPattern {
            kind: "pipeline".to_owned(),
            query: query.to_owned(),
        };
        time(
            "type a link-set pattern",
            &format!("preview_link_set_pattern {query:?}"),
            runs,
            || knobas_app::link_sets::preview_pattern(pool, &registry, &pattern),
        )
        .await;
    }
    // Inside a transaction rolled back, so every run reconciles the same
    // samples and leaves no alert behind.
    time(
        &format!("monitor sync ({} samples)", samples()),
        "alerts::reconcile",
        runs,
        || async {
            let mut tx = pool.begin().await?;
            knobas_sync::alerts::reconcile(&mut tx, "kuma").await?;
            tx.rollback().await?;
            Ok::<_, sqlx::Error>(())
        },
    )
    .await;
    time(
        "paste a URL",
        "resolve_url",
        runs,
        || {
            knobas_app::commands::entity::resolve_url_inner(
                pool,
                &registry,
                "https://jira.example.com/browse/JIRA0-42",
            )
        },
    )
    .await;

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
        // The views build an adapter, so they take the registry shared (#1705).
        let shared = crate::support::shared(registry.clone());
        for kind in ["repo", "pr", "branch", "commit"] {
            let Some(id) = pick(kind).await else {
                println!("RP-NOTE no gitea {kind} mirrored");
                continue;
            };
            let what = format!("gitea {kind}");
            time(&what, "get_entity", runs, || {
                get_entity_inner(pool, &registry, &id)
            })
            .await;
            let view = time(&what, "entity_view (whole)", runs, || {
                knobas_app::sources::crud::view(pool, &secrets, &shared, &id, None)
            })
            .await;
            let _ = view;
            let whole = knobas_app::sources::crud::view(pool, &secrets, &shared, &id, None)
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
                            &shared,
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
                            &shared,
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

    // -- a full sync, and the smart lists right after it (#2185) -------------
    // One Jira source synced whole again, every row rewritten as a full sync
    // rewrites it, then the launcher's smart lists, cold, on what it left.
    let again = Synthetic {
        descriptor: SourceDescriptor {
            id: "jira0".to_owned(),
            ..knobas_source_jira::descriptor_template()
        },
        kind: "ticket",
        count: tickets,
    };
    time(
        &format!("full sync ({tickets} tickets)"),
        "run_once",
        runs.min(2),
        || knobas_sync::run_once(pool, &again, None),
    )
    .await;
    time("after a sync", "smart_lists", runs, || {
        smart_lists_inner(pool, &registry)
    })
    .await;

    // Last: the writes queue sends and trigger post-write runs, and nothing
    // read above should be measured over what they leave.
    writes(pool, &connector, runs, tickets).await;

    ui.close().await;
    db.stop().await.unwrap();
    let _ = std::fs::remove_dir_all(&run_root);
}

/// How long one simulated send takes: the remote call the queue makes behind
/// the answer. #752 measured 582 ms for a comment against the live Gitea.
fn send_ms() -> u64 {
    env("RP_SEND_MS", 300)
}

/// The adapters the write section sends through: the built-in descriptors,
/// so `submit_write` checks the ops a real source declares, over a source
/// whose `write` takes [`send_ms`] and whose read refuses -- so the post-write
/// run cannot touch the corpus.
struct Sending;

impl AdapterRegistry for Sending {
    fn descriptors(&self) -> Vec<SourceDescriptor> {
        Registry::for_process().descriptors()
    }

    fn build(
        &self,
        instance: knobas_source::instance::SourceInstance,
    ) -> Result<Box<dyn Source>, SourceError> {
        let template = Registry::for_process()
            .descriptors()
            .into_iter()
            .find(|d| d.adapter_kind == instance.kind)
            .ok_or_else(|| SourceError::protocol(format!("no adapter {}", instance.kind)))?;
        Ok(Box::new(Sends {
            descriptor: SourceDescriptor {
                id: instance.id,
                ..template
            },
        }))
    }
}

struct Sends {
    descriptor: SourceDescriptor,
}

#[async_trait::async_trait]
impl Source for Sends {
    async fn write(
        &self,
        op: knobas_source::WriteOp,
    ) -> Result<knobas_source::WriteReceipt, SourceError> {
        tokio::time::sleep(std::time::Duration::from_millis(send_ms())).await;
        Ok(match op {
            knobas_source::WriteOp::LogWork { .. } => knobas_source::WriteReceipt::id("1"),
            _ => knobas_source::WriteReceipt::none(),
        })
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
        _sink: &mut (dyn Sink + Send),
    ) -> Result<Cursor, SourceError> {
        Err(SourceError::protocol(
            "the probe's writes read nothing back".to_owned(),
        ))
    }
}

struct Silent;
impl knobas_sync::scheduler::SyncEvents for Silent {
    fn sync_state(&self, _status: knobas_sync::SourceSyncStatus) {}
    fn source_health(&self, _health: knobas_sync::CredentialHealth) {}
    fn activity_new(&self, _row: knobas_core::activity::ActivityRow) {}
}

/// The first ticket number "write: start work" begins: a fifth of the way into
/// each Jira-shaped source, so 1,000 of the default 5,000 and a number the
/// mirror has at any smaller size (#798).
fn first_start_work_ticket(tickets: u64) -> u64 {
    tickets / 5
}

/// Every ticket number the probe names outright (`JIRA0-42`, the epic
/// `JIRA0-3`, the linked-by-hand `JIRA1-100..`, the start-work run), for a
/// mirror of `tickets` per source and `runs` warm runs.
fn fixed_ticket_numbers(tickets: u64, runs: u64) -> Vec<u64> {
    let start = first_start_work_ticket(tickets);
    vec![3, 42, 100, 100 + runs, start, start + 2 * (runs + 1) - 1]
}

/// What each write surface waits on before its dialog closes (#775): the
/// commands it awaited before, and what it awaits now, on the same corpus.
/// The send is [`send_ms`] of sleep, so "before" rows carry that much of a
/// remote call; the live figures for a real one are #752's.
async fn writes(
    pool: &PgPool,
    connector: &knobas_db::embedded::Connector,
    runs: u64,
    tickets: u64,
) {
    use knobas_app::sources::write_queue::{submit, submit_in_background};
    let secrets: std::sync::Arc<dyn SecretStore> = std::sync::Arc::new(MemoryStore::new());
    for id in ["jira0", "ci", "wiki", "forge"] {
        secrets
            .put(
                &knobas_secrets::KeychainAccount::source(id),
                &Secret::just(AuthMethod::Pat, "probe".to_owned()),
            )
            .unwrap();
    }
    let registry: std::sync::Arc<dyn AdapterRegistry> = std::sync::Arc::new(Sending);
    let scheduler =
        knobas_sync::scheduler::Scheduler::start(knobas_sync::scheduler::SchedulerDeps {
            pool: pool.clone(),
            connections: std::sync::Arc::new(Conns(connector.clone())),
            registry: std::sync::Arc::clone(&registry),
            secrets: secrets.clone(),
            events: std::sync::Arc::new(Silent),
            // The tick held off: only the sends the writes make are measured.
            timing: knobas_sync::scheduler::SchedulerTiming {
                startup_delay: std::time::Duration::from_secs(3600),
                tick: std::time::Duration::from_secs(3600),
                ..Default::default()
            },
            quit: knobas_sync::scheduler::Quit::default(),
        })
        .await
        .unwrap();
    let state = knobas_app::sources::SourcesState {
        pool: pool.clone(),
        scheduler,
        secrets: secrets.clone(),
        registry: std::sync::Arc::clone(&registry),
    };
    let state = &state;
    println!("RP-NOTE writes: the simulated send takes {} ms", send_ms());

    const TICKET: &str = "jira0:JIRA0-42";
    let comment = || serde_json::json!({"Comment": {"entity": TICKET, "body": "probe"}});
    let read_back = || async {
        get_entity_inner(pool, &Registry::for_process(), TICKET)
            .await
            .map_err(said)?;
        knobas_app::sources::crud::view(pool, &secrets, &registry, TICKET, None)
            .await
            .map_err(said)
            .map(|_| ())
    };
    // -- a view action (comment, merge, transition): ActionDialog, page comment, status select
    time(
        "write: view action",
        "before #752: submit (send awaited) + get_entity + entity_view",
        runs,
        || async {
            submit(state, comment()).await.map_err(said)?;
            read_back().await
        },
    )
    .await;
    time(
        "write: view action",
        "now: submit_write (answer on queue)",
        runs,
        || submit_in_background(state, comment()),
    )
    .await;

    // -- a setting edited from the view: closed on its settle read before #775
    let setting = || serde_json::json!({"SetParameter": {"entity": "ci:build:42", "name": "env.X", "value": "1", "base": null}});
    time(
        "write: settings row",
        "before #775: submit_write + pending_writes + get_entity + entity_view",
        runs,
        || async {
            submit_in_background(state, setting()).await.map_err(said)?;
            knobas_core::write_queue::open(pool).await.map_err(said)?;
            get_entity_inner(pool, &Registry::for_process(), "ci:build:42")
                .await
                .map_err(said)?;
            knobas_app::sources::crud::view(pool, &secrets, &registry, "ci:build:42", None)
                .await
                .map_err(said)
                .map(|_| ())
        },
    )
    .await;
    time(
        "write: settings row",
        "now: submit_write (answer on queue)",
        runs,
        || submit_in_background(state, setting()),
    )
    .await;

    // -- the worklog draft: one fresh block on the ticket for every call
    let day = chrono::NaiveDate::from_ymd_opt(2026, 9, 3).unwrap();
    let minute = std::sync::atomic::AtomicI64::new(0);
    let fresh = || async {
        let k = minute.fetch_add(2, std::sync::atomic::Ordering::Relaxed);
        let from = day.and_hms_opt(0, 0, 0).unwrap().and_utc() + chrono::Duration::minutes(k);
        sqlx::query(
            "insert into knobas.block (started_at, ended_at, entity_id, kind)
             values ($1, $2, $3, 'manual')",
        )
        .bind(from)
        .bind(from + chrono::Duration::minutes(1))
        .bind(TICKET)
        .execute(pool)
        .await
        .unwrap();
        from
    };
    time(
        "write: worklog draft",
        "before #775: log_work (send awaited), + one block insert",
        runs,
        || async {
            let from = fresh().await;
            knobas_app::time::worklog::log(state, TICKET, day, 0, from, 60, "").await
        },
    )
    .await;
    time(
        "write: worklog draft",
        "now: log_work (answer on queue), + one block insert",
        runs,
        || async {
            let from = fresh().await;
            knobas_app::time::worklog::log_in_background(state, TICKET, day, 0, from, 60, "").await
        },
    )
    .await;

    // -- links by hand: the dialog closes on the insert; the row on the re-read
    let n = std::sync::atomic::AtomicU64::new(100);
    time(
        "write: link by hand",
        "create_link + get_entity",
        runs,
        || async {
            let to = format!(
                "jira1:JIRA1-{}",
                n.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            );
            knobas_app::commands::entity::create_link_inner(pool, TICKET, &to, None, None).await?;
            get_entity_inner(pool, &Registry::for_process(), TICKET).await?;
            Ok::<_, knobas_app::IpcError>(())
        },
    )
    .await;
    let links: Vec<String> = sqlx::query_scalar(
        "select id::text from knobas.link where from_id = $1 and deleted_at is null",
    )
    .bind(TICKET)
    .fetch_all(pool)
    .await
    .unwrap();
    let next = std::sync::atomic::AtomicUsize::new(0);
    time("write: unlink", "unlink + get_entity", runs, || async {
        let id = &links[next.fetch_add(1, std::sync::atomic::Ordering::Relaxed) % links.len()];
        knobas_app::commands::entity::unlink_inner(pool, id).await?;
        get_entity_inner(pool, &Registry::for_process(), TICKET).await?;
        Ok::<_, knobas_app::IpcError>(())
    })
    .await;

    // -- notes
    let note =
        knobas_app::commands::entity::create_note_inner(pool, Some("probe"), Some("body"), &[])
            .await
            .unwrap();
    let note_id = note.note.id.clone();
    time("write: note", "create_note", runs, || {
        knobas_app::commands::entity::create_note_inner(pool, Some("probe"), Some("body"), &[])
    })
    .await;
    time("write: note", "save_note (autosave)", runs, || {
        knobas_app::commands::entity::save_note_inner(pool, &note_id, "probe", "a longer body")
    })
    .await;

    // -- timers
    time("write: timer", "start_timer + stop_timer", runs, || async {
        knobas_app::time::start(
            pool,
            knobas_app::time::TimerTarget::Entity {
                entity_id: TICKET.to_owned(),
            },
            None,
        )
        .await?;
        knobas_app::time::stop(pool).await.map(|_| ())
    })
    .await;

    // -- the inbox: an answer and the re-read it used to wait on
    let registry_builtin = Registry::for_process();
    let now = chrono::Utc::now();
    let reread = || async {
        inbox_items_inner(
            pool,
            &registry_builtin,
            now,
            knobas_core::inbox::Shelf::Stream,
        )
        .await?;
        inbox_items_inner(
            pool,
            &registry_builtin,
            now,
            knobas_core::inbox::Shelf::Snoozed,
        )
        .await?;
        inbox_count_inner(pool, &registry_builtin, now).await
    };
    time(
        "write: inbox answer",
        "the re-read an answer waited on (3 reads)",
        runs,
        reread,
    )
    .await;

    // -- #779: the three surfaces that drew the address of what they made.
    // They close on the queue's answer now; "before" is the awaited path,
    // which is still the battery's.
    let mirrored = |id: &'static str,
                    kind: &'static str,
                    title: &'static str,
                    payload: serde_json::Value| async move {
        let entity = knobas_core::entity::EntityRef::parse(id).unwrap();
        sqlx::query("insert into knobas.entity (id, kind, title) values ($1, $2, $3) on conflict do nothing")
            .bind(id)
            .bind(kind)
            .bind(title)
            .execute(pool)
            .await
            .unwrap();
        sqlx::query(
            "insert into sync.item (entity_id, source_id, kind, title, body_text, payload)
             values ($1, $2, $3, $4, '', $5) on conflict do nothing",
        )
        .bind(id)
        .bind(&entity.namespace)
        .bind(kind)
        .bind(title)
        .bind(payload)
        .execute(pool)
        .await
        .unwrap();
    };
    // The standup publish: one fresh date per call, so each is a first publish.
    mirrored(
        "wiki:probe-parent",
        "page",
        "Standup protocols",
        serde_json::json!({"space": {"key": "ENG"}}),
    )
    .await;
    let target = knobas_app::protocol::PublishTarget {
        source_id: "wiki".to_owned(),
        parent: "wiki:probe-parent".to_owned(),
    };
    let days = std::sync::atomic::AtomicI64::new(0);
    let next_day =
        || day + chrono::Duration::days(days.fetch_add(1, std::sync::atomic::Ordering::Relaxed));
    time(
        "write: standup publish",
        "before #779: publish_standup_protocol (send + sync awaited)",
        runs,
        || {
            knobas_app::commands::entity::publish_standup_protocol_inner(
                state,
                next_day(),
                Some(target.clone()),
            )
        },
    )
    .await;
    time(
        "write: standup publish",
        "now: publish_standup_protocol (answer on queue)",
        runs,
        || {
            knobas_app::commands::entity::publish_standup_protocol_in_background(
                state,
                next_day(),
                Some(target.clone()),
            )
        },
    )
    .await;

    // The action-item ticket, filed from the probe's note.
    time(
        "write: action-item ticket",
        "before #779: create_action_item_ticket (send + sync awaited)",
        runs,
        || {
            knobas_app::commands::entity::create_action_item_ticket_inner(
                state,
                &note_id,
                "jira0:JIRA0",
                "Task",
                "Ask Ines",
                "",
            )
        },
    )
    .await;
    time(
        "write: action-item ticket",
        "now: create_action_item_ticket (answer on queue)",
        runs,
        || {
            knobas_app::commands::entity::create_action_item_ticket_in_background(
                state,
                &note_id,
                "jira0:JIRA0",
                "Task",
                "Ask Ines",
                "",
            )
        },
    )
    .await;

    // Start work: a fresh ticket's flow per call, over a forge whose sends
    // sleep like every other here and whose read refuses -- so the awaited run
    // stops at the link step, after the branch and pull request sends.
    config::insert(
        pool,
        &InsertConfig {
            id: "forge".into(),
            adapter_kind: "gitea".into(),
            display_name: "Forge".into(),
            base_url: "https://forge.invalid".into(),
            auth_kind: AuthKind::Method(AuthMethod::Pat),
            config: serde_json::json!({}),
            sync_interval_secs: 86_400,
            enabled: true,
        },
    )
    .await
    .unwrap();
    mirrored(
        "forge:tidewater/probe",
        "repo",
        "tidewater/probe",
        serde_json::json!({"default_branch": "main"}),
    )
    .await;
    let repo = knobas_core::entity::EntityRef::parse("forge:tidewater/probe").unwrap();
    let tickets = std::sync::atomic::AtomicU64::new(first_start_work_ticket(tickets));
    let flow = || async {
        let n = tickets.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let ticket = knobas_core::entity::EntityRef::parse(&format!("jira0:JIRA0-{n}")).unwrap();
        knobas_app::start_work::begin(pool, &ticket, &repo)
            .await
            .map_err(said)?;
        Ok::<_, String>(ticket)
    };
    time(
        "write: start work",
        "before #779: start_work_run (each step's send awaited)",
        runs,
        || async {
            let ticket = flow().await?;
            knobas_app::start_work::run(
                pool,
                &knobas_app::start_work::queue::Queue { state },
                &ticket,
            )
            .await
            .map_err(said)
        },
    )
    .await;
    time(
        "write: start work",
        "now: start_work_run (answer, steps behind)",
        runs,
        || async {
            let ticket = flow().await?;
            knobas_app::start_work::run_in_background(
                pool,
                std::sync::Arc::new(knobas_app::start_work::queue::Queue {
                    state: state.clone(),
                }),
                &ticket,
                state.scheduler.background(),
            )
            .await
            .map_err(said)
        },
    )
    .await;

    // Opening the write queue, over the rows the writes above left in it.
    time("open write queue", "pending_writes", runs, || {
        knobas_core::write_queue::open(pool)
    })
    .await;
    time("open write queue", "write_queue_counts", runs, || {
        knobas_core::write_queue::counts(pool)
    })
    .await;

    state.scheduler.shutdown().await;
}

struct Conns(knobas_db::embedded::Connector);
#[async_trait::async_trait]
impl knobas_sync::scheduler::RunConnections for Conns {
    async fn open(&self) -> Result<sqlx::PgConnection, sqlx::Error> {
        self.0.connect().await
    }
}

/// Any error, as the probe's panic prints it.
fn said<E: std::fmt::Debug>(error: E) -> String {
    format!("{error:?}")
}

#[cfg(test)]
mod fixed_keys {
    use super::*;

    #[test]
    fn the_default_size_keeps_the_start_work_tickets_it_always_had() {
        assert_eq!(first_start_work_ticket(5_000), 1000);
    }

    #[test]
    fn every_fixed_key_exists_at_the_smallest_supported_size() {
        // `RP_ITEMS=2500`: tickets/source = 5,000 * 2,500 / 22,000.
        let tickets = 5_000 * 2_500 / 22_000;
        for n in fixed_ticket_numbers(tickets, 7) {
            assert!(n < tickets, "JIRA-{n} is not in a mirror of {tickets}");
        }
    }
}
