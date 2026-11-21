//! knobas updates itself (ADR-0052, `docs/decisions/2026-11-21-knobas-updates-itself.md`).
//!
//! The wiring of `knobas-update` to the app: the managed [`Updater`], its schedule, the download
//! and install task, the backup before an install, the hand-off of the Windows installer to the
//! exit ([`at_exit`]), and the attempt read back at the next start. The commands in
//! `commands/update.rs` are shims over it.
//!
//! Nothing here needs the database: the settings and the state are two files beside the profile,
//! so the page and the start screen's line work when PostgreSQL cannot start. The backup is
//! reached through the managed `BackupState` and skipped when there is none.
//!
//! The side effects (an event, a backup, the quit) go through [`Host`], so the order of an
//! install is tested on recording fakes.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use chrono::{DateTime, Utc};
use knobas_update::channel::{self, Channel, CheckError, Endpoints, Fetch};
use knobas_update::download::{self, DownloadError};
use knobas_update::install::{self, windows::Launch};
use knobas_update::keys::Keys;
use knobas_update::standing::{InstallKind, Standing};
use knobas_update::store::{self, Artifact, Attempt, Files, LastUpdate, Outcome, Settings};
use knobas_update::{manifest, schedule, verify, version};
use tokio_util::sync::CancellationToken;

/// What the page and the cog read: everything known, from memory.
#[derive(Debug, Clone, PartialEq, serde::Serialize, ts_rs::TS)]
#[ts(export_to = "update.ts")]
pub struct UpdateState {
    /// The running version, `3.2.9-ux27`.
    pub current: String,
    pub current_is_pre_release: bool,
    /// The greatest migration this build embeds.
    pub current_schema: i64,
    /// Whether this build trusts any signing key. Until Björn's two keys are committed it trusts
    /// none, and the page says updates aren't set up: nothing is checked or downloaded.
    pub set_up: bool,
    /// *Check for updates*.
    pub check: bool,
    pub channel: Channel,
    pub standing: Standing,
    /// False in a debug build, the demo profile and under `KNOBAS_UPDATE=off`.
    pub checks_itself: bool,
    pub checked_at: Option<DateTime<Utc>>,
    pub next_check_at: Option<DateTime<Utc>>,
    pub check_failed: Option<CheckFailed>,
    /// The channel's newest version, also when it is not newer.
    pub newest: Option<Seen>,
    /// Set only when `newest` is newer than `current`.
    pub offer: Option<Offer>,
    pub phase: Phase,
    /// Whether the cog carries the dot.
    pub marked: bool,
    pub last: Option<LastUpdate>,
    /// The update that went in on this start: the toast says so once.
    pub just_updated: bool,
}

/// Why the last check found nothing to say.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, ts_rs::TS)]
#[ts(export_to = "update.ts")]
pub struct CheckFailed {
    pub kind: CheckFailedKind,
    pub message: String,
    pub at: DateTime<Utc>,
}

/// The three ways a check fails.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
#[ts(export_to = "update.ts")]
pub enum CheckFailedKind {
    Offline,
    RateLimited,
    BadAnswer,
}

/// The channel's newest version.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, ts_rs::TS)]
#[ts(export_to = "update.ts")]
pub struct Seen {
    pub version: String,
    pub pre_release: bool,
    pub published_at: Option<DateTime<Utc>>,
    pub page_url: String,
}

/// A newer version than this one.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, ts_rs::TS)]
#[ts(export_to = "update.ts")]
pub struct Offer {
    pub version: String,
    pub pre_release: bool,
    pub published_at: Option<DateTime<Utc>>,
    /// This copy's artifact's size.
    pub bytes: Option<u64>,
    /// Whether it migrates the database, from the manifest's `knobas.schema`.
    pub changes_schema: Option<bool>,
    pub page_url: String,
}

/// Where the updater is.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, ts_rs::TS)]
#[serde(tag = "phase", rename_all = "snake_case")]
#[ts(export_to = "update.ts")]
pub enum Phase {
    Idle,
    Checking,
    Downloading {
        received: u64,
        total: Option<u64>,
    },
    /// Downloaded and checked.
    Ready,
    Installing {
        step: Step,
    },
    Failed {
        stage: Stage,
        message: String,
        /// How far a download got.
        received: Option<u64>,
    },
}

/// The steps of an install, each said on the page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
#[ts(export_to = "update.ts")]
pub enum Step {
    Checking,
    BackingUp,
    Replacing,
    Closing,
}

/// Where a failure happened.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
#[ts(export_to = "update.ts")]
pub enum Stage {
    Download,
    Signature,
    Backup,
    Install,
}

