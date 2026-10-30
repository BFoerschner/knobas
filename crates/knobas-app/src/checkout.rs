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
//! # Nothing here writes to a repository, and nothing it runs comes from the mirror (ADR-0016)
//!
//! No checkout, no fetch, and nothing here writes to an existing working tree.
//! `clone_command` produces a **string a person copies**; it is never spawned.
//! knobas runs these git commands: [`working_tree`]'s read-only `status` at
//! the checkout path (ADR-0021, #704), [`uncommitted`]'s read-only `diff`
//! there (ADR-0026, #824), [`range`]'s read-only `rev-parse` and `diff`
//! between two checked shas (ADR-0036, #2272), and [`clone`]'s `git clone` into a new folder under
//! the clones root, whose URL is built from the source's address and a checked
//! name and never from the mirror (ADR-0023, #823). [`open`] starts the
//! applications a person set (#501, #844), and the rule it keeps is the other
//! half of the ADR: the *program* is a command a person set in settings, and
//! the only values filled into it are the checkout [`view`] resolved off this
//! disk and a file inside it. No field of the mirrored repo reaches it, which
//! `knobas_core::checkout::open::Fill`, holding paths and a number and
//! nothing else, is what enforces.

use std::path::{Path, PathBuf};

use serde::Serialize;
use sqlx::{PgPool, Row};

use knobas_core::checkout;
use knobas_core::entity::EntityRef;

use crate::IpcError;

pub mod clone;
pub mod open;
pub mod range;
pub mod uncommitted;
pub mod working_tree;
pub mod worktree;

use working_tree::TreeRead;

// The `knobas.setting` reader, writer and blank rule live in `crate::settings`
// since #503, which put a second feature's string in the same table. They were
// born here; what moved is three functions and no behaviour.
use crate::settings::{read as read_setting, settable, write as write_setting};

pub use open::{CheckoutSettings, set_applications, settings};

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
///
/// - `override`: A path this repository's override carries. Set by hand, and it
///   wins.
/// - `scan`: A clone the scan matched under the clones root.
/// - `nothing`: Neither -- *no checkout*, with the clone command to copy.
///
/// Issue #499. `nothing` is a state, not a failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
#[ts(export_to = "entity.ts", rename = "CheckoutFoundBy")]
pub enum FoundBy {
    Override,
    Scan,
    Nothing,
}

/// What a repo or branch detail draws for its checkout.
///
/// Issue #499. A **checkout** (`CONTEXT.md`) is knobas-owned data about the
/// local disk, never a field of the mirrored repo: knobas issues no clone, no
/// checkout and no fetch from this view (ADR-0016).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
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
    /// Where the working tree at [`path`](Self::path) stands, read by
    /// read-only git when the view is asked for (ADR-0021). `None` exactly
    /// when there is no path.
    ///
    /// Issue #704.
    pub tree: Option<TreeRead>,
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
/// directory happens to be. A leading `~` is this user's home, stored spelled
/// out (#846): *Where should clones go?* offers `~/code`, and no program
/// knobas runs expands a tilde, because none of them runs in a shell.
///
/// # Errors
///
/// [`Invalid`](crate::IpcErrorCode::Invalid) if the path is not absolute once
/// `~` is home (#1661): Clone refuses such a root on every click and the scan
/// would read it against the process's working directory, so the one place it
/// is typed refuses it;
/// [`Internal`](crate::IpcErrorCode::Internal) if the write fails.
pub async fn set_clones_root(pool: &PgPool, path: Option<&str>) -> Result<(), IpcError> {
    let home = std::env::var("HOME").ok();
    let path = settable(path).map(|path| expand_home(path, home.as_deref()));
    if let Some(path) = &path
        && !std::path::Path::new(path).is_absolute()
    {
        return Err(IpcError::invalid(format!(
            "the clones root must be an absolute path (or start with ~/), not {path:?}"
        )));
    }
    write_setting(pool, CLONES_ROOT_KEY, path.as_deref()).await
}

/// `~` and `~/...` with `home` put in; anything else, and everything when
/// there is no home, as it is.
fn expand_home(path: &str, home: Option<&str>) -> String {
    match (home, path.strip_prefix('~')) {
        (Some(home), Some(rest)) if rest.is_empty() || rest.starts_with('/') => {
            format!("{}{rest}", home.trim_end_matches('/'))
        }
        _ => path.to_owned(),
    }
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
    let mut checkout = resolve(pool, entity_id).await?;
    if let Some(path) = checkout.path.as_deref() {
        checkout.tree = Some(working_tree::read(Path::new(path)).await);
    }
    Ok(checkout)
}

