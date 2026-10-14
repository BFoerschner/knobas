//! The reads #778 reshaped plan without leaning on row estimates, with and
//! without planner statistics.
//!
//! On the probe's 22,000 items never analyzed -- a fresh install until its
//! first analyze -- the *All* room's builds tile took 6.8 s, the inbox's
//! statements about 430 ms each, the launcher's newest 300 ms and the board
//! 225 ms, against 15-70 ms analyzed (#775's timings). Each was planned from
//! `knobas.entity` through `sync.live_item`, estimated at 0.5 % of its rows,
//! and the builds tile then joined its CTEs by nested loops, re-aggregating
//! once per build. This asserts the shapes those plans had are gone, through
//! `knobas_db::plan_shape` (#770's checker, and `driven_from` for the read
//! driven from `knobas.entity`), on one mirror in both states.
//!
//! The Code tile's statement shares the builds tile's scan (`tile_ruled!`)
//! but is not asserted on: its `open_head` lookup is a scan of a CTE per
//! branch whatever the statistics say, unchanged here and a follow-up.

use knobas_app::sources::Registry;
use knobas_source::SourceDescriptor;
use sqlx::PgPool;

use crate::responsiveness_probe::Synthetic;

/// Tables and CTEs small by construction, which a plan may scan per row: the
/// sources the user configured; `grouped`, one row per source and kind with a
/// declared state; `status_paths`, one row per source and status candidate.
const SMALL: &[&str] = &["source_config", "grouped", "status_paths"];

/// The inbox rules whose whole statement #778 reshaped. Not here, and why:
///
/// * `failed_build`: its scan was reshaped, but two lookups it makes per
///   failed build were not, and the checker sees both: the newer green build
///   of the same configuration walks every build of the kind
///   (`item_kind_updated_idx`, no key for the configuration), and on an
///   analyzed, empty `knobas.link` the confirmed-link lookup is a scan per
///   row. Both are quadratic in the failed builds whatever the statistics
///   say; they are a follow-up of their own.
/// * `credential_expiry` and `alert`: unchanged. They read the configured
///   sources and the open alerts, a handful of rows each.
const RESHAPED_RULES: &[knobas_core::inbox::Category] = &[
    knobas_core::inbox::Category::ReviewRequest,
    knobas_core::inbox::Category::Mention,
    knobas_core::inbox::Category::NewAssignment,
];

