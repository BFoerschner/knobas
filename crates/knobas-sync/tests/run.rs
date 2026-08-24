//! One sync run, against a real PostgreSQL.
//!
//! `test_util` gives every test in this binary the *same* database, and they
//! run concurrently, so each test below syncs a source id unique to itself.
//! Only the mock's own test uses `mock`, which is why it may assert absolute
//! row counts for that source.

use std::sync::Mutex;

use knobas_core::entity::EntityRef;
use knobas_source::{
    Cursor, KindInfo, Sink, Source, SourceDescriptor, SourceError, SyncItem, WriteOp,
};
use knobas_source_mock::MockSource;
use sqlx::PgPool;
use uuid::Uuid;

#[tokio::test]
async fn mock_sync_lands_in_postgres_and_is_searchable() {
    let pool = knobas_db::test_util::test_pool().await;
    let pool = &pool;
    knobas_db::migrate::run(pool).await.unwrap();

    let src = MockSource::new();
    let report = knobas_sync::run_once(pool, &src, None).await.unwrap();
    assert!(
        report.upserted > 10,
        "expected the full fixture, got {}",
        report.upserted
    );

    // idempotent: second full run upserts the same rows, no dupes
    let again = knobas_sync::run_once(pool, &src, None).await.unwrap();
    assert_eq!(report.upserted, again.upserted);
    let (cnt,): (i64,) = sqlx::query_as("select count(*) from sync.item where source_id = 'mock'")
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(cnt as u64, report.upserted);

    // incremental from the cursor is a no-op
    let inc = knobas_sync::run_once(pool, &src, Some(report.cursor.clone()))
        .await
        .unwrap();
    assert_eq!(inc.upserted, 0);

    // and the synced corpus answers FTS
    let hits = knobas_db::search::search(pool, "sepa retry", 10)
        .await
        .unwrap();
    assert!(
        hits.iter().any(|h| h.entity_id == "mock:PAY-231"),
        "hits: {hits:?}"
    );

    // sync wrote an activity line
    let acts = knobas_core::activity::recent(pool, 50).await.unwrap();
    assert!(
        acts.iter()
            .any(|a| a.actor == "sync:mock" && a.verb == "synced")
    );
    // ...one per run that changed something, and none for the no-op: a
    // five-minute scheduler must not bury the log in "synced nothing".
    let (lines,): (i64,) =
        sqlx::query_as("select count(*) from knobas.activity where actor = 'sync:mock'")
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(
        lines, 2,
        "two runs changed something, the incremental did not"
    );
}

/// An adapter under the test's control: it emits exactly the items it is given
/// and, if asked, falls over after having emitted them.
struct FakeSource {
    id: String,
    items: Vec<SyncItem>,
    /// Report a failure *after* pushing everything, to prove that items
    /// already accepted by the sink are rolled back with the run.
    fail_at_end: bool,
}

impl FakeSource {
    fn new(id: &str, items: Vec<SyncItem>) -> Self {
        Self {
            id: id.to_owned(),
            items,
            fail_at_end: false,
        }
    }

    fn failing(id: &str, items: Vec<SyncItem>) -> Self {
        Self {
            fail_at_end: true,
            ..Self::new(id, items)
        }
    }
}

#[async_trait::async_trait]
impl Source for FakeSource {
    fn descriptor(&self) -> SourceDescriptor {
        SourceDescriptor {
            id: self.id.clone(),
            adapter_kind: "fake".to_owned(),
            name: "Fake".to_owned(),
            // P12: `Search` means server-side search, which this fake has no
            // entry point for; it is read-only, so it declares nothing.
            capabilities: Vec::new(),
            adapter_version: "0.1.0".to_owned(),
            auth_methods: Vec::new(),
            write_ops: Vec::new(),
            entity_kinds: vec![KindInfo {
                id: "ticket".to_owned(),
                label: "Ticket".to_owned(),
                plural: "Tickets".to_owned(),
                monogram: "TK".to_owned(),
            }],
            // Everything it is given, every run: the engine's sweep may
            // tombstone what it stops emitting.
            full_sync_exhaustive: true,
            config_schema: serde_json::json!({ "type": "object", "properties": {} }),
        }
    }

