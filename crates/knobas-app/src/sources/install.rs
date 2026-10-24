//! Installing an external adapter from a local manifest, and loading the
//! installed ones at app start (ADR-0028, issue #1202).
//!
//! **Install** reads a `knobas-adapter.json` and the artifact beside it, runs
//! every static check ([`knobas_wasm_host::manifest::check`]), refuses a kind
//! a built-in or an installed adapter already has, stores the artifact in
//! the profile's [`STORE_DIR`] under its hash, records the install in
//! `knobas.external_adapter` (migration `0051`) and adds the adapter to the
//! registry. Each refusal names its reason, which the reader is shown.
//!
//! **Load** reads every row at bring-up, hashes its stored artifact again and
//! refuses one whose hash is no longer the row's: the code that holds a
//! credential is the code the reader installed, or none. A refused adapter is
//! recorded on the registry, so its sources fail to build with the reason.
//! The row's `hosts` become the adapter's allowlist (#1203), and its
//! `limits` the deadline and memory cap of every call into it (#1204), as
//! install's manifest made them.
//!
//! **Shown, then confirmed** (#2031). [`preview_install`] runs every check
//! install runs and writes nothing: it answers what the reader decides on,
//! the adapter's name, version, roles, hosts and limits, and the hash the
//! confirm sends back ([`confirm_sha256`]). The install that follows is held
//! to that hash, which covers the artifact and the manifest's hosts and
//! limits, so a release that moved in between is refused rather than
//! installed unseen.
//!
//! **From a release** (#1209): what the reader pastes may be a GitHub
//! repository URL or a manifest URL as well as a local path
//! ([`super::remote`]); the install records it in `source_url` with the
//! release's tag. Listing, the update check, update, uninstall and the
//! reinstall a restore makes are [`super::installed`].

use std::path::{Path, PathBuf};

use knobas_wasm_host::WasmAdapter;
use knobas_wasm_host::manifest::{self, Checked, Manifest, Refusal};
use sqlx::PgPool;

use super::Registry;
use super::registry::{KindTaken, TakenBy};
use super::remote::{self, Endpoints, Origin, Pin};

/// The directory under the profile's that holds installed artifacts, each
/// named `<sha256>.wasm`.
pub const STORE_DIR: &str = "adapters";

/// The store directory of a profile.
#[must_use]
pub fn store_dir(profile_dir: &Path) -> PathBuf {
    profile_dir.join(STORE_DIR)
}

/// What an install recorded (#1202, ADR-0028), and the adapter's descriptor
/// template: what the Add-source dialog chooses once the install answers, and
/// what `list_adapters` answers from then on.
#[derive(Debug, Clone, serde::Serialize, ts_rs::TS)]
#[ts(export_to = "sources.ts")]
pub struct InstalledAdapter {
    pub adapter_kind: String,
    pub adapter_version: String,
    pub interface_version: String,
    pub source_url: String,
    /// The release's tag; `None` for a local manifest, which is no release.
    pub tag: Option<String>,
    pub artifact_sha256: String,
    pub hosts: Vec<String>,
    pub template: knobas_source::SourceDescriptor,
}

/// What the reader is shown before an install or an update (#2031): every
/// check has passed and nothing is written. `confirm_sha256` is what the
/// confirm sends back, so what installs is what was shown.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, ts_rs::TS)]
#[ts(export_to = "sources.ts")]
pub struct AdapterPreview {
    pub adapter_kind: String,
    /// The descriptor's name, or the kind for an installed adapter refused at load.
    pub name: String,
    pub adapter_version: String,
    pub interface_version: String,
    pub source_url: String,
    /// The release's tag; `None` for a local manifest, which is no release.
    pub tag: Option<String>,
    pub artifact_sha256: String,
    /// What the confirm sends back: [`confirm_sha256`] of this release.
    pub confirm_sha256: String,
    /// The hosts it may reach, as the manifest writes them (`{base_url}`).
    pub hosts: Vec<String>,
    /// The roles its descriptor claims; empty for an installed adapter
    /// refused at load, whose descriptor was never read.
    pub roles: Vec<knobas_source::role::RoleClaim>,
    pub limits: AdapterLimits,
}

