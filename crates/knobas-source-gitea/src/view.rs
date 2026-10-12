//! The source views this adapter draws (ADR-0020, issue #661).
//!
//! A **commit** opens on its overview (who, when, its parents, its whole
//! message) and has its diff as a second tab. A **pull request** is
//! [`pr`]'s. The other kinds answer `None` until their own tickets (#660): the
//! detail then draws the mirror's generic projection, as it always has.
//!
//! Every read here is live and nothing is stored (rule 1). A refusal is the
//! error it maps to, so the detail says *Showing the mirror* and why.

use knobas_source::SourceError;
use knobas_source::view::{Block, Fact, SourceView, ViewRow, ViewSection};

use crate::keys::{self, GiteaKey};
use crate::model::{CommitDetail, Signature, UserRef};
use crate::source::GiteaSource;

mod pr;

/// The commit view's first tab.
const OVERVIEW: &str = "overview";
/// The commit view's diff tab.
const DIFF: &str = "diff";

/// How many characters of an object id a reader is shown where the whole one
/// would be noise.
const SHORT_SHA: usize = 10;

/// The view of `entity`, or `None` for a kind this adapter draws nothing for.
///
/// # Errors
///
/// [`SourceError::Protocol`] for an id that is not this source's, or a
/// section the kind's view does not have; otherwise whatever Gitea's answer
/// maps to.
pub(crate) async fn draw(
    source: &GiteaSource,
    entity: &str,
    section: Option<&str>,
) -> Result<Option<SourceView>, SourceError> {
    match source.key_of(entity)? {
        GiteaKey::Commit { owner, repo, sha } => {
            commit(source, entity, &owner, &repo, &sha, section)
                .await
                .map(Some)
        }
        GiteaKey::Pr {
            owner,
            repo,
            number,
        } => pr::draw(source, entity, &owner, &repo, number, section)
            .await
            .map(Some),
        GiteaKey::Repo { .. } | GiteaKey::Branch { .. } => Ok(None),
    }
}

/// A commit: *Overview* and *Diff*, neither deferred -- reading a commit's
/// change is why it is opened (spec #660, story 1).
async fn commit(
    source: &GiteaSource,
    entity: &str,
    owner: &str,
    repo: &str,
    sha: &str,
    section: Option<&str>,
) -> Result<SourceView, SourceError> {
    let sections = match section {
        None => vec![
            commit_overview(source, owner, repo, sha).await?,
            commit_diff(source, owner, repo, sha).await?,
        ],
        Some(OVERVIEW) => vec![commit_overview(source, owner, repo, sha).await?],
        Some(DIFF) => vec![commit_diff(source, owner, repo, sha).await?],
        Some(other) => {
            return Err(SourceError::protocol(format!(
                "gitea: a commit's view has no section {other:?} (asked for {entity:?})"
            )));
        }
    };
    Ok(SourceView { sections })
}

async fn commit_overview(
    source: &GiteaSource,
    owner: &str,
    repo: &str,
    sha: &str,
) -> Result<ViewSection, SourceError> {
    let record = source.client.commit(owner, repo, sha).await?;
    Ok(ViewSection {
        id: OVERVIEW.to_owned(),
        title: "Overview".to_owned(),
        count: None,
        deferred: false,
        blocks: overview_blocks(&source.id, owner, repo, &record),
    })
}

async fn commit_diff(
    source: &GiteaSource,
    owner: &str,
    repo: &str,
    sha: &str,
) -> Result<ViewSection, SourceError> {
    let text = source.client.commit_diff(owner, repo, sha).await?;
    let (files, truncated) = knobas_source::diff::parse_view(&text);
    Ok(ViewSection {
        id: DIFF.to_owned(),
        title: "Diff".to_owned(),
        count: Some(files.len() as u64),
        deferred: false,
        blocks: vec![Block::Diff { files, truncated }],
    })
}

