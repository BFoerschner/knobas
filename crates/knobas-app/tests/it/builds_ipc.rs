//! The Builds view (#848) through the two commands it calls: `pipelines`
//! for the list on the left and `pipeline_runs` for the selected pipeline's
//! runs, over the embedded PostgreSQL with the declarations the compiled-in
//! TeamCity adapter makes (`Registry::for_process`), never a copy of them.
//!
//! Each test configures a TeamCity source of its own, so the rows it reads
//! back are told apart by `source_id` from anything else in this database.

use knobas_app::commands::entity::{
    pipeline_runs_inner, pipelines_inner, set_pipeline_pinned_inner,
};

/// A TeamCity source nothing else in this database uses.
async fn source(pool: &sqlx::PgPool) -> String {
    let id = format!("tc{}", &uuid::Uuid::new_v4().simple().to_string()[..10]);
    crate::support::source_config(pool, &id, "teamcity", "TeamCity", &serde_json::json!({})).await;
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

    let all = pipelines_inner(&pool, &knobas_app::sources::Registry::for_process())
        .await
        .unwrap();
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

    let runs = pipeline_runs_inner(
        &pool,
        &knobas_app::sources::Registry::for_process(),
        &tests,
        None,
    )
    .await
    .unwrap();
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

    let capped = pipeline_runs_inner(
        &pool,
        &knobas_app::sources::Registry::for_process(),
        &tests,
        Some(1),
    )
    .await
    .unwrap();
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
        pipeline_runs_inner(
            &pool,
            &knobas_app::sources::Registry::for_process(),
            "not an id",
            None
        )
        .await
        .map_err(code)
        .unwrap_err(),
        knobas_app::IpcErrorCode::Invalid
    );
    for id in [run, format!("{tc}:buildType:Missing")] {
        assert_eq!(
            pipeline_runs_inner(
                &pool,
                &knobas_app::sources::Registry::for_process(),
                &id,
                None
            )
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

    let all = pipelines_inner(&pool, &knobas_app::sources::Registry::for_process())
        .await
        .unwrap();
    for (pipeline, run) in [
        (&a_payout, &run_a_payout),
        (&a_billing, &run_a_billing),
        (&b_payout, &run_b_payout),
    ] {
        let row = all.iter().find(|p| &p.entity_id == pipeline).unwrap();
        let recent: Vec<&str> = row.recent.iter().map(|r| r.entity_id.as_str()).collect();
        assert_eq!(recent, [run.as_str()], "pipelines: {pipeline}");
        let runs = pipeline_runs_inner(
            &pool,
            &knobas_app::sources::Registry::for_process(),
            pipeline,
            None,
        )
        .await
        .unwrap();
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

    let all = pipelines_inner(&pool, &knobas_app::sources::Registry::for_process())
        .await
        .unwrap();
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
    let pinned = |all: &[knobas_core::builds::PipelineRow], id: &str| {
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
    let all = pipelines_inner(&pool, &knobas_app::sources::Registry::for_process())
        .await
        .unwrap();
    assert!(pinned(&all, &tests) && pinned(&all, &lint));

    assert!(
        !set_pipeline_pinned_inner(&pool, &tests, false)
            .await
            .unwrap()
    );
    let all = pipelines_inner(&pool, &knobas_app::sources::Registry::for_process())
        .await
        .unwrap();
    assert!(!pinned(&all, &tests), "unpinned");
    assert!(pinned(&all, &lint), "the other pin is kept");

    let stored: serde_json::Value =
        sqlx::query_scalar("select value from knobas.setting where key = $1")
            .bind(knobas_core::builds::PINS_SETTING)
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

// -- the pipeline page's two live reads (#851) -------------------------------

/// A registry whose TeamCity is a fake answering the two pipeline reads, so
/// what is under test is the app's half: routing, the run kind's declared
/// paths over the answer, and the repository resolved against the mirror.
/// The real adapter's half is witnessed on the seeded TeamCity
/// (`live_teamcity_seeded.rs`).
struct Answering;

/// [`Answering`] as the reads take a registry: shared, so a build can run on
/// the blocking pool (#1705).
fn answering() -> std::sync::Arc<dyn knobas_sync::scheduler::AdapterRegistry> {
    std::sync::Arc::new(Answering)
}

impl knobas_sync::scheduler::AdapterRegistry for Answering {
    fn descriptors(&self) -> Vec<knobas_source::SourceDescriptor> {
        knobas_app::sources::Registry::for_process().descriptors()
    }
    fn build(
        &self,
        instance: knobas_source::instance::SourceInstance,
    ) -> Result<Box<dyn knobas_source::Source>, knobas_source::SourceError> {
        Ok(Box::new(Answers { id: instance.id }))
    }
}

struct Answers {
    id: String,
}

#[async_trait::async_trait]
impl knobas_source::Source for Answers {
    fn descriptor(&self) -> knobas_source::SourceDescriptor {
        use knobas_sync::scheduler::AdapterRegistry as _;
        let mut d = knobas_app::sources::Registry::for_process()
            .descriptors()
            .into_iter()
            .find(|d| d.adapter_kind == "teamcity")
            .expect("TeamCity is compiled in");
        d.id = self.id.clone();
        d
    }
    async fn test_connection(
        &self,
    ) -> Result<knobas_source::ConnectionInfo, knobas_source::SourceError> {
        Ok(knobas_source::ConnectionInfo::default())
    }
    async fn sync(
        &self,
        cursor: Option<knobas_source::Cursor>,
        _sink: &mut (dyn knobas_source::Sink + Send),
    ) -> Result<knobas_source::Cursor, knobas_source::SourceError> {
        Ok(cursor.unwrap_or_default())
    }
    async fn write(
        &self,
        _op: knobas_source::WriteOp,
    ) -> Result<knobas_source::WriteReceipt, knobas_source::SourceError> {
        Err(knobas_source::SourceError::protocol("read-only"))
    }
    async fn reachable_transitions(
        &self,
        _entity: &str,
    ) -> Result<Vec<String>, knobas_source::SourceError> {
        Err(knobas_source::SourceError::protocol("no workflow"))
    }
    async fn view(
        &self,
        _entity: &str,
        _section: Option<&str>,
    ) -> Result<Option<knobas_source::view::SourceView>, knobas_source::SourceError> {
        Ok(None)
    }
    async fn pipeline_settings(
        &self,
        entity: &str,
    ) -> Result<Option<knobas_source::pipeline::PipelineSettings>, knobas_source::SourceError> {
        use knobas_source::pipeline::{PipelineRepository, PipelineSettings, PipelineTrigger};
        if entity.ends_with(":buildType:Down") {
            return Err(knobas_source::SourceError::Unreachable(
                "connection refused".to_owned(),
            ));
        }
        if entity.ends_with(":buildType:Slow") {
            // A source that never answers (#1144): the caller's budget ends it.
            tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
        }
        let repository = |name: &str, url: Option<&str>| PipelineRepository {
            name: name.to_owned(),
            url: url.map(str::to_owned),
            default_branch: Some("refs/heads/main".to_owned()),
            entity_id: None,
        };
        Ok(Some(PipelineSettings {
            repositories: vec![
                repository(
                    "payout-service",
                    Some("HTTP://Gitea.Example/tidewater/payout-service.git"),
                ),
                repository(
                    "elsewhere",
                    Some("http://gitea.example/tidewater/elsewhere.git"),
                ),
                repository("no url", None),
            ],
            triggers: vec![PipelineTrigger {
                name: "VCS".to_owned(),
                facts: vec![],
                disabled: false,
            }],
            parameters: vec![],
            parameter_write_op: Some("set_parameter".to_owned()),
            parameter_specs: vec![],
            chain: vec![],
            steps: vec!["Checkout".to_owned(), "cargo test".to_owned()],
        }))
    }
    async fn run_chain(
        &self,
        run: &str,
    ) -> Result<Option<Vec<knobas_source::pipeline::RunChainStep>>, knobas_source::SourceError>
    {
        use knobas_source::pipeline::{ChainStep, RunChainStep};
        if run.ends_with(":build:404") {
            return Err(knobas_source::SourceError::Unreachable(
                "connection refused".to_owned(),
            ));
        }
        if !run.ends_with(":build:1200") {
            return Ok(Some(vec![]));
        }
        let step = |key: &str, name: &str| ChainStep {
            pipeline: format!("{}:buildType:{key}", self.id),
            name: name.to_owned(),
        };
        let item = |key: &str, kind: &str, payload: serde_json::Value| knobas_source::SyncItem {
            entity: knobas_core::entity::EntityRef::parse(&format!("{}:{key}", self.id)).unwrap(),
            kind: kind.to_owned(),
            title: key.to_owned(),
            body_text: String::new(),
            author: None,
            updated_at: None,
            payload,
            web_url: None,
            deleted: false,
        };
        Ok(Some(vec![
            RunChainStep {
                step: step("Payout_Build", "Build"),
                run: Some(item(
                    "build:1199",
                    "build",
                    serde_json::json!({
                        "id": 1199, "number": "1199", "buildTypeId": "Payout_Build",
                        "state": "finished", "status": "SUCCESS", "branchName": "feature/x"
                    }),
                )),
            },
            RunChainStep {
                step: step("Payout_IntegrationTests", "Integration Tests"),
                run: Some(item(
                    "build:1200",
                    "build",
                    serde_json::json!({
                        "id": 1200, "number": "1200", "buildTypeId": "Payout_IntegrationTests",
                        "state": "running", "branchName": "feature/x"
                    }),
                )),
            },
            // A kind outside the run frame is no run.
            RunChainStep {
                step: step("Payout_Docker", "Docker"),
                run: Some(item(
                    "buildType:Payout_Docker",
                    "build_config",
                    serde_json::json!({ "id": "Payout_Docker" }),
                )),
            },
            RunChainStep {
                step: step("Payout_Deploy", "Deploy"),
                run: None,
            },
        ]))
    }
    async fn older_runs(
        &self,
        entity: &str,
        before: &str,
        count: u32,
    ) -> Result<Option<Vec<knobas_source::SyncItem>>, knobas_source::SourceError> {
        assert!(
            entity.ends_with(":buildType:Payout_IntegrationTests"),
            "{entity}"
        );
        assert!(before.ends_with(":build:1100"), "{before}");
        assert!((1..=knobas_source::pipeline::OLDER_RUNS_MAX).contains(&count));
        let item = |key: &str, kind: &str, payload: serde_json::Value| knobas_source::SyncItem {
            entity: knobas_core::entity::EntityRef::parse(&format!("{}:{key}", self.id)).unwrap(),
            kind: kind.to_owned(),
            title: key.to_owned(),
            body_text: String::new(),
            author: None,
            updated_at: None,
            payload,
            web_url: Some(format!("http://ci.example/{key}")),
            deleted: false,
        };
        Ok(Some(vec![
            // The run the page holds already: the source may answer it too.
            item(
                "build:1100",
                "build",
                serde_json::json!({ "id": 1100, "buildTypeId": "Payout_IntegrationTests" }),
            ),
            item(
                "build:1099",
                "build",
                serde_json::json!({
                    "id": 1099, "number": "1099", "buildTypeId": "Payout_IntegrationTests",
                    "state": "finished", "status": "FAILURE", "branchName": "main",
                    "startDate": "20260801T100000+0000", "finishDate": "20260801T100400+0000",
                    "triggered": { "type": "user", "user": { "username": "older-run-starter" } },
                    "testOccurrences": { "failed": 1, "passed": 41, "ignored": 2 },
                    "revisions": { "revision": [{ "version": "91fe0c2b" }, { "version": "ffff" }] }
                }),
            ),
            // A kind outside the run frame is not a run, whatever it says.
            item(
                "buildType:Payout_IntegrationTests",
                "build_config",
                serde_json::json!({ "id": "Payout_IntegrationTests", "name": "x" }),
            ),
            item(
                "build:1098",
                "build",
                serde_json::json!({
                    "id": 1098, "number": "1098", "buildTypeId": "Payout_IntegrationTests",
                    "state": "finished", "status": "SUCCESS", "branchName": "feature/x",
                    "startDate": "20260801T090000+0000", "finishDate": "20260801T090300+0000"
                }),
            ),
        ]))
    }
}

fn no_secrets() -> std::sync::Arc<dyn knobas_secrets::SecretStore> {
    std::sync::Arc::new(knobas_secrets::MemoryStore::new())
}

/// **Older history** (R-9.5): the runs the source answers beyond the mirror,
/// read through the run kind's declarations as a mirrored run is, newest
/// first, without the run the page already holds and without anything that is
/// not a run.
#[tokio::test]
async fn older_runs_are_the_sources_answer_read_through_the_run_declarations() {
    use knobas_app::commands::entity::older_pipeline_runs_inner;
    let pool = pool().await;
    let tc = source(&pool).await;
    let tests = configuration(&pool, &tc, "Payout_IntegrationTests", "Integration Tests").await;
    let oldest = build(
        &pool,
        &tc,
        1100,
        "Payout_IntegrationTests",
        Some("SUCCESS"),
        "2026-08-02T10:00:00Z",
    )
    .await;

    let older =
        older_pipeline_runs_inner(&pool, &no_secrets(), &answering(), &tests, &oldest, Some(10))
            .await
            .unwrap();
    let ids: Vec<&str> = older.iter().map(|r| r.entity_id.as_str()).collect();
    assert_eq!(
        ids,
        [format!("{tc}:build:1099"), format!("{tc}:build:1098")]
    );
    let failed = &older[0];
    assert_eq!(failed.number.as_deref(), Some("1099"));
    assert_eq!(failed.branch.as_deref(), Some("main"));
    assert_eq!(
        failed.state.as_ref().map(|s| s.text.as_str()),
        Some("FAILURE")
    );
    assert_eq!(failed.duration_seconds, Some(240));
    assert_eq!(
        failed.web_url.as_deref(),
        Some("http://ci.example/build:1099")
    );
    // The trigger, who started it and its commit, as a mirrored run reads
    // them (#961); nobody configured that account, so it is not the reader.
    assert_eq!(failed.trigger, Some(knobas_core::payload::Trigger::Person));
    assert_eq!(failed.started_by.as_deref(), Some("older-run-starter"));
    assert!(!failed.started_by_me);
    assert_eq!(failed.revision.as_deref(), Some("91fe0c2b"));
    assert_eq!(
        (older[1].trigger, older[1].started_by.as_deref()),
        (None, None)
    );
    // Its test summary, as a mirrored run reads it (#1448); a run that ran
    // no tests has none.
    assert_eq!(
        failed.tests,
        Some(knobas_core::builds::TestSummary {
            failed: 1,
            passed: 41,
            ignored: 2
        })
    );
    assert_eq!(older[1].tests, None);

    let capped =
        older_pipeline_runs_inner(&pool, &no_secrets(), &answering(), &tests, &oldest, Some(1))
            .await
            .unwrap();
    assert_eq!(capped.len(), 1);
}

/// A pipeline the mirror does not hold is not found, and a run of another
/// source is a bad address -- neither reaches a source.
#[tokio::test]
async fn older_runs_of_no_pipeline_or_a_foreign_run_are_refused() {
    use knobas_app::commands::entity::older_pipeline_runs_inner;
    let pool = pool().await;
    let tc = source(&pool).await;
    let tests = configuration(&pool, &tc, "Payout_IntegrationTests", "Integration Tests").await;
    let run = build(
        &pool,
        &tc,
        1,
        "Payout_IntegrationTests",
        Some("SUCCESS"),
        "2026-08-22T10:00:00Z",
    )
    .await;
    let code = |r: Result<Vec<_>, knobas_app::IpcError>| r.unwrap_err().code;
    assert_eq!(
        code(older_pipeline_runs_inner(&pool, &no_secrets(), &answering(), &run, &run, None).await),
        knobas_app::IpcErrorCode::NotFound
    );
    assert_eq!(
        code(
            older_pipeline_runs_inner(
                &pool,
                &no_secrets(),
                &answering(),
                &tests,
                "other:build:1",
                None
            )
            .await
        ),
        knobas_app::IpcErrorCode::Invalid
    );
    assert_eq!(
        code(
            older_pipeline_runs_inner(&pool, &no_secrets(), &answering(), "no id", &run, None).await
        ),
        knobas_app::IpcErrorCode::Invalid
    );
}

/// **The settings** (R-9.4): the source's answer as it is, but for the
/// repository the mirror holds, which gets that repository's id; one the
/// mirror does not hold, or with no URL, stays a name. A source that cannot be
/// reached is a rejection, not an empty page.
#[tokio::test]
async fn settings_name_the_mirrored_repository_by_its_id() {
    use knobas_app::commands::entity::pipeline_settings_inner;
    let pool = pool().await;
    let tc = source(&pool).await;
    let tests = configuration(&pool, &tc, "Payout_IntegrationTests", "Integration Tests").await;
    let down = configuration(&pool, &tc, "Down", "Down").await;
    let repo = format!("gitea{}:repo:tidewater/payout-service", &tc[2..]);
    sqlx::query(
        "insert into knobas.entity (id, kind, title) values ($1, 'repo', 'payout-service')",
    )
    .bind(&repo)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, payload, web_url)
         values ($1, $2, 'repo', 'payout-service', '{}'::jsonb, $3)",
    )
    .bind(&repo)
    .bind(format!("gitea{}", &tc[2..]))
    .bind("http://gitea.example/tidewater/payout-service")
    .execute(&pool)
    .await
    .unwrap();

    let settings = pipeline_settings_inner(&pool, &no_secrets(), &answering(), &tests)
        .await
        .unwrap()
        .expect("a TeamCity pipeline has settings");
    let ids: Vec<Option<&str>> = settings
        .repositories
        .iter()
        .map(|r| r.entity_id.as_deref())
        .collect();
    assert_eq!(ids, [Some(repo.as_str()), None, None]);
    assert_eq!(settings.triggers[0].name, "VCS");
    // #1016: the steps a run takes cross as the source named them.
    assert_eq!(settings.steps, ["Checkout", "cargo test"]);

    let failed = pipeline_settings_inner(&pool, &no_secrets(), &answering(), &down)
        .await
        .unwrap_err();
    assert_eq!(failed.code, knobas_app::IpcErrorCode::Unreachable);
}

/// **Build chains** (#857): the source's chain of a run, each step's run read
/// through the run kind's declarations; a step that has not run on the branch,
/// or whose answer is no run, has none. A run in no chain has an empty chain,
/// and the source's failure keeps its code.
#[tokio::test]
async fn a_runs_chain_is_the_sources_answer_read_through_the_run_declarations() {
    use knobas_app::commands::entity::run_chain_inner;
    let pool = pool().await;
    let tc = source(&pool).await;

    let chain = run_chain_inner(
        &pool,
        &no_secrets(),
        &answering(),
        &format!("{tc}:build:1200"),
    )
    .await
    .unwrap();
    let got: Vec<(&str, &str, Option<&str>)> = chain
        .iter()
        .map(|s| {
            (
                s.pipeline_id.as_str(),
                s.name.as_str(),
                s.run.as_ref().map(|r| r.entity_id.as_str()),
            )
        })
        .collect();
    let (build, tests, docker, deploy) = (
        format!("{tc}:buildType:Payout_Build"),
        format!("{tc}:buildType:Payout_IntegrationTests"),
        format!("{tc}:buildType:Payout_Docker"),
        format!("{tc}:buildType:Payout_Deploy"),
    );
    let (b1199, b1200) = (format!("{tc}:build:1199"), format!("{tc}:build:1200"));
    assert_eq!(
        got,
        [
            (build.as_str(), "Build", Some(b1199.as_str())),
            (tests.as_str(), "Integration Tests", Some(b1200.as_str())),
            (docker.as_str(), "Docker", None),
            (deploy.as_str(), "Deploy", None),
        ]
    );
    let first = chain[0].run.as_ref().unwrap();
    assert_eq!(first.number.as_deref(), Some("1199"));
    assert_eq!(first.branch.as_deref(), Some("feature/x"));
    assert_eq!(
        first.state.as_ref().map(|s| s.text.as_str()),
        Some("SUCCESS")
    );

    let none = run_chain_inner(&pool, &no_secrets(), &answering(), &format!("{tc}:build:7"))
        .await
        .unwrap();
    assert!(none.is_empty());

    let failed = run_chain_inner(&pool, &no_secrets(), &answering(), &format!("{tc}:build:404"))
        .await
        .unwrap_err();
    assert_eq!(failed.code, knobas_app::IpcErrorCode::Unreachable);
    let bad = run_chain_inner(&pool, &no_secrets(), &answering(), "not an id")
        .await
        .unwrap_err();
    assert_eq!(bad.code, knobas_app::IpcErrorCode::Invalid);
}

/// **Where a run ran, what it is doing now, and the commit it opens**
/// (#1004): the declared `agent` and `current_step`, and the mirrored commit
/// whose declared `commit` is the run's revision -- a commit, never a branch
/// standing at the same sha -- on the pipeline's runs and the list's.
#[tokio::test]
async fn runs_carry_their_agent_what_they_do_now_and_their_mirrored_commit() {
    let pool = pool().await;
    let tc = source(&pool).await;
    let gitea = format!("gitea{}", &tc[2..]);
    crate::support::source_config(&pool, &gitea, "gitea", "Gitea", &serde_json::json!({})).await;
    let sha = format!("a41f2c0b9e11{}", &tc[2..]);
    let commit = format!("{gitea}:commit:tidewater/payout-service/{sha}");
    let tip = format!("{gitea}:branch:tidewater/payout-service/main");
    let at = "2026-08-20T00:00:00Z";
    item(
        &pool,
        &tip,
        &gitea,
        "branch",
        at,
        serde_json::json!({ "name": "main", "commit": { "id": sha } }),
    )
    .await;
    item(
        &pool,
        &commit,
        &gitea,
        "commit",
        at,
        serde_json::json!({ "sha": sha }),
    )
    .await;
    let tests = configuration(&pool, &tc, "Payout_IntegrationTests", "Integration Tests").await;
    let run = |id: i64, more: serde_json::Value| {
        let mut run = serde_json::json!({
            "id": id, "number": id.to_string(), "buildTypeId": "Payout_IntegrationTests",
            "branchName": "main", "startDate": "20260821T100000+0000"
        });
        run.as_object_mut()
            .unwrap()
            .extend(more.as_object().unwrap().clone());
        run
    };
    let running = run(
        1,
        serde_json::json!({
            "state": "running", "agent": { "name": "agent-linux-03" },
            "running-info": { "currentStageText": "step 3/5 `cargo test`" },
            "revisions": { "revision": [{ "version": sha }] }
        }),
    );
    let finished = run(
        2,
        serde_json::json!({
            "state": "finished", "status": "SUCCESS", "finishDate": "20260821T100412+0000",
            "agent": { "name": "agent-linux-01" },
            "revisions": { "revision": [{ "version": "0000000unmirrored" }] }
        }),
    );
    item(
        &pool,
        &format!("{tc}:build:1"),
        &tc,
        "build",
        "2026-08-21T02:00:00Z",
        running,
    )
    .await;
    item(
        &pool,
        &format!("{tc}:build:2"),
        &tc,
        "build",
        "2026-08-21T01:00:00Z",
        finished,
    )
    .await;

    let runs = pipeline_runs_inner(
        &pool,
        &knobas_app::sources::Registry::for_process(),
        &tests,
        None,
    )
    .await
    .unwrap();
    let seen: Vec<_> = runs
        .iter()
        .map(|r| {
            (
                r.agent.as_deref(),
                r.current_step.as_deref(),
                r.commit_id.as_deref(),
            )
        })
        .collect();
    assert_eq!(
        seen,
        [
            (
                Some("agent-linux-03"),
                Some("step 3/5 `cargo test`"),
                Some(commit.as_str())
            ),
            (Some("agent-linux-01"), None, None),
        ]
    );
    let listed = pipelines_inner(&pool, &knobas_app::sources::Registry::for_process())
        .await
        .unwrap();
    let row = listed.iter().find(|p| p.entity_id == tests).unwrap();
    let recent: Vec<_> = row
        .recent
        .iter()
        .map(|r| (r.agent.clone(), r.current_step.clone(), r.commit_id.clone()))
        .collect();
    let from_runs: Vec<_> = runs
        .iter()
        .map(|r| (r.agent.clone(), r.current_step.clone(), r.commit_id.clone()))
        .collect();
    assert_eq!(
        recent, from_runs,
        "the list reads a run as the pipeline does"
    );
}

/// **A run's test summary** (#1448, R-9.6): the declared `tests_failed`,
/// `tests_passed` and `tests_ignored`, on the pipeline's runs and the list's.
/// A run that ran no tests has no summary; a summary TeamCity served with a
/// zero reads the zero.
#[tokio::test]
async fn runs_carry_their_test_summary_and_a_run_without_tests_none() {
    let pool = pool().await;
    let tc = source(&pool).await;
    let tests = configuration(&pool, &tc, "Payout_IntegrationTests", "Integration Tests").await;
    let run = |id: i64, more: serde_json::Value| {
        let mut run = serde_json::json!({
            "id": id, "number": id.to_string(), "buildTypeId": "Payout_IntegrationTests",
            "state": "finished", "status": "FAILURE", "branchName": "main",
            "startDate": "20260821T100000+0000", "finishDate": "20260821T100412+0000"
        });
        run.as_object_mut()
            .unwrap()
            .extend(more.as_object().unwrap().clone());
        run
    };
    for (id, updated, more) in [
        (
            1,
            "2026-08-21T03:00:00Z",
            serde_json::json!({ "testOccurrences": { "failed": 1, "passed": 41, "ignored": 2 } }),
        ),
        (
            2,
            "2026-08-21T02:00:00Z",
            serde_json::json!({ "testOccurrences": { "failed": 0, "passed": 55, "ignored": 0 } }),
        ),
        (3, "2026-08-21T01:00:00Z", serde_json::json!({})),
    ] {
        item(
            &pool,
            &format!("{tc}:build:{id}"),
            &tc,
            "build",
            updated,
            run(id, more),
        )
        .await;
    }
    let summary = |failed, passed, ignored| {
        Some(knobas_core::builds::TestSummary {
            failed,
            passed,
            ignored,
        })
    };

    let runs = pipeline_runs_inner(
        &pool,
        &knobas_app::sources::Registry::for_process(),
        &tests,
        None,
    )
    .await
    .unwrap();
    let seen: Vec<_> = runs.iter().map(|r| r.tests).collect();
    assert_eq!(seen, [summary(1, 41, 2), summary(0, 55, 0), None]);
    let listed = pipelines_inner(&pool, &knobas_app::sources::Registry::for_process())
        .await
        .unwrap();
    let row = listed.iter().find(|p| p.entity_id == tests).unwrap();
    let recent: Vec<_> = row.recent.iter().map(|r| r.tests).collect();
    assert_eq!(recent, seen, "the list reads a run as the pipeline does");
}

/// **A sha two mirrored repositories share** (#1144): a fork or a mirror on a
/// second forge holds the same commit, so the sha alone does not say which
/// one a run built. The run's commit is the one in the repository its
/// pipeline builds, by that repository's URL and the sha as a build view's
/// commit row is (#665). Where the pipeline's repositories cannot be read,
/// an ambiguous sha links nothing, and a sha one commit holds still links it.
#[tokio::test]
async fn a_shared_sha_links_the_commit_in_the_repository_the_pipeline_builds() {
    use knobas_app::commands::entity::pipeline_runs_live_inner;
    let pool = pool().await;
    let tc = source(&pool).await;
    let suffix = &tc[2..];
    // The fork's source id sorts before the pipeline's own Gitea, so the
    // lowest id is the wrong one.
    let gitea = format!("gitea{suffix}");
    let fork = format!("fork{suffix}");
    for (id, base) in [
        (&gitea, "http://gitea.example"),
        (&fork, "http://fork.example"),
    ] {
        sqlx::query(
            "insert into knobas.source_config (id, kind, display_name, base_url, auth_kind, config)
             values ($1, 'gitea', $1, $2, 'Pat', '{}')",
        )
        .bind(id)
        .bind(base)
        .execute(&pool)
        .await
        .unwrap();
    }
    let at = "2026-08-20T00:00:00Z";
    let commit =
        |source: &str, sha: &str| format!("{source}:commit:tidewater/payout-service/{sha}");
    let mirrored = |source: String, base: &'static str, sha: String| {
        let pool = pool.clone();
        async move {
            let id = commit(&source, &sha);
            item(
                &pool,
                &id,
                &source,
                "commit",
                at,
                serde_json::json!({ "sha": sha }),
            )
            .await;
            sqlx::query("update sync.item set web_url = $2 where entity_id = $1")
                .bind(&id)
                .bind(format!("{base}/tidewater/payout-service/commit/{sha}"))
                .execute(&pool)
                .await
                .unwrap();
            id
        }
    };
    let shared = format!("b7e3{suffix}");
    let only = format!("c0de{suffix}");
    let ours = mirrored(gitea.clone(), "http://gitea.example", shared.clone()).await;
    let theirs = mirrored(fork.clone(), "http://fork.example", shared.clone()).await;
    let alone = mirrored(fork.clone(), "http://fork.example", only.clone()).await;
    assert!(theirs < ours, "the fork's commit is the lowest id");

    let tests = configuration(&pool, &tc, "Payout_IntegrationTests", "Integration Tests").await;
    let down = configuration(&pool, &tc, "Down", "Down").await;
    for (id, config, sha) in [
        (1, "Payout_IntegrationTests", &shared),
        (2, "Payout_IntegrationTests", &only),
        (3, "Down", &shared),
        (4, "Down", &only),
    ] {
        item(
            &pool,
            &format!("{tc}:build:{id}"),
            &tc,
            "build",
            &format!("2026-08-21T0{id}:00:00Z"),
            serde_json::json!({
                "id": id, "number": id.to_string(), "buildTypeId": config,
                "state": "finished", "status": "SUCCESS", "branchName": "main",
                "revisions": { "revision": [{ "version": sha }] }
            }),
        )
        .await;
    }
    let commits = |runs: Vec<knobas_core::builds::RunRow>| {
        runs.into_iter()
            .map(|r| (r.entity_id, r.commit_id))
            .collect::<Vec<_>>()
    };
    let (b1, b2, b3, b4) = (
        format!("{tc}:build:1"),
        format!("{tc}:build:2"),
        format!("{tc}:build:3"),
        format!("{tc}:build:4"),
    );

    // The pipeline builds payout-service on gitea.example: the shared sha is
    // that repository's commit, not the fork's.
    let live = pipeline_runs_live_inner(&pool, &no_secrets(), &answering(), &tests, None)
        .await
        .unwrap();
    assert_eq!(
        commits(live),
        [(b2.clone(), Some(alone.clone())), (b1.clone(), Some(ours))]
    );

    // A pipeline whose settings cannot be read: the shared sha links nothing,
    // the sha one commit holds still links it.
    let unread = pipeline_runs_live_inner(&pool, &no_secrets(), &answering(), &down, None)
        .await
        .unwrap();
    assert_eq!(commits(unread), [(b4, Some(alone.clone())), (b3, None)]);

    // The mirror alone cannot say which repository a run built: the same.
    let mirror = pipeline_runs_inner(
        &pool,
        &knobas_app::sources::Registry::for_process(),
        &tests,
        None,
    )
    .await
    .unwrap();
    assert_eq!(commits(mirror), [(b2, Some(alone)), (b1, None)]);
}

/// **The edges of a shared sha** (#1144, merge review). A sha that two of the
/// pipeline's own repositories hold names no one commit, so it links nothing.
/// A page URL that resolves to an item other than a commit is no hit, so it
/// does not crowd out the one commit that is. And a source that never answers
/// costs the runs their link, not their answer.
#[tokio::test]
async fn a_shared_sha_links_nothing_it_cannot_pin_and_never_waits_on_a_silent_source() {
    use knobas_app::commands::entity::pipeline_runs_live_inner;
    let pool = pool().await;
    let tc = source(&pool).await;
    let suffix = &tc[2..];
    let gitea = format!("gitea{suffix}");
    let fork = format!("fork{suffix}");
    for (id, base) in [
        (&gitea, "http://gitea.example"),
        (&fork, "http://fork.example"),
    ] {
        sqlx::query(
            "insert into knobas.source_config (id, kind, display_name, base_url, auth_kind, config)
             values ($1, 'gitea', $1, $2, 'Pat', '{}')",
        )
        .bind(id)
        .bind(base)
        .execute(&pool)
        .await
        .unwrap();
    }
    let at = "2026-08-20T00:00:00Z";
    let mirrored = |source: String, kind: &'static str, repo: &'static str, sha: String| {
        let pool = pool.clone();
        async move {
            let base = if source.starts_with("fork") {
                "http://fork.example"
            } else {
                "http://gitea.example"
            };
            let id = format!("{source}:{kind}:tidewater/{repo}/{sha}");
            let payload = if kind == "commit" {
                serde_json::json!({ "sha": sha })
            } else {
                serde_json::json!({})
            };
            item(&pool, &id, &source, kind, at, payload).await;
            sqlx::query("update sync.item set web_url = $2 where entity_id = $1")
                .bind(&id)
                .bind(format!("{base}/tidewater/{repo}/commit/{sha}"))
                .execute(&pool)
                .await
                .unwrap();
            id
        }
    };
    // Both of the pipeline's repositories hold `both`.
    let both = format!("b0b0{suffix}");
    mirrored(gitea.clone(), "commit", "payout-service", both.clone()).await;
    mirrored(gitea.clone(), "commit", "elsewhere", both.clone()).await;
    // `branchy`: the fork's commit, the pipeline's `elsewhere` commit, and an
    // item that is no commit at the page `payout-service` would have.
    let branchy = format!("b4a7{suffix}");
    mirrored(fork.clone(), "commit", "payout-service", branchy.clone()).await;
    let elsewhere = mirrored(gitea.clone(), "commit", "elsewhere", branchy.clone()).await;
    mirrored(gitea.clone(), "branch", "payout-service", branchy.clone()).await;

    let tests = configuration(&pool, &tc, "Payout_IntegrationTests", "Integration Tests").await;
    let slow = configuration(&pool, &tc, "Slow", "Slow").await;
    for (id, config, sha) in [
        (1, "Payout_IntegrationTests", &both),
        (2, "Payout_IntegrationTests", &branchy),
        (3, "Slow", &both),
    ] {
        item(
            &pool,
            &format!("{tc}:build:{id}"),
            &tc,
            "build",
            &format!("2026-08-21T0{id}:00:00Z"),
            serde_json::json!({
                "id": id, "number": id.to_string(), "buildTypeId": config,
                "state": "finished", "status": "SUCCESS", "branchName": "main",
                "revisions": { "revision": [{ "version": sha }] }
            }),
        )
        .await;
    }
    let commits = |runs: Vec<knobas_core::builds::RunRow>| {
        runs.into_iter()
            .map(|r| (r.entity_id, r.commit_id))
            .collect::<Vec<_>>()
    };

    let live = pipeline_runs_live_inner(&pool, &no_secrets(), &answering(), &tests, None)
        .await
        .unwrap();
    assert_eq!(
        commits(live),
        [
            (format!("{tc}:build:2"), Some(elsewhere)),
            (format!("{tc}:build:1"), None),
        ]
    );

    let started = std::time::Instant::now();
    let silent = pipeline_runs_live_inner(&pool, &no_secrets(), &answering(), &slow, None)
        .await
        .unwrap();
    assert!(
        started.elapsed() < std::time::Duration::from_secs(20),
        "the run page waited {:?} on a silent source",
        started.elapsed()
    );
    assert_eq!(commits(silent), [(format!("{tc}:build:3"), None)]);
}

/// **How a run was started** (#961): the declared trigger in the shell's
/// words, who started it, whether that is the reader -- the account a
/// configured source signs in as -- and the commit it built, on both the
/// list's last runs and the pipeline's runs.
#[tokio::test]
async fn runs_carry_their_trigger_who_started_them_and_their_commit() {
    use knobas_core::payload::Trigger;
    let pool = pool().await;
    let tc = source(&pool).await;
    let me = format!("me-{tc}");
    sqlx::query("update knobas.source_config set config = jsonb_build_object('username', $2::text) where id = $1")
        .bind(&tc)
        .bind(&me)
        .execute(&pool)
        .await
        .unwrap();
    let tests = configuration(&pool, &tc, "Payout_IntegrationTests", "Integration Tests").await;
    let run = |id: i64, triggered: serde_json::Value| {
        serde_json::json!({
            "id": id, "number": id.to_string(), "buildTypeId": "Payout_IntegrationTests",
            "state": "finished", "status": "SUCCESS", "branchName": "main",
            "triggered": triggered,
            "revisions": { "revision": [{ "version": format!("a41f2c{id}") }] }
        })
    };
    let cases = [
        (1, serde_json::json!({ "type": "vcs" })),
        (2, serde_json::json!({ "type": "schedule" })),
        (
            3,
            serde_json::json!({ "type": "user", "user": { "username": me } }),
        ),
        (
            4,
            serde_json::json!({ "type": "user", "user": { "username": "jonas" } }),
        ),
        (5, serde_json::json!({ "type": "buildType" })),
        (
            6,
            serde_json::json!({ "type": "snapshotDependency", "user": { "username": me } }),
        ),
    ];
    for (id, triggered) in cases {
        let at = format!("2026-08-21T0{id}:00:00Z");
        item(
            &pool,
            &format!("{tc}:build:{id}"),
            &tc,
            "build",
            &at,
            run(id, triggered),
        )
        .await;
    }

    let runs = pipeline_runs_inner(
        &pool,
        &knobas_app::sources::Registry::for_process(),
        &tests,
        None,
    )
    .await
    .unwrap();
    /// How a run was started, by whom, whether that is the reader, its commit.
    type Started<'a> = (Option<Trigger>, Option<&'a str>, bool, Option<&'a str>);
    let seen: Vec<Started> = runs
        .iter()
        .rev()
        .map(|r| {
            (
                r.trigger,
                r.started_by.as_deref(),
                r.started_by_me,
                r.revision.as_deref(),
            )
        })
        .collect();
    assert_eq!(
        seen,
        [
            (Some(Trigger::Change), None, false, Some("a41f2c1")),
            (Some(Trigger::Schedule), None, false, Some("a41f2c2")),
            (
                Some(Trigger::Person),
                Some(me.as_str()),
                true,
                Some("a41f2c3")
            ),
            (Some(Trigger::Person), Some("jonas"), false, Some("a41f2c4")),
            (None, None, false, Some("a41f2c5")),
            (
                Some(Trigger::Chain),
                Some(me.as_str()),
                true,
                Some("a41f2c6")
            ),
        ]
    );

    let listed = pipelines_inner(&pool, &knobas_app::sources::Registry::for_process())
        .await
        .unwrap();
    let row = listed.iter().find(|p| p.entity_id == tests).unwrap();
    let recent: Vec<_> = row
        .recent
        .iter()
        .map(|r| (r.trigger, r.started_by_me, r.revision.clone()))
        .collect();
    let from_runs: Vec<_> = runs
        .iter()
        .map(|r| (r.trigger, r.started_by_me, r.revision.clone()))
        .collect();
    assert_eq!(
        recent, from_runs,
        "the list reads a run as the pipeline does"
    );
}
