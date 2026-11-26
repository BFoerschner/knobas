//! Commands (*Run…*, ADR-0060): a person's own program, run once on a press
//! in the repository's checkout, whose stdout and stderr knobas reads into
//! lines and shows in a dialog.
//!
//! The pure half (the saved shape, what a command is offered on, what Save
//! refuses, the argument vector) is [`knobas_core::checkout::oneoff`]. Here:
//!
//! - this file: the list's settings row, `oneoff.commands`, and the
//!   `oneoff.shell_path` switch;
//! - [`run`]: the spawn at the checkout, its environment, pipes, timeout,
//!   stop and kill;
//! - [`lines`] and [`places`]: the reader and the place finder;
//! - [`kept`] and [`runs`]: a run's lines and the registry of runs, four at
//!   once, the batches and the memory cap;
//! - [`history`]: `runs.json`, the last 200 runs without their output;
//! - [`shell_path`]: the login shell's `PATH`, read once a session.
//!
//! # Nothing it printed rests
//!
//! A run's output is a live read in the app process (ADR-0032), bounded and
//! dropped at quit: in no table, no index, no log and no backup. The list of
//! runs is a file in the profile (`command-runs/runs.json`) that no backup
//! carries, and a share export carries no command.

pub mod history;
pub mod kept;
pub mod lines;
pub mod made;
pub mod places;
pub mod repo;
pub mod result;
pub mod run;
pub mod runs;
pub mod shell_path;
#[cfg(windows)]
pub mod win;

use std::path::{Path, PathBuf};

use knobas_core::checkout::oneoff::{self as rules, Draft, OneOff};
use sqlx::PgPool;

use crate::IpcError;
use crate::settings::{read_value, write_value};

/// The `knobas.setting` key holding the commands, a JSON array in the
/// person's order.
pub const COMMANDS_KEY: &str = "oneoff.commands";
/// The `knobas.setting` key of *Use my shell's PATH* (on unless `false`).
pub const SHELL_PATH_KEY: &str = "oneoff.shell_path";

/// The folder of knobas's own under the profile where runs keep their files
/// and their list.
#[must_use]
pub fn runs_dir(profile: &Path) -> PathBuf {
    profile.join("command-runs")
}

/// The saved commands, in the person's order. A row that does not parse
/// reads as none: Settings is where a person fixes it.
///
/// # Errors
///
/// [`Internal`](crate::IpcErrorCode::Internal) if the read fails.
pub async fn list(pool: &PgPool) -> Result<Vec<OneOff>, IpcError> {
    Ok(read_value(pool, COMMANDS_KEY)
        .await?
        .and_then(|value| serde_json::from_value(value).ok())
        .unwrap_or_default())
}

/// Saves the whole list, checked, and answers it as stored.
///
/// # Errors
///
/// [`Invalid`](crate::IpcErrorCode::Invalid) naming the first entry that
/// cannot be saved; [`Internal`](crate::IpcErrorCode::Internal) if the write
/// fails.
pub async fn save(pool: &PgPool, drafts: &[Draft]) -> Result<Vec<OneOff>, IpcError> {
    let commands = rules::settle(drafts, std::env::consts::OS)
        .map_err(|error| IpcError::invalid(error.to_string()))?;
    let value = serde_json::to_value(&commands).map_err(IpcError::internal)?;
    write_value(pool, COMMANDS_KEY, &value).await?;
    Ok(commands)
}

/// Whether *Use my shell's PATH* is on: the default on macOS and Linux,
/// never on Windows, where the app has the user's `PATH` already.
///
/// # Errors
///
/// [`Internal`](crate::IpcErrorCode::Internal) if the read fails.
pub async fn shell_path_on(pool: &PgPool) -> Result<bool, IpcError> {
    if cfg!(windows) {
        return Ok(false);
    }
    Ok(read_value(pool, SHELL_PATH_KEY)
        .await?
        .and_then(|value| value.as_bool())
        .unwrap_or(true))
}

/// Turns *Use my shell's PATH* on or off.
///
/// # Errors
///
/// [`Internal`](crate::IpcErrorCode::Internal) if the write fails.
pub async fn set_shell_path(pool: &PgPool, on: bool) -> Result<(), IpcError> {
    write_value(pool, SHELL_PATH_KEY, &serde_json::Value::Bool(on)).await
}

