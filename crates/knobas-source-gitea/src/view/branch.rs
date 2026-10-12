//! A branch's view (#663, spec #660 story 11): how far it is from its
//! repository's default branch, what it carries, and whether a pull request
//! is open from it.
//!
//! * *Overview* -- facts (the base, ahead and behind), the last commit, and
//!   the open pull request whose head is this branch, or `none`.
//! * *Commits* (count = ahead) -- what the branch carries over the default
//!   branch, from `compare/{default}...{branch}`.
//! * *Diff* (deferred, count = files) -- the same compare as a diff
//!   (`?output=diff`).
//!
//! The default branch itself is measured against nothing: it says *default
//! branch*, its *Commits* are its latest twenty, and it has no *Diff*.

use std::collections::BTreeSet;

use knobas_source::SourceError;
use knobas_source::view::{Block, Fact, SourceView, Tone, ViewSection};

use super::fact;
use super::rows::{self, BranchRecord, ListedCommit, RepoRecord};
use crate::source::GiteaSource;

const OVERVIEW: &str = "overview";
const COMMITS: &str = "commits";
const DIFF: &str = "diff";

/// How many commits the default branch's *Commits* tab lists.
const LATEST: u32 = 20;

/// What `compare/{base}...{head}` answers.
#[derive(Debug, serde::Deserialize)]
struct Compare {
    #[serde(default)]
    total_commits: Option<u64>,
    #[serde(default)]
    commits: Vec<ListedCommit>,
}

impl Compare {
    fn ahead(&self) -> u64 {
        self.total_commits.unwrap_or(self.commits.len() as u64)
    }

    /// How many files the compare touches: every file any of its commits
    /// touched, once. This can over-count the diff by a file a later commit
    /// put back exactly as the base has it; reading the diff to count it
    /// exactly would cost the read the deferral saves.
    fn files(&self) -> u64 {
        let touched: BTreeSet<&str> = self
            .commits
            .iter()
            .flat_map(|c| c.files.iter().flatten())
            .map(|f| f.filename.as_str())
            .collect();
        touched.len() as u64
    }
}

pub(super) async fn draw(
    source: &GiteaSource,
    entity: &str,
    owner: &str,
    repo: &str,
    branch: &str,
    section: Option<&str>,
) -> Result<SourceView, SourceError> {
    let record: RepoRecord = source.client.repo_as(owner, repo).await?;
    let base = record.default_branch().to_owned();
    let is_default = branch == base;

    let sections = match section {
        Some(DIFF) if !is_default => vec![diff(source, owner, repo, &base, branch).await?],
        Some(OVERVIEW | COMMITS) | None => {
            whole(source, &record, owner, repo, branch, section).await?
        }
        Some(other) => {
            return Err(SourceError::protocol(format!(
                "gitea: a branch's view has no section {other:?} (asked for {entity:?})"
            )));
        }
    };
    Ok(SourceView { sections })
}

/// The whole view, or one of its two eager sections.
async fn whole(
    source: &GiteaSource,
    record: &RepoRecord,
    owner: &str,
    repo: &str,
    branch: &str,
    only: Option<&str>,
) -> Result<Vec<ViewSection>, SourceError> {
    let id = &source.id;
    let base = record.default_branch();
    let is_default = branch == base;
    let tip: BranchRecord = source.client.branch_as(owner, repo, branch).await?;
    // A mirror's `/pulls` is a bare 404: it has none, which is not a failure.
    let pulls: Vec<_> = if record.can_have_pulls() {
        rows::open_pulls(source, owner, repo).await?
    } else {
        Vec::new()
    }
    .into_iter()
    .filter(|p| p.is_from(owner, repo, branch))
    .collect();

    let mut facts = Vec::new();
    let (commits, ahead, files) = if is_default {
        facts.push(fact("Branch", "default branch".to_owned()));
        let latest: Vec<ListedCommit> = source
            .client
            .latest_commits_as(owner, repo, branch, LATEST)
            .await?;
        (latest, None, None)
    } else {
        let ahead: Compare = source.client.compare_as(owner, repo, base, branch).await?;
        let behind: Compare = source.client.compare_as(owner, repo, branch, base).await?;
        facts.push(fact("Base", base.to_owned()));
        facts.push(fact("Ahead", ahead.ahead().to_string()));
        facts.push(fact("Behind", behind.ahead().to_string()));
        let (count, files) = (ahead.ahead(), ahead.files());
        (ahead.commits, Some(count), Some(files))
    };
    if pulls.is_empty() {
        facts.push(Fact {
            label: "Pull request".to_owned(),
            value: "none".to_owned(),
            tone: Some(Tone::Muted),
        });
    }

    let mut blocks = vec![Block::Facts { facts }];
    if let Some(commit) = &tip.commit {
        blocks.push(Block::Entities {
            title: Some("Last commit".to_owned()),
            rows: vec![rows::tip_row(id, owner, repo, commit)],
        });
    }
    if !pulls.is_empty() {
        blocks.push(Block::Entities {
            title: Some(if pulls.len() == 1 {
                "Pull request".to_owned()
            } else {
                "Pull requests".to_owned()
            }),
            rows: pulls
                .iter()
                .map(|p| rows::pull_row(id, owner, repo, p))
                .collect(),
        });
    }

    let mut sections = vec![
        ViewSection {
            id: OVERVIEW.to_owned(),
            title: "Overview".to_owned(),
            count: None,
            deferred: false,
            blocks,
        },
        ViewSection {
            id: COMMITS.to_owned(),
            title: "Commits".to_owned(),
            count: ahead,
            deferred: false,
            blocks: vec![Block::Entities {
                title: None,
                rows: commits
                    .iter()
                    .map(|c| rows::commit_row(id, owner, repo, c))
                    .collect(),
            }],
        },
    ];
    if let Some(files) = files {
        sections.push(ViewSection {
            id: DIFF.to_owned(),
            title: "Diff".to_owned(),
            count: Some(files),
            deferred: true,
            blocks: Vec::new(),
        });
    }
    if let Some(only) = only {
        sections.retain(|s| s.id == only);
    }
    Ok(sections)
}

/// The deferred *Diff*: the compare as `git diff` prints it.
async fn diff(
    source: &GiteaSource,
    owner: &str,
    repo: &str,
    base: &str,
    branch: &str,
) -> Result<ViewSection, SourceError> {
    let text = source
        .client
        .compare_diff(owner, repo, base, branch)
        .await?;
    let (files, truncated) = knobas_source::diff::parse_view(&text);
    Ok(ViewSection {
        id: DIFF.to_owned(),
        title: "Diff".to_owned(),
        count: Some(files.len() as u64),
        // Its own read is the read the deferral waited for (rule 3).
        deferred: false,
        blocks: vec![Block::Diff { files, truncated }],
    })
}