/// A release's notes.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, ts_rs::TS)]
#[ts(export_to = "update.ts")]
pub struct UpdateNotes {
    pub version: String,
    /// The notes' markdown, or `None` when GitHub could not be asked.
    pub markdown: Option<String>,
    pub page_url: String,
}

/// What a backup before an install came to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Backup {
    /// The archive's file name.
    Taken(String),
    /// The database is not up (the start screen's path).
    NotUp,
    /// A server knobas does not manage (`KNOBAS_DB_URL`).
    External,
    Failed(String),
}

/// How knobas leaves for an install.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Quit {
    /// Windows: exit; [`at_exit`] starts the installer after PostgreSQL stopped.
    Exit,
    /// Mac and AppImage: restart into the copy already in place.
    Restart,
}

/// The side effects an updater has, which the app makes real and the tests record.
#[async_trait::async_trait]
pub trait Host: Send + Sync + 'static {
    fn emit(&self, state: &UpdateState);
    async fn backup(&self) -> Backup;
    fn quit(&self, how: Quit);
}

/// What an updater is set up with, read once at start.
pub struct Config {
    pub current: String,
    pub current_schema: i64,
    pub standing: Standing,
    /// This copy's key in a manifest's platforms (`windows-x86_64-nsis`), `None` for a copy
    /// of no known install kind.
    pub platform: Option<String>,
    pub checks_itself: bool,
    pub demo: bool,
    pub files: Files,
    /// `<app cache>/update`: the download.
    pub cache: PathBuf,
    pub endpoints: Endpoints,
    pub keys: Keys,
    pub fetch: Arc<dyn Fetch>,
    /// The installed `.app` bundle on a Mac, the AppImage on Linux.
    pub installed: Option<PathBuf>,
    pub started_at: DateTime<Utc>,
    /// `KNOBAS_UPDATE_AUTO=install`: check, download and install with no press. The end-to-end
    /// run's switch; it still installs nothing unsigned and nothing older.
    pub auto_install: bool,
    /// How long a download may go without a byte.
    pub stall: std::time::Duration,
    /// What starts the Windows installer at the exit: `install::windows::start`, or a test's
    /// recorder.
    pub start_installer: fn(&Launch) -> std::io::Result<()>,
}

#[derive(Debug, Clone, Default)]
struct Inner {
    settings: Settings,
    stored: store::State,
    phase: Option<Phase>,
    /// The version *Later* hid the dot for, until the next start.
    later: Option<String>,
    just_updated: bool,
}

/// The updater, managed by the app.
pub struct Updater {
    config: Config,
    inner: Mutex<Inner>,
    host: Arc<dyn Host>,
    /// One check at a time.
    checking: tokio::sync::Mutex<()>,
    cancel: Mutex<Option<CancellationToken>>,
    /// The installer [`at_exit`] starts, once an install handed it over.
    launch: Mutex<Option<Launch>>,
    notes: Mutex<HashMap<String, UpdateNotes>>,
    /// Wakes the schedule when the settings change.
    wake: tokio::sync::Notify,
}

/// A command refused for the phase it came in.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[error("{0}")]
pub struct Refused(pub String);