/// The limits every call into the adapter is held to (#1204): what the
/// manifest asks for, each absent one at the host's default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, ts_rs::TS)]
#[ts(export_to = "sources.ts")]
pub struct AdapterLimits {
    /// The deadline of one call, in seconds.
    pub deadline_secs: u32,
    /// Whether the manifest asks for the deadline; `false` is the default.
    pub deadline_asked: bool,
    /// The memory cap of one call, in MiB.
    pub memory_mib: u32,
    /// Whether the manifest asks for the memory cap; `false` is the default.
    pub memory_asked: bool,
}

impl AdapterLimits {
    /// What `asked` comes to, each absent limit at the host's default.
    #[must_use]
    pub fn of(asked: &manifest::Limits) -> Self {
        let default_deadline =
            u32::try_from(knobas_wasm_host::limits::DEFAULT_DEADLINE.as_secs()).unwrap_or(u32::MAX);
        Self {
            deadline_secs: asked.deadline_secs.unwrap_or(default_deadline),
            deadline_asked: asked.deadline_secs.is_some(),
            memory_mib: asked
                .memory_mib
                .unwrap_or(knobas_wasm_host::limits::DEFAULT_MEMORY_MIB),
            memory_asked: asked.memory_mib.is_some(),
        }
    }
}

impl AdapterPreview {
    /// The preview of a release fetched from `source_url` and checked.
    pub(crate) fn of(source_url: String, fetched: &Fetched) -> Self {
        let m = &fetched.checked.manifest;
        let template = fetched.checked.adapter.template();
        Self {
            adapter_kind: m.adapter_kind.clone(),
            name: template.name,
            adapter_version: m.adapter_version.clone(),
            interface_version: m.interface_version.clone(),
            source_url,
            tag: fetched.tag.clone(),
            artifact_sha256: m.artifact.sha256.clone(),
            confirm_sha256: confirm_sha256(&m.artifact.sha256, &m.hosts, &m.limits),
            hosts: m.hosts.clone(),
            roles: template.roles,
            limits: AdapterLimits::of(&m.limits),
        }
    }
}

/// Why an install did not happen.
#[derive(Debug, thiserror::Error)]
pub enum InstallError {
    #[error("cannot read the manifest {path}: {error}")]
    ManifestUnreadable { path: String, error: std::io::Error },
    #[error("install refused: {0}")]
    Refused(#[from] Refusal),
    #[error("install refused: {0}")]
    KindTaken(#[from] KindTaken),
    #[error("cannot store the artifact: {0}")]
    Store(std::io::Error),
    #[error("database: {0}")]
    Db(#[from] sqlx::Error),
    #[error("{0}")]
    Fetch(#[from] remote::FetchError),
    #[error("no external adapter of kind {0:?} is installed")]
    NotInstalled(String),
    #[error(
        "update refused: the latest release is adapter kind {release:?}, and the installed \
         adapter is {installed:?}"
    )]
    KindChanged { installed: String, release: String },
    #[error(
        "the release changed after it was checked: its artifact, hosts or limits are not the ones \
         shown"
    )]
    ReleaseChanged,
    #[error("uninstall asks about every source of {kind:?}; no answer for {missing}")]
    Unanswered { kind: String, missing: String },
    #[error("cannot remove the source {id}: {error}")]
    Remove {
        id: String,
        error: super::SourcesError,
        /// The sources removed before this one, which the scheduler must
        /// still be told about.
        removed: Vec<super::installed::Removal>,
    },
}

