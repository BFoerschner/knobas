//! A pull request's review actions in its toolbar (issue #705, spec #701
//! workflows 6 and 7): *Comment…*, *Approve…* and *Request changes…*. One
//! action per verdict (the deputy's ruling on #705), the verdict a fixed value
//! ([`ViewAction::values`]), so each form's `required` is honest: an
//! approval's words are optional, a request for changes' are not, which is
//! Gitea's own rule (*review event REQUEST_CHANGES requires a body*).

use knobas_source::ReviewVerdict;
use knobas_source::view::{ActionField, Commenting, FieldKind, LineComments, ViewAction};

use super::{Pull, login};

/// The toolbar's conversation actions, first in it: what `viewer` -- the
/// token's own login, `None` where it could not be read -- may say on `pull`.
///
/// *Submit review…* -- the reader's pending line comments as a review that
/// only comments (#709) -- first and primary on an open pull request, whoever
/// opened it: Gitea takes a comment-only review from the author too, and the
/// shell draws it only while there are comments to send. *Comment…* always. The two verdicts
/// only where Gitea could ever take them
/// from this account, each a 422 against the real server otherwise: on an
/// open pull request (*can't submit review for a closed or merged PR*) that
/// `viewer` did not open (*approve your own pull is not allowed*, *reject your
/// own pull is not allowed*). Neither can change for this reader, so the
/// actions are absent rather than disabled (the #701 ruling: disabled only
/// where the entity can still reach the enabled state). A closed pull request
/// is reviewed after a reopen, which is its own action. Where the viewer could
/// not be read the verdicts are offered, and Gitea's refusal, should it come,
/// is the write queue's to show (ADR-0004).
pub(super) fn actions(pull: &Pull, viewer: Option<&str>) -> Vec<ViewAction> {
    let open = takes_reviews(pull);
    let own = viewer.is_some() && login(pull.user.as_ref()).as_deref() == viewer;
    let mut actions = Vec::new();
    if open {
        // First and filled: drawn only while comments wait to be sent, and
        // then sending them is the thing to do next (the #709 ruling).
        actions.push(ViewAction {
            line_comments: LineComments::Requires,
            primary: true,
            ..review(ReviewVerdict::Comment, "Submit review…", false)
        });
    }
    actions.push(comment());
    if open && !own {
        actions.push(review(ReviewVerdict::Approve, "Approve…", false));
        actions.push(review(
            ReviewVerdict::RequestChanges,
            "Request changes…",
            true,
        ));
    }
    actions
}

/// Whether Gitea takes a review of `pull` at all: only while it is open
/// (*can't submit review for a closed or merged PR*, a 422).
pub(super) fn takes_reviews(pull: &Pull) -> bool {
    pull.is_open()
}

/// How a reader comments on `pull`'s diff (#709): through `submit_review`,
/// while Gitea takes a review of it; `None` once it is closed or merged.
/// A reply to a thread goes the same way (#2277). Resolving one is offered
/// only where the server `resolves`: Gitea does, Forgejo 17 cannot through
/// its API (405, the fact note of 2026-10-30), so a Forgejo diff offers
/// replies and no resolve.
pub(super) fn commenting(pull: &Pull, resolves: bool) -> Option<Commenting> {
    takes_reviews(pull).then(|| Commenting {
        write_op: crate::WRITE_OP_SUBMIT_REVIEW.to_owned(),
        reply: Some(crate::WRITE_OP_REPLY_TO_THREAD.to_owned()),
        resolve: resolves.then(|| crate::WRITE_OP_RESOLVE_THREAD.to_owned()),
    })
}

/// *Comment…*: a reply on the discussion, through the `comment` op, in any
/// state -- Gitea takes a reply on a closed or merged pull request too.
fn comment() -> ViewAction {
    ViewAction {
        fields: vec![ActionField {
            required: true,
            ..ActionField::new("body", "Comment", FieldKind::LongText)
        }],
        ..ViewAction::new(crate::WRITE_OP_COMMENT, "Comment…")
    }
}

/// A review with `verdict`, labelled `label`, its words `required` or not,
/// sending the reader's pending line comments with it when there are some
/// (#709).
fn review(verdict: ReviewVerdict, label: &str, required: bool) -> ViewAction {
    // The SPI's own word for the verdict, as the shell's payload reads it
    // (`ReviewVerdict`'s snake_case serde form, pinned in `knobas_source`).
    let word = match verdict {
        ReviewVerdict::Approve => "approve",
        ReviewVerdict::RequestChanges => "request_changes",
        ReviewVerdict::Comment => "comment",
    };
    ViewAction {
        fields: vec![ActionField {
            required,
            ..ActionField::new("body", "Comment", FieldKind::LongText)
        }],
        values: [("verdict".to_owned(), word.to_owned())].into(),
        line_comments: LineComments::Sends,
        ..ViewAction::new(crate::WRITE_OP_SUBMIT_REVIEW, label)
    }
}
