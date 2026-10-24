//! Everything the app knows about sources: which adapters exist, how a stored
//! configuration becomes a live one, and the scheduler's lifetime.
//!
//! `commands/sources.rs` is a set of shims over this module; every decision
//! lives here, with tests, because a `#[tauri::command]` cannot be called from
//! one.

pub mod crud;
pub mod demo;
pub mod install;
pub mod installed;
pub mod paths;
pub mod progress;
pub mod registry;
pub mod remote;
pub mod write_queue;

// `pub(crate)`: the demo load command builds a `TauriEvents` of its own to
// announce its run's ending (#240); everything else reaches the adapter
// through the scheduler.
pub(crate) mod events;

pub use registry::Registry;

use std::sync::Arc;

use knobas_secrets::SecretStore;
use knobas_sync::scheduler::{Scheduler, SchedulerDeps, SchedulerTiming};
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

/// Where this process keeps credentials, read once from [`SECRET_STORE_ENV`]
/// at start and managed, so the store bring-up builds and what `app_status`
/// reports to the frontend are one decision (#1585).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretStoreKind {
    /// The OS keychain, under the profile's service name. The default.
    Keychain,
    /// [`knobas_secrets::MemoryStore`]: gone when the process exits.
    Memory,
}

impl SecretStoreKind {
    /// The store [`SECRET_STORE_ENV`] asks for.
    #[must_use]
    pub fn from_env() -> Self {
        Self::from_value(std::env::var(SECRET_STORE_ENV).ok().as_deref())
    }

    fn from_value(value: Option<&str>) -> Self {
        if value.is_some_and(|v| v.eq_ignore_ascii_case("memory")) {
            Self::Memory
        } else {
            Self::Keychain
        }
    }
}

fn secret_store(profile: &crate::Profile, kind: SecretStoreKind) -> Arc<dyn SecretStore> {
    if kind == SecretStoreKind::Memory {
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
pub(crate) struct DbConnections(pub(crate) knobas_db::embedded::Connector);

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
///
/// `Clone` is cheap -- a pool and three handles -- and exists for one caller:
/// the background send of a queued write (issue #752) takes a copy into its
/// task, because the command that queued it has already answered.
#[derive(Clone)]
pub struct SourcesState {
    /// The application pool -- for reads a command makes on the caller's thread.
    pub pool: PgPool,
    pub scheduler: Scheduler,
    pub secrets: Arc<dyn SecretStore>,
    /// **The trait, not the concrete `Registry`.**
    ///
    /// `Registry` is a `const` table plus the installed adapters, which load
    /// only from a real component, so a `crud` written
    /// against it can only ever build adapters that work -- which left the
    /// *failed*-credential path in `set_secret` untestable, and a mutation
    /// that cleared the backoff after a rejected password survived the whole
    /// suite. That is P7's exact prohibition sitting on the path a user
    /// reaches by retyping a password that is still wrong. The trait already
    /// existed and `Registry` already implemented it; taking it here is what
    /// lets a test inject an adapter that refuses.
    pub registry: Arc<dyn knobas_sync::scheduler::AdapterRegistry>,
}

impl SourcesState {
    /// The registry as the concrete list the install flow writes to, sharing
    /// the list the scheduler reads. `None` when the state was built over a
    /// registry that is not [`Registry`] (a test's), which cannot be
    /// installed into.
    #[must_use]
    pub fn installable(&self) -> Option<Registry> {
        self.registry.as_any()?.downcast_ref::<Registry>().cloned()
    }
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
#[derive(Debug, serde::Deserialize, ts_rs::TS)]
#[ts(export_to = "sources.ts")]
pub struct NewSource {
    pub id: String,
    pub adapter_kind: String,
    pub display_name: String,
    pub base_url: String,
    pub auth_kind: knobas_source::AuthMethod,
    #[ts(type = "unknown")]
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
#[derive(Debug, Default, serde::Deserialize, ts_rs::TS)]
#[ts(export_to = "sources.ts")]
pub struct SourcePatch {
    #[ts(optional = nullable)]
    pub display_name: Option<String>,
    #[ts(optional = nullable)]
    pub base_url: Option<String>,
    #[ts(type = "unknown", optional)]
    pub config: Option<serde_json::Value>,
    #[ts(optional = nullable)]
    pub sync_interval_secs: Option<u32>,
    #[ts(optional = nullable)]
    pub enabled: Option<bool>,
}

/// A typed credential on its way in. Never logged, never returned.
///
/// Both fields are optional and **absent means "keep what is stored"**
/// (issue #452). That is one rule, and it is read in both directions: a reader
/// adding an account to an Uptime Kuma cannot retype an API key Kuma showed
/// them once, and a reader replacing an expired key must not silently lose the
/// account beside it. `add_source` is the one caller for which "keep" has no
/// meaning -- nothing is stored yet -- and it refuses a `value` of `None` by
/// name.
#[derive(Clone, Default, serde::Deserialize, ts_rs::TS)]
#[ts(export_to = "sources.ts")]
pub struct SecretInput {
    #[serde(default)]
    #[ts(optional = nullable)]
    pub value: Option<String>,
    /// The optional second credential of the version-2 keychain envelope: a
    /// username and password stored *beside* [`value`](Self::value), not
    /// instead of it. Only Uptime Kuma reads one today (spec #427, story 69),
    /// and only for its write half: the API key opens `/metrics`, the account
    /// opens socket.io, and a source with only the key reads every monitor
    /// and pauses none of them.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub account: Option<AccountInput>,
}

/// A typed account on its way in -- the wire twin of
/// [`knobas_source::instance::Account`].
///
/// Its own type rather than that one re-used, because this is an IPC argument
/// and the SPI's struct is a frozen surface: the day a stored account grows a
/// field, the form and the keychain should be able to move one at a time.
#[derive(Clone, serde::Deserialize, ts_rs::TS)]
#[ts(export_to = "sources.ts", rename = "SecretAccount")]
pub struct AccountInput {
    pub username: String,
    pub password: String,
}

impl SecretInput {
    /// A credential typed with no account beside it -- what every form but
    /// Uptime Kuma's submits, and what every caller wrote before issue #452.
    #[must_use]
    pub fn of(value: impl Into<String>) -> Self {
        Self {
            value: Some(value.into()),
            account: None,
        }
    }
}

impl From<AccountInput> for knobas_source::instance::Account {
    fn from(input: AccountInput) -> Self {
        Self {
            username: input.username,
            password: input.password,
        }
    }
}

/// Hand-written for the reason `knobas_secrets::Secret`'s is: a derived
/// `Debug` puts the credential into every `tracing` line and every panic
/// message that ever formats a struct containing one.
impl std::fmt::Debug for SecretInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SecretInput")
            .field("value", &"<redacted>")
            .field("account", &self.account)
            .finish()
    }
}

/// The username survives and the password does not, for
/// [`knobas_source::instance::Account`]'s reason: a refused login has to be
/// able to say *as whom*.
impl std::fmt::Debug for AccountInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AccountInput")
            .field("username", &self.username)
            .field("password", &"<redacted>")
            .finish()
    }
}

