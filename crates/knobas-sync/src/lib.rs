//! The sync engine: one run of a [`Source`] into Postgres.
//!
//! [`run_once`] is the whole write side of a sync. It hands the adapter a
//! [`Sink`] that batches its items into `knobas.entity` and `sync.item`, all in
//! **one transaction** -- a run either lands completely or not at all, so a
//! failure halfway through cannot leave the store holding half a source's
//! world with a fresh cursor written over the gap.
//!
//! Two tables per item, because they answer different questions:
//!
//! * `knobas.entity` is the durable identity. Links, notes and activity point
//!   at it, so a remote deletion only sets `deleted_at` -- the row itself stays
//!   and everything referencing it survives.
//! * `sync.item` is the synced mirror: title, body, author, raw payload, and
//!   the generated `fts` column search reads. It is refreshed wholesale on
//!   every run, including for a tombstoned entity, so the UI can still render
//!   the last-known title of something that vanished upstream. Consumers that
//!   want only live items filter on `knobas.entity.deleted_at`.
//!
//! The engine takes a `PgPool` rather than opening one: it is called from the
//! app, from a scheduler and from tests, none of which want a second database.

use std::collections::BTreeMap;

use knobas_core::activity;
use knobas_source::{Cursor, Sink, Source, SourceError, SyncItem};
use sqlx::{PgPool, Postgres, Transaction};

/// How many items one pair of round trips writes.
///
/// The sink buffers up to this many items and then writes them as two
/// array-valued statements, so a 5,000-item source costs 20 round trips rather
/// than 10,000. Large enough to amortise the latency, small enough that the
/// arrays stay a sane size to encode.
const BATCH: usize = 500;

/// What one run wrote.
///
/// `deleted` is a *subset* of `upserted`: every item the source pushed is
/// written to both tables, and `deleted` says how many of them arrived
/// tombstoned. So after a full sync,
/// `upserted == count(sync.item where source_id = <id>)`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SyncReport {
    pub source_id: String,
    /// Items written this run.
    pub upserted: u64,
    /// How many of them carried `deleted`, and so tombstoned their entity.
    pub deleted: u64,
    /// Where the source says the next run should resume.
    pub cursor: Cursor,
}

