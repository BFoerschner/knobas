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
//!   the last-known title of something that vanished upstream. "Deleted"
//!   therefore lives on the entity alone, and the way to respect it is to read
//!   the mirror through the **`sync.live_item`** view (migration 0002), which
//!   has the `knobas.entity` join and the `deleted_at is null` filter built in.
//!   Anything reading `sync.item` directly has to filter for itself, and a
//!   smart list that forgets offers rows that no longer exist.
//!
//! The engine takes a `PgPool` rather than opening one: it is called from the
//! app, from a scheduler and from tests, none of which want a second database.
//!
//! # What the engine refuses
//!
//! An adapter is a plugin, and a plugin regresses. The SPI's contract battery
//! catches that in the adapter's own test suite; the engine catches it at the
//! moment it would corrupt the store, because entity ids are global and
//! `sync.item.entity_id` is a primary key -- one adapter emitting another's ids
//! would silently overwrite its rows. So [`run_once`] rejects an unusable
//! source id before it opens a transaction, and the sink rejects an item whose
//! id does not round-trip, is outside the source's namespace, or carries a kind
//! the descriptor never declared.

/// Declare an enum whose variants are a **closed vocabulary shared with the
/// database**: each one has a stored spelling, and migration 0002 has a CHECK
/// constraint listing exactly those spellings.
///
/// The point is that `ALL` and `as_str` are generated from the *same* variant
/// list as the enum itself, so the three cannot drift. A hand-written `ALL`
/// beside a hand-written enum is a list that a new variant silently misses --
/// and for these enums that is not a cosmetic bug: the value reaches a `text`
/// column with a CHECK constraint on it, so an unlisted spelling is a failed
/// `INSERT` at runtime. `run_log::finish` is called on the failure path of a
/// run, where the error is deliberately logged and swallowed, so the row would
/// simply never close and stream F's backoff would read nothing.
///
/// With this, adding a variant necessarily adds it to `ALL`, and the tests
/// that walk `ALL` against `0002` then fail until the constraint knows about
/// it too -- which is a red test instead of a broken write.
macro_rules! closed_vocabulary {
    (
        $(#[$enum_meta:meta])*
        pub enum $name:ident {
            $( $(#[$variant_meta:meta])* $variant:ident => $wire:literal ),+ $(,)?
        }
    ) => {
        $(#[$enum_meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
        #[serde(rename_all = "snake_case")]
        pub enum $name {
            $( $(#[$variant_meta])* $variant, )+
        }

        impl $name {
            /// Every variant, generated from the same list as the variants --
            /// so one cannot be added without appearing here.
            pub const ALL: &'static [$name] = &[ $( $name::$variant ),+ ];

            /// The spelling stored in the database and put on the wire.
            #[must_use]
            pub fn as_str(self) -> &'static str {
                match self { $( $name::$variant => $wire ),+ }
            }
        }
    };
}

pub mod config;
pub mod health;
pub mod progress;
pub mod run_log;
pub mod runner;

pub use health::{AuthState, CredentialHealth};
pub use progress::{ProgressSink, SyncPhase, SyncProgress};
pub use run_log::{RunCounts, SourceSyncStatus, SyncOutcome, SyncTrigger};
pub use runner::run;

use std::collections::{BTreeMap, HashMap, HashSet};

use knobas_core::activity;
use knobas_core::entity::EntityRef;
use knobas_source::{Cursor, Sink, Source, SourceError, SyncItem};
use sqlx::{PgPool, Postgres, Transaction};

/// How many items one pair of round trips writes.
///
/// The sink buffers up to this many items and then writes them as two
/// array-valued statements, so a 5,000-item source costs 20 round trips rather
/// than 10,000. Public because it is the boundary a source's behaviour changes
/// at -- a test that means to exercise a mid-run flush has to cross it.
pub const BATCH: usize = 500;

/// What one run wrote.
///
/// Counts are per *entity*, deduplicated across the whole run: an adapter that
/// emits the same entity twice contributes one. `deleted` is a subset of
/// `upserted` -- every item is written to both tables and `deleted` says how
/// many of them ended the run tombstoned -- so after a full sync,
/// `upserted == count(sync.item where source_id = <id>)`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SyncReport {
    pub source_id: String,
    /// Distinct entities written this run.
    pub upserted: u64,
    /// How many of them ended the run tombstoned.
    pub deleted: u64,
    /// Rows a **full** sync tombstoned because this run did not see them
    /// (hard-delete reconciliation). Always 0 for an incremental run, for an
    /// adapter whose full sync is not exhaustive, and for a full sync that
    /// emitted nothing -- see [`SWEEP`].
    pub swept: u64,
    /// Where the source says the next run should resume.
    pub cursor: Cursor,
}

/// Why a run failed.
///
/// The database appears twice on purpose: [`Source`] surfaces a sink failure as
/// [`SourceError::Sink`] because that is the only channel the SPI gives an
/// adapter, so every failed *item write* arrives as [`SyncError::Source`],
/// while statements the engine issues around the adapter arrive as
/// [`SyncError::Db`].
#[derive(Debug, thiserror::Error)]
pub enum SyncError {
    /// The descriptor's id cannot be used as an entity namespace. Raised
    /// before the transaction opens: nothing was read or written.
    #[error("source id {id:?} is unusable: {reason}")]
    BadSourceId { id: String, reason: &'static str },
    /// The source has no `knobas.source_config` row, so there is nowhere to
    /// resume from and nowhere to store the new position. Only
    /// [`run_from_stored_cursor`] raises it; [`run_once`] with an explicit
    /// cursor still syncs an unconfigured source (a test, an ad-hoc import).
    #[error("source {id:?} is not configured")]
    NotConfigured { id: String },
    /// The adapter failed, or propagated a sink failure back to us.
    #[error("source: {0}")]
    Source(#[from] SourceError),
    /// A statement the engine issued failed.
    #[error("database: {0}")]
    Db(#[from] sqlx::Error),
}

/// Reject a source id that cannot serve as an entity namespace.
///
/// Checked against the descriptor rather than trusted, because the namespace
/// guard in the sink is only as good as the id it compares against: a source
/// calling itself `jira:eu` would make `jira:eu:PAY-1` parse as namespace
/// `jira`, and one calling itself `note` would write into the local notes'
/// namespace. Both defeat the guard completely.
fn check_source_id(id: &str) -> Result<(), SyncError> {
    let bad = |reason| {
        Err(SyncError::BadSourceId {
            id: id.to_owned(),
            reason,
        })
    };
    if id.trim().is_empty() {
        return bad("blank");
    }
    if id.contains(':') {
        return bad("contains ':', which would split it into a different namespace");
    }
    // The list lives beside `EntityRef`, not here: the SPI's contract battery
    // rejects the same ids at certification time, and two copies of it are how
    // an adapter passes its own suite and then fails every real sync.
    if knobas_core::entity::is_reserved_namespace(id) {
        return bad("reserved for knobas-local entities");
    }
    Ok(())
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
/// is durable. A run that changed nothing -- no entities, no new cursor --
/// writes no line: a five-minute scheduler would otherwise bury the log under
/// 288 "synced nothing" entries per source per day.
///
/// Because it is written after the commit, a failure to write it **does not
/// fail the run**: it is logged at `warn` and the report is returned. The sync
/// is durable at that point, and reporting it as failed would tell a scheduler
/// to run it again and the UI to show an error over data that landed.
///
/// # Concurrency
///
/// The run takes `pg_advisory_xact_lock` on the source id, so two runs of the
/// same source serialise instead of interleaving their upserts. Different
/// sources run concurrently, and cannot collide anyway: the sink refuses items
/// outside the source's own namespace.
///
/// A run **pins one pool connection for its whole duration**, which for a real
/// adapter means for as long as the remote system takes to answer. A scheduler
/// syncing many sources at once must cap its concurrency below the pool size
/// (or hold a pool of its own), or the sync will starve the UI's queries.
///
/// # Limitations
///
/// A full sync does not reconcile: an item the source deleted *and stopped
/// mentioning* (a hard delete, rather than one reported with `deleted`) keeps
/// its row, because the engine sees only what the adapter pushes and has no way
/// to tell "gone" from "unchanged". Sweeping rows a full sync did not touch is
/// an M1 sync-engine feature.
///
/// # Errors
///
/// * [`SyncError::BadSourceId`] if the descriptor's id cannot be a namespace.
///   Raised first: nothing is read or written.
/// * [`SyncError::Source`] if the adapter failed, or if it propagated one of
///   our own sink failures (a rejected item, or a failed write). The
///   transaction is rolled back: nothing is written and the cursor is left
///   where it was.
/// * [`SyncError::Db`] if the advisory lock, the cursor update or the commit
///   failed.
///
/// A failed activity line is deliberately *not* in that list -- see above.
pub async fn run_once(
    pool: &PgPool,
    source: &dyn Source,
    cursor: Option<Cursor>,
) -> Result<SyncReport, SyncError> {
    run_inner(pool, source, CursorSource::Explicit(cursor)).await
}

/// Run `source` from the position `knobas.source_config` recorded for it.
///
/// This is what the scheduler calls, and the difference from [`run_once`] is
/// the whole point: the cursor is read **inside the transaction that holds the
/// source's advisory lock**, so two triggers arriving together -- a scheduler
/// tick and a *Sync now* -- serialise, and the second resumes from the position
/// the first stored instead of repeating its fetch. (M0 read it before the
/// lock; the carry-over records the cost: on a 40,000-issue Jira, a wasted
/// full re-fetch.)
///
/// # Errors
///
/// As [`run_once`], plus [`SyncError::NotConfigured`] when the source has no
/// configuration row: it has nowhere to store a position, so every later run
/// would sync everything again, for ever, with nothing to show that anything
/// was wrong.
pub async fn run_from_stored_cursor(
    pool: &PgPool,
    source: &dyn Source,
) -> Result<SyncReport, SyncError> {
    run_inner(pool, source, CursorSource::Stored).await
}

/// Where a run gets the position it resumes from.
///
/// Both entry points delegate to one `run_inner`, so [`run_once`] is
/// byte-for-byte the run it was in M0 -- `demo_load` passes an explicit `None`
/// and must keep getting exactly that -- while the scheduler's entry point gets
/// the read under the lock.
enum CursorSource {
    /// The caller decided: [`run_once`]'s argument, unchanged from M0.
    Explicit(Option<Cursor>),
    /// Read from `knobas.source_config` **inside the run's own lock**.
    Stored,
}

async fn run_inner(
    pool: &PgPool,
    source: &dyn Source,
    from: CursorSource,
) -> Result<SyncReport, SyncError> {
    let descriptor = source.descriptor();
    check_source_id(&descriptor.id)?;
    let source_id = descriptor.id;
    // Read before `entity_kinds` is consumed below.
    let exhaustive = descriptor.full_sync_exhaustive;
    let kinds: HashSet<String> = descriptor
        .entity_kinds
        .into_iter()
        .map(|kind| kind.id)
        .collect();

    let mut tx = pool.begin().await?;
    // Held until this transaction ends, however it ends. Two runs of one source
    // would otherwise take the same rows in whatever order their batches
    // happened to fall in, and a run holds its locks across every batch.
    sqlx::query("select pg_advisory_xact_lock(hashtext($1::text))")
        .bind(&source_id)
        .execute(&mut *tx)
        .await?;

    // Inside the lock, deliberately: see `run_from_stored_cursor`.
    let cursor = match from {
        CursorSource::Explicit(cursor) => cursor,
        CursorSource::Stored => {
            let row: Option<(Option<Cursor>,)> =
                sqlx::query_as("select cursor from knobas.source_config where id = $1")
                    .bind(&source_id)
                    .fetch_optional(&mut *tx)
                    .await?;
            match row {
                Some((cursor,)) => cursor,
                // The transaction is dropped here, which releases the advisory
                // lock: nothing was read from the source and nothing written.
                None => return Err(SyncError::NotConfigured { id: source_id }),
            }
        }
    };
    let previous = cursor.clone();
    let full_sync = cursor.is_none();

    let (cursor, upserted, deleted) = {
        let mut sink = PgSink::new(&mut tx, source_id.clone(), kinds);
        let cursor = source.sync(cursor, &mut sink).await?;
        // The adapter is done, so whatever is still buffered belongs to this
        // run: flush it before the cursor claims to cover it.
        sink.flush().await?;
        (cursor, sink.upserted, sink.deleted)
    };

    // Reconcile what a full sync did not see. Inside the same transaction as
    // the writes, so a failure rolls the tombstones back with them.
    //
    // Three conditions, and dropping any one of them alone is a bug:
    //  * `full_sync` -- an incremental run has not seen the whole source;
    //  * `exhaustive` -- a *bounded* full sync (TeamCity: newest N builds per
    //    configuration) does not return everything, so absence is not deletion;
    //  * `upserted > 0` -- a full sync that emitted nothing is
    //    indistinguishable from an adapter that silently failed, and sweeping
    //    there would tombstone the whole source.
    let swept = if full_sync && exhaustive && upserted > 0 {
        sqlx::query(SWEEP)
            .bind(&source_id)
            .execute(&mut *tx)
            .await?
            .rows_affected()
    } else {
        0
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
        swept,
        cursor,
    };
    let changed_nothing = report.upserted == 0
        && report.deleted == 0
        && report.swept == 0
        && previous.as_deref() == Some(&report.cursor);
    if !changed_nothing
        && let Err(error) = activity::record(
            pool,
            &format!("sync:{}", report.source_id),
            "synced",
            // A run is about a source, not about any one of the entities it
            // touched, so the line carries no entity id.
            None,
            // The report *is* the detail; hand-copying its fields here is how
            // the two drift apart.
            serde_json::to_value(&report).expect("a SyncReport serializes"),
        )
        .await
    {
        // Warned about, never raised. Everything this run wrote is already
        // durable, so a failure here is one missing log line -- and reporting
        // it as a failed sync would make the caller believe none of it landed:
        // a scheduler would repeat a run that already happened, and the UI
        // would show an error over data sitting in the database.
        tracing::warn!(
            source_id = %report.source_id,
            %error,
            "the sync committed, but its activity line did not"
        );
    }
    Ok(report)
}

/// Hard-delete reconciliation for a full sync (interfaces §1, point 4), run
/// only for an adapter that declared `full_sync_exhaustive` -- see `run_inner`.
///
/// No `last_seen_at` column is needed, and adding one would be a second truth:
/// `ITEM_UPSERT` stamps `synced_at = now()`, and `now()` is the **transaction**
/// timestamp -- one value for every row this run wrote. So inside this very
/// transaction, `synced_at < now()` is precisely "this run did not touch it".
/// (The M0 carry-over already records that property, as an accepted consequence
/// of long runs stamping every item with the run's start.)
///
/// Driven from `sync.item` rather than from `knobas.entity`: the mirror is
/// indexed by `(source_id, …)`, so this touches one source's rows instead of
/// scanning every entity knobas holds.
///
/// `deleted_at is null` keeps the **first** deletion's timestamp, exactly as
/// `ENTITY_UPSERT` does -- a tombstone restamped by every later run would
/// report a month-old deletion as fresh for ever -- and is also what makes
/// `swept` count *new* tombstones rather than every already-dead row. The
/// mirror row is left alone on purpose: the UI still renders the last-known
/// title of something that vanished upstream.
const SWEEP: &str = r#"
update knobas.entity e
   set deleted_at = now()
  from sync.item i
 where i.entity_id = e.id
   and i.source_id = $1
   and i.synced_at < now()
   and e.deleted_at is null
"#;

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
    /// The kinds the descriptor declared; nothing else may be emitted.
    kinds: HashSet<String>,
    buf: Vec<SyncItem>,
    /// Every entity id written this run, against whether its most recent write
    /// tombstoned it. Run-scoped rather than per-batch because the counts end
    /// up in a durable activity line, and an entity re-emitted in a later batch
    /// is still one row -- counting it twice would report 501 rows written for
    /// a source that has 500. One entry per distinct entity, so a very large
    /// source pays for this in memory; batching alone cannot dedupe a run.
    seen: HashMap<String, bool>,
    upserted: u64,
    deleted: u64,
}

impl<'t, 'c> PgSink<'t, 'c> {
    fn new(
        tx: &'t mut Transaction<'c, Postgres>,
        source_id: String,
        kinds: HashSet<String>,
    ) -> Self {
        Self {
            tx,
            source_id,
            kinds,
            buf: Vec::new(),
            seen: HashMap::new(),
            upserted: 0,
            deleted: 0,
        }
    }

    /// Whether this item is this source's to emit, and addressable at all.
    fn check(&self, item: &SyncItem) -> Result<(), SourceError> {
        let id = item.entity.to_string();
        // `EntityRef::new` does not validate, so an adapter can hand us a blank
        // or unparseable key; the store would then hold an id nothing can
        // address. Round-tripping through `parse` is the same check the SPI's
        // contract battery makes.
        let parsed = EntityRef::parse(&id).map_err(|e| SourceError::Sink(e.to_string()))?;
        if parsed != item.entity {
            return Err(SourceError::Sink(format!(
                "entity {id:?} does not round-trip: namespace {:?} is not addressable",
                item.entity.namespace
            )));
        }
        if parsed.namespace != self.source_id {
            return Err(SourceError::Sink(format!(
                "source {:?} emitted {id} outside its namespace",
                self.source_id
            )));
        }
        // The UI renders this source's items from the kinds it declared, so an
        // item of an undeclared kind is one nothing knows how to show.
        if !self.kinds.contains(&item.kind) {
            return Err(SourceError::Sink(format!(
                "source {:?} emitted {id} of undeclared kind {:?}",
                self.source_id, item.kind
            )));
        }
        Ok(())
    }

    /// Write everything buffered.
    ///
    /// Failures surface as [`SourceError::Sink`] from both call sites -- the
    /// adapter's mid-run flush and [`run_once`]'s final one -- so the same
    /// database failure is reported the same way wherever it happens.
    async fn flush(&mut self) -> Result<(), SourceError> {
        self.write_batch()
            .await
            .map_err(|e| SourceError::Sink(e.to_string()))
    }

    /// One batch, as two array-valued upserts.
    async fn write_batch(&mut self) -> Result<(), sqlx::Error> {
        if self.buf.is_empty() {
            return Ok(());
        }
        // Keyed by entity id, so an adapter that pushes the same entity twice
        // in one batch gets last-write-wins instead of Postgres' "ON CONFLICT
        // DO UPDATE command cannot affect row a second time". Ordered, so a
        // batch's writes have a stable order.
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
        let mut web_urls = Vec::with_capacity(n);
        for (id, item) in batch {
            ids.push(id);
            kinds.push(item.kind);
            titles.push(item.title);
            bodies.push(item.body_text);
            authors.push(item.author);
            updated.push(item.updated_at);
            deleted.push(item.deleted);
            payloads.push(item.payload);
            web_urls.push(item.web_url);
        }

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
            .bind(&web_urls)
            .bind(&self.source_id)
            .execute(&mut **self.tx)
            .await?;

        // Counted only once the rows are actually in the transaction.
        for (id, tombstoned) in ids.iter().zip(&deleted) {
            self.count(id, *tombstoned);
        }
        Ok(())
    }

    /// Fold one written row into the run's counts, at most once per entity.
    fn count(&mut self, id: &str, tombstoned: bool) {
        if let Some(was) = self.seen.get_mut(id) {
            // Same entity again in a later batch: it is still one row, but the
            // later write decides whether that row is now a tombstone.
            if *was != tombstoned {
                *was = tombstoned;
                if tombstoned {
                    self.deleted += 1;
                } else {
                    self.deleted -= 1;
                }
            }
            return;
        }
        self.seen.insert(id.to_owned(), tombstoned);
        self.upserted += 1;
        if tombstoned {
            self.deleted += 1;
        }
    }
}

/// `knobas.entity`: identity, kind, title, and the deletion tombstone.
///
/// The left join reads the row this upsert is about to replace, which is what
/// lets both timestamps keep their history:
///
/// * `updated_at` keeps the stored value when the source does not say when the
///   item changed (`SyncItem::updated_at` is `None`), instead of restamping it
///   to `now()` on every run and making an untouched item look freshly edited.
///   Only a genuinely new row falls back to `now()`.
/// * `deleted_at` keeps the *first* deletion's timestamp rather than being
///   restamped by every later run, and is cleared when the item comes back --
///   sources do resurrect things, and a stale tombstone would hide a live
///   entity.
///
/// Reading the old row and writing the new one is only atomic because
/// [`run_once`] holds the source's advisory lock: same-source runs cannot
/// interleave, and no other source may write this namespace.
const ENTITY_UPSERT: &str = r#"
with incoming as (
  select *
    from unnest($1::text[], $2::text[], $3::text[], $4::timestamptz[], $5::bool[])
         as t(id, kind, title, updated_at, deleted)
)
insert into knobas.entity (id, kind, title, updated_at, deleted_at)
select i.id, i.kind, i.title,
       coalesce(i.updated_at, old.updated_at, now()),
       case when i.deleted then coalesce(old.deleted_at, now()) end
  from incoming i
       left join knobas.entity old on old.id = i.id
    on conflict (id) do update set
       kind       = excluded.kind,
       title      = excluded.title,
       updated_at = excluded.updated_at,
       deleted_at = excluded.deleted_at
"#;

/// `sync.item`: the mirror, refreshed wholesale. `synced_at` is when this run
/// saw the item; `item_updated_at` is when the source says it changed.
///
/// `item_updated_at` keeps the stored value when the source does not say, for
/// the same reason and by the same left join as `knobas.entity.updated_at`:
/// the two columns hold the same fact, and a mirror whose timestamp disagreed
/// with its entity's on identical input would be a trap for anything reading
/// either. It stays null only while the source has never dated the item --
/// unlike the entity's, which is `not null` and falls back to `now()` on a
/// genuinely new row.
///
/// `web_url` is refreshed wholesale like the title, **not** coalesced like
/// `item_updated_at`: an adapter that stops reporting a URL is reporting that
/// there is no page, and the two timestamps coalesce only because they hold the
/// same fact as `knobas.entity.updated_at`.
const ITEM_UPSERT: &str = r#"
with incoming as (
  select *
    from unnest($1::text[], $2::text[], $3::text[], $4::text[], $5::text[],
                $6::timestamptz[], $7::jsonb[], $8::text[])
         as t(id, kind, title, body_text, author, item_updated_at, payload, web_url)
)
insert into sync.item
       (entity_id, source_id, kind, title, body_text, author, item_updated_at,
        synced_at, payload, web_url)
select i.id, $9, i.kind, i.title, i.body_text, i.author,
       coalesce(i.item_updated_at, old.item_updated_at), now(), i.payload, i.web_url
  from incoming i
       left join sync.item old on old.entity_id = i.id
    on conflict (entity_id) do update set
       source_id       = excluded.source_id,
       kind            = excluded.kind,
       title           = excluded.title,
       body_text       = excluded.body_text,
       author          = excluded.author,
       item_updated_at = excluded.item_updated_at,
       synced_at       = excluded.synced_at,
       payload         = excluded.payload,
       web_url         = excluded.web_url
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
        self.check(&item)?;
        self.buf.push(item);
        if self.buf.len() >= BATCH {
            self.flush().await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_source_id_must_be_usable_as_a_namespace() {
        check_source_id("jira").unwrap();
        check_source_id("uptime-kuma").unwrap();
        for bad in ["", "   ", "jira:eu", ":", "CTX"] {
            assert!(check_source_id(bad).is_err(), "{bad:?} should be refused");
        }
        // Iterated rather than spelled out again: the engine has to refuse
        // every namespace knobas keeps, including any added later.
        for reserved in knobas_core::entity::RESERVED_NAMESPACES {
            assert!(
                check_source_id(reserved).is_err(),
                "{reserved:?} should be refused"
            );
        }
    }

    /// `SyncReport` crosses the bridge -- `demo_load` returns one -- so its
    /// shape is contract. The **exact key set**, for the reason the M0
    /// carry-over spells out: a test that only walks a hardcoded list of
    /// fields cannot see a Rust field added with no TypeScript counterpart,
    /// and `swept` was exactly such an addition.
    #[test]
    fn the_report_shape_matches_its_typescript_mirror() {
        let mirror = include_str!("../../../app/src/lib/ipc/sources.ts");
        let report = SyncReport {
            source_id: "mock".to_owned(),
            upserted: 12,
            deleted: 1,
            swept: 2,
            cursor: r#"{"v":1}"#.to_owned(),
        };

        let wire = serde_json::to_value(&report).expect("a report serializes");
        let object = wire.as_object().expect("a report is a JSON object");
        let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            ["cursor", "deleted", "source_id", "swept", "upserted"],
            "SyncReport grew or lost a field; app/src/lib/ipc/sources.ts has \
             to grow or lose it too"
        );

        for key in &keys {
            assert!(
                mirror.contains(&format!("{key}:")),
                "SyncReport.{key} is missing from app/src/lib/ipc/sources.ts"
            );
        }
    }

    fn unit_item(source_id: &str, n: usize) -> SyncItem {
        SyncItem {
            entity: EntityRef::new(source_id, &format!("U-{n}")),
            kind: "ticket".to_owned(),
            title: format!("unit {n}"),
            body_text: String::new(),
            author: None,
            updated_at: None,
            payload: serde_json::json!({}),
            web_url: None,
            deleted: false,
        }
    }

    async fn mirrored(tx: &mut Transaction<'_, Postgres>, source_id: &str) -> i64 {
        let (rows,): (i64,) = sqlx::query_as("select count(*) from sync.item where source_id = $1")
            .bind(source_id)
            .fetch_one(&mut **tx)
            .await
            .unwrap();
        rows
    }

    /// A full batch is written **while the adapter is still syncing**, not
    /// saved up until it returns.
    ///
    /// Everything else about batching rests on this: the run-scoped counters
    /// only differ from per-batch ones once a boundary has been crossed
    /// mid-run, and the rollback test is vacuous unless rows really were
    /// written before the failure. Driving the sink directly is the only way
    /// to observe the moment, because the transaction it writes into is
    /// invisible from any other connection and gone by the time `run_once`
    /// returns.
    #[tokio::test]
    async fn a_full_batch_is_written_while_the_adapter_is_still_running() {
        let pool = knobas_db::test_util::test_pool().await;
        knobas_db::migrate::run(&pool).await.unwrap();
        let source_id = format!("unit-{}", uuid::Uuid::new_v4());

        let mut tx = pool.begin().await.unwrap();
        let mid_run = {
            let kinds = HashSet::from(["ticket".to_owned()]);
            let mut sink = PgSink::new(&mut tx, source_id.clone(), kinds);
            for n in 0..BATCH {
                sink.item(unit_item(&source_id, n)).await.unwrap();
            }
            assert!(
                sink.buf.is_empty(),
                "a full batch must be flushed, not left buffered"
            );
            assert_eq!(sink.upserted, BATCH as u64);
            // Read back through the sink's own transaction: the adapter has not
            // returned and nothing has committed.
            mirrored(sink.tx, &source_id).await
        };
        assert_eq!(
            mid_run, BATCH as i64,
            "the batch must already be in the transaction"
        );

        // ...and it is still only in the transaction.
        tx.rollback().await.unwrap();
        let (after,): (i64,) =
            sqlx::query_as("select count(*) from sync.item where source_id = $1")
                .bind(&source_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(after, 0);
    }
}
