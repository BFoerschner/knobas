//! The installed external adapters after install (ADR-0028): the list
//! Settings draws, the update check and update (#1209), uninstall and the
//! reinstall a restore makes (#1210).
//!
//! **Pinned.** Nothing here changes an install on its own. The update check
//! reads the latest release and records what it found on the registry,
//! which the list answers; only [`update`] switches, and it runs every
//! static check on the new release first. A refused update leaves the row,
//! the stored artifact and the registry as they were, and records the
//! reason beside the check.
//!
//! **Shown, then confirmed** (#2031). [`preview_update`] fetches and checks
//! the latest release as [`update`] does, and answers it beside what is
//! installed, so the reader sees what an update changes before it switches;
//! [`update`] is then held to the release the reader was shown
//! ([`install::confirm_sha256`]).
//!
//! **Uninstall** takes one answer per source of the adapter, *purge* or
//! *keep*, and removes each source through the source-removal path
//! ([`super::crud::delete`]) before the install itself.
//!
//! **Restore.** A backup is the `knobas` schema, so it carries each
//! `knobas.external_adapter` row (source URL, tag, hash and what the
//! manifest declared) and never the `.wasm`, which lives in the profile's
//! store. [`reinstall_restored`] fetches each row's release at its recorded
//! tag and refuses an artifact whose hash is not the row's; a refused one
//! is recorded on the registry and the rest go on.

use std::path::Path;
use std::sync::Arc;

use knobas_secrets::SecretStore;
use sqlx::PgPool;

use super::Registry;
use super::install::{self, InstallError, Loaded};
use super::remote::{self, Endpoints, Origin, Pin};

/// Where the installed adapters are: the registry they load into, the
/// artifact store, and where GitHub is. The app's are
/// [`Installs::for_profile`]'s; a test builds its own.
#[derive(Debug, Clone)]
pub struct Installs {
    pub registry: Registry,
    pub store: std::path::PathBuf,
    pub endpoints: Endpoints,
}

impl Installs {
    /// `registry` (the sources state's, see
    /// [`SourcesState::installable`](super::SourcesState::installable)), the
    /// profile's store, GitHub itself.
    #[must_use]
    pub fn for_profile(registry: Registry, profile_dir: &Path) -> Self {
        Self {
            registry,
            store: install::store_dir(profile_dir),
            endpoints: Endpoints::default(),
        }
    }
}

/// What the last update check of an installed adapter found (#1209).
///
/// - `current`: the installed release is the latest.
/// - `available`: a newer release exists, which nothing has switched to;
///   nothing changes until the reader updates.
/// - `failed`: the check could not read the latest release.
/// - `refused`: the reader updated, and the new release's static checks
///   refused it: the installed one stays.
///
/// The states are documented here and not on each variant because the
/// generated TypeScript (`app/src/lib/ipc/generated/sources.ts`) carries a
/// type's and a field's doc but not a variant's (#1325).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, ts_rs::TS)]
#[serde(tag = "state", rename_all = "snake_case")]
#[ts(export_to = "sources.ts")]
pub enum UpdateCheck {
    Current,
    Available {
        tag: Option<String>,
        adapter_version: String,
    },
    Failed {
        reason: String,
    },
    Refused {
        tag: Option<String>,
        reason: String,
    },
}

/// A source of an installed adapter, for uninstall's question.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, ts_rs::TS)]
#[ts(export_to = "sources.ts")]
pub struct AdapterSource {
    pub id: String,
    pub display_name: String,
}

/// One installed external adapter, as Settings lists it (#1209, #1210): its
/// pin, its sources, why it was refused at load or restore, and the last
/// update check.
#[derive(Debug, Clone, serde::Serialize, ts_rs::TS)]
#[ts(export_to = "sources.ts")]
pub struct AdapterStatus {
    pub adapter_kind: String,
    /// The descriptor's name, or the kind for an adapter refused at load.
    pub name: String,
    pub adapter_version: String,
    pub interface_version: String,
    pub source_url: String,
    pub tag: Option<String>,
    pub artifact_sha256: String,
    pub hosts: Vec<String>,
    pub sources: Vec<AdapterSource>,
    /// Why the adapter was refused at load or restore; `None` when it runs.
    pub refused: Option<String>,
    /// The last update check; `None` before one ran.
    pub update: Option<UpdateCheck>,
}

