//! The assets module's commands (issue #428) -- what the Assets view's Tree
//! tab and its fixed pane call.
//!
//! Shims, like every other module here: the decisions are in
//! [`crate::assets`], which is testable without a window. The §10.8 exception
//! this module lands under is the **`time` pair**'s precedent -- an `assets`
//! pair on both sides of the bridge -- and it is where **every** asset command
//! lives, including the route, import and alert commands #432, #439 and M4.1
//! add.
//!
//! `State<'_, Lifecycle>` and `lifecycle.pool()?`, never `State<'_,
//! AppState>`: carry-over §10.6(a), the rule `commands/mod.rs`'s own test
//! enforces for the whole directory.
//!
//! # Why the four writers take an `AppHandle` and the three reads do not
//!
//! Because they announce. Every mutation of an asset is a line in the activity
//! stream (story 11), and the way the shell learns is the `activity:new` event
//! it already watches -- the same signal `create_link`, `unlink` and the timer
//! use. **No event of the estate's own**: a channel of its own would be a
//! second thing to keep in step with the first and would carry no fact the
//! line does not already hold.
//!
//! # And why one of the three takes no state at all
//!
//! `asset_types` (#429) reads a `const`, not a database, so it has no
//! `Lifecycle` to ask and no `Result` to answer with -- the shape `app::ping`
//! and `app_status` already have. It is the reason `tests/it/assets_ipc.rs` has a
//! test of its own beside the registration loop, whose `not_ready` marker only
//! means anything for a command that asks for a pool.

use tauri::State;

use knobas_core::asset::AssetType;

use crate::assets::{
    self, AssetDetail, AssetEdit, AssetRow, DependsOnThis, ImportOutcome, ImportPreview, OpenAlert,
    PropertyValue, RouteDetail, RouteEdit, RouteRow, Visibility,
};
use crate::{IpcError, Lifecycle};

/// Put the lines a mutation wrote on the wire, the way `commands::entity` and
/// `commands::time` do.
///
/// Best-effort by design: `emit` fails only when there is no window to hear
/// it, and an asset that moved has moved whether or not the strip heard.
fn announce<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    lines: Vec<knobas_core::activity::ActivityRow>,
) {
    for line in lines {
        crate::events::emit(app, None, crate::events::ACTIVITY_NEW, &line);
    }
}

/// The built-in asset types: id, label, monogram, the ordered typed-property
/// schema, and the child types conventionally suggested under one.
///
/// **The one command here that takes no state**, because there is nothing to
/// read: the table is a `const` in `knobas_core::asset` and the answer is the
/// same before bring-up as after it. Everything the create dialog and the
/// pane's property editor need to offer a type or an input is in it, and
/// putting it on the wire is what keeps the sixteen types one list rather
/// than one list and a TypeScript copy of it.
///
/// **Answers without a `Result`**, which `app::ping` and `app_status` already
/// do: there is no read to fail and no argument to refuse, and a `Result` that
/// is always `Ok` is an error branch every caller has to write and no test can
/// reach.
#[tauri::command]
pub fn asset_types() -> Vec<AssetType> {
    assets::types()
}

/// One Miller column: what `parentId` holds, or the top of the estate.
///
/// `parentId` omitted or `null` is the **top level**, not "no filter": the
/// estate's roots are the first column, and a read that answered with every
/// asset would draw a first column holding the whole tree.
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) while the database is still
/// coming up, [`Internal`](crate::IpcErrorCode::Internal) if the read fails.
#[tauri::command]
pub async fn asset_tree(
    lifecycle: State<'_, Lifecycle>,
    parent_id: Option<String>,
) -> Result<Vec<AssetRow>, IpcError> {
    let pool = lifecycle.pool()?;
    assets::tree(&pool, parent_id.as_deref()).await
}

/// One asset with everything the pane draws: its properties, its held-by path,
/// what it holds, its history -- and which sources, if any, it could have a
/// monitor created in (issue #453).
///
/// **The `AppHandle` is for that last field alone**, and it is `monitor_roster`'s
/// arrangement for `monitor_roster`'s reason: whether a source offers
/// `create_monitor` is a property of the *instance* and lives in the keychain,
/// which `assets::get` -- a module that reads the database -- has no business
/// opening. Nothing on the wire changes; Tauri supplies the handle.
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) before bring-up,
/// [`NotFound`](crate::IpcErrorCode::NotFound) for an id no asset carries.
#[tauri::command]
pub async fn get_asset<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    lifecycle: State<'_, Lifecycle>,
    asset_id: String,
) -> Result<AssetDetail, IpcError> {
    let pool = lifecycle.pool()?;
    let mut detail = assets::get(&pool, &asset_id).await?;
    detail.monitor_targets = monitor_targets(&app).await?;
    Ok(detail)
}

/// The configured sources a new monitor could be created in, by source id.
///
/// **Narrowed before the keychain is opened.** A source's ops are read out of
/// its credential, and this read runs on every selection in the Tree -- so
/// asking every configured source would be a keychain read per source per
/// click. The narrowing is generic and not a table of adapter kinds: a source
/// is worth asking about when its *kind* either already declares
/// `create_monitor` or declares `accepts_account`, which is the SPI's way of
/// saying "what this instance offers depends on a credential". Every other
/// source's template answer is the whole answer, and it does not name the op.
///
/// The sources state is there whenever the caller's `lifecycle.pool()?` was:
/// bring-up manages it before it installs the pool (IPC-9, #1663), so the
/// `?` below is never the answer a reader gets.
async fn monitor_targets<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
) -> Result<Vec<assets::MonitorTarget>, IpcError> {
    let state = crate::sources::state(app)?;
    let could = worth_asking(&state.registry.descriptors());

    let mut targets = Vec::new();
    for cfg in knobas_sync::config::list(&state.pool)
        .await
        .map_err(IpcError::internal)?
    {
        if !could.contains(&cfg.adapter_kind) {
            continue;
        }
        let ops =
            crate::sources::crud::instance_write_ops(&state.secrets, &state.registry, &cfg).await;
        if let Some(target) = target_of(&cfg, &ops) {
            targets.push(target);
        }
    }
    // By id, so a profile with two Kumas draws the same order every time and
    // the form's default target does not move between reads.
    targets.sort_by(|a, b| a.source_id.cmp(&b.source_id));
    Ok(targets)
}

