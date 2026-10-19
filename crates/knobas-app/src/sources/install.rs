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
//! A manifest from a URL or a GitHub repository, and updates, are #1209;
//! uninstall, backup and restore #1210.

use std::path::{Path, PathBuf};

use knobas_wasm_host::WasmAdapter;
use knobas_wasm_host::manifest::{self, Manifest, Refusal};
use sqlx::PgPool;

use super::Registry;
use super::registry::{KindTaken, TakenBy};

/// The directory under the profile's that holds installed artifacts, each
/// named `<sha256>.wasm`.
pub const STORE_DIR: &str = "adapters";

/// The store directory of a profile.
#[must_use]
pub fn store_dir(profile_dir: &Path) -> PathBuf {
    profile_dir.join(STORE_DIR)
}

/// What an install recorded, and the adapter's descriptor template: what
/// the Add-source dialog chooses once the install answers.
#[derive(Debug, Clone, serde::Serialize)]
pub struct InstalledAdapter {
    pub adapter_kind: String,
    pub adapter_version: String,
    pub interface_version: String,
    pub source_url: String,
    pub tag: Option<String>,
    pub artifact_sha256: String,
    pub hosts: Vec<String>,
    pub template: knobas_source::SourceDescriptor,
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
}

impl InstallError {
    /// The frontend's shape: a refusal is the reader's to fix, a failing
    /// disk or database is not.
    #[must_use]
    pub fn to_ipc(&self) -> crate::IpcError {
        match self {
            Self::ManifestUnreadable { .. } | Self::Refused(_) => crate::IpcError::invalid(self),
            Self::KindTaken(_) => crate::IpcError::conflict(self),
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
    let unreadable = |error| InstallError::ManifestUnreadable {
        path: manifest_path.display().to_string(),
        error,
    };
    let manifest_path = std::path::absolute(manifest_path).map_err(unreadable)?;
    let text = tokio::fs::read_to_string(&manifest_path)
        .await
        .map_err(unreadable)?;
    let manifest = Manifest::parse(&text)?;
    let artifact_path = manifest_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(&manifest.artifact.file);
    let artifact =
        tokio::fs::read(&artifact_path)
            .await
            .map_err(|error| Refusal::ArtifactUnreadable {
                file: manifest.artifact.file.clone(),
                error,
            })?;
    // Compiling a component takes a while; not on a runtime worker.
    let checked = tokio::task::spawn_blocking(move || manifest::check(manifest, artifact))
        .await
        .unwrap_or_else(|e| std::panic::resume_unwind(e.into_panic()))?;
    registry.check_free(&checked.manifest.adapter_kind)?;

    store_artifact(store, &checked.manifest.artifact.sha256, &checked.artifact)
        .await
        .map_err(InstallError::Store)?;
    let m = &checked.manifest;
    let source_url = manifest_path.display().to_string();
    let inserted = sqlx::query(
        "insert into knobas.external_adapter
           (adapter_kind, adapter_version, interface_version, source_url, tag,
            artifact_sha256, hosts, limits)
         values ($1, $2, $3, $4, null, $5, $6, $7)",
    )
    .bind(&m.adapter_kind)
    .bind(&m.adapter_version)
    .bind(&m.interface_version)
    .bind(&source_url)
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
    tracing::info!(kind = %m.adapter_kind, sha256 = %m.artifact.sha256, "external adapter installed");
    Ok(InstalledAdapter {
        adapter_kind: m.adapter_kind.clone(),
        adapter_version: m.adapter_version.clone(),
        interface_version: m.interface_version.clone(),
        source_url,
        tag: None,
        artifact_sha256: m.artifact.sha256.clone(),
        hosts: m.hosts.clone(),
        template,
    })
}

/// The `install_adapter` command's body: install into the process registry
/// (the one the scheduler's copy shares), storing under `profile_dir`.
///
/// # Errors
///
/// As [`install`].
pub async fn install_for_profile(
    pool: &PgPool,
    profile_dir: &Path,
    manifest_path: &Path,
) -> Result<InstalledAdapter, InstallError> {
    install(
        pool,
        &Registry::builtin(),
        &store_dir(profile_dir),
        manifest_path,
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
async fn store_artifact(store: &Path, sha256: &str, bytes: &[u8]) -> std::io::Result<()> {
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
        .and_then(|adapter| registry.add(adapter).map_err(|e| e.to_string()));
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

fn load_one(
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
