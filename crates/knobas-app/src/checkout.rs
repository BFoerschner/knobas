//! The checkout a repo or a branch detail shows, and what its buttons run:
//! the clones root, the scan and the per-repo override (issue #499), and the
//! command templates and the spawn (issue #501). v1.5.
//!
//! `CONTEXT.md`, **Checkout**: knobas-owned data about the local disk, never a
//! field of the mirrored repo. The pure halves are
//! [`knobas_core::checkout`](knobas_core::checkout) -- normalising a remote
//! URL to a key, walking the clones root, and turning a command template into
//! an argument vector -- and what is here is everything that needs a database
//! or an operating system: the settings, the override table `0024` added, the
//! resolution order the two of them make, and the process.
//!
//! # The order, and why the override is first
//!
//! 1. **The override**, if this repository has one. A person answering the
//!    question themselves outranks a scan by construction: the cases the
//!    override exists for -- a clone outside the root, a linked worktree -- are
//!    exactly the ones the scan cannot see, so a scan that could overrule it
//!    would only ever overrule it with something worse.
//! 2. **The scan**, if a clones root is set and the repository has a URL to
//!    match against.
//! 3. **Nothing**, which is not a failure: the repo detail then shows *no
//!    checkout* and the clone command to copy.
//!
//! # Nothing here runs git, and nothing it runs comes from the mirror (ADR-0016)
//!
//! No clone, no checkout, no fetch. `clone_command` produces a **string a
//! person copies**; it is never spawned, and nothing here writes to a working
//! tree. [`open`] does start a process -- that is the whole of #501 -- and the
//! rule it keeps is the other half of the ADR: the *program* is a template a
//! person set in settings, and the only value substituted into it is the
//! checkout [`view`] resolved off this disk. No field of the mirrored repo
//! reaches it, which the one-`&Path` signature of
//! `knobas_core::checkout::expand` is what enforces.

use std::path::{Path, PathBuf};

use serde::Serialize;
use sqlx::{PgPool, Row};

use knobas_core::checkout;
use knobas_core::checkout::OpenAction;
use knobas_core::entity::EntityRef;

use crate::IpcError;

// The `knobas.setting` reader, writer and blank rule live in `crate::settings`
// since #503, which put a second feature's string in the same table. They were
// born here; what moved is three functions and no behaviour.
use crate::settings::{read as read_setting, settable, write as write_setting};

/// The `knobas.setting` key holding the clones root.
///
/// In `knobas.setting` rather than in a column of its own: migration `0002`,
/// comment 6, exists for "app-level state that has no other home", and the
/// clones root is one directory path. Migration `0024`'s header names this key
/// so the two halves of spec #491's "one migration" stay findable from either
/// side, and [`the_setting_key_is_the_one_the_migration_names`] pins the
/// spelling.
pub const CLONES_ROOT_KEY: &str = "checkout.clones_root";

/// The kinds a checkout can be asked about.
///
/// A repo has one; a branch shows its repo's, because a branch is a ref inside
/// the same working tree and spec #491 story 29 puts the same buttons on both
/// details. Nothing else does: a checkout is a fact about a clone, and a
/// ticket has no clone.
pub const CHECKOUT_KINDS: [&str; 2] = ["repo", "branch"];

/// How a checkout was arrived at, or that it was not.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FoundBy {
    /// A path this repository's override carries. Set by hand, and it wins.
    Override,
    /// A clone the scan matched under the clones root.
    Scan,
    /// Neither -- *no checkout*, with the clone command to copy.
    Nothing,
}

/// What a repo or branch detail draws for its checkout.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CheckoutView {
    /// The repo entity the answer belongs to: the entity asked about when it
    /// is a repo, its repository when it is a branch, and `None` when a branch
    /// has no repo in the mirror.
    ///
    /// The frontend needs it to *set* an override, which is keyed on the
    /// repository and never on the branch -- a per-branch path would be a
    /// second answer to one question and would go stale the moment somebody
    /// switched branches in that tree.
    pub repo_entity_id: Option<String>,
    /// The repository's own stored URL, as the mirror holds it. `None` where
    /// the adapter reported no page for it, which is the same P5 miss
    /// *Open in browser* answers by being absent.
    pub repo_url: Option<String>,
    /// Where the clone is, if there is one.
    pub path: Option<String>,
    pub found_by: FoundBy,
    /// The configured clones root, so the panel can say *no clones root set*
    /// rather than *no checkout* when that is the actual state.
    pub clones_root: Option<String>,
    /// `git clone <the repo's URL>` -- **text to copy, never run** (ADR-0016).
    /// `None` exactly when [`repo_url`](Self::repo_url) is.
    pub clone_command: Option<String>,
}

