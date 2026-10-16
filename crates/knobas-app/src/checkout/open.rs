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
//! commands keep working over the list (`commands`, `set_command`), because
//! the rooms UI and the desktop witness of ADR-0016 still press them.
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
use crate::settings::{read as read_setting, read_value, settable, write_value};

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
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum GitFound {
    /// The `git` on `PATH`.
    Found { path: String },
    /// None, or Apple's shim without the Command Line Tools.
    NoGit,
    /// The Command Line Tools probe did not answer.
    Failed { reason: String },
}

/// What Settings › Checkouts draws, and what every *Open in…* menu reads.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
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

// -- #501's two commands, over the list --------------------------------------

/// One application as #501's checkout panel draws it: a button.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OpenCommandView {
    /// The application's id: what `open_checkout` takes.
    pub action: String,
    /// What the button says: "Open in VS Code".
    pub label: String,
    /// The command. Always set: an application has one.
    pub template: Option<String>,
    /// Whether the command is the one this platform ships for this id, so
    /// the rooms settings field can offer to go back to it.
    pub is_default: bool,
}

fn command_view(app: &Application, shipped: &[Application]) -> OpenCommandView {
    OpenCommandView {
        action: app.id.clone(),
        label: format!("Open in {}", app.name),
        template: Some(app.command.clone()),
        is_default: shipped
            .iter()
            .any(|known| known.id == app.id && known.command == app.command),
    }
}

/// Every application as a button, in the list's order.
///
/// # Errors
///
/// [`Internal`](crate::IpcErrorCode::Internal) if the read fails.
pub async fn commands(pool: &PgPool) -> Result<Vec<OpenCommandView>, IpcError> {
    let shipped = rules::shipped();
    let stored = load_with(pool, shipped.clone()).await?;
    Ok(stored
        .applications
        .iter()
        .map(|app| command_view(app, &shipped))
        .collect())
}

/// Set one application's command, or put back the one this platform ships
/// for it.
///
/// # Errors
///
/// [`Invalid`](crate::IpcErrorCode::Invalid) for an id no application has, a
/// command that cannot be run, or clearing one this platform ships nothing
/// for; [`Internal`](crate::IpcErrorCode::Internal) if the write fails.
pub async fn set_command(
    pool: &PgPool,
    action: &str,
    template: Option<&str>,
) -> Result<(), IpcError> {
    let shipped = rules::shipped();
    let mut stored = load_with(pool, shipped.clone()).await?;
    let known: Vec<String> = stored.applications.iter().map(|a| a.id.clone()).collect();
    let Some(app) = stored.applications.iter_mut().find(|app| app.id == action) else {
        return Err(IpcError::invalid(format!(
            "'{action}' is not something knobas opens a checkout with; it has {}",
            known.join(", ")
        )));
    };
    let command = match settable(template) {
        Some(command) => {
            rules::check(command).map_err(|error| {
                IpcError::invalid(format!("'{command}' cannot be run: {error}"))
            })?;
            command.to_owned()
        }
        None => shipped
            .iter()
            .find(|known| known.id == action)
            .map(|known| known.command.clone())
            .ok_or_else(|| {
                IpcError::invalid(format!(
                    "{} has no command on this platform to go back to",
                    app.name
                ))
            })?,
    };
    app.command = command;
    save(pool, &stored).await
}

// -- the spawn ----------------------------------------------------------------

