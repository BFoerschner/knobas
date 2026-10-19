//! Suggestions and the room tray.

use super::*;

// -- suggestions and the room tray -----------------------------------------
//
// Four commands, and #836's evidence read, in this module and not a new one:
// a suggestion **is** a link row (#41), these are the writes that move it
// between the two states the link store already has, and the reads are the
// same graph the panel above reads from the other side. A
// `commands/suggest.rs` would be a second module over one table.

/// One page of a room's tray: the proposals, and how many there are in all.
///
/// `total` is the whole room, not the page -- #41's story 19 is "see how many
/// suggestions are waiting, so that I can choose when to spend attention on
/// them", and a number capped by `limit` answers a different question. Same
/// shape and same reason as [`EntityPage`].
#[derive(Debug, Clone, serde::Serialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct SuggestionPage {
    pub rows: Vec<SuggestionEntry>,
    /// Every proposal in the room, before `limit`.
    ///
    /// The number the tray's heading shows.
    pub total: i64,
    /// Items holding more proposals than the tray draws for one item (#774),
    /// most hidden first. The bound is applied by `suggest::proposals`.
    ///
    /// The tray draws three per item.
    pub more: Vec<suggest::SuggestionMore>,
}

/// Run a detection pass, returning how many proposals it wrote.
///
/// Idempotent, and cheap to repeat since #745: a pass over an unchanged mirror
/// writes nothing and compares nothing, and a pass after a sync writes only
/// what the new items justify. It never creates a confirmed link and never
/// writes to a source.
///
/// Through the process's one [`SuggestionPass`]: a call while a pass runs is
/// answered by the next pass rather than starting a third, and the pass runs
/// on a connection of its own, never on the UI pool (#745).
///
/// [`SuggestionPass`]: crate::suggestion_pass::SuggestionPass
///
/// # Errors
///
/// [`Internal`](crate::IpcErrorCode::Internal) if a statement fails or runs
/// past the pass's statement timeout.
pub async fn detect_suggestions_inner(
    pass: &crate::suggestion_pass::SuggestionPass,
) -> Result<u32, IpcError> {
    let written = pass.run().await?;
    // A pass that proposed more than four billion links is not a number the
    // frontend needs to be exact about.
    Ok(u32::try_from(written).unwrap_or(u32::MAX))
}

/// The proposals a room holds, newest first, with both ends resolved.
///
/// `sources` is a derived room's membership and follows [`EntityFilter`]'s
/// convention: **empty means every source**. A proposal belongs to a room when
/// either of its ends does.
///
/// `ctx` is a stored context's room (#47): its membership -- the fixed
/// one-hop rule, resolved by the store that owns it -- plus the context's own
/// entity, so a proposed *add to this context* surfaces in the room it would
/// add to. Membership is computed from confirmed links only (ADR-0008), which
/// is what keeps "the tray scopes by membership" from becoming circular with
/// "membership is built from links".
///
/// # Errors
///
/// [`Internal`](crate::IpcErrorCode::Internal) if a query fails.
pub async fn room_suggestions_inner(
    pool: &PgPool,
    sources: &[String],
    ctx: Option<&str>,
    limit: u32,
) -> Result<SuggestionPage, IpcError> {
    let members = match ctx {
        Some(ctx) => {
            let mut ids = knobas_core::context::member_ids(pool, ctx).await?;
            ids.push(ctx.to_owned());
            Some(ids)
        }
        None => None,
    };
    let members = members.as_deref();
    // The count reads the whole room and the rows read its newest end: two
    // connections at once, so the tray waits for the slower and not the sum.
    let drawn = async {
        let rows = suggest::proposals(pool, sources, members, i64::from(limit)).await?;
        let more = suggest::proposal_overflow(pool, sources, members, &rows).await?;
        Ok::<_, knobas_core::CoreError>((rows, more))
    };
    let ((rows, more), total) =
        tokio::try_join!(drawn, suggest::proposal_count(pool, sources, members))?;
    Ok(SuggestionPage { rows, total, more })
}

/// Accept a proposal, making it an ordinary link.
///
/// `Ok(None)` when there was nothing to accept -- it was accepted or dismissed
/// already. Nothing was mutated, so nothing is written to the log and nothing
/// is announced, exactly as [`unlink_inner`] does.
///
/// # Errors
///
/// [`Invalid`](crate::IpcErrorCode::Invalid) if `link_id` is not a UUID;
/// [`NotFound`](crate::IpcErrorCode::NotFound) if no link carries it;
/// [`Internal`](crate::IpcErrorCode::Internal) for a query failure.
pub async fn accept_suggestion_inner(
    pool: &PgPool,
    link_id: &str,
) -> Result<Option<LinkMutation>, IpcError> {
    let id = suggestion_id(link_id)?;
    let Some(link) = suggest::accept(pool, id).await? else {
        return Ok(None);
    };
    let activity = record_link_activity(pool, "accepted", &link).await?;
    Ok(Some(LinkMutation {
        link,
        activity,
        superseded: None,
    }))
}