impl Updater {
    /// The updater over `config`, its files read and the attempt of the last start settled.
    #[must_use]
    pub fn new(config: Config, host: Arc<dyn Host>) -> Arc<Self> {
        let settings = config.files.settings();
        let stored = config.files.state();
        let updater = Arc::new(Self {
            config,
            inner: Mutex::new(Inner {
                settings,
                stored,
                ..Inner::default()
            }),
            host,
            checking: tokio::sync::Mutex::new(()),
            cancel: Mutex::new(None),
            launch: Mutex::new(None),
            notes: Mutex::new(HashMap::new()),
            wake: tokio::sync::Notify::new(),
        });
        updater.settle_attempt();
        updater.settle_download();
        updater
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Whether this build trusts a key.
    fn set_up(&self) -> bool {
        !self.config.keys.is_empty()
    }

    /// The attempt the last start wrote, read against the running version (decisions D9).
    fn settle_attempt(&self) {
        let mut inner = self.lock();
        let Some(attempt) = inner.stored.attempt.take() else {
            return;
        };
        if let Some(staged) = &attempt.staged {
            let _ = std::fs::remove_dir_all(staged);
        }
        let current = &self.config.current;
        if version::parse(current) == version::parse(&attempt.to) {
            inner.stored.last = Some(LastUpdate {
                from: attempt.from,
                to: attempt.to,
                at: attempt.at,
                installed: true,
                backup: attempt.backup,
            });
            inner.just_updated = true;
            let _ = std::fs::remove_dir_all(&self.config.cache);
        } else if version::parse(current) == version::parse(&attempt.from) {
            inner.stored.last = Some(LastUpdate {
                from: attempt.from,
                to: attempt.to,
                at: attempt.at,
                installed: false,
                backup: attempt.backup,
            });
        }
        let stored = inner.stored.clone();
        drop(inner);
        self.save(&stored);
    }

    /// A download left from an earlier run: kept when it is the offer's, else removed.
    fn settle_download(&self) {
        let keep = self.offered_artifact().map(|artifact| artifact.file_name);
        let Ok(entries) = std::fs::read_dir(&self.config.cache) else {
            return;
        };
        let mut ready = false;
        for entry in entries.filter_map(Result::ok) {
            let name = entry.file_name().to_string_lossy().into_owned();
            if keep.as_deref() == Some(name.as_str()) {
                ready = true;
            } else {
                let path = entry.path();
                let _ = if path.is_dir() {
                    std::fs::remove_dir_all(path)
                } else {
                    std::fs::remove_file(path)
                };
            }
        }
        if ready {
            self.lock().phase = Some(Phase::Ready);
        }
    }

    fn save(&self, stored: &store::State) {
        if let Err(error) = self.config.files.save_state(stored) {
            tracing::warn!(%error, "the update state could not be written");
        }
    }

    /// The artifact of the version on offer, when there is one and this copy has an entry.
    fn offered_artifact(&self) -> Option<Artifact> {
        let inner = self.lock();
        let seen = inner.stored.newest.as_ref()?;
        if seen.channel != inner.settings.channel
            || !version::newer(&seen.version, &self.config.current)
        {
            return None;
        }
        seen.artifact.clone()
    }

    /// What is known, from memory.
    #[must_use]
    pub fn state(&self) -> UpdateState {
        let inner = self.lock();
        let settings = inner.settings;
        let stored = &inner.stored;
        let set_up = self.set_up();
        let checks_itself = self.config.checks_itself && set_up;
        let seen = stored
            .newest
            .as_ref()
            .filter(|seen| seen.channel == settings.channel);
        let offer = seen
            .filter(|seen| version::newer(&seen.version, &self.config.current))
            .map(|seen| Offer {
                version: seen.version.clone(),
                pre_release: seen.pre_release,
                published_at: seen.published_at,
                bytes: seen.artifact.as_ref().and_then(|artifact| artifact.bytes),
                changes_schema: seen
                    .schema
                    .map(|schema| schema > self.config.current_schema),
                page_url: seen.page_url.clone(),
            });
        let not_installed = stored
            .last
            .as_ref()
            .is_some_and(|last| !last.installed && version::newer(&last.to, &self.config.current));
        let marked = (offer
            .as_ref()
            .is_some_and(|offer| inner.later.as_deref() != Some(offer.version.as_str()))
            || not_installed && inner.later.is_none())
            && set_up;
        let check_failed = stored.outcome.as_ref().and_then(|outcome| {
            let at = stored.checked_at?;
            let (kind, message) = match outcome {
                Outcome::Ok => return None,
                Outcome::Offline { message } => (CheckFailedKind::Offline, message.clone()),
                Outcome::RateLimited { message, .. } => {
                    (CheckFailedKind::RateLimited, message.clone())
                }
                Outcome::BadAnswer { message } => (CheckFailedKind::BadAnswer, message.clone()),
            };
            Some(CheckFailed { kind, message, at })
        });
        UpdateState {
            current: self.config.current.clone(),
            current_is_pre_release: version::is_pre_release(&self.config.current),
            current_schema: self.config.current_schema,
            set_up,
            check: settings.check,
            channel: settings.channel,
            standing: self.config.standing,
            checks_itself,
            checked_at: stored.checked_at,
            next_check_at: schedule::due_at(self.schedule_input(&inner)),
            check_failed,
            newest: seen.map(|seen| Seen {
                version: seen.version.clone(),
                pre_release: seen.pre_release,
                published_at: seen.published_at,
                page_url: seen.page_url.clone(),
            }),
            offer,
            phase: inner.phase.clone().unwrap_or(Phase::Idle),
            marked,
            last: stored.last.clone(),
            just_updated: inner.just_updated,
        }
    }

    fn schedule_input(&self, inner: &Inner) -> schedule::Input {
        let (last_failed, retry_at) = match &inner.stored.outcome {
            None | Some(Outcome::Ok) => (false, None),
            Some(Outcome::RateLimited { reset, .. }) => (true, *reset),
            Some(_) => (true, None),
        };
        schedule::Input {
            started_at: self.config.started_at,
            last: inner.stored.checked_at,
            last_failed,
            retry_at,
            check: inner.settings.check,
            checks_itself: self.config.checks_itself && self.set_up(),
        }
    }

    fn emit(&self) -> UpdateState {
        let state = self.state();
        self.host.emit(&state);
        state
    }

    fn set_phase(&self, phase: Phase) {
        self.lock().phase = Some(phase);
    }

    fn phase(&self) -> Phase {
        self.lock().phase.clone().unwrap_or(Phase::Idle)
    }

    /// A check now, one at a time; a second call waits for the first and answers its outcome.
    ///
    /// # Errors
    ///
    /// [`Refused`] while not set up, downloading or installing.
    pub async fn check(&self) -> Result<UpdateState, Refused> {
        if !self.set_up() {
            return Err(Refused("updates aren't set up for this build".to_owned()));
        }
        if matches!(
            self.phase(),
            Phase::Downloading { .. } | Phase::Installing { .. }
        ) {
            return Err(Refused("a download or an install is running".to_owned()));
        }
        let Ok(_one) = self.checking.try_lock() else {
            let _wait = self.checking.lock().await;
            return Ok(self.state());
        };
        let before = self.phase();
        self.set_phase(Phase::Checking);
        self.emit();
        let channel = self.lock().settings.channel;
        let found =
            channel::discover(channel, self.config.fetch.as_ref(), &self.config.endpoints).await;
        let now = Utc::now();
        {
            let mut inner = self.lock();
            inner.stored.checked_at = Some(now);
            match found {
                Ok(found) => {
                    inner.stored.outcome = Some(Outcome::Ok);
                    match found.map(|found| self.seen_of(found, channel)) {
                        Some(Ok(seen)) => inner.stored.newest = Some(seen),
                        None => inner.stored.newest = None,
                        Some(Err(why)) => {
                            inner.stored.outcome = Some(Outcome::BadAnswer { message: why });
                        }
                    }
                }
                Err(error) => {
                    let message = error.to_string();
                    inner.stored.outcome = Some(match error {
                        CheckError::Offline(_) => Outcome::Offline { message },
                        CheckError::RateLimited { reset } => {
                            Outcome::RateLimited { message, reset }
                        }
                        CheckError::BadAnswer(_) => Outcome::BadAnswer { message },
                    });
                }
            }
            let back = match before {
                Phase::Checking => Phase::Idle,
                other => other,
            };
            inner.phase = Some(back);
        }
        self.drop_stale_download();
        let stored = self.lock().stored.clone();
        self.save(&stored);
        Ok(self.emit())
    }

    /// A found version as the state keeps it, this copy's artifact bound to it.
    fn seen_of(&self, found: channel::Found, channel: Channel) -> Result<store::Seen, String> {
        let manifest = &found.manifest;
        let version = version::parse(&manifest.version)
            .map_or_else(|| manifest.version.clone(), |v| v.to_string());
        if let Some(notes) = &found.notes {
            self.notes
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .insert(
                    version.clone(),
                    UpdateNotes {
                        version: version.clone(),
                        markdown: Some(notes.clone()),
                        page_url: found.page_url.clone(),
                    },
                );
        }
        let artifact = match &self.config.platform {
            None => None,
            Some(key) => {
                match manifest.entry_for(key) {
                    // No entry for this platform: nothing newer here (a leg that failed).
                    None if version::newer(&version, &self.config.current) => {
                        return Err(format!("knobas {version} has no {key} build"));
                    }
                    None => None,
                    Some((found_key, entry)) => {
                        let bound = manifest::bind(found_key, &manifest.version, entry)
                            .map_err(|error| error.to_string())?;
                        Some(Artifact {
                            key: found_key.to_owned(),
                            url: bound.url,
                            file_name: bound.file_name,
                            signature: bound.signature,
                            bytes: manifest.knobas.sizes.get(found_key).copied(),
                        })
                    }
                }
            }
        };
        Ok(store::Seen {
            version,
            pre_release: found.pre_release,
            published_at: found.published_at,
            page_url: found.page_url,
            schema: manifest.knobas.schema,
            artifact,
            channel,
        })
    }

    /// A download that is no longer the offer's is removed, and a ready phase goes back to idle.
    fn drop_stale_download(&self) {
        let keep = self.offered_artifact().map(|artifact| artifact.file_name);
        let Ok(entries) = std::fs::read_dir(&self.config.cache) else {
            return;
        };
        let mut stale = false;
        for entry in entries.filter_map(Result::ok) {
            let name = entry.file_name().to_string_lossy().into_owned();
            if keep.as_deref() != Some(name.as_str()) && !name.ends_with(".part") {
                let _ = std::fs::remove_file(entry.path());
                stale = true;
            }
        }
        let mut inner = self.lock();
        if stale && keep.is_none() && inner.phase == Some(Phase::Ready) {
            inner.phase = Some(Phase::Idle);
        }
        if keep.is_some_and(|name| !self.config.cache.join(name).is_file())
            && inner.phase == Some(Phase::Ready)
        {
            inner.phase = Some(Phase::Idle);
        }
    }

    /// Store the settings; a changed channel drops the other channel's answer and checks.
    ///
    /// # Errors
    ///
    /// [`Refused`] for a channel change while downloading or installing.
    pub async fn set(
        &self,
        check: Option<bool>,
        channel: Option<Channel>,
    ) -> Result<UpdateState, Refused> {
        let busy = matches!(
            self.phase(),
            Phase::Downloading { .. } | Phase::Installing { .. }
        );
        let changed = {
            let mut inner = self.lock();
            let changed = channel.is_some_and(|channel| channel != inner.settings.channel);
            if changed && busy {
                return Err(Refused("a download or an install is running".to_owned()));
            }
            if let Some(check) = check {
                inner.settings.check = check;
            }
            if let Some(channel) = channel {
                inner.settings.channel = channel;
            }
            if changed {
                inner.stored.newest = None;
                inner.stored.outcome = None;
            }
            if let Err(error) = self.config.files.save_settings(&inner.settings) {
                tracing::warn!(%error, "the update settings could not be written");
            }
            changed
        };
        self.wake.notify_one();
        if changed {
            self.drop_stale_download();
            let stored = self.lock().stored.clone();
            self.save(&stored);
            self.emit();
            if self.set_up() {
                return self.check().await;
            }
        }
        Ok(self.emit())
    }

    /// Start the download of the offer.
    ///
    /// # Errors
    ///
    /// [`Refused`] with nothing to download, a copy updated by hand, or a download running.
    pub fn download(self: &Arc<Self>) -> Result<UpdateState, Refused> {
        if !self.set_up() {
            return Err(Refused("updates aren't set up for this build".to_owned()));
        }
        if !matches!(self.config.standing, Standing::InPlace { .. }) {
            return Err(Refused("this copy is updated by hand".to_owned()));
        }
        match self.phase() {
            Phase::Idle | Phase::Failed { .. } => {}
            _ => {
                return Err(Refused(
                    "not now: a check, a download or an install is running".to_owned(),
                ));
            }
        }
        let artifact = self
            .offered_artifact()
            .ok_or_else(|| Refused("nothing newer to download".to_owned()))?;
        let token = CancellationToken::new();
        *self.cancel.lock().unwrap_or_else(PoisonError::into_inner) = Some(token.clone());
        self.set_phase(Phase::Downloading {
            received: 0,
            total: artifact.bytes,
        });
        let state = self.emit();
        let this = Arc::clone(self);
        tauri::async_runtime::spawn(async move {
            this.run_download(artifact, token).await;
        });
        Ok(state)
    }

    async fn run_download(self: Arc<Self>, artifact: Artifact, token: CancellationToken) {
        let bound = manifest::Bound {
            url: artifact.url.clone(),
            file_name: artifact.file_name.clone(),
            signature: artifact.signature.clone(),
        };
        // One artifact at most: another version's file goes first.
        let _ = std::fs::remove_dir_all(&self.config.cache);
        let progress_to = Arc::clone(&self);
        let mut progress = move |received: u64, total: Option<u64>| {
            progress_to.set_phase(Phase::Downloading { received, total });
            progress_to.emit();
        };
        let outcome = download::download(
            &bound,
            &self.config.cache,
            &self.config.keys,
            self.config.stall,
            &token,
            &mut progress,
        )
        .await;
        *self.cancel.lock().unwrap_or_else(PoisonError::into_inner) = None;
        let phase = match outcome {
            Ok(_) => Phase::Ready,
            Err(DownloadError::Cancelled) => Phase::Idle,
            Err(DownloadError::Signature(error)) => Phase::Failed {
                stage: Stage::Signature,
                message: error.to_string(),
                received: None,
            },
            Err(DownloadError::Stopped { received, why }) => Phase::Failed {
                stage: Stage::Download,
                message: why,
                received: Some(received),
            },
            Err(DownloadError::TooLarge { received }) => Phase::Failed {
                stage: Stage::Download,
                message: format!(
                    "the download is larger than {} MB",
                    download::MAX_BYTES >> 20
                ),
                received: Some(received),
            },
            Err(DownloadError::Disk(why)) => Phase::Failed {
                stage: Stage::Download,
                message: why,
                received: None,
            },
        };
        let ready = phase == Phase::Ready;
        self.set_phase(phase);
        self.emit();
        if ready && self.config.auto_install {
            let _ = self.install();
        }
    }

    /// Stop the download; the `.part` file goes with it.
    #[must_use]
    pub fn cancel(&self) -> UpdateState {
        if let Some(token) = self
            .cancel
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
        {
            token.cancel();
            self.set_phase(Phase::Idle);
        }
        self.emit()
    }

    /// Hide the dot until the next start or the next newer version. The download stays.
    #[must_use]
    pub fn later(&self) -> UpdateState {
        {
            let mut inner = self.lock();
            let offered = inner
                .stored
                .newest
                .as_ref()
                .filter(|seen| version::newer(&seen.version, &self.config.current))
                .map(|seen| seen.version.clone());
            let not_installed = inner.stored.last.as_ref().map(|last| last.to.clone());
            inner.later = offered.or(not_installed);
        }
        self.emit()
    }

    /// Start section 11's order: check the download again, back up, write the attempt, put the
    /// new copy in place (Mac, AppImage), and leave through the ordinary quit.
    ///
    /// # Errors
    ///
    /// [`Refused`] unless a checked download is ready.
    pub fn install(self: &Arc<Self>) -> Result<UpdateState, Refused> {
        let Standing::InPlace { kind } = self.config.standing else {
            return Err(Refused("this copy is updated by hand".to_owned()));
        };
        if self.phase() != Phase::Ready {
            return Err(Refused(
                "there is no checked download to install".to_owned(),
            ));
        }
        let artifact = self
            .offered_artifact()
            .ok_or_else(|| Refused("nothing newer to install".to_owned()))?;
        self.set_phase(Phase::Installing {
            step: Step::Checking,
        });
        let state = self.emit();
        let this = Arc::clone(self);
        tauri::async_runtime::spawn(async move {
            if let Err((stage, message)) = this.run_install(kind, artifact).await {
                tracing::warn!(?stage, %message, "the update did not install");
                this.set_phase(Phase::Failed {
                    stage,
                    message,
                    received: None,
                });
                this.emit();
            }
        });
        Ok(state)
    }

    async fn run_install(
        self: &Arc<Self>,
        kind: InstallKind,
        artifact: Artifact,
    ) -> Result<(), (Stage, String)> {
        // 1. The second pass, into a folder only this user can enter.
        let downloaded = self.config.cache.join(&artifact.file_name);
        let private =
            install::private_dir(None).map_err(|error| (Stage::Install, error.to_string()))?;
        let staged = private.path().join(&artifact.file_name);
        let signature = verify::decode(&artifact.signature)
            .map_err(|error| (Stage::Signature, error.to_string()))?;
        let keys = self.config.keys.clone();
        let (from, to) = (downloaded.clone(), staged.clone());
        let name = artifact.file_name.clone();
        let checked = tokio::task::spawn_blocking(move || {
            keys.verify_file(&from, &signature, &name, Some(&to))
        })
        .await
        .map_err(|error| (Stage::Install, error.to_string()))?;
        if let Err(error) = checked {
            let _ = std::fs::remove_file(&downloaded);
            return Err((Stage::Signature, error.to_string()));
        }

        // 2. The backup.
        self.set_phase(Phase::Installing {
            step: Step::BackingUp,
        });
        self.emit();
        let changes_schema = self
            .state()
            .offer
            .and_then(|offer| offer.changes_schema)
            .unwrap_or(true);
        let backup = match self.host.backup().await {
            Backup::Taken(file) => Some(file),
            Backup::NotUp | Backup::External => None,
            Backup::Failed(why) if changes_schema => {
                return Err((
                    Stage::Backup,
                    format!("the backup failed, and this update changes the database: {why}"),
                ));
            }
            Backup::Failed(why) => {
                tracing::warn!(%why, "no backup before an update that changes no schema");
                None
            }
        };

        // 3. The attempt.
        let to_version = self
            .state()
            .offer
            .map(|offer| offer.version)
            .unwrap_or_default();
        let private = private.keep();
        let attempt = Attempt {
            from: self.config.current.clone(),
            to: to_version,
            at: Utc::now(),
            backup,
            staged: Some(private.clone()),
        };
        let stored = {
            let mut inner = self.lock();
            inner.stored.attempt = Some(attempt);
            inner.stored.clone()
        };
        self.save(&stored);

        // 4. The swap, where knobas replaces itself before the restart.
        let quit = match kind {
            InstallKind::Nsis | InstallKind::Msi => {
                let launch = install::windows::launch_for(kind, &staged, self.config.demo)
                    .ok_or_else(|| (Stage::Install, "no installer for this copy".to_owned()))?;
                *self.launch.lock().unwrap_or_else(PoisonError::into_inner) = Some(launch);
                Quit::Exit
            }
            InstallKind::App | InstallKind::AppImage => {
                self.set_phase(Phase::Installing {
                    step: Step::Replacing,
                });
                self.emit();
                if let Err(why) = self.replace(kind, &staged) {
                    let stored = {
                        let mut inner = self.lock();
                        inner.stored.attempt = None;
                        inner.stored.clone()
                    };
                    self.save(&stored);
                    let _ = std::fs::remove_dir_all(&private);
                    return Err((Stage::Install, why));
                }
                Quit::Restart
            }
            InstallKind::Deb | InstallKind::Rpm => {
                return Err((Stage::Install, "a package is installed by hand".to_owned()));
            }
        };

        // 5. Closing, through the ordinary quit.
        self.set_phase(Phase::Installing {
            step: Step::Closing,
        });
        self.emit();
        self.host.quit(quit);
        Ok(())
    }

    #[cfg(unix)]
    fn replace(&self, kind: InstallKind, staged: &Path) -> Result<(), String> {
        let installed = self
            .config
            .installed
            .as_deref()
            .ok_or_else(|| "knobas does not know where it is installed".to_owned())?;
        match kind {
            InstallKind::App => install::macos::swap_bundle(staged, installed)
                .map(drop)
                .map_err(|error| error.to_string()),
            _ => install::appimage::replace(staged, installed).map_err(|error| error.to_string()),
        }
    }

    #[cfg(not(unix))]
    fn replace(&self, _kind: InstallKind, _staged: &Path) -> Result<(), String> {
        Err("this copy cannot replace itself".to_owned())
    }

    /// A release's notes: from memory, or from GitHub's release by tag.
    pub async fn notes(&self, version: &str) -> UpdateNotes {
        let version = version::parse(version).map_or_else(|| version.to_owned(), |v| v.to_string());
        if let Some(notes) = self
            .notes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&version)
        {
            return notes.clone();
        }
        let notes = match channel::release_notes(
            self.config.fetch.as_ref(),
            &self.config.endpoints,
            &version,
        )
        .await
        {
            Ok((markdown, page_url)) => UpdateNotes {
                version: version.clone(),
                markdown,
                page_url,
            },
            Err(error) => {
                tracing::info!(%error, %version, "a release's notes could not be read");
                return UpdateNotes {
                    page_url: self.config.endpoints.release_page(&version),
                    version,
                    markdown: None,
                };
            }
        };
        self.notes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(version, notes.clone());
        notes
    }

