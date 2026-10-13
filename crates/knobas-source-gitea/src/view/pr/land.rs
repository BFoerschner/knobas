//! Landing a pull request (issue #708, spec #701 workflow 9): *Update
//! branch…* and *Merge…* in the toolbar, and the overview's *Mergeable*.
//!
//! **Mergeability is decided here and nowhere else**: [`blockers`] is what
//! both *Merge…*'s disabled reason and the overview's fact are made from, so
//! the two cannot disagree, and a refinement of one reason (#721, Gitea's
//! *"Please try again later"* while it is still checking) is one edit.

use knobas_source::SourceError;
use knobas_source::view::{ActionField, ChoiceOption, Fact, FieldKind, Tone, ViewAction};

use super::{Pull, PullView, Review};
use crate::client::is_repo_scoped;

/// What landing reads beside the pull request: whether its head is behind its
/// base, what the base's protection asks of a merge, and whether Gitea could
/// delete the head after one. Each is `None`/`false` where Gitea would not
/// say, and then decides nothing: Gitea's own refusal at the write is the
/// last word (ADR-0004).
#[derive(Debug, Default)]
pub(super) struct Landing {
    /// The base has a commit the head lacks.
    behind: Option<bool>,
    /// Approvals the base's protection requires; 0 for none.
    required_approvals: u64,
    /// The base's protection refuses a merge of a head that is behind.
    block_on_outdated: bool,
    /// The head is a branch of this repository that is not protected, so
    /// Gitea would delete it after the merge.
    head_deletable: bool,
}

/// Gitea's `Branch`, the fields landing reads.
#[derive(Debug, serde::Deserialize)]
struct BranchRecord {
    #[serde(default)]
    protected: bool,
    #[serde(default)]
    required_approvals: u64,
    #[serde(default)]
    effective_branch_protection_name: Option<String>,
}

/// Gitea's `BranchProtection`, the one field landing reads that the branch
/// record does not carry.
#[derive(Debug, serde::Deserialize)]
struct Protection {
    #[serde(default)]
    block_on_outdated_branch: bool,
}

/// A read Gitea refused or found nothing for is `None`: a base branch gone,
/// a head deleted, a protection rule only an administrator may read. Any
/// other failure fails the view, as every read of it does.
fn unless_refused<T>(read: Result<T, SourceError>) -> Result<Option<T>, SourceError> {
    match read {
        Ok(value) => Ok(Some(value)),
        Err(error) if is_repo_scoped(&error) => Ok(None),
        Err(error) => Err(error),
    }
}

impl PullView<'_> {
    /// What landing reads for `pull`, side by side; nothing for one that is
    /// closed or merged, which neither action is offered on and the overview
    /// draws no *Mergeable* for.
    pub(super) async fn landing(&self, pull: &Pull) -> Result<Landing, SourceError> {
        if !pull.is_open() {
            return Ok(Landing::default());
        }
        let (Some(base), Some(head)) = (pull.base_ref(), pull.head_ref()) else {
            return Ok(Landing::default());
        };
        let client = &self.source.client;
        // The head's branch is this repository's only where the head is not
        // a fork's; a fork's is not read here, nor deleted by default.
        let same_repo = pull.head_in_base_repo();
        // A fork's head commit is in the base repository too (Gitea keeps it
        // under the pull request's own ref), so the SHA asks the base about
        // either; the name is the fallback where Gitea sent none.
        let head_tip = pull
            .head
            .as_ref()
            .and_then(|b| b.sha.as_deref())
            .unwrap_or(head);
        let (base_branch, head_branch, behind) = tokio::try_join!(
            async {
                unless_refused(
                    client
                        .branch_as::<BranchRecord>(self.owner, self.repo, base)
                        .await,
                )
            },
            async {
                if !same_repo {
                    return Ok(None);
                }
                unless_refused(
                    client
                        .branch_as::<BranchRecord>(self.owner, self.repo, head)
                        .await,
                )
            },
            async {
                unless_refused(
                    client
                        .has_commits_not_in(self.owner, self.repo, base, head_tip)
                        .await,
                )
            },
        )?;
        let mut landing = Landing {
            behind,
            head_deletable: head_branch.is_some_and(|b| !b.protected),
            ..Landing::default()
        };
        if let Some(base_branch) = base_branch.filter(|b| b.protected) {
            landing.required_approvals = base_branch.required_approvals;
            let rule = base_branch
                .effective_branch_protection_name
                .filter(|r| !r.is_empty());
            if let Some(rule) = rule {
                let protection = unless_refused(
                    client
                        .branch_protection_as::<Protection>(self.owner, self.repo, &rule)
                        .await,
                )?;
                landing.block_on_outdated = protection.is_some_and(|p| p.block_on_outdated_branch);
            }
        }
        Ok(landing)
    }
}

