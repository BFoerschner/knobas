//! A repository's view (#663, spec #660 story 12): a place to navigate from.
//!
//! * *Overview* -- facts (default branch, description, visibility, stars and
//!   forks where there are any, size) and the README as a text block; no
//!   README, no block.
//! * *Pull requests* (count = open) -- the open ones, with the rooms' chip.
//! * *Branches* (count) -- the default branch first, then by last commit,
//!   newest first.
//! * *Commits* -- the default branch's latest twenty.
//! * *Files* (deferred) -- the tree at the default branch ([`files`], #711);
//!   none for an empty repository, which has no tree.
//!
//! The whole view's toolbar (#707, [`super::act`]): *New branch…* from the
//! default branch or one picked, and *New pull request…* whose heads are the
//! branches ahead of the default branch that no open pull request is from --
//! asked of the [`AHEAD_CHECKS`] most recently committed candidates, side by
//! side, after the two lists they are drawn from.
//!
//! Both lists stop after twenty pages; one that did keeps its count as what
//! it read and ends on a muted row naming Gitea's page for the rest (#678).
//!
//! An empty repository (no commits at all) draws its overview and empty
//! lists: Gitea answers its contents 404, its branches `null`, its `/pulls`
//! a bare 404 and its `/commits` 409, and none of those is a failure.

use knobas_source::SourceError;
use knobas_source::view::{Block, SourceView, Tone, ViewAction, ViewRow, ViewSection};

use super::rows::{self, ListedBranch, ListedCommit, RepoRecord};
use super::{act, fact, files};
use crate::keys;
use crate::source::GiteaSource;

const OVERVIEW: &str = "overview";
const PULLS: &str = "pulls";
const BRANCHES: &str = "branches";
const COMMITS: &str = "commits";

/// How many branches a *New pull request…* asks whether they are ahead, the
/// most recently committed first: one request each, on every opening of the
/// view. Ten, so that with the view's own half-dozen reads one opening stays
/// inside the client's burst of twenty (§4.1) and the toolbar, which the
/// whole view waits for, adds one round trip rather than a queue at ten a
/// second.
const AHEAD_CHECKS: usize = 10;
/// The longest README a view carries, in characters. A longer one is cut at
/// a line and says so.
const README_CAP: usize = 100_000;

/// One entry of the top directory's listing.
#[derive(Debug, serde::Deserialize)]
struct Entry {
    name: String,
    #[serde(rename = "type", default)]
    kind: String,
}

pub(super) async fn draw(
    source: &GiteaSource,
    entity: &str,
    owner: &str,
    repo: &str,
    section: Option<&str>,
) -> Result<SourceView, SourceError> {
    let record: RepoRecord = source.client.repo_as(owner, repo).await?;
    let sections = match section {
        // Side by side over the one record (#703): each section is its own
        // reads, and one after another they were most of the wait. The first
        // to fail fails the view, as it did in turn. The toolbar is drawn from
        // the two lists, so it follows them while the rest is read.
        None => {
            let (overview, (pulls, branches, actions), commits) = tokio::try_join!(
                overview(source, &record, owner, repo),
                async {
                    let (open, listing) = tokio::try_join!(
                        read_pulls(source, &record, owner, repo),
                        read_branches(source, &record, owner, repo),
                    )?;
                    let actions = toolbar(source, &record, owner, repo, &open, &listing).await?;
                    Ok((
                        pulls(source, &record, owner, repo, &open),
                        branches(source, &record, owner, repo, &listing),
                        actions,
                    ))
                },
                commits(source, &record, owner, repo),
            )?;
            let mut sections = vec![overview, pulls, branches, commits];
            // No commit, no tree to read.
            if !record.repo.empty {
                sections.push(files::tab());
            }
            return Ok(SourceView { sections, actions });
        }
        Some(OVERVIEW) => vec![overview(source, &record, owner, repo).await?],
        Some(PULLS) => {
            let open = read_pulls(source, &record, owner, repo).await?;
            vec![pulls(source, &record, owner, repo, &open)]
        }
        Some(BRANCHES) => {
            let listing = read_branches(source, &record, owner, repo).await?;
            vec![branches(source, &record, owner, repo, &listing)]
        }
        Some(COMMITS) => vec![commits(source, &record, owner, repo).await?],
        Some(id) if files::owns(id) => {
            vec![
                files::read(
                    source,
                    &record,
                    owner,
                    repo,
                    files::At::Branch(record.default_branch()),
                    id,
                )
                .await?,
            ]
        }
        Some(other) => {
            return Err(SourceError::protocol(format!(
                "gitea: a repository's view has no section {other:?} (asked for {entity:?})"
            )));
        }
    };
    Ok(SourceView::new(sections))
}