    /// The installer the install handed to the exit, once.
    fn take_launch(&self) -> Option<Launch> {
        self.launch
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
    }

    /// The schedule: a check whenever one is due, looked at every 10 minutes and whenever the
    /// settings change. With `KNOBAS_UPDATE_AUTO=install`, a found update is downloaded too.
    pub async fn run_schedule(self: Arc<Self>) {
        loop {
            let input = self.schedule_input(&self.lock());
            let now = Utc::now();
            if schedule::is_due(input, now) {
                let _ = self.check().await;
                if self.config.auto_install && self.offered_artifact().is_some() {
                    let _ = self.download();
                }
                continue;
            }
            let wait = schedule::due_at(input)
                .and_then(|due| (due - now).to_std().ok())
                .map_or(schedule::LOOK_EVERY, |until| {
                    until.min(schedule::LOOK_EVERY)
                });
            tokio::select! {
                () = tokio::time::sleep(wait) => {}
                () = self.wake.notified() => {}
            }
        }
    }
}

/// The Tauri half of [`Host`].
struct TauriHost<R: tauri::Runtime> {
    app: tauri::AppHandle<R>,
}

#[async_trait::async_trait]
impl<R: tauri::Runtime> Host for TauriHost<R> {
    fn emit(&self, state: &UpdateState) {
        crate::events::emit(&self.app, None, crate::events::UPDATE_STATE, state);
    }

