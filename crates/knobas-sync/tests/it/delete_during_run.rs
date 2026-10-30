//! Deleting a source while a run of it has already flushed a batch (#627, the
//! deputy's second ruling; #632).
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
//!
//! **A run that re-wrote existing items (#632).** `ITEM_UPSERT`'s `on conflict
//! ... do update` row-locks every existing `sync.item` row it rewrites until the
//! run commits, and that is after all of its network work (contract
//! §10.6(c)). The purge used to wait on those rows -- and, when the run then
//! rewrote a row the purge had already deleted, the two waited on each other.
//! The purge now skips the rows a run holds and leaves them to the purge the
//! scheduler re-applies when that run settles; the last case here drives that
//! half through `Scheduler::forget_source`, as `delete_source` does.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use knobas_core::entity::EntityRef;
use knobas_secrets::MemoryStore;
use knobas_source::instance::SourceInstance;
use knobas_source::{
    Cursor, KindInfo, Sink, Source, SourceDescriptor, SourceError, SyncItem, WriteOp,
};
use knobas_sync::progress::{ProgressSink, SyncPhase, SyncProgress};
use knobas_sync::run_log::SyncTrigger;
use knobas_sync::scheduler::{
    AdapterRegistry, Purge, RunConnections, Scheduler, SchedulerDeps, SchedulerTiming,
    SourceSyncStatus, SyncEvents,
};
use sqlx::PgPool;
use tokio::sync::oneshot;
use uuid::Uuid;

/// The cases share one database and each asserts that *someone* holds the
/// key, or that *some* lock is waited on; one at a time keeps "someone" this
/// test's run.
static SERIAL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// An adapter that emits `before`, says so, waits to be let go, then emits
/// `after` and returns. With neither channel it is an ordinary run of
/// `before ++ after`, which is how a mirror is seeded.
struct PausingSource {
    id: String,
    before: Vec<String>,
    after: Vec<String>,
    reached: Mutex<Option<oneshot::Sender<()>>>,
    release: tokio::sync::Mutex<Option<oneshot::Receiver<()>>>,
}

impl PausingSource {
    fn new(
        id: &str,
        before: Vec<String>,
        after: Vec<String>,
        reached: Option<oneshot::Sender<()>>,
        release: Option<oneshot::Receiver<()>>,
    ) -> PausingSource {
        PausingSource {
            id: id.to_owned(),
            before,
            after,
            reached: Mutex::new(reached),
            release: tokio::sync::Mutex::new(release),
        }
    }
}

fn item(source: &str, key: &str) -> SyncItem {
    SyncItem {
        entity: EntityRef::new(source, key),
        kind: "ticket".to_owned(),
        title: key.to_owned(),
        body_text: String::new(),
        author: None,
        updated_at: None,
        payload: serde_json::json!({ "key": key }),
        web_url: None,
        deleted: false,
    }
}

/// `prefix-0000 .. prefix-(n-1)`, zero-padded so emission order, sort order
/// and heap order are all the same order.
fn keys(prefix: &str, range: std::ops::Range<usize>) -> Vec<String> {
    range.map(|n| format!("{prefix}-{n:04}")).collect()
}

#[async_trait::async_trait]
impl Source for PausingSource {
    /// Draws nothing: this fake has no source view (#661).
    async fn view(
        &self,
        _entity: &str,
        _section: Option<&str>,
    ) -> Result<Option<knobas_source::view::SourceView>, knobas_source::SourceError> {
        Ok(None)
    }