/// Dismiss a proposal, and remember it.
///
/// The tombstone is the withdrawal memory Links v1 already had, so a dismissal
/// suppresses re-proposal for exactly the same reason an unlink does -- there
/// is one mechanism, not two.
///
/// `Ok(None)` when there was nothing to dismiss, which includes a link that has
/// already been accepted: that is a link, and the panel's *Unlink* is what
/// withdraws it.
///
/// # Errors
///
/// As [`accept_suggestion_inner`].
pub async fn dismiss_suggestion_inner(
    pool: &PgPool,
    link_id: &str,
) -> Result<Option<LinkMutation>, IpcError> {
    let id = suggestion_id(link_id)?;
    let Some(link) = suggest::dismiss(pool, id).await? else {
        return Ok(None);
    };
    let activity = record_link_activity(pool, "dismissed", &link).await?;
    Ok(Some(LinkMutation {
        link,
        activity,
        superseded: None,
    }))
}

/// What was said for and against one proposal (#836): the stored reason, the
/// text it was found in with the match marked, what supports it and what
/// speaks against it. The Proposed links view reads it for the proposal open.
///
/// A read beside `room_suggestions`, not a field on its rows: the tray's
/// ranking and read are unchanged, and only the open proposal pays for the
/// text scan (`knobas_core::evidence`).
///
/// # Errors
///
/// As [`accept_suggestion_inner`].
pub async fn proposal_evidence_inner(
    pool: &PgPool,
    link_id: &str,
) -> Result<knobas_core::evidence::ProposalEvidence, IpcError> {
    let id = suggestion_id(link_id)?;
    Ok(knobas_core::evidence::proposal_evidence(pool, id).await?)
}

/// A suggestion is addressed by its link id, because it is a link row.
fn suggestion_id(link_id: &str) -> Result<Uuid, IpcError> {
    link_id
        .parse()
        .map_err(|_| IpcError::invalid(format!("{link_id} is not a link id")))
}

/// Run a detection pass over the mirror.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) while the database
/// is still coming up, and whatever [`detect_suggestions_inner`] refuses with.
#[tauri::command]
pub async fn detect_suggestions(lifecycle: State<'_, Lifecycle>) -> Result<u32, IpcError> {
    detect_suggestions_inner(&lifecycle.suggestions()?).await
}

/// One page of a room's suggestion tray.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) while the database
/// is still coming up, and whatever [`room_suggestions_inner`] refuses with.
#[tauri::command]
pub async fn room_suggestions(
    lifecycle: State<'_, Lifecycle>,
    sources: Vec<String>,
    ctx: Option<String>,
    limit: u32,
) -> Result<SuggestionPage, IpcError> {
    let pool = lifecycle.pool()?;
    room_suggestions_inner(&pool, &sources, ctx.as_deref(), limit).await
}

/// Accept a proposal, and announce it.
///
/// Idempotent: accepting an already-accepted suggestion resolves, writes no
/// second line and announces nothing.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) while the database
/// is still coming up, and whatever [`accept_suggestion_inner`] refuses with.
#[tauri::command]
pub async fn accept_suggestion<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    lifecycle: State<'_, Lifecycle>,
    link_id: String,
) -> Result<(), IpcError> {
    let pool = lifecycle.pool()?;
    if let Some(written) = accept_suggestion_inner(&pool, &link_id).await? {
        announce(&app, written.activity);
    }
    Ok(())
}

/// Dismiss a proposal, and announce it.
///
/// Idempotent, for the same reason [`accept_suggestion`] is.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) while the database
/// is still coming up, and whatever [`dismiss_suggestion_inner`] refuses with.
#[tauri::command]
pub async fn dismiss_suggestion<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    lifecycle: State<'_, Lifecycle>,
    link_id: String,
) -> Result<(), IpcError> {
    let pool = lifecycle.pool()?;
    if let Some(written) = dismiss_suggestion_inner(&pool, &link_id).await? {
        announce(&app, written.activity);
    }
    Ok(())
}

/// What was said for and against one proposal (#836).
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) while the database
/// is still coming up, and whatever [`proposal_evidence_inner`] refuses with.
#[tauri::command]
pub async fn proposal_evidence(
    lifecycle: State<'_, Lifecycle>,
    link_id: String,
) -> Result<knobas_core::evidence::ProposalEvidence, IpcError> {
    let pool = lifecycle.pool()?;
    proposal_evidence_inner(&pool, &link_id).await
}
