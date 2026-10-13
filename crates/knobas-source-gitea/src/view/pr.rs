//! A pull request's view (#662, spec #660 stories 6-9 and 13): *Overview*,
//! *Files* (deferred), *Commits* and *Conversation*.
//!
//! The overview reads the pull request and its reviews; *Commits* its commit
//! listing; *Conversation* the discussion Gitea keeps on the issue of the same
//! index (the read the sync makes), with the reviews among it in time order
//! (#705). *Files* is the one expensive read -- the whole `.diff` -- so it is
//! a stub with a count until its tab is opened (ADR-0020 rule 3).

use knobas_source::SourceError;
use knobas_source::view::{
    ActionField, Block, ChoiceOption, Fact, FieldKind, SourceView, Tone, ViewAction, ViewRow,
    ViewSection,
};

use super::rows::{self, ListedCommit};
use super::{fact, login, when_of};
use crate::model::UserRef;
use crate::source::GiteaSource;

mod maintain;
mod review;

const OVERVIEW: &str = "overview";
const FILES: &str = "files";
const COMMITS: &str = "commits";
const CONVERSATION: &str = "conversation";

/// The view of pull request `number` in `owner/repo`, or of its one section.
pub(super) async fn draw(
    source: &GiteaSource,
    entity: &str,
    owner: &str,
    repo: &str,
    number: u64,
    section: Option<&str>,
) -> Result<SourceView, SourceError> {
    let at = PullView {
        source,
        owner,
        repo,
        number,
    };
    let mut actions = Vec::new();
    let sections = match section {
        None => {
            let pull = at.pull().await?;
            // Each tab is its own reads over the one record, so they are read
            // side by side (#703); the first to fail fails the view. Who the
            // token is goes beside them: the review actions are not offered
            // on its own pull requests (#705).
            let (reviews, commits, comments, viewer) = tokio::try_join!(
                at.reviews(),
                at.commits(Some(&pull)),
                at.comments(),
                at.viewer(),
            )?;
            // The order the #701 ruling sets: what a reviewer says first,
            // then the maintenance (#706), *Merge…* last.
            actions.extend(review::actions(&pull, viewer.as_deref()));
            actions.extend(maintain::actions(&pull));
            actions.extend(merge_action(&pull));
            vec![
                overview(&pull, &reviews),
                files_stub(&pull),
                commits,
                conversation(&reviews, &comments),
            ]
        }
        Some(OVERVIEW) => vec![overview(&at.pull().await?, &at.reviews().await?)],
        Some(FILES) => vec![at.files().await?],
        Some(COMMITS) => vec![at.commits(None).await?],
        Some(CONVERSATION) => {
            let (reviews, comments) = tokio::try_join!(at.reviews(), at.comments())?;
            vec![conversation(&reviews, &comments)]
        }
        Some(other) => {
            return Err(SourceError::protocol(format!(
                "gitea: a pull request's view has no section {other:?} (asked for {entity:?})"
            )));
        }
    };
    Ok(SourceView { sections, actions })
}

/// The pull request one view is about.
struct PullView<'a> {
    source: &'a GiteaSource,
    owner: &'a str,
    repo: &'a str,
    number: u64,
}

