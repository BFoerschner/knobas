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
pub mod clone_all;
pub mod fetch;
pub mod local;
pub mod open;
pub mod range;
pub mod stamp;
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
        checkout.tree = Some(working_tree::read_known(Path::new(path)).await);
    }
    Ok(checkout)
}

/// One repo's or branch's checkout in [`views`]' answer.
///
/// The scale pass (PR #2479, 2026-11-10).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct CheckoutOf {
    /// The id it was asked for.
    pub entity_id: String,
    /// What [`view`] answers for it.
    pub checkout: CheckoutView,
}

/// How many working trees [`views`] reads at once.
const TREES_AT_ONCE: usize = 8;

/// [`view`] for each of `entity_ids`, in their order, in one call: what
/// Code asks of every repository it lists, each time it is shown, because a
/// commit or a clone made in a terminal changes a checkout and no sync
/// brings that (the scale pass, 2026-11-10: 65 calls, each with its own walk
/// of the clones root). The clones root is read and walked once here, and a
/// working tree nothing touched since its last read is not read again
/// ([`working_tree::read_known`]). An id [`view`] refuses is left out, as
/// the page leaves a cell empty for a read that failed.
///
/// Every id's repository and override are read in one statement
/// ([`REPOS_OF`], the query audit of PR #2479: two or three statements a
/// repository, 262 for Code's 65), by the rules [`repo_of`] and
/// [`resolve_in`] read one by.
///
/// # Errors
///
/// [`Internal`](crate::IpcErrorCode::Internal) if the clones root cannot be
/// read.
pub async fn views(pool: &PgPool, entity_ids: &[String]) -> Result<Vec<CheckoutOf>, IpcError> {
    let root = clones_root(pool).await?;
    let clones = match root.clone() {
        Some(root) => tokio::task::spawn_blocking(move || checkout::scan(Path::new(&root)))
            .await
            .unwrap_or_default(),
        None => Vec::new(),
    };
    // The ids `repo_of` would read, as it spells them; one that is not an
    // entity id is refused there, and left out here.
    let asked: Vec<(&String, String)> = entity_ids
        .iter()
        .filter_map(|asked| Some((asked, EntityRef::parse(asked).ok()?.to_string())))
        .collect();
    let ids: Vec<&str> = asked.iter().map(|(_, id)| id.as_str()).collect();
    let rows = sqlx::query(REPOS_OF)
        .bind(&ids)
        .fetch_all(pool)
        .await
        .map_err(IpcError::internal)?;
    let found: std::collections::HashMap<String, (RepoOf, Option<String>)> = rows
        .iter()
        .map(|row| {
            (
                row.get::<String, _>("id"),
                (
                    RepoOf {
                        entity_id: row.get("repo_id"),
                        url: row.get("url"),
                    },
                    row.get("override_path"),
                ),
            )
        })
        .collect();
    let mut answers = Vec::with_capacity(asked.len());
    for (entity_id, id) in asked {
        // An id asked twice answers twice, as one call per id did.
        let Some((repo, stored)) = found.get(&id).cloned() else {
            continue;
        };
        answers.push(CheckoutOf {
            entity_id: entity_id.clone(),
            checkout: view_of(repo, stored, root.clone(), Some(&clones)).await,
        });
    }
    let lanes = std::sync::Arc::new(tokio::sync::Semaphore::new(TREES_AT_ONCE));
    let mut reads = tokio::task::JoinSet::new();
    for (at, answer) in answers.iter().enumerate() {
        if let Some(path) = answer.checkout.path.clone() {
            let lanes = lanes.clone();
            reads.spawn(async move {
                let _lane = lanes.acquire_owned().await;
                (at, working_tree::read_known(Path::new(&path)).await)
            });
        }
    }
    while let Some(read) = reads.join_next().await {
        if let Ok((at, tree)) = read {
            answers[at].checkout.tree = Some(tree);
        }
    }
    // A read that did not finish (the runtime stopping) says so rather than
    // nothing.
    for answer in &mut answers {
        if answer.checkout.path.is_some() && answer.checkout.tree.is_none() {
            answer.checkout.tree = Some(TreeRead::Failed {
                reason: "the read did not finish".to_owned(),
            });
        }
    }
    Ok(answers)
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
/// - `source`: the source's compare read, kept by sha pair (ADR-0032), or
///   for one commit its own diff section, kept by sha (#2319).
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
    /// Two branch names read at the checkout between the tips the mirror
    /// holds for them (ADR-0041): when the mirror stored the newer of the
    /// two (RFC 3339). The source may have moved since, so the page checks
    /// it with `entity_source_range_diff` behind the answer. `None` for a
    /// diff between two shas, and for every source read. Absent on the wire
    /// when `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub predicted_at: Option<String>,
}

