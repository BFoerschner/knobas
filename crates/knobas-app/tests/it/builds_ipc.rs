//! The Builds view (#848) through the two commands it calls: `pipelines`
//! for the list on the left and `pipeline_runs` for the selected pipeline's
//! runs, over the embedded PostgreSQL with the declarations the compiled-in
//! TeamCity adapter makes (`Registry::builtin`), never a copy of them.
//!
//! Each test configures a TeamCity source of its own, so the rows it reads
//! back are told apart by `source_id` from anything else in this database.

use knobas_app::commands::entity::{
    pipeline_runs_inner, pipelines_inner, set_pipeline_pinned_inner,
};

/// A TeamCity source nothing else in this database uses.
async fn source(pool: &sqlx::PgPool) -> String {
    let id = format!("tc{}", &uuid::Uuid::new_v4().simple().to_string()[..10]);
    sqlx::query(
        "insert into knobas.source_config (id, kind, display_name, base_url, auth_kind, config)
         values ($1, 'teamcity', 'TeamCity', 'http://x', 'Pat', '{}')",
    )
    .bind(&id)
    .execute(pool)
    .await
    .unwrap();
    id
}

async fn pool() -> sqlx::PgPool {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    pool
}

/// One mirror item, stored as the adapter stores it.
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
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload,
                                item_updated_at)
         values ($1,$2,$3,$1,'',$4, $5::timestamptz)",
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

/// A build configuration, as `BUILD_TYPE_FIELDS` reads one.
async fn configuration(pool: &sqlx::PgPool, source: &str, id: &str, name: &str) -> String {
    let entity = format!("{source}:buildType:{id}");
    item(
        pool,
        &entity,
        source,
        "build_config",
        "2026-08-20T00:00:00Z",
        serde_json::json!({ "id": id, "name": name, "projectId": "Payout", "projectName": "Payout" }),
    )
    .await;
    entity
}

/// A build of `config`, as `BUILD_FIELDS` reads one. `status` `None` is a
/// queued build: no number, no dates.
async fn build(
    pool: &sqlx::PgPool,
    source: &str,
    id: i64,
    config: &str,
    status: Option<&str>,
    updated: &str,
) -> String {
    let entity = format!("{source}:build:{id}");
    let payload = match status {
        Some("running") => serde_json::json!({
            "id": id, "number": id.to_string(), "buildTypeId": config, "state": "running",
            "status": "SUCCESS", "branchName": "feature/PAY-231-sepa-retry",
            "startDate": "20260822T114500+0000"
        }),
        Some(status) => serde_json::json!({
            "id": id, "number": id.to_string(), "buildTypeId": config, "state": "finished",
            "status": status, "branchName": "main",
            "startDate": "20260822T100500+0000", "finishDate": "20260822T100917+0000"
        }),
        None => serde_json::json!({ "id": id, "buildTypeId": config, "state": "queued" }),
    };
    item(pool, &entity, source, "build", updated, payload).await;
    entity
}

/// **The list on the left** (R-9.4a): every pipeline the source declares,
/// by name, with its group and its last runs newest first -- at most twelve,
/// read through the declared frame, so a run is placed under the
/// configuration whose id it names and under no other.
#[tokio::test]
async fn pipelines_carry_their_name_group_and_last_twelve_runs() {
    let pool = pool().await;
    let tc = source(&pool).await;
    let tests = configuration(&pool, &tc, "Payout_IntegrationTests", "Integration Tests").await;
    let lint = configuration(&pool, &tc, "Payout_Lint", "Lint").await;
    for n in 0..14 {
        let status = if n == 13 { "FAILURE" } else { "SUCCESS" };
        let at = format!("2026-08-21T{:02}:00:00Z", n + 1);
        build(
            &pool,
            &tc,
            1000 + n,
            "Payout_IntegrationTests",
            Some(status),
            &at,
        )
        .await;
    }
    let lint_run = build(
        &pool,
        &tc,
        2000,
        "Payout_Lint",
        Some("running"),
        "2026-08-22T11:45:00Z",
    )
    .await;

    let all = pipelines_inner(&pool).await.unwrap();
    let mine: Vec<_> = all.iter().filter(|p| p.source_id == tc).collect();
    let names: Vec<&str> = mine.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["Integration Tests", "Lint"], "by name");

    let it = mine[0];
    assert_eq!(it.entity_id, tests);
    assert_eq!(it.group.as_deref(), Some("Payout"));
    assert_eq!(it.source_name, "TeamCity");
    assert_eq!(it.recent.len(), 12, "at most twelve runs");
    let newest = &it.recent[0];
    assert_eq!(newest.number.as_deref(), Some("1013"));
    assert_eq!(
        newest.state.as_ref().map(|s| s.text.as_str()),
        Some("FAILURE")
    );
    assert_eq!(
        it.recent[11].number.as_deref(),
        Some("1002"),
        "the oldest kept"
    );

    let lint_row = mine[1];
    assert_eq!(lint_row.entity_id, lint);
    assert_eq!(
        lint_row.recent.len(),
        1,
        "a run is under its own pipeline only"
    );
    assert_eq!(lint_row.recent[0].entity_id, lint_run);
    assert!(lint_row.recent[0].started.is_some());
    assert_eq!(lint_row.recent[0].finished, None, "in flight");
}