// -- a start -----------------------------------------------------------------

use std::sync::Arc;

use knobas_core::checkout::oneoff::Kind;

use self::history::RunRow;
use self::run::{Placed, Spawn, Started, Target};
use self::runs::{Runs, Sink};

/// Whether `command` is run on `target`: its kind, read from its
/// placeholders, against what the target is.
fn fits(command: &OneOff, target: &Target) -> bool {
    match (command.kind(), target) {
        (Kind::File, Target::File { .. }) | (Kind::Folder, Target::Folder { .. }) => true,
        (Kind::File, Target::Files { .. } | Target::MadeFiles { .. }) => {
            command.command.contains("{files}")
        }
        (Kind::File, Target::Made { dir: false, .. })
        | (Kind::Folder, Target::Made { dir: true, .. }) => true,
        (Kind::Repository, Target::Repository)
        | (Kind::Commit, Target::Commit { .. })
        | (Kind::Range, Target::Range { .. }) => true,
        _ => false,
    }
}

/// A path the page named, as the run names it: from the top, with `/`.
fn rel(path: &str) -> String {
    path.trim_start_matches("./").trim_matches('/').to_owned()
}

/// The target found on disk inside the checkout at `root`.
async fn placed(
    root: PathBuf,
    target: &Target,
    branch: Option<String>,
    commit: Option<String>,
) -> Result<Placed, IpcError> {
    use crate::checkout::open::{file_inside, folder_inside};
    let mut placed = Placed {
        root: root.clone(),
        file: None,
        files: Vec::new(),
        dir: None,
        lines: None,
        sha: None,
        base: None,
        head: None,
        branch,
        commit,
    };
    match target {
        Target::File { path, lines, .. } => {
            let path = rel(path);
            file_inside(&root, &path).await.map_err(|_| {
                IpcError::invalid(format!(
                    "{path} is not in your working tree at {}",
                    root.display()
                ))
            })?;
            placed.dir = Some(path.rsplit_once('/').map_or("", |(d, _)| d).to_owned());
            placed.file = Some(path);
            placed.lines = lines.map(|(a, b)| (a.max(1), b.max(a).max(1)));
        }
        Target::Folder { path } => {
            let path = rel(path);
            if !path.is_empty() {
                folder_inside(&root, &path).await?;
            }
            placed.dir = Some(path);
        }
        Target::Repository => placed.dir = Some(String::new()),
        Target::Commit { sha } => placed.sha = Some(sha.clone()),
        Target::Range { base, head, path } => {
            placed.base = Some(base.clone());
            placed.head = Some(head.clone());
            if let Some(path) = path {
                let path = rel(path);
                file_inside(&root, &path).await?;
                placed.file = Some(path);
            }
        }
        Target::Files { paths } => {
            for path in paths {
                let path = rel(path);
                file_inside(&root, &path).await?;
                placed.files.push(path);
            }
        }
        Target::Made { .. } | Target::MadeFiles { .. } => {
            return Err(IpcError::invalid(
                "a generated folder's file is run in the folder knobas kept".to_owned(),
            ));
        }
    }
    Ok(placed)
}

/// The kept folder a run on a generated folder runs in, and what is done
/// when the run changed it (ADR-0060, amended 2026-11-25).
pub struct OnMade {
    pub made: Arc<crate::folder_render::keep::Made>,
    pub on_change: Box<dyn FnOnce() + Send>,
}

impl OnMade {
    /// The generated folder `token` names in the app, dropped from the app's
    /// store when a run changes it.
    ///
    /// # Errors
    ///
    /// [`NotFound`](crate::IpcErrorCode::NotFound) once it is no longer kept,
    /// saying how to have it again.
    pub fn of_token(token: &str, target: &Target) -> Result<Self, IpcError> {
        let made = crate::folder_render::runs::Runs::global()
            .made(token)
            .ok_or_else(|| {
                let (renderer, folder) = match target {
                    Target::Made {
                        renderer, folder, ..
                    }
                    | Target::MadeFiles {
                        renderer, folder, ..
                    } => (renderer.as_str(), folder.as_str()),
                    _ => ("its renderer", ""),
                };
                IpcError::not_found(format!(
                    "{} as {renderer} makes it is not kept now. Open it, then run again",
                    if folder.is_empty() {
                        "The repository"
                    } else {
                        folder
                    }
                ))
            })?;
        let token = token.to_owned();
        Ok(Self {
            made,
            on_change: Box::new(move || {
                crate::folder_render::runs::Runs::global().drop_made(&token);
            }),
        })
    }
}