/// An unsaved (or saved) source to test a connection for.
///
/// `secret: None` **with** `source_id` set re-tests the stored credential,
/// which is what *Test connection* on an existing source does.
#[derive(Debug, serde::Deserialize, ts_rs::TS)]
#[ts(export_to = "sources.ts")]
pub struct SourceDraft {
    /// Set, with `secret: None`, to re-test the stored credential.
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
    #[ts(type = "unknown")]
    pub config: serde_json::Value,
    pub secret: Option<SecretInput>,
}

/// One row of the sources view.
#[derive(Debug, serde::Serialize, ts_rs::TS)]
#[ts(export_to = "sources.ts")]
pub struct SourceSummary {
    pub id: String,
    pub adapter_kind: String,
    pub display_name: String,
    pub base_url: String,
    pub enabled: bool,
    pub sync_interval_secs: u32,
    #[ts(type = "unknown")]
    pub config: serde_json::Value,
    pub health: knobas_sync::config::CredentialHealth,
    pub last_run: Option<knobas_sync::run_log::SyncRunRow>,
    /// When the scheduler runs this source next; `None` when the source is
    /// disabled, running, or needs a human.
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
    /// The roles the adapter claims, from its descriptor template like
    /// `kinds` (#1196, ADR-0029): what the shell draws each surface for.
    /// Empty for an adapter that claims none, and for a row whose adapter is
    /// no longer compiled in.
    ///
    /// The mirror narrows a role's reads by these claims itself; the shell
    /// reads them only for which optional reads a source answers
    /// (`tf/roles.svelte.ts`, #1300).
    pub roles: Vec<knobas_source::role::RoleClaim>,
    /// The claimed roles whose declared paths missed on the records this
    /// source's syncs emitted (#1199, ADR-0028 *The checks*): the mirror
    /// leaves the source out of those roles' reads, the shell draws none of
    /// these roles for the source and offers none of their optional reads
    /// (`tf/roles.svelte.ts`), and its health line names each path. Only
    /// roles still in [`roles`](Self::roles); empty when every claimed role
    /// is filled or not judged yet.
    pub role_misses: Vec<knobas_source::role::data::RoleMiss>,
}