/// The uncommitted changes at a repo's or a branch's checkout (#824,
/// ADR-0026): what the *Local changes* tab draws.
///
/// A command of its own rather than a field of [`CheckoutView`]: the view is
/// read on every detail open and every window focus, and a diff reads file
/// contents, so it is read when the tab opens and not before.
///
/// # Errors
///
/// [`Invalid`](crate::IpcErrorCode::Invalid) if `entity_id` is not an entity
/// id or does not name a repo or a branch;
/// [`NotFound`](crate::IpcErrorCode::NotFound) if the mirror does not hold it
/// or it has no checkout on this machine;
/// [`Internal`](crate::IpcErrorCode::Internal) for a query failure. What git
/// answered, a failure included, is a [`DiffRead`](uncommitted::DiffRead) and
/// never an error.
pub async fn local_changes(
    pool: &PgPool,
    entity_id: &str,
) -> Result<uncommitted::DiffRead, IpcError> {
    let checkout = resolve(pool, entity_id).await?;
    let path = checkout.path.ok_or_else(|| {
        IpcError::not_found(format!(
            "{entity_id} has no checkout on this machine, so there are no local changes to read"
        ))
    })?;
    Ok(uncommitted::read(Path::new(&path)).await)
}

/// Where a [`RangeDiff`] was read.
///
/// - `checkout`: local read-only git at the repository's checkout, which
///   holds both commits (ADR-0036).
/// - `source`: the source's compare read, kept by sha pair (ADR-0032).
///
/// Issue #2272.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
#[ts(export_to = "entity.ts")]
pub enum DiffReadAt {
    Checkout,
    Source,
}

/// The diff between two commits of a repository, from wherever it was read.
///
/// Issue #2272.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct RangeDiff {
    pub files: Vec<knobas_source::diff::DiffFile>,
    /// A cap cut it (ADR-0020 rule 6).
    pub truncated: bool,
    /// The two whole shas it is between, where they are settled; `None` for
    /// a range from before the repository's first commit, and where the
    /// source did not name them.
    pub range: Option<knobas_source::view::DiffRange>,
    /// Local git or the source. Whitespace is ignored only at `checkout`:
    /// the source's API cannot return a diff with whitespace ignored.
    pub read_at: DiffReadAt,
}

/// The diff from `base` to `head` of the repository `entity_id` (#2272,
/// ADR-0036): from local read-only git when its checkout holds both commits,
/// with whitespace ignored when asked; otherwise the source's
/// `compare:<base>...<head>` section, through the read cache, and whitespace
/// shown as the source answers it.
///
/// A checkout that lacks a commit is never fetched into: the source answers.
///
/// # Errors
///
/// What the source's read refuses with, as [`crate::sources::to_ipc`] maps it;
/// [`NotFound`](crate::IpcErrorCode::NotFound) when the source draws no diff
/// block for the range, which is no answer rather than "no changes".
#[allow(clippy::too_many_arguments)]
pub async fn range_diff(
    pool: &PgPool,
    secrets: &std::sync::Arc<dyn knobas_secrets::SecretStore>,
    registry: &std::sync::Arc<dyn knobas_sync::scheduler::AdapterRegistry>,
    live: &crate::sources::live::LiveSources,
    entity_id: &str,
    base: &str,
    head: &str,
    ignore_whitespace: bool,
) -> Result<RangeDiff, IpcError> {
    if let Some(path) = resolve(pool, entity_id).await.ok().and_then(|c| c.path)
        && let Some(local) = range::read(Path::new(&path), base, head, ignore_whitespace).await
    {
        return Ok(RangeDiff {
            files: local.files,
            truncated: local.truncated,
            range: local.base.map(|base| knobas_source::view::DiffRange {
                repository: entity_id.to_owned(),
                base,
                head: local.head,
                commits: Vec::new(),
                reviewed: None,
            }),
            read_at: DiffReadAt::Checkout,
        });
    }
    let section = format!("compare:{base}...{head}");
    let view = crate::sources::crud::view(pool, secrets, registry, live, entity_id, Some(&section))
        .await
        .map_err(|error| crate::sources::to_ipc(&error, None))?;
    let block = view.and_then(|view| {
        view.sections.into_iter().next().and_then(|section| {
            section.blocks.into_iter().find_map(|block| match block {
                knobas_source::view::Block::Diff {
                    files,
                    truncated,
                    range,
                    ..
                } => Some((files, truncated, range)),
                _ => None,
            })
        })
    });
    // A source that draws no diff here has not said "no changes": that is
    // an empty diff block, and this a refusal the page shows as one.
    let (files, truncated, range) = block.ok_or_else(|| {
        IpcError::not_found(format!(
            "the source draws no diff between {base} and {head}"
        ))
    })?;
    Ok(RangeDiff {
        files,
        truncated,
        range,
        read_at: DiffReadAt::Source,
    })
}

/// [`view`] without the working tree read: where the checkout is, and no git.
///
/// What [`open`] asks, because an editor needs the path and not two seconds of
/// `git status` on a large tree before it starts.
pub(crate) async fn resolve(pool: &PgPool, entity_id: &str) -> Result<CheckoutView, IpcError> {
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
        tree: None,
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A leading tilde is the home directory; a tilde anywhere else, or a
    /// `~user`, is a name like any other.
    #[test]
    fn a_leading_tilde_is_the_home_directory() {
        let home = Some("/Users/mara/");
        assert_eq!(expand_home("~/code", home), "/Users/mara/code");
        assert_eq!(expand_home("~", home), "/Users/mara");
        assert_eq!(expand_home("~bob/code", home), "~bob/code");
        assert_eq!(expand_home("/src/~/x", home), "/src/~/x");
        assert_eq!(expand_home("~/code", None), "~/code");
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
}
