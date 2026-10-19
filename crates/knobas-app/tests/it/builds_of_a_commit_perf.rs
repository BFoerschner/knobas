//! The builds of a commit are an index lookup on `sync.item_revision` (#732),
//! and at 50,000 builds one under 5 ms.
//!
//! The #710 walk read every live row through the declared paths: 437-442 ms
//! at 50,000 builds on 2026-10-13, and growing with the mirror. The stored
//! rows make it an index lookup whose cost is the handful of builds one commit
//! has.
//!
//! **The plan is the gate's assertion**, at [`GATE_BUILDS`] builds after an
//! `analyze`: the lookup uses the revision index and scans no mirror table.
//! The planner chooses on the statistics, so a few thousand rows already
//! decide it the way 50,000 do, without seeding 50,000 rows through the
//! triggers on every `just check` (7-60 s on a loaded machine).
//!
//! **The time is a reading**, at [`PERF_BUILDS`], and `#[ignore]`d for the
//! reason `knobas-search`'s `tests/perf.rs` is (`docs/agents/
//! working-model.md`): a wall-clock budget inside a gate measures the machine.
//! It checks the same plan at that size, then reads PostgreSQL's own
//! `Execution Time` from `explain analyze`, the fastest of ten runs. Take it
//! on a quiet machine with `just builds-perf`.
//!
//! The gate's test seeds this file's database (#572); the reading seeds a
//! scratch database of its own, so neither size is the other's fixture.

use knobas_app::commands::entity::DETAIL_BUILDS_AT;
use knobas_app::sources::Registry;
use sqlx::PgPool;

/// The gate's size: enough rows that the planner prices a scan above the
/// index, few enough to seed in about a second.
const GATE_BUILDS: i32 = 3_000;
/// The reading's size, the ticket's.
const PERF_BUILDS: i32 = 50_000;
/// The builds that ran on the commit asked about.
const ON_THE_COMMIT: i32 = 6;
const SHA: &str = "c90d11a3f5e2b7c4d9018e6a2b3c4d5e6f708192";

/// `builds` TeamCity builds, each on a commit of its own and a shared second
/// root's, and [`ON_THE_COMMIT`] of them on [`SHA`] too, then `analyze`.
/// Written in one statement, so the insert trigger resolves them in one pass.
async fn seed(pool: &PgPool, builds: i32) {
    knobas_app::commands::search::searcher(pool, &Registry::for_process())
        .await
        .unwrap();
    sqlx::query(
        "insert into knobas.source_config (id, kind, display_name, base_url, auth_kind)
         values ('teamcity', 'teamcity', 'TeamCity', 'http://x', 'Pat') on conflict do nothing",
    )
    .execute(pool)
    .await
    .unwrap();
    let mut tx = pool.begin().await.unwrap();
    sqlx::query(
        "insert into knobas.entity (id, kind, title)
         select 'teamcity:' || g, 'build', 'Payout / Build #' || g from generate_series(1, $1) g",
    )
    .bind(builds)
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload, item_updated_at)
         select 'teamcity:' || g, 'teamcity', 'build', 'Payout / Build #' || g, '',
                jsonb_build_object(
                  'state', 'finished',
                  'status', case when g % 7 = 0 then 'FAILURE' else 'SUCCESS' end,
                  'buildType', jsonb_build_object('id', 'Payout_' || (g % 40), 'name', 'Config ' || (g % 40)),
                  'revisions', jsonb_build_object('revision', jsonb_build_array(
                    jsonb_build_object('version', case when g <= $2 then $3 else md5(g::text) || 'abcdabcd' end),
                    jsonb_build_object('version', md5('infra' || (g / 100)::text) || 'abcdabcd')))),
                now() - make_interval(mins => g)
           from generate_series(1, $1) g",
    )
    .bind(builds)
    .bind(ON_THE_COMMIT)
    .bind(SHA)
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    sqlx::query("analyze").execute(pool).await.unwrap();
}

