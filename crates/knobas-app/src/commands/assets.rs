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
    self, AssetDetail, AssetEdit, AssetRow, MemberAsset, PropertyValue, RouteDetail, RouteEdit,
    RouteRow, Visibility,
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
/// what it holds, and its history.
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) before bring-up,
/// [`NotFound`](crate::IpcErrorCode::NotFound) for an id no asset carries.
#[tauri::command]
pub async fn get_asset(
    lifecycle: State<'_, Lifecycle>,
    asset_id: String,
) -> Result<AssetDetail, IpcError> {
    let pool = lifecycle.pool()?;
    assets::get(&pool, &asset_id).await
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::{AssetProperty, AssetStatus, Environment};
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

    /// The thirteen commands are invoked from the mirror by the names they are
    /// registered under, and registered under the names they are declared with.
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
        };
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
            ],
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
