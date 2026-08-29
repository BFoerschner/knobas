//! What knobas **proposes**, before anything happens (issue #44).
//!
//! Every value the flow will send to a source is composed here, from the
//! ticket and from the repository the user chose, and is then *shown* --
//! nothing is created from an unreviewed automatic value (ratified). The user
//! edits the branch name, the pull request title and its body; this module's
//! job is that they rarely have to.
//!
//! # Why this file names `WriteOp`
//!
//! Because the alternative is worse. A step's proposal is stored as the
//! serialized op it will submit, so that "what was shown" and "what was sent"
//! are one value rather than two that agree until somebody edits one of the
//! two code paths. Building that value by hand as a `serde_json::json!` would
//! put the SPI's serde shape in a string literal, where renaming a field is a
//! run-time surprise instead of a compile error.
//!
//! So this file is an entry in `write_choke_point.rs`'s `HANDS_TO_THE_QUEUE`:
//! it *builds* ops and hands them on, and -- like the amendment guard, the
//! list's other entry -- it may not contain a dispatch of one. The orchestrator
//! next door never names the enum at all; it moves payloads.

use knobas_core::entity::EntityRef;
use knobas_core::start_work::{PlannedStep, Step};
use knobas_source::WriteOp;
use sqlx::PgPool;

use crate::IpcError;

/// The status a ticket moves to when work starts.
///
/// One of exactly two transitions this feature is allowed to make, and the
/// spelling is the source's own: `WriteOp::Transition` carries the status the
/// user picked in the source's words, never a transition id, and the adapter
/// resolves it against what that source says is reachable *right now*.
pub const IN_PROGRESS: &str = "In Progress";

/// The status a ticket moves to when its pull request is merged.
///
/// The other ratified transition. It lives here rather than in `merge.rs`
/// because the two are one decision -- "any status other than the ratified two
/// transitions" is out of scope -- and a reader checking that should find both
/// in one place.
pub const IN_REVIEW: &str = "In Review";

/// The relation the pull-request link carries.
///
/// `implements` rather than the link store's `related` default: the panel
/// groups by relation, and a pull request that implements a ticket is the one
/// relationship in this flow anybody would name. It is folded to lowercase by
/// the link write like any other.
pub const RELATION: &str = "implements";

/// What Gitea reads as *this pull request is not ready for review*.
///
/// **Story 8 -- "opened as a draft by default" -- is met by convention rather
/// than by a flag, and that was forced.** `WriteOp::CreatePullRequest` has no
/// `draft` field, and adding one would be a growth of the SPI's op enum: a
/// frozen surface under ADR-0006 needing its own ratified exception, which
/// this issue is explicitly not (it "introduces no new SPI surface"). Gitea's
/// own answer to the same question is the title prefix -- its
/// `WORK_IN_PROGRESS_PREFIXES` default is `WIP:,[WIP]` -- so the draft state is
/// expressed in the one field the op already carries.
///
/// It is in the **proposal**, which means the user sees it in the review step
/// and deletes it if they want reviewers now. A flag they could not see would
/// have been worse.
pub const DRAFT_PREFIX: &str = "WIP: ";

/// The branch a proposal starts from when the mirror cannot say.
///
/// The repository's own `default_branch` is what is used when it is there;
/// this is for a repository knobas has not mirrored, which is a legal target
/// (a create's container need not be an item). It is a guess, it is *shown*
/// before it is used, and the user changes it.
const FALLBACK_BASE: &str = "main";

/// The most a proposed branch name may take from a ticket title.
///
/// Long enough to stay recognisable, short enough that the whole name fits
/// where branch names get shown. The user edits it either way.
const SLUG_LIMIT: usize = 40;

/// What a proposal was made from -- the two facts every step needs.
#[derive(Clone, Debug)]
pub struct Subject {
    /// The ticket the flow is about.
    pub ticket: EntityRef,
    /// Its title, as the mirror holds it.
    pub title: String,
    /// The repository the branch and the pull request go in.
    pub repo: EntityRef,
    /// The branch the new one starts from, and the pull request's base.
    pub base: String,
}

