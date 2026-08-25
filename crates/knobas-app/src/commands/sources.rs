//! Sources, secrets, sync and diagnostics -- stream F (interfaces §2.2, §2.3).
//! Mirrored in `app/src/lib/ipc/sources.ts`.
//!
//! Every body is three lines because every decision lives in `crate::sources`,
//! which has its own tests -- a `#[tauri::command]` cannot be called from one.
//!
//! # Two states, both fetched rather than declared
//!
//! Carry-over §10.6(a): a `#[tauri::command]` resolves *every* argument before
//! its body runs, and neither `AppState` nor `SourcesState` exists until
//! PostgreSQL is up. A command declaring either is rejected by Tauri itself
//! during bring-up with the bare string `"state not managed"` -- no code, and
//! `IpcErrorCode::NotReady` unreachable despite existing for exactly this. So:
//! `State<'_, Lifecycle>` (managed at build time) for the pool, and
//! `crate::sources::state(&app)` for the scheduler and the registry. Both
//! rules are pinned by tests at the bottom of this file and in `commands/mod.rs`.
//!
//! **No command reads a secret back.** There is no function here that could,
//! and `sources_crud::nothing_in_the_ipc_surface_reads_a_secret_back` scans
//! this file to keep it that way.

use tauri::State;

use crate::sources::{
    ConnectionReport, NewSource, SecretInput, SourceDraft, SourcePatch, SourceSummary, crud, to_ipc,
};
use crate::{IpcError, Lifecycle};

/// Register the demo source if absent, then sync it in full.
///
/// Safe to call repeatedly: see [`crate::sources::demo::demo_load_inner`],
/// which owns the behaviour and the test for it.
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

// -- §2.2: sources, secrets, credential health --------------------------------

/// One descriptor template per compiled-in adapter kind (`id == adapter_kind`).
///
/// Touches neither the database nor the keychain: the Add-source form is
/// generated from `config_schema` + `auth_methods`, and the launcher reads kind
/// metadata, without instantiating anything. It therefore answers before
/// bring-up, which is the point -- the form must be drawable on a cold start.
#[tauri::command]
pub fn list_adapters() -> Vec<knobas_source::SourceDescriptor> {
    crate::sources::Registry::builtin().templates()
}

#[tauri::command]
pub async fn list_sources<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
) -> Result<Vec<SourceSummary>, IpcError> {
    let state = crate::sources::state(&app)?;
    crud::list(&state.pool, state.registry.as_ref())
        .await
        .map_err(|error| to_ipc(&error, None))
}

#[tauri::command]
pub async fn add_source<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    input: NewSource,
) -> Result<SourceSummary, IpcError> {
    let state = crate::sources::state(&app)?;
    let id = input.id.clone();
    let summary = crud::add(&state.pool, &state.secrets, state.registry.as_ref(), input)
        .await
        .map_err(|error| to_ipc(&error, Some(&id)))?;
    // A brand-new source is due now; do not make the user wait out a tick.
    state.scheduler.wake();
    Ok(summary)
}

#[tauri::command]
pub async fn update_source<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    id: String,
    patch: SourcePatch,
) -> Result<SourceSummary, IpcError> {
    let state = crate::sources::state(&app)?;
    let summary = crud::update(&state.pool, state.registry.as_ref(), &id, patch)
        .await
        .map_err(|error| to_ipc(&error, Some(&id)))?;
    // Re-enabling a source, or shortening its interval, may have made it due.
    state.scheduler.wake();
    Ok(summary)
}

#[tauri::command]
pub async fn delete_source<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    id: String,
    purge_items: bool,
) -> Result<(), IpcError> {
    let state = crate::sources::state(&app)?;
    crud::delete(&state.pool, &state.secrets, &id, purge_items)
        .await
        .map_err(|error| to_ipc(&error, Some(&id)))
}

/// Store a credential for a saved source and test it (interfaces §3,
/// "Re-enter").
///
/// Write-only: there is no command that reads one back.
#[tauri::command]
pub async fn set_source_secret<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    id: String,
    secret: SecretInput,
) -> Result<knobas_sync::config::CredentialHealth, IpcError> {
    let state = crate::sources::state(&app)?;
    let health = crud::set_secret(
        &state.pool,
        &state.secrets,
        state.registry.as_ref(),
        &id,
        secret,
    )
    .await
    .map_err(|error| to_ipc(&error, Some(&id)))?;
    emit(&app, crate::events::SOURCE_HEALTH, &health);
    // A credential that now works releases the backoff, so the source may be
    // due this instant rather than at the next tick.
    state.scheduler.wake();
    Ok(health)
}

/// *Test connection*, for a draft or for a saved source. **Writes nothing.**
#[tauri::command]
pub async fn test_source<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    draft: SourceDraft,
) -> Result<ConnectionReport, IpcError> {
    let state = crate::sources::state(&app)?;
    let id = draft.source_id.clone();
    crud::test(&state.pool, &state.secrets, state.registry.as_ref(), draft)
        .await
        .map_err(|error| to_ipc(&error, id.as_deref()))
}

