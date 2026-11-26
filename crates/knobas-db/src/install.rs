//! Installing PostgreSQL from the archive a release build carries.
//!
//! A release installer holds the upstream archive of the pinned version as
//! one file in its resources (`docs/decisions/2026-11-14-knobas-ships-its-postgresql.md`).
//! [`install_from`] unpacks it into the shared installation directory, where
//! [`EmbeddedDb::start`](crate::EmbeddedDb::start) then finds PostgreSQL
//! installed and downloads nothing. A build without the file (`just dev`, a
//! test run) calls nothing here and downloads on a machine's first start, as
//! every build did before.
//!
//! The archive is unpacked rather than run where the installer put it: a
//! server that outlives the app (an orphan the next start adopts) keeps its
//! binaries through an upgrade or a removal of the app, an AppImage's mount is
//! gone the moment the app exits, and files a program writes carry no macOS
//! quarantine flag, where the files inside a downloaded `.app` do.

use std::path::{Path, PathBuf};

use crate::embedded::{DbError, PG_VERSION_REQ, installation_dir};

/// The pinned PostgreSQL version alone, `18.6.0`: [`PG_VERSION_REQ`] is a
/// semver requirement (`=18.6.0`), and a directory or a file is named after
/// the version.
#[must_use]
pub fn pinned_version() -> &'static str {
    PG_VERSION_REQ.trim_start_matches(['=', '^', '~', ' '])
}

/// File name of the archive a release build carries, `postgresql-18.6.0.tar.gz`.
///
/// The version is in the name, so a build whose pin moved never unpacks an
/// archive staged for the version before: it does not find one.
#[must_use]
pub fn archive_name() -> String {
    format!("postgresql-{}.tar.gz", pinned_version())
}

/// Where the pinned version is installed: `~/.theseus/postgresql/18.6.0`.
fn version_dir() -> PathBuf {
    installation_dir().join(pinned_version())
}

/// Whether the pinned PostgreSQL is installed on this machine, by the test
/// `postgresql_embedded` and its extractor both use: the version's directory
/// is there. It is renamed into place whole, so it is never half an install.
pub async fn installed() -> bool {
    tokio::task::spawn_blocking(|| version_dir().exists())
        .await
        .unwrap_or(false)
}

/// Install the pinned PostgreSQL from `archive`, unless it is installed.
/// Answers whether this call unpacked it.
///
/// `archive` is the upstream `.tar.gz` of the pinned version for this
/// platform. It is unpacked by the extractor a download goes through, into a
/// scratch directory beside the target and renamed into place, so an unpack
/// cut short leaves no version directory and the next start unpacks again.
///
/// # Errors
///
/// [`DbError::Io`] when the archive cannot be read, and
/// [`DbError::BundledArchive`] when it cannot be unpacked. Neither is final
/// for a caller that goes on to [`EmbeddedDb::start`](crate::EmbeddedDb::start):
/// that start downloads what is still missing.
pub async fn install_from(archive: &Path) -> Result<bool, DbError> {
    unpack(archive, &version_dir()).await
}

/// Make every unpack of a theseus archive keep the archive's hard links,
/// whether it is [`install_from`]'s or the download `EmbeddedDb::start` falls
/// back to. Called before either can run; registering twice is harmless, and
/// this registers once.
///
/// `postgresql_archive` 0.21's extractor writes files, directories and
/// symlinks, and skips hard links. theseus's 18.6.1 Linux build, which knobas
/// runs there (`PG_VERSION_REQ`), stores 257 of its time zones as hard links,
/// `UTC` among them, so without this the server refuses knobas's
/// `TimeZone=UTC` ("invalid value for parameter"). 0.22 keeps them, and also
/// rewrites the command builders knobas's start depends on (the `-F` before
/// `-c fsync=on`, see `Cargo.toml`), so it waits for its own change.
pub(crate) fn keep_hard_links() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        // Newer registrations are tried first, so this one wins for theseus's
        // URL. The registry's lock is only poisoned by a panic inside it.
        let registered = postgresql_archive::extractor::registry::register(
            |url| Ok(url.starts_with(postgresql_archive::configuration::theseus::URL)),
            extract_keeping_hard_links,
        );
        if let Err(error) = registered {
            tracing::warn!(%error, "could not register the extractor that keeps hard links");
        }
    });
}

