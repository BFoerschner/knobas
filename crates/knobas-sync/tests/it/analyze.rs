//! knobas analyzes the mirror a sync grew (#778): a table that moved by more
//! than a fifth since its last analyze is analyzed again, one that moved less
//! is left alone, and requests during a running pass are one more pass.

use std::sync::Arc;

use knobas_source::{
    Capability, ConnectionInfo, Cursor, KindInfo, Sink, Source, SourceDescriptor, SourceError,
    SyncItem, WriteOp,
};
use knobas_sync::analyze::Analyzer;
use knobas_sync::scheduler::RunConnections;
use sqlx::PgPool;

/// A source of `n` tickets, `G-0` to `G-{n-1}`: a later sync of a larger `n`
/// grows the mirror by the difference and changes nothing else.
struct Growing(usize);

#[async_trait::async_trait]
impl Source for Growing {
    async fn view(
        &self,
        _entity: &str,
        _section: Option<&str>,
    ) -> Result<Option<knobas_source::view::SourceView>, SourceError> {
        Ok(None)
    }
    fn descriptor(&self) -> SourceDescriptor {
        SourceDescriptor {
            id: "grow".into(),
            adapter_kind: "grow".into(),
            name: "Growing".into(),
            capabilities: Vec::<Capability>::new(),
            adapter_version: "0.1.0".into(),
            auth_methods: Vec::new(),
            accepts_account: false,
            min_sync_interval_secs: 60,
            roles: Vec::new(),
            source_actions: Vec::new(),
            clone_from: Vec::new(),
            url_forms: Vec::new(),
            code_pages: None,
            write_ops: Vec::new(),
            entity_kinds: vec![KindInfo {
                id: "ticket".into(),
                label: "T".into(),
                plural: "T".into(),
                monogram: "GR".into(),
                full_sync_exhaustive: false,
                refreshes: true,
            }],
            config_schema: serde_json::json!({ "type": "object", "properties": {} }),
            payload_paths: Vec::new(),
        }
    }
    async fn test_connection(&self) -> Result<ConnectionInfo, SourceError> {
        Ok(ConnectionInfo::default())
    }
    async fn sync(
        &self,
        _c: Option<Cursor>,
        sink: &mut (dyn Sink + Send),
    ) -> Result<Cursor, SourceError> {
        for n in 0..self.0 {
            sink.item(SyncItem {
                entity: knobas_core::entity::EntityRef::new("grow", &format!("G-{n}")),
                kind: "ticket".into(),
                title: format!("ticket {n}"),
                body_text: String::new(),
                author: None,
                updated_at: None,
                payload: serde_json::json!({}),
                web_url: None,
                deleted: false,
            })
            .await?;
        }
        Ok("done".into())
    }
    async fn write(&self, _op: WriteOp) -> Result<knobas_source::WriteReceipt, SourceError> {
        Err(SourceError::protocol("read-only"))
    }
    async fn reachable_transitions(&self, entity: &str) -> Result<Vec<String>, SourceError> {
        Err(SourceError::protocol(format!("no workflow for {entity:?}")))
    }
}

struct Conns(knobas_db::embedded::Connector);

#[async_trait::async_trait]
impl RunConnections for Conns {
    async fn open(&self) -> Result<sqlx::PgConnection, sqlx::Error> {
        self.0.connect().await
    }
}

/// A database of its own with autovacuum off on every table, so the only
/// analyze that happens is the one under test.
async fn fixture(label: &str) -> (PgPool, Analyzer) {
    let connector = knobas_db::test_util::scratch_database(label).await;
    fixture_on(&connector).await
}

async fn fixture_on(connector: &knobas_db::embedded::Connector) -> (PgPool, Analyzer) {
    let pool = connector.pool(3).await.unwrap();
    let tables: Vec<String> = sqlx::query_scalar(
        "select format('%I.%I', n.nspname, c.relname) from pg_class c
           join pg_namespace n on n.oid = c.relnamespace
          where c.relkind = 'r' and n.nspname in ('knobas', 'sync')",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    for table in tables {
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "alter table {table} set (autovacuum_enabled = false)"
        )))
        .execute(&pool)
        .await
        .unwrap();
    }
    (pool, Analyzer::new(Arc::new(Conns(connector.clone()))))
}

