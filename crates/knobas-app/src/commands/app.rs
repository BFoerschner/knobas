//! App lifecycle and status -- stream D (interfaces §2.1).
//!
//! Everything here answers the question the window asks before it can show
//! anything: *is the database up, and which knobas is this?* That question has
//! to be answerable **while PostgreSQL is still starting**, which is why
//! nothing in this module takes `State<'_, AppState>` -- see [`Lifecycle`].

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use knobas_sync::scheduler::AdapterRegistry;
use tauri::Manager;

use crate::sources::SecretStoreKind;
use crate::{AppState, IpcError, Profile};

/// How far the database has got.
///
/// Interfaces §2.1. Tagged on `state`, so TypeScript reads it as a
/// discriminated union and `detail` and `message` are reachable only after
/// the check that identifies which state this is.
///
/// - `starting`: coming up.
/// - `migrating`: up, and running migrations.
/// - `ready`: up, migrated, and answering queries.
/// - `failed`: it will not come up.
///
/// The states are documented here and not on each variant because the
/// generated TypeScript (`app/src/lib/ipc/generated/app.ts`) carries a type's
/// and a field's doc but not a variant's (#1325).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, ts_rs::TS)]
#[serde(tag = "state", rename_all = "snake_case")]
#[ts(export_to = "app.ts")]
pub enum DbState {
    Starting {
        /// A line for the boot screen -- "downloading PostgreSQL 18.6", which
        /// on a first run is tens of seconds.
        detail: Option<String>,
    },
    Migrating,
    Ready,
    Failed {
        /// Why, shown to the user as text.
        message: String,
    },
}

/// What the shell needs to decide what to draw.
#[derive(Debug, serde::Serialize, ts_rs::TS)]
#[ts(export_to = "app.ts")]
pub struct AppStatus {
    pub db: DbState,
    /// No source configured and the first run was never completed (§14a).
    pub first_run: bool,
    /// The `--demo` profile: its own data dir, port and keychain suffix (P13).
    pub demo: bool,
    pub source_count: u32,
    pub app_version: String,
    /// Credentials live in this process's memory, not the OS keychain, and
    /// are gone when it exits: `KNOBAS_SECRET_STORE=memory`, the headless-QA
    /// store (#1585). A source's page says so instead of naming the keychain.
    pub secrets_in_memory: bool,
}

/// The database's bring-up state, and the shared state once there is one.
///
/// Managed at **build** time, before `setup` runs, so it is the one piece of
/// managed state a command can always count on.
///
/// # Why commands go through this instead of `State<'_, AppState>`
///
/// Carry-over §10.6(a). A `#[tauri::command]` resolves *every* argument before
/// its body runs, and `AppState` exists only once PostgreSQL is up. A command
/// declaring `State<'_, AppState>` is therefore rejected by Tauri itself
/// during a slow bring-up, with the bare string `"state not managed"` -- no
/// code, nothing the frontend can branch on, and `IpcErrorCode::NotReady`
/// unreachable despite existing for exactly this.
///
/// That was latent while `setup` blocked on bring-up behind a hidden window.
/// Making bring-up asynchronous is what makes a window able to call early, so
/// the two land together: `AppState` lives *inside* this type and is reached
/// through [`Lifecycle::pool`], which answers `not_ready` in one place rather
/// than in a guard sprinkled across every command.
pub struct Lifecycle {
    db: Mutex<DbState>,
    /// `Arc`, and not a `OnceLock`: a failed bring-up can be retried, so this
    /// slot is written more than once over a process's life.
    app: Mutex<Option<Arc<AppState>>>,
    /// The server this process started or adopted, from the moment it is up
    /// until the exit handler stops it (#1634): held here and not on
    /// [`AppState`], which exists only once bring-up has finished, so a quit
    /// during the migration stops the server too. `Option` because the stop
    /// consumes the handle, and the take is what makes the second of the two
    /// exit events a no-op.
    server: Mutex<Option<knobas_db::EmbeddedDb>>,
    /// Counts the servers [`hold_server`](Self::hold_server) was given, so a
    /// watch on one server (#1779) never acts on the next.
    generation: AtomicU64,
    /// The shared state, set aside while the server that died under it
    /// (#1779) is down: a command answers `not_ready` meanwhile, and the
    /// restart puts it back, pools and engines as they were.
    suspended: Mutex<Option<Arc<AppState>>>,
    /// Held by a restart from first to last (#1779), and taken by the exit
    /// handler before it takes the server, so a quit during a restart waits
    /// for it and then stops what it started.
    restart_gate: tokio::sync::Mutex<()>,
    /// The adapter registry every read takes, as the trait object (#1321).
    /// Here rather than on `AppState`, because the reads that answer before
    /// the database is up (the Add-source form's adapter list) need it too.
    registry: Arc<dyn AdapterRegistry>,
}

