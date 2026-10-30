//! The entity list asked for a role (#1294, ADR-0029's amendment of
//! 2026-10-19): the mirror answers which sources fill it, and the read
//! narrows to them and, where it names no kinds, to the role's kinds.
//!
//! This module's database is its own (`knobas_db::test_util` keys it by this
//! file), and every source below is named with a run-unique suffix, so each
//! test asserts on exactly the sources it configured.

use knobas_app::commands::entity::list_entities_inner;
use knobas_core::entity_list::{EntityFilter, EntityOrder};
use knobas_source::role::RoleClaim;
use knobas_source::{KindInfo, SourceDescriptor};
use sqlx::PgPool;

/// The adapter kind of the custom-role fixture: it claims the ticket system,
/// CI and a role of its own, and emits tickets, pipelines and surveys.
const CUSTOM: &str = "role_desk";
/// Its custom role, namespaced to it.
const DESK: &str = "role_desk:survey-desk";

/// The process's registry, plus [`CUSTOM`]: the built-in mock's claims (all
/// five roles, #1229, #1240), Jira's (the ticket system alone) and a custom
/// role's, which nothing compiled in declares.
struct WithCustom;

impl knobas_sync::scheduler::AdapterRegistry for WithCustom {
    fn descriptors(&self) -> Vec<SourceDescriptor> {
        let mut all = knobas_app::sources::Registry::for_process().templates();
        let kind = |id: &str| KindInfo {
            id: id.to_owned(),
            label: id.to_owned(),
            plural: id.to_owned(),
            monogram: "XX".to_owned(),
            full_sync_exhaustive: true,
            refreshes: true,
        };
        all.push(SourceDescriptor {
            id: CUSTOM.to_owned(),
            adapter_kind: CUSTOM.to_owned(),
            name: "Desk".to_owned(),
            capabilities: Vec::new(),
            adapter_version: "0".to_owned(),
            auth_methods: Vec::new(),
            accepts_account: false,
            min_sync_interval_secs: 60,
            roles: vec![
                RoleClaim::new(
                    knobas_source::role::TICKET_SYSTEM,
                    ["reachable_transitions"],
                ),
                RoleClaim::new(knobas_source::role::CI, ["run_log"]),
                RoleClaim::new(DESK, []),
            ],
            source_actions: Vec::new(),
            clone_from: Vec::new(),
            url_forms: Vec::new(),
            code_pages: None,
            write_ops: Vec::new(),
            entity_kinds: vec![kind("ticket"), kind("pipeline"), kind("survey")],
            config_schema: serde_json::json!({}),
            // Its pipelines are CI's though the entity list leaves them out
            // of CI's kinds (#1344), so they are no kind of its custom role.
            payload_paths: vec![knobas_source::KindPaths {
                kind: "pipeline".to_owned(),
                frame: Some(knobas_source::Frame::Pipeline),
                ..knobas_source::KindPaths::default()
            }],
        });
        all
    }

    fn build(
        &self,
        instance: knobas_source::instance::SourceInstance,
    ) -> Result<Box<dyn knobas_source::Source>, knobas_source::SourceError> {
        knobas_app::sources::Registry::for_process().build(instance)
    }
}

async fn pool() -> PgPool {
    static MIGRATED: tokio::sync::OnceCell<()> = tokio::sync::OnceCell::const_new();
    let pool = knobas_db::test_util::test_pool().await;
    MIGRATED
        .get_or_init(|| async {
            knobas_db::migrate::run(&pool).await.unwrap();
        })
        .await;
    pool
}

fn unique(prefix: &str) -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos());
    format!("{prefix}-{nanos}-{}", NEXT.fetch_add(1, Ordering::Relaxed))
}

/// One configured source of adapter kind `adapter`.
async fn configured(pool: &PgPool, adapter: &str) -> String {
    let source = unique(adapter);
    crate::support::source_config(pool, &source, adapter, adapter, &serde_json::json!({})).await;
    source
}

/// One live item of each of `kinds` under `source`.
async fn holding(pool: &PgPool, source: &str, kinds: &[&str]) {
    for kind in kinds {
        let entity = format!("{source}:{kind}");
        sqlx::query("insert into knobas.entity (id, kind, title) values ($1, $2, $3)")
            .bind(&entity)
            .bind(kind)
            .bind(kind)
            .execute(pool)
            .await
            .unwrap();
        sqlx::query(
            "insert into sync.item (entity_id, source_id, kind, title, body_text, payload)
             values ($1, $2, $3, $3, '', '{}')",
        )
        .bind(&entity)
        .bind(source)
        .bind(kind)
        .execute(pool)
        .await
        .unwrap();
    }
}

