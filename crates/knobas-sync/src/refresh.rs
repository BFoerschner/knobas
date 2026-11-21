//! A **refresh**: one item read from its source and written into the mirror,
//! outside any run (`CONTEXT.md` *Refresh*, ADR-0031).
//!
//! The mirror's second writer, beside [`crate::run_once`] and its siblings,
//! and it writes the way a run does: under the source's run lock
//! (`pg_advisory_xact_lock` on the source id), through the run's own item and
//! entity writes (`PgSink`), and with a run's per-item work for its one item
//! (the role check, a monitor's sample, alerts and attachments, the
//! saved-view projection, the job contexts' follow). It moves no cursor,
//! sweeps nothing and writes no "synced" activity line.
//!
//! **Serialized, not versioned.** The read happens inside the lock, so a
//! refresh that arrives while a run of its source is in flight waits for that
//! run to commit and reads after it ([`Lock::Wait`], a landed write's read
//! back), or reads nothing and says so ([`Lock::Try`], the reader's
//! *Refresh*: a run holds the lock for the whole run, network included, 74 to
//! 242 s in the scale database's log, and the button said *Refreshing…* all
//! that while; the query audit of PR #2479, 2026-11-12). A run fetches inside its transaction and
//! item writes overwrite unconditionally, so a run that began before a write
//! could otherwise commit its older payload over what the refresh read.
//!
//! Links on arrival are the app's (`knobas_app::sources::write_queue`), which
//! resolves them once a refresh ends, as it does once a run ends.

use knobas_core::entity::EntityRef;
use knobas_source::{Cursor, RefreshAnswer, Sink, Source, SourceError, UnderAnswer};
use sqlx::{Connection, PgConnection, Postgres, Transaction};

use crate::scheduler::SchedulerDeps;
use crate::{Declared, PerItem, PgSink, SyncError, check_source_id, per_item_work, role_check};

/// What a refresh did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefreshOutcome {
    /// The item is in the mirror as the source holds it, live again if it
    /// was tombstoned.
    Refreshed,
    /// The source no longer has it: the entity is tombstoned.
    Gone,
    /// The adapter does not read this item by id. Nothing was written; the
    /// caller runs the source instead.
    NotOffered,
    /// Under [`Lock::Try`], a run of the source held its lock: nothing was
    /// read or written. That run writes the item as the source holds it when
    /// it ends, or, where the run started before what changed it, the next.
    RunInFlight,
}

/// Whether a refresh waits for the source's run lock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lock {
    /// Wait for a run in flight to commit, then read after it.
    Wait,
    /// Take the lock only if no run holds it, else answer
    /// [`RefreshOutcome::RunInFlight`] at once.
    Try,
}