/// The adapter kinds whose *instances* are worth a keychain read.
///
/// A kind either already declares `create_monitor` in its template -- so every
/// instance of it offers one -- or declares `accepts_account`, which is the
/// SPI's way of saying "what this instance offers depends on a credential".
/// Every other kind's template answer is the whole answer and it does not name
/// the op, so asking its instance would open a keychain to be told what the
/// template already said.
///
/// A rule about the SPI and not a table of adapter kinds: the word *Kuma* does
/// not appear, and an adapter added later joins this set by declaring, not by
/// being listed.
fn worth_asking(
    templates: &[knobas_source::SourceDescriptor],
) -> std::collections::BTreeSet<String> {
    templates
        .iter()
        .filter(|d| d.accepts_account || d.write_ops.iter().any(|op| op == assets::CREATE_MONITOR))
        .map(|d| d.adapter_kind.clone())
        .collect()
}

/// One configured source as a create target, or `None` when this instance does
/// not offer the op.
///
/// **`ops` is the *instance's*, not the kind's**, and that is the whole of the
/// criterion's negative: an Uptime Kuma with only its API key builds fine,
/// declares an empty `write_ops`, and is not a target -- so the pane draws no
/// control rather than one `submit_write` would refuse by name.
///
/// The entity comes from `knobas_source::monitor_target` and never from a
/// format string here: what a create targets is the SPI's to spell.
fn target_of(
    cfg: &knobas_sync::config::SourceConfigRow,
    ops: &[String],
) -> Option<assets::MonitorTarget> {
    ops.iter()
        .any(|op| op == assets::CREATE_MONITOR)
        .then(|| assets::MonitorTarget {
            entity: knobas_source::monitor_target(&cfg.id),
            source_id: cfg.id.clone(),
            display_name: cfg.display_name.clone(),
        })
}

/// Create an asset under `parentId`, or at the top of the estate.
///
/// The id is minted by the backend: creating is not naming (story 18).
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) before bring-up,
/// [`Invalid`](crate::IpcErrorCode::Invalid) for a blank name, an unknown
/// type, or a property that does not fit its declared kind, and
/// [`NotFound`](crate::IpcErrorCode::NotFound) for a parent that is not there.
#[tauri::command]
pub async fn create_asset<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    lifecycle: State<'_, Lifecycle>,
    parent_id: Option<String>,
    type_id: String,
    name: String,
    properties: Option<Vec<(String, PropertyValue)>>,
) -> Result<AssetRow, IpcError> {
    let pool = lifecycle.pool()?;
    let written = assets::create(
        &pool,
        parent_id.as_deref(),
        &type_id,
        &name,
        &properties.unwrap_or_default(),
    )
    .await?;
    announce(&app, written.activity);
    Ok(written.value)
}

/// Apply a list of edits, each one a history line with its old and new value.
///
/// A list rather than one call per field, so that a pane saving three changes
/// saves all three or none. An edit that changes nothing writes no line.
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) before bring-up,
/// [`NotFound`](crate::IpcErrorCode::NotFound) for an id no asset carries, and
/// [`Invalid`](crate::IpcErrorCode::Invalid) for a blank name or a property
/// value that does not fit its declared kind.
#[tauri::command]
pub async fn edit_asset<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    lifecycle: State<'_, Lifecycle>,
    asset_id: String,
    edits: Vec<AssetEdit>,
) -> Result<AssetRow, IpcError> {
    let pool = lifecycle.pool()?;
    let written = assets::edit(&pool, &asset_id, &edits).await?;
    announce(&app, written.activity);
    Ok(written.value)
}

/// Move an asset under another, or to the top of the estate.
///
/// A move that would make a cycle is refused with `invalid`, naming the asset
/// the move would have run into.
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) before bring-up,
/// [`NotFound`](crate::IpcErrorCode::NotFound) for an asset or a parent that
/// is not there, [`Invalid`](crate::IpcErrorCode::Invalid) for a cycle.
#[tauri::command]
pub async fn move_asset<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    lifecycle: State<'_, Lifecycle>,
    asset_id: String,
    new_parent_id: Option<String>,
) -> Result<AssetRow, IpcError> {
    let pool = lifecycle.pool()?;
    let written = assets::move_to(&pool, &asset_id, new_parent_id.as_deref()).await?;
    announce(&app, written.activity);
    Ok(written.value)
}

/// Delete a leaf.
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) before bring-up,
/// [`NotFound`](crate::IpcErrorCode::NotFound) for an id no asset carries, and
/// [`Conflict`](crate::IpcErrorCode::Conflict) for an asset that still holds
/// something.
#[tauri::command]
pub async fn delete_asset<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    lifecycle: State<'_, Lifecycle>,
    asset_id: String,
) -> Result<(), IpcError> {
    let pool = lifecycle.pool()?;
    let written = assets::delete(&pool, &asset_id).await?;
    announce(&app, written.activity);
    Ok(())
}

/// One route, with its own history — what `#/route/<id>` opens on (#432).
///
/// The exposing asset is [`RouteRow::asset_id`] on the answer, which is where
/// the Tree opens: a route is not a surface of its own, it is a line in the
/// pane of the asset that exposes it.
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) before bring-up,
/// [`NotFound`](crate::IpcErrorCode::NotFound) for an id no route carries.
#[tauri::command]
pub async fn get_route<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    lifecycle: State<'_, Lifecycle>,
    route_id: String,
) -> Result<RouteDetail, IpcError> {
    let pool = lifecycle.pool()?;
    let mut detail = assets::get_route(&pool, &route_id).await?;
    detail.monitor_targets = monitor_targets(&app).await?;
    Ok(detail)
}

/// Expose a route on an asset, with or without a target.
///
/// `visibility` omitted is `internal`, which is the safe reading of a route
/// nobody has classified — and the reason the argument is an `Option` rather
/// than a required field on a dialog that would otherwise ask a question
/// before it asks for the URL.
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) before bring-up,
/// [`Invalid`](crate::IpcErrorCode::Invalid) for a blank name, a URL with no
/// scheme or a property nothing could read back, and
/// [`NotFound`](crate::IpcErrorCode::NotFound) for an exposing asset or a
/// target that is not there.
#[expect(
    clippy::too_many_arguments,
    reason = "a route's own fields, flat on the wire like every other command \
              on this surface: `create_asset` takes four of them the same way, \
              and a struct here would be a nested object invented for a lint \
              rather than for a caller"
)]
#[tauri::command]
pub async fn create_route<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    lifecycle: State<'_, Lifecycle>,
    asset_id: String,
    name: String,
    url: String,
    target_id: Option<String>,
    visibility: Option<Visibility>,
    properties: Option<Vec<(String, PropertyValue)>>,
) -> Result<RouteRow, IpcError> {
    let pool = lifecycle.pool()?;
    let written = assets::create_route(
        &pool,
        &asset_id,
        &name,
        &url,
        target_id.as_deref(),
        visibility.unwrap_or_default(),
        &properties.unwrap_or_default(),
    )
    .await?;
    announce(&app, written.activity);
    Ok(written.value)
}

