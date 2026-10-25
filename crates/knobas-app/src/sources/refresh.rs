//! A **refresh** from the app's side (`CONTEXT.md` *Refresh*, ADR-0031): the
//! sync crate's read of one item ([`Scheduler::refresh`]), then the links on
//! arrival it settles, then the shell told on `item:refreshed`.
//!
//! The one way the app refreshes, so every refresh ends the same way: a
//! landed write's ([`super::write_queue::after_landing`]) and, from #2107,
//! the reader's *Refresh* on an item's detail.

use knobas_sync::refresh::{RefreshError, RefreshOutcome};
use knobas_sync::scheduler::{RefreshEnd, RefreshEnded, Scheduler};

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
