//! Sources, secrets, sync and diagnostics -- stream F (interfaces §2.2, §2.3).
//!
//! Its `State<'_, Lifecycle>` is not an accident and is not stream D being
//! tidy: carry-over §10.6(a). `AppState` exists only once PostgreSQL is up,
//! and a `#[tauri::command]` resolves every argument *before* its body runs,
//! so a command declaring `State<'_, AppState>` is rejected by Tauri itself
//! during bring-up with the bare string `"state not managed"` -- no code for
//! the frontend to branch on. `Lifecycle` is managed at build time and is
//! always there; `lifecycle.pool()?` is the single place `not_ready` comes
//! from.

use tauri::{Emitter, State};

use crate::{IpcError, Lifecycle};

/// Register the demo source if absent, then sync it in full.
///
/// Safe to call repeatedly: see [`crate::sources::demo::demo_load_inner`], which owns
/// the behaviour and the test for it.
///
/// Refused outside the demo profile (ruling P13): the Tidewater fixture is
/// twenty-one items of fiction, and a corpus that mixes it with real work is
/// one nobody can search again. The refusal comes before the first query, so
/// the wrong profile writes nothing at all.
#[tauri::command]
pub async fn demo_load(
    lifecycle: State<'_, Lifecycle>,
    profile: State<'_, crate::Profile>,
) -> Result<knobas_sync::SyncReport, IpcError> {
    // Before the pool is even asked for: the wrong profile must write nothing
    // at all, and "refused" must not be confusable with "the database was
    // busy". `tests/ipc.rs` pins that ordering against an unreachable pool.
    if !profile.allows_demo_data() {
        return Err(IpcError::invalid(format!(
            "demo data belongs to the demo profile -- start knobas with {} (or `just demo`)",
            crate::DEMO_FLAG
        )));
    }
    let pool = lifecycle.pool()?;
    Ok(crate::sources::demo::demo_load_inner(&pool).await?)
}

/// The channel `sync_now_with_progress` reports on.
///
/// A local wrapper because the orphan rule forbids implementing
/// `knobas_sync::ProgressSink` for `tauri::ipc::Channel` directly -- neither is
/// this crate's type.
struct ChannelProgress(tauri::ipc::Channel<knobas_sync::SyncProgress>);

impl knobas_sync::ProgressSink for ChannelProgress {
    fn report(&self, progress: knobas_sync::SyncProgress) {
        // A webview that stopped listening is not a sync failure: the run is
        // the point, the progress bar is not.
        if let Err(error) = self.0.send(progress) {
            tracing::debug!(%error, "dropping a progress message: nobody is listening");
        }
    }
}

/// Start a sync of one configured source and return its `sync_run.id`.
///
/// **Returns before the run finishes** (ruling P3): the log row is written,
/// the run is spawned onto its own task, and the id comes back immediately. A
/// UI must never wait on a source. What the run did is read back from
/// `knobas.sync_run`; while it is in flight, `sync:state` says so -- one event
/// when the run starts and one when it ends, which is what makes
/// `EVENTS.syncState` worth listening to.
///
/// The two refusals happen *before* the return, so a typo is still an error
/// the caller sees rather than a run id for a run that never started: an id no
/// adapter answers to, and an id with no `knobas.source_config` row.
///
/// **Stream F replaces the bare `spawn`.** What is here is one spawn per
/// call and nothing else: no concurrency cap, no queue, no backoff. That is
/// adequate for M0's single in-memory adapter and a human pressing a button;
/// it is *not* adequate for real sources, because a run holds one of the
/// pool's five connections for its whole network-bound duration (M0
/// carry-over). F's scheduler owns the cap, the queue, the backoff and the
/// cursor-inside-the-lock fix. The **shape** is what this freezes: id out
/// immediately, log row written first, coarse state on the event, per-item
/// progress on the channel only.
///
/// # Why there are two of these
///
/// P3 asked for one command with an omittable `Option<Channel<SyncProgress>>`.
/// Tauri 2.11 cannot express that: `Channel` implements `Serialize` and its own
/// `CommandArg` but no `Deserialize`, and the only route from
/// `Option<Channel<_>>` to `CommandArg` is the blanket impl over
/// `Deserialize`, so the wrapped form does not compile. P3's documented
/// fallback is therefore in force -- this command for callers that want no
/// per-item progress, [`sync_now_with_progress`] for the ones that do. The
/// evidence is pinned in `crates/knobas-app/tests/ipc.rs`.
///
/// # Errors
///
/// [`IpcErrorCode::NotFound`](crate::IpcErrorCode::NotFound) for an unknown
/// adapter, [`NotReady`](crate::IpcErrorCode::NotReady) for an unconfigured
/// source. A failure of the run *itself* arrives on `sync:state` and in the
/// run log, not here -- by then this command has already returned.
#[tauri::command]
pub async fn sync_now<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    lifecycle: State<'_, Lifecycle>,
    source_id: String,
) -> Result<i64, IpcError> {
    let pool = lifecycle.pool()?;
    spawn_sync(&app, &pool, source_id, None).await
}

