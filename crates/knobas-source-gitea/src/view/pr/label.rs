//! A pull request's source action in its toolbar (issue #1194, ADR-0029):
//! *Add label…*, a write no role's vocabulary has. Its dialog is declared
//! here in ADR-0020's action vocabulary; the shell draws it and queues
//! `source_action` with the answers, never knowing what a label is.

use knobas_source::SourceError;
use knobas_source::view::{ActionField, ChoiceOption, FieldKind, ViewAction};

use super::Pull;
use crate::GiteaSource;
use crate::client::PAGE_SIZE;

/// How many pages of fifty labels one listing is read to at most: a
/// thousand labels, past which the rest is not offered.
const LABEL_PAGES: u32 = 20;

/// One label as Gitea lists it, a repository's or a pull request's.
#[derive(Debug, Clone, serde::Deserialize)]
pub(super) struct Label {
    pub(super) id: u64,
    #[serde(default)]
    pub(super) name: String,
}

/// The labels a pull request of `owner/repo` may carry (#1235): every page of
/// the repository's own, then, where `owner` is an organisation, every page
/// of the organisation's. Gitea's 404 for the organisation's is a user owner,
/// which has none. Any other failure is the reason the action is disabled.
pub(super) async fn read(
    source: &GiteaSource,
    owner: &str,
    repo: &str,
) -> Result<Vec<Label>, String> {
    let client = &source.client;
    let own = every(|page| client.labels_as(owner, repo, page));
    let org = async {
        match every(|page| client.org_labels_as(owner, page)).await {
            Err(error) if error.status() == Some(404) => Ok(Vec::new()),
            read => read,
        }
    };
    let (mut labels, org) = tokio::try_join!(own, org).map_err(|error| error.to_string())?;
    labels.extend(org);
    Ok(labels)
}

/// Every label of one listing, page by page from the first, at most
/// [`LABEL_PAGES`] pages. The listing is whole once Gitea's `X-Total-Count`
/// is reached, or on an empty page; where Gitea sent no count, on a page
/// shorter than fifty.
async fn every<F, Fut>(mut page_of: F) -> Result<Vec<Label>, SourceError>
where
    F: FnMut(u32) -> Fut,
    Fut: Future<Output = Result<(Vec<Label>, Option<usize>), SourceError>>,
{
    let mut labels = Vec::new();
    for page in 1..=LABEL_PAGES {
        let (batch, total) = page_of(page).await?;
        let read = batch.len();
        labels.extend(batch);
        let whole = match total {
            Some(total) => labels.len() >= total,
            None => read < PAGE_SIZE as usize,
        };
        if whole || read == 0 {
            break;
        }
    }
    Ok(labels)
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
            ..ActionField::new(
                crate::ADD_LABEL_FIELD,
                "Label",
                FieldKind::Choice { options },
            )
        }],
        ..base
    }
}