/// Why a run failed.
///
/// The database appears twice on purpose: [`Source`] surfaces a sink failure as
/// [`SourceError::Sink`] because that is the only channel the SPI gives an
/// adapter, so a write that failed *under* the adapter arrives as
/// [`SyncError::Source`], while one the engine issued itself arrives as
/// [`SyncError::Db`].
#[derive(Debug, thiserror::Error)]
pub enum SyncError {
    /// The adapter failed, or propagated a sink failure back to us.
    #[error("source: {0}")]
    Source(#[from] SourceError),
    /// A statement the engine issued failed.
    #[error("database: {0}")]
    Db(#[from] sqlx::Error),
    /// The data committed, but the activity line did not.
    #[error("activity log: {0}")]
    Activity(#[from] knobas_core::CoreError),
}

/// Pull everything `source` changed since `cursor` into `pool`.
///
/// `cursor` is the one a previous run returned in its [`SyncReport`], or `None`
/// for a full sync. On success the new cursor is persisted into
/// `knobas.source_config.cursor` **if a row for this source exists** -- a run
/// against an unconfigured source (a test, an ad-hoc import) syncs fine and
/// stores no position, and never invents a configuration row for itself.
///
/// The activity line is written after the commit, from the same pool but
/// outside the transaction: it records what happened, so it must not be rolled
/// back with the run it is describing, and must not be written before that run
/// is durable.
///
/// # Errors
///
/// * [`SyncError::Source`] if the adapter failed, or if it propagated one of
///   our own sink failures. The transaction is rolled back: nothing is written
///   and the cursor is left where it was.
/// * [`SyncError::Db`] if the cursor update or the commit failed.
/// * [`SyncError::Activity`] if only the activity line failed -- the run's data
///   is committed at that point, and a later run will overwrite it anyway.
pub async fn run_once(
    pool: &PgPool,
    source: &dyn Source,
    cursor: Option<Cursor>,
) -> Result<SyncReport, SyncError> {
    let source_id = source.descriptor().id;

    let mut tx = pool.begin().await?;
    let (cursor, upserted, deleted) = {
        let mut sink = PgSink::new(&mut tx, source_id.clone());
        let cursor = source.sync(cursor, &mut sink).await?;
        // The adapter is done, so whatever is still buffered belongs to this
        // run: flush it before the cursor claims to cover it.
        sink.flush().await?;
        (cursor, sink.upserted, sink.deleted)
    };

    sqlx::query("update knobas.source_config set cursor = $1 where id = $2")
        .bind(&cursor)
        .bind(&source_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;

    let report = SyncReport {
        source_id,
        upserted,
        deleted,
        cursor,
    };
    activity::record(
        pool,
        &format!("sync:{}", report.source_id),
        "synced",
        // A run is about a source, not about any one of the entities it
        // touched, so the line carries no entity id.
        None,
        serde_json::json!({
            "source_id": report.source_id,
            "upserted": report.upserted,
            "deleted": report.deleted,
            "cursor": report.cursor,
        }),
    )
    .await?;
    Ok(report)
}

/// The [`Sink`] the engine hands to an adapter: buffers items and writes them
/// into the caller's transaction in batches.
///
/// Borrows the transaction rather than owning it so that [`run_once`] keeps the
/// commit -- a sink that could commit would let an adapter make a partial sync
/// durable by dropping it.
struct PgSink<'t, 'c> {
    tx: &'t mut Transaction<'c, Postgres>,
    /// The descriptor id, which is also the namespace every item must be in.
    source_id: String,
    buf: Vec<SyncItem>,
    upserted: u64,
    deleted: u64,
}

impl<'t, 'c> PgSink<'t, 'c> {
    fn new(tx: &'t mut Transaction<'c, Postgres>, source_id: String) -> Self {
        Self {
            tx,
            source_id,
            buf: Vec::new(),
            upserted: 0,
            deleted: 0,
        }
    }

    /// Write everything buffered, as two array-valued upserts.
    async fn flush(&mut self) -> Result<(), sqlx::Error> {
        if self.buf.is_empty() {
            return Ok(());
        }
        // Keyed by entity id, so an adapter that pushes the same entity twice
        // in one batch gets last-write-wins instead of Postgres' "ON CONFLICT
        // DO UPDATE command cannot affect row a second time". Ordered, so two
        // concurrent syncs touching the same rows take them in the same order
        // and cannot deadlock against each other.
        let batch: BTreeMap<String, SyncItem> = self
            .buf
            .drain(..)
            .map(|item| (item.entity.to_string(), item))
            .collect();

        let n = batch.len();
        let mut ids = Vec::with_capacity(n);
        let mut kinds = Vec::with_capacity(n);
        let mut titles = Vec::with_capacity(n);
        let mut bodies = Vec::with_capacity(n);
        let mut authors = Vec::with_capacity(n);
        let mut updated = Vec::with_capacity(n);
        let mut deleted = Vec::with_capacity(n);
        let mut payloads = Vec::with_capacity(n);
        for (id, item) in batch {
            ids.push(id);
            kinds.push(item.kind);
            titles.push(item.title);
            bodies.push(item.body_text);
            authors.push(item.author);
            updated.push(item.updated_at);
            deleted.push(item.deleted);
            payloads.push(item.payload);
        }
        let tombstoned = deleted.iter().filter(|d| **d).count() as u64;

        // The entity first: `sync.item.entity_id` references it.
        sqlx::query(ENTITY_UPSERT)
            .bind(&ids)
            .bind(&kinds)
            .bind(&titles)
            .bind(&updated)
            .bind(&deleted)
            .execute(&mut **self.tx)
            .await?;
        sqlx::query(ITEM_UPSERT)
            .bind(&ids)
            .bind(&kinds)
            .bind(&titles)
            .bind(&bodies)
            .bind(&authors)
            .bind(&updated)
            .bind(&payloads)
            .bind(&self.source_id)
            .execute(&mut **self.tx)
            .await?;

        self.upserted += n as u64;
        self.deleted += tombstoned;
        Ok(())
    }
}

/// `knobas.entity`: identity, kind, title, and the deletion tombstone.
///
/// `deleted_at` keeps the *first* deletion's timestamp rather than being
/// restamped by every later run, and is cleared when the item comes back --
/// sources do resurrect things, and a stale tombstone would hide a live entity.
const ENTITY_UPSERT: &str = r#"
insert into knobas.entity as e (id, kind, title, updated_at, deleted_at)
select t.id, t.kind, t.title, coalesce(t.updated_at, now()),
       case when t.deleted then now() end
  from unnest($1::text[], $2::text[], $3::text[], $4::timestamptz[], $5::bool[])
       as t(id, kind, title, updated_at, deleted)
    on conflict (id) do update set
       kind       = excluded.kind,
       title      = excluded.title,
       updated_at = excluded.updated_at,
       deleted_at = case when excluded.deleted_at is null then null
                         else coalesce(e.deleted_at, excluded.deleted_at) end
"#;

/// `sync.item`: the mirror, refreshed wholesale. `synced_at` is when this run
/// saw the item; `item_updated_at` is when the source says it changed.
const ITEM_UPSERT: &str = r#"
insert into sync.item
       (entity_id, source_id, kind, title, body_text, author, item_updated_at,
        synced_at, payload)
select t.id, $8, t.kind, t.title, t.body_text, t.author, t.item_updated_at,
       now(), t.payload
  from unnest($1::text[], $2::text[], $3::text[], $4::text[], $5::text[],
              $6::timestamptz[], $7::jsonb[])
       as t(id, kind, title, body_text, author, item_updated_at, payload)
    on conflict (entity_id) do update set
       source_id       = excluded.source_id,
       kind            = excluded.kind,
       title           = excluded.title,
       body_text       = excluded.body_text,
       author          = excluded.author,
       item_updated_at = excluded.item_updated_at,
       synced_at       = excluded.synced_at,
       payload         = excluded.payload
"#;

#[async_trait::async_trait]
impl Sink for PgSink<'_, '_> {
    /// Buffer one item, writing a full batch through as it fills.
    ///
    /// # Errors
    ///
    /// [`SourceError::Sink`] -- the only channel the SPI gives a sink -- if the
    /// item is not this source's to emit, or if the write failed. Either way
    /// the adapter must propagate it and abandon the sync, which rolls the
    /// whole run back.
    async fn item(&mut self, item: SyncItem) -> Result<(), SourceError> {
        // Entity ids are global: an adapter emitting outside its own namespace
        // would overwrite another source's rows, since `sync.item.entity_id` is
        // the primary key. The descriptor id *is* the namespace (SPI), so this
        // is a contract violation, not a data condition.
        if item.entity.namespace != self.source_id {
            return Err(SourceError::Sink(format!(
                "source {:?} emitted {} outside its namespace",
                self.source_id, item.entity
            )));
        }
        self.buf.push(item);
        if self.buf.len() >= BATCH {
            self.flush()
                .await
                .map_err(|e| SourceError::Sink(e.to_string()))?;
        }
        Ok(())
    }
}
