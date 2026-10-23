//! *Open in…* (#844, R-8.21 to R-8.24): the applications a person runs at a
//! checkout, where they are kept, and the spawn.
//!
//! The pure half -- the shipped list, the placeholders, the argument vector,
//! the checks a saved list goes through -- is
//! [`knobas_core::checkout::open`]. What is here needs a database or an
//! operating system.
//!
//! # One settings row
//!
//! The list and its default are one `knobas.setting` row,
//! [`APPLICATIONS_KEY`], a JSON object `{ "default": <id>, "applications":
//! [...] }`, under migration `0002`'s comment 6 store, for the clones root's
//! reason: a short list a person types once is not a relation. No migration.
//!
//! Until somebody saves the list there is no row, and the list is this
//! platform's shipped one with #501's stored templates over it
//! (`checkout.command.vscode`, `.jetbrains`, `.terminal`): today's templates
//! become the first entries, under the ids #501 gave them. #501's two
//! commands, which wrote those templates, are gone (#1481); a database that
//! holds one still reads it here.
//!
//! # The only values are from this disk (ADR-0016)
//!
//! [`open`] fills a command from the checkout `resolve` found on this machine
//! and a file path it has checked is inside that checkout on disk. No field of
//! the mirrored repository reaches a spawned process.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use knobas_core::checkout::open::{self as rules, Application, Draft, Fill};

use super::working_tree::{self, TreeRead};
use crate::IpcError;
use crate::settings::{read as read_setting, read_value, write_value};

/// The `knobas.setting` key holding the applications and the default.
pub const APPLICATIONS_KEY: &str = "checkout.applications";

/// #501's per-action keys, read once when no list is saved yet.
#[must_use]
pub fn legacy_key(id: &str) -> String {
    format!("checkout.command.{id}")
}

/// #501's three actions, with the name each one's application goes by. An
/// action whose id is not in this platform's shipped list and whose template
/// somebody set becomes an entry of its own.
const LEGACY: [(&str, &str); 3] = [
    ("vscode", "VS Code"),
    ("jetbrains", "IntelliJ IDEA"),
    ("terminal", "Terminal"),
];

/// What the row holds.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stored {
    /// The default's id; `None` only for an empty list.
    pub default: Option<String>,
    pub applications: Vec<Application>,
}

impl Stored {
    /// The default, else the first entry: a default whose entry is gone is
    /// never left pointing at nothing.
    fn settle_default(mut self) -> Self {
        let known = |id: &String| self.applications.iter().any(|app| &app.id == id);
        if !self.default.as_ref().is_some_and(known) {
            self.default = self.applications.first().map(|app| app.id.clone());
        }
        self
    }

    fn by_id(&self, id: &str) -> Option<&Application> {
        self.applications.iter().find(|app| app.id == id)
    }
}

/// The list as it stands: the saved row, else the shipped list with #501's
/// templates over it.
///
/// `shipped` is an argument so the list a non-macOS machine starts with --
/// none -- is reachable from a test on the Mac the gate runs on.
///
/// A row that does not parse reads as no row: an older or a broken knobas
/// wrote it, and the shipped list is the state a person can fix from
/// Settings.
async fn load_with(pool: &PgPool, shipped: Vec<Application>) -> Result<Stored, IpcError> {
    if let Some(value) = read_value(pool, APPLICATIONS_KEY).await?
        && let Ok(stored) = serde_json::from_value::<Stored>(value)
    {
        return Ok(stored.settle_default());
    }
    let mut applications = shipped;
    for (id, name) in LEGACY {
        let Some(template) = read_setting(pool, &legacy_key(id)).await? else {
            continue;
        };
        match applications.iter_mut().find(|app| app.id == id) {
            Some(app) => app.command = template,
            None => applications.push(Application {
                id: id.to_owned(),
                name: name.to_owned(),
                command: template,
                file_command: None,
            }),
        }
    }
    let default = applications
        .iter()
        .find(|app| app.id == rules::SHIPPED_DEFAULT)
        .map(|app| app.id.clone());
    Ok(Stored {
        default,
        applications,
    }
    .settle_default())
}