/// What to run: a saved application (the default when none is named), or a
/// command typed into the *Add an application* dialog that is not saved yet
/// (*Try it*).
#[derive(Clone, Copy, Debug)]
pub enum Run<'a> {
    Application(Option<&'a str>),
    Command(&'a str),
}

/// Run an application at this entity's checkout, at a file in it when one is
/// named. Answers the argument vector it started.
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
/// [`Internal`](crate::IpcErrorCode::Internal) for a query failure.
pub async fn open(
    pool: &PgPool,
    entity_id: &str,
    run: Run<'_>,
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
    let template = match run {
        Run::Command(command) => command.trim().to_owned(),
        Run::Application(id) => {
            let stored = load(pool).await?;
            let wanted = id.map(str::to_owned).or_else(|| stored.default.clone());
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
            match (&file, &app.file_command) {
                (Some(_), Some(file_command)) => file_command.clone(),
                _ => app.command.clone(),
            }
        }
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
    spawn(&argv)
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
    let (program, arguments) = argv
        .split_first()
        .ok_or_else(|| std::io::Error::other("the command has no program in it"))?;
    let child = std::process::Command::new(program)
        .args(arguments)
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

#[cfg(test)]
mod tests {
    use super::*;

    const MIRROR: &str = include_str!("../../../../app/src/lib/ipc/entity.ts");

    /// The body of one `export interface <name> {` in the mirror.
    fn interface(name: &str) -> &'static str {
        MIRROR
            .split_once(&format!("export interface {name} {{"))
            .unwrap_or_else(|| panic!("entity.ts declares {name}"))
            .1
            .split_once("\n}")
            .expect("the interface is closed")
            .0
    }

    fn keys(value: &serde_json::Value) -> Vec<String> {
        let mut keys: Vec<String> = value.as_object().unwrap().keys().cloned().collect();
        keys.sort();
        keys
    }

    /// Exactly the keys the mirror declares, for each shape on the wire: a
    /// field added on one side only is invisible to a walk over a hardcoded
    /// list.
    #[test]
    fn the_settings_serialise_the_keys_the_mirror_declares() {
        let settings = CheckoutSettings {
            clones_root: None,
            git: GitFound::NoGit,
            default_application: Some("vscode".to_owned()),
            applications: vec![Application {
                id: "vscode".to_owned(),
                name: "VS Code".to_owned(),
                command: "code {path}".to_owned(),
                file_command: None,
            }],
        };
        let json = serde_json::to_value(&settings).unwrap();
        assert_eq!(
            keys(&json),
            ["applications", "clones_root", "default_application", "git"]
        );
        let body = interface("CheckoutSettings");
        for key in keys(&json) {
            assert!(body.contains(&format!("{key}:")), "CheckoutSettings.{key}");
        }
        let app = &json["applications"][0];
        assert_eq!(keys(app), ["command", "file_command", "id", "name"]);
        let body = interface("CheckoutApplication");
        for key in keys(app) {
            assert!(
                body.contains(&format!("{key}:")),
                "CheckoutApplication.{key}"
            );
        }
        // A draft is what the mirror sends back: the same four, the id optional.
        let draft: knobas_core::checkout::open::Draft = serde_json::from_value(serde_json::json!({
            "id": null, "name": "X", "command": "x {path}", "file_command": null
        }))
        .unwrap();
        assert_eq!(draft.id, None);
        let body = interface("CheckoutApplicationDraft");
        for key in ["id", "name", "command", "file_command"] {
            assert!(
                body.contains(&format!("{key}:")),
                "CheckoutApplicationDraft.{key}"
            );
        }
    }

    /// The git found is tagged on `status`, with the words the mirror narrows on.
    #[test]
    fn the_git_found_serialises_as_the_mirror_declares() {
        for (git, word) in [
            (
                GitFound::Found {
                    path: "/usr/bin/git".to_owned(),
                },
                "found",
            ),
            (GitFound::NoGit, "no_git"),
            (
                GitFound::Failed {
                    reason: "x".to_owned(),
                },
                "failed",
            ),
        ] {
            assert_eq!(serde_json::to_value(&git).unwrap()["status"], word);
            assert!(MIRROR.contains(&format!("status: \"{word}\"")), "{word}");
        }
    }

    /// #501's button view keeps its four keys, which the rooms panel reads.
    #[test]
    fn the_open_command_serialises_the_keys_the_mirror_declares() {
        let shipped = knobas_core::checkout::open::shipped_on("macos");
        let view = command_view(&shipped[0], &shipped);
        assert_eq!(view.label, "Open in VS Code");
        assert!(view.is_default);
        let json = serde_json::to_value(view).unwrap();
        assert_eq!(keys(&json), ["action", "is_default", "label", "template"]);
        let body = interface("CheckoutCommand");
        for key in keys(&json) {
            assert!(body.contains(&format!("{key}:")), "CheckoutCommand.{key}");
        }
    }

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