impl PullView<'_> {
    async fn pull(&self) -> Result<Pull, SourceError> {
        let raw = self
            .source
            .client
            .pull(self.owner, self.repo, self.number)
            .await?;
        serde_json::from_value(raw).map_err(|error| {
            SourceError::protocol(format!(
                "gitea: {}/{}#{} is not a pull request record: {error}",
                self.owner, self.repo, self.number
            ))
        })
    }

    /// Every review Gitea lists, oldest first; one it cannot read as a review
    /// is left out.
    async fn reviews(&self) -> Result<Vec<Review>, SourceError> {
        let reviews = self
            .source
            .client
            .pull_reviews(self.owner, self.repo, self.number)
            .await?;
        Ok(reviews
            .into_iter()
            .filter_map(|r| serde_json::from_value(r).ok())
            .collect())
    }

    async fn files(&self) -> Result<ViewSection, SourceError> {
        let text = self
            .source
            .client
            .pull_diff(self.owner, self.repo, self.number)
            .await?;
        let (files, truncated) = knobas_source::diff::parse_view(&text);
        Ok(ViewSection {
            id: FILES.to_owned(),
            title: "Files".to_owned(),
            count: Some(files.len() as u64),
            // The read the stub deferred: these are its blocks, now.
            deferred: false,
            blocks: vec![Block::Diff { files, truncated }],
        })
    }

    /// The pull request's commits, at most [`rows::COMMIT_CAP`], ending on
    /// a note where Gitea has more (#678).
    async fn commits(&self, pull: Option<&Pull>) -> Result<ViewSection, SourceError> {
        let mut records = Vec::new();
        let mut total = None;
        let mut cut = true;
        for page in 1..=rows::COMMIT_PAGES {
            let (batch, count) = self
                .source
                .client
                .pull_commits(self.owner, self.repo, self.number, page)
                .await?;
            total = count.or(total);
            // Ends on an empty page or on the server's own count, never on a
            // short page (`client::PAGE_SIZE`'s rule).
            let done = batch.is_empty();
            records.extend(batch);
            if done || total.is_some_and(|t| records.len() >= t) {
                cut = false;
                break;
            }
        }
        let read = records.len();
        let mut rows: Vec<ViewRow> = records
            .into_iter()
            .filter_map(|r| serde_json::from_value::<ListedCommit>(r).ok())
            .map(|c| rows::commit_row(&self.source.id, self.owner, self.repo, &c.record))
            .collect();
        // The count and the note both say what the tab lists; a record that
        // did not parse is one the list does not hold.
        let listed = rows.len();
        let total = total.map(|t| t.max(read));
        if let Some(mut note) = rows::left_out("commits", listed, total, cut, None) {
            // Where the rest is: the pull request's own page, read for the
            // note when this tab was asked for on its own.
            // A link is not worth failing the tab over: without it the note
            // still says the list was cut.
            let page = match pull {
                Some(pull) => pull.html_url.clone(),
                None => self.pull().await.ok().and_then(|p| p.html_url),
            };
            note.url = page.and_then(|page| rows::under(&page, "/commits"));
            rows.push(note);
        }
        Ok(ViewSection {
            id: COMMITS.to_owned(),
            title: "Commits".to_owned(),
            count: Some(listed as u64),
            deferred: false,
            blocks: vec![Block::Entities { title: None, rows }],
        })
    }

    /// The discussion Gitea keeps on the issue of the same index, oldest
    /// first; a comment it cannot read is left out.
    async fn comments(&self) -> Result<Vec<PullComment>, SourceError> {
        let discussion = self
            .source
            .client
            .issue_comments(self.owner, self.repo, self.number)
            .await?;
        Ok(discussion
            .raw
            .into_iter()
            .filter_map(|c| serde_json::from_value(c).ok())
            .collect())
    }

    /// The login the token belongs to, or `None` where Gitea would not say
    /// ([`GiteaSource::viewer`], read once per token and server).
    async fn viewer(&self) -> Result<Option<String>, SourceError> {
        Ok(self.source.viewer().await)
    }
}

