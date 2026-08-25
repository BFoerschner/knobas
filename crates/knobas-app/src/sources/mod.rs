//! Everything the app knows about sources: which adapters exist, how a stored
//! configuration becomes a live one, and the scheduler's lifetime.
//!
//! `commands/sources.rs` is a set of shims over this module; every decision
//! lives here, with tests, because a `#[tauri::command]` cannot be called from
//! one.

pub mod demo;
pub mod registry;

mod events;

pub use registry::Registry;

use std::sync::Arc;

use knobas_secrets::SecretStore;
use knobas_sync::scheduler::{Scheduler, SchedulerDeps};
use sqlx::PgPool;
use tauri::Manager;

use events::TauriEvents;

/// Which secret store this process uses.
///
/// `KNOBAS_SECRET_STORE=memory` is the escape hatch for headless frontend QA
/// (roadmap §3) and for a `just dev` on a machine whose keychain prompts are in
/// the way: credentials then live for the length of the process and nothing is
/// written to the OS store. It is deliberately **not** the default anywhere.
pub const SECRET_STORE_ENV: &str = "KNOBAS_SECRET_STORE";

fn secret_store(profile: &crate::Profile) -> Arc<dyn SecretStore> {
    let memory = std::env::var(SECRET_STORE_ENV).is_ok_and(|v| v.eq_ignore_ascii_case("memory"));
    if memory {
        tracing::warn!("{SECRET_STORE_ENV}=memory: credentials will not survive this process");
        return Arc::new(knobas_secrets::MemoryStore::new());
    }
    // **`Profile::keychain_service()`, never a rebuilt string** (interfaces §3).
    // A second copy of that rule is a copy that can lose the `.demo` suffix
    // while still looking right, and a demo run reading the real credentials is
    // exactly what P13 exists to prevent.
    let store = knobas_secrets::KeyringStore::new(profile.keychain_service());
    tracing::info!(service = store.service(), "using the OS keychain");
    Arc::new(store)
}

/// The connections a sync run holds -- one each, outside every pool.
///
/// A newtype because the orphan rule forbids implementing `knobas-sync`'s trait
/// for `knobas-db`'s type from anywhere but one of those two crates, and
/// neither may depend on the other. See `RunConnections` for why a run must not
/// take a pooled connection (interfaces §10.6(c)).
struct DbConnections(knobas_db::embedded::Connector);

#[async_trait::async_trait]
impl knobas_sync::scheduler::RunConnections for DbConnections {
    async fn open(&self) -> Result<sqlx::PgConnection, sqlx::Error> {
        self.0.connect().await
    }
}

/// Everything the sources and sync commands share.
///
/// Managed by Tauri, and reached through [`state`] rather than declared as a
/// command argument -- see there.
pub struct SourcesState {
    /// The application pool -- for reads a command makes on the caller's thread.
    pub pool: PgPool,
    pub scheduler: Scheduler,
    pub secrets: Arc<dyn SecretStore>,
    pub registry: Arc<Registry>,
}

/// The sources state, or the one honest refusal for a call that beat bring-up.
///
/// **Never `State<'_, SourcesState>` as a command argument.** Carry-over
/// §10.6(a): a `#[tauri::command]` resolves every argument *before* its body
/// runs, and this state is managed only once the database is up -- so a command
/// declaring it is rejected by Tauri itself during bring-up with the bare
/// string `"state not managed"`, with no code for the frontend to branch on.
/// An `AppHandle` is always resolvable, so asking it for the state is what
/// keeps `NotReady` reachable. `commands::sources`'s
/// `no_command_takes_the_sources_state_directly` is the pin.
///
/// # Errors
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) while the sync
/// engine has not started.
pub fn state<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
) -> Result<tauri::State<'_, SourcesState>, crate::IpcError> {
    app.try_state::<SourcesState>()
        .ok_or_else(|| crate::IpcError::not_ready("the sync engine is still starting".to_owned()))
}

