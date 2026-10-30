//! The knobas desktop shell: the Tauri application, the state its commands
//! share, and the database lifecycle bolted to the window's.
//!
//! The crate is a library with a one-line binary in front of it so the command
//! bodies stay reachable from `tests/`; a binary target cannot be linked
//! against by an integration test.
//!
//! ## The CSP in `tauri.conf.json`
//!
//! Recorded here because JSON has no comments. **`app.security.csp` governs
//! production only.** Tauri attaches it in `Asset::csp_header`, on the
//! `tauri://` asset protocol; `tauri dev` on desktop navigates straight to
//! `devUrl` (`PROXY_DEV_SERVER = cfg!(all(dev, mobile))` is false off mobile),
//! so a dev window is served by Vite over `http://` with no CSP header at all.
//! Nothing about the policy can therefore be verified by running `just dev` --
//! only a bundle built with `custom-protocol` exercises it.
//!
//! What the directives are for: `default-src 'self'` covers the extracted
//! `assets/index-*.js`; `style-src 'self'` covers `assets/index-*.css`, and
//! carries no `'unsafe-inline'` because `vite build` extracts every Svelte
//! component style into that file and the emitted `dist/index.html` has no
//! inline `<style>` or `style="..."` anywhere. `connect-src ipc:
//! http://ipc.localhost` is what the IPC needs: Tauri's `ipc-protocol.js`
//! reaches the backend with a `fetch`, and without it every command call is
//! blocked in a release build while working perfectly in dev.

pub mod assets;
pub mod backup;
pub mod capture;
pub mod checkout;
pub mod commands;
pub mod conversation;
pub mod docs;
pub mod downloads;
mod error;
pub mod file_pipeline;
pub mod inbox;
#[cfg(test)]
mod ipc_mirror;
pub mod link_sets;
pub mod logging;
pub mod notes_export;
pub mod notify;
mod profile;
pub mod protocol;
pub mod renderers;
pub mod run_presets;
pub mod settings;
mod slug;
pub mod sources;
pub mod standup;
pub mod start_work;
pub mod suggestion_pass;
pub mod ticket_views;
pub mod time;

pub use commands::app::{DbState, Lifecycle};
pub use error::{IpcError, IpcErrorCode};
pub use profile::{APP_IDENTIFIER, DEMO_FLAG, Profile};

/// Tauri event names, mirrored in `app/src/lib/ipc/index.ts` as `EVENTS`.
///
/// Orchestrator-owned and append-only: a stream that needs a new event asks
/// for the constant. Tauri 2 permits `:` in event names, and the prefix is the
/// subject -- `db:`, `sync:`, `source:` -- so a listener reads as what it is
/// listening to.
///
/// Rule (roadmap §4: events are not for throughput): these carry **coarse
/// state**, at most a handful per run. Per-item progress goes on an
/// `ipc::Channel` and nowhere else.
pub mod events {
    /// Payload: `DbState` (stream D). Fired during bring-up, replayed by
    /// `frontend_ready` -- the webview cannot listen before it says it can
    /// (roadmap §4 gotcha 9).
    pub const DB_STATE: &str = "db:state";
    /// Payload: `SourceSyncStatus` (stream F), on every run transition.
    pub const SYNC_STATE: &str = "sync:state";
    /// Payload: `CredentialHealth` (stream F), on a health *change* only.
    pub const SOURCE_HEALTH: &str = "source:health";
    /// Payload: `ActivityRow`, coalesced to at most one per second.
    pub const ACTIVITY_NEW: &str = "activity:new";
    /// Payload: `ContextRow` -- the context that was created or promoted
    /// (#47). The switcher re-lists on it rather than splicing.
    pub const CONTEXTS_CHANGED: &str = "contexts:changed";
    /// Payload: `notify::NotificationClicked` -- a desktop notification's
    /// body was clicked (#339). The first event added after #290's entry
    /// said nothing new crosses the bridge; its §10.8 entry says why.
    pub const NOTIFICATION_CLICKED: &str = "notification:clicked";
    /// Payload: the note's entity id, as a bare string -- the capture window
    /// asked for its note to be opened in the main window (#503). Sent to the
    /// `main` window by label and not broadcast: the capture window is closing
    /// and has no use for it.
    pub const CAPTURE_OPEN_NOTE: &str = "capture:open-note";
    /// Payload: `checkout::clone::CloneEvent` -- a clone's step and share
    /// done, at most one per second besides one per change of step, and how
    /// it ended (#823, ADR-0023). Broadcast: every view that shows the
    /// repository draws the same button.
    pub const CLONE_PROGRESS: &str = "clone:progress";
    /// Payload: `knobas_sync::scheduler::RefreshEnded` -- a refresh of one
    /// item ended (#2095, ADR-0031): the item, the write it followed if any,
    /// and whether it was refreshed, gone or failed. Broadcast: every view
    /// that shows the item re-reads it, and a write's echo settles on it.
    pub const ITEM_REFRESHED: &str = "item:refreshed";
    /// Payload: `sources::live::ViewFresh` -- a source view the shell was
    /// answered from memory was read again behind the answer, and the
    /// answer moved (#2242, ADR-0032): the entity and the section. Broadcast:
    /// every page showing that view reads it again.
    pub const VIEW_FRESH: &str = "view:fresh";

    /// Send `payload` to the frontend as `name` -- to every window, or to the
    /// one labelled `target` -- from the main thread, whichever thread asks.
    ///
    /// Every emit in this crate goes through here; the test
    /// `only_events_emit_names_the_emitter` keeps it that way.
    ///
    /// Why the hop: with Tauri's `tracing` feature on (#747), an `emit` holds
    /// Tauri's webview-list lock while it evaluates its script *synchronously*
    /// -- it posts the script to the main thread and waits for the answer. An
    /// IPC request the main thread is serving in that moment wants the same
    /// lock first. Neither moves, and the window freezes for good: on
    /// 2026-10-14 a gitea sync finishing seven seconds after launch did exactly
    /// that, mid-burst of the frontend's first invokes. On the main thread the
    /// same `emit` runs its script inline and has nobody to wait for.
    ///
    /// Best effort, like every emit here always was: the payload is serialized
    /// on the caller's thread, and a failure at either end is logged and
    /// dropped -- an event nobody hears does not fail an operation that already
    /// landed. Order is kept: the event loop runs queued tasks first in, first
    /// out.
    pub(crate) fn emit<R: tauri::Runtime>(
        app: &tauri::AppHandle<R>,
        target: Option<&'static str>,
        name: &'static str,
        payload: impl serde::Serialize,
    ) {
        let payload = match serde_json::to_value(payload) {
            Ok(payload) => payload,
            Err(error) => {
                tracing::warn!(event = name, %error, "an event payload did not serialize");
                return;
            }
        };
        let handle = app.clone();
        let sent = app.run_on_main_thread(move || {
            use tauri::Emitter;
            let emitted = match target {
                Some(label) => handle.emit_to(label, name, payload),
                None => handle.emit(name, payload),
            };
            if let Err(error) = emitted {
                tracing::debug!(event = name, %error, "nothing was listening for this event");
            }
        });
        if let Err(error) = sent {
            tracing::debug!(event = name, %error, "the event loop has gone; event dropped");
        }
    }
}