async fn overview(
    source: &GiteaSource,
    record: &RepoRecord,
    owner: &str,
    repo: &str,
) -> Result<ViewSection, SourceError> {
    let mut facts = vec![fact("Default branch", record.default_branch().to_owned())];
    if let Some(description) = record
        .repo
        .description
        .as_deref()
        .map(str::trim)
        .filter(|d| !d.is_empty())
    {
        facts.push(fact("Description", description.to_owned()));
    }
    let visibility = if record.private {
        "private"
    } else if record.internal {
        "internal"
    } else {
        "public"
    };
    facts.push(fact("Visibility", visibility.to_owned()));
    if record.stars_count > 0 {
        facts.push(fact("Stars", record.stars_count.to_string()));
    }
    if record.forks_count > 0 {
        facts.push(fact("Forks", record.forks_count.to_string()));
    }
    if record.repo.empty {
        facts.push(fact("Contents", "empty".to_owned()));
    } else if record.size > 0 {
        facts.push(fact("Size", size_of(record.size)));
    }

    let mut blocks = vec![Block::Facts { facts }];
    if !record.repo.empty {
        blocks.extend(readme(source, owner, repo, record.default_branch()).await?);
    }
    Ok(ViewSection {
        id: OVERVIEW.to_owned(),
        title: "Overview".to_owned(),
        count: None,
        deferred: false,
        blocks,
    })
}

/// The README's text block, and a line saying it was cut where it was; none
/// for a repository whose top directory has no README.
async fn readme(
    source: &GiteaSource,
    owner: &str,
    repo: &str,
    at: &str,
) -> Result<Vec<Block>, SourceError> {
    let listing: Vec<Entry> = match source.client.contents_at_as(owner, repo, "", at).await {
        Ok(listing) => listing,
        // No tree at the default branch: nothing to read a README from.
        Err(error) if error.status() == Some(404) => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let Some((name, markdown)) = pick_readme(&listing) else {
        return Ok(Vec::new());
    };
    let text = source.client.raw_file(owner, repo, name).await?;
    let text = text.trim_end();
    if text.trim().is_empty() {
        return Ok(Vec::new());
    }
    let mut blocks = Vec::new();
    match cut(text, README_CAP) {
        Some(kept) => {
            blocks.push(Block::Text {
                text: kept.to_owned(),
                markdown,
            });
            blocks.push(Block::Text {
                text: format!(
                    "The README is longer than {README_CAP} characters; the rest is on Gitea."
                ),
                markdown: false,
            });
        }
        None => blocks.push(Block::Text {
            text: text.to_owned(),
            markdown,
        }),
    }
    Ok(blocks)
}

/// The README among a directory's files -- `README`, `README.md`,
/// `readme.txt`, any case -- a Markdown one first, as Gitea's own page
/// prefers it. With whether it is Markdown.
fn pick_readme(listing: &[Entry]) -> Option<(&str, bool)> {
    let readmes = listing.iter().filter(|e| e.kind == "file").filter_map(|e| {
        let lower = e.name.to_ascii_lowercase();
        let ext = match lower.strip_prefix("readme")? {
            "" => "",
            rest => rest.strip_prefix('.')?,
        };
        let markdown = matches!(ext, "md" | "markdown" | "mdown" | "mkd");
        Some((e.name.as_str(), markdown))
    });
    let mut best: Option<(&str, bool)> = None;
    for candidate in readmes {
        if best.is_none_or(|(_, markdown)| !markdown && candidate.1) {
            best = Some(candidate);
        }
    }
    best
}

/// `text` cut at the last line break within `cap` characters, or `None`
/// where it fits.
fn cut(text: &str, cap: usize) -> Option<&str> {
    let (at, _) = text.char_indices().nth(cap)?;
    let head = &text[..at];
    Some(
        head.rfind('\n')
            .map_or(head, |line| &head[..line])
            .trim_end(),
    )
}

/// Gitea's size, in KiB, as a reader says it.
fn size_of(kib: u64) -> String {
    const UNIT: f64 = 1024.0;
    #[allow(clippy::cast_precision_loss)]
    let kib_f = kib as f64;
    if kib_f < UNIT {
        format!("{kib} KiB")
    } else if kib_f < UNIT * UNIT {
        format!("{:.1} MiB", kib_f / UNIT)
    } else {
        format!("{:.1} GiB", kib_f / UNIT / UNIT)
    }
}

/// The open pull requests; none for a repository that cannot have any.
async fn read_pulls(
    source: &GiteaSource,
    record: &RepoRecord,
    owner: &str,
    repo: &str,
) -> Result<rows::Paged<rows::OpenPull>, SourceError> {
    if record.repo.can_have_pulls() {
        rows::open_pulls(source, owner, repo).await
    } else {
        Ok(rows::Paged::default())
    }
}

fn pulls(
    source: &GiteaSource,
    record: &RepoRecord,
    owner: &str,
    repo: &str,
    open: &rows::Paged<rows::OpenPull>,
) -> ViewSection {
    let rows: Vec<ViewRow> = open
        .items
        .iter()
        .map(|p| rows::pull_row(&source.id, owner, repo, p))
        .collect();
    let note = rows::left_out(
        "open pull requests",
        rows.len(),
        None,
        open.cut,
        record.web("/pulls"),
    );
    listed(PULLS, "Pull requests", rows, note)
}

/// When a branch's tip was committed, where Gitea says.
fn tip_time(branch: &ListedBranch) -> Option<chrono::DateTime<chrono::FixedOffset>> {
    branch
        .commit
        .as_ref()
        .and_then(|c| c.timestamp.as_deref())
        .and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok())
}

