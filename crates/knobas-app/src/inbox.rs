//! The decisions the inbox's IPC surface makes (issue #45).
//!
//! Two of them, and both need something `knobas_core::inbox` cannot see:
//!
//! * **Which actions an item actually offers.** A category names the write ops
//!   it is asking for, as identifiers; whether a *particular* source can
//!   perform one is a property of that source's `SourceDescriptor`, and the
//!   descriptors are here, in the crate that holds the registry. An op the
//!   source does not declare is never offered -- the interface must not present
//!   a button that will fail (story 23).
//! * **What an answer records.** Snoozing and marking done are actions, and
//!   *every* inbox action is recorded in the activity stream by ratified
//!   amendment. They are recorded here, through the one activity writer, and
//!   the item they name is read back from the stream so a line cannot describe
//!   an item that is not there.
//!
//! ## What is deliberately not here: a write path
//!
//! An inbox action that acts on a source is a `WriteOp` submitted through
//! `submit_write`, which is #43's command over #42's queue. **The inbox
//! introduces no write path of its own**, so there is no `dispatch` in this
//! module and no second call site for `Source::write` -- and the queue already
//! writes an activity line per transition, so an action that goes through it
//! is recorded exactly once. This module writes lines only for the two answers
//! the queue knows nothing about.
//!
//! This lives beside `sources/write_queue.rs` in the *decision* layer for the
//! reason that file's own header gives: a `#[tauri::command]` cannot be called
//! from a test, so anything worth asserting has to be reachable without one.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use knobas_core::activity::ActivityRow;
use knobas_core::entity::EntityRef;
use knobas_core::inbox::{self, Category, InboxItem, Shelf};
use knobas_sync::scheduler::AdapterRegistry;
use sqlx::PgPool;

use crate::IpcError;

/// Who an inbox answer belongs to, in the activity line's `actor`.
///
/// The same string `commands::entity`'s link writes use, and for the same
/// reason: knobas has no identity system, and the only other actor the log
/// knows is `sync:<source_id>`.
const ACTOR: &str = "user";

/// One line of the stream, with the actions this source can really perform.
///
/// **Nested, not flattened** -- `{item, actions}` -- which is #53's ratified
/// shape for exactly this pairing and is recorded there as a decision rather
/// than a habit: the item is `knobas_core`'s record and the actions are this
/// crate's answer about it, and a flattened bag would make a later reader
/// guess which half a field came from.
#[derive(Debug, Clone, serde::Serialize)]
pub struct InboxEntry {
    pub item: InboxItem,
    /// `knobas_source::WriteOp` identifiers, in the order the category asks
    /// for them, filtered to what this item's source declares.
    ///
    /// Empty is a real answer and not a missing one: a credential expiry has
    /// nothing to ask a source for, and a source with no write capability
    /// offers nothing on any of its items. *Open*, *snooze* and *done* are not
    /// in here -- they are knobas' own and always available.
    pub actions: Vec<String>,
}

/// The stream (or the snoozed shelf), with each item's offered actions.
///
/// `identity` is the same identity `@me` resolves to; the caller loads it from
/// `knobas_search::Vocabulary` rather than reading `config -> username` again,
/// because a second identity mechanism is a second answer to "who am I".
///
/// # Errors
///
/// `internal` if the derivation or the source listing fails.
pub async fn stream(
    pool: &PgPool,
    registry: &dyn AdapterRegistry,
    identity: &[String],
    now: DateTime<Utc>,
    shelf: Shelf,
) -> Result<Vec<InboxEntry>, IpcError> {
    let items = inbox::items(pool, identity, now, shelf)
        .await
        .map_err(IpcError::internal)?;
    let declared = declared_ops(pool, registry).await?;
    Ok(items
        .into_iter()
        .map(|item| InboxEntry {
            actions: offer(item.category, declared.get(&item.source_id)),
            item,
        })
        .collect())
}