/// The diff from `base` to `head` of the repository `entity_id` (#2272,
/// ADR-0036): from local read-only git when its checkout holds both commits,
/// with whitespace ignored when asked; otherwise the source's
/// `compare:<base>...<head>` section, through the read cache, and whitespace
/// shown as the source answers it.
///
/// A checkout that lacks a branch tip the mirror names answers through the
/// source, and has that tip fetched behind it (ADR-0041,
/// [`fetch_missing_tip`]); two shas are never fetched.
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
    if let Some(local) = local_diff(pool, entity_id, base, head, ignore_whitespace).await {
        return Ok(local);
    }
    if let Some(predicted) = predicted_diff(
        pool,
        registry.as_ref(),
        entity_id,
        base,
        head,
        ignore_whitespace,
    )
    .await
    {
        return Ok(predicted);
    }
    fetch_missing_tip(pool, registry.as_ref(), entity_id, base, head).await;
    source_range_diff(pool, secrets, registry, live, entity_id, base, head).await
}

/// A checkout that lacks a branch tip the mirror names for a compare has that
/// tip fetched behind the source's answer, as *Files* has (ADR-0041), so the
/// next *New pull request* from a branch pushed elsewhere reads locally. The
/// head first, which is the one a push elsewhere moves; one fetch per checkout
/// at a time ([`fetch::Fetches::behind`]).
async fn fetch_missing_tip(
    pool: &PgPool,
    registry: &dyn knobas_sync::scheduler::AdapterRegistry,
    entity_id: &str,
    base: &str,
    head: &str,
) {
    let Ok(Some(repo)) = repo_of(pool, entity_id).await.map(|r| r.entity_id) else {
        return;
    };
    let Ok(Some(path)) = resolve(pool, &repo).await.map(|c| c.path) else {
        return;
    };
    for name in [head, base] {
        let Ok(Some(tip)) = branch_tip(pool, registry, &repo, name).await else {
            continue;
        };
        if !range::holds_commits(Path::new(&path), &[tip.commit.as_str()]).await {
            fetch::Fetches::global()
                .behind(pool, registry, &repo, &path, &tip.commit)
                .await;
            return;
        }
    }
}

/// The diff from `base` to `head` as the source's compare answers it now,
/// never local: what a page that drew [`range_diff`]'s predicted answer asks
/// behind it, to see whether a branch moved since the mirror's sync
/// (ADR-0041).
///
/// The head the source's compare settled is fetched into a checkout that
/// lacks it, behind the answer ([`fetch_answered_tip`]): a branch the mirror
/// has not synced yet, pushed elsewhere since the page drew, reads at the
/// clone from its next open (the third smoothness pass of PR #2479).
///
/// # Errors
///
/// As [`range_diff`]. [`NotFound`](crate::IpcErrorCode::NotFound) too where
/// the source answers the compare 404, which is a branch it does not hold:
/// the page says so only then, never while a read could still find it.
pub async fn source_range_diff(
    pool: &PgPool,
    secrets: &std::sync::Arc<dyn knobas_secrets::SecretStore>,
    registry: &std::sync::Arc<dyn knobas_sync::scheduler::AdapterRegistry>,
    live: &crate::sources::live::LiveSources,
    entity_id: &str,
    base: &str,
    head: &str,
) -> Result<RangeDiff, IpcError> {
    let section = format!("compare:{base}...{head}");
    let diff =
        match crate::sources::crud::view(pool, secrets, registry, live, entity_id, Some(&section))
            .await
        {
            Ok(view) => diff_of(view),
            Err(crate::sources::SourcesError::Source(error)) if error.status() == Some(404) => {
                return Err(IpcError::not_found(format!(
                    "the source holds no branch {head} or {base} to compare: {error}"
                )));
            }
            Err(error) => return Err(crate::sources::to_ipc(&error, None)),
        }
        .ok_or_else(|| {
            IpcError::not_found(format!(
                "the source draws no diff between {base} and {head}"
            ))
        })?;
    fetch_answered_tip(
        &fetch::Fetches::global(),
        pool,
        registry.as_ref(),
        entity_id,
        &diff,
    )
    .await;
    Ok(diff)
}