    async fn backup(&self) -> Backup {
        use tauri::Manager as _;
        if std::env::var(crate::DB_URL_ENV).is_ok_and(|url| !url.trim().is_empty()) {
            return Backup::External;
        }
        let Some(state) = self.app.try_state::<Arc<crate::backup::BackupState>>() else {
            return Backup::NotUp;
        };
        let state = Arc::clone(state.inner());
        match crate::backup::export_now(&state).await {
            Ok(record) => Backup::Taken(record.file),
            Err(error) => Backup::Failed(error.to_string()),
        }
    }

    fn quit(&self, how: Quit) {
        match how {
            Quit::Exit => self.app.exit(0),
            Quit::Restart => self.app.request_restart(),
        }
    }
}

/// This copy's standing, read once: the bundle marker, the executable's path, `APPIMAGE`.
fn standing_here() -> (Standing, Option<PathBuf>) {
    use knobas_update::standing::{BundleMarker, Os, app_bundle_of, standing_of};
    let marker = tauri::utils::platform::bundle_type().map(|bundle| {
        use tauri::utils::config::BundleType;
        match bundle {
            BundleType::Deb => BundleMarker::Deb,
            BundleType::Rpm => BundleMarker::Rpm,
            BundleType::AppImage => BundleMarker::AppImage,
            BundleType::Msi => BundleMarker::Msi,
            BundleType::Nsis => BundleMarker::Nsis,
            BundleType::App | BundleType::Dmg => BundleMarker::App,
        }
    });
    let exe = std::env::current_exe().unwrap_or_default();
    let appimage = std::env::var("APPIMAGE").ok();
    let standing = standing_of(
        Os::this(),
        marker,
        &exe,
        appimage.as_deref(),
        cfg!(debug_assertions),
    );
    let installed = match standing {
        Standing::InPlace {
            kind: InstallKind::App,
        } => app_bundle_of(&exe).map(Path::to_path_buf),
        Standing::InPlace {
            kind: InstallKind::AppImage,
        } => appimage.map(PathBuf::from),
        _ => None,
    };
    (standing, installed)
}