/// What *Test connection* found.
#[derive(Debug, serde::Serialize, ts_rs::TS)]
#[ts(export_to = "sources.ts")]
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
    /// code.
    pub error: Option<String>,
    /// The adapter's **connection note** (`CONTEXT.md`): the one line it says
    /// about the far end that nothing else on this report already says --
    /// Jira's Epic Link clause is the first. Copied through from
    /// [`knobas_source::ConnectionInfo::detail`] on a successful test and
    /// `None` on a failed one, where [`error`](Self::error) is the line.
    ///
    /// It belongs to the moment of *Test connection*: the Add-source dialog
    /// and a saved source's row show it beside the test result, and nothing
    /// stores it. It is not credential health, which a good sync may rewrite
    /// -- a note is true of the far end as just found, not of the credential
    /// (#326, §10.8).
    pub detail: Option<String>,
    /// The class the UI branches on -- `unauthorized` is what turns *Test*
    /// into *Re-enter*, and a message is not something to branch on.
    pub code: Option<crate::IpcErrorCode>,
    pub elapsed_ms: u32,
    /// Config values the adapter learned from the far end
    /// ([`knobas_source::ConnectionInfo::discovered`]), keyed by the
    /// `config_schema` property each belongs in: Jira's Epic Link custom
    /// field id, whose value is different on every instance (#297).
    ///
    /// Carried through verbatim, and **acted on by the dialog, not here**:
    /// `AddSource.svelte` puts a value into the matching form field when the
    /// reader left it empty, the same way it fills `username` from
    /// [`Self::account`] (#82). Nothing on this side writes a config -- *Test
    /// connection* writes nothing at all, which is the property this whole
    /// command is built around.
    ///
    /// Empty on a failed test: an adapter that could not connect learned
    /// nothing, and a stale map would fill the form with another server's ids.
    /// Empty too for an adapter that discovers nothing.
    pub discovered: std::collections::BTreeMap<String, String>,
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
        SourcesError::Source(Se::Unreachable(_) | Se::Unconfirmed { .. }) => Code::Unreachable,
        SourcesError::Source(_) => Code::Internal,
        SourcesError::Sync(knobas_sync::SyncError::Source(Se::Unauthorized { .. })) => {
            Code::Unauthorized
        }
        SourcesError::Sync(knobas_sync::SyncError::Source(
            Se::Unreachable(_) | Se::Unconfirmed { .. },
        )) => Code::Unreachable,
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
/// [`SourcesError::Db`] if the installed adapters cannot be read or the
/// scheduler's startup reconciliation fails. Both mean the
/// database is not usable, which the caller reports as a failed bring-up.
///
/// `registry` is the list the installed adapters load into: the process's
/// ([`Registry::for_process`]) in the app, which a retried bring-up finds
/// already holding them. `suggestions` is the process's one suggestion pass
/// (#1485): the scheduler requests it after every OK run.
pub async fn start<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    pool: &PgPool,
    connector: &knobas_db::embedded::Connector,
    profile: &crate::Profile,
    registry: Registry,
    suggestions: crate::suggestion_pass::SuggestionPass,
) -> Result<(), SourcesError> {
    // Its own pool for the scheduler's bookkeeping; the runs themselves take a
    // connection each from the connector, outside every pool (§10.6(c)).
    let sync_pool = connector
        .pool(knobas_sync::scheduler::SYNC_POOL_SIZE)
        .await
        .map_err(knobas_db::DbError::from)?;
    let state = bring_up(
        app,
        BringUp {
            pool: pool.clone(),
            sync_pool,
            connections: connector.clone(),
            secrets: secret_store(profile, *app.state::<SecretStoreKind>()),
            registry: registry.clone(),
            store: install::store_dir(&profile.dir),
            suggestions,
        },
    )
    .await?;

    // The update check at app start (#1209): it reads each installed
    // adapter's latest release and records what it found, which Settings
    // shows. It changes no install, and it waits for nothing.
    let pool = state.pool.clone();
    tauri::async_runtime::spawn(async move {
        let endpoints = remote::Endpoints::default();
        if let Err(error) = installed::check_updates(&pool, &registry, &endpoints).await {
            tracing::warn!(%error, "the external adapters' update check did not run");
        }
    });

    // `manage` is the **last** step, and nothing between it and the scheduler's
    // creation can fail. Tauri's `manage` keeps the first value for a type, so
    // a second successful `start` in one process would silently leave a
    // scheduler on a closed pool behind; there is no such path today (a retry
    // only follows a bring-up that failed *before* this line), and this note is
    // what keeps it that way.
    if !app.manage(state) {
        tracing::error!(
            "the sync engine was started twice in one process; the second one is orphaned"
        );
    }
    Ok(())
}

