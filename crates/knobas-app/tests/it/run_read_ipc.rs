//! Mark builds as read (ux18, migration `0098`) through its commands, over
//! the embedded PostgreSQL with the declarations the compiled-in TeamCity
//! adapter makes (`Registry::for_process`).
//!
//! Each test has a database of its own: the top bar's count reads every
//! pipeline in it.

use knobas_app::commands::entity::{
    MarkScope, RunMarks, complete_inbox_item_inner, inbox_items_inner, mark_run_opened_inner,
    mark_runs_read_inner, mark_runs_unread_inner, run_counts_inner,
};
use knobas_core::run_read::BubbleCounts;

const ME: &str = "mara.lindqvist";

fn registry() -> knobas_app::sources::Registry {
    knobas_app::sources::Registry::for_process()
}

/// A database of this test's own, migrated, with one TeamCity source whose
/// account is [`ME`].
async fn scratch(name: &str) -> (sqlx::PgPool, String) {
    let pool = knobas_db::test_util::scratch_database(name)
        .await
        .pool(4)
        .await
        .expect("a pool onto this test's own database");
    let tc = "tc".to_owned();
    crate::support::source_config(
        &pool,
        &tc,
        "teamcity",
        "TeamCity",
        &serde_json::json!({ "username": ME }),
    )
    .await;
    (pool, tc)
}

/// One mirror item, stored as the adapter stores it, by `author`.
async fn item(
    pool: &sqlx::PgPool,
    id: &str,
    source: &str,
    kind: &str,
    updated: &str,
    author: Option<&str>,
    payload: serde_json::Value,
) {
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1,$2,$1)")
        .bind(id)
        .bind(kind)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload,
                                item_updated_at, author)
         values ($1,$2,$3,$1,'',$4, $5::timestamptz, $6)",
    )
    .bind(id)
    .bind(source)
    .bind(kind)
    .bind(payload)
    .bind(updated)
    .bind(author)
    .execute(pool)
    .await
    .unwrap();
}

async fn configuration(pool: &sqlx::PgPool, source: &str, id: &str) -> String {
    let entity = format!("{source}:buildType:{id}");
    item(
        pool,
        &entity,
        source,
        "build_config",
        "2026-08-20T00:00:00Z",
        None,
        serde_json::json!({ "id": id, "name": id, "projectId": "Payout", "projectName": "Payout" }),
    )
    .await;
    entity
}

/// A build of `config`: `running`, or finished with `status`.
async fn build(
    pool: &sqlx::PgPool,
    source: &str,
    id: i64,
    config: &str,
    status: &str,
    updated: &str,
) -> String {
    let entity = format!("{source}:build:{id}");
    item(
        pool,
        &entity,
        source,
        "build",
        updated,
        Some(ME),
        payload(id, config, status),
    )
    .await;
    entity
}

/// A TeamCity build's payload: `running` (already reporting a failure, as
/// TeamCity does once a test failed), `queued`, or finished with `status`.
fn payload(id: i64, config: &str, status: &str) -> serde_json::Value {
    match status {
        "running" => serde_json::json!({
            "id": id, "number": id.to_string(), "buildTypeId": config, "state": "running",
            "status": "FAILURE", "branchName": "main", "startDate": "20260822T114500+0000"
        }),
        "queued" => serde_json::json!({
            "id": id, "buildTypeId": config, "state": "queued", "branchName": "main"
        }),
        _ => serde_json::json!({
            "id": id, "number": id.to_string(), "buildTypeId": config, "state": "finished",
            "status": status, "branchName": "main",
            "startDate": "20260822T100500+0000", "finishDate": "20260822T100917+0000"
        }),
    }
}

/// The runs a mark reads, sorted.
async fn read(pool: &sqlx::PgPool) -> Vec<String> {
    marks(pool).await.read
}

/// Every live run's mark, each list sorted.
async fn marks(pool: &sqlx::PgPool) -> RunMarks {
    sorted(knobas_core::run_read::read_runs(pool).await.unwrap())
}

fn sorted(mut marks: RunMarks) -> RunMarks {
    marks.read.sort();
    marks.while_running.sort();
    marks
}

/// Every id a mark answered it wrote, sorted: what *Undo* hands back.
fn ids(marks: &RunMarks) -> Vec<String> {
    let mut ids: Vec<String> = marks
        .read
        .iter()
        .chain(&marks.while_running)
        .cloned()
        .collect();
    ids.sort();
    ids
}

fn by_id(runs: &[String]) -> MarkScope {
    MarkScope {
        runs: runs.to_vec(),
        ..MarkScope::default()
    }
}

fn of_pipelines(pipelines: &[String]) -> MarkScope {
    MarkScope {
        pipelines: pipelines.to_vec(),
        ..MarkScope::default()
    }
}

