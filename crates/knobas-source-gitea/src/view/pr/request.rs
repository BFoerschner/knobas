//! *Request…* in a pull request's Reviewers panel (#2303, spec #2234 step
//! 6d): ask somebody to review, through `request_reviewers`.
//!
//! Offered where Gitea would take the request from this reader (ADR-0020
//! rule 4): on an open pull request, to its author or to an account that may
//! push to the base repository -- Gitea's `CanDoerChangeReviewRequests`. Any
//! other reader is refused with a 403 by Gitea, so the action is absent for
//! them, not disabled: nothing the reader can do here changes it. The people
//! offered are the ones Gitea says a review can be requested from
//! (`GET …/reviewers`), less the author, whom Gitea refuses, and those already
//! asked. A failed read of them, or nobody left to ask, offers nothing.

use knobas_source::view::{ActionField, ChoiceOption, FieldKind, ViewAction};

use super::{Pull, login};
use crate::model::UserRef;

/// The field the picked logins go under, which the shell sends as
/// `RequestReviewers::reviewers`.
pub(super) const REVIEWERS: &str = "reviewers";

/// *Request…*, or `None` where this reader may not ask or there is nobody
/// to ask. `requested` are the logins already asked.
pub(super) fn action(
    pull: &Pull,
    viewer: Option<&str>,
    candidates: &[UserRef],
    requested: &[String],
) -> Option<ViewAction> {
    if !pull.is_open() {
        return None;
    }
    let author = login(pull.user.as_ref());
    let own = viewer.is_some() && author.as_deref() == viewer;
    if !own && !pull.may_push() {
        return None;
    }
    let options: Vec<ChoiceOption> = candidates
        .iter()
        .filter_map(|user| login(Some(user)))
        .filter(|who| Some(who) != author.as_ref() && !requested.contains(who))
        .map(|who| ChoiceOption::new(who.clone(), who))
        .collect();
    if options.is_empty() {
        return None;
    }
    Some(ViewAction {
        fields: vec![ActionField {
            required: true,
            ..ActionField::new(REVIEWERS, "Reviewer", FieldKind::Choice { options })
        }],
        ..ViewAction::new(crate::WRITE_OP_REQUEST_REVIEWERS, "Request…")
    })
}
