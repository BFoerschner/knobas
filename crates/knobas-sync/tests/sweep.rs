//! Carry-over (M0 → M1, stream F): "a full sync cannot express items the source
//! stopped returning; rows stay live forever."
//!
//! The reconciliation is the one interfaces §1 specifies: after a `cursor:
//! None` run, tombstone every entity whose mirror row was not touched by this
//! run. No `last_seen_at` column is needed -- `sync.item.synced_at` is already
//! the run's transaction timestamp, identical for every row the run wrote, so
//! `synced_at < now()` inside that same transaction means exactly "this run did
//! not see it".
//!
//! **Gated on `SourceDescriptor.full_sync_exhaustive`.** The inference "this
//! full sync did not return it, therefore it is gone" only holds for an adapter
//! whose full sync really does return everything. TeamCity's does not -- it
//! fetches the newest N builds per configuration -- so it declares `false` and
//! is never swept; Jira, Gitea and the mock declare `true`.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use knobas_source::{
    Capability, ConnectionInfo, Cursor, KindInfo, Sink, Source, SourceDescriptor, SourceError,
    SyncItem, WriteOp,
};
use sqlx::PgPool;

/// An adapter emitting whichever keys it is currently told to, and declaring
/// whether its full sync is exhaustive.
struct Shrinking {
    id: String,
    keys: Arc<Mutex<Vec<&'static str>>>,
    /// The gate the sweep obeys: `true` means "a full sync of me returns
    /// everything I have", which is what makes absence proof of deletion.
    exhaustive: bool,
}

#[async_trait]
impl Source for Shrinking {
    fn descriptor(&self) -> SourceDescriptor {
        SourceDescriptor {
            id: self.id.clone(),
            adapter_kind: "shrinking".into(),
            name: "Shrinking".into(),
            capabilities: Vec::<Capability>::new(),
            adapter_version: "0.1.0".into(),
            auth_methods: Vec::new(),
            write_ops: Vec::new(),
            entity_kinds: vec![KindInfo {
                id: "ticket".into(),
                label: "Ticket".into(),
                plural: "Tickets".into(),
                monogram: "SH".into(),
            }],
            full_sync_exhaustive: self.exhaustive,
            config_schema: serde_json::json!({ "type": "object", "properties": {} }),
        }
    }

    async fn test_connection(&self) -> Result<ConnectionInfo, SourceError> {
        Ok(ConnectionInfo::default())
    }

