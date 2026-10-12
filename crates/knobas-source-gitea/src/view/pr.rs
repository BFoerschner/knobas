//! A pull request's view (#662, spec #660 stories 6-9 and 13): *Overview*,
//! *Files* (deferred), *Commits* and *Conversation*.
//!
//! The overview reads the pull request and its reviews; *Commits* its commit
//! listing; *Conversation* the discussion Gitea keeps on the issue of the same
//! index (the read the sync makes). *Files* is the one expensive read -- the
//! whole `.diff` -- so it is a stub with a count until its tab is opened
//! (ADR-0020 rule 3).

use chrono::{DateTime, Utc};
use knobas_source::SourceError;
use knobas_source::view::{Block, Fact, SourceView, Tone, ViewAction, ViewRow, ViewSection};

use super::{fact, login, short, when_of};
use crate::model::{Commit, UserRef};
use crate::source::GiteaSource;
use crate::{keys, map};

const OVERVIEW: &str = "overview";
const FILES: &str = "files";
const COMMITS: &str = "commits";
const CONVERSATION: &str = "conversation";

/// How many pages of commits the *Commits* tab reads at most: 250 commits at
/// Gitea's page of 50. A longer pull request says how many it left out.
const COMMIT_PAGES: u32 = 5;

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
    let sections = match section {
        None => {
            let pull = at.pull().await?;
            vec![
                at.overview(&pull).await?,
                files_stub(&pull),
                at.commits().await?,
                at.conversation().await?,
            ]
        }
        Some(OVERVIEW) => vec![at.overview(&at.pull().await?).await?],
        Some(FILES) => vec![at.files().await?],
        Some(COMMITS) => vec![at.commits().await?],
        Some(CONVERSATION) => vec![at.conversation().await?],
        Some(other) => {
            return Err(SourceError::protocol(format!(
                "gitea: a pull request's view has no section {other:?} (asked for {entity:?})"
            )));
        }
    };
    Ok(SourceView { sections })
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

    async fn overview(&self, pull: &Pull) -> Result<ViewSection, SourceError> {
        let reviews = self
            .source
            .client
            .pull_reviews(self.owner, self.repo, self.number)
            .await?;
        let reviews: Vec<Review> = reviews
            .into_iter()
            .filter_map(|r| serde_json::from_value(r).ok())
            .collect();
        let mut facts = overview_facts(pull);
        facts.extend(verdicts(&reviews));
        let mut blocks = vec![Block::Facts { facts }];
        blocks.extend(merge_action(pull));
        let body = pull.body.as_deref().map(str::trim_end).unwrap_or_default();
        if !body.trim().is_empty() {
            blocks.push(Block::Text {
                text: body.to_owned(),
                markdown: true,
            });
        }
        Ok(ViewSection {
            id: OVERVIEW.to_owned(),
            title: "Overview".to_owned(),
            count: None,
            deferred: false,
            blocks,
        })
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

    async fn commits(&self) -> Result<ViewSection, SourceError> {
        let mut records = Vec::new();
        let mut total = None;
        let mut capped = true;
        for page in 1..=COMMIT_PAGES {
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
                capped = false;
                break;
            }
        }
        // Counted before parsing: a record that does not parse was still
        // read, and is not one the budget left out.
        let read = records.len();
        let now = Utc::now();
        let rows: Vec<ViewRow> = records
            .into_iter()
            .filter_map(|r| serde_json::from_value::<Commit>(r).ok())
            .map(|c| self.commit_row(&c, now))
            .collect();
        let total = total.unwrap_or(read).max(read);
        // Said whenever the walk stopped on its budget: with the server's
        // count, how many it left out; without one, that there may be more.
        let left_out = if total > read {
            Some(format!(
                "The first {read} of {total} commits; the rest are on Gitea."
            ))
        } else if capped {
            Some(format!(
                "The first {read} commits; there may be more on Gitea."
            ))
        } else {
            None
        }
        .map(|text| Block::Text {
            text,
            markdown: false,
        });
        let mut blocks = vec![Block::Entities { title: None, rows }];
        blocks.extend(left_out);
        Ok(ViewSection {
            id: COMMITS.to_owned(),
            title: "Commits".to_owned(),
            count: Some(total as u64),
            deferred: false,
            blocks,
        })
    }

    fn commit_row(&self, commit: &Commit, now: DateTime<Utc>) -> ViewRow {
        let inner = commit.commit.as_ref();
        let title = inner
            .and_then(|c| c.message.as_deref())
            .map(map::subject)
            .filter(|t| !t.is_empty())
            .unwrap_or_else(|| short(&commit.sha));
        let author = inner
            .and_then(|c| c.author.as_ref())
            .and_then(|a| a.name.clone())
            .or_else(|| login(commit.author.as_ref()))
            .filter(|a| !a.trim().is_empty());
        let when = commit.happened_at().map(|t| ago(t, now));
        let meta = match (author, when) {
            (Some(who), Some(when)) => Some(format!("{who} · {when}")),
            (Some(one), None) | (None, Some(one)) => Some(one),
            (None, None) => None,
        };
        ViewRow {
            // The address the commit would have; the app clears it where the
            // mirror does not hold it (rule 7).
            entity_id: Some(format!(
                "{}:{}",
                self.source.id,
                keys::commit_key(self.owner, self.repo, &commit.sha)
            )),
            title,
            meta,
            tone: None,
            chip: Some(short(&commit.sha)),
            url: commit.html_url.clone(),
            commit: None,
        }
    }

    /// Each comment, in order, as its author and time and then its text.
    async fn conversation(&self) -> Result<ViewSection, SourceError> {
        let discussion = self
            .source
            .client
            .issue_comments(self.owner, self.repo, self.number)
            .await?;
        let comments: Vec<PullComment> = discussion
            .raw
            .into_iter()
            .filter_map(|c| serde_json::from_value(c).ok())
            .collect();
        let mut blocks = Vec::with_capacity(comments.len() * 2);
        for comment in &comments {
            let who = login(comment.user.as_ref()).unwrap_or_else(|| "someone".to_owned());
            let when = comment
                .created_at
                .as_deref()
                .map(when_of)
                .unwrap_or_default();
            blocks.push(Block::Facts {
                facts: vec![fact(&who, when)],
            });
            let body = comment
                .body
                .as_deref()
                .map(str::trim_end)
                .unwrap_or_default();
            if !body.trim().is_empty() {
                blocks.push(Block::Text {
                    text: body.to_owned(),
                    markdown: true,
                });
            }
        }
        Ok(ViewSection {
            id: CONVERSATION.to_owned(),
            title: "Conversation".to_owned(),
            count: Some(comments.len() as u64),
            deferred: false,
            blocks,
        })
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

/// *Merge…*, when Gitea would merge this pull request now (issue #667): it is
/// open, not merged, and Gitea answered `mergeable: true`.
///
/// Gitea's flag already says *no* for a conflict, a work-in-progress title and
/// a mergeability check still running, which are the 405s a merge would get;
/// an absent flag is not a yes. The styles are the SPI's
/// [`MergeStyle`](knobas_source::MergeStyle) words, `merge` first because it
/// is Gitea's own default. Whether the account may merge is not asked: that is
/// a refusal the write queue shows with Gitea's sentence (ADR-0004).
fn merge_action(pull: &Pull) -> Option<Block> {
    let open = !pull.merged && pull.state.as_deref() == Some("open");
    if !(open && pull.mergeable == Some(true)) {
        return None;
    }
    let head = pull.head.as_ref().and_then(|b| b.r#ref.as_deref());
    let base = pull.base.as_ref().and_then(|b| b.r#ref.as_deref());
    let confirm = match (head, base) {
        (Some(head), Some(base)) => format!("Merge {head} into {base}?"),
        _ => "Merge this pull request?".to_owned(),
    };
    Some(Block::Actions {
        actions: vec![ViewAction {
            write_op: crate::WRITE_OP_MERGE_PULL_REQUEST.to_owned(),
            label: "Merge…".to_owned(),
            options: knobas_source::MergeStyle::ALL
                .iter()
                .map(|style| style.as_str().to_owned())
                .collect(),
            confirm: Some(confirm),
        }],
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
                "REQUEST_CHANGES" => (format!("changes requested by {who}"), Some(Tone::Err)),
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

/// How long ago, as the shell's `ago()` (`app/src/lib/shell/time.ts`) says
/// it, so a commit's age reads the same here as on a room's row: minutes,
/// then hours, then the UTC date. A time in the future is *just now*.
fn ago(then: DateTime<Utc>, now: DateTime<Utc>) -> String {
    let seconds = (now - then).num_seconds();
    #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
    let minutes = (seconds as f64 / 60.0).round() as i64;
    if minutes < 1 {
        return "just now".to_owned();
    }
    if minutes < 60 {
        return format!("{minutes} min ago");
    }
    #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
    let hours = (minutes as f64 / 60.0).round() as i64;
    if hours < 24 {
        return format!("{hours} h ago");
    }
    then.format("%Y-%m-%d").to_string()
}

/// The fields of Gitea's `PullRequest` this view reads.
#[derive(Debug, serde::Deserialize)]
struct Pull {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn at(raw: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(raw)
            .unwrap()
            .with_timezone(&Utc)
    }

    /// The boundaries of the shell's `ago()`, which this mirrors.
    #[test]
    fn an_age_reads_as_the_shells_does() {
        let now = at("2026-08-22T12:00:00Z");
        for (then, said) in [
            ("2026-08-22T12:05:00Z", "just now"),
            ("2026-08-22T11:59:40Z", "just now"),
            ("2026-08-22T11:59:00Z", "1 min ago"),
            ("2026-08-22T11:01:00Z", "59 min ago"),
            ("2026-08-22T11:00:00Z", "1 h ago"),
            ("2026-08-21T12:31:00Z", "23 h ago"),
            ("2026-08-21T12:00:00Z", "2026-08-21"),
        ] {
            assert_eq!(ago(at(then), now), said, "{then}");
        }
    }
}
