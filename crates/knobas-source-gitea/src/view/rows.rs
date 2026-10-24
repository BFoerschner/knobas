//! What the Gitea views share (#663, #678): the partial records they read,
//! the entity rows those become -- one commit row for a pull request's, a
//! branch's and a repository's lists -- and the note a list ends on where it
//! stopped on its budget.
//!
//! Every row names the entity id this adapter mints for the thing it shows,
//! so it opens that thing's detail; the app clears the id where the mirror
//! does not hold it (ADR-0020 rule 7).

use chrono::{DateTime, Utc};
use knobas_source::SourceError;
use knobas_source::view::{Block, Tone, ViewRow};

use crate::model::{Commit, Repo};
use crate::source::GiteaSource;
use crate::{keys, map};

/// How many of the default branch's newest commits a *Commits* tab lists.
pub(super) const LATEST_COMMITS: u32 = 20;

/// How many open pull requests a view reads at most: twenty pages of fifty.
/// A repository past that has a different problem than its view.
const PULL_PAGES: u32 = 20;

/// How many of Gitea's pages of fifty a pull request's *Commits* tab reads.
pub(super) const COMMIT_PAGES: u32 = 5;
/// How many commits a *Commits* tab lists at most, a pull request's and a
/// branch's alike: 250.
pub(super) const COMMIT_CAP: usize = (COMMIT_PAGES * crate::client::PAGE_SIZE) as usize;

/// The repository fields the two views read: the sync's own projection, and
/// what only a view shows.
#[derive(Debug, Clone, serde::Deserialize)]
pub(super) struct RepoRecord {
    #[serde(flatten)]
    pub repo: Repo,
    #[serde(default)]
    pub private: bool,
    #[serde(default)]
    pub internal: bool,
    #[serde(default)]
    pub stars_count: u64,
    #[serde(default)]
    pub forks_count: u64,
    /// Gitea counts it in KiB.
    #[serde(default)]
    pub size: u64,
}

impl RepoRecord {
    /// The branch everything is measured against. Gitea names one on every
    /// repository, an empty one included; `main` is its own default.
    pub(super) fn default_branch(&self) -> &str {
        self.repo
            .default_branch
            .as_deref()
            .filter(|b| !b.is_empty())
            .unwrap_or("main")
    }

    /// The repository's web page with `tail` after it (`/branches`), where
    /// Gitea named the page.
    pub(super) fn web(&self, tail: &str) -> Option<String> {
        under(self.repo.html_url.as_deref()?, tail)
    }
}

/// `page` with `tail` after it, or `None` for a blank page.
pub(super) fn under(page: &str, tail: &str) -> Option<String> {
    let base = page.trim_end_matches('/');
    (!base.is_empty()).then(|| format!("{base}{tail}"))
}

