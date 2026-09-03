//! Everything the app knows about sources: which adapters exist, how a stored
//! configuration becomes a live one, and the scheduler's lifetime.
//!
//! `commands/sources.rs` is a set of shims over this module; every decision
//! lives here, with tests, because a `#[tauri::command]` cannot be called from
//! one.

pub mod crud;
pub mod demo;
pub mod paths;
pub mod progress;
pub mod registry;
pub mod write_queue;

// `pub(crate)`: the demo load command builds a `TauriEvents` of its own to
// announce its run's ending (#240); everything else reaches the adapter
// through the scheduler.
pub(crate) mod events;

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
    /// **The trait, not the concrete `Registry`.**
    ///
    /// `Registry` is a unit struct over a `const` table, so a `crud` written
    /// against it can only ever build adapters that work -- which left the
    /// *failed*-credential path in `set_secret` untestable, and a mutation
    /// that cleared the backoff after a rejected password survived the whole
    /// suite. That is P7's exact prohibition sitting on the path a user
    /// reaches by retyping a password that is still wrong. The trait already
    /// existed and `Registry` already implemented it; taking it here is what
    /// lets a test inject an adapter that refuses.
    pub registry: Arc<dyn knobas_sync::scheduler::AdapterRegistry>,
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

/// What the Add-source form submits. Carries the typed secret, which goes to
/// the keychain and never to Postgres (§14).
#[derive(Debug, serde::Deserialize)]
pub struct NewSource {
    pub id: String,
    pub adapter_kind: String,
    pub display_name: String,
    pub base_url: String,
    pub auth_kind: knobas_source::AuthMethod,
    pub config: serde_json::Value,
    pub secret: SecretInput,
    pub sync_interval_secs: u32,
    pub enabled: bool,
}

/// What *Edit source* may change.
///
/// `id` and `adapter_kind` are absent **on purpose**: the instance id is the
/// entity namespace, baked into every entity id, link and activity row, and is
/// therefore immutable (P10). `display_name` is the renameable one.
#[derive(Debug, Default, serde::Deserialize)]
pub struct SourcePatch {
    pub display_name: Option<String>,
    pub base_url: Option<String>,
    pub config: Option<serde_json::Value>,
    pub sync_interval_secs: Option<u32>,
    pub enabled: Option<bool>,
}

/// A typed credential on its way in. Never logged, never returned.
#[derive(Clone, serde::Deserialize)]
pub struct SecretInput {
    pub value: String,
}

/// Hand-written for the reason `knobas_secrets::Secret`'s is: a derived
/// `Debug` puts the credential into every `tracing` line and every panic
/// message that ever formats a struct containing one.
impl std::fmt::Debug for SecretInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SecretInput")
            .field("value", &"<redacted>")
            .finish()
    }
}

/// An unsaved (or saved) source to test a connection for.
///
/// `secret: None` **with** `source_id` set re-tests the stored credential,
/// which is what *Test connection* on an existing source does.
#[derive(Debug, serde::Deserialize)]
pub struct SourceDraft {
    pub source_id: Option<String>,
    pub adapter_kind: String,
    pub base_url: String,
    /// The frozen shape's `AuthMethod`, not an `Option`.
    ///
    /// An earlier draft widened it so a no-credential source could be tested.
    /// That was a mistake with teeth: for a **saved** source the draft's fields
    /// then described a configuration the scheduled run would not use, so *Test
    /// connection* could pass against auth the run never attempts. The fix is
    /// the other way round -- a saved source is tested against its **stored**
    /// configuration (see [`crud::test`]), and this field applies only to a
    /// draft that has not been saved yet. Every source the Add-source form can
    /// create has an auth method by construction ([`NewSource::auth_kind`]).
    pub auth_kind: knobas_source::AuthMethod,
    pub config: serde_json::Value,
    pub secret: Option<SecretInput>,
}

/// One row of the sources view.
#[derive(Debug, serde::Serialize)]
pub struct SourceSummary {
    pub id: String,
    pub adapter_kind: String,
    pub display_name: String,
    pub base_url: String,
    pub enabled: bool,
    pub sync_interval_secs: u32,
    pub config: serde_json::Value,
    pub health: knobas_sync::config::CredentialHealth,
    pub last_run: Option<knobas_sync::run_log::SyncRunRow>,
    pub next_run_at: Option<chrono::DateTime<chrono::Utc>>,
    pub item_count: i64,
    /// Which kind of credential this source authenticates with, or `None`
    /// when it needs none.
    ///
    /// The **same union** as [`NewSource::auth_kind`] and
    /// [`SourceDraft::auth_kind`], widened by `None` rather than spelled a
    /// second way: every source the Add-source form can create has an
    /// `AuthMethod` by construction, but a *stored* row need not -- the
    /// compiled-in mock reaches nothing and stores `auth_kind = 'none'`. That
    /// is what `None` is here, and it is also what an `auth_kind` column
    /// written by a newer knobas reads as, because
    /// [`AuthKind::from_db`](knobas_sync::config::AuthKind::from_db) refuses to
    /// guess. Both say the same thing to the sources view: there is no
    /// credential kind to name.
    ///
    /// Carries no secret and cannot: it is the *kind*, and the value itself
    /// lives in the OS keychain with no command that reads it back (§14).
    pub auth_kind: Option<knobas_source::AuthMethod>,
    /// From the adapter's descriptor template, so the sources view labels a
    /// source's kinds without a hardcoded table (§3a).
    pub kinds: Vec<knobas_source::KindInfo>,
}