async fn load(pool: &PgPool) -> Result<Stored, IpcError> {
    load_with(pool, rules::shipped()).await
}

async fn save(pool: &PgPool, stored: &Stored) -> Result<(), IpcError> {
    let value = serde_json::to_value(stored).map_err(IpcError::internal)?;
    write_value(pool, APPLICATIONS_KEY, &value).await
}

/// The `git` knobas runs at a checkout, as Settings › Checkouts says it.
///
/// Where it is and nothing more: knobas runs git only for `status`, `diff`
/// and `clone` (ADR-0021, ADR-0026, ADR-0023), so it does not ask git its
/// version.
///
/// - `found`: The `git` on `PATH`.
/// - `no_git`: None, or Apple's shim without the Command Line Tools.
/// - `failed`: The Command Line Tools probe did not answer.
///
/// Issue #844.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, ts_rs::TS)]
#[serde(tag = "status", rename_all = "snake_case")]
#[ts(export_to = "entity.ts")]
pub enum GitFound {
    Found { path: String },
    NoGit,
    Failed { reason: String },
}

/// What Settings › Checkouts draws, and what every *Open in…* menu reads.
///
/// Issue #844, R-13a.3.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct CheckoutSettings {
    pub clones_root: Option<String>,
    pub git: GitFound,
    /// The default's id; `None` only when there are no applications.
    pub default_application: Option<String>,
    /// In the order the person put them.
    pub applications: Vec<Application>,
}

/// The applications, the default, the clones root and the git found.
///
/// # Errors
///
/// [`Internal`](crate::IpcErrorCode::Internal) if a read fails.
pub async fn settings(pool: &PgPool) -> Result<CheckoutSettings, IpcError> {
    let stored = load(pool).await?;
    let git = match working_tree::git_on_path().await {
        Ok(path) => GitFound::Found {
            path: path.to_string_lossy().into_owned(),
        },
        Err(TreeRead::Failed { reason }) => GitFound::Failed { reason },
        Err(_) => GitFound::NoGit,
    };
    Ok(CheckoutSettings {
        clones_root: super::clones_root(pool).await?,
        git,
        default_application: stored.default,
        applications: stored.applications,
    })
}

/// Save the whole list and the default.
///
/// The whole list, because each change the dialogs make -- add, edit, remove,
/// move up, a new default -- is the list as it now stands, and one write of it
/// cannot leave half of a reorder behind. A default that names no entry is the
/// first one.
///
/// # Errors
///
/// [`Invalid`](crate::IpcErrorCode::Invalid) for an entry with no name or a
/// command that cannot be run, naming it;
/// [`Internal`](crate::IpcErrorCode::Internal) if the write fails.
pub async fn set_applications(
    pool: &PgPool,
    drafts: &[Draft],
    default: Option<&str>,
) -> Result<(), IpcError> {
    let applications =
        rules::settle(drafts).map_err(|error| IpcError::invalid(error.to_string()))?;
    let stored = Stored {
        default: default.map(str::to_owned),
        applications,
    }
    .settle_default();
    save(pool, &stored).await
}

// -- the spawn ----------------------------------------------------------------

