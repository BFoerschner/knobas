//! The Tickets tile's **mini board**: a context's live tickets, grouped into
//! the status columns their own sources gave them (ADR-0009 names it; spec
//! #175 asks for it; #177 is the one granted read behind it).
//!
//! # What this reads, and what it refuses to
//!
//! Membership is [`crate::context::member_ids`]' answer and nothing else --
//! the fixed rule of §16.11 / ADR-0008, computed once and reused here rather
//! than re-spelled, so the board can never disagree with the rest of the room
//! about who is in it. Of those members, the board draws the ones that are
//! **live tickets**: `sync.live_item` is the join that puts a ticket the
//! source deleted off the board while it is still a member by the rule.
//!
//! There is no normalized status model and no migration behind any of this.
//! Contract §4.1 normalizes four fields and a status is not one of them, so a
//! status and a priority can only come out of the [payload][crate], in the
//! source's own shape -- which makes both of them **payload reads outside an
//! adapter**, governed by ADR-0007. Hence [`status_read!`] and
//! [`priority_read!`]: one macro each, so a second source's spelling is one
//! more `coalesce` in one place and nothing anywhere else, and both failure
//! directions are stated on the macros and pinned by tests named there.
//!
//! # Two consumers, one grant
//!
//! [`MiniBoard::columns`] is the tile's board. [`MiniBoard::sources`] is the
//! ticket detail's status select (#179): the statuses each source's own
//! *corpus* shows, which is deliberately wider than the room's columns -- a
//! room with nothing finished still has to be able to offer *Done*. Both come
//! out of this one read.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use serde::Serialize;
use sqlx::PgPool;

use crate::CoreError;
use crate::entity::EntityRef;

/// The statuses the board leads with when the corpus shows them, in the order
/// work moves through them (spec story 3, and the round-3 mockup's four).
///
/// Matched case-insensitively and displayed in the source's own spelling: the
/// list decides *order*, never wording.
const LEADING: [&str; 4] = ["To Do", "In Progress", "In Review", "Done"];

/// SQL for "the string this path leads to, or nothing".
///
/// `->>` yields an object's or an array's *text form* rather than nothing, so
/// without the type check a source that spells its status some other way would
/// arrive as a column headed `{"id":3}` -- a guess dressed as an observation.
/// The `nullif(btrim(...))` is the same refusal for a status that is blank or
/// only whitespace: unreadable, not a column.
macro_rules! string_at {
    ($path:literal) => {
        concat!(
            "(case when jsonb_typeof(",
            $path,
            ") = 'string' then nullif(btrim(",
            $path,
            " #>> '{}'), '') end)"
        )
    };
}

/// The one place a ticket's status is spelled (ADR-0007 requirement 2).
///
/// Jira Data Center's `fields.status.name` first -- where every `ticket` in the
/// mirror comes from today -- then the flat `status` the mock source emits,
/// which is what the demo profile's board is drawn from. A record carrying
/// neither contributes no status.
///
/// **Failure direction (requirement 3): a miss is visible, never a guess.** A
/// ticket whose record has no readable status lands in the board's terminal
/// group ([`MiniBoardColumn::status`] `= None`) instead of being dropped or
/// sorted into a column somebody inferred. Pinned by
/// `a_ticket_with_no_recognizable_status_lands_in_the_terminal_group` in
/// `knobas-app/tests/mini_board_ipc.rs`.
macro_rules! status_read {
    () => {
        concat!(
            "coalesce(",
            string_at!("i.payload->'fields'->'status'->'name'"),
            ", ",
            string_at!("i.payload->'status'"),
            ")"
        )
    };
}

/// The one place a ticket's priority is spelled (ADR-0007 requirement 2), the
/// same two shapes as [`status_read!`].
///
/// **Failure direction (requirement 3): a miss renders nothing.** A card whose
/// record has no readable priority simply omits it, which is the whole of what
/// story 8 asks for -- a board that never guesses at what a source did not
/// say. Pinned by `a_ticket_with_no_recognizable_priority_carries_none` in
/// `knobas-app/tests/mini_board_ipc.rs`.
macro_rules! priority_read {
    () => {
        concat!(
            "coalesce(",
            string_at!("i.payload->'fields'->'priority'->'name'"),
            ", ",
            string_at!("i.payload->'priority'"),
            ")"
        )
    };
}

