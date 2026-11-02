//! A pipeline's latest run log, from the source into the launcher's answer
//! (#2431, spec #2419): `knobas_sync::pipeline_logs::refresh` stores the
//! newest finished run's tail after a sync, and the search IPC answers a word
//! only that log holds with the pipeline, its excerpt naming the line.

use std::sync::{Arc, Mutex};

use knobas_app::commands::search::search_inner;
use knobas_app::sources::Registry;
use knobas_search::{SearchFilters, SearchQuery, SearchResponse};
use knobas_source::run::{LogLine, RunLog};
use knobas_source::{Source, SourceError};

async fn pool() -> sqlx::PgPool {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    pool
}

/// A word no other row holds, in letters only, so it lexes as one word.
fn token() -> String {
    let hex = uuid::Uuid::new_v4().simple().to_string();
    hex[..12]
        .chars()
        .map(|c| match c.to_digit(16) {
            Some(n) => char::from(b'g' + u8::try_from(n).unwrap()),
            None => c,
        })
        .collect()
}

async fn item(
    pool: &sqlx::PgPool,
    source: &str,
    kind: &str,
    key: &str,
    title: &str,
    payload: serde_json::Value,
    hours_ago: i64,
) -> String {
    let id = format!("{source}:{key}");
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1,$2,$3)")
        .bind(&id)
        .bind(kind)
        .bind(title)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload,
                                item_updated_at, synced_at)
         values ($1,$2,$3,$4,'',$5, now() - make_interval(hours => $6::int), now())",
    )
    .bind(&id)
    .bind(source)
    .bind(kind)
    .bind(title)
    .bind(payload)
    .bind(i32::try_from(hours_ago).unwrap())
    .execute(pool)
    .await
    .unwrap();
    id
}

/// A run of `config`, finished unless `running`.
fn run(config: &str, number: u32, running: bool) -> serde_json::Value {
    if running {
        serde_json::json!({
            "buildTypeId": config, "number": number.to_string(), "state": "running",
            "buildType": { "id": config, "name": "Deploy", "projectId": "Payouts" },
        })
    } else {
        serde_json::json!({
            "buildTypeId": config, "number": number.to_string(), "state": "finished",
            "status": "FAILURE", "finishDate": "20261102T100000+0000",
            "buildType": { "id": config, "name": "Deploy", "projectId": "Payouts" },
        })
    }
}

/// A TeamCity that answers every run's log with `lines[run]`, starting at
/// line 101, and counts the reads. With a `gate`, a read answers only once
/// the gate has a permit for it.
#[derive(Clone)]
struct Logs {
    id: String,
    lines: Vec<(String, Vec<String>)>,
    reads: Arc<Mutex<Vec<String>>>,
    gate: Option<Arc<tokio::sync::Semaphore>>,
}

#[async_trait::async_trait]
impl Source for Logs {
    fn descriptor(&self) -> knobas_source::SourceDescriptor {
        use knobas_sync::scheduler::AdapterRegistry as _;
        let mut d = Registry::for_process()
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
        assert_eq!(from, None, "the tail is asked for");
        assert_eq!(count, knobas_sync::pipeline_logs::TAIL_LINES);
        self.reads.lock().unwrap().push(entity.to_owned());
        if let Some(gate) = &self.gate {
            gate.acquire().await.unwrap().forget();
        }
        let (_, lines) = self
            .lines
            .iter()
            .find(|(run, _)| run == entity)
            .ok_or_else(|| SourceError::protocol(format!("{entity} is not a build here")))?;
        Ok(Some(RunLog {
            lines: lines
                .iter()
                .enumerate()
                .map(|(i, text)| LogLine {
                    number: 101 + u64::try_from(i).unwrap(),
                    time: None,
                    text: text.clone(),
                    tone: None,
                    step: None,
                })
                .collect(),
            steps: vec![],
            total: 200,
        }))
    }
}

async fn everything(pool: &sqlx::PgPool, raw: &str, titles: bool) -> SearchResponse {
    search_inner(
        pool,
        &Registry::for_process(),
        SearchQuery {
            titles,
            raw: raw.to_owned(),
            limit: 50,
            filters: SearchFilters::default(),
        },
    )
    .await
    .unwrap()
}

/// `(kind, id, [(place, text)])` of every hit.
/// A hit as the tests read it: kind, entity id, `(place, text)` per excerpt.
type Read = (String, String, Vec<(Option<String>, String)>);

fn hits(response: &SearchResponse) -> Vec<Read> {
    response
        .groups
        .iter()
        .flat_map(|g| &g.hits)
        .map(|hit| {
            (
                hit.row.kind.clone(),
                hit.row.entity_id.clone(),
                hit.excerpts
                    .iter()
                    .map(|e| {
                        (
                            e.place.clone(),
                            e.segments.iter().map(|s| s.text.as_str()).collect(),
                        )
                    })
                    .collect(),
            )
        })
        .collect()
}

