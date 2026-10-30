//! A branch's life from its views (#707, spec #701 workflows 2, 4 and 10):
//! the toolbar actions a repository, a branch and a commit offer.
//!
//! * *New branch…* (`create_branch`) on all three: from the default branch or
//!   one picked on a repository, from this branch on a branch, from this
//!   commit on a commit.
//! * *Open pull request…* (`create_pull_request`) on a branch with no open
//!   pull request, prefilled from its commits; *New pull request…* on a
//!   repository, its head picked among the branches ahead of the default.
//! * *Delete branch…* (`delete_branch`) on a branch, a danger whose sentence
//!   says whether it is merged.
//!
//! The two creates target the **repository** (the container those ops act
//! in), which a branch's and a commit's view name as the action's own
//! `entity` (the #707 ruling); a ref the view fixes is a one-option choice,
//! so the dialog shows it and the payload reads it like any other.

use knobas_source::view::{ActionField, ChoiceOption, FieldKind, ViewAction};

use crate::keys;
use crate::source::GiteaSource;

/// The repository's entity id, which the creates target.
pub(super) fn repo_id(source: &GiteaSource, owner: &str, repo: &str) -> String {
    format!("{}:{}", source.id, keys::repo_key(owner, repo))
}

/// A branch name as the option a choice offers: its value and its label.
pub(super) fn branch_option(name: &str) -> ChoiceOption {
    ChoiceOption::new(name, name)
}

/// The default branch first, then the rest in the order given, `except` left
/// out: the bases a pull request may go into, the refs a branch may start at.
pub(super) fn default_first<'a>(
    default: &str,
    names: impl IntoIterator<Item = &'a str>,
    except: Option<&str>,
) -> Vec<ChoiceOption> {
    let mut options = vec![branch_option(default)];
    options.extend(
        names
            .into_iter()
            .filter(|n| *n != default && Some(*n) != except)
            .map(branch_option),
    );
    options
}

/// Why a mirror's branch actions are drawn disabled: Gitea refuses a branch
/// write on a pull mirror (a 403, `Git Repository is a mirror.`), so the view
/// must not offer one it knows will be refused. A mirror can be turned
/// into a plain repository, so the actions stay, disabled (the #701 ruling).
pub(crate) const MIRRORED: &str = "A mirror: its branches come from upstream";

/// Why *New branch…* is refused in a repository with no commit to start at.
pub(crate) const EMPTY: &str = "The repository is empty";

/// *New branch…*: a name, and where it starts. `repository` is the entity the
/// write targets where the view is not the repository's own.
pub(super) fn new_branch(
    repository: Option<String>,
    from: Vec<ChoiceOption>,
    disabled: Option<String>,
) -> ViewAction {
    ViewAction {
        fields: vec![
            ActionField {
                required: true,
                placeholder: Some("feature/…".to_owned()),
                ..ActionField::new(
                    "name",
                    "Name",
                    FieldKind::Text {
                        suggestions: Vec::new(),
                    },
                )
            },
            ActionField {
                required: true,
                ..ActionField::new("from_ref", "From", FieldKind::Choice { options: from })
            },
        ],
        disabled,
        entity: repository,
        ..ViewAction::new(crate::WRITE_OP_CREATE_BRANCH, "New branch…")
    }
}

/// What a pull request's dialog starts with: its title and its description.
/// Nothing, by default: a repository's *New pull request…* knows no head yet.
#[derive(Default)]
pub(super) struct Proposal {
    pub title: Option<String>,
    pub body: Option<String>,
}

/// *Open pull request…* / *New pull request…*: a title, a description, the
/// head and the base, each a choice of what the view offers.
pub(super) fn pull_request(
    label: &str,
    repository: Option<String>,
    proposal: Proposal,
    heads: Vec<ChoiceOption>,
    bases: Vec<ChoiceOption>,
    disabled: Option<String>,
) -> ViewAction {
    ViewAction {
        fields: vec![
            ActionField {
                value: proposal.title,
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
                value: proposal.body,
                ..ActionField::new("body", "Description", FieldKind::LongText)
            },
            ActionField {
                required: true,
                ..ActionField::new("head", "Head", FieldKind::Choice { options: heads })
            },
            ActionField {
                required: true,
                ..ActionField::new("base", "Base", FieldKind::Choice { options: bases })
            },
        ],
        disabled,
        entity: repository,
        ..ViewAction::new(crate::WRITE_OP_CREATE_PULL_REQUEST, label)
    }
}

