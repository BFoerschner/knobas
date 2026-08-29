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

pub mod backup;
pub mod commands;
mod error;
mod profile;
pub mod sources;

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
}

use std::sync::{Mutex, PoisonError};

use sqlx::PgPool;
use tauri::{Emitter, Manager};

/// Points knobas at an already-running PostgreSQL instead of starting its own.
///
/// The escape hatch for development against a server with real data in it, and
/// for any environment where downloading and running an embedded server is not
/// wanted. An empty value counts as unset.
pub const DB_URL_ENV: &str = "KNOBAS_DB_URL";

/// Everything a command needs, managed by Tauri and shared by every window.
pub struct AppState {
    /// The pool commands run their queries on. Cloned out of [`EmbeddedDb`], so
    /// it stays usable without touching the mutex below.
    ///
    /// [`EmbeddedDb`]: knobas_db::EmbeddedDb
    pub pool: PgPool,

    /// The server this process started, until it is shut down.
    ///
    /// `Option` because shutdown *consumes* the handle: `EmbeddedDb::stop`
    /// takes `self`. `ExitRequested` can fire more than once -- a handler that
    /// vetoes the exit leaves the user free to quit again -- so the take is
    /// what makes the second attempt a no-op rather than a panic.
    ///
    /// Private: taking it is this module's business, and a second taker would
    /// mean a live pool over a stopped server.
    db: Mutex<Option<knobas_db::EmbeddedDb>>,
}

#[cfg(feature = "test-util")]
impl AppState {
    /// The shared state over a pool this process did not start. **Tests only.**
    ///
    /// It exists because a [`tauri::State`] cannot be built by hand, so a
    /// command body is unreachable from a mock app until its state is managed
    /// -- see `tests/ipc.rs`, where `demo_load`'s profile guard is checked
    /// against a pool pointing at nothing (the guard has to refuse before any
    /// query, and that is what makes the test say so). Streams D, E and F test
    /// their commands through this one blessed path rather than each inventing
    /// a way in.
    ///
    /// `db` is `None`, which is the truth: [`shutdown_database`] stops only a
    /// server knobas owns, and there is none here. That is also why the
    /// constructor is behind `test-util` and not merely `#[doc(hidden)]`: a
    /// production caller would build an `AppState` whose server nothing ever
    /// stops, leaving a postmaster running after every quit. Hidden-but-present
    /// makes that a review catch; absent from the shipping build makes it a
    /// compile error. `cargo build`, `tauri build` and the `clippy --lib` half
    /// of `just check` all see the crate without this feature, so a stream that
    /// reaches for it outside `tests/` cannot get the gate green.
    #[must_use]
    pub fn over_pool(pool: PgPool) -> Self {
        Self {
            pool,
            db: Mutex::new(None),
        }
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
    init_tracing();
    install_panic_hook();

    tauri::Builder::default()
        // *Open in browser*, the app's only plugin. Its permission is granted
        // in `capabilities/default.json`; without that entry the command is
        // registered and every call is denied at run time.
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            // Both managed synchronously, before anything can call in, and
            // both for the same reason: they are what a command asks when the
            // database is *not* up yet. `Lifecycle` is the shared state's
            // only door (see `commands::app::Lifecycle`), so managing it here
            // rather than after bring-up is what makes `not_ready` reachable
            // instead of Tauri's bare "state not managed".
            let handle = app.handle().clone();
            let profile = Profile::from_args(std::env::args(), &handle.path().app_data_dir()?);
            tracing::info!(demo = profile.demo, dir = %profile.dir.display(), "profile");
            handle.manage(profile);
            handle.manage(Lifecycle::new());

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
            commands::backup::backup_status,
            commands::backup::backup_now,
            commands::backup::set_backup_schedule,
            commands::backup::restore_backup,
            commands::entity::create_link,
            commands::entity::get_entity,
            commands::entity::list_entities,
            commands::entity::recent_activity,
            commands::entity::unlink,
            commands::search::search,
            commands::search::launcher_home,
            commands::search::smart_lists,
            commands::search::smart_list_items,
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
                // **Before** the database: the scheduler's runs hold
                // connections inside transactions, and closing the pool under
                // them is the stall the M0 carry-over describes. Both calls are
                // idempotent, so whichever event arrives first does the work.
                sources::shutdown(app);
                // Before the pool closes under it, for the reason the sync
                // scheduler is stopped first: its tick reads `knobas.setting`.
                backup::shutdown(app);
                shutdown_database(app);
            }
        });
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
        let config = profile.db_config(std::env::var(DB_URL_ENV).ok());

        // A true sentence about what is about to take time, and `None` when
        // nothing unusual is: the boot screen shows this verbatim, so a
        // reassuring guess would be a lie in the one place the app is asking
        // for patience.
        let detail = if config.existing_url.is_some() {
            tracing::info!("{DB_URL_ENV} is set: using an externally managed postgres");
            Some(format!("connecting to the server {DB_URL_ENV} points at"))
        } else if config.root_dir.exists() {
            tracing::info!(root_dir = %config.root_dir.display(), "starting the embedded postgres");
            None
        } else {
            tracing::info!(root_dir = %config.root_dir.display(), "first run: provisioning postgres");
            Some("first run: downloading and initialising PostgreSQL".to_owned())
        };
        set_db_state(&handle, DbState::Starting { detail });

        let outcome = async {
            let db = knobas_db::EmbeddedDb::start(config).await?;
            set_db_state(&handle, DbState::Migrating);
            knobas_db::migrate::run(db.pool()).await?;
            // Before `Ready`, and before `AppState` is installed: the sync
            // engine is part of "the database is up" as far as the frontend is
            // concerned, and a window that reacted to `ready` by calling
            // `sync_status` must not race the scheduler into existence.
            sources::start(&handle, &db).await?;
            // Beside the sync scheduler, not inside it: a backup is not a
            // source (see `backup`'s module docs).
            backup::start(&handle, &db);
            Ok::<_, Box<dyn std::error::Error>>(db)
        }
        .await;

        match outcome {
            Ok(db) => {
                handle.state::<Lifecycle>().install(AppState {
                    pool: db.pool().clone(),
                    db: Mutex::new(Some(db)),
                });
                // Installed *before* the state moves to `ready`: a frontend
                // that reacted to `ready` by fetching would otherwise race the
                // pool it was told about.
                set_db_state(&handle, DbState::Ready);
            }
            Err(error) => {
                tracing::error!(%error, "the database did not start");
                set_db_state(
                    &handle,
                    DbState::Failed {
                        message: error.to_string(),
                    },
                );
            }
        }
    });
}

