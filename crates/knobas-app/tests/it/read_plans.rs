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
/// declared state; `status_paths`, one row per source and status candidate;
/// `lane_paths`, `label_paths` and `state_paths` the same for a lane's field,
/// the labels and a build's or pull request's state (#939); `assignee_paths`
/// the inbox's assignee candidates (#1164), and `pipeline_paths` its failed
/// build's pipeline candidates (#2364).
const SMALL: &[&str] = &[
    "source_config",
    "grouped",
    "status_paths",
    "lane_paths",
    "label_paths",
    "state_paths",
    "assignee_paths",
    "pipeline_paths",
    // `ticket_views`' saved views by project and liveness (#2193): one row
    // per view, project its members name and live or not, so bounded by the
    // user's views times the projects one view spans, never the members,
    // which are read once.
    "view_projects",
];

/// The inbox rules whose whole statement #778 and #785 reshaped. Not here,
/// and why: `credential_expiry` and `alert`, unchanged, and `failed_sign_in`
/// (#1571), new. They read the configured sources and the open alerts, a
/// handful of rows each.
///
/// `failed_build` joined in #785: its newer green build of the same
/// configuration was a walk of every build of the kind per failed build
/// (`item_kind_updated_idx`, no key for the configuration), and its
/// confirmed-link lookup a scan of `knobas.link` per build on an analyzed,
/// empty table. The newer green is read once per statement now, and the
/// links are probed per red build by key (#800, [`unkeyed_link_reads`]). The
/// rules are bound with an identity that owns every item, so each of the
/// mirror's red builds reaches both: the case that read 195 ms on the probe;
/// and with one that owns nothing, the case that read 100 ms after #939 gave
/// the probe its 12,000 confirmed links.
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
/// 1,500 tickets, a TeamCity-shaped one of 1,500 builds over 130
/// configurations (five quiet), an Uptime-Kuma-shaped one of ten monitors
/// with 20,000 samples, a Confluence-shaped one of 500 pages, synced through
/// the real sink so the stored parents, projects and pipelines are what a
/// sync leaves, and a link set of every configuration (#2185). Three bodies in four carry the probe's
/// common word (#1849), as the probe's do: a heavier load of trigrams, which
/// left `item_body_trgm_idx` 411 pages of pending list that the planner
/// charged a random read each until `0057` capped the list (#1864).
async fn mirror(analyzed: bool) -> (PgPool, knobas_core::payload::Declarations) {
    let connector = knobas_db::test_util::scratch_database("read_plans").await;
    let pool = connector.app_pool().await.unwrap();
    knobas_app::commands::search::searcher(&pool, &Registry::for_process())
        .await
        .unwrap();
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
            newest_updated_at: None,
        });
    }
    sources.push(Synthetic {
        descriptor: SourceDescriptor {
            id: "ci".to_owned(),
            ..knobas_source_teamcity::descriptor_template()
        },
        kind: "build",
        count: 1_500,
        newest_updated_at: None,
    });
    // Uptime Kuma's monitors (#2185), whose samples are seeded below.
    sources.push(Synthetic {
        descriptor: SourceDescriptor {
            id: "kuma".to_owned(),
            ..knobas_source_kuma::descriptor_template()
        },
        kind: "monitor",
        count: 10,
        newest_updated_at: None,
    });
    sources.push(Synthetic {
        descriptor: SourceDescriptor {
            id: "wiki".to_owned(),
            ..knobas_source_confluence::descriptor_template()
        },
        kind: "page",
        count: 500,
        newest_updated_at: None,
    });
    for source in &sources {
        knobas_app::sources::demo::load_mirror_of(&pool, source)
            .await
            .unwrap();
    }
    // Monitor history (#2185): 2,000 samples of each of the ten monitors, a
    // minute apart, so a read that sorts them all has rows to sort.
    sqlx::query(
        "insert into knobas.monitor_sample (entity_id, taken_at, state)
         select 'kuma:monitor:' || m, now() - make_interval(mins => n),
                case when n % 50 = 0 then 'down' else 'up' end
           from generate_series(0, 9) m cross join generate_series(1, 2000) n",
    )
    .execute(&pool)
    .await
    .unwrap();
    // A link set whose members are every configuration (#2185), the quiet
    // ones included, on the tickets of `JIRA00`.
    knobas_app::link_sets::save(
        &pool,
        &Registry::for_process(),
        knobas_app::link_sets::LinkSetDraft {
            id: None,
            name: "Payments CI".to_owned(),
            description: String::new(),
            members: pipeline_members(),
            patterns: Vec::new(),
            rules: vec![knobas_app::link_sets::LinkSetRule {
                kind: "project".to_owned(),
                value: "jira0:JIRA00".to_owned(),
            }],
        },
    )
    .await
    .unwrap();
    // A revision on every build (#2185), so *This job*'s pull-request
    // check has stored commits to look its runs up by.
    sqlx::query(
        "update sync.item
            set payload = payload || jsonb_build_object('revisions', jsonb_build_object(
                  'revision', jsonb_build_array(jsonb_build_object('version', md5(entity_id) || 'abcdabcd'))))
          where source_id = 'ci' and kind = 'build'",
    )
    .execute(&pool)
    .await
    .unwrap();
    saved_views(&pool).await;
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
    let declarations = knobas_app::sources::paths::declared_paths(&pool, &Registry::for_process())
        .await
        .unwrap();
    (pool, declarations)
}

