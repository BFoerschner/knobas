//! A repository's view (#663, spec #660 story 12): a place to navigate from.
//!
//! * *Overview* -- facts (default branch, description, visibility, stars and
//!   forks where there are any, size) and the README as a text block; no
//!   README, no block.
//! * *Pull requests* (count = open) -- the open ones, with the rooms' chip.
//! * *Branches* (count) -- the default branch first, then by last commit,
//!   newest first.
//!
//! Both lists stop after twenty pages; one that did keeps its count as what
//! it read and ends on a muted row naming Gitea's page for the rest (#678).
//! * *Commits* -- the default branch's latest twenty.
//!
//! An empty repository (no commits at all) draws its overview and empty
//! lists: Gitea answers its contents 404, its branches `null`, its `/pulls`
//! a bare 404 and its `/commits` 409, and none of those is a failure.

use knobas_source::SourceError;
use knobas_source::view::{Block, SourceView, Tone, ViewRow, ViewSection};

use super::fact;
use super::rows::{self, ListedCommit, RepoRecord, TipCommit};
use crate::keys;
use crate::source::GiteaSource;

const OVERVIEW: &str = "overview";
const PULLS: &str = "pulls";
const BRANCHES: &str = "branches";
const COMMITS: &str = "commits";

/// How many pages of fifty branches a view reads at most.
const BRANCH_PAGES: u32 = 20;
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

/// One branch of the listing.
#[derive(Debug, serde::Deserialize)]
struct ListedBranch {
    name: String,
    #[serde(default)]
    commit: Option<TipCommit>,
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
        None => vec![
            overview(source, &record, owner, repo).await?,
            pulls(source, &record, owner, repo).await?,
            branches(source, &record, owner, repo).await?,
            commits(source, &record, owner, repo).await?,
        ],
        Some(OVERVIEW) => vec![overview(source, &record, owner, repo).await?],
        Some(PULLS) => vec![pulls(source, &record, owner, repo).await?],
        Some(BRANCHES) => vec![branches(source, &record, owner, repo).await?],
        Some(COMMITS) => vec![commits(source, &record, owner, repo).await?],
        Some(other) => {
            return Err(SourceError::protocol(format!(
                "gitea: a repository's view has no section {other:?} (asked for {entity:?})"
            )));
        }
    };
    Ok(SourceView { sections })
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
        blocks.extend(readme(source, owner, repo).await?);
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
async fn readme(source: &GiteaSource, owner: &str, repo: &str) -> Result<Vec<Block>, SourceError> {
    let listing: Vec<Entry> = match source.client.root_contents_as(owner, repo).await {
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

async fn pulls(
    source: &GiteaSource,
    record: &RepoRecord,
    owner: &str,
    repo: &str,
) -> Result<ViewSection, SourceError> {
    let open = if record.repo.can_have_pulls() {
        rows::open_pulls(source, owner, repo).await?
    } else {
        rows::Paged {
            items: Vec::new(),
            cut: false,
        }
    };
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
    Ok(listed(PULLS, "Pull requests", rows, note))
}

async fn branches(
    source: &GiteaSource,
    record: &RepoRecord,
    owner: &str,
    repo: &str,
) -> Result<ViewSection, SourceError> {
    let mut all: Vec<ListedBranch> = Vec::new();
    let mut cut = true;
    for page in 1..=BRANCH_PAGES {
        let batch = source.client.branches(owner, repo, page).await?;
        if batch.is_empty() {
            cut = false;
            break;
        }
        for raw in batch {
            let branch: ListedBranch = serde_json::from_value(raw).map_err(|error| {
                SourceError::protocol(format!("gitea: an unreadable branch record: {error}"))
            })?;
            all.push(branch);
        }
    }
    let default = record.default_branch();
    let when = |b: &ListedBranch| {
        b.commit
            .as_ref()
            .and_then(|c| c.timestamp.as_deref())
            .and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok())
    };
    all.sort_by(|a, b| {
        (b.name == default)
            .cmp(&(a.name == default))
            .then_with(|| when(b).cmp(&when(a)))
            .then_with(|| a.name.cmp(&b.name))
    });
    let now = chrono::Utc::now();
    let rows: Vec<ViewRow> = all
        .iter()
        .map(|b| {
            let tip = b.commit.as_ref();
            // How long ago its tip was committed, as a commit row says it.
            let age = when(b).map(|t| super::ago(t.with_timezone(&chrono::Utc), now));
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
    let note = rows::left_out("branches", rows.len(), None, cut, record.web("/branches"));
    Ok(listed(BRANCHES, "Branches", rows, note))
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
