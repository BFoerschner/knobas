//! The run page's reads and its download (#854, #855, #856) through their
//! commands, over the embedded PostgreSQL with a fake TeamCity: what is under
//! test is the app's half -- the routing by the run's namespace, the caps,
//! the refusals that reach no source, and the file a download leaves. The
//! adapter's half is `knobas-source-teamcity`'s (`tests/it/run_reads.rs`, and
//! on the seeded server `live_teamcity_seeded.rs`).

use knobas_app::IpcErrorCode;
use knobas_app::commands::entity::{
    artifact_preview_inner, download_run_file_inner, run_artifacts_inner, run_log_inner,
    run_tests_inner, test_history_inner,
};
use knobas_source::run::{
    ArtifactEntry, ArtifactPreview, DownloadSink, DownloadTarget, LOG_LINES_MAX, RunLog, RunTests,
    TestOutcome, TestStatus,
};
use knobas_source::{Source, SourceError};

async fn pool() -> sqlx::PgPool {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    pool
}

/// A TeamCity source nothing else in this database uses.
async fn source(pool: &sqlx::PgPool) -> String {
    let id = format!("tc{}", &uuid::Uuid::new_v4().simple().to_string()[..10]);
    crate::support::source_config(pool, &id, "teamcity", "TeamCity", &serde_json::json!({})).await;
    id
}

fn no_secrets() -> std::sync::Arc<dyn knobas_secrets::SecretStore> {
    std::sync::Arc::new(knobas_secrets::MemoryStore::new())
}

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
    ) -> Result<Box<dyn Source>, SourceError> {
        Ok(Box::new(Answers { id: instance.id }))
    }
}

/// Answers build 7 of its own namespace, and refuses anything else as the
/// real adapter does.
struct Answers {
    id: String,
}

impl Answers {
    fn check(&self, entity: &str) -> Result<(), SourceError> {
        if entity == format!("{}:build:7", self.id) {
            Ok(())
        } else {
            Err(SourceError::protocol(format!(
                "{entity} is not a build here"
            )))
        }
    }
}

