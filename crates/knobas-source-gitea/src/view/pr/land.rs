//! Landing a pull request (issue #708, spec #701 workflow 9): *Update
//! branch…* and *Merge…* in the toolbar, and the overview's *Mergeable*.
//!
//! **Mergeability is decided here and nowhere else**: [`blockers`] is what
//! both *Merge…*'s disabled reason and the overview's fact are made from, so
//! the two cannot disagree. Gitea's own merge check also refuses for what
//! these do not read (signed commits, protected files, an owner's required
//! workflows); its refusal says so at the write (ADR-0004).

use knobas_source::SourceError;
use knobas_source::view::{ActionField, ChoiceOption, Fact, FieldKind, Tone, ViewAction};

use super::super::checks::Status;
use super::super::login;
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
    /// The base's protection does not count an approval given before the
    /// head last moved (Gitea's `ignore_stale_approvals`).
    ignore_stale: bool,
    /// The head is a branch of this repository that is not protected, so
    /// Gitea would delete it after the merge.
    head_deletable: bool,
    /// The base's protection refuses a merge while an official reviewer's
    /// request for changes stands (Gitea's `block_on_rejected_reviews`).
    block_on_rejected: bool,
    /// The base's protection refuses a merge while a review requested from
    /// an official reviewer is open (`block_on_official_review_requests`).
    block_on_requested: bool,
    /// The base's required status checks (Gitea's `status_check_contexts`,
    /// none for "every status reported"), where its protection enables them.
    required_checks: Option<Vec<String>>,
    /// Those against what the head reported, once the view's one status read
    /// has answered ([`Landing::with_statuses`], #721).
    checks: Option<Checks>,
}

/// The base's required status checks against the head commit's statuses.
#[derive(Debug)]
struct Checks {
    /// Gitea's `status_check_contexts`: glob patterns, none for "every
    /// status reported".
    required: Vec<String>,
    /// The latest status of each context reported on the head commit; a
    /// state or context Gitea sent `null` reads as empty, and an empty state
    /// is pending, as Gitea's own combination reads it.
    reported: Vec<Status>,
}