/// Three Jira views, projected as a sync projects them (#2193), in #2182's
/// shape: a board of every ticket of `jira0`, a sprint of 60 of
/// `jira1`'s, and a filter of every ticket of `jira2` with three keys of a
/// project that source does not sync, which names its projects so the
/// view's `unsynced` has a project to count.
async fn saved_views(pool: &PgPool) {
    sqlx::query("update knobas.source_config set config = config || '{\"projects\": [\"JIRA2\"]}' where id = 'jira2'")
        .execute(pool)
        .await
        .unwrap();
    for (source, name, cap, extra, view) in [
        (
            "jira0",
            "board-1",
            100_000_i64,
            &[][..],
            serde_json::json!({
                "kind": "board", "id": 1, "type": "kanban", "jql": "project = JIRA0",
                "query": "(project = JIRA0)", "sprint": null,
                "columns": [{ "name": "To Do", "statuses": ["To Do"], "limit": null }],
            }),
        ),
        (
            "jira1",
            "board-2",
            60,
            &[][..],
            serde_json::json!({
                "kind": "board", "id": 2, "type": "scrum", "jql": "project = JIRA1",
                "query": "(project = JIRA1)",
                "sprint": { "id": 9, "name": "Sprint 9", "end_date": null },
                "columns": [{ "name": "To Do", "statuses": ["To Do"], "limit": null }],
            }),
        ),
        (
            "jira2",
            "filter-3",
            100_000,
            &["WEB-1", "WEB-2", "WEB-3"][..],
            serde_json::json!({
                "kind": "filter", "id": 3, "type": null, "jql": "project = JIRA2",
                "query": "(project = JIRA2)", "sub_query": null,
            }),
        ),
    ] {
        let id = format!("{source}:{name}");
        sqlx::query("insert into knobas.entity (id, kind, title) values ($1, 'jira_view', $2)")
            .bind(&id)
            .bind(name)
            .execute(pool)
            .await
            .unwrap();
        sqlx::query(
            "insert into sync.item (entity_id, source_id, kind, title, body_text, payload, item_updated_at)
             select $1, $2, 'jira_view', $3, $3,
                    $4 || jsonb_build_object('keys', (
                      select to_jsonb(array_agg(k order by k)) from (
                        (select substring(entity_id from ':(.*)$') as k from sync.item
                          where source_id = $2 and kind = 'ticket'
                          order by entity_id limit $5)
                        union all select unnest($6::text[])) keys)),
                    now()",
        )
        .bind(&id)
        .bind(source)
        .bind(name)
        .bind(view)
        .bind(cap)
        .bind(extra)
        .execute(pool)
        .await
        .unwrap();
        let declared =
            knobas_sync::saved_views::declared(&knobas_source_jira::descriptor_template())
                .expect("Jira declares saved views");
        let mut tx = pool.begin().await.unwrap();
        knobas_sync::saved_views::project(&mut tx, source, &declared)
            .await
            .unwrap();
        tx.commit().await.unwrap();
    }
    let members: i64 = sqlx::query_scalar("select count(*) from sync.saved_view_member")
        .fetch_one(pool)
        .await
        .unwrap();
    // `mirror`'s 1,500 tickets a source, the sprint's 60, the three keys.
    assert_eq!(members, 1_500 + 60 + 1_503, "every view holds its members");
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
            knobas_core::entity_list::planned_latest_per_parent(),
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
        // `nobody` owns nothing, the reader whose failed-build rule walked
        // every confirmed link (#800); `mara` owns everything.
        for who in ["mara", "nobody"] {
            let name = format!("inbox {category:?} as {who}");
            let name = name.as_str();
            let plan = explain(&pool, sql, |q| {
                q.bind(vec![who.to_owned()])
                    .bind(chrono::Utc::now())
                    .bind(false)
                    .bind(declared.clone())
            })
            .await;
            wrong.extend(shapes(name, &plan));
            if category == knobas_core::inbox::Category::FailedBuild {
                wrong.extend(
                    unkeyed_link_reads(&plan[0]["Plan"])
                        .into_iter()
                        .map(|f| format!("{name}: {f}")),
                );
            }
            if category == knobas_core::inbox::Category::Mention
                && !reads_body_index(&plan[0]["Plan"])
            {
                wrong.push(format!(
                    "{name}: reads every item's body, not {BODY_INDEX} (#1170)"
                ));
            }
            if category == knobas_core::inbox::Category::NewAssignment {
                wrong.extend(
                    item_scans_filtered_per_row(&plan[0]["Plan"])
                        .into_iter()
                        .map(|f| format!("{name}: {f}")),
                );
            }
        }
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
            .bind(None::<String>)
            .bind(None::<i64>)
    })
    .await;
    wrong.extend(shapes("mini_board", &plan));
    // In lanes (#939): by epic, the costliest lane read, its first ten.
    let plan = explain(&pool, knobas_core::mini_board::planned_statement(), |q| {
        q.bind(None::<Vec<String>>)
            .bind(None::<Vec<String>>)
            .bind(None::<String>)
            .bind(declared.clone())
            .bind(Some(8_i64))
            .bind(Some("epic".to_owned()))
            .bind(Some(10_i64))
    })
    .await;
    wrong.extend(shapes("mini_board in lanes", &plan));
    // Narrowed to a third of the tickets (#1149), as `has:blocker` narrows
    // the probe's: the ids are a set probed per ticket, never an index
    // search per id.
    let matched: Vec<String> = sqlx::query_scalar(
        "select id from knobas.entity where kind = 'ticket' and right(id, 1) in ('1', '4', '7')",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert!(matched.len() > 1_000, "{} tickets matched", matched.len());
    let plan = explain(&pool, knobas_core::mini_board::planned_statement(), |q| {
        q.bind(Some(matched.clone()))
            .bind(None::<Vec<String>>)
            .bind(None::<String>)
            .bind(declared.clone())
            .bind(Some(8_i64))
            .bind(None::<String>)
            .bind(None::<i64>)
    })
    .await;
    wrong.extend(shapes("mini_board narrowed", &plan));
    wrong.extend(
        searches_per_id(&plan[0]["Plan"])
            .into_iter()
            .map(|f| format!("mini_board narrowed: {f}")),
    );
    // The statuses the board offers (#1149): its sources' whole corpora,
    // live by the tombstones read once, never by a probe per ticket.
    let plan = explain(
        &pool,
        knobas_core::mini_board::planned_observed_statuses(),
        |q| {
            q.bind(vec![
                "jira0".to_owned(),
                "jira1".to_owned(),
                "jira2".to_owned(),
            ])
            .bind(declared.clone())
        },
    )
    .await;
    wrong.extend(shapes("mini_board statuses", &plan));
    wrong.extend(
        entity_probes(&plan[0]["Plan"])
            .into_iter()
            .map(|f| format!("mini_board statuses: {f}")),
    );
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
    // `has:`'s walk (#1163), every kind asked: from the ends the kinds can
    // start at, each step a probe of a key compared bytewise (`0050`). A
    // probe that compares an id in another collation than its column's
    // cannot use the key's index, and is a scan.
    const HAS: &str = concat!(
        "select sg.tid from ",
        knobas_core::ticket_signals!(
            "$1",
            "$2",
            knobas_core::signal_ends!("$1", "$2", "$3"),
            "$3"
        ),
        " sg"
    );
    let has = explain(&pool, HAS, |q| {
        q.bind(declared.clone())
            .bind(vec!["mara".to_owned()])
            .bind(None::<Vec<String>>)
    })
    .await;
    sqlx::query("update knobas.link set confirmed_at = null where confirmed_at is not null")
        .execute(&pool)
        .await
        .unwrap();
    wrong.extend(shapes("mini_board signals", &plan));
    wrong.extend(shapes("has: walk", &has));
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

    // A promoted epic's members (#1172): its children found by source, never
    // every entity scanned with a probe of `sync.item` each, which
    // `sync.live_item` was planned as on tables never analyzed.
    let plan = explain(&pool, knobas_core::context::planned_member_ids(), |q| {
        q.bind("ctx:none")
            .bind("jira0:JIRA0-3")
            .bind("jira0")
            .bind("JIRA0-3")
    })
    .await;
    wrong.extend(shapes("context member_ids", &plan));

    // The Docs tree's pages (#981): found by their kind, never by a probe per
    // item of the mirror that filters the kind after (the timings are on
    // `knobas_app::docs::PAGES`). `$2` is the wiki's fillers (#1296).
    let plan = explain(&pool, knobas_app::docs::planned_tree(), |q| {
        q.bind(declared.clone()).bind(vec!["wiki".to_owned()])
    })
    .await;
    wrong.extend(shapes("docs_tree", &plan));
    wrong.extend(
        kind_filtered_probes(&plan[0]["Plan"])
            .into_iter()
            .map(|f| format!("docs_tree: {f}")),
    );

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

    // The reads that walked a pipeline's runs, and the alert roster (#2185):
    // through the stored rows' index, each pipeline's head of range, never a
    // run's payload tested per pipeline or per member; every sample read
    // under a `Limit` of one row per monitor.
    let fillers = vec!["ci".to_owned()];
    let builds = explain(&pool, knobas_core::builds::PIPELINES, |q| {
        q.bind(declared.clone())
            .bind(knobas_core::builds::PIPELINE_RECENT)
            .bind(fillers.clone())
    })
    .await;
    let mut runs_reads = vec![("builds list", builds)];
    for pipeline in ["ci:buildType:Cfg_7", "ci:buildType:Cfg_129"] {
        let plan = explain(&pool, knobas_core::builds::planned_pipeline_runs(), |q| {
            q.bind(declared.clone()).bind(pipeline).bind(100_i64)
        })
        .await;
        runs_reads.push(("pipeline runs", plan));
    }
    let members: Vec<String> = pipeline_members()
        .into_iter()
        .map(|m| m.entity_id)
        .collect();
    let plan = explain(&pool, knobas_app::link_sets::planned_member_rows(), |q| {
        q.bind(declared.clone()).bind(members.clone())
    })
    .await;
    runs_reads.push(("link-set member rows", plan));
    for (name, plan) in &runs_reads {
        wrong.extend(corpus_shapes(name, plan));
        wrong.extend(
            knobas_db::plan_shape::filtered_per_row(plan, "item")
                .into_iter()
                .chain(whole_reads(&plan[0]["Plan"], "item_pipeline"))
                .map(|f| format!("{name}: {f}")),
        );
    }
    let reached = knobas_core::reach::reach(&pool, "jira0:JIRA0-40")
        .await
        .unwrap();
    assert!(
        reached.len() > 100,
        "the job reaches the link set's pipelines: {}",
        reached.len()
    );
    // And a branch the job reaches (#2192), one the mirror's runs were built
    // on: the runs on it are a keyed lookup of the stored rows, never every
    // run's `branch` read through the declarations.
    let branch = job_branch(&pool, "feature/10").await;
    for (name, reached_branch) in [("this job", None), ("this job on a branch", Some(&branch))] {
        let ids: Vec<String> = reached
            .iter()
            .map(|r| r.entity_id.clone())
            .chain(reached_branch.cloned())
            .collect();
        let steps: Vec<String> = reached
            .iter()
            .map(|r| r.step.clone())
            .chain(reached_branch.cloned())
            .collect();
        let plan = explain(&pool, knobas_core::builds::planned_job_pipelines(), |q| {
            q.bind(declared.clone())
                .bind(ids.clone())
                .bind(steps.clone())
                .bind(vec!["link_set".to_owned(); ids.len()])
        })
        .await;
        wrong.extend(corpus_shapes(name, &plan));
        wrong.extend(
            knobas_db::plan_shape::filtered_per_row(&plan, "item")
                .into_iter()
                .chain(whole_reads(&plan[0]["Plan"], "item_pipeline"))
                .chain(cte_driven_from(&plan[0]["Plan"], "CTE p", "entity"))
                .map(|f| format!("{name}: {f}")),
        );
    }
    let plan = explain(&pool, knobas_sync::alerts::ROSTER, |q| {
        q.bind("kuma").bind(knobas_sync::samples::KIND)
    })
    .await;
    wrong.extend(corpus_shapes("alert roster", &plan));
    wrong.extend(
        knobas_db::plan_shape::unbounded_reads(&plan, "monitor_sample")
            .into_iter()
            .map(|f| format!("alert roster: {f}")),
    );

    // `ticket_views`' counts of the three Jira views (#2193): live by the
    // tombstones read once, as the board's are, never a probe of
    // `knobas.entity` per member.
    let views = vec![
        "jira0:board-1".to_owned(),
        "jira1:board-2".to_owned(),
        "jira2:filter-3".to_owned(),
    ];
    let plan = explain(
        &pool,
        knobas_app::ticket_views::planned_saved_view_sources(),
        |q| q.bind(views.clone()),
    )
    .await;
    wrong.extend(shapes("ticket_views counts", &plan));
    wrong.extend(
        entity_probes(&plan[0]["Plan"])
            .into_iter()
            .map(|f| format!("ticket_views counts: {f}")),
    );

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

/// A Gitea-shaped branch titled `name`, of a source of its own (#2192): what
/// *This job* reaches when a ticket is linked to a branch.
async fn job_branch(pool: &PgPool, name: &str) -> String {
    crate::support::source_config(pool, "git", "gitea", "gitea", &serde_json::json!({})).await;
    let id = format!("git:acme/svc@refs/heads/{name}");
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1, 'branch', $2)")
        .bind(&id)
        .bind(name)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload)
         values ($1, 'git', 'branch', $2, '', jsonb_build_object('name', $2::text))",
    )
    .bind(&id)
    .bind(name)
    .execute(pool)
    .await
    .unwrap();
    id
}

