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
/// from the round-3 mockup, with one rule over it: **a parent kind always comes
/// before its child kinds** (ADR-0018, #593). So `repo` sits above the `pr`,
/// `commit` and `branch` it holds, `build_config` above `build`, and `asset`
/// above `route` -- the thing you navigate *to* above the things you scroll
/// *through*. Kinds not named here -- a source knobas has never seen -- sort
/// after it, alphabetically, so a new adapter's items appear in a stable place
/// without an entry being added anywhere (spec §3a).
pub(crate) const GROUP_ORDER: &[&str] = &[
    "view",
    "set",
    "ticket",
    "asset",
    "repo",
    "pr",
    "pipeline",
    "build_config",
    "build",
    "page",
    "note",
    "commit",
    "branch",
    "route",
    "monitor",
    "list",
];

/// One row of the generated statement, exactly as it is projected.
///
/// **Every column but the group and its count is `Option`**, and that is not
/// defensiveness: `totals left join detail` is what makes a kind the global
/// limit cut still report a true total, and such a row carries nulls for every
/// display column. A non-optional `rank: f32` here panics at runtime on the
/// *normal* case, not on an edge one.
///
/// Names the ten columns it reads and no more. `sync.live_item` carries a
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
    /// Where the row sits inside its source, joined by the statement (#284,
    /// #596). Null for a group header, and null for every record whose kind
    /// declares no parent or whose declared parent misses -- two absences that
    /// mean the same thing to a reader.
    path: Option<String>,
    /// The row's state chip (#666), `declared_state!`'s `jsonb`. Null for a
    /// group header, a kind that declares no state, and a corpus knobas owns.
    state: Option<sqlx::types::Json<knobas_core::payload::StateChip>>,
    /// The key cell (#2452). Null for a group header and a corpus knobas owns.
    cell: Option<String>,
    rank: Option<f32>,
    snippet: Option<String>,
    /// The text the excerpts are read from (#2431): null but under
    /// *Everything* with words.
    excerpt_text: Option<String>,
    /// The number of `excerpt_text`'s first line when it is a run log.
    excerpt_line: Option<i64>,
}

/// Whether a names answer is **few**: it fills no kind (`per_group` rows of
/// one) and not the page (`limit` rows in all). Where two or three typed
/// characters name so little under *Everything*, the text is asked as well
/// ([`filled`]); names that fill a kind are the answer by themselves.
#[must_use]
pub(crate) fn few(names: &[RawHit], per_group: u32, limit: u32) -> bool {
    let mut of_kind: std::collections::HashMap<&str, u32> = std::collections::HashMap::new();
    let mut all = 0_u32;
    for row in names.iter().filter(|row| row.is_hit()) {
        let held = of_kind.entry(row.group_kind.as_str()).or_default();
        *held += 1;
        all += 1;
        if *held >= per_group {
            return false;
        }
    }
    all < limit
}

