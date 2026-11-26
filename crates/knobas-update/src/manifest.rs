//! `latest.json`, in the official updater's static format (decisions D3).
//!
//! `{ version, notes?, pub_date?, platforms: { "<os>-<arch>-<installer>": { url, signature } } }`,
//! plus knobas's own key `knobas: { schema, sizes }`. Unknown keys are ignored, so a field the
//! plugin adds later breaks no installed copy.

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::standing::InstallKind;
use crate::verify::{self, VerifyError};

/// A release's manifest.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct Manifest {
    pub version: String,
    #[serde(default)]
    pub pub_date: Option<String>,
    #[serde(default)]
    pub platforms: BTreeMap<String, Entry>,
    #[serde(default)]
    pub knobas: Extra,
}

/// One platform's artifact.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct Entry {
    pub url: String,
    /// The `.sig` file's text.
    pub signature: String,
}

/// knobas's own key.
#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
pub struct Extra {
    /// The highest migration number in the tagged tree.
    #[serde(default)]
    pub schema: Option<i64>,
    /// Each artifact's size in bytes, by platform key.
    #[serde(default)]
    pub sizes: BTreeMap<String, u64>,
}

/// Why a manifest was not taken.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ManifestError {
    #[error("latest.json does not read: {0}")]
    Unreadable(String),
    #[error("latest.json names {0:?}, which is no version")]
    NoVersion(String),
    #[error("latest.json's {0} entry is not of version {1}: {2}")]
    NotBound(String, String, String),
}

/// The platform key of this copy (`windows-x86_64-nsis`).
#[must_use]
pub fn platform_key(kind: InstallKind) -> String {
    let os = if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "macos") {
        "darwin"
    } else {
        "linux"
    };
    let arch = if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else {
        "x86_64"
    };
    format!("{os}-{arch}-{}", kind.word())
}

/// `text` as a manifest.
///
/// # Errors
///
/// [`ManifestError`].
pub fn parse(text: &str) -> Result<Manifest, ManifestError> {
    let manifest: Manifest =
        serde_json::from_str(text).map_err(|error| ManifestError::Unreadable(error.to_string()))?;
    if crate::version::parse(&manifest.version).is_none() {
        return Err(ManifestError::NoVersion(manifest.version));
    }
    Ok(manifest)
}

impl Manifest {
    /// This platform's entry, by `<os>-<arch>-<installer>` and then `<os>-<arch>`, the plugin's
    /// own lookup, with the key it was found under.
    #[must_use]
    pub fn entry_for(&self, key: &str) -> Option<(&str, &Entry)> {
        if let Some((found, entry)) = self.platforms.get_key_value(key) {
            return Some((found.as_str(), entry));
        }
        let short = key.rsplit_once('-').map(|(short, _)| short)?;
        self.platforms
            .get_key_value(short)
            .map(|(found, entry)| (found.as_str(), entry))
    }
}

/// An entry tied to its manifest's version, before a byte is downloaded (decisions D3): the
/// signature's comment names the URL's last segment, that name holds the version, and the URL is
/// that version's release download. The signature itself is verified on the bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bound {
    pub url: String,
    pub file_name: String,
    pub signature: String,
}