    async fn sync(
        &self,
        cursor: Option<Cursor>,
        sink: &mut (dyn Sink + Send),
    ) -> Result<Cursor, SourceError> {
        let keys = self.keys.lock().unwrap().clone();
        for key in &keys {
            sink.item(SyncItem {
                entity: knobas_core::entity::EntityRef::new(&self.id, key),
                kind: "ticket".into(),
                title: format!("{key} title"),
                body_text: String::new(),
                author: None,
                updated_at: None,
                payload: serde_json::json!({}),
                web_url: None,
                deleted: false,
            })
            .await?;
        }
        // Battery clause 2: a run that emitted nothing returns the cursor it
        // was handed, byte-identical.
        if keys.is_empty() {
            return Ok(cursor.unwrap_or_default());
        }
        Ok(r#"{"v":1,"n":1}"#.to_owned())
    }

    async fn write(&self, _op: WriteOp) -> Result<(), SourceError> {
        Err(SourceError::Protocol("read-only".into()))
    }
}

async fn pool() -> PgPool {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    pool
}

async fn deleted_at(pool: &PgPool, id: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    let (d,): (Option<chrono::DateTime<chrono::Utc>>,) =
        sqlx::query_as("select deleted_at from knobas.entity where id = $1")
            .bind(id)
            .fetch_one(pool)
            .await
            .unwrap();
    d
}

async fn live_count(pool: &PgPool, source_id: &str) -> i64 {
    let (n,): (i64,) = sqlx::query_as("select count(*) from sync.live_item where source_id = $1")
        .bind(source_id)
        .fetch_one(pool)
        .await
        .unwrap();
    n
}

fn unique() -> String {
    format!("swp-{}", uuid::Uuid::new_v4().simple())
}

/// An adapter whose full sync returns everything it has -- the Jira/Gitea/mock
/// shape, where absence really does mean deletion.
fn source(id: &str, keys: &[&'static str]) -> (Shrinking, Arc<Mutex<Vec<&'static str>>>) {
    let keys = Arc::new(Mutex::new(keys.to_vec()));
    (
        Shrinking {
            id: id.to_owned(),
            keys: Arc::clone(&keys),
            exhaustive: true,
        },
        keys,
    )
}

/// An adapter whose full sync is **bounded** -- the TeamCity shape: "the newest
/// N builds per configuration", so an item this run did not return may simply
/// have fallen off the window.
fn bounded_source(id: &str, keys: &[&'static str]) -> (Shrinking, Arc<Mutex<Vec<&'static str>>>) {
    let keys = Arc::new(Mutex::new(keys.to_vec()));
    (
        Shrinking {
            id: id.to_owned(),
            keys: Arc::clone(&keys),
            exhaustive: false,
        },
        keys,
    )
}

/// A second of daylight, so `synced_at` is unmistakably older than the next
/// run's transaction timestamp even at coarse clock resolution.
async fn a_moment_passes() {
    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
}

#[tokio::test]
async fn a_full_sync_tombstones_what_the_source_stopped_returning_and_keeps_its_mirror_row() {
    let pool = pool().await;
    let id = unique();
    let (src, keys) = source(&id, &["A-1", "A-2", "A-3"]);

    let first = knobas_sync::run_once(&pool, &src, None).await.unwrap();
    assert_eq!(first.upserted, 3);
    assert_eq!(first.swept, 0, "nothing is stale on the first full sync");

    // Upstream hard-deletes A-2: it simply stops appearing.
    *keys.lock().unwrap() = vec!["A-1", "A-3"];
    a_moment_passes().await;

    let second = knobas_sync::run_once(&pool, &src, None).await.unwrap();
    assert_eq!(second.upserted, 2);
    assert_eq!(second.swept, 1, "the vanished item is reconciled");

    assert!(deleted_at(&pool, &format!("{id}:A-2")).await.is_some());
    assert!(deleted_at(&pool, &format!("{id}:A-1")).await.is_none());
    assert!(deleted_at(&pool, &format!("{id}:A-3")).await.is_none());

    // The mirror row survives, so the UI can still render the last-known title
    // of something that vanished upstream.
    let (title,): (String,) = sqlx::query_as("select title from sync.item where entity_id = $1")
        .bind(format!("{id}:A-2"))
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(title, "A-2 title");

    // And `sync.live_item` -- the structural tombstone filter every reader gets
    // for free (interfaces §1, point 1) -- no longer offers it.
    assert_eq!(live_count(&pool, &id).await, 2);
}

/// **The gate.** Not every adapter's full sync is exhaustive: TeamCity's is
/// "the newest N builds per configuration", so an old build that this run did
/// not return has not been deleted -- it has fallen off the end of a bounded
/// window. Sweeping there would tombstone a source's entire history one page at
/// a time. The adapter declares which it is, and the engine believes it.
#[tokio::test]
async fn a_source_whose_full_sync_is_bounded_is_never_swept() {
    let pool = pool().await;
    let id = unique();
    let (src, keys) = bounded_source(&id, &["T-1", "T-2", "T-3"]);

    let first = knobas_sync::run_once(&pool, &src, None).await.unwrap();
    assert_eq!(first.upserted, 3);

    // The window slid: T-1 is simply older than the newest two builds.
    *keys.lock().unwrap() = vec!["T-2", "T-3"];
    a_moment_passes().await;
    let second = knobas_sync::run_once(&pool, &src, None).await.unwrap();

    assert_eq!(second.upserted, 2);
    assert_eq!(
        second.swept, 0,
        "a bounded full sync proves nothing about absence"
    );
    assert!(
        deleted_at(&pool, &format!("{id}:T-1")).await.is_none(),
        "an old build that fell out of the window is still a real build"
    );
    assert_eq!(live_count(&pool, &id).await, 3, "all three are still live");
}

#[tokio::test]
async fn an_incremental_run_never_sweeps() {
    let pool = pool().await;
    let id = unique();
    let (src, keys) = source(&id, &["B-1", "B-2"]);
    let first = knobas_sync::run_once(&pool, &src, None).await.unwrap();

    *keys.lock().unwrap() = vec!["B-1"];
    a_moment_passes().await;
    let inc = knobas_sync::run_once(&pool, &src, Some(first.cursor))
        .await
        .unwrap();

    assert_eq!(
        inc.swept, 0,
        "an incremental run has not seen the whole source"
    );
    assert!(
        deleted_at(&pool, &format!("{id}:B-2")).await.is_none(),
        "only a full sync knows that something is gone"
    );
}

/// A full sync that emitted **nothing** is indistinguishable from an adapter
/// that silently failed -- an expired token accepted with an empty 200, a
/// misconfigured project filter. Sweeping there would tombstone the entire
/// source. One stale row is cheap; wiping a corpus is not.
#[tokio::test]
async fn a_full_sync_that_emitted_nothing_sweeps_nothing() {
    let pool = pool().await;
    let id = unique();
    let (src, keys) = source(&id, &["C-1", "C-2"]);
    knobas_sync::run_once(&pool, &src, None).await.unwrap();

    *keys.lock().unwrap() = vec![];
    a_moment_passes().await;
    let empty = knobas_sync::run_once(&pool, &src, None).await.unwrap();

    assert_eq!((empty.upserted, empty.swept), (0, 0));
    assert!(deleted_at(&pool, &format!("{id}:C-1")).await.is_none());
    assert!(deleted_at(&pool, &format!("{id}:C-2")).await.is_none());
    assert_eq!(live_count(&pool, &id).await, 2);
}

/// The sweep must keep the *first* deletion's timestamp, exactly as
/// `ENTITY_UPSERT` does -- a tombstone restamped on every run makes "deleted 3
/// days ago" say "deleted just now" for ever.
#[tokio::test]
async fn the_sweep_does_not_restamp_an_existing_tombstone() {
    let pool = pool().await;
    let id = unique();
    let (src, keys) = source(&id, &["D-1", "D-2"]);
    knobas_sync::run_once(&pool, &src, None).await.unwrap();

    *keys.lock().unwrap() = vec!["D-1"];
    a_moment_passes().await;
    let swept = knobas_sync::run_once(&pool, &src, None).await.unwrap();
    assert_eq!(swept.swept, 1);
    let first = deleted_at(&pool, &format!("{id}:D-2")).await.unwrap();

    a_moment_passes().await;
    let again = knobas_sync::run_once(&pool, &src, None).await.unwrap();
    assert_eq!(
        again.swept, 0,
        "an already-tombstoned row is not swept twice"
    );
    assert_eq!(
        deleted_at(&pool, &format!("{id}:D-2")).await.unwrap(),
        first
    );
}

/// The sweep is scoped to one source. Two sources sharing a database must not
/// tombstone each other's world.
#[tokio::test]
async fn the_sweep_only_touches_its_own_source() {
    let pool = pool().await;
    let mine = unique();
    let theirs = unique();
    let (a, keys) = source(&mine, &["E-1", "E-2"]);
    let (b, _) = source(&theirs, &["E-1"]);
    knobas_sync::run_once(&pool, &a, None).await.unwrap();
    knobas_sync::run_once(&pool, &b, None).await.unwrap();

    *keys.lock().unwrap() = vec!["E-1"];
    a_moment_passes().await;
    let swept = knobas_sync::run_once(&pool, &a, None).await.unwrap();

    assert_eq!(swept.swept, 1, "its own vanished item is still swept");
    assert!(
        deleted_at(&pool, &format!("{theirs}:E-1")).await.is_none(),
        "another source's items are not this run's to tombstone"
    );
    assert_eq!(live_count(&pool, &theirs).await, 1);
}

/// A sweep that runs must be inside the run's own transaction, or a failure
/// after it would leave tombstones over data that was rolled back. Proven by
/// the tombstone and the upserts landing together: `swept` is reported on the
/// same committed report as `upserted`.
#[tokio::test]
async fn a_resurrected_item_loses_its_tombstone_again() {
    let pool = pool().await;
    let id = unique();
    let (src, keys) = source(&id, &["F-1", "F-2"]);
    knobas_sync::run_once(&pool, &src, None).await.unwrap();

    *keys.lock().unwrap() = vec!["F-1"];
    a_moment_passes().await;
    assert_eq!(
        knobas_sync::run_once(&pool, &src, None)
            .await
            .unwrap()
            .swept,
        1
    );
    assert!(deleted_at(&pool, &format!("{id}:F-2")).await.is_some());

    // Sources do resurrect things -- an issue is un-deleted, a repo restored.
    *keys.lock().unwrap() = vec!["F-1", "F-2"];
    a_moment_passes().await;
    let back = knobas_sync::run_once(&pool, &src, None).await.unwrap();
    assert_eq!(back.swept, 0);
    assert!(
        deleted_at(&pool, &format!("{id}:F-2")).await.is_none(),
        "a stale tombstone would hide a live entity"
    );
    assert_eq!(live_count(&pool, &id).await, 2);
}