/// [`sync_now`], reporting per-item progress on `progress`.
///
/// Same return value and same semantics -- it too returns as soon as the run
/// is recorded. The channel is the only difference, and it is **required**;
/// see [`sync_now`] for why the two are separate commands. Ruling P3 and
/// roadmap §4: per-item progress goes on the channel and nowhere else, so a
/// caller that only wants to know a run started should call [`sync_now`] and
/// listen to `sync:state` instead of opening a channel it will not read.
///
/// # Errors
///
/// As [`sync_now`].
#[tauri::command]
pub async fn sync_now_with_progress<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    lifecycle: State<'_, Lifecycle>,
    source_id: String,
    progress: tauri::ipc::Channel<knobas_sync::SyncProgress>,
) -> Result<i64, IpcError> {
    let pool = lifecycle.pool()?;
    spawn_sync(
        &app,
        &pool,
        source_id,
        Some(Box::new(ChannelProgress(progress))),
    )
    .await
}

/// Record a run, start it on its own task, and hand back its id.
///
/// Generic over the runtime because a bare `tauri::AppHandle` means
/// `AppHandle<Wry>`, which `tests/ipc.rs`'s `MockRuntime` is not -- a
/// non-generic command taking a handle simply cannot be registered on a mock
/// app, and the IPC tests would have to stop covering these two.
///
/// The order is the contract: refuse what cannot run, open the log row, say
/// `running` on the event, *then* spawn. A caller holding the id can read the
/// run's fate out of `knobas.sync_run` whatever happens to the task.
async fn spawn_sync<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    pool: &sqlx::PgPool,
    source_id: String,
    sink: Option<Box<dyn knobas_sync::ProgressSink>>,
) -> Result<i64, IpcError> {
    let prepared =
        crate::sources::demo::prepare_sync(pool, &source_id, knobas_sync::SyncTrigger::Manual)
            .await?;
    let run_id = prepared.run_id;
    emit_sync_state(
        app,
        &knobas_sync::SourceSyncStatus::started(&source_id, run_id),
    );

    // Owned clones: the task outlives this call by design. `PgPool` and
    // `AppHandle` are both handle types, so this is a refcount each.
    let pool = pool.clone();
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let result = knobas_sync::run(
            &pool,
            prepared.source.as_ref(),
            prepared.cursor,
            run_id,
            sink.as_deref(),
        )
        .await;
        match &result {
            Ok(report) => {
                tracing::info!(run_id, source_id = %source_id, upserted = report.upserted, "sync finished");
            }
            // Nowhere to return it to -- the command answered long ago. The
            // run log has it (`knobas_sync::run` wrote it there before
            // returning), the event carries its class, and this line is what
            // makes it visible in a terminal.
            Err(error) => {
                tracing::warn!(run_id, source_id = %source_id, %error, "sync failed");
            }
        }
        let outcome = outcome_of(&result);
        emit_sync_state(
            &app,
            &knobas_sync::SourceSyncStatus::finished(&source_id, run_id, outcome),
        );
    });

    Ok(run_id)
}

/// How the run ended, in the log's vocabulary.
///
/// Separated from the task body so it can be asserted without a database and a
/// spawn: it is the only *decision* the spawned task makes, and getting it
/// wrong means stream F's backoff reads the wrong class -- retrying a 401 for
/// ever, or never retrying something transient.
fn outcome_of(
    result: &Result<knobas_sync::SyncReport, knobas_sync::SyncError>,
) -> knobas_sync::SyncOutcome {
    match result {
        Ok(_) => knobas_sync::SyncOutcome::Ok,
        Err(error) => knobas_sync::SyncOutcome::of(error),
    }
}

/// Emit one coarse `sync:state`.
///
/// Failure is logged and dropped: a webview that is not listening (or not
/// there yet) is not a sync failure, and the run log is the durable record
/// either way.
fn emit_sync_state<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    status: &knobas_sync::SourceSyncStatus,
) {
    if let Err(error) = app.emit(crate::events::SYNC_STATE, status) {
        tracing::debug!(%error, "nobody is listening to sync:state");
    }
}

#[cfg(test)]
mod tests {
    use knobas_source::SourceError;
    use knobas_sync::{SyncError, SyncOutcome};

    /// The classification that reaches `sync:state` -- and through it stream
    /// F's backoff, which never retries `unauthorized` and does retry
    /// `unreachable`. A failed run that reported `ok` would look to the
    /// scheduler like a source that is fine.
    #[test]
    fn the_terminal_outcome_is_the_class_of_the_failure() {
        let report = knobas_sync::SyncReport {
            source_id: "mock".to_owned(),
            upserted: 1,
            deleted: 0,
            swept: 0,
            cursor: "c".to_owned(),
        };
        assert_eq!(super::outcome_of(&Ok(report)), SyncOutcome::Ok);

        for (error, expected) in [
            (SourceError::Unauthorized, SyncOutcome::Unauthorized),
            (
                SourceError::Unreachable("dns".to_owned()),
                SyncOutcome::Unreachable,
            ),
            (
                SourceError::Protocol("bad json".to_owned()),
                SyncOutcome::Error,
            ),
        ] {
            let described = format!("{error:?}");
            assert_eq!(
                super::outcome_of(&Err(SyncError::Source(error))),
                expected,
                "{described}"
            );
        }
    }
}
