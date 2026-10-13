//! *Checks* (#710, spec #701 workflow 5): what Gitea's own commit statuses
//! say about the commit a pull request, a branch or a commit view stands at.
//!
//! One read, `GET /repos/{o}/{r}/commits/{sha}/status`: Gitea's combined
//! status, the latest status of each context. Drawn on *Overview* right after
//! the facts as an entity list, the failing first and the passing last, so
//! the line a reader is looking for is the top one. A commit nobody reported
//! on draws nothing: no title over an empty list (the view stays quiet).
//!
//! **A failed read does not fail the view** (orchestrator ruling on #710,
//! 2026-10-13): checks are secondary to the record they are drawn on, so the
//! view draws without them and says so in one muted line in their place,
//! *Checks could not be read: <reason>* (ADR-0020 rule 2: never silent about
//! an error).
//!
//! The TeamCity builds knobas mirrors for the same commit are not this
//! module's: they are a mirror fact the shell draws beside the view, so they
//! show even when this read fails.

use knobas_source::SourceError;
use knobas_source::view::{Block, Tone, ViewRow};

use crate::source::GiteaSource;

/// The *Checks* block for the commit `reference` names in `owner/repo` (a
/// sha, or a branch for its tip), or `None` where Gitea holds no status for
/// it. Where the read fails, the one muted line that says why ([`unread`]).
pub(super) async fn block(
    source: &GiteaSource,
    owner: &str,
    repo: &str,
    reference: &str,
) -> Option<Block> {
    match source
        .client
        .combined_status_as::<Combined>(owner, repo, reference)
        .await
    {
        Ok(combined) => drawn(combined.statuses),
        Err(error) => Some(unread(&error)),
    }
}

/// The line drawn where the checks would be when they could not be read: an
/// untitled list of one muted row, the shape a list's own muted note has
/// (#678), so nothing new crosses to the shell.
fn unread(error: &SourceError) -> Block {
    Block::Entities {
        title: None,
        rows: vec![ViewRow {
            entity_id: None,
            title: format!("Checks could not be read: {error}"),
            meta: None,
            tone: Some(Tone::Muted),
            chip: None,
            url: None,
            commit: None,
        }],
    }
}

/// The statuses as rows, failing first; `None` for none.
fn drawn(statuses: Vec<Status>) -> Option<Block> {
    let mut rows: Vec<(u8, ViewRow)> = statuses
        .into_iter()
        .filter_map(|status| {
            let context = nonblank(status.context)?;
            let state = nonblank(status.status).unwrap_or_default();
            let (rank, tone) = weigh(&state);
            Some((
                rank,
                ViewRow {
                    entity_id: None,
                    title: context,
                    meta: nonblank(status.description),
                    tone,
                    chip: (!state.is_empty()).then_some(state),
                    url: nonblank(status.target_url),
                    commit: None,
                },
            ))
        })
        .collect();
    if rows.is_empty() {
        return None;
    }
    // Stable: within one weight, the order Gitea gave.
    rows.sort_by_key(|(rank, _)| *rank);
    Some(Block::Entities {
        title: Some("Checks".to_owned()),
        rows: rows.into_iter().map(|(_, row)| row).collect(),
    })
}

/// Where a state sorts and how loud it is: Gitea's five states, `failure`
/// and `error` first, a state it may add later after them all and untoned.
fn weigh(state: &str) -> (u8, Option<Tone>) {
    match state {
        "failure" | "error" => (0, Some(Tone::Err)),
        "warning" => (1, Some(Tone::Warn)),
        "pending" => (2, Some(Tone::Info)),
        "success" => (3, Some(Tone::Ok)),
        _ => (4, None),
    }
}

fn nonblank(value: Option<String>) -> Option<String> {
    value.map(|v| v.trim().to_owned()).filter(|v| !v.is_empty())
}

/// The fields of Gitea's `CombinedStatus` this block reads.
#[derive(Debug, serde::Deserialize)]
struct Combined {
    /// Gitea answers `null` for a commit with no statuses on some versions.
    #[serde(default, deserialize_with = "null_as_empty")]
    statuses: Vec<Status>,
}

/// One `CommitStatus`. Gitea names its state `status` here.
#[derive(Debug, serde::Deserialize)]
struct Status {
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    context: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    target_url: Option<String>,
}

fn null_as_empty<'de, D>(deserializer: D) -> Result<Vec<Status>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::Deserialize;
    Ok(Option::<Vec<Status>>::deserialize(deserializer)?.unwrap_or_default())
}