/// What a branch carries that its pull request is about: the subjects of the
/// commits it is ahead by, as listed, and how many it is ahead by in all. A
/// list cut at its cap ends on a line saying how many more there are.
pub(super) fn proposal(branch: &str, subjects: &[String], ahead: u64) -> Proposal {
    let title = match subjects {
        [only] if ahead == 1 => only.clone(),
        _ => readable(branch),
    };
    let mut lines: Vec<String> = subjects.iter().map(|s| format!("- {s}")).collect();
    let more = ahead.saturating_sub(subjects.len() as u64);
    if more > 0 && !lines.is_empty() {
        lines.push(format!("- and {more} more"));
    }
    let body = lines.join("\n");
    Proposal {
        title: Some(title),
        body: (!body.is_empty()).then_some(body),
    }
}

/// A branch's name as a title: its last segment, a leading ticket key kept
/// (`PAY-231`), the rest's dashes and underscores as spaces, its first letter
/// capitalised where no key leads (`feature/sepa-retry` → `Sepa retry`).
pub(super) fn readable(branch: &str) -> String {
    let last = branch.rsplit('/').next().unwrap_or(branch);
    let parts: Vec<&str> = last.split(['-', '_']).filter(|p| !p.is_empty()).collect();
    let keyed = matches!(parts.as_slice(), [project, number, ..]
        if project.chars().next().is_some_and(|c| c.is_ascii_uppercase())
            && project.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
            && number.chars().all(|c| c.is_ascii_digit()));
    if keyed {
        let rest = parts[2..].join(" ");
        let key = format!("{}-{}", parts[0], parts[1]);
        return if rest.is_empty() {
            key
        } else {
            format!("{key} {rest}")
        };
    }
    let words = parts.join(" ");
    let mut chars = words.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => branch.to_owned(),
    }
}

/// *Delete branch…*: a danger, its sentence saying whether the branch is
/// merged into the default branch, which is what a reader is about to lose.
/// `ahead` is how many commits it carries over the default branch.
pub(super) fn delete_branch(
    branch: &str,
    default: &str,
    ahead: u64,
    disabled: Option<String>,
) -> ViewAction {
    let confirm = match ahead {
        0 => format!("Delete {branch}? It is merged into {default}: nothing on it is lost."),
        1 => format!(
            "Delete {branch}? Not merged into {default}: 1 commit will only be reachable by SHA."
        ),
        n => format!(
            "Delete {branch}? Not merged into {default}: {n} commits will only be reachable by \
             SHA."
        ),
    };
    ViewAction {
        confirm: Some(confirm),
        danger: true,
        disabled,
        ..ViewAction::new(crate::WRITE_OP_DELETE_BRANCH, "Delete branch…")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_proposal_is_titled_by_its_one_commit_and_lists_what_it_carries() {
        let subjects = ["PAY-231: jitter".to_owned(), "PAY-231: retry".to_owned()];
        let two = proposal("feature/PAY-231-sepa-retry", &subjects, 2);
        assert_eq!(two.title.as_deref(), Some("PAY-231 sepa retry"));
        assert_eq!(
            two.body.as_deref(),
            Some("- PAY-231: jitter\n- PAY-231: retry")
        );
        let one = proposal("feature/x", &subjects[..1], 1);
        assert_eq!(one.title.as_deref(), Some("PAY-231: jitter"));
        // Listed up to the cap, then counted: the reader learns the list is cut.
        let cut = proposal("feature/PAY-231-sepa-retry", &subjects, 300);
        assert_eq!(
            cut.body.as_deref(),
            Some("- PAY-231: jitter\n- PAY-231: retry\n- and 298 more")
        );
        assert_eq!(cut.title.as_deref(), Some("PAY-231 sepa retry"));
        assert_eq!(proposal("feature/x", &[], 0).body, None);
    }

    #[test]
    fn a_branch_name_reads_as_a_title() {
        assert_eq!(readable("feature/PAY-231-sepa-retry"), "PAY-231 sepa retry");
        assert_eq!(readable("PAY-231"), "PAY-231");
        assert_eq!(readable("feature/sepa_retry"), "Sepa retry");
        assert_eq!(readable("hotfix"), "Hotfix");
        assert_eq!(readable("v2-api"), "V2 api", "a lowercase start is no key");
    }
}