impl Landing {
    /// The head commit's statuses as the view read them for *Checks*
    /// (`None` where the record names no head sha), held against the base's
    /// required checks. A read that failed decides nothing: *Checks* says
    /// why, and Gitea's own refusal at the write says the rest (ADR-0004).
    pub(super) fn with_statuses(mut self, read: Option<&Result<Vec<Status>, SourceError>>) -> Self {
        if let (Some(required), Some(Ok(reported))) = (self.required_checks.take(), read) {
            self.checks = Some(Checks {
                required,
                reported: reported.clone(),
            });
        }
        self
    }
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

/// Gitea's `BranchProtection`, the fields landing reads that the branch
/// record does not carry.
#[derive(Debug, serde::Deserialize)]
struct Protection {
    #[serde(default)]
    block_on_outdated_branch: bool,
    #[serde(default)]
    ignore_stale_approvals: bool,
    #[serde(default)]
    block_on_rejected_reviews: bool,
    #[serde(default)]
    block_on_official_review_requests: bool,
    #[serde(default)]
    enable_status_check: bool,
    #[serde(default)]
    status_check_contexts: Option<Vec<String>>,
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
                if let Some(protection) = protection {
                    landing.block_on_outdated = protection.block_on_outdated_branch;
                    landing.ignore_stale = protection.ignore_stale_approvals;
                    landing.block_on_rejected = protection.block_on_rejected_reviews;
                    landing.block_on_requested = protection.block_on_official_review_requests;
                    if protection.enable_status_check {
                        landing.required_checks =
                            Some(protection.status_check_contexts.unwrap_or_default());
                    }
                }
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
    /// progress: its check still running, a conflict, or a check that failed
    /// -- the flag does not say which, and Gitea 1.27's API has nothing that
    /// does (#721). Still checking is said first, because it is the common
    /// case: for a second or two after a pull request opens or its head
    /// moves, which is exactly when the reader is looking at it.
    Conflict,
    /// The title carries one of Gitea's default work-in-progress prefixes.
    WorkInProgress,
    /// The head is behind and the base's protection forbids that.
    Outdated,
    /// Fewer official approvals than the base's protection requires.
    Approvals { given: u64, required: u64 },
    /// Required status checks that have not passed, each said as what it is:
    /// `ci/build failed`, `ci/lint is pending`, `ci/docs has not reported`.
    /// Empty where nothing is named and nothing has reported at all.
    Checks(Vec<String>),
    /// Official reviewers whose request for changes stands.
    ChangesRequested(Vec<String>),
    /// Official reviewers (or teams) a review is still requested from.
    ReviewRequested(Vec<String>),
}

impl Blocker {
    /// The phrase, lowercase where the sentence it goes into is not.
    fn phrase(&self, base: &str) -> String {
        match self {
            Blocker::Unknown => STILL_CHECKING.to_owned(),
            Blocker::Conflict => format!("{STILL_CHECKING}, or it conflicts with {base}"),
            Blocker::WorkInProgress => "marked work in progress in its title".to_owned(),
            Blocker::Outdated => {
                format!("behind {base}, which its protection forbids: update the branch")
            }
            Blocker::Approvals { given, required } => {
                format!("{given} of {required} required approvals")
            }
            Blocker::Checks(unmet) => match unmet.as_slice() {
                [] => "required checks have not reported".to_owned(),
                [one] => format!("required check {one}"),
                many => format!("required checks {}", many.join(", ")),
            },
            Blocker::ChangesRequested(who) => format!("changes requested by {}", who.join(", ")),
            Blocker::ReviewRequested(who) => format!("review requested from {}", who.join(", ")),
        }
    }
}

/// What *Merge…* says while Gitea works out whether it can merge (#721).
const STILL_CHECKING: &str = "Gitea is still checking whether this can merge";

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
    let given = official_approvals(reviews, landing.ignore_stale);
    if given < landing.required_approvals {
        blockers.push(Blocker::Approvals {
            given,
            required: landing.required_approvals,
        });
    }
    if let Some(unmet) = landing.checks.as_ref().and_then(unmet_checks) {
        blockers.push(Blocker::Checks(unmet));
    }
    if landing.block_on_rejected {
        let who = official_reviewers(reviews, "REQUEST_CHANGES");
        if !who.is_empty() {
            blockers.push(Blocker::ChangesRequested(who));
        }
    }
    if landing.block_on_requested {
        let who = official_reviewers(reviews, "REQUEST_REVIEW");
        if !who.is_empty() {
            blockers.push(Blocker::ReviewRequested(who));
        }
    }
    blockers
}

/// Who stands behind each official, undismissed review in `state` -- Gitea's
/// `MergeBlockedByRejectedReview` and `MergeBlockedByOfficialReviewRequests`
/// ask only whether one exists; the names are what the reader can act on.
/// Gitea marks only a reviewer's latest review official, so an approval
/// given after a request for changes takes it back.
fn official_reviewers(reviews: &[Review], state: &str) -> Vec<String> {
    reviews
        .iter()
        .filter(|r| r.official && !r.dismissed && r.state.as_deref() == Some(state))
        .map(|r| {
            login(r.user.as_ref())
                .or_else(|| r.team.as_ref().and_then(|t| t.name.clone()))
                .unwrap_or_else(|| "a reviewer".to_owned())
        })
        .collect()
}

/// What stands among the required checks, as Gitea's
/// `MergeRequiredContextsCommitStatus` decides it (a `warning` fails a merge
/// there, though *Checks* draws it as a warning): with no context named,
/// every reported status must pass and at least one must have reported;
/// otherwise each named pattern must match a reported context and every
/// match must pass. `success` and `skipped` pass; `error`, `failure` and
/// `warning` fail; anything else is pending. `None` where nothing stands.
/// A pattern beyond `*` and `?` (Gitea's glob also has classes and
/// alternatives) is not judged: the doubt leaves *Merge…* enabled and
/// Gitea's refusal says the rest (ADR-0004).
fn unmet_checks(checks: &Checks) -> Option<Vec<String>> {
    if checks.required.is_empty() && checks.reported.is_empty() {
        return Some(Vec::new());
    }
    let unmet: Vec<String> = required_standing(checks)
        .into_iter()
        .filter_map(|(name, standing)| match standing {
            Standing::Passed => None,
            Standing::Failed => Some(format!("{name} failed")),
            Standing::Pending => Some(format!("{name} is pending")),
            Standing::Unreported => Some(format!("{name} has not reported")),
        })
        .collect();
    (!unmet.is_empty()).then_some(unmet)
}