/// The crate's own theseus extractor, then the archive's hard links laid next
/// to it.
/// It has the registry's `ExtractFn` shape, `&Vec<u8>` and all.
fn extract_keeping_hard_links(
    bytes: &Vec<u8>,
    directories: &postgresql_archive::extractor::ExtractDirectories,
) -> postgresql_archive::Result<Vec<PathBuf>> {
    let mut files = postgresql_archive::configuration::theseus::extract(bytes, directories)?;
    // The directory the theseus extractor renamed the unpack to.
    let into = directories.get_path(".")?;
    files.extend(link_hard_links(bytes, &into)?);
    Ok(files)
}

/// Lay each hard link in the `.tar.gz` `bytes` into `into`, which holds the
/// archive already unpacked without them: a link to the file it names, or a
/// copy where the filesystem has no hard links. The top-level directory is
/// stripped as the extractor strips it. A link that is there already, whose
/// file is not, or whose path leaves `into` is passed over.
fn link_hard_links(bytes: &[u8], into: &Path) -> std::io::Result<Vec<PathBuf>> {
    /// `top/a/b` as `a/b`, or `None` for anything that is not plain names.
    fn inside(path: &Path) -> Option<PathBuf> {
        let mut parts = path.components();
        parts.next()?;
        let rest: PathBuf = parts
            .map(|part| match part {
                std::path::Component::Normal(name) => Some(name),
                _ => None,
            })
            .collect::<Option<_>>()?;
        (!rest.as_os_str().is_empty()).then_some(rest)
    }

    let mut made = Vec::new();
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(bytes));
    for entry in archive.entries()? {
        let entry = entry?;
        if entry.header().entry_type() != tar::EntryType::Link {
            continue;
        }
        let Some(link) = inside(&entry.path()?) else {
            continue;
        };
        let Some(original) = entry.link_name()?.and_then(|name| inside(&name)) else {
            continue;
        };
        let (link, original) = (into.join(link), into.join(original));
        if link.symlink_metadata().is_ok() || !original.is_file() {
            continue;
        }
        if let Some(parent) = link.parent() {
            std::fs::create_dir_all(parent)?;
        }
        if std::fs::hard_link(&original, &link).is_err() {
            std::fs::copy(&original, &link)?;
        }
        made.push(link);
    }
    Ok(made)
}

