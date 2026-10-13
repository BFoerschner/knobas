//! A pull request's review actions in its toolbar (issue #705, spec #701
//! workflows 6 and 7): *Comment…*, *Approve…* and *Request changes…*. One
//! action per verdict (the deputy's ruling on #705), the verdict a fixed value
//! ([`ViewAction::values`]), so each form's `required` is honest: an
//! approval's words are optional, a request for changes' are not, which is
//! Gitea's own rule (*review event REQUEST_CHANGES requires a body*).

use knobas_source::ReviewVerdict;
use knobas_source::view::{ActionField, FieldKind, ViewAction};

use super::{Pull, login};

/// The toolbar's conversation actions, first in it: what `viewer` -- the
/// token's own login, `None` where it could not be read -- may say on `pull`.
///
/// *Comment…* always. The two verdicts only where Gitea could ever take them
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
    let mut actions = vec![comment()];
    let open = !pull.merged && pull.state.as_deref() == Some("open");
    let own = viewer.is_some() && login(pull.user.as_ref()).as_deref() == viewer;
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

/// A review with `verdict`, labelled `label`, its words `required` or not.
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
        ..ViewAction::new(crate::WRITE_OP_SUBMIT_REVIEW, label)
    }
}