/// An update before it switches (#2031): what is installed, and the latest
/// release, which has passed every check.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, ts_rs::TS)]
#[ts(export_to = "sources.ts")]
pub struct UpdatePreview {
    pub installed: install::AdapterPreview,
    pub release: install::AdapterPreview,
}

/// Uninstall's answer for one source: remove its items, or keep them.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export_to = "sources.ts")]
pub struct Removal {
    pub source_id: String,
    pub purge_items: bool,
}

#[derive(sqlx::FromRow)]
struct Row {
    adapter_kind: String,
    adapter_version: String,
    interface_version: String,
    source_url: String,
    tag: Option<String>,
    artifact_sha256: String,
    hosts: Vec<String>,
    limits: sqlx::types::Json<knobas_wasm_host::manifest::Limits>,
}

/// The rows' select, with `$tail` after it: one spelling of the columns.
macro_rules! rows_sql {
    ($tail:literal) => {
        concat!(
            "select adapter_kind, adapter_version, interface_version, source_url, tag,
                    artifact_sha256, hosts, limits
               from knobas.external_adapter ",
            $tail
        )
    };
}

async fn rows(pool: &PgPool) -> Result<Vec<Row>, sqlx::Error> {
    sqlx::query_as(rows_sql!("order by adapter_kind"))
        .fetch_all(pool)
        .await
}

async fn row(pool: &PgPool, kind: &str) -> Result<Row, InstallError> {
    sqlx::query_as(rows_sql!("where adapter_kind = $1"))
        .bind(kind)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| InstallError::NotInstalled(kind.to_owned()))
}

async fn sources_of(pool: &PgPool, kind: &str) -> Result<Vec<AdapterSource>, sqlx::Error> {
    let rows: Vec<(String, String)> = sqlx::query_as(
        "select id, display_name from knobas.source_config where kind = $1 order by id",
    )
    .bind(kind)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(id, display_name)| AdapterSource { id, display_name })
        .collect())
}

async fn status(
    pool: &PgPool,
    registry: &Registry,
    row: Row,
) -> Result<AdapterStatus, sqlx::Error> {
    let name = registry
        .external_template(&row.adapter_kind)
        .map_or_else(|| row.adapter_kind.clone(), |t| t.name);
    Ok(AdapterStatus {
        sources: sources_of(pool, &row.adapter_kind).await?,
        refused: registry.refusal(&row.adapter_kind),
        update: registry.update(&row.adapter_kind),
        name,
        adapter_kind: row.adapter_kind,
        adapter_version: row.adapter_version,
        interface_version: row.interface_version,
        source_url: row.source_url,
        tag: row.tag,
        artifact_sha256: row.artifact_sha256,
        hosts: row.hosts,
    })
}

/// Every installed adapter, by kind.
///
/// # Errors
///
/// [`sqlx::Error`] when the rows cannot be read.
pub async fn list(pool: &PgPool, registry: &Registry) -> Result<Vec<AdapterStatus>, sqlx::Error> {
    let mut out = Vec::new();
    for row in rows(pool).await? {
        out.push(status(pool, registry, row).await?);
    }
    Ok(out)
}

/// What the latest release at `row`'s origin is, against what is installed:
/// a release with a tag is newer when its tag differs, one without (a local
/// manifest, a URL that is no release) when its artifact's hash does.
async fn check_one(row: &Row, endpoints: &Endpoints) -> UpdateCheck {
    let failed = |reason: String| UpdateCheck::Failed { reason };
    let origin = match Origin::parse(&row.source_url, endpoints) {
        Ok(origin) => origin,
        Err(error) => return failed(error.to_string()),
    };
    let located = match remote::locate(&origin, Pin::Latest, endpoints).await {
        Ok(located) => located,
        Err(error) => return failed(error.to_string()),
    };
    let manifest = match knobas_wasm_host::manifest::Manifest::parse(&located.manifest) {
        Ok(manifest) => manifest,
        Err(error) => return failed(format!("the latest release's {error}")),
    };
    let newer = match (&located.tag, &row.tag) {
        (Some(latest), Some(installed)) => latest != installed,
        _ => manifest.artifact.sha256 != row.artifact_sha256,
    };
    if newer {
        UpdateCheck::Available {
            tag: located.tag,
            adapter_version: manifest.adapter_version,
        }
    } else {
        UpdateCheck::Current
    }
}