/// Every configuration of the mirror's CI source as a pipeline member: the
/// probe's [`pipelines`](crate::responsiveness_probe::pipelines), the quiet
/// ones included.
fn pipeline_members() -> Vec<knobas_app::link_sets::LinkSetMember> {
    (0..crate::responsiveness_probe::pipelines())
        .map(|n| knobas_app::link_sets::LinkSetMember {
            kind: "pipeline".to_owned(),
            entity_id: format!("ci:buildType:Cfg_{n}"),
        })
        .collect()
}

/// What `plan_shape` finds in one of #2185's plans that reaches the run
/// corpus or the samples: a per-row read of `sync.item` or a table kept
/// beside it (`item_pipeline`, `item_revision`), or of
/// `knobas.monitor_sample`. What else these statements read -- a job's
/// reach, the pins, the assets, the open alerts -- is theirs from before
/// #2185, small or keyed, and not what this asserts.
fn corpus_shapes(name: &str, plan: &serde_json::Value) -> Vec<String> {
    shapes(name, plan)
        .into_iter()
        .filter(|f| {
            [" on item", " on monitor_sample"]
                .iter()
                .any(|on| f.contains(on))
        })
        .collect()
}

/// Every sequential scan of `relation` (#2185): the stored rows read whole,
/// where a pipeline's runs are one keyed range of `item_pipeline_runs_idx`.
fn whole_reads(node: &serde_json::Value, relation: &str) -> Vec<String> {
    let mut found = Vec::new();
    if node["Node Type"] == "Seq Scan" && node["Relation Name"] == relation {
        found.push(format!("reads {relation} whole"));
    }
    for child in node["Plans"].as_array().into_iter().flatten() {
        found.extend(whole_reads(child, relation));
    }
    found
}