use sqlx::PgPool;
use tauri::Manager;

/// Points knobas at an already-running PostgreSQL instead of starting its own.
///
/// The escape hatch for development against a server with real data in it, and
/// for any environment where downloading and running an embedded server is not
/// wanted. An empty value counts as unset.
pub const DB_URL_ENV: &str = "KNOBAS_DB_URL";

/// Everything a command needs, managed by Tauri and shared by every window.
pub struct AppState {
    /// The pool commands run their queries on: the server's, which
    /// [`Lifecycle`] holds from the moment it starts (#1634) so a quit at any
    /// stage of bring-up stops it.
    pub pool: PgPool,
}

impl AppState {
    /// The shared state bring-up installs, over the migrated server's pool.
    fn over(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[cfg(feature = "test-util")]
impl AppState {
    /// The shared state over a pool, **for tests**.
    ///
    /// It exists because a [`tauri::State`] cannot be built by hand, so a
    /// command body is unreachable from a mock app until its state is managed
    /// -- see `tests/it/ipc.rs`, where `demo_load`'s profile guard is checked
    /// against a pool pointing at nothing (the guard has to refuse before any
    /// query, and that is what makes the test say so). Streams D, E and F test
    /// their commands through this one blessed path rather than each inventing
    /// a way in.
    ///
    /// The server is not here: [`Lifecycle::hold_server`] is what makes
    /// [`shutdown`] stop one, and a test that wants that holds its server
    /// there (#1634).
    ///
    /// Behind `test-util` and not merely `#[doc(hidden)]`: the one production
    /// caller is bring-up, which installs only a pool it has migrated, and a
    /// second would hand commands a pool nothing migrated and nothing
    /// stops. Hidden-but-present makes that a review catch; absent from the
    /// shipping build makes it a compile error. `cargo build`, `tauri build`
    /// and the `clippy --lib` half of `just check` all see the crate without
    /// this feature, so a stream that reaches for it outside `tests/` cannot
    /// get the gate green.
    #[must_use]
    pub fn over_pool(pool: PgPool) -> Self {
        Self::over(pool)
    }
}

/// Build the application, bring the database up, and run the event loop.
///
/// Returns when the last window has closed and the database has been stopped.
///
/// # Panics
///
/// Panics if the Tauri context is invalid or the database cannot be started --
/// neither leaves a usable window to report the failure in.
pub fn run() {
    let log_sink = logging::init();
    install_panic_hook();

    // The flag half of the profile, read **once** (LIFE-8, spec #1619): the
    // single-instance exemption below and `setup`'s profile are one decision,
    // so a flag that changed in one place cannot leave the other keyed to the
    // old one.
    let demo = Profile::demo_in(std::env::args());

    let builder = tauri::Builder::default();
    // The single-instance guard (#748), registered **first**: its setup runs
    // before the builder's `setup` closure, so a second launch exits before it
    // can start the database or the scheduler (whose startup closes every
    // unfinished run row, the first instance's live runs included). The demo
    // profile (`--demo`, its own data dir and database) is left out: the
    // plugin keys its lock by the bundle identifier, which cannot differ per
    // profile without moving `app_data_dir`, so guarding it would stop a demo
    // from running beside a real instance, which it does today. A second
    // `--demo` launch gets past this and is refused by the embedded lifecycle
    // instead, before the scheduler starts (ruling B6, spec #1619): the boot
    // screen names the demo profile and the window holding it.
    let builder = if demo {
        builder
    } else {
        builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            capture::focus_main_window(app)
        }))
    };
    builder
        // *Open in browser*. Its permission is granted in
        // `capabilities/default.json`; without that entry the command is
        // registered and every call is denied at run time.
        .plugin(tauri_plugin_opener::init())
        // Desktop notifications for inbox items (#290): the two permission
        // commands, granted one by one in `capabilities/default.json`; without
        // those entries the commands are registered and every call is denied
        // at run time, with nothing failing in the build. The send itself is
        // knobas' own `notify` since #339 (`notify.rs`).
        .plugin(tauri_plugin_notification::init())
        // The capture window's global shortcut (#503). Nothing is registered
        // here: the accelerator is a setting, empty by default, and
        // `capture::register_stored` applies it once the database is up. What
        // this line installs is the *handler* -- one for every shortcut this
        // application ever holds, of which there is exactly one -- and the
        // plugin's own four commands, which no capability grants to any
        // window, so the webview cannot reach them.
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    // The press only. A shortcut fires twice -- down and up --
                    // and a handler that did not say which would open the
                    // window and then focus it again on the release.
                    if event.state() == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                        capture::open_window(app);
                    }
                })
                .build(),
        )
        .setup(move |app| {
            // Both managed synchronously, before anything can call in, and
            // both for the same reason: they are what a command asks when the
            // database is *not* up yet. `Lifecycle` is the shared state's
            // only door (see `commands::app::Lifecycle`), so managing it here
            // rather than after bring-up is what makes `not_ready` reachable
            // instead of Tauri's bare "state not managed".
            let handle = app.handle().clone();
            start_file_log(&handle, &log_sink);
            let profile = Profile::new(demo, &handle.path().app_data_dir()?);
            tracing::info!(demo = profile.demo, dir = %profile.dir.display(), "profile");
            handle.manage(profile);
            // The file manager behind *Open the backup folder* (#1704), next
            // to the profile whose folder it opens.
            handle.manage(backup::FolderOpener::platform());
            handle.manage(sources::SecretStoreKind::from_env());
            handle.manage(Lifecycle::new());
            // The desktop notification sender and its waiter registry (#339),
            // over the real backend and the real event bridge. Managed here
            // for the same reason as `Lifecycle`: the command that reaches for
            // it must never see "state not managed".
            let backend: std::sync::Arc<dyn notify::Backend> =
                std::sync::Arc::new(notify::platform::show);
            let events = std::sync::Arc::new(sources::events::TauriEvents::new(handle.clone()));
            handle.manage(std::sync::Arc::new(notify::Notifier::new(backend, events)));
            // What the main window records about where the reader is, and why
            // the stored capture shortcut is not registered (#503). Managed
            // here for `Lifecycle`'s reason: the capture window can call in
            // before the database is up, and "state not managed" is not an
            // answer anything can branch on.
            handle.manage(capture::CaptureState::new());
            // The clones running in this process, which `clone_repo` starts
            // and stops and the quit stops with their folders (#1633).
            handle.manage(checkout::clone::Clones::global());

            // And the database comes up on its own task. M0 blocked here,
            // which froze the event loop for the length of a first run -- a
            // PostgreSQL download plus an `initdb`, tens of seconds -- so the
            // window had to be created hidden to avoid showing a white frame
            // that never repaints. It now appears immediately and renders the
            // boot screen, which is the M0 carry-over this discharges: the app
            // says what it is doing instead of not existing yet.
            //
            // Nothing is emitted from here (gotcha 9): the webview is not
            // listening yet. `spawn_bring_up` emits as it goes and
            // `frontend_ready` replays the current state to whoever missed it.
            spawn_bring_up(handle);
            Ok(())
        })
        // Append-only, orchestrator-owned, grouped by owning module so a
        // stream adding a command touches one line in one group.
        .invoke_handler(tauri::generate_handler![
            commands::app::ping,
            commands::app::app_status,
            commands::app::frontend_ready,
            commands::app::retry_database,
            commands::app::complete_first_run,
            commands::app::pages_kept_open,
            commands::app::set_pages_kept_open,
            commands::backup::backup_status,
            commands::backup::backup_now,
            commands::backup::share_export,
            commands::backup::set_backup_schedule,
            commands::backup::restore_backup,
            commands::backup::open_backup_folder,
            commands::entity::list_contexts,
            commands::entity::promote_context,
            commands::entity::mini_board,
            commands::entity::create_link,
            commands::entity::create_note,
            commands::entity::delete_note,
            commands::entity::get_entity,
            commands::entity::get_note,
            commands::entity::list_entities,
            commands::entity::save_note,
            commands::entity::unlink,
            commands::entity::room_suggestions,
            commands::entity::accept_suggestion,
            commands::entity::dismiss_suggestion,
            commands::entity::start_work_flow,
            commands::entity::start_work_run,
            commands::entity::start_work_retry,
            commands::entity::start_work_skip,
            commands::entity::start_work_amend,
            commands::entity::follow_merges,
            commands::entity::submit_write,
            commands::entity::reachable_transitions,
            commands::entity::entity_view,
            commands::entity::inbox_items,
            commands::entity::inbox_count,
            commands::entity::snooze_inbox_item,
            commands::entity::complete_inbox_item,
            commands::entity::list_projects,
            commands::entity::docs_tree,
            commands::entity::docs_page,
            commands::entity::docs_search,
            commands::entity::docs_export_markdown,
            commands::entity::docs_edit_section,
            commands::entity::docs_create_page,
            commands::entity::set_clones_root,
            commands::entity::entity_checkout,
            commands::entity::set_checkout_override,
            commands::entity::open_checkout,
            commands::entity::open_object,
            commands::entity::object_page,
            commands::search::search,
            commands::search::launcher_home,
            commands::search::smart_lists,
            commands::search::create_smart_list,
            commands::search::rename_smart_list,
            commands::search::delete_smart_list,
            commands::sources::demo_load,
            commands::sources::list_adapters,
            commands::sources::list_sources,
            commands::sources::add_source,
            commands::sources::update_source,
            commands::sources::delete_source,
            commands::sources::set_source_secret,
            commands::sources::test_source,
            commands::sources::credential_health,
            commands::sources::sync_now,
            commands::sources::sync_now_with_progress,
            commands::sources::backfill_source,
            commands::sources::sync_all,
            commands::sources::sync_status,
            commands::sources::pending_writes,
            commands::sources::write_queue_counts,
            commands::sources::flush_writes,
            commands::sources::apply_held_write,
            commands::sources::amend_write,
            commands::sources::discard_write,
            commands::sources::list_sync_runs,
            commands::sources::db_stats,
            commands::sources::reindex_fts,
            commands::time::current_timer,
            commands::time::start_timer,
            commands::time::stop_timer,
            commands::time::timer_heartbeat,
            commands::time::day_blocks,
            commands::time::update_block,
            commands::time::delete_block,
            commands::time::dismiss_block,
            commands::time::create_block,
            commands::time::passive_attribution,
            commands::time::set_passive_attribution,
            commands::time::worklog_draft,
            commands::time::log_work,
            commands::time::ad_hoc_block,
            commands::time::week_timesheet,
            commands::time::log_all_preview,
            // Appended at the foot of the list rather than beside the other
            // `commands::entity::` lines: the barrel is append-only (§10.8),
            // and #288 was appending to it in the same week.
            commands::entity::notification_kinds,
            commands::entity::set_notification_kinds,
            commands::entity::standup_digest,
            commands::entity::standup_protocol,
            commands::entity::publish_standup_protocol,
            commands::entity::standup_publish_target,
            commands::entity::set_standup_publish_target,
            commands::entity::create_action_item_ticket,
            commands::entity::notify,
            // Appended at the foot of the list, after #288's group: the barrel
            // is append-only (§10.8), and this is M4.0's first group.
            commands::assets::asset_tree,
            commands::assets::get_asset,
            commands::assets::create_asset,
            commands::assets::edit_asset,
            commands::assets::move_asset,
            commands::assets::delete_asset,
            commands::assets::asset_types,
            // #432's routes, appended after those. (#434's `context_assets`
            // stood here until #1578 retired it.)
            commands::assets::get_route,
            commands::assets::create_route,
            commands::assets::edit_route,
            commands::assets::delete_route,
            // (#435's `source_assets` stood here until #1578 retired it.)
            // #439's Import, appended after those for the same reason.
            commands::assets::preview_estate_import,
            commands::assets::apply_estate_import,
            // #443's monitoring settings, appended after those for the same
            // reason: the list is append-only (§10.8) and never re-sorted.
            commands::assets::monitoring_settings,
            commands::assets::set_monitoring_settings,
            // #448's Monitors tab, appended after those for the same reason.
            commands::assets::monitor_roster,
            // #444's open-alert read, appended after those: one read, and the
            // top strip's count is its length.
            commands::assets::open_alerts,
            // #446's ack, appended after that: the one write on this surface
            // that leaves its subject open on purpose.
            commands::assets::ack_alert,
            // #449's *Not monitored* roster, appended after that: the other
            // list the Monitors tab draws, and a list of assets rather than
            // of monitors.
            commands::assets::unmonitored_assets,
            // #496's URL resolver, appended after those for the same
            // reason: it is stream E's read but the list is one list,
            // append-only (§10.8) and never re-sorted.
            commands::entity::resolve_url,
            // #505's *Depends on this* panel, appended after that: an assets
            // read arriving after an entity one, because the list is in merge
            // order and never re-sorted (§10.8).
            commands::assets::depends_on_this,
            // #503's capture window, appended after that for the same reason.
            // Five: two for the shortcut setting, two for the pair the main
            // window records and the capture window reads, and one for the
            // button that brings the note into the main window.
            commands::entity::capture_shortcut,
            commands::entity::set_capture_shortcut,
            commands::entity::record_capture_context,
            commands::entity::capture_context,
            commands::entity::reveal_note,
            // #509's hcloud importer, appended at the foot: the list is
            // append-only (§10.8). The Import commands above are what consume
            // what this answers with.
            commands::assets::produce_estate_file,
            // #601's query-box suggestions, appended at the foot: a read on
            // the search module, and the list is append-only (§10.8).
            commands::search::filter_suggestions,
            // #779's read of an action item's ticket, appended at the foot:
            // the list is append-only (§10.8).
            commands::entity::action_item_ticket,
            // #823's clone, appended at the foot: the list is append-only
            // (§10.8).
            commands::entity::clone_repo,
            // #824's uncommitted diff at a checkout, appended at the foot:
            // the list is append-only (§10.8).
            commands::entity::entity_checkout_diff,
            // #819's two conversation reads, appended at the foot: the list
            // is append-only (§10.8).
            commands::entity::visit_entity,
            commands::entity::entity_thread,
            // #817's job-reach read and R-7.3's switch, appended at the foot:
            // the list is append-only (§10.8).
            commands::entity::job_reach,
            commands::entity::parent_links,
            commands::entity::set_parent_links,
            // #832's notes and capture setting and the notes export,
            // appended at the foot: the list is append-only (§10.8).
            commands::entity::set_notes_shown,
            commands::entity::export_notes_markdown,
            // #812's ticket views, appended at the foot: the list is
            // append-only (§10.8).
            commands::search::ticket_views,
            commands::search::save_ticket_view,
            commands::search::delete_ticket_view,
            commands::search::hide_saved_view,
            commands::search::open_saved_view,
            commands::search::translate_ticket_view,
            // #848's two Builds reads, appended at the foot: the list is
            // append-only (§10.8).
            commands::entity::pipelines,
            commands::entity::pipeline_runs,
            // #871's Dependencies read, appended at the foot: the list is
            // append-only (§10.8).
            commands::assets::dependencies,
            // #836's proposal evidence, appended at the foot: the list is
            // append-only (§10.8).
            commands::entity::proposal_evidence,
            // #849's pin and #850's *This job* read, appended at the foot:
            // the list is append-only (§10.8).
            commands::entity::set_pipeline_pinned,
            commands::entity::job_pipelines,
            // #851's two pipeline reads, appended at the foot: the list is
            // append-only (§10.8).
            commands::entity::older_pipeline_runs,
            commands::entity::pipeline_settings,
            // #844's *Open in…* applications, appended at the foot: the list
            // is append-only (§10.8).
            commands::entity::checkout_settings,
            commands::entity::set_checkout_applications,
            // #852's Run… presets, appended at the foot: the list is
            // append-only (§10.8).
            commands::entity::run_presets,
            commands::entity::save_run_preset,
            // #842's *Viewed* on a pull request's files, appended at the
            // foot: the list is append-only (§10.8).
            commands::entity::pr_viewed_files,
            commands::entity::set_pr_file_viewed,
            // #873's layer writes and saved views, appended at the foot: the list is
            // append-only (§10.8).
            commands::assets::create_dep_layer,
            commands::assets::rename_dep_layer,
            commands::assets::delete_dep_layer,
            commands::assets::order_dep_layers,
            commands::assets::place_dep_kind,
            commands::assets::dep_views,
            commands::assets::save_dep_view,
            commands::assets::delete_dep_view,
            commands::assets::set_dep_view_defaults,
            // #866's link sets, appended at the foot: the list is append-only
            // (§10.8).
            commands::entity::link_sets,
            commands::entity::link_set,
            commands::entity::save_link_set,
            commands::entity::delete_link_set,
            commands::entity::preview_link_set_rule,
            commands::entity::preview_link_set_pattern,
            commands::entity::hide_link_set_member,
            // #857's build chain read, appended at the foot: the list is
            // append-only (§10.8).
            commands::entity::run_chain,
            // #854 to #856's run page reads and its download, appended at
            // the foot: the list is append-only (§10.8).
            commands::entity::run_log,
            commands::entity::run_tests,
            commands::entity::test_history,
            commands::entity::run_artifacts,
            commands::entity::artifact_preview,
            commands::entity::download_run_file,
            // #867's job link sets read, appended at the foot: the list is
            // append-only (§10.8).
            commands::entity::job_link_sets,
            // #966's facts Code draws beside a row, appended at the foot: the
            // list is append-only (§10.8).
            commands::entity::code_facts,
            // #996's removal of a Run… preset, appended at the foot: the list
            // is append-only (§10.8).
            commands::entity::delete_run_preset,
            // #941's *Move to Confluence…*, appended at the foot: the list
            // is append-only (§10.8).
            commands::entity::move_notes_to_confluence,
            // #870's link-set hint card, appended at the foot: the list is
            // append-only (§10.8).
            commands::entity::link_set_hints,
            // #1202's install of an external adapter from a local manifest,
            // appended at the foot: the list is append-only (§10.8).
            commands::sources::install_adapter,
            // #1209's update check and update and #1210's uninstall, appended
            // at the foot: the list is append-only (§10.8).
            commands::sources::list_installed_adapters,
            commands::sources::check_adapter_updates,
            commands::sources::update_adapter,
            commands::sources::uninstall_adapter,
            // #1384's join of the run in flight for a source's settings page,
            // appended at the foot: the list is append-only (§10.8).
            commands::sources::watch_sync,
            // #1953's read of the tickets filed from an entity that it does
            // not link yet, appended at the foot: the list is append-only
            // (§10.8).
            commands::entity::owed_tickets,
            // #2031's previews an install and an update show before the
            // reader confirms, appended at the foot: the list is append-only
            // (§10.8).
            commands::sources::preview_install_adapter,
            commands::sources::preview_update_adapter,
            // #2040's read of the writes a source's credential cannot make,
            // appended at the foot: the list is append-only (§10.8).
            commands::entity::refused_writes,
            // #2106's *Sync now* for a role view, appended at the foot: the
            // list is append-only (§10.8).
            commands::sources::sync_role,
            // #2107's *Refresh* on an item's detail, appended at the foot.
            commands::sources::refresh_entity,
            // #2262's command renderers, appended at the foot: the list is
            // append-only (§10.8).
            commands::app::user_renderers,
            commands::app::set_user_renderers,
            commands::app::run_renderer,
            // #2263's default renderer per file pattern, appended at the foot.
            commands::app::renderer_defaults,
            commands::app::set_renderer_defaults,
            // #2256's byte read, appended at the foot (§10.8).
            commands::entity::entity_file_bytes,
            // #2264's pipeline over a file's bytes, appended at the foot (§10.8).
            commands::entity::entity_file_piped,
        ])
        .build(tauri::generate_context!())
        .expect("build the tauri application")
        .run(|app, event| {
            // Both, because they are different exits. Closing the last window
            // raises `ExitRequested`; quitting the application (Cmd-Q on
            // macOS) goes straight to `Exit` without ever raising it, and
            // measurably leaves the server running if only the first is
            // handled. `shutdown_database` takes the handle, so whichever
            // arrives first does the work and the other is a no-op.
            if matches!(
                event,
                tauri::RunEvent::ExitRequested { .. } | tauri::RunEvent::Exit
            ) {
                shutdown_app(app);
            }
        });
}

