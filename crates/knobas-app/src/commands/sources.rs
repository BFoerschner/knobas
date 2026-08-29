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

use knobas_sync::scheduler::Purge;
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
    // Something exists under this id again, so a purge the scheduler still owes
    // the *previous* holder of it must not fire over this source's first sync
    // (#127). Told after the insert committed, for the same reason
    // `delete_source` tells it after the delete did.
    state.scheduler.source_added(&id).await;
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

/// Delete a source: its configuration row, its keychain item, and -- because
/// this is the one place a source stops existing -- the scheduler's memory of
/// it.
///
/// The scheduler keeps a run entry per source id that deliberately outlives the
/// *run* (ADR-0005: it is what serves a late caller that run's ending). It must
/// not outlive the *source*: `knobas.sync_run` has no foreign key to
/// `source_config`, so a source added again under a deleted one's id inherits a
/// readable run history that is not its own, and the first-run wizard for the
/// new source could be handed the deleted source's ending. Told after the
/// delete has committed, so a delete that failed leaves the scheduler's claim
/// exactly as it was.
///
/// **`purge_items` is told to the scheduler too, and that is the ordering
/// guarantee (#127).** The purge is not the last word: a run of this source can
/// still be in flight, it checked that its source existed only at the very top
/// of the run, and its commit lands however many minutes later the remote
/// system takes -- writing the mirror back *and* clearing the tombstones the
/// purge set, so items the user deleted return **live** to `sync.live_item` for
/// a source that no longer exists. This command does not cancel that run and
/// does not wait for it (either would change what it promises over IPC, which
/// contract §10.8 freezes). It hands the purge intent to
/// [`Scheduler::forget_source`], which applies the purge again once that run
/// settles. The guarantee is therefore **eventual**: once the last run of a
/// source deleted with `purge_items` is over, it has no `sync.item` rows and
/// nothing of it is searchable. What it is not is synchronous -- the mirror
/// can be non-empty for the length of one already-running sync after this
/// returns.
///
/// **Two residuals, both logged rather than papered over**, and they are what
/// a reader chasing "I deleted it and it is still in search" should look for
/// first: the re-applied purge is one statement whose failure is warned about
/// and never retried, because by then there is nobody left to raise it to;
/// and a run whose task `shutdown` aborts *after* it committed never reaches
/// the claim, so its write-back outlives the process. Both are recorded where
/// they happen -- `Claims::pending_purges` and the scheduler's own log.
///
/// [`Scheduler::forget_source`]: knobas_sync::scheduler::Scheduler::forget_source
#[tauri::command]
pub async fn delete_source<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    id: String,
    purge_items: bool,
) -> Result<(), IpcError> {
    let state = crate::sources::state(&app)?;
    let purge = Purge::from(purge_items);
    crud::delete(&state.pool, &state.secrets, &id, purge_items)
        .await
        .map_err(|error| to_ipc(&error, Some(&id)))?;
    state.scheduler.forget_source(&id, purge).await;
    Ok(())
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
/// # Why this one asks for `FirstRun` and [`sync_now`] does not
///
/// This is the first-run wizard's command -- the only caller of it, and the
/// only surface in knobas that draws a per-item bar over a sync. ADR-0005:
/// *the wizard triggers `FirstRun` rather than `Manual`, so a first sync
/// carries that spelling whoever starts it and the run log stops recording who
/// won a race.* Two paths reach the scheduler when a source is added -- the
/// wake inside [`add_source`] and this command -- and before ADR-0005 the log
/// said `first_run` or `manual` depending on which of them got there first.
///
/// The spelling also **asks for something narrower**: the source's first sync,
/// rather than a sync. If the wake's run is already over by the time this
/// arrives, the scheduler hands back that run and its recorded ending instead
/// of starting a second one over a corpus that is already mirrored -- which is
/// what used to put *knobas mirrored 0 items* over a full first sync. A second
/// call (the wizard's *Retry*) finds nothing left to be handed and gets a run.
///
/// [`sync_now`], and therefore *Sync now* in the sources view and the retry
/// behind *Re-enter*, keeps `Manual`: those ask for work, not for news.
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
        .trigger(&source_id, knobas_sync::SyncTrigger::FirstRun, Some(sink))
        .await
        .map_err(|error| to_ipc(&error.into(), Some(&source_id)))
}

