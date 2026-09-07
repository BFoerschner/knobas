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
//! and `app_status` already have. It is the reason `tests/assets_ipc.rs` has a
//! test of its own beside the registration loop, whose `not_ready` marker only
//! means anything for a command that asks for a pool.

use tauri::{Emitter, State};

use knobas_core::asset::AssetType;

use crate::assets::{
    self, AssetDetail, AssetEdit, AssetRow, ImportOutcome, ImportPreview, MemberAsset, OpenAlert,
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
        if let Err(error) = app.emit(crate::events::ACTIVITY_NEW, &line) {
            tracing::warn!(%error, verb = %line.verb, "an asset activity line was not announced");
        }
    }
}

/// The built-in asset types: id, label, monogram, the ordered typed-property
/// schema, and the child types conventionally suggested under one.
///
/// **The one command here that takes no state**, because there is nothing to
/// read: the table is a `const` in `knobas_core::asset` and the answer is the
/// same before bring-up as after it. Everything the create dialog and the
/// pane's property editor need to offer a type or an input is in it, and
/// putting it on the wire is what keeps the nineteen types one list rather
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

/// The `WriteOp` identifier the pane's *Create monitor for this asset* queues.
///
/// Spelled out beside `assets::PAUSE_MONITOR` and `assets::RESUME_MONITOR` and
/// for the same two reasons: what a *surface* offers is a choice made here,
/// and `knobas-sync`'s `write_choke_point` proves there is one outbound write
/// path by finding every file under `crates/*/src/` that names the SPI's write
/// op -- a read that decides which button to draw is not one and must not look
/// like one. `tests/assets_ipc.rs` holds this spelling to `WriteOp::identifier`
/// from a file that scan does not read.
const CREATE_MONITOR: &str = "create_monitor";

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
/// **An empty list rather than a refusal** when the sync engine has not
/// started, `monitor_write_ops`' rule: the pane draws during bring-up, and a
/// moment when knobas cannot say which sources can write is a moment for no
/// button rather than for no pane.
async fn monitor_targets<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
) -> Result<Vec<assets::MonitorTarget>, IpcError> {
    let Ok(state) = crate::sources::state(app) else {
        return Ok(Vec::new());
    };
    let templates = state.registry.descriptors();
    let could: std::collections::BTreeSet<&str> = templates
        .iter()
        .filter(|d| d.accepts_account || d.write_ops.iter().any(|op| op == CREATE_MONITOR))
        .map(|d| d.adapter_kind.as_str())
        .collect();

    let mut targets = Vec::new();
    for cfg in knobas_sync::config::list(&state.pool)
        .await
        .map_err(IpcError::internal)?
    {
        if !could.contains(cfg.adapter_kind.as_str()) {
            continue;
        }
        let ops =
            crate::sources::crud::instance_write_ops(&state.secrets, state.registry.as_ref(), &cfg)
                .await;
        if ops.iter().any(|op| op == CREATE_MONITOR) {
            targets.push(assets::MonitorTarget {
                roster: knobas_source::monitor_roster(&cfg.id),
                source_id: cfg.id,
                display_name: cfg.display_name,
            });
        }
    }
    // By id, so a profile with two Kumas draws the same order every time and
    // the form's default target does not move between reads.
    targets.sort_by(|a, b| a.source_id.cmp(&b.source_id));
    Ok(targets)
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

/// The assets a stored room's Assets tile draws: this context's member assets,
/// worst health first, each with the path it sits at (#434).
///
/// The membership rule is `knobas_core::context::member_ids`' -- the same one
/// statement `context_members` answers with and the per-context inbox filter
/// intersects on -- so an asset reached through its ancestors is in this list
/// and in that badge by construction, and there is no second walk to keep in
/// step.
///
/// A context that does not exist answers with no assets rather than an error,
/// like `context_members`: an address can outlive the thing it names.
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) while the database is still
/// coming up, [`Internal`](crate::IpcErrorCode::Internal) if a read fails.
#[tauri::command]
pub async fn context_assets(
    lifecycle: State<'_, Lifecycle>,
    ctx_id: String,
) -> Result<Vec<MemberAsset>, IpcError> {
    let pool = lifecycle.pool()?;
    assets::in_context(&pool, &ctx_id).await
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
pub async fn get_route(
    lifecycle: State<'_, Lifecycle>,
    route_id: String,
) -> Result<RouteDetail, IpcError> {
    let pool = lifecycle.pool()?;
    assets::get_route(&pool, &route_id).await
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

/// The assets a **source** room's Assets tile draws: the ones this source's
/// monitors are attached to, worst health first (#435).
///
/// The rule is spec #427's *"a source room lists assets with a monitored-by
/// link to that source's monitors"*, and it is [`assets::monitored_by`]'s one
/// statement. It answers with nothing until the Kuma adapter lands in M4.1 --
/// no source emits a `monitor` yet -- which is the read working rather than a
/// stub standing in for it.
///
/// A source id nothing was ever synced under answers with no assets rather
/// than an error, like [`context_assets`]: a room can outlive its source.
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) while the database is still
/// coming up, [`Internal`](crate::IpcErrorCode::Internal) if a read fails.
#[tauri::command]
pub async fn source_assets(
    lifecycle: State<'_, Lifecycle>,
    source_id: String,
) -> Result<Vec<MemberAsset>, IpcError> {
    let pool = lifecycle.pool()?;
    assets::monitored_by(&pool, &source_id).await
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
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) before bring-up, and
/// [`Invalid`](crate::IpcErrorCode::Invalid) for a file that is not JSON,
/// carries a key the format does not define, names a type nobody declares,
/// names a parent or a target that is nowhere, or whose assets hold each
/// other.
#[tauri::command]
pub async fn preview_estate_import(
    lifecycle: State<'_, Lifecycle>,
    file: String,
) -> Result<ImportPreview, IpcError> {
    let pool = lifecycle.pool()?;
    assets::preview_import(&pool, &file).await
}

/// Apply the import [`preview_estate_import`] previewed.
///
/// The file is sent again rather than a plan being sent back: the plan is
/// recomputed inside the write's own transaction, so what is applied is what
/// the file says at the moment it is applied and no caller can hand over a
/// plan the reader never saw.
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
) -> Result<ImportOutcome, IpcError> {
    let pool = lifecycle.pool()?;
    let written = assets::apply_import(&pool, &file).await?;
    announce(&app, written.activity);
    Ok(written.value)
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
/// arrives from, and only one of them has a spinner on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct MonitoringSettings {
    /// How many days of samples knobas keeps. Default ninety.
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
/// Takes no room filter, unlike `context_assets` and `source_assets`: the tab
/// is a destination of its own (`#/assets/monitors`) and the estate is not
/// scoped by the room the reader came from.
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
    let mut rows = assets::monitor_roster(&pool).await?;
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
/// **An empty answer before the sync engine is up**, rather than a refusal.
/// The roster itself needs only the pool, so the tab draws during bring-up;
/// what it does not have then is a keychain, and a tab with no buttons is the
/// right drawing for a moment when knobas cannot say which buttons there are.
///
/// A keychain that *is* there and refuses reads the same way, and that is
/// [`instance_write_ops`](crate::sources::crud::instance_write_ops)' doing
/// rather than this loop's: a locked keychain must not cost the reader the
/// whole Monitors tab, which needs nothing from it but two buttons.
async fn monitor_write_ops<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    rows: &[assets::MonitorRow],
) -> Result<std::collections::HashMap<String, Vec<String>>, IpcError> {
    let Ok(state) = crate::sources::state(app) else {
        return Ok(std::collections::HashMap::new());
    };
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
            crate::sources::crud::instance_write_ops(&state.secrets, state.registry.as_ref(), &cfg)
                .await;
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
/// **No type argument**, although the tab filters by type: nineteen types over
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
    use crate::assets::{
        AssetChange, AssetProperty, AssetStatus, Environment, ImportEntry, MonitorLink,
        PropertyChange, PropertyPlan,
    };
    use knobas_core::asset::{self, PropertyKind};
    use knobas_sync::mirror::{assert_shape, declared_union};

    const MIRROR: &str = include_str!("../../../../app/src/lib/ipc/assets.ts");

    fn row() -> AssetRow {
        AssetRow {
            id: "asset:7f2c".to_owned(),
            parent_id: Some("asset:site".to_owned()),
            type_id: "vm".to_owned(),
            type_label: "VM".to_owned(),
            monogram: "VM".to_owned(),
            name: "vm-db-01".to_owned(),
            status: AssetStatus::Warn,
            environment: Some(Environment::Prod),
            owner: Some("Björn".to_owned()),
            has_children: true,
            health: AssetStatus::Down,
            inside: AssetStatus::Down,
            problems_inside: 3,
            linked_work: 2,
        }
    }

    #[test]
    fn the_asset_row_matches_its_typescript_mirror() {
        assert_shape(
            MIRROR,
            "AssetRow",
            &serde_json::to_value(row()).unwrap(),
            &[
                "id",
                "parent_id",
                "type_id",
                "type_label",
                "monogram",
                "name",
                "status",
                "environment",
                "owner",
                "has_children",
                "health",
                "inside",
                "problems_inside",
                "linked_work",
            ],
        );
    }

    /// The inherited value's carrier (#431), whose mirror is **generic**.
    ///
    /// `"Inherited<T>"` and not `"Inherited"`: [`assert_shape`] finds an
    /// interface by the text of its header, and the mirror's header is
    /// `export interface Inherited<T> {` because the value it carries is an
    /// `Environment` in one field of `AssetDetail` and a `string` in the
    /// other. One generic interface rather than two concrete ones, so a fourth
    /// inherited field later grows nothing.
    #[test]
    fn the_inherited_value_matches_its_typescript_mirror() {
        assert_shape(
            MIRROR,
            "Inherited<T>",
            &serde_json::to_value(assets::Inherited {
                value: Environment::Prod,
                source_id: "asset:hel1".to_owned(),
                source_name: "hel1".to_owned(),
            })
            .unwrap(),
            &["value", "source_id", "source_name"],
        );
    }

    /// The pane's property row, exercised with a **value present**: an
    /// `Option` serializes to a `null` key either way, and the mirror declares
    /// a union on the promise that the populated shape is the tagged one.
    #[test]
    fn the_asset_property_matches_its_typescript_mirror() {
        assert_shape(
            MIRROR,
            "AssetProperty",
            &serde_json::to_value(AssetProperty {
                key: "ip".to_owned(),
                label: "IP".to_owned(),
                value: Some(PropertyValue::Text {
                    value: "10.0.0.4".to_owned(),
                }),
                custom: false,
            })
            .unwrap(),
            &["key", "label", "value", "custom"],
        );
    }

    /// All four property kinds, because the union is what the pane branches on
    /// to decide whether to draw a link, a date or a number.
    #[test]
    fn every_property_value_matches_its_typescript_mirror() {
        for (interface, value, fields) in [
            (
                "TextProperty",
                PropertyValue::Text {
                    value: "Debian 13".to_owned(),
                },
                &["kind", "value"][..],
            ),
            (
                "NumberProperty",
                PropertyValue::Number { value: 8080.0 },
                &["kind", "value"][..],
            ),
            (
                "DateProperty",
                PropertyValue::Date {
                    value: "2026-09-06".to_owned(),
                },
                &["kind", "value"][..],
            ),
            (
                "UrlProperty",
                PropertyValue::Url {
                    value: "https://kuma.local".to_owned(),
                },
                &["kind", "value"][..],
            ),
        ] {
            assert_shape(
                MIRROR,
                interface,
                &serde_json::to_value(&value).unwrap(),
                fields,
            );
        }
        // And the kinds themselves, read out of the mirror rather than listed
        // here -- the rule `entity_mirror.rs` states: a fifth kind added on
        // the Rust side has to fail here rather than fall through.
        let declared = declared_union(MIRROR, "PropertyKind");
        let ours: Vec<String> = PropertyKind::ALL
            .iter()
            .map(|kind| kind.as_str().to_owned())
            .collect();
        assert_eq!(declared, ours, "PropertyKind and its mirror disagree");
    }

    #[test]
    fn every_status_and_environment_is_in_the_mirror() {
        assert_eq!(
            declared_union(MIRROR, "AssetStatus"),
            AssetStatus::ALL
                .iter()
                .map(|status| status.as_str().to_owned())
                .collect::<Vec<_>>()
        );
        assert_eq!(
            declared_union(MIRROR, "Environment"),
            Environment::ALL
                .iter()
                .map(|environment| environment.as_str().to_owned())
                .collect::<Vec<_>>()
        );
    }

    /// The five edits are a tagged union on the wire, and every arm is
    /// exercised: the tag is what the backend matches on, and an arm the
    /// mirror spells differently is an edit that never arrives.
    #[test]
    fn every_edit_matches_its_typescript_mirror() {
        for (interface, edit, fields) in [
            (
                "NameEdit",
                AssetEdit::Name {
                    value: "vm-db-02".to_owned(),
                },
                &["field", "value"][..],
            ),
            (
                "StatusEdit",
                AssetEdit::Status {
                    value: AssetStatus::Down,
                },
                &["field", "value"][..],
            ),
            (
                "EnvironmentEdit",
                AssetEdit::Environment {
                    value: Some(Environment::Stage),
                },
                &["field", "value"][..],
            ),
            (
                "OwnerEdit",
                AssetEdit::Owner {
                    value: Some("Björn".to_owned()),
                },
                &["field", "value"][..],
            ),
            (
                "PropertyEdit",
                AssetEdit::Property {
                    key: "ip".to_owned(),
                    value: Some(PropertyValue::Text {
                        value: "10.0.0.5".to_owned(),
                    }),
                },
                &["field", "key", "value"][..],
            ),
        ] {
            assert_shape(
                MIRROR,
                interface,
                &serde_json::to_value(&edit).unwrap(),
                fields,
            );
        }
    }

    /// The type table's two shapes, exercised on a **type that declares
    /// properties and suggests children** -- `custom` declares and suggests
    /// nothing, so it would let both lists go missing without a word.
    #[test]
    fn the_type_table_matches_its_typescript_mirror() {
        let vm = asset::find("vm").expect("`vm` is in the table");
        assert!(!vm.properties.is_empty() && !vm.suggests.is_empty());
        assert_shape(
            MIRROR,
            "AssetType",
            &serde_json::to_value(vm).unwrap(),
            &["id", "label", "monogram", "properties", "suggests"],
        );
        assert_shape(
            MIRROR,
            "TypedProperty",
            &serde_json::to_value(vm.properties[0]).unwrap(),
            &["key", "label", "kind"],
        );
    }

    fn route() -> RouteRow {
        RouteRow {
            id: "route:9a1b".to_owned(),
            asset_id: "asset:traefik".to_owned(),
            asset_name: "traefik".to_owned(),
            target_id: Some("asset:7f2c".to_owned()),
            target_name: Some("vm-db-01".to_owned()),
            name: "Gitea".to_owned(),
            url: "https://gitea.local/".to_owned(),
            visibility: Visibility::Public,
            properties: Vec::new(),
        }
    }

    #[test]
    fn the_route_row_matches_its_typescript_mirror() {
        assert_shape(
            MIRROR,
            "RouteRow",
            &serde_json::to_value(route()).unwrap(),
            &[
                "id",
                "asset_id",
                "asset_name",
                "target_id",
                "target_name",
                "name",
                "url",
                "visibility",
                "properties",
            ],
        );
        assert_eq!(
            declared_union(MIRROR, "Visibility"),
            Visibility::ALL
                .iter()
                .map(|visibility| visibility.as_str().to_owned())
                .collect::<Vec<_>>()
        );
    }

    /// The route's carrier, asserted the way [`AssetDetail`]'s is and for the
    /// same reason: the field *names* are what is under test, and they do not
    /// depend on what is in the list this module does not own.
    #[test]
    fn the_route_detail_matches_its_typescript_mirror() {
        let detail = RouteDetail {
            route: route(),
            history: Vec::new(),
        };
        assert_shape(
            MIRROR,
            "RouteDetail",
            &serde_json::to_value(detail).unwrap(),
            &["route", "history"],
        );
    }

    /// The five route edits, every arm exercised — the tag is what the backend
    /// matches on, and an arm the mirror spells differently is an edit that
    /// never arrives.
    ///
    /// The interfaces are `Route…Edit` and not `…Edit`: `NameEdit` and
    /// `PropertyEdit` are the asset's and carry different fields, and two
    /// unions in one file cannot share a member name.
    #[test]
    fn every_route_edit_matches_its_typescript_mirror() {
        for (interface, edit, fields) in [
            (
                "RouteNameEdit",
                RouteEdit::Name {
                    value: "Gitea (tunnel)".to_owned(),
                },
                &["field", "value"][..],
            ),
            (
                "RouteUrlEdit",
                RouteEdit::Url {
                    value: "https://gitea.local/".to_owned(),
                },
                &["field", "value"][..],
            ),
            (
                "RouteTargetEdit",
                RouteEdit::Target {
                    value: Some("asset:7f2c".to_owned()),
                },
                &["field", "value"][..],
            ),
            (
                "RouteVisibilityEdit",
                RouteEdit::Visibility {
                    value: Visibility::Public,
                },
                &["field", "value"][..],
            ),
            (
                "RoutePropertyEdit",
                RouteEdit::Property {
                    key: "cert_expires".to_owned(),
                    value: Some(PropertyValue::Date {
                        value: "2026-12-01".to_owned(),
                    }),
                },
                &["field", "key", "value"][..],
            ),
        ] {
            assert_shape(
                MIRROR,
                interface,
                &serde_json::to_value(&edit).unwrap(),
                fields,
            );
        }
    }

    /// The Monitors tab's roster and the three shapes it carries (#448).
    ///
    /// A row with **every** field filled: a shape assertion over `None`s and
    /// empty lists cannot tell a field that is spelled differently on the two
    /// sides from one that is merely absent, and half this shape is optional.
    #[test]
    fn the_monitor_roster_matches_its_typescript_mirror() {
        let row = assets::MonitorRow {
            entity_id: "kuma:7".to_owned(),
            source_id: "kuma".to_owned(),
            name: "gitea".to_owned(),
            state: Some("warn".to_owned()),
            monitor_type: Some("http".to_owned()),
            target: Some("http://gitea:3000/api/healthz".to_owned()),
            response_time_ms: Some(1_900.0),
            checked_at: "2026-09-07T09:00:00Z".parse().expect("an instant"),
            uptime: vec![assets::UptimeRatio {
                window: "1d".to_owned(),
                ratio: 0.98,
            }],
            cert_days_remaining: Some(9.0),
            web_url: Some("http://127.0.0.1:3001/dashboard/7".to_owned()),
            tombstoned: false,
            assets: vec![assets::MonitoredAsset {
                id: "asset:7f2c".to_owned(),
                name: "gitea".to_owned(),
                path: Some("notebook / knobas-stack".to_owned()),
            }],
            actions: vec!["pause_monitor".to_owned()],
            samples: vec![assets::MonitorSample {
                taken_at: "2026-09-07T08:59:00Z".parse().expect("an instant"),
                state: Some("up".to_owned()),
            }],
        };
        let json = serde_json::to_value(&row).expect("MonitorRow serializes");
        assert_shape(
            MIRROR,
            "MonitorRow",
            &json,
            &[
                "entity_id",
                "source_id",
                "name",
                "state",
                "monitor_type",
                "target",
                "response_time_ms",
                "checked_at",
                "uptime",
                "cert_days_remaining",
                "web_url",
                "tombstoned",
                "assets",
                "actions",
                "samples",
            ],
        );
        assert_shape(
            MIRROR,
            "UptimeRatio",
            &json["uptime"][0],
            &["window", "ratio"],
        );
        assert_shape(
            MIRROR,
            "MonitoredAsset",
            &json["assets"][0],
            &["id", "name", "path"],
        );
        assert_shape(
            MIRROR,
            "MonitorSample",
            &json["samples"][0],
            &["taken_at", "state"],
        );
        assert_eq!(
            json["checked_at"], "2026-09-07T09:00:00Z",
            "the mirror declares a string, and chrono has to be writing RFC 3339 into it"
        );
    }

    /// One row of the *Not monitored* roster (#449).
    ///
    /// With a `path`, which is the field that tells the two languages apart:
    /// `None` serializes to a shape any declared type accepts, and the path is
    /// the one thing on this row the criterion names.
    #[test]
    fn the_unmonitored_asset_matches_its_typescript_mirror() {
        assert_shape(
            MIRROR,
            "UnmonitoredAsset",
            &serde_json::to_value(assets::UnmonitoredAsset {
                id: "asset:7f2c".to_owned(),
                type_id: "container".to_owned(),
                type_label: "Container".to_owned(),
                monogram: "CT".to_owned(),
                name: "postgres".to_owned(),
                path: Some("notebook / knobas-stack".to_owned()),
            })
            .unwrap(),
            &["id", "type_id", "type_label", "monogram", "name", "path"],
        );
    }

    /// The two numbers monitoring is shaped by, in both directions, with the
    /// ratified defaults asserted as values.
    ///
    /// The values and not only the shape: two `u32` fields decode from each
    /// other's positions without complaint, and "the default retention is
    /// ninety days" is the sentence spec #427 ratified rather than "there is a
    /// number here". The literals are checked against the crate's own
    /// constants too, so moving a default in one place and not the other is a
    /// failure here rather than a surprise on a fresh profile.
    #[test]
    fn the_monitoring_settings_match_their_typescript_mirror() {
        let settings = MonitoringSettings {
            sample_retention_days: 90,
            response_time_warn_ms: 1500,
        };
        assert_eq!(
            i64::from(settings.sample_retention_days),
            knobas_sync::samples::DEFAULT_RETENTION_DAYS
        );
        assert_eq!(
            i64::from(settings.response_time_warn_ms),
            knobas_sync::samples::DEFAULT_THRESHOLD_MS
        );
        let json = serde_json::to_value(settings).expect("MonitoringSettings serializes");
        assert_shape(
            MIRROR,
            "MonitoringSettings",
            &json,
            &["sample_retention_days", "response_time_warn_ms"],
        );
        assert_eq!(
            serde_json::from_value::<MonitoringSettings>(json).expect("and decodes"),
            settings,
            "the dialog posts this shape back and the backend has to read it"
        );
    }

    /// The open alert, its two-word state and the asset it names, in the shape
    /// the top strip counts and the Assets view lists (#444).
    ///
    /// Exercised with `acked_at` populated and one asset in the list: `None`
    /// and `[]` serialize to shapes any declared type accepts, so the
    /// populated one is the one that tells the two languages apart -- the
    /// discipline `the_asset_property_matches_its_typescript_mirror` states
    /// above.
    #[test]
    fn the_open_alert_matches_its_typescript_mirror() {
        let asset = crate::assets::AlertAsset {
            id: "asset:knobas-jira".to_owned(),
            name: "knobas-jira".to_owned(),
            path: Some("hel / hel1".to_owned()),
        };
        assert_shape(
            MIRROR,
            "AlertAsset",
            &serde_json::to_value(&asset).unwrap(),
            &["id", "name", "path"],
        );
        assert_shape(
            MIRROR,
            "OpenAlert",
            &serde_json::to_value(OpenAlert {
                id: 12,
                monitor_id: "kuma:7".to_owned(),
                monitor_name: "jira (tunnel)".to_owned(),
                state: crate::assets::AlertState::Down,
                opened_at: chrono::Utc::now(),
                acked_at: Some(chrono::Utc::now()),
                assets: vec![asset],
            })
            .unwrap(),
            &[
                "id",
                "monitor_id",
                "monitor_name",
                "state",
                "opened_at",
                "acked_at",
                "assets",
            ],
        );
        // And the union, read out of the mirror rather than listed here: a
        // third state added on the Rust side has to fail here rather than
        // fall through as a value the list draws nothing for.
        assert_eq!(
            declared_union(MIRROR, "AlertState"),
            crate::assets::AlertState::ALL
                .iter()
                .map(|state| state.as_str().to_owned())
                .collect::<Vec<_>>(),
            "AlertState and its mirror disagree"
        );
    }

    /// The twenty-one commands are invoked from the mirror by the names they
    /// are registered under, and registered under the names they are declared
    /// with.
    ///
    /// `tests/wiring.rs` proves every declared command is in the handler list;
    /// this proves the *frontend* calls them by those names. A typo on either
    /// side is a call that fails only at run time, with "command not found"
    /// and nothing else in the tree noticing.
    #[test]
    fn the_mirror_invokes_the_commands_by_their_registered_names() {
        for command in [
            "asset_tree",
            "get_asset",
            "context_assets",
            "source_assets",
            "create_asset",
            "edit_asset",
            "move_asset",
            "delete_asset",
            "asset_types",
            "get_route",
            "create_route",
            "edit_route",
            "delete_route",
            "preview_estate_import",
            "apply_estate_import",
            "monitoring_settings",
            "set_monitoring_settings",
            "monitor_roster",
            "open_alerts",
            "ack_alert",
            "unmonitored_assets",
        ] {
            assert!(
                MIRROR.contains(&format!("\"{command}\"")),
                "{command} is not invoked from app/src/lib/ipc/assets.ts"
            );
            let registry = include_str!("../lib.rs");
            assert!(
                registry.contains(&format!("commands::assets::{command}")),
                "{command} is not in the generate_handler! list"
            );
        }
    }

    /// Tauri renames a command's *arguments* to camelCase and leaves struct
    /// fields alone. Both spellings are on this surface at once -- the
    /// `assetId` argument and the `parent_id` field inside the row it answers
    /// with -- and getting either wrong is a call that arrives with the value
    /// missing and no error anywhere.
    #[test]
    fn the_mirror_sends_the_argument_names_tauri_expects() {
        for (call, argument) in [
            ("asset_tree", "parentId"),
            ("get_asset", "assetId"),
            ("context_assets", "ctxId"),
            ("source_assets", "sourceId"),
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
            ("apply_estate_import", "file"),
            ("set_monitoring_settings", "settings"),
            ("ack_alert", "monitorId"),
        ] {
            let at = MIRROR
                .find(&format!("\"{call}\""))
                .unwrap_or_else(|| panic!("{call} is not invoked from the mirror"));
            let body = &MIRROR[at..MIRROR[at..].find(");").map_or(MIRROR.len(), |end| at + end)];
            assert!(
                body.contains(argument),
                "{call} does not send {argument}: {body}"
            );
        }
    }

    /// The detail is a carrier of shapes pinned above plus one list this
    /// module does not own -- `ActivityRow`, whose mirror lives in
    /// `entity.ts`. What is asserted here is the carrier's own field list, and
    /// it gets [`assert_shape`] like every other shape in this file rather
    /// than a `contains` per name: `body.contains("asset")` cannot fail -- the
    /// body reads `asset: AssetRow;` and `properties: AssetProperty[];`, so
    /// the substring is there several times over whatever the field is called
    /// -- and a substring check is one-directional besides, so a field the
    /// mirror declares and Rust does not would pass it. The empty lists are
    /// what let the carrier be built without owning `ActivityRow`'s fixture;
    /// the field *names* are what is under test here, and those do not depend
    /// on what is in the lists.
    #[test]
    fn the_asset_detail_matches_its_typescript_mirror() {
        let detail = AssetDetail {
            asset: row(),
            properties: Vec::new(),
            effective_environment: Some(assets::Inherited {
                value: Environment::Prod,
                source_id: "asset:hel1".to_owned(),
                source_name: "hel1".to_owned(),
            }),
            effective_owner: None,
            held_by: Vec::new(),
            holds: Vec::new(),
            // Not empty, unlike the two lists above: `RouteRow` is this
            // module's own shape, so the carrier can hold a real one — and a
            // populated list is what proves the field carries routes rather
            // than being a name that happens to serialize.
            exposes: vec![route()],
            reachable_via: Vec::new(),
            history: Vec::new(),
            links: Vec::new(),
            // Populated for the reason `exposes` is: a `Vec<String>` that is
            // empty serializes to the same `[]` a missing field never
            // produces, but a name in it is what proves the field carries the
            // estate file's monitor names rather than being a spelling that
            // happens to serialize.
            monitors: vec!["knobas-jira".to_owned()],
            // Populated for the same reason, and with `state` and `web_url`
            // *present*: both are `Option`, so an empty one would let the
            // mirror declare them anything at all -- the rule
            // `the_asset_property_matches_its_typescript_mirror` states.
            monitoring: vec![assets::AttachedMonitor {
                entity_id: "kuma:7".to_owned(),
                name: "gitea".to_owned(),
                state: Some("up".to_owned()),
                web_url: Some("http://127.0.0.1:3001/dashboard/7".to_owned()),
                tombstoned: false,
            }],
            // Populated for the same reason again: the pane branches on this
            // list being empty, so a fixture that left it so would let the
            // mirror declare the target shape anything at all (#453).
            monitor_targets: vec![assets::MonitorTarget {
                source_id: "kuma".to_owned(),
                display_name: "Uptime Kuma".to_owned(),
                roster: "kuma:monitors".to_owned(),
            }],
        };
        assert_shape(
            MIRROR,
            "AttachedMonitor",
            &serde_json::to_value(&detail.monitoring[0]).unwrap(),
            &["entity_id", "name", "state", "web_url", "tombstoned"],
        );
        assert_shape(
            MIRROR,
            "MonitorTarget",
            &serde_json::to_value(&detail.monitor_targets[0]).unwrap(),
            &["source_id", "display_name", "roster"],
        );
        assert_shape(
            MIRROR,
            "AssetDetail",
            &serde_json::to_value(detail).unwrap(),
            &[
                "asset",
                "properties",
                "effective_environment",
                "effective_owner",
                "held_by",
                "holds",
                "exposes",
                "reachable_via",
                "history",
                "links",
                "monitors",
                "monitoring",
                "monitor_targets",
            ],
        );
    }

    /// The identifier this module offers is the **SPI's**, and the roster it
    /// addresses is the SPI's too.
    ///
    /// `assets::PAUSE_MONITOR`'s pin, applied to the third op. Both halves are
    /// spelled out in `src/` on purpose (see [`CREATE_MONITOR`]) and both
    /// would fail silently: an identifier that drifted would be a control
    /// nothing ever draws, and a roster composed by hand would be a write
    /// `submit_write` routes at a source that is not there.
    #[test]
    fn the_pane_offers_the_op_the_spi_names_at_the_roster_the_spi_spells() {
        assert_eq!(
            CREATE_MONITOR,
            knobas_source::WriteOp::CreateMonitor {
                entity: knobas_source::monitor_roster("kuma"),
                name: "gitea".to_owned(),
                url: "http://gitea:3000/".to_owned(),
            }
            .identifier()
        );
        assert_eq!(
            knobas_source::monitor_roster("kuma-eu"),
            "kuma-eu:monitors",
            "a target the form submits must be the roster the adapter refuses everything else for"
        );
    }

    /// The Import's six shapes, every one exercised **populated** (#439).
    ///
    /// The rule `the_asset_property_matches_its_typescript_mirror` states,
    /// applied to a preview: an `Option` and an empty `Vec` serialize to keys
    /// a declared `T | null` and `T[]` accept whatever is really behind them,
    /// so the populated shape is the one that tells the two languages apart.
    /// `PropertyChange::from` is `Some` here for exactly that reason -- it is
    /// the field a preview draws the *old* value from, and a `null` in this
    /// fixture would let the mirror declare it anything at all.
    #[test]
    fn the_import_preview_matches_its_typescript_mirror() {
        let entry = ImportEntry {
            id: "asset:knobas-jira".to_owned(),
            kind: assets::NAMESPACE.to_owned(),
            name: "knobas-jira".to_owned(),
            type_label: Some("Container".to_owned()),
            parent_id: Some("asset:hetzner-jira-docker".to_owned()),
        };
        let change = AssetChange {
            id: "asset:hetzner-jira".to_owned(),
            name: "knobas-jira".to_owned(),
            properties: vec![PropertyChange {
                key: "server_type".to_owned(),
                label: "server_type".to_owned(),
                from: Some(PropertyValue::Text {
                    value: "cx23".to_owned(),
                }),
                to: PropertyValue::Text {
                    value: "cpx22".to_owned(),
                },
                plan: PropertyPlan::Kept,
            }],
            monitors: vec!["knobas-jira".to_owned()],
        };
        let link = MonitorLink {
            asset_id: "asset:knobas-gitea".to_owned(),
            asset_name: "knobas-gitea".to_owned(),
            monitor_name: "gitea".to_owned(),
            monitor_id: "monitor:kuma:3".to_owned(),
        };
        let waiting = assets::UnresolvedMonitor {
            asset_id: "asset:knobas-jira".to_owned(),
            asset_name: "knobas-jira".to_owned(),
            monitor_name: "jira (tunnel)".to_owned(),
        };
        let preview = ImportPreview {
            name: "knobas test estate".to_owned(),
            known: vec![entry.clone()],
            new: vec![entry.clone()],
            changes: vec![change.clone()],
            monitor_links: vec![link.clone()],
            unresolved: vec![waiting.clone()],
        };

        assert_shape(
            MIRROR,
            "ImportEntry",
            &serde_json::to_value(&entry).unwrap(),
            &["id", "kind", "name", "type_label", "parent_id"],
        );
        assert_shape(
            MIRROR,
            "PropertyChange",
            &serde_json::to_value(&change.properties[0]).unwrap(),
            &["key", "label", "from", "to", "plan"],
        );
        assert_shape(
            MIRROR,
            "AssetChange",
            &serde_json::to_value(&change).unwrap(),
            &["id", "name", "properties", "monitors"],
        );
        assert_shape(
            MIRROR,
            "MonitorLink",
            &serde_json::to_value(&link).unwrap(),
            &["asset_id", "asset_name", "monitor_name", "monitor_id"],
        );
        assert_shape(
            MIRROR,
            "UnresolvedMonitor",
            &serde_json::to_value(&waiting).unwrap(),
            &["asset_id", "asset_name", "monitor_name"],
        );
        assert_shape(
            MIRROR,
            "ImportPreview",
            &serde_json::to_value(&preview).unwrap(),
            &[
                "name",
                "known",
                "new",
                "changes",
                "monitor_links",
                "unresolved",
            ],
        );
        assert_shape(
            MIRROR,
            "ImportOutcome",
            &serde_json::to_value(assets::ImportOutcome {
                assets_created: 23,
                routes_created: 9,
                properties_set: 4,
                properties_kept: 1,
                monitors_kept: 7,
                monitors_linked: 0,
            })
            .unwrap(),
            &[
                "assets_created",
                "routes_created",
                "properties_set",
                "properties_kept",
                "monitors_kept",
                "monitors_linked",
            ],
        );
        // And the two fates, read out of the mirror rather than listed here --
        // a third one added on the Rust side has to fail here rather than fall
        // through as a value the dialog draws nothing for.
        assert_eq!(
            declared_union(MIRROR, "PropertyPlan"),
            ["set", "kept"].map(str::to_owned).to_vec(),
            "PropertyPlan and its mirror disagree"
        );
    }

    /// The relation a source room's tile reads is one the dialog can draw
    /// (#435).
    ///
    /// `assets::MONITORED_BY` is the word `assets::MONITORED_ASSETS` filters
    /// on, and the estate file's monitor names become links carrying it
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

    /// The Assets tile's row (#434), exercised with a **path present**.
    ///
    /// `None` would serialize to the same `null` key a declared
    /// `string | null` accepts, so the populated shape is the one that tells
    /// the two languages apart -- the discipline
    /// `the_asset_property_matches_its_typescript_mirror` states above.
    #[test]
    fn the_member_asset_matches_its_typescript_mirror() {
        assert_shape(
            MIRROR,
            "MemberAsset",
            &serde_json::to_value(MemberAsset {
                asset: row(),
                path: Some("hel / hel1".to_owned()),
            })
            .unwrap(),
            &["asset", "path"],
        );
    }
}
