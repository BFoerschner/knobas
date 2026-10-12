//! Deleting a source while a run of it has already flushed a batch (#627, the
//! deputy's second ruling).
//!
//! Migration `0028` puts a trigger on `sync.item` that takes the advisory key
//! `(627, 0)` shared, from a run's first flush to its commit. A first version
//! also put a trigger on `knobas.source_config` that took the same key
//! exclusive: `config::delete` row-locked the config row and then waited for
//! the key, and the run's closing cursor update waited for that row --
//! a deadlock. `scheduler_loop.rs`'s
//! `a_purge_survives_the_run_that_was_in_flight_when_the_source_was_deleted`
//! deletes before the run's first flush, which is why the gate stayed green
//! over it.
//!
//! So this drives the real run (`run_once`, a fake `Source` under ADR-0013's
//! trait-level allowance, a real database) past a flush, pauses it there, and
//! deletes. `delete_source` keeps #127's "same non-blocking timing": the delete
//! returns while the run is still paused, and the run then finishes.

use std::sync::Mutex;
use std::time::Duration;

use knobas_core::entity::EntityRef;
use knobas_source::{
    Cursor, KindInfo, Sink, Source, SourceDescriptor, SourceError, SyncItem, WriteOp,
};
use sqlx::PgPool;
use tokio::sync::oneshot;
use uuid::Uuid;

/// The two cases share one database and each asserts that *someone* holds
/// the key; one at a time keeps "someone" this test's run.
static SERIAL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// An adapter that emits more than one batch's worth of new items, says so,
/// and waits to be let go before it returns.
struct PausingSource {
    id: String,
    reached: Mutex<Option<oneshot::Sender<()>>>,
    release: tokio::sync::Mutex<Option<oneshot::Receiver<()>>>,
}

#[async_trait::async_trait]
impl Source for PausingSource {
    fn descriptor(&self) -> SourceDescriptor {
        SourceDescriptor {
            id: self.id.clone(),
            adapter_kind: "fake".to_owned(),
            name: "Fake".to_owned(),
            capabilities: Vec::new(),
            adapter_version: "0.1.0".to_owned(),
            auth_methods: Vec::new(),
            accepts_account: false,
            write_ops: Vec::new(),
            entity_kinds: vec![KindInfo {
                id: "ticket".to_owned(),
                label: "Ticket".to_owned(),
                plural: "Tickets".to_owned(),
                monogram: "TK".to_owned(),
                full_sync_exhaustive: false,
            }],
            config_schema: serde_json::json!({ "type": "object", "properties": {} }),
            payload_paths: Vec::new(),
        }
    }

    async fn test_connection(&self) -> Result<knobas_source::ConnectionInfo, SourceError> {
        Ok(knobas_source::ConnectionInfo::default())
    }

    async fn sync(
        &self,
        _cursor: Option<Cursor>,
        sink: &mut (dyn Sink + Send),
    ) -> Result<Cursor, SourceError> {
        // One more than a batch, so the first `BATCH` have been flushed --
        // through `sync.item`'s trigger -- before the pause.
        for n in 0..=knobas_sync::BATCH {
            sink.item(SyncItem {
                entity: EntityRef::new(&self.id, &format!("NEW-{n}")),
                kind: "ticket".to_owned(),
                title: format!("new {n}"),
                body_text: String::new(),
                author: None,
                updated_at: None,
                payload: serde_json::json!({ "n": n }),
                web_url: None,
                deleted: false,
            })
            .await?;
        }
        if let Some(reached) = self.reached.lock().unwrap().take() {
            let _ = reached.send(());
        }
        if let Some(release) = self.release.lock().await.take() {
            let _ = release.await;
        }
        Ok("paused".to_owned())
    }

    async fn write(&self, _op: WriteOp) -> Result<knobas_source::WriteReceipt, SourceError> {
        Err(SourceError::protocol("read-only".to_owned()))
    }

    async fn reachable_transitions(&self, entity: &str) -> Result<Vec<String>, SourceError> {
        Err(SourceError::protocol(format!(
            "this fake has no workflow, so there are no reachable transitions for {entity:?}"
        )))
    }
}

/// Whether anybody holds `0028`'s `(627, 0)` key: `pg_advisory_xact_lock(int4,
/// int4)` shows as `classid` = the first key, `objid` = the second,
/// `objsubid` = 2.
async fn the_key_is_held(pool: &PgPool) -> bool {
    sqlx::query_scalar(
        "select exists (select 1 from pg_locks
                         where locktype = 'advisory' and classid = 627 and objid = 0
                           and objsubid = 2 and granted)",
    )
    .fetch_one(pool)
    .await
    .unwrap()
}

/// Configure a fresh source, start a run of it, let it flush and pause, then
/// delete the source; return once the run has ended.
async fn delete_while_paused(purge_items: bool) {
    let _serial = SERIAL.lock().await;
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    let id = format!("fake-{}", Uuid::new_v4());
    sqlx::query(
        "insert into knobas.source_config (id, kind, display_name, base_url, auth_kind)
         values ($1, 'fake', 'Fake', 'http://localhost', 'none')",
    )
    .bind(&id)
    .execute(&pool)
    .await
    .unwrap();

    let (reached_tx, reached_rx) = oneshot::channel();
    let (release_tx, release_rx) = oneshot::channel();
    let source = PausingSource {
        id: id.clone(),
        reached: Mutex::new(Some(reached_tx)),
        release: tokio::sync::Mutex::new(Some(release_rx)),
    };
    let run_pool = pool.clone();
    let run = tokio::spawn(async move { knobas_sync::run_once(&run_pool, &source, None).await });

    tokio::time::timeout(Duration::from_secs(10), reached_rx)
        .await
        .expect("the run never reached its pause")
        .unwrap();
    assert!(
        the_key_is_held(&pool).await,
        "the run has flushed a batch through `sync.item`'s trigger, so it holds the key \
         shared; without that this test deletes before the interaction it is about"
    );

    // Well short of the pause, which lasts until the release below.
    let deleted = tokio::time::timeout(
        Duration::from_secs(5),
        knobas_sync::config::delete(&pool, &id, purge_items),
    )
    .await;
    let still_paused = !run.is_finished();
    let _ = release_tx.send(());
    let deleted = deleted.expect(
        "delete_source waited on a run that had flushed a batch: it must return while the \
         run is still paused (#127's non-blocking timing)",
    );
    assert!(
        deleted.unwrap(),
        "the configuration row was there to remove"
    );
    assert!(
        still_paused,
        "the delete has to return while the run is paused"
    );

    let report = tokio::time::timeout(Duration::from_secs(10), run)
        .await
        .expect("the run never ended after its release")
        .unwrap()
        .expect("the run ends Finished, not in an error");
    assert_eq!(report.upserted, knobas_sync::BATCH as u64 + 1);
}

/// `delete_source(purge_items: false)` during a run that has flushed a batch.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_source_deleted_after_its_run_flushed_returns_and_the_run_finishes() {
    delete_while_paused(false).await;
}

/// The same with `purge_items: true`. The flushed batch holds only items new
/// to the mirror: `PURGE_ITEMS` deletes `sync.item` rows, and a run that has
/// *re*-written existing rows holds their row locks until it commits -- a wait
/// that predates `0028` and is reported on #627, not tested away here.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_source_purged_after_its_run_flushed_new_items_returns_and_the_run_finishes() {
    delete_while_paused(true).await;
}