/// The board's cards: the live tickets among a context's members.
///
/// `sync.live_item` rather than `sync.item`, which is what keeps a tombstoned
/// ticket off the board (story 18). Newest first within the answer, so a
/// column reads like the recency list the tile used to be; the grouping below
/// preserves this order.
const CARDS: &str = concat!(
    "select i.entity_id, i.source_id, i.title, ",
    status_read!(),
    " as status, ",
    priority_read!(),
    " as priority
       from sync.live_item i
      where i.kind = 'ticket'
        and i.entity_id = any($1)
      order by coalesce(i.item_updated_at, i.synced_at) desc, i.entity_id"
);

/// Every status these sources' live ticket corpora show -- the select's
/// offer (#179), not the room's columns.
///
/// Scoped to the sources the board actually drew a card from: the board
/// answers for the tickets on it, and a source nothing in this room came from
/// is not one the detail here can be asked about.
const OBSERVED_STATUSES: &str = concat!(
    "select distinct source_id, status
       from (select i.source_id as source_id, ",
    status_read!(),
    " as status
               from sync.live_item i
              where i.kind = 'ticket'
                and i.source_id = any($1)) observed
      where status is not null"
);

/// One context's tickets as the mini board draws them.
#[derive(Clone, Debug, Serialize)]
pub struct MiniBoard {
    /// The observed status columns, in display order: the four of [`LEADING`]
    /// first where the corpus shows them, every other observed status after
    /// them alphabetically, and the terminal group last.
    pub columns: Vec<MiniBoardColumn>,
    /// Per source that put a card on this board, the statuses its own corpus
    /// shows -- what the ticket detail's status select offers (#179).
    pub sources: Vec<SourceStatuses>,
}

/// One column: a status, and the cards standing in it.
///
/// The count the column header shows is `cards.len()`; it is not a field,
/// because a count beside the list it counts is a second copy of the same fact
/// and only one of them can be right.
#[derive(Clone, Debug, Serialize)]
pub struct MiniBoardColumn {
    /// The status, in the source's own words -- or `None` for the terminal
    /// group: tickets whose mirrored record carries no status this read
    /// recognizes. The words on screen for that group are the shell's ("No
    /// status"); what crosses the bridge is the absence itself.
    pub status: Option<String>,
    /// Newest first.
    pub cards: Vec<MiniBoardCard>,
}

/// One ticket on the board.
#[derive(Clone, Debug, Serialize)]
pub struct MiniBoardCard {
    /// The ticket's stable in-app address, which is what a click opens.
    pub entity_id: String,
    /// Which source this ticket came from -- the same source whose statuses
    /// [`MiniBoard::sources`] lists.
    pub source_id: String,
    /// The source's own key for it (`PAY-231`), i.e. `entity_id` past its
    /// namespace.
    pub key: String,
    pub title: String,
    /// `None` where the record carries no readable priority; see
    /// [`priority_read!`].
    pub priority: Option<String>,
}

/// The statuses one source's live ticket corpus shows, in the board's order.
#[derive(Clone, Debug, Serialize)]
pub struct SourceStatuses {
    pub source_id: String,
    /// Never carries the terminal group: "no status" is something a ticket can
    /// be *in*, not something it can be moved to.
    pub statuses: Vec<String>,
}

/// One row of [`CARDS`].
#[derive(sqlx::FromRow)]
struct CardRow {
    entity_id: String,
    source_id: String,
    title: String,
    status: Option<String>,
    priority: Option<String>,
}

/// Where a column sits in the board's order.
///
/// Four parts, each breaking the tie the one before it leaves: the class
/// (leading, observed, terminal), the position within [`LEADING`], the status
/// case-folded, and the status as spelled. The last is only ever reached by
/// two sources spelling one status in different cases, and exists so the
/// answer does not depend on which of them was written first.
fn column_rank(status: Option<&str>) -> (u8, usize, String, String) {
    let Some(status) = status else {
        return (2, 0, String::new(), String::new());
    };
    let folded = status.to_lowercase();
    match LEADING
        .iter()
        .position(|leading| leading.eq_ignore_ascii_case(status))
    {
        Some(at) => (0, at, folded, status.to_owned()),
        None => (1, 0, folded, status.to_owned()),
    }
}

