//! The mini board.

use super::*;

// -- the mini board (#177) --------------------------------------------------
//
// In this module for the reason the contexts block above gives: §10.8 freezes
// the `commands/` + `ipc/` layout, so a new read gets a section rather than a
// file. It is a room read, and the shell reaches it from the room.

/// The ticket board (ADR-0009, spec #175).
///
/// `role` (#1299, ADR-0029 as amended on 2026-10-19) narrows the board to
/// the sources that fill that role (`sources::crud::fillers`), read per call:
/// the Tickets page asks for the ticket system's. Its columns and counts are
/// then the fillers' alone; a role nobody fills is an empty board, read.
/// Omitted, every source's tickets are on it. `ctx_id` is a stored
/// context and `project` a project key; every narrowing given intersects.
/// The grouping, the column order and the two payload reads all live in
/// [`knobas_core::mini_board`]; this seam adds only the role's fillers.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) while the
/// database is still coming up, [`Internal`](crate::IpcErrorCode::Internal)
/// for a query failure. An unknown context is an empty board, not a
/// `not_found`: the room's other tiles answer that way too, and a tile that
/// errored would say "this broke" where the truth is "nothing here".
///
/// `per_column` cuts each column to its newest cards (#753), the Tickets
/// tile's read; omitted, the board is whole, which is what the ticket detail's
/// status select reads.
///
/// `query` is the ticket-first Tickets view's filter (#810, R-2.7): search
/// grammar text, the view's base query and the filter bar's words and chips.
/// A non-blank one narrows the board to every ticket the grammar admits
/// ([`knobas_search::Searcher::matching_ids`]); omitted or blank, it narrows
/// nothing.
///
/// `saved_view` (#877; named for the role by #1245) names a ticket system's
/// saved view (`sync.saved_view`'s entity id): the board is then
/// [`saved_view_board`]'s, the tickets the source put on that view in its
/// order under the view's own columns, and the room dimensions are not read.
///
/// `lanes` (#939, #1098) puts the board in lanes by that field (R-3.5) on
/// the server: `per_column` then caps each lane's column, and only the
/// first `lanes_first` lanes, biggest first, are sent
/// ([`knobas_core::mini_board::MiniBoard::lanes`]). Omitted, the board has
/// no lanes, as before.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn mini_board(
    lifecycle: State<'_, Lifecycle>,
    ctx_id: Option<String>,
    role: Option<String>,
    project: Option<String>,
    per_column: Option<u32>,
    query: Option<String>,
    saved_view: Option<String>,
    lanes: Option<knobas_core::mini_board::LaneField>,
    lanes_first: Option<u32>,
) -> Result<knobas_core::mini_board::MiniBoard, IpcError> {
    let pool = lifecycle.pool()?;
    let lanes = lanes.map(|by| knobas_core::mini_board::Lanes {
        by,
        first: lanes_first,
    });
    if let Some(view) = saved_view {
        return saved_view_board_laned(
            &pool,
            lifecycle.registry(),
            &view,
            per_column,
            query.as_deref(),
            lanes,
        )
        .await;
    }
    mini_board_laned(
        &pool,
        lifecycle.registry(),
        Room {
            ctx_id: ctx_id.as_deref(),
            role: role.as_deref(),
            project: project.as_deref(),
        },
        per_column,
        query.as_deref(),
        lanes,
    )
    .await
}

/// A board's three dimensions, as [`mini_board`] takes them.
#[derive(Clone, Copy, Debug, Default)]
pub struct Room<'a> {
    pub ctx_id: Option<&'a str>,
    /// The role whose fillers the board is read over (#1299); `None` reads
    /// every source.
    pub role: Option<&'a str>,
    pub project: Option<&'a str>,
}

/// [`mini_board`], against a pool.
///
/// Every card says whether it is assigned to me, by the usernames the
/// enabled sources are configured with: the identity `@me` resolves to.
///
/// # Errors
///
/// See [`mini_board`]; a query past the launcher's length is `invalid`.
pub async fn mini_board_inner(
    pool: &PgPool,
    registry: &dyn knobas_sync::scheduler::AdapterRegistry,
    ctx_id: Option<&str>,
    role: Option<&str>,
    project: Option<&str>,
    per_column: Option<u32>,
    query: Option<&str>,
) -> Result<knobas_core::mini_board::MiniBoard, IpcError> {
    mini_board_laned(
        pool,
        registry,
        Room {
            ctx_id,
            role,
            project,
        },
        per_column,
        query,
        None,
    )
    .await
}

