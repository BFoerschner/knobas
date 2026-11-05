//! What the planner does with the coverage probes (#2497, #2500).
//!
//! The verdicts are pinned in `tests/coverage.rs` and `under_coverage.rs`. This
//! pins the cost, without a clock: a probe that answers right and reads the
//! table looks the same to those tests. Under `limit 1` the planner expects a
//! row soon, so a scan can look cheap, and a source with no row in its scope
//! reads all of it. On the benchmark's 100,000 items that was 4.6 ms for three
//! quiet sources, and at two parent rows an item, with one source holding them, the
//! plan moved to `sync.item` and read every item's parents: 750 ms on every
//! `under:` keystroke.
//!
//! So the plans are read in every state that moved them: `sync.item_parent`
//! never analyzed, as a fresh sync leaves it, analyzed, and analyzed at the
//! density a real corpus has, through custom plans and a generic one. A few
//! thousand rows are enough: the old statements fell into both traps at 3,000
//! items, a `Seq Scan on item_parent` once analyzed and a `Seq Scan on item` at
//! density, as they did at 100,000.
//!
//! Two sources here are shapes the benchmark's corpus cannot hold, and each cost
//! tens of milliseconds a keystroke with no answer wrong: a Gitea under its own
//! commits, which went to the grouped read (#2503), and one with a long run of
//! withdrawn child rows at the head of its range, which read them all (#2504).

use knobas_search::Searcher;
use knobas_search::testing::{
    self, PlanCache, left_for_the_grouped_read, newest_of_each_kind_plan, newest_rows_plan,
    under_coverage_plan,
};

const ITEMS: i64 = 3_000;
/// A Gitea whose newest rows are commits, which the launcher leaves out, with what
/// it keeps beneath them (#2503). `tests/it/under_coverage.rs` holds the verdict.
const UNDER_ITS_COMMITS: &str = "gitea-commits-on-top";
/// A Gitea with a corpus and a long run of withdrawn child rows ahead of its live
/// ones in the index (#2504).
const SWEPT: &str = "gitea-swept";
/// How many withdrawn commits [`SWEPT`] holds, at two child rows each.
const SWEPT_COMMITS: i64 = 500;
/// The other way round: its live child rows are at the head of the range, and the
/// far end, where the probe looks first, is the withdrawn run.
const SWEPT_LAST: &str = "gitea-swept-last";

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
    a_gitea(&pool, UNDER_ITS_COMMITS).await;
    rows_of(&pool, UNDER_ITS_COMMITS, "repo", "kept-repo", 3, "2 years", false).await;
    rows_of(&pool, UNDER_ITS_COMMITS, "branch", "kept-branch", 20, "1 year", false).await;
    rows_of(&pool, UNDER_ITS_COMMITS, "commit", "commit", 200, "-1 year", false).await;
    a_gitea(&pool, SWEPT).await;
    rows_of(&pool, SWEPT, "commit", "gone", SWEPT_COMMITS, "3 years", true).await;
    rows_of(&pool, SWEPT, "branch", "live", 20, "1 year", false).await;
    a_gitea(&pool, SWEPT_LAST).await;
    rows_of(&pool, SWEPT_LAST, "branch", "live", 20, "1 year", false).await;
    rows_of(&pool, SWEPT_LAST, "commit", "gone", SWEPT_COMMITS, "3 years", true).await;
    testing::prepare(&pool).await.unwrap();
    let (child_rows,): (i64,) = sqlx::query_as(
        "select count(*) from sync.item_parent where arm = 'child' and source_id = 'gitea'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(
        child_rows > 0,
        "no Gitea row names a parent: the fixture moved"
    );
    pool
}

async fn a_gitea(pool: &sqlx::PgPool, id: &str) {
    sqlx::query(
        "insert into knobas.source_config (id, kind, display_name, base_url, auth_kind)
         values ($1, 'gitea', $1, 'http://x', 'Pat')",
    )
    .bind(id)
    .execute(pool)
    .await
    .unwrap();
}

/// `count` rows of one kind under a source, written in one go and so together in
/// the child index, each naming a repository. `age` says how old they are.
async fn rows_of(
    pool: &sqlx::PgPool,
    source: &str,
    kind: &str,
    tag: &str,
    count: i64,
    age: &str,
    withdrawn: bool,
) {
    sqlx::query(
        "insert into knobas.entity (id, kind, title, deleted_at)
         select $1 || ':' || $3 || '-' || g, $2, 't', case when $5 then now() end
           from generate_series(1, $4::bigint) g",
    )
    .bind(source)
    .bind(kind)
    .bind(tag)
    .bind(count)
    .bind(withdrawn)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "insert into sync.item
           (entity_id, source_id, kind, title, body_text, author, item_updated_at, synced_at, payload)
         select $1 || ':' || $3 || '-' || g, $1, $2, 't', 't', 'mara.lindqvist',
                now() - $5::interval - make_interval(mins => g::int), now(),
                jsonb_build_object('repository', jsonb_build_object(
                  'name', $1 || '-repo', 'full_name', 'bench/' || $1 || '-repo'))
           from generate_series(1, $4::bigint) g",
    )
    .bind(source)
    .bind(kind)
    .bind(tag)
    .bind(count)
    .bind(age)
    .execute(pool)
    .await
    .unwrap();
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

