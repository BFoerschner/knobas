//! Mark builds as read (ux18, migration `0098`) through its commands, over
//! the embedded PostgreSQL with the declarations the compiled-in TeamCity
//! adapter makes (`Registry::for_process`).
//!
//! Each test has a database of its own: the top bar's count reads every
//! pipeline in it.

use knobas_app::commands::entity::{
    complete_inbox_item_inner, inbox_items_inner, mark_runs_read_inner, mark_runs_unread_inner,
    unread_failing_builds_inner,
};

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
    let payload = if status == "running" {
        serde_json::json!({
            "id": id, "number": id.to_string(), "buildTypeId": config, "state": "running",
            "status": "FAILURE", "branchName": "main", "startDate": "20260822T114500+0000"
        })
    } else {
        serde_json::json!({
            "id": id, "number": id.to_string(), "buildTypeId": config, "state": "finished",
            "status": status, "branchName": "main",
            "startDate": "20260822T100500+0000", "finishDate": "20260822T100917+0000"
        })
    };
    item(pool, &entity, source, "build", updated, Some(ME), payload).await;
    entity
}

async fn read(pool: &sqlx::PgPool) -> Vec<String> {
    let mut ids = knobas_core::run_read::read_runs(pool).await.unwrap();
    ids.sort();
    ids
}

/// *Mark all as read* on a pipeline writes its finished failures and nothing
/// else: not a pass, not a run still going (whose state may yet change), not
/// another pipeline's failure. It answers what it wrote, so a second press
/// answers nothing and *Undo* takes back exactly the first.
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
    build(
        &pool,
        &tc,
        1004,
        "Payout_Tests",
        "running",
        "2026-08-21T04:00:00Z",
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

    let (mut wrote, line) =
        mark_runs_read_inner(&pool, &registry(), &[], std::slice::from_ref(&tests))
            .await
            .unwrap();
    wrote.sort();
    assert_eq!(wrote, [red1.clone(), red2.clone()]);
    let line = line.expect("a mark that wrote says so");
    assert_eq!(line.verb, "marked read");
    assert_eq!(line.detail["runs"], 2);
    assert_eq!(read(&pool).await, [red1.clone(), red2.clone()]);
    assert!(
        !read(&pool).await.contains(&lint),
        "another pipeline's stays"
    );

    let (again, line) = mark_runs_read_inner(&pool, &registry(), &[], &[tests])
        .await
        .unwrap();
    assert!(again.is_empty(), "already read: {again:?}");
    assert!(line.is_none(), "a mark that wrote nothing writes no line");

    let (gone, line) = mark_runs_unread_inner(&pool, &wrote).await.unwrap();
    assert_eq!(gone.len(), 2, "Undo takes back both");
    assert_eq!(line.expect("and says so").verb, "marked unread");
    assert!(read(&pool).await.is_empty());
}

/// The top bar's count is pipelines whose newest run failed and is unread:
/// reading that run takes the pipeline off it, and a new failure is a new run
/// with no mark, so it asks again.
#[tokio::test]
async fn the_count_drops_when_the_newest_failure_is_read_and_a_new_failure_asks_again() {
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
    assert_eq!(
        unread_failing_builds_inner(&pool, &registry())
            .await
            .unwrap(),
        1
    );

    mark_runs_read_inner(&pool, &registry(), std::slice::from_ref(&red), &[])
        .await
        .unwrap();
    assert_eq!(
        unread_failing_builds_inner(&pool, &registry())
            .await
            .unwrap(),
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
    assert_eq!(
        unread_failing_builds_inner(&pool, &registry())
            .await
            .unwrap(),
        1,
        "a new failure is unread"
    );
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

    mark_runs_read_inner(&pool, &registry(), std::slice::from_ref(&red), &[])
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
    mark_runs_read_inner(&pool, &registry(), std::slice::from_ref(&red), &[])
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