/// The overview's blocks: facts, then the parents, then the message.
///
/// The committer is named only where it is not the author -- a rebase, a
/// merge through the web UI -- since repeating the author's name under a
/// second label is a line that says nothing. Its time is always shown: the
/// commit time is the one a branch's history is ordered by.
fn overview_blocks(source_id: &str, owner: &str, repo: &str, record: &CommitDetail) -> Vec<Block> {
    let inner = record.commit.as_ref();
    let author = inner.and_then(|c| c.author.as_ref());
    let committer = inner.and_then(|c| c.committer.as_ref());

    let mut facts = vec![
        fact("Commit", short(&record.sha)),
        fact("SHA", record.sha.clone()),
    ];
    let author_name = author
        .and_then(person)
        .or_else(|| login(record.author.as_ref()));
    if let Some(name) = author_name {
        facts.push(fact("Author", name));
    }
    if let Some(when) = author.and_then(|a| a.date.as_deref()) {
        facts.push(fact("Authored", when_of(when)));
    }
    if let Some(committer) = committer {
        let same_person =
            author.is_some_and(|a| a.name == committer.name && a.email == committer.email);
        if !same_person && let Some(name) = person(committer) {
            facts.push(fact("Committer", name));
        }
        if let Some(when) = committer.date.as_deref() {
            facts.push(fact("Committed", when_of(when)));
        }
    }

    let mut blocks = vec![Block::Facts { facts }];
    if !record.parents.is_empty() {
        blocks.push(Block::Entities {
            title: Some(if record.parents.len() == 1 {
                "Parent".to_owned()
            } else {
                "Parents".to_owned()
            }),
            rows: record
                .parents
                .iter()
                .map(|parent| ViewRow {
                    // The address the parent would have; the app clears it
                    // where the mirror does not hold that commit (rule 7).
                    entity_id: Some(format!(
                        "{source_id}:{}",
                        keys::commit_key(owner, repo, &parent.sha)
                    )),
                    title: short(&parent.sha),
                    meta: None,
                    tone: None,
                    chip: None,
                    url: sibling_url(record, &parent.sha),
                })
                .collect(),
        });
    }
    let message = inner
        .and_then(|c| c.message.as_deref())
        .map(str::trim_end)
        .unwrap_or_default();
    if !message.is_empty() {
        blocks.push(Block::Text {
            text: message.to_owned(),
            markdown: false,
        });
    }
    blocks
}

fn fact(label: &str, value: String) -> Fact {
    Fact {
        label: label.to_owned(),
        value,
        tone: None,
    }
}

fn short(sha: &str) -> String {
    sha.chars().take(SHORT_SHA).collect()
}

/// A Gitea account's login, where it has a non-blank one.
fn login(user: Option<&UserRef>) -> Option<String> {
    user.and_then(|u| u.login.clone())
        .filter(|l| !l.trim().is_empty())
}

/// `Name <email>`, or whichever half git recorded.
fn person(signature: &Signature) -> Option<String> {
    let name = signature
        .name
        .as_deref()
        .map(str::trim)
        .filter(|n| !n.is_empty());
    let email = signature
        .email
        .as_deref()
        .map(str::trim)
        .filter(|e| !e.is_empty());
    match (name, email) {
        (Some(name), Some(email)) => Some(format!("{name} <{email}>")),
        (Some(name), None) => Some(name.to_owned()),
        (None, Some(email)) => Some(format!("<{email}>")),
        (None, None) => None,
    }
}

/// A git time to the minute, in the offset it was recorded in -- an author's
/// own clock is part of what the time says. Verbatim where it does not parse.
fn when_of(raw: &str) -> String {
    chrono::DateTime::parse_from_rfc3339(raw)
        .map(|t| t.format("%Y-%m-%d %H:%M %:z").to_string())
        .unwrap_or_else(|_| raw.to_owned())
}

/// The web page of another commit in the same repository, made from this
/// commit's own: Gitea's is `<base>/<owner>/<repo>/commit/<sha>`.
fn sibling_url(record: &CommitDetail, sha: &str) -> Option<String> {
    let own = record.html_url.as_deref()?;
    let base = own.strip_suffix(record.sha.as_str())?;
    base.ends_with("/commit/").then(|| format!("{base}{sha}"))
}
