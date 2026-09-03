//! *Log an ad-hoc block*: which ticket a page's, a note's, a repo's or a
//! label's afternoon should go to, and why (spec #272 "Worklog draft and
//! ad-hoc block", issue #281).
//!
//! `commands/time.rs` is shims over this, the arrangement [`super`] records.
//!
//! # The question this answers
//!
//! Stopping the timer on a **ticket** opens the worklog draft (#280): the
//! target is already where the time goes. Stopping it on anything else leaves a
//! block with nowhere to be logged, and knobas either says something useful
//! about it or the afternoon quietly stays local. What it says is one ticket
//! and **the name of the rule that produced it**, because a suggestion whose
//! reason is not on screen is a guess the reader has to either trust or ignore.
//!
//! # The three rules, in order, and why that order
//!
//! 1. **A ticket directly linked to the target**, most recently linked winning.
//!    A link is the one thing in knobas the reader drew themselves, so it
//!    outranks anything knobas worked out. Most recent rather than oldest
//!    because a link drawn this morning is what the reader is doing today; the
//!    design page linked to last quarter's ticket is not.
//! 2. **The anchor ticket of the stored context the block ran in.** The room
//!    was chosen by the reader too, but it says *what I am working on*, not
//!    *what this page is*, so it comes second. The room is read off the block
//!    ([`super::start`] recorded it, migration `0016`), never off the room the
//!    reader happens to be in now -- those are different facts, and the second
//!    one attributes this morning's page to this afternoon's epic.
//! 3. **Today's last logged ticket.** The weakest and the most often right: a
//!    person who has logged to PAY-231 twice today was probably reading that
//!    page for PAY-231. It is last precisely because it says nothing about the
//!    target at all.
//!
//! No rule firing is **no suggestion**, and the dialog then offers *Keep local*
//! as its default. Inventing a ticket from "the only one in the mirror" or "the
//! most recently viewed" would be knobas putting hours on a ticket for a reason
//! it could not name.
//!
//! # Confirmed links only (ADR-0008)
//!
//! Rule one reads `knobas.confirmed_link` and never `knobas.link`. A proposal
//! is a detector's guess; a suggestion built on one would be a guess about a
//! guess, offered in a dialog whose *Log to a ticket…* puts real minutes on a
//! real ticket. ADR-0008 draws the line for context membership -- "membership
//! built from proposals would make the two circular" -- and #41's sentence is
//! the same one: an unconfirmed guess shown as a link "would be a correctness
//! bug, not a cosmetic one". `crates/knobas-core/tests/link_reads.rs` is what
//! keeps this file honest about it.
//!
//! # What counts as a ticket
//!
//! **What the source says it takes a worklog on**, never a kind list. The rule
//! is [`super::worklog`]'s and it is read from the same descriptor: a
//! suggestion the reader accepts opens that draft, so a suggestion the draft
//! would then refuse is a dead end with a comment already typed into it. Spec
//! §3a from the other side -- a hardcoded "tickets only" table is exactly what
//! the descriptor exists to replace.
//!
//! That is also what makes the offer's `null` meaningful: a block whose *own*
//! target takes a worklog is not an ad-hoc block at all, and this read says so
//! by answering nothing, so the shell has one question to ask rather than a
//! list of kinds to keep.

use std::collections::HashSet;

use chrono::NaiveDate;
use knobas_core::entity::EntityRef;
use sqlx::{PgPool, Row};

use super::worklog::{LOG_WORK, day_bounds};
use crate::IpcError;

/// Which rule produced a suggestion -- the *why* the dialog puts on screen.
///
/// A word rather than a sentence, and the sentences live in the component.
/// The reason names knobas' own rule and has to be readable beside the ticket
/// it explains ("linked to this page", "the room you were in"), which is
/// wording that belongs with the layout it sits in; what the backend owes is
/// the fact of *which* rule fired, and that is one closed vocabulary the
/// mirror pins as a union.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SuggestionRule {
    /// Rule one: a confirmed link from the block's target to this ticket.
    LinkedToTarget,
    /// Rule two: this ticket anchors the stored context the block ran in.
    ContextAnchor,
    /// Rule three: this ticket is the last one logged to on the block's day.
    LastLogged,
}

