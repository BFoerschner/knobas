//! The manifest an external adapter is installed from, and the static checks
//! install runs on it (ADR-0028 *The checks*, issue #1202).
//!
//! A release names its adapter in `knobas-adapter.json`:
//!
//! ```json
//! {
//!   "adapter_kind": "wasm-fixture",
//!   "adapter_version": "0.1.0",
//!   "interface_version": "1.0.0",
//!   "artifact": { "file": "knobas_wasm_fixture.wasm", "sha256": "<64 hex digits>" },
//!   "hosts": ["{base_url}"],
//!   "limits": { "deadline_secs": 60, "memory_mib": 256 }
//! }
//! ```
//!
//! [`check`] holds a manifest and the artifact it names to every static
//! check, in this order, and answers the first that fails as a [`Refusal`]
//! naming why: the manifest is well formed, its interface version is one this
//! knobas hosts, the artifact's hash is the manifest's, the artifact is a
//! knobas adapter built against that interface version, its descriptor is
//! well formed and names the manifest's kind and version, and every role it
//! claims is covered by its kinds, declared paths and reads
//! ([`knobas_source::role::check`]).
//!
//! `hosts` is the adapter's allowlist (#1203, [`crate::allow`]): each entry
//! is a fixed host or a whole-entry template `{key}`, and a template's key
//! must be `base_url` or a key the descriptor's `config_schema` declares.
//! `limits` are stored as written and enforced by nothing yet: the deadline
//! and memory cap are #1204.

use knobas_source::SourceDescriptor;
use knobas_source::role::ClaimError;
use sha2::Digest;

use crate::{INTERFACE_VERSION, LoadError, PACKAGE, Version, WasmAdapter};

/// The file name a release's manifest has.
pub const FILE_NAME: &str = "knobas-adapter.json";

/// A manifest, as written. Every field is required but `hosts` and `limits`,
/// and a field it does not know is refused: a misspelt `host` would
/// otherwise install an adapter whose allowlist is silently empty.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub adapter_kind: String,
    pub adapter_version: String,
    pub interface_version: String,
    pub artifact: Artifact,
    /// The hosts the adapter may reach, fixed (`api.example.com`) or as a
    /// template over its config (`{base_url}`): [`crate::allow`].
    #[serde(default)]
    pub hosts: Vec<String>,
    /// The limits the adapter asks for. Enforced from #1204.
    #[serde(default)]
    pub limits: Limits,
}

/// The component a manifest names: a file beside the manifest, and its hash.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Artifact {
    /// A bare file name, as a release asset has: no directory in it.
    pub file: String,
    /// The SHA-256 of the file, as 64 lower-case hex digits.
    pub sha256: String,
}

/// What a manifest asks for beyond the host defaults. Absent asks for the
/// default.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Limits {
    /// A deadline per call into the component, in seconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deadline_secs: Option<u32>,
    /// A memory cap per instance, in MiB.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_mib: Option<u32>,
}

/// Why an install was refused: the static check that failed, and what it
/// found. The message is what the reader is shown.
#[derive(Debug, thiserror::Error)]
pub enum Refusal {
    #[error("the manifest is not well formed: {0}")]
    Manifest(String),
    #[error(
        "the manifest asks for {PACKAGE}@{found}, and this knobas hosts {PACKAGE}@{major}.x up \
         to {INTERFACE_VERSION}",
        major = INTERFACE_VERSION.major
    )]
    UnsupportedInterface { found: Version },
    #[error("cannot read the artifact {file}: {error}")]
    ArtifactUnreadable { file: String, error: std::io::Error },
    #[error("the artifact's SHA-256 is {found}, and the manifest names {expected}")]
    HashMismatch { expected: String, found: String },
    #[error("the artifact is not a knobas adapter: {0}")]
    NotAnAdapter(String),
    #[error(
        "the artifact is built against {PACKAGE}@{component}, and the manifest says {manifest}"
    )]
    InterfaceMismatch {
        manifest: Version,
        component: Version,
    },
    #[error("the adapter's descriptor is not well formed: {0}")]
    Descriptor(String),
    #[error("the descriptor names {field} {descriptor:?}, and the manifest {manifest:?}")]
    Disagrees {
        field: &'static str,
        manifest: String,
        descriptor: String,
    },
    #[error(
        "the manifest's host template {{{key}}} names no key the descriptor's config_schema \
         declares (nor base_url)"
    )]
    UndeclaredTemplate { key: String },
    #[error("the adapter's role claims are refused: {0}")]
    Role(#[from] ClaimError),
}

/// A manifest and its artifact that passed every static check.
#[derive(Debug, Clone)]
pub struct Checked {
    pub manifest: Manifest,
    pub adapter: WasmAdapter,
    /// The artifact's bytes, for the store.
    pub artifact: Vec<u8>,
}

/// The SHA-256 of `bytes`, as 64 lower-case hex digits.
#[must_use]
pub fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", sha2::Sha256::digest(bytes))
}

