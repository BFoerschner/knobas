//! Interfaces §10.6(c): a scheduled run's transaction and advisory lock live
//! on a connection that belongs to that run alone.
//!
//! The finding this discharges is not "syncing is slow". `run_once` held its
//! transaction across the whole of `Source::sync`, so every second the adapter
//! spent on the network was a second one of the pool's five connections was
//! unavailable to the UI -- and an adapter loop with no reachable exit pinned
//! that connection *and* the source's lock until the process died (found while
//! proving a Jira paging defect had no exit). A second, larger pool removes the
//! symptom; moving the lock is what removes the finding.
//!
//! Every test here therefore measures the pool from the outside while a run is
//! parked, rather than reading the code and believing it.

use std::sync::Arc;

use async_trait::async_trait;
use knobas_source::{
    ConnectionInfo, Cursor, KindInfo, Sink, Source, SourceDescriptor, SourceError, SyncItem,
    WriteOp,
};
use sqlx::{PgConnection, PgPool};
use tokio::sync::{Notify, oneshot};

/// An adapter that parks inside `sync` until it is let go -- standing in for a
/// remote system that has not answered yet.
struct Parked {
    id: String,
    /// Fired once the adapter is inside `sync`, so the test never races it.
    inside: Arc<Notify>,
    release: tokio::sync::Mutex<Option<oneshot::Receiver<()>>>,
}

#[async_trait]
impl Source for Parked {
    fn descriptor(&self) -> SourceDescriptor {
        SourceDescriptor {
            id: self.id.clone(),
            adapter_kind: "parked".into(),
            name: "Parked".into(),
            capabilities: Vec::new(),
            adapter_version: "0.1.0".into(),
            auth_methods: Vec::new(),
            write_ops: Vec::new(),
            entity_kinds: vec![KindInfo {
                id: "ticket".into(),
                label: "Ticket".into(),
                plural: "Tickets".into(),
                monogram: "PA".into(),
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
        _cursor: Option<Cursor>,
        sink: &mut (dyn Sink + Send),
    ) -> Result<Cursor, SourceError> {
        // One item first, so the run has really opened its transaction and
        // written through it before the test looks at the pool: a run that had
        // not started yet would prove nothing.
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

        self.inside.notify_waiters();
        if let Some(release) = self.release.lock().await.take() {
            let _ = release.await;
        }
        Ok(r#"{"v":1,"n":1}"#.to_owned())
    }

    async fn write(&self, _op: WriteOp) -> Result<(), SourceError> {
        Err(SourceError::protocol("read-only"))
    }
}

/// An adapter that fails after writing, so the run's failure path is exercised
/// with the lock genuinely taken.
struct Failing {
    id: String,
}

#[async_trait]
impl Source for Failing {
    fn descriptor(&self) -> SourceDescriptor {
        SourceDescriptor {
            id: self.id.clone(),
            ..Parked {
                id: self.id.clone(),
                inside: Arc::new(Notify::new()),
                release: tokio::sync::Mutex::new(None),
            }
            .descriptor()
        }
    }
    async fn test_connection(&self) -> Result<ConnectionInfo, SourceError> {
        Ok(ConnectionInfo::default())
    }
    async fn sync(
        &self,
        _cursor: Option<Cursor>,
        _sink: &mut (dyn Sink + Send),
    ) -> Result<Cursor, SourceError> {
        Err(SourceError::Unreachable("simulated".into()))
    }
    async fn write(&self, _op: WriteOp) -> Result<(), SourceError> {
        Err(SourceError::protocol("read-only"))
    }
}

fn unique() -> String {
    format!("ded-{}", uuid::Uuid::new_v4().simple())
}

async fn configure(pool: &PgPool, id: &str) {
    sqlx::query(
        "insert into knobas.source_config (id, kind, display_name, base_url, auth_kind)
         values ($1, 'parked', 'Parked', '', 'none')",
    )
    .bind(id)
    .execute(pool)
    .await
    .unwrap();
}

async fn dedicated() -> PgConnection {
    knobas_db::test_util::test_connector()
        .await
        .connect()
        .await
        .expect("a connection outside every pool")
}

/// The property, measured rather than asserted from the source: while a run's
/// adapter is parked on the network, the pool that run was **handed** still
/// answers.
///
/// Two details carry the whole test, and getting either wrong makes it pass for
/// the wrong reason.
///
/// * The probe is against the *same* pool the run was given. An earlier draft
///   probed a second pool and stayed green when the run was moved back onto a
///   pooled transaction -- it was measuring a pool nothing was competing for.
/// * That pool has exactly **one** connection. It stands in for "the
///   application's five, all busy" without needing five slow queries: a run
///   that took a pooled connection would hold the only one for the length of
///   the park, and the probe would wait out sqlx' acquire timeout.
///
/// So a `Host::Pool` run fails here, which is what makes this the §10.6(c)
/// assertion rather than a description of it.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_parked_run_holds_no_connection_from_the_application_pool() {
    let connector = knobas_db::test_util::test_connector().await;
    let setup = connector.pool(2).await.unwrap();
    knobas_db::migrate::run(&setup).await.unwrap();
    let id = unique();
    configure(&setup, &id).await;

    // The pool the run is handed, and the pool the probe measures: one
    // connection, which the run must not be holding.
    let app_pool = connector.pool(1).await.unwrap();

    let inside = Arc::new(Notify::new());
    let waiting = inside.notified();
    let (release, released) = oneshot::channel();
    let source = Parked {
        id: id.clone(),
        inside: Arc::clone(&inside),
        release: tokio::sync::Mutex::new(Some(released)),
    };

    let activity_pool = app_pool.clone();
    let mut conn = dedicated().await;
    let run = tokio::spawn(async move {
        knobas_sync::run_from_stored_cursor(&mut conn, &activity_pool, &source).await
    });

    // The adapter is now inside `sync`, with the run's transaction open and its
    // advisory lock taken.
    tokio::time::timeout(std::time::Duration::from_secs(10), waiting)
        .await
        .expect("the adapter reached its park");

    let probe = tokio::time::timeout(
        std::time::Duration::from_secs(3),
        sqlx::query_as::<_, (i32,)>("select 1").fetch_one(&app_pool),
    )
    .await;
    assert!(
        matches!(probe, Ok(Ok((1,)))),
        "the application pool must still answer while a run is parked on the \
         network -- that is §10.6(c). Got {probe:?}"
    );

    release.send(()).unwrap();
    let report = run.await.unwrap().unwrap();
    assert_eq!(report.upserted, 1);

    app_pool.close().await;
    setup.close().await;
}

/// The other half, and the bug that shape hides: dropping a `Transaction` only
/// *queues* a `ROLLBACK`. A pool flushes it when the connection goes back; a
/// run's own connection has no pool to do that, so a failed run left
/// `pg_advisory_xact_lock` held and the next run of the same source blocked for
/// ever.
///
/// Found by writing this test, not by reading the code.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_failed_run_releases_the_source_lock_before_it_returns() {
    let connector = knobas_db::test_util::test_connector().await;
    let pool = connector.pool(2).await.unwrap();
    knobas_db::migrate::run(&pool).await.unwrap();
    let id = unique();
    configure(&pool, &id).await;

    let mut conn = dedicated().await;
    let failed = knobas_sync::run_from_stored_cursor(&mut conn, &pool, &Failing { id: id.clone() })
        .await
        .expect_err("the adapter is unreachable");
    assert!(matches!(
        failed,
        knobas_sync::SyncError::Source(SourceError::Unreachable(_))
    ));

    // Whoever holds the lock, it is not this connection any more: ask for it
    // from somewhere else, without waiting.
    let mut other = dedicated().await;
    let (got,): (bool,) = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        sqlx::query_as("select pg_try_advisory_lock(hashtext($1::text))")
            .bind(&id)
            .fetch_one(&mut other),
    )
    .await
    .expect("asking for the lock must not block")
    .unwrap();
    assert!(
        got,
        "a failed run left the source's advisory lock held; the next run of \
         this source would block for ever"
    );

    pool.close().await;
}

/// Same rule for the refusal that happens *inside* the lock and before the
/// adapter is ever called. This is the path that hung first.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_refused_run_releases_the_lock_too() {
    let connector = knobas_db::test_util::test_connector().await;
    let pool = connector.pool(2).await.unwrap();
    knobas_db::migrate::run(&pool).await.unwrap();
    let id = unique();

