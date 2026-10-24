//! The source views this adapter draws (ADR-0020, issue #661).
//!
//! A **commit** opens on its overview (who, when, its parents, its whole
//! message) and has its diff as a second tab; its toolbar offers *New
//! branch…* from it ([`act`], #707). A **pull request** is
//! [`pr`]'s, a **branch** is [`branch`]'s and a **repository** is
//! [`repo`]'s (#663).
//!
//! Every read here is live and nothing is stored (rule 1). A refusal is the
//! error it maps to, so the detail says *Showing the mirror* and why. A few
//! parts of a view are drawn without what they could not read, saying so: a
//! pull request's checks (#710) and labels (#1194) on any failure, and its
//! comments where Gitea refuses the token's scope for them, naming it
//! ([`grants::unread`], #2042).

use chrono::{DateTime, Utc};
use knobas_source::SourceError;
use knobas_source::view::{Block, Fact, SourceView, ViewRow, ViewSection};

use crate::keys::{self, GiteaKey};
use crate::model::{CommitDetail, Signature, UserRef};
use crate::source::GiteaSource;

mod act;
mod branch;
mod checks;
mod files;
mod grants;
mod pr;
mod repo;
mod rows;

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
    let key = source.key_of(entity)?;
    let mut view = match &key {
        GiteaKey::Commit { owner, repo, sha } => {
            commit(source, entity, owner, repo, sha, section).await?
        }
        GiteaKey::Pr {
            owner,
            repo,
            number,
        } => pr::draw(source, entity, owner, repo, *number, section).await?,
        GiteaKey::Branch {
            owner,
            repo,
            branch,
        } => branch::draw(source, entity, owner, repo, branch, section).await?,
        GiteaKey::Repo { owner, repo } => repo::draw(source, entity, owner, repo, section).await?,
        // An issue's detail is the ticket's, drawn from the mirror (#1206).
        GiteaKey::Issue { .. } => return Ok(None),
    };
    // What the view offers is the entity's answer; what the token may make
    // of it is Gitea's, asked once the view knows which writes it offers
    // (#2030).
    let (owner, repo) = key.repository();
    grants::narrow(source, owner, repo, &mut view).await;
    Ok(Some(view))
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
        None => {
            // The whole view's toolbar (#707): a branch from this commit, in
            // its repository -- disabled on a mirror, which Gitea refuses a
            // branch on (#722), as the branch and repository views have it.
            let (record, overview, diff) = tokio::try_join!(
                source.client.repo_as::<rows::RepoRecord>(owner, repo),
                commit_overview(source, owner, repo, sha),
                commit_diff(source, owner, repo, sha),
            )?;
            let new_branch = act::new_branch(
                Some(act::repo_id(source, owner, repo)),
                vec![knobas_source::view::ChoiceOption::new(sha, short(sha))],
                record.repo.mirror.then(|| act::MIRRORED.to_owned()),
            );
            return Ok(SourceView {
                sections: vec![overview, diff],
                actions: vec![new_branch],
            });
        }
        Some(OVERVIEW) => vec![commit_overview(source, owner, repo, sha).await?],
        Some(DIFF) => vec![commit_diff(source, owner, repo, sha).await?],
        Some(other) => {
            return Err(SourceError::protocol(format!(
                "gitea: a commit's view has no section {other:?} (asked for {entity:?})"
            )));
        }
    };
    Ok(SourceView::new(sections))
}

async fn commit_overview(
    source: &GiteaSource,
    owner: &str,
    repo: &str,
    sha: &str,
) -> Result<ViewSection, SourceError> {
    let (record, checks) = tokio::join!(
        source.client.commit(owner, repo, sha),
        checks::block(source, owner, repo, sha),
    );
    let record = record?;
    let mut blocks = overview_blocks(&source.id, owner, repo, &record);
    // Right after the facts (#710): whether it is green is the next thing
    // asked of a commit.
    if let Some(checks) = checks {
        blocks.insert(1, checks);
    }
    Ok(ViewSection {
        id: OVERVIEW.to_owned(),
        title: "Overview".to_owned(),
        count: None,
        deferred: false,
        blocks,
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
        blocks: vec![Block::Diff {
            files,
            truncated,
            commentable: None,
            gone: Vec::new(),
        }],
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
                    commit: None,
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

/// How long ago, as the shell's `ago()` (`app/src/lib/shell/time.ts`) says
/// it, so a commit's age reads the same here as on a room's row: minutes,
/// then hours, then the UTC date. A time in the future is *just now*.
pub(super) fn ago(then: DateTime<Utc>, now: DateTime<Utc>) -> String {
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

/// The web page of another commit in the same repository, made from this
/// commit's own: Gitea's is `<base>/<owner>/<repo>/commit/<sha>`.
fn sibling_url(record: &CommitDetail, sha: &str) -> Option<String> {
    let own = record.html_url.as_deref()?;
    let base = own.strip_suffix(record.sha.as_str())?;
    base.ends_with("/commit/").then(|| format!("{base}{sha}"))
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