    async fn test_connection(&self) -> Result<knobas_source::ConnectionInfo, SourceError> {
        // A fake that connects to nothing: `Default` is exactly "connected,
        // nothing to report".
        Ok(knobas_source::ConnectionInfo::default())
    }

    async fn sync(
        &self,
        _cursor: Option<Cursor>,
        sink: &mut (dyn Sink + Send),
    ) -> Result<Cursor, SourceError> {
        for item in &self.items {
            sink.item(item.clone()).await?;
        }
        if self.fail_at_end {
            return Err(SourceError::Unreachable("simulated: gave up".to_owned()));
        }
        Ok(format!("at-{}", self.items.len()))
    }

    async fn write(&self, _op: WriteOp) -> Result<(), SourceError> {
        Err(SourceError::Protocol("read-only".to_owned()))
    }
}

/// An adapter that looks the engine's transaction up in `pg_locks` from a
/// *second* connection while its own sync is still running.
///
/// The advisory lock is only observable from outside for as long as the run
/// holds it, and a run is over by the time `run_once` returns -- so the check
/// has to happen from inside the sync itself.
struct LockProbingSource {
    id: String,
    pool: PgPool,
    /// Whether the source's advisory lock was held mid-run.
    held: Mutex<Option<bool>>,
}

#[async_trait::async_trait]
impl Source for LockProbingSource {
    fn descriptor(&self) -> SourceDescriptor {
        FakeSource::new(&self.id, Vec::new()).descriptor()
    }

    async fn test_connection(&self) -> Result<knobas_source::ConnectionInfo, SourceError> {
        // A fake that connects to nothing: `Default` is exactly "connected,
        // nothing to report".
        Ok(knobas_source::ConnectionInfo::default())
    }

    async fn sync(
        &self,
        _cursor: Option<Cursor>,
        sink: &mut (dyn Sink + Send),
    ) -> Result<Cursor, SourceError> {
        sink.item(item(&self.id, "TIDE-11", "locked", false))
            .await?;
        let held = advisory_lock_held(&self.pool, &self.id).await;
        *self.held.lock().unwrap() = Some(held);
        Ok("locked".to_owned())
    }

    async fn write(&self, _op: WriteOp) -> Result<(), SourceError> {
        Err(SourceError::Protocol("read-only".to_owned()))
    }
}

/// Whether anyone holds the transaction-scoped advisory lock for `source_id`.
///
/// `pg_advisory_xact_lock(bigint)` records the key split across `classid`
/// (high 32 bits) and `objid` (low 32), with `objsubid = 1`; matching on the
/// exact key is what keeps this from seeing a *concurrent* test's lock.
async fn advisory_lock_held(pool: &PgPool, source_id: &str) -> bool {
    let (held,): (bool,) = sqlx::query_as(
        r#"select exists(
             select 1
               from pg_locks l, (select hashtext($1::text)::bigint as k) x
              where l.locktype = 'advisory'
                and l.objsubid = 1
                and l.classid = ((x.k >> 32) & 4294967295)::oid
                and l.objid   = (x.k & 4294967295)::oid
                and l.granted
           )"#,
    )
    .bind(source_id)
    .fetch_one(pool)
    .await
    .unwrap();
    held
}

/// One item in `source`'s namespace.
fn item(source: &str, key: &str, title: &str, deleted: bool) -> SyncItem {
    SyncItem {
        entity: EntityRef::new(source, key),
        kind: "ticket".to_owned(),
        title: title.to_owned(),
        body_text: format!("{title} body"),
        author: Some("mara".to_owned()),
        updated_at: None,
        payload: serde_json::json!({ "key": key }),
        web_url: None,
        deleted,
    }
}

/// `n` distinct items in `source`'s namespace.
fn many(source: &str, n: usize) -> Vec<SyncItem> {
    (0..n)
        .map(|i| item(source, &format!("BULK-{i}"), &format!("bulk {i}"), false))
        .collect()
}

/// A migrated pool, and a source id no other test uses.
async fn fixture() -> (PgPool, String) {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    let id = format!("fake-{}", Uuid::new_v4());
    (pool, id)
}

/// Give `id` a configured source, so its cursor has somewhere to land.
async fn configure(pool: &PgPool, id: &str) {
    sqlx::query(
        r#"insert into knobas.source_config
               (id, kind, display_name, base_url, auth_kind, cursor)
           values ($1, 'fake', 'Fake', 'http://localhost', 'none', 'before')"#,
    )
    .bind(id)
    .execute(pool)
    .await
    .unwrap();
}