#[tokio::test]
async fn the_newest_finished_runs_log_is_stored_once_and_found_by_everything() {
    let pool = pool().await;
    let t = token();
    let source = format!("tc{}", &t[..8]);
    crate::support::source_config(
        &pool,
        &source,
        "teamcity",
        "TeamCity",
        &serde_json::json!({}),
    )
    .await;
    // The declarations, stored by the search as every launcher opening does.
    everything(&pool, "warmup", false).await;

    let config = format!("{t}_Deploy");
    let pipeline = item(
        &pool,
        &source,
        "build_config",
        &config,
        "Deploy payouts",
        serde_json::json!({ "id": config, "name": "Deploy payouts", "projectId": "Payouts",
                            "projectPath": [{ "id": "Payouts", "name": "Payouts" }] }),
        5,
    )
    .await;
    let older = item(
        &pool,
        &source,
        "build",
        "7",
        "Deploy #7",
        run(&config, 7, false),
        3,
    )
    .await;
    let newer = item(
        &pool,
        &source,
        "build",
        "8",
        "Deploy #8",
        run(&config, 8, false),
        2,
    )
    .await;
    let reads = Arc::new(Mutex::new(Vec::new()));
    let logs = Logs {
        id: source.clone(),
        lines: vec![
            (older.clone(), vec![format!("old{t} words")]),
            (
                newer.clone(),
                vec!["compiling".to_owned(), format!("refused{t} by the bank")],
            ),
        ],
        reads: Arc::clone(&reads),
        gate: None,
    };

    let stored = knobas_sync::pipeline_logs::refresh(&pool, &source, &logs)
        .await
        .unwrap();
    assert_eq!(stored, 1);
    assert_eq!(
        *reads.lock().unwrap(),
        vec![newer.clone()],
        "only the newest run's log"
    );

    assert_eq!(
        hits(&everything(&pool, &format!("refused{t}"), false).await),
        vec![(
            "pipeline".to_owned(),
            pipeline.clone(),
            vec![(
                Some("line 102".to_owned()),
                format!("refused{t} by the bank")
            )]
        )]
    );
    assert!(hits(&everything(&pool, &format!("refused{t}"), true).await).is_empty());
    assert!(hits(&everything(&pool, &format!("old{t}"), false).await).is_empty());

    // Nothing newer: the next sync reads no log.
    assert_eq!(
        knobas_sync::pipeline_logs::refresh(&pool, &source, &logs)
            .await
            .unwrap(),
        0
    );
    assert_eq!(reads.lock().unwrap().len(), 1);

    // A run still going is not read; the stored log stays the last finished one's.
    item(
        &pool,
        &source,
        "build",
        "9",
        "Deploy #9",
        run(&config, 9, true),
        1,
    )
    .await;
    assert_eq!(
        knobas_sync::pipeline_logs::refresh(&pool, &source, &logs)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        hits(&everything(&pool, &format!("refused{t}"), false).await).len(),
        1
    );
}

#[tokio::test]
async fn a_newer_finished_run_replaces_the_stored_log() {
    let pool = pool().await;
    let t = token();
    let source = format!("tc{}", &t[..8]);
    crate::support::source_config(
        &pool,
        &source,
        "teamcity",
        "TeamCity",
        &serde_json::json!({}),
    )
    .await;
    everything(&pool, "warmup", false).await;
    let config = format!("{t}_Build");
    let pipeline = item(
        &pool,
        &source,
        "build_config",
        &config,
        "Build",
        serde_json::json!({ "id": config, "name": "Build" }),
        5,
    )
    .await;
    let first = item(
        &pool,
        &source,
        "build",
        "1",
        "Build #1",
        run(&config, 1, false),
        3,
    )
    .await;
    let mut lines = vec![(first.clone(), vec![format!("first{t}")])];
    let reads = Arc::new(Mutex::new(Vec::new()));
    let logs = |lines: Vec<(String, Vec<String>)>| Logs {
        id: source.clone(),
        lines,
        reads: Arc::clone(&reads),
        gate: None,
    };
    knobas_sync::pipeline_logs::refresh(&pool, &source, &logs(lines.clone()))
        .await
        .unwrap();

    let second = item(
        &pool,
        &source,
        "build",
        "2",
        "Build #2",
        run(&config, 2, false),
        1,
    )
    .await;
    lines.push((second, vec![format!("second{t}")]));
    assert_eq!(
        knobas_sync::pipeline_logs::refresh(&pool, &source, &logs(lines))
            .await
            .unwrap(),
        1
    );
    assert!(hits(&everything(&pool, &format!("first{t}"), false).await).is_empty());
    assert_eq!(
        hits(&everything(&pool, &format!("second{t}"), false).await)
            .into_iter()
            .map(|(_, id, _)| id)
            .collect::<Vec<_>>(),
        vec![pipeline]
    );
}

/// A registry that builds [`Logs`] for every source.
struct Builds(Logs);