/// [`install_from`], into `version_dir`.
async fn unpack(archive: &Path, version_dir: &Path) -> Result<bool, DbError> {
    keep_hard_links();
    let started = std::time::Instant::now();
    let (from, into) = (archive.to_path_buf(), version_dir.to_path_buf());
    // The extractor is synchronous under its `async fn`: it inflates and
    // writes every file before it answers, 156 MB on Windows. So the whole of
    // it runs on a blocking thread, which drives the future itself.
    let runtime = tokio::runtime::Handle::current();
    let unpacked = tokio::task::spawn_blocking(move || {
        if into.exists() {
            return Ok(false);
        }
        let bytes = std::fs::read(&from).map_err(|source| DbError::io(&from, source))?;
        let extracted = runtime.block_on(postgresql_archive::extract(
            postgresql_archive::configuration::theseus::URL,
            &bytes,
            &into,
        ));
        match extracted {
            // Nothing extracted: another process installed it meanwhile.
            Ok(files) => Ok(!files.is_empty()),
            // Two first starts at once (the demo profile beside the default
            // one) both unpack, and the second rename finds the first's
            // directory in place. That is an install, not a failure.
            Err(_) if into.exists() => Ok(false),
            Err(source) => Err(DbError::BundledArchive {
                archive: from,
                source: Box::new(source),
            }),
        }
    })
    .await
    .map_err(|error| DbError::io(archive, std::io::Error::other(error)))??;
    if unpacked {
        tracing::info!(
            archive = %archive.display(),
            into = %version_dir.display(),
            ms = started.elapsed().as_millis(),
            "postgres unpacked from the archive this build carries"
        );
    }
    Ok(unpacked)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::embedded::{build_settings, launch};

    /// A `.tar.gz` laid out as upstream's: one top-level directory, which the
    /// extractor strips.
    fn archive_of(dir: &Path, fill: impl FnOnce(&mut tar::Builder<Vec<u8>>)) -> PathBuf {
        let mut tar = tar::Builder::new(Vec::new());
        tar.follow_symlinks(false);
        fill(&mut tar);
        let tar = tar.into_inner().unwrap();
        // Stored, not deflated: the extractor inflates either, and an
        // unoptimised test build deflates a whole install slowly.
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::none());
        std::io::Write::write_all(&mut gz, &tar).unwrap();
        let path = dir.join(archive_name());
        std::fs::write(&path, gz.finish().unwrap()).unwrap();
        path
    }

    /// An archive holding one program, `bin/postgres`, under the directory
    /// entries upstream's has: the extractor creates no parent of its own.
    fn small_archive(dir: &Path) -> PathBuf {
        archive_of(dir, |tar| {
            for directory in [
                "postgresql-18.6.0-some-target/",
                "postgresql-18.6.0-some-target/bin/",
            ] {
                let mut header = tar::Header::new_gnu();
                header.set_entry_type(tar::EntryType::Directory);
                header.set_size(0);
                header.set_mode(0o755);
                header.set_cksum();
                tar.append_data(&mut header, directory, std::io::empty())
                    .unwrap();
            }
            let body = b"#!/bin/sh\n";
            let mut header = tar::Header::new_gnu();
            header.set_size(body.len() as u64);
            header.set_mode(0o755);
            header.set_cksum();
            tar.append_data(
                &mut header,
                "postgresql-18.6.0-some-target/bin/postgres",
                &body[..],
            )
            .unwrap();
        })
    }

    #[test]
    fn the_archive_is_named_after_the_pinned_version() {
        // theseus's 18.6.1 carries its libraries on Linux; macOS and Windows
        // are still on 18.6.0 (`PG_VERSION_REQ`).
        let want = if cfg!(target_os = "linux") {
            "18.6.1"
        } else {
            "18.6.0"
        };
        assert_eq!(pinned_version(), want);
        assert_eq!(archive_name(), format!("postgresql-{want}.tar.gz"));
        assert!(version_dir().ends_with(format!("postgresql/{want}")));
    }

    /// A hard link in the archive is in the install too. theseus's 18.6.1
    /// keeps `share/timezone/UTC` as one, and `postgresql_archive` 0.21 alone
    /// drops it: the server then refuses `TimeZone=UTC` and knobas never
    /// starts. A link whose path would leave the install is not followed.
    #[tokio::test]
    async fn a_hard_link_in_the_archive_is_unpacked_with_it() {
        let dir = tempfile::tempdir().unwrap();
        let archive = archive_of(dir.path(), |tar| {
            for directory in ["top/", "top/share/", "top/share/timezone/"] {
                let mut header = tar::Header::new_gnu();
                header.set_entry_type(tar::EntryType::Directory);
                header.set_size(0);
                header.set_mode(0o755);
                header.set_cksum();
                tar.append_data(&mut header, directory, std::io::empty())
                    .unwrap();
            }
            let body = b"TZif2 UTC";
            let mut header = tar::Header::new_gnu();
            header.set_size(body.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            tar.append_data(&mut header, "top/share/timezone/UCT", &body[..])
                .unwrap();
            for (link, to) in [
                ("top/share/timezone/UTC", "top/share/timezone/UCT"),
                ("top/share/timezone/Etc/UTC", "top/share/timezone/UCT"),
                ("top/share/timezone/away", "top/../../outside"),
            ] {
                let mut header = tar::Header::new_gnu();
                header.set_entry_type(tar::EntryType::Link);
                header.set_size(0);
                header.set_mode(0o644);
                tar.append_link(&mut header, link, to).unwrap();
            }
        });
        let into = dir.path().join("install").join(pinned_version());

        assert!(unpack(&archive, &into).await.unwrap());

        let zones = into.join("share").join("timezone");
        assert_eq!(std::fs::read(zones.join("UTC")).unwrap(), b"TZif2 UTC");
        assert_eq!(
            std::fs::read(zones.join("Etc").join("UTC")).unwrap(),
            b"TZif2 UTC"
        );
        assert!(!zones.join("away").exists());
        assert!(!dir.path().join("outside").exists());
    }

    /// The archive's own top-level directory is not part of the install, and
    /// a program in it is still one after the unpack.
    #[tokio::test]
    async fn an_archive_unpacks_into_the_version_directory() {
        let dir = tempfile::tempdir().unwrap();
        let archive = small_archive(dir.path());
        let into = dir.path().join("install").join(pinned_version());

        assert!(unpack(&archive, &into).await.unwrap());

        let postgres = into.join("bin").join("postgres");
        assert_eq!(std::fs::read(&postgres).unwrap(), b"#!/bin/sh\n");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&postgres).unwrap().permissions().mode();
            assert_eq!(mode & 0o111, 0o111, "the program lost its execute bits");
        }
        // No scratch directory and no lock file are left beside it.
        let left: Vec<_> = std::fs::read_dir(into.parent().unwrap())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(left, [std::ffi::OsString::from(pinned_version())]);
    }

    /// An installed version is not unpacked over, and its archive is not even
    /// read: the one here does not exist.
    #[tokio::test]
    async fn an_installed_version_is_left_as_it_is() {
        let dir = tempfile::tempdir().unwrap();
        let into = dir.path().join(pinned_version());
        std::fs::create_dir_all(&into).unwrap();
        std::fs::write(into.join("kept"), "as it was").unwrap();

        let unpacked = unpack(&dir.path().join("no-such.tar.gz"), &into).await;

        assert!(!unpacked.unwrap());
        assert_eq!(
            std::fs::read_to_string(into.join("kept")).unwrap(),
            "as it was"
        );
    }

    #[tokio::test]
    async fn a_missing_archive_is_an_error_that_names_it() {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join(archive_name());
        let into = dir.path().join(pinned_version());

        match unpack(&archive, &into).await {
            Err(DbError::Io { path, .. }) => assert_eq!(path, archive),
            other => panic!("expected the archive's read error, got {other:?}"),
        }
        assert!(!into.exists());
    }

    /// A file that is not an archive installs nothing: no version directory
    /// stands in for PostgreSQL, so the start that follows still installs it.
    #[tokio::test]
    async fn a_damaged_archive_is_an_error_and_installs_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join(archive_name());
        std::fs::write(&archive, b"not a gzip stream").unwrap();
        let into = dir.path().join("install").join(pinned_version());

        match unpack(&archive, &into).await {
            Err(error @ DbError::BundledArchive { .. }) => {
                let said = error.to_string();
                assert!(said.contains(&archive.display().to_string()), "{said}");
            }
            other => panic!("expected the unpack to fail, got {other:?}"),
        }
        assert!(!into.exists());
    }

    /// Settings for a server on `root`, whose PostgreSQL is looked for under
    /// `install` and downloaded from nowhere: the address names no repository,
    /// so a start that reaches for the network fails at once, on any machine.
    fn settings_without_a_network(root: &Path, install: &Path) -> postgresql_embedded::Settings {
        let mut settings = build_settings(root).unwrap();
        settings.installation_dir = install.to_path_buf();
        settings.releases_url = "http://127.0.0.1:9/no-such-repository".to_owned();
        settings
    }

    /// The whole of a first start on a machine with no PostgreSQL and no
    /// network: unpacked from the archive, `initdb`, started, queried.
    ///
    /// The archive is this machine's own install packed again, so the test
    /// runs the real programs for this platform through the extractor
    /// (symbolic links and execute bits included) and needs no download of
    /// its own beyond the one every embedded test shares.
    #[tokio::test]
    async fn a_first_start_from_the_archive_needs_no_network() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("db");
        let install = dir.path().join("install");

        // This machine's install, present after any embedded start.
        let warm = tempfile::tempdir().unwrap();
        crate::EmbeddedDb::start(crate::DbConfig {
            root_dir: warm.path().to_path_buf(),
            existing_url: None,
        })
        .await
        .expect("the machine's own PostgreSQL starts")
        .stop()
        .await
        .unwrap();
        let installed = version_dir();
        let archive = archive_of(dir.path(), |tar| {
            tar.append_dir_all("postgresql-18.6.0-this-machine", &installed)
                .unwrap();
        });

        assert!(
            unpack(&archive, &install.join(pinned_version()))
                .await
                .unwrap()
        );
        let db = launch(&root, settings_without_a_network(&root, &install))
            .await
            .expect("a start on the unpacked archive downloads nothing");
        let version: (String,) = sqlx::query_as("show server_version")
            .fetch_one(db.pool())
            .await
            .unwrap();
        assert!(pinned_version().starts_with(&version.0), "{}", version.0);
        db.stop().await.unwrap();
    }

    /// Without the archive the same start has nothing to install from, and
    /// says so by the version and the place, not as a failed request alone.
    #[tokio::test]
    async fn a_first_start_with_nothing_to_install_from_says_what_is_missing() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("db");
        let install = dir.path().join("install");

        let Err(error) = launch(&root, settings_without_a_network(&root, &install)).await else {
            panic!("nothing is installed and nothing can be downloaded")
        };

        match &error {
            DbError::NotInstalled {
                version,
                installation_dir,
                ..
            } => {
                assert_eq!(version, pinned_version());
                assert!(installation_dir.starts_with(&install));
            }
            other => panic!("expected the install to be what failed, got {other:?}"),
        }
        let said = error.to_string();
        assert!(
            said.contains(&format!("PostgreSQL {} is not installed", pinned_version())),
            "{said}"
        );
        assert!(
            !root.join("data").join("PG_VERSION").exists(),
            "no data directory is made without PostgreSQL"
        );
    }
}
