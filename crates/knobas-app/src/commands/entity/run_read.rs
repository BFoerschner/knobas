//! The failed runs the person marked as read (ux18, migration `0098`,
//! `knobas_core::run_read`): the set the shell holds, the two marks, and the
//! top bar's count of what still asks.

use super::*;

/// The activity line a mark writes: one per press, so the Inbox and the top
/// bar re-read on its `activity:new`, and the log says what happened.
async fn record_mark(
    pool: &PgPool,
    verb: &str,
    runs: &[String],
) -> Result<knobas_core::activity::ActivityRow, IpcError> {
    let entity = match runs {
        [one] => EntityRef::parse(one).ok(),
        _ => None,
    };
    knobas_core::activity::record(
        pool,
        "user",
        verb,
        entity.as_ref(),
        serde_json::json!({ "runs": runs.len() }),
    )
    .await
    .map_err(IpcError::internal)
}

/// The ids of the live runs marked read.
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) while the database is coming
/// up, [`Internal`](crate::IpcErrorCode::Internal) for a query failure.
#[tauri::command]
pub async fn read_runs(lifecycle: State<'_, Lifecycle>) -> Result<Vec<String>, IpcError> {
    let pool = lifecycle.pool()?;
    Ok(knobas_core::run_read::read_runs(&pool).await?)
}

/// [`mark_runs_read`] without the app handle: the write and its line.
///
/// # Errors
///
/// As [`mark_runs_read`].
pub async fn mark_runs_read_inner(
    pool: &PgPool,
    registry: &dyn knobas_sync::scheduler::AdapterRegistry,
    runs: &[String],
    pipelines: &[String],
) -> Result<(Vec<String>, Option<knobas_core::activity::ActivityRow>), IpcError> {
    let declarations = if pipelines.is_empty() {
        knobas_core::payload::Declarations::default()
    } else {
        // A pipeline's runs are read from `sync.item_pipeline`, resolved from
        // the stored declarations, which every reader of it writes first
        // (#2185), as `pipelines` does.
        crate::commands::search::declare(pool, registry).await?;
        declared_paths(pool, registry).await?
    };
    let written = knobas_core::run_read::mark_read(pool, &declarations, runs, pipelines).await?;
    let line = if written.is_empty() {
        None
    } else {
        Some(record_mark(pool, "marked read", &written).await?)
    };
    Ok((written, line))
}

/// Mark runs read: `runs` by id, and every finished failed run of each of
/// `pipelines`. Answers the ids it wrote, which is what *Undo* hands back to
/// [`mark_runs_unread`]: a run already read is not among them.
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) while the database is coming
/// up, [`Internal`](crate::IpcErrorCode::Internal) for a query failure.
#[tauri::command]
pub async fn mark_runs_read<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    lifecycle: State<'_, Lifecycle>,
    runs: Vec<String>,
    pipelines: Vec<String>,
) -> Result<Vec<String>, IpcError> {
    let pool = lifecycle.pool()?;
    let (written, line) =
        mark_runs_read_inner(&pool, lifecycle.registry(), &runs, &pipelines).await?;
    if let Some(line) = line {
        crate::events::emit(&app, None, crate::events::ACTIVITY_NEW, &line);
    }
    Ok(written)
}

/// [`mark_runs_unread`] without the app handle.
///
/// # Errors
///
/// As [`mark_runs_unread`].
pub async fn mark_runs_unread_inner(
    pool: &PgPool,
    runs: &[String],
) -> Result<(Vec<String>, Option<knobas_core::activity::ActivityRow>), IpcError> {
    let gone = knobas_core::run_read::mark_unread(pool, runs).await?;
    let line = if gone.is_empty() {
        None
    } else {
        Some(record_mark(pool, "marked unread", &gone).await?)
    };
    Ok((gone, line))
}

/// Mark runs unread, and take back each one's Inbox *Done*, so a failed
/// build's line comes back. Answers the ids that were read.
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) while the database is coming
/// up, [`Internal`](crate::IpcErrorCode::Internal) for a query failure.
#[tauri::command]
pub async fn mark_runs_unread<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    lifecycle: State<'_, Lifecycle>,
    runs: Vec<String>,
) -> Result<Vec<String>, IpcError> {
    let pool = lifecycle.pool()?;
    let (gone, line) = mark_runs_unread_inner(&pool, &runs).await?;
    if let Some(line) = line {
        crate::events::emit(&app, None, crate::events::ACTIVITY_NEW, &line);
    }
    Ok(gone)
}

/// The most pipelines the count reads: the newest run of each, failures
/// first, so a number past it is a lower bound.
const PIPELINES_COUNTED: u32 = 2_000;

/// [`unread_failing_builds`] without the state.
///
/// # Errors
///
/// As [`unread_failing_builds`].
pub async fn unread_failing_builds_inner(
    pool: &PgPool,
    registry: &dyn knobas_sync::scheduler::AdapterRegistry,
) -> Result<u32, IpcError> {
    let declarations = declared_paths(pool, registry).await?;
    let filter = knobas_core::entity_list::EntityFilter {
        sources: Vec::new(),
        kinds: vec!["build".to_owned()],
        updated_within_days: None,
        context: None,
        project: None,
        order: knobas_core::entity_list::EntityOrder::LatestPerParent,
        include_deleted: false,
        reach: None,
        role: None,
        parent: None,
    };
    let page =
        list_entities_inner(pool, registry, &filter, PIPELINES_COUNTED, 0, &declarations).await?;
    let failing: Vec<String> = page
        .rows
        .into_iter()
        .filter(|row| {
            row.state
                .as_ref()
                .is_some_and(|state| state.tone == Some(knobas_core::tone::Tone::Err))
        })
        .map(|row| row.entity_id)
        .collect();
    if failing.is_empty() {
        return Ok(0);
    }
    let read = knobas_core::run_read::count_read(pool, &failing).await?;
    let failing = i64::try_from(failing.len()).unwrap_or(i64::MAX);
    Ok(u32::try_from((failing - read).max(0)).unwrap_or(u32::MAX))
}

/// The top bar's Builds count: pipelines whose newest run failed and is not
/// read (ux18). The room's Builds tile still counts every failing one, read
/// or not: there it is a fact about the work, here what still asks.
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) while the database is coming
/// up, [`Internal`](crate::IpcErrorCode::Internal) for a query failure.
#[tauri::command]
pub async fn unread_failing_builds(lifecycle: State<'_, Lifecycle>) -> Result<u32, IpcError> {
    let pool = lifecycle.pool()?;
    unread_failing_builds_inner(&pool, lifecycle.registry()).await
}