/// Runs saved command `command_id` on `target` of `entity_id`'s checkout,
/// its batches to `sink`. Answers once the run has its words; the program
/// runs on in threads of its own. The same command on the same thing already
/// running answers that run, with `sink` added to it.
///
/// A refusal that needs the run's own folder (a value refused at the fill)
/// ends the run as *Did not start*, told on `sink`; a refusal before that is
/// an error and starts nothing.
///
/// # Errors
///
/// [`Invalid`](crate::IpcErrorCode::Invalid) for a command that is not one of
/// the person's, a target it does not take, a path not inside the checkout,
/// or a fifth run; [`NotFound`](crate::IpcErrorCode::NotFound) for a
/// repository with no checkout here.
pub async fn start(
    pool: &PgPool,
    entity_id: &str,
    command_id: &str,
    target: Target,
    made_token: Option<&str>,
    sink: Arc<dyn Sink>,
) -> Result<Started, IpcError> {
    let made = if made::is_made(&target) {
        let token = made_token.ok_or_else(|| {
            IpcError::invalid("a run on a generated folder names the folder knobas kept".to_owned())
        })?;
        Some(OnMade::of_token(token, &target)?)
    } else {
        None
    };
    start_made(
        &Runs::global(),
        pool,
        entity_id,
        command_id,
        target,
        made,
        sink,
    )
    .await
}

/// [`start`] in a registry of the caller's: a test's own.
///
/// # Errors
///
/// As [`start`].
pub async fn start_in(
    runs: &Arc<Runs>,
    pool: &PgPool,
    entity_id: &str,
    command_id: &str,
    target: Target,
    sink: Arc<dyn Sink>,
) -> Result<Started, IpcError> {
    start_made(runs, pool, entity_id, command_id, target, None, sink).await
}

