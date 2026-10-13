//! A pull request's maintenance actions in its toolbar (issue #706, spec #701
//! workflow 8): *Edit…*, and *Close* or *Reopen*. All three are
//! `edit_pull_request`, one `PATCH`; close and reopen are told apart by the
//! state they fix ([`ViewAction::values`], the #705 ruling), never by label.

use knobas_source::PullRequestState;
use knobas_source::view::{ActionField, FieldKind, ViewAction};

use super::Pull;

/// The toolbar's maintenance actions, after the reviews and before *Merge…*
/// (the #701 ruling's order).
///
/// *Edit…* in every state: Gitea takes a new title or description on a closed
/// or merged pull request too. *Close* on an open one, *Reopen* on a closed one
/// that was not merged. A merged pull request is offered neither: Gitea
/// refuses its state with a 412 (*cannot change state of this pull request,
/// it was already merged*, measured on 1.27.3), and nothing the reader waits
/// for changes that, so both are absent rather than disabled (the #701
/// ruling).
pub(super) fn actions(pull: &Pull) -> Vec<ViewAction> {
    let mut actions = vec![edit(pull)];
    if pull.merged {
        return actions;
    }
    match pull.state.as_deref() {
        Some("open") => actions.push(close(pull)),
        Some("closed") => actions.push(set_state("Reopen", PullRequestState::Open)),
        _ => {}
    }
    actions
}

/// *Edit…*: the title and the description, each prefilled with exactly what
/// Gitea holds -- untrimmed, so the shell can tell an untouched field from a
/// changed one and send only the changed (the deputy's ruling on #706).
fn edit(pull: &Pull) -> ViewAction {
    ViewAction {
        fields: vec![
            ActionField {
                value: pull.title.clone(),
                // Gitea refuses an empty title; the dialog says so first.
                required: true,
                ..ActionField::new(
                    "title",
                    "Title",
                    FieldKind::Text {
                        suggestions: Vec::new(),
                    },
                )
            },
            ActionField {
                value: pull.body.clone(),
                ..ActionField::new("body", "Description", FieldKind::LongText)
            },
        ],
        ..ViewAction::new(crate::WRITE_OP_EDIT_PULL_REQUEST, "Edit…")
    }
}

/// *Close*: it ends the pull request unmerged, so a danger, through a sentence
/// naming what goes where.
fn close(pull: &Pull) -> ViewAction {
    let head = pull.head.as_ref().and_then(|b| b.r#ref.as_deref());
    let base = pull.base.as_ref().and_then(|b| b.r#ref.as_deref());
    let confirm = match (head, base) {
        (Some(head), Some(base)) => format!("Close {head} → {base} without merging it?"),
        _ => "Close this pull request without merging it?".to_owned(),
    };
    ViewAction {
        confirm: Some(confirm),
        danger: true,
        ..set_state("Close", PullRequestState::Closed)
    }
}

/// The action labelled `label` that puts the pull request in `state`, fixed
/// as the SPI's word for it.
fn set_state(label: &str, state: PullRequestState) -> ViewAction {
    ViewAction {
        values: [("state".to_owned(), state.as_str().to_owned())].into(),
        ..ViewAction::new(crate::WRITE_OP_EDIT_PULL_REQUEST, label)
    }
}