impl InstallError {
    /// The frontend's shape: a refusal is the reader's to fix, a failing
    /// disk or database is not.
    #[must_use]
    pub fn to_ipc(&self) -> crate::IpcError {
        match self {
            Self::ManifestUnreadable { .. }
            | Self::Refused(_)
            | Self::KindChanged { .. }
            | Self::ReleaseChanged
            | Self::Unanswered { .. } => crate::IpcError::invalid(self),
            Self::Fetch(remote::FetchError::Unreachable { .. }) => {
                crate::IpcError::new(crate::IpcErrorCode::Unreachable, self)
            }
            Self::Fetch(_) => crate::IpcError::invalid(self),
            Self::KindTaken(_) => crate::IpcError::conflict(self),
            Self::NotInstalled(_) => crate::IpcError::not_found(self),
            Self::Remove { id, error, .. } => {
                let mut ipc = super::to_ipc(error, Some(id));
                ipc.message = self.to_string();
                ipc
            }
            Self::Store(_) | Self::Db(_) => crate::IpcError::internal(self),
        }
    }
}

/// Install the adapter `manifest_path` names (see the module docs).
///
/// # Errors
///
/// [`InstallError`], naming the check that refused it or what failed.
pub async fn install(
    pool: &PgPool,
    registry: &Registry,
    store: &Path,
    manifest_path: &Path,
) -> Result<InstalledAdapter, InstallError> {
    let manifest_path =
        std::path::absolute(manifest_path).map_err(|error| InstallError::ManifestUnreadable {
            path: manifest_path.display().to_string(),
            error,
        })?;
    install_origin(
        pool,
        registry,
        store,
        &Endpoints::default(),
        &Origin::Local(manifest_path),
        None,
    )
    .await
}

/// Install from what the reader pasted (#1209): a GitHub repository URL
/// (its latest release), a manifest URL, or a local manifest's path.
///
/// `confirmed` is the [`AdapterPreview::confirm_sha256`] the reader was
/// shown ([`preview_install`]); a release it does not match is refused.
///
/// # Errors
///
/// As [`install`], [`InstallError::Fetch`] when the release cannot be
/// found or read, and [`InstallError::ReleaseChanged`] when it is not the
/// confirmed one.
pub async fn install_from(
    pool: &PgPool,
    registry: &Registry,
    store: &Path,
    endpoints: &Endpoints,
    input: &str,
    confirmed: Option<&str>,
) -> Result<InstalledAdapter, InstallError> {
    let origin = Origin::parse(input, endpoints)?;
    install_origin(pool, registry, store, endpoints, &origin, confirmed).await
}

/// What installing from `input` would record, with every check install
/// runs, the kind's too, and nothing written (#2031).
///
/// # Errors
///
/// As [`install_from`], but never one that writes.
pub async fn preview_install(
    registry: &Registry,
    endpoints: &Endpoints,
    input: &str,
) -> Result<AdapterPreview, InstallError> {
    let origin = Origin::parse(input, endpoints)?;
    let fetched = fetch_checked(&origin, Pin::Latest, endpoints).await?;
    registry.check_free(&fetched.checked.manifest.adapter_kind)?;
    Ok(AdapterPreview::of(origin.record(endpoints), &fetched))
}

/// The hash a confirm is held to (#2031): over the artifact's hash and the
/// hosts and limits the manifest asks for, which the artifact's hash does not
/// cover. The kind, the versions and the roles are the artifact's own: the
/// static checks hold the manifest's to its descriptor.
#[must_use]
pub fn confirm_sha256(
    artifact_sha256: &str,
    hosts: &[String],
    limits: &manifest::Limits,
) -> String {
    let shown = serde_json::to_vec(&(artifact_sha256, hosts, limits))
        .expect("a hash, strings and numbers serialize");
    manifest::sha256(&shown)
}

/// [`InstallError::ReleaseChanged`] unless the fetched release is the `confirmed` one.
pub(crate) fn held_to(fetched: &Fetched, confirmed: Option<&str>) -> Result<(), InstallError> {
    let m = &fetched.checked.manifest;
    match confirmed {
        Some(confirmed) if confirmed != confirm_sha256(&m.artifact.sha256, &m.hosts, &m.limits) => {
            Err(InstallError::ReleaseChanged)
        }
        _ => Ok(()),
    }
}