/// [`start_in`], with the kept folder a run on a generated folder runs in.
///
/// # Errors
///
/// As [`start_in`]; a generated target with no kept folder, or a command
/// that names a git value there, is [`Invalid`](crate::IpcErrorCode::Invalid).
#[allow(clippy::too_many_lines)]
pub async fn start_made(
    runs: &Arc<Runs>,
    pool: &PgPool,
    entity_id: &str,
    command_id: &str,
    target: Target,
    on_made: Option<OnMade>,
    sink: Arc<dyn Sink>,
) -> Result<Started, IpcError> {
    let runs = Arc::clone(runs);
    let command = list(pool)
        .await?
        .into_iter()
        .find(|c| c.id == command_id)
        .ok_or_else(|| {
            IpcError::invalid(format!(
                "'{command_id}' is not one of your commands: see Settings › Commands"
            ))
        })?;
    if let Some(running) = runs.running_of(&command.id, entity_id, &target) {
        runs.attach(&running, sink);
        let row = running.row();
        return Ok(Started {
            run: row.id,
            argv: row.argv,
            cwd: row.cwd,
            branch: row.branch,
            head: row.head,
            started_at: row.started_at,
            already: true,
        });
    }
    if !fits(&command, &target) {
        return Err(IpcError::invalid(format!(
            "{} is not run on {}",
            command.name,
            target.label()
        )));
    }
    let made_on = if made::is_made(&target) {
        if let Some(name) = made::git_name(&command) {
            return Err(IpcError::invalid(format!(
                "{} uses {name}, and a generated folder is no git repository",
                command.name
            )));
        }
        Some(on_made.ok_or_else(|| {
            IpcError::invalid("a run on a generated folder names the folder knobas kept".to_owned())
        })?)
    } else {
        None
    };
    let checkout = crate::checkout::resolve(pool, entity_id).await?;
    // A generated folder is run in where knobas kept it, cloned or not: the
    // checkout only gives *Add to PATH*'s `{root}`.
    let (root, path_root, placed, branch, commit) = if let Some(on) = &made_on {
        let placed = made::placed(&on.made, &target)?;
        (
            on.made.dir.clone(),
            checkout.path.map(PathBuf::from),
            placed,
            None,
            None,
        )
    } else {
        let root = PathBuf::from(checkout.path.ok_or_else(|| {
            IpcError::not_found(format!(
                "{entity_id} is not cloned on this machine, so there is nothing to run {} in",
                command.name
            ))
        })?);
        let (branch, commit) = match crate::checkout::working_tree::read_known(&root).await {
            crate::checkout::working_tree::TreeRead::Read(state) => (state.branch, state.head),
            _ => (None, None),
        };
        let placed = placed(root.clone(), &target, branch.clone(), commit.clone()).await?;
        (root.clone(), Some(root), placed, branch, commit)
    };
    let shell = if shell_path_on(pool).await? {
        shell_path::get(false).await.path
    } else {
        None
    };
    let started_at = chrono::Utc::now().timestamp_millis();
    let handle = runs
        .reserve(
            RunRow {
                id: String::new(),
                command: command.id.clone(),
                name: command.name.clone(),
                entity_id: entity_id.to_owned(),
                target: target.clone(),
                argv: Vec::new(),
                cwd: root.to_string_lossy().into_owned(),
                branch: branch.clone(),
                head: commit.clone(),
                started_at,
                duration_ms: None,
                ended: None,
                lines: 0,
                findings: 0,
                kept: true,
            },
            kept::Limits::default(),
        )
        .map_err(|why| IpcError::invalid(why.to_string()))?;
    runs.attach(&handle, sink);
    let answer = |argv: Vec<String>, cwd: String| Started {
        run: handle.id.clone(),
        argv,
        cwd,
        branch: branch.clone(),
        head: commit.clone(),
        started_at,
        already: false,
    };
    let (argv, cwd) = match run::words_at(&command, &placed, &handle.scratch) {
        Ok(words) => words,
        Err(reason) => {
            let cwd = root.to_string_lossy().into_owned();
            runs.end(&handle, run::Ended::NotStarted { reason });
            return Ok(answer(Vec::new(), cwd));
        }
    };
    handle.set_words(&argv, &cwd.to_string_lossy());
    let wants = if command.command.contains("{outdir}") {
        run::Wants::Outdir
    } else if command.command.contains("{out}") {
        run::Wants::Out
    } else {
        run::Wants::Stdout
    };
    if wants != run::Wants::Stdout {
        let _ = std::fs::create_dir_all(handle.scratch.join("out"));
    }
    let cwd_rel = cwd
        .strip_prefix(&root)
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_default();
    let alias = if command.wsl_paths && cfg!(windows) {
        rules::wsl_path(&root.to_string_lossy())
    } else {
        None
    };
    let env = run::environment(
        &command,
        path_root.as_deref(),
        std::env::vars_os(),
        shell.as_deref(),
    );
    let spawn = Spawn {
        argv: argv.clone(),
        cwd: cwd.clone(),
        env,
        timeout: std::time::Duration::from_secs(u64::from(
            command.timeout_secs.clamp(1, rules::MAX_TIMEOUT),
        )),
        stdin: run::stdin_of(command.stdin, &placed),
        root,
        cwd_rel,
        wants,
        shows: command.shows,
        alias,
        ext: match &target {
            Target::File { path, .. }
            | Target::Made {
                path, dir: false, ..
            } => path.rsplit_once('.').map(|(_, e)| e.to_owned()),
            _ => None,
        },
        made: made_on.map(|on| made::Watch::new(on.made, on.on_change)),
    };
    let to_run = Arc::clone(&handle);
    tokio::task::spawn_blocking(move || run::launch(&runs, &to_run, spawn))
        .await
        .map_err(IpcError::internal)?;
    Ok(answer(argv, cwd.to_string_lossy().into_owned()))
}

// -- a repository's own commands (section 22) --------------------------------

