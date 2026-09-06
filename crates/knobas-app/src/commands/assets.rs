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
//! # Why the four writers take an `AppHandle` and the two reads do not
//!
//! Because they announce. Every mutation of an asset is a line in the activity
//! stream (story 11), and the way the shell learns is the `activity:new` event
//! it already watches -- the same signal `create_link`, `unlink` and the timer
//! use. **No event of the estate's own**: a channel of its own would be a
//! second thing to keep in step with the first and would carry no fact the
//! line does not already hold.

use tauri::{Emitter, State};

use crate::assets::{self, AssetDetail, AssetEdit, AssetNode, PropertyValue};
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
) -> Result<Vec<AssetNode>, IpcError> {
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
) -> Result<AssetNode, IpcError> {
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
) -> Result<AssetNode, IpcError> {
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
) -> Result<AssetNode, IpcError> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::{AssetProperty, AssetStatus, Environment};
    use knobas_core::asset::PropertyKind;
    use knobas_sync::mirror::{assert_shape, declared_union, interface_body};

    const MIRROR: &str = include_str!("../../../../app/src/lib/ipc/assets.ts");

    fn node() -> AssetNode {
        AssetNode {
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
        }
    }

    #[test]
    fn the_asset_node_matches_its_typescript_mirror() {
        assert_shape(
            MIRROR,
            "AssetNode",
            &serde_json::to_value(node()).unwrap(),
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
            ],
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

    /// The six commands are invoked from the mirror by the names they are
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
            "create_asset",
            "edit_asset",
            "move_asset",
            "delete_asset",
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
    /// `assetId` argument and the `parent_id` field inside the node it answers
    /// with -- and getting either wrong is a call that arrives with the value
    /// missing and no error anywhere.
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
    /// `entity.ts`. What is asserted here is the carrier's own field list.
    #[test]
    fn the_asset_detail_matches_its_typescript_mirror() {
        let body = interface_body(MIRROR, "AssetDetail");
        for field in ["asset", "properties", "held_by", "holds", "history"] {
            assert!(body.contains(field), "AssetDetail has no {field}: {body}");
        }
    }
}
