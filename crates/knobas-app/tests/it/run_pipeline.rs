//! The runs of a pipeline, read from `sync.item_pipeline` (#2185, migration
//! `0062`), against the walk they replace, on a real PostgreSQL.
//!
//! One pipeline's runs, the Builds list's recent runs and a link-set
//! member's state used to walk every run of the pipeline's source through the
//! declared `pipeline` path. Since #2185 the database stores each run's
//! pipeline as the run is written, and the readers ask an index. This file
//! holds what they draw to the walk -- the statement before #2185, kept here
//! as the oracle -- after every change the trigger code claims to follow: an
//! insert, a payload update, a time that moves alone, a tombstone, a re-sync
//! in the writer's shape, a source given another adapter kind, a change to
//! the stored declarations, and a delete. Two sources share a pipeline key
//! throughout, and two runs carry no pipeline the walk can read.
//!
//! `sync.parent_declaration` is one row per adapter kind and a step rewrites
//! it, so the test takes a database of its own ([`scratch`], #1907).

use knobas_app::sources::Registry;
use sqlx::PgPool;

async fn scratch(label: &str) -> PgPool {
    let pool = knobas_db::test_util::scratch_database(label)
        .await
        .pool(2)
        .await
        .expect("a pool on the scratch database");
    declared(pool).await
}

/// The declarations' stored copy, written as bring-up writes it (ADR-0019).
async fn declared(pool: PgPool) -> PgPool {
    knobas_app::commands::search::searcher(&pool, &Registry::for_process())
        .await
        .unwrap();
    pool
}

/// Write `rows` -- `(kind, key, payload, minutes ago)` of `source` -- in the
/// sync writer's shape: one multi-row `insert ... on conflict do update`, so
/// a batch reaches the statement-level triggers together.
async fn upsert(pool: &PgPool, source: &str, rows: &[(&str, &str, serde_json::Value, i32)]) {
    let ids: Vec<String> = rows
        .iter()
        .map(|(_, k, _, _)| format!("{source}:{k}"))
        .collect();
    let kinds: Vec<&str> = rows.iter().map(|(kind, ..)| *kind).collect();
    let titles: Vec<&str> = rows.iter().map(|(_, k, ..)| *k).collect();
    let payloads: Vec<&serde_json::Value> = rows.iter().map(|(_, _, p, _)| p).collect();
    let ago: Vec<i32> = rows.iter().map(|(.., m)| *m).collect();
    sqlx::query(
        "insert into knobas.entity (id, kind, title)
         select * from unnest($1::text[], $2::text[], $3::text[])
             on conflict (id) do nothing",
    )
    .bind(&ids)
    .bind(&kinds)
    .bind(&titles)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload, item_updated_at)
         select t.id, $6, t.kind, t.title, '', t.payload,
                timestamptz '2026-10-26T12:00:00Z' - make_interval(mins => t.ago)
           from unnest($1::text[], $2::text[], $3::text[], $4::jsonb[], $5::int[])
                as t(id, kind, title, payload, ago)
             on conflict (entity_id) do update set
                source_id       = excluded.source_id,
                kind            = excluded.kind,
                title           = excluded.title,
                payload         = excluded.payload,
                item_updated_at = excluded.item_updated_at",
    )
    .bind(&ids)
    .bind(&kinds)
    .bind(&titles)
    .bind(&payloads)
    .bind(&ago)
    .bind(source)
    .execute(pool)
    .await
    .unwrap();
}

/// A TeamCity configuration.
fn config(id: &str) -> serde_json::Value {
    serde_json::json!({ "id": id, "name": format!("Config {id}"), "projectId": "P", "projectName": "P" })
}

/// A finished TeamCity build of the configuration `pipeline` (a JSON value:
/// a string is read, anything else is a miss).
fn run(pipeline: serde_json::Value, status: &str) -> serde_json::Value {
    serde_json::json!({
        "buildTypeId": pipeline, "number": "1", "state": "finished", "status": status,
        "branchName": "main",
        "startDate": "20260822T100500+0000", "finishDate": "20260822T100917+0000",
    })
}

