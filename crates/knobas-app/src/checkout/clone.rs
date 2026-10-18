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
//! - **The URL is built, never taken from the mirror**: an address a person
//!   typed into the repository's source -- its `base_url`, or a config value,
//!   whichever its adapter answers the code-host read `clone_urls` with
//!   ([`SourceDescriptor::clone_from`](knobas_source::SourceDescriptor::clone_from),
//!   #1197) -- plus the repository's full name, checked by
//!   [`clone::FullName::parse`]. Any code-host source clones; Gitea answers
//!   its `base_url`. The repository's mirrored `web_url` never reaches git.
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
use knobas_sync::scheduler::AdapterRegistry;

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
    /// git could not sign in with the person's own credentials.
    Auth,
    /// The disk is full.
    DiskFull,
    /// The network, the server, or anything else; the reason says which.
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
    /// One clone's progress, or how it ended.
    fn clone_progress(&self, event: CloneEvent);
}

/// The clones running in this process, by repo entity id.
#[derive(Default)]
pub struct Clones {
    running: Mutex<HashMap<String, Running>>,
}

struct Running {
    /// Which start this entry belongs to, so a start that gives up removes its
    /// own entry and never a later one's.
    run: u64,
    path: PathBuf,
    /// Sent a reply channel to stop the clone. The runner answers whether the
    /// folder is gone, or `None` when git had already finished. Taken by the
    /// first [`stop`]; the entry itself stays until the folder is removed, so
    /// no start can make the folder again while a removal is still running.
    stop: Option<oneshot::Sender<Reply>>,
}

