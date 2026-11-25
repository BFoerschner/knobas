//! Builds' *Recent* (2026-11-25) through `recent_runs`: the newest runs
//! across every pipeline, newest first by when each started or, not started,
//! was queued (`runMade` in `app/src/lib/tf/builds/builds.ts`), from
//! `sync.run_made` (migration `0103`), over the embedded PostgreSQL with the
//! compiled-in TeamCity adapter's declarations.
//!
//! Each test configures a TeamCity source of its own and asks for it alone.

use knobas_app::commands::entity::recent_runs_inner;
use knobas_core::builds::{RecentCursor, RecentFilter, RecentRuns, RunRow};

async fn pool() -> sqlx::PgPool {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    pool
}

async fn source(pool: &sqlx::PgPool) -> String {
    let id = format!("tc{}", &uuid::Uuid::new_v4().simple().to_string()[..10]);
    crate::support::source_config(pool, &id, "teamcity", "TeamCity", &serde_json::json!({})).await;
    id
}

async fn item(
    pool: &sqlx::PgPool,
    id: &str,
    source: &str,
    kind: &str,
    updated: &str,
    payload: serde_json::Value,
) {
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1,$2,$1)")
        .bind(id)
        .bind(kind)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload, item_updated_at)
         values ($1,$2,$3,$1,'',$4,$5::timestamptz)",
    )
    .bind(id)
    .bind(source)
    .bind(kind)
    .bind(payload)
    .bind(updated)
    .execute(pool)
    .await
    .unwrap();
}

async fn configuration(pool: &sqlx::PgPool, source: &str, id: &str, name: &str) -> String {
    let entity = format!("{source}:buildType:{id}");
    let payload = serde_json::json!({ "id": id, "name": name, "projectId": "Payout", "projectName": "Payout" });
    item(
        pool,
        &entity,
        source,
        "build_config",
        "2026-08-20T00:00:00Z",
        payload,
    )
    .await;
    entity
}

/// A build of `config`: started `start` (TeamCity's basic spelling) and
/// finished `finish` with `status`, or running with no `finish`, or queued with
/// neither; `updated` is the source's last word on it.
#[allow(clippy::too_many_arguments)]
async fn build(
    pool: &sqlx::PgPool,
    source: &str,
    id: i64,
    config: &str,
    branch: &str,
    start: Option<&str>,
    finish: Option<(&str, &str)>,
    updated: &str,
) -> String {
    let entity = format!("{source}:build:{id}");
    let mut payload = serde_json::json!({ "id": id, "buildTypeId": config, "branchName": branch });
    match (start, finish) {
        (Some(start), Some((finish, status))) => {
            payload["number"] = id.to_string().into();
            payload["state"] = "finished".into();
            payload["status"] = status.into();
            payload["startDate"] = start.into();
            payload["finishDate"] = finish.into();
        }
        (Some(start), None) => {
            payload["number"] = id.to_string().into();
            payload["state"] = "running".into();
            payload["status"] = "SUCCESS".into();
            payload["startDate"] = start.into();
        }
        _ => payload["state"] = "queued".into(),
    }
    item(pool, &entity, source, "build", updated, payload).await;
    entity
}

/// A page, as the app reads it after bring-up: the declarations stored and every run's stored row filled
/// (`knobas_core::item_run::fill`), which writes each run's instant (migration `0103`). A run the tests insert after the
/// declarations are stored gets its row, and its instant, from the item triggers as a sync's does.
async fn recent(
    pool: &sqlx::PgPool,
    filter: RecentFilter,
    before: Option<&RecentCursor>,
    limit: Option<u32>,
) -> RecentRuns {
    let registry = knobas_app::sources::Registry::for_process();
    knobas_app::commands::search::declare(pool, &registry)
        .await
        .unwrap();
    knobas_core::item_run::fill(pool).await.unwrap();
    recent_runs_inner(pool, &registry, &filter, before, limit)
        .await
        .unwrap()
}

fn only(source: &str) -> RecentFilter {
    RecentFilter {
        sources: vec![source.to_owned()],
        ..RecentFilter::default()
    }
}

fn ids(page: &RecentRuns) -> Vec<&String> {
    page.runs.iter().map(|r| &r.run.entity_id).collect()
}

/// `runMade`: when it started, else the source's time for it.
fn made(run: &RunRow) -> Option<chrono::DateTime<chrono::Utc>> {
    run.started.or(run.updated_at)
}

