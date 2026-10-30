//! A branch's view (#663, spec #660 story 11): how far it is from its
//! repository's default branch, what it carries, and whether a pull request
//! is open from it.
//!
//! * *Overview* -- the open pull request whose head is this branch first
//!   (the #707 ruling: where a reader looks for it, and the link to it), then
//!   facts (the base, ahead and behind, or *Pull request: none*) and the last
//!   commit.
//! * *Commits* (count = listed) -- what the branch carries over the default
//!   branch, from `compare/{default}...{branch}`, at most
//!   [`rows::COMMIT_CAP`] of them and then a note saying where the rest is.
//! * *Diff* (deferred, count = files) -- the same compare as a diff
//!   (`?output=diff`).
//! * *Files* (deferred) -- the tree at this branch ([`files`], #711).
//!
//! *Behind* is Gitea's count of `/commits?sha={default}&not={branch}`, not
//! the reverse compare, which would list every commit it counts.
//!
//! The default branch itself is measured against nothing: it says *default
//! branch*, its *Commits* are its latest twenty (counted by every commit it
//! has, #2311), and it has no *Diff*.
//!
//! The whole view's toolbar (#707, [`super::act`]): *Open pull request…*
//! where no pull request is open from the branch (disabled with nothing
//! ahead; absent on the default branch), *New branch…* from this branch, and
//! *Delete branch…*, absent on the default branch (it can never be deleted)
//! and disabled, saying how, on a protected one.

use std::collections::BTreeSet;

use knobas_source::SourceError;
use knobas_source::view::{Block, Fact, SourceView, Tone, ViewAction, ViewSection};

use super::rows::{self, BranchRecord, ListedCommit, RepoRecord};
use super::{act, fact, files};
use crate::map;
use crate::source::GiteaSource;

const OVERVIEW: &str = "overview";
const COMMITS: &str = "commits";
const DIFF: &str = "diff";

/// What `compare/{base}...{head}` answers.
#[derive(Debug, serde::Deserialize)]
struct Compare {
    #[serde(default)]
    total_commits: Option<u64>,
    #[serde(default)]
    commits: Vec<ListedCommit>,
}