/// One thing that stands between a pull request and its merge, each one the
/// reader can still change -- which is why *Merge…* is drawn disabled rather
/// than left out (the #701 ruling).
#[derive(Debug, PartialEq)]
enum Blocker {
    /// Gitea has not answered `mergeable`: it has not checked yet.
    Unknown,
    /// Gitea said `mergeable: false` for a title it does not read as work in
    /// progress: a conflict, or its check still running or failed -- the flag
    /// does not say which.
    Conflict,
    /// The title carries one of Gitea's default work-in-progress prefixes.
    WorkInProgress,
    /// The head is behind and the base's protection forbids that.
    Outdated,
    /// Fewer official approvals than the base's protection requires.
    Approvals { given: u64, required: u64 },
}

impl Blocker {
    /// The phrase, lowercase where the sentence it goes into is not.
    fn phrase(&self, base: &str) -> String {
        match self {
            Blocker::Unknown => "Gitea has not said yet whether it can be merged".to_owned(),
            Blocker::Conflict => format!("conflicts with {base}, or Gitea is still checking"),
            Blocker::WorkInProgress => "marked work in progress in its title".to_owned(),
            Blocker::Outdated => {
                format!("behind {base}, which its protection forbids: update the branch")
            }
            Blocker::Approvals { given, required } => {
                format!("{given} of {required} required approvals")
            }
        }
    }
}

/// Gitea's default `WORK_IN_PROGRESS_PREFIXES`, matched as it matches them:
/// the start of the title, any case.
const WIP_PREFIXES: [&str; 2] = ["WIP:", "[WIP]"];

/// What stands between an open pull request and its merge, in the order the
/// reader would see to it: Gitea's own check first, then the protection's
/// demands. Empty where nothing does.
fn blockers(pull: &Pull, reviews: &[Review], landing: &Landing) -> Vec<Blocker> {
    let mut blockers = Vec::new();
    match pull.mergeable {
        Some(true) => {}
        None => blockers.push(Blocker::Unknown),
        Some(false) => blockers.push(if is_wip(pull) {
            Blocker::WorkInProgress
        } else {
            Blocker::Conflict
        }),
    }
    if landing.block_on_outdated && landing.behind == Some(true) {
        blockers.push(Blocker::Outdated);
    }
    let given = official_approvals(reviews);
    if given < landing.required_approvals {
        blockers.push(Blocker::Approvals {
            given,
            required: landing.required_approvals,
        });
    }
    blockers
}

fn is_wip(pull: &Pull) -> bool {
    let title = pull.title.as_deref().unwrap_or_default().trim_start();
    WIP_PREFIXES.iter().any(|prefix| {
        title
            .get(..prefix.len())
            .is_some_and(|start| start.eq_ignore_ascii_case(prefix))
    })
}

/// The approvals Gitea counts against a protection: approving reviews it
/// marks `official` (from a reviewer the protection names, and only each
/// reviewer's latest) and has not dismissed.
fn official_approvals(reviews: &[Review]) -> u64 {
    reviews
        .iter()
        .filter(|r| r.official && !r.dismissed && r.state.as_deref() == Some("APPROVED"))
        .count() as u64
}

/// [`blockers`], each with its phrase about `pull`'s base.
fn phrased(pull: &Pull, reviews: &[Review], landing: &Landing) -> Vec<(Blocker, String)> {
    let base = pull.base_ref().unwrap_or("its base");
    blockers(pull, reviews, landing)
        .into_iter()
        .map(|b| {
            let phrase = b.phrase(base);
            (b, phrase)
        })
        .collect()
}

/// The sentence the phrases make, its first letter raised.
fn sentence(phrases: &[String]) -> String {
    let mut text = phrases.join("; ");
    if let Some(first) = text.get_mut(..1) {
        first.make_ascii_uppercase();
    }
    text
}