/// A run's payload as `build` writes it for `status`, written over the stored
/// one: the run moved at the source and a sync brought it.
async fn moved(pool: &sqlx::PgPool, run: &str, status: &str) {
    let id: i64 = run.rsplit(':').next().unwrap().parse().unwrap();
    let config: String =
        sqlx::query_scalar("select payload->>'buildTypeId' from sync.item where entity_id = $1")
            .bind(run)
            .fetch_one(pool)
            .await
            .unwrap();
    sqlx::query("update sync.item set payload = $2 where entity_id = $1")
        .bind(run)
        .bind(payload(id, &config, status))
        .execute(pool)
        .await
        .unwrap();
}

/// *Mark all as read* on a pipeline reads its finished failures and takes its
/// running run out of the blue (ADR-0063), and nothing else: not a pass, not
/// a queued run, not another pipeline's failure. It answers what it wrote, so
/// a second press answers nothing and *Undo* takes back exactly the first.
#[tokio::test]
async fn a_pipelines_mark_writes_its_finished_failures_and_answers_what_it_wrote() {
    let (pool, tc) = scratch("run-read-pipeline").await;
    let tests = configuration(&pool, &tc, "Payout_Tests").await;
    configuration(&pool, &tc, "Payout_Lint").await;
    let red1 = build(
        &pool,
        &tc,
        1001,
        "Payout_Tests",
        "FAILURE",
        "2026-08-21T01:00:00Z",
    )
    .await;
    build(
        &pool,
        &tc,
        1002,
        "Payout_Tests",
        "SUCCESS",
        "2026-08-21T02:00:00Z",
    )
    .await;
    let red2 = build(
        &pool,
        &tc,
        1003,
        "Payout_Tests",
        "FAILURE",
        "2026-08-21T03:00:00Z",
    )
    .await;
    let running = build(
        &pool,
        &tc,
        1004,
        "Payout_Tests",
        "running",
        "2026-08-21T04:00:00Z",
    )
    .await;
    build(
        &pool,
        &tc,
        1005,
        "Payout_Tests",
        "queued",
        "2026-08-21T04:30:00Z",
    )
    .await;
    let lint = build(
        &pool,
        &tc,
        2001,
        "Payout_Lint",
        "FAILURE",
        "2026-08-21T05:00:00Z",
    )
    .await;

    let (wrote, line) = mark_runs_read_inner(
        &pool,
        &registry(),
        &of_pipelines(std::slice::from_ref(&tests)),
    )
    .await
    .unwrap();
    let wrote = sorted(wrote);
    assert_eq!(wrote.read, [red1.clone(), red2.clone()]);
    assert_eq!(
        wrote.while_running,
        std::slice::from_ref(&running),
        "out of the blue only"
    );
    let line = line.expect("a mark that wrote says so");
    assert_eq!(line.verb, "marked read");
    assert_eq!(line.detail["runs"], 3);
    assert_eq!(marks(&pool).await, wrote);
    assert!(
        !read(&pool).await.contains(&lint),
        "another pipeline's stays"
    );

    let (again, line) = mark_runs_read_inner(&pool, &registry(), &of_pipelines(&[tests]))
        .await
        .unwrap();
    assert!(ids(&again).is_empty(), "already read: {again:?}");
    assert!(line.is_none(), "a mark that wrote nothing writes no line");

    let (gone, line) = mark_runs_unread_inner(&pool, &ids(&wrote)).await.unwrap();
    assert_eq!(gone.len(), 3, "Undo takes back all three");
    assert_eq!(line.expect("and says so").verb, "marked unread");
    assert_eq!(marks(&pool).await, RunMarks::default());
}

/// The red count is the failures no mark reads: reading one takes it off,
/// and a new failure is a new run with no mark, so it asks again.
#[tokio::test]
async fn the_red_count_drops_when_a_failure_is_read_and_a_new_failure_asks_again() {
    let (pool, tc) = scratch("run-read-count").await;
    configuration(&pool, &tc, "Payout_Tests").await;
    configuration(&pool, &tc, "Payout_Lint").await;
    let red = build(
        &pool,
        &tc,
        1001,
        "Payout_Tests",
        "FAILURE",
        "2026-08-21T01:00:00Z",
    )
    .await;
    build(
        &pool,
        &tc,
        2001,
        "Payout_Lint",
        "SUCCESS",
        "2026-08-21T01:00:00Z",
    )
    .await;
    assert_eq!(total(&pool).await.red, 1);

    mark_runs_read_inner(&pool, &registry(), &by_id(std::slice::from_ref(&red)))
        .await
        .unwrap();
    assert_eq!(
        total(&pool).await.red,
        0,
        "read, the pipeline asks for nothing"
    );

    build(
        &pool,
        &tc,
        1002,
        "Payout_Tests",
        "FAILURE",
        "2026-08-21T02:00:00Z",
    )
    .await;
    assert_eq!(total(&pool).await.red, 1, "a new failure is unread");
}