/// A commit as the compare, `/commits` and `/pulls/{n}/commits` list it.
#[derive(Debug, Clone, serde::Deserialize)]
pub(super) struct ListedCommit {
    #[serde(flatten)]
    pub record: Commit,
    /// The files this commit touched; the compare carries them by default.
    /// `/commits?files=false` answers `null` here (1.27.3, measured), which
    /// is none, not an unreadable record.
    #[serde(default)]
    pub files: Option<Vec<TouchedFile>>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub(super) struct TouchedFile {
    pub filename: String,
}

/// A branch's tip as `/branches` answers it (`PayloadCommit`), and whether
/// it is protected (#707: a protected branch is not offered for deletion).
#[derive(Debug, Clone, serde::Deserialize)]
pub(super) struct BranchRecord {
    #[serde(default)]
    pub commit: Option<TipCommit>,
    #[serde(default)]
    pub protected: bool,
}

/// One branch of a repository's listing.
#[derive(Debug, serde::Deserialize)]
pub(super) struct ListedBranch {
    pub name: String,
    #[serde(default)]
    pub commit: Option<TipCommit>,
}

/// How many pages of fifty branches a view reads at most.
const BRANCH_PAGES: u32 = 20;

/// Every branch of a repository, page by page until an empty one, at most
/// [`BRANCH_PAGES`] pages, in Gitea's own order.
pub(super) async fn all_branches(
    source: &GiteaSource,
    owner: &str,
    repo: &str,
) -> Result<Paged<ListedBranch>, SourceError> {
    let mut items = Vec::new();
    for page in 1..=BRANCH_PAGES {
        let (batch, _) = source.client.branches(owner, repo, page).await?;
        if batch.is_empty() {
            return Ok(Paged { items, cut: false });
        }
        for raw in batch {
            let branch: ListedBranch = serde_json::from_value(raw).map_err(|error| {
                SourceError::protocol(format!("gitea: an unreadable branch record: {error}"))
            })?;
            items.push(branch);
        }
    }
    Ok(Paged { items, cut: true })
}

#[derive(Debug, Clone, serde::Deserialize)]
pub(super) struct TipCommit {
    pub id: String,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub timestamp: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub author: Option<TipAuthor>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub(super) struct TipAuthor {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub username: Option<String>,
}

/// An open pull request as `/pulls` lists it.
#[derive(Debug, Clone, serde::Deserialize)]
pub(super) struct OpenPull {
    pub number: u64,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub html_url: Option<String>,
    #[serde(default)]
    pub user: Option<crate::model::UserRef>,
    #[serde(default)]
    pub head: Option<PullEnd>,
    #[serde(default)]
    pub base: Option<PullEnd>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub(super) struct PullEnd {
    #[serde(rename = "ref", default)]
    pub reference: Option<String>,
    #[serde(default)]
    pub repo: Option<PullRepo>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub(super) struct PullRepo {
    #[serde(default)]
    pub full_name: Option<String>,
}

impl OpenPull {
    /// Whether this pull request is opened **from** `branch` of `owner/repo`
    /// -- not from a fork's branch of the same name, nor from one whose
    /// repository Gitea no longer names (a deleted fork's head is `null`).
    pub(super) fn is_from(&self, owner: &str, repo: &str, branch: &str) -> bool {
        let Some(head) = &self.head else { return false };
        let same_repo = head
            .repo
            .as_ref()
            .and_then(|r| r.full_name.as_deref())
            .is_some_and(|name| name == keys::repo_key(owner, repo));
        same_repo && head.reference.as_deref() == Some(branch)
    }
}

/// What a paged read gathered, and whether it stopped on its page budget
/// rather than on an empty page.
pub(super) struct Paged<T> {
    pub items: Vec<T>,
    pub cut: bool,
}

impl<T> Default for Paged<T> {
    /// Nothing, and nothing left out: what a repository with no pull
    /// requests to read has.
    fn default() -> Self {
        Self {
            items: Vec::new(),
            cut: false,
        }
    }
}

/// Every open pull request of a repository, page by page until an empty one
/// (#81's rule), at most [`PULL_PAGES`] pages.
pub(super) async fn open_pulls(
    source: &GiteaSource,
    owner: &str,
    repo: &str,
) -> Result<Paged<OpenPull>, SourceError> {
    let mut items = Vec::new();
    for page in 1..=PULL_PAGES {
        let batch: Vec<OpenPull> = source.client.open_pulls_as(owner, repo, page).await?;
        if batch.is_empty() {
            return Ok(Paged { items, cut: false });
        }
        items.extend(batch);
    }
    Ok(Paged { items, cut: true })
}

/// The last row of a list that holds less than the source has: muted, no
/// address, and Gitea's own page for the rest. `None` where the list is
/// whole.
///
/// `read` is what the list holds, `total` the source's own count where it
/// sent one, and `cut` whether the read stopped on its budget.
pub(super) fn left_out(
    noun: &str,
    read: usize,
    total: Option<usize>,
    cut: bool,
    url: Option<String>,
) -> Option<ViewRow> {
    let title = match total {
        Some(total) if total > read => {
            format!("The first {read} of {total} {noun}; the rest are on Gitea.")
        }
        _ if cut => format!("The first {read} {noun}; there may be more on Gitea."),
        _ => return None,
    };
    Some(ViewRow {
        entity_id: None,
        title,
        meta: None,
        tone: Some(Tone::Muted),
        chip: None,
        url,
        commit: None,
    })
}

/// A commit, however Gitea listed it, as every view's row reads it.
pub(super) struct CommitLine<'a> {
    pub sha: &'a str,
    pub message: Option<&'a str>,
    pub author: Option<&'a str>,
    pub when: Option<DateTime<Utc>>,
    pub url: Option<&'a str>,
}

impl<'a> From<&'a Commit> for CommitLine<'a> {
    /// The author git recorded, or the Gitea account where git named none.
    fn from(c: &'a Commit) -> Self {
        let inner = c.commit.as_ref();
        let git_author = inner
            .and_then(|i| i.author.as_ref())
            .and_then(|a| a.name.as_deref());
        Self {
            sha: &c.sha,
            message: inner.and_then(|i| i.message.as_deref()),
            author: non_blank(git_author)
                .or_else(|| non_blank(c.author.as_ref().and_then(|u| u.login.as_deref()))),
            // Go's zero time is no time (`map::real_time`).
            when: map::real_time(c.happened_at()),
            url: c.html_url.as_deref(),
        }
    }
}

impl<'a> From<&'a TipCommit> for CommitLine<'a> {
    fn from(tip: &'a TipCommit) -> Self {
        let author = tip.author.as_ref();
        Self {
            sha: &tip.id,
            message: tip.message.as_deref(),
            author: non_blank(author.and_then(|a| a.name.as_deref()))
                .or_else(|| non_blank(author.and_then(|a| a.username.as_deref()))),
            when: map::real_time(tip.timestamp.as_deref().and_then(utc)),
            url: tip.url.as_deref(),
        }
    }
}

fn non_blank(text: Option<&str>) -> Option<&str> {
    text.map(str::trim).filter(|t| !t.is_empty())
}

fn utc(raw: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(raw)
        .ok()
        .map(|t| t.with_timezone(&Utc))
}

/// `a · b · c`, skipping the blanks.
pub(super) fn joined<'a>(parts: impl IntoIterator<Item = Option<&'a str>>) -> Option<String> {
    let parts: Vec<&str> = parts
        .into_iter()
        .flatten()
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .collect();
    (!parts.is_empty()).then(|| parts.join(" · "))
}

