//! A section of *Files* drawn from the tree the sync stored for its branch
//! (the stored-trees pass of PR #2479, 2026-11-14; migration `0085`,
//! ADR-0041 and ADR-0042 as amended that day).
//!
//! The adapter names what the section reads (`Source::tree_ask`): a branch,
//! the commit the section's id names, and a directory or every file. The
//! branch's stored row is drawn only where it stands at that commit, or,
//! where the section names none, at the commit the mirror holds for the
//! branch: a row behind the mirror is never drawn as current. The levels the
//! place needs are read by their shas (a directory's way down from the
//! root, or all of them for *Go to file*), and the adapter draws the section
//! from them as its own read at the same commit draws it
//! (`Source::tree_view`).

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use knobas_source::Source;
use knobas_source::tree::{StoredTree, TreeItem, TreePlace};
use knobas_source::view::SourceView;
use sqlx::PgPool;

/// What the store answers for a section.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Stored {
    /// The section drawn from the stored tree, and when the sync last
    /// confirmed the branch's head (what the held line says it is from).
    Drawn(SourceView, DateTime<Utc>),
    /// The branch's tree is past the cap: nothing is stored, and the section
    /// is the source's read. Its entries, at least.
    TooLarge(u64),
}

/// The branch's stored row, the mirror's head of the branch, when the sync
/// last saw it, and the repository's page.
#[derive(sqlx::FromRow)]
struct Row {
    source_id: String,
    commit_sha: String,
    root: Option<Vec<u8>>,
    entries: i64,
    synced_at: DateTime<Utc>,
    tip: Option<String>,
    repository_url: Option<String>,
}

/// `section` of `entity`'s view as the store holds it for `source`, or
/// `None` where it holds nothing for it (the adapter stores no tree, the
/// section is no tree's, the branch was not read yet or moved since).
pub(crate) async fn stored(
    pool: &PgPool,
    source: &dyn Source,
    entity: &str,
    section: Option<&str>,
) -> Option<Stored> {
    let shape = source.tree_shape()?;
    let payload: serde_json::Value =
        sqlx::query_scalar("select payload from sync.item where entity_id = $1")
            .bind(entity)
            .fetch_optional(pool)
            .await
            .ok()??;
    let ask = source.tree_ask(entity, section, &payload)?;
    let commit_path: Vec<String> = shape
        .branch_commit
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
    let row: Row = sqlx::query_as(
        "select bt.source_id, bt.commit_sha, bt.root, bt.entries, i.synced_at,
                i.payload #>> $2::text[] as tip, r.web_url as repository_url
           from sync.branch_tree bt
           join sync.item i on i.entity_id = bt.entity_id
           left join sync.item r on r.entity_id = bt.repository_id
          where bt.entity_id = $1",
    )
    .bind(&ask.branch)
    .bind(&commit_path)
    .fetch_optional(pool)
    .await
    .ok()??;
    let at = ask.commit.as_deref().or(row.tip.as_deref())?;
    if at != row.commit_sha {
        return None;
    }
    let Some(root) = row.root else {
        return Some(Stored::TooLarge(u64::try_from(row.entries).unwrap_or(0)));
    };
    let levels = match &ask.place {
        TreePlace::Dir(dir) => way_down(pool, &row.source_id, &root, dir).await?,
        TreePlace::Every => every_level(pool, &row.source_id, &root).await?,
    };
    let tree = StoredTree {
        commit: row.commit_sha,
        root: hex(&root),
        levels,
        repository_url: row.repository_url,
    };
    let view = source.tree_view(entity, section, &payload, &tree)?;
    Some(Stored::Drawn(view, row.synced_at))
}

/// One stored level as the database holds it.
type LevelRow = (Vec<u8>, Vec<String>, Vec<i32>, Vec<i64>, Vec<Vec<u8>>);

fn level_of((_, names, modes, sizes, shas): LevelRow) -> Vec<TreeItem> {
    names
        .into_iter()
        .zip(modes)
        .zip(sizes)
        .zip(shas)
        .map(|(((name, mode), size), sha)| TreeItem {
            name,
            mode: u32::try_from(mode).unwrap_or(0),
            size: u64::try_from(size).unwrap_or(0),
            sha: hex(&sha),
        })
        .collect()
}

/// The levels from the root down to `dir`, one read a level.
async fn way_down(
    pool: &PgPool,
    source_id: &str,
    root: &[u8],
    dir: &str,
) -> Option<HashMap<String, Vec<TreeItem>>> {
    let mut levels = HashMap::new();
    let mut sha = root.to_vec();
    let mut parts = dir.split('/').filter(|part| !part.is_empty());
    loop {
        let row: LevelRow = sqlx::query_as(
            "select sha, names, modes, sizes, shas from sync.git_tree
              where source_id = $1 and sha = $2",
        )
        .bind(source_id)
        .bind(&sha)
        .fetch_optional(pool)
        .await
        .ok()??;
        let level = level_of(row);
        let next = match parts.next() {
            None => None,
            Some(part) => Some(
                level
                    .iter()
                    .find(|item| item.name == part && item.is_tree())
                    .map(|item| unhex(&item.sha))?,
            ),
        };
        levels.insert(hex(&sha), level);
        match next {
            None => return Some(levels),
            Some(below) => sha = below,
        }
    }
}

/// Every level the root reaches, in one read.
async fn every_level(
    pool: &PgPool,
    source_id: &str,
    root: &[u8],
) -> Option<HashMap<String, Vec<TreeItem>>> {
    let rows: Vec<LevelRow> = sqlx::query_as(
        "with recursive reached(sha) as (
           select $2::bytea
           union
           select u.sha
             from reached r
             join sync.git_tree t on t.source_id = $1 and t.sha = r.sha
             cross join lateral unnest(t.shas, t.modes) as u(sha, mode)
            where u.mode = 16384
         )
         select t.sha, t.names, t.modes, t.sizes, t.shas
           from reached r
           join sync.git_tree t on t.source_id = $1 and t.sha = r.sha",
    )
    .bind(source_id)
    .bind(root)
    .fetch_all(pool)
    .await
    .ok()?;
    Some(
        rows.into_iter()
            .map(|row| (hex(&row.0), level_of(row)))
            .collect(),
    )
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut out, b| {
            let _ = write!(out, "{b:02x}");
            out
        })
}

fn unhex(sha: &str) -> Vec<u8> {
    let bytes = sha.as_bytes();
    if !bytes.len().is_multiple_of(2) || !bytes.iter().all(u8::is_ascii_hexdigit) {
        return bytes.to_vec();
    }
    bytes
        .chunks(2)
        .map(|pair| {
            let digit = |b: u8| (b as char).to_digit(16).unwrap_or(0) as u8;
            digit(pair[0]) << 4 | digit(pair[1])
        })
        .collect()
}