/// One notion with the Inbox: reading a failed build's run takes its line
/// away, *Done* on the line reads the run, and *Mark as unread* brings the
/// line back.
#[tokio::test]
async fn reading_a_run_and_doing_its_inbox_line_are_one_answer() {
    let (pool, tc) = scratch("run-read-inbox").await;
    configuration(&pool, &tc, "Payout_Tests").await;
    configuration(&pool, &tc, "Ledger_Tests").await;
    let red = build(
        &pool,
        &tc,
        1001,
        "Payout_Tests",
        "FAILURE",
        "2026-08-21T01:00:00Z",
    )
    .await;
    let other = build(
        &pool,
        &tc,
        3001,
        "Ledger_Tests",
        "FAILURE",
        "2026-08-21T01:00:00Z",
    )
    .await;
    let now = chrono::Utc::now();
    let keys = |pool: sqlx::PgPool| async move {
        inbox_items_inner(&pool, &registry(), now, knobas_core::inbox::Shelf::Stream)
            .await
            .unwrap()
            .into_iter()
            .map(|entry| entry.item.key)
            .collect::<Vec<_>>()
    };
    let key = format!("failed_build:{red}");
    let other_key = format!("failed_build:{other}");
    assert!(keys(pool.clone()).await.contains(&key));

    mark_runs_read_inner(&pool, &registry(), &by_id(std::slice::from_ref(&red)))
        .await
        .unwrap();
    assert!(!keys(pool.clone()).await.contains(&key), "read, it left");
    mark_runs_unread_inner(&pool, std::slice::from_ref(&red))
        .await
        .unwrap();
    assert!(
        keys(pool.clone()).await.contains(&key),
        "unread, it is back"
    );

    complete_inbox_item_inner(&pool, &registry(), now, &other_key)
        .await
        .unwrap();
    assert_eq!(
        read(&pool).await,
        std::slice::from_ref(&other),
        "Done read the run"
    );
    mark_runs_unread_inner(&pool, &[other]).await.unwrap();
    assert!(
        keys(pool.clone()).await.contains(&other_key),
        "unread takes the Done back, so the line returns"
    );
}

/// `read_runs` answers live runs only: a run the source withdrew keeps its
/// row and is not answered.
#[tokio::test]
async fn a_withdrawn_runs_mark_is_not_answered() {
    let (pool, tc) = scratch("run-read-withdrawn").await;
    configuration(&pool, &tc, "Payout_Tests").await;
    let red = build(
        &pool,
        &tc,
        1001,
        "Payout_Tests",
        "FAILURE",
        "2026-08-21T01:00:00Z",
    )
    .await;
    mark_runs_read_inner(&pool, &registry(), &by_id(std::slice::from_ref(&red)))
        .await
        .unwrap();
    assert_eq!(read(&pool).await, std::slice::from_ref(&red));
    sqlx::query("update knobas.entity set deleted_at = now() where id = $1")
        .bind(&red)
        .execute(&pool)
        .await
        .unwrap();
    assert!(read(&pool).await.is_empty());
}

/// The Builds list names each pipeline's unread failed runs over its whole
/// history (#2543), not only the twelve its row draws: what the rail's
/// right-click counts and marks. Not a pass, not a run still going, not a
/// failure already read, not another pipeline's. A pipeline's mark empties
/// it, and *Undo* brings back exactly those.
#[tokio::test]
async fn the_list_names_every_unread_failure_of_a_pipeline_past_its_twelve() {
    let (pool, tc) = scratch("run-read-list-unread").await;
    let tests = configuration(&pool, &tc, "Payout_Tests").await;
    let lint = configuration(&pool, &tc, "Payout_Lint").await;
    let mut failures = Vec::new();
    for n in 0..14_i64 {
        let at = format!("2026-08-01T{:02}:00:00Z", n);
        failures.push(build(&pool, &tc, 1000 + n, "Payout_Tests", "FAILURE", &at).await);
    }
    // Twelve newer passes push every failure out of the row's twelve.
    for n in 0..12_i64 {
        let at = format!("2026-08-02T{:02}:00:00Z", n);
        build(&pool, &tc, 1100 + n, "Payout_Tests", "SUCCESS", &at).await;
    }
    build(
        &pool,
        &tc,
        1200,
        "Payout_Tests",
        "running",
        "2026-08-03T00:00:00Z",
    )
    .await;
    let other = build(
        &pool,
        &tc,
        2001,
        "Payout_Lint",
        "FAILURE",
        "2026-08-03T00:00:00Z",
    )
    .await;
    let read_one = failures.remove(0);
    mark_runs_read_inner(&pool, &registry(), &by_id(std::slice::from_ref(&read_one)))
        .await
        .unwrap();

    let unread_of = |pool: sqlx::PgPool, id: String| async move {
        let listed = knobas_app::commands::entity::pipelines_inner(&pool, &registry())
            .await
            .unwrap();
        let row = listed
            .into_iter()
            .find(|p| p.entity_id == id)
            .expect("the pipeline is listed");
        let mut unread = row.unread;
        unread.sort();
        (row.recent.len(), unread)
    };
    let (drawn, unread) = unread_of(pool.clone(), tests.clone()).await;
    assert_eq!(drawn, 12, "the row draws its last twelve");
    let mut expected = failures.clone();
    expected.sort();
    assert_eq!(
        unread, expected,
        "thirteen unread failures, none of them drawn"
    );
    assert_eq!(
        unread_of(pool.clone(), lint.clone()).await.1,
        std::slice::from_ref(&other)
    );

    let (wrote, _) = mark_runs_read_inner(
        &pool,
        &registry(),
        &of_pipelines(std::slice::from_ref(&tests)),
    )
    .await
    .unwrap();
    assert_eq!(
        wrote.while_running.len(),
        1,
        "its running run, out of the blue"
    );
    assert_eq!(
        wrote.read.len(),
        13,
        "the pipeline's mark writes what the list named"
    );
    assert!(unread_of(pool.clone(), tests.clone()).await.1.is_empty());
    assert_eq!(
        unread_of(pool.clone(), lint).await.1,
        [other],
        "another pipeline's stays"
    );

    mark_runs_unread_inner(&pool, &ids(&wrote)).await.unwrap();
    assert_eq!(
        unread_of(pool.clone(), tests).await.1,
        expected,
        "Undo brings them back"
    );
}