/// Five runs over two pipelines. The long Integration build started first and
/// finished last: by the source's last word it is the newest, by when it
/// started the oldest. The queued one sorts by when it was queued.
async fn estate(pool: &sqlx::PgPool, tc: &str) -> (String, String, [String; 5]) {
    let it = configuration(pool, tc, "Payout_IntegrationTests", "Integration Tests").await;
    let lint = configuration(pool, tc, "Payout_Lint", "Lint").await;
    let long = build(
        pool,
        tc,
        1,
        "Payout_IntegrationTests",
        "main",
        Some("20260822T080000+0000"),
        Some(("20260822T120000+0000", "FAILURE")),
        "2026-08-22T12:00:00Z",
    )
    .await;
    let short = build(
        pool,
        tc,
        2,
        "Payout_Lint",
        "main",
        Some("20260822T090000+0000"),
        Some(("20260822T090500+0000", "SUCCESS")),
        "2026-08-22T09:05:00Z",
    )
    .await;
    let feature = build(
        pool,
        tc,
        3,
        "Payout_Lint",
        "feature/PAY-231",
        Some("20260822T100000+0000"),
        Some(("20260822T100500+0000", "FAILURE")),
        "2026-08-22T10:05:00Z",
    )
    .await;
    let running = build(
        pool,
        tc,
        4,
        "Payout_IntegrationTests",
        "feature/PAY-231",
        Some("20260822T110000+0000"),
        None,
        "2026-08-22T11:00:00Z",
    )
    .await;
    let queued = build(
        pool,
        tc,
        5,
        "Payout_Lint",
        "main",
        None,
        None,
        "2026-08-22T11:30:00Z",
    )
    .await;
    (it, lint, [long, short, feature, running, queued])
}

/// **The order is `runMade`'s**: newest first by start, a queued run by when
/// it was queued, never by the source's last word (2026-11-12's ruling).
#[tokio::test]
async fn runs_are_newest_first_by_when_they_started_or_were_queued() {
    let pool = pool().await;
    let tc = source(&pool).await;
    let (it, lint, [long, short, feature, running, queued]) = estate(&pool, &tc).await;

    let page = recent(&pool, only(&tc), None, None).await;
    assert_eq!(ids(&page), [&queued, &running, &feature, &short, &long]);
    // Held to the front end's order: each run's `runMade` falls down the page.
    let order: Vec<_> = page.runs.iter().map(|r| made(&r.run)).collect();
    let mut sorted = order.clone();
    sorted.sort_by(|a, b| b.cmp(a));
    assert_eq!(order, sorted, "newest first by runMade");
    assert!(page.next.is_none(), "one page holds them all");

    // Each run names its pipeline, and each pipeline is named once.
    let by_run: Vec<&String> = page.runs.iter().map(|r| &r.pipeline_id).collect();
    assert_eq!(by_run, [&lint, &it, &lint, &lint, &it]);
    let mut named: Vec<(&str, &str)> = page
        .pipelines
        .iter()
        .map(|p| (p.entity_id.as_str(), p.name.as_str()))
        .collect();
    named.sort_unstable();
    let mut want = vec![(it.as_str(), "Integration Tests"), (lint.as_str(), "Lint")];
    want.sort_unstable();
    assert_eq!(named, want);
    assert!(
        page.pipelines
            .iter()
            .all(|p| p.source_name == "TeamCity" && p.source_id == tc)
    );
}

/// **Pages**: a page of two hands a cursor; the next starts after it and
/// overlaps nothing; the last says there is no more.
#[tokio::test]
async fn pages_follow_on_from_their_cursor() {
    let pool = pool().await;
    let tc = source(&pool).await;
    let (_, _, [long, short, feature, running, queued]) = estate(&pool, &tc).await;

    let first = recent(&pool, only(&tc), None, Some(2)).await;
    assert_eq!(ids(&first), [&queued, &running]);
    let second = recent(&pool, only(&tc), first.next.as_ref(), Some(2)).await;
    assert_eq!(ids(&second), [&feature, &short]);
    let third = recent(&pool, only(&tc), second.next.as_ref(), Some(2)).await;
    assert_eq!(ids(&third), [&long]);
    assert!(third.next.is_none());
}