impl knobas_sync::scheduler::AdapterRegistry for Builds {
    fn descriptors(&self) -> Vec<knobas_source::SourceDescriptor> {
        Registry::for_process().descriptors()
    }
    fn build(
        &self,
        _instance: knobas_source::instance::SourceInstance,
    ) -> Result<Box<dyn Source>, SourceError> {
        Ok(Box::new(self.0.clone()))
    }
}

struct Quiet;

impl knobas_sync::scheduler::SyncEvents for Quiet {
    fn sync_state(&self, _status: knobas_sync::SourceSyncStatus) {}
    fn source_health(&self, _health: knobas_sync::CredentialHealth) {}
    fn activity_new(&self, _row: knobas_core::activity::ActivityRow) {}
}

struct Connections(knobas_db::embedded::Connector);

#[async_trait::async_trait]
impl knobas_sync::scheduler::RunConnections for Connections {
    async fn open(&self) -> Result<sqlx::PgConnection, sqlx::Error> {
        self.0.connect().await
    }
}

/// Resolves when the run it watches ends.
struct Ending(Mutex<Option<tokio::sync::oneshot::Sender<()>>>);

impl knobas_sync::progress::ProgressSink for Ending {
    fn report(&self, progress: knobas_sync::progress::SyncProgress) {
        use knobas_sync::progress::SyncPhase;
        if matches!(progress.phase, SyncPhase::Finished | SyncPhase::Failed)
            && let Some(done) = self.0.lock().unwrap().take()
        {
            let _ = done.send(());
        }
    }
}

/// A sync run ends before the logs that follow it are read: a log that is
/// slow to come holds neither the run's verdict nor the source, and is stored
/// once it comes.
#[tokio::test]
async fn a_sync_run_ends_before_its_pipelines_logs_are_read() {
    let connector = knobas_db::test_util::scratch_database("pipeline_logs").await;
    let pool = connector.pool(4).await.unwrap();
    let t = token();
    let source = format!("tc{}", &t[..8]);
    crate::support::source_config(
        &pool,
        &source,
        "teamcity",
        "TeamCity",
        &serde_json::json!({}),
    )
    .await;
    let secrets = Arc::new(knobas_secrets::MemoryStore::new());
    knobas_secrets::SecretStore::put(
        secrets.as_ref(),
        &knobas_secrets::KeychainAccount::source(&source),
        &knobas_secrets::Secret::just(knobas_source::AuthMethod::Pat, "token"),
    )
    .unwrap();
    everything(&pool, "warmup", false).await;
    let config = format!("{t}_Deploy");
    let pipeline = item(
        &pool,
        &source,
        "build_config",
        &config,
        "Deploy",
        serde_json::json!({ "id": config, "name": "Deploy" }),
        5,
    )
    .await;
    let run = item(
        &pool,
        &source,
        "build",
        "1",
        "Deploy #1",
        run(&config, 1, false),
        1,
    )
    .await;
    let gate = Arc::new(tokio::sync::Semaphore::new(0));
    let reads = Arc::new(Mutex::new(Vec::new()));
    let scheduler =
        knobas_sync::scheduler::Scheduler::start(knobas_sync::scheduler::SchedulerDeps {
            pool: pool.clone(),
            connections: Arc::new(Connections(connector)),
            registry: Arc::new(Builds(Logs {
                id: source.clone(),
                lines: vec![(run.clone(), vec![format!("slow{t} to come")])],
                reads: Arc::clone(&reads),
                gate: Some(Arc::clone(&gate)),
            })),
            secrets,
            events: Arc::new(Quiet),
            timing: knobas_sync::scheduler::SchedulerTiming::default(),
            quit: knobas_sync::scheduler::Quit::default(),
        })
        .await
        .unwrap();

    let (done, ended) = tokio::sync::oneshot::channel();
    scheduler
        .trigger(
            &source,
            knobas_sync::SyncTrigger::Manual,
            Some(Arc::new(Ending(Mutex::new(Some(done))))),
        )
        .await
        .unwrap();
    // Well inside `pipeline_logs::BUDGET`, which a run that waited on the
    // held read would take before it ended.
    tokio::time::timeout(std::time::Duration::from_secs(8), ended)
        .await
        .expect("the run ends while its pipeline's log is still coming")
        .unwrap();

    gate.add_permits(1);
    let stored = async {
        loop {
            let run_id: Option<String> =
                sqlx::query_scalar("select run_id from sync.pipeline_log where pipeline_id = $1")
                    .bind(&pipeline)
                    .fetch_optional(&pool)
                    .await
                    .unwrap();
            if let Some(run_id) = run_id {
                return run_id;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    };
    assert_eq!(
        tokio::time::timeout(std::time::Duration::from_secs(10), stored)
            .await
            .expect("the log is stored once it comes"),
        run
    );
    assert_eq!(*reads.lock().unwrap(), vec![run]);
    scheduler.shutdown().await;
}