/// What *Test connection* found.
#[derive(Debug, serde::Serialize)]
pub struct ConnectionReport {
    pub ok: bool,
    pub account: Option<String>,
    pub server_version: Option<String>,
    pub secret_expires_at: Option<chrono::DateTime<chrono::Utc>>,
    /// One line for the form, or `None` when it connected.
    ///
    /// Interfaces §2.2 spells this `Option<SourceError>`. A `String` plus
    /// [`code`](Self::code) instead, and it is the **one** declared deviation
    /// in this shape: serializing `SourceError` puts an untagged Rust enum on
    /// the bridge for the frontend to pattern-match, and P1 says branch on a
    /// code. `ConnectionInfo::detail` is deliberately *not* carried -- it
    /// would be a second undeclared field, and `account` plus
    /// `server_version` already say what it would.
    pub error: Option<String>,
    /// The class the UI branches on -- `unauthorized` is what turns *Test*
    /// into *Re-enter*, and a message is not something to branch on.
    pub code: Option<crate::IpcErrorCode>,
    pub elapsed_ms: u32,
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
    /// The sync engine could not get its own connections at bring-up.
    #[error("the sync engine's database: {0}")]
    Bringup(#[from] knobas_db::DbError),
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
        SourcesError::Source(Se::Unauthorized { .. }) => Code::Unauthorized,
        SourcesError::Source(Se::Unreachable(_)) => Code::Unreachable,
        SourcesError::Source(_) => Code::Internal,
        SourcesError::Sync(knobas_sync::SyncError::Source(Se::Unauthorized { .. })) => {
            Code::Unauthorized
        }
        SourcesError::Sync(knobas_sync::SyncError::Source(Se::Unreachable(_))) => Code::Unreachable,
        SourcesError::Sync(knobas_sync::SyncError::NotConfigured { .. }) => Code::NotFound,
        SourcesError::Sync(_) => Code::Internal,
        SourcesError::Trigger(Te::ShuttingDown) => Code::NotReady,
        SourcesError::Trigger(Te::Db(_)) | SourcesError::Db(_) | SourcesError::Bringup(_) => {
            Code::Internal
        }
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
/// [`SourcesError::Bringup`] if the scheduler cannot open its pool, and
/// [`SourcesError::Db`] if its startup reconciliation fails. Both mean the
/// database is not usable, which the caller reports as a failed bring-up.
pub async fn start<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    db: &knobas_db::EmbeddedDb,
) -> Result<(), SourcesError> {
    // Its own pool for the scheduler's bookkeeping; the runs themselves take a
    // connection each from the connector, outside every pool (§10.6(c)).
    let sync_pool = db.pool_for(knobas_sync::scheduler::SYNC_POOL_SIZE).await?;
    let profile = app.state::<crate::Profile>().inner().clone();
    let secrets = secret_store(&profile);
    let registry: Arc<dyn knobas_sync::scheduler::AdapterRegistry> = Arc::new(Registry::builtin());

    let scheduler = Scheduler::start(SchedulerDeps {
        pool: sync_pool,
        connections: Arc::new(DbConnections(db.connector())),
        registry: Arc::clone(&registry),
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

/// A scheduler over an already-open pool, for the IPC tests. **Tests only.**
///
/// It exists because `start` needs an `EmbeddedDb` -- the thing that hands out
/// connections outside every pool -- and `tests/ipc.rs` has a `test_pool`, not
/// a database handle. Everything else is the real thing: the real registry, the
/// real `TauriEvents`, real runs on real dedicated connections. Behind
/// `test-util` for the reason `AppState::over_pool` is: a production caller
/// would get a scheduler whose pool nothing owns.
///
/// # Errors
/// [`sqlx::Error`] if the startup reconciliation fails.
#[cfg(feature = "test-util")]
pub async fn test_scheduler<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    pool: PgPool,
) -> Result<Scheduler, sqlx::Error> {
    Scheduler::start(SchedulerDeps {
        connections: Arc::new(DbConnections(knobas_db::test_util::test_connector().await)),
        pool,
        registry: Arc::new(Registry::builtin()),
        secrets: Arc::new(knobas_secrets::MemoryStore::new()),
        events: Arc::new(TauriEvents::new(app.clone())),
    })
    .await
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