/// Apply a list of edits to a route, each one a history line with its old and
/// new value.
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) before bring-up,
/// [`NotFound`](crate::IpcErrorCode::NotFound) for an id no route carries or a
/// target that is not there, and [`Invalid`](crate::IpcErrorCode::Invalid) for
/// a blank name, a URL with no scheme or a property nothing could read back.
#[tauri::command]
pub async fn edit_route<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    lifecycle: State<'_, Lifecycle>,
    route_id: String,
    edits: Vec<RouteEdit>,
) -> Result<RouteRow, IpcError> {
    let pool = lifecycle.pool()?;
    let written = assets::edit_route(&pool, &route_id, &edits).await?;
    announce(&app, written.activity);
    Ok(written.value)
}

/// Delete a route.
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) before bring-up,
/// [`NotFound`](crate::IpcErrorCode::NotFound) for an id no route carries.
#[tauri::command]
pub async fn delete_route<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    lifecycle: State<'_, Lifecycle>,
    route_id: String,
) -> Result<(), IpcError> {
    let pool = lifecycle.pool()?;
    let written = assets::delete_route(&pool, &route_id).await?;
    announce(&app, written.activity);
    Ok(())
}

/// What breaks if this asset goes down — the pane's *Depends on this* panel
/// (spec §12.2, issue #505).
///
/// A read of its own rather than a field on [`get_asset`]'s answer, and that
/// is the one design decision on this side of the bridge. The pane's read runs
/// on every selection in every Miller column; this walk is a round trip per
/// step of the closure and the panel is one section of the pane a reader
/// scrolls to. Keeping it apart means the tree stays as quick as it was and
/// the panel pays for itself.
///
/// **No `AppHandle`**, for the reason the other reads here have none: it
/// announces nothing and writes nothing.
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) before bring-up,
/// [`NotFound`](crate::IpcErrorCode::NotFound) for an id no asset carries,
/// [`Internal`](crate::IpcErrorCode::Internal) if a read fails.
#[tauri::command]
pub async fn depends_on_this(
    lifecycle: State<'_, Lifecycle>,
    asset_id: String,
) -> Result<DependsOnThis, IpcError> {
    let pool = lifecycle.pool()?;
    assets::depends_on_this(&pool, &asset_id).await
}

/// What Dependencies draws from the top bar (#871): the shared layers, every
/// kind a layer can hold, and every asset, route, repository, pipeline, open
/// pull request and linked ticket with its health and the known relations.
/// See [`assets::dependencies`].
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) before bring-up,
/// [`Internal`](crate::IpcErrorCode::Internal) if a read fails.
#[tauri::command]
pub async fn dependencies(
    lifecycle: State<'_, Lifecycle>,
) -> Result<assets::dependencies::Dependencies, IpcError> {
    let pool = lifecycle.pool()?;
    assets::dependencies::read(&pool, lifecycle.registry()).await
}

/// The shared layers' writes and the saved views of Dependencies (#873,
/// R-11.13, R-11.16). Shims over [`assets::dependencies`], where each one
/// says what it refuses. A layer write answers with every layer, top to
/// bottom; a view write with what it saved, or every view.
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) before bring-up, and the
/// refusals [`assets::dependencies::create_layer`] names.
#[tauri::command]
pub async fn create_dep_layer(
    lifecycle: State<'_, Lifecycle>,
    name: String,
) -> Result<Vec<assets::dependencies::Layer>, IpcError> {
    let pool = lifecycle.pool()?;
    assets::dependencies::create_layer(&pool, &name).await
}

/// Rename a shared layer (#873). See [`assets::dependencies::rename_layer`].
///
/// # Errors
///
/// `not_ready`, `invalid`, `conflict`, `not_found`.
#[tauri::command]
pub async fn rename_dep_layer(
    lifecycle: State<'_, Lifecycle>,
    layer_id: String,
    name: String,
) -> Result<Vec<assets::dependencies::Layer>, IpcError> {
    let pool = lifecycle.pool()?;
    assets::dependencies::rename_layer(&pool, &layer_id, &name).await
}

/// Remove a shared layer (#873). See [`assets::dependencies::delete_layer`].
///
/// # Errors
///
/// `not_ready`, `not_found`.
#[tauri::command]
pub async fn delete_dep_layer(
    lifecycle: State<'_, Lifecycle>,
    layer_id: String,
) -> Result<Vec<assets::dependencies::Layer>, IpcError> {
    let pool = lifecycle.pool()?;
    assets::dependencies::delete_layer(&pool, &layer_id).await
}

/// Reorder the shared layers (#873). See [`assets::dependencies::order_layers`].
///
/// # Errors
///
/// `not_ready`, `invalid`.
#[tauri::command]
pub async fn order_dep_layers(
    lifecycle: State<'_, Lifecycle>,
    layer_ids: Vec<String>,
) -> Result<Vec<assets::dependencies::Layer>, IpcError> {
    let pool = lifecycle.pool()?;
    assets::dependencies::order_layers(&pool, &layer_ids).await
}

/// Put a kind in a layer, or in none (#873). See
/// [`assets::dependencies::place_kind`].
///
/// # Errors
///
/// `not_ready`, `invalid`, `not_found`.
#[tauri::command]
pub async fn place_dep_kind(
    lifecycle: State<'_, Lifecycle>,
    kind: String,
    layer_id: Option<String>,
) -> Result<Vec<assets::dependencies::Layer>, IpcError> {
    let pool = lifecycle.pool()?;
    assets::dependencies::place_kind(&pool, &kind, layer_id.as_deref()).await
}

/// Every saved view of Dependencies and the two that open first (#873).
///
/// # Errors
///
/// `not_ready`, `internal`.
#[tauri::command]
pub async fn dep_views(
    lifecycle: State<'_, Lifecycle>,
) -> Result<assets::dependencies::DepViews, IpcError> {
    let pool = lifecycle.pool()?;
    assets::dependencies::views(&pool).await
}