/// The overview's *Mergeable*, for an open pull request: *yes* (saying where
/// the head is behind, which is no reason not to), or *no* with what stands in
/// the way, toned warn. Nothing where the only thing known is that Gitea has
/// not said: the toolbar says why it waits.
pub(super) fn mergeable_fact(pull: &Pull, reviews: &[Review], landing: &Landing) -> Option<Fact> {
    if !pull.is_open() {
        return None;
    }
    let base = pull.base_ref().unwrap_or("its base");
    let phrases: Vec<String> = phrased(pull, reviews, landing)
        .into_iter()
        .filter(|(b, _)| *b != Blocker::Unknown)
        .map(|(_, phrase)| phrase)
        .collect();
    if pull.mergeable.is_none() && phrases.is_empty() {
        return None;
    }
    let (value, tone) = if phrases.is_empty() {
        let value = if landing.behind == Some(true) {
            format!("yes, behind {base}")
        } else {
            "yes".to_owned()
        };
        (value, None)
    } else {
        (format!("no: {}", phrases.join("; ")), Some(Tone::Warn))
    };
    Some(Fact {
        label: "Mergeable".to_owned(),
        value,
        tone,
    })
}

/// The toolbar's landing actions, in the #701 ruling's order: *Update
/// branch…* (maintenance) where the head is behind, then *Merge…* last. On a
/// merged or closed pull request neither is offered: nothing the reader waits
/// for makes them apply again, and a closed one merges only after *Reopen*,
/// which is its own action.
pub(super) fn actions(pull: &Pull, reviews: &[Review], landing: &Landing) -> Vec<ViewAction> {
    if !pull.is_open() {
        return Vec::new();
    }
    let mut actions = Vec::new();
    if landing.behind == Some(true) {
        actions.push(update(pull));
    }
    actions.push(merge(pull, reviews, landing));
    actions
}

/// *Update branch…*: the base merged into the head, or the head rebased onto
/// it, `merge` first -- Gitea's own default, and the one that rewrites
/// nothing. A conflict, or a style the repository turned off, is Gitea's
/// refusal with its sentence (ADR-0004).
fn update(pull: &Pull) -> ViewAction {
    let base = pull.base_ref().unwrap_or("its base");
    let confirm = match pull.head_ref() {
        Some(head) => format!("Bring {head} up to date with {base}?"),
        None => format!("Bring this pull request up to date with {base}?"),
    };
    let options = knobas_source::UpdateStyle::ALL
        .iter()
        .map(|style| {
            let label = match style {
                knobas_source::UpdateStyle::Merge => format!("Merge {base} in"),
                knobas_source::UpdateStyle::Rebase => format!("Rebase onto {base}"),
            };
            ChoiceOption::new(style.as_str(), label)
        })
        .collect();
    ViewAction {
        fields: vec![ActionField {
            required: true,
            ..ActionField::new("style", "How", FieldKind::Choice { options })
        }],
        confirm: Some(confirm),
        ..ViewAction::new(crate::WRITE_OP_UPDATE_PULL_REQUEST_BRANCH, "Update branch…")
    }
}

/// *Merge…*, the pull request's main write (issues #667, #702): enabled when
/// nothing in [`blockers`] stands in the way, else disabled with all of it.
/// The styles are the SPI's [`MergeStyle`](knobas_source::MergeStyle) words,
/// `merge` first because it is Gitea's own default. *Delete branch after
/// merge* is ticked where Gitea can delete the head (#708). Whether the
/// account may merge is not asked: that is a refusal the write queue shows
/// with Gitea's sentence (ADR-0004).
fn merge(pull: &Pull, reviews: &[Review], landing: &Landing) -> ViewAction {
    let base = pull.base_ref().unwrap_or("its base");
    let confirm = match pull.head_ref() {
        Some(head) => format!("Merge {head} into {base}?"),
        None => "Merge this pull request?".to_owned(),
    };
    let phrases: Vec<String> = phrased(pull, reviews, landing)
        .into_iter()
        .map(|(_, phrase)| phrase)
        .collect();
    let disabled = (!phrases.is_empty()).then(|| sentence(&phrases));
    let options = knobas_source::MergeStyle::ALL
        .iter()
        .map(|style| {
            let label = match style {
                knobas_source::MergeStyle::Merge => "Merge commit",
                knobas_source::MergeStyle::Rebase => "Rebase",
                knobas_source::MergeStyle::Squash => "Squash",
            };
            ChoiceOption::new(style.as_str(), label)
        })
        .collect();
    ViewAction {
        fields: vec![
            ActionField {
                required: true,
                ..ActionField::new("style", "Style", FieldKind::Choice { options })
            },
            ActionField {
                value: Some(landing.head_deletable.to_string()),
                ..ActionField::new(
                    "delete_branch",
                    "Delete branch after merge",
                    FieldKind::Flag,
                )
            },
        ],
        confirm: Some(confirm),
        primary: true,
        disabled,
        ..ViewAction::new(crate::WRITE_OP_MERGE_PULL_REQUEST, "Merge…")
    }
}
