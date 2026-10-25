//! The read of one item by id, a **refresh** (ADR-0031, #2098): what follows
//! a landed write before its run, and what an item's detail offers as
//! *Refresh*.
//!
//! **A pull request and an issue, and no other kind.** Each is read by
//! number (`GET /pulls/{n}`, `GET /issues/{n}`) plus its discussion, through
//! the reads and the mappers a run uses, so the item is the one a run would
//! emit for it: the payload a run stores (`map::pr_item`,
//! `map::ticket_item`), the discussion in `body_text`. Those are what the writes this adapter declares change, a
//! merge, a comment, a review, an edit, a transition, and what they make, a
//! ticket and a pull request.
//!
//! Repositories, branches and commits answer
//! [`NotOffered`](RefreshAnswer::NotOffered), and the app runs the source for
//! them instead (ruled in #2098's PR): a repository's item carries its teams
//! and a branch's its repository's record, so either is a second and third
//! read the run already makes; a commit never changes once made, and the run
//! after a file commit reads it with the branch it moved.
//!
//! **Gone is Gitea's 404 on the item's own read**, as the run's probes read
//! it (#724): an issue deleted, a pull request in a repository deleted. A
//! pull request whose discussion read then confirms it deleted is gone too.
//! Every other failure is an error with the run's fault classification, never
//! gone: the engine tombstones what is gone.

use knobas_source::{RefreshAnswer, SourceError};
use serde_json::Value;

use crate::keys::GiteaKey;
use crate::map::{self, RepoRef};
use crate::model;
use crate::sync::{Thread, discussion_of};

/// [`knobas_source::Source::refresh`] for Gitea.
///
/// # Errors
///
/// As [`crate::GiteaSource::key_of`] for an id this source did not issue,
/// and the read's own failure, a 404 excepted.
pub(crate) async fn refresh(
    source: &crate::GiteaSource,
    entity: &str,
) -> Result<RefreshAnswer, SourceError> {
    match source.key_of(entity)? {
        GiteaKey::Pr {
            owner,
            repo,
            number,
        } => pull(source, &owner, &repo, number, entity).await,
        GiteaKey::Issue {
            owner,
            repo,
            number,
        } => ticket(source, &owner, &repo, number, entity).await,
        GiteaKey::Repo { .. } | GiteaKey::Branch { .. } | GiteaKey::Commit { .. } => {
            Ok(RefreshAnswer::NotOffered)
        }
    }
}

/// One pull request, `GET /pulls/{n}` and its discussion.
async fn pull(
    source: &crate::GiteaSource,
    owner: &str,
    repo: &str,
    number: u64,
    entity: &str,
) -> Result<RefreshAnswer, SourceError> {
    let Some(raw) = unless_gone(source.client.pull(owner, repo, number).await)? else {
        return Ok(RefreshAnswer::Gone);
    };
    let pr: model::PullRequest = parse(&raw, entity)?;
    let full_name = format!("{owner}/{repo}");
    let at = repo_ref(owner, repo, &full_name);
    let Some(comments) = discussion_of(source, at, number, pr.comments, Thread::Pull).await? else {
        return Ok(RefreshAnswer::Gone);
    };
    Ok(RefreshAnswer::Item(map::pr_item(
        &source.id, &raw, at, &pr, &comments,
    )))
}

/// One issue, `GET /issues/{n}` and its discussion.
async fn ticket(
    source: &crate::GiteaSource,
    owner: &str,
    repo: &str,
    number: u64,
    entity: &str,
) -> Result<RefreshAnswer, SourceError> {
    let Some(raw) = unless_gone(source.client.issue(owner, repo, number).await)? else {
        return Ok(RefreshAnswer::Gone);
    };
    let issue: model::Issue = parse(&raw, entity)?;
    // Gitea answers a pull request's number on the issue endpoint too. The
    // run never emits it as a ticket, so no ticket of that id exists.
    if issue.pull_request.is_some() {
        return Ok(RefreshAnswer::Gone);
    }
    let full_name = format!("{owner}/{repo}");
    let at = repo_ref(owner, repo, &full_name);
    let Some(comments) = discussion_of(source, at, number, issue.comments, Thread::Issue).await?
    else {
        return Ok(RefreshAnswer::Gone);
    };
    Ok(RefreshAnswer::Item(map::ticket_item(
        &source.id, &raw, at, &issue, &comments,
    )))
}

/// The item's own read, `None` for Gitea's 404.
fn unless_gone(read: Result<Value, SourceError>) -> Result<Option<Value>, SourceError> {
    match read {
        Ok(raw) => Ok(Some(raw)),
        Err(error) if error.status() == Some(404) => Ok(None),
        Err(error) => Err(error),
    }
}

/// The repository a pull request or an issue is in, as its mapper reads it:
/// the owner and the name alone. The browser URL and the default branch are
/// a branch's and a repository's.
fn repo_ref<'a>(owner: &'a str, name: &'a str, full_name: &'a str) -> RepoRef<'a> {
    RepoRef {
        owner,
        name,
        full_name,
        html_url: None,
        default_branch: None,
    }
}

/// The record read as the run reads its listing's: one it cannot read is a
/// protocol fault, where a run skips it with a warning, since a refresh has
/// no other item to go on to.
fn parse<T: serde::de::DeserializeOwned>(raw: &Value, entity: &str) -> Result<T, SourceError> {
    serde_json::from_value(raw.clone()).map_err(|error| {
        SourceError::protocol(format!(
            "gitea: {entity:?} answered a record this adapter cannot read: {error}"
        ))
    })
}