/// What [`bring_up`] starts the engine over: everything [`start`] takes from
/// the database handle and the profile.
pub struct BringUp {
    /// The application pool.
    pub pool: PgPool,
    /// The scheduler's own bookkeeping pool, on the same database.
    pub sync_pool: PgPool,
    /// Where each run takes its dedicated connection, on the same database.
    pub connections: knobas_db::embedded::Connector,
    pub secrets: Arc<dyn SecretStore>,
    /// The registry the installed adapters load into: the process's
    /// ([`Registry::for_process`]) in the app.
    pub registry: Registry,
    /// The installed adapters' artifact store ([`install::store_dir`]).
    pub store: std::path::PathBuf,
    /// The process's one suggestion pass, requested after every run that
    /// ends OK ([`SuggestOnSync`](crate::suggestion_pass::SuggestOnSync),
    /// #1485): the one bring-up built, never one of the scheduler's own.
    pub suggestions: crate::suggestion_pass::SuggestionPass,
}

/// [`start`]'s body below the database handle: load the installed external
/// adapters (#1202), then start the scheduler over the registry that holds
/// them. A refused adapter is logged and recorded on the registry; it does
/// not fail bring-up.
///
/// Split from `start` so a test can drive it without an `EmbeddedDb`: the
/// load must run here, or an installed adapter is gone after a restart
/// (`tests/it/external_adapter_install.rs` is the witness).
///
/// # Errors
/// [`SourcesError::Db`] if the installed adapters cannot be read or the
/// scheduler's startup reconciliation fails.
pub async fn bring_up<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    deps: BringUp,
) -> Result<SourcesState, SourcesError> {
    install::load_installed(&deps.pool, &deps.registry, &deps.store).await?;
    archive_done_contexts(&deps.pool, &deps.registry).await;
    let registry: Arc<dyn knobas_sync::scheduler::AdapterRegistry> = Arc::new(deps.registry);
    // One set for the background sends and the owed-link resolutions, which
    // the scheduler's shutdown awaits before it closes the pool (#1633).
    let background = knobas_sync::background::Background::new();
    let scheduler = Scheduler::start_with_background(
        SchedulerDeps {
            pool: deps.sync_pool,
            connections: Arc::new(DbConnections(deps.connections)),
            registry: Arc::clone(&registry),
            secrets: Arc::clone(&deps.secrets),
            // The owed links of #1050 resolved after every run (`ArrivalLinks`),
            // and a suggestion pass requested after every OK one (#1485).
            events: Arc::new(crate::suggestion_pass::SuggestOnSync::new(
                write_queue::ArrivalLinks::new(
                    TauriEvents::new(app.clone()),
                    deps.pool.clone(),
                    Arc::clone(&registry),
                    background.clone(),
                ),
                deps.suggestions,
            )),
            timing: SchedulerTiming::default(),
            quit: knobas_sync::scheduler::Quit::default(),
        },
        background,
    )
    .await?;
    Ok(SourcesState {
        pool: deps.pool,
        scheduler,
        secrets: deps.secrets,
        registry,
    })
}

/// The one-time pass of #1501: archive every job context whose ticket the
/// mirror already holds done-like, so the inbox stops routing alerts through
/// jobs finished before the sync run kept `archived_at`.
///
/// After the installed adapters load, so an external adapter's done-like set
/// counts, and before the scheduler starts. Every bring-up runs it, and every
/// one after the first finds nothing to do: the statement archives only a
/// live context on a done anchor, and the sync run has already archived those
/// (`knobas_core::context::archive_done`). A failure is logged and starts the
/// engine anyway: the next sync run of each source does the same work.
async fn archive_done_contexts(pool: &PgPool, registry: &Registry) {
    let declarations = match paths::declared_paths(pool, registry).await {
        Ok(declarations) => declarations,
        Err(error) => {
            tracing::warn!(?error, "the done-job pass could not read the declarations");
            return;
        }
    };
    match knobas_core::context::archive_done(pool, &declarations).await {
        Ok(0) => {}
        Ok(archived) => tracing::info!(archived, "archived the contexts of done jobs"),
        Err(error) => tracing::warn!(%error, "the done-job pass did not run"),
    }
}