/// A release's manifest and artifact, fetched and held to every static
/// check: what install and update both switch to.
pub(crate) struct Fetched {
    pub tag: Option<String>,
    pub checked: Checked,
}

/// Fetch the release `pin` names at `origin` and run the static checks.
pub(crate) async fn fetch_checked(
    origin: &Origin,
    pin: Pin<'_>,
    endpoints: &Endpoints,
) -> Result<Fetched, InstallError> {
    let located = remote::locate(origin, pin, endpoints)
        .await
        .map_err(|error| match (origin, error) {
            (Origin::Local(_), remote::FetchError::Unreadable { path, error }) => {
                InstallError::ManifestUnreadable { path, error }
            }
            (_, error) => error.into(),
        })?;
    let manifest = Manifest::parse(&located.manifest)?;
    let artifact = remote::artifact(&located, &manifest.artifact.file)
        .await
        .map_err(|error| match error {
            remote::FetchError::Unreadable { error, .. } => {
                InstallError::Refused(Refusal::ArtifactUnreadable {
                    file: manifest.artifact.file.clone(),
                    error,
                })
            }
            other => other.into(),
        })?;
    // Compiling a component takes a while; not on a runtime worker.
    let checked = tokio::task::spawn_blocking(move || manifest::check(manifest, artifact))
        .await
        .unwrap_or_else(|e| std::panic::resume_unwind(e.into_panic()))?;
    Ok(Fetched {
        tag: located.tag,
        checked,
    })
}