/// `plan_shape::driven_from` inside the CTE `cte` of `node` (#2192), which
/// the checker itself does not look into. *This job*'s pipelines (`CTE p`),
/// planned from `frames` inlined, were found by probing every live item from
/// `knobas.entity` on tables never analyzed, where a pipeline kind's items are
/// one range of the item's source-and-kind index.
fn cte_driven_from(node: &serde_json::Value, cte: &str, relation: &str) -> Vec<String> {
    let mut found = Vec::new();
    for child in node["Plans"].as_array().into_iter().flatten() {
        if child["Subplan Name"] == cte {
            let one = serde_json::json!([{ "Plan": child }]);
            found.extend(
                knobas_db::plan_shape::driven_from(&one, relation)
                    .into_iter()
                    .map(|f| format!("{cte}: {f}")),
            );
        } else {
            found.extend(cte_driven_from(child, cte, relation));
        }
    }
    found
}

/// What `plan_shape` finds in one plan, named.
fn shapes(name: &str, plan: &serde_json::Value) -> Vec<String> {
    knobas_db::plan_shape::findings(plan, SMALL)
        .into_iter()
        .chain(knobas_db::plan_shape::driven_from(plan, "entity"))
        .map(|f| format!("{name}: {f}"))
        .collect()
}

/// Every probe into `sync.item` by key that filters the kind afterwards: the
/// statement then reaches the items of every kind first and keeps one kind
/// of them, a probe per item of the mirror (#981). An index condition on the
/// kind, or a scan that filters it once, is what reading one kind costs.
fn kind_filtered_probes(node: &serde_json::Value) -> Vec<String> {
    let mut found = Vec::new();
    let is_probe = matches!(
        node["Node Type"].as_str(),
        Some("Index Scan" | "Index Only Scan")
    );
    if is_probe
        && node["Relation Name"] == "item"
        && node["Filter"]
            .as_str()
            .is_some_and(|f| f.contains("(kind = "))
    {
        found.push(format!(
            "probes sync.item by {} and filters the kind after ({})",
            node["Index Name"].as_str().unwrap_or("an index"),
            node["Filter"].as_str().unwrap_or_default()
        ));
    }
    for child in node["Plans"].as_array().into_iter().flatten() {
        found.extend(kind_filtered_probes(child));
    }
    found
}

