//! Deleting the demo source while *Load demo data* is running (#636).
//!
//! The demo load syncs through `knobas_sync::run_once` called directly, so the
//! scheduler holds no claim on that run and cannot re-apply the purge behind
//! it (#632's fourth residual). The load purges behind its own run instead,
//! once that run has committed, if no source holds the id by then.
//!
//! These drive the load's work half (`demo::load_mirror_of`) with a fake
//! `Source` that pauses mid-run -- ADR-0013 keeps trait-level fakes for engine
//! tests -- on a real PostgreSQL. Nothing here sleeps: the run waits on a
//! channel the test releases only after the delete has returned, so the
//! interleaving is the one the test names or the test fails.

use std::sync::Mutex;
use std::time::Duration;

use knobas_app::sources::demo;
use knobas_core::entity::EntityRef;
use knobas_source::{
    Cursor, KindInfo, Sink, Source, SourceDescriptor, SourceError, SyncItem, WriteOp,
};
use sqlx::PgPool;
use tokio::sync::oneshot;
use uuid::Uuid;

/// An adapter that emits `before`, says so, waits to be let go, then emits
/// `after` and returns -- or fails, if `fail` is set. With neither channel it
/// is an ordinary run of `before ++ after`, which is how a mirror is seeded.
struct PausingSource {
    id: String,
    before: Vec<String>,
    after: Vec<String>,
    fail: bool,
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
            fail: false,
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

/// `prefix-0000 .. prefix-(n-1)`, zero-padded so emission order and heap order
/// are the same order.
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
        if self.fail {
            return Err(SourceError::protocol("failed after the pause".to_owned()));
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

async fn mirrored_items(pool: &PgPool, id: &str) -> i64 {
    sqlx::query_scalar("select count(*) from sync.item where source_id = $1")
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// The entities among `keys` of source `id` that are still live.
async fn live_entities(pool: &PgPool, id: &str, keys: &[String]) -> Vec<String> {
    let named: Vec<String> = keys.iter().map(|key| format!("{id}:{key}")).collect();
    let present: i64 = sqlx::query_scalar("select count(*) from knobas.entity where id = any($1)")
        .bind(&named)
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(present, named.len() as i64, "tombstoned, never deleted");
    sqlx::query_scalar(
        "select id from knobas.entity where id = any($1) and deleted_at is null order by id",
    )
    .bind(&named)
    .fetch_all(pool)
    .await
    .unwrap()
}

/// A demo load of a fresh source whose mirror already holds `OLD` items, paused
/// after it has rewritten `BATCH` of them and written `BATCH` new ones -- two
/// flushes, both inside its open transaction. Then the source is deleted with
/// its mirror, and `between` runs before the load is released. With `fail`,
/// the load's run fails after its release and rolls back.
///
/// Returns the database, the source id, and the keys by who wrote them.
async fn delete_during_a_paused_load<F, Fut>(fail: bool, between: F) -> (PgPool, String, Named)
where
    F: FnOnce(PgPool, String) -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    let id = format!("fake-{}", Uuid::new_v4());
    let batch = knobas_sync::BATCH;

    // The old mirror: `rewritten` the paused load writes again, and `untouched`
    // it never reaches.
    let rewritten = keys("OLD", 0..batch);
    let untouched = keys("OLD", batch..batch + 5);
    let seed = PausingSource::new(
        &id,
        rewritten.iter().chain(&untouched).cloned().collect(),
        Vec::new(),
        None,
        None,
    );
    demo::load_mirror_of(&pool, &seed).await.unwrap();
    assert_eq!(
        mirrored_items(&pool, &id).await,
        (rewritten.len() + untouched.len()) as i64,
        "a demo load of a source that exists purged its own mirror (#154: a source under the \
         id wins)"
    );

    let fresh = keys("NEW", 0..batch);
    let late = keys("LATE", 0..3);
    let (reached_tx, reached_rx) = oneshot::channel();
    let (release_tx, release_rx) = oneshot::channel();
    let mut source = PausingSource::new(
        &id,
        rewritten.iter().chain(&fresh).cloned().collect(),
        late.clone(),
        Some(reached_tx),
        Some(release_rx),
    );
    source.fail = fail;
    let load_pool = pool.clone();
    let load = tokio::spawn(async move { demo::load_mirror_of(&load_pool, &source).await });
    tokio::time::timeout(Duration::from_secs(10), reached_rx)
        .await
        .expect("the load never reached its pause")
        .unwrap();

    // `delete_source`'s purge, while the load is paused. The deadline is a
    // bound on a hang, not a wait: the pause lasts until the release below.
    let deleted = tokio::time::timeout(
        Duration::from_secs(10),
        knobas_sync::config::delete(&pool, &id, true),
    )
    .await;
    let still_paused = !load.is_finished();
    let deleted = match deleted {
        Ok(deleted) => deleted,
        Err(_) => {
            let _ = release_tx.send(());
            panic!("delete_source waited on the paused demo load (#127's non-blocking timing)");
        }
    };
    assert!(
        deleted.unwrap(),
        "the configuration row was there to remove"
    );
    assert!(
        still_paused,
        "the delete has to return while the load is paused"
    );
    assert_eq!(
        mirrored_items(&pool, &id).await,
        rewritten.len() as i64,
        "the inline purge left exactly the rows the paused load holds; otherwise there is \
         nothing here for the load to purge behind itself"
    );

    between(pool.clone(), id.clone()).await;
    let _ = release_tx.send(());

    let loaded = tokio::time::timeout(Duration::from_secs(10), load)
        .await
        .expect("the load never returned after its release")
        .unwrap();
    if fail {
        assert!(
            matches!(loaded, Err(demo::DemoError::Sync { .. })),
            "the load answers with its run's failure: {loaded:?}"
        );
    } else {
        let report = loaded.expect("the load ends without an error");
        assert_eq!(
            report.upserted,
            (rewritten.len() + fresh.len() + late.len()) as u64,
            "the load's run committed everything it emitted"
        );
    }

    let named = Named {
        untouched,
        rewritten,
        new: fresh.into_iter().chain(late).collect(),
    };
    (pool, id, named)
}

/// The keys a test named, by who wrote them last.
struct Named {
    /// The old mirror's rows the paused load never reached.
    untouched: Vec<String>,
    /// The old mirror's rows the paused load wrote again.
    rewritten: Vec<String>,
    /// The keys new to the mirror that the load emitted, before its pause and
    /// after.
    new: Vec<String>,
}

impl Named {
    /// Every key the old mirror held.
    fn old(&self) -> Vec<String> {
        self.untouched
            .iter()
            .chain(&self.rewritten)
            .cloned()
            .collect()
    }
    /// Every key the load emitted.
    fn run(&self) -> Vec<String> {
        self.rewritten.iter().chain(&self.new).cloned().collect()
    }
    /// Every key the old mirror held or the load emitted.
    fn all(&self) -> Vec<String> {
        self.untouched.iter().chain(&self.run()).cloned().collect()
    }
}

/// **#636's witness.** A source deleted while its demo load is paused has no
/// mirror once the load returns: every row the old mirror held and every row
/// the load rewrote or inserted is gone, and each entity is tombstoned.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_source_deleted_during_a_demo_load_has_no_mirror_once_the_load_returns() {
    let (pool, id, named) = delete_during_a_paused_load(false, |_, _| async {}).await;

    assert_eq!(
        mirrored_items(&pool, &id).await,
        0,
        "the demo load wrote its mirror back for a source that no longer exists"
    );
    let live = live_entities(&pool, &id, &named.all()).await;
    assert!(live.is_empty(), "entities left untombstoned: {live:?}");
}

/// **#154's rule on the demo path.** A source added back under the id before
/// the load finishes is the user's newest instruction about the id, so the
/// load purges nothing behind itself and its mirror stays live.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_demo_load_purges_nothing_when_a_source_holds_the_id_again() {
    let (pool, id, named) = delete_during_a_paused_load(false, |pool, id| async move {
        sqlx::query(
            "insert into knobas.source_config (id, kind, display_name, base_url, auth_kind)
             values ($1, 'fake', 'Fake again', 'http://localhost', 'none')",
        )
        .bind(&id)
        .execute(&pool)
        .await
        .unwrap();
    })
    .await;

    // The run's rows. The old mirror's untouched ones went with the inline
    // purge, before the source came back.
    assert_eq!(
        mirrored_items(&pool, &id).await,
        named.run().len() as i64,
        "the load purged the mirror of a source the user had added back under the id"
    );
    let run = named.run();
    let live = live_entities(&pool, &id, &run).await;
    assert_eq!(live.len(), run.len(), "the run's entities stay live");
}

/// **A load whose run fails is purged behind too.** The run's rollback puts
/// back the rows it rewrote, and those are the rows the inline purge skipped
/// because the run held them: live again, for a source that no longer exists.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_source_deleted_during_a_demo_load_that_fails_has_no_mirror_once_it_returns() {
    let (pool, id, named) = delete_during_a_paused_load(true, |_, _| async {}).await;

    assert_eq!(
        mirrored_items(&pool, &id).await,
        0,
        "the failed load's rollback put back rows of a source that no longer exists"
    );
    // The new keys were never committed, so only the old mirror's entities
    // exist to be tombstoned.
    let live = live_entities(&pool, &id, &named.old()).await;
    assert!(live.is_empty(), "entities left untombstoned: {live:?}");
}