/// #2555's first bug: a running run already reporting a failure counted red
/// and could not be marked, because the mark asked for a finish. Now any run
/// can carry a mark, and the mark answers its id, as one taken while it ran.
#[tokio::test]
async fn a_running_run_already_reporting_failure_can_be_marked() {
    let (pool, tc) = scratch("run-read-running-err").await;
    configuration(&pool, &tc, "Payout_Tests").await;
    let running = build(
        &pool,
        &tc,
        1001,
        "Payout_Tests",
        "running",
        "2026-08-21T01:00:00Z",
    )
    .await;

    let (wrote, line) =
        mark_runs_read_inner(&pool, &registry(), &by_id(std::slice::from_ref(&running)))
            .await
            .unwrap();
    assert_eq!(
        ids(&wrote),
        std::slice::from_ref(&running),
        "the mark answers its id"
    );
    assert_eq!(
        wrote.while_running,
        std::slice::from_ref(&running),
        "taken while it ran"
    );
    assert!(line.is_some());
    assert_eq!(marks(&pool).await.while_running, [running]);
}

/// Question 1 (#2554): a mark taken while a run ran takes only its blue, so
/// when it fails it is unread, on the list and in the Inbox. Opening it while
/// it ran is having looked: it fails read. Marked again once failed, the
/// mark is written over and answers the run.
#[tokio::test]
async fn a_run_marked_while_running_fails_unread_and_one_opened_while_running_fails_read() {
    let (pool, tc) = scratch("run-read-while-running").await;
    let tests = configuration(&pool, &tc, "Payout_Tests").await;
    let marked = build(
        &pool,
        &tc,
        1001,
        "Payout_Tests",
        "running",
        "2026-08-21T01:00:00Z",
    )
    .await;
    let opened = build(
        &pool,
        &tc,
        1002,
        "Payout_Tests",
        "running",
        "2026-08-21T02:00:00Z",
    )
    .await;
    mark_runs_read_inner(&pool, &registry(), &by_id(std::slice::from_ref(&marked)))
        .await
        .unwrap();
    let (wrote, line) = mark_run_opened_inner(&pool, &registry(), &opened)
        .await
        .unwrap();
    assert!(wrote, "the first open writes");
    assert!(line.is_none(), "opening writes no activity line");
    assert_eq!(
        marks(&pool).await,
        RunMarks {
            read: vec![opened.clone()],
            while_running: vec![marked.clone()],
        }
    );

    moved(&pool, &marked, "FAILURE").await;
    moved(&pool, &opened, "FAILURE").await;

    let unread = knobas_app::commands::entity::pipelines_inner(&pool, &registry())
        .await
        .unwrap()
        .into_iter()
        .find(|p| p.entity_id == tests)
        .expect("the pipeline is listed")
        .unread;
    assert_eq!(
        unread,
        std::slice::from_ref(&marked),
        "the marked run asks, the opened one not"
    );
    let keys = inbox_keys(&pool).await;
    assert!(keys.contains(&format!("failed_build:{marked}")), "{keys:?}");
    assert!(
        !keys.contains(&format!("failed_build:{opened}")),
        "{keys:?}"
    );

    let (wrote, _) =
        mark_runs_read_inner(&pool, &registry(), &by_id(std::slice::from_ref(&marked)))
            .await
            .unwrap();
    assert_eq!(
        wrote.read,
        std::slice::from_ref(&marked),
        "marked once failed, it is read"
    );
    assert_eq!(read(&pool).await, {
        let mut both = vec![marked, opened];
        both.sort();
        both
    });
}