/// What a repository's `.knobas/commands.toml` offers, as its copy dialog
/// shows it.
#[derive(Clone, Debug, serde::Serialize, ts_rs::TS)]
#[ts(export_to = "app.ts", rename = "OneOffRepoCommands")]
pub struct RepoCommands {
    /// The file's path on disk, where the checkout holds one.
    #[ts(optional = nullable)]
    pub file: Option<String>,
    pub offered: Vec<repo::Offered>,
    /// Why the file could not be read.
    #[ts(optional = nullable)]
    pub failed: Option<String>,
}

/// The commands a repository's checkout offers, read from the disk now.
///
/// # Errors
///
/// [`NotFound`](crate::IpcErrorCode::NotFound) for a repository with no
/// checkout here; [`Internal`](crate::IpcErrorCode::Internal) for a read of
/// the list.
pub async fn repo_commands(pool: &PgPool, entity_id: &str) -> Result<RepoCommands, IpcError> {
    let checkout = crate::checkout::resolve(pool, entity_id).await?;
    let Some(root) = checkout.path.map(PathBuf::from) else {
        return Ok(RepoCommands {
            file: None,
            offered: Vec::new(),
            failed: None,
        });
    };
    let saved = list(pool).await?;
    let at = root.clone();
    let text = tokio::task::spawn_blocking(move || repo::read(&at))
        .await
        .map_err(IpcError::internal)?;
    let Some(text) = text else {
        return Ok(RepoCommands {
            file: None,
            offered: Vec::new(),
            failed: None,
        });
    };
    let file = Some(
        root.join(".knobas")
            .join("commands.toml")
            .to_string_lossy()
            .into_owned(),
    );
    Ok(
        match repo::offered(&text, entity_id, &saved, std::env::consts::OS) {
            Ok(offered) => RepoCommands {
                file,
                offered,
                failed: None,
            },
            Err(why) => RepoCommands {
                file,
                offered: Vec::new(),
                failed: Some(why),
            },
        },
    )
}

/// Copies the commands named `names` from the repository's file, as it is on
/// disk now, into the person's list, scoped to the repository: a new one at
/// the list's end, one already copied changed in place. Answers the list as
/// stored.
///
/// # Errors
///
/// As [`repo_commands`]; [`Invalid`](crate::IpcErrorCode::Invalid) for a
/// command Save refuses.
pub async fn copy_repo_commands(
    pool: &PgPool,
    entity_id: &str,
    names: &[String],
) -> Result<Vec<OneOff>, IpcError> {
    let read = repo_commands(pool, entity_id).await?;
    let saved = list(pool).await?;
    let mut drafts: Vec<Draft> = saved.iter().map(draft_of).collect();
    for offered in read.offered.iter().filter(|o| names.contains(&o.name)) {
        if let Some(id) = &offered.copied {
            if let Some(slot) = drafts.iter_mut().find(|d| d.id.as_deref() == Some(id)) {
                // The file's line and scope; what the person set around it
                // (PATH folders, variables, colours) stays theirs.
                let theirs = std::mem::take(slot);
                *slot = Draft {
                    path: theirs.path,
                    env: theirs.env,
                    colours: theirs.colours,
                    wsl_paths: theirs.wsl_paths,
                    ..repo::draft(offered, entity_id, Some(id.clone()))
                };
            }
        } else {
            drafts.push(repo::draft(offered, entity_id, None));
        }
    }
    save(pool, &drafts).await
}

/// A saved command as the draft that saves it unchanged.
fn draft_of(c: &OneOff) -> Draft {
    Draft {
        id: Some(c.id.clone()),
        name: c.name.clone(),
        command: c.command.clone(),
        stdin: c.stdin,
        files: c.files.clone(),
        folders: c.folders.clone(),
        has: c.has.clone(),
        repo_holds: c.repo_holds.clone(),
        repos: c.repos.clone(),
        shows: c.shows,
        start_in: c.start_in,
        path: c.path.clone(),
        env: c.env.clone(),
        colours: Some(c.colours),
        timeout_secs: Some(c.timeout_secs),
        wsl_paths: c.wsl_paths,
    }
}