/// Save a view of Dependencies, new when `viewId` is omitted (#873). See
/// [`assets::dependencies::save_view`].
///
/// # Errors
///
/// `not_ready`, `invalid`, `not_found`.
#[tauri::command]
pub async fn save_dep_view(
    lifecycle: State<'_, Lifecycle>,
    view_id: Option<String>,
    name: String,
    settings: assets::dependencies::DepViewSettings,
) -> Result<assets::dependencies::DepView, IpcError> {
    let pool = lifecycle.pool()?;
    assets::dependencies::save_view(&pool, view_id.as_deref(), &name, &settings).await
}

/// Delete a view of Dependencies (#873). See
/// [`assets::dependencies::delete_view`].
///
/// # Errors
///
/// `not_ready`, `conflict`, `not_found`.
#[tauri::command]
pub async fn delete_dep_view(
    lifecycle: State<'_, Lifecycle>,
    view_id: String,
) -> Result<assets::dependencies::DepViews, IpcError> {
    let pool = lifecycle.pool()?;
    assets::dependencies::delete_view(&pool, &view_id).await
}

/// Choose which view opens from a ticket and which from the top bar (#873).
///
/// # Errors
///
/// `not_ready`, `not_found`.
#[tauri::command]
pub async fn set_dep_view_defaults(
    lifecycle: State<'_, Lifecycle>,
    from_ticket: String,
    from_top: String,
) -> Result<assets::dependencies::DepViews, IpcError> {
    let pool = lifecycle.pool()?;
    assets::dependencies::set_view_defaults(&pool, &from_ticket, &from_top).await
}

/// What importing `file` would do, having written nothing.
///
/// The argument is the file's **text**, not a path: reading a file off the
/// disk is the webview's own `<input type="file">` and the browser's `File`
/// API, so the Import needs neither the dialog plugin nor a filesystem
/// capability, and the harness a frontend test runs in can open a file the
/// same way a person can. The parse is Rust's all the same -- one refusal
/// story, in one place, for a file that is not an estate file.
///
/// **`producer` is the chooser's answer**, not a field of the file (#508). A
/// producer returns an estate file in the checked-in shape (ADR-0015: an
/// importer produces an estate file and is not a source), so the text says
/// nothing about where it came from, and what the planner needs from it is one
/// thing: the **origin key** it declares, which is the Import's second matching
/// rule. `assets::ESTATE_FILE_PRODUCER` is the file a person picked off the
/// disk and declares none.
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) before bring-up, and
/// [`Invalid`](crate::IpcErrorCode::Invalid) for a producer this build does
/// not know, or for a file that is not JSON, carries a key the format does not
/// define, names a type nobody declares, names a parent or a target that is
/// nowhere, whose assets hold each other, or whose origin key matches two
/// assets at once.
#[tauri::command]
pub async fn preview_estate_import(
    lifecycle: State<'_, Lifecycle>,
    file: String,
    producer: String,
) -> Result<ImportPreview, IpcError> {
    let pool = lifecycle.pool()?;
    assets::preview_import(&pool, &file, &producer).await
}

/// Apply the import [`preview_estate_import`] previewed.
///
/// The file is sent again rather than a plan being sent back: the plan is
/// recomputed inside the write's own transaction, so what is applied is what
/// the file says at the moment it is applied and no caller can hand over a
/// plan the reader never saw. `producer` is sent again for the same reason --
/// it is half of what decides the plan.
///
/// **One line is announced**, the per-run summary, though every created asset
/// and every property set writes a line of its own -- the first import of the
/// checked-in estate writes thirty-three, and the status bar's latest-change
/// line wants one sentence about what just happened.
///
/// # Errors
///
/// [`preview_estate_import`]'s, plus
/// [`Internal`](crate::IpcErrorCode::Internal) if a write fails.
#[tauri::command]
pub async fn apply_estate_import<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    lifecycle: State<'_, Lifecycle>,
    file: String,
    producer: String,
) -> Result<ImportOutcome, IpcError> {
    let pool = lifecycle.pool()?;
    let written = assets::apply_import(&pool, &file, &producer).await?;
    announce(&app, written.activity);
    Ok(written.value)
}

