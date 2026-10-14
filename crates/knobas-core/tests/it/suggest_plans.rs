//! Every statement of the suggestion pass plans the same way with and without
//! planner statistics (#770).
//!
//! #770 measured the pass on the responsiveness probe's mirror. On tables
//! never analyzed -- a fresh install for about a minute after its first sync
//! -- the exact-key rules were planned from `knobas.entity`, estimated at 84
//! live rows for 22,000: every ticket against every entity, past the pass's
//! statement limit in its first rule, so the pass proposed nothing at all.
//! `similar_text` joined its batch's candidates to the settled corpus by a
//! nested loop over a CTE scan (144 million rows a batch). And with `knobas.link`
//! analyzed while it was nearly empty, the suppression could be hashed on
//! `relation` alone, which compares every candidate with every link.
//!
//! A wall clock in the gate would measure the machine, so this asserts the
//! shapes those plans had instead, on one fixture in both states:
//!
//! * nothing executed once per row -- the inner side of a nested loop, or a
//!   correlated subplan that is not hashed -- scans a table or a CTE. An index
//!   probe per row is what a lookup should cost; a scan per row is the
//!   quadratic plan. The tables and CTEs small by construction (`SMALL`: the
//!   configured sources, the curated estate, one batch, the ticket
//!   namespaces) are allowed;
//! * no index scan on the inner side of a nested loop is linked to the outer
//!   row only by a range (#771): `similar_text`'s third arm walked
//!   `similar_text_seen_pkey` for `entity_id < o.entity_id`, half the table per
//!   batch item, its GIN-indexed lexemes only a filter on the walk;
//! * no anti-join is hashed on part of its key: on `relation` without both of
//!   the pair's ids, or with any of the key left to a join filter (#771).
//!
//! The checker is `knobas_db::plan_shape`, shared with the reads (#778).
//!
//! `rank_similar` is planned with seq scans off, as the pass runs it
//! (`PlannedStatement::no_seq_scan`).

use knobas_core::suggest;
use sqlx::PgPool;

/// Ticket keys and namespaces enough that a quadratic plan is the planner's
/// choice on tables never analyzed, small enough to seed in a second.
const TICKETS: i32 = 10_000;
const PAGES: i32 = 3_000;

/// A mirror with every rule's inputs.
async fn seed_mirror(pool: &PgPool) {
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("select setseed(0.770)")
        .execute(&mut *tx)
        .await
        .unwrap();
    // `g * 0` makes each word subquery correlated, so it runs once per row
    // rather than once for the whole series.
    sqlx::query(
        "create temporary table seed on commit drop as
         select g, source, id, kind,
                (select string_agg('w' || (random() * 4000)::int, ' ')
                   from generate_series(1, 6 + g * 0)) as title,
                (select string_agg('w' || (random() * 4000)::int, ' ')
                   from generate_series(1, 40 + g * 0)) as body
           from (select g, 'jira' as source, 'jira:PAY-' || g as id, 'ticket' as kind
                   from generate_series(1, $1) g
                 union all
                 select g, 'wiki', 'wiki:page:' || g, 'page' from generate_series(1, $2) g
                 union all
                 select g, 'gitea', 'gitea:commit:' || g, 'commit' from generate_series(1, 2000) g
                 union all
                 select g, 'gitea', 'gitea:branch:' || g, 'branch' from generate_series(1, 500) g
                 union all
                 select g, 'ci', 'ci:build:' || g, 'build' from generate_series(1, 5000) g
                 union all
                 select g, 'kuma', 'kuma:monitor:' || g, 'monitor' from generate_series(1, 20) g) s",
    )
    .bind(TICKETS)
    .bind(PAGES)
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query("insert into knobas.entity (id, kind, title) select id, kind, title from seed")
        .execute(&mut *tx)
        .await
        .unwrap();
    // Keys in the commits, branches and builds; a recorded relation on a
    // fifth of the tickets; a URL on each monitor.
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload)
         select id, source, kind,
                case when kind in ('commit', 'branch', 'build')
                     then 'PAY-' || (1 + g * 7 % $1) || ' ' || title else title end,
                body,
                case when kind = 'ticket' and g % 5 = 0
                     then jsonb_build_object('fields', jsonb_build_object('issuelinks',
                            jsonb_build_array(jsonb_build_object(
                              'type', jsonb_build_object('name', 'Blocks', 'outward', 'blocks'),
                              'outwardIssue', jsonb_build_object('key', 'PAY-' || (g + 1))))))
                     when kind = 'monitor'
                     then jsonb_build_object('url', 'http://host' || g || ':3000/')
                     else '{}' end
           from seed",
    )
    .bind(TICKETS)
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
}

