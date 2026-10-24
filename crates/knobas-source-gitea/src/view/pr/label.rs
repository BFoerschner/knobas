//! A pull request's source action in its toolbar (issue #1194, ADR-0029):
//! *Add label…*, a write no role's vocabulary has. Its dialog is declared
//! here in ADR-0020's action vocabulary; the shell draws it and queues
//! `source_action` with the answers, never knowing what a label is.

use knobas_http::{PageNumber, PageWalk, Walked};
use knobas_source::SourceError;
use knobas_source::view::{ActionField, ChoiceOption, FieldKind, ViewAction};

use super::Pull;
use crate::GiteaSource;
use crate::view::grants;

/// How many pages of fifty labels one listing is read to at most: a
/// thousand labels, past which the rest is not offered.
const LABEL_PAGES: u32 = 20;

/// One label as Gitea lists it: a repository's, an organisation's or a
/// pull request's.
#[derive(Debug, Clone, serde::Deserialize)]
pub(super) struct Label {
    pub(super) id: u64,
    #[serde(default)]
    pub(super) name: String,
    /// Whether it is the organisation's, not the repository's (set by
    /// [`read`], not by Gitea).
    #[serde(skip)]
    org: bool,
}

/// The labels a pull request of `owner/repo` may carry (#1235): every page of
/// the repository's own, then, where `owner` is an organisation, every page
/// of the organisation's. Gitea's 404 for the organisation's is a user owner,
/// which has none. Any other failure is the reason the action is disabled,
/// naming the scope where Gitea refused the token for it (#2042): the
/// repository's labels ask for `read:issue`, the organisation's for
/// `read:organization`.
pub(super) async fn read(
    source: &GiteaSource,
    owner: &str,
    repo: &str,
) -> Result<Vec<Label>, String> {
    let client = &source.client;
    let refused = |error: SourceError, what, scope| {
        grants::unread(&error, what, scope)
            .unwrap_or_else(|| format!("Could not read this repository's labels: {error}"))
    };
    let own = async {
        all_pages(|page| client.labels_as(owner, repo, page))
            .await
            .map_err(|error| refused(error, "this repository's labels", grants::READ_ISSUE))
    };
    let org = async {
        match all_pages(|page| client.org_labels_as(owner, page)).await {
            Err(error) if error.status() == Some(404) => Ok(Vec::new()),
            labels => labels.map_err(|error| {
                refused(error, "its organisation's labels", grants::READ_ORGANIZATION)
            }),
        }
    };
    let (mut labels, org) = tokio::try_join!(own, org)?;
    labels.extend(org.into_iter().map(|label| Label { org: true, ..label }));
    Ok(labels)
}

/// Every label of one listing, page by page from the first, at most
/// [`LABEL_PAGES`] pages. The listing is whole on an empty page -- empty, not
/// short, as #81 rules for the sync's walks -- or once Gitea's
/// `X-Total-Count` is reached, which spares the empty page's request.
async fn all_pages<F, Fut>(page_of: F) -> Result<Vec<Label>, SourceError>
where
    F: FnMut(u32) -> Fut,
    Fut: Future<Output = Result<(Vec<Label>, Option<usize>), SourceError>>,
{
    let mut labels = Vec::new();
    // Past the cap the rest is not offered, as the constant says: not a
    // failure.
    let mut walk = PageWalk::new(PageNumber, LABEL_PAGES, page_of);
    while let Walked::Page((batch, _)) = walk.next().await? {
        labels.extend(batch);
    }
    Ok(labels)
}

/// *Add label…*: the labels the repository may use (its own and its
/// organisation's, [`read`]) that the pull request does not carry, as
/// a required choice, in every state (Gitea labels a closed or merged pull
/// request too). An organisation label whose name a repository label also
/// has reads `name (owner)`, `owner` being the organisation (#1271); every
/// other option is the plain name. Where none is left, or the labels could
/// not be read, the action is drawn disabled saying why: a label can be made
/// or taken off, so the reader can still get to an enabled one (the #701
/// ruling).
pub(super) fn action(pull: &Pull, owner: &str, labels: &Result<Vec<Label>, String>) -> ViewAction {
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
        Err(reason) => return disabled(reason.clone()),
    };
    if labels.is_empty() {
        return disabled("This repository has no labels.".to_owned());
    }
    // Gitea lets a repository and its organisation share a label name; the
    // organisation's then carries the organisation's name (#1271).
    let shared = |label: &Label| {
        label.org
            && labels
                .iter()
                .any(|other| !other.org && other.name == label.name)
    };
    let options: Vec<ChoiceOption> = labels
        .iter()
        .filter(|label| !pull.labels.iter().any(|on| on.id == label.id))
        .map(|label| {
            let name = if shared(label) {
                format!("{} ({owner})", label.name)
            } else {
                label.name.clone()
            };
            ChoiceOption::new(label.id.to_string(), name)
        })
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