impl Default for Lifecycle {
    fn default() -> Self {
        Self::new()
    }
}

impl Lifecycle {
    /// A lifecycle that has not started anything yet, over the process's
    /// registry ([`Registry::for_process`](crate::sources::Registry::for_process)):
    /// the list bring-up fills with the installed external adapters and the
    /// sources state shares.
    #[must_use]
    pub fn new() -> Self {
        Self::with_registry(Arc::new(crate::sources::Registry::for_process()))
    }

    /// [`new`](Self::new), over `registry` instead of the process's: what a
    /// test gives a command to witness it against adapters it controls.
    #[must_use]
    pub fn with_registry(registry: Arc<dyn AdapterRegistry>) -> Self {
        Self {
            db: Mutex::new(DbState::Starting { detail: None }),
            app: Mutex::new(None),
            server: Mutex::new(None),
            generation: AtomicU64::new(0),
            suspended: Mutex::new(None),
            restart_gate: tokio::sync::Mutex::new(()),
            registry,
        }
    }

    /// The adapter registry. Answers before bring-up: it needs no database.
    #[must_use]
    pub fn registry(&self) -> &dyn AdapterRegistry {
        self.registry.as_ref()
    }

    /// Where bring-up has got to.
    #[must_use]
    pub fn get(&self) -> DbState {
        self.db
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Record a transition. The caller emits `db:state` after it.
    pub fn set(&self, state: DbState) {
        *self.db.lock().unwrap_or_else(PoisonError::into_inner) = state;
    }

    /// Claim a retry: move `Failed` back to `Starting`, and say whether this
    /// call is the one that now owns a bring-up.
    ///
    /// The check and the write are one critical section on purpose. Two quick
    /// clicks on *Retry* would otherwise both read `Failed` and both spawn,
    /// which is two `initdb`s racing for one data directory.
    pub fn begin_retry(&self) -> bool {
        let mut db = self.db.lock().unwrap_or_else(PoisonError::into_inner);
        if matches!(*db, DbState::Failed { .. }) {
            *db = DbState::Starting { detail: None };
            true
        } else {
            false
        }
    }

    /// Hand over the shared state, once the pool is live and migrated.
    pub fn install(&self, state: AppState) {
        *self.app.lock().unwrap_or_else(PoisonError::into_inner) = Some(Arc::new(state));
    }

    /// Hold `db` until [`take_server`](Self::take_server): bring-up's first
    /// step once the server is up, so the exit handler stops it whatever
    /// stage bring-up reached. A test that installs an [`AppState`] by hand
    /// holds its server here for the same reason.
    pub fn hold_server(&self, db: knobas_db::EmbeddedDb) {
        let mut server = self.server.lock().unwrap_or_else(PoisonError::into_inner);
        *server = Some(db);
        self.generation.fetch_add(1, Ordering::SeqCst);
    }

    /// Which server [`hold_server`](Self::hold_server) holds: a number that
    /// changes each time it is given one.
    pub(crate) fn generation(&self) -> u64 {
        self.generation.load(Ordering::SeqCst)
    }

    /// Whether a server is held: false once a quit or a failed bring-up took
    /// it.
    pub(crate) fn holds_server(&self) -> bool {
        self.server
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .is_some()
    }

    /// The server of `generation` died under a `ready` window (#1779): set
    /// the shared state aside and move to `failed` with `message`, so the
    /// boot screen offers *Retry*. The server stays held, and with it the
    /// profile's ownership lock, so no second launch is let in meanwhile.
    ///
    /// False, and nothing changes, when that server is no longer the one
    /// held (a quit took it, or it was restarted) or the window is not
    /// `ready`. The caller emits the new state when this answers true.
    pub(crate) fn server_died(&self, generation: u64, message: String) -> bool {
        let server = self.server.lock().unwrap_or_else(PoisonError::into_inner);
        let mut db = self.db.lock().unwrap_or_else(PoisonError::into_inner);
        if server.is_none() || self.generation() != generation || *db != DbState::Ready {
            return false;
        }
        let state = self
            .app
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        *self
            .suspended
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = state;
        *db = DbState::Failed { message };
        true
    }

    /// Whether the failure *Retry* answers is a server that died after
    /// `ready` (#1779), which a restart in place recovers, rather than a
    /// bring-up that never finished.
    pub(crate) fn server_is_down(&self) -> bool {
        self.suspended
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .is_some()
    }

    /// Put back the shared state [`server_died`](Self::server_died) set
    /// aside, once its server is up again.
    pub(crate) fn resume(&self) {
        let state = self
            .suspended
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        if let Some(state) = state {
            *self.app.lock().unwrap_or_else(PoisonError::into_inner) = Some(state);
        }
    }

    /// The gate a restart holds from first to last; see the field.
    pub(crate) fn restart_gate(&self) -> &tokio::sync::Mutex<()> {
        &self.restart_gate
    }

    /// The server [`hold_server`](Self::hold_server) was given, once: for the
    /// one caller that stops it -- the exit handler, or a bring-up that failed.
    pub(crate) fn take_server(&self) -> Option<knobas_db::EmbeddedDb> {
        self.server
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
    }

    /// The shared state, if the database came up.
    ///
    /// An `Arc` clone rather than a borrow: the guard must not be held across
    /// an await, and every caller here is in an async command.
    #[must_use]
    pub fn app_state(&self) -> Option<Arc<AppState>> {
        self.app
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// The pool, or the one honest refusal for a call that beat bring-up.
    ///
    /// # Errors
    ///
    /// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) while there is
    /// no database. Retrying later is the right move, and the frontend knows
    /// that from the code without parsing the message.
    pub fn pool(&self) -> Result<sqlx::PgPool, IpcError> {
        match self.app_state() {
            // A `PgPool` is a handle: cloning it shares the same connections.
            Some(state) => Ok(state.pool.clone()),
            None => Err(self.not_ready()),
        }
    }

    /// The refusal for a call that beat bring-up, or followed a failed one.
    fn not_ready(&self) -> IpcError {
        IpcError::not_ready(match self.get() {
            // It did start, and then stopped (#1779): the message says so.
            DbState::Failed { message } if self.server_is_down() => message,
            DbState::Failed { message } => format!("the database did not start: {message}"),
            _ => "the database is still starting".to_owned(),
        })
    }
}

/// Liveness probe. Answers as soon as the webview can call, which is now
/// *before* the database is up -- so a successful `ping` says the backend is
/// running and nothing about PostgreSQL. Ask [`app_status`] for that.
#[tauri::command]
pub fn ping() -> &'static str {
    "pong"
}

/// Everything the shell needs to draw its current state.
///
/// Deliberately reads no `State<'_, AppState>`: it is the one command whose
/// whole job is to answer while the database is still coming up.
///
/// Generic over the runtime for the reason `sync_now` is (§10.2): a bare
/// `tauri::AppHandle` means `AppHandle<Wry>`, and a command taking one cannot
/// be registered on the `tauri::test` mock app at all -- which would put this
/// command out of reach of `tests/it/ipc.rs`.
#[tauri::command]
pub async fn app_status<R: tauri::Runtime>(app: tauri::AppHandle<R>) -> AppStatus {
    let db = app.state::<Lifecycle>().get();
    let demo = app.state::<Profile>().demo;
    let secrets_in_memory = *app.state::<SecretStoreKind>() == SecretStoreKind::Memory;
    let app_version = app.package_info().version.to_string();

    // Counted live rather than cached, so adding a source is reflected without
    // a restart. Before the pool exists there is nothing to count -- and "0
    // sources while starting" is *not* the same claim as "this is a first
    // run", so both stay false-y until the database can answer.
    let pool = app.state::<Lifecycle>().app_state();
    let (source_count, first_run) = match pool {
        Some(state) => crate::settings::sources_and_first_run(&state.pool)
            .await
            .unwrap_or((0, false)),
        None => (0, false),
    };

    AppStatus {
        db,
        first_run,
        demo,
        source_count,
        app_version,
        secrets_in_memory,
    }
}

/// Arm event emission: the frontend has its listeners up.
///
/// Roadmap §4 gotcha 9 -- the backend must not `emit` before the webview is
/// listening, and a `db:state` fired during startup would otherwise be lost.
/// The frontend registers its `listen()`s, calls this, and gets the current
/// state replayed.
///
/// # Errors
///
/// None any more: the replay is best effort, like every emit
/// ([`crate::events::emit`]). The `Result` is the frozen IPC shape.
#[tauri::command]
pub fn frontend_ready<R: tauri::Runtime>(app: tauri::AppHandle<R>) -> Result<(), IpcError> {
    let state = app.state::<Lifecycle>().get();
    crate::events::emit(&app, None, crate::events::DB_STATE, state);
    Ok(())
}

/// Start the database again after a failure.
///
/// The *Retry* button on the boot screen. A retry that only re-polled would
/// redraw the same failure for ever -- the database has to be asked to start
/// again, and only this side can do that.
///
/// Idempotent by construction: [`Lifecycle::begin_retry`] hands the bring-up
/// to exactly one caller, so a second click while the first attempt is still
/// running succeeds and starts nothing. That is the right answer to "start the
/// database" when it is already starting.
///
/// # Errors
///
/// Never, today. It returns a `Result` because every command does and because
/// a future bring-up that can refuse (a profile whose data directory is gone)
/// has somewhere to say so.
#[tauri::command]
pub fn retry_database<R: tauri::Runtime>(app: tauri::AppHandle<R>) -> Result<(), IpcError> {
    let lifecycle = app.state::<Lifecycle>();
    if lifecycle.begin_retry() {
        if lifecycle.server_is_down() {
            // Up, migrated and running engines until its server died
            // (#1779): starting the server again is the whole recovery.
            crate::spawn_restart(app.clone());
        } else {
            crate::spawn_bring_up(app.clone());
        }
    }
    Ok(())
}

/// Record that the §14a wizard finished.
///
/// `app_status.first_run` is `source_count == 0 && !completed` -- two facts,
/// because they are different ones. Without this row, a person who finished
/// the wizard and later removed their only source would be shown the wizard
/// again, which is the app forgetting something it was told.
///
/// It lives in `knobas.setting` rather than in the webview's `localStorage`
/// because §14 makes the database the thing an export carries: a flag outside
/// it would not survive the export/import round trip the milestone is built
/// around.
///
/// Idempotent by `on conflict`: finishing the wizard twice -- a double click,
/// a second window -- is the same as finishing it once.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) while PostgreSQL
/// is still coming up, and
/// [`IpcErrorCode::Internal`](crate::IpcErrorCode::Internal) if the write
/// fails.
#[tauri::command]
pub async fn complete_first_run<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
) -> Result<(), IpcError> {
    // Through `Lifecycle`, not `State<'_, AppState>`: a command declaring the
    // latter is rejected by Tauri itself during bring-up with a bare "state
    // not managed" that the frontend cannot branch on. See [`Lifecycle`].
    let pool = app.state::<Lifecycle>().pool()?;
    crate::settings::complete_first_run(&pool)
        .await
        .map_err(IpcError::internal)
}