/// **Filters** narrow at the source of the read: results, branches, sources,
/// and *Mine* with no configured username keeps nothing.
#[tokio::test]
async fn filters_narrow_the_read() {
    let pool = pool().await;
    let tc = source(&pool).await;
    let other = source(&pool).await;
    let (_, _, [long, short, feature, running, queued]) = estate(&pool, &tc).await;
    configuration(&pool, &other, "Other_Build", "Other").await;
    let elsewhere = build(
        &pool,
        &other,
        9,
        "Other_Build",
        "main",
        Some("20260822T113000+0000"),
        None,
        "2026-08-22T11:30:00Z",
    )
    .await;

    let failed = recent(
        &pool,
        RecentFilter {
            results: vec!["Failed".into()],
            ..only(&tc)
        },
        None,
        None,
    )
    .await;
    assert_eq!(ids(&failed), [&feature, &long]);
    let flight = recent(
        &pool,
        RecentFilter {
            results: vec!["Running".into(), "Queued".into()],
            ..only(&tc)
        },
        None,
        None,
    )
    .await;
    assert_eq!(ids(&flight), [&queued, &running]);
    let passed = recent(
        &pool,
        RecentFilter {
            results: vec!["Passed".into()],
            ..only(&tc)
        },
        None,
        None,
    )
    .await;
    assert_eq!(ids(&passed), [&short]);

    let on_branch = recent(
        &pool,
        RecentFilter {
            branches: Some(vec!["feature/PAY-231".into()]),
            ..only(&tc)
        },
        None,
        None,
    )
    .await;
    assert_eq!(ids(&on_branch), [&running, &feature]);
    let no_branch = recent(
        &pool,
        RecentFilter {
            branches: Some(vec![]),
            ..only(&tc)
        },
        None,
        None,
    )
    .await;
    assert!(
        no_branch.runs.is_empty(),
        "a job that reaches no branch keeps nothing"
    );

    let both = recent(
        &pool,
        RecentFilter {
            sources: vec![tc.clone(), other.clone()],
            ..RecentFilter::default()
        },
        None,
        None,
    )
    .await;
    assert_eq!(
        ids(&both),
        [&queued, &elsewhere, &running, &feature, &short, &long]
    );

    let mine = recent(
        &pool,
        RecentFilter {
            mine: true,
            ..only(&tc)
        },
        None,
        None,
    )
    .await;
    assert!(
        mine.runs.is_empty(),
        "no configured username: nobody's runs are mine"
    );
}

/// **The stored instant follows the run**: a queued run that starts moves to
/// when it started, a run whose time moves moves with it, and a deleted run
/// leaves the read.
#[tokio::test]
async fn the_instant_follows_the_run() {
    let pool = pool().await;
    let tc = source(&pool).await;
    let (_, _, [long, short, feature, running, queued]) = estate(&pool, &tc).await;

    // The queued build starts at 08:30: it now sorts between the long one and the short one.
    sqlx::query(
        "update sync.item set payload = payload || '{\"state\":\"running\",\"number\":\"5\",\"startDate\":\"20260822T083000+0000\"}'::jsonb,
                item_updated_at = '2026-08-22T11:40:00Z' where entity_id = $1",
    )
    .bind(&queued)
    .execute(&pool)
    .await
    .unwrap();
    let page = recent(&pool, only(&tc), None, None).await;
    assert_eq!(ids(&page), [&running, &feature, &short, &queued, &long]);

    // A queued run's time is the source's: only `item_updated_at` moves.
    let later = build(
        &pool,
        &tc,
        6,
        "Payout_Lint",
        "main",
        None,
        None,
        "2026-08-22T07:00:00Z",
    )
    .await;
    assert_eq!(
        ids(&recent(&pool, only(&tc), None, None).await).last(),
        Some(&&later)
    );
    sqlx::query(
        "update sync.item set item_updated_at = '2026-08-22T12:30:00Z' where entity_id = $1",
    )
    .bind(&later)
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(
        ids(&recent(&pool, only(&tc), None, None).await).first(),
        Some(&&later)
    );

    sqlx::query("delete from sync.item where entity_id = $1")
        .bind(&feature)
        .execute(&pool)
        .await
        .unwrap();
    let left = recent(&pool, only(&tc), None, None).await;
    assert!(!ids(&left).contains(&&feature));
    let stored: i64 = sqlx::query_scalar("select count(*) from sync.run_made where entity_id = $1")
        .bind(&feature)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(stored, 0, "the instant goes with the run");
}

/// **`sync.run_instant` reads what `parse_instant` reads**, and nothing else.
#[tokio::test]
async fn the_instant_is_read_as_parse_instant_reads_it() {
    let pool = pool().await;
    for raw in [
        "2026-08-22T10:20:00Z",
        "2026-08-22T10:20:00.123+02:00",
        "2026-08-12T10:00:00.000+0200",
        "20260822T102000+0000",
        "2026-08-22 10:20:00+00:00",
        "2026-08-22T10:20:00",
        "20260822T102000",
        "2026-13-40T10:20:00Z",
        "yesterday",
        "  20260822T102000-0130 ",
    ] {
        let sql: Option<chrono::DateTime<chrono::Utc>> =
            sqlx::query_scalar("select sync.run_instant($1)")
                .bind(raw)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(sql, knobas_core::payload::parse_instant(raw), "{raw:?}");
    }
}