/// Stop everything bring-up started, in the one order that does not stall:
/// the running clones, the sync scheduler, the backup task, then the
/// database.
///
/// **The clones first**, and with their folders (#1633): a runner the
/// runtime drops kills git and leaves the half-made folder behind. They need
/// no database, so where they go is only a matter of not being last.
///
/// **The scheduler before the database**: its runs hold connections inside
/// transactions, and closing the pool under them is the stall the M0
/// carry-over describes. The backup task before the pool closes under it, for
/// the same reason: its tick reads `knobas.setting`. Every step is idempotent,
/// so a second call -- whichever of the two exit events arrives second -- is a
/// no-op.
///
/// Blocks: it runs from the event loop's exit handler, which is not async.
/// `tests/it/bring_up.rs` drives it through `shutdown`, its `test-util` door.
fn shutdown_app<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    if let Some(clones) = app.try_state::<std::sync::Arc<checkout::clone::Clones>>() {
        tauri::async_runtime::block_on(checkout::clone::stop_all(&clones));
    }
    sources::shutdown(app);
    backup::shutdown(app);
    shutdown_database(app);
}

/// [`shutdown_app`], for a test on a `tauri::test` mock app. **Tests only**:
/// the exit handler is its one production caller, and a second would stop
/// the database while the app still runs on it.
///
/// It blocks on Tauri's runtime, so a `#[tokio::test]` calls it inside
/// `tokio::task::block_in_place`.
#[cfg(feature = "test-util")]
pub fn shutdown<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    shutdown_app(app);
}