/// A ticket the ad-hoc dialog offers, and the rule that produced it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Suggestion {
    pub entity_id: String,
    /// What the mirror calls it, or `null`.
    ///
    /// Beside the id rather than inside it, and blank read as nothing -- the
    /// rule [`super::day::DayBlock`] states, for the same reason: the title is
    /// the mirror's current opinion of a row that may since have been renamed
    /// or purged, and the view's one fallback is to show the id.
    pub title: Option<String>,
    pub rule: SuggestionRule,
}

/// What stopping the timer on a page, a note, a repo or a label offers.
///
/// **Two nested absences, and they are different answers**, which is why this
/// is a struct and not an `Option<Option<Suggestion>>` (serde spells both of
/// those `null`):
///
/// * [`offer`] answering `None` is *this is not an ad-hoc block* -- its target
///   is a ticket, and the worklog draft is what opens.
/// * `Some(AdHocBlock { suggestion: None })` is *the dialog opens and knobas
///   has nothing to suggest* -- no rule fired, and *Keep local* is the default.
///
/// One field, and the block's id is deliberately **not** echoed back: the
/// caller passed it in and still holds it, so a copy on the way out would be a
/// value with no reader and a promise of a guard nothing makes.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct AdHocBlock {
    pub suggestion: Option<Suggestion>,
}

/// The block's target and the room it ran in. Three columns, one read.
const READ_BLOCK: &str = "select entity_id, label, context_id from knobas.block where id = $1";

/// Every confirmed link touching the target, most recently linked first.
///
/// **`knobas.confirmed_link`, per ADR-0008 and #41** -- see the module docs.
///
/// **Both directions.** A link is one bidirectional record whichever way round
/// it was drawn (`CONTEXT.md`, *link*), so a rule that read `from_id` alone
/// would fire or not fire depending on which end the reader started dragging
/// from.
///
/// **Ordered by `confirmed_at`**, and that is the reading of "most recently
/// linked" this file commits to: `created_at` is when the *row* came to exist,
/// which for an accepted suggestion is when a detector guessed, possibly weeks
/// before anybody agreed. The moment a link became a link is the moment
/// somebody confirmed it. `created_at` and the id break ties, so two links
/// confirmed in the same instant still order deterministically rather than by
/// whatever the planner returned.
const LINKED: &str = "select case when l.from_id = $1 then l.to_id else l.from_id end as other
       from knobas.confirmed_link l
      where l.from_id = $1 or l.to_id = $1
      order by l.confirmed_at desc, l.created_at desc, l.id desc";

/// The anchor of one stored context.
///
/// No `archived_at` filter: the question is where the block *ran*, and
/// archiving a room later does not unmake the afternoon that happened in it.
const ANCHOR: &str = "select anchor_id from knobas.context where id = $1";

/// The tickets logged to inside a day, most recently logged first.
///
/// `created_at`, not `started_at`: "today's last logged ticket" is about the
/// order the reader logged things in, and a worklog written this afternoon for
/// this morning's work is still the last thing they logged.
///
/// **The whole day and not `limit 1`**, so that a worklog whose source has
/// since been removed is *skipped* rather than ending the rule -- the same
/// reading [`linked_to_target`] gives its own list. A day holds a handful of
/// these.
const LAST_LOGGED: &str = "select entity_id from knobas.worklog
      where created_at >= $1 and created_at < $2
      order by created_at desc, id desc";

/// What the mirror calls an entity. Blank is nothing, as everywhere else.
const TITLE: &str = "select title from knobas.entity where id = $1";

