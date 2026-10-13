//! A pull request's review actions in its toolbar (issue #705, spec #701
//! workflows 6 and 7): *Comment…*, *Approve…* and *Request changes…*. One
//! action per verdict (the deputy's ruling on #705), the verdict a fixed value
//! ([`ViewAction::values`]), so each form's `required` is honest: an
//! approval's words are optional, a request for changes' are not, which is
//! Gitea's own rule (*review event REQUEST_CHANGES requires a body*).

use knobas_source::ReviewVerdict;
use knobas_source::view::{ActionField, FieldKind, ViewAction};

use super::{Pull, login};

/// The toolbar's conversation actions, after *Merge…*: what `viewer` -- the
/// token's own login, `None` where it could not be read -- may say on `pull`.
pub(super) fn actions(pull: &Pull, viewer: Option<&str>) -> Vec<ViewAction> {
    vec![
        comment(),
        review(
            pull,
            viewer,
            ReviewVerdict::Approve,
            "Approve…",
            false,
            "You opened this pull request: Gitea does not let its author approve it",
        ),
        review(
            pull,
            viewer,
            ReviewVerdict::RequestChanges,
            "Request changes…",
            true,
            "You opened this pull request: Gitea does not let its author request changes",
        ),
    ]
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
/// drawn disabled where Gitea would refuse it, each a 422 against the real
/// server: any review of a closed or merged pull request (*can't submit review
/// for a closed or merged PR*), and its author's approval or request for
/// changes (*approve your own pull is not allowed*, *reject your own pull is
/// not allowed*), which says `own_reason`.
fn review(
    pull: &Pull,
    viewer: Option<&str>,
    verdict: ReviewVerdict,
    label: &str,
    required: bool,
    own_reason: &str,
) -> ViewAction {
    let own = viewer.is_some() && login(pull.user.as_ref()).as_deref() == viewer;
    let disabled = if pull.merged {
        Some("Already merged: Gitea takes no review on a merged pull request".to_owned())
    } else if pull.state.as_deref() != Some("open") {
        Some("Closed: Gitea takes no review on a closed pull request".to_owned())
    } else if own {
        Some(own_reason.to_owned())
    } else {
        None
    };
    // The SPI's own word for the verdict, as the shell's payload reads it.
    let word = serde_json::to_value(verdict)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default();
    ViewAction {
        fields: vec![ActionField {
            required,
            ..ActionField::new("body", "Comment", FieldKind::LongText)
        }],
        disabled,
        values: [("verdict".to_owned(), word)].into(),
        ..ViewAction::new(crate::WRITE_OP_SUBMIT_REVIEW, label)
    }
}