/// Read what a proposal is made from.
///
/// The ticket must be mirrored -- its title is half the proposal. The
/// repository need not be: `WriteOp`'s creating variants take a container, and
/// a container knobas does not mirror is a legal target (#43). What is lost by
/// not mirroring it is the default branch, and [`FALLBACK_BASE`] is what stands
/// in.
///
/// # Errors
///
/// [`NotFound`](crate::IpcErrorCode::NotFound) if the ticket is not in the
/// mirror; [`Internal`](crate::IpcErrorCode::Internal) for a query failure.
pub async fn subject(
    pool: &PgPool,
    ticket: &EntityRef,
    repo: &EntityRef,
) -> Result<Subject, IpcError> {
    let title: Option<String> =
        sqlx::query_scalar("select title from sync.live_item where entity_id = $1")
            .bind(ticket.to_string())
            .fetch_optional(pool)
            .await
            .map_err(IpcError::internal)?;
    let title = title.ok_or_else(|| {
        IpcError::not_found(format!(
            "{ticket} is not in the mirror, so there is nothing to start work on"
        ))
    })?;

    // `default_branch` is a Gitea repository record's own field. Read through
    // `->>` and defaulted, never required: this is the one place the proposal
    // benefits from knowing a source's payload shape, the cost of being wrong
    // is a base branch the user corrects in the review step, and no other part
    // of the flow depends on it.
    let base: Option<String> = sqlx::query_scalar(
        "select payload->>'default_branch' from sync.live_item where entity_id = $1",
    )
    .bind(repo.to_string())
    .fetch_optional(pool)
    .await
    .map_err(IpcError::internal)?
    .flatten();

    Ok(Subject {
        ticket: ticket.clone(),
        title,
        repo: repo.clone(),
        base: base.unwrap_or_else(|| FALLBACK_BASE.to_owned()),
    })
}

/// The branch name knobas proposes for a ticket.
///
/// `feature/<KEY>-<slug of the title>` -- the shape the Tidewater corpus
/// already uses, so the proposal reads like the branches beside it. Derived
/// from the **ticket** and from nothing else: guessing a convention from
/// repository history is out of scope, and a proposal that changed depending on
/// what somebody pushed last week is one nobody could predict.
///
/// A ticket whose title slugs to nothing (punctuation, a script this slugger
/// does not transliterate) yields `feature/<KEY>`, which is still a legal and
/// recognisable branch name.
#[must_use]
pub fn branch_name(key: &str, title: &str) -> String {
    let slug = slug(title);
    if slug.is_empty() {
        format!("feature/{key}")
    } else {
        format!("feature/{key}-{slug}")
    }
}