/// Every read of `knobas.link` not keyed by the row it is asked about (#800):
/// a scan of the table, or an index read whose condition names no entity.
/// The failed-build rule asks "is this red build confirmed-linked to my
/// work"; read as the whole set of confirmed links, joined to their items, it
/// probed `sync.item` once per link -- 12,000 on the probe, about 100 ms --
/// for a reader who owns none of them. Probed per red build, it is the few
/// links each one has.
fn unkeyed_link_reads(node: &serde_json::Value) -> Vec<String> {
    let mut found = Vec::new();
    let on_link = node["Relation Name"] == "link"
        || node["Index Name"]
            .as_str()
            .is_some_and(|index| index.starts_with("link_"));
    let keyed = node["Index Cond"]
        .as_str()
        .is_some_and(|cond| cond.contains("entity_id"));
    let reads = matches!(
        node["Node Type"].as_str(),
        Some("Seq Scan" | "Index Scan" | "Index Only Scan" | "Bitmap Index Scan")
    );
    if on_link && reads && !keyed {
        found.push(format!(
            "reads knobas.link unkeyed ({} {})",
            node["Node Type"].as_str().unwrap_or("?"),
            node["Index Name"].as_str().unwrap_or("")
        ));
    }
    for child in node["Plans"].as_array().into_iter().flatten() {
        found.extend(unkeyed_link_reads(child));
    }
    found
}