/// What a pass leaves behind, written directly rather than by running one
/// (a pass over this mirror is minutes, not a gate test's seconds): every
/// prose item settled, a proposal per prose item in `knobas.link`, and then a
/// hundred items changed -- the next pass after a sync, with a settled corpus
/// to compare a batch against and proposals to suppress against.
async fn settle_and_change(pool: &PgPool) {
    let prose: Vec<String> = sqlx::query_scalar(
        "select entity_id from sync.item where kind in ('ticket', 'page') order by 1",
    )
    .fetch_all(pool)
    .await
    .unwrap();
    let mark = suggest::planned_statements()
        .into_iter()
        .find(|s| s.name == "mark_seen")
        .expect("planned_statements names MARK_SEEN `mark_seen`");
    sqlx::query(mark.sql)
        .bind(&prose)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into knobas.link
                (from_id, to_id, relation, origin, created_by, confirmed_at, rule, rule_class, reason)
         select 'jira:PAY-' || g, 'wiki:page:' || (1 + g % $2), 'related', 'suggested', 'knobas',
                null, 'similar_text', 'similarity', 'both mention w1'
           from generate_series(1, $1) g",
    )
    .bind(TICKETS)
    .bind(PAGES)
    .execute(pool)
    .await
    .unwrap();
    // Ten ranked counterparts per prose item (#771), the digest each was
    // ranked with its own: the rankings a changed item holds a place in.
    sqlx::query(
        "insert into sync.similar_text_top (entity_id, other_id, shared, other_at, other_digest)
         select s.entity_id, o.entity_id, 5 + n, now(), s.digest
           from sync.similar_text_seen s
           cross join generate_series(1, 10) n
           join sync.similar_text_seen o
             on o.entity_id = (array['jira:PAY-', 'wiki:page:'])[1 + n % 2]
                              || (1 + (hashtext(s.entity_id) & 1023) * 7 + n)
          where o.entity_id <> s.entity_id
         on conflict do nothing",
    )
    .execute(pool)
    .await
    .unwrap();
    // Every prose item counted (#776), so the changed ones below are taken
    // off the counts and counted again.
    let count = suggest::planned_statements()
        .into_iter()
        .find(|s| s.name == "count_arrived")
        .expect("planned_statements names COUNT_ARRIVED `count_arrived`");
    sqlx::query(count.sql).execute(pool).await.unwrap();
    sqlx::query(
        "update sync.item set title = title || ' w4001'
          where entity_id in (select 'jira:PAY-' || g from generate_series(1, 100) g)",
    )
    .execute(pool)
    .await
    .unwrap();
}

/// The plan of one statement, as `explain (format json)` gives it.
async fn plan(
    pool: &PgPool,
    statement: &suggest::PlannedStatement,
    batch: &[String],
) -> serde_json::Value {
    let mut conn = pool.acquire().await.unwrap();
    let seq_scans = if statement.no_seq_scan { "off" } else { "on" };
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "set enable_seqscan = {seq_scans}"
    )))
    .execute(&mut *conn)
    .await
    .unwrap();
    let explain = format!("explain (format json) {}", statement.sql);
    let query = sqlx::query_scalar::<_, serde_json::Value>(sqlx::AssertSqlSafe(explain));
    let query = match statement.binds {
        suggest::Binds::Rule => query
            .bind("suggested")
            .bind("knobas")
            .bind(statement.name)
            .bind(statement.class),
        suggest::Binds::RuleAndBatch => query
            .bind("suggested")
            .bind("knobas")
            .bind(statement.name)
            .bind(statement.class)
            .bind(batch),
        suggest::Binds::Batch => query.bind(batch),
        suggest::Binds::Nothing => query,
    };
    query.fetch_one(&mut *conn).await.unwrap()
}