#[async_trait::async_trait]
impl Source for Answers {
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
    async fn test_connection(&self) -> Result<knobas_source::ConnectionInfo, SourceError> {
        Ok(knobas_source::ConnectionInfo::default())
    }
    async fn sync(
        &self,
        cursor: Option<knobas_source::Cursor>,
        _sink: &mut (dyn knobas_source::Sink + Send),
    ) -> Result<knobas_source::Cursor, SourceError> {
        Ok(cursor.unwrap_or_default())
    }
    async fn write(
        &self,
        _op: knobas_source::WriteOp,
    ) -> Result<knobas_source::WriteReceipt, SourceError> {
        Err(SourceError::protocol("read-only"))
    }
    async fn reachable_transitions(&self, _entity: &str) -> Result<Vec<String>, SourceError> {
        Err(SourceError::protocol("no workflow"))
    }
    async fn view(
        &self,
        _entity: &str,
        _section: Option<&str>,
    ) -> Result<Option<knobas_source::view::SourceView>, SourceError> {
        Ok(None)
    }
    async fn run_log(
        &self,
        entity: &str,
        from: Option<u64>,
        count: u32,
    ) -> Result<Option<RunLog>, SourceError> {
        self.check(entity)?;
        // What was asked, echoed back, so the test reads the cap the app sent.
        Ok(Some(RunLog {
            lines: vec![],
            steps: vec![],
            total: from.unwrap_or(0) * 1_000_000 + u64::from(count),
        }))
    }
    async fn run_tests(&self, entity: &str) -> Result<Option<RunTests>, SourceError> {
        self.check(entity)?;
        Ok(Some(RunTests {
            tests: vec![],
            truncated: false,
        }))
    }
    async fn test_history(
        &self,
        entity: &str,
        test: &str,
    ) -> Result<Option<Vec<TestOutcome>>, SourceError> {
        self.check(entity)?;
        Ok(Some(vec![TestOutcome {
            run: entity.to_owned(),
            number: Some(test.to_owned()),
            branch: None,
            status: TestStatus::Failed,
        }]))
    }
    async fn run_artifacts(
        &self,
        entity: &str,
        path: &str,
    ) -> Result<Option<Vec<ArtifactEntry>>, SourceError> {
        self.check(entity)?;
        Ok(Some(vec![ArtifactEntry {
            path: format!("{path}/x"),
            name: "x".to_owned(),
            size: Some(1),
            folder: false,
            children: None,
        }]))
    }
    async fn artifact_preview(
        &self,
        entity: &str,
        path: &str,
    ) -> Result<Option<ArtifactPreview>, SourceError> {
        self.check(entity)?;
        Ok(Some(ArtifactPreview {
            text: Some(path.to_owned()),
            truncated: false,
        }))
    }
    async fn download(
        &self,
        entity: &str,
        target: &DownloadTarget,
        sink: &mut dyn DownloadSink,
    ) -> Result<bool, SourceError> {
        self.check(entity)?;
        match target {
            // A source that suggests a path is told no by the sink.
            DownloadTarget::Log => {
                sink.start("../../escape/Payout-7.log").await?;
                sink.write(b"line 1\n").await?;
                sink.write(b"line 2\n").await?;
            }
            // One that breaks off half-way leaves nothing behind.
            DownloadTarget::All => {
                sink.start("all.zip").await?;
                sink.write(b"PK").await?;
                return Err(SourceError::Unreachable("the zip broke off".to_owned()));
            }
            DownloadTarget::File { .. } | DownloadTarget::Folder { .. } => {
                sink.start("junit.xml").await?;
                sink.write(b"<testsuites/>").await?;
            }
        }
        Ok(true)
    }
}

