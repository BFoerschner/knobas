//! A **refresh** from the app's side (`CONTEXT.md` *Refresh*, ADR-0031): the
//! sync crate's read of one item ([`Scheduler::refresh`]), then the links on
//! arrival it settles, then the shell told on `item:refreshed`.
//!
//! The one way the app refreshes, so every refresh ends the same way: a
//! landed write's ([`super::write_queue::after_landing`]) and, from #2107,
//! the reader's *Refresh* on an item's detail.

use knobas_sync::refresh::{RefreshError, RefreshOutcome};
use knobas_sync::scheduler::{AdapterRegistry, RefreshEnd, RefreshEnded, Scheduler};

/// Whether `adapter_kind`'s template declares the read of one item
/// (`SourceDescriptor::refreshes`): what a detail's
/// `EntityDetail::refreshes` (#2107) and a roster row's `MonitorRow::refreshes`
/// (#2138) say, read off the registry so no source is built and no keychain
/// read. `false` for no configured kind or a kind no template answers.
pub fn template_refreshes(registry: &dyn AdapterRegistry, adapter_kind: Option<&str>) -> bool {
    adapter_kind.is_some_and(|kind| {
        registry
            .descriptors()
            .iter()
            .any(|template| template.adapter_kind == kind && template.refreshes())
    })
}

/// Refresh `entity_id` from its source, after the write `write_id` when one
/// is being read back.
///
/// Once the item is written, the links its source's sent writes still owe
/// are resolved (`write_queue::link_arrivals`), as after a run, so a ticket a
/// write filed is linked to what it was filed from before the run that
/// follows the write ends. Then [`SyncEvents::refresh_ended`] is told how it
/// ended, which is what the shell re-reads the item on.
///
/// A refresh that did not happen tells nothing: one the adapter does not
/// offer ([`RefreshOutcome::NotOffered`]), and one refused before anything
/// was read, for a source that is turned off, not configured, or an id that
/// is not an entity's. A refresh that failed tells `failed` and changes
/// nothing.
///
/// # Errors
/// [`RefreshError`], unchanged from [`Scheduler::refresh`].
///
/// [`SyncEvents::refresh_ended`]: knobas_sync::scheduler::SyncEvents::refresh_ended
pub async fn refresh(
    scheduler: &Scheduler,
    entity_id: &str,
    write_id: Option<i64>,
) -> Result<RefreshOutcome, RefreshError> {
    let outcome = scheduler.refresh(entity_id).await;
    let ended = match &outcome {
        Ok(RefreshOutcome::Refreshed) => RefreshEnd::Refreshed,
        Ok(RefreshOutcome::Gone) => RefreshEnd::Gone,
        Ok(RefreshOutcome::NotOffered)
        | Err(
            RefreshError::NotAnEntity(_)
            | RefreshError::NotConfigured(_)
            | RefreshError::TurnedOff(_),
        ) => return outcome,
        Err(RefreshError::Build(_) | RefreshError::Sync(_)) => RefreshEnd::Failed,
    };
    let deps = scheduler.deps();
    // `Scheduler::refresh` answered, so the id parsed.
    if ended == RefreshEnd::Refreshed
        && let Ok(entity) = knobas_core::entity::EntityRef::parse(entity_id)
    {
        super::write_queue::resolve_arrivals(&deps.pool, deps.registry.as_ref(), &entity.namespace)
            .await;
    }
    deps.events.refresh_ended(&RefreshEnded {
        entity_id: entity_id.to_owned(),
        write_id,
        ended,
    });
    outcome
}

/// How the reader's *Refresh* on an item's detail ended (#2107): what
/// `refresh_entity` answers once the refresh is over.
///
/// `{ "outcome": "refreshed" }` and its siblings on the wire, `failed`
/// carrying the source's reason, which the detail draws as *Couldn't
/// refresh: <reason>*.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, ts_rs::TS)]
#[serde(tag = "outcome", rename_all = "snake_case")]
#[ts(export_to = "sources.ts")]
pub enum EntityRefresh {
    /// The item is in the mirror as its source holds it, live again if it
    /// was tombstoned.
    Refreshed,
    /// The source no longer has it: it is tombstoned.
    Gone,
    /// The adapter does not read one item: the caller runs the source.
    NotOffered,
    /// The source is turned off, so nothing was read.
    TurnedOff,
    /// The read failed and nothing changed.
    Failed {
        /// The source's own reason.
        reason: String,
    },
}

/// The reader's refresh of `entity_id`: [`refresh`], with no write behind it,
/// answered as an [`EntityRefresh`].
///
/// # Errors
/// [`Invalid`](crate::IpcErrorCode::Invalid) for an id that is not an
/// entity's and [`NotFound`](crate::IpcErrorCode::NotFound) for a namespace no
/// source is configured for, as `get_entity` answers both.
pub async fn of_reader(
    scheduler: &Scheduler,
    entity_id: &str,
) -> Result<EntityRefresh, crate::IpcError> {
    match refresh(scheduler, entity_id, None).await {
        Ok(RefreshOutcome::Refreshed) => Ok(EntityRefresh::Refreshed),
        Ok(RefreshOutcome::Gone) => Ok(EntityRefresh::Gone),
        Ok(RefreshOutcome::NotOffered) => Ok(EntityRefresh::NotOffered),
        Err(RefreshError::TurnedOff(_)) => Ok(EntityRefresh::TurnedOff),
        Err(error @ RefreshError::NotAnEntity(_)) => Err(crate::IpcError::invalid(error)),
        Err(error @ RefreshError::NotConfigured(_)) => Err(crate::IpcError::not_found(error)),
        Err(RefreshError::Build(reason)) => Ok(EntityRefresh::Failed { reason }),
        // The source's own words, without the engine's `source:` prefix.
        Err(RefreshError::Sync(knobas_sync::SyncError::Source(error))) => {
            Ok(EntityRefresh::Failed {
                reason: error.to_string(),
            })
        }
        Err(RefreshError::Sync(error)) => Ok(EntityRefresh::Failed {
            reason: error.to_string(),
        }),
    }
}
