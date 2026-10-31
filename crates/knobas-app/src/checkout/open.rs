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
//! # What reaches an argument (ADR-0016, ADR-0034)
//!
//! [`open_object`] fills a command from the checkout `resolve` found on this
//! machine and a file path it has checked is inside that checkout on disk.
//! A code object's own values (`{sha}`, `{base}`, `{head}`, `{pr}`,
//! `{branch}`) are held to the grammar `knobas_core::checkout::open` gives,
//! and `{url}` is [`page`]: the source's typed base URL and the page its
//! adapter declares. No other field of the mirror reaches a spawned process.
//!
//! # Every object, one runner (#2274)
//!
//! A repository, a file, a commit, a range and a pull request all go through
//! [`open_object`] and the one [`spawn`]; an application is offered for an
//! object only when it has a command for it. A pull request's `{path}` is its
//! worktree under the clones root, not the checkout (#2285, ADR-0035,
//! [`super::worktree`]).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use knobas_core::checkout::open::{self as rules, Application, Code, Draft, Fill, Object};
use knobas_core::entity::EntityRef;
use knobas_source::code_pages::{self, Page};
use knobas_sync::scheduler::AdapterRegistry;

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
                ..Application::default()
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
/// argument vector it started. [`open_object`] for a repository or a file,
/// with no page for `{url}`.
///
/// # Errors
///
/// As [`open_object`].
pub async fn open(
    pool: &PgPool,
    entity_id: &str,
    application: Option<&str>,
    file: Option<&str>,
    line: Option<u32>,
) -> Result<Vec<String>, IpcError> {
    open_object(pool, None, entity_id, application, &file_object(file, line)).await
}

/// The object `open_checkout` names: a file when it names one, else the
/// repository.
#[must_use]
pub fn file_object(file: Option<&str>, line: Option<u32>) -> Object {
    match file.map(str::trim).filter(|file| !file.is_empty()) {
        Some(path) => Object::File {
            path: path.to_owned(),
            line,
            at: None,
        },
        None => Object::Repository,
    }
}

/// Run a saved application on `object` of this entity's repository (the
/// default when `application` is `None`), at its checkout. Answers the
/// argument vector it started.
///
/// The program is always a template from `knobas.setting` (ADR-0016): *Try
/// it* saves the draft first and opens it by its id (#1643), so nothing the
/// webview sends names a program. The template is the application's command
/// for the object's kind; an application without one is refused, and the
/// menu never offers it.
///
/// A file is a path relative to the checkout, and it must be a file inside
/// it **on disk**: the words are checked first ([`rules::inside`]), and then
/// the canonical path, which is where a symlink out of the checkout is caught.
/// `{url}` is [`page`] when `registry` is given and the source declares one.
///
/// # Errors
///
/// [`Invalid`](crate::IpcErrorCode::Invalid) for an unknown application, no
/// application at all, one that does not accept the object, a file that is
/// not inside the checkout, a command that cannot be run or an object value
/// it refuses, or a program that would not start;
/// [`NotFound`](crate::IpcErrorCode::NotFound) if the mirror does not hold the
/// entity or it has no checkout on this machine;
/// [`Internal`](crate::IpcErrorCode::Internal) for a query failure, or a launch
/// whose blocking task did not finish.
pub async fn open_object(
    pool: &PgPool,
    registry: Option<&dyn AdapterRegistry>,
    entity_id: &str,
    application: Option<&str>,
    object: &Object,
) -> Result<Vec<String>, IpcError> {
    open_object_admitting(
        pool,
        registry,
        entity_id,
        application,
        object,
        &knobas_core::checkout::clone::PROTOCOLS,
    )
    .await
}