/// A standing miss of `role` on `source`'s declared paths (ADR-0028, #1199).
async fn missed(pool: &PgPool, source: &str, role: &str) {
    crate::support::role_miss(pool, source, role, "build", "pipeline", "buildType.id").await;
}

fn asking(role: &str) -> EntityFilter {
    EntityFilter {
        sources: Vec::new(),
        kinds: Vec::new(),
        updated_within_days: None,
        context: None,
        project: None,
        order: EntityOrder::UpdatedDesc,
        include_deleted: false,
        reach: None,
        role: Some(role.to_owned()),
    }
}

/// `(source, kind)` of every row `filter` reads among `sources`, sorted.
/// Other tests in this module write beside these, so their rows are left
/// out of the pairs.
async fn read(pool: &PgPool, filter: &EntityFilter, sources: &[&String]) -> Vec<(String, String)> {
    let page = list_entities_inner(
        pool,
        &WithCustom,
        filter,
        500,
        0,
        &knobas_core::payload::Declarations::empty(),
    )
    .await
    .unwrap();
    let mut pairs: Vec<(String, String)> = page
        .rows
        .into_iter()
        .filter(|row| sources.contains(&&row.source_id))
        .map(|row| (row.source_id, row.kind))
        .collect();
    pairs.sort();
    pairs
}

fn pairs(rows: &[(&String, &str)]) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = rows
        .iter()
        .map(|(source, kind)| ((*source).clone(), (*kind).to_owned()))
        .collect();
    out.sort();
    out
}

/// A filler's rows come back; a claimer with a miss, a source whose adapter
/// does not claim the role, a removed source's leftovers and a source whose
/// adapter is not installed are left out. With no kinds named the read is
/// the role's kinds: CI's are the run kinds its fillers declare (the mock's
/// `build`; its `pipeline` is the `pipelines` read's, #1344), so the
/// filler's ticket and pipeline are not drawn.
#[tokio::test]
async fn a_role_reads_its_fillers_and_its_kinds_and_nothing_else() {
    let pool = pool().await;
    let filler = configured(&pool, "mock").await;
    let with_miss = configured(&pool, "mock").await;
    let not_claiming = configured(&pool, "jira").await;
    let uninstalled = configured(&pool, "role_nowhere").await;
    let removed = configured(&pool, "mock").await;
    for source in [&filler, &with_miss, &not_claiming, &uninstalled, &removed] {
        holding(&pool, source, &["pipeline", "build", "ticket"]).await;
    }
    missed(&pool, &with_miss, knobas_source::role::CI).await;
    sqlx::query("delete from knobas.source_config where id = $1")
        .bind(&removed)
        .execute(&pool)
        .await
        .unwrap();

    let all = [&filler, &with_miss, &not_claiming, &uninstalled, &removed];
    // Every one of them is live: absent a role, the read draws them all.
    let unscoped = EntityFilter {
        role: None,
        ..asking(knobas_source::role::CI)
    };
    assert_eq!(read(&pool, &unscoped, &all).await.len(), 15);
    assert_eq!(
        read(&pool, &asking(knobas_source::role::CI), &all).await,
        pairs(&[(&filler, "build")]),
    );
    // The miss takes one surface away and nothing else (ADR-0028): the
    // source still fills the ticket system.
    assert_eq!(
        read(&pool, &asking(knobas_source::role::TICKET_SYSTEM), &all).await,
        pairs(&[
            (&filler, "ticket"),
            (&with_miss, "ticket"),
            (&not_claiming, "ticket")
        ]),
    );
}

/// A source filling two roles is drawn for each, with each role's kinds.
#[tokio::test]
async fn a_source_filling_two_roles_is_drawn_for_each() {
    let pool = pool().await;
    let source = configured(&pool, "mock").await;
    holding(&pool, &source, &["pr", "branch", "build", "monitor"]).await;
    assert_eq!(
        read(&pool, &asking(knobas_source::role::CODE_HOST), &[&source]).await,
        pairs(&[(&source, "branch"), (&source, "pr")]),
    );
    assert_eq!(
        read(&pool, &asking(knobas_source::role::MONITORING), &[&source]).await,
        pairs(&[(&source, "monitor")]),
    );
}