/// Question 3 (#2554): opening a failed run reads it, and its Inbox line goes
/// with it. A second open writes nothing and no line; *Mark as unread* takes
/// the open back like any mark.
#[tokio::test]
async fn a_failed_run_opened_is_read_and_its_inbox_line_goes() {
    let (pool, tc) = scratch("run-read-opened").await;
    configuration(&pool, &tc, "Payout_Tests").await;
    let red = build(
        &pool,
        &tc,
        1001,
        "Payout_Tests",
        "FAILURE",
        "2026-08-21T01:00:00Z",
    )
    .await;
    let key = format!("failed_build:{red}");
    assert!(inbox_keys(&pool).await.contains(&key));
    assert_eq!(total(&pool).await.red, 1);

    let (wrote, line) = mark_run_opened_inner(&pool, &registry(), &red)
        .await
        .unwrap();
    assert!(wrote);
    assert!(line.is_none(), "opening writes no activity line");
    assert_eq!(read(&pool).await, std::slice::from_ref(&red));
    assert!(!inbox_keys(&pool).await.contains(&key), "its line went");
    assert_eq!(total(&pool).await.red, 0, "and the top bar's count");

    let (again, line) = mark_run_opened_inner(&pool, &registry(), &red)
        .await
        .unwrap();
    assert!(!again, "already read");
    assert!(line.is_none());

    mark_runs_unread_inner(&pool, std::slice::from_ref(&red))
        .await
        .unwrap();
    assert!(
        inbox_keys(&pool).await.contains(&key),
        "unread, it asks again"
    );
}

/// #2555's second bug, on the backend: *everything* needs no pipeline from
/// the page. It reads every failed run and takes every running run out of
/// the blue, of every pipeline, and nothing else; *Undo* takes back exactly
/// the rows it wrote, so a run read before stays read.
#[tokio::test]
async fn a_mark_of_everything_needs_no_pipeline_and_undo_takes_back_exactly_it() {
    let (pool, tc) = scratch("run-read-everything").await;
    configuration(&pool, &tc, "Payout_Tests").await;
    configuration(&pool, &tc, "Ledger_Tests").await;
    let red1 = build(
        &pool,
        &tc,
        1001,
        "Payout_Tests",
        "FAILURE",
        "2026-08-21T01:00:00Z",
    )
    .await;
    let red2 = build(
        &pool,
        &tc,
        2001,
        "Ledger_Tests",
        "FAILURE",
        "2026-08-21T01:00:00Z",
    )
    .await;
    let before = build(
        &pool,
        &tc,
        2002,
        "Ledger_Tests",
        "FAILURE",
        "2026-08-21T02:00:00Z",
    )
    .await;
    build(
        &pool,
        &tc,
        1002,
        "Payout_Tests",
        "SUCCESS",
        "2026-08-21T02:00:00Z",
    )
    .await;
    build(
        &pool,
        &tc,
        1003,
        "Payout_Tests",
        "queued",
        "2026-08-21T03:00:00Z",
    )
    .await;
    let running = build(
        &pool,
        &tc,
        2003,
        "Ledger_Tests",
        "running",
        "2026-08-21T03:00:00Z",
    )
    .await;
    mark_runs_read_inner(&pool, &registry(), &by_id(std::slice::from_ref(&before)))
        .await
        .unwrap();

    let everything = MarkScope {
        everything: true,
        ..MarkScope::default()
    };
    let (wrote, line) = mark_runs_read_inner(&pool, &registry(), &everything)
        .await
        .unwrap();
    let wrote = sorted(wrote);
    let mut failed = vec![red1, red2];
    failed.sort();
    assert_eq!(
        wrote.read, failed,
        "every failed run, not the one read before"
    );
    assert_eq!(wrote.while_running, std::slice::from_ref(&running));
    assert_eq!(line.expect("one line").detail["runs"], 3);
    assert_eq!(total(&pool).await.red, 0);

    mark_runs_unread_inner(&pool, &ids(&wrote)).await.unwrap();
    assert_eq!(
        marks(&pool).await,
        RunMarks {
            read: vec![before],
            while_running: Vec::new(),
        },
        "Undo leaves what was read before"
    );
}

/// `0105` turns every mark already written into one *marked after finishing*,
/// and a database at `0104` starts under this build. The scratch database is
/// put back to `0104`'s shape, a mark written as `0098` wrote it, then
/// migrated.
#[tokio::test]
async fn a_database_at_0104_starts_and_its_marks_read_their_runs() {
    let (pool, tc) = scratch("run-read-0105").await;
    configuration(&pool, &tc, "Payout_Tests").await;
    let red = build(
        &pool,
        &tc,
        1001,
        "Payout_Tests",
        "FAILURE",
        "2026-08-21T01:00:00Z",
    )
    .await;
    for statement in [
        "alter table knobas.run_read drop column how, drop column finished",
        "delete from public._sqlx_migrations where version = 105",
    ] {
        sqlx::query(statement).execute(&pool).await.unwrap();
    }
    sqlx::query("insert into knobas.run_read (entity_id) values ($1)")
        .bind(&red)
        .execute(&pool)
        .await
        .unwrap();

    knobas_db::migrate::run(&pool)
        .await
        .expect("a database at 0104 starts");
    let (how, finished): (String, bool) =
        sqlx::query_as("select how, finished from knobas.run_read where entity_id = $1")
            .bind(&red)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!((how.as_str(), finished), ("marked", true));
    assert_eq!(read(&pool).await, [red]);
}