#[tauri::command]
pub async fn credential_health<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
) -> Result<Vec<knobas_sync::config::CredentialHealth>, IpcError> {
    let state = crate::sources::state(&app)?;
    crud::health(&state.pool)
        .await
        .map_err(|error| to_ipc(&error, None))
}

// -- §2.3: sync, status, the run log, diagnostics -----------------------------

/// Start a sync of one source and return its `sync_run.id` **immediately**
/// (P3).
///
/// It does not wait for the run: a network-bound sync behind a command is
/// exactly the UI blocking on a source that §14 forbids. The coarse
/// `sync:state` event fires when the run starts and again when it ends; what
/// the run *did* is read from `list_sync_runs`.
///
/// A source that is already syncing is not started twice -- the id of the run
/// in flight comes back, so a double-clicked *Sync now* is harmless.
///
/// # Why there are two of these
///
/// P3 asked for one command with an omittable `Option<Channel<SyncProgress>>`.
/// Tauri 2 cannot express that: `Channel` implements `Serialize` and its own
/// `CommandArg` but no `Deserialize`, and the only route from
/// `Option<Channel<_>>` to `CommandArg` is the blanket impl over `Deserialize`,
/// so the wrapped form does not compile. P3's documented fallback is therefore
/// in force, and the evidence is pinned in `crates/knobas-app/tests/ipc.rs`.
///
/// # Errors
/// [`IpcErrorCode::NotFound`](crate::IpcErrorCode::NotFound) for a source that
/// does not exist, [`NotReady`](crate::IpcErrorCode::NotReady) while the engine
/// is starting or stopping. A failure of the run *itself* arrives on
/// `sync:state` and in the run log -- by then this command has returned.
#[tauri::command]
pub async fn sync_now<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    source_id: String,
) -> Result<i64, IpcError> {
    let state = crate::sources::state(&app)?;
    state
        .scheduler
        .trigger(&source_id, knobas_sync::SyncTrigger::Manual, None)
        .await
        .map_err(|error| to_ipc(&error.into(), Some(&source_id)))
}

/// [`sync_now`], reporting per-item progress on `progress`.
///
/// Same return value and same semantics -- it too returns as soon as the run is
/// recorded. The channel is **required**; a caller that only needs to know a
/// run started should call [`sync_now`] and listen to `sync:state`, because
/// per-item progress goes on the channel and nowhere else (roadmap §4).
///
/// # Errors
/// As [`sync_now`].
#[tauri::command]
pub async fn sync_now_with_progress<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    source_id: String,
    progress: tauri::ipc::Channel<knobas_sync::SyncProgress>,
) -> Result<i64, IpcError> {
    let state = crate::sources::state(&app)?;
    let sink = std::sync::Arc::new(crate::sources::progress::ChannelSink::new(progress))
        as std::sync::Arc<dyn knobas_sync::ProgressSink>;
    state
        .scheduler
        .trigger(&source_id, knobas_sync::SyncTrigger::Manual, Some(sink))
        .await
        .map_err(|error| to_ipc(&error.into(), Some(&source_id)))
}

/// Start a sync for every enabled source that does not need a human. Returns
/// one run id per source it started, in id order.
#[tauri::command]
pub async fn sync_all<R: tauri::Runtime>(app: tauri::AppHandle<R>) -> Result<Vec<i64>, IpcError> {
    let state = crate::sources::state(&app)?;
    state
        .scheduler
        .trigger_all()
        .await
        .map_err(|error| to_ipc(&error.into(), None))
}

/// What every source is doing, and when it goes next.
///
/// The authoritative read: `sync:state` events are a hint that something moved
/// (and may be missed while the webview is still mounting -- roadmap §4 gotcha
/// 9), this is the truth. Call it on mount.
#[tauri::command]
pub async fn sync_status<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
) -> Result<Vec<knobas_sync::SourceSyncStatus>, IpcError> {
    let state = crate::sources::state(&app)?;
    knobas_sync::scheduler::status_all(&state.pool)
        .await
        .map_err(|error| to_ipc(&crate::sources::SourcesError::Db(error), None))
}

/// The per-source sync log the diagnostics view reads (§3): errors, durations
/// (`finished_at - started_at`), item counts. `source_id: None` spans every
/// source.
#[tauri::command]
pub async fn list_sync_runs<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    source_id: Option<String>,
    limit: u32,
) -> Result<Vec<knobas_sync::run_log::SyncRunRow>, IpcError> {
    let state = crate::sources::state(&app)?;
    knobas_sync::run_log::list(&state.pool, source_id.as_deref(), i64::from(limit.min(500)))
        .await
        .map_err(|error| {
            to_ipc(
                &crate::sources::SourcesError::Db(error),
                source_id.as_deref(),
            )
        })
}