/// The runner's answer to a stop: whether the folder is gone, or `None` when
/// the clone had already finished and was kept.
type Reply = oneshot::Sender<Option<bool>>;

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

    /// Remove `repo_id`'s entry if it is still run `run`'s.
    fn release(&self, repo_id: &str, run: u64) {
        let mut running = self.lock();
        if running.get(repo_id).is_some_and(|entry| entry.run == run) {
            running.remove(repo_id);
        }
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
/// repo or branch, a repository whose source's adapter is not installed or
/// answers no address to clone from, a name or address ADR-0023 refuses, no
/// clones root (or a relative one), or no git;
/// [`NotFound`](crate::IpcErrorCode::NotFound) if the mirror does not hold it
/// or its source is gone;
/// [`Conflict`](crate::IpcErrorCode::Conflict) if the folder exists and is not
/// a clone of this repository, or another repository of the same name is
/// cloning into it;
/// [`Internal`](crate::IpcErrorCode::Internal) for a query or filesystem
/// failure.
pub async fn start(
    pool: &PgPool,
    registry: &dyn AdapterRegistry,
    clones: &Arc<Clones>,
    events: Arc<dyn CloneEvents>,
    entity_id: &str,
) -> Result<CloneAnswer, IpcError> {
    start_with(pool, registry, clones, events, entity_id, &clone::PROTOCOLS).await
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
    registry: &dyn AdapterRegistry,
    clones: &Arc<Clones>,
    events: Arc<dyn CloneEvents>,
    entity_id: &str,
    protocols: &[&str],
) -> Result<CloneAnswer, IpcError> {
    start_with(pool, registry, clones, events, entity_id, protocols).await
}

/// Stop the running clone of `entity_id`'s repository and remove its folder.
///
/// Answers once the folder is gone (or could not be removed, which `removed`
/// says), and [`CloneAnswer::NotCloning`] when nothing was running or git
/// finished before the stop reached it.
///
/// # Errors
///
/// [`Invalid`](crate::IpcErrorCode::Invalid) for an address that is not a
/// repo or branch; [`NotFound`](crate::IpcErrorCode::NotFound) if the mirror
/// holds neither it nor its repository.
pub async fn stop(
    pool: &PgPool,
    clones: &Arc<Clones>,
    entity_id: &str,
) -> Result<CloneAnswer, IpcError> {
    let repo_id = repo_id(pool, entity_id).await?;
    let (path, sender) = {
        let mut running = clones.lock();
        // No entry, or one a stop already took: nothing more to stop.
        let Some(entry) = running.get_mut(&repo_id) else {
            return Ok(CloneAnswer::NotCloning);
        };
        let Some(sender) = entry.stop.take() else {
            return Ok(CloneAnswer::NotCloning);
        };
        (entry.path.clone(), sender)
    };
    let (reply, removed) = oneshot::channel();
    if sender.send(reply).is_err() {
        // The runner ended between the lookup and the send: the clone is
        // finished or failed, and its own event says which.
        return Ok(CloneAnswer::NotCloning);
    }
    let removed = match removed.await {
        Ok(Some(removed)) => removed,
        Ok(None) => return Ok(CloneAnswer::NotCloning),
        // The start gave up before its runner existed.
        Err(_) => !path.exists(),
    };
    Ok(CloneAnswer::Stopped {
        path: path.to_string_lossy().into_owned(),
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
    registry: &dyn AdapterRegistry,
    clones: &Arc<Clones>,
    events: Arc<dyn CloneEvents>,
    entity_id: &str,
    protocols: &[&str],
) -> Result<CloneAnswer, IpcError> {
    let repo_id = repo_id(pool, entity_id).await?;

    let source: Option<(String, String, serde_json::Value)> = sqlx::query_as(
        "select s.kind, s.base_url, s.config
           from sync.item i join knobas.source_config s on s.id = i.source_id
          where i.entity_id = $1",
    )
    .bind(&repo_id)
    .fetch_optional(pool)
    .await
    .map_err(IpcError::internal)?;
    let (kind, base_url, config) = source.ok_or_else(|| {
        IpcError::not_found(format!(
            "{repo_id}'s source is no longer configured, so there is no address to clone from"
        ))
    })?;

    let key = EntityRef::parse(&repo_id).map_err(IpcError::invalid)?.key;
    let name = clone::FullName::parse(&key).map_err(IpcError::invalid)?;
    let url = resolve_clone_url(
        registry, &repo_id, &kind, &base_url, &config, &name, protocols,
    )?;

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

    // Reserved before anything is looked at on disk, under one lock, so two
    // starts at once cannot both reach the folder. The entry is held until the
    // folder is removed, and a folder another repository's run holds (`a/x`
    // and `b/x` share `<root>/x`) is refused, so a removal still retrying
    // never removes a folder a later start made.
    static RUNS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let run_id = RUNS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let (stop_tx, stop_rx) = oneshot::channel();
    {
        let mut running = clones.lock();
        if running.contains_key(&repo_id) {
            return Ok(CloneAnswer::Cloning { path: shown });
        }
        if running.values().any(|other| other.path == target) {
            return Err(IpcError::conflict(format!(
                "{shown} is being cloned from another repository of the same name"
            )));
        }
        running.insert(
            repo_id.clone(),
            Running {
                run: run_id,
                path: target.clone(),
                stop: Some(stop_tx),
            },
        );
    }

    match spawn_into(&root, &target, &url, protocols, &shown).await {
        Ok(Ok(child)) => {
            tokio::spawn(run(
                Arc::clone(clones),
                events,
                repo_id,
                run_id,
                target,
                child,
                stop_rx,
            ));
            Ok(CloneAnswer::Cloning { path: shown })
        }
        Ok(Err(answer)) => {
            clones.release(&repo_id, run_id);
            Ok(answer)
        }
        Err(error) => {
            clones.release(&repo_id, run_id);
            Err(error)
        }
    }
}

/// The URL git clones `name` from: the code-host read `clone_urls` (#1197).
///
/// The adapter's template descriptor names which of the instance's typed
/// addresses a repository is cloned from, so this builds no adapter and asks
/// the keychain nothing: a clone needs no credential from knobas (ADR-0023).
/// The first address [`clone::clone_url`] admits wins; when none does, the
/// first address's refusal is the answer.
fn resolve_clone_url(
    registry: &dyn AdapterRegistry,
    repo_id: &str,
    kind: &str,
    base_url: &str,
    config: &serde_json::Value,
    name: &clone::FullName,
    protocols: &[&str],
) -> Result<String, IpcError> {
    let template = registry
        .descriptors()
        .into_iter()
        .find(|d| d.adapter_kind == kind)
        .ok_or_else(|| {
            IpcError::invalid(format!(
                "{repo_id} comes from a {kind} source, and no {kind} adapter is installed to say \
                 where it is cloned from"
            ))
        })?;
    let addresses = knobas_source::clone_urls::addresses(&template.clone_from, base_url, config);
    let mut refusal = None;
    for address in &addresses {
        match clone::clone_url(address, name, protocols) {
            Ok(url) => return Ok(url),
            Err(error) => {
                refusal.get_or_insert(error);
            }
        }
    }
    if let Some(error) = refusal {
        return Err(IpcError::invalid(error));
    }
    // The adapter names config keys the instance leaves empty: say which, so
    // the person knows what to fill in. The base URL is no fallback (#1197).
    let unset: Vec<&str> = template
        .clone_from
        .iter()
        .filter_map(|from| match from {
            knobas_source::clone_urls::CloneFrom::Config(key) => Some(key.as_str()),
            knobas_source::clone_urls::CloneFrom::BaseUrl => None,
        })
        .collect();
    Err(IpcError::invalid(if unset.is_empty() {
        format!("{repo_id} comes from a {kind} source, which names no address to clone from")
    } else {
        format!(
            "{repo_id} comes from a {kind} source with no {} set, so there is no address to \
             clone from: fill it in with Edit source",
            unset.join(" or ")
        )
    }))
}

/// Adopt the folder at `target`, or make it and start git cloning into it.
async fn spawn_into(
    root: &Path,
    target: &Path,
    url: &str,
    protocols: &[&str],
    shown: &str,
) -> Result<Result<tokio::process::Child, CloneAnswer>, IpcError> {
    if std::fs::symlink_metadata(target).is_ok() {
        return adopt_or_refuse(target, url, shown).map(Err);
    }

    let git = working_tree::git_on_path()
        .await
        .map_err(|read| match read {
            working_tree::TreeRead::Failed { reason } => {
                IpcError::invalid(format!("git could not be checked: {reason}"))
            }
            _ => IpcError::invalid("git is not installed, so knobas cannot clone"),
        })?;

    std::fs::create_dir_all(root).map_err(|error| {
        IpcError::internal(format!(
            "could not make the clones root {}: {error}",
            root.display()
        ))
    })?;
    if let Err(error) = std::fs::create_dir(target) {
        if error.kind() == std::io::ErrorKind::AlreadyExists {
            return adopt_or_refuse(target, url, shown).map(Err);
        }
        return Err(IpcError::internal(format!(
            "could not make {shown}: {error}"
        )));
    }

    let mut command = tokio::process::Command::new(git);
    command
        .args(clone::clone_args(url, target, protocols))
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
    match command.spawn() {
        Ok(child) => Ok(Ok(child)),
        Err(error) => {
            let _ = std::fs::remove_dir_all(target);
            Err(IpcError::invalid(format!("git could not be run: {error}")))
        }
    }
}

/// A folder is already at the target: adopted if it is a clone of the
/// repository at `url`, refused otherwise.
fn adopt_or_refuse(target: &Path, url: &str, shown: &str) -> Result<CloneAnswer, IpcError> {
    let wanted = core_checkout::remote_key(url);
    let found = core_checkout::clone_at(target).map(|clone| clone.key);
    if wanted.is_some() && found == wanted {
        if clone::is_finished(target) {
            return Ok(CloneAnswer::Adopted {
                path: shown.to_owned(),
            });
        }
        return Err(IpcError::conflict(format!(
            "{shown} is a clone of this repository that did not finish. Remove it and clone again"
        )));
    }
    Err(IpcError::conflict(format!(
        "{shown} exists and is not a clone of this repository. Rename it, or set a path by hand"
    )))
}

/// How much of git's stderr is kept for the failure's cause.
const KEPT: usize = 8 * 1024;

/// Read git's progress until it ends or is stopped, then say how it ended.
async fn run(
    clones: Arc<Clones>,
    events: Arc<dyn CloneEvents>,
    repo_id: String,
    run_id: u64,
    target: PathBuf,
    mut child: tokio::process::Child,
    mut stop: oneshot::Receiver<Reply>,
) {
    let path = target.to_string_lossy().into_owned();
    let send = |state: CloneState| {
        events.clone_progress(CloneEvent {
            entity_id: repo_id.clone(),
            path: path.clone(),
            state,
        });
    };
    // A stop's sender dropped without a send (the entry was replaced) never
    // stops anything.
    let asked = async {
        match (&mut stop).await {
            Ok(reply) => reply,
            Err(_) => std::future::pending().await,
        }
    };

    let exited = tokio::select! {
        reply = asked => {
            let _ = child.kill().await;
            let removed = remove(&target).await;
            clones.release(&repo_id, run_id);
            send(CloneState::Stopped { removed });
            let _ = reply.send(Some(removed));
            return;
        }
        exited = transfer(&mut child, &send) => exited,
    };
    let (success, kept) = exited;

    // A stop that took the stop sender as git exited is answered here: a
    // finished clone is kept and the stop told so, a failed one is removed
    // either way.
    stop.close();
    let late = stop.try_recv().ok();
    if success {
        clones.release(&repo_id, run_id);
        send(CloneState::Done);
        if let Some(reply) = late {
            let _ = reply.send(None);
        }
        return;
    }
    let removed = remove(&target).await;
    clones.release(&repo_id, run_id);
    let reason = clone::failure_reason(&kept)
        .unwrap_or("git stopped without saying why")
        .to_owned();
    send(CloneState::Failed {
        reason,
        cause: clone::failure_cause(&kept).into(),
        removed,
    });
    if let Some(reply) = late {
        let _ = reply.send(Some(removed));
    }
}

/// Read git's stderr to its end, sending progress, and wait for git: whether
/// it succeeded, and the lines that were not progress.
async fn transfer(child: &mut tokio::process::Child, send: &impl Fn(CloneState)) -> (bool, String) {
    let mut kept = String::new();
    let mut line = Vec::new();
    let mut last_phase = String::new();
    let mut last_sent: Option<Instant> = None;
    let keep = |text: &str, kept: &mut String| {
        kept.push_str(text);
        kept.push('\n');
        if kept.len() > KEPT {
            let cut = kept.len() - KEPT;
            let cut = (cut..kept.len())
                .find(|&i| kept.is_char_boundary(i))
                .unwrap_or(0);
            kept.drain(..cut);
        }
    };
    if let Some(mut stderr) = child.stderr.take() {
        let mut buf = [0_u8; 4096];
        loop {
            let n = stderr.read(&mut buf).await.unwrap_or(0);
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
                let Some(progress) = clone::parse_progress(&text) else {
                    keep(&text, &mut kept);
                    continue;
                };
                let due = last_sent.is_none_or(|at| at.elapsed() >= PROGRESS_EVERY);
                if progress.phase != last_phase || due {
                    last_phase.clone_from(&progress.phase);
                    last_sent = Some(Instant::now());
                    send(CloneState::Progress {
                        phase: progress.phase,
                        percent: progress.percent,
                    });
                }
            }
        }
    }
    if !line.is_empty() {
        keep(String::from_utf8_lossy(&line).trim(), &mut kept);
    }
    let success = child.wait().await.is_ok_and(|status| status.success());
    (success, kept)
}

/// Remove the folder this run made; whether it is gone.
///
/// Tried until the folder stays gone for a moment, up to five seconds: killing
/// git does not kill the helpers it started (`git-remote-https`,
/// `index-pack`), and one still writing a pack can put a file back after the
/// first removal. They exit once their pipes to git close.
async fn remove(target: &Path) -> bool {
    for _ in 0..50 {
        let _ = tokio::fs::remove_dir_all(target).await;
        tokio::time::sleep(Duration::from_millis(100)).await;
        if tokio::fs::symlink_metadata(target).await.is_err() {
            return true;
        }
    }
    false
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
