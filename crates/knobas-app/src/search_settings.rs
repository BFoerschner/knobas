//! **Settings › Search** (the everything-search decisions, 2026-11-20, D13):
//! what *Everything* matches beyond names and keys, a box each, kept in
//! `knobas.setting` under [`EVERYTHING_KEY`] as one JSON object.
//!
//! A box filters what the next key matches and changes nothing kept: the
//! mirror syncs and indexes as before, so setting a box again is instant.
//! *Titles* matches names and keys whatever is set here. A key the object
//! lacks reads as on ([`knobas_search::EverythingMatches`]'s `serde`
//! default), and so does an object an older or a broken knobas wrote.

use knobas_search::EverythingMatches;
use sqlx::PgPool;

use crate::IpcError;

/// The `knobas.setting` key of what *Everything* matches.
pub const EVERYTHING_KEY: &str = "search.everything";

/// What Settings › Search draws: the boxes, and beside each how much of the
/// mirror it covers.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, ts_rs::TS)]
#[ts(export_to = "search.ts")]
pub struct SearchSettings {
    /// The boxes.
    pub everything: EverythingMatches,
    /// The tickets whose description and comments the first box covers.
    pub tickets: u32,
    /// The pull requests the second covers.
    pub pull_requests: u32,
    /// The Docs pages the third covers.
    pub pages: u32,
    /// The pipelines whose last finished run's log the fourth covers.
    pub pipelines: u32,
}

/// What every command's searcher is built with, in one read of
/// `knobas.setting` by its key: whether notes are shown (#940), and what
/// *Everything* matches (D13).
///
/// # Errors
///
/// [`Internal`](crate::IpcErrorCode::Internal) if the read fails.
pub async fn for_searcher(pool: &PgPool) -> Result<(bool, EverythingMatches), IpcError> {
    let rows: Vec<(String, serde_json::Value)> =
        sqlx::query_as("select key, value from knobas.setting where key = any($1)")
            .bind([crate::capture::SHOWN_KEY, EVERYTHING_KEY])
            .fetch_all(pool)
            .await
            .map_err(IpcError::internal)?;
    let mut shown = false;
    let mut matches = EverythingMatches::default();
    for (key, value) in rows {
        if key == crate::capture::SHOWN_KEY {
            // `capture::shown`'s rule: the string `true`, and nothing else.
            shown = value.as_str() == Some("true");
        } else {
            matches = read(value);
        }
    }
    Ok((shown, matches))
}

/// A stored object as the boxes; one that does not read as them is every box
/// on, the miss direction a person can see and fix on the page.
fn read(value: serde_json::Value) -> EverythingMatches {
    serde_json::from_value(value).unwrap_or_default()
}

/// The boxes as stored, every one on until set.
///
/// # Errors
///
/// [`Internal`](crate::IpcErrorCode::Internal) if the read fails.
pub async fn everything(pool: &PgPool) -> Result<EverythingMatches, IpcError> {
    Ok(crate::settings::read_value(pool, EVERYTHING_KEY)
        .await?
        .map(read)
        .unwrap_or_default())
}

/// Store the boxes.
///
/// # Errors
///
/// [`Internal`](crate::IpcErrorCode::Internal) if the write fails.
pub async fn set_everything(pool: &PgPool, matches: EverythingMatches) -> Result<(), IpcError> {
    let value = serde_json::to_value(matches).map_err(IpcError::internal)?;
    crate::settings::write_value(pool, EVERYTHING_KEY, &value).await
}

/// The page's answer: the boxes, and the mirror's live tickets, pull
/// requests and pages and its stored run logs, counted in one statement.
///
/// # Errors
///
/// [`Internal`](crate::IpcErrorCode::Internal) if a read fails.
pub async fn settings(pool: &PgPool) -> Result<SearchSettings, IpcError> {
    let everything = everything(pool).await?;
    let (tickets, pull_requests, pages, pipelines): (i64, i64, i64, i64) =
        sqlx::query_as(COUNTS_SQL)
            .fetch_one(pool)
            .await
            .map_err(IpcError::internal)?;
    let count = |n: i64| u32::try_from(n).unwrap_or(u32::MAX);
    Ok(SearchSettings {
        everything,
        tickets: count(tickets),
        pull_requests: count(pull_requests),
        pages: count(pages),
        pipelines: count(pipelines),
    })
}

/// What each box covers: the mirror's live rows of the three kinds, and its
/// stored run logs. Read when Settings › Search opens, never on a key.
pub const COUNTS_SQL: &str = "\
select count(*) filter (where i.kind = 'ticket'),
       count(*) filter (where i.kind = 'pr'),
       count(*) filter (where i.kind = 'page'),
       (select count(*) from sync.pipeline_log)
  from sync.item i
  join knobas.entity e on e.id = i.entity_id and e.deleted_at is null
 where i.kind in ('ticket', 'pr', 'page')";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_box_is_on_and_an_unreadable_object_is_every_box_on() {
        let read_back = read(serde_json::json!({ "ticket_text": false }));
        assert_eq!(
            read_back,
            EverythingMatches {
                ticket_text: false,
                ..EverythingMatches::default()
            }
        );
        assert_eq!(
            read(serde_json::json!("garbage")),
            EverythingMatches::default()
        );
        assert_eq!(
            read(serde_json::json!({ "ticket_text": "no" })),
            EverythingMatches::default()
        );
    }
}