/// The mini board for one context.
///
/// An unknown or empty context answers with an empty board rather than an
/// error, for the reason [`crate::context::member_ids`] does: an address can
/// outlive the thing it names, and "nothing here" is what the tile needs to be
/// able to say.
///
/// # Errors
///
/// [`CoreError::Db`] if a query fails.
pub async fn read(pool: &PgPool, ctx_id: &str) -> Result<MiniBoard, CoreError> {
    let members = crate::context::member_ids(pool, ctx_id).await?;
    let rows: Vec<CardRow> = sqlx::query_as(CARDS).bind(&members).fetch_all(pool).await?;

    // Grouped in the order the rows arrived, so the newest-first ordering the
    // statement asks for survives into each column; the columns themselves are
    // put in the board's order afterwards.
    let mut at: HashMap<Option<String>, usize> = HashMap::new();
    let mut columns: Vec<MiniBoardColumn> = Vec::new();
    let mut sources: BTreeSet<String> = BTreeSet::new();
    for row in rows {
        sources.insert(row.source_id.clone());
        let index = *at.entry(row.status.clone()).or_insert_with(|| {
            columns.push(MiniBoardColumn {
                status: row.status.clone(),
                cards: Vec::new(),
            });
            columns.len() - 1
        });
        columns[index].cards.push(MiniBoardCard {
            // The sink refuses an item whose id does not round-trip through
            // `EntityRef` (`knobas_sync`'s `check`), so this parses for
            // everything the mirror holds; an id that somehow did not is
            // shown whole rather than not at all.
            key: EntityRef::parse(&row.entity_id)
                .map_or_else(|_| row.entity_id.clone(), |entity| entity.key),
            entity_id: row.entity_id,
            source_id: row.source_id,
            title: row.title,
            priority: row.priority,
        });
    }
    columns.sort_by_key(|column| column_rank(column.status.as_deref()));

    Ok(MiniBoard {
        sources: observed_statuses(pool, &sources).await?,
        columns,
    })
}

/// The second half of the grant: what each of these sources' corpora show.
async fn observed_statuses(
    pool: &PgPool,
    sources: &BTreeSet<String>,
) -> Result<Vec<SourceStatuses>, CoreError> {
    if sources.is_empty() {
        return Ok(Vec::new());
    }
    let ids: Vec<String> = sources.iter().cloned().collect();
    let rows: Vec<(String, String)> = sqlx::query_as(OBSERVED_STATUSES)
        .bind(&ids)
        .fetch_all(pool)
        .await?;

    let mut by_source: BTreeMap<String, Vec<String>> =
        ids.into_iter().map(|id| (id, Vec::new())).collect();
    for (source_id, status) in rows {
        by_source.entry(source_id).or_default().push(status);
    }
    Ok(by_source
        .into_iter()
        .map(|(source_id, mut statuses)| {
            statuses.sort_by_key(|status| column_rank(Some(status)));
            SourceStatuses {
                source_id,
                statuses,
            }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The order the board promises, over a set deliberately shuffled: the
    /// four lead in their own order rather than alphabetically, strays follow
    /// alphabetically rather than in the order they were met, and the terminal
    /// group is last however the rest sorted.
    #[test]
    fn the_four_lead_then_the_rest_alphabetically_then_the_terminal_group() {
        let mut observed = vec![
            None,
            Some("Done"),
            Some("blocked"),
            Some("To Do"),
            Some("Awaiting deploy"),
            Some("In Review"),
            Some("In Progress"),
        ];
        observed.sort_by_key(|status| column_rank(*status));
        assert_eq!(
            observed,
            vec![
                Some("To Do"),
                Some("In Progress"),
                Some("In Review"),
                Some("Done"),
                Some("Awaiting deploy"),
                Some("blocked"),
                None,
            ]
        );
    }

    /// A source shouting its statuses still gets the board's order: the list
    /// decides where a column sits, never how it is spelled.
    #[test]
    fn a_leading_status_leads_whatever_case_it_is_spelled_in() {
        let mut observed = vec![Some("Archived"), Some("DONE"), Some("to do")];
        observed.sort_by_key(|status| column_rank(*status));
        assert_eq!(
            observed,
            vec![Some("to do"), Some("DONE"), Some("Archived")]
        );
    }
}