/// Where one required check stands on the head commit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Standing {
    Passed,
    Failed,
    Pending,
    /// A required pattern no reported context matched.
    Unreported,
}

/// Each required check by name, once, with where it stands, as
/// [`unmet_checks`] reads Gitea's rule: with no context named every reported
/// status, else each pattern's matches in the order Gitea reported them, or
/// the pattern itself where it matched none. A pattern Gitea's glob reads
/// beyond `*` and `?` is left out (not judged).
fn required_standing(checks: &Checks) -> Vec<(String, Standing)> {
    let context = |status: &Status| status.context.clone().unwrap_or_default();
    let standing = |status: &Status| match status.status.as_deref().unwrap_or_default() {
        "success" | "skipped" => Standing::Passed,
        "error" | "failure" | "warning" => Standing::Failed,
        _ => Standing::Pending,
    };
    let mut named: Vec<(String, Standing)> = Vec::new();
    let mut add = |name: String, at: Standing| {
        if !named.iter().any(|(seen, _)| *seen == name) {
            named.push((name, at));
        }
    };
    if checks.required.is_empty() {
        for status in &checks.reported {
            add(context(status), standing(status));
        }
        return named;
    }
    for pattern in &checks.required {
        if pattern.contains(['[', '{', '\\']) {
            continue;
        }
        let matched: Vec<&Status> = checks
            .reported
            .iter()
            .filter(|s| glob_matches(pattern, &context(s)))
            .collect();
        if matched.is_empty() {
            add(pattern.clone(), Standing::Unreported);
        }
        for status in matched {
            add(context(status), standing(status));
        }
    }
    named
}

/// *Required checks* and one *Required check* per required context, for
/// the pull request's side rail (#2278): the passed of all required, toned
/// by the loudest standing (a failure err, a pending or unreported one info,
/// all passed ok). Nothing where the base's protection does not enable the
/// status check, or its statuses could not be read.
fn required_facts(landing: &Landing) -> Vec<Fact> {
    let Some(checks) = landing.checks.as_ref() else {
        return Vec::new();
    };
    let named = required_standing(checks);
    let passed = named
        .iter()
        .filter(|(_, at)| *at == Standing::Passed)
        .count();
    let (value, tone) = if named.is_empty() {
        ("none have reported".to_owned(), Tone::Info)
    } else {
        let tone = if named.iter().any(|(_, at)| *at == Standing::Failed) {
            Tone::Err
        } else if passed < named.len() {
            Tone::Info
        } else {
            Tone::Ok
        };
        (format!("{passed} of {} pass", named.len()), tone)
    };
    let mut facts = vec![Fact {
        label: "Required checks".to_owned(),
        value,
        tone: Some(tone),
    }];
    facts.extend(named.into_iter().map(|(name, _)| Fact {
        label: "Required check".to_owned(),
        value: name,
        tone: None,
    }));
    facts
}

/// `pattern` against the whole of `text`: `*` any run of characters, `?`
/// one, everything else itself -- Gitea's glob with no separators, as its
/// status check compiles them.
fn glob_matches(pattern: &str, text: &str) -> bool {
    let (p, t): (Vec<char>, Vec<char>) = (pattern.chars().collect(), text.chars().collect());
    let (mut pi, mut ti) = (0, 0);
    let mut star: Option<(usize, usize)> = None;
    while ti < t.len() {
        match p.get(pi) {
            Some('*') => {
                star = Some((pi, ti));
                pi += 1;
            }
            Some(&c) if c == '?' || c == t[ti] => {
                pi += 1;
                ti += 1;
            }
            _ => match star {
                Some((sp, st)) => {
                    pi = sp + 1;
                    ti = st + 1;
                    star = Some((sp, st + 1));
                }
                None => return false,
            },
        }
    }
    p[pi..].iter().all(|&c| c == '*')
}

fn is_wip(pull: &Pull) -> bool {
    let title = pull.title.as_deref().unwrap_or_default().trim_start();
    WIP_PREFIXES.iter().any(|prefix| {
        title
            .get(..prefix.len())
            .is_some_and(|start| start.eq_ignore_ascii_case(prefix))
    })
}