/// The page two or three typed characters are answered with where the names
/// are few (the two-letters pass, 2026-11-16;
/// `docs/decisions/2026-11-16-two-letters-by-names-and-keys-wait-for-their-answer.md`):
/// `names`, every row of them where it stood, and in the room they leave the
/// rows `text` found, the answer the words' ranking gives the same
/// characters.
///
/// * **A name is never displaced.** `names` holds fewer rows than `limit`,
///   so every one is on the page, first in its kind and in its own order.
/// * **A row is on the page once.** One `text` found that `names` holds is
///   the name.
/// * **A kind holds `per_group` rows at most**, its names and what follows
///   them.
/// * **The room is dealt a row a kind at a time**, as the names' page is: a
///   kind's rows of `text` count on from its names, so a kind that has no
///   row yet takes its first before another takes its fourth. Within a
///   round the higher rank first, then the newer, then the id.
/// * **A kind's total is the rows the page holds of it**, as a names answer
///   counts: no kind is a header alone.
///
/// The rows come back names first, then `text`'s in the order they arrived,
/// which is by kind and then as ranked, so [`group`] draws a kind's names
/// above what the text added.
#[must_use]
pub(crate) fn filled(
    names: Vec<RawHit>,
    text: Vec<RawHit>,
    per_group: u32,
    limit: u32,
) -> Vec<RawHit> {
    use std::cmp::Ordering;
    use std::collections::{HashMap, HashSet};

    let per_group = usize::try_from(per_group).unwrap_or(usize::MAX);
    let limit = usize::try_from(limit).unwrap_or(usize::MAX);
    let mut page: Vec<RawHit> = names.into_iter().filter(RawHit::is_hit).collect();
    let mut held: HashSet<(String, String)> = HashSet::new();
    let mut of_kind: HashMap<String, usize> = HashMap::new();
    for row in &page {
        held.insert((
            row.group_kind.clone(),
            row.entity_id.clone().unwrap_or_default(),
        ));
        *of_kind.entry(row.group_kind.clone()).or_default() += 1;
    }

    // Each row of the text that could join, with the place it would take in
    // its kind.
    let mut offered: Vec<(usize, usize)> = Vec::new();
    let mut next = of_kind.clone();
    for (index, row) in text.iter().enumerate() {
        let Some(id) = row.entity_id.as_ref().filter(|_| row.is_hit()) else {
            continue;
        };
        if held.contains(&(row.group_kind.clone(), id.clone())) {
            continue;
        }
        let place = next.entry(row.group_kind.clone()).or_default();
        if *place >= per_group {
            continue;
        }
        *place += 1;
        offered.push((*place, index));
    }
    // A row a kind at a time, the best of a round first.
    let best_first = |a: &RawHit, b: &RawHit| {
        b.rank
            .partial_cmp(&a.rank)
            .unwrap_or(Ordering::Equal)
            .then_with(|| match (a.updated_at, b.updated_at) {
                (Some(a), Some(b)) => b.cmp(&a),
                (Some(_), None) => Ordering::Less,
                (None, Some(_)) => Ordering::Greater,
                (None, None) => Ordering::Equal,
            })
            .then_with(|| a.entity_id.cmp(&b.entity_id))
    };
    offered.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then_with(|| best_first(&text[a.1], &text[b.1]))
    });
    offered.truncate(limit.saturating_sub(page.len()));
    let taken: HashSet<usize> = offered.into_iter().map(|(_, index)| index).collect();
    for (index, row) in text.into_iter().enumerate() {
        if taken.contains(&index) {
            *of_kind.entry(row.group_kind.clone()).or_default() += 1;
            page.push(row);
        }
    }
    for row in &mut page {
        row.kind_total = i64::try_from(of_kind[&row.group_kind]).unwrap_or(i64::MAX);
    }
    page
}

impl RawHit {
    /// Whether the row is a hit and not the header of a kind the page cut:
    /// the two columns [`hit`] asks for.
    fn is_hit(&self) -> bool {
        self.entity_id.is_some() && self.synced_at.is_some()
    }
}

/// Fold the flat rows into groups, ordered by [`GROUP_ORDER`].
///
/// The rows arrive ordered by kind and then by rank, so a group's hits keep
/// the statement's ordering and only the *groups* are re-sorted here.
#[must_use]
pub fn group(rows: Vec<RawHit>, kinds: &KindCatalog) -> Vec<ResultGroup> {
    grouped(rows, kinds, &[])
}

/// [`group`], each hit carrying the places in its text that hold one of
/// `needles` (#2431, [`crate::excerpt`]).
#[must_use]
pub fn with_excerpts(
    rows: Vec<RawHit>,
    kinds: &KindCatalog,
    needles: &[String],
) -> Vec<ResultGroup> {
    grouped(rows, kinds, needles)
}

