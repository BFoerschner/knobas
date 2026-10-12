//! What a branch's and a repository's views share (#663): the partial
//! records they read, and the entity rows those become.
//!
//! Every row names the entity id this adapter mints for the thing it shows,
//! so it opens that thing's detail; the app clears the id where the mirror
//! does not hold it (ADR-0020 rule 7).

use knobas_source::SourceError;
use knobas_source::view::{Tone, ViewRow};

use crate::keys;
use crate::source::GiteaSource;

/// How many open pull requests a view reads at most: twenty pages of fifty.
/// A repository past that has a different problem than its view.
const PULL_PAGES: u32 = 20;

/// The repository fields the two views read.
#[derive(Debug, Clone, serde::Deserialize)]
pub(super) struct RepoRecord {
    #[serde(default)]
    pub default_branch: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub private: bool,
    #[serde(default)]
    pub internal: bool,
    #[serde(default)]
    pub empty: bool,
    #[serde(default)]
    pub mirror: bool,
    #[serde(default = "enabled")]
    pub has_pull_requests: bool,
    #[serde(default)]
    pub stars_count: u64,
    #[serde(default)]
    pub forks_count: u64,
    /// Gitea counts it in KiB.
    #[serde(default)]
    pub size: u64,
}

fn enabled() -> bool {
    true
}

impl RepoRecord {
    /// The branch everything is measured against. Gitea names one on every
    /// repository, an empty one included; `main` is its own default.
    pub(super) fn default_branch(&self) -> &str {
        self.default_branch
            .as_deref()
            .filter(|b| !b.is_empty())
            .unwrap_or("main")
    }

    /// Whether `/pulls` can answer anything but a bare 404 -- Gitea's
    /// `CanEnablePulls`, as `model::Repo::can_have_pulls` reads it.
    pub(super) fn can_have_pulls(&self) -> bool {
        self.has_pull_requests && !self.mirror && !self.empty
    }
}

/// A commit as the compare and `/commits` list it.
#[derive(Debug, Clone, serde::Deserialize)]
pub(super) struct ListedCommit {
    pub sha: String,
    #[serde(default)]
    pub html_url: Option<String>,
    #[serde(default)]
    pub created: Option<String>,
    #[serde(default)]
    pub commit: Option<ListedInner>,
    /// The files this commit touched; the compare carries them by default.
    /// `/commits?files=false` answers `null` here (1.27.3, measured), which
    /// is none, not an unreadable record.
    #[serde(default)]
    pub files: Option<Vec<TouchedFile>>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub(super) struct ListedInner {
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub author: Option<super::Signature>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub(super) struct TouchedFile {
    pub filename: String,
}

/// A branch's tip as `/branches` answers it (`PayloadCommit`).
#[derive(Debug, Clone, serde::Deserialize)]
pub(super) struct BranchRecord {
    #[serde(default)]
    pub commit: Option<TipCommit>,
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
    /// -- not from a fork's branch of the same name.
    pub(super) fn is_from(&self, owner: &str, repo: &str, branch: &str) -> bool {
        let Some(head) = &self.head else { return false };
        let same_repo = head
            .repo
            .as_ref()
            .and_then(|r| r.full_name.as_deref())
            .is_none_or(|name| name == keys::repo_key(owner, repo));
        same_repo && head.reference.as_deref() == Some(branch)
    }
}

/// Every open pull request of a repository, page by page until an empty one
/// (#81's rule), at most [`PULL_PAGES`] pages.
pub(super) async fn open_pulls(
    source: &GiteaSource,
    owner: &str,
    repo: &str,
) -> Result<Vec<OpenPull>, SourceError> {
    let mut all = Vec::new();
    for page in 1..=PULL_PAGES {
        let batch: Vec<OpenPull> = source.client.open_pulls_as(owner, repo, page).await?;
        if batch.is_empty() {
            break;
        }
        all.extend(batch);
    }
    Ok(all)
}

/// The first line of a commit message: what a row is titled by.
pub(super) fn subject(message: Option<&str>) -> String {
    message
        .unwrap_or_default()
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or_default()
        .trim()
        .to_owned()
}

/// A git time as a row's quiet reading: to the minute, in the offset it was
/// recorded in. Verbatim where it does not parse.
pub(super) fn stamp(raw: &str) -> String {
    chrono::DateTime::parse_from_rfc3339(raw)
        .map(|t| t.format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_else(|_| raw.to_owned())
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

/// A listed commit as a row: its subject, then short sha, author and time.
pub(super) fn commit_row(source_id: &str, owner: &str, repo: &str, c: &ListedCommit) -> ViewRow {
    let inner = c.commit.as_ref();
    let author = inner.and_then(|i| i.author.as_ref());
    let when = c
        .created
        .as_deref()
        .or_else(|| author.and_then(|a| a.date.as_deref()))
        .map(stamp);
    ViewRow {
        entity_id: Some(format!(
            "{source_id}:{}",
            keys::commit_key(owner, repo, &c.sha)
        )),
        title: subject(inner.and_then(|i| i.message.as_deref())),
        meta: joined([
            Some(super::short(&c.sha).as_str()),
            author.and_then(|a| a.name.as_deref()),
            when.as_deref(),
        ]),
        tone: None,
        chip: None,
        url: c.html_url.clone(),
    }
}

/// A branch's tip as a row, the same reading as [`commit_row`].
pub(super) fn tip_row(source_id: &str, owner: &str, repo: &str, tip: &TipCommit) -> ViewRow {
    let author = tip
        .author
        .as_ref()
        .and_then(|a| a.name.as_deref().or(a.username.as_deref()));
    let when = tip.timestamp.as_deref().map(stamp);
    ViewRow {
        entity_id: Some(format!(
            "{source_id}:{}",
            keys::commit_key(owner, repo, &tip.id)
        )),
        title: subject(tip.message.as_deref()),
        meta: joined([
            Some(super::short(&tip.id).as_str()),
            author,
            when.as_deref(),
        ]),
        tone: None,
        chip: None,
        url: tip.url.clone(),
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
    }
}