async fn cursor_of(pool: &PgPool, id: &str) -> Option<String> {
    let (cursor,): (Option<String>,) =
        sqlx::query_as("select cursor from knobas.source_config where id = $1")
            .bind(id)
            .fetch_one(pool)
            .await
            .unwrap();
    cursor
}

async fn deleted_at(pool: &PgPool, entity: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    let (at,): (Option<chrono::DateTime<chrono::Utc>>,) =
        sqlx::query_as("select deleted_at from knobas.entity where id = $1")
            .bind(entity)
            .fetch_one(pool)
            .await
            .unwrap();
    at
}

/// The adapter's `web_url` reaches the mirror, or *Open in browser* has
/// nothing to open (interfaces §2.5, P5).
#[tokio::test]
async fn the_mirror_stores_the_item_web_url() {
    let (pool, id) = fixture().await;
    let with_url = SyncItem {
        web_url: Some("https://tidewater.example/browse/TIDE-9".to_owned()),
        ..item(&id, "TIDE-9", "has a page", false)
    };
    knobas_sync::run_once(&pool, &FakeSource::new(&id, vec![with_url]), None)
        .await
        .unwrap();

    let (stored,): (Option<String>,) =
        sqlx::query_as("select web_url from sync.item where entity_id = $1")
            .bind(format!("{id}:TIDE-9"))
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        stored.as_deref(),
        Some("https://tidewater.example/browse/TIDE-9")
    );

    // An adapter that stops reporting one clears it, exactly like the title:
    // the mirror is refreshed wholesale, never merged.
    knobas_sync::run_once(
        &pool,
        &FakeSource::new(&id, vec![item(&id, "TIDE-9", "has a page", false)]),
        None,
    )
    .await
    .unwrap();
    let (stored,): (Option<String>,) =
        sqlx::query_as("select web_url from sync.item where entity_id = $1")
            .bind(format!("{id}:TIDE-9"))
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(stored, None);
}

/// A remote deletion tombstones the entity without dropping it -- links point
/// at it -- keeps the first deletion's timestamp across repeated runs, and is
/// undone when the source hands the item back.
#[tokio::test]
async fn a_deleted_item_tombstones_its_entity_and_can_come_back() {
    let (pool, id) = fixture().await;
    let gone = format!("{id}:TIDE-2");

    let live = vec![
        item(&id, "TIDE-1", "stays", false),
        item(&id, "TIDE-2", "vanishes", false),
    ];
    let report = knobas_sync::run_once(&pool, &FakeSource::new(&id, live.clone()), None)
        .await
        .unwrap();
    assert_eq!((report.upserted, report.deleted), (2, 0));
    assert_eq!(deleted_at(&pool, &gone).await, None);

    let with_deletion = vec![live[0].clone(), item(&id, "TIDE-2", "vanishes", true)];
    let report = knobas_sync::run_once(&pool, &FakeSource::new(&id, with_deletion.clone()), None)
        .await
        .unwrap();
    // The tombstoned item is still written -- `deleted` counts a subset of
    // `upserted` -- so the UI keeps its last-known title.
    assert_eq!((report.upserted, report.deleted), (2, 1));
    let first = deleted_at(&pool, &gone).await.expect("tombstoned");
    let (title,): (String,) = sqlx::query_as("select title from sync.item where entity_id = $1")
        .bind(&gone)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(title, "vanishes");

    // syncing the deletion again does not restamp it
    knobas_sync::run_once(&pool, &FakeSource::new(&id, with_deletion), None)
        .await
        .unwrap();
    assert_eq!(deleted_at(&pool, &gone).await, Some(first));

    // and the item coming back clears the tombstone
    knobas_sync::run_once(&pool, &FakeSource::new(&id, live), None)
        .await
        .unwrap();
    assert_eq!(deleted_at(&pool, &gone).await, None);
}