/// Build and manage the updater, and start its schedule. Needs no database.
pub fn start<R: tauri::Runtime>(app: &tauri::AppHandle<R>, demo: bool) {
    use tauri::Manager as _;
    let (Ok(data), Ok(cache)) = (app.path().app_data_dir(), app.path().app_cache_dir()) else {
        tracing::error!("no data or cache folder: knobas does not check for updates");
        return;
    };
    let keys = Keys::compiled().unwrap_or_else(|error| {
        tracing::error!(%error, "a compiled-in update key does not decode");
        Keys::default()
    });
    let endpoints = Endpoints::from_env(
        std::env::var("KNOBAS_UPDATE_WEB").ok(),
        std::env::var("KNOBAS_UPDATE_API").ok(),
    )
    .unwrap_or_else(|error| {
        tracing::error!(%error, "an update endpoint is refused; GitHub's stand");
        Endpoints::default()
    });
    let (standing, installed) = standing_here();
    let env = std::env::var("KNOBAS_UPDATE").ok();
    let checks_itself = schedule::checks_itself(cfg!(debug_assertions), demo, env.as_deref());
    let auto_install = std::env::var("KNOBAS_UPDATE_AUTO").is_ok_and(|value| value == "install");
    tracing::info!(
        ?standing,
        installed = ?installed,
        checks_itself,
        keys = ?keys.all().iter().map(|key| format!("{} {}", key.name, key.id)).collect::<Vec<_>>(),
        "updates"
    );
    let platform = match standing {
        Standing::InPlace { kind }
        | Standing::ByHand {
            kind: Some(kind), ..
        } => Some(manifest::platform_key(kind)),
        Standing::ByHand { kind: None, .. } => None,
    };
    let config = Config {
        current: env!("CARGO_PKG_VERSION").to_owned(),
        current_schema: knobas_db::migrate::newest(),
        standing,
        platform,
        checks_itself: checks_itself || auto_install && !cfg!(debug_assertions),
        demo,
        files: Files::new(data.join("update")),
        cache: cache.join("update"),
        fetch: Arc::new(knobas_update::fetch::HttpFetch {
            endpoints: endpoints.clone(),
        }),
        endpoints,
        keys,
        installed,
        started_at: Utc::now(),
        auto_install,
        stall: knobas_http::STALL_TIMEOUT,
        start_installer: install::windows::start,
    };
    let updater = Updater::new(config, Arc::new(TauriHost { app: app.clone() }));
    if !app.manage(Arc::clone(&updater)) {
        return;
    }
    tauri::async_runtime::spawn(updater.run_schedule());
}