/// The walk before #2185, verbatim in its rule: every live run of the
/// pipeline's source whose declared `pipeline` is the pipeline's own, newest
/// first. The oracle the stored rows are held to.
const WALK: &str = concat!(
    "with t as (select i.source_id, ",
    knobas_core::declared_string!("$1", "pipeline"),
    " as pkey from sync.live_item i where i.entity_id = $2)
select i.entity_id from t join sync.live_item i on i.source_id = t.source_id
 where ($1::jsonb #>> array[i.source_id, i.kind, 'frame']) = 'run'
   and ",
    knobas_core::declared_string!("$1", "pipeline"),
    " = t.pkey
 order by i.item_updated_at desc nulls last, i.entity_id desc"
);

async fn walked(pool: &PgPool, pipeline: &str) -> Vec<String> {
    let declarations = knobas_app::sources::paths::declared_paths(pool, &Registry::for_process())
        .await
        .unwrap();
    sqlx::query_scalar(WALK)
        .bind(declarations.as_param())
        .bind(pipeline)
        .fetch_all(pool)
        .await
        .unwrap()
}

/// What the pipeline page draws: its runs, newest first.
async fn drawn(pool: &PgPool, pipeline: &str) -> Vec<String> {
    drawn_at_most(pool, pipeline, None).await
}

/// The pipeline page's runs at most `limit`: the head of the stored range.
async fn drawn_at_most(pool: &PgPool, pipeline: &str, limit: Option<u32>) -> Vec<String> {
    knobas_app::commands::entity::pipeline_runs_inner(
        pool,
        &Registry::for_process(),
        pipeline,
        limit,
    )
    .await
    .unwrap()
    .into_iter()
    .map(|r| r.entity_id)
    .collect()
}

/// What the Builds list draws under `pipeline`: its last twelve runs.
async fn listed(pool: &PgPool, pipeline: &str) -> Vec<String> {
    knobas_app::commands::entity::pipelines_inner(pool, &Registry::for_process())
        .await
        .unwrap()
        .into_iter()
        .find(|p| p.entity_id == pipeline)
        .map(|p| p.recent.into_iter().map(|r| r.entity_id).collect())
        .unwrap_or_default()
}

/// The state chip text a link set's member row draws for each id.
/// Saved afresh each time, under the id its name gives it.
async fn member_states(pool: &PgPool, members: &[String]) -> Vec<Option<String>> {
    sqlx::query("delete from knobas.link_set")
        .execute(pool)
        .await
        .unwrap();
    let saved = knobas_app::link_sets::save(
        pool,
        &Registry::for_process(),
        knobas_app::link_sets::LinkSetDraft {
            id: None,
            name: "CI".to_owned(),
            description: String::new(),
            members: members
                .iter()
                .map(|id| knobas_app::link_sets::LinkSetMember {
                    kind: "pipeline".to_owned(),
                    entity_id: id.clone(),
                })
                .collect(),
            patterns: Vec::new(),
            rules: Vec::new(),
        },
    )
    .await
    .unwrap();
    saved
        .set
        .members
        .into_iter()
        .map(|m| m.state.map(|chip| chip.text))
        .collect()
}

/// Every pipeline's page, list row and member state agree with the walk;
/// answers the first pipeline's runs.
async fn agree(pool: &PgPool, pipelines: &[String], step: &str) -> Vec<String> {
    for pipeline in pipelines {
        let walk = walked(pool, pipeline).await;
        assert_eq!(
            drawn(pool, pipeline).await,
            walk,
            "after {step}: {pipeline}'s page is not the walk"
        );
        assert_eq!(
            drawn_at_most(pool, pipeline, Some(1)).await,
            walk.iter().take(1).cloned().collect::<Vec<_>>(),
            "after {step}: {pipeline}'s newest run is not the walk's"
        );
        assert_eq!(
            listed(pool, pipeline).await,
            walk.iter().take(12).cloned().collect::<Vec<_>>(),
            "after {step}: {pipeline}'s Builds row is not the walk"
        );
    }
    drawn(pool, &pipelines[0]).await
}

/// How many rows `sync.item_pipeline` holds for one item.
async fn stored_count(pool: &PgPool, entity: &str) -> i64 {
    sqlx::query_scalar("select count(*) from sync.item_pipeline where entity_id = $1")
        .bind(entity)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// Every change the trigger code follows, in turn, each followed by the
/// comparison with the walk. One test: the steps build on each other.
#[tokio::test]
async fn a_pipelines_runs_are_the_walk_after_every_change() {
    let pool = scratch("run_pipeline_walk").await;
    // Two TeamCity sources whose configurations share the key `A`.
    for source in ["tc1", "tc2"] {
        crate::support::source_config(&pool, source, "teamcity", source, &serde_json::json!({}))
            .await;
    }
    let a1 = "tc1:buildType:A".to_owned();
    let a2 = "tc2:buildType:A".to_owned();
    let b1 = "tc1:buildType:B".to_owned();
    let quiet = "tc1:buildType:Q".to_owned();
    let pipelines = [a1.clone(), a2.clone(), b1.clone(), quiet.clone()];
    let id = |key: &str| format!("tc1:{key}");
    upsert(
        &pool,
        "tc1",
        &[
            ("build_config", "buildType:A", config("A"), 0),
            ("build_config", "buildType:B", config("B"), 0),
            ("build_config", "buildType:Q", config("Q"), 0),
            ("build", "build:1", run("A".into(), "UNKNOWN"), 50),
            ("build", "build:2", run("A".into(), "FAILURE"), 40),
            ("build", "build:3", run("B".into(), "SUCCESS"), 30),
            // A miss twice: no `pipeline` value, and one that is no string.
            (
                "build",
                "build:4",
                run(serde_json::Value::Null, "FAILURE"),
                1,
            ),
            ("build", "build:5", run(7.into(), "FAILURE"), 1),
        ],
    )
    .await;
    upsert(
        &pool,
        "tc2",
        &[
            ("build_config", "buildType:A", config("A"), 0),
            ("build", "build:1", run("A".into(), "FAILURE"), 5),
        ],
    )
    .await;
    assert_eq!(
        agree(&pool, &pipelines, "the inserts").await,
        [id("build:2"), id("build:1")],
        "each run under its own source's pipeline only"
    );
    assert_eq!(drawn(&pool, &a2).await, ["tc2:build:1"]);
    assert!(drawn(&pool, &quiet).await.is_empty());
    assert_eq!(stored_count(&pool, &id("build:4")).await, 0, "a miss");
    assert_eq!(stored_count(&pool, &id("build:5")).await, 0, "a miss");
    // The quiet pipeline draws no state; the others their newest run's.
    assert_eq!(
        member_states(&pool, &pipelines).await,
        [
            Some("FAILURE".to_owned()),
            Some("FAILURE".to_owned()),
            Some("SUCCESS".to_owned()),
            None
        ]
    );

    // A payload update moves build 3 from B to A, the newest of A's runs now.
    upsert(
        &pool,
        "tc1",
        &[("build", "build:3", run("A".into(), "SUCCESS"), 30)],
    )
    .await;
    assert_eq!(
        agree(&pool, &pipelines, "a payload update").await,
        [id("build:3"), id("build:2"), id("build:1")]
    );

    // The source's time moves alone: build 1 is the newest now.
    sqlx::query("update sync.item set item_updated_at = timestamptz '2026-10-26T12:00:00Z' where entity_id = $1")
        .bind(id("build:1"))
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        agree(&pool, &pipelines, "a time that moved").await,
        [id("build:1"), id("build:3"), id("build:2")]
    );
    assert_eq!(
        member_states(&pool, &pipelines).await[0].as_deref(),
        Some("UNKNOWN"),
        "the member's state is its newest run's"
    );

    // A tombstone: the mirror row stays, the run is not drawn.
    sqlx::query("update knobas.entity set deleted_at = now() where id = $1")
        .bind(id("build:1"))
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        agree(&pool, &pipelines, "a tombstone").await,
        [id("build:3"), id("build:2")]
    );

    // A re-sync in the writer's shape, every payload unchanged: nothing is
    // added twice and nothing is lost.
    upsert(
        &pool,
        "tc1",
        &[
            ("build", "build:2", run("A".into(), "FAILURE"), 40),
            ("build", "build:3", run("A".into(), "SUCCESS"), 30),
        ],
    )
    .await;
    assert_eq!(
        agree(&pool, &pipelines, "a re-sync").await,
        [id("build:3"), id("build:2")]
    );
    assert_eq!(stored_count(&pool, &id("build:2")).await, 1);

    // The source pointed at an adapter that declares no runs: its rows are
    // matched at read time, so nothing is drawn and nothing recomputed.
    sqlx::query("update knobas.source_config set kind = 'jira' where id = 'tc1'")
        .execute(&pool)
        .await
        .unwrap();
    assert!(member_states(&pool, &pipelines).await[0].is_none());
    sqlx::query("update knobas.source_config set kind = 'teamcity' where id = 'tc1'")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        agree(&pool, &pipelines, "the adapter kind changed back").await,
        [id("build:3"), id("build:2")]
    );

    // The stored declaration loses `pipeline`: every run's row goes. Put
    // back, they return.
    sqlx::query(
        "update sync.parent_declaration
            set payload_paths = (select jsonb_agg(p - 'pipeline') from jsonb_array_elements(payload_paths) p)
          where adapter_kind = 'teamcity'",
    )
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(
        stored_count(&pool, &id("build:2")).await,
        0,
        "a declaration change"
    );
    knobas_app::commands::search::searcher(&pool, &Registry::for_process())
        .await
        .unwrap();
    assert_eq!(
        agree(&pool, &pipelines, "the declaration restored").await,
        [id("build:3"), id("build:2")]
    );

    // A delete cascades, as removing a source deletes its items.
    sqlx::query("delete from knobas.entity where id = $1")
        .bind(id("build:2"))
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(agree(&pool, &pipelines, "a delete").await, [id("build:3")]);
    assert_eq!(stored_count(&pool, &id("build:2")).await, 0);
}

/// The resolver carries `declared_string!`'s expansion as text, because a
/// migration is SQL and cannot call it, once for a run's `pipeline` and once
/// for its `branch` (#2192, migration `0065`). This holds that text to what
/// the macro expands to **now**: change the macro and this fails until a new
/// migration restates `sync.resolve_item_pipelines`.
#[tokio::test]
async fn the_stored_resolver_is_the_macros_expansion() {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    let source: String = sqlx::query_scalar(
        "select p.prosrc from pg_proc p join pg_namespace n on n.oid = p.pronamespace
          where n.nspname = 'sync' and p.proname = 'resolve_item_pipelines'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    for expansion in [
        knobas_core::declared_string!("dd.decl", "pipeline"),
        knobas_core::declared_string!("dd.decl", "branch"),
    ] {
        assert!(
            source.contains(expansion),
            "sync.resolve_item_pipelines no longer carries declared_string!'s expansion; \
             restate it in a new migration:\n{expansion}"
        );
    }
}
