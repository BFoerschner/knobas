//! The files pass runs after a sync in the scheduler the app starts (the
//! everything-search decisions, 2026-11-20, D7): `sources::start_scheduler`
//! over the app's events with the files index handed in, a run of the demo
//! source triggered, and the repositories' files read into the index.
//!
//! The ux26 boot check on Björn's PC found the pass never ran in the app:
//! `start_scheduler` wraps the events in `FollowLandings`, which did not pass
//! `files()` on, so the scheduler saw the trait's `None` and skipped the pass
//! on every run while the stored-trees pass beside it ran. The builder's own
//! fill called the pass directly.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use knobas_app::sources::files_on_sync::FilesOnSync;
use knobas_app::sources::{Registry, demo};
use knobas_files::FileIndex;
use knobas_secrets::MemoryStore;
use knobas_sync::code_files::{Files, FilesProgress};
use knobas_sync::scheduler::{
    AdapterRegistry, RunConnections, SchedulerDeps, SchedulerTiming, SyncEvents,
};

struct Connections(knobas_db::embedded::Connector);

#[async_trait]
impl RunConnections for Connections {
    async fn open(&self) -> Result<sqlx::PgConnection, sqlx::Error> {
        self.0.connect().await
    }
}

struct Quiet;

impl SyncEvents for Quiet {
    fn sync_state(&self, _status: knobas_sync::SourceSyncStatus) {}
    fn source_health(&self, _health: knobas_sync::CredentialHealth) {}
    fn activity_new(&self, _row: knobas_core::activity::ActivityRow) {}
}

#[tokio::test]
async fn a_sync_in_the_apps_scheduler_reads_the_repositories_files() {
    let connector = knobas_db::test_util::scratch_database("files-on-sync").await;
    let pool = connector.pool(4).await.expect("a pool");
    knobas_db::migrate::run(&pool).await.unwrap();
    // The demo source registered and synced, and the adapters' parent
    // declarations stored, as bring-up stores them before the first sync.
    demo::demo_load_inner(&pool).await.unwrap();
    knobas_app::commands::search::declare(&pool, &Registry::for_process())
        .await
        .unwrap();

    let dir = tempfile::tempdir().unwrap();
    let index = FileIndex::open(&dir.path().join("files-1")).unwrap();
    let told = Arc::new(AtomicUsize::new(0));
    let heard = Arc::clone(&told);
    let registry: Arc<dyn AdapterRegistry> = Arc::new(Registry::for_process());
    let scheduler = knobas_app::sources::start_scheduler(
        SchedulerDeps {
            pool: pool.clone(),
            connections: Arc::new(Connections(connector)),
            registry,
            secrets: Arc::new(MemoryStore::new()),
            // Outermost, as `sources::bring_up` builds them.
            events: Arc::new(FilesOnSync::new(
                Quiet,
                Some(Files {
                    index: index.clone(),
                    clones: None,
                }),
                move |_: &FilesProgress| {
                    heard.fetch_add(1, Ordering::SeqCst);
                },
            )),
            timing: SchedulerTiming {
                startup_delay: Duration::from_secs(3600),
                tick: Duration::from_secs(3600),
                shutdown_grace: knobas_sync::scheduler::SHUTDOWN_GRACE,
            },
            quit: knobas_sync::scheduler::Quit::default(),
        },
        knobas_sync::background::Background::new(),
    )
    .await
    .expect("a scheduler");

    scheduler
        .trigger("mock", knobas_sync::SyncTrigger::Manual, None)
        .await
        .expect("a run of the demo source");
    let read = tokio::time::timeout(Duration::from_secs(60), async {
        loop {
            let read: i64 = sqlx::query_scalar(
                "select count(*) from sync.code_repo where commit_sha is not null",
            )
            .fetch_one(&pool)
            .await
            .unwrap();
            if read == 3 {
                return read;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("the files pass read the demo's three repositories with files after the run");
    assert_eq!(read, 3);
    assert!(
        told.load(Ordering::SeqCst) > 0,
        "and told the window how far it got (`code:files`)"
    );
    let files: i64 = sqlx::query_scalar("select count(*) from sync.code_file")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(files > 0, "the files are recorded");
    scheduler.shutdown().await;
}