impl Compare {
    fn commit_count(&self) -> u64 {
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
    // The tree reads nothing of the repository (#2241).
    if let Some(id) = section.filter(|id| files::owns(id)) {
        return Ok(SourceView::new(vec![
            files::read(source, owner, repo, files::Of::Branch(branch), id).await?,
        ]));
    }
    let record: RepoRecord = source.client.repo_as(owner, repo).await?;
    let base = record.default_branch().to_owned();
    let is_default = branch == base;

    match section {
        Some(DIFF) if !is_default => Ok(SourceView::new(vec![
            diff(source, owner, repo, &base, branch).await?,
        ])),
        Some(OVERVIEW | COMMITS) | None => {
            whole(source, &record, owner, repo, branch, section).await
        }
        Some(other) => Err(SourceError::protocol(format!(
            "gitea: a branch's view has no section {other:?} (asked for {entity:?})"
        ))),
    }
}

/// The whole view with its toolbar, or one of its two eager sections.
async fn whole(
    source: &GiteaSource,
    record: &RepoRecord,
    owner: &str,
    repo: &str,
    branch: &str,
    only: Option<&str>,
) -> Result<SourceView, SourceError> {
    let id = &source.id;
    let is_default = branch == record.default_branch();
    // The tip, the open pull requests, what the branch carries and -- for a
    // pull request's bases, on the whole view of a branch that may propose
    // one -- the repository's branches are each their own reads, so they are
    // read side by side (#703); the first to fail fails the view, as it did
    // in turn.
    let (tip, open, carried, listed, checks) = tokio::try_join!(
        source.client.branch_as::<BranchRecord>(owner, repo, branch),
        async {
            // A mirror's `/pulls` is a bare 404: it has none, which is not a failure.
            if record.repo.can_have_pulls() {
                rows::open_pulls(source, owner, repo).await
            } else {
                Ok(rows::Paged::default())
            }
        },
        read_carried(source, record, owner, repo, branch),
        async {
            if only.is_none() && !is_default {
                rows::all_branches(source, owner, repo).await
            } else {
                Ok(rows::Paged::default())
            }
        },
        // The tip's own statuses (#710), by the branch's name so this need
        // not wait for the read that learns the tip. Only *Overview* draws
        // them, and a failed read is drawn as a line rather than failing the
        // view (`checks`).
        async {
            Ok(if only == Some(COMMITS) {
                None
            } else {
                super::checks::block(source, owner, repo, branch).await
            })
        },
    )?;
    let pulls: Vec<_> = open
        .items
        .iter()
        .filter(|p| p.is_from(owner, repo, branch))
        .collect();
    let Carried {
        mut facts,
        commits,
        count,
        ahead,
        files,
        note,
    } = carried;
    if pulls.is_empty() {
        facts.push(Fact {
            label: "Pull request".to_owned(),
            // Where the walk stopped on its budget, its none is not a none.
            value: if open.cut {
                format!("none in the first {} open", open.items.len())
            } else {
                "none".to_owned()
            },
            tone: Some(Tone::Muted),
        });
    }

    let mut blocks = Vec::new();
    // The open pull request first: the overview is where a reader looks for
    // it, and its row is the way there (the #707 ruling).
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
    blocks.push(Block::Facts { facts });
    blocks.extend(checks);
    if let Some(commit) = &tip.commit {
        blocks.push(Block::Entities {
            title: Some("Last commit".to_owned()),
            rows: vec![rows::commit_row(id, owner, repo, commit)],
        });
    }
    let actions = if only.is_none() {
        let subjects: Vec<String> = commits
            .iter()
            .map(|c| {
                crate::map::subject(
                    c.record
                        .commit
                        .as_ref()
                        .and_then(|i| i.message.as_deref())
                        .unwrap_or_default(),
                )
            })
            .collect();
        toolbar(
            source,
            record,
            owner,
            repo,
            branch,
            &Standing {
                has_pull: !pulls.is_empty(),
                protected: tip.protected,
                ahead,
                subjects: &subjects,
                branches: &listed.items,
            },
        )
    } else {
        Vec::new()
    };

    let mut commit_rows: Vec<_> = commits
        .iter()
        .map(|c| rows::commit_row(id, owner, repo, &c.record))
        .collect();
    commit_rows.extend(note);
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
            count,
            deferred: false,
            blocks: vec![Block::Entities {
                title: None,
                rows: commit_rows,
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
    sections.push(files::tab());
    if let Some(only) = only {
        sections.retain(|s| s.id == only);
    }
    Ok(SourceView { sections, actions })
}

/// What the toolbar is decided on: what the view read of the branch.
struct Standing<'a> {
    /// A pull request is open from it.
    has_pull: bool,
    protected: bool,
    /// Commits it carries over the default branch; `None` for that branch
    /// itself, which offers neither a pull request nor a delete.
    ahead: Option<u64>,
    /// The subjects of those it lists, as listed.
    subjects: &'a [String],
    /// The repository's branches, for a pull request's bases.
    branches: &'a [rows::ListedBranch],
}

/// The branch's toolbar (#707): *Open pull request…*, *New branch…*,
/// *Delete branch…*, in that order.
fn toolbar(
    source: &GiteaSource,
    record: &RepoRecord,
    owner: &str,
    repo: &str,
    branch: &str,
    standing: &Standing<'_>,
) -> Vec<ViewAction> {
    let default = record.default_branch();
    let repository = act::repo_id(source, owner, repo);
    let mirrored = record.repo.mirror.then(|| act::MIRRORED.to_owned());
    let mut actions = Vec::new();
    // A branch with an open pull request links to it from the overview and
    // offers no second one; the default branch has nothing to propose, and
    // neither has a repository that takes no pull requests (a mirror, or one
    // with them switched off), as on the repository's own toolbar.
    if let Some(ahead) = standing.ahead
        && !standing.has_pull
        && record.repo.can_have_pulls()
    {
        let disabled = (ahead == 0).then(|| format!("No commits ahead of {default}"));
        actions.push(ViewAction {
            primary: true,
            ..act::pull_request(
                "Open pull request…",
                Some(repository.clone()),
                act::proposal(branch, standing.subjects, ahead),
                vec![act::branch_option(branch)],
                act::default_first(
                    default,
                    standing.branches.iter().map(|b| b.name.as_str()),
                    Some(branch),
                ),
                disabled,
            )
        });
    }
    actions.push(act::new_branch(
        Some(repository),
        vec![act::branch_option(branch)],
        mirrored.clone(),
    ));
    // The default branch can never be deleted, so it offers no delete at all
    // (the #701 ruling: disabled-with-reason only where the action can still
    // become possible); a protection can be lifted, so a protected branch
    // keeps it, disabled, saying how.
    if let Some(ahead) = standing.ahead {
        let disabled = mirrored.or_else(|| {
            standing
                .protected
                .then(|| "Protected branch: lift its protection on Gitea to delete it".to_owned())
        });
        actions.push(act::delete_branch(branch, default, ahead, disabled));
    }
    actions
}

/// What a branch carries: its facts about the base, the commits its tab
/// lists, the tab's count, the files the diff touches and the note ending the
/// list.
struct Carried {
    facts: Vec<Fact>,
    commits: Vec<ListedCommit>,
    count: Option<u64>,
    /// How many commits it carries over the default branch; `None` for that
    /// branch itself.
    ahead: Option<u64>,
    files: Option<u64>,
    note: Option<knobas_source::view::ViewRow>,
}

/// The default branch's latest commits, or what another branch carries over
/// it, ahead and behind read side by side (#703).
async fn read_carried(
    source: &GiteaSource,
    record: &RepoRecord,
    owner: &str,
    repo: &str,
    branch: &str,
) -> Result<Carried, SourceError> {
    let base = record.default_branch();
    if branch == base {
        // The latest twenty, counted by every commit the branch has (#2311).
        let (latest, count): (Vec<ListedCommit>, _) = source
            .client
            .latest_commits_counted(owner, repo, branch, rows::LATEST_COMMITS)
            .await?;
        return Ok(Carried {
            facts: vec![fact("Branch", "default branch".to_owned())],
            commits: latest,
            count,
            ahead: None,
            files: None,
            note: None,
        });
    }
    let (ahead, behind) = tokio::try_join!(
        source
            .client
            .compare_as::<Compare>(owner, repo, base, branch),
        async {
            // Behind is a count, so it is read as one; the reverse compare
            // would list every commit the default branch gained since.
            match source
                .client
                .count_commits_not_in(owner, repo, base, branch)
                .await?
            {
                Some(count) => Ok(count),
                None => {
                    let behind: Compare =
                        source.client.compare_as(owner, repo, branch, base).await?;
                    Ok(behind.commit_count())
                }
            }
        },
    )?;
    let facts = vec![
        fact("Base", base.to_owned()),
        fact("Ahead", ahead.commit_count().to_string()),
        fact("Behind", behind.to_string()),
    ];
    let files = ahead.files();
    let ahead_by = ahead.commit_count();
    let total = usize::try_from(ahead_by).unwrap_or(usize::MAX);
    let mut commits = ahead.commits;
    let read = commits.len();
    commits.truncate(rows::COMMIT_CAP);
    let listed = commits.len();
    let note = rows::left_out(
        "commits",
        listed,
        Some(total.max(read)),
        false,
        record.web(&format!(
            "/compare/{}...{}",
            map::url_path(base),
            map::url_path(branch)
        )),
    );
    Ok(Carried {
        facts,
        commits,
        count: Some(listed as u64),
        ahead: Some(ahead_by),
        files: Some(files),
        note,
    })
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
        blocks: vec![Block::Diff {
            files,
            truncated,
            commentable: None,
            gone: Vec::new(),
            range: None,
        }],
    })
}