/// The cursor is stored for a configured source, and only for one: a run
/// against a source with no configuration row must not invent one.
#[tokio::test]
async fn the_cursor_lands_in_an_existing_source_config_only() {
    let (pool, id) = fixture().await;
    configure(&pool, &id).await;
    assert_eq!(cursor_of(&pool, &id).await.as_deref(), Some("before"));

    let src = FakeSource::new(&id, vec![item(&id, "TIDE-9", "cursor", false)]);
    let report = knobas_sync::run_once(&pool, &src, None).await.unwrap();
    assert_eq!(report.cursor, "at-1");
    assert_eq!(cursor_of(&pool, &id).await, Some(report.cursor));

    let (_, unconfigured) = fixture().await;
    let src = FakeSource::new(
        &unconfigured,
        vec![item(&unconfigured, "TIDE-9", "x", false)],
    );
    knobas_sync::run_once(&pool, &src, None).await.unwrap();
    let (rows,): (i64,) = sqlx::query_as("select count(*) from knobas.source_config where id = $1")
        .bind(&unconfigured)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(rows, 0, "a sync must not configure a source for itself");
}

/// A run is all-or-nothing: an adapter that fails *after a batch has already
/// been written* leaves no rows, no cursor and no activity line behind.
///
/// The item count crosses `BATCH` deliberately. Below it nothing is ever sent
/// to the database before the failure, so the test would pass on an engine with
/// no transaction at all.
#[tokio::test]
async fn an_adapter_failure_rolls_the_whole_run_back() {
    let (pool, id) = fixture().await;
    configure(&pool, &id).await;

    let items = many(&id, knobas_sync::BATCH + 1);
    assert!(
        items.len() > knobas_sync::BATCH,
        "must force a mid-run flush"
    );
    let src = FakeSource::failing(&id, items);
    let err = knobas_sync::run_once(&pool, &src, None).await.unwrap_err();
    assert!(
        matches!(
            err,
            knobas_sync::SyncError::Source(SourceError::Unreachable(_))
        ),
        "{err:?}"
    );

    assert_eq!(
        rows_for(&pool, &id).await,
        0,
        "the flushed batch rolled back"
    );
    assert_eq!(entities_for(&pool, &id).await, 0);
    assert_eq!(cursor_of(&pool, &id).await.as_deref(), Some("before"));
    let (acts,): (i64,) = sqlx::query_as("select count(*) from knobas.activity where actor = $1")
        .bind(format!("sync:{id}"))
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(acts, 0);
}

/// Entity ids are global, so a source may only write its own namespace --
/// otherwise one adapter could overwrite another's rows.
///
/// Probed by entity id, not by `source_id`: the stray row would have been
/// written with *this* source's id in the `source_id` column, so counting by
/// `somebody-else` would be zero whether the guard works or not.
#[tokio::test]
async fn an_item_outside_the_sources_namespace_is_refused() {
    let (pool, id) = fixture().await;

    let stray = item("somebody-else", "TIDE-4", "not yours", false);
    let src = FakeSource::new(&id, vec![stray]);
    let err = knobas_sync::run_once(&pool, &src, None).await.unwrap_err();
    assert!(
        matches!(err, knobas_sync::SyncError::Source(SourceError::Sink(_))),
        "{err:?}"
    );
    assert!(!entity_exists(&pool, "somebody-else:TIDE-4").await);
    assert!(!mirror_exists(&pool, "somebody-else:TIDE-4").await);
    assert_eq!(rows_for(&pool, &id).await, 0);
}

/// An id `EntityRef::parse` would reject, and a kind the descriptor never
/// declared, are both refused: `EntityRef::new` validates nothing, and the SPI
/// battery only proves an adapter *was* well-behaved when it was last tested.
#[tokio::test]
async fn an_unaddressable_id_or_undeclared_kind_is_refused() {
    let (pool, id) = fixture().await;

    let blank_key = SyncItem {
        entity: EntityRef::new(&id, "   "),
        ..item(&id, "ignored", "blank key", false)
    };
    let err = knobas_sync::run_once(&pool, &FakeSource::new(&id, vec![blank_key]), None)
        .await
        .unwrap_err();
    assert!(
        matches!(err, knobas_sync::SyncError::Source(SourceError::Sink(_))),
        "{err:?}"
    );

    let undeclared = SyncItem {
        kind: "monitor".to_owned(),
        ..item(&id, "TIDE-5", "wrong kind", false)
    };
    let err = knobas_sync::run_once(&pool, &FakeSource::new(&id, vec![undeclared]), None)
        .await
        .unwrap_err();
    assert!(
        matches!(err, knobas_sync::SyncError::Source(SourceError::Sink(_))),
        "{err:?}"
    );
    assert_eq!(rows_for(&pool, &id).await, 0);
}