/// Every index read of `sync.item` keyed by an array of ids (#1149): an
/// index search per id, 5,000 for the probe's `has:blocker` board, where a
/// hashed set is one probe per ticket the scan reads anyway.
fn searches_per_id(node: &serde_json::Value) -> Vec<String> {
    let mut found = Vec::new();
    let reads = matches!(
        node["Node Type"].as_str(),
        Some("Index Scan" | "Index Only Scan" | "Bitmap Index Scan")
    );
    if reads
        && node["Index Name"] == "item_pkey"
        && node["Index Cond"]
            .as_str()
            .is_some_and(|cond| cond.contains("ANY"))
    {
        // The condition names every id; its head says which.
        let cond: String = node["Index Cond"]
            .as_str()
            .unwrap_or_default()
            .chars()
            .take(60)
            .collect();
        found.push(format!("searches item_pkey once per id ({cond}…)"));
    }
    for child in node["Plans"].as_array().into_iter().flatten() {
        found.extend(searches_per_id(child));
    }
    found
}

/// Every index read of `knobas.entity` (#1149): liveness as a probe per row,
/// `sync.live_item`'s join, where the tombstones read once is a hashed set.
fn entity_probes(node: &serde_json::Value) -> Vec<String> {
    let mut found = Vec::new();
    let probes = matches!(
        node["Node Type"].as_str(),
        Some("Index Scan" | "Index Only Scan" | "Bitmap Index Scan")
    );
    if probes
        && node["Index Name"]
            .as_str()
            .is_some_and(|index| index.starts_with("entity_"))
    {
        found.push(format!(
            "probes knobas.entity by {}",
            node["Index Name"].as_str().unwrap_or_default()
        ));
    }
    for child in node["Plans"].as_array().into_iter().flatten() {
        found.extend(entity_probes(child));
    }
    found
}