/// With sources named, the role reads the fillers among them; with kinds
/// named, the role's kinds among them.
#[tokio::test]
async fn a_role_intersects_the_named_sources_and_kinds() {
    let pool = pool().await;
    let one = configured(&pool, "mock").await;
    let two = configured(&pool, "mock").await;
    let jira = configured(&pool, "jira").await;
    for source in [&one, &two, &jira] {
        holding(&pool, source, &["pipeline", "build", "ticket"]).await;
    }
    let all = [&one, &two, &jira];

    let named = EntityFilter {
        sources: vec![one.clone(), jira.clone()],
        ..asking(knobas_source::role::CI)
    };
    assert_eq!(
        read(&pool, &named, &all).await,
        pairs(&[(&one, "build")]),
        "the fillers among the named sources: jira is named and fills nothing here"
    );

    let none_filling = EntityFilter {
        sources: vec![jira.clone()],
        ..asking(knobas_source::role::CI)
    };
    let page = list_entities_inner(
        &pool,
        &WithCustom,
        &none_filling,
        50,
        0,
        &knobas_core::payload::Declarations::empty(),
    )
    .await
    .unwrap();
    assert_eq!(
        (page.rows.len(), page.total),
        (0, 0),
        "named sources none of which fills the role read nothing, not every filler"
    );

    let kinds = EntityFilter {
        kinds: vec!["build".to_owned(), "ticket".to_owned()],
        ..asking(knobas_source::role::CI)
    };
    assert_eq!(
        read(&pool, &kinds, &all).await,
        pairs(&[(&one, "build"), (&two, "build")]),
        "the role's kinds among the named kinds: a ticket is not CI's"
    );
}

/// An unknown role is an empty page whose total is zero, never an error, in
/// a plain order and in a tile order.
#[tokio::test]
async fn an_unknown_role_is_an_empty_page() {
    let pool = pool().await;
    let source = configured(&pool, "mock").await;
    holding(&pool, &source, &["ticket"]).await;
    for order in [EntityOrder::UpdatedDesc, EntityOrder::Attention] {
        let page = list_entities_inner(
            &pool,
            &WithCustom,
            &EntityFilter {
                order,
                ..asking("nobody:fills-this")
            },
            50,
            0,
            &knobas_core::payload::Declarations::empty(),
        )
        .await
        .unwrap();
        assert!(page.rows.is_empty(), "{order:?}");
        assert_eq!(page.total, 0, "{order:?}");
    }
}

/// A custom role reads its adapter's kinds that belong to no built-in role
/// the source claims: the desk's surveys, not its tickets nor its CI
/// pipelines, which the entity list reads for no role (#1344).
#[tokio::test]
async fn a_custom_role_reads_its_adapters_other_kinds() {
    let pool = pool().await;
    let desk = configured(&pool, CUSTOM).await;
    let mock = configured(&pool, "mock").await;
    for source in [&desk, &mock] {
        holding(&pool, source, &["ticket", "pipeline", "survey"]).await;
    }
    assert_eq!(
        read(&pool, &asking(DESK), &[&desk, &mock]).await,
        pairs(&[(&desk, "survey")]),
    );
    assert_eq!(
        read(
            &pool,
            &asking(knobas_source::role::TICKET_SYSTEM),
            &[&desk, &mock]
        )
        .await,
        pairs(&[(&desk, "ticket"), (&mock, "ticket")]),
    );
}

/// Every statement takes the scope, the tile orders and the withdrawn
/// reads included: a non-filler's rows are in none of them, and the tally of
/// a tile order counts the fillers' rows alone.
#[tokio::test]
async fn every_statement_reads_the_scope() {
    let pool = pool().await;
    let filler = configured(&pool, "mock").await;
    let other = configured(&pool, "jira").await;
    for source in [&filler, &other] {
        holding(&pool, source, &["pr", "branch"]).await;
    }
    let mut orders = vec![
        (false, EntityOrder::UpdatedDesc),
        (false, EntityOrder::TitleAsc),
        (true, EntityOrder::UpdatedDesc),
        (true, EntityOrder::TitleAsc),
        (false, EntityOrder::Attention),
        (false, EntityOrder::LatestPerParent),
    ];
    for (include_deleted, order) in orders.drain(..) {
        let filter = EntityFilter {
            include_deleted,
            order,
            sources: vec![filler.clone(), other.clone()],
            ..asking(knobas_source::role::CODE_HOST)
        };
        let page = list_entities_inner(
            &pool,
            &WithCustom,
            &filter,
            50,
            0,
            &knobas_core::payload::Declarations::empty(),
        )
        .await
        .unwrap();
        let sources: std::collections::BTreeSet<&str> =
            page.rows.iter().map(|row| row.source_id.as_str()).collect();
        assert_eq!(
            sources,
            [filler.as_str()].into(),
            "{include_deleted} {order:?}"
        );
        assert_eq!(page.total, 2, "{include_deleted} {order:?}");
        if let Some(tally) = page.tally {
            assert_eq!(tally.iter().map(|t| t.count).sum::<i64>(), 2, "{order:?}");
        }
    }
}