/// A source id that cannot be a namespace is refused before any work happens:
/// `jira:eu` would make `jira:eu:PAY-1` parse as namespace `jira`, and `note`
/// would write into the local notes' namespace -- either one defeats the
/// per-item namespace guard, which can only compare against the id it is given.
#[tokio::test]
async fn a_source_id_that_cannot_be_a_namespace_is_refused() {
    let (pool, id) = fixture().await;

    for bad in ["", "   ", "jira:eu", "note", "CTX"] {
        let src = FakeSource::new(bad, vec![item(bad, "TIDE-6", "nope", false)]);
        let err = knobas_sync::run_once(&pool, &src, None).await.unwrap_err();
        assert!(
            matches!(err, knobas_sync::SyncError::BadSourceId { .. }),
            "{bad:?}: {err:?}"
        );
    }
    // the well-formed neighbour still syncs
    knobas_sync::run_once(&pool, &FakeSource::new(&id, many(&id, 1)), None)
        .await
        .unwrap();
    assert_eq!(rows_for(&pool, &id).await, 1);
}

/// The counts end up in a durable activity line, so they have to be per entity
/// across the whole run -- not per batch. An entity re-emitted after a batch
/// boundary is still one row.
#[tokio::test]
async fn a_duplicate_across_a_batch_boundary_counts_once() {
    let (pool, id) = fixture().await;

    let mut items = many(&id, knobas_sync::BATCH);
    // The batch flushes on the 500th push, so these two land in the next one.
    items.push(items[0].clone());
    items.push(item(&id, "BULK-extra", "extra", false));

    let report = knobas_sync::run_once(&pool, &FakeSource::new(&id, items), None)
        .await
        .unwrap();
    let rows = rows_for(&pool, &id).await as u64;
    assert_eq!(rows, knobas_sync::BATCH as u64 + 1);
    assert_eq!(
        report.upserted, rows,
        "the report must count rows, not pushes"
    );
    assert_eq!(entities_for(&pool, &id).await as u64, rows);
}