/// Bring the database up on its own task, narrating it on `db:state`.
///
/// Every transition is written into the managed [`Lifecycle`] *and* emitted.
/// Both, deliberately: the event is what makes the boot screen move the
/// instant something happens, and the stored state is what `app_status` and
/// `frontend_ready` answer with for a webview that was not listening yet, or
/// that reloaded. A frontend polling `app_status` therefore converges on the
/// truth even if every single event is lost.
///
/// Called once from `setup`, and again by `commands::app::retry_database`
/// after a failure -- which is why it takes a handle rather than closing over
/// `setup`'s.
pub(crate) fn spawn_bring_up<R: tauri::Runtime>(handle: tauri::AppHandle<R>) {
    tauri::async_runtime::spawn(async move {
        let profile = handle.state::<Profile>().inner().clone();
        bring_up_app(
            &handle,
            &profile,
            std::env::var(DB_URL_ENV).ok(),
            // The process's registry, which a retry finds already holding
            // every adapter the first attempt loaded (LIFE-1, spec #1619).
            sources::Registry::for_process(),
        )
        .await;
    });
}

/// [`spawn_bring_up`]'s body, for a test on a `tauri::test` mock app.
/// **Tests only**, for the reason [`AppState::over_pool`] is: a production
/// caller would skip `Lifecycle::begin_retry`'s claim and could start two
/// bring-ups at once.
///
/// `db_url` is what `KNOBAS_DB_URL` would say, so a test points bring-up at
/// its scratch database without setting a process-wide variable under every
/// other test in the binary; `registry` is the list the installed adapters
/// load into, the process's in the app, and the one the declarations are
/// written from. The handle must manage the [`Lifecycle`] (whose registry
/// the commands read, so the same one) and `capture::CaptureState`, as
/// `setup` does; the state `sources::start` and `backup::start` manage on
/// success is managed on it here too.
///
/// Answers once the state is `Ready` or `Failed`, which is what
/// [`Lifecycle::get`] then reads.
#[cfg(feature = "test-util")]
pub async fn bring_up<R: tauri::Runtime>(
    handle: &tauri::AppHandle<R>,
    profile: &Profile,
    db_url: Option<String>,
    registry: sources::Registry,
) {
    bring_up_app(handle, profile, db_url, registry).await;
}

