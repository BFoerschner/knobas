//! One *logged* run: the composition around [`run_once`](crate::run_once).
//!
//! [`run_once`](crate::run_once) is the write side and nothing else -- pull,
//! upsert, advance the cursor. Everything a run needs *around* that is here:
//! announce the start, classify the failure, close the `knobas.sync_run` row
//! either way, and report the phases to whoever is listening.
//!
//! It lives in this crate rather than in the app shell because stream F's
//! scheduler is the main caller and must be able to extend it without
//! importing `knobas-app`'s demo module. The app's *Sync now* is one caller of
//! it, not its owner.
//!
//! The log row is opened by the caller ([`run_log::start`]) rather than here,
//! and that split is load-bearing: `sync_now` returns the run id **before** the
//! run executes (ruling P3), so the id has to exist while the run is still in
//! front of us.

use std::time::Instant;

use knobas_source::{Cursor, Source};
use sqlx::PgPool;

use crate::progress::{ProgressSink, SyncPhase, SyncProgress};
use crate::run_log::{self, RunResult};
use crate::{SyncError, SyncReport};

/// Execute one run whose log row is already open, and close it.
///
/// `run_id` comes from [`run_log::start`]. Whichever way the run ends the row
/// is closed: `ok` with the counts, or the [`SyncOutcome`] the failure
/// classifies as plus its message. A failed *log write* on the failure path is
/// logged and swallowed -- the run's own error is what the caller must hear,
/// and replacing a diagnosable adapter failure with a database one would hide
/// it.
///
/// # Progress
///
/// [`SyncPhase::Started`] and then [`SyncPhase::Finished`] or
/// [`SyncPhase::Failed`]. [`SyncPhase::Fetching`] and [`SyncPhase::Writing`]
/// are deliberately **absent rather than faked**: `run_once` is one call with
/// no observable middle, and a `Fetching` message corresponding to no fetch
/// would make a progress bar lie about what it is watching. Emitting them from
/// inside the run, where they are true, is stream F's.
///
/// # Errors
///
/// Whatever the run failed with ([`SyncError`]); the log row is closed first.
pub async fn run(
    pool: &PgPool,
    source: &dyn Source,
    cursor: Option<Cursor>,
    run_id: i64,
    progress: Option<&dyn ProgressSink>,
) -> Result<SyncReport, SyncError> {
    let source_id = source.descriptor().id;
    let started = Instant::now();
    report(
        progress,
        run_id,
        &source_id,
        SyncPhase::Started,
        0,
        started,
        None,
    );

    match crate::run_once(pool, source, cursor).await {
        Ok(done) => {
            run_log::finish(pool, run_id, &RunResult::ok(&done)).await?;
            report(
                progress,
                run_id,
                &source_id,
                SyncPhase::Finished,
                done.upserted,
                started,
                None,
            );
            Ok(done)
        }
        Err(error) => {
            let message = error.to_string();
            if let Err(log_error) = run_log::finish(pool, run_id, &RunResult::failed(&error)).await
            {
                tracing::warn!(run_id, %log_error, "the run failed, and so did logging it");
            }
            report(
                progress,
                run_id,
                &source_id,
                SyncPhase::Failed,
                0,
                started,
                Some(message),
            );
            Err(error)
        }
    }
}

/// Send one progress message, if anyone is listening.
fn report(
    sink: Option<&dyn ProgressSink>,
    run_id: i64,
    source_id: &str,
    phase: SyncPhase,
    items: u64,
    started: Instant,
    message: Option<String>,
) {
    if let Some(sink) = sink {
        sink.report(SyncProgress {
            run_id,
            source_id: source_id.to_owned(),
            phase,
            items,
            elapsed_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
            message,
        });
    }
}