/// Tables and CTEs small by construction, which a plan may scan per row: the
/// sources the user configured, the estate the user curates (`asset`,
/// `route`), the one batch `similar_text` compares, and the namespaces the
/// tickets come from -- one per ticket source.
const SMALL: &[&str] = &["source_config", "asset", "route", "batch", "ticket_ns"];

async fn assert_every_plan_holds(pool: &PgPool, state: &str) {
    let batch: Vec<String> = sqlx::query_scalar(
        "select entity_id from sync.item
          where entity_id like 'jira:PAY-%' and title like '%w4001' order by entity_id",
    )
    .fetch_all(pool)
    .await
    .unwrap();
    assert_eq!(batch.len(), 100, "the fixture's changed items");
    let settled: i64 = sqlx::query_scalar("select count(*) from sync.similar_text_seen")
        .fetch_one(pool)
        .await
        .unwrap();
    assert!(
        settled > i64::from(TICKETS),
        "the fixture has a settled corpus: {settled}"
    );
    let ranked: i64 = sqlx::query_scalar(
        "select count(*) from sync.similar_text_top
          where other_id in (select entity_id from sync.item where title like '%w4001')",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert!(
        ranked > 0,
        "the fixture has rankings that hold a changed item: {ranked}"
    );
    let links: i64 = sqlx::query_scalar("select count(*) from knobas.link")
        .fetch_one(pool)
        .await
        .unwrap();
    assert!(
        links > 1_000,
        "the fixture has proposals to suppress against: {links}"
    );

    let mut wrong = Vec::new();
    for statement in suggest::planned_statements() {
        let json = plan(pool, &statement, &batch).await;
        let found = knobas_db::plan_shape::findings(&json, SMALL);
        wrong.extend(
            found
                .into_iter()
                .map(|f| format!("{}: {f}", statement.name)),
        );
    }
    assert!(wrong.is_empty(), "tables {state}: {wrong:#?}");
}

#[tokio::test]
async fn every_pass_statement_plans_by_index_on_tables_never_analyzed() {
    let pool = knobas_db::test_util::scratch_database("plans_na")
        .await
        .pool(2)
        .await
        .unwrap();
    knobas_db::plan_shape::never_analyzed(&pool).await;
    seed_mirror(&pool).await;
    settle_and_change(&pool).await;
    let analyzed: i64 = sqlx::query_scalar(
        "select count(*) from pg_stat_user_tables
          where schemaname in ('knobas', 'sync')
            and (last_analyze is not null or last_autoanalyze is not null)",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(analyzed, 0, "no table of the fixture is analyzed");
    assert_every_plan_holds(&pool, "never analyzed").await;
}

/// Analyzed once the mirror is synced and before the pass -- what autovacuum
/// does on a real install, and what the probe's `RP_ANALYZED=1` does -- so the
/// statistics say `knobas.link` and `sync.similar_text_seen` are empty when
/// they are not; then analyzed again, with statistics that are current.
#[tokio::test]
async fn every_pass_statement_plans_by_index_on_analyzed_tables() {
    let pool = knobas_db::test_util::scratch_database("plans_an")
        .await
        .pool(2)
        .await
        .unwrap();
    knobas_db::plan_shape::never_analyzed(&pool).await;
    seed_mirror(&pool).await;
    sqlx::query("analyze").execute(&pool).await.unwrap();
    settle_and_change(&pool).await;
    assert_every_plan_holds(&pool, "analyzed before the pass").await;
    sqlx::query("analyze").execute(&pool).await.unwrap();
    assert_every_plan_holds(&pool, "analyzed after the pass").await;
}
