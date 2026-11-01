//! A pull request opened in a worktree under the clones root (#2285,
//! ADR-0035), and the *Diff tool* target (#2275).
//!
//! The argument lists are [`knobas_core::checkout::worktree`]'s; this is the
//! database, the folders and the processes.
//!
//! # A pull request's worktree
//!
//! "Open in" on a pull request runs the application at
//! `<clones root>/pull-requests/<owner>/<repo>-<n>`, a linked worktree of the
//! person's checkout, detached at the pull request's head:
//!
//! 1. The first open adds it with no files (`worktree add --detach
//!    --no-checkout`, the one command run at the person's checkout).
//! 2. Every open fetches `refs/pull/<n>/head` into the worktree's own
//!    `FETCH_HEAD`, from the URL *Clone* would clone from (ADR-0023), and
//!    moves the worktree there (`checkout --detach FETCH_HEAD`). A change in
//!    the worktree that the new head would overwrite makes git refuse, and
//!    the open is refused with git's words: nothing a person wrote is lost.
//! 3. A first open whose fetch fails takes back the empty worktree it made.
//!
//! The person's checkout keeps its branch, its `HEAD`, its index and its
//! files. What it gains is the fetched objects and git's record of the
//! worktree under `.git/worktrees/`.
//!
//! The worktrees knobas made are one `knobas.setting` row, [`WORKTREES_KEY`]:
//! which repository and pull request each is, and where. A folder at the
//! worktree's place that the row does not name is refused, never written
//! into.
//!
//! # Removed when closed, on the next open, or from Settings
//!
//! Every open of a pull request, once its own worktree is at the head,
//! removes the worktrees of pull requests the mirror now holds closed or merged ([`sweep`]); Settings › Checkouts
//! lists them and removes one by hand ([`remove`]). Not on sync: the sync
//! engine runs no git, and a git write nobody clicked for is what ADR-0016
//! keeps out. `worktree remove` refuses a worktree with changes or untracked
//! files, and that refusal stands: such a worktree stays, listed.
//!
//! # The Diff tool
//!
//! `git difftool --dir-diff --gui --no-prompt <base> <head>` at the checkout,
//! offered only when both ends are shas the checkout holds ([`diff_tool`],
//! read with [`super::range::holds_commits`], never fetched). It writes
//! nothing to the repository.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::LazyLock;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use knobas_core::checkout::worktree::{self as rules, Step, difftool_args};
use knobas_core::payload::Declarations;
use knobas_sync::scheduler::AdapterRegistry;

use super::working_tree::{self, PROTOCOL_VARIABLES, REPOSITORY_VARIABLES};
use crate::IpcError;
use crate::settings::{read_value, write_value};
use crate::sources::paths::declared_paths;

/// The `knobas.setting` key holding the worktrees knobas made.
pub const WORKTREES_KEY: &str = "checkout.pull_request_worktrees";

/// How long a fetch may take: it is the network.
pub const FETCH_TIMEOUT: Duration = Duration::from_secs(120);

/// How long any other git command here may take: the disk.
pub const LOCAL_TIMEOUT: Duration = Duration::from_secs(60);

/// One worktree knobas made, as the row keeps it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Record {
    /// The repository's entity id.
    repo: String,
    /// The pull request's entity id, when the page named it: what [`sweep`]
    /// reads its state by.
    #[serde(default)]
    item: Option<String>,
    number: u64,
    path: String,
}

/// A worktree, as Settings › Checkouts lists it.
///
/// - `closed`: The mirror holds the pull request closed or merged.
///
/// Issue #2285.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct PullRequestWorktree {
    pub repo_entity_id: String,
    pub number: u64,
    pub path: String,
    pub closed: bool,
}

/// One worktree operation at a time, process-wide: two opens of one pull
/// request must not both add it, and a sweep must not remove what an open is
/// moving.
static BUSY: LazyLock<tokio::sync::Mutex<()>> = LazyLock::new(|| tokio::sync::Mutex::new(()));