/// A commit as a row, the same in every list (#678): its subject as the
/// title (its short sha where the message is blank), the short sha as the
/// chip, `author · age` as the meta.
pub(super) fn commit_row<'a>(
    source_id: &str,
    owner: &str,
    repo: &str,
    commit: impl Into<CommitLine<'a>>,
) -> ViewRow {
    let line = commit.into();
    let short = super::short(line.sha);
    let subject = map::subject(line.message.unwrap_or_default());
    let age = line.when.map(|t| super::ago(t, Utc::now()));
    ViewRow {
        // The address the commit would have; the app clears it where the
        // mirror does not hold it (rule 7).
        entity_id: Some(format!(
            "{source_id}:{}",
            keys::commit_key(owner, repo, line.sha)
        )),
        title: if subject.is_empty() {
            short.clone()
        } else {
            subject
        },
        meta: joined([line.author, age.as_deref()]),
        tone: None,
        chip: Some(short),
        url: line.url.map(str::to_owned),
        commit: None,
    }
}

/// An open pull request as a row: its title, the rooms' `open` chip, and
/// `#n · head → base · author`.
pub(super) fn pull_row(source_id: &str, owner: &str, repo: &str, pull: &OpenPull) -> ViewRow {
    let number = format!("#{}", pull.number);
    let route = match (
        pull.head.as_ref().and_then(|h| h.reference.as_deref()),
        pull.base.as_ref().and_then(|b| b.reference.as_deref()),
    ) {
        (Some(head), Some(base)) => Some(format!("{head} → {base}")),
        _ => None,
    };
    ViewRow {
        entity_id: Some(format!(
            "{source_id}:{}",
            keys::pr_key(owner, repo, pull.number)
        )),
        title: pull
            .title
            .clone()
            .filter(|t| !t.trim().is_empty())
            .unwrap_or_else(|| number.clone()),
        meta: joined([
            Some(number.as_str()),
            route.as_deref(),
            pull.user.as_ref().and_then(|u| u.login.as_deref()),
        ]),
        // The same chip a room draws for an open pull request (#666).
        tone: Some(Tone::Info),
        chip: Some("open".to_owned()),
        url: pull.html_url.clone(),
        commit: None,
    }
}

/// Every future to its end, side by side, in the order given; the first
/// failure fails the lot, as `try_join!` does for a fixed few. For the
/// per-branch reads a repository's *New pull request…* makes, whose number
/// is the listing's.
pub(super) async fn all_of<T, E, F>(futures: Vec<F>) -> Result<Vec<T>, E>
where
    F: std::future::Future<Output = Result<T, E>>,
{
    let mut pending: Vec<_> = futures.into_iter().map(Box::pin).collect();
    let mut done: Vec<Option<T>> = std::iter::repeat_with(|| None)
        .take(pending.len())
        .collect();
    std::future::poll_fn(|cx| {
        let mut waiting = false;
        for (future, slot) in pending.iter_mut().zip(done.iter_mut()) {
            if slot.is_some() {
                continue;
            }
            match future.as_mut().poll(cx) {
                std::task::Poll::Ready(Ok(value)) => *slot = Some(value),
                std::task::Poll::Ready(Err(error)) => return std::task::Poll::Ready(Err(error)),
                std::task::Poll::Pending => waiting = true,
            }
        }
        if waiting {
            std::task::Poll::Pending
        } else {
            std::task::Poll::Ready(Ok(()))
        }
    })
    .await?;
    Ok(done.into_iter().flatten().collect())
}

/// One muted line standing where something could not be read, saying why:
/// a pull request's checks (#710) or its comments (#2042).
pub(super) fn muted_line(title: String) -> Block {
    Block::Entities {
        title: None,
        rows: vec![ViewRow {
            entity_id: None,
            title,
            meta: None,
            tone: Some(Tone::Muted),
            chip: None,
            url: None,
            commit: None,
        }],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One boxed probe future, so a list of differently-timed ones has one type.
    type Probe = std::pin::Pin<Box<dyn std::future::Future<Output = Result<u8, &'static str>>>>;

    #[tokio::test]
    async fn all_of_keeps_the_order_and_fails_on_the_first_failure() {
        let ok: Vec<Probe> = vec![
            Box::pin(async { Ok(1) }),
            Box::pin(async {
                tokio::task::yield_now().await;
                Ok(2)
            }),
            Box::pin(async { Ok(3) }),
        ];
        assert_eq!(all_of(ok).await, Ok(vec![1, 2, 3]));
        let failing: Vec<Probe> = vec![Box::pin(async { Ok(1) }), Box::pin(async { Err("no") })];
        assert_eq!(all_of(failing).await, Err("no"));
        assert_eq!(
            all_of::<u8, &str, std::future::Ready<_>>(Vec::new()).await,
            Ok(vec![])
        );
    }
}
