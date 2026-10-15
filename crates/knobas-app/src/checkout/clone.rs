//! *Clone*: the one writing git command knobas runs (#823, ADR-0023).
//!
//! `git clone` into a new folder, `<clones root>/<repository name>`, in the
//! background, with progress on [`crate::events::CLONE_PROGRESS`] and a way to
//! stop it that removes the half-made folder. The checks, the URL, the argument
//! list and the reading of git's output are
//! [`knobas_core::checkout::clone`]'s; this is the database, the folder and the
//! process.
//!
//! # ADR-0023, rule by rule
//!
//! - **No shell.** git runs as an argument list built by
//!   [`clone::clone_args`], with `--` before the URL and the folder.
//! - **The URL is built, never taken from the mirror**: the Gitea source's
//!   `base_url` from `knobas.source_config`, which a person typed, plus the
//!   repository's full name, checked by [`clone::FullName::parse`]. The
//!   repository's mirrored `web_url` is never read here.
//! - **Only https and ssh**: the address's scheme, and git's `protocol.*`
//!   flags. Inherited `GIT_ALLOW_PROTOCOL` and `GIT_CONFIG_*` would override
//!   those flags, so they are removed from git's environment.
//! - **Always a new folder.** knobas makes `<clones root>/<name>` itself with
//!   `create_dir`, which fails if anything is there, and git clones into it.
//!   So the folder a failure or a stop removes is always one this run made. A
//!   folder already there is adopted if it is a clone of this repository and
//!   refused otherwise.
//! - **No credentials from knobas.** git uses the person's ssh agent or
//!   credential helper. `GIT_TERMINAL_PROMPT=0` makes a missing credential a
//!   failure git reports instead of a prompt nobody can answer.
//! - **Cancellable**, by [`stop`].

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, LazyLock, Mutex, PoisonError};
use std::time::{Duration, Instant};

use serde::Serialize;
use sqlx::PgPool;
use tokio::io::AsyncReadExt;
use tokio::sync::oneshot;

use knobas_core::checkout::{self as core_checkout, clone};
use knobas_core::entity::EntityRef;

use crate::IpcError;

use super::working_tree;

/// At most one progress event per this long, per clone, besides one on each
/// change of step. `activity:new`'s rate, and for its reason: events carry
/// coarse state, never a stream.
pub const PROGRESS_EVERY: Duration = Duration::from_secs(1);

/// What one clone reports while it runs and when it ends.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CloneEvent {
    /// The repo entity being cloned (a branch's repository when a branch was
    /// asked about).
    pub entity_id: String,
    /// The folder it is cloning into.
    pub path: String,
    #[serde(flatten)]
    pub state: CloneState,
}

/// Where a clone is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum CloneState {
    /// Running: git's step, and the whole clone from 0 to 100.
    Progress { phase: String, percent: u8 },
    /// Finished; the scan now finds the folder.
    Done,
    /// git failed. `reason` is git's own line (`clone::failure_reason`);
    /// `removed` says whether the half-made folder is gone.
    Failed {
        reason: String,
        cause: FailureCause,
        removed: bool,
    },
    /// Stopped by [`stop`]. `removed` says whether the folder is gone.
    Stopped { removed: bool },
}

/// [`clone::FailureCause`], on the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureCause {
    Auth,
    DiskFull,
    Other,
}

impl From<clone::FailureCause> for FailureCause {
    fn from(cause: clone::FailureCause) -> Self {
        match cause {
            clone::FailureCause::Auth => Self::Auth,
            clone::FailureCause::DiskFull => Self::DiskFull,
            clone::FailureCause::Other => Self::Other,
        }
    }
}

/// What `clone_repo` answers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum CloneAnswer {
    /// The clone started, or was already running; events follow.
    Cloning { path: String },
    /// The folder was already a clone of this repository; nothing ran.
    Adopted { path: String },
    /// A running clone was stopped. `removed` says whether the half-made
    /// folder is gone.
    Stopped { path: String, removed: bool },
    /// A stop was asked for and no clone of this repository was running.
    NotCloning,
}