async fn inbox_keys(pool: &sqlx::PgPool) -> Vec<String> {
    inbox_items_inner(
        pool,
        &registry(),
        chrono::Utc::now(),
        knobas_core::inbox::Shelf::Stream,
    )
    .await
    .unwrap()
    .into_iter()
    .map(|entry| entry.item.key)
    .collect()
}

// -- the counts by bubble and the instants (#2568) ---------------------------

/// Every pipeline's unread runs summed, as the top bar reads them.
async fn total(pool: &sqlx::PgPool) -> BubbleCounts {
    run_counts_inner(pool, &registry()).await.unwrap().total
}

/// One pipeline's unread runs, zero where it has none.
async fn counts_of(pool: &sqlx::PgPool, pipeline: &str) -> BubbleCounts {
    run_counts_inner(pool, &registry())
        .await
        .unwrap()
        .pipelines
        .into_iter()
        .find(|p| p.pipeline == pipeline)
        .map(|p| p.counts)
        .unwrap_or_default()
}

/// The source has synced whole once: from now, a run the mirror sees end is
/// news (migration `0106`). Before, its whole history was being read. The
/// declarations are stored by then, as the app's first read stores them, so
/// a run the sync writes gets its stored row in the same statement.
async fn synced_once(pool: &sqlx::PgPool, source: &str) {
    run_counts_inner(pool, &registry()).await.unwrap();
    sqlx::query("update knobas.source_config set cursor = 'synced' where id = $1")
        .bind(source)
        .execute(pool)
        .await
        .unwrap();
}

/// A run moved at the source, as a sync writes it: the item's `synced_at` is the run's
/// transaction, as `ITEM_UPSERT` writes it on a row that changed.
async fn synced(pool: &sqlx::PgPool, run: &str, status: &str) {
    let id: i64 = run.rsplit(':').next().unwrap().parse().unwrap();
    let config: String =
        sqlx::query_scalar("select payload->>'buildTypeId' from sync.item where entity_id = $1")
            .bind(run)
            .fetch_one(pool)
            .await
            .unwrap();
    sqlx::query("update sync.item set payload = $2, synced_at = now() where entity_id = $1")
        .bind(run)
        .bind(payload(id, &config, status))
        .execute(pool)
        .await
        .unwrap();
}

fn counts(blue: u32, green: u32, yellow: u32, red: u32, grey: u32) -> BubbleCounts {
    BubbleCounts {
        blue,
        green,
        yellow,
        red,
        grey,
    }
}

/// The spec's table, run by run (ADR-0063): blue is running with no mark,
/// red a failure no mark reads, green and grey a pass and a cancel that
/// ended after the instant and no mark reads. A queued run is in none.
#[tokio::test]
async fn the_counts_answer_each_bubble_as_the_spec_table_defines_it() {
    let (pool, tc) = scratch("run-counts-table").await;
    let tests = configuration(&pool, &tc, "Payout_Tests").await;
    synced_once(&pool, &tc).await;
    let at = |h: u32| format!("2026-08-21T{h:02}:00:00Z");
    // Before the instant: a pass Builds was left after.
    build(&pool, &tc, 1001, "Payout_Tests", "SUCCESS", &at(1)).await;
    knobas_core::run_read::builds_in_front(&pool, true)
        .await
        .unwrap();
    knobas_core::run_read::builds_in_front(&pool, false)
        .await
        .unwrap();
    assert_eq!(
        counts_of(&pool, &tests).await,
        BubbleCounts::default(),
        "seen"
    );

    build(&pool, &tc, 1002, "Payout_Tests", "running", &at(2)).await;
    let marked_running = build(&pool, &tc, 1003, "Payout_Tests", "running", &at(3)).await;
    build(&pool, &tc, 1004, "Payout_Tests", "queued", &at(4)).await;
    build(&pool, &tc, 1005, "Payout_Tests", "SUCCESS", &at(5)).await;
    let opened_pass = build(&pool, &tc, 1006, "Payout_Tests", "SUCCESS", &at(6)).await;
    build(&pool, &tc, 1007, "Payout_Tests", "FAILURE", &at(7)).await;
    let marked_red = build(&pool, &tc, 1008, "Payout_Tests", "FAILURE", &at(8)).await;
    let opened_red = build(&pool, &tc, 1009, "Payout_Tests", "FAILURE", &at(9)).await;
    build(&pool, &tc, 1010, "Payout_Tests", "UNKNOWN", &at(10)).await;
    assert_eq!(
        counts_of(&pool, &tests).await,
        counts(2, 2, 0, 3, 1),
        "before any mark"
    );

    mark_runs_read_inner(
        &pool,
        &registry(),
        &by_id(&[marked_running.clone(), marked_red]),
    )
    .await
    .unwrap();
    for run in [&opened_pass, &opened_red] {
        mark_run_opened_inner(&pool, &registry(), run)
            .await
            .unwrap();
    }
    let answer = run_counts_inner(&pool, &registry()).await.unwrap();
    assert_eq!(answer.pipelines.len(), 1);
    assert_eq!(answer.pipelines[0].counts, counts(1, 1, 0, 1, 1));
    assert_eq!(answer.total, counts(1, 1, 0, 1, 1), "the top bar's sum");
    assert!(!answer.visiting && answer.fresh.is_empty());

    // The run marked while it ran fails: red, the mark took only its blue.
    synced(&pool, &marked_running, "FAILURE").await;
    assert_eq!(counts_of(&pool, &tests).await, counts(1, 1, 0, 2, 1));
}

