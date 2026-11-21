//! The PostgreSQL a release build carries, and what a start still has to do
//! to have one (`docs/decisions/2026-11-14-knobas-ships-its-postgresql.md`).
//!
//! Each installer holds the upstream archive of the pinned version as one
//! file in its resources, `postgresql/postgresql-<version>.tar.gz`
//! (`tauri.conf.json`'s `bundle.resources`; `scripts/fetch-postgres.sh`
//! stages it and `release.yml` fails a build without it). Bring-up unpacks it
//! on a machine that has no PostgreSQL, so a first start needs no network. A
//! build without the file downloads, as every build did before.

use std::path::{Path, PathBuf};

use tauri::Manager;

/// The folder of the bundle's resources that holds the archive, and of this
/// crate that `bundle.resources` maps into it.
pub(crate) const RESOURCE_FOLDER: &str = "postgresql";

/// What a start has to do before PostgreSQL can run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Provision {
    /// It is installed on this machine.
    Installed,
    /// It is not, and this build carries its archive, here.
    Unpack(PathBuf),
    /// It is not, and this build carries none: the start downloads it.
    Download,
}

impl Provision {
    /// What this start has to do, on this machine and from this build.
    pub(crate) async fn of<R: tauri::Runtime>(handle: &tauri::AppHandle<R>) -> Self {
        if knobas_db::install::installed().await {
            return Self::Installed;
        }
        let Ok(resources) = handle.path().resource_dir() else {
            return Self::Download;
        };
        // A stat, so off the worker like the profile's.
        tokio::task::spawn_blocking(move || carried_archive(&resources))
            .await
            .ok()
            .flatten()
            .map_or(Self::Download, Self::Unpack)
    }

    /// The boot screen's line for what is about to take time, `None` when
    /// nothing unusual is. Shown verbatim, so each is what happens next.
    pub(crate) fn detail(&self, first_run: bool) -> Option<String> {
        let sentence = match (self, first_run) {
            (Self::Installed, false) => return None,
            (Self::Installed, true) => "first run: initialising PostgreSQL",
            (Self::Unpack(_), true) => "first run: unpacking and initialising PostgreSQL",
            (Self::Unpack(_), false) => "unpacking PostgreSQL",
            (Self::Download, true) => "first run: downloading and initialising PostgreSQL",
            (Self::Download, false) => "downloading PostgreSQL",
        };
        Some(sentence.to_owned())
    }

    /// Unpack the archive this build carries, if that is what the start has
    /// to do. A failure is returned as text for [`explain`] and is not the
    /// start's: [`knobas_db::EmbeddedDb::start`] downloads what is missing.
    pub(crate) async fn install(&self) -> Option<String> {
        let Self::Unpack(archive) = self else {
            return None;
        };
        match knobas_db::install::install_from(archive).await {
            Ok(_) => None,
            Err(error) => {
                tracing::error!(%error, "the PostgreSQL this build carries did not unpack: downloading it instead");
                Some(error.to_string())
            }
        }
    }
}

/// The archive under `resources`, if this build carries the pinned version's.
fn carried_archive(resources: &Path) -> Option<PathBuf> {
    let archive = resources
        .join(RESOURCE_FOLDER)
        .join(knobas_db::install::archive_name());
    archive.is_file().then_some(archive)
}

