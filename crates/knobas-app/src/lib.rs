//! The knobas desktop shell: the Tauri application, the state its commands
//! share, and the database lifecycle bolted to the window's.
//!
//! The crate is a library with a one-line binary in front of it so the command
//! bodies stay reachable from `tests/`; a binary target cannot be linked
//! against by an integration test.

pub mod commands;
pub mod demo;

use std::sync::{Mutex, PoisonError};

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
        .setup(|app| {
            // `setup` is synchronous and the database is not; `block_on` is
            // deliberate, since every command would have to wait for the pool
            // anyway. No `emit` from here (M0 has no events): a listener
            // registered by the frontend cannot exist yet.
            //
            // It also means the window does not appear until the database is
            // up, and a first run -- which downloads and `initdb`s PostgreSQL
            // -- looks like a hang with no UI to say so. Bringing the database
            // up behind a loading state needs a window to put that state in,
            // which is M1 stream D; until then the panic hook below is what
            // makes a failure legible.
            let handle = app.handle().clone();
            tauri::async_runtime::block_on(async move { start_database(&handle).await })?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::ping,
            commands::demo_load,
            commands::sync_now,
            commands::search,
            commands::recent_activity,
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
                shutdown_database(app);
            }
        });
}

/// Start (or connect to) the database, migrate it, and hand it to Tauri.
async fn start_database(handle: &tauri::AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let root_dir = handle.path().app_data_dir()?.join("db");
    let existing_url = std::env::var(DB_URL_ENV)
        .ok()
        .filter(|url| !url.trim().is_empty());

    if existing_url.is_some() {
        tracing::info!("{DB_URL_ENV} is set: using an externally managed postgres");
    } else {
        tracing::info!(root_dir = %root_dir.display(), "starting the embedded postgres");
    }

    let db = knobas_db::EmbeddedDb::start(knobas_db::DbConfig {
        root_dir,
        existing_url,
    })
    .await?;
    knobas_db::migrate::run(db.pool()).await?;

    handle.manage(AppState {
        pool: db.pool().clone(),
        db: Mutex::new(Some(db)),
    });
    Ok(())
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
/// (see `knobas_db::EmbeddedDb::start`), reusing it as a warm start. Nothing
/// stops it but the run that started it, so a signal-killed session leaves a
/// PostgreSQL running until the next clean quit.
///
/// `db.stop()` closes the pool first, which waits for in-flight queries. A quit
/// during a long sync therefore blocks the exit for as long as that sync's
/// connection is busy. Bounding it -- a timeout, or cancelling the run --
/// belongs with the sync scheduler in M1 stream F.
fn shutdown_database(app: &tauri::AppHandle) {
    let Some(state) = app.try_state::<AppState>() else {
        // Startup failed before the state was managed; nothing was started.
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