/// Why a sources operation did not happen.
#[derive(Debug, thiserror::Error)]
pub enum SourcesError {
    #[error("no source with id {0:?}")]
    NotFound(String),
    #[error("{0}")]
    Conflict(String),
    #[error("{0}")]
    Invalid(String),
    #[error("no adapter of kind {0:?} is compiled in")]
    UnknownAdapter(String),
    #[error("keychain: {0}")]
    Secret(#[from] knobas_secrets::SecretError),
    #[error("source: {0}")]
    Source(#[from] knobas_source::SourceError),
    #[error("sync: {0}")]
    Sync(#[from] knobas_sync::SyncError),
    #[error("scheduler: {0}")]
    Trigger(#[from] knobas_sync::scheduler::TriggerError),
    #[error("database: {0}")]
    Db(#[from] sqlx::Error),
}

/// Map a stream-F failure onto the frontend's branchable shape (P1).
///
/// The one branch that has to be right is `Unauthorized`: it is what makes the
/// UI offer *Re-enter password* instead of shrugging at a protocol error (§3).
#[must_use]
pub fn to_ipc(error: &SourcesError, source_id: Option<&str>) -> crate::IpcError {
    use crate::IpcErrorCode as Code;
    use knobas_source::SourceError as Se;
    use knobas_sync::scheduler::TriggerError as Te;

    let code = match error {
        SourcesError::NotFound(_) | SourcesError::Trigger(Te::UnknownSource(_)) => Code::NotFound,
        SourcesError::Conflict(_) => Code::Conflict,
        SourcesError::Invalid(_) | SourcesError::UnknownAdapter(_) => Code::Invalid,
        // A keychain item that is simply not there is the `missing_secret`
        // state the sources view offers *Re-enter* for -- the same offer a
        // rejected credential gets, and the same code.
        SourcesError::Secret(knobas_secrets::SecretError::NotFound) => Code::Unauthorized,
        SourcesError::Secret(_) => Code::Internal,
        SourcesError::Source(Se::Unauthorized) => Code::Unauthorized,
        SourcesError::Source(Se::Unreachable(_)) => Code::Unreachable,
        SourcesError::Source(_) => Code::Internal,
        SourcesError::Sync(knobas_sync::SyncError::Source(Se::Unauthorized)) => Code::Unauthorized,
        SourcesError::Sync(knobas_sync::SyncError::Source(Se::Unreachable(_))) => Code::Unreachable,
        SourcesError::Sync(knobas_sync::SyncError::NotConfigured { .. }) => Code::NotFound,
        SourcesError::Sync(_) => Code::Internal,
        SourcesError::Trigger(Te::ShuttingDown) => Code::NotReady,
        SourcesError::Trigger(Te::Db(_)) | SourcesError::Db(_) => Code::Internal,
    };
    let mapped = crate::IpcError::new(code, error);
    match source_id {
        Some(id) => mapped.with_source(id),
        None => mapped,
    }
}

/// Bring the sync engine up and manage its state.
///
/// Called from the bring-up task **after** the database is up and migrated. It
/// does not `emit` anything itself (roadmap §4 gotcha 9 -- the webview may not
/// be listening yet); the scheduler's own startup delay covers the first tick,
/// and `sync_status()` on mount is the authoritative read either way.
///
/// # Errors
/// [`SourcesError::Db`] if the scheduler's pool or its startup reconciliation
/// fails -- both mean the database is not usable, which the caller reports as a
/// failed bring-up.
pub async fn start<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    db: &knobas_db::EmbeddedDb,
) -> Result<(), SourcesError> {
    // Its own pool for the scheduler's bookkeeping; the runs themselves take a
    // connection each from the connector, outside every pool (§10.6(c)).
    let sync_pool = db
        .pool_for(knobas_sync::scheduler::SYNC_POOL_SIZE)
        .await
        .map_err(|error| SourcesError::Invalid(error.to_string()))?;
    let profile = app.state::<crate::Profile>().inner().clone();
    let secrets = secret_store(&profile);
    let registry = Arc::new(Registry::builtin());

    let scheduler = Scheduler::start(SchedulerDeps {
        pool: sync_pool,
        connections: Arc::new(DbConnections(db.connector())),
        registry: Arc::clone(&registry) as Arc<dyn knobas_sync::scheduler::AdapterRegistry>,
        secrets: Arc::clone(&secrets),
        events: Arc::new(TauriEvents::new(app.clone())),
    })
    .await?;

    // `manage` is the **last** step, and nothing between it and the scheduler's
    // creation can fail. Tauri's `manage` keeps the first value for a type, so
    // a second successful `start` in one process would silently leave a
    // scheduler on a closed pool behind; there is no such path today (a retry
    // only follows a bring-up that failed *before* this line), and this note is
    // what keeps it that way.
    if !app.manage(SourcesState {
        pool: db.pool().clone(),
        scheduler,
        secrets,
        registry,
    }) {
        tracing::error!(
            "the sync engine was started twice in one process; the second one is orphaned"
        );
    }
    Ok(())
}

/// Stop the scheduler, cancelling whatever is in flight.
///
/// **Must run before the database is stopped**: the runs hold connections
/// inside transactions, and closing the database under them is the stall the
/// carry-over describes. Safe to call twice -- `RunEvent::ExitRequested` can
/// fire more than once.
pub fn shutdown(app: &tauri::AppHandle) {
    let Some(state) = app.try_state::<SourcesState>() else {
        return; // bring-up failed before the scheduler existed
    };
    tracing::info!("stopping the sync scheduler");
    tauri::async_runtime::block_on(state.scheduler.shutdown());
}
