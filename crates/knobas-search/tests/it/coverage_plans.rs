//! What the planner does with the coverage probes (#2497, #2500).
//!
//! The verdicts are pinned in `tests/coverage.rs` and `under_coverage.rs`. This
//! pins the cost, without a clock: a probe that answers right and reads the
//! table looks the same to those tests. Under `limit 1` the planner expects a
//! row soon, so a scan can look cheap, and a source with no row in its scope
//! reads all of it. On the benchmark's 100,000 items that was 4.6 ms for three
//! quiet sources, and at two parent rows an item the plan moved to `sync.item`
//! and read every item's parents: 784 ms on every `under:` keystroke.
//!
//! So the plans are read in every state that moved them: `sync.item_parent`
//! never analyzed, as a fresh sync leaves it, analyzed, and analyzed at the
//! density a real corpus has, through custom plans and a generic one. A few
//! thousand rows are enough: the choice comes from proportions, and the
//! benchmark's plans were the same at 3,000 items as at 100,000.

use knobas_search::Searcher;
use knobas_search::testing::{self, PlanCache, newest_rows_plan, under_coverage_plan};

const ITEMS: i64 = 3_000;

/// The benchmark's parents, in proportion: a Gitea source's commits and pull
/// requests name a repository, and the other sources name nothing.
async fn corpus_with_parents() -> sqlx::PgPool {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    testing::seed_sources(&pool).await.unwrap();
    Searcher::declaring(pool.clone(), [knobas_source_gitea::descriptor_template()])
        .await
        .unwrap();
    testing::seed_corpus(&pool, ITEMS).await.unwrap();
    sqlx::query(
        "update sync.item i
            set payload = i.payload || jsonb_build_object('repository', jsonb_build_object(
                  'name', 'repo-' || (i.payload->>'n')::int % 41,
                  'full_name', 'bench/repo-' || (i.payload->>'n')::int % 41))
          where i.source_id = 'gitea' and i.kind = 'commit'",
    )
    .execute(&pool)
    .await
    .unwrap();
    // A second Gitea with nothing synced: the source this is about.
    sqlx::query(
        "insert into knobas.source_config (id, kind, display_name, base_url, auth_kind)
         values ('gitea-quiet', 'gitea', 'Quiet Gitea', 'http://x', 'Pat')",
    )
    .execute(&pool)
    .await
    .unwrap();
    testing::prepare(&pool).await.unwrap();
    pool
}

/// A child-arm row for every Gitea item, `per_item` of them each.
async fn more_child_rows(pool: &sqlx::PgPool, per_item: i64) {
    sqlx::query(
        "insert into sync.item_parent (entity_id, adapter_kind, name, kind, arm, source_id)
         select i.entity_id, 'gitea', 'dense-' || k || '-' || i.entity_id, 'repo', 'child', 'gitea'
           from sync.item i cross join generate_series(1, $1::bigint) k
          where i.source_id = 'gitea'",
    )
    .bind(per_item)
    .execute(pool)
    .await
    .unwrap();
}

/// On the index, whichever way it is read: over a few hundred child rows the
/// planner can take a bitmap of it, whose range is as empty for a quiet source.
fn assert_stays_on(plan: &str, index: &str, table: &str, state: &str) {
    assert!(plan.contains(index), "{state}: not on {index}:\n{plan}");
    assert!(
        !plan.contains(&format!("Seq Scan on {table}")) && !plan.contains("Seq Scan on item "),
        "{state}: a scan, which a source with nothing in range reads whole:\n{plan}"
    );
}

/// How many rows the walk's `Limit` passed up over all its loops.
fn rows_out_of_the_limit(plan: &str) -> f64 {
    let line = plan
        .lines()
        .find(|line| line.contains("Limit (actual"))
        .unwrap_or_else(|| panic!("no Limit in:\n{plan}"));
    let figure = |name: &str| -> f64 {
        line.split(name)
            .nth(1)
            .and_then(|rest| rest.split([' ', ')']).next())
            .and_then(|n| n.parse().ok())
            .unwrap_or_else(|| panic!("no {name} in {line}"))
    };
    figure("rows=") * figure("loops=")
}

/// One test and not two, because each step changes what the next one plans over.
///
/// A quiet source proves it names no parent from an empty range of the index,
/// and the walk costs the same whatever a source holds and whatever the query
/// asks: the source's newest rows and no more. The three kind scopes are the
/// shapes whose custom plans differ, and a generic plan sees none of them.
#[tokio::test]
async fn a_probe_never_reads_a_table_to_prove_a_source_has_nothing() {
    let pool = corpus_with_parents().await;
    let ids: Vec<String> = ["gitea", "gitea-quiet", "jira", "teamcity"]
        .map(str::to_owned)
        .to_vec();
    let hidden = vec!["commit".to_owned()];

    for state in [
        "never analyzed",
        "analyzed",
        "two rows an item",
        "ten rows an item",
    ] {
        match state {
            "two rows an item" => more_child_rows(&pool, 6).await,
            "ten rows an item" => more_child_rows(&pool, 24).await,
            _ => {}
        }
        if state != "never analyzed" {
            sqlx::query("analyze sync.item_parent")
                .execute(&pool)
                .await
                .unwrap();
        }
        for cache in [PlanCache::Custom, PlanCache::Generic] {
            let plan = under_coverage_plan(&pool, &ids, cache).await.unwrap();
            assert_stays_on(
                &plan,
                "item_parent_child_idx",
                "item_parent",
                &format!("under:, {state}, {cache:?}"),
            );
            for kinds in [
                vec![],
                vec!["ticket".to_owned()],
                vec!["nothing".to_owned()],
            ] {
                let plan = newest_rows_plan(&pool, &ids, &kinds, &hidden, cache)
                    .await
                    .unwrap();
                let at = format!("the walk, {kinds:?}, {state}, {cache:?}");
                assert_stays_on(&plan, "item_source_updated_idx", "item ", &at);
                // Three of the sources hold rows, and the fourth is quiet.
                let read = rows_out_of_the_limit(&plan);
                assert!(read <= 3.0 * 64.0, "{at}: read {read} rows:\n{plan}");
            }
        }
    }
}