    fn descriptor(&self) -> SourceDescriptor {
        SourceDescriptor {
            id: self.id.clone(),
            adapter_kind: "fake".to_owned(),
            name: "Fake".to_owned(),
            capabilities: Vec::new(),
            adapter_version: "0.1.0".to_owned(),
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
                id: "ticket".to_owned(),
                label: "Ticket".to_owned(),
                plural: "Tickets".to_owned(),
                monogram: "TK".to_owned(),
                full_sync_exhaustive: false,
                refreshes: true,
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
        for key in &self.before {
            sink.item(item(&self.id, key)).await?;
        }
        if let Some(reached) = self.reached.lock().unwrap().take() {
            let _ = reached.send(());
        }
        if let Some(release) = self.release.lock().await.take() {
            let _ = release.await;
        }
        for key in &self.after {
            sink.item(item(&self.id, key)).await?;
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

/// Whether anybody holds `0028`'s `(627, 0)` key in this file's database:
/// `pg_advisory_xact_lock(int4, int4)` shows as `classid` = the first key,
/// `objid` = the second, `objsubid` = 2. Advisory keys are per database, and
/// `pg_locks` lists every database on the server the gate's binaries share.
async fn the_key_is_held(pool: &PgPool) -> bool {
    sqlx::query_scalar(
        "select exists (select 1 from pg_locks
                         where locktype = 'advisory' and classid = 627 and objid = 0
                           and objsubid = 2 and granted
                           and database = (select oid from pg_database
                                            where datname = current_database()))",
    )
    .fetch_one(pool)
    .await
    .unwrap()
}

/// Whether some backend on this file's database is waiting for a lock right
/// now. Only this file's tests use this database and they run one at a time,
/// so a waiter is the delete under test.
async fn somebody_waits_on_a_lock(pool: &PgPool) -> bool {
    sqlx::query_scalar(
        "select exists (select 1 from pg_stat_activity
                         where datname = current_database() and wait_event_type = 'Lock')",
    )
    .fetch_one(pool)
    .await
    .unwrap()
}

/// A configured source no scheduler's ticker will adopt on its own.
async fn configure(pool: &PgPool) -> String {
    let id = format!("fake-{}", Uuid::new_v4());
    sqlx::query(
        "insert into knobas.source_config
                (id, kind, display_name, base_url, auth_kind, backoff_until)
         values ($1, 'fake', 'Fake', 'http://localhost', 'none', now() + interval '1 hour')",
    )
    .bind(&id)
    .execute(pool)
    .await
    .unwrap();
    id
}

/// One whole run of `keys`, so the source has a mirror before anything pauses.
async fn seed_mirror(pool: &PgPool, id: &str, keys: Vec<String>) {
    let n = keys.len();
    let source = PausingSource::new(id, keys, Vec::new(), None, None);
    let report = knobas_sync::run_once(pool, &source, None).await.unwrap();
    assert_eq!(report.upserted, n as u64, "the seed run mirrored every key");
}

async fn mirrored_items(pool: &PgPool, id: &str) -> i64 {
    sqlx::query_scalar("select count(*) from sync.item where source_id = $1")
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// What a test does to one source: the mirror it starts with (in heap order),
/// what its run emits before its pause, and what after.
struct Plan {
    mirror: Vec<String>,
    before: Vec<String>,
    after: Vec<String>,
}

/// Configure a fresh source, mirror `plan.mirror`, start a run of it, let it
/// flush `plan.before` and pause, then delete the source; return once the run
/// has ended. The delete has to return while the run is still paused.
async fn delete_while_paused(purge_items: bool, plan: Plan) {
    let _serial = SERIAL.lock().await;
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    let id = configure(&pool).await;
    if !plan.mirror.is_empty() {
        seed_mirror(&pool, &id, plan.mirror).await;
    }
    let emitted = plan.before.len() + plan.after.len();

    let (reached_tx, reached_rx) = oneshot::channel();
    let (release_tx, release_rx) = oneshot::channel();
    let source = PausingSource::new(
        &id,
        plan.before,
        plan.after,
        Some(reached_tx),
        Some(release_rx),
    );
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
        Duration::from_secs(10),
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
    assert_eq!(report.upserted, emitted as u64);
}

/// `delete_source(purge_items: false)` during a run that has flushed a batch.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_source_deleted_after_its_run_flushed_returns_and_the_run_finishes() {
    delete_while_paused(
        false,
        Plan {
            mirror: Vec::new(),
            before: keys("NEW", 0..knobas_sync::BATCH + 1),
            after: Vec::new(),
        },
    )
    .await;
}

/// The same with `purge_items: true`, over a flushed batch of items new to
/// the mirror: rows the purge's snapshot cannot see, so nothing to wait on.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_source_purged_after_its_run_flushed_new_items_returns_and_the_run_finishes() {
    delete_while_paused(
        true,
        Plan {
            mirror: Vec::new(),
            before: keys("NEW", 0..knobas_sync::BATCH + 1),
            after: Vec::new(),
        },
    )
    .await;
}

/// **#632.** The same over a flushed batch that *re*-wrote items the mirror
/// already held: the run holds those rows' locks until it commits, and the
/// purge must not wait for that.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_source_purged_after_its_run_rewrote_existing_items_returns_and_the_run_finishes() {
    let mirror = keys("OLD", 0..knobas_sync::BATCH);
    delete_while_paused(
        true,
        Plan {
            before: mirror.clone(),
            mirror,
            after: Vec::new(),
        },
    )
    .await;
}