/// The namespaces a worklog can actually be sent to.
///
/// One read of the configuration rather than [`super::worklog`]'s per-entity
/// question, because rule one walks a list of candidates and asking per
/// candidate would be one query per link on a busy page.
///
/// The namespace is an **instance** id and descriptors are per *kind* (§4.2),
/// so the configuration row is the hop between them -- the same hop
/// `worklog::takes_a_worklog` and `sources::write_queue::submittable` make, for
/// the same reason: a second Jira called `jira-eu` declares what every Jira
/// declares, and matching the namespace against a template's id would find
/// nothing.
///
/// Disabled sources are **in**: a source switched off still takes worklogs, the
/// write queue holds them until it is back, and a suggestion that vanished
/// while a source was paused would be a different suggestion tomorrow for no
/// reason the reader could see.
async fn logging_namespaces(
    pool: &PgPool,
    registry: &dyn knobas_sync::scheduler::AdapterRegistry,
) -> Result<HashSet<String>, IpcError> {
    let kinds: HashSet<String> = registry
        .descriptors()
        .into_iter()
        .filter(|d| d.write_ops.iter().any(|op| op == LOG_WORK))
        .map(|d| d.adapter_kind.to_owned())
        .collect();
    Ok(knobas_sync::config::list(pool)
        .await
        .map_err(IpcError::internal)?
        .into_iter()
        .filter(|source| kinds.contains(&source.adapter_kind))
        .map(|source| source.id)
        .collect())
}

/// Whether `entity_id` is a ticket in the only sense this module has one: its
/// source declares `log_work`.
///
/// Named apart from [`super::worklog`]'s `takes_a_worklog`, which asks the same
/// question one entity and one configuration read at a time. This is the same
/// rule read off a set built once -- see [`logging_namespaces`] -- and two
/// functions of one name in sibling modules would read as one function moved.
fn worklog_can_go_to(entity_id: &str, namespaces: &HashSet<String>) -> bool {
    EntityRef::parse(entity_id).is_ok_and(|reference| namespaces.contains(&reference.namespace))
}

async fn title_of(pool: &PgPool, entity_id: &str) -> Result<Option<String>, IpcError> {
    let title: Option<String> = sqlx::query_scalar(TITLE)
        .bind(entity_id)
        .fetch_optional(pool)
        .await?
        .flatten();
    Ok(title.filter(|title| !title.trim().is_empty()))
}

/// Rule one: the most recently confirmed link from the target to a ticket.
///
/// `None` for a label block -- there is no entity to have links -- and that is
/// not a special case but the shape of the data: an ad-hoc label is work with
/// no entity behind it, so there is nothing for a link to touch.
async fn linked_to_target(
    pool: &PgPool,
    target: Option<&str>,
    namespaces: &HashSet<String>,
) -> Result<Option<String>, IpcError> {
    let Some(target) = target else {
        return Ok(None);
    };
    let rows = sqlx::query(LINKED).bind(target).fetch_all(pool).await?;
    for row in &rows {
        let other: String = row.try_get("other")?;
        if worklog_can_go_to(&other, namespaces) {
            return Ok(Some(other));
        }
    }
    Ok(None)
}

/// Rule two: the anchor of the stored context the block ran in.
async fn context_anchor(
    pool: &PgPool,
    context_id: Option<&str>,
    namespaces: &HashSet<String>,
) -> Result<Option<String>, IpcError> {
    let Some(context_id) = context_id else {
        return Ok(None);
    };
    let anchor: Option<String> = sqlx::query_scalar(ANCHOR)
        .bind(context_id)
        .fetch_optional(pool)
        .await?
        .flatten();
    Ok(anchor.filter(|anchor| worklog_can_go_to(anchor, namespaces)))
}

/// Rule three: the last ticket logged to on the block's own day.
///
/// The day is the reader's, and it is **the block's day** rather than the day
/// the dialog happens to be open on -- the rule #280's draft follows for the
/// same reason (`App.svelte`'s `draftWorklog`): a stop at 00:10 closes an
/// afternoon that belongs to yesterday, and a suggestion drawn from the new
/// day's empty worklog list would answer nothing with no way to tell why.
async fn last_logged(
    pool: &PgPool,
    day: NaiveDate,
    offset_minutes: i32,
    namespaces: &HashSet<String>,
) -> Result<Option<String>, IpcError> {
    let (from, to) = day_bounds(day, offset_minutes)?;
    let logged: Vec<String> = sqlx::query_scalar(LAST_LOGGED)
        .bind(from)
        .bind(to)
        .fetch_all(pool)
        .await?;
    Ok(logged
        .into_iter()
        .find(|entity_id| worklog_can_go_to(entity_id, namespaces)))
}