/// The clones root, or nothing.
///
/// # Errors
///
/// [`Internal`](crate::IpcErrorCode::Internal) if the read fails.
pub async fn clones_root(pool: &PgPool) -> Result<Option<String>, IpcError> {
    read_setting(pool, CLONES_ROOT_KEY).await
}

/// Set the clones root, or clear it.
///
/// A blank path clears the setting rather than storing an empty string: an
/// empty root would make the scan walk whatever the process's working
/// directory happens to be.
///
/// # Errors
///
/// [`Internal`](crate::IpcErrorCode::Internal) if the write fails.
pub async fn set_clones_root(pool: &PgPool, path: Option<&str>) -> Result<(), IpcError> {
    write_setting(pool, CLONES_ROOT_KEY, settable(path)).await
}

/// Set a repository's checkout path by hand, or clear it back to the scan.
///
/// # Errors
///
/// [`Invalid`](crate::IpcErrorCode::Invalid) if `entity_id` is not an entity
/// id or does not name a repo or a branch;
/// [`NotFound`](crate::IpcErrorCode::NotFound) if the mirror does not hold it,
/// or holds a branch whose repository it does not;
/// [`Internal`](crate::IpcErrorCode::Internal) if the write fails.
pub async fn set_override(
    pool: &PgPool,
    entity_id: &str,
    path: Option<&str>,
) -> Result<(), IpcError> {
    let repo = repo_of(pool, entity_id).await?;
    let repo_id = repo.entity_id.ok_or_else(|| {
        IpcError::not_found(format!(
            "{entity_id} is a branch whose repository is not in the mirror, \
             so there is nothing to set a checkout on"
        ))
    })?;
    match settable(path) {
        Some(value) => {
            sqlx::query(
                "insert into knobas.checkout_override (entity_id, path) values ($1, $2)
                 on conflict (entity_id) do update
                    set path = excluded.path, updated_at = now()",
            )
            .bind(&repo_id)
            .bind(value)
            .execute(pool)
            .await
            .map_err(IpcError::internal)?;
        }
        None => {
            sqlx::query("delete from knobas.checkout_override where entity_id = $1")
                .bind(&repo_id)
                .execute(pool)
                .await
                .map_err(IpcError::internal)?;
        }
    }
    Ok(())
}

/// The checkout for a repo or a branch.
///
/// # Errors
///
/// [`Invalid`](crate::IpcErrorCode::Invalid) if `entity_id` is not an entity
/// id or does not name a repo or a branch;
/// [`NotFound`](crate::IpcErrorCode::NotFound) if the mirror does not hold it;
/// [`Internal`](crate::IpcErrorCode::Internal) for a query failure.
pub async fn view(pool: &PgPool, entity_id: &str) -> Result<CheckoutView, IpcError> {
    let repo = repo_of(pool, entity_id).await?;
    let root = clones_root(pool).await?;

    let stored: Option<String> = match repo.entity_id.as_deref() {
        Some(id) => {
            sqlx::query_scalar("select path from knobas.checkout_override where entity_id = $1")
                .bind(id)
                .fetch_optional(pool)
                .await
                .map_err(IpcError::internal)?
        }
        None => None,
    };

    let (path, found_by) = match (stored, root.as_deref(), repo.url.as_deref()) {
        (Some(path), _, _) => (Some(path), FoundBy::Override),
        (None, Some(root), Some(url)) => match scan_for(root, url).await {
            Some(path) => (Some(path.to_string_lossy().into_owned()), FoundBy::Scan),
            None => (None, FoundBy::Nothing),
        },
        _ => (None, FoundBy::Nothing),
    };

    Ok(CheckoutView {
        repo_entity_id: repo.entity_id,
        clone_command: repo.url.as_deref().map(checkout::clone_command),
        repo_url: repo.url,
        path,
        found_by,
        clones_root: root,
    })
}