/// An item the source gives no `updated_at` for keeps the timestamp already
/// stored. Restamping it to `now()` every run would make an untouched item look
/// freshly edited on every poll -- and "recently updated" is what the UI sorts
/// by.
#[tokio::test]
async fn a_re_sync_without_an_updated_at_does_not_restamp_the_entity() {
    let (pool, id) = fixture().await;
    let entity = format!("{id}:TIDE-8");

    let undated = FakeSource::new(&id, vec![item(&id, "TIDE-8", "no timestamp", false)]);
    knobas_sync::run_once(&pool, &undated, None).await.unwrap();
    let first = updated_at(&pool, &entity).await;

    knobas_sync::run_once(&pool, &undated, None).await.unwrap();
    assert_eq!(
        updated_at(&pool, &entity).await,
        first,
        "a re-sync with no source timestamp must not bump updated_at"
    );

    // but a source that *does* say when it changed still wins
    let stated = chrono::DateTime::parse_from_rfc3339("2026-08-24T09:15:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc);
    let dated = SyncItem {
        updated_at: Some(stated),
        ..item(&id, "TIDE-8", "timestamped", false)
    };
    knobas_sync::run_once(&pool, &FakeSource::new(&id, vec![dated]), None)
        .await
        .unwrap();
    assert_eq!(updated_at(&pool, &entity).await, stated);
    assert_eq!(item_updated_at(&pool, &entity).await, Some(stated));

    // and the mirror keeps it too: the two columns hold the same fact, so a
    // later undated sync must not leave them disagreeing.
    knobas_sync::run_once(&pool, &undated, None).await.unwrap();
    assert_eq!(updated_at(&pool, &entity).await, stated);
    assert_eq!(
        item_updated_at(&pool, &entity).await,
        Some(stated),
        "sync.item.item_updated_at must survive an undated re-sync too"
    );
}

/// Two pushes of one entity inside a *single* batch collapse into one upsert.
///
/// Postgres refuses an `on conflict do update` that would touch the same row
/// twice in one statement ("cannot affect row a second time"), so without the
/// dedupe this run fails outright rather than miscounting.
#[tokio::test]
async fn the_same_entity_twice_in_one_batch_is_one_upsert() {
    let (pool, id) = fixture().await;

    let items = vec![
        item(&id, "TIDE-10", "first", false),
        item(&id, "TIDE-10", "second", false),
    ];
    let report = knobas_sync::run_once(&pool, &FakeSource::new(&id, items), None)
        .await
        .unwrap();

    assert_eq!(report.upserted, 1);
    assert_eq!(rows_for(&pool, &id).await, 1);
    let (title,): (String,) = sqlx::query_as("select title from sync.item where entity_id = $1")
        .bind(format!("{id}:TIDE-10"))
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(title, "second", "the last push in a batch wins");
}

/// The run takes its source's advisory lock, so two runs of one source
/// serialise instead of interleaving their batches -- a run holds row locks
/// across every batch, which is exactly the shape that cycles.
#[tokio::test]
async fn a_run_holds_the_sources_advisory_lock() {
    let (pool, id) = fixture().await;

    assert!(
        !advisory_lock_held(&pool, &id).await,
        "nothing should hold it before the run"
    );
    let src = LockProbingSource {
        id: id.clone(),
        pool: pool.clone(),
        held: Mutex::new(None),
    };
    knobas_sync::run_once(&pool, &src, None).await.unwrap();

    assert_eq!(
        *src.held.lock().unwrap(),
        Some(true),
        "the run must hold its source's advisory lock while syncing"
    );
    assert!(
        !advisory_lock_held(&pool, &id).await,
        "and release it with the transaction"
    );
}

/// An entity that flips state across a batch boundary is still one row, and
/// the run's `deleted` count follows its *latest* write in both directions.
#[tokio::test]
async fn a_state_flip_across_a_batch_boundary_adjusts_the_deleted_count() {
    let (pool, id) = fixture().await;
    let flipper = format!("{id}:BULK-0");

    // live in the first batch, tombstoned in the second
    let mut items = many(&id, knobas_sync::BATCH);
    items.push(item(&id, "BULK-0", "bulk 0", true));
    let report = knobas_sync::run_once(&pool, &FakeSource::new(&id, items), None)
        .await
        .unwrap();
    assert_eq!(
        (report.upserted, report.deleted),
        (knobas_sync::BATCH as u64, 1)
    );
    assert!(deleted_at(&pool, &flipper).await.is_some());

    // tombstoned in the first batch, alive again in the second
    let mut items = many(&id, knobas_sync::BATCH);
    items[0] = item(&id, "BULK-0", "bulk 0", true);
    items.push(item(&id, "BULK-0", "bulk 0", false));
    let report = knobas_sync::run_once(&pool, &FakeSource::new(&id, items), None)
        .await
        .unwrap();
    assert_eq!(
        (report.upserted, report.deleted),
        (knobas_sync::BATCH as u64, 0)
    );
    assert_eq!(deleted_at(&pool, &flipper).await, None);
}

async fn rows_for(pool: &PgPool, source_id: &str) -> i64 {
    let (rows,): (i64,) = sqlx::query_as("select count(*) from sync.item where source_id = $1")
        .bind(source_id)
        .fetch_one(pool)
        .await
        .unwrap();
    rows
}

/// Entities in `source_id`'s namespace, counted without going through
/// `sync.item` -- the two tables are what a rollback has to clear together.
async fn entities_for(pool: &PgPool, source_id: &str) -> i64 {
    let (rows,): (i64,) = sqlx::query_as("select count(*) from knobas.entity where id like $1")
        .bind(format!("{source_id}:%"))
        .fetch_one(pool)
        .await
        .unwrap();
    rows
}

async fn entity_exists(pool: &PgPool, entity: &str) -> bool {
    let (found,): (bool,) =
        sqlx::query_as("select exists(select 1 from knobas.entity where id=$1)")
            .bind(entity)
            .fetch_one(pool)
            .await
            .unwrap();
    found
}

async fn mirror_exists(pool: &PgPool, entity: &str) -> bool {
    let (found,): (bool,) =
        sqlx::query_as("select exists(select 1 from sync.item where entity_id=$1)")
            .bind(entity)
            .fetch_one(pool)
            .await
            .unwrap();
    found
}

async fn item_updated_at(pool: &PgPool, entity: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    let (at,): (Option<chrono::DateTime<chrono::Utc>>,) =
        sqlx::query_as("select item_updated_at from sync.item where entity_id = $1")
            .bind(entity)
            .fetch_one(pool)
            .await
            .unwrap();
    at
}

async fn updated_at(pool: &PgPool, entity: &str) -> chrono::DateTime<chrono::Utc> {
    let (at,): (chrono::DateTime<chrono::Utc>,) =
        sqlx::query_as("select updated_at from knobas.entity where id = $1")
            .bind(entity)
            .fetch_one(pool)
            .await
            .unwrap();
    at
}

/// The activity line is written *after* the commit, so its failure cannot
/// un-write the sync -- and must not be reported as if it had.
///
/// Failing the run there tells the caller nothing landed while the entities,
/// the mirror rows and the cursor are all durable. A scheduler would retry a
/// run that already happened; the UI would show an error over data that is
/// sitting in the database. The line is a log entry: losing one is worth a
/// warning, not a lie about the outcome.
///
/// The failure is arranged with a trigger keyed on *this* run's actor, so
/// nothing else writing the shared log is disturbed.
#[tokio::test]
async fn a_failing_activity_line_does_not_fail_a_committed_sync() {
    let (pool, id) = fixture().await;
    configure(&pool, &id).await;
    let actor = format!("sync:{id}");
    let guard = format!("reject_{}", Uuid::new_v4().simple());

    sqlx::query(sqlx::AssertSqlSafe(format!(
        r#"create function knobas.{guard}() returns trigger language plpgsql as $$
           begin
             if new.actor = '{actor}' then
               raise exception 'the activity log is down';
             end if;
             return new;
           end $$"#
    )))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "create trigger {guard} before insert on knobas.activity
         for each row execute function knobas.{guard}()"
    )))
    .execute(&pool)
    .await
    .unwrap();

    let src = FakeSource::new(&id, vec![item(&id, "TIDE-12", "committed", false)]);
    let outcome = knobas_sync::run_once(&pool, &src, None).await;

    // Dropped before asserting: a failure here must not leave the shared log
    // with a trigger on it for the rest of the binary.
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "drop trigger {guard} on knobas.activity"
    )))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "drop function knobas.{guard}()"
    )))
    .execute(&pool)
    .await
    .unwrap();

    let report = outcome.expect("a committed sync must survive a failed activity line");
    assert_eq!(report.upserted, 1);
    assert_eq!(rows_for(&pool, &id).await, 1, "the run's data is committed");
    assert_eq!(cursor_of(&pool, &id).await.as_deref(), Some("at-1"));
    let (lines,): (i64,) = sqlx::query_as("select count(*) from knobas.activity where actor = $1")
        .bind(&actor)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(lines, 0, "the line is the part that did not land");
}