/// Run a saved application at this entity's checkout (the default when
/// `application` is `None`), at a file in it when one is named. Answers the
/// argument vector it started.
///
/// The program is always a template from `knobas.setting` (ADR-0016): *Try
/// it* saves the draft first and opens it by its id (#1643), so nothing the
/// webview sends names a program.
///
/// The file is a path relative to the checkout, and it must be a file inside
/// it **on disk**: the words are checked first ([`rules::inside`]), and then
/// the canonical path, which is where a symlink out of the checkout is caught.
/// With a file, the application's file command runs, else its command.
///
/// # Errors
///
/// [`Invalid`](crate::IpcErrorCode::Invalid) for an unknown application, no
/// application at all, a file that is not inside the checkout, a command that
/// cannot be run, or a program that would not start;
/// [`NotFound`](crate::IpcErrorCode::NotFound) if the mirror does not hold the
/// entity or it has no checkout on this machine;
/// [`Internal`](crate::IpcErrorCode::Internal) for a query failure, or a launch
/// whose blocking task did not finish.
pub async fn open(
    pool: &PgPool,
    entity_id: &str,
    application: Option<&str>,
    file: Option<&str>,
    line: Option<u32>,
) -> Result<Vec<String>, IpcError> {
    let checkout = super::resolve(pool, entity_id).await?;
    let path = checkout.path.ok_or_else(|| {
        IpcError::not_found(format!(
            "{entity_id} has no checkout on this machine, so there is nothing to open"
        ))
    })?;
    let checkout = PathBuf::from(path);
    let file = match file.map(str::trim).filter(|file| !file.is_empty()) {
        Some(relative) => Some(file_inside(&checkout, relative).await?),
        None => None,
    };
    let stored = load(pool).await?;
    let wanted = application
        .map(str::to_owned)
        .or_else(|| stored.default.clone());
    let app = wanted
        .as_deref()
        .and_then(|id| stored.by_id(id))
        .ok_or_else(|| match &wanted {
            Some(id) => IpcError::invalid(format!(
                "'{id}' is not one of your applications: see Settings › Checkouts"
            )),
            None => IpcError::invalid(
                "there is no application to open a checkout with: add one in Settings › \
                 Checkouts",
            ),
        })?;
    let template = match (&file, &app.file_command) {
        (Some(_), Some(file_command)) => file_command.clone(),
        _ => app.command.clone(),
    };
    let argv = rules::fill(
        &template,
        &Fill {
            checkout: &checkout,
            file: file.as_deref(),
            line,
        },
    )
    .map_err(|error| IpcError::invalid(format!("'{template}' cannot be run: {error}")))?;
    // The fork and exec run off the async runtime.
    let to_run = argv.clone();
    tokio::task::spawn_blocking(move || spawn(&to_run))
        .await
        .map_err(IpcError::internal)?
        .map_err(|error| IpcError::invalid(format!("'{template}' could not be run: {error}")))?;
    Ok(argv)
}

/// `relative` inside `checkout`, as a file that is there.
async fn file_inside(checkout: &Path, relative: &str) -> Result<PathBuf, IpcError> {
    let refuse = || {
        IpcError::invalid(format!(
            "{relative} is not a file inside the checkout at {}",
            checkout.display()
        ))
    };
    let joined = rules::inside(checkout, relative).ok_or_else(refuse)?;
    let root = checkout.to_path_buf();
    let probe = joined.clone();
    let inside = tokio::task::spawn_blocking(move || {
        let root = std::fs::canonicalize(root).ok()?;
        let real = std::fs::canonicalize(probe).ok()?;
        (real.starts_with(&root) && real.is_file()).then_some(())
    })
    .await
    .ok()
    .flatten();
    inside.map(|()| joined).ok_or_else(refuse)
}

/// Start `argv[0]` with the rest as its arguments, and do not wait for it.
///
/// **No shell.** The argument vector goes straight to `exec`, so a checkout
/// path holding a space, a `$` or a `;` is one argument and stays one, and a
/// command cannot pipe or chain.
///
/// The three standard streams go to `/dev/null`: an editor that prints on
/// startup would otherwise write into the app's own stdout, and a child that
/// fills a pipe nobody reads blocks for ever. A thread reaps it, because an
/// editor outlives the click that started it and a dropped `Child` that is
/// never waited on is a zombie for the life of the process.
fn spawn(argv: &[String]) -> std::io::Result<()> {
    let child = scrubbed_command(argv, std::env::vars_os())?
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()?;
    std::thread::spawn(move || {
        let mut child = child;
        let _ = child.wait();
    });
    Ok(())
}