/// Nothing old turns green (ADR-0063): the runs a source's first sync reads,
/// before it has a cursor, are older than any instant, and so is a pass that
/// ended before stamping existed.
#[tokio::test]
async fn a_sources_first_sync_turns_nothing_green() {
    let (pool, tc) = scratch("run-counts-first-sync").await;
    let tests = configuration(&pool, &tc, "Payout_Tests").await;
    // The declarations stored, so the first sync's runs get their stored rows
    // as it writes them, in its own transaction: only the missing cursor says
    // they are history.
    run_counts_inner(&pool, &registry()).await.unwrap();
    build(
        &pool,
        &tc,
        1001,
        "Payout_Tests",
        "SUCCESS",
        "2026-08-21T01:00:00Z",
    )
    .await;
    let running = build(
        &pool,
        &tc,
        1002,
        "Payout_Tests",
        "running",
        "2026-08-21T02:00:00Z",
    )
    .await;
    assert_eq!(counts_of(&pool, &tests).await, counts(1, 0, 0, 0, 0));

    synced_once(&pool, &tc).await;
    // A sync writes the old pass again with nothing new: still not news.
    synced(&pool, "tc:build:1001", "SUCCESS").await;
    synced(&pool, &running, "SUCCESS").await;
    assert_eq!(
        counts_of(&pool, &tests).await,
        counts(0, 1, 0, 0, 0),
        "only the run seen ending"
    );
}

/// The green instant (ADR-0063, question 2 on #2554): coming in front holds
/// the visit's green, a pass that lands while Builds is in front adds none
/// and a failure turns its blue red; leaving clears the green, and the
/// instant is kept across a restart.
#[tokio::test]
async fn a_visit_holds_its_green_and_leaving_clears_it_for_good() {
    let (pool, tc) = scratch("run-counts-visit").await;
    let tests = configuration(&pool, &tc, "Payout_Tests").await;
    synced_once(&pool, &tc).await;
    let pass = build(
        &pool,
        &tc,
        1001,
        "Payout_Tests",
        "SUCCESS",
        "2026-08-21T01:00:00Z",
    )
    .await;
    let running = build(
        &pool,
        &tc,
        1002,
        "Payout_Tests",
        "running",
        "2026-08-21T02:00:00Z",
    )
    .await;
    let failing = build(
        &pool,
        &tc,
        1003,
        "Payout_Tests",
        "running",
        "2026-08-21T03:00:00Z",
    )
    .await;
    assert_eq!(counts_of(&pool, &tests).await, counts(2, 1, 0, 0, 0));

    knobas_core::run_read::builds_in_front(&pool, true)
        .await
        .unwrap();
    let answer = run_counts_inner(&pool, &registry()).await.unwrap();
    assert!(answer.visiting);
    assert_eq!(answer.fresh, std::slice::from_ref(&pass), "the dots' runs");
    assert_eq!(
        answer.total,
        counts(2, 1, 0, 0, 0),
        "the visit's green stays"
    );

    synced(&pool, &running, "SUCCESS").await;
    synced(&pool, &failing, "FAILURE").await;
    assert_eq!(
        counts_of(&pool, &tests).await,
        counts(0, 1, 0, 1, 0),
        "the pass left the blue and added no green; the failure turned red"
    );

    knobas_core::run_read::builds_in_front(&pool, false)
        .await
        .unwrap();
    assert_eq!(counts_of(&pool, &tests).await, counts(0, 0, 0, 1, 0));

    // A restart: a new pool on the same database reads the instant back.
    let again = sqlx::PgPool::connect_with((*pool.connect_options()).clone())
        .await
        .unwrap();
    let answer = run_counts_inner(&again, &registry()).await.unwrap();
    assert!(!answer.visiting);
    assert_eq!(
        answer.total,
        counts(0, 0, 0, 1, 0),
        "kept across the restart"
    );
    assert!(
        !knobas_core::run_read::builds_in_front(&again, false)
            .await
            .unwrap(),
        "leaving what was not in front writes nothing"
    );
}