/// [`fetch_answered_tip`] through `fetches`, one that admits the testenv's
/// http. **Tests only**, as [`fetch::Fetches::admitting`].
#[cfg(feature = "test-util")]
pub async fn fetch_answered_tip_with(
    fetches: &std::sync::Arc<fetch::Fetches>,
    pool: &PgPool,
    registry: &dyn knobas_sync::scheduler::AdapterRegistry,
    entity_id: &str,
    diff: &RangeDiff,
) {
    fetch_answered_tip(fetches, pool, registry, entity_id, diff).await;
}

/// The head commit a source's compare settled, fetched into the checkout
/// behind the answer where the checkout lacks it (ADR-0041 as amended): the
/// tip of a branch made on the forge after the last sync, which the mirror
/// names no tip for. A checked sha, as every fetched one; nothing else of the
/// answer reaches git.
async fn fetch_answered_tip(
    fetches: &std::sync::Arc<fetch::Fetches>,
    pool: &PgPool,
    registry: &dyn knobas_sync::scheduler::AdapterRegistry,
    entity_id: &str,
    diff: &RangeDiff,
) {
    let Some(range) = diff.range.as_ref() else {
        return;
    };
    if !checkout::open::is_sha(&range.head) {
        return;
    }
    let Ok(Some(repo)) = repo_of(pool, entity_id).await.map(|r| r.entity_id) else {
        return;
    };
    let Ok(Some(path)) = resolve(pool, &repo).await.map(|c| c.path) else {
        return;
    };
    if range::holds_commits(Path::new(&path), &[range.head.as_str()]).await {
        return;
    }
    fetches
        .behind(pool, registry, &repo, &path, &range.head)
        .await;
}

/// The change one commit made (#2319, story 78): `sha` against its first
/// parent, from local read-only git when the checkout of `repo` holds it
/// (whitespace ignored when asked; a root against the empty tree);
/// otherwise the commit entity `commit`'s own `diff` section, through the
/// read cache.
///
/// The source half is the one read the commit tab made before it went
/// local-first, and deliberately not [`range_diff`]'s compare of
/// `<sha>^...<sha>`: Forgejo answers a compare in two requests and refuses
/// it on a private repository, and a root commit's compare in three.
///
/// # Errors
///
/// As [`range_diff`]: what the source's read refuses with, and
/// [`NotFound`](crate::IpcErrorCode::NotFound) when it draws no diff.
#[allow(clippy::too_many_arguments)]
pub async fn commit_diff(
    pool: &PgPool,
    secrets: &std::sync::Arc<dyn knobas_secrets::SecretStore>,
    registry: &std::sync::Arc<dyn knobas_sync::scheduler::AdapterRegistry>,
    live: &crate::sources::live::LiveSources,
    repo: &str,
    commit: &str,
    sha: &str,
    ignore_whitespace: bool,
) -> Result<RangeDiff, IpcError> {
    if let Some(local) = local_diff(pool, repo, &format!("{sha}^"), sha, ignore_whitespace).await {
        return Ok(local);
    }
    source_diff(pool, secrets, registry, live, commit, "diff")
        .await?
        .ok_or_else(|| IpcError::not_found(format!("the source draws no diff for {commit}")))
}

/// [`range_diff`] between two branch names, predicted at the checkout
/// (ADR-0041): each name looked up in the mirror for the tip it last synced
/// (compared there, never passed to git), the source's three-dot compare
/// reproduced as the diff from the two tips' merge base to the head's tip.
/// `None` when either name has no tip in the mirror, there is no checkout,
/// or it lacks either commit.
async fn predicted_diff(
    pool: &PgPool,
    registry: &dyn knobas_sync::scheduler::AdapterRegistry,
    entity_id: &str,
    base: &str,
    head: &str,
    ignore_whitespace: bool,
) -> Option<RangeDiff> {
    let repo = repo_of(pool, entity_id).await.ok()?.entity_id?;
    let path = resolve(pool, &repo).await.ok()?.path?;
    let base_tip = branch_tip(pool, registry, &repo, base).await.ok()??;
    let head_tip = branch_tip(pool, registry, &repo, head).await.ok()??;
    let path = Path::new(&path);
    let from = range::merge_base(path, &base_tip.commit, &head_tip.commit).await?;
    let local = range::read(path, &from, &head_tip.commit, ignore_whitespace).await?;
    let predicted_at = match (base_tip.as_of, head_tip.as_of) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (a, b) => a.or(b),
    };
    Some(RangeDiff {
        files: local.files,
        truncated: local.truncated,
        range: local.base.map(|base| knobas_source::view::DiffRange {
            repository: repo.clone(),
            base,
            head: local.head,
            commits: Vec::new(),
            reviewed: None,
        }),
        read_at: DiffReadAt::Checkout,
        // The mirror stamps every row, so this is always there.
        predicted_at: Some(predicted_at.unwrap_or_default()),
    })
}