/// **#632's deadlock interleaving.** The mirror holds `A` then `B`, in that
/// heap order. The run rewrites `B` and pauses; the purge starts, deleting `A`
/// as it scans. After its release the run rewrites `A` -- rows the purge
/// reached. A purge that waited for `B` would hold `A` while the run waited
/// for `A`: each waiting on the other, and PostgreSQL aborting one of them.
///
/// No assertion on *when* the delete returns -- the case above owns that -- so
/// that on a purge that waits, this case reports the deadlock rather than a
/// timeout. Both sides succeed.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_purge_and_a_run_that_rewrites_rows_the_purge_reached_both_succeed() {
    let _serial = SERIAL.lock().await;
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    let id = configure(&pool).await;
    let a = keys("OLD", 0..knobas_sync::BATCH);
    let b = keys("OLD", knobas_sync::BATCH..2 * knobas_sync::BATCH);
    seed_mirror(&pool, &id, a.iter().chain(&b).cloned().collect()).await;

    let (reached_tx, reached_rx) = oneshot::channel();
    let (release_tx, release_rx) = oneshot::channel();
    // `after` is exactly one batch, so it is flushed by the sink -- while the
    // purge may still be open -- and not by the run's closing flush.
    let source = PausingSource::new(&id, b, a, Some(reached_tx), Some(release_rx));
    let run_pool = pool.clone();
    let run = tokio::spawn(async move { knobas_sync::run_once(&run_pool, &source, None).await });
    tokio::time::timeout(Duration::from_secs(10), reached_rx)
        .await
        .expect("the run never reached its pause")
        .unwrap();

    let delete_pool = pool.clone();
    let delete_id = id.clone();
    let delete =
        tokio::spawn(
            async move { knobas_sync::config::delete(&delete_pool, &delete_id, true).await },
        );
    // Release the run once the delete is either done or stuck on a row lock --
    // the two states the interleaving can be in.
    let mut stuck = false;
    for _ in 0..500 {
        if delete.is_finished() {
            break;
        }
        if somebody_waits_on_a_lock(&pool).await {
            stuck = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let _ = release_tx.send(());

    let report = tokio::time::timeout(Duration::from_secs(30), run)
        .await
        .expect("the run never ended after its release")
        .unwrap();
    let deleted = tokio::time::timeout(Duration::from_secs(30), delete)
        .await
        .expect("the delete never returned")
        .unwrap();
    let deleted =
        deleted.unwrap_or_else(|e| panic!("the delete failed (stuck on a lock: {stuck}): {e}"));
    assert!(deleted, "the configuration row was there to remove");
    let report =
        report.unwrap_or_else(|e| panic!("the run failed (delete stuck on a lock: {stuck}): {e}"));
    assert_eq!(report.upserted, 2 * knobas_sync::BATCH as u64);
}

/// A registry that hands the scheduler one prepared `PausingSource`.
struct OneSource(Mutex<Option<PausingSource>>);

impl AdapterRegistry for OneSource {
    fn descriptors(&self) -> Vec<SourceDescriptor> {
        Vec::new()
    }
    fn build(&self, instance: SourceInstance) -> Result<Box<dyn Source>, SourceError> {
        let source = self
            .0
            .lock()
            .unwrap()
            .take()
            .ok_or_else(|| SourceError::protocol("built twice".to_owned()))?;
        assert_eq!(source.id, instance.id);
        Ok(Box::new(source))
    }
}

struct Silent;
impl SyncEvents for Silent {
    fn sync_state(&self, _s: SourceSyncStatus) {}
    fn source_health(&self, _h: knobas_sync::config::CredentialHealth) {}
    fn activity_new(&self, _r: knobas_core::activity::ActivityRow) {}
}

struct TestConnections(knobas_db::embedded::Connector);

#[async_trait::async_trait]
impl RunConnections for TestConnections {
    async fn open(&self) -> Result<sqlx::PgConnection, sqlx::Error> {
        self.0.connect().await
    }
}

#[derive(Default)]
struct Heard(Mutex<Vec<SyncProgress>>);

impl ProgressSink for Heard {
    fn report(&self, progress: SyncProgress) {
        self.0.lock().unwrap().push(progress);
    }
}

impl Heard {
    fn ending(&self) -> Option<SyncProgress> {
        self.0
            .lock()
            .unwrap()
            .iter()
            .find(|p| matches!(p.phase, SyncPhase::Finished | SyncPhase::Failed))
            .cloned()
    }
}

/// **Eventual, through the scheduler.** `delete_source`'s two steps --
/// `config::delete` with the purge, then `forget_source(Purge::Items)` --
/// while a scheduled run that has rewritten part of the mirror is paused. The
/// inline purge leaves the rows that run holds; once the run has settled, the
/// source has no `sync.item` rows, and every entity the mirror or the run
/// named is tombstoned.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_rows_a_purge_skipped_are_purged_once_the_run_settles() {
    let _serial = SERIAL.lock().await;
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    let id = configure(&pool).await;
    let a = keys("OLD", 0..knobas_sync::BATCH);
    let b = keys("OLD", knobas_sync::BATCH..2 * knobas_sync::BATCH);
    seed_mirror(&pool, &id, a.iter().chain(&b).cloned().collect()).await;
    // After the release: `A` again, and items new to the mirror.
    let fresh = keys("NEW", 0..10);
    let after: Vec<String> = a.iter().chain(&fresh).cloned().collect();

    let (reached_tx, reached_rx) = oneshot::channel();
    let (release_tx, release_rx) = oneshot::channel();
    let source = PausingSource::new(&id, b.clone(), after, Some(reached_tx), Some(release_rx));
    let scheduler_pool = knobas_db::test_util::test_pool().await;
    let scheduler = Scheduler::start(SchedulerDeps {
        pool: scheduler_pool,
        connections: Arc::new(TestConnections(
            knobas_db::test_util::test_connector().await,
        )),
        registry: Arc::new(OneSource(Mutex::new(Some(source)))),
        secrets: Arc::new(MemoryStore::new()),
        events: Arc::new(Silent),
        timing: SchedulerTiming::default(),
        quit: knobas_sync::scheduler::Quit::default(),
    })
    .await
    .unwrap();

    let heard = Arc::new(Heard::default());
    scheduler
        .trigger(
            &id,
            SyncTrigger::Manual,
            Some(Arc::clone(&heard) as Arc<dyn ProgressSink>),
        )
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(10), reached_rx)
        .await
        .expect("the run never reached its pause")
        .unwrap();

    // `delete_source`, in its order.
    let deleted = tokio::time::timeout(
        Duration::from_secs(10),
        knobas_sync::config::delete(&pool, &id, true),
    )
    .await
    .expect("the delete has to return while the run is paused")
    .unwrap();
    assert!(deleted);
    assert_eq!(
        mirrored_items(&pool, &id).await,
        b.len() as i64,
        "the inline purge took the rows the run does not hold and left the ones it does; \
         otherwise there is nothing here for the scheduler's purge to finish"
    );
    scheduler.forget_source(&id, Purge::Items).await;
    let _ = release_tx.send(());

    let mut ending = None;
    for _ in 0..400 {
        ending = heard.ending();
        if ending.is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    let ending = ending.expect("the enrolled caller was told how its run ended (ADR-0005)");
    assert_eq!(
        (ending.phase, ending.items),
        (
            SyncPhase::Finished,
            (2 * knobas_sync::BATCH + fresh.len()) as u64
        ),
        "the run committed everything it emitted after the delete: {ending:?}"
    );
    scheduler.shutdown().await;

    assert_eq!(
        mirrored_items(&pool, &id).await,
        0,
        "the run that was in flight when the source was deleted left rows in the mirror"
    );
    let named: Vec<String> = a
        .iter()
        .chain(&b)
        .chain(&fresh)
        .map(|key| format!("{id}:{key}"))
        .collect();
    let live: Vec<String> = sqlx::query_scalar(
        "select id from knobas.entity where id = any($1) and deleted_at is null order by id",
    )
    .bind(&named)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert!(live.is_empty(), "entities left untombstoned: {live:?}");
    let present: i64 = sqlx::query_scalar("select count(*) from knobas.entity where id = any($1)")
        .bind(&named)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(present, named.len() as i64, "tombstoned, never deleted");
}