async fn load(pool: &PgPool) -> Result<Vec<Record>, IpcError> {
    Ok(read_value(pool, WORKTREES_KEY)
        .await?
        .and_then(|value| serde_json::from_value(value).ok())
        .unwrap_or_default())
}

async fn save(pool: &PgPool, records: &[Record]) -> Result<(), IpcError> {
    let value = serde_json::to_value(records).map_err(IpcError::internal)?;
    write_value(pool, WORKTREES_KEY, &value).await
}

/// Whether the mirror holds pull request `item` closed or merged: the state
/// its kind declares (#2358) resolves, and to something other than `open` --
/// Gitea's `closed`, or `merged` where its merged flag leads. One the mirror
/// no longer holds, or whose state no declaration names, is not known to be
/// closed, so its worktree is never swept on a guess.
async fn closed(
    pool: &PgPool,
    declarations: &Declarations,
    item: Option<&str>,
) -> Result<bool, IpcError> {
    let Some(item) = item else { return Ok(false) };
    let state: Option<Option<String>> = sqlx::query_scalar(concat!(
        "select ",
        knobas_core::declared_state!("$2"),
        "->>'text' from sync.item i where i.entity_id = $1"
    ))
    .bind(item)
    .bind(declarations.as_param())
    .fetch_optional(pool)
    .await
    .map_err(IpcError::internal)?;
    Ok(state.flatten().is_some_and(|state| state != "open"))
}

async fn git() -> Result<PathBuf, IpcError> {
    working_tree::git_on_path()
        .await
        .map_err(|read| match read {
            working_tree::TreeRead::Failed { reason } => {
                IpcError::invalid(format!("git could not be checked: {reason}"))
            }
            _ => IpcError::invalid("git is not installed, so knobas cannot do this"),
        })
}

/// Run `git <args>` to its end, under `timeout`: `Ok` with its stdout, or
/// git's first words on stderr. No terminal prompt, git's own English, and no
/// inherited variable that points git at another repository or overrides the
/// `protocol.*` flags.
async fn run(
    git: &Path,
    args: Vec<std::ffi::OsString>,
    timeout: Duration,
) -> Result<String, String> {
    let mut command = tokio::process::Command::from(knobas_db::console::command(git));
    command
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    for name in PROTOCOL_VARIABLES.into_iter().chain(REPOSITORY_VARIABLES) {
        command.env_remove(name);
    }
    let child = working_tree::spawn_off_runtime(command)
        .await
        .map_err(|error| error.to_string())?
        .map_err(|error| format!("git could not be run: {error}"))?;
    let output = tokio::time::timeout(timeout, child.wait_with_output())
        .await
        .map_err(|_| format!("git did not finish within {} s", timeout.as_secs()))?
        .map_err(|error| error.to_string())?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        Err(refusal(&String::from_utf8_lossy(&output.stderr)))
    }
}

/// git's words on stderr, up to eight lines on one: a refusal to move names
/// the files in the way on the lines after its first.
fn refusal(stderr: &str) -> String {
    let lines: Vec<&str> = stderr
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .take(8)
        .collect();
    if lines.is_empty() {
        working_tree::refusal(stderr)
    } else {
        lines.join(" ")
    }
}

async fn exists(path: &Path) -> bool {
    tokio::fs::symlink_metadata(path).await.is_ok()
}

/// Whether the folder at `path` is still a linked worktree: its `.git` is a
/// file. A folder whose `.git` is gone is no repository of its own, and git
/// run there would look upward and find whatever repository holds the clones
/// root, so nothing is run there.
async fn is_worktree(path: &Path) -> bool {
    tokio::fs::symlink_metadata(path.join(".git"))
        .await
        .is_ok_and(|meta| meta.is_file())
}