/// A tombstone hides the entity from search while its mirror row stays put:
/// the launcher must stop offering something that no longer exists upstream,
/// but the row is what still holds its last-known title for anything already
/// pointing at it.
#[tokio::test]
async fn a_tombstoned_item_leaves_search_but_keeps_its_mirror_row() {
    let (pool, id) = fixture().await;
    let entity = format!("{id}:TIDE-7");
    // A token no other test's corpus contains, so the query matches this item
    // alone even though every test shares one database.
    let token = format!("zq{}", Uuid::new_v4().simple());

    let live = FakeSource::new(&id, vec![item(&id, "TIDE-7", &token, false)]);
    knobas_sync::run_once(&pool, &live, None).await.unwrap();
    let hits = knobas_db::search::search(&pool, &token, 10).await.unwrap();
    assert_eq!(hits.len(), 1, "{hits:?}");
    assert_eq!(hits[0].entity_id, entity);

    let gone = FakeSource::new(&id, vec![item(&id, "TIDE-7", &token, true)]);
    knobas_sync::run_once(&pool, &gone, None).await.unwrap();
    let hits = knobas_db::search::search(&pool, &token, 10).await.unwrap();
    assert!(
        hits.is_empty(),
        "a tombstoned entity must not answer search: {hits:?}"
    );

    let (title,): (String,) = sqlx::query_as("select title from sync.item where entity_id = $1")
        .bind(&entity)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(title, token, "the mirror row survives the tombstone");
}