/// Run one **importer** and hand back the estate file it produced (#509).
///
/// `CONTEXT.md`, **Importer**: a producer of an estate file from a live system,
/// **not** a [source](knobas_source) (ADR-0015). So this command configures
/// nothing, stores no row, syncs nothing and queues no write; what it answers
/// with is text, and [`preview_estate_import`] and [`apply_estate_import`] are
/// what do anything with it. It is on `commands::assets` and not on a module of
/// its own for the reason the two Import commands are: the estate is what this
/// produces, and the `commands/` + `ipc/` layout is frozen (§10.8).
///
/// # Which importer, and what each of them needs
///
/// **hcloud** needs a token and may need a landing place; **Docker** (#510)
/// needs neither. It spawns the docker CLI under each engine asset's
/// `docker_context` -- a person-typed property, which is ADR-0016's rule -- so
/// there is no credential to ask for, and a container lands under the engine
/// whose context found it (spec #491, story 68), so there is nothing to ask
/// *land under* about. A token sent to it is refused rather than stored under
/// an account nothing would read, and `land_under` is ignored.
///
/// # Three answers, and why an hcloud run can take two calls
///
/// [`Produced::TokenNeeded`] comes back before any request is made, when
/// nothing is stored under this importer's keychain account and the caller sent
/// no token -- so a reader who has never used this importer is asked for one
/// rather than shown a 401 from a call made with nothing. A token that *is*
/// sent is stored **only once the run succeeded**, which is what makes *asked
/// once* true without ever keeping a credential the far end refused.
///
/// [`Produced::LandingNeeded`] comes back when the live system holds servers
/// this estate does not, and the caller has not said where they go. The reader
/// answers once, and the next call carries `land_under`. **The answer may be
/// the top of the estate** -- [`assets::Landing`] with a `parent` of `null`,
/// the spelling `create_asset` and `move_asset` already take -- which is a
/// different thing from the argument being absent, and on an estate with no
/// assets it is the only answer there is. That second call reads
/// the live system again rather than holding the first read: this command owns
/// no state between calls, and a cached answer would be a second place for the
/// estate to be out of date.
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) before bring-up,
/// [`Invalid`](crate::IpcErrorCode::Invalid) for a producer this build does not
/// know or for the estate-file producer (which is chosen from the disk and
/// produces nothing), and whatever the live system's own read classified --
/// [`Unauthorized`](crate::IpcErrorCode::Unauthorized) for a refused token,
/// which is the one fault a person can act on (ADR-0004).
#[tauri::command]
pub async fn produce_estate_file<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    lifecycle: State<'_, Lifecycle>,
    producer: String,
    token: Option<String>,
    land_under: Option<assets::Landing>,
) -> Result<assets::Produced, IpcError> {
    let pool = lifecycle.pool()?;
    let producer = assets::find_producer(&producer)?;
    // No wildcard arm on the importer below (ADR-0006's rule applied to this
    // enum): a producer that reads a live system added later has to be given a
    // run here, rather than falling through to a refusal that reads like a bug.
    let importer = assets::importer_of(producer)?;

    match importer {
        assets::Importer::Hcloud => {
            // The keychain, which is where an importer's credential lives --
            // under its own namespace, so nothing that walks *sources* can
            // reach it (ADR-0015).
            let secrets = &crate::sources::state(&app)?.secrets;
            let Some(credential) = assets::token_for(secrets, producer.id, token).await? else {
                return Ok(assets::Produced::TokenNeeded);
            };

            // The client is `assets::hcloud::client`'s and not built here, so
            // the live recipe and the recorded-shape suite exercise the one the
            // command uses rather than a second one spelled the same way (#509,
            // part 4c of the deputy's ruling of 2026-09-08). Only the base URL
            // differs between the three callers, and only a test passes
            // anything but `hcloud::API`.
            let run = match assets::hcloud::client(assets::hcloud::API, credential.as_str()) {
                Ok(client) => assets::hcloud::produce(&pool, &client, land_under.as_ref()).await,
                Err(refused) => Err(refused),
            };

            // The run's answer goes *through* the store rather than being
            // checked before it, so "a credential the far end refused is never
            // kept" is a property of `remember` that its own tests hold it to,
            // rather than a property of the order these two lines are written
            // in.
            assets::remember(secrets, producer.id, credential, run).await
        }
        // **The keychain is not touched at all on this arm** (#510). The Docker
        // importer spawns the docker CLI, which reads this machine's own
        // `docker context ls` and `~/.ssh/config`; there is no credential to
        // store, so a token sent here is refused by name rather than written
        // under an account nothing would ever read. `land_under` is ignored for
        // the reason story 68 gives -- a container lands under the engine whose
        // context found it -- and saying so is cheaper than a state the dialog
        // can never reach.
        assets::Importer::Docker => {
            if token.is_some_and(|typed| !typed.trim().is_empty()) {
                return Err(IpcError::invalid(
                    "the Docker importer takes no credential: it spawns the \
                     docker CLI, which uses this machine's own docker contexts \
                     and its SSH configuration. Nothing would read a token \
                     stored for it.",
                ));
            }
            assets::docker::produce(&pool, &assets::docker::cli(assets::docker::PROGRAM)).await
        }
    }
}

/// The two numbers monitoring is shaped by (issue #443, spec #427's "Settings
/// keys for the sample retention, the response-time threshold").
///
/// One DTO and two commands rather than four commands, the shape
/// `BackupSchedule` and `set_backup_schedule` have: they are edited on one
/// section of one dialog and saved together, and a surface that could save
/// half of them is a surface that can be interrupted between the halves.
///
/// `u32` because neither is ever negative, and both are clamped into range by
/// `knobas_sync::samples` on the way in and on the way out -- a settings
/// dialog and a hand-edited `knobas.setting` row are both places a number
/// arrives from, and only one of them has a spinner on it. So what a caller
/// posts and what it gets back can differ, and the settings section draws the
/// answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export_to = "assets.ts")]
pub struct MonitoringSettings {
    /// How many days of samples knobas keeps. Default ninety, floor one.
    pub sample_retention_days: u32,
    /// The response time, in milliseconds, above which an otherwise-up
    /// monitor samples as *warn*. Default 1500.
    pub response_time_warn_ms: u32,
}

/// A stored setting narrowed to what the wire carries, saturating.
///
/// `knobas_sync::samples` clamps both into ranges no `u32` conversion can fail
/// from, so the saturation is **unreachable** and is written rather than
/// unwrapped because a panic is the wrong way for a settings *read* to say
/// that a number was out of range.
fn narrowed(value: i64) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

/// What monitoring is set to today.
///
/// Answers the ratified defaults on a database nobody has written a setting
/// to: neither key is inserted by a migration, which is the convention
/// `knobas.setting` has had since `0002` -- the default lives in Rust, beside
/// the reader, and a fresh profile and a profile whose row was deleted give
/// the same answer.
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) before the database is up,
/// [`Internal`](crate::IpcErrorCode::Internal) if a read fails.
#[tauri::command]
pub async fn monitoring_settings(
    lifecycle: State<'_, Lifecycle>,
) -> Result<MonitoringSettings, IpcError> {
    let pool = lifecycle.pool()?;
    read_monitoring(&pool).await
}

/// Change both, and answer with what is now stored.
///
/// The stored values rather than the posted ones, the rule
/// `set_backup_schedule` and `set_passive_attribution` both follow on this
/// surface: the section redraws what the database holds instead of what the
/// click asked for, so a value the clamp moved is shown moved.
///
/// **Both rows move or neither does.** `samples::set_settings` writes them in
/// one transaction, which is what makes the single DTO above honest: a save
/// that stored the retention and then failed on the threshold would be exactly
/// the half-saved surface that DTO exists to rule out.
///
/// **Nothing already sampled is rewritten.** *Warn* is derived at sample time
/// and stored, so a new threshold decides the next poll and leaves every hour
/// already on the Monitors tab's bar as it was recorded. A threshold that
/// redrew history would make the bar a picture of the current setting rather
/// than of what happened.
///
/// # Errors
///
/// [`monitoring_settings`]'s.
#[tauri::command]
pub async fn set_monitoring_settings(
    lifecycle: State<'_, Lifecycle>,
    settings: MonitoringSettings,
) -> Result<MonitoringSettings, IpcError> {
    let pool = lifecycle.pool()?;
    let (days, ms) = knobas_sync::samples::set_settings(
        &pool,
        i64::from(settings.sample_retention_days),
        i64::from(settings.response_time_warn_ms),
    )
    .await
    .map_err(crate::IpcError::from)?;
    Ok(MonitoringSettings {
        sample_retention_days: narrowed(days),
        response_time_warn_ms: narrowed(ms),
    })
}

