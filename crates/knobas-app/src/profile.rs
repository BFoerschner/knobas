//! Which knobas this process is: the real one, or the demo.
//!
//! Ruling P13. §14a's demo mode loads the Tidewater fixture, and stream D and
//! stream E develop against it while real sources are being built into the same
//! application. Sharing one database would mean the Tidewater fixture's items
//! in every search over a real corpus, for ever -- so `--demo` selects a whole
//! profile: its own data directory (and therefore its own embedded PostgreSQL,
//! on its own port), and its own keychain service, so a demo run is not even
//! offered the real credentials.
//!
//! Nothing here is a security boundary -- both profiles belong to the same user
//! on the same machine. It is a *mixing* boundary, which is the one §14a asks
//! for.

use std::path::{Path, PathBuf};

/// The bundle identifier, which is also the keychain service's stem
/// (interfaces §3). Pinned against `tauri.conf.json` by a test below.
pub const APP_IDENTIFIER: &str = "dev.knobas.desktop";

/// The flag that selects the demo profile.
pub const DEMO_FLAG: &str = "--demo";

/// Which profile this process runs as, and where its state lives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Profile {
    /// Whether this is the demo profile. Reported to the frontend as
    /// `AppStatus.demo` (stream D).
    pub demo: bool,
    /// The root of everything this profile owns.
    pub dir: PathBuf,
}

impl Profile {
    /// Read the profile out of the process arguments.
    ///
    /// A flag rather than an environment variable: it is what §14a writes
    /// (`knobas --demo`), it shows up in a process list, and it cannot be
    /// inherited by accident from a shell that once exported it.
    #[must_use]
    pub fn from_args<I>(args: I, app_data_dir: &Path) -> Self
    where
        I: IntoIterator<Item = String>,
    {
        let demo = args.into_iter().any(|arg| arg == DEMO_FLAG);
        Self {
            demo,
            // A subdirectory, so the default profile keeps M0's layout
            // untouched: an existing installation must not find its database
            // moved.
            dir: if demo {
                app_data_dir.join("demo")
            } else {
                app_data_dir.to_path_buf()
            },
        }
    }

    /// Where the embedded PostgreSQL's data directory lives.
    ///
    /// Separate directories mean separate servers on separate ports, because
    /// `postgresql_embedded` records the port inside the data directory --
    /// so the demo and the real app can run side by side.
    #[must_use]
    pub fn db_root(&self) -> PathBuf {
        self.dir.join("db")
    }

    /// Where scheduled backup archives are written (§14).
    ///
    /// Inside the profile, like the database, so a demo run cannot write into
    /// the real profile's backups (P13) -- and so that deleting a profile
    /// takes its archives with it rather than orphaning them.
    #[must_use]
    pub fn backup_dir(&self) -> PathBuf {
        self.dir.join("backups")
    }

    /// The OS keychain service this profile's secrets live under (§3).
    ///
    /// `dev.knobas.desktop` in a release build, `.dev` in a debug one (so
    /// `just dev` cannot reach the credentials of an installed knobas), and
    /// `.demo` on top of either.
    #[must_use]
    pub fn keychain_service(&self) -> String {
        let mut service = APP_IDENTIFIER.to_owned();
        if cfg!(debug_assertions) {
            service.push_str(".dev");
        }
        if self.demo {
            service.push_str(".demo");
        }
        service
    }

    /// The database this profile connects to.
    ///
    /// `existing_url` is `KNOBAS_DB_URL`, the escape hatch for running against
    /// a server the user manages -- which is a server with real data in it.
    /// A demo profile therefore ignores it and starts its own: honouring it
    /// would load the fixture into exactly the corpus P13 keeps it out of.
    #[must_use]
    pub fn db_config(&self, existing_url: Option<String>) -> knobas_db::DbConfig {
        let existing_url = existing_url.filter(|url| !url.trim().is_empty());
        if self.demo && existing_url.is_some() {
            tracing::warn!(
                "{} is set but this is the demo profile: starting a scratch database instead",
                crate::DB_URL_ENV
            );
        }
        knobas_db::DbConfig {
            root_dir: self.db_root(),
            existing_url: if self.demo { None } else { existing_url },
        }
    }