async fn declarations(pool: &PgPool) -> knobas_core::payload::Declarations {
    knobas_app::sources::paths::declared_paths(pool, &Registry::for_process())
        .await
        .unwrap()
}

/// Every build is stored, the lookup is an index lookup that scans no
/// mirror table, and it answers the commit's builds.
async fn assert_an_index_lookup(pool: &PgPool, builds: i32) {
    let stored: i64 =
        sqlx::query_scalar("select count(distinct entity_id) from sync.item_revision")
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(
        stored,
        i64::from(builds),
        "every build's revisions are stored"
    );

    let declarations = declarations(pool).await;
    let (json,): (serde_json::Value,) = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "explain (format json) {DETAIL_BUILDS_AT}"
    )))
    .bind(SHA)
    .bind(declarations.as_param())
    .bind(20_i64)
    .fetch_one(pool)
    .await
    .unwrap();
    let mut nodes = Vec::new();
    walk(&json[0]["Plan"], &mut nodes);
    let scans: Vec<_> = nodes.iter().filter(|(_, on)| !on.is_empty()).collect();
    println!("builds of a commit at {builds} builds: scans {scans:?}");
    assert!(
        nodes
            .iter()
            .any(|(node, index)| node.contains("Index") && index == "item_revision_revision_idx"),
        "the lookup does not use the revision index: {nodes:?}"
    );
    assert!(
        !nodes.iter().any(|(node, relation)| node == "Seq Scan"
            && (relation == "item" || relation == "item_revision")),
        "the lookup scans the mirror: {nodes:?}"
    );

    let rows: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "select count(*) from ({DETAIL_BUILDS_AT}) x"
    )))
    .bind(SHA)
    .bind(declarations.as_param())
    .bind(20_i64)
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(rows, i64::from(ON_THE_COMMIT), "the builds of the commit");
}

#[tokio::test]
async fn a_commits_builds_are_an_index_lookup() {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    seed(&pool, GATE_BUILDS).await;
    assert_an_index_lookup(&pool, GATE_BUILDS).await;
}

/// The reading: the same plan at 50,000 builds, and under 5 ms, the fastest
/// of ten `explain analyze` runs.
#[tokio::test]
#[ignore = "wall-clock against a budget; run deliberately on a quiet machine with `just builds-perf`"]
async fn a_commits_builds_take_under_five_milliseconds_at_fifty_thousand_builds() {
    let pool = knobas_db::test_util::scratch_database("builds-of-a-commit-perf")
        .await
        .pool(2)
        .await
        .expect("a pool on the scratch database");
    seed(&pool, PERF_BUILDS).await;
    assert_an_index_lookup(&pool, PERF_BUILDS).await;

    let declarations = declarations(&pool).await;
    let explain = format!("explain (analyze, format json) {DETAIL_BUILDS_AT}");
    let mut fastest = f64::INFINITY;
    for _ in 0..10 {
        let (json,): (serde_json::Value,) = sqlx::query_as(sqlx::AssertSqlSafe(explain.clone()))
            .bind(SHA)
            .bind(declarations.as_param())
            .bind(20_i64)
            .fetch_one(&pool)
            .await
            .unwrap();
        fastest = fastest.min(json[0]["Execution Time"].as_f64().unwrap());
    }
    println!("builds of a commit at {PERF_BUILDS} builds: {fastest:.3} ms");
    assert!(
        fastest < 5.0,
        "the lookup took {fastest:.3} ms at {PERF_BUILDS} builds"
    );
}

/// Every plan node as `(node type, index or relation name)`.
fn walk(node: &serde_json::Value, out: &mut Vec<(String, String)>) {
    let kind = node["Node Type"].as_str().unwrap_or_default().to_owned();
    let on = node["Index Name"]
        .as_str()
        .or_else(|| node["Relation Name"].as_str())
        .unwrap_or_default()
        .to_owned();
    out.push((kind, on));
    for child in node["Plans"].as_array().map_or(&[][..], Vec::as_slice) {
        walk(child, out);
    }
}