/// What one item offers: its category's candidates, kept only where the source
/// declares them.
///
/// Two absences read the same way and both are correct: a source knobas has no
/// configuration for, and a source whose adapter declares no write ops at all,
/// each offer nothing. Neither is an error -- an item whose source cannot act
/// is still an item worth seeing, and story 23 asks for the *button* to be
/// absent, not the item.
///
/// The candidate order is kept rather than the descriptor's, because it is the
/// category's statement of what the item is asking for: *approve* comes before
/// *comment* on a review request because approving is the answer and commenting
/// is the alternative.
fn offer(category: Category, declared: Option<&Vec<String>>) -> Vec<String> {
    let Some(declared) = declared else {
        return Vec::new();
    };
    category
        .candidate_ops()
        .iter()
        .filter(|op| declared.iter().any(|d| d == *op))
        .map(|op| (*op).to_owned())
        .collect()
}

/// Every configured source's declared write ops, by source id.
///
/// One listing and one descriptor sweep for the whole stream, rather than a
/// lookup per item: an inbox of thirty items over four sources would otherwise
/// make thirty round trips to answer four questions.
///
/// The **template** descriptor, which is what `list_adapters` serves and what
/// the action bar is rendered from -- `write_ops` is a property of the adapter
/// kind, not of the instance, so this needs no secret and no built adapter.
/// That is `sources::write_queue::submittable`'s reading and this must agree
/// with it, or the inbox would offer a button that call then refuses.
async fn declared_ops(
    pool: &PgPool,
    registry: &dyn AdapterRegistry,
) -> Result<HashMap<String, Vec<String>>, IpcError> {
    let by_kind: HashMap<String, Vec<String>> = registry
        .descriptors()
        .into_iter()
        .map(|d| (d.adapter_kind, d.write_ops))
        .collect();
    let sources = knobas_sync::config::list(pool)
        .await
        .map_err(IpcError::internal)?;
    Ok(sources
        .into_iter()
        .filter_map(|source| {
            by_kind
                .get(&source.adapter_kind)
                .map(|ops| (source.id, ops.clone()))
        })
        .collect())
}

/// The two answers the inbox itself records.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Answer {
    /// Not now -- come back on this date.
    Snooze(DateTime<Utc>),
    /// Handled.
    Done,
}

impl Answer {
    /// The activity verb, in the past tense every other verb in the log uses.
    fn verb(self) -> &'static str {
        match self {
            Answer::Snooze(_) => "snoozed",
            Answer::Done => "completed",
        }
    }
}

/// Record the user's answer to one item, and announce it.
///
/// **The item is read back from the stream first**, and the refusal when it is
/// not there is the point rather than defensiveness: `item_key` arrives from a
/// webview that has been holding a list, and an answer to an item that has
/// since been resolved at the source would otherwise write a durable row and
/// an activity line about something that no longer exists. Both shelves are
/// searched, so re-snoozing something already snoozed works -- that is changing
/// your mind about a date, not answering a phantom.
///
/// The activity line is what makes story 21 true for these two actions: an
/// action that acts on a *source* is a queued write and is announced by the
/// queue, so between the two writers every inbox action is recorded exactly
/// once.
///
/// # Errors
///
/// `not_found` if no item on either shelf carries `item_key`; `internal` if a
/// write fails.
pub async fn answer(
    pool: &PgPool,
    identity: &[String],
    now: DateTime<Utc>,
    item_key: &str,
    answer: Answer,
) -> Result<ActivityRow, IpcError> {
    let item = find(pool, identity, now, item_key).await?;

    match answer {
        Answer::Snooze(until) => inbox::snooze(pool, item_key, until).await,
        Answer::Done => inbox::complete(pool, item_key, now).await,
    }
    .map_err(IpcError::internal)?;

    // The entity, where the item has one. A credential expiry is keyed on a
    // source and has no entity, so its line carries none -- which is what
    // `knobas.activity.entity_id` being nullable is for, and is honest in a
    // way that pointing it at some nearby entity would not be.
    let entity = item
        .entity_id
        .as_deref()
        .and_then(|id| EntityRef::parse(id).ok());
    let detail = serde_json::json!({
        "item_key": item.key,
        "category": item.category,
        "source_id": item.source_id,
        "title": item.title,
        "until": match answer { Answer::Snooze(until) => Some(until), Answer::Done => None },
    });
    knobas_core::activity::record(pool, ACTOR, answer.verb(), entity.as_ref(), detail)
        .await
        .map_err(IpcError::internal)
}