/// Where [`CloneEvent`]s go: the Tauri bridge in the app, a list in a test.
pub trait CloneEvents: Send + Sync {
    fn clone_progress(&self, event: CloneEvent);
}

/// The clones running in this process, by repo entity id.
#[derive(Default)]
pub struct Clones {
    running: Mutex<HashMap<String, Running>>,
}

struct Running {
    path: PathBuf,
    /// Sent a reply channel to stop the clone; the runner answers on it once
    /// the folder is removed.
    stop: oneshot::Sender<oneshot::Sender<bool>>,
}

impl Clones {
    /// The one registry the app's command uses.
    #[must_use]
    pub fn global() -> Arc<Clones> {
        static GLOBAL: LazyLock<Arc<Clones>> = LazyLock::new(Arc::default);
        Arc::clone(&GLOBAL)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, Running>> {
        self.running.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Start cloning `entity_id`'s repository, or answer why not.
///
/// A repo or a branch (its repository is cloned). A second start while one is
/// running answers [`CloneAnswer::Cloning`] again and starts nothing.
///
/// # Errors
///
/// [`Invalid`](crate::IpcErrorCode::Invalid) for an address that is not a
/// repo or branch, a repository not from a Gitea source, a name or address
/// ADR-0023 refuses, no clones root (or a relative one), or no git;
/// [`NotFound`](crate::IpcErrorCode::NotFound) if the mirror does not hold it
/// or its source is gone;
/// [`Conflict`](crate::IpcErrorCode::Conflict) if the folder exists and is not
/// a clone of this repository;
/// [`Internal`](crate::IpcErrorCode::Internal) for a query or filesystem
/// failure.
pub async fn start(
    pool: &PgPool,
    clones: &Arc<Clones>,
    events: Arc<dyn CloneEvents>,
    entity_id: &str,
) -> Result<CloneAnswer, IpcError> {
    start_with(pool, clones, events, entity_id, &clone::PROTOCOLS).await
}

/// [`start`], admitting `protocols` instead of https and ssh.
///
/// **Tests only**, behind `test-util` for `AppState::over_pool`'s reason: the
/// testenv Gitea speaks plain http, and a test that clones a local folder
/// needs `file`. The production list is pinned by `knobas_core`'s
/// `the_argument_list_is_the_adrs`, and `tests/it/clone_ipc.rs` shows git
/// itself refusing what [`start`] passes it.
#[cfg(feature = "test-util")]
pub async fn start_admitting(
    pool: &PgPool,
    clones: &Arc<Clones>,
    events: Arc<dyn CloneEvents>,
    entity_id: &str,
    protocols: &[&str],
) -> Result<CloneAnswer, IpcError> {
    start_with(pool, clones, events, entity_id, protocols).await
}

/// Stop the running clone of `entity_id`'s repository and remove its folder.
///
/// Answers once the folder is gone (or could not be removed, which `removed`
/// says).
///
/// # Errors
///
/// What [`super::view`] refuses an address with.
pub async fn stop(
    pool: &PgPool,
    clones: &Arc<Clones>,
    entity_id: &str,
) -> Result<CloneAnswer, IpcError> {
    let repo_id = repo_id(pool, entity_id).await?;
    let Some(running) = clones.lock().remove(&repo_id) else {
        return Ok(CloneAnswer::NotCloning);
    };
    let (reply, removed) = oneshot::channel();
    if running.stop.send(reply).is_err() {
        // The runner ended between the lookup and the send: the clone is
        // finished or failed, and its own event says which.
        return Ok(CloneAnswer::NotCloning);
    }
    let removed = removed.await.unwrap_or_else(|_| !running.path.exists());
    Ok(CloneAnswer::Stopped {
        path: running.path.to_string_lossy().into_owned(),
        removed,
    })
}

async fn repo_id(pool: &PgPool, entity_id: &str) -> Result<String, IpcError> {
    super::repo_of(pool, entity_id)
        .await?
        .entity_id
        .ok_or_else(|| {
            IpcError::not_found(format!(
                "{entity_id} is a branch whose repository is not in the mirror, so there is \
             nothing to clone"
            ))
        })
}

async fn start_with(
    pool: &PgPool,
    clones: &Arc<Clones>,
    events: Arc<dyn CloneEvents>,
    entity_id: &str,
    protocols: &[&str],
) -> Result<CloneAnswer, IpcError> {
    let repo_id = repo_id(pool, entity_id).await?;

    let source: Option<(String, String)> = sqlx::query_as(
        "select s.kind, s.base_url
           from sync.item i join knobas.source_config s on s.id = i.source_id
          where i.entity_id = $1",
    )
    .bind(&repo_id)
    .fetch_optional(pool)
    .await
    .map_err(IpcError::internal)?;
    let (kind, base_url) = source.ok_or_else(|| {
        IpcError::not_found(format!(
            "{repo_id}'s source is no longer configured, so there is no address to clone from"
        ))
    })?;
    if kind != "gitea" {
        return Err(IpcError::invalid(format!(
            "{repo_id} comes from a {kind} source, and knobas clones from Gitea sources only"
        )));
    }

    let key = EntityRef::parse(&repo_id).map_err(IpcError::invalid)?.key;
    let name = clone::FullName::parse(&key).map_err(IpcError::invalid)?;
    let url = clone::clone_url(&base_url, &name, protocols).map_err(IpcError::invalid)?;

    let root = super::clones_root(pool).await?.ok_or_else(|| {
        IpcError::invalid("no clones root is set: choose where clones go in Settings › Checkouts")
    })?;
    let root = PathBuf::from(root);
    if !root.is_absolute() {
        return Err(IpcError::invalid(format!(
            "the clones root '{}' is not an absolute path",
            root.display()
        )));
    }
    let target = root.join(name.repo());
    let shown = target.to_string_lossy().into_owned();

    if clones.lock().contains_key(&repo_id) {
        return Ok(CloneAnswer::Cloning { path: shown });
    }

    if std::fs::symlink_metadata(&target).is_ok() {
        return adopt_or_refuse(&target, &url, shown);
    }

    let git = find_git().await?;

    std::fs::create_dir_all(&root).map_err(|error| {
        IpcError::internal(format!(
            "could not make the clones root {}: {error}",
            root.display()
        ))
    })?;
    if let Err(error) = std::fs::create_dir(&target) {
        if error.kind() == std::io::ErrorKind::AlreadyExists {
            return adopt_or_refuse(&target, &url, shown);
        }
        return Err(IpcError::internal(format!(
            "could not make {shown}: {error}"
        )));
    }

    let mut command = tokio::process::Command::new(git);
    command
        .args(clone::clone_args(&url, &target, protocols))
        .env("GIT_TERMINAL_PROMPT", "0")
        // git's own words, untranslated: `failure_cause` matches English.
        .env("LC_ALL", "C")
        // Each of these would override the `-c protocol.*` flags or point git
        // at a repository other than the new folder.
        .env_remove("GIT_ALLOW_PROTOCOL")
        .env_remove("GIT_CONFIG_PARAMETERS")
        .env_remove("GIT_CONFIG_COUNT")
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .env_remove("GIT_COMMON_DIR")
        .env_remove("GIT_OBJECT_DIRECTORY")
        .env_remove("GIT_ALTERNATE_OBJECT_DIRECTORIES")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            let _ = std::fs::remove_dir_all(&target);
            return Err(IpcError::invalid(format!("git could not be run: {error}")));
        }
    };

    let (stop_tx, stop_rx) = oneshot::channel();
    clones.lock().insert(
        repo_id.clone(),
        Running {
            path: target.clone(),
            stop: stop_tx,
        },
    );
    tokio::spawn(run(
        Arc::clone(clones),
        events,
        repo_id,
        target,
        child,
        stop_rx,
    ));
    Ok(CloneAnswer::Cloning { path: shown })
}

/// A folder is already at the target: adopted if it is a clone of the
/// repository at `url`, refused otherwise.
fn adopt_or_refuse(target: &Path, url: &str, shown: String) -> Result<CloneAnswer, IpcError> {
    let wanted = core_checkout::remote_key(url);
    let found = core_checkout::clone_at(target).map(|clone| clone.key);
    if wanted.is_some() && found == wanted {
        return Ok(CloneAnswer::Adopted { path: shown });
    }
    Err(IpcError::conflict(format!(
        "{shown} exists and is not a clone of this repository. Rename it, or set a path by hand"
    )))
}

/// The `git` on `PATH`, and not Apple's install-dialog shim (#715).
async fn find_git() -> Result<PathBuf, IpcError> {
    let search = std::env::var_os("PATH").unwrap_or_default();
    let no_git = || IpcError::invalid("git is not installed, so knobas cannot clone");
    let git = working_tree::find_on(&search, "git").ok_or_else(no_git)?;
    if git == Path::new(working_tree::APPLE_SHIM)
        && !tokio::time::timeout(
            working_tree::TIMEOUT,
            working_tree::command_line_tools_installed(),
        )
        .await
        .unwrap_or(false)
    {
        return Err(no_git());
    }
    Ok(git)
}

/// How much of git's stderr is kept for the failure's cause.
const KEPT: usize = 8 * 1024;

/// Read git's progress until it ends or is stopped, then say how it ended.
async fn run(
    clones: Arc<Clones>,
    events: Arc<dyn CloneEvents>,
    repo_id: String,
    target: PathBuf,
    mut child: tokio::process::Child,
    mut stop: oneshot::Receiver<oneshot::Sender<bool>>,
) {
    let path = target.to_string_lossy().into_owned();
    let send = |state: CloneState| {
        events.clone_progress(CloneEvent {
            entity_id: repo_id.clone(),
            path: path.clone(),
            state,
        });
    };

    let mut stderr = child.stderr.take();
    let mut buf = [0_u8; 4096];
    let mut line = Vec::new();
    let mut kept = String::new();
    let mut last_phase = String::new();
    let mut last_sent: Option<Instant> = None;
    let mut stop_open = true;

    loop {
        let read = async {
            match stderr.as_mut() {
                Some(stderr) => stderr.read(&mut buf).await.unwrap_or(0),
                None => 0,
            }
        };
        tokio::select! {
            asked = &mut stop, if stop_open => {
                match asked {
                    Ok(reply) => {
                        let _ = child.kill().await;
                        stopped(&send, &target, reply).await;
                        return;
                    }
                    Err(_) => stop_open = false,
                }
            }
            n = read => {
                if n == 0 {
                    break;
                }
                for &byte in &buf[..n] {
                    if byte != b'\r' && byte != b'\n' {
                        line.push(byte);
                        continue;
                    }
                    let text = String::from_utf8_lossy(&line).trim().to_owned();
                    line.clear();
                    if text.is_empty() {
                        continue;
                    }
                    if let Some(progress) = clone::parse_progress(&text) {
                        let due = last_sent.is_none_or(|at| at.elapsed() >= PROGRESS_EVERY);
                        if progress.phase != last_phase || due {
                            last_phase.clone_from(&progress.phase);
                            last_sent = Some(Instant::now());
                            send(CloneState::Progress {
                                phase: progress.phase,
                                percent: progress.percent,
                            });
                        }
                    } else {
                        kept.push_str(&text);
                        kept.push('\n');
                        if kept.len() > KEPT {
                            let cut = kept.len() - KEPT;
                            let cut = (cut..kept.len()).find(|&i| kept.is_char_boundary(i)).unwrap_or(0);
                            kept.drain(..cut);
                        }
                    }
                }
            }
        }
    }
    if !line.is_empty() {
        kept.push_str(String::from_utf8_lossy(&line).trim());
    }

    // stderr is closed; a stop can still arrive while git exits.
    let status = loop {
        tokio::select! {
            asked = &mut stop, if stop_open => match asked {
                Ok(reply) => {
                    let _ = child.kill().await;
                    stopped(&send, &target, reply).await;
                    return;
                }
                Err(_) => stop_open = false,
            },
            status = child.wait() => break status,
        }
    };
    // A stop that took the registry entry while git was exiting is honoured:
    // the person asked for the folder to go.
    stop.close();
    if stop_open && let Ok(reply) = stop.try_recv() {
        stopped(&send, &target, reply).await;
        return;
    }
    clones.lock().remove(&repo_id);
    if status.is_ok_and(|status| status.success()) {
        send(CloneState::Done);
        return;
    }
    let removed = remove(&target).await;
    let reason = clone::failure_reason(&kept)
        .unwrap_or("git stopped without saying why")
        .to_owned();
    send(CloneState::Failed {
        reason,
        cause: clone::failure_cause(&kept).into(),
        removed,
    });
}

/// Remove the folder, say so, and answer the stop.
async fn stopped(send: &impl Fn(CloneState), target: &Path, reply: oneshot::Sender<bool>) {
    let removed = remove(target).await;
    send(CloneState::Stopped { removed });
    let _ = reply.send(removed);
}

/// Remove the folder this run made; whether it is gone.
async fn remove(target: &Path) -> bool {
    let _ = tokio::fs::remove_dir_all(target).await;
    tokio::fs::symlink_metadata(target).await.is_err()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shapes cross the bridge with exactly the keys
    /// `app/src/lib/ipc/entity.ts` declares, and the tags it narrows on.
    #[test]
    fn the_clone_shapes_serialise_as_the_mirror_declares() {
        let mirror = include_str!("../../../../app/src/lib/ipc/entity.ts");
        let event = serde_json::to_value(CloneEvent {
            entity_id: "gitea:a/b".to_owned(),
            path: "/x/b".to_owned(),
            state: CloneState::Failed {
                reason: String::new(),
                cause: FailureCause::Auth,
                removed: true,
            },
        })
        .unwrap();
        let mut keys: Vec<&String> = event.as_object().unwrap().keys().collect();
        keys.sort();
        assert_eq!(
            keys,
            ["cause", "entity_id", "path", "reason", "removed", "state"]
        );
        for key in keys {
            assert!(
                mirror.contains(&format!("{key}:")),
                "{key} is not in entity.ts"
            );
        }
        for (state, word) in [
            (
                CloneState::Progress {
                    phase: String::new(),
                    percent: 0,
                },
                "progress",
            ),
            (CloneState::Done, "done"),
            (CloneState::Stopped { removed: true }, "stopped"),
        ] {
            assert_eq!(serde_json::to_value(state).unwrap()["state"], word);
            assert!(mirror.contains(&format!("state: \"{word}\"")), "{word}");
        }
        assert!(mirror.contains("state: \"failed\""));
        for (cause, word) in [
            (FailureCause::Auth, "auth"),
            (FailureCause::DiskFull, "disk_full"),
            (FailureCause::Other, "other"),
        ] {
            assert_eq!(serde_json::to_value(cause).unwrap(), word);
            assert!(mirror.contains(&format!("\"{word}\"")), "{word}");
        }
        for (answer, word) in [
            (
                CloneAnswer::Cloning {
                    path: String::new(),
                },
                "cloning",
            ),
            (
                CloneAnswer::Adopted {
                    path: String::new(),
                },
                "adopted",
            ),
            (
                CloneAnswer::Stopped {
                    path: String::new(),
                    removed: true,
                },
                "stopped",
            ),
            (CloneAnswer::NotCloning, "not_cloning"),
        ] {
            assert_eq!(serde_json::to_value(answer).unwrap()["status"], word);
            assert!(mirror.contains(&format!("status: \"{word}\"")), "{word}");
        }
    }
}