/// *Mark all as read* clears green by moving the scope's instant (ADR-0063):
/// everything's, or a set of pipelines' own. *Undo* brings back the same
/// number, unless leaving Builds has moved the instant since.
#[tokio::test]
async fn mark_all_takes_the_green_to_zero_and_undo_brings_the_same_number_back() {
    let (pool, tc) = scratch("run-counts-mark-all").await;
    let tests = configuration(&pool, &tc, "Payout_Tests").await;
    let lint = configuration(&pool, &tc, "Payout_Lint").await;
    synced_once(&pool, &tc).await;
    build(
        &pool,
        &tc,
        1001,
        "Payout_Tests",
        "SUCCESS",
        "2026-08-21T01:00:00Z",
    )
    .await;
    build(
        &pool,
        &tc,
        1002,
        "Payout_Tests",
        "SUCCESS",
        "2026-08-21T02:00:00Z",
    )
    .await;
    build(
        &pool,
        &tc,
        2001,
        "Payout_Lint",
        "UNKNOWN",
        "2026-08-21T01:00:00Z",
    )
    .await;
    assert_eq!(total(&pool).await, counts(0, 2, 0, 0, 1));

    let everything = MarkScope {
        everything: true,
        ..MarkScope::default()
    };
    let moved = knobas_core::run_read::move_seen(&pool, &everything)
        .await
        .unwrap()
        .expect("everything reaches the pipelines");
    assert_eq!(total(&pool).await, BubbleCounts::default(), "all clear");
    assert!(
        knobas_core::run_read::put_back_seen(&pool, &moved)
            .await
            .unwrap()
    );
    assert_eq!(
        total(&pool).await,
        counts(0, 2, 0, 0, 1),
        "the same number back"
    );

    let moved =
        knobas_core::run_read::move_seen(&pool, &of_pipelines(std::slice::from_ref(&tests)))
            .await
            .unwrap()
            .unwrap();
    assert_eq!(counts_of(&pool, &tests).await, BubbleCounts::default());
    assert_eq!(
        counts_of(&pool, &lint).await,
        counts(0, 0, 0, 0, 1),
        "another pipeline's stays"
    );
    assert!(
        knobas_core::run_read::put_back_seen(&pool, &moved)
            .await
            .unwrap()
    );
    assert_eq!(total(&pool).await, counts(0, 2, 0, 0, 1));

    assert_eq!(
        knobas_core::run_read::move_seen(&pool, &by_id(&["tc:build:1001".to_owned()]))
            .await
            .unwrap(),
        None,
        "runs by id move no instant"
    );

    let moved = knobas_core::run_read::move_seen(&pool, &everything)
        .await
        .unwrap()
        .unwrap();
    knobas_core::run_read::builds_in_front(&pool, true)
        .await
        .unwrap();
    knobas_core::run_read::builds_in_front(&pool, false)
        .await
        .unwrap();
    assert!(
        !knobas_core::run_read::put_back_seen(&pool, &moved)
            .await
            .unwrap(),
        "leaving Builds moved it since"
    );
    assert_eq!(total(&pool).await, BubbleCounts::default());
}

/// The quit while Builds is in front leaves it (ADR-0063): the exit handler
/// moves the instant before the database stops, so the visit's green is not
/// counted again at the next start. On Tauri's runtime, as the exit handler
/// runs: the leave's timer is made inside it (a timer made outside it had no
/// reactor and the quit panicked, found on the scale database).
#[test]
fn the_quit_while_on_builds_leaves_it() {
    use tauri::Manager as _;
    let pool = tauri::async_runtime::block_on(async {
        let (pool, tc) = scratch("run-counts-quit").await;
        configuration(&pool, &tc, "Payout_Tests").await;
        synced_once(&pool, &tc).await;
        build(
            &pool,
            &tc,
            1001,
            "Payout_Tests",
            "SUCCESS",
            "2026-08-21T01:00:00Z",
        )
        .await;
        knobas_core::run_read::builds_in_front(&pool, true)
            .await
            .unwrap();
        pool
    });
    let lifecycle = knobas_app::Lifecycle::new();
    lifecycle.install(knobas_app::AppState::over_pool(pool.clone()));
    lifecycle.set(knobas_app::DbState::Ready);
    let app = tauri::test::mock_builder()
        .build(tauri::test::mock_context(tauri::test::noop_assets()))
        .expect("a mock app");
    app.manage(lifecycle);

    knobas_app::shutdown(app.handle());

    let answer = tauri::async_runtime::block_on(run_counts_inner(&pool, &registry())).unwrap();
    assert!(!answer.visiting, "the quit left Builds");
    assert_eq!(
        answer.total,
        BubbleCounts::default(),
        "the visit's green was seen"
    );
}
