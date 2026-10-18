//! A pull request's source action in its toolbar (issue #1194, ADR-0029):
//! *Add label…*, a write no role's vocabulary has. Its dialog is declared
//! here in ADR-0020's action vocabulary; the shell draws it and queues
//! `source_action` with the answers, never knowing what a label is.

use knobas_source::view::{ActionField, ChoiceOption, FieldKind, ViewAction};

use super::Pull;

/// One label as Gitea lists it, a repository's or a pull request's.
#[derive(Debug, Clone, serde::Deserialize)]
pub(super) struct Label {
    pub(super) id: u64,
    #[serde(default)]
    pub(super) name: String,
}

/// *Add label…*: the repository's labels the pull request does not carry, as
/// a required choice, in every state (Gitea labels a closed or merged pull
/// request too). Where none is left, or the labels could not be read, the
/// action is drawn disabled saying why: a label can be made or taken off, so
/// the reader can still get to an enabled one (the #701 ruling).
pub(super) fn action(pull: &Pull, labels: &Result<Vec<Label>, String>) -> ViewAction {
    let base = ViewAction {
        values: [(
            knobas_source::SOURCE_ACTION_ID.to_owned(),
            crate::SOURCE_ACTION_ADD_LABEL.to_owned(),
        )]
        .into(),
        ..ViewAction::new(knobas_source::SOURCE_ACTION, "Add label…")
    };
    let disabled = |reason: String| ViewAction {
        disabled: Some(reason),
        ..base.clone()
    };
    let labels = match labels {
        Ok(labels) => labels,
        Err(error) => return disabled(format!("Could not read this repository's labels: {error}")),
    };
    if labels.is_empty() {
        return disabled("This repository has no labels.".to_owned());
    }
    let options: Vec<ChoiceOption> = labels
        .iter()
        .filter(|label| !pull.labels.iter().any(|on| on.id == label.id))
        .map(|label| ChoiceOption::new(label.id.to_string(), label.name.clone()))
        .collect();
    if options.is_empty() {
        return disabled("Every label of this repository is on this pull request.".to_owned());
    }
    ViewAction {
        fields: vec![ActionField {
            required: true,
            ..ActionField::new("label", "Label", FieldKind::Choice { options })
        }],
        ..base
    }
}