    let mut conn = dedicated().await;
    // No configuration row: refused under the lock.
    let refused = knobas_sync::run_from_stored_cursor(
        &mut conn,
        &pool,
        &Parked {
            id: id.clone(),
            inside: Arc::new(Notify::new()),
            release: tokio::sync::Mutex::new(None),
        },
    )
    .await
    .expect_err("an unconfigured source is refused");
    assert!(matches!(
        refused,
        knobas_sync::SyncError::NotConfigured { .. }
    ));

    // Asked from **another** session, deliberately. PostgreSQL's advisory locks
    // are re-entrant within a session, so `pg_try_advisory_lock` on `conn`
    // itself would succeed whether or not `conn` still held the lock -- an
    // earlier draft did exactly that and stayed green when the explicit
    // rollback was removed.
    configure(&pool, &id).await;
    let mut other = dedicated().await;
    let (got,): (bool,) = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        sqlx::query_as("select pg_try_advisory_lock(hashtext($1::text))")
            .bind(&id)
            .fetch_one(&mut other),
    )
    .await
    .expect("asking for the lock must not block")
    .unwrap();
    assert!(got, "the refused run left its lock held");

    // And the connection is reusable: a run on it goes straight through
    // instead of blocking behind its own unsent rollback.
    let (probe,): (i32,) = sqlx::query_as("select 1")
        .fetch_one(&mut conn)
        .await
        .unwrap();
    assert_eq!(probe, 1);

    pool.close().await;
}
