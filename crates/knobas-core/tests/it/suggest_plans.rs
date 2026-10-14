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
//! * no anti-join is hashed on `relation` without the pair's ids.

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
    sqlx::query(
        "update sync.item set title = title || ' w4001'
          where entity_id in (select 'jira:PAY-' || g from generate_series(1, 100) g)",
    )
    .execute(pool)
    .await
    .unwrap();
}

/// Autovacuum off on every table of the database, so nothing analyzes them
/// behind the test's back: the method #767 and #768 used.
async fn never_analyzed(pool: &PgPool) {
    let tables: Vec<(String, String)> = sqlx::query_as(
        "select n.nspname::text, c.relname::text from pg_class c
           join pg_namespace n on n.oid = c.relnamespace
          where c.relkind = 'r' and n.nspname in ('knobas', 'sync')",
    )
    .fetch_all(pool)
    .await
    .unwrap();
    for (schema, table) in tables {
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "alter table {schema}.{table} set (autovacuum_enabled = false)"
        )))
        .execute(pool)
        .await
        .unwrap();
    }
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

/// Every string anywhere in `value`.
fn strings<'a>(value: &'a serde_json::Value, out: &mut Vec<&'a str>) {
    match value {
        serde_json::Value::String(s) => out.push(s),
        serde_json::Value::Array(items) => items.iter().for_each(|v| strings(v, out)),
        serde_json::Value::Object(map) => map.values().for_each(|v| strings(v, out)),
        _ => {}
    }
}

fn children(node: &serde_json::Value) -> impl Iterator<Item = &serde_json::Value> {
    node["Plans"].as_array().into_iter().flatten()
}

fn describe(node: &serde_json::Value) -> String {
    format!(
        "{} on {}",
        node["Node Type"].as_str().unwrap_or("?"),
        node["Relation Name"]
            .as_str()
            .or(node["CTE Name"].as_str())
            .unwrap_or("-")
    )
}

/// Tables and CTEs small by construction, which a plan may scan per row: the
/// sources the user configured, the estate the user curates (`asset`,
/// `route`), the one batch `similar_text` compares, and the namespaces the
/// tickets come from -- one per ticket source.
const SMALL: &[&str] = &["source_config", "asset", "route", "batch", "ticket_ns"];

/// What `node` and everything under it would scan once per row of an outer
/// side: every table or CTE scan but the [`SMALL`] ones, not descending into
/// init plans or hashed subplans, which run once.
fn scans_per_row(node: &serde_json::Value, hashed: &[String], out: &mut Vec<String>) {
    let scanned = match node["Node Type"].as_str() {
        Some("Seq Scan") => node["Relation Name"].as_str(),
        Some("CTE Scan") => node["CTE Name"].as_str(),
        _ => None,
    };
    if scanned.is_some_and(|name| !SMALL.contains(&name)) {
        out.push(describe(node));
    }
    for child in children(node) {
        if child["Parent Relationship"] != "InitPlan" && !is_hashed(child, hashed) {
            scans_per_row(child, hashed, out);
        }
    }
}

fn is_hashed(node: &serde_json::Value, hashed: &[String]) -> bool {
    node["Parent Relationship"] == "SubPlan"
        && hashed.iter().any(|h| node["Subplan Name"] == h.as_str())
}

/// The aliases the scans under `node` give their rows.
fn aliases(node: &serde_json::Value, out: &mut Vec<String>) {
    if let Some(alias) = node["Alias"].as_str() {
        out.push(alias.to_owned());
    }
    for child in children(node) {
        aliases(child, out);
    }
}

/// Whether anything under `node` (init plans aside) names a column of one of
/// `aliases`: what makes the inner side of a nested loop a probe
/// parameterized by the outer row rather than the same rows every time.
fn mentions(node: &serde_json::Value, aliases: &[String]) -> bool {
    let mut texts = Vec::new();
    strings(node, &mut texts);
    texts.iter().any(|text| {
        aliases.iter().any(|alias| {
            text.contains(&format!("\"{alias}\"."))
                || text.match_indices(&format!("{alias}.")).any(|(i, _)| {
                    !text[..i]
                        .chars()
                        .next_back()
                        .is_some_and(|c| c.is_alphanumeric() || c == '_')
                })
        })
    })
}

/// The tables and CTEs under `node` but the [`SMALL`] ones, init plans aside.
fn relations(node: &serde_json::Value, out: &mut Vec<String>) {
    let name = node["Relation Name"].as_str().or(node["CTE Name"].as_str());
    if name.is_some_and(|name| !SMALL.contains(&name)) {
        out.push(describe(node));
    }
    for child in children(node) {
        if child["Parent Relationship"] != "InitPlan" {
            relations(child, out);
        }
    }
}

/// Whether an index condition's clause compares by a range: `<`, `>`, `<=`,
/// `>=`, and nothing that narrows to a key (`=`, `@@`, `&&`).
fn is_range(clause: &str) -> bool {
    [" < ", " > ", " <= ", " >= "]
        .iter()
        .any(|op| clause.contains(op))
}

