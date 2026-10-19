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
        return saved_view_board_laned(&pool, &view, per_column, query.as_deref(), lanes).await;
    }
    mini_board_laned(
        &pool,
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
    ctx_id: Option<&str>,
    role: Option<&str>,
    project: Option<&str>,
    per_column: Option<u32>,
    query: Option<&str>,
) -> Result<knobas_core::mini_board::MiniBoard, IpcError> {
    mini_board_laned(
        pool,
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
            let fillers =
                crate::sources::crud::fillers(pool, &crate::sources::Registry::for_process(), role)
                    .await?;
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
    let declarations = declared_paths(pool, &crate::sources::Registry::for_process()).await?;
    let only = match query.map(str::trim).filter(|query| !query.is_empty()) {
        Some(query) => {
            crate::commands::search::searcher(pool, &crate::sources::Registry::for_process())
                .await?
                .matching_ids(query, "ticket")
                .await?
        }
        None => None,
    };
    let identity = knobas_search::Vocabulary::load(pool, knobas_search::KindCatalog::default())
        .await?
        .identity;
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

/// A saved view's board, such as a Jira view's (#877, #1245, R-3.21, R-3.24):
/// the live tickets the source put on the view (`sync.saved_view_member`),
/// narrowed by `query` like [`mini_board_inner`], **in the source's order**,
/// under the view's own columns -- each holding the tickets whose status is
/// one of its statuses, empty ones included, as on the source's board. A
/// status on no column does not show. A view with no columns keeps one column
/// per status. `per_column` cuts each column after its first cards in that
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
    view: &str,
    per_column: Option<u32>,
    query: Option<&str>,
) -> Result<knobas_core::mini_board::MiniBoard, IpcError> {
    saved_view_board_laned(pool, view, per_column, query, None).await
}

/// [`saved_view_board`] in lanes (#939): the whole view put in lanes after it
/// is read ([`knobas_core::mini_board::in_lanes`]), each lane's column in
/// the source's order and capped at `per_column`.
///
/// # Errors
///
/// See [`saved_view_board`].
pub async fn saved_view_board_laned(
    pool: &PgPool,
    view: &str,
    per_column: Option<u32>,
    query: Option<&str>,
    lanes: Option<knobas_core::mini_board::Lanes>,
) -> Result<knobas_core::mini_board::MiniBoard, IpcError> {
    use knobas_core::mini_board::{MiniBoard, MiniBoardCard, MiniBoardColumn};
    let (per_column, lanes_cap) = match lanes {
        Some(_) => (None, per_column),
        None => (per_column, None),
    };
    let columns: Option<serde_json::Value> =
        sqlx::query_scalar("select columns from sync.saved_view where entity_id = $1")
            .bind(view)
            .fetch_optional(pool)
            .await?;
    let Some(columns) = columns else {
        return Err(IpcError::not_found(format!(
            "no Jira view is called {view}"
        )));
    };
    let columns: Vec<crate::ticket_views::ViewColumn> =
        serde_json::from_value(columns).map_err(IpcError::internal)?;
    let members: Vec<String> = sqlx::query_scalar(
        "select entity_id from sync.saved_view_member where view_id = $1 order by rank",
    )
    .bind(view)
    .fetch_all(pool)
    .await?;
    let rank: std::collections::HashMap<&str, usize> = members
        .iter()
        .enumerate()
        .map(|(at, id)| (id.as_str(), at))
        .collect();
    let mut only = members.clone();
    if let Some(query) = query.map(str::trim).filter(|query| !query.is_empty())
        && let Some(matched) =
            crate::commands::search::searcher(pool, &crate::sources::Registry::for_process())
                .await?
                .matching_ids(query, "ticket")
                .await?
    {
        let matched: std::collections::HashSet<String> = matched.into_iter().collect();
        only.retain(|id| matched.contains(id));
    }
    let declarations = declared_paths(pool, &crate::sources::Registry::for_process()).await?;
    let identity = knobas_search::Vocabulary::load(pool, knobas_search::KindCatalog::default())
        .await?
        .identity;
    let whole = knobas_core::mini_board::read_scoped(
        pool,
        knobas_core::mini_board::Scope {
            only: Some(&only),
            identity: &identity,
            ..knobas_core::mini_board::Scope::default()
        },
        &declarations,
        None,
    )
    .await?;
    let in_order = |mut cards: Vec<MiniBoardCard>| -> MiniBoardColumn {
        cards.sort_by_key(|card| {
            rank.get(card.entity_id.as_str())
                .copied()
                .unwrap_or(usize::MAX)
        });
        let total = u32::try_from(cards.len()).unwrap_or(u32::MAX);
        if let Some(cap) = per_column {
            cards.truncate(cap as usize);
        }
        MiniBoardColumn {
            status: None,
            cards,
            total,
        }
    };
    let drawn: Vec<MiniBoardColumn> = if columns.is_empty() {
        whole
            .columns
            .into_iter()
            .map(|column| MiniBoardColumn {
                status: column.status,
                ..in_order(column.cards)
            })
            .collect()
    } else {
        let mut by_status: std::collections::HashMap<Option<String>, Vec<MiniBoardCard>> = whole
            .columns
            .into_iter()
            .map(|column| (column.status, column.cards))
            .collect();
        columns
            .iter()
            .map(|column| MiniBoardColumn {
                status: Some(column.name.clone()),
                ..in_order(
                    column
                        .statuses
                        .iter()
                        .flat_map(|status| {
                            by_status.remove(&Some(status.clone())).unwrap_or_default()
                        })
                        .collect(),
                )
            })
            .collect()
    };
    let board = MiniBoard {
        columns: drawn,
        sources: whole.sources,
        lanes: Vec::new(),
        more_lanes: 0,
    };
    Ok(match lanes {
        Some(lanes) => knobas_core::mini_board::in_lanes(board, lanes, lanes_cap),
        None => board,
    })
}

/// The statuses one source's live ticket corpus shows (#760): the ticket
/// detail's fallback offer when the workflow cannot be read (#179, #498).
///
/// Asked only then. It used to come out of a whole [`mini_board`] read of the
/// ticket's source on every open (133-137 ms and 725 KB at 5,000 tickets);
/// the current status is [`EntityDetail::status`] now.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) while the
/// database is still coming up; a query failure is `Internal`.
#[tauri::command]
pub async fn source_statuses(
    lifecycle: State<'_, Lifecycle>,
    source_id: String,
) -> Result<Vec<String>, IpcError> {
    let pool = lifecycle.pool()?;
    let declarations = declared_paths(&pool, &crate::sources::Registry::for_process()).await?;
    Ok(knobas_core::mini_board::source_statuses(&pool, &source_id, &declarations).await?)
}