/// A scheduler over an already-open pool, for the IPC tests. **Tests only.**
///
/// It exists because `start` needs an `EmbeddedDb` -- the thing that hands out
/// connections outside every pool -- and `tests/it/ipc.rs` has a `test_pool`, not
/// a database handle. Everything else is the real thing: the real registry, the
/// real `TauriEvents`, real runs on real dedicated connections. Except the
/// suggestion pass after an OK run (#1485): a pass reads the whole mirror and
/// proposes into it, and these tests share one database, so a run here
/// requests none. `bring_up` is the path that carries it. Behind
/// `test-util` for the reason `AppState::over_pool` is: a production caller
/// would get a scheduler whose pool nothing owns.
///
/// **The caller names the database twice, and must name the same one both
/// times**: `pool` is the scheduler's bookkeeping, `connections` is where each
/// run takes its own dedicated connection (§10.6(c)). This took a connector of
/// its own choosing until #300, which was fine only while every test shared one
/// database -- a test that wants a private one (`scratch_database`, so that
/// `status_for` sees its runs and nobody else's) would otherwise get a
/// scheduler whose bookkeeping and whose runs are in *different* databases.
///
/// **The caller names the registry too**, and gives the same one to the
/// `SourcesState` it builds, so the scheduler and the commands see one list.
///
/// # Errors
/// [`sqlx::Error`] if the startup reconciliation fails.
#[cfg(feature = "test-util")]
pub async fn test_scheduler<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    connections: knobas_db::embedded::Connector,
    pool: PgPool,
    registry: Arc<dyn knobas_sync::scheduler::AdapterRegistry>,
) -> Result<Scheduler, sqlx::Error> {
    let background = knobas_sync::background::Background::new();
    Scheduler::start_with_background(
        SchedulerDeps {
            connections: Arc::new(DbConnections(connections)),
            pool: pool.clone(),
            registry: Arc::clone(&registry),
            secrets: Arc::new(knobas_secrets::MemoryStore::new()),
            events: Arc::new(write_queue::ArrivalLinks::new(
                TauriEvents::new(app.clone()),
                pool,
                registry,
                background.clone(),
            )),
            timing: SchedulerTiming::default(),
            quit: knobas_sync::scheduler::Quit::default(),
        },
        background,
    )
    .await
}

/// Stop the scheduler, cancelling whatever is in flight.
///
/// **Must run before the database is stopped**: the runs hold connections
/// inside transactions, and closing the database under them is the stall the
/// carry-over describes. Safe to call twice -- `RunEvent::ExitRequested` can
/// fire more than once.
pub fn shutdown<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    let Some(state) = app.try_state::<SourcesState>() else {
        return; // bring-up failed before the scheduler existed
    };
    tracing::info!("stopping the sync scheduler");
    tauri::async_runtime::block_on(state.scheduler.shutdown());
}

#[cfg(test)]
mod tests {
    use super::SecretStoreKind;

    #[test]
    fn memory_in_any_case_is_the_memory_store_and_anything_else_the_keychain() {
        assert_eq!(
            SecretStoreKind::from_value(Some("memory")),
            SecretStoreKind::Memory
        );
        assert_eq!(
            SecretStoreKind::from_value(Some("MEMORY")),
            SecretStoreKind::Memory
        );
        assert_eq!(SecretStoreKind::from_value(None), SecretStoreKind::Keychain);
        assert_eq!(
            SecretStoreKind::from_value(Some("keychain")),
            SecretStoreKind::Keychain
        );
        assert_eq!(
            SecretStoreKind::from_value(Some("")),
            SecretStoreKind::Keychain
        );
    }

    /// The generated `SourceSummary` types `roles` and `role_misses` as
    /// required arrays (#1664), so the shell reads them with no fallback. That
    /// is only true while serde sends them when they are empty, as they are
    /// for the mock, which claims no role, and for a source whose claimed
    /// roles are all filled.
    #[test]
    fn a_summary_that_claims_and_misses_nothing_still_sends_both_arrays() {
        let summary = serde_json::to_value(super::SourceSummary {
            id: "s".into(),
            adapter_kind: "mock".into(),
            display_name: "S".into(),
            base_url: String::new(),
            enabled: true,
            sync_interval_secs: 300,
            config: serde_json::json!({}),
            health: knobas_sync::config::CredentialHealth {
                source_id: "s".into(),
                state: knobas_sync::config::AuthState::Ok,
                checked_at: None,
                detail: None,
                secret_expires_at: None,
            },
            last_run: None,
            next_run_at: None,
            item_count: 0,
            auth_kind: None,
            kinds: Vec::new(),
            roles: Vec::new(),
            role_misses: Vec::new(),
        })
        .unwrap();
        assert_eq!(summary["roles"], serde_json::json!([]), "{summary}");
        assert_eq!(summary["role_misses"], serde_json::json!([]), "{summary}");
    }
}
