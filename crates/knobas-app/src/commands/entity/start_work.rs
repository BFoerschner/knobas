//! The start-work flow.

use super::*;

// -- the start-work flow ----------------------------------------------------
//
// Six commands, in this module and not a new one -- the `commands/` + `ipc/`
// layout is frozen, and the flow starts from a ticket *entity*, which is what
// this module is about. They are thin: everything they decide lives in
// `crate::start_work`, where a test can reach it without a Tauri app.
//
// **None of them is a write path of its own.** Every side effect the flow has
// is an existing `WriteOp` going through `submit_write`'s queue, or a link
// going through `create_link_inner` -- the same two doors the *Comment* button
// and the *Link to...* dialog use.

/// The flow for a ticket, proposing one if there is none and `repo_id` says
/// where it would go.
///
/// One command rather than a read and a create, because the address
/// `#/start-work/<key>` has to answer both questions at once: *is there a flow,
/// and if not, what would one look like?* An empty answer means there is no
/// flow and no repository was named -- which is the state where the view asks
/// the user to pick one.
///
/// Proposing does **not** dispatch anything. The whole sequence is composed and
/// stored so it can be shown before anything happens; `start_work_run` is what
/// performs it.
///
/// # Errors
///
/// `invalid` for an id that is not an entity id; `not_found` if the ticket is
/// not in the mirror; `not_ready` before bring-up.
#[tauri::command]
pub async fn start_work_flow(
    lifecycle: State<'_, Lifecycle>,
    entity_id: String,
    repo_id: Option<String>,
) -> Result<Vec<knobas_core::start_work::FlowStep>, IpcError> {
    let pool = lifecycle.pool()?;
    let ticket = EntityRef::parse(&entity_id).map_err(IpcError::invalid)?;
    let existing = knobas_core::start_work::flow(&pool, &ticket).await?;
    if !existing.is_empty() {
        return Ok(existing);
    }
    let Some(repo_id) = repo_id else {
        return Ok(Vec::new());
    };
    let repo = EntityRef::parse(&repo_id).map_err(IpcError::invalid)?;
    let declarations = crate::sources::paths::declared_paths(&pool, lifecycle.registry()).await?;
    crate::start_work::begin(&pool, &declarations, &ticket, &repo).await
}

/// Run the flow as far as it will go, **behind the answer** (#779): answers
/// the flow as it stands, and the steps run on in the background.
///
/// # Errors
///
/// `invalid` for an id that is not an entity id; `not_ready` before bring-up.
#[tauri::command]
pub async fn start_work_run<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    entity_id: String,
) -> Result<Vec<knobas_core::start_work::FlowStep>, IpcError> {
    let state = crate::sources::state(&app)?;
    let ticket = EntityRef::parse(&entity_id).map_err(IpcError::invalid)?;
    run_behind(&state, &ticket).await
}

/// The flow run on behind the answer (#779): the stepper draws each step as
/// it lands, by re-reading the flow while it moves.
async fn run_behind(
    state: &crate::sources::SourcesState,
    ticket: &EntityRef,
) -> Result<Vec<knobas_core::start_work::FlowStep>, IpcError> {
    crate::start_work::run_in_background(
        &state.pool,
        std::sync::Arc::new(crate::start_work::queue::Queue {
            state: state.clone(),
        }),
        ticket,
        state.scheduler.background(),
    )
    .await
}

/// Retry one step, without redoing the ones that succeeded. The flow runs on
/// from it behind the answer (#779). A queued step's *Check again* lands here
/// too: it re-reads the step's write and sends nothing (#1998, #2015).
///
/// # Errors
///
/// `not_found` if no step carries the id; `conflict` if an earlier step is
/// where the flow stopped; `not_ready` before bring-up.
#[tauri::command]
pub async fn start_work_retry<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    step_id: i64,
) -> Result<Vec<knobas_core::start_work::FlowStep>, IpcError> {
    let state = crate::sources::state(&app)?;
    let ticket = crate::start_work::ready_retry(
        &state.pool,
        &crate::start_work::queue::Queue { state: &*state },
        step_id,
    )
    .await?;
    run_behind(&state, &ticket).await
}

/// Skip one step, so a ticket that needs no branch still gets its status moved.
/// The flow runs on past it behind the answer (#779).
///
/// # Errors
///
/// `not_found` if no step carries the id; `not_ready` before bring-up.
#[tauri::command]
pub async fn start_work_skip<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    step_id: i64,
) -> Result<Vec<knobas_core::start_work::FlowStep>, IpcError> {
    let state = crate::sources::state(&app)?;
    let ticket = crate::start_work::ready_skip(&state.pool, step_id).await?;
    run_behind(&state, &ticket).await
}

/// Replace a step's proposal with the one the user edited.
///
/// `payload` is the step's own stored value, edited -- the same shape
/// `amend_write` takes, and untyped on the wire for the same reason: `WriteOp`
/// grows per milestone (ADR-0006), so typing the argument would drag the SPI's
/// enum onto the IPC surface.
///
/// # Errors
///
/// `not_found` if no step carries the id; `conflict` if the step has already
/// happened; `not_ready` before bring-up.
#[tauri::command]
pub async fn start_work_amend(
    lifecycle: State<'_, Lifecycle>,
    step_id: i64,
    payload: serde_json::Value,
) -> Result<Vec<knobas_core::start_work::FlowStep>, IpcError> {
    let pool = lifecycle.pool()?;
    crate::start_work::repropose(&pool, step_id, payload).await
}

/// The reverse direction: move every ticket whose linked pull request has been
/// merged, and answer how many moved.
///
/// A pass over the mirror, asked for by a command -- there is no per-item hook
/// in the sync engine, and adding one to serve this would be a second
/// mechanism. The count is what the shell announces, so an automatic
/// change is visible rather than mysterious.
///
/// # Errors
///
/// `not_ready` before bring-up; `internal` if the pass's own read fails.
#[tauri::command]
pub async fn follow_merges<R: tauri::Runtime>(app: tauri::AppHandle<R>) -> Result<u32, IpcError> {
    let state = crate::sources::state(&app)?;
    let declarations =
        crate::sources::paths::declared_paths(&state.pool, state.registry.as_ref()).await?;
    crate::start_work::merge::follow_merges(
        &state.pool,
        &crate::start_work::queue::Queue { state: &*state },
        crate::start_work::plan::IN_REVIEW,
        &declarations,
    )
    .await
}