/// A probe-shaped mirror, a third of its size: three Jira-shaped sources of
/// 1,500 tickets, a TeamCity-shaped one of 1,500 builds, a Confluence-shaped
/// one of 500 pages, synced through the real sink so the stored parents and
/// projects are what a sync leaves.
async fn mirror(analyzed: bool) -> (PgPool, knobas_core::payload::Declarations) {
    let connector = knobas_db::test_util::scratch_database("read_plans").await;
    let pool = connector.app_pool().await.unwrap();
    knobas_app::commands::search::searcher(&pool).await.unwrap();
    knobas_db::plan_shape::never_analyzed(&pool).await;
    let mut sources = Vec::new();
    for n in 0..3 {
        sources.push(Synthetic {
            descriptor: SourceDescriptor {
                id: format!("jira{n}"),
                ..knobas_source_jira::descriptor_template()
            },
            kind: "ticket",
            count: 1_500,
        });
    }
    sources.push(Synthetic {
        descriptor: SourceDescriptor {
            id: "ci".to_owned(),
            ..knobas_source_teamcity::descriptor_template()
        },
        kind: "build",
        count: 1_500,
    });
    sources.push(Synthetic {
        descriptor: SourceDescriptor {
            id: "wiki".to_owned(),
            ..knobas_source_confluence::descriptor_template()
        },
        kind: "page",
        count: 500,
    });
    for source in &sources {
        knobas_app::sources::demo::load_mirror_of(&pool, source)
            .await
            .unwrap();
    }
    // A tombstone and a disabled source, so both exclusion sets hold a row.
    sqlx::query("update knobas.entity set deleted_at = now() where id = 'jira0:JIRA0-7'")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("update knobas.source_config set enabled = false where id = 'wiki'")
        .execute(&pool)
        .await
        .unwrap();
    let state: i64 = sqlx::query_scalar(
        "select count(*) from pg_stat_user_tables
          where schemaname in ('knobas', 'sync')
            and (last_analyze is not null or last_autoanalyze is not null)",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(state, 0, "no table of the mirror is analyzed yet");
    if analyzed {
        sqlx::query("analyze").execute(&pool).await.unwrap();
    }
    let declarations = knobas_app::sources::paths::declared_paths(&pool, &Registry::builtin())
        .await
        .unwrap();
    (pool, declarations)
}

/// `explain (format json)` of `sql`, bound with what `bind` adds.
async fn explain<'q>(
    pool: &PgPool,
    sql: &'q str,
    bind: impl FnOnce(
        sqlx::query::QueryScalar<'q, sqlx::Postgres, serde_json::Value, sqlx::postgres::PgArguments>,
    ) -> sqlx::query::QueryScalar<
        'q,
        sqlx::Postgres,
        serde_json::Value,
        sqlx::postgres::PgArguments,
    >,
) -> serde_json::Value {
    let explain: &'q str = Box::leak(format!("explain (format json) {sql}").into_boxed_str());
    bind(sqlx::query_scalar(sqlx::AssertSqlSafe(explain)).persistent(false))
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn assert_every_read_plans_without_estimates(analyzed: bool) {
    let (pool, declarations) = mirror(analyzed).await;
    let declared = declarations.as_param();
    let mut wrong = Vec::new();
    let mut check = |name: &str, plan: serde_json::Value| {
        wrong.extend(
            knobas_db::plan_shape::findings(&plan, SMALL)
                .into_iter()
                .chain(knobas_db::plan_shape::driven_from(&plan, "entity"))
                .map(|f| format!("{name}: {f}")),
        );
    };

    // The *All* room's builds tile, and a project room's: the project's
    // predicate is a declared read per row, which no statistics describe, so
    // the room is estimated at a handful of builds analyzed or not -- where a
    // join between the CTEs went to nested loops.
    for (name, project) in [
        ("builds tile", None),
        ("builds tile, project room", Some("Payments")),
    ] {
        let plan = explain(
            &pool,
            knobas_app::commands::entity::planned_latest_per_parent(),
            |q| {
                q.bind(None::<Vec<String>>)
                    .bind(Some(vec!["build".to_owned()]))
                    .bind(None::<i32>)
                    .bind(50_i64)
                    .bind(0_i64)
                    .bind(None::<Vec<String>>)
                    .bind(project.map(str::to_owned))
                    .bind(declared.clone())
            },
        )
        .await;
        check(name, plan);
    }
    for (category, sql) in knobas_core::inbox::planned_statements() {
        if !RESHAPED_RULES.contains(&category) {
            continue;
        }
        let name = format!("inbox {category:?}");
        let name = name.as_str();
        let plan = explain(&pool, sql, |q| {
            q.bind(vec!["mara".to_owned()])
                .bind(chrono::Utc::now())
                .bind(false)
                .bind(declared.clone())
        })
        .await;
        check(name, plan);
    }
    let plan = explain(&pool, knobas_core::mini_board::planned_statement(), |q| {
        q.bind(None::<Vec<String>>)
            .bind(None::<Vec<String>>)
            .bind(None::<String>)
            .bind(declared.clone())
            .bind(Some(50_i64))
    })
    .await;
    check("mini_board", plan);
    let plan = explain(&pool, knobas_search::home::planned_recent(), |q| {
        q.bind(vec![
            "ticket".to_owned(),
            "build".to_owned(),
            "page".to_owned(),
        ])
        .bind(20_i64)
        .bind(declared.clone())
    })
    .await;
    check("launcher_recent", plan);

    let state = if analyzed {
        "analyzed"
    } else {
        "never analyzed"
    };
    assert!(wrong.is_empty(), "tables {state}: {wrong:#?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn every_reshaped_read_plans_without_estimates_on_tables_never_analyzed() {
    assert_every_read_plans_without_estimates(false).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn every_reshaped_read_plans_without_estimates_on_analyzed_tables() {
    assert_every_read_plans_without_estimates(true).await;
}