/// The index scans under `node` (init plans and hashed subplans aside) whose
/// index condition reaches the rows around it (`around`: the loop's outer
/// side and the inner side's other scans) only through a range, or not at all:
/// each outer row then walks a slice of the index the range leaves -- about
/// half the table for `entity_id < o.entity_id` -- and filters it, which is the
/// scan per row in an index's clothing (#771).
fn linked_only_by_a_range(
    node: &serde_json::Value,
    hashed: &[String],
    around: &[String],
    out: &mut Vec<String>,
) {
    let is_index = matches!(
        node["Node Type"].as_str(),
        Some("Index Scan" | "Index Only Scan" | "Bitmap Index Scan")
    );
    let small = node["Relation Name"]
        .as_str()
        .is_some_and(|name| SMALL.contains(&name));
    if is_index && !small {
        let cond = node["Index Cond"].as_str().unwrap_or("");
        let own = node["Alias"].as_str().unwrap_or("");
        let others: Vec<String> = around.iter().filter(|a| *a != own).cloned().collect();
        // A clause on a subplan's answer computed inside the inner side --
        // the ticket lookup's `id = ANY ((InitPlan n).col1)`, built from the
        // key the outer row found -- is a link too.
        let linked = cond.split(" AND ").any(|clause| {
            let reaches = mentions(&serde_json::Value::String(clause.to_owned()), &others)
                || clause.contains("(InitPlan ")
                || clause.contains("(SubPlan ");
            reaches && !is_range(clause)
        });
        if !linked {
            out.push(format!(
                "{} using {} ({})",
                describe(node),
                node["Index Name"].as_str().unwrap_or("?"),
                if cond.is_empty() {
                    "no condition"
                } else {
                    cond
                }
            ));
        }
    }
    for child in children(node) {
        if child["Parent Relationship"] != "InitPlan" && !is_hashed(child, hashed) {
            linked_only_by_a_range(child, hashed, around, out);
        }
    }
}

/// Every per-row scan and every anti-join hashed without the pair's ids.
fn findings(node: &serde_json::Value, hashed: &[String], out: &mut Vec<String>) {
    if node["Node Type"] == "Nested Loop" {
        let mut outer_aliases = Vec::new();
        for outer in children(node).filter(|c| c["Parent Relationship"] == "Outer") {
            aliases(outer, &mut outer_aliases);
        }
        for inner in children(node).filter(|c| c["Parent Relationship"] == "Inner") {
            let mut scans = Vec::new();
            scans_per_row(inner, hashed, &mut scans);
            out.extend(
                scans
                    .into_iter()
                    .map(|s| format!("a nested loop rescans {s}")),
            );
            // Linked means to anything the loop's rows carry: the outer side,
            // or a sibling scan inside the inner side the index is probed for.
            let mut around = outer_aliases.clone();
            aliases(inner, &mut around);
            let mut ranged = Vec::new();
            linked_only_by_a_range(inner, hashed, &around, &mut ranged);
            out.extend(ranged.into_iter().map(|r| {
                format!("a nested loop walks {r} per outer row, linked to it only by a range")
            }));
            if !mentions(inner, &outer_aliases) {
                let mut read = Vec::new();
                relations(inner, &mut read);
                out.extend(
                    read.into_iter()
                        .map(|r| format!("a nested loop crosses its outer side with {r}")),
                );
            }
        }
    }
    if node["Parent Relationship"] == "SubPlan" && !is_hashed(node, hashed) {
        let mut scans = Vec::new();
        scans_per_row(node, hashed, &mut scans);
        out.extend(
            scans
                .into_iter()
                .map(|s| format!("a correlated subplan rescans {s}")),
        );
    }
    if node["Node Type"] == "Hash Join"
        && node["Join Type"]
            .as_str()
            .is_some_and(|t| t.contains("Anti"))
    {
        let cond = node["Hash Cond"].as_str().unwrap_or("");
        if cond.contains("relation") && !(cond.contains("from_id") && cond.contains("to_id")) {
            out.push(format!("an anti-join hashed on {cond}"));
        }
        // Part of the key left to a filter: every row of a hash bucket is
        // compared with it, which is the `relation`-only hash of #770 in any
        // other spelling.
        if let Some(filter) = node["Join Filter"].as_str() {
            out.push(format!(
                "an anti-join hashed on {cond} leaves part of its key to a filter: {filter}"
            ));
        }
    }
    for child in children(node) {
        findings(child, hashed, out);
    }
}

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
        let mut texts = Vec::new();
        strings(&json, &mut texts);
        let hashed: Vec<String> = texts
            .iter()
            .flat_map(|t| {
                t.match_indices("hashed SubPlan ")
                    .map(|(i, _)| &t[i + "hashed ".len()..])
            })
            .map(|rest| {
                rest.chars()
                    .take_while(|c| c.is_alphanumeric() || *c == ' ')
                    .collect::<String>()
            })
            .collect();
        let mut found = Vec::new();
        findings(&json[0]["Plan"], &hashed, &mut found);
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
    never_analyzed(&pool).await;
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
    never_analyzed(&pool).await;
    seed_mirror(&pool).await;
    sqlx::query("analyze").execute(&pool).await.unwrap();
    settle_and_change(&pool).await;
    assert_every_plan_holds(&pool, "analyzed before the pass").await;
    sqlx::query("analyze").execute(&pool).await.unwrap();
    assert_every_plan_holds(&pool, "analyzed after the pass").await;
}
