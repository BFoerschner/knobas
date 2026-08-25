//! Flat result rows into the launcher's groups, in an order that does not move.
//!
//! The query builder returns one flat row set: a header row per matching kind
//! carrying its true count, outer-joined to the page's detail rows. Folding
//! that into [`ResultGroup`]s is the only place the two halves meet, and it is
//! deliberately *not* in the SQL -- a kind with a count and no hits is a
//! perfectly ordinary row down there, and turning it into "a group with no
//! hits" is a display decision.

use chrono::{DateTime, Utc};

use crate::saturating_u32;
use crate::snippet;
use crate::types::{EntityRow, ResultGroup, SearchHit};
use crate::vocab::KindCatalog;

/// The order groups appear in, regardless of what ranked highest.
///
/// A launcher whose sections reshuffle per keystroke cannot be used by muscle
/// memory, so the order is fixed (spec §4 "results grouped by type") and taken
/// from the round-3 mockup. Kinds not named here -- a source knobas has never
/// seen -- sort after it, alphabetically, so a new adapter's items appear in a
/// stable place without an entry being added anywhere (spec §3a).
pub(crate) const GROUP_ORDER: &[&str] = &[
    "ticket",
    "asset",
    "pr",
    "build",
    "build_config",
    "page",
    "note",
    "commit",
    "branch",
    "repo",
    "route",
    "monitor",
];

/// One row of the generated statement, exactly as it is projected.
///
/// **Every column but the group and its count is `Option`**, and that is not
/// defensiveness: `totals left join detail` is what makes a kind the global
/// limit cut still report a true total, and such a row carries nulls for every
/// display column. A non-optional `rank: f32` here panics at runtime on the
/// *normal* case, not on an edge one.
///
/// Names the nine columns it reads and no more. `sync.live_item` carries a
/// `tsvector`; `select *` into a `FromRow` struct panics at runtime
/// (interfaces §1).
#[derive(Debug, sqlx::FromRow)]
pub struct RawHit {
    group_kind: String,
    kind_total: i64,
    entity_id: Option<String>,
    source_id: Option<String>,
    title: Option<String>,
    updated_at: Option<DateTime<Utc>>,
    synced_at: Option<DateTime<Utc>>,
    rank: Option<f32>,
    snippet: Option<String>,
}

/// Fold the flat rows into groups, ordered by [`GROUP_ORDER`].
///
/// The rows arrive ordered by kind and then by rank, so a group's hits keep
/// the statement's ordering and only the *groups* are re-sorted here.
#[must_use]
pub fn group(rows: Vec<RawHit>, kinds: &KindCatalog) -> Vec<ResultGroup> {
    let mut groups: Vec<ResultGroup> = Vec::new();
    for row in rows {
        let index = match groups.iter().position(|g| g.kind == row.group_kind) {
            Some(index) => index,
            None => {
                let info = kinds.info(&row.group_kind);
                groups.push(ResultGroup {
                    kind: row.group_kind.clone(),
                    label: info.label,
                    plural: info.plural,
                    monogram: info.monogram,
                    total: saturating_u32(row.kind_total),
                    hits: Vec::new(),
                });
                groups.len() - 1
            }
        };
        if let Some(hit) = hit(row) {
            groups[index].hits.push(hit);
        }
    }

    groups.sort_by(|a, b| (order_index(&a.kind), &a.kind).cmp(&(order_index(&b.kind), &b.kind)));
    groups
}

/// The detail half of a row, if it has one.
///
/// `entity_id` and `synced_at` are the two columns that are non-null for every
/// real row of the mirror, so their presence is what tells a detail row from
/// the header of a kind the limit cut. Requiring **both** rather than one is
/// what keeps a half-decoded row from becoming a hit with an invented
/// timestamp.
fn hit(row: RawHit) -> Option<SearchHit> {
    let (entity_id, synced_at) = row.entity_id.zip(row.synced_at)?;
    Some(SearchHit {
        row: EntityRow {
            entity_id,
            kind: row.group_kind,
            source_id: row.source_id.unwrap_or_default(),
            title: row.title.unwrap_or_default(),
            updated_at: row.updated_at,
            synced_at,
        },
        rank: row.rank.unwrap_or_default(),
        snippet: row
            .snippet
            .as_deref()
            .map(snippet::segments)
            .unwrap_or_default(),
    })
}