    /// Whether the Tidewater fixture may be loaded here.
    #[must_use]
    pub fn allows_demo_data(&self) -> bool {
        self.demo
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(extra: &[&str]) -> Vec<String> {
        std::iter::once("knobas".to_owned())
            .chain(extra.iter().map(|a| (*a).to_owned()))
            .collect()
    }

    fn data_dir() -> PathBuf {
        PathBuf::from("/Users/tester/Library/Application Support/dev.knobas.desktop")
    }

    /// The default profile keeps M0's layout exactly: an existing installation
    /// must not find its database moved out from under it.
    #[test]
    fn the_default_profile_is_the_app_data_directory() {
        let profile = Profile::from_args(args(&[]), &data_dir());
        assert!(!profile.demo);
        assert_eq!(profile.dir, data_dir());
        assert_eq!(profile.db_root(), data_dir().join("db"));
    }

    /// A demo profile is a sibling directory, so nothing it writes can reach
    /// the real corpus -- including the embedded server, which lives in the
    /// data directory and therefore gets its own port too.
    #[test]
    fn demo_is_a_separate_directory_database_and_keychain() {
        let real = Profile::from_args(args(&[]), &data_dir());
        let demo = Profile::from_args(args(&["--demo"]), &data_dir());
        assert!(demo.demo);
        assert_eq!(demo.dir, data_dir().join("demo"));
        assert_ne!(demo.db_root(), real.db_root());
        assert_ne!(
            demo.backup_dir(),
            real.backup_dir(),
            "a demo run must not write into the real profile's archives"
        );
        assert_ne!(demo.keychain_service(), real.keychain_service());
        assert!(
            demo.keychain_service().ends_with(".demo"),
            "{}",
            demo.keychain_service()
        );
        for service in [real.keychain_service(), demo.keychain_service()] {
            assert!(service.starts_with(APP_IDENTIFIER), "{service}");
        }
    }

    /// `KNOBAS_DB_URL` points knobas at a server with real data in it. Honoured
    /// in a demo profile it would load the fixture straight into that -- the
    /// one thing P13 exists to prevent.
    #[test]
    fn a_demo_profile_ignores_an_externally_managed_database() {
        let url = Some("postgres://localhost/knobas".to_owned());
        let real = Profile::from_args(args(&[]), &data_dir()).db_config(url.clone());
        assert_eq!(real.existing_url, url);

        let demo = Profile::from_args(args(&["--demo"]), &data_dir()).db_config(url);
        assert_eq!(
            demo.existing_url, None,
            "demo data must never reach a real database"
        );
        assert_eq!(demo.root_dir, data_dir().join("demo").join("db"));
    }

    /// M0's rule, kept: an empty value counts as unset.
    #[test]
    fn a_blank_database_url_counts_as_unset() {
        let profile = Profile::from_args(args(&[]), &data_dir());
        assert_eq!(profile.db_config(Some("   ".to_owned())).existing_url, None);
    }

    /// Only the demo profile may load the fixture. The guard is here rather
    /// than in the command so it is testable without a webview.
    #[test]
    fn only_the_demo_profile_allows_demo_data() {
        assert!(Profile::from_args(args(&["--demo"]), &data_dir()).allows_demo_data());
        assert!(!Profile::from_args(args(&[]), &data_dir()).allows_demo_data());
    }

    /// The keychain service is built from the bundle identifier, which lives
    /// in a file this module cannot see. A rename there without one here would
    /// orphan every stored credential.
    #[test]
    fn the_identifier_matches_the_tauri_config() {
        let config: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).expect("tauri.conf.json");
        assert_eq!(config["identifier"], APP_IDENTIFIER);
    }
}
