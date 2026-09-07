//! The backfill: a deliberate cursor-less run whose purpose is re-fetching
//! unchanged items after the *fetched payload* widened (CONTEXT.md, and issue
//! #32's third part).
//!
//! Why it needs an entry point of its own, and is not just "clear the cursor
//! and let the scheduler run":
//!
//! 1. **An incremental run cannot repair the mirror.** It re-fetches only what
//!    *changed upstream*, and a payload widening changes nothing upstream. An
//!    issue nobody has touched since keeps whatever the narrow query stored,
//!    for ever. That is the first test here, and it is the whole reason #32
//!    has a third part at all.
//! 2. **A cursor-less run sweeps, and a backfill must not.** The engine
//!    tombstones every row of an exhaustive kind that a `cursor: None` run did
//!    not return (ADR-0003), which is right for a source's *first* sync and
//!    wrong for a backfill: an operator asking for a wider payload is not
//!    asking to reconcile deletions, and the two failure modes are not
//!    comparable. A source whose credential has quietly narrowed answers with
//!    a smaller corpus and no error at all -- there is nothing for an error
//!    classification to catch -- so a sweeping backfill tombstones every issue
//!    the credential can no longer see. A stale row is the cheap error; a
//!    tombstoned corpus is not, and it is the *same* trade the engine already
//!    makes for a full sync that emitted nothing.
//!
//! So a backfill hands the adapter `None` and takes the sweep away. It still
//! writes the position the run came back with, so the next scheduled run is
//! incremental again.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use knobas_source::{
    Capability, ConnectionInfo, Cursor, KindInfo, Sink, Source, SourceDescriptor, SourceError,
    SyncItem, WriteOp,
};
use sqlx::PgPool;

/// The Jira shape, reduced to what this file is about.
///
/// A full run (`cursor: None`) emits the whole corpus; an incremental run
/// emits only the keys currently marked as changed. Every item carries
/// `payload_version`, which stands in for "how wide the `fields=` list was
/// when this record was fetched" -- widening it is a change to the *query*,
/// invisible to the source's own notion of what has been touched.
struct Widening {
    id: String,
    corpus: Arc<Mutex<Vec<&'static str>>>,
    changed: Arc<Mutex<Vec<&'static str>>>,
    payload_version: Arc<Mutex<u32>>,
    /// Every cursor the adapter was handed, in order.
    seen: Arc<Mutex<Vec<Option<Cursor>>>>,
}