/// [`range_diff`]'s local half: `None` when `entity_id` has no checkout or
/// its checkout does not hold both ends ([`range::read`]).
async fn local_diff(
    pool: &PgPool,
    entity_id: &str,
    base: &str,
    head: &str,
    ignore_whitespace: bool,
) -> Option<RangeDiff> {
    let path = resolve(pool, entity_id).await.ok().and_then(|c| c.path)?;
    let local = range::read(Path::new(&path), base, head, ignore_whitespace).await?;
    Some(RangeDiff {
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
        predicted_at: None,
    })
}

/// The diff block of `entity`'s `section`, read through the cache; `None`
/// when the source draws none there. A source that draws no diff has not
/// said "no changes": that is an empty diff block, and `None` a refusal the
/// page shows as one.
async fn source_diff(
    pool: &PgPool,
    secrets: &std::sync::Arc<dyn knobas_secrets::SecretStore>,
    registry: &std::sync::Arc<dyn knobas_sync::scheduler::AdapterRegistry>,
    live: &crate::sources::live::LiveSources,
    entity: &str,
    section: &str,
) -> Result<Option<RangeDiff>, IpcError> {
    let view = crate::sources::crud::view(pool, secrets, registry, live, entity, Some(section))
        .await
        .map_err(|error| crate::sources::to_ipc(&error, None))?;
    Ok(diff_of(view))
}

/// The diff block of a view's first section, as a [`RangeDiff`] the source
/// answered.
fn diff_of(view: Option<knobas_source::view::SourceView>) -> Option<RangeDiff> {
    view.and_then(|view| {
        view.sections.into_iter().next().and_then(|section| {
            section.blocks.into_iter().find_map(|block| match block {
                knobas_source::view::Block::Diff {
                    files,
                    truncated,
                    range,
                    ..
                } => Some(RangeDiff {
                    files,
                    truncated,
                    range,
                    read_at: DiffReadAt::Source,
                    predicted_at: None,
                }),
                _ => None,
            })
        })
    })
}

/// The commit the mirror names for a branch's tree: its declared `commit`
/// as the last sync stored it, and when that was.
///
/// The smoothness pass (PR #2479, ADR-0041).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct MirrorTip {
    /// The whole sha the mirror holds for the branch's tip.
    pub commit: String,
    /// When the mirror stored it (RFC 3339).
    pub as_of: Option<String>,
}

/// The tip the mirror holds for `entity_id`'s tree: a branch's own declared
/// `commit`, or a repository's declared default branch's. `None` where the
/// mirror holds no such branch or its kind declares no commit. Read off the
/// mirror alone: no git, no source.
///
/// # Errors
///
/// [`Internal`](crate::IpcErrorCode::Internal) for a query failure.
pub async fn mirror_tip(
    pool: &PgPool,
    registry: &dyn knobas_sync::scheduler::AdapterRegistry,
    entity_id: &str,
) -> Result<Option<MirrorTip>, IpcError> {
    let declarations = crate::sources::paths::declared_paths(pool, registry).await?;
    let row = sqlx::query(concat!(
        "select i.kind, i.source_id, ",
        knobas_core::declared_string!("$2", "commit"),
        " as commit, ",
        knobas_core::declared_string!("$2", "default_branch"),
        " as default_branch, to_char(i.synced_at at time zone 'UTC', \
           'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') as as_of \
           from sync.live_item i where i.entity_id = $1"
    ))
    .bind(entity_id)
    .bind(declarations.as_param())
    .fetch_optional(pool)
    .await
    .map_err(IpcError::internal)?;
    let Some(row) = row else {
        return Ok(None);
    };
    let kind: String = row.get("kind");
    match kind.as_str() {
        "branch" => Ok(row
            .get::<Option<String>, _>("commit")
            .filter(|sha| checkout::open::is_sha(sha))
            .map(|commit| MirrorTip {
                commit,
                as_of: row.get("as_of"),
            })),
        "repo" => match row.get::<Option<String>, _>("default_branch") {
            Some(name) => branch_tip(pool, registry, entity_id, &name).await,
            None => Ok(None),
        },
        _ => Ok(None),
    }
}

