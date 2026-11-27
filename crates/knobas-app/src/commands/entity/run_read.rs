//! The marks a person's runs carry (ux18, migrations `0098` and `0105`,
//! ADR-0063, `knobas_core::run_read`): the set the shell holds, the marks by
//! scope, the mark opening a run writes, and the counts by bubble the top
//! bar, the rail and the rows draw, with the instants green counts from
//! (#2568).

use super::*;

pub use knobas_core::run_read::{MarkScope, RunCounts, RunMarks, SeenMove};

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

/// The live runs that carry a mark: those it reads, and those marked while
/// they ran, which it takes out of the blue only (ADR-0063).
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) while the database is coming
/// up, [`Internal`](crate::IpcErrorCode::Internal) for a query failure.
#[tauri::command]
pub async fn read_runs(lifecycle: State<'_, Lifecycle>) -> Result<RunMarks, IpcError> {
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
    scope: &MarkScope,
) -> Result<(RunMarks, Option<knobas_core::activity::ActivityRow>), IpcError> {
    if scope.reaches_pipelines() {
        // A pipeline's runs are read from `sync.item_pipeline`, resolved from
        // the stored declarations, which every reader of it writes first
        // (#2185), as `pipelines` does.
        crate::commands::search::declare(pool, registry).await?;
    }
    // Every mark records whether its run had finished, so even a mark by id
    // reads the run's state under the declarations.
    let declarations = declared_paths(pool, registry).await?;
    let written = knobas_core::run_read::mark_read(pool, &declarations, scope).await?;
    let ids: Vec<String> = written
        .read
        .iter()
        .chain(&written.while_running)
        .cloned()
        .collect();
    let line = if ids.is_empty() {
        None
    } else {
        Some(record_mark(pool, "marked read", &ids).await?)
    };
    Ok((written, line))
}

/// Mark runs read by scope (ADR-0063): every run, the runs of some
/// pipelines, runs by id. Of a pipeline's runs it takes each running one out
/// of the blue and reads each finished failure; a run by id is marked
/// whatever its state. Answers the marks it wrote, which is what *Undo* hands
/// back to [`mark_runs_unread`]: a run already read is not among them.
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) while the database is coming
/// up, [`Internal`](crate::IpcErrorCode::Internal) for a query failure.
#[tauri::command]
pub async fn mark_runs_read<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    lifecycle: State<'_, Lifecycle>,
    scope: MarkScope,
) -> Result<RunMarks, IpcError> {
    let pool = lifecycle.pool()?;
    let (written, line) = mark_runs_read_inner(&pool, lifecycle.registry(), &scope).await?;
    if let Some(line) = line {
        crate::events::emit(&app, None, crate::events::ACTIVITY_NEW, &line);
    }
    Ok(written)
}

/// Whether opening a run writes an activity line (Björn, 2026-11-27: not for
/// now; only a *Mark … as read* writes one). Without one the window that
/// opened it re-reads the Inbox and the top bar itself (`runs-read.svelte.ts`).
const OPENING_WRITES_A_LINE: bool = false;

/// [`mark_run_opened`] without the app handle: the write, and its line where
/// [`OPENING_WRITES_A_LINE`].
///
/// # Errors
///
/// As [`mark_run_opened`].
pub async fn mark_run_opened_inner(
    pool: &PgPool,
    registry: &dyn knobas_sync::scheduler::AdapterRegistry,
    run: &str,
) -> Result<(bool, Option<knobas_core::activity::ActivityRow>), IpcError> {
    let declarations = declared_paths(pool, registry).await?;
    let wrote = knobas_core::run_read::mark_opened(pool, &declarations, run).await?;
    let line = if wrote && OPENING_WRITES_A_LINE {
        Some(record_mark(pool, "opened", &[run.to_owned()]).await?)
    } else {
        None
    };
    Ok((wrote, line))
}