/// The branches, the default branch first, then by last commit, newest
/// first.
async fn read_branches(
    source: &GiteaSource,
    record: &RepoRecord,
    owner: &str,
    repo: &str,
) -> Result<rows::Paged<ListedBranch>, SourceError> {
    let mut listing = rows::all_branches(source, owner, repo).await?;
    let default = record.default_branch();
    listing.items.sort_by(|a, b| {
        (b.name == default)
            .cmp(&(a.name == default))
            .then_with(|| tip_time(b).cmp(&tip_time(a)))
            .then_with(|| a.name.cmp(&b.name))
    });
    Ok(listing)
}

fn branches(
    source: &GiteaSource,
    record: &RepoRecord,
    owner: &str,
    repo: &str,
    listing: &rows::Paged<ListedBranch>,
) -> ViewSection {
    let default = record.default_branch();
    let now = chrono::Utc::now();
    let rows: Vec<ViewRow> = listing
        .items
        .iter()
        .map(|b| {
            let tip = b.commit.as_ref();
            // How long ago its tip was committed, as a commit row says it.
            // Go's zero time is no time, as on a commit row (`map::real_time`).
            let age = crate::map::real_time(tip_time(b).map(|t| t.with_timezone(&chrono::Utc)))
                .map(|t| super::ago(t, now));
            let short = tip.map(|c| super::short(&c.id));
            let is_default = b.name == default;
            ViewRow {
                entity_id: Some(format!(
                    "{}:{}",
                    source.id,
                    keys::branch_key(owner, repo, &b.name)
                )),
                title: b.name.clone(),
                meta: rows::joined([short.as_deref(), age.as_deref()]),
                tone: is_default.then_some(Tone::Muted),
                chip: is_default.then(|| "default".to_owned()),
                url: None,
                commit: None,
            }
        })
        .collect();
    let note = rows::left_out(
        "branches",
        rows.len(),
        None,
        listing.cut,
        record.web("/branches"),
    );
    listed(BRANCHES, "Branches", rows, note)
}