/// One pipeline `id` of `source`, as the mock's Builds frame reads one
/// (`name`, `id`), with one run of it.
async fn pipeline_with_a_run(pool: &PgPool, source: &str, id: &str) {
    for (kind, entity, payload) in [
        (
            "pipeline",
            format!("{source}:pipeline:{id}"),
            serde_json::json!({ "id": id, "name": id }),
        ),
        (
            "build",
            format!("{source}:build:{id}-1"),
            serde_json::json!({ "pipeline": id, "number": "1", "status": "success" }),
        ),
    ] {
        sqlx::query("insert into knobas.entity (id, kind, title) values ($1, $2, $1)")
            .bind(&entity)
            .bind(kind)
            .execute(pool)
            .await
            .unwrap();
        sqlx::query(
            "insert into sync.item (entity_id, source_id, kind, title, body_text, payload)
             values ($1, $2, $3, $1, '', $4)",
        )
        .bind(&entity)
        .bind(source)
        .bind(kind)
        .bind(payload)
        .execute(pool)
        .await
        .unwrap();
    }
}

/// **The pipelines read is CI's** (#1295): it narrows itself to the CI
/// role's fillers, with no argument. A filler's pipelines come back with
/// their runs; a claimer whose records missed a CI path, a source whose
/// adapter does not claim CI, a source whose adapter is not installed and a
/// removed source's leftovers are left out, though every one of them holds
/// a live pipeline and run.
#[tokio::test]
async fn the_pipelines_read_is_the_ci_fillers() {
    let pool = pool().await;
    let filler = configured(&pool, "mock").await;
    let with_miss = configured(&pool, "mock").await;
    let not_claiming = configured(&pool, "jira").await;
    let uninstalled = configured(&pool, "role_nowhere").await;
    let removed = configured(&pool, "mock").await;
    let all = [&filler, &with_miss, &not_claiming, &uninstalled, &removed];
    for source in all {
        pipeline_with_a_run(&pool, source, "Deploy").await;
    }
    missed(&pool, &with_miss, knobas_source::role::CI).await;
    // The miss is CI's alone: a miss of another role takes nothing from Builds.
    missed(&pool, &filler, knobas_source::role::WIKI).await;
    sqlx::query("delete from knobas.source_config where id = $1")
        .bind(&removed)
        .execute(&pool)
        .await
        .unwrap();

    let listed = knobas_app::commands::entity::pipelines_inner(
        &pool,
        &knobas_app::sources::Registry::for_process(),
    )
    .await
    .unwrap();
    let ours: Vec<(&str, &str, usize)> = listed
        .iter()
        .filter(|row| all.contains(&&row.source_id))
        .map(|row| (row.source_id.as_str(), row.name.as_str(), row.recent.len()))
        .collect();
    assert_eq!(ours, vec![(filler.as_str(), "Deploy", 1)]);
}