fn grouped(rows: Vec<RawHit>, kinds: &KindCatalog, needles: &[String]) -> Vec<ResultGroup> {
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
        if let Some(hit) = hit(row, needles) {
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
fn hit(row: RawHit, needles: &[String]) -> Option<SearchHit> {
    let (entity_id, synced_at) = row.entity_id.zip(row.synced_at)?;
    let title = row.title.unwrap_or_default();
    let excerpts = match &row.excerpt_text {
        Some(text) if !needles.is_empty() => {
            crate::excerpt::excerpts(&title, text, row.excerpt_line, needles)
        }
        _ => Vec::new(),
    };
    Some(SearchHit {
        row: EntityRow {
            entity_id,
            kind: row.group_kind,
            source_id: row.source_id.unwrap_or_default(),
            title,
            updated_at: row.updated_at,
            synced_at,
            path: row.path,
            state: row.state.map(|chip| chip.0),
            cell: row.cell,
        },
        rank: row.rank.unwrap_or_default(),
        snippet: row
            .snippet
            .as_deref()
            .map(snippet::segments)
            .unwrap_or_default(),
        excerpts,
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
            path: None,
            state: None,
            cell: None,
            rank: None,
            snippet: None,
            excerpt_text: None,
            excerpt_line: None,
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
            path: None,
            state: None,
            cell: None,
            rank: Some(rank),
            snippet: Some(format!(
                "{}sepa{} batch",
                snippet::HIT_START,
                snippet::HIT_STOP
            )),
            excerpt_text: None,
            excerpt_line: None,
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

    /// Every parent -> child kind pair the CONTEXT.md **Parent** entry
    /// and ADR-0018 name, per source as #591 lists them. The descriptor's parent
    /// declaration (#594) is planned to name a payload path and the names the
    /// parent answers to, not the parent's kind, so this table is the record
    /// of which kind sits in which. A kind that is its own parent (a page under
    /// a page, a ticket under an epic, an asset under an asset) orders itself
    /// trivially and is not listed.
    const PARENT_KINDS: &[(&str, &str)] = &[
        ("repo", "branch"),
        ("repo", "pr"),
        ("repo", "commit"),
        ("build_config", "build"),
        ("asset", "route"),
    ];

    /// The rule, not today's list (#593, ADR-0018): a parent kind's group is
    /// drawn above each of its child kinds', whatever order the rows came in.
    #[test]
    fn a_parent_kind_always_comes_before_its_child_kinds() {
        for (parent, child) in PARENT_KINDS {
            for rows in [
                vec![detail(child, 1, "x:c", 0.9), detail(parent, 1, "x:p", 0.1)],
                vec![detail(parent, 1, "x:p", 0.1), detail(child, 1, "x:c", 0.9)],
            ] {
                let groups = group(rows, &KindCatalog::default());
                assert_eq!(
                    groups.iter().map(|g| g.kind.as_str()).collect::<Vec<_>>(),
                    [*parent, *child],
                    "{parent} must be drawn above its child {child}"
                );
            }
        }
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

    /// What a page holds, as `(kind, total, ids)` in the rows' own order.
    fn page(rows: &[RawHit]) -> Vec<(String, i64, String)> {
        rows.iter()
            .map(|row| {
                (
                    row.group_kind.clone(),
                    row.kind_total,
                    row.entity_id.clone().unwrap_or_default(),
                )
            })
            .collect()
    }

    fn of(rows: &[(&str, i64, &str)]) -> Vec<(String, i64, String)> {
        rows.iter()
            .map(|(kind, total, id)| ((*kind).to_owned(), *total, (*id).to_owned()))
            .collect()
    }

    /// **The text fills the room the names leave** (`filled`): every name
    /// stays where it stood, a row both found is the name, a kind holds its
    /// cap and no more, and each kind's total is the rows the page holds.
    #[test]
    fn the_text_fills_the_room_the_names_leave() {
        let names = || {
            vec![
                detail("ticket", 2, "ji:1", 1.0),
                detail("ticket", 2, "ji:2", 0.0),
                detail("repo", 1, "gt:r", 1.0),
            ]
        };
        // As the words' statement gives them: by kind, then as ranked, with
        // the header of a kind its page cut.
        let text = || {
            vec![
                detail("page", 9, "cf:1", 0.6),
                detail("page", 9, "cf:2", 0.2),
                header("pr", 4),
                detail("ticket", 30, "ji:9", 0.9),
                detail("ticket", 30, "ji:2", 0.8),
                detail("ticket", 30, "ji:8", 0.7),
                detail("ticket", 30, "ji:7", 0.1),
            ]
        };
        // Room for all of it: names first, each kind's text after its names,
        // `ji:2` once, and no header.
        assert_eq!(
            page(&filled(names(), text(), 10, 40)),
            of(&[
                ("ticket", 5, "ji:1"),
                ("ticket", 5, "ji:2"),
                ("repo", 1, "gt:r"),
                ("page", 2, "cf:1"),
                ("page", 2, "cf:2"),
                ("ticket", 5, "ji:9"),
                ("ticket", 5, "ji:8"),
                ("ticket", 5, "ji:7"),
            ]),
        );
        // Three rows a kind: the tickets take one more after their two
        // names, the best of the text's.
        assert_eq!(
            page(&filled(names(), text(), 3, 40)),
            of(&[
                ("ticket", 3, "ji:1"),
                ("ticket", 3, "ji:2"),
                ("repo", 1, "gt:r"),
                ("page", 2, "cf:1"),
                ("page", 2, "cf:2"),
                ("ticket", 3, "ji:9"),
            ]),
        );
        // No text: the names as they were.
        assert_eq!(
            page(&filled(names(), Vec::new(), 10, 40)),
            of(&[
                ("ticket", 2, "ji:1"),
                ("ticket", 2, "ji:2"),
                ("repo", 1, "gt:r"),
            ]),
        );
    }

    /// **The room is dealt a row a kind at a time.** A kind's text counts on
    /// from its names, so the kind with no row yet takes its first before a
    /// kind with two takes its third, whatever the ranks; within a round the
    /// higher rank is first, then the newer row, then the id. No name gives
    /// way, however many rows the text ranks above it.
    #[test]
    fn the_room_is_dealt_a_row_a_kind_at_a_time() {
        let names = || {
            vec![
                detail("ticket", 2, "ji:1", 1.0),
                detail("ticket", 2, "ji:2", 0.0),
            ]
        };
        let at = |mut row: RawHit, minutes: i64| {
            row.updated_at = Some(Utc::now() + chrono::Duration::minutes(minutes));
            row
        };
        // The first rows of the pages and of the pull requests tie on rank,
        // and the pull request is the newer.
        let text = || {
            vec![
                at(detail("page", 3, "cf:1", 0.5), 1),
                detail("page", 3, "cf:2", 0.1),
                at(detail("pr", 2, "gt:a", 0.5), 9),
                detail("pr", 2, "gt:b", 0.4),
                detail("ticket", 9, "ji:9", 9.0),
                detail("ticket", 9, "ji:8", 8.0),
            ]
        };
        // Room for one: the first row of a kind that has none, the newer of
        // the two that tie, and never the ticket that outranks both.
        assert_eq!(
            page(&filled(names(), text(), 10, 3)),
            of(&[
                ("ticket", 2, "ji:1"),
                ("ticket", 2, "ji:2"),
                ("pr", 1, "gt:a"),
            ]),
        );
        // Room for three: both first rows, then the second round's best.
        assert_eq!(
            page(&filled(names(), text(), 10, 5)),
            of(&[
                ("ticket", 2, "ji:1"),
                ("ticket", 2, "ji:2"),
                ("page", 1, "cf:1"),
                ("pr", 2, "gt:a"),
                ("pr", 2, "gt:b"),
            ]),
        );
        // Room for five: the tickets' third row waits for the third round.
        assert_eq!(
            page(&filled(names(), text(), 10, 7)),
            of(&[
                ("ticket", 3, "ji:1"),
                ("ticket", 3, "ji:2"),
                ("page", 2, "cf:1"),
                ("page", 2, "cf:2"),
                ("pr", 2, "gt:a"),
                ("pr", 2, "gt:b"),
                ("ticket", 3, "ji:9"),
            ]),
        );
        // A full page of names takes nothing.
        assert_eq!(
            page(&filled(names(), text(), 10, 2)),
            of(&[("ticket", 2, "ji:1"), ("ticket", 2, "ji:2")]),
        );

        // Within a round the higher rank, whatever the dates; and where rank
        // and date tie, the id.
        let ranked = vec![
            at(detail("page", 1, "cf:1", 0.2), 9),
            at(detail("pr", 1, "gt:a", 0.5), 1),
        ];
        assert_eq!(
            page(&filled(names(), ranked, 10, 3)),
            of(&[
                ("ticket", 2, "ji:1"),
                ("ticket", 2, "ji:2"),
                ("pr", 1, "gt:a"),
            ]),
        );
        let tied = vec![detail("pr", 1, "gt:a", 0.5), detail("page", 1, "cf:1", 0.5)];
        assert_eq!(
            page(&filled(names(), tied, 10, 3)),
            of(&[
                ("ticket", 2, "ji:1"),
                ("ticket", 2, "ji:2"),
                ("page", 1, "cf:1"),
            ]),
        );
        // A row with a date is ahead of one with none.
        let dated = vec![
            detail("page", 1, "cf:1", 0.5),
            at(detail("pr", 1, "gt:a", 0.5), 0),
        ];
        assert_eq!(
            page(&filled(names(), dated, 10, 3)),
            of(&[
                ("ticket", 2, "ji:1"),
                ("ticket", 2, "ji:2"),
                ("pr", 1, "gt:a"),
            ]),
        );
    }

    /// **Names are few where they fill no kind and not the page** (`few`):
    /// then the text is asked as well. A kind at its cap, or a full page, is
    /// an answer by itself; a header is no row.
    #[test]
    fn names_are_few_where_they_fill_no_kind_and_not_the_page() {
        let names = vec![
            detail("ticket", 2, "ji:1", 1.0),
            detail("ticket", 2, "ji:2", 0.0),
            detail("repo", 1, "gt:r", 1.0),
            header("pr", 4),
        ];
        assert!(few(&names, 10, 40));
        // Three rows a kind: two tickets are under it, and at two they fill
        // their kind.
        assert!(few(&names, 3, 40));
        assert!(!few(&names, 2, 40));
        // Three rows in all fill a page of three, and leave room in four.
        assert!(few(&names, 10, 4));
        assert!(!few(&names, 10, 3));
        assert!(few(&[], 10, 40));
        assert!(few(&[header("pr", 4)], 1, 1));
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