/// On `RunEvent::Exit`, after `shutdown_app` stopped the sync and PostgreSQL: start the installer
/// an install handed over (Windows). Nothing on any other exit.
pub fn at_exit<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    use tauri::Manager as _;
    let Some(updater) = app.try_state::<Arc<Updater>>() else {
        return;
    };
    if let Some(launch) = updater.take_launch() {
        tracing::info!(program = %launch.program.display(), "starting the update's installer");
        if let Err(error) = (updater.config.start_installer)(&launch) {
            tracing::error!(%error, "the update's installer did not start");
        }
    }
}

/// The managed updater, or the refusal for a call before `setup` managed it.
///
/// # Errors
///
/// `not_ready`.
pub fn updater<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
) -> Result<Arc<Updater>, crate::IpcError> {
    use tauri::Manager as _;
    app.try_state::<Arc<Updater>>()
        .map(|state| Arc::clone(state.inner()))
        .ok_or_else(|| crate::IpcError::not_ready("the updater is not running"))
}

#[cfg(feature = "test-util")]
impl Updater {
    /// Hand `launch` to the exit as an install would, **for tests**: the exit-order test starts
    /// it through [`at_exit`] without a download behind it.
    pub fn hand_over_for_test(&self, launch: Launch) {
        *self.launch.lock().unwrap_or_else(PoisonError::into_inner) = Some(launch);
    }
}

#[cfg(test)]
mod tests;
