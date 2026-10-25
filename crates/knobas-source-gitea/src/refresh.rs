//! The read of one item by id, a **refresh** (ADR-0031, #2098): what follows
//! a landed write before its run, and what an item's detail offers as
//! *Refresh*.
//!
//! **A pull request and an issue, and no other kind.** Each is read by
//! number (`GET /pulls/{n}`, `GET /issues/{n}`) plus its discussion, through
//! the reads and the mappers a run uses, so the item is the one a run would
//! emit for it: the record verbatim as the payload, the discussion in
//! `body_text`. Those are what the writes this adapter declares change, a
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
    let (owner, repo, number, of) = match source.key_of(entity)? {
        GiteaKey::Pr {
            owner,
            repo,
            number,
        } => (owner, repo, number, Thread::Pull),
        GiteaKey::Issue {
            owner,
            repo,
            number,
        } => (owner, repo, number, Thread::Issue),
        GiteaKey::Repo { .. } | GiteaKey::Branch { .. } | GiteaKey::Commit { .. } => {
            return Ok(RefreshAnswer::NotOffered);
        }
    };
    let read = match of {
        Thread::Pull => source.client.pull(&owner, &repo, number).await,
        Thread::Issue => source.client.issue(&owner, &repo, number).await,
    };
    let mut raw = match read {
        Ok(raw) => raw,
        Err(error) if error.status() == Some(404) => return Ok(RefreshAnswer::Gone),
        Err(error) => return Err(error),
    };
    let full_name = format!("{owner}/{repo}");
    // Only the owner and the name are read for a pull request or a ticket:
    // the browser URL and the default branch are a branch's and a
    // repository's.
    let at = RepoRef {
        owner: &owner,
        name: &repo,
        full_name: &full_name,
        html_url: None,
        default_branch: None,
    };
    let item = match of {
        Thread::Pull => {
            as_listed(&mut raw);
            let pr: model::PullRequest = parse(&raw, entity)?;
            let Some(comments) = discussion_of(source, at, number, pr.comments, of).await? else {
                return Ok(RefreshAnswer::Gone);
            };
            map::pr_item(&source.id, &raw, at, &pr, &comments)
        }
        Thread::Issue => {
            let issue: model::Issue = parse(&raw, entity)?;
            // Gitea answers a pull request's number on the issue endpoint
            // too. The run never emits it as a ticket, so no ticket of
            // that id exists.
            if issue.pull_request.is_some() {
                return Ok(RefreshAnswer::Gone);
            }
            let Some(comments) = discussion_of(source, at, number, issue.comments, of).await?
            else {
                return Ok(RefreshAnswer::Gone);
            };
            map::ticket_item(&source.id, &raw, at, &issue, &comments)
        }
    };
    Ok(RefreshAnswer::Item(item))
}

/// The record read as the run reads its listing's: one it cannot read is a
/// protocol fault, where a run skips it with a warning, since a refresh has
/// no other item to go on to.
fn parse<T: serde::de::DeserializeOwned>(raw: &Value, entity: &str) -> Result<T, SourceError> {
    serde_json::from_value(raw.clone()).map_err(|error| {
        SourceError::protocol(format!("gitea: {entity:?} answered a record this adapter cannot read: {error}"))
    })
}

/// A pull request read whole, in the shape the `/pulls` listing serves it,
/// which is the shape a run stores. They differ in one field the descriptor
/// declares (Gitea 1.27.3, measured by the live battery's clause 13, #2098):
/// for a pull request nobody was asked to review, the listing answers
/// `requested_reviewers: null` and the read of the one pull request `[]`.
/// Both say "nobody", and the listing never answers `[]`, so `[]` is written
/// as `null`.
fn as_listed(raw: &mut Value) {
    if let Some(reviewers) = raw.get_mut("requested_reviewers")
        && reviewers.as_array().is_some_and(Vec::is_empty)
    {
        *reviewers = Value::Null;
    }
}
