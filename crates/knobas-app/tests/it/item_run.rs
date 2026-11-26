//! `sync.item_run` (migration `0088`): what Builds reads of a run, and a
//! commit's own sha, as the database keeps them, held to the payload walks
//! they replace for every field of every run and commit of the reference
//! adapter, TeamCity and Gitea, kept through every kind of write, and the
//! Builds reads that take them held to their answers without them.

use knobas_app::commands::entity::{pipeline_runs_inner, pipelines_inner, repo_pipelines_inner};
use knobas_app::sources::Registry;
use serde_json::{Value, json};
use sqlx::PgPool;

/// A database of the test's own: the adapters' declarations stored as
/// bring-up stores them (ADR-0019), then the demo corpus (the reference
/// adapter's builds, pipelines and commits), a TeamCity source's
/// configuration and builds with every field the list reads, and a Gitea
/// source's repository and the commits the builds ran on. Returns the
/// TeamCity source, the Gitea one and the configuration's entity.
async fn mirror(label: &str) -> (PgPool, String, String, String) {
    let pool = knobas_db::test_util::scratch_database(label)
        .await
        .pool(4)
        .await
        .expect("a pool on the scratch database");
    knobas_app::commands::search::searcher(&pool, &Registry::for_process())
        .await
        .unwrap();
    knobas_app::sources::demo::demo_load_inner(&pool)
        .await
        .unwrap();
    let tc = "tc-runs".to_owned();
    let gitea = "gitea-runs".to_owned();
    crate::support::source_config(&pool, &tc, "teamcity", "TeamCity", &json!({})).await;
    crate::support::source_config(&pool, &gitea, "gitea", "Gitea", &json!({})).await;
    write(
        &pool,
        &gitea,
        "repo",
        "repo:tidewater/payout-service",
        json!({ "full_name": "tidewater/payout-service", "name": "payout-service", "default_branch": "main" }),
    )
    .await;
    for sha in ["a41f2c0b9e11", "b52e3d1c0f22"] {
        write(
            &pool,
            &gitea,
            "commit",
            &format!("commit:tidewater/payout-service/{sha}"),
            json!({ "sha": sha, "repository": { "full_name": "tidewater/payout-service" } }),
        )
        .await;
    }
    let config = write(
        &pool,
        &tc,
        "build_config",
        "buildType:Payout_Tests",
        json!({ "id": "Payout_Tests", "name": "Tests", "projectId": "Payout", "projectName": "Payout" }),
    )
    .await;
    for (id, more) in [
        (
            1,
            json!({
                "state": "running", "status": "SUCCESS", "agent": { "name": "agent-linux-03" },
                "running-info": { "currentStageText": "step 3/5 `cargo test`" },
                "revisions": { "revision": [{ "version": "a41f2c0b9e11" }] },
                "triggered": { "type": "vcs" },
            }),
        ),
        (
            2,
            json!({
                "state": "finished", "status": "FAILURE", "finishDate": "20260821T100412+0000",
                "agent": { "name": "agent-linux-01" },
                "revisions": { "revision": [{ "version": "b52e3d1c0f22" }, { "version": "ffff" }] },
                "triggered": { "type": "user", "user": { "username": "pat" } },
                "testOccurrences": { "failed": 2, "passed": 40, "ignored": 1 },
            }),
        ),
        // Nothing the list reads past the pipeline: every field misses.
        (3, json!({ "state": "queued" })),
    ] {
        let mut run = json!({
            "id": id, "number": id.to_string(), "buildTypeId": "Payout_Tests",
            "branchName": "main", "startDate": "20260821T100000+0000"
        });
        if id == 3 {
            run = json!({ "id": id, "buildTypeId": "Payout_Tests" });
        }
        run.as_object_mut()
            .unwrap()
            .extend(more.as_object().unwrap().clone());
        write(&pool, &tc, "build", &format!("build:{id}"), run).await;
    }
    (pool, tc, gitea, config)
}