/// *Pages kept open* (#2247): how many pages the shell keeps mounted and hidden
/// besides the one in front, 5 until set (`crate::settings::pages_kept_open`).
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) before bring-up,
/// [`IpcErrorCode::Internal`](crate::IpcErrorCode::Internal) if the read fails.
#[tauri::command]
pub async fn pages_kept_open<R: tauri::Runtime>(app: tauri::AppHandle<R>) -> Result<u32, IpcError> {
    let pool = app.state::<Lifecycle>().pool()?;
    crate::settings::pages_kept_open(&pool).await
}

/// Store *Pages kept open*, held to 15, and answer what is now stored, so
/// Settings › Performance draws what the database holds rather than what was
/// typed -- the rule `set_passive_attribution` follows.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) before bring-up,
/// [`IpcErrorCode::Internal`](crate::IpcErrorCode::Internal) if the write fails.
#[tauri::command]
pub async fn set_pages_kept_open<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    count: u32,
) -> Result<u32, IpcError> {
    let pool = app.state::<Lifecycle>().pool()?;
    crate::settings::set_pages_kept_open(&pool, count).await
}

/// The command renderers saved in Settings › Renderers (#2262), in order.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) before bring-up,
/// [`IpcErrorCode::Internal`](crate::IpcErrorCode::Internal) if the read fails.
#[tauri::command]
pub async fn user_renderers<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
) -> Result<Vec<crate::renderers::UserRenderer>, IpcError> {
    let pool = app.state::<Lifecycle>().pool()?;
    crate::renderers::list(&pool).await
}