/// [`mini_board_inner`] in lanes (#939): see [`mini_board`]'s `lanes`.
///
/// # Errors
///
/// See [`mini_board_inner`].
pub async fn mini_board_laned(
    pool: &PgPool,
    registry: &dyn knobas_sync::scheduler::AdapterRegistry,
    room: Room<'_>,
    per_column: Option<u32>,
    query: Option<&str>,
    lanes: Option<knobas_core::mini_board::Lanes>,
) -> Result<knobas_core::mini_board::MiniBoard, IpcError> {
    let Room {
        ctx_id,
        role,
        project,
    } = room;
    // The role's fillers (#1299). The core reads an empty source list as
    // every source, so a role nobody fills answers here, with nothing on it.
    let sources = match role {
        Some(role) => {
            let fillers = crate::sources::crud::fillers(pool, registry, role).await?;
            if fillers.is_empty() {
                return Ok(knobas_core::mini_board::MiniBoard {
                    columns: Vec::new(),
                    sources: Vec::new(),
                    lanes: Vec::new(),
                    more_lanes: 0,
                });
            }
            fillers
        }
        None => Vec::new(),
    };
    let declarations = declared_paths(pool, registry).await?;
    let (only, identity) = tickets_matching(pool, registry, query).await?;
    Ok(knobas_core::mini_board::read_scoped(
        pool,
        knobas_core::mini_board::Scope {
            ctx_id,
            sources: &sources,
            project,
            only: only.as_deref(),
            identity: &identity,
            lanes,
        },
        &declarations,
        per_column,
    )
    .await?)
}

/// The ticket ids a board's search admits (`None`: no search, nothing
/// narrowed) and the identity `@me` resolves to, from **one** vocabulary read:
/// the searcher loads it for the parse, and the board wants the identity out
/// of the same load rather than a second one (#1659). Without a search the
/// identity alone is read, and no searcher is built.
async fn tickets_matching(
    pool: &PgPool,
    registry: &dyn knobas_sync::scheduler::AdapterRegistry,
    query: Option<&str>,
) -> Result<(Option<Vec<String>>, Vec<String>), IpcError> {
    let Some(query) = query.map(str::trim).filter(|query| !query.is_empty()) else {
        return Ok((None, identity_of(pool).await?));
    };
    let searcher = crate::commands::search::searcher(pool, registry).await?;
    let vocabulary = searcher.vocabulary().await?;
    let only = searcher
        .matching_ids_in(&vocabulary, query, "ticket")
        .await?;
    Ok((only, vocabulary.identity))
}

/// A saved view's board, such as a Jira view's (#877, #1245, R-3.21, R-3.24):
/// the live tickets the source put on the view (`sync.saved_view_member`),
/// narrowed by `query` like [`mini_board_inner`], **in the source's order**,
/// under the view's own columns -- each holding the tickets whose status is
/// one of its statuses, empty ones included, as on the source's board. A
/// status on no column does not show. A view with no columns keeps one column
/// per status. A column's `status` is the view column's name; each card
/// carries its own status (#1423). `per_column` cuts each column after its first cards in that
/// order; `total` counts them all.
///
/// Keys the source returned that knobas does not mirror (a project the source
/// does not sync) are not on the board; they stay in the member table.
///
/// # Errors
///
/// `not_found` for a view the mirror does not hold; otherwise as
/// [`mini_board_inner`].
pub async fn saved_view_board(
    pool: &PgPool,
    registry: &dyn knobas_sync::scheduler::AdapterRegistry,
    view: &str,
    per_column: Option<u32>,
    query: Option<&str>,
) -> Result<knobas_core::mini_board::MiniBoard, IpcError> {
    saved_view_board_laned(pool, registry, view, per_column, query, None).await
}