/// **A pipeline a ticket reaches is not a row of the job's Builds section**
/// (#1344). The section asks for the CI role with no kinds, in the
/// latest-per-parent order, scoped by the ticket's reach; CI's kinds on the
/// entity list are the run kinds its fillers declare, so the ticket's linked
/// pipeline is left out and its run is drawn. Named, the pipeline kind is
/// none of CI's here: a pipeline is the `pipelines` read's.
#[tokio::test]
async fn a_reached_pipeline_is_not_read_for_ci() {
    let pool = pool().await;
    let filler = configured(&pool, "mock").await;
    holding(&pool, &filler, &["ticket"]).await;
    pipeline_with_a_run(&pool, &filler, "Deploy").await;
    let ticket = format!("{filler}:ticket");
    for reached in [
        format!("{filler}:pipeline:Deploy"),
        format!("{filler}:build:Deploy-1"),
    ] {
        knobas_app::commands::entity::create_link_inner(&pool, &ticket, &reached, None, None)
            .await
            .unwrap();
    }
    let builds = EntityFilter {
        order: EntityOrder::LatestPerParent,
        reach: Some(ticket.clone()),
        ..asking(knobas_source::role::CI)
    };
    assert_eq!(
        read(&pool, &builds, &[&filler]).await,
        pairs(&[(&filler, "build")]),
    );
    let pipelines = EntityFilter {
        kinds: vec!["pipeline".to_owned()],
        ..builds
    };
    assert_eq!(read(&pool, &pipelines, &[&filler]).await, Vec::new());
}

/// One live ticket `key` of `source` in `status`, at the path the mock
/// declares a ticket's status (`status`) and at Jira's.
async fn ticket_in(pool: &PgPool, source: &str, key: &str, status: &str) {
    let entity = format!("{source}:{key}");
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1, 'ticket', $1)")
        .bind(&entity)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload)
         values ($1, $2, 'ticket', $1, '', $3)",
    )
    .bind(&entity)
    .bind(source)
    .bind(serde_json::json!({ "status": status, "fields": { "status": { "name": status } } }))
    .execute(pool)
    .await
    .unwrap();
}

/// **The ticket board is read for a role** (#1299): `mini_board` narrows
/// itself to the role's fillers. A filler's tickets are drawn and counted; a
/// claimer whose records missed a ticket-system path, a source whose adapter
/// does not claim the ticket system and a removed source's leftovers are
/// neither drawn nor counted, though each holds a live ticket. The status is
/// this test's own, so its column holds only what this test wrote.
#[tokio::test]
async fn the_ticket_board_reads_the_roles_fillers() {
    let pool = pool().await;
    let status = unique("Triage");
    let filler = configured(&pool, "mock").await;
    let with_miss = configured(&pool, "mock").await;
    let not_claiming = configured(&pool, "teamcity").await;
    let removed = configured(&pool, "mock").await;
    let all = [&filler, &with_miss, &not_claiming, &removed];
    for source in all {
        ticket_in(&pool, source, "T-1", &status).await;
    }
    ticket_in(&pool, &filler, "T-2", &status).await;
    missed(&pool, &with_miss, knobas_source::role::TICKET_SYSTEM).await;
    // A miss of another role takes nothing off the ticket board.
    missed(&pool, &filler, knobas_source::role::CI).await;
    sqlx::query("delete from knobas.source_config where id = $1")
        .bind(&removed)
        .execute(&pool)
        .await
        .unwrap();

    let board = knobas_app::commands::entity::mini_board_inner(
        &pool,
        &knobas_app::sources::Registry::for_process(),
        None,
        Some(knobas_source::role::TICKET_SYSTEM),
        None,
        None,
        None,
    )
    .await
    .unwrap();
    let mut drawn: Vec<(&str, &str)> = board
        .columns
        .iter()
        .flat_map(|column| &column.cards)
        .filter(|card| all.contains(&&card.source_id))
        .map(|card| (card.source_id.as_str(), card.key.as_str()))
        .collect();
    drawn.sort_unstable();
    assert_eq!(
        drawn,
        vec![(filler.as_str(), "T-1"), (filler.as_str(), "T-2")]
    );
    let column = board
        .columns
        .iter()
        .find(|column| column.status.as_deref() == Some(status.as_str()))
        .expect("the filler's column");
    assert_eq!(
        column.total, 2,
        "the column counts the filler's tickets alone"
    );
}

/// A role nobody fills is an empty board, read: not every source's, which
/// is what no source named once meant.
#[tokio::test]
async fn a_role_nobody_fills_is_an_empty_board() {
    let pool = pool().await;
    let source = configured(&pool, "mock").await;
    ticket_in(&pool, &source, "T-1", "To Do").await;
    let board = knobas_app::commands::entity::mini_board_inner(
        &pool,
        &knobas_app::sources::Registry::for_process(),
        None,
        Some("role_nobody:fills-this"),
        None,
        None,
        None,
    )
    .await
    .unwrap();
    assert!(board.columns.is_empty(), "{:?}", board.columns);
    assert!(board.sources.is_empty());
}
