//! The reads #778 and #785 reshaped plan without leaning on row estimates,
//! with and without planner statistics.
//!
//! On the probe's 22,000 items never analyzed -- a fresh install until its
//! first analyze -- the *All* room's builds tile took 6.8 s, the inbox's
//! statements about 430 ms each, the launcher's newest 300 ms and the board
//! 225 ms, against 15-70 ms analyzed (#775's timings). Each was planned from
//! `knobas.entity` through `sync.live_item`, estimated at 0.5 % of its rows,
//! and the builds tile then joined its CTEs by nested loops, re-aggregating
//! once per build. #785 added the inbox's `failed_build` rule, whose lookups
//! walked the builds per red build whatever the statistics. This asserts the
//! shapes those plans had are gone, through
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

/// The inbox rules whose whole statement #778 and #785 reshaped. Not here,
/// and why: `credential_expiry` and `alert`, unchanged. They read the
/// configured sources and the open alerts, a handful of rows each.
///
/// `failed_build` joined in #785: its newer green build of the same
/// configuration was a walk of every build of the kind per failed build
/// (`item_kind_updated_idx`, no key for the configuration), and its
/// confirmed-link lookup a scan of `knobas.link` per build on an analyzed,
/// empty table. Both are read once per statement now. The rules are bound
/// with an identity that owns every item, so each of the mirror's red builds
/// reaches both: the case that read 195 ms on the probe.
///
/// The stream itself is these unioned with the two rules left out, so it is
/// not planned here; that it is planned for its values on every read is
/// `knobas-core`'s `the_inbox_statements_are_never_kept_as_prepared_statements`.
const RESHAPED_RULES: &[knobas_core::inbox::Category] = &[
    knobas_core::inbox::Category::ReviewRequest,
    knobas_core::inbox::Category::Mention,
    knobas_core::inbox::Category::FailedBuild,
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
    // Proposals, so the tray's lookups (#774) are planned over a table that
    // holds some: each ticket proposed with its next three, about 13,500.
    sqlx::query(
        "with t as (select id, row_number() over (order by id) as n
                      from knobas.entity where kind = 'ticket')
         insert into knobas.link
                (from_id, to_id, relation, origin, created_by,
                 rule, rule_class, reason)
         select a.id, b.id, 'related', 'suggested', 'knobas',
                'similar_text', 'similarity', 'plan test'
           from t a join t b on b.n between a.n + 1 and a.n + 3",
    )
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
        wrong.extend(shapes(name, &plan));
    }
    let mut rules = 0;
    for (category, sql) in knobas_core::inbox::planned_statements() {
        if !RESHAPED_RULES.contains(&category) {
            continue;
        }
        rules += 1;
        let name = format!("inbox {category:?}");
        let name = name.as_str();
        let plan = explain(&pool, sql, |q| {
            q.bind(vec!["mara".to_owned()])
                .bind(chrono::Utc::now())
                .bind(false)
                .bind(declared.clone())
        })
        .await;
        wrong.extend(shapes(name, &plan));
    }
    // A rule renamed or dropped from `RULES` would otherwise go unchecked.
    assert_eq!(
        rules,
        RESHAPED_RULES.len(),
        "every reshaped rule was planned"
    );
    let plan = explain(&pool, knobas_core::mini_board::planned_statement(), |q| {
        q.bind(None::<Vec<String>>)
            .bind(None::<Vec<String>>)
            .bind(None::<String>)
            .bind(declared.clone())
            .bind(Some(50_i64))
    })
    .await;
    wrong.extend(shapes("mini_board", &plan));
    // The board's signals (#815), for fifty of its tickets, with a tenth of
    // the proposals confirmed for the plan so the walk has links to follow,
    // and put back so the tray's plans below see the mirror as it was.
    let tickets: Vec<String> = sqlx::query_scalar(
        "select id from knobas.entity where kind = 'ticket' order by id limit 50",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    let confirm = "update knobas.link set confirmed_at = now()
                    where ctid in (select ctid from knobas.link where confirmed_at is null
                                    order by from_id limit 1350)";
    sqlx::query(confirm).execute(&pool).await.unwrap();
    let plan = explain(&pool, knobas_core::mini_board::planned_signals(), |q| {
        q.bind(declared.clone())
            .bind(vec!["mara".to_owned()])
            .bind(tickets.clone())
    })
    .await;
    sqlx::query("update knobas.link set confirmed_at = null where confirmed_at is not null")
        .execute(&pool)
        .await
        .unwrap();
    wrong.extend(shapes("mini_board signals", &plan));
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
    wrong.extend(shapes("launcher_recent", &plan));

    // The tray's count in a project room, as the server plans it for the
    // values and as it plans it generically: PostgreSQL switched the
    // persistent statement to its generic plan at the sixth execution, and
    // there the join to `sync.item` was a probe per end per proposal (#778).
    let count = knobas_core::suggest::planned_proposal_count();
    let custom = explain(&pool, count, |q| {
        q.bind(Some(vec!["jira0".to_owned()]))
            .bind(None::<Vec<String>>)
    })
    .await;
    // `generic_plan` takes no values, which the extended protocol insists on
    // sending: the simple protocol.
    let generic = {
        use sqlx::Row as _;
        sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
            "explain (generic_plan, format json) {count}"
        )))
        .fetch_one(&pool)
        .await
        .unwrap()
        .get::<serde_json::Value, _>(0)
    };
    for (name, plan) in [
        ("proposal_count", custom),
        ("proposal_count, generic", generic),
    ] {
        let mut types = Vec::new();
        node_types(&plan[0]["Plan"], &mut types);
        if let Some(join) = types
            .iter()
            .find(|t| t.contains("Join") || t.contains("Nested Loop"))
        {
            wrong.push(format!(
                "{name}: joins ({join}) where an end's room is a membership test"
            ));
        }
        wrong.extend(shapes(name, &plan));
    }

    // The tray's lookups (#774): one item's or one id's rows, never a walk of
    // `link_proposed_idx`. Never analyzed, the planner read every member's
    // rows off that partial index, the whole table filtered, whenever the
    // lookup's predicate implied it.
    let [members, drawn, overflow] = knobas_core::suggest::planned_tray_lookups();
    let ids = vec!["jira0:JIRA0-3".to_owned(), "jira1:JIRA1-7".to_owned()];
    let plans = [
        (
            "tray, a context's walk",
            explain(&pool, members, |q| {
                q.bind(None::<Vec<String>>).bind(Some(ids.clone()))
            })
            .await,
        ),
        (
            "tray, the drawn rows",
            explain(&pool, drawn, |q| q.bind(vec![uuid::Uuid::new_v4()])).await,
        ),
        (
            "tray, the overflow counts",
            explain(&pool, overflow, |q| {
                q.bind(Some(vec!["jira0".to_owned()]))
                    .bind(None::<Vec<String>>)
                    .bind(ids.clone())
            })
            .await,
        ),
    ];
    for (name, plan) in plans {
        if plan.to_string().contains("link_proposed_idx") {
            wrong.push(format!("{name}: reads link_proposed_idx, the whole tray"));
        }
        wrong.extend(shapes(name, &plan));
    }

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

/// What `plan_shape` finds in one plan, named.
fn shapes(name: &str, plan: &serde_json::Value) -> Vec<String> {
    knobas_db::plan_shape::findings(plan, SMALL)
        .into_iter()
        .chain(knobas_db::plan_shape::driven_from(plan, "entity"))
        .map(|f| format!("{name}: {f}"))
        .collect()
}

/// Every node type in a plan, subplans included.
fn node_types(node: &serde_json::Value, out: &mut Vec<String>) {
    if let Some(t) = node["Node Type"].as_str() {
        out.push(t.to_owned());
    }
    for child in node["Plans"].as_array().into_iter().flatten() {
        node_types(child, out);
    }
}