/// Check every installed adapter for a newer release, record each answer on
/// the registry, and answer the list. Changes no install.
///
/// # Errors
///
/// [`sqlx::Error`] when the rows cannot be read.
pub async fn check_updates(
    pool: &PgPool,
    registry: &Registry,
    endpoints: &Endpoints,
) -> Result<Vec<AdapterStatus>, sqlx::Error> {
    for row in rows(pool).await? {
        let check = check_one(&row, endpoints).await;
        registry.set_update(&row.adapter_kind, check);
    }
    list(pool, registry).await
}

/// The latest release at `installed`'s origin, held to every static check
/// and to `installed`'s kind. A refusal or a release that cannot be read is
/// recorded beside the update check.
async fn latest_checked(
    registry: &Registry,
    endpoints: &Endpoints,
    installed: &Row,
) -> Result<install::Fetched, InstallError> {
    let kind = installed.adapter_kind.as_str();
    let origin = Origin::parse(&installed.source_url, endpoints)?;
    let fetched = match install::fetch_checked(&origin, Pin::Latest, endpoints).await {
        Ok(fetched) if fetched.checked.manifest.adapter_kind != kind => {
            Err(InstallError::KindChanged {
                installed: kind.to_owned(),
                release: fetched.checked.manifest.adapter_kind,
            })
        }
        other => other,
    };
    fetched.inspect_err(|error| {
        let reason = error.to_string();
        let check = match error {
            InstallError::Refused(_) | InstallError::KindChanged { .. } => {
                let tag = match registry.update(kind) {
                    Some(UpdateCheck::Available { tag, .. } | UpdateCheck::Refused { tag, .. }) => {
                        tag
                    }
                    _ => None,
                };
                Some(UpdateCheck::Refused { tag, reason })
            }
            InstallError::Fetch(_) | InstallError::ManifestUnreadable { .. } => {
                Some(UpdateCheck::Failed { reason })
            }
            _ => None,
        };
        if let Some(check) = check {
            registry.set_update(kind, check);
        }
    })
}

/// What updating the adapter of `kind` would switch to, beside what is
/// installed, with every check [`update`] runs and nothing switched
/// (#2031). A refusal is recorded beside the update check, as [`update`]
/// records it.
///
/// # Errors
///
/// As [`update`], but never one that writes.
pub async fn preview_update(
    pool: &PgPool,
    registry: &Registry,
    endpoints: &Endpoints,
    kind: &str,
) -> Result<UpdatePreview, InstallError> {
    let installed = row(pool, kind).await?;
    let fetched = latest_checked(registry, endpoints, &installed).await?;
    let release = install::AdapterPreview::of(installed.source_url.clone(), &fetched);
    let template = registry.external_template(kind);
    let installed = install::AdapterPreview {
        name: template
            .as_ref()
            .map_or_else(|| installed.adapter_kind.clone(), |t| t.name.clone()),
        roles: template.map(|t| t.roles).unwrap_or_default(),
        limits: install::AdapterLimits::of(&installed.limits),
        adapter_kind: installed.adapter_kind,
        adapter_version: installed.adapter_version,
        interface_version: installed.interface_version,
        source_url: installed.source_url,
        tag: installed.tag,
        confirm_sha256: install::confirm_sha256(
            &installed.artifact_sha256,
            &installed.hosts,
            &installed.limits,
        ),
        artifact_sha256: installed.artifact_sha256,
        hosts: installed.hosts,
    };
    Ok(UpdatePreview { installed, release })
}

