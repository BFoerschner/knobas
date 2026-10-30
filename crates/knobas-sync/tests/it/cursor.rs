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
    /// Where `sync` waits, after recording, until the test lets it through.
    hold: Option<Arc<tokio::sync::Semaphore>>,
}

#[async_trait]
impl Source for Recorder {
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
            adapter_kind: "recorder".into(),
            name: "Recorder".into(),
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
                label: "Ticket".into(),
                plural: "Tickets".into(),
                monogram: "RE".into(),
                full_sync_exhaustive: true,
                refreshes: true,
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
        if let Some(hold) = &self.hold {
            hold.acquire().await.unwrap().forget();
        }
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

    /// No workflow: this fake declares no `transition` write op, which is the
    /// contract battery's rule for when the read is refused (#498).
    async fn reachable_transitions(
        &self,
        entity: &str,
    ) -> Result<Vec<String>, knobas_source::SourceError> {
        Err(knobas_source::SourceError::protocol(format!(
            "this fake has no workflow, so there are no reachable transitions for {entity:?}"
        )))
    }
}

async fn pool() -> PgPool {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    pool
}

/// A connection of this run's own -- what `run_from_stored_cursor` now
/// requires, and what interfaces §10.6(c) is about. `tests/it/dedicated.rs` is
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

    // The first run in is held in `sync`, its lock held, until the second is
    // waiting on that lock (#1927). Before, a 300 ms dwell stood for this,
    // and a second run slower to get there than the dwell found the first
    // one over and read its cursor, which passes without the overlap.
    let hold = Arc::new(tokio::sync::Semaphore::new(0));
    let seen = Arc::new(Mutex::new(Vec::new()));
    let a = Recorder {
        id: id.clone(),
        seen: Arc::clone(&seen),
        hold: Some(Arc::clone(&hold)),
    };
    let b = Recorder {
        id: id.clone(),
        seen: Arc::clone(&seen),
        hold: Some(Arc::clone(&hold)),
    };

    let (mut ca, mut cb) = (dedicated().await, dedicated().await);
    let mut pids = Vec::new();
    for conn in [&mut ca, &mut cb] {
        let pid: i32 = sqlx::query_scalar("select pg_backend_pid()")
            .fetch_one(conn)
            .await
            .unwrap();
        pids.push(pid);
    }
    let overlapped = async {
        tokio::time::timeout(std::time::Duration::from_secs(30), async {
            loop {
                let waiting: i64 = sqlx::query_scalar(
                    "select count(*) from pg_locks where pid = any($1) and not granted",
                )
                .bind(&pids)
                .fetch_one(&pool)
                .await
                .unwrap();
                if waiting > 0 {
                    return;
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("neither run waited on the other's lock");
        hold.add_permits(2);
    };
    let (ra, rb, ()) = tokio::join!(
        knobas_sync::run_from_stored_cursor(&mut ca, &pool, &a),
        knobas_sync::run_from_stored_cursor(&mut cb, &pool, &b),
        overlapped,
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
        hold: None,
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
        hold: None,
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
        hold: None,
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