/// Save the whole list of command renderers and answer it as stored: ids
/// made from new names, fence languages lowercase, timeouts held to 1-120 s.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) before bring-up,
/// [`IpcErrorCode::Invalid`](crate::IpcErrorCode::Invalid) for a renderer with
/// no name or a command that cannot be run,
/// [`IpcErrorCode::Internal`](crate::IpcErrorCode::Internal) if the write fails.
#[tauri::command]
pub async fn set_user_renderers<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    renderers: Vec<crate::renderers::UserRendererDraft>,
) -> Result<Vec<crate::renderers::UserRenderer>, IpcError> {
    let pool = app.state::<Lifecycle>().pool()?;
    crate::renderers::save(&pool, &renderers).await
}

/// Run saved renderer `id` with `content` on its stdin (ADR-0033): the
/// program and its arguments are the saved ones, never the webview's, and
/// no shell runs. A failure, a timeout or too much output is an answer.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) before bring-up,
/// [`IpcErrorCode::Invalid`](crate::IpcErrorCode::Invalid) for an unknown id
/// or bytes that are not base64,
/// [`IpcErrorCode::Internal`](crate::IpcErrorCode::Internal) if the read fails.
#[tauri::command]
pub async fn run_renderer<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    id: String,
    content: crate::renderers::RenderContent,
) -> Result<crate::renderers::RenderRun, IpcError> {
    let pool = app.state::<Lifecycle>().pool()?;
    crate::renderers::run(&pool, &id, content).await
}