/// Open pull request `number` of this entity's repository in its worktree:
/// make it on the first open, fetch its head and move there on every open.
/// Answers the worktree's path, for the application's `{path}`.
///
/// `item` is the pull request's entity id, when the page knows it.
/// `protocols` is [`knobas_core::checkout::clone::PROTOCOLS`] everywhere but a
/// test that fetches from a folder or the testenv's http.
///
/// # Errors
///
/// [`NotFound`](crate::IpcErrorCode::NotFound) for no checkout on this
/// machine or a source that is gone; [`Invalid`](crate::IpcErrorCode::Invalid)
/// for no clones root, no git, an address knobas does not fetch from, or a
/// fetch git refused; [`Conflict`](crate::IpcErrorCode::Conflict) for a
/// folder at the worktree's place that knobas did not make, or a change in
/// the worktree that the new head would overwrite.
pub async fn open(
    pool: &PgPool,
    registry: &dyn AdapterRegistry,
    entity_id: &str,
    number: u64,
    item: Option<&str>,
    protocols: &[&str],
) -> Result<PathBuf, IpcError> {
    let checkout = super::resolve(pool, entity_id).await?;
    let repo_id = checkout.repo_entity_id.clone().ok_or_else(|| {
        IpcError::not_found(format!(
            "{entity_id} is a branch whose repository is not in the mirror"
        ))
    })?;
    let checkout = PathBuf::from(checkout.path.ok_or_else(|| {
        IpcError::not_found(format!(
            "{entity_id} has no checkout on this machine to make a pull request's worktree from"
        ))
    })?);
    let root = PathBuf::from(super::clones_root(pool).await?.ok_or_else(|| {
        IpcError::invalid(
            "no clones root is set: choose where pull request worktrees go in Settings › Checkouts",
        )
    })?);
    if !root.is_absolute() {
        return Err(IpcError::invalid(format!(
            "the clones root '{}' is not an absolute path",
            root.display()
        )));
    }
    let (url, name) = super::clone::url_of(pool, registry, &repo_id, protocols).await?;
    let git = git().await?;

    let busy = BUSY.lock().await;
    let mut records = load(pool).await?;
    let known = records
        .iter()
        .position(|r| r.repo == repo_id && r.number == number);
    let worktree = match known {
        Some(at) => PathBuf::from(&records[at].path),
        None => rules::folder(&root, &name, number),
    };
    let shown = worktree.display().to_string();
    if known.is_none() && records.iter().any(|r| Path::new(&r.path) == worktree) {
        return Err(IpcError::conflict(format!(
            "{shown} is the worktree of a pull request of another repository of the same name"
        )));
    }

    let there = exists(&worktree).await;
    if there && known.is_none() {
        return Err(IpcError::conflict(format!(
            "{shown} is already there and knobas did not make it, so it is left alone"
        )));
    }
    if there && !is_worktree(&worktree).await {
        return Err(IpcError::conflict(format!(
            "{shown} is no longer a git worktree (its .git is gone), so it is left alone: \
             remove the folder by hand"
        )));
    }
    // The record is written before the worktree is added: an app that dies
    // between the two leaves a record whose folder is missing, which the next
    // open adds again, never a folder knobas would then refuse as not its own.
    match known {
        Some(at) => {
            if item.is_some() {
                records[at].item = item.map(str::to_owned);
            }
        }
        None => records.push(Record {
            repo: repo_id.clone(),
            item: item.map(str::to_owned),
            number,
            path: shown.clone(),
        }),
    }
    save(pool, &records).await?;
    let made = !there;
    if made {
        let added = add(&git, &checkout, &worktree, known.is_some()).await;
        if let Err(why) = added {
            if known.is_none() {
                records.retain(|r| Path::new(&r.path) != worktree);
                save(pool, &records).await?;
            }
            return Err(IpcError::invalid(format!(
                "git could not add {shown}: {why}"
            )));
        }
    }

    let fetch = Step::Fetch {
        worktree: &worktree,
        url: &url,
        number,
        protocols,
    };
    let fetched = run(&git, fetch.args(), FETCH_TIMEOUT).await;
    if let Err(why) = fetched {
        if made {
            // Empty, and this open's: take it back, and its record.
            let _ = run(
                &git,
                Step::Discard {
                    worktree: &worktree,
                }
                .args(),
                LOCAL_TIMEOUT,
            )
            .await;
            records.retain(|r| Path::new(&r.path) != worktree);
            save(pool, &records).await?;
        }
        return Err(IpcError::invalid(format!(
            "git could not fetch pull request #{number} from {url}: {why}"
        )));
    }
    run(
        &git,
        Step::Move {
            worktree: &worktree,
        }
        .args(),
        LOCAL_TIMEOUT,
    )
    .await
    .map_err(|why| {
        IpcError::conflict(format!(
            "{shown} stays where it was, because git would not move it to the pull \
                 request's new head: {why}"
        ))
    })?;
    drop(busy);

    if let Err(error) = sweep_but(pool, registry, Some(&worktree)).await {
        tracing::warn!(%error, "the closed pull requests' worktrees were not swept");
    }
    Ok(worktree)
}