/// Record a transition and tell the frontend about it.
///
/// An emit that nobody is listening to is not a failure -- during `setup`
/// there is no webview at all, and gotcha 9 is exactly that. It is logged and
/// dropped; the stored state is the durable half.
fn set_db_state<R: tauri::Runtime>(handle: &tauri::AppHandle<R>, state: DbState) {
    tracing::info!(?state, "database lifecycle");
    handle.state::<Lifecycle>().set(state.clone());
    if let Err(error) = handle.emit(events::DB_STATE, state) {
        tracing::debug!(%error, "nobody is listening to db:state yet");
    }
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
/// turn. An adopter now takes the ownership lock when nobody else holds it, so
/// the next clean quit stops the server for good -- while a server a *live*
/// sibling owns is still left alone. `EmbeddedDb::stop` reports which case it
/// was.
///
/// `db.stop()` closes the pool first, and that close is **bounded**: a quit
/// during a long query no longer waits it out. The sync runs are stopped before
/// this is reached at all -- `sources::shutdown` cancels them, which is what
/// keeps Cmd-Q during a thirty-second remote call from being a thirty-second
/// hang.
fn shutdown_database(app: &tauri::AppHandle) {
    let Some(lifecycle) = app.try_state::<Lifecycle>() else {
        // `setup` never ran; nothing was started.
        return;
    };
    let Some(state) = lifecycle.app_state() else {
        // Bring-up is still running or failed; there is no server to stop.
        // A quit during a first-run download therefore leaves the download
        // half-finished, which the next start resumes -- the same as any other
        // interrupted first run.
        return;
    };
    // The guard is dropped before `block_on`: nothing may hold a lock across
    // an await point, and the take is all the mutex is protecting.
    let db = state
        .db
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .take();

    if let Some(db) = db {
        tracing::info!("stopping the embedded postgres");
        if let Err(error) = tauri::async_runtime::block_on(db.stop()) {
            tracing::error!(%error, "stopping the embedded postgres failed");
        }
    }
}

/// Send `tracing` output to stderr, at `info` unless `RUST_LOG` says otherwise.
///
/// Without this the database layer's diagnostics -- which port came up, whether
/// an existing data directory was reused -- are discarded, and `tauri dev` is
/// exactly when they are wanted.
fn init_tracing() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        // Explicit, and not the `fmt()` default of stdout: diagnostics are not
        // this program's output, and anything piping knobas' stdout should get
        // nothing but what knobas means to say.
        .with_writer(std::io::stderr)
        .init();
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

    /// The window cannot be made narrower than the shell is designed for.
    ///
    /// Found by running `just dev` and looking at the result: the window came
    /// up at 1028 px, `.app { min-width: 1100px }` overflowed it, and
    /// `html, body { overflow: hidden }` meant the clipped right-hand end of
    /// the top strip -- the sources gear -- was simply unreachable. No
    /// scrollbar, no error, no way to get to it.
    ///
    /// Two files that cannot see each other, so the invariant is asserted
    /// here: whatever floor the stylesheet sets, the window may not go below
    /// it. Raising one without the other silently amputates the top strip
    /// again, and only at window widths a developer has to think to try.
    #[test]
    fn the_window_may_not_be_narrower_than_the_stylesheet_floor() {
        let config: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).expect("tauri.conf.json");
        let min_width = config["app"]["windows"][0]["minWidth"]
            .as_u64()
            .expect("app.windows[0].minWidth");

        let css = include_str!("../../../app/src/app.css");
        let floor: u64 = css
            .split_once(".app{")
            .and_then(|(_, rest)| rest.split_once("min-width:"))
            .and_then(|(_, rest)| rest.split_once("px"))
            .and_then(|(value, _)| value.trim().parse().ok())
            .expect("`.app` declares a `min-width` in px");

        assert!(
            min_width >= floor,
            "the window may shrink to {min_width}px but the shell needs {floor}px: \
             everything past the floor is clipped, and `overflow: hidden` means \
             there is no scrollbar to reach it with"
        );
    }

    /// The event names are one list in two languages. A rename on one side is
    /// a listener that silently never fires -- the failure mode this test
    /// exists to make loud.
    #[test]
    fn the_event_names_match_their_typescript_mirror() {
        let mirror = include_str!("../../../app/src/lib/ipc/index.ts");
        for name in [
            super::events::DB_STATE,
            super::events::SYNC_STATE,
            super::events::SOURCE_HEALTH,
            super::events::ACTIVITY_NEW,
        ] {
            assert!(
                mirror.contains(&format!("\"{name}\"")),
                "{name:?} is missing from app/src/lib/ipc/index.ts"
            );
        }
    }
}