/// What the ad-hoc dialog should show for `block_id`, or `None` when the block
/// is not an ad-hoc one at all.
///
/// See [`AdHocBlock`] for what the two absences mean, and the module docs for
/// the three rules and their order.
///
/// # Errors
/// [`NotFound`](crate::IpcErrorCode::NotFound) for a block that is not there,
/// [`Invalid`](crate::IpcErrorCode::Invalid) for an offset that is not an
/// offset, [`IpcError`] if a read fails.
pub async fn offer(
    pool: &PgPool,
    registry: &dyn knobas_sync::scheduler::AdapterRegistry,
    block_id: i64,
    day: NaiveDate,
    offset_minutes: i32,
) -> Result<Option<AdHocBlock>, IpcError> {
    let row = sqlx::query(READ_BLOCK)
        .bind(block_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| {
            IpcError::not_found(format!(
                "there is no block {block_id} -- it may have been deleted in another window"
            ))
        })?;
    let target: Option<String> = row.try_get("entity_id")?;
    let context_id: Option<String> = row.try_get("context_id")?;

    let namespaces = logging_namespaces(pool, registry).await?;
    if target
        .as_deref()
        .is_some_and(|target| worklog_can_go_to(target, &namespaces))
    {
        // The block is on a ticket. Its time goes to that ticket, and the
        // worklog draft is the surface for that -- offering to re-target it
        // onto a *different* ticket would be knobas asking a person who timed
        // PAY-231 whether they really meant PAY-104.
        return Ok(None);
    }

    // The order is the whole rule. Each arm answers `None` when its own input
    // is missing, so "no room" and "no links" fall through rather than
    // shortcutting to no suggestion at all.
    let found = match linked_to_target(pool, target.as_deref(), &namespaces).await? {
        Some(entity_id) => Some((entity_id, SuggestionRule::LinkedToTarget)),
        None => match context_anchor(pool, context_id.as_deref(), &namespaces).await? {
            Some(entity_id) => Some((entity_id, SuggestionRule::ContextAnchor)),
            None => last_logged(pool, day, offset_minutes, &namespaces)
                .await?
                .map(|entity_id| (entity_id, SuggestionRule::LastLogged)),
        },
    };

    let suggestion = match found {
        Some((entity_id, rule)) => Some(Suggestion {
            title: title_of(pool, &entity_id).await?,
            entity_id,
            rule,
        }),
        None => None,
    };
    Ok(Some(AdHocBlock { suggestion }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The rule names on the wire. Read here as well as against the mirror,
    /// because these three words are what the dialog switches its sentence on:
    /// a rename that only failed `svelte-check` would leave the dialog drawing
    /// a suggestion with no reason beside it.
    #[test]
    fn every_rule_is_spelled_in_snake_case_on_the_wire() {
        for (rule, word) in [
            (SuggestionRule::LinkedToTarget, "linked_to_target"),
            (SuggestionRule::ContextAnchor, "context_anchor"),
            (SuggestionRule::LastLogged, "last_logged"),
        ] {
            assert_eq!(serde_json::to_value(rule).unwrap(), serde_json::json!(word));
        }
    }

    /// A candidate is a ticket because its **source** takes worklogs, so an id
    /// in a namespace nobody configured is not one -- however ticket-shaped it
    /// reads.
    #[test]
    fn only_a_configured_logging_namespace_makes_an_id_a_ticket() {
        let namespaces: HashSet<String> = ["jira".to_owned(), "jira-eu".to_owned()].into();
        assert!(worklog_can_go_to("jira:PAY-231", &namespaces));
        assert!(worklog_can_go_to("jira-eu:PAY-231", &namespaces));
        assert!(
            !worklog_can_go_to("gitea:tidewater/payments#4", &namespaces),
            "a repo's pull request is not somewhere a worklog can go"
        );
        assert!(
            !worklog_can_go_to("ctx:5b1c0f1e", &namespaces),
            "a stored context is linkable, so it turns up as a candidate -- and \
             it is never a ticket"
        );
        assert!(
            !worklog_can_go_to("PAY-231", &namespaces),
            "a string that is not an entity id is not a ticket"
        );
    }
}