/// Update the adapter of `kind` to its origin's latest release: every
/// static check first, then the artifact stored, the row moved and the
/// registry's adapter replaced. A refusal changes nothing installed, and is
/// recorded beside the update check. `confirmed` is the release's
/// `confirm_sha256` the reader was shown ([`preview_update`]); a release it
/// does not match is refused.
///
/// # Errors
///
/// [`InstallError::NotInstalled`], the refusal of the check that failed,
/// [`InstallError::KindChanged`] when the release is another adapter,
/// [`InstallError::ReleaseChanged`] when it is not the confirmed one, or
/// what could not be fetched or stored.
pub async fn update(
    pool: &PgPool,
    registry: &Registry,
    store: &Path,
    endpoints: &Endpoints,
    kind: &str,
    confirmed: Option<&str>,
) -> Result<AdapterStatus, InstallError> {
    let installed = row(pool, kind).await?;
    let fetched = latest_checked(registry, endpoints, &installed).await?;
    install::held_to(&fetched, confirmed)?;
    let install::Fetched { tag, checked } = fetched;
    let m = &checked.manifest;
    install::store_artifact(store, &m.artifact.sha256, &checked.artifact)
        .await
        .map_err(InstallError::Store)?;
    sqlx::query(
        "update knobas.external_adapter
            set adapter_version = $2, interface_version = $3, tag = $4,
                artifact_sha256 = $5, hosts = $6, limits = $7, installed_at = now()
          where adapter_kind = $1",
    )
    .bind(kind)
    .bind(&m.adapter_version)
    .bind(&m.interface_version)
    .bind(&tag)
    .bind(&m.artifact.sha256)
    .bind(&m.hosts)
    .bind(sqlx::types::Json(&m.limits))
    .execute(pool)
    .await?;
    // Cannot fail: the kind is installed, so no compiled-in row has it.
    registry.replace(checked.adapter)?;
    tracing::info!(%kind, ?tag, sha256 = %m.artifact.sha256, "external adapter updated");
    if installed.artifact_sha256 != m.artifact.sha256 {
        drop_unused_artifact(pool, store, &installed.artifact_sha256).await;
    }
    status(pool, registry, row(pool, kind).await?)
        .await
        .map_err(Into::into)
}

/// Remove the stored artifact of `sha256` once no install pins it.
/// Best-effort: a file left behind is a file, and no install loads it.
async fn drop_unused_artifact(pool: &PgPool, store: &Path, sha256: &str) {
    let pinned: Result<bool, sqlx::Error> = sqlx::query_scalar(
        "select exists (select 1 from knobas.external_adapter where artifact_sha256 = $1)",
    )
    .bind(sha256)
    .fetch_one(pool)
    .await;
    if let Ok(false) = pinned
        && let Err(error) = tokio::fs::remove_file(install::artifact_path(store, sha256)).await
    {
        tracing::warn!(%error, %sha256, "an unused adapter artifact was not removed");
    }
}

/// Uninstall the adapter of `kind`: each of its sources removed as its
/// answer in `removals` says, through the source-removal path, then the
/// install. Answers the removals it made, for the scheduler to forget.
///
/// # Errors
///
/// [`InstallError::NotInstalled`], [`InstallError::Unanswered`] (and nothing
/// removed) when a source of the adapter has no answer, or
/// [`InstallError::Remove`] naming the source that could not be removed.
pub async fn uninstall(
    pool: &PgPool,
    secrets: &Arc<dyn SecretStore>,
    live: &super::live::LiveSources,
    registry: &Registry,
    store: &Path,
    kind: &str,
    removals: &[Removal],
) -> Result<Vec<Removal>, InstallError> {
    let installed = row(pool, kind).await?;
    let sources = sources_of(pool, kind).await?;
    let mut answered = Vec::with_capacity(sources.len());
    let mut missing = Vec::new();
    for source in &sources {
        match removals.iter().find(|r| r.source_id == source.id) {
            Some(removal) => answered.push(removal.clone()),
            None => missing.push(source.id.clone()),
        }
    }
    if !missing.is_empty() {
        return Err(InstallError::Unanswered {
            kind: kind.to_owned(),
            missing: missing.join(", "),
        });
    }
    for (done, removal) in answered.iter().enumerate() {
        if let Err(error) =
            super::crud::delete(pool, secrets, live, &removal.source_id, removal.purge_items).await
        {
            return Err(InstallError::Remove {
                id: removal.source_id.clone(),
                error,
                removed: answered[..done].to_vec(),
            });
        }
    }
    sqlx::query("delete from knobas.external_adapter where adapter_kind = $1")
        .bind(kind)
        .execute(pool)
        .await?;
    registry.remove(kind);
    drop_unused_artifact(pool, store, &installed.artifact_sha256).await;
    tracing::info!(%kind, sources = answered.len(), "external adapter uninstalled");
    Ok(answered)
}