#[async_trait]
impl Source for Widening {
    fn descriptor(&self) -> SourceDescriptor {
        SourceDescriptor {
            id: self.id.clone(),
            adapter_kind: "widening".into(),
            name: "Widening".into(),
            capabilities: Vec::<Capability>::new(),
            adapter_version: "0.1.0".into(),
            auth_methods: Vec::new(),
            accepts_account: false,
            write_ops: Vec::new(),
            // Jira's `ticket` declares `true`, which is exactly what makes a
            // cursor-less run over this source a sweeping one.
            entity_kinds: vec![KindInfo {
                id: "ticket".into(),
                label: "Ticket".into(),
                plural: "Tickets".into(),
                monogram: "WI".into(),
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
        let version = *self.payload_version.lock().unwrap();
        let keys: Vec<&'static str> = if cursor.is_none() {
            self.corpus.lock().unwrap().clone()
        } else {
            let corpus = self.corpus.lock().unwrap().clone();
            self.changed
                .lock()
                .unwrap()
                .iter()
                .filter(|k| corpus.contains(k))
                .copied()
                .collect()
        };
        for key in &keys {
            sink.item(SyncItem {
                entity: knobas_core::entity::EntityRef::new(&self.id, key),
                kind: "ticket".into(),
                title: format!("{key} title"),
                body_text: String::new(),
                author: None,
                updated_at: None,
                payload: serde_json::json!({ "key": key, "payload_version": version }),
                web_url: None,
                deleted: false,
            })
            .await?;
        }
        if keys.is_empty() {
            return Ok(cursor.unwrap_or_default());
        }
        Ok(format!(
            r#"{{"v":1,"n":{}}}"#,
            self.seen.lock().unwrap().len()
        ))
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

struct Handles {
    corpus: Arc<Mutex<Vec<&'static str>>>,
    changed: Arc<Mutex<Vec<&'static str>>>,
    payload_version: Arc<Mutex<u32>>,
    seen: Arc<Mutex<Vec<Option<Cursor>>>>,
}

fn source(id: &str, corpus: &[&'static str]) -> (Widening, Handles) {
    let h = Handles {
        corpus: Arc::new(Mutex::new(corpus.to_vec())),
        changed: Arc::new(Mutex::new(Vec::new())),
        payload_version: Arc::new(Mutex::new(1)),
        seen: Arc::new(Mutex::new(Vec::new())),
    };
    (
        Widening {
            id: id.to_owned(),
            corpus: Arc::clone(&h.corpus),
            changed: Arc::clone(&h.changed),
            payload_version: Arc::clone(&h.payload_version),
            seen: Arc::clone(&h.seen),
        },
        h,
    )
}

async fn pool() -> PgPool {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    pool
}

/// A connection of this run's own -- interfaces §10.6(c), the same requirement
/// `run_from_stored_cursor` carries.
async fn dedicated() -> sqlx::PgConnection {
    knobas_db::test_util::test_connector()
        .await
        .connect()
        .await
        .expect("a connection outside every pool")
}

fn unique() -> String {
    format!("bkf-{}", uuid::Uuid::new_v4().simple())
}

async fn configure(pool: &PgPool, id: &str) {
    sqlx::query(
        "insert into knobas.source_config (id, kind, display_name, base_url, auth_kind)
         values ($1, 'widening', 'Widening', '', 'none')",
    )
    .bind(id)
    .execute(pool)
    .await
    .unwrap();
}

/// What the mirror is holding for one entity, which is the thing a payload
/// widening is about.
async fn stored_version(pool: &PgPool, entity_id: &str) -> Option<u64> {
    let (payload,): (serde_json::Value,) =
        sqlx::query_as("select payload from sync.item where entity_id = $1")
            .bind(entity_id)
            .fetch_one(pool)
            .await
            .unwrap();
    payload["payload_version"].as_u64()
}

async fn stored_cursor(pool: &PgPool, id: &str) -> Option<String> {
    let (cursor,): (Option<String>,) =
        sqlx::query_as("select cursor from knobas.source_config where id = $1")
            .bind(id)
            .fetch_one(pool)
            .await
            .unwrap();
    cursor
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

/// A second of daylight, so `synced_at` is unmistakably older than the next
/// run's transaction timestamp even at coarse clock resolution.
async fn a_moment_passes() {
    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
}

/// **M2 exit criterion 6, and #32's "done when": an untouched issue carries
/// the widened payload after the backfill.**
///
/// The three-run shape is the point, and no shorter sequence can show it. The
/// middle run is the control: it proves the scheduled path *cannot* repair the
/// mirror, so the backfill is not merely a faster way of doing what would have
/// happened anyway. Drop it and the test still passes with `run_backfill`
/// aliased to `run_from_stored_cursor`.
#[tokio::test]
async fn a_backfill_rewrites_the_payload_of_an_item_nobody_touched() {
    let pool = pool().await;
    let id = unique();
    configure(&pool, &id).await;
    let (src, h) = source(&id, &["A-1", "A-2", "A-3"]);

    // 1. The mirror is filled by the narrow query.
    let mut conn = dedicated().await;
    knobas_sync::run_from_stored_cursor(&mut conn, &pool, &src)
        .await
        .unwrap();
    for key in ["A-1", "A-2", "A-3"] {
        assert_eq!(stored_version(&pool, &format!("{id}:{key}")).await, Some(1));
    }

    // 2. The query widens, and one issue happens to be edited upstream. The
    //    scheduled run brings back that issue and nothing else.
    *h.payload_version.lock().unwrap() = 2;
    *h.changed.lock().unwrap() = vec!["A-2"];
    let incremental = knobas_sync::run_from_stored_cursor(&mut conn, &pool, &src)
        .await
        .unwrap();
    assert_eq!(incremental.upserted, 1);
    assert_eq!(stored_version(&pool, &format!("{id}:A-2")).await, Some(2));
    assert_eq!(
        stored_version(&pool, &format!("{id}:A-1")).await,
        Some(1),
        "an untouched item keeps the narrow payload -- this is the defect #32 exists to fix, \
         and it is not repaired by waiting"
    );
    assert_eq!(stored_version(&pool, &format!("{id}:A-3")).await, Some(1));

    // 3. The backfill.
    let backfill = knobas_sync::run_backfill(&mut conn, &pool, &src)
        .await
        .unwrap();
    assert_eq!(backfill.upserted, 3, "the whole corpus is re-fetched");
    for key in ["A-1", "A-2", "A-3"] {
        assert_eq!(
            stored_version(&pool, &format!("{id}:{key}")).await,
            Some(2),
            "{key} still carries the narrow payload after the backfill"
        );
    }

    // It is a cursor-less run *at the adapter*, and only the third one is.
    let seen = h.seen.lock().unwrap().clone();
    assert_eq!(seen.len(), 3);
    assert!(
        seen[0].is_none(),
        "the first sync has no position to resume"
    );
    assert!(
        seen[1].is_some(),
        "the middle run must resume, or it is not the control it claims to be"
    );
    assert!(
        seen[2].is_none(),
        "a backfill hands the adapter no position, whatever is stored: {seen:?}"
    );
}

/// The backfill leaves the source *incremental* again.
///
/// A backfill that cleared the stored cursor -- the obvious way to spell
/// "run without one" -- would make the next scheduled run a full sync too, and
/// a sweeping one at that. The position the run came back with is written
/// exactly as any other run's is.
#[tokio::test]
async fn a_backfill_stores_the_position_it_came_back_with() {
    let pool = pool().await;
    let id = unique();
    configure(&pool, &id).await;
    let (src, h) = source(&id, &["B-1", "B-2"]);
    let mut conn = dedicated().await;

    knobas_sync::run_from_stored_cursor(&mut conn, &pool, &src)
        .await
        .unwrap();
    let after_first = stored_cursor(&pool, &id).await;
    assert!(after_first.is_some());

    let report = knobas_sync::run_backfill(&mut conn, &pool, &src)
        .await
        .unwrap();
    assert_eq!(stored_cursor(&pool, &id).await, Some(report.cursor.clone()));
    assert_ne!(
        stored_cursor(&pool, &id).await,
        after_first,
        "the run reported a new position and it was stored"
    );

    // ...and the next scheduled run resumes from it.
    knobas_sync::run_from_stored_cursor(&mut conn, &pool, &src)
        .await
        .unwrap();
    let seen = h.seen.lock().unwrap().clone();
    assert!(seen[1].is_none(), "the backfill itself resumed nothing");
    assert_eq!(
        seen[2].as_deref(),
        Some(report.cursor.as_str()),
        "the run after a backfill is incremental: {seen:?}"
    );
}

/// **The tombstone trap.** `ticket` declares `full_sync_exhaustive`, so a
/// plain cursor-less run over this source sweeps -- and a backfill over the
/// same source, with the same corpus, must not.
///
/// Both halves in one test, because only the pair is evidence: the sweeping
/// half is what says the corpus really did shrink in a way the engine would
/// have acted on, so the backfill's `0` is a decision and not an accident of
/// the fixture.
#[tokio::test]
async fn a_backfill_never_sweeps_where_a_plain_full_sync_would() {
    let pool = pool().await;
    let id = unique();
    configure(&pool, &id).await;
    let (src, h) = source(&id, &["C-1", "C-2", "C-3"]);
    let mut conn = dedicated().await;

    knobas_sync::run_from_stored_cursor(&mut conn, &pool, &src)
        .await
        .unwrap();

    // The credential quietly loses sight of C-3. Upstream said nothing: this
    // is a smaller 200, not an error, so nothing in any error classification
    // can see it.
    *h.corpus.lock().unwrap() = vec!["C-1", "C-2"];
    a_moment_passes().await;

    let backfill = knobas_sync::run_backfill(&mut conn, &pool, &src)
        .await
        .unwrap();
    assert_eq!(backfill.upserted, 2);
    assert_eq!(
        backfill.swept, 0,
        "a backfill re-fetches payloads; it does not reconcile deletions"
    );
    assert!(
        deleted_at(&pool, &format!("{id}:C-3")).await.is_none(),
        "the backfill tombstoned an item the operator never asked it to judge"
    );

    // The same corpus, through a run that *is* asked to reconcile.
    a_moment_passes().await;
    let full = knobas_sync::run_once(&pool, &src, None).await.unwrap();
    assert_eq!(
        full.swept, 1,
        "if this is 0 the shrink never happened and the assertion above proves nothing"
    );
    assert!(deleted_at(&pool, &format!("{id}:C-3")).await.is_some());
}

/// A backfill needs somewhere to store the position it comes back with, so an
/// unconfigured source is refused for the same reason
/// [`knobas_sync::run_from_stored_cursor`] refuses one -- and refused *before*
/// the source is read, so a mistyped id costs no network traffic.
#[tokio::test]
async fn a_backfill_of_an_unconfigured_source_is_refused_without_reading_it() {
    let pool = pool().await;
    let id = unique();
    let (src, h) = source(&id, &["D-1"]);
    let mut conn = dedicated().await;

    let failed = knobas_sync::run_backfill(&mut conn, &pool, &src)
        .await
        .expect_err("no source_config row");
    assert!(
        matches!(failed, knobas_sync::SyncError::NotConfigured { .. }),
        "{failed:?}"
    );
    assert!(
        h.seen.lock().unwrap().is_empty(),
        "the adapter must not be called for a source with nowhere to store a cursor"
    );
}