/// How many times `table` has been analyzed by hand (`analyze`), which is
/// what a pass does.
async fn analyzed(pool: &PgPool, schema: &str, table: &str) -> i64 {
    sqlx::query_scalar(
        "select analyze_count from pg_stat_user_tables where schemaname = $1 and relname = $2",
    )
    .bind(schema)
    .bind(table)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn sync_then_pass(pool: &PgPool, analyzer: &Analyzer, items: usize) {
    knobas_sync::run_once(pool, &Growing(items), None)
        .await
        .unwrap();
    analyzer.request();
    analyzer.idle().await;
}

#[tokio::test]
async fn a_sync_that_grows_the_mirror_by_more_than_a_fifth_leaves_it_analyzed() {
    let (pool, analyzer) = fixture("analyze_grow").await;

    sync_then_pass(&pool, &analyzer, 1_000).await;
    assert_eq!(
        analyzed(&pool, "sync", "item").await,
        1,
        "a table never analyzed is analyzed after the first sync"
    );
    assert_eq!(analyzed(&pool, "knobas", "entity").await, 1);

    // 1,000 to 1,150: 15 %, under the fifth.
    sync_then_pass(&pool, &analyzer, 1_150).await;
    assert_eq!(
        analyzed(&pool, "sync", "item").await,
        1,
        "a sync that grew the table by 15 % leaves it alone"
    );
    assert_eq!(analyzed(&pool, "knobas", "entity").await, 1);

    // 1,000 (what the last analyze saw) to 1,250: 25 %.
    sync_then_pass(&pool, &analyzer, 1_250).await;
    assert_eq!(
        analyzed(&pool, "sync", "item").await,
        2,
        "a sync that grew the table by 25 % since its last analyze has it analyzed again"
    );
    assert_eq!(analyzed(&pool, "knobas", "entity").await, 2);
    assert_eq!(analyzer.passes(), 3);
}

#[tokio::test]
async fn requests_during_a_running_pass_are_one_more_pass() {
    let (pool, analyzer) = fixture("analyze_coalesce").await;
    knobas_sync::run_once(&pool, &Growing(500), None)
        .await
        .unwrap();

    // Hold a lock `analyze` waits for, so the first pass is still running
    // while the next requests arrive.
    let mut holder = pool.begin().await.unwrap();
    sqlx::query("lock table sync.item in share update exclusive mode")
        .execute(&mut *holder)
        .await
        .unwrap();
    analyzer.request();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    loop {
        let waiting: bool = sqlx::query_scalar(
            "select exists (select 1 from pg_stat_activity
                             where wait_event_type = 'Lock' and query ilike 'analyze%')",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        if waiting {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the first pass never reached its analyze"
        );
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    for _ in 0..5 {
        analyzer.request();
    }
    assert_eq!(analyzer.passes(), 0, "the first pass is still waiting");
    holder.rollback().await.unwrap();

    analyzer.idle().await;
    assert_eq!(
        analyzer.passes(),
        2,
        "five requests during the running pass are one more pass, not five"
    );
    assert_eq!(analyzed(&pool, "sync", "item").await, 1);
}

struct OneGrowing;
impl knobas_sync::scheduler::AdapterRegistry for OneGrowing {
    fn descriptors(&self) -> Vec<SourceDescriptor> {
        Vec::new()
    }
    fn build(
        &self,
        _instance: knobas_source::instance::SourceInstance,
    ) -> Result<Box<dyn Source>, SourceError> {
        Ok(Box::new(Growing(800)))
    }
}

struct Silent;
impl knobas_sync::scheduler::SyncEvents for Silent {
    fn sync_state(&self, _s: knobas_sync::SourceSyncStatus) {}
    fn source_health(&self, _h: knobas_sync::CredentialHealth) {}
    fn activity_new(&self, _r: knobas_core::activity::ActivityRow) {}
}

#[derive(Default)]
struct Ended(std::sync::Mutex<bool>);
impl knobas_sync::ProgressSink for Ended {
    fn report(&self, progress: knobas_sync::SyncProgress) {
        if matches!(
            progress.phase,
            knobas_sync::SyncPhase::Finished | knobas_sync::SyncPhase::Failed
        ) {
            *self.0.lock().unwrap() = true;
        }
    }
}

/// The wiring: a scheduled run that commits asks for the pass itself.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_scheduled_run_that_commits_has_the_tables_it_grew_analyzed() {
    let connector = knobas_db::test_util::scratch_database("analyze_scheduled").await;
    let (pool, _) = fixture_on(&connector).await;
    // An hour of backoff, so the ticker leaves the source to the trigger.
    sqlx::query(
        "insert into knobas.source_config
                (id, kind, display_name, base_url, auth_kind, backoff_until)
         values ('grow', 'grow', 'Growing', 'http://localhost', 'none', now() + interval '1 hour')",
    )
    .execute(&pool)
    .await
    .unwrap();
    let scheduler =
        knobas_sync::scheduler::Scheduler::start(knobas_sync::scheduler::SchedulerDeps {
            pool: connector.pool(3).await.unwrap(),
            connections: Arc::new(Conns(connector.clone())),
            registry: Arc::new(OneGrowing),
            secrets: Arc::new(knobas_secrets::MemoryStore::new()),
            events: Arc::new(Silent),
            timing: knobas_sync::scheduler::SchedulerTiming::default(),
            quit: knobas_sync::scheduler::Quit::default(),
        })
        .await
        .unwrap();

    let ended = Arc::new(Ended::default());
    scheduler
        .trigger(
            "grow",
            knobas_sync::SyncTrigger::Manual,
            Some(Arc::clone(&ended) as Arc<dyn knobas_sync::ProgressSink>),
        )
        .await
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while !*ended.0.lock().unwrap() {
        assert!(std::time::Instant::now() < deadline, "the run never ended");
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    // The ending is sent after the request, so the pass is running or done.
    scheduler.analyzer().idle().await;
    assert_eq!(scheduler.analyzer().passes(), 1);
    assert_eq!(analyzed(&pool, "sync", "item").await, 1);
    scheduler.shutdown().await;
}