/// The scan, off the async runtime.
///
/// `spawn_blocking` because a `read_dir` over a clones root is filesystem work
/// on a runtime that is also serving every other command, and a root on a
/// slow-to-wake external disk would otherwise stall them. A join failure --
/// the runtime shutting down under us -- reads as *no checkout*, the same miss
/// an unreadable directory produces.
async fn scan_for(root: &str, url: &str) -> Option<PathBuf> {
    let root = PathBuf::from(root);
    let url = url.to_owned();
    tokio::task::spawn_blocking(move || checkout::find(Path::new(&root), &url))
        .await
        .ok()
        .flatten()
}

/// The repository a checkout question is about.
struct RepoOf {
    /// `None` only for a branch whose repository the mirror does not hold.
    entity_id: Option<String>,
    url: Option<String>,
}

/// A repo's own row for a repo; its repository's row for a branch.
///
/// # Why a branch finds its repo by id prefix
///
/// Interfaces §4.2 fixes the key grammar per source, and where a source has
/// both kinds the branch's id is the repo's plus a suffix -- Gitea's
/// `gitea:owner/repo` and `gitea:owner/repo@refs/heads/name`. So the repository
/// is the **longest** repo id in the same source that this id starts with,
/// which is one indexed statement and costs no knowledge of how any particular
/// adapter spells a ref.
///
/// `starts_with` and not `like entity_id || '%'`: a `%` or a `_` inside an
/// entity id would be a wildcard in the second, and an id is a source's string
/// rather than one knobas chose.
///
/// The alternative -- reconstructing `owner/repo` out of the branch key -- is
/// what `knobas_app::start_work::queue`'s `BRANCH_BY_NAME` deliberately avoids
/// and for the same reason: it puts one statement in the business of how every
/// adapter spells a key, and the next adapter breaks it silently.
///
/// A branch with no repo in the mirror is **not** an error: the repository may
/// be outside the configured allowlist while a link points into it. It is a
/// view with no repo id, no URL and no clone command, which the panel draws as
/// *no checkout* -- and `set_override` refuses, because there is nothing to key
/// the override on.
///
/// # Why both statements read `sync.item` and not `sync.live_item`
///
/// This is the **fourth** reader to reach past the view, and `CONTEXT.md`'s
/// **Live item** entry names it beside the other three. Exempt from both of
/// the view's halves, and for the detail read's own reason (reader 2): the
/// panel is mounted *inside* `get_entity`'s detail, which is exempt so that a
/// withdrawn or turned-off entity's page still opens and says which. A
/// checkout read that went through the view would answer *not in the mirror* on
/// a page the app can open -- and the clone is still on the disk after the
/// server withdrew the repository or somebody turned its source off, which is
/// exactly when knowing where it is helps. `0024`'s cascade is the
/// same rule from the other side: a tombstoned repo keeps its entity row and
/// therefore its override; only a purge takes it.
/// `a_withdrawn_repo_and_a_turned_off_source_still_answer_their_checkout` pins
/// both halves.
async fn repo_of(pool: &PgPool, entity_id: &str) -> Result<RepoOf, IpcError> {
    let entity = EntityRef::parse(entity_id).map_err(IpcError::invalid)?;
    let id = entity.to_string();

    let row = sqlx::query("select kind, source_id, web_url from sync.item where entity_id = $1")
        .bind(&id)
        .fetch_optional(pool)
        .await
        .map_err(IpcError::internal)?
        .ok_or_else(|| IpcError::not_found(format!("{id} is not in the mirror")))?;

    let kind: String = row.get("kind");
    if !CHECKOUT_KINDS.contains(&kind.as_str()) {
        return Err(IpcError::invalid(format!(
            "a checkout belongs to a repo or a branch, and {id} is a {kind}"
        )));
    }
    if kind == "repo" {
        return Ok(RepoOf {
            entity_id: Some(id),
            url: row.get("web_url"),
        });
    }

    let source_id: String = row.get("source_id");
    let repo = sqlx::query(
        "select entity_id, web_url from sync.item
          where kind = 'repo' and source_id = $1 and starts_with($2, entity_id)
          order by length(entity_id) desc
          limit 1",
    )
    .bind(&source_id)
    .bind(&id)
    .fetch_optional(pool)
    .await
    .map_err(IpcError::internal)?;

    Ok(match repo {
        Some(repo) => RepoOf {
            entity_id: repo.get("entity_id"),
            url: repo.get("web_url"),
        },
        None => RepoOf {
            entity_id: None,
            url: None,
        },
    })
}

