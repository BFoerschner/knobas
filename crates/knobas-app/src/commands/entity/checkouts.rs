//! Checkouts, the job's conversation, and opening a checkout.

use super::*;

// -- checkouts (#499) -------------------------------------------------------
//
// Here rather than in a module pair of their own for the reason `notify` and
// #290's setting are: a repo and a branch are entities, `get_entity` is in this
// module, and §10.8 freezes the `commands/` + `ipc/` layout. The behaviour is
// `crate::checkout`; these are the thin shims the layout asks for. #1481
// retired the root's own read: the root reaches the app on
// `checkout_settings` and on every `CheckoutView`.

/// Set the clones root, or clear it with a blank one.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) before bring-up,
/// [`IpcErrorCode::Internal`](crate::IpcErrorCode::Internal) if the write
/// fails.
#[tauri::command]
pub async fn set_clones_root(
    lifecycle: State<'_, Lifecycle>,
    path: Option<String>,
) -> Result<(), IpcError> {
    let pool = lifecycle.pool()?;
    crate::checkout::set_clones_root(&pool, path.as_deref()).await
}

/// The checkout a repo or branch detail draws -- the override, else the scan,
/// else *no checkout* and the clone command to copy.
///
/// Read on every open rather than stored: a clone that moved between two opens
/// would leave a cached path pointing at nothing.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) before bring-up,
/// [`IpcErrorCode::Invalid`](crate::IpcErrorCode::Invalid) for an address that
/// is not an entity id or not a repo or branch,
/// [`IpcErrorCode::NotFound`](crate::IpcErrorCode::NotFound) if the mirror
/// does not hold it, [`IpcErrorCode::Internal`](crate::IpcErrorCode::Internal)
/// for a query failure.
#[tauri::command]
pub async fn entity_checkout(
    lifecycle: State<'_, Lifecycle>,
    entity_id: String,
) -> Result<crate::checkout::CheckoutView, IpcError> {
    let pool = lifecycle.pool()?;
    crate::checkout::view(&pool, &entity_id).await
}

/// Set this repository's checkout path by hand, or clear it back to the scan.
///
/// Answers the fresh view rather than nothing, so the panel draws what is now
/// true without a second round trip -- and so clearing an override *shows* the
/// scan taking over, which is the whole of what clearing means.
///
/// Keyed on the repository even when a branch was the address: a per-branch
/// path would be a second answer to one question, stale the moment somebody
/// switched branches in that working tree.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) before bring-up,
/// [`IpcErrorCode::Invalid`](crate::IpcErrorCode::Invalid) for an address that
/// is not an entity id or not a repo or branch,
/// [`IpcErrorCode::NotFound`](crate::IpcErrorCode::NotFound) if the mirror
/// holds neither it nor its repository,
/// [`IpcErrorCode::Internal`](crate::IpcErrorCode::Internal) if the write
/// fails.
#[tauri::command]
pub async fn set_checkout_override(
    lifecycle: State<'_, Lifecycle>,
    entity_id: String,
    path: Option<String>,
) -> Result<crate::checkout::CheckoutView, IpcError> {
    let pool = lifecycle.pool()?;
    crate::checkout::set_override(&pool, &entity_id, path.as_deref()).await?;
    crate::checkout::view(&pool, &entity_id).await
}

/// The uncommitted changes at this entity's checkout (#824, ADR-0026): the
/// unstaged and staged diffs and the untracked paths, read by read-only git
/// when the *Local changes* tab opens.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) before bring-up,
/// [`IpcErrorCode::Invalid`](crate::IpcErrorCode::Invalid) for an address that
/// is not an entity id or not a repo or branch,
/// [`IpcErrorCode::NotFound`](crate::IpcErrorCode::NotFound) if the mirror
/// does not hold it or it has no checkout on this machine,
/// [`IpcErrorCode::Internal`](crate::IpcErrorCode::Internal) for a query
/// failure. What git answered is the read's own arm, never an error.
#[tauri::command]
pub async fn entity_checkout_diff(
    lifecycle: State<'_, Lifecycle>,
    entity_id: String,
) -> Result<crate::checkout::uncommitted::DiffRead, IpcError> {
    let pool = lifecycle.pool()?;
    crate::checkout::local_changes(&pool, &entity_id).await
}