/// The run's page was shown (ADR-0063): its mark becomes *opened*, which
/// reads it whatever it does after, and its Inbox *failed build* line goes
/// with it. Answers whether it wrote: not when a mark already read the run.
/// It writes no activity line ([`OPENING_WRITES_A_LINE`]): the window that
/// opened it re-reads the Inbox and the top bar on the answer.
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) while the database is coming
/// up, [`Internal`](crate::IpcErrorCode::Internal) for a query failure.
#[tauri::command]
pub async fn mark_run_opened<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    lifecycle: State<'_, Lifecycle>,
    run: String,
) -> Result<bool, IpcError> {
    let pool = lifecycle.pool()?;
    let (wrote, line) = mark_run_opened_inner(&pool, lifecycle.registry(), &run).await?;
    if let Some(line) = line {
        crate::events::emit(&app, None, crate::events::ACTIVITY_NEW, &line);
    }
    Ok(wrote)
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

/// Mark runs unread: their marks go, however they were got, and each one's
/// Inbox *Done* with them, so a failed build's line comes back. Answers the
/// ids that had a mark.
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

/// [`run_counts`] without the state.
///
/// # Errors
///
/// As [`run_counts`].
pub async fn run_counts_inner(
    pool: &PgPool,
    registry: &dyn knobas_sync::scheduler::AdapterRegistry,
) -> Result<RunCounts, IpcError> {
    // A pipeline's runs are read from `sync.item_pipeline`, resolved from the
    // stored declarations, which every reader of it writes first (#2185), as
    // `pipelines` does.
    crate::commands::search::declare(pool, registry).await?;
    let declarations = declared_paths(pool, registry).await?;
    let fillers = crate::sources::crud::fillers(pool, registry, knobas_source::role::CI).await?;
    Ok(knobas_core::run_read::run_counts(pool, &declarations, &fillers).await?)
}

/// The unread runs of every pipeline the Builds list draws, by bubble
/// (spec #2566, #2568, ADR-0063): blue running, green passed, yellow passed
/// with warnings, red failed, grey cancelled. What the top bar, the rail and
/// the rows draw; a folder sums its pipelines. Replaces
/// `unread_failing_builds`, which counted pipelines whose newest run failed.
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) while the database is coming
/// up, [`Internal`](crate::IpcErrorCode::Internal) for a query failure.
#[tauri::command]
pub async fn run_counts(lifecycle: State<'_, Lifecycle>) -> Result<RunCounts, IpcError> {
    let pool = lifecycle.pool()?;
    run_counts_inner(&pool, lifecycle.registry()).await
}

/// A scope's *Mark all as read* on green, yellow and grey (ADR-0063): the
/// scope's instant moves to now, the one instant for *everything*, each
/// pipeline's own for a set of pipelines. Beside [`mark_runs_read`], which
/// writes the rows of red and blue. Answers what it moved, for *Undo*
/// ([`put_back_seen`]); nothing for runs by id alone.
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) while the database is coming
/// up, [`Internal`](crate::IpcErrorCode::Internal) for a query failure.
#[tauri::command]
pub async fn move_seen(
    lifecycle: State<'_, Lifecycle>,
    scope: MarkScope,
) -> Result<Option<SeenMove>, IpcError> {
    let pool = lifecycle.pool()?;
    Ok(knobas_core::run_read::move_seen(&pool, &scope).await?)
}

/// *Undo* of [`move_seen`]: each instant it moved goes back, unless leaving
/// Builds has moved it since. Answers whether any went back.
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) while the database is coming
/// up, [`Internal`](crate::IpcErrorCode::Internal) for a query failure.
#[tauri::command]
pub async fn put_back_seen(
    lifecycle: State<'_, Lifecycle>,
    seen: SeenMove,
) -> Result<bool, IpcError> {
    let pool = lifecycle.pool()?;
    Ok(knobas_core::run_read::put_back_seen(&pool, &seen).await?)
}

/// Builds came in front, or stopped being in front (ADR-0063, *The
/// instant*): the shell calls it on each section change to or from Builds,
/// and once at its start when it is not on Builds, which ends a visit a
/// crash left open. Leaving moves the instant green counts from to now;
/// coming in front holds the visit's start, so the visit's green stays drawn
/// (question 2 on #2554). The quit while on Builds leaves it too
/// ([`leave_builds_at_quit`]). Answers whether it wrote.
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) while the database is coming
/// up, [`Internal`](crate::IpcErrorCode::Internal) for a query failure.
#[tauri::command]
pub async fn builds_in_front(
    lifecycle: State<'_, Lifecycle>,
    front: bool,
) -> Result<bool, IpcError> {
    let pool = lifecycle.pool()?;
    Ok(knobas_core::run_read::builds_in_front(&pool, front).await?)
}

/// The quit while Builds is in front leaves it (ADR-0063): the exit handler
/// calls this before the database stops. A quit before the database was up
/// has nothing to write. Once a process: the exit handler runs on both
/// `ExitRequested` and `Exit`, and the second finds the database stopped.
/// Held to a second, so a database that stopped answering never holds the
/// quit.
pub fn leave_builds_at_quit<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    use tauri::Manager as _;
    static LEFT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if LEFT.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return;
    }
    let Some(lifecycle) = app.try_state::<Lifecycle>() else {
        return;
    };
    let Ok(pool) = lifecycle.pool() else {
        return;
    };
    // The timer is made inside the runtime: one made outside it has no
    // reactor to run on.
    let left = tauri::async_runtime::block_on(async {
        tokio::time::timeout(
            std::time::Duration::from_secs(1),
            knobas_core::run_read::builds_in_front(&pool, false),
        )
        .await
    });
    match left {
        Ok(Ok(_)) => {}
        Ok(Err(e)) => tracing::warn!(error = %e, "the quit could not leave Builds"),
        Err(_) => tracing::warn!("the quit gave up leaving Builds after a second"),
    }
}