/// On the index, whichever way it is read, and with no table read whole. Over a
/// few hundred child rows the planner can take a bitmap of the index, whose range
/// is as empty for a quiet source.
fn assert_stays_on(plan: &str, index: &str, state: &str) {
    assert!(plan.contains(index), "{state}: not on {index}:\n{plan}");
    for table in ["item_parent", "item ", "entity"] {
        assert!(
            !plan.contains(&format!("Seq Scan on {table}")),
            "{state}: a scan of {table}, which a source with nothing in range reads whole:\n{plan}"
        );
    }
}

/// How many rows came off the index over all the loops and every scan of it. This
/// is what the walk read, where a `Limit` above a filter would say only what got
/// through it. A scan the statement never reached read nothing.
fn rows_read_from(index: &str, plan: &str) -> f64 {
    let scans: Vec<&str> = plan.lines().filter(|line| line.contains(index)).collect();
    assert!(!scans.is_empty(), "no {index} in:\n{plan}");
    scans
        .into_iter()
        .filter(|line| !line.contains("never executed"))
        .map(|line| {
            let figure = |name: &str| -> f64 {
                line.split(name)
                    .nth(1)
                    .and_then(|rest| rest.split([' ', ')']).next())
                    .and_then(|n| n.parse().ok())
                    .unwrap_or_else(|| panic!("no {name} in {line}"))
            };
            figure("rows=") * figure("loops=")
        })
        .sum()
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
        "a source withdrawn whole",
    ] {
        match state {
            "two rows an item" => more_child_rows(&pool, 6).await,
            "ten rows an item" => more_child_rows(&pool, 24).await,
            "a source withdrawn whole" => {
                sqlx::query(
                    "update knobas.entity set deleted_at = now()
                      where id in (select entity_id from sync.item where source_id = 'jira')",
                )
                .execute(&pool)
                .await
                .unwrap();
                testing::prepare(&pool).await.unwrap();
            }
            _ => {}
        }
        if state != "never analyzed" {
            sqlx::query("analyze sync.item_parent")
                .execute(&pool)
                .await
                .unwrap();
        }
        // #2504. A run of withdrawn child rows costs a bounded look at whichever end
        // of the range it sits: 256 rows, and one more from the head when the far end
        // saw nothing live.
        for source in [SWEPT, SWEPT_LAST] {
            let ids = [source.to_owned()];
            for cache in [PlanCache::Custom, PlanCache::Generic] {
                let plan = under_coverage_plan(&pool, &ids, cache).await.unwrap();
                let read = rows_read_from("item_parent_child_idx", &plan);
                assert!(
                    read <= 257.0,
                    "under:, {state}, {cache:?}: read {read} child rows of {source}:\n{plan}"
                );
            }
        }
        // #2503. A source under its own left-out rows is settled without a read of it.
        let under_commits = [UNDER_ITS_COMMITS.to_owned()];
        let left = left_for_the_grouped_read(&pool, &under_commits, &[], &hidden, false)
            .await
            .unwrap();
        assert!(left.is_empty(), "under:, {state}: {left:?} read whole");
        for cache in [PlanCache::Custom, PlanCache::Generic] {
            for kinds in [
                vec![],
                vec!["branch".to_owned()],
                vec!["nothing".to_owned()],
            ] {
                let plan =
                    newest_of_each_kind_plan(&pool, &under_commits, &kinds, &hidden, cache)
                        .await
                        .unwrap();
                let at = format!("the look at each kind, {kinds:?}, {state}, {cache:?}");
                assert_stays_on(&plan, "item_kind_updated_idx", &at);
                // 64 rows for each of the seven kinds, and a row each to list them.
                let read = rows_read_from("item_kind_updated_idx", &plan);
                assert!(read <= 7.0 * 65.0, "{at}: read {read} rows:\n{plan}");
            }
        }
        // Thousands of child rows under one source: a bitmap would read them all
        // and sort them to give the first, so only the index's own order will do.
        let dense = state.ends_with("an item");
        for cache in [PlanCache::Custom, PlanCache::Generic] {
            let plan = under_coverage_plan(&pool, &ids, cache).await.unwrap();
            let at = format!("under:, {state}, {cache:?}");
            assert_stays_on(&plan, "item_parent_child_idx", &at);
            assert!(
                !dense || !plan.contains("Bitmap Heap Scan on item_parent"),
                "{at}: reads the source's whole range:\n{plan}"
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
                assert_stays_on(&plan, "item_source_updated_idx", &at);
                // Three of the sources hold rows, and the fourth is quiet.
                let read = rows_read_from("item_source_updated_idx", &plan);
                assert!(
                    read > 0.0 && read <= 3.0 * 64.0,
                    "{at}: read {read} rows:\n{plan}"
                );
            }
        }
    }
}
