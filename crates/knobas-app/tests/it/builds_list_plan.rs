//! The Builds list reads the mirror's runs once, however many pipelines it
//! draws.
//!
//! `PIPELINES` kept each pipeline's last twelve runs through a subquery the
//! planner ran once per pipeline, over every run in the mirror: 2.5 s at 130
//! pipelines and 3,000 runs on 2026-10-26, doubling with either. Read once,
//! the same list is about 140 ms.
//!
//! **The plan is the assertion**, as in `builds_of_a_commit_perf.rs`: after
//! an `analyze`, no node of `explain analyze` handles, over all its loops, a
//! tenth of the pipelines times the runs, which is what reading the runs once
//! per pipeline handles. A tenth leaves room for the pipelines joined to
//! their recent runs (130 × 130). A wall-clock budget would measure the
//! machine.

use knobas_app::sources::Registry;
use sqlx::PgPool;

/// The pipelines the list draws.
const PIPELINES: i32 = 130;
/// The runs among them.
const RUNS: i32 = 3_000;

/// [`PIPELINES`] TeamCity build configurations over nine projects and
/// [`RUNS`] builds spread across them, then `analyze`.
async fn seed(pool: &PgPool) {
    knobas_app::commands::search::searcher(pool, &Registry::for_process())
        .await
        .unwrap();
    sqlx::query(
        "insert into knobas.source_config (id, kind, display_name, base_url, auth_kind)
         values ('teamcity', 'teamcity', 'TeamCity', 'http://x', 'Pat')",
    )
    .execute(pool)
    .await
    .unwrap();
    let mut tx = pool.begin().await.unwrap();
    sqlx::query(
        "insert into knobas.entity (id, kind, title)
         select 'teamcity:buildType:C' || g, 'build_config', 'C' || g from generate_series(1, $1) g
         union all
         select 'teamcity:build:' || g, 'build', 'Build #' || g from generate_series(1, $2) g",
    )
    .bind(PIPELINES)
    .bind(RUNS)
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload, item_updated_at)
         select 'teamcity:buildType:C' || g, 'teamcity', 'build_config', 'C' || g, '',
                jsonb_build_object('id', 'C' || g, 'name', 'Config ' || g,
                                   'projectId', 'P' || (g % 9), 'projectName', 'Project ' || (g % 9)),
                now()
           from generate_series(1, $1) g
         union all
         select 'teamcity:build:' || g, 'teamcity', 'build', 'Build #' || g, '',
                jsonb_build_object('id', g, 'number', g::text, 'buildTypeId', 'C' || (g % $1 + 1),
                  'state', 'finished', 'status', case when g % 7 = 0 then 'FAILURE' else 'SUCCESS' end,
                  'branchName', 'main',
                  'startDate', '20260822T100500+0000', 'finishDate', '20260822T100917+0000',
                  'revisions', jsonb_build_object('revision', jsonb_build_array(
                    jsonb_build_object('version', md5(g::text) || 'abcdabcd')))),
                now() - make_interval(mins => g)
           from generate_series(1, $2) g",
    )
    .bind(PIPELINES)
    .bind(RUNS)
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    sqlx::query("analyze").execute(pool).await.unwrap();
}

#[tokio::test]
async fn the_builds_list_reads_the_runs_once() {
    let pool = knobas_db::test_util::scratch_database("builds-list-plan")
        .await
        .pool(2)
        .await
        .expect("a pool on the scratch database");
    seed(&pool).await;
    let registry = Registry::for_process();

    let listed = knobas_app::commands::entity::pipelines_inner(&pool, &registry)
        .await
        .unwrap();
    assert_eq!(listed.len(), PIPELINES as usize, "every pipeline is drawn");
    assert!(
        listed.iter().all(|p| p.recent.len() == 12),
        "each with its last twelve runs"
    );

    let declarations = knobas_app::sources::paths::declared_paths(&pool, &registry)
        .await
        .unwrap();
    let fillers = knobas_app::sources::crud::fillers(&pool, &registry, knobas_source::role::CI)
        .await
        .unwrap();
    let (json,): (serde_json::Value,) = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "explain (analyze, format json) {}",
        knobas_core::builds::PIPELINES
    )))
    .bind(declarations.as_param())
    .bind(knobas_core::builds::PIPELINE_RECENT)
    .bind(&fillers)
    .fetch_one(&pool)
    .await
    .unwrap();
    let mut nodes = Vec::new();
    walk(&json[0]["Plan"], &mut nodes);
    let (node, most) = nodes
        .iter()
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .expect("a plan");
    println!(
        "the Builds list at {PIPELINES} pipelines and {RUNS} runs: at most {most} rows in {node}"
    );
    assert!(
        *most < f64::from(PIPELINES) * f64::from(RUNS) / 10.0,
        "{node} handles {most} rows for {RUNS} runs: the list reads the runs once per pipeline"
    );
}

/// Every plan node as `(node type, rows over all its loops)`.
fn walk(node: &serde_json::Value, out: &mut Vec<(String, f64)>) {
    let kind = node["Node Type"].as_str().unwrap_or_default().to_owned();
    let rows = node["Actual Rows"].as_f64().unwrap_or_default()
        * node["Actual Loops"].as_f64().unwrap_or_default();
    out.push((kind, rows));
    for child in node["Plans"].as_array().map_or(&[][..], Vec::as_slice) {
        walk(child, out);
    }
}