/// **The runs on the right** (#848's plain list until #851): one pipeline's
/// runs newest first, each with its number, state, branch and both instants,
/// read from TeamCity's basic ISO 8601. A queued run has no number and no
/// dates, and is still a row.
#[tokio::test]
async fn a_pipelines_runs_come_newest_first_with_their_fields() {
    let pool = pool().await;
    let tc = source(&pool).await;
    let tests = configuration(&pool, &tc, "Payout_IntegrationTests", "Integration Tests").await;
    configuration(&pool, &tc, "Payout_Lint", "Lint").await;
    build(
        &pool,
        &tc,
        1186,
        "Payout_IntegrationTests",
        Some("SUCCESS"),
        "2026-08-21T18:10:00Z",
    )
    .await;
    let failed = build(
        &pool,
        &tc,
        1187,
        "Payout_IntegrationTests",
        Some("FAILURE"),
        "2026-08-22T10:09:17Z",
    )
    .await;
    let queued = build(
        &pool,
        &tc,
        1190,
        "Payout_IntegrationTests",
        None,
        "2026-08-22T14:20:00Z",
    )
    .await;
    build(
        &pool,
        &tc,
        2000,
        "Payout_Lint",
        Some("SUCCESS"),
        "2026-08-22T14:30:00Z",
    )
    .await;

    let runs = pipeline_runs_inner(&pool, &tests, None).await.unwrap();
    let ids: Vec<&str> = runs.iter().map(|r| r.entity_id.as_str()).collect();
    assert_eq!(
        ids.len(),
        3,
        "the other pipeline's run is not here: {ids:?}"
    );
    assert_eq!(ids[0], queued);
    assert_eq!(ids[1], failed);

    let q = &runs[0];
    assert_eq!(
        (q.number.as_deref(), q.started, q.finished),
        (None, None, None)
    );
    let f = &runs[1];
    assert_eq!(f.number.as_deref(), Some("1187"));
    assert_eq!(f.branch.as_deref(), Some("main"));
    assert_eq!(f.state.as_ref().map(|s| s.text.as_str()), Some("FAILURE"));
    assert_eq!(
        f.started.map(|t| t.to_rfc3339()).as_deref(),
        Some("2026-08-22T10:05:00+00:00")
    );
    assert_eq!(f.duration_seconds, Some(257));

    let capped = pipeline_runs_inner(&pool, &tests, Some(1)).await.unwrap();
    assert_eq!(capped.len(), 1);
}

/// An address that is not an entity id is a bad address; a mirrored entity
/// that is no pipeline, or none at all, is not found -- neither is an empty
/// list that looks like a pipeline nothing ran.
#[tokio::test]
async fn runs_of_something_that_is_no_pipeline_are_refused() {
    let pool = pool().await;
    let tc = source(&pool).await;
    let run = build(
        &pool,
        &tc,
        1,
        "Nope",
        Some("SUCCESS"),
        "2026-08-22T10:00:00Z",
    )
    .await;
    let code = |e: knobas_app::IpcError| e.code;
    assert_eq!(
        pipeline_runs_inner(&pool, "not an id", None)
            .await
            .map_err(code)
            .unwrap_err(),
        knobas_app::IpcErrorCode::Invalid
    );
    for id in [run, format!("{tc}:buildType:Missing")] {
        assert_eq!(
            pipeline_runs_inner(&pool, &id, None)
                .await
                .map_err(code)
                .unwrap_err(),
            knobas_app::IpcErrorCode::NotFound,
            "{id}"
        );
    }
}

