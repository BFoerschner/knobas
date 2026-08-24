//! One sync run, against a real PostgreSQL.
//!
//! `test_util` gives every test in this binary the *same* database, and they
//! run concurrently, so each test below syncs a source id unique to itself.
//! Only the mock's own test uses `mock`, which is why it may assert absolute
//! row counts for that source.

use knobas_core::entity::EntityRef;
use knobas_source::{
    Capability, Cursor, KindInfo, Sink, Source, SourceDescriptor, SourceError, SyncItem, WriteOp,
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
            capabilities: vec![Capability::Search],
            adapter_version: "0.1.0".to_owned(),
            auth_methods: Vec::new(),
            write_ops: Vec::new(),
            entity_kinds: vec![KindInfo {
                id: "ticket".to_owned(),
                label: "Ticket".to_owned(),
                plural: "Tickets".to_owned(),
                monogram: "TK".to_owned(),
            }],
            config_schema: serde_json::json!({ "type": "object", "properties": {} }),
        }
    }

    async fn test_connection(&self) -> Result<(), SourceError> {
        Ok(())
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
        deleted,
    }
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

/// A run is all-or-nothing: an adapter that fails after the sink accepted items
/// leaves no rows, no cursor and no activity line behind.
#[tokio::test]
async fn an_adapter_failure_rolls_the_whole_run_back() {
    let (pool, id) = fixture().await;
    configure(&pool, &id).await;

    let src = FakeSource::failing(&id, vec![item(&id, "TIDE-3", "half written", false)]);
    let err = knobas_sync::run_once(&pool, &src, None).await.unwrap_err();
    assert!(
        matches!(
            err,
            knobas_sync::SyncError::Source(SourceError::Unreachable(_))
        ),
        "{err:?}"
    );

    assert_eq!(rows_for(&pool, &id).await, 0);
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
    assert_eq!(rows_for(&pool, "somebody-else").await, 0);
}

async fn rows_for(pool: &PgPool, source_id: &str) -> i64 {
    let (rows,): (i64,) = sqlx::query_as("select count(*) from sync.item where source_id = $1")
        .bind(source_id)
        .fetch_one(pool)
        .await
        .unwrap();
    rows
}