/// The tip the mirror holds for the branch `name` of the repository
/// `repo_id`: the branch item under the repository whose declared `name` is
/// `name`. The name is compared in the mirror and never passed to git.
///
/// # Errors
///
/// [`Internal`](crate::IpcErrorCode::Internal) for a query failure.
pub async fn branch_tip(
    pool: &PgPool,
    registry: &dyn knobas_sync::scheduler::AdapterRegistry,
    repo_id: &str,
    name: &str,
) -> Result<Option<MirrorTip>, IpcError> {
    let declarations = crate::sources::paths::declared_paths(pool, registry).await?;
    let row = sqlx::query(concat!(
        "select ",
        knobas_core::declared_string!("$2", "commit"),
        " as commit, to_char(i.synced_at at time zone 'UTC', \
           'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') as as_of \
           from sync.live_item i \
          where i.kind = 'branch' \
            and i.source_id = (select source_id from sync.item where entity_id = $1) \
            and starts_with(i.entity_id, $1) and i.entity_id <> $1 \
            and (",
        knobas_core::declared_string!("$2", "name"),
        " = $3 or i.title = $3) \
          order by length(i.entity_id) limit 1"
    ))
    .bind(repo_id)
    .bind(declarations.as_param())
    .bind(name)
    .fetch_optional(pool)
    .await
    .map_err(IpcError::internal)?;
    Ok(row.and_then(|row| {
        row.get::<Option<String>, _>("commit")
            .filter(|sha| checkout::open::is_sha(sha))
            .map(|commit| MirrorTip {
                commit,
                as_of: row.get("as_of"),
            })
    }))
}

/// `entity_id`'s whole tree read at its checkout, at the commit the mirror
/// names for it (ADR-0041): what *Files* draws first. `None` where it has no
/// checkout, the mirror names no tip, or the checkout does not hold that
/// commit; then the tab reads the source as before, and a checkout that
/// lacks the commit is fetched into behind it ([`fetch`]), which tells
/// `checkout:fetched` when the tree can be read here.
///
/// # Errors
///
/// [`Invalid`](crate::IpcErrorCode::Invalid) for an address that is not a
/// repo or a branch; [`NotFound`](crate::IpcErrorCode::NotFound) if the mirror
/// does not hold it; [`Internal`](crate::IpcErrorCode::Internal) for a query
/// failure.
pub async fn local_tree(
    pool: &PgPool,
    registry: &dyn knobas_sync::scheduler::AdapterRegistry,
    fetches: Option<&std::sync::Arc<fetch::Fetches>>,
    entity_id: &str,
) -> Result<Option<local::LocalTree>, IpcError> {
    let checkout = resolve(pool, entity_id).await?;
    let Some(path) = checkout.path else {
        return Ok(None);
    };
    let Some(tip) = mirror_tip(pool, registry, entity_id).await? else {
        return Ok(None);
    };
    match local::tree(Path::new(&path), &tip.commit).await {
        Some(tree) => {
            let mut tree = (*tree).clone();
            tree.as_of = tip.as_of;
            Ok(Some(tree))
        }
        None => {
            if let Some(fetches) = fetches {
                fetches
                    .behind(pool, registry, entity_id, &path, &tip.commit)
                    .await;
            }
            Ok(None)
        }
    }
}

/// The file at `file` in `entity_id`'s tree at `commit`, read at its
/// checkout (ADR-0041); `None` where the checkout cannot answer it.
/// `commit` is a sha a [`local_tree`] answered; anything else is `None`.
///
/// # Errors
///
/// As [`local_tree`].
pub async fn local_file(
    pool: &PgPool,
    entity_id: &str,
    commit: &str,
    file: &str,
) -> Result<Option<knobas_source::view::Block>, IpcError> {
    let checkout = resolve(pool, entity_id).await?;
    let Some(path) = checkout.path else {
        return Ok(None);
    };
    Ok(local::file(Path::new(&path), commit, file).await)
}