/// The discussion: each comment and each review, in time order, as its
/// author and time -- a review's verdict beside them -- and then its text
/// (issue #705).
///
/// Gitea keeps the comments on the issue of the same index and the reviews on
/// the pull request; the two lists are merged by the instant each was made,
/// and a tie keeps the order Gitea gave (reviews first). A draft (`PENDING`)
/// is nobody's yet and a review request (`REQUEST_REVIEW`) is not something
/// anyone said, so neither is drawn.
fn conversation(reviews: &[Review], comments: &[PullComment]) -> ViewSection {
    let mut said: Vec<Said<'_>> = reviews
        .iter()
        .filter_map(|review| {
            let verdict = review_verdict(review)?;
            Some(Said {
                user: review.user.as_ref(),
                at: review.submitted_at.as_deref(),
                verdict: Some(verdict),
                body: review.body.as_deref(),
            })
        })
        .chain(comments.iter().map(|comment| Said {
            user: comment.user.as_ref(),
            at: comment.created_at.as_deref(),
            verdict: None,
            body: comment.body.as_deref(),
        }))
        .collect();
    // Stable: equal instants, and entries with none (last), keep Gitea's
    // order.
    said.sort_by_key(|entry| {
        let instant = entry
            .at
            .and_then(|at| chrono::DateTime::parse_from_rfc3339(at).ok());
        (instant.is_none(), instant)
    });
    let mut blocks = Vec::with_capacity(said.len() * 2);
    for entry in &said {
        let who = login(entry.user).unwrap_or_else(|| "someone".to_owned());
        let when = entry.at.map(when_of).unwrap_or_default();
        let mut facts = vec![fact(&who, when)];
        if let Some((value, tone)) = &entry.verdict {
            facts.push(Fact {
                label: "Review".to_owned(),
                value: value.clone(),
                tone: *tone,
            });
        }
        blocks.push(Block::Facts { facts });
        let body = entry.body.map(str::trim_end).unwrap_or_default();
        if !body.trim().is_empty() {
            blocks.push(Block::Text {
                text: body.to_owned(),
                markdown: true,
            });
        }
    }
    ViewSection {
        id: CONVERSATION.to_owned(),
        title: "Conversation".to_owned(),
        count: Some(said.len() as u64),
        deferred: false,
        blocks,
    }
}

/// One thing said in *Conversation*: a comment, or a review with its verdict.
struct Said<'a> {
    user: Option<&'a UserRef>,
    at: Option<&'a str>,
    verdict: Option<(String, Option<Tone>)>,
    body: Option<&'a str>,
}

/// What a review says in *Conversation*, and its tone: an approval ok, a
/// request for changes warn, a plain comment none, and a dismissed review
/// muted whatever it said. `None` for what is not part of the conversation:
/// a draft, a review request, a state Gitea did not send.
fn review_verdict(review: &Review) -> Option<(String, Option<Tone>)> {
    let (word, tone) = match review.state.as_deref()? {
        "APPROVED" => ("approved", Some(Tone::Ok)),
        "REQUEST_CHANGES" => ("changes requested", Some(Tone::Warn)),
        "COMMENT" => ("commented", None),
        "PENDING" | "REQUEST_REVIEW" | "" => return None,
        other => return Some((other.to_lowercase().replace('_', " "), None)),
    };
    if review.dismissed {
        return Some((format!("{word}, dismissed"), Some(Tone::Muted)));
    }
    Some((word.to_owned(), tone))
}

/// The facts, each reviewer's standing verdict, and the description.
fn overview(pull: &Pull, reviews: &[Review]) -> ViewSection {
    let mut facts = overview_facts(pull);
    facts.extend(verdicts(reviews));
    let mut blocks = vec![Block::Facts { facts }];
    let body = pull.body.as_deref().map(str::trim_end).unwrap_or_default();
    if !body.trim().is_empty() {
        blocks.push(Block::Text {
            text: body.to_owned(),
            markdown: true,
        });
    }
    ViewSection {
        id: OVERVIEW.to_owned(),
        title: "Overview".to_owned(),
        count: None,
        deferred: false,
        blocks,
    }
}

/// The *Files* tab before it is opened: its count, from the pull request's
/// own `changed_files`, and no blocks.
fn files_stub(pull: &Pull) -> ViewSection {
    ViewSection {
        id: FILES.to_owned(),
        title: "Files".to_owned(),
        count: pull.changed_files,
        deferred: true,
        blocks: Vec::new(),
    }
}