// -- the spawn half: what the buttons run (#501, ADR-0016) ------------------

/// The `knobas.setting` key holding one action's command template.
///
/// One row per action under a shared prefix, in the store migration `0002`'s
/// comment 6 keeps for "app-level state that has no other home" -- the clones
/// root's reason exactly, and for the same reason no table: three strings
/// somebody types once are not a relation.
#[must_use]
pub fn command_key(action: OpenAction) -> String {
    format!("checkout.command.{}", action.id())
}

/// One button on the checkout panel, and one field in settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OpenCommandView {
    /// `vscode`, `jetbrains`, `terminal` -- the id `open_checkout` takes.
    pub action: String,
    /// What the button says.
    ///
    /// On the wire rather than in the component, which is why nothing here
    /// pins `CheckoutPanel.svelte` against these three strings the way #499's
    /// `FoundBy` is pinned: the panel draws one button per row of this list
    /// and labels it from this field, so there is no second spelling to drift.
    pub label: String,
    /// The template this action would run: what somebody set, else this
    /// platform's default. `None` is *not configured*, which is every action
    /// off macOS until a template is set.
    pub template: Option<String>,
    /// Whether [`template`](Self::template) is the platform default rather
    /// than a stored row -- so the settings field can say which it is drawing
    /// and offer to go back to it.
    pub is_default: bool,
}

/// Every action, with the command it would run on this machine.
///
/// # Errors
///
/// [`Internal`](crate::IpcErrorCode::Internal) if the read fails.
pub async fn commands(pool: &PgPool) -> Result<Vec<OpenCommandView>, IpcError> {
    let mut out = Vec::with_capacity(checkout::OPEN_ACTIONS.len());
    for action in checkout::OPEN_ACTIONS {
        let stored = read_setting(pool, &command_key(action)).await?;
        out.push(command_view(action, stored, action.default_template()));
    }
    Ok(out)
}

/// One row of [`commands`], given what is stored and what the platform starts
/// with.
///
/// The platform's default is an **argument**, and that is why this is a
/// function at all: `None` for it is *not configured*, which is the state
/// every action is in off macOS, and the gate runs on a Mac. Written inline it
/// would be a branch nothing anywhere could execute.
fn command_view(
    action: OpenAction,
    stored: Option<String>,
    platform_default: Option<&str>,
) -> OpenCommandView {
    let is_default = stored.is_none();
    OpenCommandView {
        action: action.id().to_owned(),
        label: action.label().to_owned(),
        template: stored.or_else(|| platform_default.map(str::to_owned)),
        is_default,
    }
}

/// The template to run, or the refusal a person reads instead.
///
/// The platform's default is an argument for [`command_view`]'s reason: with
/// `None` for it and nothing stored there is no command, and the message that
/// says so is the one spec #491 asks a non-macOS button to carry. On a Mac
/// that state is unreachable through the database, so a test is the only
/// thing that can reach it at all.
fn template_or_refusal(
    action: OpenAction,
    stored: Option<String>,
    platform_default: Option<&str>,
) -> Result<String, IpcError> {
    stored
        .or_else(|| platform_default.map(str::to_owned))
        .ok_or_else(|| {
            IpcError::invalid(format!(
                "{} is not configured on this platform: set a command for it in Settings",
                action.label()
            ))
        })
}