/// Why a refresh did not happen, or failed.
#[derive(Debug, thiserror::Error)]
pub enum RefreshError {
    /// Not an entity id at all.
    #[error("{0:?} is not an entity id")]
    NotAnEntity(String),
    /// No source holds this id's namespace.
    #[error("source {0:?} is not configured")]
    NotConfigured(String),
    /// The reader turned the source off: nothing is read from it.
    #[error("source {0:?} is turned off")]
    TurnedOff(String),
    /// The adapter could not be built: its keychain entry, or its config.
    #[error("{0}")]
    Build(String),
    /// The read or the write failed. Nothing was written.
    #[error(transparent)]
    Sync(#[from] SyncError),
}

/// Refresh `entity_id` from the source its namespace names, as configured:
/// refused for a source the reader turned off before anything is read, and
/// [`RefreshOutcome::NotOffered`] off the built adapter's descriptor
/// ([`knobas_source::SourceDescriptor::refreshes`]) before a connection is
/// opened. Otherwise [`refresh_item`] on a connection of its own (§10.6(c),
/// as a run has), which it may hold while it waits behind a run in flight.
///
/// # Errors
///
/// [`RefreshError`].
pub async fn of_entity(
    deps: &SchedulerDeps,
    entity_id: &str,
    lock: Lock,
) -> Result<RefreshOutcome, RefreshError> {
    let entity =
        EntityRef::parse(entity_id).map_err(|_| RefreshError::NotAnEntity(entity_id.to_owned()))?;
    let cfg = crate::config::get(&deps.pool, &entity.namespace)
        .await
        .map_err(SyncError::Db)?
        .ok_or_else(|| RefreshError::NotConfigured(entity.namespace.clone()))?;
    if !cfg.enabled {
        return Err(RefreshError::TurnedOff(cfg.id));
    }
    let source = crate::scheduler::build_source(deps, &cfg)
        .await
        .map_err(|failure| RefreshError::Build(failure.message()))?;
    if !source.descriptor().refreshes() {
        return Ok(RefreshOutcome::NotOffered);
    }
    let mut conn = deps.connections.open().await.map_err(SyncError::Db)?;
    let outcome = refresh_item_under(&mut conn, source.as_ref(), entity_id, lock).await;
    if let Err(error) = Connection::close(conn).await {
        tracing::debug!(entity_id, %error, "closing the refresh's connection failed");
    }
    Ok(outcome?)
}

/// Read `entity` from `source` under the source's run lock and write what it
/// answered, in one transaction on `conn`.
///
/// * [`RefreshAnswer::Item`] goes through the run's item write, which also
///   clears a tombstone: an item that answers again is live again. An item
///   the source answers deleted is tombstoned, as a run would.
/// * [`RefreshAnswer::Gone`] tombstones the entity, keeping the first
///   deletion's time as the sweep does. An id the mirror never held stays
///   unheld.
/// * [`RefreshAnswer::NotOffered`] writes nothing.
///
/// # Errors
///
/// [`SyncError::BadSourceId`] before anything is read;
/// [`SyncError::Source`] for a failed read, an id outside the source's
/// namespace, or an answer about another item; [`SyncError::Db`] for a failed
/// statement. Every one rolls back, which releases the lock.
pub async fn refresh_item(
    conn: &mut PgConnection,
    source: &dyn Source,
    entity: &str,
) -> Result<RefreshOutcome, SyncError> {
    refresh_item_under(conn, source, entity, Lock::Wait).await
}

/// [`refresh_item`], waiting for the run lock or not as `lock` says.
///
/// # Errors
///
/// As [`refresh_item`].
pub async fn refresh_item_under(
    conn: &mut PgConnection,
    source: &dyn Source,
    entity: &str,
    lock: Lock,
) -> Result<RefreshOutcome, SyncError> {
    let descriptor = source.descriptor();
    check_source_id(&descriptor.id)?;
    let (source_id, declared) = Declared::of(descriptor);
    let in_namespace = EntityRef::parse(entity).is_ok_and(|e| e.namespace == source_id);
    if !in_namespace {
        return Err(SyncError::Source(SourceError::protocol(format!(
            "{entity:?} is not an item of source {source_id:?}"
        ))));
    }
    let mut tx = conn.begin().await?;
    // The explicit rollback, for the reason `run_inner` gives: on a
    // connection of its own, a dropped transaction keeps the lock.
    match refresh_locked(&mut tx, source, &source_id, declared, entity, lock).await {
        Ok(outcome) => {
            tx.commit().await?;
            Ok(outcome)
        }
        Err(error) => {
            if let Err(rollback) = tx.rollback().await {
                tracing::warn!(source_id, %rollback, "rolling the failed refresh back failed");
            }
            Err(error)
        }
    }
}

/// Tombstone one entity, keeping a tombstone's first time.
const TOMBSTONE: &str =
    "update knobas.entity set deleted_at = now() where id = $1 and deleted_at is null";

async fn refresh_locked(
    tx: &mut Transaction<'_, Postgres>,
    source: &dyn Source,
    source_id: &str,
    declared: Declared,
    entity: &str,
    lock: Lock,
) -> Result<RefreshOutcome, SyncError> {
    let Declared {
        kinds,
        exhaustive: _,
        monitor_paths,
        saved_views,
        role_check,
        payload_paths,
    } = declared;
    // The run's lock: waits for a run in flight to commit, or under
    // `Lock::Try` reads nothing while one holds it; either way holds off the
    // next run until this commits.
    match lock {
        Lock::Wait => {
            sqlx::query("select pg_advisory_xact_lock(hashtext($1::text))")
                .bind(source_id)
                .execute(&mut **tx)
                .await?;
        }
        Lock::Try => {
            let taken: bool =
                sqlx::query_scalar("select pg_try_advisory_xact_lock(hashtext($1::text))")
                    .bind(source_id)
                    .fetch_one(&mut **tx)
                    .await?;
            if !taken {
                return Ok(RefreshOutcome::RunInFlight);
            }
        }
    }

    let outcome = match source.refresh(entity).await? {
        RefreshAnswer::NotOffered => {
            // The read of the item with what stands under it (ADR-0047),
            // from the position the source stands at, read inside the lock
            // as a run reads it: a run's end writes the whole cursor, and a
            // position read before the lock could be one that run replaced.
            let stored: Option<(Option<Cursor>,)> =
                sqlx::query_as("select cursor from knobas.source_config where id = $1")
                    .bind(source_id)
                    .fetch_optional(&mut **tx)
                    .await?;
            let stored = stored.and_then(|(cursor,)| cursor);
            let (items, cursor) = match source.refresh_under(entity, stored.as_deref()).await? {
                UnderAnswer::NotOffered => return Ok(RefreshOutcome::NotOffered),
                UnderAnswer::Walked { items, cursor } => (items, cursor),
            };
            let gone = items
                .iter()
                .any(|item| item.deleted && item.entity.to_string() == entity);
            write_items(tx, source_id, kinds, role_check, items).await?;
            // The parent's position moves with its rows, in this transaction:
            // Gitea's change gates compare the forge with the position, so
            // rows written past it would be judged against a forge that may
            // since have gone back to it (ADR-0047).
            if let Some(cursor) = cursor {
                sqlx::query("update knobas.source_config set cursor = $1 where id = $2")
                    .bind(&cursor)
                    .bind(source_id)
                    .execute(&mut **tx)
                    .await?;
            }
            if gone {
                RefreshOutcome::Gone
            } else {
                RefreshOutcome::Refreshed
            }
        }
        RefreshAnswer::Gone => {
            sqlx::query(TOMBSTONE)
                .bind(entity)
                .execute(&mut **tx)
                .await?;
            RefreshOutcome::Gone
        }
        RefreshAnswer::Item(item) => {
            if item.entity.to_string() != entity {
                return Err(SyncError::Source(SourceError::protocol(format!(
                    "asked for {entity}, the source answered {}",
                    item.entity
                ))));
            }
            let outcome = if item.deleted {
                RefreshOutcome::Gone
            } else {
                RefreshOutcome::Refreshed
            };
            write_items(tx, source_id, kinds, role_check, vec![item]).await?;
            outcome
        }
    };

    per_item_work(
        tx,
        source_id,
        &PerItem {
            monitor_paths,
            saved_views,
            payload_paths,
        },
        Some(entity),
    )
    .await?;
    Ok(outcome)
}

/// Write what a refresh read through the run's item write, judged as a run
/// judges: the same role rule an incremental run's handful of records is
/// held to.
async fn write_items(
    tx: &mut Transaction<'_, Postgres>,
    source_id: &str,
    kinds: std::collections::HashSet<String>,
    role_check: knobas_source::role::data::DataCheck,
    items: Vec<knobas_source::SyncItem>,
) -> Result<(), SyncError> {
    let kept_roles = role_check::load(tx, source_id).await?;
    let role_check = (!role_check.is_empty() && role_check::wanted(&role_check, &kept_roles))
        .then_some(role_check);
    let roles = {
        let mut sink = PgSink::new(tx, source_id.to_owned(), kinds, role_check);
        for item in items {
            sink.item(item).await?;
        }
        sink.flush().await?;
        sink.roles
    };
    if let Some(check) = roles {
        role_check::record(tx, source_id, &kept_roles, &check.finish()).await?;
    }
    Ok(())
}