/// Every scan of `sync.item` whose filter runs a subplan per row (#1164): a
/// `SubPlan` in the filter that is not `hashed`, so not computed once. The
/// new-assignment rule read its declared assignee that way, the whole
/// declaration walked for every ticket in the window -- 58 ms on the probe's
/// 15,000 for a reader who raised none of them. The assignee is resolved in
/// the join now, for the tickets one of whose own source's paths names the
/// reader; a hashed exclusion set (`live_by_exclusion!`) is one read.
fn item_scans_filtered_per_row(node: &serde_json::Value) -> Vec<String> {
    let mut found = Vec::new();
    if node["Relation Name"] == "item"
        && let Some(filter) = node["Filter"].as_str()
        && filter.matches("SubPlan").count() > filter.matches("hashed SubPlan").count()
    {
        found.push(format!(
            "filters sync.item through a subplan per row ({} {filter})",
            node["Node Type"].as_str().unwrap_or("?")
        ));
    }
    for child in node["Plans"].as_array().into_iter().flatten() {
        found.extend(item_scans_filtered_per_row(child));
    }
    found
}

/// The trigram index over the prose kinds' bodies (migration `0049`).
const BODY_INDEX: &str = "item_body_trgm_idx";

/// Whether the plan finds its items through [`BODY_INDEX`] (#1170). The
/// mention rule read every prose item's body for the reader's name: 12 ms of
/// the stream on the probe's 18,500, for every reader, the one rule left that
/// cost the same whoever reads it. Through the index it reads the bodies that
/// hold the name's trigrams, which for a name nobody wrote is none.
fn reads_body_index(node: &serde_json::Value) -> bool {
    node["Node Type"] == "Bitmap Index Scan" && node["Index Name"] == BODY_INDEX
        || node["Plans"]
            .as_array()
            .into_iter()
            .flatten()
            .any(reads_body_index)
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