/// One mirrored item of `source`, inserted or rewritten.
async fn write(pool: &PgPool, source: &str, kind: &str, key: &str, payload: Value) -> String {
    let entity = format!("{source}:{key}");
    sqlx::query(
        "insert into knobas.entity (id, kind, title) values ($1, $2, $3)
         on conflict (id) do nothing",
    )
    .bind(&entity)
    .bind(kind)
    .bind(key)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload, item_updated_at)
         values ($1, $2, $3, $4, '', $5, now())
         on conflict (entity_id) do update set payload = excluded.payload, kind = excluded.kind",
    )
    .bind(&entity)
    .bind(source)
    .bind(kind)
    .bind(key)
    .bind(payload)
    .execute(pool)
    .await
    .unwrap();
    entity
}

/// Every item against its stored run row, read as the Builds reads read it
/// (`run_keys!`, `run_join!`) under the declarations the app binds: how many
/// items there are, how many of a stored kind have no row, how many rows
/// hold a field that is not the payload walk's answer, and how many rows
/// hold each field at all.
const HELD_TO_THE_WALK: &str = concat!(
    "with ",
    knobas_core::run_keys!("$1"),
    " select count(*) as items,
             count(*) filter (where ru_k.k is not null and ru.entity_id is null
                              and i.kind in (select a.kind from sync.run_adapters() a)) as unstored,
             count(*) filter (where ru.entity_id is not null and (
                   ru.pkey is distinct from ",
    knobas_core::declared_string!("$1", "pipeline"),
    "           or ru.number is distinct from ",
    knobas_core::declared_string!("$1", "number"),
    "           or ru.state is distinct from ",
    knobas_core::declared_state!("$1"),
    "           or ru.branch is distinct from ",
    knobas_core::declared_string!("$1", "branch"),
    "           or ru.started is distinct from ",
    knobas_core::declared_string!("$1", "started"),
    "           or ru.finished is distinct from ",
    knobas_core::declared_string!("$1", "finished"),
    "           or ru.trig is distinct from ",
    knobas_core::declared_trigger!("$1"),
    "           or ru.started_by is distinct from ",
    knobas_core::declared_string!("$1", "started_by"),
    "           or ru.agent is distinct from ",
    knobas_core::declared_string!("$1", "agent"),
    "           or ru.current_step is distinct from ",
    knobas_core::declared_string!("$1", "current_step"),
    "           or ru.tests_failed is distinct from ",
    knobas_core::declared_integer!("$1", "tests_failed"),
    "           or ru.tests_passed is distinct from ",
    knobas_core::declared_integer!("$1", "tests_passed"),
    "           or ru.tests_ignored is distinct from ",
    knobas_core::declared_integer!("$1", "tests_ignored"),
    "           or ru.revision is distinct from (select l.value from ",
    knobas_core::declared_list!("$1", "revision"),
    " l limit 1)
                or ru.sha is distinct from ",
    knobas_core::declared_string!("$1", "commit"),
    ")) as differing,
             array[count(ru.pkey), count(ru.number), count(ru.state), count(ru.branch),
                   count(ru.started), count(ru.finished), count(ru.trig), count(ru.started_by),
                   count(ru.agent), count(ru.current_step), count(ru.tests_failed),
                   count(ru.tests_passed), count(ru.tests_ignored), count(ru.revision),
                   count(ru.sha)] as held
        from sync.item i",
    knobas_core::run_join!("ru", "i"),
);

/// The stored fields, in [`HELD_TO_THE_WALK`]'s `held` order.
const FIELDS: [&str; 15] = [
    "pkey",
    "number",
    "state",
    "branch",
    "started",
    "finished",
    "trig",
    "started_by",
    "agent",
    "current_step",
    "tests_failed",
    "tests_passed",
    "tests_ignored",
    "revision",
    "sha",
];

#[derive(Debug, sqlx::FromRow)]
struct Held {
    items: i64,
    unstored: i64,
    differing: i64,
    held: Vec<i64>,
}

async fn held(pool: &PgPool) -> Held {
    let declarations = knobas_app::sources::paths::declared_paths(pool, &Registry::for_process())
        .await
        .unwrap();
    sqlx::query_as(HELD_TO_THE_WALK)
        .bind(declarations.as_param())
        .fetch_one(pool)
        .await
        .unwrap()
}

/// Where a run's row is stored, so a test sees whether a write wrote it again.
async fn place(pool: &PgPool, entity: &str) -> Option<String> {
    sqlx::query_scalar("select ctid::text from sync.item_run where entity_id = $1")
        .bind(entity)
        .fetch_optional(pool)
        .await
        .unwrap()
}

/// Every Builds read that takes the stored rows, as JSON: the list, each
/// pipeline's runs (all and the newest one), and the Gitea repository's
/// Builds tab.
async fn answers(pool: &PgPool, gitea: &str) -> Vec<String> {
    let registry = Registry::for_process();
    let pipelines = pipelines_inner(pool, &registry).await.unwrap();
    let mut out = vec![serde_json::to_string(&pipelines).unwrap()];
    for pipeline in &pipelines {
        for limit in [None, Some(1)] {
            let runs = pipeline_runs_inner(pool, &registry, &pipeline.entity_id, limit)
                .await
                .unwrap();
            out.push(serde_json::to_string(&runs).unwrap());
        }
    }
    let repo = format!("{gitea}:repo:tidewater/payout-service");
    let tab = repo_pipelines_inner(pool, &registry, &repo).await.unwrap();
    out.push(serde_json::to_string(&tab).unwrap());
    out
}

/// `sync.item_run_rows` carries each macro's expansion: a stored field is
/// read by the expression the Builds reads walk the payload with.
#[tokio::test]
async fn the_stored_runs_are_the_macros_expansions() {
    let (pool, ..) = mirror("item_run_expansions").await;
    let source: String = sqlx::query_scalar(
        "select p.prosrc from pg_proc p join pg_namespace n on n.oid = p.pronamespace
          where n.nspname = 'sync' and p.proname = 'item_run_rows'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    for expansion in [
        knobas_core::declared_string!("dd.decl", "pipeline"),
        knobas_core::declared_string!("dd.decl", "number"),
        knobas_core::declared_state!("dd.decl"),
        knobas_core::declared_string!("dd.decl", "branch"),
        knobas_core::declared_string!("dd.decl", "started"),
        knobas_core::declared_string!("dd.decl", "finished"),
        knobas_core::declared_trigger!("dd.decl"),
        knobas_core::declared_string!("dd.decl", "started_by"),
        knobas_core::declared_string!("dd.decl", "agent"),
        knobas_core::declared_string!("dd.decl", "current_step"),
        knobas_core::declared_integer!("dd.decl", "tests_failed"),
        knobas_core::declared_integer!("dd.decl", "tests_passed"),
        knobas_core::declared_integer!("dd.decl", "tests_ignored"),
        concat!(
            "(select l.value from ",
            knobas_core::declared_list!("dd.decl", "revision"),
            " l limit 1)"
        ),
        knobas_core::declared_string!("dd.decl", "commit"),
    ] {
        assert!(
            source.contains(expansion),
            "sync.item_run_rows no longer carries a macro's expansion; \
             restate it in a new migration:\n{expansion}"
        );
    }
    // And the key reads every part those expansions read, and no other.
    let paths: String = sqlx::query_scalar(
        "select p.prosrc from pg_proc p join pg_namespace n on n.oid = p.pronamespace
          where n.nspname = 'sync' and p.proname = 'run_paths'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    for part in [
        "pipeline",
        "number",
        "state",
        "branch",
        "started",
        "finished",
        "trigger",
        "started_by",
        "agent",
        "current_step",
        "tests_failed",
        "tests_passed",
        "tests_ignored",
        "revision",
        "commit",
    ] {
        assert!(
            paths.contains(&format!("declared->'{part}'")),
            "sync.run_paths reads {part}:\n{paths}"
        );
    }
    assert_eq!(paths.matches("declared->'").count(), 15, "{paths}");
}

/// Every run and commit has its row under the key the Builds reads read by,
/// and every stored field is the payload walk's answer: the reference
/// adapter's corpus and TeamCity's and Gitea's, with every field held by
/// some row, so none is compared as null against null alone.
#[tokio::test]
async fn a_stored_run_is_the_walks_answer_for_every_field() {
    let (pool, ..) = mirror("item_run_walk").await;
    let held = held(&pool).await;
    assert!(held.items > 30, "the corpus is there: {held:?}");
    assert_eq!(
        held.unstored, 0,
        "every run and commit has its row: {held:?}"
    );
    assert_eq!(held.differing, 0, "every field is the walk's: {held:?}");
    for (field, rows) in FIELDS.iter().zip(&held.held) {
        assert!(*rows > 0, "some row holds a {field}: {held:?}");
    }
}

/// **The same answer** from the stored rows, from some of them, and from
/// none: the list, every pipeline's runs and a repository's Builds tab, with
/// runs linked to their mirrored commits, byte for byte.
#[tokio::test]
async fn the_builds_reads_answer_the_same_from_their_rows_and_from_the_payloads() {
    let (pool, tc, gitea, config) = mirror("item_run_same").await;
    let stored = answers(&pool, &gitea).await;
    assert!(
        stored[0].contains(&format!(
            "{gitea}:commit:tidewater/payout-service/a41f2c0b9e11"
        )),
        "a run is linked to its commit: {}",
        stored[0]
    );
    assert!(stored[0].contains("agent-linux-03") && stored[0].contains(&config));
    let some: i64 = sqlx::query_scalar(
        "with gone as (delete from sync.item_run r using sync.item i
                        where i.entity_id = r.entity_id and (i.source_id = $1 or i.kind = 'commit')
                        returning 1)
         select count(*) from gone",
    )
    .bind(&tc)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(some > 2, "some rows went: {some}");
    assert_eq!(answers(&pool, &gitea).await, stored, "with some rows");
    sqlx::query("delete from sync.item_run")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(answers(&pool, &gitea).await, stored, "with none");
}

/// The rows follow the mirror: a rewritten run's row is the new payload's, a
/// stamp writes none, a ticket's write writes none, a deleted run's row goes,
/// a changed declaration drops the rows read under it and no other, and the
/// fill writes them back.
#[tokio::test]
async fn the_runs_rows_follow_every_write_and_the_fill_restores_them() {
    let (pool, tc, gitea, _) = mirror("item_run_writes").await;
    let run = format!("{tc}:build:2");
    let commit = format!("{gitea}:commit:tidewater/payout-service/a41f2c0b9e11");
    let stored = async |entity: &str| -> (Option<String>, Option<String>, Option<i64>) {
        sqlx::query_as(
            "select number, revision, tests_failed from sync.item_run where entity_id = $1",
        )
        .bind(entity)
        .fetch_one(&pool)
        .await
        .unwrap()
    };
    assert_eq!(
        stored(&run).await,
        (
            Some("2".to_owned()),
            Some("b52e3d1c0f22".to_owned()),
            Some(2)
        )
    );

    // A stamp: the row stays where it is.
    let before = place(&pool, &run).await;
    sqlx::query("update sync.item set synced_at = now() where entity_id = $1")
        .bind(&run)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(place(&pool, &run).await, before, "a stamp writes no row");

    // A rewrite: another number, another commit, no tests.
    write(
        &pool,
        &tc,
        "build",
        "build:2",
        json!({ "id": 2, "number": "2b", "buildTypeId": "Payout_Tests",
                "revisions": { "revision": [{ "version": "a41f2c0b9e11" }] } }),
    )
    .await;
    assert_eq!(
        stored(&run).await,
        (Some("2b".to_owned()), Some("a41f2c0b9e11".to_owned()), None)
    );
    assert_eq!(held(&pool).await.differing, 0);

    // A commit's sha follows its payload.
    write(
        &pool,
        &gitea,
        "commit",
        "commit:tidewater/payout-service/a41f2c0b9e11",
        json!({ "sha": "c0ffee" }),
    )
    .await;
    let sha: Option<String> =
        sqlx::query_scalar("select sha from sync.item_run where entity_id = $1")
            .bind(&commit)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(sha.as_deref(), Some("c0ffee"));

    // A delete takes the row with it.
    sqlx::query("delete from sync.item where entity_id = $1")
        .bind(format!("{tc}:build:3"))
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(place(&pool, &format!("{tc}:build:3")).await, None);

    // TeamCity's builds are declared another way: their rows go, Gitea's
    // commits' and the reference adapter's stay where they are.
    let commit_place = place(&pool, &commit).await;
    let mock_rows: i64 = sqlx::query_scalar(
        "select count(*) from sync.item_run r join sync.item i using (entity_id)
          where i.source_id = 'mock'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(mock_rows > 3, "the reference adapter's rows: {mock_rows}");
    sqlx::query(
        "update sync.parent_declaration
            set payload_paths = (
              select jsonb_agg(case when kp.p->>'kind' = 'build'
                                    then jsonb_set(kp.p, '{number}', '[[\"id\"]]')
                                    else kp.p end order by kp.ord)
                from jsonb_array_elements(payload_paths) with ordinality as kp(p, ord))
          where adapter_kind = 'teamcity'",
    )
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(place(&pool, &run).await, None, "a row under the old key");
    assert_eq!(
        place(&pool, &commit).await,
        commit_place,
        "another adapter's kind"
    );
    let mock_after: i64 = sqlx::query_scalar(
        "select count(*) from sync.item_run r join sync.item i using (entity_id)
          where i.source_id = 'mock'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(mock_after, mock_rows, "another adapter's rows");

    // The declarations as the app holds them again, and the fill: every row
    // back, each the walk's answer, and a second fill writes nothing.
    knobas_app::commands::search::searcher(&pool, &Registry::for_process())
        .await
        .unwrap();
    assert_eq!(place(&pool, &run).await, None, "nothing fills but the fill");
    assert!(held(&pool).await.unstored > 0);
    let batches = knobas_core::item_run::fill(&pool).await.unwrap();
    assert!(batches > 0);
    let after = held(&pool).await;
    assert_eq!((after.unstored, after.differing), (0, 0), "{after:?}");
    let filled = place(&pool, &run).await;
    assert!(filled.is_some());
    knobas_core::item_run::fill(&pool).await.unwrap();
    assert_eq!(
        place(&pool, &run).await,
        filled,
        "a filled row is left alone"
    );
}

/// **The fill steps past what a running sync holds** (migration `0091`, the
/// ux13 forge-load pass, 2026-11-15): a sync started right after an upgrade
/// deadlocked with the fill one run in three, each waiting on the other's
/// uncommitted rows. With a run's transaction holding a build (locked and
/// left unchanged, as a run that re-reads it does), the fill writes every
/// other row without waiting on it, keeps passing while it holds it, and
/// writes its row once the run has committed.
#[tokio::test]
async fn the_fill_steps_past_an_item_a_sync_holds_and_fills_it_after() {
    let (pool, tc, ..) = mirror("item_run_fill_held").await;
    let run = format!("{tc}:build:2");
    sqlx::query("delete from sync.item_run")
        .execute(&pool)
        .await
        .unwrap();
    let mut sync = pool.begin().await.unwrap();
    sqlx::query("update sync.item set synced_at = now() where entity_id = $1")
        .bind(&run)
        .execute(&mut *sync)
        .await
        .unwrap();

    let fill = tokio::spawn({
        let pool = pool.clone();
        async move { knobas_core::item_run::fill(&pool).await }
    });
    let mut others_filled = false;
    for _ in 0..100 {
        if held(&pool).await.unstored == 1 {
            others_filled = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert!(
        others_filled,
        "every other row, while the run holds its build"
    );
    assert_eq!(place(&pool, &run).await, None, "stepped past");
    assert!(!fill.is_finished(), "passing again while the run holds it");

    sync.commit().await.unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(10), fill)
        .await
        .expect("the fill ends once the run committed")
        .unwrap()
        .unwrap();
    assert!(place(&pool, &run).await.is_some(), "filled after");
    let after = held(&pool).await;
    assert_eq!((after.unstored, after.differing), (0, 0), "{after:?}");
}

/// **A fill with nothing to write is one read** (the ux18 pass, 2026-11-17),
/// as `item_row`'s: with every run's row written by the triggers the fill
/// runs no batch; one run without its row, and it walks and writes it again.
#[tokio::test]
async fn a_fill_with_every_run_written_runs_no_batch() {
    let (pool, tc, ..) = mirror("item_run_owed").await;
    let run = format!("{tc}:build:2");
    assert_eq!(
        held(&pool).await.unstored,
        0,
        "the triggers wrote every row"
    );
    assert_eq!(
        knobas_core::item_run::fill(&pool).await.unwrap(),
        0,
        "nothing owed, no batch"
    );
    assert!(place(&pool, &run).await.is_some());
    sqlx::query("delete from sync.item_run where entity_id = $1")
        .bind(&run)
        .execute(&pool)
        .await
        .unwrap();
    assert!(
        knobas_core::item_run::fill(&pool).await.unwrap() > 0,
        "owed one"
    );
    assert!(place(&pool, &run).await.is_some(), "written again");
    assert_eq!(held(&pool).await.unstored, 0);
    assert_eq!(knobas_core::item_run::fill(&pool).await.unwrap(), 0);
}