impl Manifest {
    /// Read a manifest's JSON and check what it says on its own: the kind
    /// is an adapter kind, the versions are versions, the artifact is a bare
    /// file name with a well-formed hash.
    ///
    /// # Errors
    ///
    /// [`Refusal::Manifest`] naming the first thing wrong.
    pub fn parse(text: &str) -> Result<Self, Refusal> {
        let manifest: Self =
            serde_json::from_str(text).map_err(|e| Refusal::Manifest(e.to_string()))?;
        knobas_source::instance::validate_instance_id(&manifest.adapter_kind).map_err(|e| {
            Refusal::Manifest(format!(
                "adapter_kind {:?} is not an adapter kind: {e}",
                manifest.adapter_kind
            ))
        })?;
        if manifest.adapter_version.trim().is_empty() {
            return Err(Refusal::Manifest("adapter_version is empty".to_owned()));
        }
        manifest.interface()?;
        manifest.host_patterns()?;
        let file = &manifest.artifact.file;
        // One normal component, and no `:`: on Windows `C:x.wasm` is a
        // drive-relative path, which `Path::join` would follow off the release.
        let bare = std::path::Path::new(file).file_name() == Some(std::ffi::OsStr::new(file));
        if !bare || file.contains(['/', '\\', ':', '\0']) {
            return Err(Refusal::Manifest(format!(
                "artifact.file {file:?} must be a file name beside the manifest"
            )));
        }
        let hash = &manifest.artifact.sha256;
        if hash.len() != 64 || !hash.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')) {
            return Err(Refusal::Manifest(format!(
                "artifact.sha256 {hash:?} is not 64 lower-case hex digits"
            )));
        }
        Ok(manifest)
    }

    /// The `hosts` entries, read.
    ///
    /// # Errors
    ///
    /// [`Refusal::Manifest`] naming the first entry that is neither a fixed
    /// host nor a whole-entry template.
    pub fn host_patterns(&self) -> Result<Vec<crate::allow::HostPattern>, Refusal> {
        self.hosts
            .iter()
            .map(|entry| crate::allow::HostPattern::parse(entry).map_err(Refusal::Manifest))
            .collect()
    }

    /// The interface version the manifest names.
    ///
    /// # Errors
    ///
    /// [`Refusal::Manifest`] when it is not `major.minor.patch`.
    pub fn interface(&self) -> Result<Version, Refusal> {
        Version::parse(&self.interface_version).ok_or_else(|| {
            Refusal::Manifest(format!(
                "interface_version {:?} is not major.minor.patch",
                self.interface_version
            ))
        })
    }
}

/// Run every static check on `manifest` and the artifact bytes it names.
///
/// Blocks while the component describes itself (see
/// [`WasmAdapter::from_bytes`]).
///
/// # Errors
///
/// The [`Refusal`] of the first check that fails.
pub fn check(manifest: Manifest, artifact: Vec<u8>) -> Result<Checked, Refusal> {
    let interface = manifest.interface()?;
    if !interface.is_hosted() {
        return Err(Refusal::UnsupportedInterface { found: interface });
    }
    let found = sha256(&artifact);
    if found != manifest.artifact.sha256 {
        return Err(Refusal::HashMismatch {
            expected: manifest.artifact.sha256.clone(),
            found,
        });
    }
    let adapter = WasmAdapter::from_bytes(&artifact).map_err(|e| match e {
        LoadError::Unsupported { found } => Refusal::InterfaceMismatch {
            manifest: interface,
            component: found,
        },
        LoadError::Descriptor(reason) => Refusal::Descriptor(reason),
        other => Refusal::NotAnAdapter(other.to_string()),
    })?;
    if adapter.version() != interface {
        return Err(Refusal::InterfaceMismatch {
            manifest: interface,
            component: adapter.version(),
        });
    }
    check_descriptor(&manifest, &adapter.template())?;
    let adapter = adapter.with_hosts(manifest.host_patterns()?);
    Ok(Checked {
        manifest,
        adapter,
        artifact,
    })
}

/// The descriptor's half of the static checks: it names the manifest's
/// kind and version, every host template names a key its `config_schema`
/// declares (or `base_url`), and its role claims hold.
///
/// # Errors
///
/// [`Refusal::Disagrees`], [`Refusal::Manifest`],
/// [`Refusal::UndeclaredTemplate`] or [`Refusal::Role`].
pub fn check_descriptor(manifest: &Manifest, descriptor: &SourceDescriptor) -> Result<(), Refusal> {
    let disagrees = |field, manifest: &str, descriptor: &str| Refusal::Disagrees {
        field,
        manifest: manifest.to_owned(),
        descriptor: descriptor.to_owned(),
    };
    if descriptor.adapter_kind != manifest.adapter_kind {
        return Err(disagrees(
            "adapter_kind",
            &manifest.adapter_kind,
            &descriptor.adapter_kind,
        ));
    }
    if descriptor.adapter_version != manifest.adapter_version {
        return Err(disagrees(
            "adapter_version",
            &manifest.adapter_version,
            &descriptor.adapter_version,
        ));
    }
    let declared = crate::allow::declared_keys(&descriptor.config_schema);
    for pattern in manifest.host_patterns()? {
        if let Some(key) = pattern.template_key()
            && key != crate::allow::BASE_URL_KEY
            && !declared.iter().any(|d| d == key)
        {
            return Err(Refusal::UndeclaredTemplate {
                key: key.to_owned(),
            });
        }
    }
    knobas_source::role::check(descriptor)?;
    Ok(())
}