/// **Backfill** one source: re-read it from the top and rewrite every mirrored
/// item, ignoring the position it has stored.
///
/// Same shape as [`sync_now`] -- the run id comes back immediately (P3) and the
/// run itself is watched on `sync:state` -- and the same in-flight rule, so
/// pressing it twice does not start two full re-reads.
///
/// This is what makes a *payload widening* reach items nobody has touched. A
/// scheduled run re-fetches what changed upstream, and widening an adapter's
/// field list changes nothing upstream, so without this an issue last edited
/// last year keeps whatever the narrower query stored -- for ever
/// (`knobas_sync::run_backfill`, and issue #32, where Jira's `fields=` gained
/// `parent` and epic membership had to reach every already-mirrored issue).
///
/// It is the longest run a source ever does, and it deliberately does **not**
/// tombstone anything: the reasoning is on `knobas_sync::run_backfill`. It is
/// logged under the `backfill` trigger for that reason, not `manual` -- see
/// [`Scheduler::backfill`](knobas_sync::scheduler::Scheduler::backfill).
///
/// # Errors
/// As [`sync_now`].
#[tauri::command]
pub async fn backfill_source<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    source_id: String,
) -> Result<i64, IpcError> {
    let state = crate::sources::state(&app)?;
    state
        .scheduler
        .backfill(&source_id)
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

    /// **`delete_source` tells the scheduler the source is gone** (#119).
    ///
    /// A source scan for the same reason as the one above: a
    /// `#[tauri::command]` body cannot be called from a test, so the only place
    /// this one line can be checked is here. It is worth checking because
    /// losing it is silent -- everything still compiles, the delete still
    /// works, and the cost only shows up when somebody adds a source back under
    /// the deleted one's id and the first-run wizard describes a run that
    /// belonged to the source they threw away. `Scheduler::forget_source` and
    /// its own tests live in `knobas-sync`; this pins that it is *called*.
    #[test]
    fn deleting_a_source_tells_the_scheduler_to_forget_it() {
        let body = body_of(include_str!("sources.rs"), "pub async fn delete_source")
            .expect("delete_source is not in this file any more");
        assert!(
            body.contains("scheduler.forget_source("),
            "delete_source must tell the scheduler, or a source added again \
             under this id inherits the deleted one's run entry -- and with it \
             the ending the first-run wizard is served (#119, ADR-0005)"
        );
        assert_eq!(
            forget_arguments(&body),
            Some("&id, purge".to_owned()),
            "the forget has to carry what the delete was asked to do with the \
             mirror. Without it a source deleted with `purge_items` while a \
             sync of it was in flight gets its items -- and its cleared \
             tombstones -- written back by that run, live in `sync.live_item` \
             for ever (#127)"
        );
        assert!(
            body.contains("Purge::from(purge_items)"),
            "...and the intent has to be derived from *this* command's \
             `purge_items`, not a constant that happens to type-check"
        );
    }

    /// The argument list of the `forget_source(` call in a body, or `None` if
    /// there is no such call.
    ///
    /// Text, because a `#[tauri::command]` body cannot be called from a test
    /// and the argument is the whole of what #127 added here: the call was
    /// already present and already green before the purge intent rode along
    /// with it, so "is it called?" cannot notice the intent going missing.
    fn forget_arguments(body: &str) -> Option<String> {
        let at = body.find("forget_source(")? + "forget_source(".len();
        let rest = &body[at..];
        let end = rest.find(')')?;
        Some(rest[..end].to_owned())
    }

    /// **`add_source` voids a purge the scheduler still owes that id** (#127).
    ///
    /// The other end of the same in-memory intent, and the same reason for a
    /// source scan: the body is a `#[tauri::command]`. Losing this line is
    /// silent and worse than the defect it guards -- a source deleted with
    /// `purge_items` and added straight back under the same id would have its
    /// **new** mirror purged the moment the old source's run settled, with
    /// nothing on screen to say why.
    #[test]
    fn adding_a_source_voids_a_purge_the_scheduler_still_owes_that_id() {
        assert!(
            body_of(include_str!("sources.rs"), "pub async fn add_source")
                .expect("add_source is not in this file any more")
                .contains("scheduler.source_added("),
            "add_source must tell the scheduler the id has changed hands, or a \
             purge armed for the source that used to hold it fires over this \
             one's first sync (#127)"
        );
    }

    /// The scan can tell a body that makes the call from one that does not.
    #[test]
    fn the_forget_scan_would_notice_the_call_going_missing() {
        let with = "pub async fn delete_source() {\n  state.scheduler.forget_source(&id).await;\n}\npub async fn next() {}";
        let without = "pub async fn delete_source() {\n  crud::delete().await\n}\npub async fn next() { state.scheduler.forget_source(&id).await; }";
        assert!(
            body_of(with, "pub async fn delete_source")
                .unwrap()
                .contains("scheduler.forget_source(")
        );
        assert!(
            !body_of(without, "pub async fn delete_source")
                .unwrap()
                .contains("scheduler.forget_source("),
            "the scan must read this command's body, not the whole file -- \
             another command making the call is not this one making it"
        );

        // ...and the argument scan can tell the intent being carried from the
        // intent being dropped, which is the #127 half of this guard.
        assert_eq!(forget_arguments(with), Some("&id".to_owned()));
        assert_eq!(
            forget_arguments("state.scheduler.forget_source(&id, purge).await;"),
            Some("&id, purge".to_owned())
        );
        assert_eq!(forget_arguments("nothing of the sort"), None);

        // ...and a signature quoted inside a string literal is not a
        // declaration of it. This file is full of those -- the two literals
        // above, one of which makes the very call the scan looks for -- so
        // without the line-start rule the guard would land on one of them the
        // moment `delete_source` was renamed away, and pass green over a
        // command that no longer exists.
        let decoy = "mod tests {\n    let a = \"pub async fn delete_source() { scheduler.forget_source(&id); }\";\n}";
        assert!(
            body_of(decoy, "pub async fn delete_source").is_none(),
            "an indented look-alike is not the item this scan is about"
        );
    }

    /// From a function's declaration to the start of the next item, comments
    /// stripped. Crude on purpose: it only has to be narrower than the file.
    ///
    /// **The signature has to start a line**, and that is what keeps this
    /// honest rather than merely narrow. The same text also appears in this
    /// file quoted inside the literals the test above drives this over -- one
    /// of which makes the very call the scan looks for -- so a plain `find`
    /// would land on one of those the moment the real item was renamed away,
    /// and the guard would pass green over a command that no longer exists.
    /// `None` instead, which its caller turns into a failure.
    ///
    /// The attribute above the item is deliberately *not* part of the anchor:
    /// `#[tauri::command]` written anywhere in `src/commands/**`, string
    /// literals included, is a command declaration as far as
    /// `tests/wiring.rs::every_command_is_in_the_handler_list` is concerned.
    fn body_of(source: &str, signature: &str) -> Option<String> {
        let code = strip_line_comments(source);
        let at = code
            .match_indices(signature)
            .map(|(at, _)| at)
            .find(|at| *at == 0 || code[..*at].ends_with('\n'))?;
        let rest = &code[at + signature.len()..];
        let end = ["\npub ", "\n#["]
            .iter()
            .filter_map(|marker| rest.find(marker))
            .min()
            .unwrap_or(rest.len());
        Some(rest[..end].to_owned())
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