/// Purge a mirror of two items while another transaction holds a lock of
/// `strength` on the first one's entity; return the purge's result (bounded)
/// and the keys still mirrored after it, with that transaction rolled back.
async fn purge_with_entity_locked(strength: &str) -> (bool, Vec<String>) {
    let _serial = SERIAL.lock().await;
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    let id = configure(&pool).await;
    seed_mirror(&pool, &id, keys("OLD", 0..2)).await;

    let mut holder = pool.begin().await.unwrap();
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "select 1 from knobas.entity where id = $1 for {strength}"
    )))
    .bind(format!("{id}:OLD-0000"))
    .execute(&mut *holder)
    .await
    .unwrap();

    let returned = tokio::time::timeout(
        Duration::from_secs(10),
        knobas_sync::config::delete(&pool, &id, true),
    )
    .await
    .map(Result::unwrap)
    .is_ok();
    holder.rollback().await.unwrap();

    let left: Vec<String> = sqlx::query_scalar(
        "select entity_id from sync.item where source_id = $1 order by entity_id",
    )
    .bind(&id)
    .fetch_all(&pool)
    .await
    .unwrap();
    (
        returned,
        left.into_iter()
            .map(|e| e.rsplit(':').next().unwrap().to_owned())
            .collect(),
    )
}

/// An item whose *entity* a run holds -- `ENTITY_UPSERT` locks it before
/// `ITEM_UPSERT` reaches the item -- is left whole, not deleted and then
/// waited on at the entity's tombstone.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_purge_leaves_an_item_whose_entity_is_held_for_update() {
    let (returned, left) = purge_with_entity_locked("update").await;
    assert!(returned, "the purge waited on an entity somebody holds");
    assert_eq!(
        left,
        vec!["OLD-0000".to_owned()],
        "only the held item is left"
    );
}

/// A `for key share` holder -- what writing a link or a note against the
/// entity takes -- is no reason to skip it: the purge's tombstone does not
/// conflict with it.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_purge_takes_an_item_whose_entity_is_only_key_shared() {
    let (returned, left) = purge_with_entity_locked("key share").await;
    assert!(returned, "the purge waited on a key-share lock");
    assert!(
        left.is_empty(),
        "the purge skipped items it could take: {left:?}"
    );
}