/// [`open_object`], a pull request's worktree fetching over `protocols`: the
/// test-only door to a folder or the testenv's http, as *Clone*'s
/// `start_admitting` is.
///
/// # Errors
///
/// As [`open_object`], and [`super::worktree::open`]'s for a pull request.
pub async fn open_object_admitting(
    pool: &PgPool,
    registry: Option<&dyn AdapterRegistry>,
    entity_id: &str,
    application: Option<&str>,
    object: &Object,
    protocols: &[&str],
) -> Result<Vec<String>, IpcError> {
    let checkout = super::resolve(pool, entity_id).await?;
    let path = checkout.path.ok_or_else(|| {
        IpcError::not_found(format!(
            "{entity_id} has no checkout on this machine, so there is nothing to open"
        ))
    })?;
    let checkout = PathBuf::from(path);
    let (file, line) = match object {
        Object::File { path, line, .. } => (Some(file_inside(&checkout, path).await?), *line),
        _ => (None, None),
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
    let kind = object.kind();
    let template = app
        .command_for(kind)
        .ok_or_else(|| {
            IpcError::invalid(format!(
                "{} has no {}, so it does not open one: add it in Settings › Checkouts",
                app.name,
                kind.command_word()
            ))
        })?
        .to_owned();
    let url = match registry {
        Some(registry) if template.contains("{url}") => {
            page(pool, registry, entity_id, object).await?
        }
        _ => None,
    };
    // A pull request opens in its own worktree, never the person's checkout
    // (#2285, ADR-0035). The template is checked first, so a command that
    // cannot run fetches nothing.
    rules::check_for(&template, kind)
        .map_err(|error| IpcError::invalid(format!("'{template}' cannot be run: {error}")))?;
    let checkout = match (object, registry) {
        (Object::PullRequest { number, item, .. }, Some(registry)) => {
            super::worktree::open(
                pool,
                registry,
                entity_id,
                *number,
                item.as_deref(),
                protocols,
            )
            .await?
        }
        (Object::PullRequest { .. }, None) => {
            return Err(IpcError::invalid(
                "a pull request opens in its worktree, which needs the source's address",
            ));
        }
        _ => checkout,
    };
    let code = Code::of(object, url);
    let argv = rules::fill(
        &template,
        &Fill {
            checkout: &checkout,
            file: file.as_deref(),
            line,
            code: &code,
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

/// The page `object` of this entity's repository has at its source: what
/// *Open in › Browser* opens and `{url}` is (#2274). `None` when the source's
/// adapter declares no code pages, or a file is not read at a commit or
/// branch.
///
/// Built as ADR-0023 builds a clone URL: the instance's typed base URL, the
/// template the adapter declares (`knobas_source::code_pages`), and the
/// values, the repository's `owner/name` checked as *Clone* checks it and the
/// object's held to the grammar its placeholders are, each percent-encoded.
/// The item's stored `web_url` is never read.
///
/// # Errors
///
/// [`Invalid`](crate::IpcErrorCode::Invalid) for an address that is not a
/// repository or a branch, or an object value refused;
/// [`NotFound`](crate::IpcErrorCode::NotFound) if the mirror does not hold the
/// repository or its source is gone;
/// [`Internal`](crate::IpcErrorCode::Internal) for a query failure.
pub async fn page(
    pool: &PgPool,
    registry: &dyn AdapterRegistry,
    entity_id: &str,
    object: &Object,
) -> Result<Option<String>, IpcError> {
    let repo_id = super::repo_of(pool, entity_id)
        .await?
        .entity_id
        .ok_or_else(|| {
            IpcError::not_found(format!(
                "{entity_id} is a branch whose repository is not in the mirror, so it has no page"
            ))
        })?;
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
        IpcError::not_found(format!("{repo_id}'s source is no longer configured"))
    })?;
    let Some(pages) = registry
        .descriptors()
        .into_iter()
        .find(|d| d.adapter_kind == kind)
        .and_then(|d| d.code_pages)
    else {
        return Ok(None);
    };
    let key = EntityRef::parse(&repo_id).map_err(IpcError::invalid)?.key;
    let name = knobas_core::checkout::clone::FullName::parse(&key).map_err(IpcError::invalid)?;
    let repo = format!("{}/{}", name.owner(), name.repo());
    let refuse = |what: &str, value: &str| {
        IpcError::invalid(format!(
            "{what} '{value}' is not a name knobas opens a page for"
        ))
    };
    let rev = |what: &str, value: &str, caret: bool| {
        if rules::is_ref_name(value, caret) {
            Ok(value.to_owned())
        } else {
            Err(refuse(what, value))
        }
    };
    let (which, values): (Page, Vec<(&str, String)>) = match object {
        Object::Repository => (Page::Repository, vec![]),
        Object::File { path, line, at } => {
            let Some(at) = at else { return Ok(None) };
            if rules::inside(Path::new("/"), path).is_none() {
                return Err(refuse("the file", path));
            }
            (
                Page::File,
                vec![
                    ("ref", rev("the commit or branch", at, false)?),
                    ("file", path.trim_start_matches("./").to_owned()),
                    ("line", line.unwrap_or(1).to_string()),
                ],
            )
        }
        Object::Commit { sha } => {
            if !rules::is_sha(sha) {
                return Err(refuse("the commit", sha));
            }
            (Page::Commit, vec![("sha", sha.clone())])
        }
        Object::Range { base, head } => (
            Page::Range,
            vec![
                ("base", rev("the base", base, true)?),
                ("head", rev("the head", head, false)?),
            ],
        ),
        Object::PullRequest {
            number, base, head, ..
        } => (
            Page::PullRequest,
            vec![
                ("pr", number.to_string()),
                ("base", base.clone()),
                ("head", head.clone()),
            ],
        ),
    };
    let mut all: Vec<(&str, &str)> = vec![("repo", repo.as_str())];
    all.extend(values.iter().map(|(k, v)| (*k, v.as_str())));
    Ok(code_pages::page(&base_url, pages.template(which), &all))
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
/// every `KNOBAS_*` variable (#1643, `knobas_db::console::scrub_knobas_env`):
/// a terminal opened at a checkout would otherwise hand `KNOBAS_DB_URL` to
/// every shell started in it.
fn scrubbed_command(
    argv: &[String],
    parent: impl IntoIterator<Item = (std::ffi::OsString, std::ffi::OsString)>,
) -> std::io::Result<std::process::Command> {
    let (program, arguments) = argv
        .split_first()
        .ok_or_else(|| std::io::Error::other("the command has no program in it"))?;
    let mut built = std::process::Command::new(program);
    knobas_db::console::scrub_knobas_env(&mut built, parent).args(arguments);
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
                code: &Code::NONE,
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