/// The bring-up itself: start the server, migrate, sweep, start the engines,
/// install the shared state, announce `Ready` -- or `Failed`.
async fn bring_up_app<R: tauri::Runtime>(
    handle: &tauri::AppHandle<R>,
    profile: &Profile,
    db_url: Option<String>,
    registry: sources::Registry,
) {
    let config = profile.db_config(db_url);

    // A true sentence about what is about to take time, and `None` when
    // nothing unusual is: the boot screen shows this verbatim, so a
    // reassuring guess would be a lie in the one place the app is asking
    // for patience.
    let detail = if config.existing_url.is_some() {
        tracing::info!("{DB_URL_ENV} is set: using an externally managed postgres");
        Some(format!("connecting to the server {DB_URL_ENV} points at"))
    } else if tokio::fs::metadata(&config.root_dir).await.is_ok() {
        // `Path::exists` off the worker (`tokio::fs` runs under
        // `spawn_blocking`); a dropped bring-up does not cut the stat off.
        tracing::info!(root_dir = %config.root_dir.display(), "starting the embedded postgres");
        None
    } else {
        tracing::info!(root_dir = %config.root_dir.display(), "first run: provisioning postgres");
        Some("first run: downloading and initialising PostgreSQL".to_owned())
    };
    set_db_state(handle, DbState::Starting { detail });

    let lifecycle = handle.state::<Lifecycle>();
    let outcome = async {
        let db = knobas_db::EmbeddedDb::start(config)
            .await
            .map_err(|error| naming_the_profile(error, profile))?;
        let pool = db.pool().clone();
        let connector = db.connector();
        // Taken now, while the server is the one this bring-up started, and
        // armed at `ready` (#1779). `None` for `KNOBAS_DB_URL`'s server.
        let watch = db.watch();
        // **Held the moment it is up** (#1634): the exit handler stops what the
        // lifecycle holds, so a quit during the migration, the sweeps or the
        // engines' start stops this server too. Bring-up goes on over the pool
        // and the connector, which the handle hands out but does not need.
        lifecycle.hold_server(db);
        let generation = lifecycle.generation();
        set_db_state(handle, DbState::Migrating);
        knobas_db::migrate::run(&pool).await?;
        // `migrating` is the schema and nothing else (#1703, ruling D8):
        // every step from here on is `starting`, naming itself, so a slow
        // disk's boot screen says what it is waiting on.
        let step = |detail: &str| {
            set_db_state(
                handle,
                DbState::Starting {
                    detail: Some(detail.to_owned()),
                },
            );
        };
        step("closing any timer left running");
        // **Before anything can read the timer row** (#278): a timer that
        // outlived the last process is closed at the last moment knobas
        // was alive, flagged, and gone before the shell's first
        // `current_timer`. Placed here rather than beside `backup::start`
        // because the order matters and the other two do not: a shell that
        // read a stranded timer would draw a clock that has been running
        // all night, and correcting it a moment later is worse than never
        // showing it.
        //
        // A sweep that fails is logged and bring-up goes on. The database
        // is up, which is the thing the window is waiting for; the row
        // stays and the next launch tries again. Refusing to start over a
        // failed sweep would make a forgotten timer a reason knobas cannot
        // open at all.
        if let Err(error) = time::close_stranded(&pool).await {
            tracing::error!(%error, "the stranded-timer sweep failed");
        }
        step("interrupting start-work steps a quit left running");
        // A start-work step a quit left `running` becomes interrupted
        // (#790), so its ticket's flow can be retried or skipped. Logged,
        // not fatal, like the sweep above.
        match start_work::interrupt_stranded(&pool).await {
            Ok(0) => {}
            Ok(count) => tracing::info!(count, "start-work steps interrupted by a quit"),
            Err(error) => tracing::error!(%error, "the interrupted-step sweep failed"),
        }
        step("storing the adapters' declarations");
        // The adapters' declarations, into the database before the first
        // sync writes an item (#627): the item trigger resolves each row's
        // parents from them, so a sync that ran first would resolve against
        // nothing and be recomputed a moment later. Failure is logged, not
        // fatal -- every search command writes them again on its way in.
        if let Err(error) = commands::search::searcher(&pool, &registry).await {
            tracing::error!(?error, "the adapters' declarations could not be stored");
        }
        // Before `Ready`, and before `AppState` is installed: the sync
        // engine is part of "the database is up" as far as the frontend is
        // concerned, and a window that reacted to `ready` by calling
        // `sync_status` must not race the scheduler into existence.
        //
        // The suggestion pass is built here, once, and goes to the
        // scheduler, which requests a pass after every OK run: two would
        // mean two passes at once (#745, #1485). Nothing else asks for
        // one since `detect_suggestions` retired (#1611).
        let suggestions = suggestion_pass::SuggestionPass::new(
            std::sync::Arc::new(sources::DbConnections(connector.clone())),
            suggestion_pass::PASS_STATEMENT_TIMEOUT,
        );
        step("starting the sync engine");
        sources::start(handle, &pool, &connector, profile, registry, suggestions).await?;
        // Beside the sync scheduler, not inside it: a backup is not a
        // source (see `backup`'s module docs).
        step("starting the nightly backup");
        backup::start(handle, &pool, connector, profile);
        step("registering the capture shortcut");
        // The capture shortcut (#503), from the setting. **Here and not in
        // `setup`**: the plugin hands its registration to the main thread
        // and blocks until it answers, so a call from the event loop would
        // be the event loop waiting on itself. This task is not the main
        // thread, and it is the first moment there is a pool to read the
        // setting from. A refusal is recorded rather than fatal -- the
        // settings pane is where it is read, and no shortcut is a reason
        // to run without one, never a reason not to start.
        if let Err(error) = capture::register_stored(
            &pool,
            &handle.state::<capture::CaptureState>(),
            std::sync::Arc::new(capture::Plugin::new(handle.clone())),
        )
        .await
        {
            tracing::error!(%error, "the capture shortcut could not be read");
        }
        Ok::<_, Box<dyn std::error::Error + Send + Sync>>((pool, watch, generation))
    }
    .await;

    let message = match outcome {
        Ok((pool, watch, generation)) => {
            lifecycle.install(AppState::over(pool));
            // Installed *before* the state moves to `ready`: a frontend
            // that reacted to `ready` by fetching would otherwise race the
            // pool it was told about.
            set_db_state(handle, DbState::Ready);
            if let Some(watch) = watch {
                spawn_watch(handle.clone(), watch, generation);
            }
            return;
        }
        Err(error) => {
            tracing::error!(%error, "the database did not start");
            error.to_string()
        }
    };
    // **Stopped, never dropped** (#1634, LIFE-6), and before `Failed`, so a
    // Retry starts from a stopped server. Dropping a server this process
    // started runs `pg_ctl stop` synchronously in `PostgreSQL`'s `Drop`,
    // holding a runtime thread for the shutdown checkpoint while the window
    // polls `app_status`. `None` when a quit took it first.
    if let Some(db) = lifecycle.take_server()
        && let Err(error) = db.stop().await
    {
        tracing::error!(%error, "stopping the server of a failed bring-up failed");
    }
    set_db_state(handle, DbState::Failed { message });
}