async fn install_origin(
    pool: &PgPool,
    registry: &Registry,
    store: &Path,
    endpoints: &Endpoints,
    origin: &Origin,
    confirmed: Option<&str>,
) -> Result<InstalledAdapter, InstallError> {
    let fetched = fetch_checked(origin, Pin::Latest, endpoints).await?;
    held_to(&fetched, confirmed)?;
    let Fetched { tag, checked } = fetched;
    registry.check_free(&checked.manifest.adapter_kind)?;

    store_artifact(store, &checked.manifest.artifact.sha256, &checked.artifact)
        .await
        .map_err(InstallError::Store)?;
    let m = &checked.manifest;
    let source_url = origin.record(endpoints);
    let inserted = sqlx::query(
        "insert into knobas.external_adapter
           (adapter_kind, adapter_version, interface_version, source_url, tag,
            artifact_sha256, hosts, limits)
         values ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(&m.adapter_kind)
    .bind(&m.adapter_version)
    .bind(&m.interface_version)
    .bind(&source_url)
    .bind(&tag)
    .bind(&m.artifact.sha256)
    .bind(&m.hosts)
    .bind(sqlx::types::Json(&m.limits))
    .execute(pool)
    .await;
    if let Err(error) = inserted {
        let unique = error
            .as_database_error()
            .and_then(sqlx::error::DatabaseError::code)
            .is_some_and(|code| code == "23505");
        return Err(if unique {
            KindTaken::new(&m.adapter_kind, TakenBy::Installed).into()
        } else {
            error.into()
        });
    }
    let template = checked.adapter.template();
    registry.add(checked.adapter)?;
    tracing::info!(kind = %m.adapter_kind, sha256 = %m.artifact.sha256, ?tag, "external adapter installed");
    Ok(InstalledAdapter {
        adapter_kind: m.adapter_kind.clone(),
        adapter_version: m.adapter_version.clone(),
        interface_version: m.interface_version.clone(),
        source_url,
        tag,
        artifact_sha256: m.artifact.sha256.clone(),
        hosts: m.hosts.clone(),
        template,
    })
}

/// The `install_adapter` command's body: install into `registry` (the sources
/// state's, which the scheduler shares), storing under `profile_dir`.
///
/// # Errors
///
/// As [`install_from`].
pub async fn install_for_profile(
    pool: &PgPool,
    registry: &Registry,
    profile_dir: &Path,
    input: &str,
    confirmed: &str,
) -> Result<InstalledAdapter, InstallError> {
    install_from(
        pool,
        registry,
        &store_dir(profile_dir),
        &Endpoints::default(),
        input,
        Some(confirmed),
    )
    .await
}

/// Where the artifact of hash `sha256` is stored.
#[must_use]
pub fn artifact_path(store: &Path, sha256: &str) -> PathBuf {
    store.join(format!("{sha256}.wasm"))
}

/// Write the artifact under its hash, through a temporary file renamed into
/// place, so a crash never leaves a half-written file under a hash.
pub(crate) async fn store_artifact(
    store: &Path,
    sha256: &str,
    bytes: &[u8],
) -> std::io::Result<()> {
    tokio::fs::create_dir_all(store).await?;
    let target = artifact_path(store, sha256);
    let partial = store.join(format!("{sha256}.wasm.partial"));
    tokio::fs::write(&partial, bytes).await?;
    tokio::fs::rename(&partial, &target).await
}

/// How one installed adapter fared at load: `Ok` when it is in the
/// registry, else the reason it was refused.
pub type Loaded = (String, Result<(), String>);

/// Load every installed adapter into `registry`, refusing a stored artifact
/// that is missing, whose hash is no longer its row's, or that no longer
/// loads. Each refusal is logged and recorded on the registry.
///
/// Each adapter **replaces** the loaded one of its kind, as a restore's
/// reinstall does (LIFE-1, spec #1619): the app's registry is the process's, so a
/// bring-up retried after a failure finds every adapter the first attempt
/// loaded, and `add` refused each one as a kind already installed.
///
/// # Errors
///
/// [`sqlx::Error`] when the rows cannot be read.
pub async fn load_installed(
    pool: &PgPool,
    registry: &Registry,
    store: &Path,
) -> Result<Vec<Loaded>, sqlx::Error> {
    type Row = (
        String,
        String,
        Vec<String>,
        sqlx::types::Json<manifest::Limits>,
    );
    let rows: Vec<Row> = sqlx::query_as(
        "select adapter_kind, artifact_sha256, hosts, limits from knobas.external_adapter
         order by adapter_kind",
    )
    .fetch_all(pool)
    .await?;
    let mut out = Vec::with_capacity(rows.len());
    for (kind, sha256, hosts, sqlx::types::Json(limits)) in rows {
        let path = artifact_path(store, &sha256);
        let outcome = match tokio::fs::read(&path).await {
            Err(error) => Err(format!("cannot read {}: {error}", path.display())),
            Ok(bytes) => {
                let kind = kind.clone();
                tokio::task::spawn_blocking(move || {
                    load_one(&kind, &sha256, &bytes, &hosts, &limits)
                })
                .await
                .unwrap_or_else(|e| Err(e.to_string()))
            }
        }
        .and_then(|adapter| registry.replace(adapter).map_err(|e| e.to_string()));
        match &outcome {
            Ok(()) => tracing::info!(%kind, "external adapter loaded"),
            Err(reason) => {
                tracing::error!(%kind, %reason, "an installed external adapter is refused");
                registry.refuse(&kind, reason.clone());
            }
        }
        out.push((kind, outcome));
    }
    Ok(out)
}

pub(crate) fn load_one(
    kind: &str,
    sha256: &str,
    bytes: &[u8],
    hosts: &[String],
    limits: &manifest::Limits,
) -> Result<WasmAdapter, String> {
    let limits = knobas_wasm_host::limits::Limits::from_manifest(limits)?;
    let hosts = hosts
        .iter()
        .map(|entry| knobas_wasm_host::allow::HostPattern::parse(entry))
        .collect::<Result<Vec<_>, _>>()?;
    let found = manifest::sha256(bytes);
    if found != sha256 {
        return Err(format!(
            "the stored artifact's SHA-256 is {found}, and the install pinned {sha256}"
        ));
    }
    let adapter = WasmAdapter::from_bytes(bytes).map_err(|e| e.to_string())?;
    if adapter.kind() != kind {
        return Err(format!(
            "the stored artifact is adapter kind {:?}, and the install recorded {kind:?}",
            adapter.kind()
        ));
    }
    Ok(adapter.with_hosts(hosts).with_limits(limits))
}