/// After a restore: bring every restored install's artifact into `store`
/// and its adapter into `registry`. An artifact already stored under the
/// row's hash is loaded; any other is fetched from the row's origin at its
/// recorded tag, and refused unless its hash is the row's. A refusal is
/// logged and recorded on the registry, and the next row goes on.
///
/// # Errors
///
/// [`sqlx::Error`] when the rows cannot be read.
pub async fn reinstall_restored(
    pool: &PgPool,
    registry: &Registry,
    store: &Path,
    endpoints: &Endpoints,
) -> Result<Vec<Loaded>, sqlx::Error> {
    let mut out = Vec::new();
    for row in rows(pool).await? {
        let outcome = match reinstall_one(&row, store, endpoints).await {
            Ok(adapter) => registry.replace(adapter).map_err(|e| e.to_string()),
            Err(reason) => Err(reason),
        };
        match &outcome {
            Ok(()) => {
                tracing::info!(kind = %row.adapter_kind, "restored external adapter installed")
            }
            Err(reason) => {
                tracing::error!(kind = %row.adapter_kind, %reason, "a restored external adapter is refused");
                registry.refuse(&row.adapter_kind, reason.clone());
            }
        }
        out.push((row.adapter_kind, outcome));
    }
    Ok(out)
}

async fn reinstall_one(
    row: &Row,
    store: &Path,
    endpoints: &Endpoints,
) -> Result<knobas_wasm_host::WasmAdapter, String> {
    let pinned = &row.artifact_sha256;
    let load = |bytes: Vec<u8>| {
        let (kind, sha, hosts) = (row.adapter_kind.clone(), pinned.clone(), row.hosts.clone());
        let limits = row.limits.0.clone();
        async move {
            tokio::task::spawn_blocking(move || {
                install::load_one(&kind, &sha, &bytes, &hosts, &limits)
            })
            .await
            .unwrap_or_else(|e| Err(e.to_string()))
        }
    };
    if let Ok(bytes) = tokio::fs::read(install::artifact_path(store, pinned)).await
        && let Ok(adapter) = load(bytes).await
    {
        return Ok(adapter);
    }
    let origin = Origin::parse(&row.source_url, endpoints).map_err(|e| e.to_string())?;
    let located = remote::locate(&origin, Pin::Recorded(row.tag.as_deref()), endpoints)
        .await
        .map_err(|e| e.to_string())?;
    let manifest = knobas_wasm_host::manifest::Manifest::parse(&located.manifest)
        .map_err(|e| format!("the release's {e}"))?;
    if manifest.artifact.sha256 != *pinned {
        return Err(format!(
            "the release's artifact SHA-256 is now {}, and the backup pinned {pinned}",
            manifest.artifact.sha256
        ));
    }
    let bytes = remote::artifact(&located, &manifest.artifact.file)
        .await
        .map_err(|e| e.to_string())?;
    let found = knobas_wasm_host::manifest::sha256(&bytes);
    if found != *pinned {
        return Err(format!(
            "the release's artifact SHA-256 is now {found}, and the backup pinned {pinned}"
        ));
    }
    install::store_artifact(store, pinned, &bytes)
        .await
        .map_err(|e| format!("cannot store the artifact: {e}"))?;
    load(bytes).await
}