// -- the job's conversation (#819) ------------------------------------------
//
// Two commands, beside the entity reads they belong with (§10.8 freezes the
// `commands/` + `ipc/` layout). The behaviour is `crate::conversation`.

/// Record that the reader opened this ticket's job, and answer when they last
/// did before now, or `null` for a first visit (#819, R-5.5's *last visit*).
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) before bring-up,
/// [`IpcErrorCode::Invalid`](crate::IpcErrorCode::Invalid) for something that
/// is not an entity id, [`IpcErrorCode::NotFound`](crate::IpcErrorCode::NotFound)
/// for one the database does not know,
/// [`IpcErrorCode::Internal`](crate::IpcErrorCode::Internal) for a query
/// failure.
#[tauri::command]
pub async fn visit_entity(
    lifecycle: State<'_, Lifecycle>,
    entity_id: String,
) -> Result<Option<DateTime<Utc>>, IpcError> {
    let pool = lifecycle.pool()?;
    crate::conversation::visit(&pool, &entity_id).await
}

/// A ticket's thread (#819, R-5.2): its comments in the source's order, each
/// marked when it is the reader's own or mentions them, and nothing else
/// (#2163).
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) before bring-up,
/// [`IpcErrorCode::Invalid`](crate::IpcErrorCode::Invalid) for something that
/// is not an entity id, [`IpcErrorCode::NotFound`](crate::IpcErrorCode::NotFound)
/// for one the mirror does not hold,
/// [`IpcErrorCode::Internal`](crate::IpcErrorCode::Internal) for a query
/// failure.
#[tauri::command]
pub async fn entity_thread(
    lifecycle: State<'_, Lifecycle>,
    entity_id: String,
) -> Result<crate::conversation::Thread, IpcError> {
    let pool = lifecycle.pool()?;
    crate::conversation::thread(&pool, lifecycle.registry(), &entity_id).await
}

// -- opening a checkout (#501) ----------------------------------------------
//
// Beside #499's four and for its reason: a repo and a branch are entities, and
// §10.8 freezes the `commands/` + `ipc/` layout. The behaviour is
// `crate::checkout`; these are the shims. #844 adds `checkout_settings` and
// `set_checkout_applications` here, and `open_checkout` takes an application.
// #1481 retired #501's two template commands, which the *Open in…* list had
// replaced and nothing pressed any more.

/// Run an application at this entity's checkout (#501, #844), at a file in it
/// when `file` names one. Answers the argument vector it started.
///
/// `application` is an id from `checkout_settings`, the default when it is
/// `None`. `file` is a path relative to the checkout and must be a file inside
/// it on disk; `line` fills `{line}`.
///
/// The one place knobas starts a program a person chose. The program is a
/// template from `knobas.setting`, never one the webview sends: *Try it* saves
/// the draft and opens it by id (#1643). Every value filled into it is from
/// this disk: the checkout path and a file inside it, never a field of the
/// mirrored repo (ADR-0016). The program inherits no `KNOBAS_*` variable. It
/// returns as soon as the program has started.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) before bring-up,
/// [`IpcErrorCode::Invalid`](crate::IpcErrorCode::Invalid) for an unknown
/// application, an address that is not a repo or a branch, a file that is not
/// inside the checkout, a saved command that cannot be run, or a program that
/// would not start; [`IpcErrorCode::NotFound`](crate::IpcErrorCode::NotFound) if the
/// mirror does not hold the entity or it has no checkout on this machine;
/// [`IpcErrorCode::Internal`](crate::IpcErrorCode::Internal) for a query
/// failure.
#[tauri::command]
pub async fn open_checkout(
    lifecycle: State<'_, Lifecycle>,
    entity_id: String,
    application: Option<String>,
    file: Option<String>,
    line: Option<u32>,
) -> Result<Vec<String>, IpcError> {
    let pool = lifecycle.pool()?;
    crate::checkout::open::open_object(
        &pool,
        Some(lifecycle.registry()),
        &entity_id,
        application.as_deref(),
        &crate::checkout::open::file_object(file.as_deref(), line),
    )
    .await
}