/// The Monitors tab's roster: every mirrored monitor with its state, its last
/// day of samples, its last check, its uptime, its certificate days, the
/// assets it watches and its page in Uptime Kuma (spec #427 story 68, issue
/// #448).
///
/// **One read for the whole tab**, chips included. The counts on the state
/// chips are counts of these rows, so a command that answered counts as well
/// would be answering a question the caller can only ask by having the list --
/// and two answers that could disagree.
///
/// Takes no room filter: the tab is a destination of its own
/// (`#/assets/monitors`) and the estate is not scoped by the room the reader
/// came from.
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) before the database is up,
/// [`Internal`](crate::IpcErrorCode::Internal) if a read fails.
#[tauri::command]
pub async fn monitor_roster<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    lifecycle: State<'_, Lifecycle>,
) -> Result<Vec<assets::MonitorRow>, IpcError> {
    let pool = lifecycle.pool()?;
    let mut rows = assets::monitor_roster(&pool, lifecycle.registry()).await?;
    let offered = monitor_write_ops(&app, &rows).await?;
    assets::offer_actions(&mut rows, &offered);
    Ok(rows)
}

/// The write ops each source in `rows` offers, for `MonitorRow::actions`
/// (issue #452).
///
/// **Per source and not per row**: one keychain read for a roster of a hundred
/// monitors watched by one Kuma, because the answer is a property of the
/// source. `instance_write_ops` is what asks, and it asks the *instance* --
/// for Uptime Kuma the answer depends on whether that source's keychain item
/// carries an account, so a template descriptor could not give it.
///
/// The sources state is there whenever the caller's `lifecycle.pool()?` was,
/// as for [`monitor_targets`].
///
/// A keychain that refuses reads as a source offering nothing, so its
/// monitors draw no buttons, and that is
/// [`instance_write_ops`](crate::sources::crud::instance_write_ops)' doing
/// rather than this loop's: a locked keychain must not cost the reader the
/// whole Monitors tab, which needs nothing from it but two buttons.
async fn monitor_write_ops<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    rows: &[assets::MonitorRow],
) -> Result<std::collections::HashMap<String, Vec<String>>, IpcError> {
    let state = crate::sources::state(app)?;
    let mut offered = std::collections::HashMap::new();
    for source_id in rows
        .iter()
        .map(|row| row.source_id.as_str())
        .collect::<std::collections::BTreeSet<_>>()
    {
        let Some(cfg) = knobas_sync::config::get(&state.pool, source_id)
            .await
            .map_err(IpcError::internal)?
        else {
            continue;
        };
        let ops =
            crate::sources::crud::instance_write_ops(&state.secrets, &state.registry, &cfg).await;
        offered.insert(source_id.to_owned(), ops);
    }
    Ok(offered)
}

/// The Monitors tab's *Not monitored* roster: every asset with no confirmed
/// `monitored-by` link to a monitor (spec #427 story 68, issue #449).
///
/// **A read of its own and not a field on `monitor_roster`.** The roster is a
/// list of *monitors* and this is a list of *assets*; they have no row in
/// common, and the one shape that could carry both would be an answer whose
/// two halves are read for two different surfaces. It is also the read a
/// reader can have an empty answer to while the other is full, which is the
/// state the tab exists to make visible.
///
/// **No type argument**, although the tab filters by type: sixteen types over
/// an estate a person built by hand is not a set worth paging, the filter's
/// options are the types the answer actually holds -- so the frontend cannot
/// draw them without the whole list -- and a filtered read would be a second
/// answer that could disagree with the counts beside it. The chips' argument
/// on `monitor_roster`, applied to the other list.
///
/// **A read, so no `AppHandle`**: nothing here announces.
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) before the database is up,
/// [`Internal`](crate::IpcErrorCode::Internal) if the read fails.
#[tauri::command]
pub async fn unmonitored_assets(
    lifecycle: State<'_, Lifecycle>,
) -> Result<Vec<assets::UnmonitoredAsset>, IpcError> {
    let pool = lifecycle.pool()?;
    assets::unmonitored_assets(&pool).await
}

/// Every open alert in the estate, newest first (spec #427 stories 57 and 58,
/// issue #444).
///
/// **One read, and every surface counts it rather than asking for a count.**
/// The top strip's number is this list's length and the Assets view's list is
/// this list, which is what makes them one answer: a badge counted by a
/// statement of its own is a badge that can disagree with the list under it.
/// The inbox is the deliberate counterexample and its store says why -- there
/// the count and the stream really are different statements, because the count
/// excludes what is snoozed.
///
/// **A read, so no `AppHandle`**: nothing here announces. An alert opens and
/// closes inside a sync run, and the signal every surface already listens to
/// for that is `sync:state` -- a channel of its own would be a second thing to
/// keep in step, which is the argument `commands::sources::pending_writes`
/// records for the write queue and `inbox.svelte.ts` for the inbox.
///
/// An estate with nothing wrong in it answers with an empty list, which is the
/// read working: the top strip draws no badge at zero, the rule the inbox
/// count follows.
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) while the database is still
/// coming up, [`Internal`](crate::IpcErrorCode::Internal) if a read fails.
#[tauri::command]
pub async fn open_alerts(lifecycle: State<'_, Lifecycle>) -> Result<Vec<OpenAlert>, IpcError> {
    let pool = lifecycle.pool()?;
    assets::open_alerts(&pool).await
}

/// Ack the open alert of one monitor: seen, not fixed (spec #427 story 62,
/// issue #446).
///
/// Clears the reader's inbox item and leaves the alert **open** -- only a
/// return to `up` closes one -- and writes a history line on every asset the
/// monitor watches. `assets::ack_alert` is where the three writes and the one
/// transaction are argued.
///
/// **Takes the monitor, not the alert row's id**, because the inbox item's
/// subject is the monitor entity and `monitor_alert_one_open_idx` makes "the
/// open alert of this monitor" exactly one row or none; #449's cards carry
/// `OpenAlert::monitor_id` and are served by the same argument.
///
/// **An `AppHandle`, unlike the alert *read* beside it**: this one is a
/// mutation and its lines go out on `activity:new`, so the status bar's
/// latest-change line hears about an ack the way it hears about every other
/// estate write.
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) before the database is up,
/// [`NotFound`](crate::IpcErrorCode::NotFound) when that monitor has no open
/// alert -- which is what acking a row that recovered while the reader was
/// looking at it gets -- and
/// [`Internal`](crate::IpcErrorCode::Internal) if a write fails.
#[tauri::command]
pub async fn ack_alert<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    lifecycle: State<'_, Lifecycle>,
    monitor_id: String,
) -> Result<OpenAlert, IpcError> {
    let pool = lifecycle.pool()?;
    let written = assets::ack_alert(&pool, &monitor_id, chrono::Utc::now()).await?;
    announce(&app, written.activity);
    Ok(written.value)
}