/// `argv[0]` with the rest as its arguments, in `parent`'s environment less
/// every `KNOBAS_*` variable (#1643).
///
/// knobas's own variables are for knobas: under `just dev` `KNOBAS_DB_URL`
/// carries the database password, and a terminal opened at a checkout would
/// otherwise hand it to every shell started in it. The git paths drop `GIT_*`
/// for their reason; this drops `KNOBAS_*` for this one. Everything else,
/// `PATH` and `HOME` first, is what the program needs to find its way.
fn scrubbed_command(
    argv: &[String],
    parent: impl IntoIterator<Item = (std::ffi::OsString, std::ffi::OsString)>,
) -> std::io::Result<std::process::Command> {
    let (program, arguments) = argv
        .split_first()
        .ok_or_else(|| std::io::Error::other("the command has no program in it"))?;
    let mut built = std::process::Command::new(program);
    built.args(arguments).env_clear().envs(
        parent
            .into_iter()
            .filter(|(name, _)| !name.as_encoded_bytes().starts_with(b"KNOBAS_")),
    );
    Ok(built)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A default that names no entry is the first one, and an empty list has
    /// none.
    #[test]
    fn a_default_whose_entry_is_gone_is_the_first() {
        let shipped = knobas_core::checkout::open::shipped_on("macos");
        let stored = Stored {
            default: Some("gone".to_owned()),
            applications: shipped.clone(),
        }
        .settle_default();
        assert_eq!(stored.default.as_deref(), Some(shipped[0].id.as_str()));
        let empty = Stored {
            default: Some("gone".to_owned()),
            applications: vec![],
        }
        .settle_default();
        assert_eq!(empty.default, None);
    }

    /// A template of `env`, started as [`spawn`] starts a program: knobas's own
    /// `KNOBAS_*` variables (under `just dev`, the database URL and its
    /// password) do not reach it, and the rest of the environment does (#1643).
    #[cfg(unix)]
    #[test]
    fn a_spawned_program_inherits_no_knobas_variable() {
        let path = std::env::var_os("PATH").expect("the tests run with a PATH");
        let parent = [
            ("PATH".into(), path.clone()),
            (
                "KNOBAS_DB_URL".into(),
                "postgres://knobas:hunter2@127.0.0.1:5433/knobas".into(),
            ),
            ("KNOBAS_".into(), "bare".into()),
            ("KNOBAS_LOG".into(), "debug".into()),
            ("LANG".into(), "en_US.UTF-8".into()),
        ];
        let argv = rules::fill(
            "env",
            &Fill {
                checkout: Path::new("/"),
                file: None,
                line: None,
            },
        )
        .unwrap();

        let output = scrubbed_command(&argv, parent).unwrap().output().unwrap();

        assert!(output.status.success(), "{output:?}");
        let printed = String::from_utf8(output.stdout).unwrap();
        let names: Vec<&str> = printed
            .lines()
            .filter_map(|line| line.split_once('=').map(|(name, _)| name))
            .collect();
        assert!(
            !names.iter().any(|name| name.starts_with("KNOBAS_")),
            "{printed}"
        );
        assert!(!printed.contains("hunter2"), "{printed}");
        assert!(
            printed
                .lines()
                .any(|line| line == format!("PATH={}", path.to_string_lossy())),
            "{printed}"
        );
        assert!(
            printed.lines().any(|line| line == "LANG=en_US.UTF-8"),
            "{printed}"
        );
    }

    /// The legacy keys are #501's spellings, and the list's key is its own.
    #[test]
    fn the_keys_are_501s_and_the_lists_own() {
        let legacy: Vec<String> = LEGACY.iter().map(|(id, _)| legacy_key(id)).collect();
        assert_eq!(
            legacy,
            [
                "checkout.command.vscode",
                "checkout.command.jetbrains",
                "checkout.command.terminal"
            ]
        );
        assert!(!legacy.contains(&APPLICATIONS_KEY.to_owned()));
        assert_ne!(APPLICATIONS_KEY, super::super::CLONES_ROOT_KEY);
    }
}
