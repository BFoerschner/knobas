//! Carry-over (M0 → M1, stream F): "the cursor is read outside `run_once`'s
//! advisory lock (overlapping same-source triggers double-fetch)".
//!
//! What that costs in practice: two triggers arriving together -- the scheduler
//! tick and a *Sync now* -- both read `cursor = null`, both run a full sync, and
//! the second re-fetches the entire source over the network. The upserts are
//! idempotent, so nothing corrupts; it is the *fetch* that is wasted, and on a
//! 40,000-issue Jira that is the difference between a five-second poll and a
//! ten-minute one.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use knobas_source::{
    Capability, ConnectionInfo, Cursor, KindInfo, Sink, Source, SourceDescriptor, SourceError,
    SyncItem, WriteOp,
};
use sqlx::PgPool;

/// An adapter that records every cursor it was handed and then advances it.
struct Recorder {
    id: String,
    seen: Arc<Mutex<Vec<Option<Cursor>>>>,
}

#[async_trait]
impl Source for Recorder {
    fn descriptor(&self) -> SourceDescriptor {
        SourceDescriptor {
            id: self.id.clone(),
            adapter_kind: "recorder".into(),
            name: "Recorder".into(),
            capabilities: Vec::<Capability>::new(),
            adapter_version: "0.1.0".into(),
            auth_methods: Vec::new(),
            accepts_account: false,
            write_ops: Vec::new(),
            entity_kinds: vec![KindInfo {
                id: "ticket".into(),
                label: "Ticket".into(),
                plural: "Tickets".into(),
                monogram: "RE".into(),
                full_sync_exhaustive: true,
            }],
            config_schema: serde_json::json!({ "type": "object", "properties": {} }),
            // Nothing declared: this stand-in has no payload shapes to
            // read, so every path-driven read misses on it (#277).
            payload_paths: Vec::new(),
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
        self.seen.lock().unwrap().push(cursor.clone());
        // Long enough that the second run is certainly waiting on the advisory
        // lock while this one still holds it.
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        sink.item(SyncItem {
            entity: knobas_core::entity::EntityRef::new(&self.id, "ONE"),
            kind: "ticket".into(),
            title: "one".into(),
            body_text: String::new(),
            author: None,
            updated_at: None,
            payload: serde_json::json!({}),
            web_url: None,
            deleted: false,
        })
        .await?;
        Ok(r#"{"v":1,"n":1}"#.to_owned())
    }

    async fn write(&self, _op: WriteOp) -> Result<knobas_source::WriteReceipt, SourceError> {
        Err(SourceError::protocol("read-only"))
    }
}

async fn pool() -> PgPool {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    pool
}

/// A connection of this run's own -- what `run_from_stored_cursor` now
/// requires, and what interfaces §10.6(c) is about. `tests/dedicated.rs` is
/// where the property itself is pinned; here it is only the plumbing.
async fn dedicated() -> sqlx::PgConnection {
    knobas_db::test_util::test_connector()
        .await
        .connect()
        .await
        .expect("a connection outside every pool")
}

fn unique() -> String {
    format!("cur-{}", uuid::Uuid::new_v4().simple())
}

async fn configure(pool: &PgPool, id: &str) {
    sqlx::query(
        "insert into knobas.source_config (id, kind, display_name, base_url, auth_kind)
         values ($1, 'recorder', 'Recorder', '', 'none')",
    )
    .bind(id)
    .execute(pool)
    .await
    .unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn two_overlapping_runs_of_one_source_do_not_both_start_from_scratch() {
    let pool = pool().await;
    let id = unique();
    configure(&pool, &id).await;

    let seen = Arc::new(Mutex::new(Vec::new()));
    let a = Recorder {
        id: id.clone(),
        seen: Arc::clone(&seen),
    };
    let b = Recorder {
        id: id.clone(),
        seen: Arc::clone(&seen),
    };

    let (mut ca, mut cb) = (dedicated().await, dedicated().await);
    let (ra, rb) = tokio::join!(
        knobas_sync::run_from_stored_cursor(&mut ca, &pool, &a),
        knobas_sync::run_from_stored_cursor(&mut cb, &pool, &b),
    );
    ra.unwrap();
    rb.unwrap();

    let seen = seen.lock().unwrap().clone();
    assert_eq!(seen.len(), 2);
    assert!(
        seen.iter().any(Option::is_some),
        "the second run must see the cursor the first stored, not another full \
         sync: {seen:?}"
    );
    // …and exactly one of them was the full sync: both seeing a cursor would
    // mean neither ran first.
    assert_eq!(
        seen.iter().filter(|c| c.is_none()).count(),
        1,
        "exactly one of two overlapping runs is the initial full sync: {seen:?}"
    );
}

/// The stored cursor is what the next run resumes from -- the whole point of
/// reading it inside the lock is that it is *fresh*.
#[tokio::test]
async fn a_later_run_resumes_from_the_cursor_the_previous_one_stored() {
    let pool = pool().await;
    let id = unique();
    configure(&pool, &id).await;

    let seen = Arc::new(Mutex::new(Vec::new()));
    let source = Recorder {
        id: id.clone(),
        seen: Arc::clone(&seen),
    };

    let mut conn = dedicated().await;
    knobas_sync::run_from_stored_cursor(&mut conn, &pool, &source)
        .await
        .unwrap();
    knobas_sync::run_from_stored_cursor(&mut conn, &pool, &source)
        .await
        .unwrap();

    let seen = seen.lock().unwrap().clone();
    assert_eq!(
        seen[0], None,
        "the first run of a new source is a full sync"
    );
    assert_eq!(
        seen[1].as_deref(),
        Some(r#"{"v":1,"n":1}"#),
        "the second resumes from what the first stored"
    );
}

#[tokio::test]
async fn a_source_with_no_configuration_row_is_refused_rather_than_syncing_into_a_void() {
    let pool = pool().await;
    let id = unique();
    let source = Recorder {
        id: id.clone(),
        seen: Arc::new(Mutex::new(Vec::new())),
    };

    let mut conn = dedicated().await;
    match knobas_sync::run_from_stored_cursor(&mut conn, &pool, &source).await {
        Err(knobas_sync::SyncError::NotConfigured { id: got }) => assert_eq!(got, id),
        other => panic!("expected NotConfigured, got {other:?}"),
    }
    // An unconfigured source has nowhere to store a cursor, so every later run
    // would sync everything again for ever with no sign that anything is wrong.
    // `run_once` still syncs it -- that is the ad-hoc/import path -- but the
    // scheduler must not.
    knobas_sync::run_once(&pool, &source, None)
        .await
        .expect("run_once still accepts it");
}

/// `run_once` keeps its M0 contract exactly: the caller's cursor is what the
/// run uses, even when a different one is stored. `demo_load` passes `None`
/// and must keep getting a full sync.
#[tokio::test]
async fn run_once_still_obeys_the_cursor_it_was_handed() {
    let pool = pool().await;
    let id = unique();
    configure(&pool, &id).await;

    let seen = Arc::new(Mutex::new(Vec::new()));
    let source = Recorder {
        id: id.clone(),
        seen: Arc::clone(&seen),
    };

    // Store a cursor by running once…
    let mut conn = dedicated().await;
    knobas_sync::run_from_stored_cursor(&mut conn, &pool, &source)
        .await
        .unwrap();
    // …then ask for a full sync explicitly.
    knobas_sync::run_once(&pool, &source, None).await.unwrap();
    // …and for a cursor of the caller's own choosing.
    knobas_sync::run_once(&pool, &source, Some("caller-chose".to_owned()))
        .await
        .unwrap();

    let seen = seen.lock().unwrap().clone();
    assert_eq!(seen[0], None);
    assert_eq!(
        seen[1], None,
        "an explicit None is a full sync, whatever is stored"
    );
    assert_eq!(seen[2].as_deref(), Some("caller-chose"));
}