/// The repository's toolbar (#707): *New branch…*, then *New pull
/// request…* where the repository can have pull requests.
///
/// A head is a branch other than the default one, whose tip is not the
/// default's, from which no pull request is open, and which has a commit the
/// default branch has not -- asked of the [`AHEAD_CHECKS`] newest such
/// candidates, side by side.
async fn toolbar(
    source: &GiteaSource,
    record: &RepoRecord,
    owner: &str,
    repo: &str,
    open: &rows::Paged<rows::OpenPull>,
    listing: &rows::Paged<ListedBranch>,
) -> Result<Vec<ViewAction>, SourceError> {
    let default = record.default_branch();
    let names = || listing.items.iter().map(|b| b.name.as_str());
    let empty = record
        .repo
        .empty
        .then(|| "The repository is empty".to_owned());
    let mut actions = vec![act::new_branch(
        None,
        act::default_first(default, names(), None),
        empty
            .clone()
            .or_else(|| record.repo.mirror.then(|| act::MIRRORED.to_owned())),
    )];
    if !record.repo.can_have_pulls() {
        return Ok(actions);
    }
    let tip = |name: &str| {
        listing
            .items
            .iter()
            .find(|b| b.name == name)
            .and_then(|b| b.commit.as_ref())
            .map(|c| c.id.as_str())
    };
    let default_tip = tip(default);
    let mut candidates: Vec<&str> = names()
        .filter(|name| *name != default)
        .filter(|name| tip(name).is_none() || tip(name) != default_tip)
        .filter(|name| !open.items.iter().any(|p| p.is_from(owner, repo, name)))
        .collect();
    let cut = candidates.len() > AHEAD_CHECKS;
    candidates.truncate(AHEAD_CHECKS);
    // A branch deleted since the listing answers 404: it is not offered, and
    // the view still opens.
    let checks = candidates
        .iter()
        .map(|name| async move {
            match source
                .client
                .has_commits_not_in(owner, repo, name, default)
                .await
            {
                Err(error) if error.status() == Some(404) => Ok(false),
                answer => answer,
            }
        })
        .collect();
    let answers = rows::all_of(checks).await?;
    let heads: Vec<_> = candidates
        .iter()
        .zip(answers)
        .filter(|(_, is_ahead)| *is_ahead)
        .map(|(name, _)| act::branch_option(name))
        .collect();
    let disabled = empty.or_else(|| {
        heads
            .is_empty()
            .then(|| format!("No branch is ahead of {default} without a pull request"))
    });
    let mut new_pull = act::pull_request(
        "New pull request…",
        None,
        act::Proposal::default(),
        heads,
        act::default_first(default, names(), None),
        disabled,
    );
    // Where the asking stopped short, the field says among what it chose.
    if cut && let Some(head) = new_pull.fields.iter_mut().find(|f| f.name == "head") {
        head.label = format!("Head, among the {AHEAD_CHECKS} most recent branches");
    }
    actions.push(new_pull);
    Ok(actions)
}

async fn commits(
    source: &GiteaSource,
    record: &RepoRecord,
    owner: &str,
    repo: &str,
) -> Result<ViewSection, SourceError> {
    let latest: Vec<ListedCommit> = if record.repo.empty {
        Vec::new()
    } else {
        source
            .client
            .latest_commits_as(owner, repo, record.default_branch(), rows::LATEST_COMMITS)
            .await?
    };
    let rows = latest
        .iter()
        .map(|c| rows::commit_row(&source.id, owner, repo, &c.record))
        .collect();
    Ok(ViewSection {
        id: COMMITS.to_owned(),
        title: "Commits".to_owned(),
        // The latest twenty by design, not a list that stopped paging.
        count: None,
        deferred: false,
        blocks: vec![Block::Entities { title: None, rows }],
    })
}

/// A list counted by what it holds, with the note saying where the rest is
/// after it where the read stopped on its budget (#678).
fn listed(id: &str, title: &str, mut rows: Vec<ViewRow>, note: Option<ViewRow>) -> ViewSection {
    let count = rows.len() as u64;
    rows.extend(note);
    ViewSection {
        id: id.to_owned(),
        title: title.to_owned(),
        count: Some(count),
        deferred: false,
        blocks: vec![Block::Entities { title: None, rows }],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_long_text_is_cut_at_a_line_within_the_cap() {
        assert_eq!(cut("short", 10), None);
        assert_eq!(cut("one\ntwo\nthree", 9), Some("one\ntwo"));
        assert_eq!(cut("äöü", 2), Some("äö"), "characters, not bytes");
    }

    #[test]
    fn sizes_read_in_the_unit_that_fits() {
        assert_eq!(size_of(123), "123 KiB");
        assert_eq!(size_of(2560), "2.5 MiB");
        assert_eq!(size_of(3 * 1024 * 1024), "3.0 GiB");
    }
}