/// [`Bound`] for `entry` under `version`.
///
/// # Errors
///
/// [`ManifestError::NotBound`] naming the rule that failed.
pub fn bind(key: &str, version: &str, entry: &Entry) -> Result<Bound, ManifestError> {
    let refuse = |why: String| ManifestError::NotBound(key.to_owned(), version.to_owned(), why);
    let url = url::Url::parse(&entry.url).map_err(|error| refuse(format!("the url: {error}")))?;
    let file_name = url
        .path_segments()
        .and_then(|mut segments| segments.next_back())
        .filter(|name| !name.is_empty())
        .ok_or_else(|| refuse("the url names no file".to_owned()))?
        .to_owned();
    let signature = verify::decode(&entry.signature).map_err(|error| refuse(error.to_string()))?;
    verify::names(&signature, &file_name).map_err(|error| match error {
        VerifyError::OtherFile { signed, .. } => refuse(format!(
            "its signature is for {signed:?}, the url names {file_name:?}"
        )),
        other => refuse(other.to_string()),
    })?;
    let underscore = format!("knobas_{version}_");
    let dash = format!("knobas-{version}-");
    if !(file_name.starts_with(&underscore) || file_name.starts_with(&dash)) {
        return Err(refuse(format!("{file_name:?} is another version's file")));
    }
    let release = format!("/releases/download/v{version}/");
    if !url.path().contains(&release) {
        return Err(refuse(format!("{} is not under {release}", url.path())));
    }
    Ok(Bound {
        url: entry.url.clone(),
        file_name,
        signature: entry.signature.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIG: &str = include_str!("../tests/fixtures/one/knobas_3.2.9-ux28_x64-setup.exe.sig");
    const SIG_27: &str =
        include_str!("../tests/fixtures/other/knobas_3.2.9-ux27_x64-setup.exe.sig");
    const BASE: &str = "https://github.com/BFoerschner/knobas/releases/download";

    fn sample() -> String {
        serde_json::json!({
            "version": "3.2.9-ux28",
            "pub_date": "2026-11-22T09:41:07Z",
            "platforms": {
                "windows-x86_64-nsis": { "url": format!("{BASE}/v3.2.9-ux28/knobas_3.2.9-ux28_x64-setup.exe"), "signature": SIG.trim() },
                "darwin-aarch64-app": { "url": format!("{BASE}/v3.2.9-ux28/knobas_3.2.9-ux28_aarch64.app.tar.gz"), "signature": "x" },
            },
            "knobas": { "schema": 102, "sizes": { "windows-x86_64-nsis": 89_490_164 } },
            "future": { "a plugin field": true }
        })
        .to_string()
    }

    #[test]
    fn parse_reads_the_example_of_section_five() {
        let manifest = parse(&sample()).unwrap();
        assert_eq!(manifest.version, "3.2.9-ux28");
        assert_eq!(manifest.knobas.schema, Some(102));
        assert_eq!(manifest.knobas.sizes["windows-x86_64-nsis"], 89_490_164);
        assert_eq!(manifest.platforms.len(), 2);
    }

    #[test]
    fn unknown_keys_are_ignored() {
        assert!(parse(&sample()).is_ok());
        assert!(parse(r#"{"version":"3.2.9","platforms":{},"notes":"n","x":1}"#).is_ok());
    }

    #[test]
    fn a_manifest_without_this_platform_offers_nothing() {
        let manifest = parse(&sample()).unwrap();
        assert!(manifest.entry_for("linux-x86_64-appimage").is_none());
        assert_eq!(
            manifest
                .entry_for("windows-x86_64-nsis")
                .map(|(key, _)| key),
            Some("windows-x86_64-nsis")
        );
    }

    #[test]
    fn the_short_key_is_the_fallback() {
        let manifest = parse(
            r#"{"version":"3.2.9","platforms":{"windows-x86_64":{"url":"u","signature":"s"}}}"#,
        )
        .unwrap();
        assert_eq!(
            manifest.entry_for("windows-x86_64-msi").map(|(key, _)| key),
            Some("windows-x86_64")
        );
    }

    #[test]
    fn a_bound_entry_names_its_file() {
        let entry = Entry {
            url: format!("{BASE}/v3.2.9-ux28/knobas_3.2.9-ux28_x64-setup.exe"),
            signature: SIG.to_owned(),
        };
        let bound = bind("windows-x86_64-nsis", "3.2.9-ux28", &entry).unwrap();
        assert_eq!(bound.file_name, "knobas_3.2.9-ux28_x64-setup.exe");
    }

    /// Mutant: drop the name check, and a 3.2.9-ux27 file under version 3.2.9-ux28 is offered.
    #[test]
    fn an_entry_whose_file_is_not_of_the_manifests_version_is_refused() {
        let entry = Entry {
            url: format!("{BASE}/v3.2.9-ux28/knobas_3.2.9-ux27_x64-setup.exe"),
            signature: SIG_27.to_owned(),
        };
        let error = bind("windows-x86_64-nsis", "3.2.9-ux28", &entry).unwrap_err();
        assert!(
            error.to_string().contains("another version's file"),
            "{error}"
        );
    }

    #[test]
    fn an_entry_copied_from_another_release_is_refused() {
        let entry = Entry {
            url: format!("{BASE}/v3.2.9-ux27/knobas_3.2.9-ux28_x64-setup.exe"),
            signature: SIG.to_owned(),
        };
        let error = bind("windows-x86_64-nsis", "3.2.9-ux28", &entry).unwrap_err();
        assert!(error.to_string().contains("is not under"), "{error}");
    }

    #[test]
    fn an_entry_whose_signature_names_another_file_is_refused() {
        let entry = Entry {
            url: format!("{BASE}/v3.2.9-ux28/knobas_3.2.9-ux28_x64_en-US.msi"),
            signature: SIG.to_owned(),
        };
        let error = bind("windows-x86_64-msi", "3.2.9-ux28", &entry).unwrap_err();
        assert!(
            error.to_string().contains("its signature is for"),
            "{error}"
        );
    }

    #[test]
    fn a_version_that_is_no_version_is_refused() {
        assert!(matches!(
            parse(r#"{"version":"latest","platforms":{}}"#),
            Err(ManifestError::NoVersion(_))
        ));
    }
}