/// Make the folder's parents and add the worktree there.
async fn add(git: &Path, checkout: &Path, worktree: &Path, again: bool) -> Result<(), String> {
    if let Some(parent) = worktree.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|error| format!("could not make {}: {error}", parent.display()))?;
    }
    let add = Step::Add {
        checkout,
        worktree,
        again,
    };
    run(git, add.args(), LOCAL_TIMEOUT).await.map(drop)
}

/// Remove the worktree of every pull request the mirror holds closed or
/// merged, and forget the ones whose folder is gone. A worktree git will not
/// remove (changes, untracked files) stays. Answers the paths removed.
///
/// # Errors
///
/// [`Internal`](crate::IpcErrorCode::Internal) if the row cannot be read or
/// written; git's refusals are not errors.
pub async fn sweep(pool: &PgPool, registry: &dyn AdapterRegistry) -> Result<Vec<String>, IpcError> {
    sweep_but(pool, registry, None).await
}

async fn sweep_but(
    pool: &PgPool,
    registry: &dyn AdapterRegistry,
    keep: Option<&Path>,
) -> Result<Vec<String>, IpcError> {
    let _busy = BUSY.lock().await;
    let declarations = declared_paths(pool, registry).await?;
    let records = load(pool).await?;
    let mut kept = Vec::new();
    let mut removed = Vec::new();
    for record in records {
        let path = PathBuf::from(&record.path);
        if keep == Some(path.as_path()) {
            kept.push(record);
            continue;
        }
        if !exists(&path).await {
            continue;
        }
        if !closed(pool, &declarations, record.item.as_deref()).await? || !is_worktree(&path).await
        {
            kept.push(record);
            continue;
        }
        let git = git().await?;
        match run(&git, Step::Remove { worktree: &path }.args(), LOCAL_TIMEOUT).await {
            Ok(_) => removed.push(record.path),
            Err(why) => {
                tracing::info!(path = %record.path, %why, "a closed pull request's worktree stays");
                kept.push(record);
            }
        }
    }
    save(pool, &kept).await?;
    Ok(removed)
}

/// The worktrees knobas made, for Settings › Checkouts.
///
/// # Errors
///
/// [`Internal`](crate::IpcErrorCode::Internal) for a read that fails.
pub async fn list(
    pool: &PgPool,
    registry: &dyn AdapterRegistry,
) -> Result<Vec<PullRequestWorktree>, IpcError> {
    let declarations = declared_paths(pool, registry).await?;
    let mut out = Vec::new();
    for record in load(pool).await? {
        out.push(PullRequestWorktree {
            closed: closed(pool, &declarations, record.item.as_deref()).await?,
            repo_entity_id: record.repo,
            number: record.number,
            path: record.path,
        });
    }
    Ok(out)
}