/// *Merge…*, the pull request's main write, in the toolbar (issues #667,
/// #702): enabled when Gitea would merge it now -- open, not merged, and
/// Gitea answered `mergeable: true` -- and otherwise disabled with the reason,
/// so the reader learns why rather than looking for the button. On a merged or
/// closed pull request it is not offered at all (`None`): the #701 ruling
/// draws a disabled action only where the entity can still reach the state
/// that enables it, and a closed pull request merges only after a reopen,
/// which is its own action.
///
/// Gitea's flag already says *no* for a conflict, a work-in-progress title and
/// a mergeability check still running, which are the 405s a merge would get;
/// it does not say which, so the reason names all three. An absent flag is not
/// a yes. The styles are the SPI's [`MergeStyle`](knobas_source::MergeStyle)
/// words, `merge` first because it is Gitea's own default. Whether the account
/// may merge is not asked: that is a refusal the write queue shows with
/// Gitea's sentence (ADR-0004).
fn merge_action(pull: &Pull) -> Option<ViewAction> {
    if pull.merged || pull.state.as_deref() != Some("open") {
        return None;
    }
    let head = pull.head.as_ref().and_then(|b| b.r#ref.as_deref());
    let base = pull.base.as_ref().and_then(|b| b.r#ref.as_deref());
    let confirm = match (head, base) {
        (Some(head), Some(base)) => format!("Merge {head} into {base}?"),
        _ => "Merge this pull request?".to_owned(),
    };
    let disabled = match pull.mergeable {
        Some(true) => None,
        Some(false) => Some(format!(
            "Not mergeable into {}: a conflict, a work-in-progress title or a check still \
             running",
            base.unwrap_or("its base")
        )),
        None => Some("Gitea has not said yet whether this can be merged".to_owned()),
    };
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
    Some(ViewAction {
        fields: vec![ActionField {
            required: true,
            ..ActionField::new("style", "Style", FieldKind::Choice { options })
        }],
        confirm: Some(confirm),
        primary: true,
        disabled,
        ..ViewAction::new(crate::WRITE_OP_MERGE_PULL_REQUEST, "Merge…")
    })
}

/// State, author, `head → base`, whether it can be merged, its times.
fn overview_facts(pull: &Pull) -> Vec<Fact> {
    let (state, tone) = if pull.merged {
        ("merged".to_owned(), Some(Tone::Ok))
    } else {
        let word = pull.state.clone().unwrap_or_default();
        let tone = match word.as_str() {
            "open" => Some(Tone::Info),
            "closed" => Some(Tone::Muted),
            _ => None,
        };
        (word, tone)
    };
    let mut facts = Vec::new();
    if !state.is_empty() {
        facts.push(Fact {
            label: "State".to_owned(),
            value: state.clone(),
            tone,
        });
    }
    if let Some(author) = login(pull.user.as_ref()) {
        facts.push(fact("Author", author));
    }
    if let (Some(head), Some(base)) = (pull.head.as_ref(), pull.base.as_ref())
        && let (Some(head_ref), Some(base_ref)) = (head.r#ref.as_deref(), base.r#ref.as_deref())
    {
        let head_repo = head.repo.as_ref().and_then(|r| r.full_name.as_deref());
        let base_repo = base.repo.as_ref().and_then(|r| r.full_name.as_deref());
        let head_name = match head_repo {
            Some(fork) if Some(fork) != base_repo => format!("{fork}:{head_ref}"),
            _ => head_ref.to_owned(),
        };
        facts.push(fact("Branches", format!("{head_name} → {base_ref}")));
    }
    // Only an open pull request can still be merged; Gitea keeps answering
    // the flag on a closed one, where it says nothing.
    if state == "open"
        && let Some(mergeable) = pull.mergeable
    {
        facts.push(Fact {
            label: "Mergeable".to_owned(),
            value: if mergeable { "yes" } else { "no" }.to_owned(),
            tone: (!mergeable).then_some(Tone::Warn),
        });
    }
    if let Some(at) = pull.created_at.as_deref() {
        facts.push(fact("Created", when_of(at)));
    }
    if let Some(at) = pull.updated_at.as_deref() {
        facts.push(fact("Updated", when_of(at)));
    }
    if pull.merged
        && let Some(at) = pull.merged_at.as_deref()
    {
        let value = match login(pull.merged_by.as_ref()) {
            Some(by) => format!("{} by {by}", when_of(at)),
            None => when_of(at),
        };
        facts.push(fact("Merged", value));
    }
    facts
}

/// Each reviewer's standing verdict, in the order reviewers first appear.
///
/// A reviewer's latest review is their verdict, with three exceptions: a
/// draft (`PENDING`) is not yet anyone's verdict, a dismissed review leaves
/// its reviewer none (an earlier verdict does not come back: Gitea counts only
/// a reviewer's latest review), and a plain comment after a verdict does not
/// take it back.
fn verdicts(reviews: &[Review]) -> Vec<Fact> {
    let mut standing: Vec<(String, String)> = Vec::new();
    for review in reviews {
        let Some(who) = login(review.user.as_ref()) else {
            continue;
        };
        let state = review.state.as_deref().unwrap_or_default();
        if review.dismissed {
            standing.retain(|(name, _)| *name != who);
            continue;
        }
        if state.is_empty() || state == "PENDING" {
            continue;
        }
        match standing.iter_mut().find(|(name, _)| *name == who) {
            Some(_) if state == "COMMENT" => {}
            Some((_, verdict)) => state.clone_into(verdict),
            None => standing.push((who, state.to_owned())),
        }
    }
    standing
        .into_iter()
        .map(|(who, state)| {
            let (value, tone) = match state.as_str() {
                "APPROVED" => (format!("approved by {who}"), Some(Tone::Ok)),
                "REQUEST_CHANGES" => (format!("changes requested by {who}"), Some(Tone::Warn)),
                "REQUEST_REVIEW" => (format!("requested from {who}"), Some(Tone::Muted)),
                "COMMENT" => (format!("commented by {who}"), None),
                other => (
                    format!("{} by {who}", other.to_lowercase().replace('_', " ")),
                    None,
                ),
            };
            Fact {
                label: "Review".to_owned(),
                value,
                tone,
            }
        })
        .collect()
}

/// The fields of Gitea's `PullRequest` this view reads.
#[derive(Debug, serde::Deserialize)]
struct Pull {
    /// What *Edit…* prefills (#706).
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    state: Option<String>,
    #[serde(default)]
    merged: bool,
    #[serde(default)]
    mergeable: Option<bool>,
    #[serde(default)]
    user: Option<UserRef>,
    #[serde(default)]
    merged_by: Option<UserRef>,
    #[serde(default)]
    head: Option<PullBranch>,
    #[serde(default)]
    base: Option<PullBranch>,
    /// Gitea types the times as `date-time`; kept as text for [`when_of`],
    /// which shows the offset the server sent.
    #[serde(default)]
    created_at: Option<String>,
    #[serde(default)]
    updated_at: Option<String>,
    #[serde(default)]
    merged_at: Option<String>,
    #[serde(default)]
    changed_files: Option<u64>,
    #[serde(default)]
    html_url: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
struct PullBranch {
    #[serde(default)]
    r#ref: Option<String>,
    #[serde(default)]
    repo: Option<PullRepo>,
}

#[derive(Debug, serde::Deserialize)]
struct PullRepo {
    #[serde(default)]
    full_name: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
struct Review {
    #[serde(default)]
    user: Option<UserRef>,
    #[serde(default)]
    state: Option<String>,
    #[serde(default)]
    dismissed: bool,
    #[serde(default)]
    body: Option<String>,
    /// As for [`Pull`]'s times, kept as text for [`when_of`].
    #[serde(default)]
    submitted_at: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
struct PullComment {
    #[serde(default)]
    user: Option<UserRef>,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    created_at: Option<String>,
}