/// **A run finds its pipeline by identity within its own source** (#848):
/// TeamCity names are not unique across projects, and a second server can
/// reuse a configuration id the first one has. Two configurations called
/// "Integration Tests" in two projects of one source, and a second source
/// with the first one's id: each pipeline carries its own run and no other,
/// in both reads.
#[tokio::test]
async fn a_run_joins_its_pipeline_by_id_within_its_source_only() {
    let pool = pool().await;
    let (a, b) = (source(&pool).await, source(&pool).await);
    let a_payout = configuration(&pool, &a, "Payout_IntegrationTests", "Integration Tests").await;
    let a_billing = format!("{a}:buildType:Billing_IntegrationTests");
    item(
        &pool,
        &a_billing,
        &a,
        "build_config",
        "2026-08-20T00:00:00Z",
        serde_json::json!({ "id": "Billing_IntegrationTests", "name": "Integration Tests",
                            "projectId": "Billing", "projectName": "Billing" }),
    )
    .await;
    let b_payout = configuration(&pool, &b, "Payout_IntegrationTests", "Integration Tests").await;
    let at = "2026-08-22T10:09:17Z";
    let run_a_payout = build(&pool, &a, 1, "Payout_IntegrationTests", Some("SUCCESS"), at).await;
    let run_a_billing = build(
        &pool,
        &a,
        2,
        "Billing_IntegrationTests",
        Some("FAILURE"),
        at,
    )
    .await;
    let run_b_payout = build(&pool, &b, 3, "Payout_IntegrationTests", Some("SUCCESS"), at).await;

    let all = pipelines_inner(&pool).await.unwrap();
    for (pipeline, run) in [
        (&a_payout, &run_a_payout),
        (&a_billing, &run_a_billing),
        (&b_payout, &run_b_payout),
    ] {
        let row = all.iter().find(|p| &p.entity_id == pipeline).unwrap();
        let recent: Vec<&str> = row.recent.iter().map(|r| r.entity_id.as_str()).collect();
        assert_eq!(recent, [run.as_str()], "pipelines: {pipeline}");
        let runs = pipeline_runs_inner(&pool, pipeline, None).await.unwrap();
        let ids: Vec<&str> = runs.iter().map(|r| r.entity_id.as_str()).collect();
        assert_eq!(ids, [run.as_str()], "pipeline_runs: {pipeline}");
    }
}

/// **The project tree** (#849, R-9.3): a pipeline carries the chain of
/// groups its source declares, outermost first, and a record without one
/// carries an empty chain (the list draws it under its group).
#[tokio::test]
async fn a_pipeline_carries_its_chain_of_groups_outermost_first() {
    let pool = pool().await;
    let tc = source(&pool).await;
    let deep = format!("{tc}:buildType:Payout_Deploy_Prod");
    item(
        &pool,
        &deep,
        &tc,
        "build_config",
        "2026-08-20T00:00:00Z",
        serde_json::json!({
            "id": "Payout_Deploy_Prod", "name": "Deploy to prod",
            "projectId": "Payments_Payouts_Deploy", "projectName": "Deploy",
            "projectPath": [
                { "id": "Payments", "name": "Payments" },
                { "id": "Payments_Payouts", "name": "Payouts" },
                { "id": "Payments_Payouts_Deploy", "name": "Deploy" }
            ]
        }),
    )
    .await;
    let flat = configuration(&pool, &tc, "Payout_Lint", "Lint").await;

    let all = pipelines_inner(&pool).await.unwrap();
    let path = |id: &str| all.iter().find(|p| p.entity_id == id).unwrap().path.clone();
    assert_eq!(path(&deep), ["Payments", "Payouts", "Deploy"]);
    assert!(path(&flat).is_empty());
}

/// **Pins** (#849): knobas-local, a set by entity id, read back on the list.
#[tokio::test]
async fn a_pin_is_kept_until_it_is_taken_back() {
    let pool = pool().await;
    let tc = source(&pool).await;
    let tests = configuration(&pool, &tc, "Payout_IntegrationTests", "Integration Tests").await;
    let lint = configuration(&pool, &tc, "Payout_Lint", "Lint").await;
    let pinned = |all: &[knobas_app::commands::entity::PipelineRow], id: &str| {
        all.iter().find(|p| p.entity_id == id).unwrap().pinned
    };

    assert!(
        set_pipeline_pinned_inner(&pool, &tests, true)
            .await
            .unwrap()
    );
    assert!(
        set_pipeline_pinned_inner(&pool, &tests, true)
            .await
            .unwrap(),
        "twice is once"
    );
    assert!(set_pipeline_pinned_inner(&pool, &lint, true).await.unwrap());
    let all = pipelines_inner(&pool).await.unwrap();
    assert!(pinned(&all, &tests) && pinned(&all, &lint));

    assert!(
        !set_pipeline_pinned_inner(&pool, &tests, false)
            .await
            .unwrap()
    );
    let all = pipelines_inner(&pool).await.unwrap();
    assert!(!pinned(&all, &tests), "unpinned");
    assert!(pinned(&all, &lint), "the other pin is kept");

    let stored: serde_json::Value =
        sqlx::query_scalar("select value from knobas.setting where key = $1")
            .bind(knobas_app::commands::entity::PINS_SETTING)
            .fetch_one(&pool)
            .await
            .unwrap();
    let ids: Vec<&str> = stored
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|v| v.as_str())
        .collect();
    assert_eq!(ids.iter().filter(|id| **id == lint).count(), 1);
    assert!(!ids.contains(&tests.as_str()));

    let refused = set_pipeline_pinned_inner(&pool, "not an id", true)
        .await
        .unwrap_err();
    assert_eq!(refused.code, knobas_app::IpcErrorCode::Invalid);
}