/// Where a kind sits in the fixed order; unknown kinds sort after all of it.
fn order_index(kind: &str) -> usize {
    GROUP_ORDER
        .iter()
        .position(|known| *known == kind)
        .unwrap_or(GROUP_ORDER.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header(kind: &str, total: i64) -> RawHit {
        RawHit {
            group_kind: kind.to_owned(),
            kind_total: total,
            entity_id: None,
            source_id: None,
            title: None,
            updated_at: None,
            synced_at: None,
            rank: None,
            snippet: None,
        }
    }

    fn detail(kind: &str, total: i64, id: &str, rank: f32) -> RawHit {
        RawHit {
            group_kind: kind.to_owned(),
            kind_total: total,
            entity_id: Some(id.to_owned()),
            source_id: Some("jira".to_owned()),
            title: Some(id.to_owned()),
            updated_at: None,
            synced_at: Some(Utc::now()),
            rank: Some(rank),
            snippet: Some(format!(
                "{}sepa{} batch",
                snippet::HIT_START,
                snippet::HIT_STOP
            )),
        }
    }

    #[test]
    fn the_group_order_is_the_mockups_and_not_the_ranking() {
        // The rows arrive kind-alphabetical (`order by t.kind`), which is
        // exactly the order the launcher must *not* show.
        let rows = vec![
            detail("build", 1, "tc:1", 0.9),
            detail("page", 1, "cf:1", 0.8),
            detail("pr", 1, "gt:1", 0.7),
            detail("ticket", 1, "ji:1", 0.1),
        ];
        let groups = group(rows, &KindCatalog::default());
        assert_eq!(
            groups.iter().map(|g| g.kind.as_str()).collect::<Vec<_>>(),
            ["ticket", "pr", "build", "page"]
        );
    }

    /// A kind no adapter ever declared still has to be grouped and drawn, and
    /// it must land somewhere stable rather than wherever the DB emitted it.
    #[test]
    fn kinds_outside_the_order_sort_after_it_alphabetically() {
        let rows = vec![
            detail("zebra", 1, "x:z", 0.5),
            detail("aardvark", 1, "x:a", 0.4),
            detail("pr", 1, "gt:1", 0.3),
        ];
        let groups = group(rows, &KindCatalog::default());
        assert_eq!(
            groups.iter().map(|g| g.kind.as_str()).collect::<Vec<_>>(),
            ["pr", "aardvark", "zebra"]
        );
    }

    /// The outer join's whole purpose: a kind whose rows the limit cut is a
    /// header with a true count and no hits, never a missing group.
    #[test]
    fn a_header_row_becomes_a_group_with_a_total_and_no_hits() {
        let rows = vec![
            detail("ticket", 12, "ji:1", 0.9),
            detail("ticket", 12, "ji:2", 0.8),
            header("pr", 7),
        ];
        let groups = group(rows, &KindCatalog::default());
        let pr = groups.iter().find(|g| g.kind == "pr").expect("a pr group");
        assert_eq!((pr.total, pr.hits.len()), (7, 0));
        let ticket = groups.iter().find(|g| g.kind == "ticket").expect("tickets");
        assert_eq!((ticket.total, ticket.hits.len()), (12, 2));
        // Hits keep the statement's order; only the groups are re-sorted.
        assert_eq!(ticket.hits[0].row.entity_id, "ji:1");
    }

    /// Display metadata is the adapter's where it declared any, derived where
    /// it did not -- and the group never carries the kind id as its label.
    #[test]
    fn declared_metadata_wins_over_derived() {
        let declared = KindCatalog::from_kinds(["pr"]);
        let groups = group(vec![detail("pr", 1, "gt:1", 0.5)], &declared);
        assert_eq!(groups[0].plural, "Pull requests");
        assert_eq!(groups[0].monogram, "PR");
    }

    /// The snippet crosses as segments with the match flagged, and the
    /// sentinels never survive into one (roadmap §4 gotcha 7).
    #[test]
    fn the_snippet_arrives_as_flagged_segments() {
        let groups = group(
            vec![detail("ticket", 1, "ji:1", 0.5)],
            &KindCatalog::default(),
        );
        let snippet = &groups[0].hits[0].snippet;
        assert!(snippet.iter().any(|s| s.hit && s.text == "sepa"));
        assert!(
            snippet
                .iter()
                .all(|s| !s.text.contains([snippet::HIT_START, snippet::HIT_STOP]))
        );
    }

    /// A null snippet is browse mode (no text, so no headline), not an error.
    #[test]
    fn a_row_with_no_headline_has_no_segments() {
        let mut row = detail("ticket", 1, "ji:1", 0.0);
        row.snippet = None;
        let groups = group(vec![row], &KindCatalog::default());
        assert!(groups[0].hits[0].snippet.is_empty());
    }
}