/// The default renderer per file pattern (#2263), in order.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) before bring-up,
/// [`IpcErrorCode::Internal`](crate::IpcErrorCode::Internal) if the read fails.
#[tauri::command]
pub async fn renderer_defaults<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
) -> Result<Vec<crate::renderers::RendererDefault>, IpcError> {
    let pool = app.state::<Lifecycle>().pool()?;
    crate::renderers::defaults(&pool).await
}

/// Save the whole list of default renderers per file pattern and answer it
/// as stored, trimmed.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) before bring-up,
/// [`IpcErrorCode::Invalid`](crate::IpcErrorCode::Invalid) for a row with no
/// pattern, or neither a renderer nor a pipeline, or a pattern listed twice,
/// [`IpcErrorCode::Internal`](crate::IpcErrorCode::Internal) if the write fails.
#[tauri::command]
pub async fn set_renderer_defaults<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    defaults: Vec<crate::renderers::RendererDefault>,
) -> Result<Vec<crate::renderers::RendererDefault>, IpcError> {
    let pool = app.state::<Lifecycle>().pool()?;
    crate::renderers::save_defaults(&pool, &defaults).await
}

/// The command steps saved in Settings › Renderers (#2265), in order.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) before bring-up,
/// [`IpcErrorCode::Internal`](crate::IpcErrorCode::Internal) if the read fails.
#[tauri::command]
pub async fn command_steps<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
) -> Result<Vec<crate::file_pipeline::saved::CommandStep>, IpcError> {
    let pool = app.state::<Lifecycle>().pool()?;
    crate::file_pipeline::saved::command_steps(&pool).await
}