/// Remove the worktree at `path`, one knobas made, and answer the list as it
/// then stands. One whose folder is gone is forgotten.
///
/// # Errors
///
/// [`NotFound`](crate::IpcErrorCode::NotFound) for a path that is not one of
/// knobas's worktrees; [`Conflict`](crate::IpcErrorCode::Conflict) when git
/// refuses (changes or untracked files in it), in git's words.
pub async fn remove(
    pool: &PgPool,
    registry: &dyn AdapterRegistry,
    path: &str,
) -> Result<Vec<PullRequestWorktree>, IpcError> {
    {
        let _busy = BUSY.lock().await;
        let mut records = load(pool).await?;
        let at = records
            .iter()
            .position(|r| r.path == path)
            .ok_or_else(|| IpcError::not_found(format!("{path} is not a worktree knobas made")))?;
        let worktree = PathBuf::from(path);
        if exists(&worktree).await {
            if !is_worktree(&worktree).await {
                return Err(IpcError::conflict(format!(
                    "{path} is no longer a git worktree (its .git is gone), so knobas leaves \
                     it alone: remove the folder by hand"
                )));
            }
            let git = git().await?;
            run(
                &git,
                Step::Remove {
                    worktree: &worktree,
                }
                .args(),
                LOCAL_TIMEOUT,
            )
            .await
            .map_err(|why| IpcError::conflict(format!("git would not remove {path}: {why}")))?;
        }
        records.remove(at);
        save(pool, &records).await?;
    }
    list(pool, registry).await
}

// -- the Diff tool ------------------------------------------------------------

/// What the *Diff tool* would run for `base`..`head` of this entity's
/// repository: the argument vector, `git` first, when both ends are shas its
/// checkout holds; `None` when it is not offered (no checkout, no git, an end
/// that is not a sha or not in the checkout).
///
/// # Errors
///
/// [`NotFound`](crate::IpcErrorCode::NotFound) if the mirror does not hold
/// the entity; [`Internal`](crate::IpcErrorCode::Internal) for a query
/// failure.
pub async fn diff_tool(
    pool: &PgPool,
    entity_id: &str,
    base: &str,
    head: &str,
) -> Result<Option<Vec<String>>, IpcError> {
    let Some(path) = super::resolve(pool, entity_id).await?.path else {
        return Ok(None);
    };
    let checkout = PathBuf::from(path);
    let Some(args) = difftool_args(&checkout, base, head) else {
        return Ok(None);
    };
    let Ok(git) = git().await else {
        return Ok(None);
    };
    // The one "does the checkout hold these commits" read (#2272's), never
    // fetched and never walking up past the checkout.
    if !super::range::holds_commits(&checkout, &[base, head]).await {
        return Ok(None);
    }
    let mut argv = vec![git.to_string_lossy().into_owned()];
    argv.extend(args.iter().map(|a| a.to_string_lossy().into_owned()));
    Ok(Some(argv))
}

/// Start the *Diff tool* on `base`..`head`, and do not wait for it. Answers
/// the argument vector it started.
///
/// # Errors
///
/// [`Invalid`](crate::IpcErrorCode::Invalid) when the Diff tool is not
/// offered for this range, or git would not start; as [`diff_tool`]
/// otherwise.
pub async fn open_diff_tool(
    pool: &PgPool,
    entity_id: &str,
    base: &str,
    head: &str,
) -> Result<Vec<String>, IpcError> {
    let argv = diff_tool(pool, entity_id, base, head)
        .await?
        .ok_or_else(|| {
            IpcError::invalid(format!(
                "the Diff tool opens two commits this repository's checkout holds, and \
             {base}...{head} is not that"
            ))
        })?;
    let to_run = argv.clone();
    tokio::task::spawn_blocking(move || {
        let (program, arguments) = to_run.split_first().expect("git and its arguments");
        let mut command = knobas_db::console::command(program);
        knobas_db::console::scrub_knobas_env(&mut command, std::env::vars_os())
            .args(arguments)
            .env("GIT_TERMINAL_PROMPT", "0")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        for name in REPOSITORY_VARIABLES {
            command.env_remove(name);
        }
        let child = command.spawn()?;
        // `difftool --dir-diff` waits for the tool and then removes its
        // temporary folders: reaped, so it is never a zombie.
        std::thread::spawn(move || {
            let mut child = child;
            let _ = child.wait();
        });
        Ok::<(), std::io::Error>(())
    })
    .await
    .map_err(IpcError::internal)?
    .map_err(|error| IpcError::invalid(format!("git difftool could not be run: {error}")))?;
    Ok(argv)
}