/// Run an application on a code object of this entity's repository (#2274):
/// a repository, a file at a line, a commit, a range or a pull request
/// (`OpenObject`), at its checkout. Answers the argument vector it started.
///
/// `open_checkout`'s rules, through the same runner: the program is a saved
/// application's command for the object's kind, `{sha}`, `{base}`, `{head}`,
/// `{pr}` and `{branch}` are held to a git name's grammar, and `{url}` is
/// `object_page` (ADR-0016, ADR-0034).
///
/// # Errors
///
/// As `open_checkout`, and [`IpcErrorCode::Invalid`](crate::IpcErrorCode::Invalid)
/// for an application with no command for the object or an object value it
/// refuses.
#[tauri::command]
pub async fn open_object(
    lifecycle: State<'_, Lifecycle>,
    entity_id: String,
    application: Option<String>,
    object: knobas_core::checkout::open::Object,
) -> Result<Vec<String>, IpcError> {
    let pool = lifecycle.pool()?;
    crate::checkout::open::open_object(
        &pool,
        Some(lifecycle.registry()),
        &entity_id,
        application.as_deref(),
        &object,
    )
    .await
}

/// The page a code object of this entity's repository has at its source
/// (#2274): what *Open in › Browser* hands the opener. Built from the
/// source's typed base URL and the pages its adapter declares, never from a
/// stored `web_url`. `null` when the source declares none, or a file has no
/// commit or branch to be read at.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) before bring-up,
/// [`IpcErrorCode::Invalid`](crate::IpcErrorCode::Invalid) for an address
/// that is not a repository or a branch or a value refused,
/// [`IpcErrorCode::NotFound`](crate::IpcErrorCode::NotFound) for a repository
/// the mirror does not hold, [`IpcErrorCode::Internal`](crate::IpcErrorCode::Internal)
/// for a query failure.
#[tauri::command]
pub async fn object_page(
    lifecycle: State<'_, Lifecycle>,
    entity_id: String,
    object: knobas_core::checkout::open::Object,
) -> Result<Option<String>, IpcError> {
    let pool = lifecycle.pool()?;
    crate::checkout::open::page(&pool, lifecycle.registry(), &entity_id, &object).await
}

/// What *Open in › Diff tool* would run for `base`..`head` of this entity's
/// repository (#2275): `git difftool --dir-diff --gui --no-prompt` at its
/// checkout, `git` first. `null` when the Diff tool is not offered: no
/// checkout, or an end that is not a sha the checkout holds.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) before bring-up,
/// [`IpcErrorCode::NotFound`](crate::IpcErrorCode::NotFound) for an entity the
/// mirror does not hold, [`IpcErrorCode::Internal`](crate::IpcErrorCode::Internal)
/// for a query failure.
#[tauri::command]
pub async fn diff_tool(
    lifecycle: State<'_, Lifecycle>,
    entity_id: String,
    base: String,
    head: String,
) -> Result<Option<Vec<String>>, IpcError> {
    let pool = lifecycle.pool()?;
    crate::checkout::worktree::diff_tool(&pool, &entity_id, &base, &head).await
}

/// Start the Diff tool on `base`..`head` (#2275). Answers the argument
/// vector it started.
///
/// # Errors
///
/// As [`diff_tool`], and [`IpcErrorCode::Invalid`](crate::IpcErrorCode::Invalid)
/// when it is not offered for this range or git would not start.
#[tauri::command]
pub async fn open_diff_tool(
    lifecycle: State<'_, Lifecycle>,
    entity_id: String,
    base: String,
    head: String,
) -> Result<Vec<String>, IpcError> {
    let pool = lifecycle.pool()?;
    crate::checkout::worktree::open_diff_tool(&pool, &entity_id, &base, &head).await
}