/// How often a `ready` window looks at its own server (#1779). A read of
/// `postmaster.pid` and one `ps -p`, on a blocking thread, off every
/// command's path.
const WATCH_INTERVAL: std::time::Duration = std::time::Duration::from_secs(5);

/// What the boot screen says when the window's server died after `ready`.
const SERVER_DIED: &str =
    "the database server stopped while knobas was running; Retry starts it again";

/// Watch the server of `generation` until it dies or stops being the one
/// held (#1779).
///
/// Nothing else notices a postmaster that dies after `ready`: the window
/// stayed `ready`, every command failed against a port that no longer
/// answered, and *Retry* is only offered from `failed`. On a death the
/// window moves to `failed`, the server and its ownership lock stay held,
/// and *Retry* restarts it in place ([`spawn_restart`]).
///
/// The watch ends when a quit or a restart replaced the server it was
/// armed for; a restart arms a watch of its own.
fn spawn_watch<R: tauri::Runtime>(
    handle: tauri::AppHandle<R>,
    watch: knobas_db::embedded::ServerWatch,
    generation: u64,
) {
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(WATCH_INTERVAL).await;
            let lifecycle = handle.state::<Lifecycle>();
            if lifecycle.generation() != generation || !lifecycle.holds_server() {
                return;
            }
            let probe = watch.clone();
            let gone = tauri::async_runtime::spawn_blocking(move || probe.is_gone())
                .await
                .unwrap_or(false);
            if !gone {
                continue;
            }
            if lifecycle.server_died(generation, SERVER_DIED.to_owned()) {
                tracing::error!("the embedded postgres stopped under a ready window");
                events::emit(&handle, None, events::DB_STATE, lifecycle.get());
            }
            return;
        }
    });
}