/// [`saved_view_board`] in lanes (#939): each lane's column in the
/// source's order and capped at `per_column`, the lanes ranked as
/// [`knobas_core::mini_board::in_lanes`] ranks them.
///
/// # Errors
///
/// See [`saved_view_board`].
pub async fn saved_view_board_laned(
    pool: &PgPool,
    registry: &dyn knobas_sync::scheduler::AdapterRegistry,
    view: &str,
    per_column: Option<u32>,
    query: Option<&str>,
    lanes: Option<knobas_core::mini_board::Lanes>,
) -> Result<knobas_core::mini_board::MiniBoard, IpcError> {
    let Some(layout) = crate::ticket_views::saved_view_layout(pool, view).await? else {
        return Err(IpcError::not_found(format!(
            "no Jira view is called {view}"
        )));
    };
    let mut only = layout.members.clone();
    let (matched, identity) = tickets_matching(pool, registry, query).await?;
    if let Some(matched) = matched {
        let matched: std::collections::HashSet<String> = matched.into_iter().collect();
        only.retain(|id| matched.contains(id));
    }
    let declarations = declared_paths(pool, registry).await?;
    let statuses = knobas_core::mini_board::statuses_of(
        pool,
        &only,
        &declarations,
        lanes.map(|lanes| lanes.by),
    )
    .await?;
    let Some(lanes) = lanes else {
        let arranged = layout.arrange(statuses.columns, placed_id, per_column);
        return read_kept(pool, arranged, &statuses.sources, &identity, &declarations).await;
    };
    let arranged = layout.arrange(statuses.columns, placed_id, None);
    let (kept, more_lanes) = knobas_core::mini_board::lanes_of(
        arranged
            .iter()
            .map(|(status, tickets, _)| (status, tickets.as_slice())),
        |ticket| {
            (
                ticket.entity_id.as_str(),
                ticket.lane.as_str(),
                ticket.updated_at,
            )
        },
        lanes,
        per_column,
    );
    let in_a_lane: std::collections::HashSet<&str> = kept
        .iter()
        .flat_map(|lane| lane.columns.iter().flat_map(|cell| &cell.ids))
        .map(String::as_str)
        .collect();
    let arranged = arranged
        .into_iter()
        .map(|(status, tickets, total)| {
            let tickets = tickets
                .into_iter()
                .filter(|ticket| in_a_lane.contains(ticket.entity_id.as_str()))
                .collect();
            (status, tickets, total)
        })
        .collect();
    let mut board = read_kept(pool, arranged, &statuses.sources, &identity, &declarations).await?;
    board.lanes = kept;
    board.more_lanes = more_lanes;
    Ok(board)
}

/// A placed ticket's id, which a saved view's order ranks it by.
fn placed_id(ticket: &knobas_core::mini_board::Placed) -> &str {
    &ticket.entity_id
}

/// A board of `columns`, each its status, its cards and its total, with no
/// lanes.
fn unlaned(
    columns: Vec<(
        Option<String>,
        Vec<knobas_core::mini_board::MiniBoardCard>,
        u32,
    )>,
    sources: Vec<knobas_core::mini_board::SourceStatuses>,
) -> knobas_core::mini_board::MiniBoard {
    knobas_core::mini_board::MiniBoard {
        columns: columns
            .into_iter()
            .map(
                |(status, cards, total)| knobas_core::mini_board::MiniBoardColumn {
                    status,
                    cards,
                    total,
                },
            )
            .collect(),
        sources,
        lanes: Vec::new(),
        more_lanes: 0,
    }
}

/// A saved view's board from its columns already cut (Björn, 2026-10-26;
/// #2183): each ticket's status, and its lane, were read first
/// ([`knobas_core::mini_board::statuses_of`]), the view's columns and lanes
/// cut in the source's order, and only the tickets kept are read here
/// through the card statement. It read every ticket on the view as a card
/// and cut after: 0.6 s for a board of 2,250 tickets and 1.1 s for a filter
/// of 4,500, eight a column drawn. The answer is the same: the totals are
/// the statuses', and the sources' statuses are read again only when the
/// cut left one of `sources` with no card.
async fn read_kept(
    pool: &PgPool,
    arranged: Vec<(Option<String>, Vec<knobas_core::mini_board::Placed>, u32)>,
    sources: &std::collections::BTreeSet<String>,
    identity: &[String],
    declarations: &knobas_core::payload::Declarations,
) -> Result<knobas_core::mini_board::MiniBoard, IpcError> {
    let kept: Vec<String> = arranged
        .iter()
        .flat_map(|(_, tickets, _)| tickets.iter().map(|ticket| ticket.entity_id.clone()))
        .collect();
    let read = knobas_core::mini_board::read_scoped(
        pool,
        knobas_core::mini_board::Scope {
            only: Some(&kept),
            identity,
            ..knobas_core::mini_board::Scope::default()
        },
        declarations,
        None,
    )
    .await?;
    let mut cards: std::collections::HashMap<String, knobas_core::mini_board::MiniBoardCard> = read
        .columns
        .into_iter()
        .flat_map(|column| column.cards)
        .map(|card| (card.entity_id.clone(), card))
        .collect();
    let read_from: std::collections::BTreeSet<&str> =
        read.sources.iter().map(|s| s.source_id.as_str()).collect();
    let sources = if read_from
        .iter()
        .copied()
        .eq(sources.iter().map(String::as_str))
    {
        read.sources
    } else {
        knobas_core::mini_board::observed_statuses(pool, sources, declarations).await?
    };
    let columns = arranged
        .into_iter()
        .map(|(status, tickets, total)| {
            let cards = tickets
                .iter()
                .filter_map(|ticket| cards.remove(&ticket.entity_id))
                .collect();
            (status, cards, total)
        })
        .collect();
    Ok(unlaned(columns, sources))
}
