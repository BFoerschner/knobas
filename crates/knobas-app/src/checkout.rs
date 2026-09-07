//! The checkout a repo or a branch detail shows: the clones root, the scan and
//! the per-repo override (issue #499, v1.5).
//!
//! `CONTEXT.md`, **Checkout**: knobas-owned data about the local disk, never a
//! field of the mirrored repo. The finding is
//! [`knobas_core::checkout`](knobas_core::checkout) -- normalising a remote
//! URL to a key and walking the clones root -- and what is here is everything
//! that needs a database: the setting, the override table `0024` added, and
//! the resolution order the two of them make.
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
//! # Nothing here runs git (ADR-0016)
//!
//! No clone, no checkout, no fetch. `clone_command` produces a **string a
//! person copies**; it is never spawned, and the resolution never writes to a
//! working tree. The spawn half -- *Open in editor*, *open a terminal here* --
//! is #501, and it takes its one argument from what this module answers.

use std::path::{Path, PathBuf};

use serde::Serialize;
use sqlx::{PgPool, Row};

use knobas_core::checkout;
use knobas_core::entity::EntityRef;

use crate::IpcError;

/// A path a person typed, cleared to `None` when it is blank.
///
/// One function because both writes here mean the same thing by an empty
/// field -- *forget this* -- and neither may store a blank: an empty clones
/// root would make the scan walk the process's working directory, and a blank
/// override is a row `0024`'s CHECK refuses anyway.
fn settable(path: Option<&str>) -> Option<&str> {
    path.map(str::trim).filter(|value| !value.is_empty())
}

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
    let value: Option<serde_json::Value> =
        sqlx::query_scalar("select value from knobas.setting where key = $1")
            .bind(CLONES_ROOT_KEY)
            .fetch_optional(pool)
            .await
            .map_err(IpcError::internal)?;
    // A row that is not a JSON string is a row an older or a broken knobas
    // wrote; it reads as *no clones root*, which is the miss direction and
    // the one a person can fix from the settings pane.
    Ok(value
        .as_ref()
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
        .filter(|root| !root.trim().is_empty()))
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
    match settable(path) {
        Some(value) => {
            sqlx::query(
                "insert into knobas.setting (key, value) values ($1, $2)
                 on conflict (key) do update set value = excluded.value, updated_at = now()",
            )
            .bind(CLONES_ROOT_KEY)
            .bind(serde_json::Value::String(value.to_owned()))
            .execute(pool)
            .await
            .map_err(IpcError::internal)?;
        }
        None => {
            sqlx::query("delete from knobas.setting where key = $1")
                .bind(CLONES_ROOT_KEY)
                .execute(pool)
                .await
                .map_err(IpcError::internal)?;
        }
    }
    Ok(())
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
            "{entity_id} is a branch whose repository is not in the local index, \
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
/// checkout read that went through the view would answer *not in the local
/// index* on a page the app can open -- and the clone is still on the disk
/// after the server withdrew the repository or somebody turned its source off,
/// which is exactly when knowing where it is helps. `0024`'s cascade is the
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
        .ok_or_else(|| IpcError::not_found(format!("{id} is not in the local index")))?;

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