/// The bytes of `file` in `entity_id`'s tree at `commit`, read at its
/// checkout where it holds that commit (ADR-0041): `None` where it cannot
/// answer, so the source is asked; `Some(Err(size))` for a file over `cap`.
pub async fn local_file_bytes(
    pool: &PgPool,
    entity_id: &str,
    commit: &str,
    file: &str,
    cap: u64,
) -> Option<Result<Vec<u8>, u64>> {
    if !checkout::open::is_sha(commit) {
        return None;
    }
    let path = resolve(pool, entity_id).await.ok()?.path?;
    local::file_bytes(Path::new(&path), commit, file, cap).await
}

/// [`view`] without the working tree read: where the checkout is, and no git.
///
/// What [`open`] asks, because an editor needs the path and not two seconds of
/// `git status` on a large tree before it starts.
pub(crate) async fn resolve(pool: &PgPool, entity_id: &str) -> Result<CheckoutView, IpcError> {
    let root = clones_root(pool).await?;
    resolve_in(pool, entity_id, root, None).await
}

/// [`resolve`] under the clones root `root`, matching against `clones`, the
/// root's scan, where it was walked already ([`views`]); `None` walks it.
async fn resolve_in(
    pool: &PgPool,
    entity_id: &str,
    root: Option<String>,
    clones: Option<&[checkout::FoundClone]>,
) -> Result<CheckoutView, IpcError> {
    let repo = repo_of(pool, entity_id).await?;

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
    Ok(view_of(repo, stored, root, clones).await)
}

/// What [`resolve_in`] answers for `repo`, its stored override `stored`, the
/// clones root `root` and its scan `clones` where it was walked already.
async fn view_of(
    repo: RepoOf,
    stored: Option<String>,
    root: Option<String>,
    clones: Option<&[checkout::FoundClone]>,
) -> CheckoutView {
    let (path, found_by) = match (stored, root.as_deref(), repo.url.as_deref()) {
        (Some(path), _, _) => (Some(path), FoundBy::Override),
        (None, Some(_), Some(url)) if clones.is_some() => {
            match matching(clones.unwrap_or_default(), url) {
                Some(path) => (Some(path.to_string_lossy().into_owned()), FoundBy::Scan),
                None => (None, FoundBy::Nothing),
            }
        }
        (None, Some(root), Some(url)) => match scan_for(root, url).await {
            Some(path) => (Some(path.to_string_lossy().into_owned()), FoundBy::Scan),
            None => (None, FoundBy::Nothing),
        },
        _ => (None, FoundBy::Nothing),
    };

    CheckoutView {
        repo_entity_id: repo.entity_id,
        clone_command: repo.url.as_deref().map(checkout::clone_command),
        repo_url: repo.url,
        path,
        found_by,
        clones_root: root,
        tree: None,
    }
}

/// [`repo_of`] and the override [`resolve_in`] reads, for each id of `$1` at
/// once ([`views`]): an id the mirror holds as a repo or a branch, with its
/// repository's id and address (a branch's the longest repo id of its source
/// it starts with, `null` where none is held) and that repository's stored
/// override. An id not held, or of another kind, has no row.
const REPOS_OF: &str = "
select a.id,
       case when i.kind = 'repo' then i.entity_id else r.entity_id end as repo_id,
       case when i.kind = 'repo' then i.web_url else r.web_url end as url,
       o.path as override_path
  from unnest($1::text[]) as a(id)
  join sync.item i on i.entity_id = a.id
  left join lateral (
         select x.entity_id, x.web_url
           from sync.item x
          where i.kind = 'branch' and x.kind = 'repo' and x.source_id = i.source_id
            and starts_with(a.id, x.entity_id)
          order by length(x.entity_id) desc
          limit 1) r on true
  left join knobas.checkout_override o
         on o.entity_id = case when i.kind = 'repo' then i.entity_id else r.entity_id end
 where i.kind in ('repo', 'branch')
";

/// The first of `clones` (a scan, in its order) whose remote names the same
/// repository as `url`: [`checkout::find`] over a scan already made.
fn matching(clones: &[checkout::FoundClone], url: &str) -> Option<PathBuf> {
    let wanted = checkout::remote_key(url)?;
    clones
        .iter()
        .find(|clone| clone.key == wanted)
        .map(|clone| clone.path.clone())
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
#[derive(Clone)]
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