/// A start that failed for want of PostgreSQL, said as what a person can act
/// on; any other error as it is.
///
/// `knobas-db` names the version, the place and the failed request. Only the
/// app knows whether this build carried a copy, and that a release installer
/// does: `unpack_failure` is [`Provision::install`]'s answer.
pub(crate) fn explain(
    error: knobas_db::DbError,
    provision: &Provision,
    unpack_failure: Option<&str>,
) -> Box<dyn std::error::Error + Send + Sync> {
    if !matches!(error, knobas_db::DbError::NotInstalled { .. }) {
        return error.into();
    }
    match (provision, unpack_failure) {
        (Provision::Unpack(_), Some(unpack)) => format!(
            "The PostgreSQL this copy of knobas carries could not be unpacked, and downloading \
             it failed too. Reinstall knobas or connect to the internet, then retry. {unpack}. {error}"
        ),
        _ => format!(
            "knobas needs PostgreSQL, and this copy carries none. It tried to download it and \
             could not. Connect to the internet or set a proxy and retry, or install knobas from \
             a release installer, which carries PostgreSQL. {error}"
        ),
    }
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The archive is looked for where `bundle.resources` puts it, under the
    /// name the fetch script gives it; both come from `tauri.conf.json` and
    /// the pin, which this module cannot see.
    #[test]
    fn the_bundle_config_maps_the_folder_the_archive_is_looked_for_in() {
        let config: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).expect("tauri.conf.json");
        assert_eq!(
            config["bundle"]["resources"][format!("{RESOURCE_FOLDER}/")],
            format!("{RESOURCE_FOLDER}/"),
            "bundle.resources must map this crate's `postgresql/` to the same folder"
        );
    }

    /// The pin file names one archive per release target, all of the version
    /// `knobas-db` runs: a pin moved in one place alone would stage an
    /// archive the app never looks for.
    #[test]
    fn the_pinned_archives_are_of_the_version_knobas_runs() {
        let pins = include_str!("../postgresql/archives.sha256");
        let version = knobas_db::install::pinned_version();
        let mut targets = Vec::new();
        for line in pins.lines().filter(|line| !line.starts_with('#')) {
            let (hash, name) = line.split_once("  ").expect("`<sha256>  <file>` per line");
            assert_eq!(hash.len(), 64, "{line}");
            assert!(hash.bytes().all(|b| b.is_ascii_hexdigit()), "{line}");
            let target = name
                .strip_prefix(&format!("postgresql-{version}-"))
                .and_then(|rest| rest.strip_suffix(".tar.gz"))
                .unwrap_or_else(|| panic!("{name} is not an archive of PostgreSQL {version}"));
            targets.push(target);
        }
        targets.sort_unstable();
        assert_eq!(
            targets,
            [
                "aarch64-apple-darwin",
                "x86_64-pc-windows-msvc",
                "x86_64-unknown-linux-gnu"
            ],
            "one pinned archive per target `release.yml` builds"
        );
    }

    #[test]
    fn a_build_carries_the_archive_only_under_the_pinned_versions_name() {
        let resources = tempfile::tempdir().unwrap();
        let folder = resources.path().join(RESOURCE_FOLDER);
        std::fs::create_dir_all(&folder).unwrap();
        assert_eq!(carried_archive(resources.path()), None);

        // Another version's archive is not this build's.
        std::fs::write(folder.join("postgresql-17.5.0.tar.gz"), b"old").unwrap();
        assert_eq!(carried_archive(resources.path()), None);

        let archive = folder.join(knobas_db::install::archive_name());
        std::fs::write(&archive, b"archive").unwrap();
        assert_eq!(carried_archive(resources.path()), Some(archive));
    }

    /// Each line is what the start is about to do, and a start with nothing
    /// unusual to do says nothing.
    #[test]
    fn the_boot_line_says_what_the_start_is_about_to_do() {
        let unpack = Provision::Unpack(PathBuf::from("a.tar.gz"));
        assert_eq!(Provision::Installed.detail(false), None);
        assert_eq!(
            Provision::Installed.detail(true).as_deref(),
            Some("first run: initialising PostgreSQL")
        );
        assert_eq!(
            unpack.detail(true).as_deref(),
            Some("first run: unpacking and initialising PostgreSQL")
        );
        assert_eq!(
            unpack.detail(false).as_deref(),
            Some("unpacking PostgreSQL")
        );
        assert_eq!(
            Provision::Download.detail(true).as_deref(),
            Some("first run: downloading and initialising PostgreSQL")
        );
        assert_eq!(
            Provision::Download.detail(false).as_deref(),
            Some("downloading PostgreSQL")
        );
    }

    fn not_installed() -> knobas_db::DbError {
        knobas_db::DbError::NotInstalled {
            version: "18.6.0".to_owned(),
            installation_dir: PathBuf::from("/home/u/.theseus/postgresql/18.6.0"),
            source: "error sending request".into(),
        }
    }

    /// A build with no archive that could not download says what to do about
    /// it, and keeps what `knobas-db` said.
    #[test]
    fn a_failed_download_says_what_a_person_can_do() {
        let said = explain(not_installed(), &Provision::Download, None).to_string();
        assert!(said.contains("this copy carries none"), "{said}");
        assert!(
            said.contains("a release installer, which carries PostgreSQL"),
            "{said}"
        );
        assert!(
            said.contains("PostgreSQL 18.6.0 is not installed"),
            "{said}"
        );
        assert!(said.contains("error sending request"), "{said}");
    }

    #[test]
    fn a_carried_archive_that_did_not_unpack_is_named_first() {
        let provision = Provision::Unpack(PathBuf::from("a.tar.gz"));
        let said =
            explain(not_installed(), &provision, Some("no space left on device")).to_string();
        assert!(said.contains("could not be unpacked"), "{said}");
        assert!(said.contains("no space left on device"), "{said}");
        assert!(said.contains("downloading it failed too"), "{said}");
    }

    /// Any other error is passed on as `knobas-db` said it.
    #[test]
    fn another_error_is_left_as_it_is() {
        let error = knobas_db::DbError::AlreadyRunning {
            data_dir: PathBuf::from("/data"),
            reason: "knobas process 7 has this profile open".to_owned(),
        };
        let before = error.to_string();
        let said = explain(error, &Provision::Download, None).to_string();
        assert_eq!(said, before);
    }
}
