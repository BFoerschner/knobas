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
    let kind = match node["Relation Name"].as_str() {
        Some(relation) => format!(
            "{} on {relation}",
            node["Node Type"].as_str().unwrap_or_default()
        ),
        None => node["Node Type"].as_str().unwrap_or_default().to_owned(),
    };
    let rows = node["Actual Rows"].as_f64().unwrap_or_default()
        * node["Actual Loops"].as_f64().unwrap_or_default();
    out.push((kind, rows));
    for child in node["Plans"].as_array().map_or(&[][..], Vec::as_slice) {
        walk(child, out);
    }
}

/// The runs of the one long pipeline [`the_unread_failures_read_one_range`]
/// seeds: years of a busy configuration.
const LONG_RUNS: i32 = 10_000;

/// The unread failures the list names (#2543) are the one read of the
/// pipelines' whole history: each run looked at once, however many
/// pipelines. At one pipeline of [`LONG_RUNS`] runs beside [`PIPELINES`]
/// short ones holding [`RUNS`], one run in seven failing and half the long
/// one's failures read, no node of the `unread` CTE handles more than twice
/// the runs, and the runs are read by `item_pipeline_runs_idx` (index-backed,
/// as `read_plans.rs` holds every read of a pipeline's runs). The timing is
/// printed for the PR, not asserted.
#[tokio::test]
async fn the_unread_failures_read_one_range() {
    let pool = knobas_db::test_util::scratch_database("builds-unread-plan")
        .await
        .pool(2)
        .await
        .expect("a pool on the scratch database");
    knobas_app::commands::search::searcher(&pool, &Registry::for_process())
        .await
        .unwrap();
    sqlx::query(
        "insert into knobas.source_config (id, kind, display_name, base_url, auth_kind)
         values ('teamcity', 'teamcity', 'TeamCity', 'http://x', 'Pat')",
    )
    .execute(&pool)
    .await
    .unwrap();
    let mut tx = pool.begin().await.unwrap();
    sqlx::query(
        "insert into knobas.entity (id, kind, title)
         select 'teamcity:buildType:Long', 'build_config', 'Long'
         union all
         select 'teamcity:buildType:C' || g, 'build_config', 'C' || g from generate_series(1, $2) g
         union all
         select 'teamcity:build:' || g, 'build', 'Build #' || g from generate_series(1, $1 + $3) g",
    )
    .bind(LONG_RUNS)
    .bind(PIPELINES)
    .bind(RUNS)
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload, item_updated_at)
         select 'teamcity:buildType:Long', 'teamcity', 'build_config', 'Long', '',
                jsonb_build_object('id', 'Long', 'name', 'Long', 'projectId', 'P', 'projectName', 'P'), now()
         union all
         select 'teamcity:buildType:C' || g, 'teamcity', 'build_config', 'C' || g, '',
                jsonb_build_object('id', 'C' || g, 'name', 'Config ' || g, 'projectId', 'P', 'projectName', 'P'), now()
           from generate_series(1, $2) g
         union all
         select 'teamcity:build:' || g, 'teamcity', 'build', 'Build #' || g, '',
                jsonb_build_object('id', g, 'number', g::text,
                  'buildTypeId', case when g <= $1 then 'Long' else 'C' || (g % $2 + 1) end,
                  'state', 'finished', 'status', case when g % 7 = 0 then 'FAILURE' else 'SUCCESS' end,
                  'branchName', 'main',
                  'startDate', '20260822T100500+0000', 'finishDate', '20260822T100917+0000'),
                now() - make_interval(mins => g)
           from generate_series(1, $1 + $3) g",
    )
    .bind(LONG_RUNS)
    .bind(PIPELINES)
    .bind(RUNS)
    .execute(&mut *tx)
    .await
    .unwrap();
    // Half the failures read.
    sqlx::query(
        "insert into knobas.run_read (entity_id)
         select 'teamcity:build:' || g from generate_series(14, $1, 14) g",
    )
    .bind(LONG_RUNS)
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    let registry = Registry::for_process();
    let listed = knobas_app::commands::entity::pipelines_inner(&pool, &registry)
        .await
        .unwrap();
    let failures = LONG_RUNS / 7;
    let unread = failures - LONG_RUNS / 14;
    let long = listed
        .iter()
        .find(|p| p.entity_id == "teamcity:buildType:Long")
        .expect("the long pipeline is listed");
    assert_eq!(
        long.unread.len(),
        unread as usize,
        "every unread failure, past the twelve"
    );
    let (stored,): (i64,) = sqlx::query_as("select count(*) from sync.item_run")
        .fetch_one(&pool)
        .await
        .unwrap();
    sqlx::query("analyze").execute(&pool).await.unwrap();

    let declarations = knobas_app::sources::paths::declared_paths(&pool, &registry)
        .await
        .unwrap();
    let fillers = knobas_app::sources::crud::fillers(&pool, &registry, knobas_source::role::CI)
        .await
        .unwrap();
    let mut times = Vec::new();
    let mut plan = serde_json::Value::Null;
    for _ in 0..7 {
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
        let cte = find(&json[0]["Plan"], "CTE unread").expect("the unread CTE in the plan");
        times.push((
            json[0]["Execution Time"].as_f64().unwrap_or_default(),
            cte["Actual Total Time"].as_f64().unwrap_or_default(),
        ));
        plan = cte.clone();
    }
    times.sort_by(|a, b| a.0.total_cmp(&b.0));
    let (whole, cte) = times[times.len() / 2];
    let mut nodes = Vec::new();
    walk(&plan, &mut nodes);
    let (node, most) = nodes
        .iter()
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .expect("a plan");
    println!(
        "the Builds list at one pipeline of {LONG_RUNS} runs ({stored} stored run rows), {unread} unread failures: \
         median of 7 {whole:.1} ms, the unread CTE {cte:.1} ms; at most {most} rows in {node}"
    );
    let runs = f64::from(LONG_RUNS + RUNS);
    assert!(
        *most <= 2.0 * runs,
        "{node} handles {most} rows for {runs} runs: the history is read more than once"
    );
    let text = plan.to_string();
    assert!(
        text.contains("item_pipeline_runs_idx")
            && !nodes
                .iter()
                .any(|(kind, _)| kind == "Seq Scan on item_pipeline"),
        "each pipeline's runs are one range of item_pipeline_runs_idx: {text}"
    );

    // A mark of everything (ADR-0063) reads each pipeline's runs the same
    // way, and writes every unread failure: rolled back, so the list above
    // is what the next run reads.
    let mut tx = pool.begin().await.unwrap();
    let (json,): (serde_json::Value,) = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "explain (analyze, format json) {}",
        knobas_core::run_read::MARK_READ
    )))
    .bind(declarations.as_param())
    .bind(Vec::<String>::new())
    .bind(Vec::<String>::new())
    .bind(true)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    tx.rollback().await.unwrap();
    let mut nodes = Vec::new();
    walk(&json[0]["Plan"], &mut nodes);
    let text = json.to_string();
    println!(
        "a mark of everything over the same mirror: {:.1} ms",
        json[0]["Execution Time"].as_f64().unwrap_or_default()
    );
    assert!(
        text.contains("item_pipeline_runs_idx")
            && !nodes
                .iter()
                .any(|(kind, _)| kind == "Seq Scan on item_pipeline"),
        "a mark of everything reads each pipeline's runs as one range: {text}"
    );
}

/// The plan node named `name` (`Subplan Name`), anywhere under `node`.
fn find<'a>(node: &'a serde_json::Value, name: &str) -> Option<&'a serde_json::Value> {
    if node["Subplan Name"].as_str() == Some(name) {
        return Some(node);
    }
    node["Plans"]
        .as_array()
        .into_iter()
        .flatten()
        .find_map(|child| find(child, name))
}