async fn read_monitoring(pool: &sqlx::PgPool) -> Result<MonitoringSettings, IpcError> {
    use knobas_sync::samples;
    Ok(MonitoringSettings {
        sample_retention_days: narrowed(samples::retention_days(pool).await?),
        response_time_warn_ms: narrowed(samples::threshold_ms(pool).await?),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIRROR: &str = include_str!("../../../../app/src/lib/ipc/assets.ts");

    /// The commands below are invoked from the mirror by the names they
    /// are registered under, and registered under the names they are declared
    /// with.
    ///
    /// `tests/it/wiring.rs` proves every declared command is in the handler list;
    /// this proves the *frontend* calls them by those names. A typo on either
    /// side is a call that fails only at run time, with "command not found"
    /// and nothing else in the tree noticing.
    #[test]
    fn the_mirror_invokes_the_commands_by_their_registered_names() {
        crate::commands::mirror::assert_mirrors("assets", MIRROR, "app/src/lib/ipc/assets.ts", 20);
    }

    /// **Every producer the chooser can send is one this build declares.**
    ///
    /// `IMPORT_PRODUCERS` in the mirror is the Import dialog's chooser -- the
    /// list of entries a reader picks from -- and each entry's `id` is what
    /// arrives as the `producer` argument. An id `assets::PRODUCERS` does not
    /// carry is an entry that refuses every file the reader chooses under it,
    /// and nothing else in the tree would notice: the two lists are in
    /// different languages and no compiler reads both.
    ///
    /// **One direction only, deliberately.** A producer is declared here before
    /// anything can produce its files -- #508 declared `hcloud`'s origin key a
    /// ticket before #509 gave it a produce command and a chooser entry, and
    /// Docker (spec #491, story 67) is in that state now -- so a producer with
    /// no entry is the expected state and not a fault.
    ///
    /// The ids are read **out of the list** rather than looked for anywhere in
    /// the file, so a `"hcloud"` written in a comment somewhere else in the
    /// mirror cannot make this pass, and the list is asserted to be non-empty
    /// so that a renamed or moved declaration fails as a broken parse instead
    /// of checking nothing. **Non-empty and not a count**: #509 added the
    /// hcloud entry and stream 10 adds the Docker one, and a number here would
    /// go red on the day the chooser grew the entry this test exists to check
    /// -- which is exactly what #509 would have done.
    #[test]
    fn the_chooser_offers_producers_this_build_knows() {
        let at = MIRROR
            .find("export const IMPORT_PRODUCERS")
            .expect("the mirror declares the chooser's entries");
        // `= [` and not the first `[`, which is the type annotation's
        // `ImportProducer[]` and would make this read an empty list.
        let opens = MIRROR[at..].find("= [").expect("the list opens") + at;
        let closes = MIRROR[opens..].find(']').expect("the list closes") + opens;
        let list = &MIRROR[opens..closes];

        let offered: Vec<(&str, bool)> = list
            .match_indices("id: ")
            .map(|(at, keyword)| {
                let rest = &list[at + keyword.len()..];
                let quoted = rest.strip_prefix('"').expect("an id is a string literal");
                let id = &quoted[..quoted.find('"').expect("an unterminated id")];
                // The entry is one object literal, so its `importer:` is
                // whatever appears before the next entry's `id:`.
                let entry = &rest[..rest.find("id: ").unwrap_or(rest.len())];
                let at = entry
                    .find("importer: ")
                    .unwrap_or_else(|| panic!("the chooser's {id:?} declares no `importer`"));
                (id, entry[at..].starts_with("importer: true"))
            })
            .collect();
        assert!(
            !offered.is_empty(),
            "this parse found no producer in the chooser's list; if the \
             declaration moved, fix the parse rather than deleting the check"
        );
        for (id, importer) in offered {
            let declared = assets::PRODUCERS
                .iter()
                .find(|producer| producer.id == id)
                .unwrap_or_else(|| {
                    panic!(
                        "the chooser offers {id:?} and no producer in this build \
                         carries that id, so every file chosen under it is refused"
                    )
                });
            // **And whether it reads a live system**, which is what decides
            // whether the dialog draws a file input or a token and a produce
            // command (#509). A chooser that called the estate file an importer
            // would draw a *Read* button whose every press the backend refuses;
            // one that called hcloud a file would offer an `<input type="file">`
            // for a file nobody has.
            assert_eq!(
                declared.importer.is_some(),
                importer,
                "the chooser and this build disagree about whether {id:?} reads \
                 a live system"
            );
        }
    }

    /// Tauri renames a command's *arguments* to camelCase and leaves struct
    /// fields alone. Both spellings are on this surface at once -- the
    /// `assetId` argument and the `parent_id` field inside the row it answers
    /// with -- and getting either wrong is a call that arrives with the value
    /// missing and no error anywhere. Read off the call's keys, not its text:
    /// a name on the value side, as in `changes: edits`, sends nothing
    /// (#2008).
    #[test]
    fn the_mirror_sends_the_argument_names_tauri_expects() {
        for (call, argument) in [
            ("asset_tree", "parentId"),
            ("get_asset", "assetId"),
            ("create_asset", "typeId"),
            ("create_asset", "parentId"),
            ("edit_asset", "edits"),
            ("move_asset", "newParentId"),
            ("delete_asset", "assetId"),
            ("get_route", "routeId"),
            ("create_route", "assetId"),
            ("create_route", "targetId"),
            ("create_route", "visibility"),
            ("edit_route", "edits"),
            ("delete_route", "routeId"),
            ("preview_estate_import", "file"),
            ("preview_estate_import", "producer"),
            ("apply_estate_import", "file"),
            ("apply_estate_import", "producer"),
            ("produce_estate_file", "producer"),
            ("produce_estate_file", "token"),
            ("produce_estate_file", "landUnder"),
            ("set_monitoring_settings", "settings"),
            ("ack_alert", "monitorId"),
        ] {
            let body = crate::commands::mirror::invocation(MIRROR, call);
            assert!(
                crate::commands::mirror::keys(body).contains(&argument),
                "{call} does not send {argument}: {body}"
            );
        }
    }

    /// The two words the panel walks are two the dialog offers (#505).
    ///
    /// The pin `the_relation_a_source_rooms_tile_reads_is_in_the_frontend_vocabulary`
    /// makes for `monitored-by`, made for the other two the moment they became
    /// `where`-clause words: `assets::depends_on_this` filters on these
    /// spellings, and a reader can only produce a row it will find by picking
    /// one of them out of *Link to…*. Dropping either from `relations.ts`
    /// would leave the panel reading a word nothing in the app can draw, and
    /// nothing on that side of the bridge would notice.
    ///
    /// **The `id`, not the reading.** `runs-on`'s inverse *hosts* is a label
    /// and not a second stored word, so it is the `id:` line that has to be
    /// there; a table offering only "hosts" would store some other spelling.
    #[test]
    fn the_relations_the_panel_walks_are_in_the_frontend_vocabulary() {
        const RELATIONS: &str = include_str!("../../../../app/src/lib/detail/relations.ts");
        for relation in [assets::DEPENDS_ON, assets::RUNS_ON] {
            assert!(
                RELATIONS.contains(&format!("id: \"{relation}\"")),
                "{relation} is not a curated relation, so *Link to…* cannot draw \
                 a link the Depends on this panel would count"
            );
        }
    }

    /// One configured source, as `monitor_targets` reads it.
    fn configured(id: &str, kind: &str, name: &str) -> knobas_sync::config::SourceConfigRow {
        knobas_sync::config::SourceConfigRow {
            id: id.to_owned(),
            adapter_kind: kind.to_owned(),
            display_name: name.to_owned(),
            base_url: "http://127.0.0.1:3001/".to_owned(),
            auth_kind: knobas_sync::config::AuthKind::Method(knobas_source::AuthMethod::ApiToken),
            config: serde_json::json!({}),
            sync_interval_secs: 60,
            enabled: true,
            cursor: None,
            health: knobas_sync::health::CredentialHealth {
                source_id: id.to_owned(),
                state: knobas_sync::health::AuthState::Ok,
                checked_at: None,
                detail: None,
                secret_expires_at: None,
            },
            backoff_until: None,
        }
    }

    /// **The criterion's negative, at the seam that decides it**: the ops read
    /// are the *instance's*, so an Uptime Kuma with only its API key is not a
    /// target and the pane draws no control.
    ///
    /// Which ops a key-only instance declares is the Kuma crate's claim
    /// (`an_account_is_what_gives_an_instance_its_write_ops`) and
    /// `sources_crud.rs`' over a real keychain; what is asserted here is the
    /// one line downstream of them -- that an empty list, or a list of the two
    /// ops #452 added, is not a create target.
    #[test]
    fn only_an_instance_that_offers_the_op_is_a_create_target() {
        let kuma = configured("kuma", "kuma", "Uptime Kuma");
        assert_eq!(target_of(&kuma, &[]), None);
        assert_eq!(
            target_of(
                &kuma,
                &["pause_monitor".to_owned(), "resume_monitor".to_owned()]
            ),
            None,
            "the write half #452 added is not a create"
        );
        assert_eq!(
            target_of(
                &kuma,
                &[
                    "pause_monitor".to_owned(),
                    "resume_monitor".to_owned(),
                    assets::CREATE_MONITOR.to_owned(),
                ]
            ),
            Some(assets::MonitorTarget {
                source_id: "kuma".to_owned(),
                display_name: "Uptime Kuma".to_owned(),
                entity: "kuma:monitors".to_owned(),
            })
        );
        // A second Kuma addresses its **own**: the entity is composed from the
        // source's id, so a create in one never reaches the other.
        assert_eq!(
            target_of(
                &configured("kuma-eu", "kuma", "Kuma EU"),
                &[assets::CREATE_MONITOR.to_owned()]
            )
            .expect("a target")
            .entity,
            "kuma-eu:monitors"
        );
    }

    /// Which *kinds* are worth opening a keychain for, against the **real**
    /// registry.
    ///
    /// The narrowing runs on every selection in the Tree, so being wrong in
    /// one direction costs a keychain read per configured source per click and
    /// in the other costs a control that never appears. Asserted against
    /// `Registry::for_process` rather than against a fixture, because the claim is
    /// about what the compiled-in adapters declare -- and it is the rule that
    /// makes this a statement about the SPI rather than a table of kinds.
    #[test]
    fn only_a_kind_whose_ops_could_include_it_is_worth_a_keychain_read() {
        use knobas_sync::scheduler::AdapterRegistry as _;
        let templates = crate::sources::Registry::for_process().descriptors();
        assert!(
            templates.len() >= 5,
            "the builtin registry lost its adapters: {}",
            templates.len()
        );
        let asking = worth_asking(&templates);
        assert!(
            asking.contains(knobas_source_kuma::ADAPTER_KIND),
            "the one adapter whose ops depend on a credential is not asked: {asking:?}"
        );
        for quiet in templates
            .iter()
            .filter(|d| d.adapter_kind != knobas_source_kuma::ADAPTER_KIND)
        {
            assert!(
                !asking.contains(&quiet.adapter_kind),
                "{:?} is asked, and its template already answers: {:?}",
                quiet.adapter_kind,
                quiet.write_ops
            );
        }
    }

    /// The relation the monitor roster reads is one the dialog can draw
    /// (#435).
    ///
    /// `assets::MONITORED_BY` is the word the roster's statements filter on,
    /// and the estate file's monitor names become links carrying it
    /// (#439). If `relations.ts` did not curate it, the reader could still
    /// type it -- relations are open -- but the link would read `monitored-by`
    /// from **both** ends, since an unknown relation has no inverse to give.
    /// So the two lists are one list, and this is the pin: the frontend table
    /// is where the sentence lives, the backend const is where the key lives,
    /// and neither may lose the other.
    #[test]
    fn the_relation_a_source_rooms_tile_reads_is_in_the_frontend_vocabulary() {
        const RELATIONS: &str = include_str!("../../../../app/src/lib/detail/relations.ts");
        assert!(
            RELATIONS.contains(&format!("id: \"{}\"", assets::MONITORED_BY)),
            "{} is not a curated relation, so it would read the same from both ends",
            assets::MONITORED_BY
        );
        // #445: *Link to…* offers `monitors` on a monitor's own detail and
        // stores this word with the ends swapped, so that one relation covers
        // both ends and every estate read keeps filtering on one word. If the
        // frontend named a different one there, the dialog would draw a link
        // no tile and no pane could see -- and nothing on that side of the
        // bridge would notice.
        assert!(
            RELATIONS.contains(&format!("inverseOf: \"{}\"", assets::MONITORED_BY)),
            "nothing in relations.ts is drawn as the inverse of {}, so linking \
             from a monitor would store some other word",
            assets::MONITORED_BY
        );
    }
}