/// Set one action's command template, or clear it back to the platform's.
///
/// The template is checked here, where somebody is typing it, rather than at
/// the button: a refusal that arrives three days later at an editor that did
/// not open is a refusal nobody can act on.
///
/// # Errors
///
/// [`Invalid`](crate::IpcErrorCode::Invalid) for an action that does not exist
/// or a template that cannot be run;
/// [`Internal`](crate::IpcErrorCode::Internal) if the write fails.
pub async fn set_command(
    pool: &PgPool,
    action: &str,
    template: Option<&str>,
) -> Result<(), IpcError> {
    let action = open_action(action)?;
    let template = settable(template);
    if let Some(template) = template {
        checkout::check_template(template)
            .map_err(|error| IpcError::invalid(format!("'{template}' cannot be run: {error}")))?;
    }
    write_setting(pool, &command_key(action), template).await
}

/// Run one action's command at this entity's checkout.
///
/// # The only argument is the checkout path (ADR-0016)
///
/// The value substituted into the template comes from [`view`] -- the
/// override a person typed, or a directory the scan found on this disk -- and
/// `knobas_core::checkout::expand` takes one `&Path` and nothing else, so no
/// field of the mirrored repo can reach a spawned process. That is the whole
/// point of the feature having a settings surface at all: the *program* is
/// what the person chose, and the *argument* is what knobas found.
///
/// # Errors
///
/// [`Invalid`](crate::IpcErrorCode::Invalid) for an unknown action, an address
/// that is not a repo or a branch, an action with no template on this platform,
/// a template that cannot be run, or a program that would not start;
/// [`NotFound`](crate::IpcErrorCode::NotFound) if the mirror does not hold the
/// entity, or it has no checkout on this machine;
/// [`Internal`](crate::IpcErrorCode::Internal) for a query failure.
pub async fn open(pool: &PgPool, entity_id: &str, action: &str) -> Result<(), IpcError> {
    let action = open_action(action)?;
    let checkout = view(pool, entity_id).await?;
    let path = checkout.path.ok_or_else(|| {
        IpcError::not_found(format!(
            "{entity_id} has no checkout on this machine, so there is nothing to open"
        ))
    })?;
    let stored = read_setting(pool, &command_key(action)).await?;
    let template = template_or_refusal(action, stored, action.default_template())?;
    let argv = checkout::expand(&template, Path::new(&path))
        .map_err(|error| IpcError::invalid(format!("'{template}' cannot be run: {error}")))?;
    spawn(&argv)
        .map_err(|error| IpcError::invalid(format!("'{template}' could not be run: {error}")))
}

/// The action with this id, or a refusal naming what was asked for.
fn open_action(action: &str) -> Result<OpenAction, IpcError> {
    OpenAction::from_id(action).ok_or_else(|| {
        let known: Vec<&str> = checkout::OPEN_ACTIONS.iter().map(|a| a.id()).collect();
        IpcError::invalid(format!(
            "'{action}' is not something knobas opens a checkout with; it has {}",
            known.join(", ")
        ))
    })
}