/// Save the whole list of command steps and answer it as stored: ids made
/// from new names, timeouts held to 1-120 s.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) before bring-up,
/// [`IpcErrorCode::Invalid`](crate::IpcErrorCode::Invalid) for a step with no
/// name or a command that cannot be run,
/// [`IpcErrorCode::Internal`](crate::IpcErrorCode::Internal) if the write fails.
#[tauri::command]
pub async fn set_command_steps<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    steps: Vec<crate::file_pipeline::saved::CommandStepDraft>,
) -> Result<Vec<crate::file_pipeline::saved::CommandStep>, IpcError> {
    let pool = app.state::<Lifecycle>().pool()?;
    crate::file_pipeline::saved::save_command_steps(&pool, &steps).await
}

/// The named pipelines (#2265), in order.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) before bring-up,
/// [`IpcErrorCode::Internal`](crate::IpcErrorCode::Internal) if the read fails.
#[tauri::command]
pub async fn named_pipelines<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
) -> Result<Vec<crate::file_pipeline::saved::NamedPipeline>, IpcError> {
    let pool = app.state::<Lifecycle>().pool()?;
    crate::file_pipeline::saved::named_pipelines(&pool).await
}

/// Save the whole list of named pipelines and answer it as stored.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) before bring-up,
/// [`IpcErrorCode::Invalid`](crate::IpcErrorCode::Invalid) for a pipeline with
/// no name or no steps, or a name used twice,
/// [`IpcErrorCode::Internal`](crate::IpcErrorCode::Internal) if the write fails.
#[tauri::command]
pub async fn set_named_pipelines<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    pipelines: Vec<crate::file_pipeline::saved::NamedPipelineDraft>,
) -> Result<Vec<crate::file_pipeline::saved::NamedPipeline>, IpcError> {
    let pool = app.state::<Lifecycle>().pool()?;
    crate::file_pipeline::saved::save_named_pipelines(&pool, &pipelines).await
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The frontend narrows on `state` and reads `message` only inside the
    /// `failed` arm. A different tag, or a payload that moved, is a union that
    /// type-checks in TypeScript and is wrong at run time.
    #[test]
    fn db_state_serialises_as_the_contract_declares() {
        assert_eq!(
            serde_json::to_value(DbState::Starting {
                detail: Some("downloading postgres".into())
            })
            .unwrap(),
            serde_json::json!({ "state": "starting", "detail": "downloading postgres" })
        );
        assert_eq!(
            serde_json::to_value(DbState::Starting { detail: None }).unwrap(),
            serde_json::json!({ "state": "starting", "detail": null })
        );
        assert_eq!(
            serde_json::to_value(DbState::Migrating).unwrap(),
            serde_json::json!({ "state": "migrating" })
        );
        assert_eq!(
            serde_json::to_value(DbState::Ready).unwrap(),
            serde_json::json!({ "state": "ready" })
        );
        assert_eq!(
            serde_json::to_value(DbState::Failed {
                message: "port 5432 in use".into()
            })
            .unwrap(),
            serde_json::json!({ "state": "failed", "message": "port 5432 in use" })
        );
    }

    /// Every command in this module is callable from the frontend, and every
    /// command the mirror calls is one this module registers.
    ///
    /// The invoke functions are hand-written (contract §2.6), so a command
    /// added on the Rust side alone compiles, lints and tests green while
    /// being unreachable from the window -- and a command *renamed* on the
    /// Rust side alone leaves the mirror invoking a name that no longer
    /// exists, which fails only when a person clicks the button.
    ///
    /// Matched on the `invoke<..>("<name>"` call sites, because that is the
    /// thing that actually has to agree: a TS function may be called
    /// anything, and a doc comment naming a command calls nothing (#1984).
    /// The list is `lib.rs`'s handler list; `tests/it/wiring.rs` pins that
    /// every command declared here is on it.
    #[test]
    fn every_command_in_this_module_has_a_typescript_mirror() {
        let mirror = include_str!("../../../../app/src/lib/ipc/app.ts");
        crate::commands::mirror::assert_mirrors("app", mirror, "app/src/lib/ipc/app.ts", 16);
    }

    /// The mirror sends each argument by its Rust name (#2262).
    #[test]
    fn the_mirror_sends_the_renderer_arguments() {
        let mirror = include_str!("../../../../app/src/lib/ipc/app.ts");
        for (command, args) in [
            ("set_user_renderers", &["renderers"][..]),
            ("run_renderer", &["id", "content"][..]),
            ("set_renderer_defaults", &["defaults"][..]),
            ("set_command_steps", &["steps"][..]),
            ("set_named_pipelines", &["pipelines"][..]),
        ] {
            let invocation = crate::commands::mirror::invocation(mirror, command);
            let keys = crate::commands::mirror::keys(invocation);
            for arg in args {
                assert!(keys.contains(arg), "{command} without {arg}: {invocation}");
            }
        }
    }

    /// The mirror sends `count`, the argument's name: a mirror sending
    /// anything else arrives with the value missing (#2247).
    #[test]
    fn the_mirror_sends_the_count_of_pages_kept_open() {
        let mirror = include_str!("../../../../app/src/lib/ipc/app.ts");
        let invocation = crate::commands::mirror::invocation(mirror, "set_pages_kept_open");
        assert!(
            crate::commands::mirror::keys(invocation).contains(&"count"),
            "the mirror calls set_pages_kept_open without passing count: {invocation}"
        );
    }

    #[test]
    fn lifecycle_starts_starting_and_remembers_the_last_set() {
        let life = Lifecycle::new();
        assert_eq!(life.get(), DbState::Starting { detail: None });

        life.set(DbState::Starting {
            detail: Some("downloading postgres".into()),
        });
        assert_eq!(
            life.get(),
            DbState::Starting {
                detail: Some("downloading postgres".into())
            }
        );

        life.set(DbState::Migrating);
        assert_eq!(life.get(), DbState::Migrating);
    }

    /// The carry-over, discharged: a call that beats bring-up gets a code the
    /// frontend can branch on, and not Tauri's bare `"state not managed"`.
    #[test]
    fn a_call_before_the_database_is_up_is_not_ready() {
        let life = Lifecycle::new();
        let error = life.pool().expect_err("there is no pool yet");
        assert_eq!(error.code, crate::IpcErrorCode::NotReady);
        assert!(error.message.contains("starting"), "{}", error.message);
    }

    /// One retry, however many callers ask for one.
    #[test]
    fn a_retry_is_claimed_once_and_only_out_of_a_failure() {
        let life = Lifecycle::new();

        // Nothing to retry while it is still coming up: a second `initdb` on
        // the same data directory is the failure this prevents.
        assert!(!life.begin_retry(), "a starting database is not retried");

        life.set(DbState::Failed {
            message: "port 5432 in use".to_owned(),
        });
        assert!(life.begin_retry(), "a failure can be retried");
        assert_eq!(
            life.get(),
            DbState::Starting { detail: None },
            "the retry moves the state, so the boot screen stops saying `failed`"
        );
        assert!(
            !life.begin_retry(),
            "the second click must not start a second bring-up"
        );

        life.set(DbState::Ready);
        assert!(!life.begin_retry(), "a live database is not restarted");
    }

    /// ...and a bring-up that *failed* says so, rather than repeating "still
    /// starting" for the rest of the session.
    #[test]
    fn a_failed_bring_up_says_why_in_the_refusal() {
        let life = Lifecycle::new();
        life.set(DbState::Failed {
            message: "port 5432 in use".to_owned(),
        });
        let error = life.pool().expect_err("there is no pool");
        assert_eq!(error.code, crate::IpcErrorCode::NotReady);
        assert!(
            error.message.contains("port 5432 in use"),
            "{}",
            error.message
        );
    }
}