/// Start the window's server again after it died under it (#1779), on its
/// own task: *Retry* from the `failed` that [`spawn_watch`] set.
///
/// The server is started in place, on its port, under the ownership lock the
/// window never let go of, so the pools, the sync engine and the backup task
/// reach it again as they are, and the shared state set aside at the death
/// is put back. Nothing is migrated or started anew: none of it stopped.
pub(crate) fn spawn_restart<R: tauri::Runtime>(handle: tauri::AppHandle<R>) {
    tauri::async_runtime::spawn(async move {
        restart_server(&handle).await;
    });
}

async fn restart_server<R: tauri::Runtime>(handle: &tauri::AppHandle<R>) {
    let lifecycle = handle.state::<Lifecycle>();
    // Held to the end, so a quit meanwhile waits and then stops what this
    // started; see `shutdown_database`.
    let _gate = lifecycle.restart_gate().lock().await;
    let Some(mut db) = lifecycle.take_server() else {
        // A quit took it first.
        return;
    };
    set_db_state(
        handle,
        DbState::Starting {
            detail: Some("restarting the database server".to_owned()),
        },
    );
    let restarted = db.restart().await;
    let watch = db.watch();
    lifecycle.hold_server(db);
    match restarted {
        Ok(()) => {
            lifecycle.resume();
            set_db_state(handle, DbState::Ready);
            if let Some(watch) = watch {
                spawn_watch(handle.clone(), watch, lifecycle.generation());
            }
        }
        Err(error) => {
            tracing::error!(%error, "the embedded postgres did not start again");
            set_db_state(
                handle,
                DbState::Failed {
                    message: format!(
                        "the database server stopped while knobas was running and did not \
                         start again: {error}"
                    ),
                },
            );
        }
    }
}

/// A start refused because the data directory is in use, prefixed with the
/// profile it was refused on; any other error as it is.
///
/// Ruling B2 (spec #1619): a second window on a live owner is refused with a
/// message naming the owner and the profile. `knobas-db` names the owner's
/// pid and the data directory; only the app knows which profile that is, and
/// a second `--demo` launch, which the single-instance guard lets through
/// (ruling B6), says *demo* here.
fn naming_the_profile(
    error: knobas_db::DbError,
    profile: &Profile,
) -> Box<dyn std::error::Error + Send + Sync> {
    if matches!(error, knobas_db::DbError::AlreadyRunning { .. }) {
        let name = if profile.demo { "demo" } else { "default" };
        format!("{name} profile: {error}").into()
    } else {
        error.into()
    }
}

/// Record a transition and tell the frontend about it.
///
/// An emit that nobody is listening to is not a failure -- during `setup`
/// there is no webview at all, and gotcha 9 is exactly that. It is logged and
/// dropped; the stored state is the durable half.
fn set_db_state<R: tauri::Runtime>(handle: &tauri::AppHandle<R>, state: DbState) {
    tracing::info!(?state, "database lifecycle");
    handle.state::<Lifecycle>().set(state.clone());
    events::emit(handle, None, events::DB_STATE, state);
}

/// Stop the embedded server, once.
///
/// Best-effort by design: the process is on its way out, and failing loudly
/// here would only replace a clean exit with a panic in an exit handler.
///
/// This runs only on an exit knobas is told about. A signal -- Ctrl-C under
/// `just dev`, a `kill`, a crash -- delivers no `RunEvent` at all, so the
/// server outlives the process. It is *not* cleaned up on the next start
/// either: what happens is that the next start finds it alive and adopts it
/// (see `knobas_db::EmbeddedDb::start`), reusing it as a warm start.
///
/// **Adoption is no longer a one-way door.** M0's adopted handle owned nothing,
/// so no clean quit ever stopped that server and every later run adopted it in
/// turn. An adopter now takes the ownership lock, so the next clean quit stops
/// the server for good. A server a *live* sibling owns is never adopted: that
/// start is refused (ruling B2), so the only handle that stops nothing is one
/// over `KNOBAS_DB_URL`.
///
/// `db.stop()` closes the pool first, and that close is **bounded**: a quit
/// during a long query no longer waits it out. The sync runs are stopped before
/// this is reached at all -- `sources::shutdown` cancels them, which is what
/// keeps Cmd-Q during a thirty-second remote call from being a thirty-second
/// hang.
fn shutdown_database<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    let Some(lifecycle) = app.try_state::<Lifecycle>() else {
        // `setup` never ran; nothing was started.
        return;
    };
    // Whatever stage bring-up reached (#1634): the lifecycle holds the
    // server from the moment it is up, so a quit during the migration stops
    // it too, and the bring-up that was using it fails on the closed pool.
    // `None` before the server is up -- a quit during a first-run download
    // leaves the download half-finished, which the next start resumes, the
    // same as any other interrupted first run -- and after a failed bring-up,
    // which stopped its own.
    // After a restart in flight (#1779), which holds this gate to its end.
    let server = tauri::async_runtime::block_on(async {
        let _gate = lifecycle.restart_gate().lock().await;
        lifecycle.take_server()
    });
    if let Some(db) = server {
        tracing::info!("stopping the embedded postgres");
        if let Err(error) = tauri::async_runtime::block_on(db.stop()) {
            tracing::error!(%error, "stopping the embedded postgres failed");
        }
    }
}