/// The item this key names, on whichever shelf it is on.
async fn find(
    pool: &PgPool,
    identity: &[String],
    now: DateTime<Utc>,
    item_key: &str,
) -> Result<InboxItem, IpcError> {
    for shelf in [Shelf::Stream, Shelf::Snoozed] {
        let items = inbox::items(pool, identity, now, shelf)
            .await
            .map_err(IpcError::internal)?;
        if let Some(item) = items.into_iter().find(|item| item.key == item_key) {
            return Ok(item);
        }
    }
    Err(IpcError::not_found(format!(
        "no inbox item {item_key:?} -- it may have been resolved at the source"
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ops(list: &[&str]) -> Vec<String> {
        list.iter().map(|op| (*op).to_owned()).collect()
    }

    /// Story 12 and story 23 in one: a Gitea-shaped source declares `approve`,
    /// so a review request offers it -- and the candidate order is the
    /// category's, because approving is the answer and commenting is the
    /// alternative.
    #[test]
    fn a_review_request_offers_the_ops_its_source_declares_in_the_categorys_order() {
        let declared = ops(&["comment", "create_branch", "approve", "create_pull_request"]);
        assert_eq!(
            offer(Category::ReviewRequest, Some(&declared)),
            ops(&["approve", "comment"])
        );
    }

    /// Story 23: an op the source does not declare is **absent**, not offered
    /// and failing. A Jira-shaped source cannot approve a pull request, and an
    /// inbox that showed the button would be an interface that lies.
    #[test]
    fn an_op_the_source_does_not_declare_is_not_offered() {
        let declared = ops(&["comment", "transition", "create_ticket"]);
        assert_eq!(
            offer(Category::ReviewRequest, Some(&declared)),
            ops(&["comment"]),
            "approve is absent because this source does not declare it"
        );
    }

    /// A source with no write capability at all offers nothing, and so does a
    /// source knobas has no configuration for -- neither is an error, and the
    /// item is still worth seeing.
    #[test]
    fn a_source_that_declares_nothing_offers_nothing() {
        assert!(offer(Category::FailedBuild, Some(&ops(&[]))).is_empty());
        assert!(offer(Category::FailedBuild, None).is_empty());
    }

    /// The two categories with no write op are empty whatever the source
    /// declares. Both absences are decisions -- see `Category::candidate_ops`.
    #[test]
    fn the_categories_with_no_write_op_offer_nothing_from_any_source() {
        let generous = ops(&[
            "comment",
            "transition",
            "approve",
            "rerun_build",
            "trigger_build",
        ]);
        assert!(offer(Category::NewAssignment, Some(&generous)).is_empty());
        assert!(offer(Category::CredentialExpiry, Some(&generous)).is_empty());
    }

    /// Every candidate op a category names has a **form in the interface**.
    ///
    /// The other half of `app/src/lib/inbox/actions.ts`, and the half that
    /// cannot be checked from TypeScript: that file skips an op it has no form
    /// for, which is right for an op a *newer backend* offers (`WriteOp` grows
    /// per milestone, ADR-0006) and wrong for one this build asks for. Without
    /// this, adding a candidate op here would be a button that silently never
    /// appears -- on the one surface whose whole promise is that the action an
    /// item wants is one press away.
    #[test]
    fn every_candidate_op_has_a_form_in_the_interface() {
        const FORMS: &str = include_str!("../../../app/src/lib/inbox/actions.ts");
        for category in Category::ALL {
            for op in category.candidate_ops() {
                assert!(
                    FORMS.contains(&format!("\n  {op}: {{")),
                    "{category} asks for {op:?}, which ACTION_FORMS has no entry for"
                );
            }
        }
    }
}