/// The approvals Gitea counts against a protection (its
/// `GetGrantedApprovalsCount`): approving reviews it marks `official` (from a
/// reviewer the protection names, and only each reviewer's latest), not
/// dismissed, and not `stale` where the protection ignores stale approvals.
/// A protection the token may not read counts a stale one, so the doubt
/// leaves *Merge…* enabled and Gitea's refusal says the rest (ADR-0004).
fn official_approvals(reviews: &[Review], ignore_stale: bool) -> u64 {
    reviews
        .iter()
        .filter(|r| {
            r.official
                && !r.dismissed
                && !(ignore_stale && r.stale)
                && r.state.as_deref() == Some("APPROVED")
        })
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

/// *Ready to merge?*'s two facts for an open pull request (#994), so the
/// shell reads them without taking *Mergeable*'s sentence apart:
///
/// - *Approvals*: the official approvals [`blockers`] counts against what the
///   base's protection requires, `1 of 2`, ok when met and warn when short;
///   `none required` (ok) where the protection asks for none or cannot be
///   read.
/// - *Conflicts*: `none with <base>` (ok) where Gitea says it can merge,
///   `conflicts with <base>` (warn) where it says it cannot for a title it
///   does not read as work in progress ([`Blocker::Conflict`]'s reading);
///   nothing while Gitea has not said, and nothing for a work-in-progress
///   title, whose `false` says nothing about a conflict.
/// - *Required checks* and each *Required check* ([`required_facts`],
///   #2278), where the base's protection enables the status check.
pub(super) fn ready_facts(pull: &Pull, reviews: &[Review], landing: &Landing) -> Vec<Fact> {
    if !pull.is_open() {
        return Vec::new();
    }
    let base = pull.base_ref().unwrap_or("its base");
    let required = landing.required_approvals;
    let approvals = if required == 0 {
        Fact {
            label: "Approvals".to_owned(),
            value: "none required".to_owned(),
            tone: Some(Tone::Ok),
        }
    } else {
        let given = official_approvals(reviews, landing.ignore_stale);
        Fact {
            label: "Approvals".to_owned(),
            value: format!("{given} of {required}"),
            tone: Some(if given >= required {
                Tone::Ok
            } else {
                Tone::Warn
            }),
        }
    };
    let mut facts = vec![approvals];
    let conflicts = match pull.mergeable {
        Some(true) => Some((format!("none with {base}"), Tone::Ok)),
        Some(false) if !is_wip(pull) => Some((format!("conflicts with {base}"), Tone::Warn)),
        _ => None,
    };
    facts.extend(conflicts.map(|(value, tone)| Fact {
        label: "Conflicts".to_owned(),
        value,
        tone: Some(tone),
    }));
    facts.extend(required_facts(landing));
    facts
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
            // A squash's commit message (#843, R-8.19), prefilled as the one
            // Gitea writes for a squash by default: the title and `(#n)`.
            // The shell sends it only with *Squash*; the others keep Gitea's.
            ActionField {
                value: Some(squash_title(pull)),
                ..ActionField::new(
                    "message",
                    "Squash commit message",
                    FieldKind::Text {
                        suggestions: Vec::new(),
                    },
                )
            },
        ],
        confirm: Some(confirm),
        primary: true,
        disabled,
        ..ViewAction::new(crate::WRITE_OP_MERGE_PULL_REQUEST, "Merge…")
    }
}

/// The title Gitea gives a squash commit when nobody writes one: the pull
/// request's title and `(#<number>)`.
fn squash_title(pull: &Pull) -> String {
    let title = pull.title.as_deref().unwrap_or_default().trim();
    match pull.number {
        Some(n) => format!("{title} (#{n})"),
        None => title.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::glob_matches;

    /// Gitea compiles a required context with no separators: `*` crosses
    /// `/`, and the pattern covers the whole context.
    #[test]
    fn a_required_context_pattern_is_matched_whole() {
        for (pattern, context, matches) in [
            ("ci/build", "ci/build", true),
            ("ci/build", "ci/build-2", false),
            ("ci/*", "ci/build", true),
            ("ci/*", "ci/build/linux", true),
            ("*/lint", "ci/lint", true),
            ("ci/?uild", "ci/build", true),
            ("ci/*/linux", "ci/build/mac", false),
            ("*", "", true),
            ("ci", "ci/build", false),
        ] {
            assert_eq!(
                glob_matches(pattern, context),
                matches,
                "{pattern} ~ {context}"
            );
        }
    }
}