/// The pull request worktrees knobas made under the clones root (#2285,
/// ADR-0035), for Settings › Checkouts.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) before bring-up,
/// [`IpcErrorCode::Internal`](crate::IpcErrorCode::Internal) if a read fails.
#[tauri::command]
pub async fn pull_request_worktrees(
    lifecycle: State<'_, Lifecycle>,
) -> Result<Vec<crate::checkout::worktree::PullRequestWorktree>, IpcError> {
    let pool = lifecycle.pool()?;
    crate::checkout::worktree::list(&pool, lifecycle.registry()).await
}

/// Remove one pull request worktree knobas made (#2285), and answer the list.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) before bring-up,
/// [`IpcErrorCode::NotFound`](crate::IpcErrorCode::NotFound) for a path that
/// is not one, [`IpcErrorCode::Conflict`](crate::IpcErrorCode::Conflict) when
/// git refuses (changes or untracked files in it).
#[tauri::command]
pub async fn remove_pull_request_worktree(
    lifecycle: State<'_, Lifecycle>,
    path: String,
) -> Result<Vec<crate::checkout::worktree::PullRequestWorktree>, IpcError> {
    let pool = lifecycle.pool()?;
    crate::checkout::worktree::remove(&pool, lifecycle.registry(), &path).await
}

/// Settings › Checkouts (#844, R-13a.3): the clones root, the git found, the
/// *Open in…* applications in their order and the default's id. What every
/// *Open in…* menu reads.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) before bring-up,
/// [`IpcErrorCode::Internal`](crate::IpcErrorCode::Internal) if a read fails.
#[tauri::command]
pub async fn checkout_settings(
    lifecycle: State<'_, Lifecycle>,
) -> Result<crate::checkout::CheckoutSettings, IpcError> {
    let pool = lifecycle.pool()?;
    crate::checkout::settings(&pool).await
}

/// Save the *Open in…* applications, in order, and which is the default
/// (#844, R-8.21, R-8.23). Each entry is `{ id?, name, command,
/// file_command? }`; a new one has no id and gets one from its name. Answers
/// the fresh settings.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) before bring-up,
/// [`IpcErrorCode::Invalid`](crate::IpcErrorCode::Invalid) for an entry with
/// no name or a command that cannot be run,
/// [`IpcErrorCode::Internal`](crate::IpcErrorCode::Internal) if the write
/// fails.
#[tauri::command]
pub async fn set_checkout_applications(
    lifecycle: State<'_, Lifecycle>,
    applications: Vec<knobas_core::checkout::open::Draft>,
    default_application: Option<String>,
) -> Result<crate::checkout::CheckoutSettings, IpcError> {
    let pool = lifecycle.pool()?;
    crate::checkout::set_applications(&pool, &applications, default_application.as_deref()).await?;
    crate::checkout::settings(&pool).await
}

/// Clone a repository into the clones root, or stop the clone that is running
/// (#823, ADR-0023).
///
/// One command for both, because ADR-0023 names one: `stop` absent or false
/// starts (answering at once, with progress on `clone:progress`), `stop` true
/// stops and answers once the half-made folder is removed. A repo or a
/// branch; a branch clones its repository.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) before bring-up,
/// and what [`crate::checkout::clone::start`] and
/// [`crate::checkout::clone::stop`] refuse with.
#[tauri::command]
pub async fn clone_repo<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    lifecycle: State<'_, Lifecycle>,
    clones: State<'_, std::sync::Arc<crate::checkout::clone::Clones>>,
    entity_id: String,
    stop: Option<bool>,
) -> Result<crate::checkout::clone::CloneAnswer, IpcError> {
    let pool = lifecycle.pool()?;
    let clones = std::sync::Arc::clone(&clones);
    if stop.unwrap_or(false) {
        return crate::checkout::clone::stop(&pool, &clones, &entity_id).await;
    }
    let events = std::sync::Arc::new(crate::sources::events::TauriEvents::new(app));
    crate::checkout::clone::start(&pool, lifecycle.registry(), &clones, events, &entity_id).await
}
