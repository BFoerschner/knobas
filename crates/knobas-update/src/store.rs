//! Settings and state, in two files beside the profile (decisions D11).
//!
//! `<app data>/update/settings.json` and `state.json`, each written to a temp file and renamed.
//! A missing or unreadable file reads as the defaults, with a warning; a broken one is left as it
//! is until the next change writes it. Not the database: the check must work when the database
//! cannot start, and the channel belongs to the installed copy, not to a profile's data.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::channel::Channel;

/// What the person chose.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// *Check for updates*.
    pub check: bool,
    pub channel: Channel,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            check: true,
            channel: Channel::Releases,
        }
    }
}

/// How the last check ended.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Outcome {
    Ok,
    Offline {
        message: String,
    },
    RateLimited {
        message: String,
        reset: Option<DateTime<Utc>>,
    },
    BadAnswer {
        message: String,
    },
}

/// The channel's newest version as last seen, and this copy's entry in its manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Seen {
    pub version: String,
    pub pre_release: bool,
    pub published_at: Option<DateTime<Utc>>,
    pub page_url: String,
    pub schema: Option<i64>,
    /// This copy's artifact, bound to the version; `None` when the manifest has no entry for it.
    pub artifact: Option<Artifact>,
    /// Which channel answered.
    pub channel: Channel,
}

/// One artifact, bound to its version (`manifest::bind`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Artifact {
    pub key: String,
    pub url: String,
    pub file_name: String,
    pub signature: String,
    pub bytes: Option<u64>,
}

/// An install in flight, written before the quit and read at the next start.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attempt {
    pub from: String,
    pub to: String,
    pub at: DateTime<Utc>,
    pub backup: Option<String>,
    /// The private folder the artifact was staged in, removed at the next start.
    pub staged: Option<PathBuf>,
    /// Where the Mac's swap left the old bundle when it could not stage inside `staged` (the app on another volume than
    /// the temp folder), removed at the next start too.
    #[serde(default)]
    pub aside: Option<PathBuf>,
}

/// The last update on this computer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export_to = "update.ts")]
pub struct LastUpdate {
    pub from: String,
    pub to: String,
    pub at: DateTime<Utc>,
    pub installed: bool,
    pub backup: Option<String>,
}

/// `state.json`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct State {
    pub checked_at: Option<DateTime<Utc>>,
    pub outcome: Option<Outcome>,
    pub newest: Option<Seen>,
    pub attempt: Option<Attempt>,
    pub last: Option<LastUpdate>,
}

/// The two files of one copy.
#[derive(Debug, Clone)]
pub struct Files {
    dir: PathBuf,
}

impl Files {
    /// The files under `<app data>/update`.
    #[must_use]
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    /// Where they are.
    #[must_use]
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// The settings, or the defaults.
    #[must_use]
    pub fn settings(&self) -> Settings {
        read(&self.dir.join("settings.json"))
    }

    /// The state, or the defaults.
    #[must_use]
    pub fn state(&self) -> State {
        read(&self.dir.join("state.json"))
    }

    /// Store the settings, whole or not at all.
    ///
    /// # Errors
    ///
    /// The file system's.
    pub fn save_settings(&self, settings: &Settings) -> std::io::Result<()> {
        write(&self.dir, "settings.json", settings)
    }

    /// Store the state, whole or not at all.
    ///
    /// # Errors
    ///
    /// The file system's.
    pub fn save_state(&self, state: &State) -> std::io::Result<()> {
        write(&self.dir, "state.json", state)
    }
}

fn read<T: Default + serde::de::DeserializeOwned>(path: &Path) -> T {
    match std::fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_else(|error| {
            tracing::warn!(file = %path.display(), %error, "an update file does not read; the defaults stand");
            T::default()
        }),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => T::default(),
        Err(error) => {
            tracing::warn!(file = %path.display(), %error, "an update file cannot be read; the defaults stand");
            T::default()
        }
    }
}

fn write<T: Serialize>(dir: &Path, name: &str, value: &T) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let text = serde_json::to_vec_pretty(value).map_err(std::io::Error::other)?;
    let mut temp = tempfile::NamedTempFile::new_in(dir)?;
    std::io::Write::write_all(&mut temp, &text)?;
    temp.as_file().sync_all()?;
    temp.persist(dir.join(name)).map_err(|error| error.error)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Mutant: default channel `pre_releases`.
    #[test]
    fn a_missing_file_is_the_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let files = Files::new(dir.path().join("update"));
        assert_eq!(
            files.settings(),
            Settings {
                check: true,
                channel: Channel::Releases
            }
        );
        assert_eq!(files.state(), State::default());
    }

    #[test]
    fn a_broken_file_is_the_defaults_and_is_not_overwritten_until_a_change() {
        let dir = tempfile::tempdir().unwrap();
        let files = Files::new(dir.path().to_path_buf());
        std::fs::write(dir.path().join("settings.json"), b"{ not json").unwrap();
        assert_eq!(files.settings(), Settings::default());
        assert_eq!(
            std::fs::read(dir.path().join("settings.json")).unwrap(),
            b"{ not json"
        );
        files
            .save_settings(&Settings {
                check: false,
                channel: Channel::PreReleases,
            })
            .unwrap();
        assert_eq!(
            files.settings(),
            Settings {
                check: false,
                channel: Channel::PreReleases
            }
        );
    }

    /// The rename: no temp file is left, and the file holds the whole value.
    #[test]
    fn a_write_is_whole_or_absent() {
        let dir = tempfile::tempdir().unwrap();
        let files = Files::new(dir.path().to_path_buf());
        let state = State {
            checked_at: DateTime::from_timestamp(1_800_000_000, 0),
            outcome: Some(Outcome::Ok),
            ..State::default()
        };
        files.save_state(&state).unwrap();
        assert_eq!(files.state(), state);
        let names: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec!["state.json".to_owned()]);
    }

    #[test]
    fn the_settings_file_is_the_documented_shape() {
        let text = serde_json::to_string(&Settings::default()).unwrap();
        assert_eq!(text, r#"{"check":true,"channel":"releases"}"#);
        let pre: Settings = serde_json::from_str(r#"{"channel":"pre_releases"}"#).unwrap();
        assert_eq!(
            pre,
            Settings {
                check: true,
                channel: Channel::PreReleases
            }
        );
    }
}