/// Start writing the log file (#747) and say what is running.
///
/// A directory that cannot be opened leaves stderr logging as it was.
fn start_file_log(handle: &tauri::AppHandle, sink: &logging::Sink) {
    match handle.path().app_log_dir() {
        Ok(dir) => match sink.attach(&dir, logging::MAX_BYTES, logging::KEEP_FILES) {
            Ok(()) => tracing::info!(
                version = env!("CARGO_PKG_VERSION"),
                build = if cfg!(debug_assertions) { "debug" } else { "release" },
                os = std::env::consts::OS,
                log_dir = %dir.display(),
                data_dir = ?handle.path().app_data_dir().ok(),
                "knobas starting"
            ),
            Err(error) => {
                tracing::error!(%error, dir = %dir.display(), "the log file could not be opened")
            }
        },
        Err(error) => tracing::error!(%error, "no log directory on this platform"),
    }
}

/// Route panics through `tracing` as well as the default handler.
///
/// The whole of startup happens before there is a window, so a failure there --
/// a database that will not start, a corrupt data directory -- can only be
/// reported by the process dying. The default hook writes to stderr, which is
/// exactly nothing in a release build launched from Finder or, on Windows,
/// launched at all. Sending it through the subscriber too means it lands
/// wherever the logs land, and keeps the default hook's backtrace.
fn install_panic_hook() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        tracing::error!("{info}");
        default(info);
    }));
}

#[cfg(test)]
mod tests {
    /// The window `tauri.conf.json` declares is visible from the start, and
    /// `run` no longer shows it by hand.
    ///
    /// M0 asserted the opposite, and was right to: bring-up blocked the event
    /// loop, so a visible window meant an empty white frame that did not
    /// repaint for the length of a first run. Bring-up is asynchronous now and
    /// the frontend renders a boot screen, so hiding the window would hide the
    /// very thing that says what is taking so long. A test still asserting the
    /// old behaviour would be worse than none.
    ///
    /// The two halves live in different files and neither compiles against the
    /// other, which is why the config is read back here.
    #[test]
    fn the_main_window_is_declared_visible_and_run_does_not_show_it() {
        let config: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).expect("tauri.conf.json");
        let windows = config["app"]["windows"]
            .as_array()
            .expect("app.windows is an array");

        assert_eq!(windows.len(), 1, "knobas has exactly one window");
        assert_eq!(windows[0]["label"], "main");
        assert_eq!(
            windows[0]["visible"], true,
            "the window must appear before the database does -- that is what \
             the boot screen is for"
        );

        // The other half: a `show()` left behind would be harmless today and
        // wrong the moment the window is ever deliberately hidden. Spelled in
        // pieces so this assertion does not match itself.
        let source = include_str!("lib.rs");
        let getter = concat!("get_webview", "_window");
        assert!(
            !source.contains(getter),
            "run() still reaches for the window; showing it is the config's job now"
        );
    }

    /// The event names are one list in two languages. A rename on one side is
    /// a listener that silently never fires -- the failure mode this test
    /// exists to make loud. Read off the code lines of `EVENTS` alone, so a
    /// doc comment quoting a name is no entry (#1984), and compared whole, so
    /// an entry on one side only fails too.
    #[test]
    fn the_event_names_match_their_typescript_mirror() {
        let mirror = include_str!("../../../app/src/lib/ipc/index.ts");
        let at = mirror
            .find("export const EVENTS = {")
            .expect("index.ts declares EVENTS");
        let block = &mirror[at..at + mirror[at..].find("} as const;").expect("EVENTS closes")];
        let mut declared: Vec<&str> = block
            .lines()
            .map(str::trim)
            .filter(|line| !line.starts_with('/') && !line.starts_with('*'))
            .filter_map(|line| line.split_once(": \"")?.1.split_once('"'))
            .map(|(name, _)| name)
            .collect();
        let mut names = [
            super::events::DB_STATE,
            super::events::SYNC_STATE,
            super::events::SOURCE_HEALTH,
            super::events::ACTIVITY_NEW,
            super::events::CONTEXTS_CHANGED,
            super::events::NOTIFICATION_CLICKED,
            super::events::CAPTURE_OPEN_NOTE,
            super::events::CLONE_PROGRESS,
            super::events::ITEM_REFRESHED,
            super::events::VIEW_FRESH,
        ];
        names.sort_unstable();
        declared.sort_unstable();
        assert_eq!(declared, names, "app/src/lib/ipc/index.ts's EVENTS");
    }

    /// Every emit hops to the main thread through `events::emit`, because one
    /// that does not can freeze the window for good (see that function). Tauri
    /// cannot emit without its emitter trait in scope, so naming the trait is
    /// the necessary condition: exactly one file may, exactly once.
    ///
    /// The count of one is also the scan's own witness -- a walk that read
    /// nothing would find zero, and fail.
    #[test]
    fn only_events_emit_names_the_emitter() {
        // Spelled in two halves so this test is not itself a hit.
        let needle = concat!("Emit", "ter");
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut stack = vec![src.clone()];
        let mut hits = Vec::new();
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).expect("src is readable") {
                let path = entry.expect("a directory entry").path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().is_some_and(|ext| ext == "rs") {
                    let text = std::fs::read_to_string(&path).expect("a source file");
                    for (n, line) in text.lines().enumerate() {
                        if line.contains(needle) {
                            let file = path.strip_prefix(&src).expect("under src");
                            hits.push(format!("{}:{}", file.display(), n + 1));
                        }
                    }
                }
            }
        }
        assert_eq!(
            hits.len(),
            1,
            "emit through crate::events::emit only: {hits:?}"
        );
        assert!(hits[0].starts_with("lib.rs:"), "{hits:?}");
    }
}