#[tokio::test]
async fn each_run_read_reaches_the_runs_own_source_with_the_cap_applied() {
    let pool = pool().await;
    let tc = source(&pool).await;
    let run = format!("{tc}:build:7");
    let s = no_secrets();

    let whole = run_log_inner(
        &pool,
        &s,
        &answering(),
        &knobas_app::sources::live::LiveSources::default(),
        &run,
        None,
        None,
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(whole.total, u64::from(LOG_LINES_MAX), "no count is the cap");
    let over = run_log_inner(
        &pool,
        &s,
        &answering(),
        &knobas_app::sources::live::LiveSources::default(),
        &run,
        Some(3),
        Some(LOG_LINES_MAX * 5),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(
        over.total,
        3_000_000 + u64::from(LOG_LINES_MAX),
        "a count over the cap is the cap"
    );

    assert!(
        run_tests_inner(
            &pool,
            &s,
            &answering(),
            &knobas_app::sources::live::LiveSources::default(),
            &run
        )
        .await
        .unwrap()
        .is_some()
    );
    let history = test_history_inner(
        &pool,
        &s,
        &answering(),
        &knobas_app::sources::live::LiveSources::default(),
        &run,
        "-12",
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(history[0].number.as_deref(), Some("-12"));
    let folder = run_artifacts_inner(
        &pool,
        &s,
        &answering(),
        &knobas_app::sources::live::LiveSources::default(),
        &run,
        Some("logs"),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(folder[0].path, "logs/x");
    let root = run_artifacts_inner(
        &pool,
        &s,
        &answering(),
        &knobas_app::sources::live::LiveSources::default(),
        &run,
        None,
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(root[0].path, "/x", "no path is the root, \"\"");
    let preview = artifact_preview_inner(
        &pool,
        &s,
        &answering(),
        &knobas_app::sources::live::LiveSources::default(),
        &run,
        "logs/a.log",
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(preview.text.as_deref(), Some("logs/a.log"));
}

#[tokio::test]
async fn a_bad_address_or_path_is_refused_and_a_foreign_run_is_the_sources_refusal() {
    let pool = pool().await;
    let tc = source(&pool).await;
    let run = format!("{tc}:build:7");
    let s = no_secrets();
    let code = |e: knobas_app::IpcError| e.code;

    assert_eq!(
        code(
            run_log_inner(
                &pool,
                &s,
                &answering(),
                &knobas_app::sources::live::LiveSources::default(),
                "no id",
                None,
                None
            )
            .await
            .unwrap_err()
        ),
        IpcErrorCode::Invalid
    );
    assert_eq!(
        code(
            run_tests_inner(
                &pool,
                &s,
                &answering(),
                &knobas_app::sources::live::LiveSources::default(),
                "nobody:build:7"
            )
            .await
            .unwrap_err()
        ),
        IpcErrorCode::NotFound
    );
    assert_eq!(
        code(
            run_artifacts_inner(
                &pool,
                &s,
                &answering(),
                &knobas_app::sources::live::LiveSources::default(),
                &run,
                Some("../etc")
            )
            .await
            .unwrap_err()
        ),
        IpcErrorCode::Invalid
    );
    assert_eq!(
        code(
            artifact_preview_inner(
                &pool,
                &s,
                &answering(),
                &knobas_app::sources::live::LiveSources::default(),
                &run,
                "/etc/passwd"
            )
            .await
            .unwrap_err()
        ),
        IpcErrorCode::Invalid
    );
    let dir = tempfile::tempdir().unwrap();
    let escape = DownloadTarget::Folder {
        path: "a/../../b".to_owned(),
    };
    assert_eq!(
        code(
            download_run_file_inner(
                &pool,
                &s,
                &answering(),
                &knobas_app::sources::live::LiveSources::default(),
                &run,
                &escape,
                dir.path()
            )
            .await
            .unwrap_err()
        ),
        IpcErrorCode::Invalid
    );
    // The source's own refusal of an id that is not one of its runs.
    let other = format!("{tc}:buildType:Payout_Build");
    assert_eq!(
        code(
            run_tests_inner(
                &pool,
                &s,
                &answering(),
                &knobas_app::sources::live::LiveSources::default(),
                &other
            )
            .await
            .unwrap_err()
        ),
        IpcErrorCode::Internal
    );
}

#[tokio::test]
async fn a_download_is_a_new_file_in_the_folder_named_by_its_last_segment() {
    let pool = pool().await;
    let tc = source(&pool).await;
    let run = format!("{tc}:build:7");
    let s = no_secrets();
    let dir = tempfile::tempdir().unwrap();
    let down = async |target: DownloadTarget| {
        download_run_file_inner(
            &pool,
            &s,
            &answering(),
            &knobas_app::sources::live::LiveSources::default(),
            &run,
            &target,
            dir.path(),
        )
        .await
    };

    let log = down(DownloadTarget::Log).await.unwrap().unwrap();
    assert_eq!(std::path::Path::new(&log), dir.path().join("Payout-7.log"));
    assert_eq!(std::fs::read(&log).unwrap(), b"line 1\nline 2\n");
    let file = DownloadTarget::File {
        path: "test-report/junit.xml".to_owned(),
    };
    let first = down(file.clone()).await.unwrap().unwrap();
    let second = down(file).await.unwrap().unwrap();
    assert_eq!(std::path::Path::new(&first), dir.path().join("junit.xml"));
    assert_eq!(
        std::path::Path::new(&second),
        dir.path().join("junit (1).xml"),
        "a taken name is never overwritten"
    );

    let failed = down(DownloadTarget::All).await.unwrap_err();
    assert_eq!(failed.code, IpcErrorCode::Unreachable);
    assert!(
        !dir.path().join("all.zip").exists(),
        "a download that broke off leaves no file"
    );
    let mut names: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    names.sort();
    assert_eq!(names, ["Payout-7.log", "junit (1).xml", "junit.xml"]);
}