/// Start `argv[0]` with the rest as its arguments, and do not wait for it.
///
/// **No shell.** The expansion answers an argument vector and this passes it
/// straight to `exec`, so a checkout path holding a space, a `$` or a `;` is
/// one argument and stays one, and a template cannot pipe or chain. `sh -c`
/// would undo both properties in one line.
///
/// The three standard streams go to `/dev/null`. An editor that prints on
/// startup would otherwise write into the app's own stdout, and a child that
/// fills a pipe nobody reads blocks for ever.
///
/// The thread exists only to reap. A `Child` that is dropped without being
/// waited on leaves a zombie for the life of the process, and an editor is
/// exactly the kind of program that outlives the click that started it -- so
/// the wait happens on a thread of its own rather than on tokio's blocking
/// pool, where a terminal emulator left open all afternoon would sit on a
/// worker the rest of the IPC surface needs.
fn spawn(argv: &[String]) -> std::io::Result<()> {
    // `expand` refuses an empty template, so a command word is always here;
    // this is the shape that says so without an index that could panic.
    let (program, arguments) = argv
        .split_first()
        .ok_or_else(|| std::io::Error::other("an expanded template has no command in it"))?;
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

    /// The *not configured* state, which no Mac can reach through the database
    /// -- macOS has a default for all three -- and which is what every button
    /// off macOS is in. Reachable here because the platform's default is an
    /// argument rather than a `cfg!`.
    #[test]
    fn an_action_with_no_template_on_this_platform_is_not_configured() {
        let view = command_view(OpenAction::JetBrains, None, None);
        assert_eq!(view.template, None);
        assert!(view.is_default, "nothing is stored, so nothing overrode it");

        let error = template_or_refusal(OpenAction::JetBrains, None, None)
            .expect_err("no template, nothing to run");
        assert_eq!(error.code, crate::IpcErrorCode::Invalid);
        assert!(
            error.message.contains("not configured"),
            "{}",
            error.message
        );
        assert!(
            error.message.contains("Open in JetBrains"),
            "{}",
            error.message
        );
        assert!(error.message.contains("Settings"), "{}", error.message);
    }

    /// And the two states that do have a command: a stored template wins over
    /// the platform's, and the platform's answers when nothing is stored.
    #[test]
    fn a_stored_template_wins_and_the_platforms_answers_when_nothing_is() {
        let stored = command_view(
            OpenAction::VsCode,
            Some("code {path}".to_owned()),
            Some("open -a X {path}"),
        );
        assert_eq!(stored.template.as_deref(), Some("code {path}"));
        assert!(!stored.is_default);
        assert_eq!(
            template_or_refusal(
                OpenAction::VsCode,
                Some("code {path}".to_owned()),
                Some("open -a X {path}")
            )
            .unwrap(),
            "code {path}"
        );

        let fallback = command_view(OpenAction::VsCode, None, Some("open -a X {path}"));
        assert_eq!(fallback.template.as_deref(), Some("open -a X {path}"));
        assert!(fallback.is_default);
    }

    // -- the spawn half (#501) ---------------------------------------------

    /// The settings keys are one string in two places -- here and in whatever
    /// a person reads when they wonder where their template went -- and they
    /// are the only thing standing between an upgrade and three forgotten
    /// commands. Spelled out rather than derived from `command_key`, which is
    /// the function under test.
    #[test]
    fn every_action_stores_its_template_under_its_own_key() {
        let keys: Vec<String> = checkout::OPEN_ACTIONS
            .into_iter()
            .map(command_key)
            .collect();
        assert_eq!(
            keys,
            [
                "checkout.command.vscode",
                "checkout.command.jetbrains",
                "checkout.command.terminal"
            ]
        );
        // And under the clones root's own store, not beside it: a key that
        // collided with `checkout.clones_root` would make one field eat the
        // other.
        assert!(!keys.contains(&CLONES_ROOT_KEY.to_owned()));
    }

    /// Exactly the keys the mirror declares -- a field added on one side only
    /// is invisible to a walk over a hardcoded list.
    #[test]
    fn the_open_command_serialises_the_keys_the_mirror_declares() {
        let json = serde_json::to_value(OpenCommandView {
            action: "vscode".to_owned(),
            label: "Open in VS Code".to_owned(),
            template: None,
            is_default: true,
        })
        .unwrap();
        let mut keys: Vec<String> = json.as_object().unwrap().keys().cloned().collect();
        keys.sort();
        assert_eq!(keys, ["action", "is_default", "label", "template"]);
        // The **interface's own body**, not the whole file: `entity.ts`
        // declares `label:` and `template:` on other shapes too, so a
        // whole-file substring would answer yes for a key `CheckoutCommand`
        // had lost -- a check measuring the file where the claim is about one
        // declaration.
        let mirror = include_str!("../../../app/src/lib/ipc/entity.ts");
        let body = mirror
            .split_once("export interface CheckoutCommand {")
            .expect("entity.ts declares CheckoutCommand")
            .1
            // `"\n}"` and not `'}'`: a doc comment inside the interface
            // carries `{@link openCheckout}`, and splitting on the first
            // brace would cut the body off above the first field.
            .split_once("\n}")
            .expect("the CheckoutCommand interface is closed")
            .0;
        for key in &keys {
            assert!(
                body.contains(&format!("{key}:")),
                "OpenCommandView.{key} is missing from app/src/lib/ipc/entity.ts's CheckoutCommand"
            );
        }
    }

    /// An action nobody has is refused, and the refusal lists the ones there
    /// are -- the message a person reading a log needs, and the one a renamed
    /// id produces.
    #[test]
    fn an_action_that_does_not_exist_is_refused_by_name() {
        let error = open_action("emacs").expect_err("emacs is not an action");
        assert_eq!(error.code, crate::IpcErrorCode::Invalid);
        assert!(error.message.contains("emacs"), "{}", error.message);
        for action in checkout::OPEN_ACTIONS {
            assert!(error.message.contains(action.id()), "{}", error.message);
        }
    }

    /// The setting key is one string in two files, and the migration is the
    /// one a reader looking for the clones root's storage will open.
    #[test]
    fn the_setting_key_is_the_one_the_migration_names() {
        let migration =
            include_str!("../../knobas-db/migrations/0024_the_checkout_and_its_override.sql");
        assert!(
            migration.contains(CLONES_ROOT_KEY),
            "migration 0024 no longer names {CLONES_ROOT_KEY}, so spec #491's \
             \"a settings row for the clones root\" has no record in the schema"
        );
    }

    /// The kinds that have a checkout are one rule in two languages.
    ///
    /// `entity_checkout` refuses anything else **by name**, so a `Detail.svelte`
    /// that mounted the panel on a fourth kind would draw a refusal at a reader
    /// who opened an ordinary ticket, and one that dropped a kind would hide a
    /// checkout that exists. The other two mirrors in this file are pinned
    /// against `entity.ts` the same way; this one is pinned against the
    /// component, because that is where the gate is written.
    #[test]
    fn the_kinds_with_a_checkout_are_the_ones_the_panel_is_mounted_for() {
        let detail = include_str!("../../../app/src/lib/detail/Detail.svelte");
        let gate = detail
            .lines()
            .find(|line| line.contains("const hasCheckout"))
            .expect("Detail.svelte still gates the checkout panel on `hasCheckout`");
        for kind in CHECKOUT_KINDS {
            assert!(
                gate.contains(&format!("\"{kind}\"")),
                "{kind:?} has a checkout here and Detail.svelte does not mount the panel for it: \
                 {gate}"
            );
        }
        // And no more than those: a kind on the component's side alone is a
        // panel that draws `entity_checkout`'s refusal.
        assert_eq!(
            gate.matches("shownKind ===").count(),
            CHECKOUT_KINDS.len(),
            "Detail.svelte gates on a different number of kinds than CHECKOUT_KINDS has: {gate}"
        );
    }

    /// The three states serialise as the words `app/src/lib/ipc/entity.ts`
    /// narrows on. A renamed variant is a panel that draws nothing.
    #[test]
    fn found_by_serialises_as_the_mirror_declares() {
        let mirror = include_str!("../../../app/src/lib/ipc/entity.ts");
        for (state, word) in [
            (FoundBy::Override, "override"),
            (FoundBy::Scan, "scan"),
            (FoundBy::Nothing, "nothing"),
        ] {
            assert_eq!(
                serde_json::to_value(state).unwrap(),
                serde_json::json!(word)
            );
            assert!(
                mirror.contains(&format!("\"{word}\"")),
                "{word:?} is missing from app/src/lib/ipc/entity.ts"
            );
        }
    }

    /// Exactly the keys the mirror declares -- a field added on one side only
    /// is invisible to a walk over a hardcoded list.
    #[test]
    fn the_view_serialises_the_keys_the_mirror_declares() {
        let view = CheckoutView {
            repo_entity_id: Some("gitea:tidewater/payout-service".to_owned()),
            repo_url: Some("https://gitea.example.com/tidewater/payout-service".to_owned()),
            path: None,
            found_by: FoundBy::Nothing,
            clones_root: None,
            clone_command: None,
        };
        let json = serde_json::to_value(&view).unwrap();
        let mut keys: Vec<String> = json.as_object().unwrap().keys().cloned().collect();
        keys.sort();
        assert_eq!(
            keys,
            [
                "clone_command",
                "clones_root",
                "found_by",
                "path",
                "repo_entity_id",
                "repo_url"
            ]
        );
        let mirror = include_str!("../../../app/src/lib/ipc/entity.ts");
        for key in &keys {
            assert!(
                mirror.contains(&format!("{key}:")),
                "CheckoutView.{key} is missing from app/src/lib/ipc/entity.ts"
            );
        }
    }
}