/// A title, as a branch-name fragment.
///
/// ASCII-alphanumeric runs joined by single hyphens, lowercased, cut to
/// [`SLUG_LIMIT`] at a hyphen so the name never ends mid-word or on a
/// separator. Deliberately conservative about what survives: git refuses a
/// handful of characters outright (`~ ^ : ? * [ \`, a space, `..`, a trailing
/// `.`) and mangles others per platform, and a proposal the user has to repair
/// before it will work is not a proposal.
fn slug(title: &str) -> String {
    let mut out = String::new();
    for ch in title.chars() {
        if ch.is_ascii_alphanumeric() {
            out.extend(ch.to_lowercase());
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    let trimmed = out.trim_end_matches('-');
    if trimmed.len() <= SLUG_LIMIT {
        return trimmed.to_owned();
    }
    let cut = &trimmed[..SLUG_LIMIT];
    // Cut back to the last whole word rather than through one.
    match cut.rfind('-') {
        Some(boundary) if boundary > 0 => cut[..boundary].to_owned(),
        _ => cut.trim_end_matches('-').to_owned(),
    }
}

/// The body knobas proposes for the pull request.
///
/// The ticket's own title and its address, so a reviewer arriving at the pull
/// request knows what it is for without opening anything, and so the
/// relationship is legible to people who do not run knobas. The knobas link is
/// the relationship that *survives*; this is the courtesy copy.
#[must_use]
pub fn pull_request_body(ticket: &EntityRef, title: &str) -> String {
    format!("{}\n\n{title}\n\nStarted from knobas.", ticket.key)
}

/// The whole sequence, proposed.
///
/// Four steps in the order they have to happen: the branch, the pull request
/// from it, the link back to the ticket, and the ticket's status. Every one of
/// them is editable and skippable before and while it runs -- this is the
/// value the stepper is shown, not a commitment.
#[must_use]
pub fn propose(subject: &Subject) -> Vec<PlannedStep> {
    let branch = branch_name(&subject.ticket.key, &subject.title);
    let repo = subject.repo.to_string();
    vec![
        planned(
            Step::CreateBranch,
            &WriteOp::CreateBranch {
                entity: repo.clone(),
                name: branch.clone(),
                from_ref: subject.base.clone(),
            },
        ),
        planned(
            Step::CreatePullRequest,
            &WriteOp::CreatePullRequest {
                entity: repo,
                title: format!("{DRAFT_PREFIX}{}", subject.title),
                body: pull_request_body(&subject.ticket, &subject.title),
                head: branch,
                base: subject.base.clone(),
            },
        ),
        PlannedStep {
            step: Step::LinkPullRequest,
            // Not a `WriteOp`, and that is the point: the link is knobas-owned
            // and local, written to neither source, which is why the
            // relationship survives whatever Jira and Gitea record (story 9).
            payload: serde_json::json!({ "relation": RELATION }),
        },
        PlannedStep {
            step: Step::Transition,
            payload: transition(&subject.ticket, IN_PROGRESS),
        },
    ]
}

/// The serialized op that moves `ticket` to `status`.
///
/// Composed here rather than where it is dispatched, so that this file stays
/// the only one in the flow that names the SPI's op enum -- the reverse
/// direction (`merge.rs`) moves the payload and never sees the type. Both
/// ratified transitions go through it.
#[must_use]
pub fn transition(ticket: &EntityRef, status: &str) -> serde_json::Value {
    serde_json::to_value(WriteOp::Transition {
        entity: ticket.to_string(),
        status: status.to_owned(),
    })
    .expect("a write op is a map of strings")
}

/// One step, carrying the op it will submit.
///
/// The serialization cannot fail -- every `WriteOp` variant is a struct of
/// owned strings -- and a `Result` here would be an error path no caller could
/// do anything about, so it is asserted rather than propagated.
fn planned(step: Step, op: &WriteOp) -> PlannedStep {
    PlannedStep {
        step,
        payload: serde_json::to_value(op).expect("a write op is a map of strings"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ticket() -> EntityRef {
        EntityRef::new("jira", "PAY-231")
    }

    fn subject(title: &str) -> Subject {
        Subject {
            ticket: ticket(),
            title: title.to_owned(),
            repo: EntityRef::new("gitea", "tidewater/payout-service"),
            base: "main".to_owned(),
        }
    }

    /// The convention, and the fact that it comes from the ticket alone.
    #[test]
    fn a_branch_name_is_the_key_and_a_slug_of_the_title() {
        assert_eq!(
            branch_name("PAY-231", "SEPA retry storm on payout"),
            "feature/PAY-231-sepa-retry-storm-on-payout"
        );
    }

    /// A title made of punctuation still yields a legal branch name, rather
    /// than one ending in a separator or nothing at all.
    #[test]
    fn a_title_that_slugs_to_nothing_still_proposes_a_usable_name() {
        assert_eq!(branch_name("PAY-9", "!!! ??? ..."), "feature/PAY-9");
        assert_eq!(
            branch_name("PAY-9", "  spaced  out  "),
            "feature/PAY-9-spaced-out"
        );
    }

    /// Git refuses several characters outright and mangles others. A proposal
    /// the user has to repair before it will work is not a proposal.
    #[test]
    fn a_proposed_branch_name_carries_nothing_git_refuses() {
        let name = branch_name("PAY-231", "fix: a~b^c:d?e*f[g\\h and a space");
        for forbidden in ['~', '^', ':', '?', '*', '[', '\\', ' '] {
            assert!(
                !name.contains(forbidden),
                "{name:?} carries {forbidden:?}, which git refuses in a ref"
            );
        }
        assert!(!name.contains(".."), "{name:?} carries `..`");
        assert!(!name.ends_with('.'), "{name:?} ends in a dot");
    }

    /// A very long title is cut at a word rather than through one, and the cut
    /// never leaves a trailing separator.
    #[test]
    fn a_long_title_is_cut_at_a_word_boundary() {
        let name = branch_name(
            "PAY-231",
            "the payout dashboard latency regression that appeared after the ledger migration",
        );
        assert!(name.len() < 60, "{name:?} is too long to be readable");
        assert!(!name.ends_with('-'), "{name:?} ends on a separator");
        assert!(
            name.starts_with("feature/PAY-231-the-payout-dashboard"),
            "{name:?} lost the front of the title"
        );
    }

    /// Story 8, and the shape it had to take. The op has no `draft` field --
    /// adding one would grow the SPI -- so the draft state is the title prefix
    /// Gitea itself reads, and it is *in the proposal* where the user can
    /// delete it.
    #[test]
    fn the_proposed_pull_request_is_a_draft_by_a_prefix_the_user_can_see() {
        let steps = propose(&subject("payout dashboard latency"));
        let pr = steps
            .iter()
            .find(|s| s.step == Step::CreatePullRequest)
            .expect("the sequence proposes a pull request");
        let title = pr.payload["CreatePullRequest"]["title"]
            .as_str()
            .expect("a title");
        assert!(
            title.starts_with(DRAFT_PREFIX),
            "{title:?} would summon reviewers the moment work started"
        );
        assert!(title.contains("payout dashboard latency"));
    }

    /// The sequence, and the join between its first two steps: the pull
    /// request's `head` is the branch the first step creates, and its `base` is
    /// what that branch was cut from. A proposal whose two halves disagreed
    /// would open a pull request pointing at nothing.
    #[test]
    fn the_pull_request_is_proposed_from_the_branch_the_first_step_creates() {
        let steps = propose(&subject("SEPA retry storm"));
        let branch = steps[0].payload["CreateBranch"]["name"].as_str().unwrap();
        let pr = &steps[1].payload["CreatePullRequest"];
        assert_eq!(pr["head"].as_str(), Some(branch));
        assert_eq!(
            pr["base"].as_str(),
            steps[0].payload["CreateBranch"]["from_ref"].as_str()
        );
    }

    /// The four steps, in the order they have to happen. Anything else opens a
    /// pull request before its branch exists or moves the ticket before there
    /// is anything to review.
    #[test]
    fn the_sequence_is_branch_then_pull_request_then_link_then_status() {
        let steps: Vec<_> = propose(&subject("t")).into_iter().map(|s| s.step).collect();
        assert_eq!(
            steps,
            vec![
                Step::CreateBranch,
                Step::CreatePullRequest,
                Step::LinkPullRequest,
                Step::Transition
            ]
        );
    }

    /// The ratified transitions, both of them, and no third.
    #[test]
    fn the_only_status_the_flow_proposes_is_in_progress() {
        let steps = propose(&subject("t"));
        let transition = steps.last().expect("a transition step");
        assert_eq!(
            transition.payload["Transition"]["status"].as_str(),
            Some(IN_PROGRESS)
        );
        assert_eq!(
            transition.payload["Transition"]["entity"].as_str(),
            Some("jira:PAY-231")
        );
    }

    /// **The reverse direction's `not exists` reads this shape in SQL**, which
    /// a rename of the variant or of the field would silently make match
    /// nothing -- and a memory that matches nothing transitions the ticket on
    /// every pass. So the path `payload->'Transition'->>'status'` is pinned
    /// here, against the value that is actually stored.
    #[test]
    fn the_transition_payload_is_shaped_the_way_the_reverse_direction_reads_it() {
        let payload = transition(&ticket(), IN_REVIEW);
        assert_eq!(
            payload
                .get("Transition")
                .and_then(|op| op.get("status"))
                .and_then(serde_json::Value::as_str),
            Some(IN_REVIEW),
            "merge.rs reads payload->'Transition'->>'status' and would find nothing"
        );
        assert!(
            crate::start_work::merge::MERGED_AND_LINKED_PATH == "payload->'Transition'->>'status'",
            "the statement and this pin have to name the same path"
        );
    }

    /// The body carries the ticket back to whoever reads the pull request in
    /// Gitea, where knobas' own link cannot reach.
    #[test]
    fn the_proposed_body_names_the_ticket() {
        let body = pull_request_body(&ticket(), "payout dashboard latency");
        assert!(body.contains("PAY-231"), "{body:?}");
        assert!(body.contains("payout dashboard latency"), "{body:?}");
    }
}