#[tauri::command]
pub async fn db_stats<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
) -> Result<knobas_sync::stats::DbStats, IpcError> {
    let state = crate::sources::state(&app)?;
    knobas_sync::stats::db_stats(&state.pool)
        .await
        .map_err(|error| to_ipc(&crate::sources::SourcesError::Db(error), None))
}

/// Rebuild the FTS index (§3, the diagnostics view's *Re-index* button).
#[tauri::command]
pub async fn reindex_fts<R: tauri::Runtime>(app: tauri::AppHandle<R>) -> Result<(), IpcError> {
    let state = crate::sources::state(&app)?;
    knobas_sync::stats::reindex_fts(&state.pool)
        .await
        .map_err(|error| to_ipc(&crate::sources::SourcesError::Db(error), None))
}

/// Emit one event, best effort.
///
/// A webview that is not listening is not a failure of the operation that just
/// succeeded -- the credential is stored either way, and `credential_health()`
/// is the authoritative read.
fn emit<R: tauri::Runtime, T: serde::Serialize + Clone>(
    app: &tauri::AppHandle<R>,
    name: &str,
    payload: T,
) {
    use tauri::Emitter;
    if let Err(error) = app.emit(name, payload) {
        tracing::debug!(event = name, %error, "nothing was listening for this event");
    }
}

#[cfg(test)]
mod tests {
    /// No command in this file takes `State<'_, SourcesState>`.
    ///
    /// The same rule `commands/mod.rs` enforces for `AppState`, for the same
    /// reason and with the same failure mode: `SourcesState` is managed only
    /// once the sync engine has started, and a command declaring it **builds
    /// and lints clean** while failing at run time during bring-up with
    /// Tauri's bare `"state not managed"` -- no code for the frontend to branch
    /// on. `crate::sources::state(&app)` is the shape; it returns
    /// `IpcErrorCode::NotReady`.
    ///
    /// A source scan and not a type-level check because there is no type-level
    /// check to be had -- see the note in `commands/mod.rs`.
    #[test]
    fn no_command_takes_the_sources_state_directly() {
        let source = include_str!("sources.rs");
        // The doc comments above name the very thing they forbid, so the scan
        // runs over code only -- and the forbidden name is **assembled** rather
        // than written, because this test's own failure message quotes it and
        // a literal here would make the scan match itself. (It did, first run.)
        let forbidden = concat!("Sources", "State");
        let code = strip_line_comments(source);
        let mut rest = code.as_str();
        let mut seen = 0_usize;
        while let Some(at) = rest.find("State<'") {
            rest = &rest[at + "State<'".len()..];
            let end = rest.find('>').unwrap_or(rest.len());
            seen += 1;
            assert!(
                !rest[..end].contains(forbidden),
                "a command here declares that state as a `tauri::State` argument. \
                 It builds and lints clean and fails at *run time* during \
                 bring-up with Tauri's bare \"state not managed\" -- carry-over \
                 §10.6(a). Use `crate::sources::state(&app)?`, which answers \
                 `not_ready`."
            );
        }
        assert!(
            seen >= 1,
            "no `State<'_, _>` argument was found at all, so the scan proves nothing"
        );
    }

    /// The detector has to be able to catch something, or a green run is not
    /// evidence there was nothing to catch.
    ///
    /// It is run against a line of the shape it forbids, and against the shape
    /// that is allowed, so both a false negative and a false positive fail
    /// here rather than in the next stream's PR.
    #[test]
    fn the_scan_catches_the_signature_it_forbids() {
        let forbidden = concat!("Sources", "State");
        let bad = format!("pub async fn x(state: State<'_, {forbidden}>) {{}}");
        let good = "pub async fn x(app: tauri::AppHandle<R>) {}";
        let commented = format!("// State<'_, {forbidden}> is what this forbids\n{good}");

        assert!(declares_forbidden_state(&bad, forbidden));
        assert!(!declares_forbidden_state(good, forbidden));
        assert!(
            !declares_forbidden_state(&commented, forbidden),
            "prose about the rule is not a violation of it"
        );
        assert!(
            !declares_forbidden_state(
                "pub async fn x(lifecycle: State<'_, Lifecycle>) {}",
                forbidden
            ),
            "the allowed state must not trip it"
        );
    }

    /// The scan itself, factored out so the test above can drive it over text
    /// that is not this file.
    fn declares_forbidden_state(code: &str, forbidden: &str) -> bool {
        let code = strip_line_comments(code);
        let mut rest = code.as_str();
        while let Some(at) = rest.find("State<'") {
            rest = &rest[at + "State<'".len()..];
            let end = rest.find('>').unwrap_or(rest.len());
            if rest[..end].contains(forbidden) {
                return true;
            }
        }
        false
    }

    /// Doc comments only -- the forbidden spelling appears in prose above, and
    /// nowhere in a string literal in this file.
    fn strip_line_comments(source: &str) -> String {
        source
            .lines()
            .filter(|line| {
                let trimmed = line.trim_start();
                !trimmed.starts_with("//")
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}
