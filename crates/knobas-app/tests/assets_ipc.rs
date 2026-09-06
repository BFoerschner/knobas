//! The estate at the seam the nine commands are shims over, and at the seam
//! Tauri dispatches through (issues #428, #429, #431, #434 and #435).
//!
//! Two halves, the same split `tests/backup_ipc.rs` and `tests/time_ipc.rs`
//! make:
//!
//! * the *behaviour* -- create a three-level tree, read it column by column,
//!   read one asset with its held-by path, edit, move, refuse a cycle, delete
//!   -- driven through `knobas_app::assets`, which is what each
//!   `#[tauri::command]` calls;
//! * the *wiring* -- every command registered under the name the TypeScript
//!   mirror invokes, and its argument shape decoding -- driven through the
//!   `tauri::test` mock runtime.
//!
//! Every behavioural test gets a **database of its own**
//! (`knobas_db::test_util::scratch_database`), not the binary's shared one.
//! The estate is a tree with one top level, and the reads this file makes
//! hardest use of -- "what does the top of the estate hold" and "what does
//! this asset's history say" -- are answers about a *whole* database. Two
//! tests sharing one would be each other's fixture, and the assertion that a
//! deleted leaf leaves a column of two would be an assertion about whichever
//! test ran first.

use knobas_app::assets::{self, AssetEdit, AssetRow, AssetStatus, Environment, PropertyValue};
use knobas_app::{IpcError, IpcErrorCode};
use sqlx::{PgPool, Row};
use tauri::ipc::CallbackFn;
use tauri::test::MockRuntime;

#[cfg(windows)]
const LOCAL_ORIGIN: &str = "http://tauri.localhost";
#[cfg(not(windows))]
const LOCAL_ORIGIN: &str = "tauri://localhost";

/// A migrated database of this test's own -- `scratch_database` creates it and
/// runs the migrations before it hands the connector back.
async fn pool(label: &str) -> PgPool {
    knobas_db::test_util::scratch_database(label)
        .await
        .pool(2)
        .await
        .expect("a pool on the scratch database")
}

fn text(value: &str) -> PropertyValue {
    PropertyValue::Text {
        value: value.to_owned(),
    }
}

fn code(error: &IpcError) -> IpcErrorCode {
    error.code
}

/// Create an asset and hand back its id, failing loudly rather than returning
/// a `Result` no test would look at.
async fn make(
    pool: &PgPool,
    parent: Option<&str>,
    type_id: &str,
    name: &str,
    properties: &[(String, PropertyValue)],
) -> AssetRow {
    assets::create(pool, parent, type_id, name, properties)
        .await
        .unwrap_or_else(|error| panic!("create {name}: {}", error.message))
        .value
}

/// The estate this file works over: **site → VM → container**, three levels,
/// which is the shape the acceptance criterion asks for and the shallowest one
/// in which "the path to a container" is a sentence with a middle.
async fn three_levels(pool: &PgPool) -> (AssetRow, AssetRow, AssetRow) {
    let site = make(pool, None, "site", "hel1", &[]).await;
    let vm = make(
        pool,
        Some(&site.id),
        "vm",
        "vm-db-01",
        &[
            ("ip".to_owned(), text("10.0.0.4")),
            ("os".to_owned(), text("Debian 13")),
        ],
    )
    .await;
    let container = make(
        pool,
        Some(&vm.id),
        "container",
        "postgres",
        &[("image".to_owned(), text("postgres:18"))],
    )
    .await;
    (site, vm, container)
}

/// The `detail` of every line the asset's history holds, newest first.
async fn history(pool: &PgPool, id: &str) -> Vec<(String, serde_json::Value)> {
    assets::get(pool, id)
        .await
        .expect("the asset")
        .history
        .into_iter()
        .map(|line| (line.verb, line.detail))
        .collect()
}

// ---------------------------------------------------------------------------
// The tree, read column by column
// ---------------------------------------------------------------------------

/// Three levels created through the commands, and read back the way the Tree
/// reads them: one call per column, each answering with what one parent holds.
///
/// The top-level read is what makes the `None` parent a *level* rather than a
/// missing filter: the VM and the container exist and neither is in the first
/// column.
#[tokio::test]
async fn a_three_level_tree_reads_back_one_column_per_level() {
    let pool = pool("assets-tree").await;
    let (site, vm, container) = three_levels(&pool).await;

    let top = assets::tree(&pool, None).await.expect("the top level");
    assert_eq!(
        top.iter().map(|a| a.id.as_str()).collect::<Vec<_>>(),
        [site.id.as_str()],
        "the top of the estate is the roots, not every asset"
    );
    assert!(top[0].has_children, "the site holds the VM");
    assert_eq!(top[0].monogram, "SI", "the chip is the *type*'s");

    let under_site = assets::tree(&pool, Some(&site.id)).await.expect("column 2");
    assert_eq!(
        under_site.iter().map(|a| a.id.as_str()).collect::<Vec<_>>(),
        [vm.id.as_str()]
    );
    assert_eq!(under_site[0].monogram, "VM");
    assert!(under_site[0].has_children);

    let under_vm = assets::tree(&pool, Some(&vm.id)).await.expect("column 3");
    assert_eq!(
        under_vm.iter().map(|a| a.id.as_str()).collect::<Vec<_>>(),
        [container.id.as_str()]
    );
    assert_eq!(under_vm[0].monogram, "CT");
    assert!(
        !under_vm[0].has_children,
        "nothing sits under the container, so the Tree draws no fourth column"
    );

    assert!(
        assets::tree(&pool, Some(&container.id))
            .await
            .expect("a leaf's column")
            .is_empty()
    );
}

/// A column is ordered by name, so two siblings created in the wrong order
/// still read in the right one.
#[tokio::test]
async fn a_column_is_ordered_by_name_and_not_by_when_it_was_made() {
    let pool = pool("assets-order").await;
    let site = make(&pool, None, "site", "hel1", &[]).await;
    for name in ["vm-web-02", "vm-db-01", "vm-app-03"] {
        make(&pool, Some(&site.id), "vm", name, &[]).await;
    }
    let column = assets::tree(&pool, Some(&site.id)).await.expect("column");
    assert_eq!(
        column.iter().map(|a| a.name.as_str()).collect::<Vec<_>>(),
        ["vm-app-03", "vm-db-01", "vm-web-02"]
    );
}

// ---------------------------------------------------------------------------
// One asset, as the pane draws it
// ---------------------------------------------------------------------------

/// The pane's read: the held-by path outermost first and **excluding the asset
/// itself**, what it holds, and its properties with the type's keys first.
#[tokio::test]
async fn one_asset_reads_with_its_held_by_path_and_its_properties() {
    let pool = pool("assets-detail").await;
    let (site, vm, container) = three_levels(&pool).await;

    let detail = assets::get(&pool, &container.id).await.expect("the pane");
    assert_eq!(
        detail
            .held_by
            .iter()
            .map(|a| a.name.as_str())
            .collect::<Vec<_>>(),
        ["hel1", "vm-db-01"],
        "outermost first, and the container is not its own ancestor"
    );
    assert!(detail.holds.is_empty());
    assert_eq!(detail.asset.type_label, "Container");

    // The container's schema is image, ports, restart_policy -- in that order,
    // with the two nobody filled in drawn as rows with no value.
    let keys: Vec<&str> = detail.properties.iter().map(|p| p.key.as_str()).collect();
    assert_eq!(keys, ["image", "ports", "restart_policy"]);
    assert_eq!(detail.properties[0].value, Some(text("postgres:18")));
    assert_eq!(detail.properties[1].value, None);
    assert!(detail.properties.iter().all(|p| !p.custom));

    // The site is at the top, so its path is empty rather than absent.
    let top = assets::get(&pool, &site.id).await.expect("the site");
    assert!(top.held_by.is_empty());
    assert_eq!(
        top.holds.iter().map(|a| a.id.as_str()).collect::<Vec<_>>(),
        [vm.id.as_str()]
    );

    let missing = assets::get(&pool, "asset:nobody").await.unwrap_err();
    assert_eq!(code(&missing), IpcErrorCode::NotFound);
}

/// A custom key is stored, listed after the type's own, and marked as custom
/// -- which is what lets the pane rule a line between the two groups without
/// keeping a copy of the schema.
#[tokio::test]
async fn a_custom_property_is_stored_beside_the_typed_ones_and_marked() {
    let pool = pool("assets-custom").await;
    let (_, vm, _) = three_levels(&pool).await;

    assets::edit(
        &pool,
        &vm.id,
        &[AssetEdit::Property {
            key: "backup window".to_owned(),
            value: Some(text("02:00–03:00")),
        }],
    )
    .await
    .expect("the custom property");

    let listed = assets::get(&pool, &vm.id)
        .await
        .expect("the pane")
        .properties;
    let custom: Vec<&str> = listed
        .iter()
        .filter(|p| p.custom)
        .map(|p| p.key.as_str())
        .collect();
    assert_eq!(custom, ["backup window"]);
    assert_eq!(
        listed.iter().position(|p| p.custom),
        Some(4),
        "the VM's four declared keys come first"
    );
}

// ---------------------------------------------------------------------------
// Editing, and what the history says about it
// ---------------------------------------------------------------------------

/// Story 11: a property edit is a line with the **old and the new** value.
///
/// Both halves are asserted, and the old one is the half that matters: "who
/// changed the IP" is only answerable if what it changed *from* was written
/// down at the moment it stopped being true.
#[tokio::test]
async fn editing_a_property_writes_the_old_to_new_line() {
    let pool = pool("assets-edit").await;
    let (_, vm, _) = three_levels(&pool).await;

    assets::edit(
        &pool,
        &vm.id,
        &[AssetEdit::Property {
            key: "ip".to_owned(),
            value: Some(text("10.0.0.9")),
        }],
    )
    .await
    .expect("the edit");

    let lines = history(&pool, &vm.id).await;
    let (verb, detail) = lines.first().expect("a line for the edit");
    assert_eq!(verb, "edited");
    assert_eq!(detail["field"], "property");
    assert_eq!(detail["key"], "ip");
    assert_eq!(detail["from"]["value"], "10.0.0.4");
    assert_eq!(detail["to"]["value"], "10.0.0.9");

    // And the value really moved, not just the line.
    let listed = assets::get(&pool, &vm.id)
        .await
        .expect("the pane")
        .properties;
    assert_eq!(listed[1].value, Some(text("10.0.0.9")));
}

/// Clearing a property removes the key and says so with a `to` of `null` --
/// the one thing a bag of optionals could not have expressed.
#[tokio::test]
async fn clearing_a_property_removes_the_key_and_records_the_clear() {
    let pool = pool("assets-clear").await;
    let (_, vm, _) = three_levels(&pool).await;

    assets::edit(
        &pool,
        &vm.id,
        &[AssetEdit::Property {
            key: "os".to_owned(),
            value: None,
        }],
    )
    .await
    .expect("the clear");

    let (verb, detail) = history(&pool, &vm.id).await.remove(0);
    assert_eq!(verb, "edited");
    assert_eq!(detail["from"]["value"], "Debian 13");
    assert!(detail["to"].is_null());

    let listed = assets::get(&pool, &vm.id)
        .await
        .expect("the pane")
        .properties;
    assert_eq!(
        listed
            .iter()
            .find(|p| p.key == "os")
            .expect("the row")
            .value,
        None,
        "the schema still lists it; the value is gone"
    );
}

/// A rename is its own verb, and it moves the **entity title** too -- which is
/// what a link chip, a room tile and a search row draw.
#[tokio::test]
async fn a_rename_is_its_own_line_and_moves_the_entity_title() {
    let pool = pool("assets-rename").await;
    let (_, vm, _) = three_levels(&pool).await;

    assets::edit(
        &pool,
        &vm.id,
        &[AssetEdit::Name {
            value: "  vm-db-02  ".to_owned(),
        }],
    )
    .await
    .expect("the rename");

    let (verb, detail) = history(&pool, &vm.id).await.remove(0);
    assert_eq!(verb, "renamed");
    assert_eq!(detail["from"], "vm-db-01");
    assert_eq!(detail["to"], "vm-db-02", "and it is trimmed");

    let title: String = sqlx::query_scalar("select title from knobas.entity where id = $1")
        .bind(&vm.id)
        .fetch_one(&pool)
        .await
        .expect("the entity row");
    assert_eq!(title, "vm-db-02");
}

/// Status, environment and owner in one call: three lines, one transaction,
/// and each line naming the field it is about.
#[tokio::test]
async fn a_list_of_edits_writes_one_line_each_and_an_unchanged_field_writes_none() {
    let pool = pool("assets-edit-list").await;
    let (_, vm, _) = three_levels(&pool).await;

    let after = assets::edit(
        &pool,
        &vm.id,
        &[
            AssetEdit::Status {
                value: AssetStatus::Warn,
            },
            AssetEdit::Environment {
                value: Some(Environment::Prod),
            },
            AssetEdit::Owner {
                value: Some("Björn".to_owned()),
            },
        ],
    )
    .await
    .expect("three edits");
    assert_eq!(after.activity.len(), 3);
    assert_eq!(after.value.status, AssetStatus::Warn);
    assert_eq!(after.value.environment, Some(Environment::Prod));
    assert_eq!(after.value.owner.as_deref(), Some("Björn"));

    let fields: Vec<String> = after
        .activity
        .iter()
        .map(|line| line.detail["field"].as_str().unwrap_or_default().to_owned())
        .collect();
    assert_eq!(fields, ["status", "environment", "owner"]);

    // Re-applying the same three changes nothing, so it writes nothing: a
    // history of "set it to what it already was" is noise, and story 11 is
    // about what changed.
    let again = assets::edit(
        &pool,
        &vm.id,
        &[
            AssetEdit::Status {
                value: AssetStatus::Warn,
            },
            AssetEdit::Environment {
                value: Some(Environment::Prod),
            },
        ],
    )
    .await
    .expect("a no-op edit");
    assert!(again.activity.is_empty());
}

/// The two refusals a pane can provoke: a value that does not fit the type's
/// declared kind, and a name that is only whitespace.
#[tokio::test]
async fn an_edit_that_does_not_fit_the_schema_is_refused_by_name() {
    let pool = pool("assets-refuse").await;
    let site = make(&pool, None, "site", "hel1", &[]).await;
    let service = make(&pool, Some(&site.id), "service", "kuma", &[]).await;

    let wrong = assets::edit(
        &pool,
        &service.id,
        &[AssetEdit::Property {
            key: "port".to_owned(),
            value: Some(text("3001")),
        }],
    )
    .await
    .unwrap_err();
    assert_eq!(code(&wrong), IpcErrorCode::Invalid);
    assert!(wrong.message.contains("port"), "{}", wrong.message);

    let blank = assets::edit(
        &pool,
        &service.id,
        &[AssetEdit::Name {
            value: "   ".to_owned(),
        }],
    )
    .await
    .unwrap_err();
    assert_eq!(code(&blank), IpcErrorCode::Invalid);

    // Nothing was written: the refusal came before the transaction committed.
    assert_eq!(
        assets::get(&pool, &service.id).await.unwrap().asset.name,
        "kuma"
    );
    assert!(
        history(&pool, &service.id).await.len() == 1,
        "only `created`"
    );

    let unknown = assets::create(&pool, None, "flowrun-scenario", "orchestra", &[])
        .await
        .unwrap_err();
    assert_eq!(code(&unknown), IpcErrorCode::Invalid);
    assert!(
        unknown.message.contains("flowrun-scenario"),
        "{}",
        unknown.message
    );
}

// ---------------------------------------------------------------------------
// Moving, and the cycle it must refuse
// ---------------------------------------------------------------------------

/// Story 3: an asset's place is one parent, edited by moving it, and the move
/// is a line in its history.
#[tokio::test]
async fn moving_an_asset_re_parents_it_and_writes_the_move() {
    let pool = pool("assets-move").await;
    let (site, vm, container) = three_levels(&pool).await;
    let other = make(&pool, Some(&site.id), "vm", "vm-app-02", &[]).await;

    let moved = assets::move_to(&pool, &container.id, Some(&other.id))
        .await
        .expect("the move");
    assert_eq!(moved.value.parent_id.as_deref(), Some(other.id.as_str()));

    // One line, not two: the count is asserted as well as the content, because
    // a writer that recorded its move twice would pass every assertion about
    // what the line *says*.
    assert_eq!(moved.activity.len(), 1);
    let verbs: Vec<String> = history(&pool, &container.id)
        .await
        .into_iter()
        .map(|(verb, _)| verb)
        .collect();
    assert_eq!(verbs, ["moved", "created"]);
    let (verb, detail) = history(&pool, &container.id).await.remove(0);
    assert_eq!(verb, "moved");
    assert_eq!(detail["from"], serde_json::json!(vm.id));
    assert_eq!(detail["to"], serde_json::json!(other.id));

    // Moving it where it already is changes nothing and writes nothing -- the
    // edit path's twin, and the branch a reader of `move_to` has to trust.
    let again = assets::move_to(&pool, &container.id, Some(&other.id))
        .await
        .expect("a no-op move");
    assert!(
        again.activity.is_empty(),
        "a move to the same parent is not a move"
    );
    assert_eq!(history(&pool, &container.id).await.len(), 2);

    // The columns agree: the old parent has lost it and the new one has it.
    assert!(assets::tree(&pool, Some(&vm.id)).await.unwrap().is_empty());
    assert_eq!(
        assets::tree(&pool, Some(&other.id))
            .await
            .unwrap()
            .iter()
            .map(|a| a.id.as_str())
            .collect::<Vec<_>>(),
        [container.id.as_str()]
    );

    // And the held-by path is the new one, which is the read the pane draws.
    let detail = assets::get(&pool, &container.id).await.unwrap();
    assert_eq!(
        detail
            .held_by
            .iter()
            .map(|a| a.name.as_str())
            .collect::<Vec<_>>(),
        ["hel1", "vm-app-02"]
    );
}

/// A move to the top of the estate is a move, not a refusal: `null` is a
/// parent.
#[tokio::test]
async fn an_asset_can_be_moved_to_the_top_of_the_estate() {
    let pool = pool("assets-move-top").await;
    let (site, vm, _) = three_levels(&pool).await;

    assets::move_to(&pool, &vm.id, None)
        .await
        .expect("the move");
    let top = assets::tree(&pool, None).await.expect("the top level");
    let mut names: Vec<&str> = top.iter().map(|a| a.name.as_str()).collect();
    names.sort_unstable();
    assert_eq!(names, ["hel1", "vm-db-01"]);
    assert!(
        assets::tree(&pool, Some(&site.id))
            .await
            .unwrap()
            .is_empty(),
        "the site no longer holds it"
    );
}

/// A move that would make a cycle is refused, **by name**, and nothing moves.
///
/// Three levels is the shallowest fixture in which the refusal is not the
/// one-step case `asset_no_self_parent_chk` already closes: moving the *site*
/// under the *container* it (transitively) holds is a two-hop loop, which no
/// check constraint can see.
#[tokio::test]
async fn a_move_that_would_make_a_cycle_is_refused_by_name() {
    let pool = pool("assets-cycle").await;
    let (site, vm, container) = three_levels(&pool).await;

    let refused = assets::move_to(&pool, &site.id, Some(&container.id))
        .await
        .unwrap_err();
    assert_eq!(code(&refused), IpcErrorCode::Invalid);
    assert!(
        refused.message.contains("hel1") && refused.message.contains("postgres"),
        "the refusal must name both ends: {}",
        refused.message
    );
    // And the asset that **closes** the loop, which is the one step below the
    // moved asset on the walk: `hel1` already holds `vm-db-01` on the way down
    // to `postgres`. Asserted separately because it is the only name in the
    // sentence no other argument supplies -- a guard answering with the moved
    // asset's own name would render "hel1 is already held by hel1", pass the
    // two assertions above, and say nothing.
    assert!(
        refused.message.contains("vm-db-01"),
        "the refusal must name the asset that closes the loop: {}",
        refused.message
    );

    // The one-hop case, which the schema also forbids, refused here first so
    // the reader gets a sentence rather than a constraint violation.
    let itself = assets::move_to(&pool, &vm.id, Some(&vm.id))
        .await
        .unwrap_err();
    assert_eq!(code(&itself), IpcErrorCode::Invalid);
    assert!(itself.message.contains("vm-db-01"), "{}", itself.message);

    // Nothing moved, and no line was written.
    assert_eq!(
        assets::get(&pool, &site.id).await.unwrap().asset.parent_id,
        None
    );
    assert_eq!(
        history(&pool, &site.id)
            .await
            .iter()
            .map(|(verb, _)| verb.as_str())
            .collect::<Vec<_>>(),
        ["created"]
    );

    let nowhere = assets::move_to(&pool, &vm.id, Some("asset:nobody"))
        .await
        .unwrap_err();
    assert_eq!(code(&nowhere), IpcErrorCode::NotFound);
}

/// A move rewrites `path_text` for the whole subtree, which is what the
/// launcher matches ancestor names against.
///
/// The **container** is what is asserted, not the VM that moved: a subtree
/// update that only fixed the asset it was handed would leave the container
/// still claiming to live under a site it has left, and nothing else in this
/// file would notice.
#[tokio::test]
async fn a_move_rewrites_the_ancestor_path_of_everything_underneath() {
    let pool = pool("assets-paths").await;
    let (_, vm, container) = three_levels(&pool).await;
    let elsewhere = make(&pool, None, "site", "nbg1", &[]).await;

    let path = |id: String| {
        let pool = pool.clone();
        async move {
            sqlx::query_scalar::<_, String>("select path_text from knobas.asset where id = $1")
                .bind(id)
                .fetch_one(&pool)
                .await
                .expect("the row")
        }
    };

    assert_eq!(path(container.id.clone()).await, "hel1 / vm-db-01");

    assets::move_to(&pool, &vm.id, Some(&elsewhere.id))
        .await
        .expect("the move");
    assert_eq!(path(vm.id.clone()).await, "nbg1");
    assert_eq!(
        path(container.id.clone()).await,
        "nbg1 / vm-db-01",
        "the subtree moved with it"
    );

    // A rename walks the same subtree, for the same reason.
    assets::edit(
        &pool,
        &vm.id,
        &[AssetEdit::Name {
            value: "vm-db-99".to_owned(),
        }],
    )
    .await
    .expect("the rename");
    assert_eq!(path(container.id.clone()).await, "nbg1 / vm-db-99");
}

// ---------------------------------------------------------------------------
// Deleting
// ---------------------------------------------------------------------------

/// A leaf goes; the column it was in loses it; the entity row is **tombstoned
/// rather than removed**, so a link drawn to it stays visible and marked.
#[tokio::test]
async fn a_leaf_is_deleted_and_its_entity_row_is_tombstoned() {
    let pool = pool("assets-delete").await;
    let (_, vm, container) = three_levels(&pool).await;

    assets::delete(&pool, &container.id)
        .await
        .expect("the delete");

    assert!(assets::tree(&pool, Some(&vm.id)).await.unwrap().is_empty());
    assert_eq!(
        code(&assets::get(&pool, &container.id).await.unwrap_err()),
        IpcErrorCode::NotFound
    );
    let tombstoned: Option<chrono::DateTime<chrono::Utc>> =
        sqlx::query_scalar("select deleted_at from knobas.entity where id = $1")
            .bind(&container.id)
            .fetch_one(&pool)
            .await
            .expect("the entity row survives");
    assert!(tombstoned.is_some(), "the address is kept and marked");

    // The parent stops claiming a child, which is what the chevron reads.
    assert!(!assets::get(&pool, &vm.id).await.unwrap().asset.has_children);

    // The delete is a line in the stream too, and it is the one mutation whose
    // line `get` can never read back -- the asset is gone. Read from
    // `knobas.activity` directly, so that dropping the `record_with` call does
    // not leave this file green.
    let (verb, detail): (String, serde_json::Value) = sqlx::query_as(
        "select verb, detail from knobas.activity
          where entity_id = $1 order by id desc limit 1",
    )
    .bind(&container.id)
    .fetch_one(&pool)
    .await
    .expect("the delete's own line");
    assert_eq!(verb, "deleted");
    assert_eq!(detail["asset"]["name"], serde_json::json!("postgres"));
    assert_eq!(detail["asset"]["type"], serde_json::json!("container"));
}

/// An asset that still holds something is refused, and the refusal says how
/// many: deleting a subtree by deleting its root is the one destructive action
/// nobody asks for twice.
#[tokio::test]
async fn deleting_an_asset_that_still_holds_something_is_refused() {
    let pool = pool("assets-delete-parent").await;
    let (site, vm, _) = three_levels(&pool).await;

    for held in [&site, &vm] {
        let refused = assets::delete(&pool, &held.id).await.unwrap_err();
        assert_eq!(code(&refused), IpcErrorCode::Conflict);
        assert!(refused.message.contains(&held.name), "{}", refused.message);
    }

    // Nothing was removed by the attempt.
    assert_eq!(assets::tree(&pool, None).await.unwrap().len(), 1);
    assert_eq!(
        code(&assets::delete(&pool, "asset:nobody").await.unwrap_err()),
        IpcErrorCode::NotFound
    );
}

// ---------------------------------------------------------------------------
// The estate in the address space
// ---------------------------------------------------------------------------

/// An asset is an **entity**: `asset:` namespace, kind `asset`, one row in
/// `knobas.entity` carrying the title -- the pair notes have had since `0006`,
/// and what makes an asset linkable and searchable.
#[tokio::test]
async fn an_asset_is_an_entity_in_the_namespace_knobas_reserves() {
    let pool = pool("assets-entity").await;
    let (_, vm, _) = three_levels(&pool).await;

    assert!(vm.id.starts_with("asset:"));
    let (kind, title): (String, String) =
        sqlx::query_as("select kind, title from knobas.entity where id = $1")
            .bind(&vm.id)
            .fetch_one(&pool)
            .await
            .expect("the entity row");
    assert_eq!(kind, "asset");
    assert_eq!(title, "vm-db-01");
    assert!(knobas_core::entity::is_owned_kind(&kind));
    assert!(knobas_core::entity::is_reserved_namespace("asset"));

    // The launcher's corpus reads the asset table, so the estate is findable
    // by name and by the name of anything above it (`knobas_search`'s
    // `corpus::ASSET`).
    let matched: Vec<String> = sqlx::query_scalar(
        "select name from knobas.asset
          where fts @@ websearch_to_tsquery('english', $1)",
    )
    .bind("hel1")
    .fetch_all(&pool)
    .await
    .expect("the index");
    // The *names*, not the count: `>= 2` passes whether or not the container
    // matched, and the container is the whole claim -- it carries `hel1`
    // nowhere but in `path_text`, which is the weight-B ancestor match the
    // column exists for.
    let mut matched = matched;
    matched.sort();
    assert_eq!(
        matched,
        ["hel1", "postgres", "vm-db-01"],
        "the site matches its own name and everything under it matches through `path_text`"
    );
}

// ---------------------------------------------------------------------------
// What is inherited, and what rolls up (#431)
// ---------------------------------------------------------------------------

/// The estate the three #431 rules are read over, and the shape their
/// **negatives** need: a site holding two VMs, each holding containers.
///
/// `three_levels` above cannot witness any of this. It is a single chain, so
/// "a sibling subtree is unaffected" has no sibling to be unaffected, and
/// "the nearest ancestor" and "any ancestor" name the same row for every asset
/// in it. This one branches at the VM and again below it:
///
/// ```text
/// hel1                       site, environment prod, owner Björn
///   vm-db-01                 vm
///     postgres               container
///     redis                  container
///   vm-app-02                vm
///     nginx                  container
/// ```
struct Estate {
    site: AssetRow,
    db: AssetRow,
    postgres: AssetRow,
    redis: AssetRow,
    app: AssetRow,
    nginx: AssetRow,
}

async fn two_branches(pool: &PgPool) -> Estate {
    let site = make(pool, None, "site", "hel1", &[]).await;
    let db = make(pool, Some(&site.id), "vm", "vm-db-01", &[]).await;
    let postgres = make(pool, Some(&db.id), "container", "postgres", &[]).await;
    let redis = make(pool, Some(&db.id), "container", "redis", &[]).await;
    let app = make(pool, Some(&site.id), "vm", "vm-app-02", &[]).await;
    let nginx = make(pool, Some(&app.id), "container", "nginx", &[]).await;
    Estate {
        site,
        db,
        postgres,
        redis,
        app,
        nginx,
    }
}

/// Apply one edit, failing loudly.
async fn edit_one(pool: &PgPool, id: &str, edit: AssetEdit) {
    assets::edit(pool, id, &[edit])
        .await
        .unwrap_or_else(|error| panic!("edit {id}: {}", error.message));
}

/// The environment in force on `id`, and the name of the asset that sets it.
async fn env_of(pool: &PgPool, id: &str) -> Option<(Environment, String)> {
    assets::get(pool, id)
        .await
        .expect("the pane")
        .effective_environment
        .map(|from| (from.value, from.source_name))
}

/// The owner in force on `id`, and the name of the asset that sets it.
async fn owner_of(pool: &PgPool, id: &str) -> Option<(String, String)> {
    assets::get(pool, id)
        .await
        .expect("the pane")
        .effective_owner
        .map(|from| (from.value, from.source_name))
}

/// Stories 8, 9 and 10: the value in force is the nearest one at or above, and
/// the read says **which asset** set it.
///
/// The negative is the second VM's subtree, which is checked after every
/// override: an implementation that read the environment off the whole estate
/// -- or off the outermost setter -- would pass the positive half of each
/// step and fail here.
#[tokio::test]
async fn environment_and_owner_come_from_the_nearest_asset_at_or_above() {
    let pool = pool("assets-inherit").await;
    let estate = two_branches(&pool).await;

    edit_one(
        &pool,
        &estate.site.id,
        AssetEdit::Environment {
            value: Some(Environment::Prod),
        },
    )
    .await;
    edit_one(
        &pool,
        &estate.site.id,
        AssetEdit::Owner {
            value: Some("Björn".to_owned()),
        },
    )
    .await;

    // Two levels down, with nothing set in between: the site is the source,
    // and the read names it so the pane can link to it.
    let deep = assets::get(&pool, &estate.postgres.id).await.expect("pane");
    let from = deep.effective_environment.expect("the site's environment");
    assert_eq!(from.value, Environment::Prod);
    assert_eq!(from.source_id, estate.site.id);
    assert_eq!(from.source_name, "hel1");
    assert!(
        deep.asset.environment.is_none(),
        "nothing is set *on* the container -- the stored field and the \
         effective one are different facts"
    );
    let owner = deep.effective_owner.expect("the site's owner");
    assert_eq!(owner.value, "Björn");
    assert_eq!(owner.source_id, estate.site.id);

    // The middle VM overrides it. Everything under that VM takes the new
    // value; the other VM's subtree does not hear about it.
    edit_one(
        &pool,
        &estate.db.id,
        AssetEdit::Environment {
            value: Some(Environment::Dev),
        },
    )
    .await;
    assert_eq!(
        env_of(&pool, &estate.postgres.id).await,
        Some((Environment::Dev, "vm-db-01".to_owned())),
        "the nearest setter wins, and it is the VM now"
    );
    assert_eq!(
        env_of(&pool, &estate.redis.id).await,
        Some((Environment::Dev, "vm-db-01".to_owned())),
        "the override is for the whole subtree, not one child"
    );
    assert_eq!(
        env_of(&pool, &estate.nginx.id).await,
        Some((Environment::Prod, "hel1".to_owned())),
        "the sibling subtree is unaffected"
    );

    // Set on the asset itself: the source is the asset, which is how the pane
    // tells "set here" from "inherited from".
    edit_one(
        &pool,
        &estate.postgres.id,
        AssetEdit::Environment {
            value: Some(Environment::Stage),
        },
    )
    .await;
    let own = assets::get(&pool, &estate.postgres.id).await.expect("pane");
    let from = own.effective_environment.expect("its own environment");
    assert_eq!(from.value, Environment::Stage);
    assert_eq!(from.source_id, estate.postgres.id, "set here");
    assert_eq!(
        env_of(&pool, &estate.redis.id).await,
        Some((Environment::Dev, "vm-db-01".to_owned())),
        "a sibling *asset* is unaffected too, not only a sibling subtree"
    );

    // Owner walks the same field, and overriding one does not move the other:
    // the two are read independently or the pane would show a team that never
    // owned the thing.
    edit_one(
        &pool,
        &estate.db.id,
        AssetEdit::Owner {
            value: Some("Platform".to_owned()),
        },
    )
    .await;
    let deep = assets::get(&pool, &estate.postgres.id).await.expect("pane");
    assert_eq!(
        deep.effective_owner.expect("the VM's owner").value,
        "Platform"
    );
    assert_eq!(
        deep.effective_environment.expect("still its own").source_id,
        estate.postgres.id,
        "the owner override left the environment where it was"
    );
    assert_eq!(
        assets::get(&pool, &estate.nginx.id)
            .await
            .expect("pane")
            .effective_owner
            .expect("the site's owner")
            .source_name,
        "hel1",
        "the sibling subtree still has the site's owner"
    );
}

/// Clearing a child's own environment falls back to the ancestor's again.
///
/// `null` is an unambiguous clear on this wire (`AssetEdit` is a tagged
/// union), so this is the one rule of the three that has a *write* in the
/// middle of it. The negative is the same read on an estate where nothing is
/// set anywhere: the answer there is `None`, not the first value the walk
/// happens to meet.
#[tokio::test]
async fn clearing_a_childs_environment_falls_back_to_the_ancestors() {
    let pool = pool("assets-clear").await;
    let estate = two_branches(&pool).await;

    // Nothing set anywhere: nothing is in force, and that is a `null` rather
    // than a default.
    assert_eq!(env_of(&pool, &estate.postgres.id).await, None);
    assert!(
        assets::get(&pool, &estate.postgres.id)
            .await
            .expect("pane")
            .effective_owner
            .is_none()
    );

    edit_one(
        &pool,
        &estate.site.id,
        AssetEdit::Environment {
            value: Some(Environment::Prod),
        },
    )
    .await;
    edit_one(
        &pool,
        &estate.db.id,
        AssetEdit::Environment {
            value: Some(Environment::Dev),
        },
    )
    .await;
    assert_eq!(
        env_of(&pool, &estate.postgres.id).await,
        Some((Environment::Dev, "vm-db-01".to_owned()))
    );

    edit_one(&pool, &estate.db.id, AssetEdit::Environment { value: None }).await;

    let cleared = assets::get(&pool, &estate.db.id).await.expect("pane");
    assert!(
        cleared.asset.environment.is_none(),
        "the stored value is gone"
    );
    let from = cleared
        .effective_environment
        .expect("the site's value is in force again");
    assert_eq!(from.value, Environment::Prod);
    assert_eq!(from.source_id, estate.site.id);
    assert_eq!(
        env_of(&pool, &estate.postgres.id).await,
        Some((Environment::Prod, "hel1".to_owned())),
        "and everything under it falls back with it"
    );
    assert_eq!(
        env_of(&pool, &estate.nginx.id).await,
        Some((Environment::Prod, "hel1".to_owned())),
        "the sibling subtree never moved"
    );

    // Owner clears the same way, and it is asserted rather than assumed: the
    // two fields share `inherited`'s walk but not the write, and
    // `AssetEdit::Owner { value: None }` is a different statement from
    // `AssetEdit::Environment { value: None }`.
    for (id, owner) in [(&estate.site.id, "Björn"), (&estate.db.id, "Platform")] {
        edit_one(
            &pool,
            id,
            AssetEdit::Owner {
                value: Some(owner.to_owned()),
            },
        )
        .await;
    }
    assert_eq!(
        owner_of(&pool, &estate.postgres.id).await,
        Some(("Platform".to_owned(), "vm-db-01".to_owned()))
    );
    edit_one(&pool, &estate.db.id, AssetEdit::Owner { value: None }).await;
    assert_eq!(
        owner_of(&pool, &estate.postgres.id).await,
        Some(("Björn".to_owned(), "hel1".to_owned())),
        "clearing the VM's owner falls back to the site's"
    );
    assert_eq!(
        owner_of(&pool, &estate.nginx.id).await,
        Some(("Björn".to_owned(), "hel1".to_owned())),
        "and the sibling subtree, which never had the VM's, is where it was"
    );
}

/// A **move** changes what an asset inherits and what rolls up, without any
/// edit to either.
///
/// The negative for the two tests above, which prove "for that subtree"
/// against *writes* only: both answers are read-time walks over `parent_id`,
/// so re-parenting has to move them, and an implementation that cached either
/// on the row would pass every other test in this file and fail here.
#[tokio::test]
async fn a_move_carries_the_inherited_value_and_the_rollup_with_it() {
    let pool = pool("assets-move-rollup").await;
    let estate = two_branches(&pool).await;

    edit_one(
        &pool,
        &estate.db.id,
        AssetEdit::Environment {
            value: Some(Environment::Dev),
        },
    )
    .await;
    edit_one(
        &pool,
        &estate.app.id,
        AssetEdit::Environment {
            value: Some(Environment::Prod),
        },
    )
    .await;
    edit_one(
        &pool,
        &estate.postgres.id,
        AssetEdit::Status {
            value: AssetStatus::Down,
        },
    )
    .await;

    assert_eq!(
        env_of(&pool, &estate.postgres.id).await,
        Some((Environment::Dev, "vm-db-01".to_owned()))
    );

    assets::move_to(&pool, &estate.postgres.id, Some(&estate.app.id))
        .await
        .expect("the move");

    assert_eq!(
        env_of(&pool, &estate.postgres.id).await,
        Some((Environment::Prod, "vm-app-02".to_owned())),
        "the container inherits from where it is now, not from where it was"
    );

    let vms = assets::tree(&pool, Some(&estate.site.id))
        .await
        .expect("the VMs");
    let moved_to = vms.iter().find(|row| row.name == "vm-app-02").expect("app");
    let moved_from = vms.iter().find(|row| row.name == "vm-db-01").expect("db");
    assert_eq!(moved_to.problems_inside, 1, "the problem moved in");
    assert_eq!(moved_to.health, AssetStatus::Down);
    assert_eq!(moved_from.problems_inside, 0, "and out");
    assert_eq!(moved_from.health, AssetStatus::None);
}

/// Story 37 and story 32: effective health is the worst of an asset and
/// everything under it, and `problems_inside` counts the descendants carrying
/// one.
///
/// Read through `assets::tree`, which is the read a **column** makes -- the
/// badge is drawn on a column row, so a rollup that only worked in the pane
/// would be a rollup nobody sees.
#[tokio::test]
async fn a_column_row_reports_its_effective_health_and_what_is_wrong_inside() {
    let pool = pool("assets-rollup").await;
    let estate = two_branches(&pool).await;

    /// One column row by name, or a panic naming what the column held.
    fn find<'c>(column: &'c [AssetRow], name: &str) -> &'c AssetRow {
        column
            .iter()
            .find(|row| row.name == name)
            .unwrap_or_else(|| {
                panic!(
                    "{name} is not in the column: {:?}",
                    column.iter().map(|row| &row.name).collect::<Vec<_>>()
                )
            })
    }

    // Nothing rated: no badge anywhere, and health is `none` rather than `up`
    // -- "nobody has said" is not "it is fine".
    let top = assets::tree(&pool, None).await.expect("the top");
    assert_eq!(find(&top, "hel1").problems_inside, 0);
    assert_eq!(find(&top, "hel1").health, AssetStatus::None);
    assert_eq!(find(&top, "hel1").inside, AssetStatus::None);

    // One container down, two levels below the site.
    edit_one(
        &pool,
        &estate.postgres.id,
        AssetEdit::Status {
            value: AssetStatus::Down,
        },
    )
    .await;

    let top = assets::tree(&pool, None).await.expect("the top");
    let site = find(&top, "hel1");
    assert_eq!(site.problems_inside, 1, "a red badge of 1");
    assert_eq!(site.inside, AssetStatus::Down, "and it is red, not amber");
    assert_eq!(
        site.health,
        AssetStatus::Down,
        "the problem rolls all the way up"
    );
    assert_eq!(
        site.status,
        AssetStatus::None,
        "nobody rated the site itself -- the rollup is not a write"
    );

    let vms = assets::tree(&pool, Some(&estate.site.id))
        .await
        .expect("the VMs");
    assert_eq!(find(&vms, "vm-db-01").problems_inside, 1);
    assert_eq!(find(&vms, "vm-db-01").health, AssetStatus::Down);
    assert_eq!(
        find(&vms, "vm-app-02").problems_inside,
        0,
        "the sibling subtree holds nothing wrong"
    );
    assert_eq!(find(&vms, "vm-app-02").health, AssetStatus::None);
    assert_eq!(find(&vms, "vm-app-02").inside, AssetStatus::None);

    // The container itself: it *is* the problem, and an asset is never a
    // problem inside itself.
    let containers = assets::tree(&pool, Some(&estate.db.id))
        .await
        .expect("the containers");
    assert_eq!(find(&containers, "postgres").health, AssetStatus::Down);
    assert_eq!(find(&containers, "postgres").problems_inside, 0);
    assert_eq!(find(&containers, "postgres").inside, AssetStatus::None);
    assert_eq!(find(&containers, "redis").health, AssetStatus::None);

    // A warn in the other branch: an amber badge of 1 there, and the site's
    // count grows to two while staying red -- the worst wins, the count adds.
    edit_one(
        &pool,
        &estate.nginx.id,
        AssetEdit::Status {
            value: AssetStatus::Warn,
        },
    )
    .await;
    let vms = assets::tree(&pool, Some(&estate.site.id))
        .await
        .expect("the VMs");
    assert_eq!(
        find(&vms, "vm-app-02").problems_inside,
        1,
        "an amber badge of 1"
    );
    assert_eq!(find(&vms, "vm-app-02").inside, AssetStatus::Warn);
    assert_eq!(find(&vms, "vm-app-02").health, AssetStatus::Warn);

    let top = assets::tree(&pool, None).await.expect("the top");
    assert_eq!(find(&top, "hel1").problems_inside, 2);
    assert_eq!(
        find(&top, "hel1").inside,
        AssetStatus::Down,
        "two problems, the worse of them red"
    );

    // The case that makes `inside` a field of its own: an asset worse than
    // what it holds. The VM goes down while its one container is only
    // warning, so the row is `down` and the badge is **amber** -- the badge is
    // about what is inside.
    edit_one(
        &pool,
        &estate.app.id,
        AssetEdit::Status {
            value: AssetStatus::Down,
        },
    )
    .await;
    let vms = assets::tree(&pool, Some(&estate.site.id))
        .await
        .expect("the VMs");
    let app = find(&vms, "vm-app-02");
    assert_eq!(app.health, AssetStatus::Down);
    assert_eq!(app.problems_inside, 1);
    assert_eq!(
        app.inside,
        AssetStatus::Warn,
        "the VM's own trouble is not a problem *inside* it"
    );

    // Story 37 orders `up` **above** `none`, so an asset nobody has rated
    // reads `up` when the only thing under it is up. A fresh branch of its
    // own, because every asset in the estate above is now rated or holds
    // something that is: a leaf whose own status is `up` reads `up` under any
    // ordering, and would witness nothing.
    let spare = make(&pool, None, "site", "spare", &[]).await;
    let lone = make(&pool, Some(&spare.id), "vm", "vm-spare-01", &[]).await;
    edit_one(
        &pool,
        &lone.id,
        AssetEdit::Status {
            value: AssetStatus::Up,
        },
    )
    .await;
    let top = assets::tree(&pool, None).await.expect("the top");
    let unrated = find(&top, "spare");
    assert_eq!(unrated.status, AssetStatus::None, "nobody rated the site");
    assert_eq!(
        unrated.health,
        AssetStatus::Up,
        "and `up` is worse than nobody-has-said, so the rollup reports it"
    );
    assert_eq!(unrated.inside, AssetStatus::Up);
    assert_eq!(unrated.problems_inside, 0, "`up` is not a problem");

    // And the pane's own row carries the same numbers as the column's, so the
    // reader is never told two different things about one asset.
    // Three by now, not two: the VM that went down is itself a problem inside
    // the site, on top of the two containers.
    let pane = assets::get(&pool, &estate.site.id).await.expect("pane");
    assert_eq!(pane.asset.problems_inside, 3);
    assert_eq!(pane.asset.health, AssetStatus::Down);
    assert_eq!(
        pane.holds
            .iter()
            .map(|row| (row.name.as_str(), row.problems_inside))
            .collect::<Vec<_>>(),
        [("vm-app-02", 1), ("vm-db-01", 1)],
        "what it holds carries its own counts"
    );
    let deep = assets::get(&pool, &estate.nginx.id).await.expect("pane");
    assert_eq!(
        deep.held_by
            .iter()
            .map(|row| (row.name.as_str(), row.problems_inside))
            .collect::<Vec<_>>(),
        [("hel1", 3), ("vm-app-02", 1)],
        "and so does every ancestor on the path"
    );
}

// ---------------------------------------------------------------------------
// Assets in a room (#434)
// ---------------------------------------------------------------------------

/// An ad-hoc context and an explicit *Add to context* -- which is an ordinary
/// confirmed link, spec §5a.
async fn context_holding(pool: &PgPool, title: &str, member: &str) -> String {
    let ctx = knobas_core::context::create_adhoc(pool, title)
        .await
        .expect("the context");
    add(pool, &ctx.id, member).await;
    ctx.id
}

/// A confirmed link between two entities, drawn by hand.
async fn add(pool: &PgPool, from: &str, to: &str) {
    knobas_core::link::create(
        pool,
        &knobas_core::entity::EntityRef::parse(from).expect("a ref"),
        &knobas_core::entity::EntityRef::parse(to).expect("a ref"),
        "related",
        knobas_core::link::Origin::Manual,
        None,
        "user",
    )
    .await
    .expect("the link");
}

/// A mirrored ticket: the entity row a link needs and the `sync.item` row that
/// makes it a live one.
async fn ticket(pool: &PgPool, key: &str) -> String {
    let id = format!("jira:{key}");
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1,'ticket',$2)")
        .bind(&id)
        .bind(key)
        .execute(pool)
        .await
        .expect("the entity row");
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload)
         values ($1,'jira','ticket',$2,'','{}'::jsonb)",
    )
    .bind(&id)
    .bind(key)
    .execute(pool)
    .await
    .expect("the mirror row");
    id
}

/// What the tile draws, in the order it draws it: `(name, path)`.
async fn tile(pool: &PgPool, ctx: &str) -> Vec<(String, Option<String>)> {
    assets::in_context(pool, ctx)
        .await
        .expect("the tile's read")
        .into_iter()
        .map(|row| (row.asset.name, row.path))
        .collect()
}

/// Story 41 and story 48 at the tile: a VM added to a context brings what it
/// holds, each row carries the path it sits at, and the worst row is first.
///
/// `two_branches` is the fixture because the claim needs a **sibling subtree**
/// to leave alone: a single chain cannot tell "the members are this VM's
/// descendants" from "the members are every asset in the estate".
///
/// The order is the assertion a name-only sort fails: `postgres` is
/// alphabetically first and is drawn **last**, because it is the only one of
/// the three that is well. The two `down` rows are separated by name, which is
/// the tiebreak, and `vm-db-01` is `down` only through what it holds -- so a
/// sort over the asset's *own* status would put it elsewhere.
#[tokio::test]
async fn a_stored_rooms_tile_lists_the_member_assets_with_their_path_worst_first() {
    let pool = pool("assets-in-context").await;
    let estate = two_branches(&pool).await;
    edit_one(
        &pool,
        &estate.redis.id,
        AssetEdit::Status {
            value: AssetStatus::Down,
        },
    )
    .await;
    edit_one(
        &pool,
        &estate.postgres.id,
        AssetEdit::Status {
            value: AssetStatus::Up,
        },
    )
    .await;

    let ctx = context_holding(&pool, "payments stack", &estate.db.id).await;

    assert_eq!(
        tile(&pool, &ctx).await,
        vec![
            ("redis".to_owned(), Some("hel1 / vm-db-01".to_owned())),
            ("vm-db-01".to_owned(), Some("hel1".to_owned())),
            ("postgres".to_owned(), Some("hel1 / vm-db-01".to_owned())),
        ]
    );

    // The far edges: the site *above* the VM is not brought in, and neither is
    // the other branch -- membership counts through ancestors, not through
    // whatever else the estate holds.
    let drawn: Vec<String> = tile(&pool, &ctx)
        .await
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    for absent in [&estate.site, &estate.app, &estate.nginx] {
        assert!(
            !drawn.contains(&absent.name),
            "{} is outside the rule: {drawn:?}",
            absent.name
        );
    }
}

/// Story 42: *"linking an asset to a ticket adds the asset to that ticket's
/// contexts"*, which is what makes the tile fill itself -- and it is
/// **computed**, per spec #427, so the link the reader draws is the only row
/// written and removing it empties the tile again.
#[tokio::test]
async fn linking_an_asset_to_a_member_ticket_fills_the_tile_and_unlinking_empties_it() {
    let pool = pool("assets-implied").await;
    let estate = two_branches(&pool).await;
    let ticket = ticket(&pool, "PAY-1").await;
    let ctx = context_holding(&pool, "payout retries", &ticket).await;

    assert_eq!(tile(&pool, &ctx).await, vec![], "no asset is linked yet");

    let link = knobas_core::link::create(
        &pool,
        &knobas_core::entity::EntityRef::parse(&ticket).expect("a ref"),
        &knobas_core::entity::EntityRef::parse(&estate.db.id).expect("a ref"),
        "runs-on",
        knobas_core::link::Origin::Manual,
        None,
        "user",
    )
    .await
    .expect("the link");

    assert_eq!(
        tile(&pool, &ctx)
            .await
            .into_iter()
            .map(|(name, _)| name)
            .collect::<Vec<_>>(),
        vec!["postgres", "redis", "vm-db-01"],
        "the VM and both containers it holds"
    );

    knobas_core::link::unlink(&pool, link.id)
        .await
        .expect("unlink");
    assert_eq!(
        tile(&pool, &ctx).await,
        vec![],
        "and the tile empties again"
    );
}

/// An address can outlive the thing it names, so a room whose context is gone
/// draws an empty tile rather than an error -- `context::member_ids`' own
/// behaviour, carried through.
#[tokio::test]
async fn a_context_that_is_not_there_draws_an_empty_tile() {
    let pool = pool("assets-no-context").await;
    two_branches(&pool).await;
    assert_eq!(tile(&pool, "ctx:gone").await, vec![]);
}

// ---------------------------------------------------------------------------
// Routes, read from both ends (#432)
// ---------------------------------------------------------------------------

/// The estate the route tests work over: [`three_levels`] plus the thing that
/// exposes routes.
///
/// **site hel1 → VM vm-db-01 → { container postgres, reverse proxy traefik }**.
/// The proxy is a sibling of the container rather than a level of its own,
/// which is where the real estate keeps one, and it is what makes "exposed by
/// one asset, landing on another" a sentence this fixture can say.
async fn with_a_proxy(pool: &PgPool) -> (AssetRow, AssetRow, AssetRow, AssetRow) {
    let (site, vm, container) = three_levels(pool).await;
    let proxy = make(pool, Some(&vm.id), "reverse_proxy", "traefik", &[]).await;
    (site, vm, container, proxy)
}

/// One route as the reader would type it: exposed by the proxy, landing on the
/// container.
async fn expose(
    pool: &PgPool,
    asset: &AssetRow,
    name: &str,
    url: &str,
    target: Option<&AssetRow>,
) -> assets::RouteRow {
    assets::create_route(
        pool,
        &asset.id,
        name,
        url,
        target.map(|row| row.id.as_str()),
        assets::Visibility::Internal,
        &[],
    )
    .await
    .unwrap_or_else(|error| panic!("expose {name}: {}", error.message))
    .value
}

/// What one asset's pane says under *Exposes* and under *Reachable via*.
async fn both_ends(pool: &PgPool, id: &str) -> (Vec<String>, Vec<(String, String)>) {
    let detail = assets::get(pool, id).await.expect("the pane");
    (
        detail.exposes.into_iter().map(|route| route.name).collect(),
        detail
            .reachable_via
            .into_iter()
            .map(|route| (route.name, route.target_name.unwrap_or_default()))
            .collect(),
    )
}

/// The acceptance criterion, end to end: a route on a reverse proxy landing on
/// a container is read by the proxy that exposes it, by the container it lands
/// on, **and by every other asset on the container's containment path** --
/// the VM that holds it and the site above that.
///
/// The last of those is the reading `assets::ROUTES_REACHABLE` argues for and
/// this ticket's criterion asks for in as many words. It is what the real
/// estate needs: every route in `testenv/hetzner/estate.json` lands on a
/// container, so under the narrower reading no server in it would read one.
///
/// The proxy is the **negative control** in the same fixture: it exposes the
/// route and is not on the container's path, so its *Reachable via* is empty.
/// Without it, a read that answered with every route in the database would
/// pass every other assertion here.
#[tokio::test]
async fn a_route_reads_from_both_ends_and_from_every_asset_on_the_path() {
    let pool = pool("routes-both-ends").await;
    let (site, vm, container, proxy) = with_a_proxy(&pool).await;
    let route = expose(
        &pool,
        &proxy,
        "Postgres UI",
        "https://pg.hel1.internal/",
        Some(&container),
    )
    .await;

    assert!(route.id.starts_with("route:"), "{}", route.id);
    assert_eq!(route.asset_name, "traefik", "the end that exposes it");
    assert_eq!(route.target_name.as_deref(), Some("postgres"));
    assert_eq!(route.visibility, assets::Visibility::Internal);

    // The exposing end.
    let (exposes, reachable) = both_ends(&pool, &proxy.id).await;
    assert_eq!(exposes, ["Postgres UI"], "the proxy exposes it");
    assert!(
        reachable.is_empty(),
        "and is not reached by it: {reachable:?}"
    );

    // The landing end, and everything that holds it.
    for (asset, who) in [
        (&container, "the asset it lands on"),
        (&vm, "the VM that holds the container"),
        (&site, "and the site above that"),
    ] {
        let (exposes, reachable) = both_ends(&pool, &asset.id).await;
        assert!(exposes.is_empty(), "{who} exposes nothing: {exposes:?}");
        assert_eq!(
            reachable,
            [("Postgres UI".to_owned(), "postgres".to_owned())],
            "{who} reads it under reachable via, with the asset it lands on named"
        );
    }

    // Which end a row is on is read off `target_id` and nothing else: on the
    // container it is the asset's own id, and on the VM it is not -- which is
    // how the pane says "lands here" against "through postgres" and how story
    // 31's wire knows to be dashed.
    let here = assets::get(&pool, &container.id)
        .await
        .expect("the container");
    assert_eq!(
        here.reachable_via[0].target_id.as_deref(),
        Some(container.id.as_str())
    );
    let above = assets::get(&pool, &vm.id).await.expect("the VM");
    assert_ne!(
        above.reachable_via[0].target_id.as_deref(),
        Some(vm.id.as_str())
    );
}

/// A route with no target is a route: an endpoint that lands on nothing knobas
/// knows still reads under *Exposes*, and reaches nobody.
///
/// The two-route fixture is what makes the second half a claim: the targeted
/// route is in the same list on the same asset, so "reachable via is empty
/// everywhere" cannot be true by the read being broken.
#[tokio::test]
async fn a_route_with_no_target_reads_under_exposes_only() {
    let pool = pool("routes-no-target").await;
    let (_, _, container, proxy) = with_a_proxy(&pool).await;
    let endpoint = expose(
        &pool,
        &proxy,
        "Traefik dashboard",
        "http://10.0.0.4:8080/dashboard",
        None,
    )
    .await;
    expose(
        &pool,
        &proxy,
        "Postgres UI",
        "https://pg.hel1.internal/",
        Some(&container),
    )
    .await;

    assert_eq!(endpoint.target_id, None);
    assert_eq!(endpoint.target_name, None);

    // A blank target is the wire's other spelling of "no target", and the two
    // commands agree about it: `RouteEdit::Target` has always trimmed and
    // dropped an empty one, and `create_route` does now -- before, the same
    // value cleared the target on an edit and answered `not_found` on a
    // create.
    let blank_target = assets::create_route(
        &pool,
        &proxy.id,
        "Kuma",
        "https://kuma.hel1.internal/",
        Some("   "),
        assets::Visibility::Internal,
        &[],
    )
    .await
    .expect("a blank target is no target, not a missing asset")
    .value;
    assert_eq!(blank_target.target_id, None);

    let (exposes, reachable) = both_ends(&pool, &proxy.id).await;
    assert_eq!(
        exposes,
        ["Kuma", "Postgres UI", "Traefik dashboard"],
        "all three, by name"
    );
    assert!(reachable.is_empty(), "{reachable:?}");

    // The one with a target still reaches its end, and the one without reaches
    // nowhere -- which is the whole difference between them.
    let (_, reachable) = both_ends(&pool, &container.id).await;
    assert_eq!(
        reachable,
        [("Postgres UI".to_owned(), "postgres".to_owned())]
    );
}

/// Editing the target moves the route from one end to the other, and both
/// ends say so on the next read.
///
/// Three states over one route -- landing on the container, landing on the
/// proxy itself, landing on nothing -- because the middle one is the case a
/// self-target refusal would have made impossible, and the estate has one: a
/// reverse proxy's own dashboard is exposed by the proxy and reached at the
/// proxy.
#[tokio::test]
async fn editing_the_target_updates_both_ends_and_writes_the_line() {
    let pool = pool("routes-edit-target").await;
    let (_, vm, container, proxy) = with_a_proxy(&pool).await;
    let route = expose(
        &pool,
        &proxy,
        "Postgres UI",
        "https://pg.hel1.internal/",
        Some(&container),
    )
    .await;

    // Onto the proxy itself: the container stops reading it and so does the
    // VM, because the VM held the *container*, and the proxy now reads it at
    // both ends at once.
    let moved = assets::edit_route(
        &pool,
        &route.id,
        &[assets::RouteEdit::Target {
            value: Some(proxy.id.clone()),
        }],
    )
    .await
    .expect("the edit")
    .value;
    assert_eq!(moved.target_name.as_deref(), Some("traefik"));

    let (exposes, reachable) = both_ends(&pool, &proxy.id).await;
    assert_eq!(exposes, ["Postgres UI"]);
    assert_eq!(
        reachable,
        [("Postgres UI".to_owned(), "traefik".to_owned())],
        "a route may land on the asset that exposes it"
    );
    let (_, gone) = both_ends(&pool, &container.id).await;
    assert!(gone.is_empty(), "the container is not reached any more");
    // The VM still reads it -- not because the container does, but because the
    // proxy is on the VM. Asserted so that the line above cannot be read as
    // "the whole path forgot it".
    let (_, still) = both_ends(&pool, &vm.id).await;
    assert_eq!(still.len(), 1, "the proxy is inside the VM too");

    // And cleared: `null` is an unambiguous clear, and nothing reaches it.
    let cleared = assets::edit_route(
        &pool,
        &route.id,
        &[assets::RouteEdit::Target { value: None }],
    )
    .await
    .expect("the clear")
    .value;
    assert_eq!(cleared.target_id, None);
    let (_, none) = both_ends(&pool, &proxy.id).await;
    assert!(none.is_empty(), "{none:?}");

    // Story 11 for the route's own history: each edit is a line with a `from`
    // and a `to`, newest first.
    let lines: Vec<(String, serde_json::Value)> = assets::get_route(&pool, &route.id)
        .await
        .expect("the route")
        .history
        .into_iter()
        .map(|line| (line.verb, line.detail))
        .collect();
    assert_eq!(
        lines
            .iter()
            .map(|(verb, _)| verb.as_str())
            .collect::<Vec<_>>(),
        ["edited", "edited", "created"]
    );
    assert_eq!(lines[0].1["field"], serde_json::json!("target"));
    assert_eq!(lines[0].1["from"], serde_json::json!(proxy.id));
    assert_eq!(lines[0].1["to"], serde_json::Value::Null);
    assert_eq!(lines[1].1["from"], serde_json::json!(container.id));
    assert_eq!(lines[1].1["to"], serde_json::json!(proxy.id));
}

/// The other four edits, and the one that is not there.
///
/// A rename moves the entity's title with it -- the launcher reads that
/// column, so a route renamed in the pane and not in `knobas.entity` would be
/// two names for one thing. An edit that changes nothing writes no line, which
/// is [`assets::edit`]'s rule applied to the other entity this module writes.
#[tokio::test]
async fn a_route_is_renamed_re_pointed_and_re_classified_one_line_each() {
    let pool = pool("routes-edit").await;
    let (_, _, container, proxy) = with_a_proxy(&pool).await;
    let route = expose(
        &pool,
        &proxy,
        "Postgres UI",
        "https://pg.hel1.internal/",
        Some(&container),
    )
    .await;

    let edited = assets::edit_route(
        &pool,
        &route.id,
        &[
            assets::RouteEdit::Name {
                value: "  Postgres console  ".to_owned(),
            },
            assets::RouteEdit::Url {
                value: "https://pg.hel1.internal/console".to_owned(),
            },
            assets::RouteEdit::Visibility {
                value: assets::Visibility::Public,
            },
            assets::RouteEdit::Property {
                key: "cert_expires".to_owned(),
                value: Some(PropertyValue::Date {
                    value: "2026-12-01".to_owned(),
                }),
            },
        ],
    )
    .await
    .expect("the edits")
    .value;

    assert_eq!(edited.name, "Postgres console", "trimmed");
    assert_eq!(edited.url, "https://pg.hel1.internal/console");
    assert_eq!(edited.visibility, assets::Visibility::Public);
    assert_eq!(
        edited
            .properties
            .iter()
            .map(|p| (p.key.as_str(), p.custom))
            .collect::<Vec<_>>(),
        [("cert_expires", true)],
        "a route declares no schema, so the expiry is the reader's own key"
    );

    let title: String = sqlx::query_scalar("select title from knobas.entity where id = $1")
        .bind(&route.id)
        .fetch_one(&pool)
        .await
        .expect("the entity row");
    assert_eq!(title, "Postgres console", "the address's title moved too");

    let before = assets::get_route(&pool, &route.id)
        .await
        .unwrap()
        .history
        .len();
    let again = assets::edit_route(
        &pool,
        &route.id,
        &[assets::RouteEdit::Visibility {
            value: assets::Visibility::Public,
        }],
    )
    .await
    .expect("a second edit")
    .activity;
    assert!(
        again.is_empty(),
        "an edit that changes nothing writes no line"
    );
    assert_eq!(
        assets::get_route(&pool, &route.id)
            .await
            .unwrap()
            .history
            .len(),
        before
    );
}

/// A route goes, and its address is kept and marked -- the treatment an asset
/// and a note get, so a link drawn to it stays visible rather than dangling.
#[tokio::test]
async fn a_route_is_deleted_and_its_entity_row_is_tombstoned() {
    let pool = pool("routes-delete").await;
    let (_, _, container, proxy) = with_a_proxy(&pool).await;
    let route = expose(
        &pool,
        &proxy,
        "Postgres UI",
        "https://pg.hel1.internal/",
        Some(&container),
    )
    .await;

    assets::delete_route(&pool, &route.id)
        .await
        .expect("the delete");

    assert_eq!(
        code(&assets::get_route(&pool, &route.id).await.unwrap_err()),
        IpcErrorCode::NotFound
    );
    let (exposes, _) = both_ends(&pool, &proxy.id).await;
    assert!(exposes.is_empty(), "{exposes:?}");
    let (_, reachable) = both_ends(&pool, &container.id).await;
    assert!(reachable.is_empty(), "{reachable:?}");

    let tombstoned: Option<chrono::DateTime<chrono::Utc>> =
        sqlx::query_scalar("select deleted_at from knobas.entity where id = $1")
            .bind(&route.id)
            .fetch_one(&pool)
            .await
            .expect("the entity row survives");
    assert!(tombstoned.is_some(), "the address is kept and marked");

    // Read from `knobas.activity` directly: `get_route` can never read this
    // line back, so dropping the `record_with` call would otherwise leave this
    // file green.
    let (verb, detail): (String, serde_json::Value) = sqlx::query_as(
        "select verb, detail from knobas.activity
          where entity_id = $1 order by id desc limit 1",
    )
    .bind(&route.id)
    .fetch_one(&pool)
    .await
    .expect("the delete's own line");
    assert_eq!(verb, "deleted");
    assert_eq!(detail["route"]["name"], serde_json::json!("Postgres UI"));
}

/// The two ways an asset and a route meet at a delete, and they are
/// deliberately different.
///
/// An asset that still **exposes** routes is refused by name and by count: a
/// route with nothing answering it is not a row this model can hold. An asset
/// a route only **lands on** is deleted, and the route survives as an endpoint
/// that lands on nothing knobas knows -- with a line of its own saying the
/// target went, because a `set null` performed by a constraint is a change
/// nobody wrote down.
#[tokio::test]
async fn deleting_an_asset_refuses_the_routes_it_exposes_and_clears_the_ones_it_answers() {
    let pool = pool("routes-asset-delete").await;
    let (_, _, container, proxy) = with_a_proxy(&pool).await;
    let route = expose(
        &pool,
        &proxy,
        "Postgres UI",
        "https://pg.hel1.internal/",
        Some(&container),
    )
    .await;

    let refused = assets::delete(&pool, &proxy.id).await.unwrap_err();
    assert_eq!(code(&refused), IpcErrorCode::Conflict);
    assert!(refused.message.contains("traefik"), "{}", refused.message);
    assert!(refused.message.contains('1'), "{}", refused.message);
    assets::get(&pool, &proxy.id)
        .await
        .expect("nothing was removed by the attempt");

    // The other end: the container goes, and the route stays.
    assets::delete(&pool, &container.id)
        .await
        .expect("an asset a route lands on is still a leaf");
    let orphaned = assets::get_route(&pool, &route.id)
        .await
        .expect("the route");
    assert_eq!(orphaned.route.target_id, None, "it lands on nothing now");
    assert_eq!(
        orphaned
            .history
            .first()
            .map(|line| (line.verb.as_str(), line.detail["field"].clone())),
        Some(("edited", serde_json::json!("target"))),
        "and the route says so in its own history"
    );
    assert_eq!(
        orphaned.history[0].detail["from"],
        serde_json::json!(container.id)
    );

    // Which leaves the proxy deletable, once its route goes.
    assets::delete_route(&pool, &route.id)
        .await
        .expect("the route");
    assets::delete(&pool, &proxy.id).await.expect("now a leaf");
}

/// A route is an **entity**: `route:` namespace, kind `route`, one row in
/// `knobas.entity` carrying the title -- which is what gives it the
/// `#/route/<id>` address story 14 asks for, and what puts it in the
/// launcher's corpus.
///
/// The `fts` half is the corpus' own claim read at the source: a route is
/// matched on its **name and its URL** (`knobas_search::corpus::ROUTE`), so a
/// query for a host or a port finds the route that carries it. The negative is
/// in the same assertion: the site's name is nowhere in the route's own
/// columns and does not match, which is the difference between this corpus and
/// `ASSET`'s ancestor-matching one.
#[tokio::test]
async fn a_route_is_an_entity_in_the_namespace_knobas_reserves() {
    let pool = pool("routes-entity").await;
    let (_, _, container, proxy) = with_a_proxy(&pool).await;
    let route = expose(
        &pool,
        &proxy,
        "Postgres UI",
        "https://pg.hel1.internal/",
        Some(&container),
    )
    .await;

    let (kind, title): (String, String) =
        sqlx::query_as("select kind, title from knobas.entity where id = $1")
            .bind(&route.id)
            .fetch_one(&pool)
            .await
            .expect("the entity row");
    assert_eq!(kind, "route");
    assert_eq!(title, "Postgres UI");
    assert!(knobas_core::entity::is_owned_kind(&kind));
    assert!(knobas_core::entity::is_reserved_namespace("route"));

    for (query, hit) in [
        ("Postgres", true),
        ("pg.hel1.internal", true),
        ("hel1", false),
    ] {
        let matched: Vec<String> = sqlx::query_scalar(
            "select name from knobas.route
              where fts @@ websearch_to_tsquery('english', $1)",
        )
        .bind(query)
        .fetch_all(&pool)
        .await
        .expect("the index");
        assert_eq!(
            !matched.is_empty(),
            hit,
            "{query:?} against a route's own name and url: {matched:?}"
        );
    }
}

/// What a route refuses, each for its own reason and each by name.
#[tokio::test]
async fn a_route_that_could_not_be_read_back_is_refused() {
    let pool = pool("routes-refusals").await;
    let (_, _, container, proxy) = with_a_proxy(&pool).await;

    let blank = assets::create_route(
        &pool,
        &proxy.id,
        "   ",
        "https://pg.hel1.internal/",
        None,
        assets::Visibility::Internal,
        &[],
    )
    .await
    .unwrap_err();
    assert_eq!(code(&blank), IpcErrorCode::Invalid);
    assert!(blank.message.contains("a route"), "{}", blank.message);

    let bare = assets::create_route(
        &pool,
        &proxy.id,
        "Postgres UI",
        "pg.hel1.internal",
        None,
        assets::Visibility::Internal,
        &[],
    )
    .await
    .unwrap_err();
    assert_eq!(code(&bare), IpcErrorCode::Invalid);
    assert!(bare.message.contains("scheme"), "{}", bare.message);

    // Neither end may be an asset that is not there, and the refusal says
    // which end it was wanted for.
    let no_asset = assets::create_route(
        &pool,
        "asset:nobody",
        "Postgres UI",
        "https://pg.hel1.internal/",
        None,
        assets::Visibility::Internal,
        &[],
    )
    .await
    .unwrap_err();
    assert_eq!(code(&no_asset), IpcErrorCode::NotFound);
    assert!(no_asset.message.contains("expose"), "{}", no_asset.message);

    let no_target = assets::create_route(
        &pool,
        &proxy.id,
        "Postgres UI",
        "https://pg.hel1.internal/",
        Some("asset:nobody"),
        assets::Visibility::Internal,
        &[],
    )
    .await
    .unwrap_err();
    assert_eq!(code(&no_target), IpcErrorCode::NotFound);
    assert!(
        no_target.message.contains("land on"),
        "{}",
        no_target.message
    );

    // Nothing was written by any of the four.
    let (exposes, _) = both_ends(&pool, &proxy.id).await;
    assert!(exposes.is_empty(), "{exposes:?}");

    // And an edit refuses the same way, against a route that does exist.
    let route = expose(
        &pool,
        &proxy,
        "Postgres UI",
        "https://pg.hel1.internal/",
        Some(&container),
    )
    .await;
    for (edit, expected) in [
        (
            assets::RouteEdit::Url {
                value: "pg.hel1.internal".to_owned(),
            },
            IpcErrorCode::Invalid,
        ),
        (
            assets::RouteEdit::Target {
                value: Some("asset:nobody".to_owned()),
            },
            IpcErrorCode::NotFound,
        ),
    ] {
        let refused = assets::edit_route(&pool, &route.id, &[edit])
            .await
            .unwrap_err();
        assert_eq!(code(&refused), expected);
    }
    assert_eq!(
        assets::get_route(&pool, &route.id).await.unwrap().route.url,
        "https://pg.hel1.internal/",
        "a refused edit leaves the route as it was"
    );
    assert_eq!(
        code(&assets::get_route(&pool, "route:nobody").await.unwrap_err()),
        IpcErrorCode::NotFound
    );
}

// ---------------------------------------------------------------------------
// Links from an asset (#435)
// ---------------------------------------------------------------------------

/// A confirmed link carrying the relation the case is about.
async fn link_as(pool: &PgPool, from: &str, to: &str, relation: &str) -> uuid::Uuid {
    knobas_core::link::create(
        pool,
        &knobas_core::entity::EntityRef::parse(from).expect("a ref"),
        &knobas_core::entity::EntityRef::parse(to).expect("a ref"),
        relation,
        knobas_core::link::Origin::Manual,
        None,
        "user",
    )
    .await
    .expect("the link")
    .id
}

/// A mirrored **monitor**: what M4.1's Kuma adapter will write, written here
/// by hand because no adapter emits the kind yet.
///
/// The row is what a source room's tile reads through, so seeding it is what
/// makes "empty until M4.1" a statement about the *mirror* rather than about
/// the read -- a read that always answered with nothing would pass this
/// file's negatives and fail nothing.
async fn monitor(pool: &PgPool, source: &str, key: &str) -> String {
    monitor_reading(pool, source, key, None, None).await
}

/// The same monitor, with the two facts the pane's monitoring section draws:
/// the state word Kuma last published and the page it sits on in Kuma.
///
/// `web_url` is `sync.item`'s own column -- the one *Open in browser* renders
/// from everywhere else in this app -- so the deep link the pane offers is the
/// adapter's, not a URL this module builds out of parts.
async fn monitor_reading(
    pool: &PgPool,
    source: &str,
    key: &str,
    state: Option<&str>,
    web_url: Option<&str>,
) -> String {
    let id = format!("{source}:{key}");
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1,'monitor',$2)")
        .bind(&id)
        .bind(key)
        .execute(pool)
        .await
        .expect("the entity row");
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload, web_url)
         values ($1,$2,'monitor',$3,'',jsonb_build_object('state',$4::text),$5)",
    )
    .bind(&id)
    .bind(source)
    .bind(key)
    .bind(state)
    .bind(web_url)
    .execute(pool)
    .await
    .expect("the mirror row");
    id
}

/// How one asset's pane reads its links: `(relation, the other end, its kind)`.
async fn links_of(pool: &PgPool, id: &str) -> Vec<(String, String, String)> {
    assets::get(pool, id)
        .await
        .expect("the pane")
        .links
        .into_iter()
        .map(|entry| (entry.link.relation, entry.other.entity_id, entry.other.kind))
        .collect()
}

/// Story 36 end to end, at the seam the two panes read through: a container
/// linked to a ticket with `deployed-from` and to a VM with `runs-on` is on
/// **both** the ticket's detail and the VM's pane.
///
/// The two reads are deliberately different functions -- `get_entity_inner` is
/// what a ticket's slide-over calls and `assets::get` is what the Tree's pane
/// calls -- because story 36's claim is that they agree: one row, read
/// undirected, so neither end has to be the end it was drawn from.
///
/// The **wording** is the frontend's and is asserted there
/// (`app/src/lib/detail/relations.test.ts`: `runs-on` reads `runs on` forwards
/// and `hosts` back). What has to be true here is the part a table of English
/// cannot supply: the VM's read carries the link with `from_id` at the
/// *container*, which is the only thing that lets the pane know it is the end
/// being pointed at.
#[tokio::test]
async fn an_asset_linked_to_a_ticket_and_a_vm_is_on_both_of_their_details() {
    let pool = pool("assets-link-both-ends").await;
    let estate = two_branches(&pool).await;
    let ticket = ticket(&pool, "PAY-231").await;

    link_as(&pool, &estate.postgres.id, &ticket, "deployed-from").await;
    link_as(&pool, &estate.postgres.id, &estate.db.id, "runs-on").await;

    // The container's own pane carries both, drawn from the end it drew them.
    let mut drawn = links_of(&pool, &estate.postgres.id).await;
    drawn.sort();
    assert_eq!(
        drawn,
        vec![
            (
                "deployed-from".to_owned(),
                ticket.clone(),
                "ticket".to_owned()
            ),
            (
                "runs-on".to_owned(),
                estate.db.id.clone(),
                "asset".to_owned()
            ),
        ]
    );

    // The VM's pane carries the same row from the other side: the other end is
    // the container, and `from_id` is the container too -- which is what makes
    // the reading `hosts` rather than `runs on`.
    let vm = assets::get(&pool, &estate.db.id).await.expect("the pane");
    let held = vm
        .links
        .iter()
        .find(|entry| entry.link.relation == "runs-on")
        .expect("the VM is one end of the runs-on link");
    assert_eq!(held.other.entity_id, estate.postgres.id);
    assert_eq!(held.link.from_id, estate.postgres.id);
    assert_ne!(
        held.link.from_id, estate.db.id,
        "the VM is the end pointed at, and a pane that read it as the from end \
         would print `runs on` over a row that means `hosts`"
    );

    // And the ticket's own detail -- the other idiom, the slide-over -- shows
    // the asset under its links.
    let detail = knobas_app::commands::entity::get_entity_inner(&pool, &ticket)
        .await
        .expect("the ticket's detail");
    assert_eq!(
        detail
            .links
            .iter()
            .map(|entry| (
                entry.link.relation.as_str(),
                entry.other.entity_id.as_str(),
                entry.other.kind.as_str()
            ))
            .collect::<Vec<_>>(),
        vec![("deployed-from", estate.postgres.id.as_str(), "asset")],
        "the ticket shows the container it is deployed from"
    );
}

/// Story 32's other badge: `linked_work` counts confirmed links to **work
/// items** and to nothing else.
///
/// Every population the count has to exclude is in the fixture at once, which
/// is the only way a number can witness an exclusion: two tickets (counted), a
/// context and another asset (knobas' own, not work), and a proposal nobody
/// has accepted (not a link at all). A rule counting confirmed *links* rather
/// than links to *work* reads 4 here; one that kept the work join and forgot
/// the confirmation reads 3; one that did neither reads 5.
#[tokio::test]
async fn the_linked_work_badge_counts_confirmed_links_to_work_items_only() {
    let pool = pool("assets-linked-work").await;
    let estate = two_branches(&pool).await;
    let one = ticket(&pool, "PAY-1").await;
    let two = ticket(&pool, "PAY-2").await;
    let three = ticket(&pool, "PAY-3").await;
    let ctx = knobas_core::context::create_adhoc(&pool, "payments stack")
        .await
        .expect("the context");

    let counted = link_as(&pool, &estate.postgres.id, &one, "deployed-from").await;
    link_as(&pool, &two, &estate.postgres.id, "documented-in").await;
    link_as(&pool, &estate.postgres.id, &ctx.id, "related").await;
    link_as(&pool, &estate.postgres.id, &estate.db.id, "runs-on").await;
    propose(&pool, &estate.postgres.id, &three).await;

    assert_eq!(
        column_row(&pool, &estate.db.id, "postgres")
            .await
            .linked_work,
        2,
        "two tickets, whichever end each was drawn from"
    );
    assert_eq!(
        assets::get(&pool, &estate.postgres.id)
            .await
            .expect("the pane")
            .asset
            .linked_work,
        2,
        "and the pane says what the column says"
    );
    assert_eq!(
        column_row(&pool, &estate.site.id, "vm-db-01")
            .await
            .linked_work,
        0,
        "the VM's one link is to an asset, which is not work"
    );

    // Withdrawing the link takes its count with it: a tombstone is out of
    // `knobas.confirmed_link`.
    knobas_core::link::unlink(&pool, counted)
        .await
        .expect("unlink");
    assert_eq!(
        column_row(&pool, &estate.db.id, "postgres")
            .await
            .linked_work,
        1
    );

    // And so does the *source* withdrawing the ticket. `sync.live_item` is
    // what makes an item work, tombstone filter included -- the pane's list
    // still shows the row and marks it, because a link that dangles must not
    // vanish silently, but a number has nothing to mark.
    sqlx::query("update knobas.entity set deleted_at = now() where id = $1")
        .bind(&two)
        .execute(&pool)
        .await
        .expect("the withdrawal");
    assert_eq!(
        column_row(&pool, &estate.db.id, "postgres")
            .await
            .linked_work,
        0
    );
    assert_eq!(
        links_of(&pool, &estate.postgres.id).await.len(),
        3,
        "the withdrawn ticket, the context and the VM are all still in the pane"
    );
}

/// One row of the column `parent` holds, by name.
async fn column_row(pool: &PgPool, parent: &str, name: &str) -> AssetRow {
    assets::tree(pool, Some(parent))
        .await
        .expect("the column")
        .into_iter()
        .find(|row| row.name == name)
        .unwrap_or_else(|| panic!("no {name} in the column under {parent}"))
}

/// An unconfirmed proposal: a `knobas.link` row with no `confirmed_at`, which
/// is what `knobas.proposed_link` holds and `knobas.confirmed_link` cannot.
async fn propose(pool: &PgPool, from: &str, to: &str) {
    sqlx::query(
        "insert into knobas.link
             (from_id, to_id, relation, origin, created_by,
              confirmed_at, rule, rule_class, reason)
         values ($1,$2,'related','suggested','knobas',
              null,'exact_key','exact_key','the branch name carries the key')",
    )
    .bind(from)
    .bind(to)
    .execute(pool)
    .await
    .expect("the proposal");
}

/// **The pane's monitoring section: the monitors attached to an asset, each
/// with its state and its page in Kuma** (issue #445, spec #427 story 33's
/// *monitoring* row of the pane).
///
/// Read out of the `monitored-by` links this asset already takes part in, so
/// the section and the *Linked* panel below it can never disagree about what
/// is attached. Three negatives make that claim mean something, and each of
/// them is a way a looser read would go wrong:
///
/// * a `related` link to a monitor is **not** monitoring -- a read filtering
///   only on the other end's kind would list it;
/// * a `monitored-by` link to a **ticket** is not a monitor -- a read
///   filtering only on the relation would list it;
/// * a monitor attached to a **different** asset is not attached to this one.
///
/// The state and the deep link come from the mirror row, which is why the
/// second monitor here reads `down` and the first `up`: a section that showed
/// the same word on every row would pass a fixture of one.
#[tokio::test]
async fn the_pane_lists_the_monitors_watching_an_asset_with_their_state_and_a_link_to_kuma() {
    let pool = pool("assets-pane-monitoring").await;
    let vm = make(&pool, None, "vm", "vm-db-01", &[]).await;
    let other = make(&pool, None, "vm", "vm-web-01", &[]).await;

    let up = monitor_reading(
        &pool,
        "kuma",
        "gitea",
        Some("up"),
        Some("http://127.0.0.1:3001/dashboard/7"),
    )
    .await;
    let down = monitor_reading(
        &pool,
        "kuma",
        "confluence (tunnel)",
        Some("down"),
        Some("http://127.0.0.1:3001/dashboard/6"),
    )
    .await;
    let merely_related = monitor_reading(&pool, "kuma", "canary", Some("up"), None).await;
    let elsewhere = monitor_reading(&pool, "kuma", "jira (tunnel)", Some("up"), None).await;
    let not_a_monitor = ticket(&pool, "PAY-9").await;

    link_as(&pool, &vm.id, &up, "monitored-by").await;
    link_as(&pool, &vm.id, &down, "monitored-by").await;
    link_as(&pool, &vm.id, &merely_related, "related").await;
    link_as(&pool, &vm.id, &not_a_monitor, "monitored-by").await;
    link_as(&pool, &other.id, &elsewhere, "monitored-by").await;

    let pane = assets::get(&pool, &vm.id).await.expect("the pane");
    assert_eq!(
        pane.monitoring
            .iter()
            .map(|watch| {
                (
                    watch.entity_id.as_str(),
                    watch.name.as_str(),
                    watch.state.as_deref(),
                    watch.web_url.as_deref(),
                    watch.withdrawn,
                )
            })
            .collect::<Vec<_>>(),
        [
            (
                "kuma:confluence (tunnel)",
                "confluence (tunnel)",
                Some("down"),
                Some("http://127.0.0.1:3001/dashboard/6"),
                false,
            ),
            (
                "kuma:gitea",
                "gitea",
                Some("up"),
                Some("http://127.0.0.1:3001/dashboard/7"),
                false,
            ),
        ],
        "the two monitors watching this asset, by name, each with its state \
         and its own page in Kuma"
    );

    assert_eq!(
        assets::get(&pool, &other.id)
            .await
            .expect("the other pane")
            .monitoring
            .len(),
        1,
        "a monitor watches the asset it was attached to and no other"
    );
}

/// **A paused monitor is still attached, and says so.**
///
/// #442's own sentence: a paused monitor is absent from `/metrics`, so the
/// adapter tombstones it -- and a `monitored-by` link then points at a
/// tombstone. Dropping the row would make the pane say *nothing watches this*
/// about an asset somebody deliberately silenced a check on, which is the
/// opposite of true; `sync.live_item`'s tombstone filter would do exactly
/// that, and this read joins `knobas.entity` for the reason
/// `knobas_core::link::entries_of` does.
///
/// No state and no deep link, because a monitor Kuma no longer publishes has
/// neither -- `map::tombstone` drops the `web_url` at the source.
#[tokio::test]
async fn a_paused_monitor_stays_in_the_section_with_no_state_and_no_link() {
    let pool = pool("assets-pane-paused").await;
    let vm = make(&pool, None, "vm", "vm-db-01", &[]).await;
    let paused = monitor_reading(&pool, "kuma", "gitea", None, None).await;
    link_as(&pool, &vm.id, &paused, "monitored-by").await;

    sqlx::query("update knobas.entity set deleted_at = now() where id = $1")
        .bind(&paused)
        .execute(&pool)
        .await
        .expect("the tombstone");

    let pane = assets::get(&pool, &vm.id).await.expect("the pane");
    assert_eq!(
        pane.monitoring
            .iter()
            .map(|watch| (watch.name.as_str(), watch.state.as_deref(), watch.withdrawn))
            .collect::<Vec<_>>(),
        [("gitea", None, true)],
        "still attached, and marked as no longer in Kuma"
    );
    assert!(pane.monitoring[0].web_url.is_none());
}

/// Story 44: a source room's tile lists the assets **that source's monitors**
/// are attached to, and nothing else.
///
/// Three near misses are in the fixture, because each is a clause of the rule
/// and a read that dropped any one of them would still list `postgres`: a
/// monitor of a *different* source, an item of the right source that is not a
/// monitor, and a `related` link to the right monitor. The container's holder
/// is the fourth: attachment does not roll up the tree the way membership
/// does, so `vm-db-01` is absent though it holds the monitored container.
#[tokio::test]
async fn a_source_rooms_tile_lists_the_assets_its_own_monitors_watch() {
    let pool = pool("assets-monitored-by").await;
    let estate = two_branches(&pool).await;

    let kuma = monitor(&pool, "kuma", "1").await;
    let elsewhere = monitor(&pool, "kuma2", "1").await;
    let not_a_monitor = ticket(&pool, "PAY-9").await;

    link_as(&pool, &estate.postgres.id, &kuma, "monitored-by").await;
    link_as(&pool, &estate.redis.id, &elsewhere, "monitored-by").await;
    link_as(&pool, &estate.app.id, &not_a_monitor, "monitored-by").await;
    link_as(&pool, &estate.nginx.id, &kuma, "related").await;

    assert_eq!(
        assets::monitored_by(&pool, "kuma")
            .await
            .expect("the tile's read")
            .into_iter()
            .map(|row| (row.asset.name, row.path))
            .collect::<Vec<_>>(),
        vec![("postgres".to_owned(), Some("hel1 / vm-db-01".to_owned()))],
        "the monitored container, with the path a flat list needs"
    );

    assert_eq!(
        assets::monitored_by(&pool, "kuma2")
            .await
            .expect("the tile's read")
            .into_iter()
            .map(|row| row.asset.name)
            .collect::<Vec<_>>(),
        vec!["redis".to_owned()],
        "each source's room lists its own monitors' assets"
    );

    assert_eq!(
        assets::monitored_by(&pool, "jira")
            .await
            .expect("the tile's read")
            .len(),
        0,
        "a source with no monitors has an empty tile, not an error -- which is \
         every source until M4.1"
    );
}

/// Worst first, the same order the stored room's tile draws in, because the
/// order is the tile's and not one room kind's.
#[tokio::test]
async fn a_source_rooms_tile_draws_its_worst_asset_first() {
    let pool = pool("assets-monitored-order").await;
    let estate = two_branches(&pool).await;
    let kuma = monitor(&pool, "kuma", "1").await;

    for asset in [&estate.postgres, &estate.redis, &estate.app] {
        link_as(&pool, &asset.id, &kuma, "monitored-by").await;
    }
    edit_one(
        &pool,
        &estate.redis.id,
        AssetEdit::Status {
            value: AssetStatus::Down,
        },
    )
    .await;
    edit_one(
        &pool,
        &estate.postgres.id,
        AssetEdit::Status {
            value: AssetStatus::Up,
        },
    )
    .await;
    // `vm-app-02` is well itself and holds a container nobody has rated, so it
    // sorts between the two -- and it is alphabetically *last*, which is what
    // a name-only sort would get wrong.
    assert_eq!(
        assets::monitored_by(&pool, "kuma")
            .await
            .expect("the tile's read")
            .into_iter()
            .map(|row| row.asset.name)
            .collect::<Vec<_>>(),
        vec!["redis", "postgres", "vm-app-02"]
    );
}

// ---------------------------------------------------------------------------
// The Import (#439)
// ---------------------------------------------------------------------------

/// The real estate file, embedded.
///
/// The only asset fixture there is: spec #427 rules out anything
/// Tidewater-shaped for assets, because a made-up estate proves that the
/// import parses and not that a real estate fits the model (ADR-0013).
/// Embedded rather than read at run time, so this suite and the file's own
/// checker (`knobas-core`'s `tests/estate_file.rs`) fail together the day it
/// stops being true.
const ESTATE_FILE: &str = include_str!("../../../testenv/hetzner/estate.json");

/// One count.
async fn rows(pool: &PgPool, statement: &'static str) -> i64 {
    sqlx::query(statement)
        .fetch_one(pool)
        .await
        .expect("a count")
        .try_get("n")
        .expect("a count")
}

/// How many assets, routes, activity lines and links there are.
///
/// The four tables an import can write, counted together, so that "wrote
/// nothing" is a claim about all of them rather than about the one a reader
/// thought to check.
async fn tables(pool: &PgPool) -> (i64, i64, i64, i64) {
    (
        rows(pool, "select count(*) as n from knobas.asset").await,
        rows(pool, "select count(*) as n from knobas.route").await,
        rows(pool, "select count(*) as n from knobas.activity").await,
        rows(pool, "select count(*) as n from knobas.link").await,
    )
}

/// The estate file with one asset's properties changed -- what a file that has
/// moved on since the last import looks like.
fn estate_with(id: &str, properties: &[(&str, serde_json::Value)]) -> String {
    let mut file: serde_json::Value = serde_json::from_str(ESTATE_FILE).expect("the estate file");
    let asset = file["assets"]
        .as_array_mut()
        .expect("the assets")
        .iter_mut()
        .find(|asset| asset["id"] == id)
        .unwrap_or_else(|| panic!("{id} is in the estate file"));
    for (key, value) in properties {
        asset["properties"][*key] = value.clone();
    }
    file.to_string()
}

/// One asset's property, as the pane reads it.
async fn property(pool: &PgPool, id: &str, key: &str) -> Option<PropertyValue> {
    assets::get(pool, id)
        .await
        .unwrap_or_else(|error| panic!("{id}: {}", error.message))
        .properties
        .into_iter()
        .find(|property| property.key == key)
        .and_then(|property| property.value)
}

/// **The preview writes nothing at all** -- the first acceptance criterion,
/// and the one a reader cannot check by looking at a dialog.
///
/// Over the *real* file and over a database that already holds it, so the
/// preview has every reason to write: a property to set and a plan to record.
/// All four tables are counted, because a preview that wrote one activity line
/// would still leave the asset count right.
#[tokio::test]
async fn a_preview_writes_nothing_at_all() {
    let pool = pool("assets-import-preview-writes-nothing").await;
    assets::apply_import(&pool, ESTATE_FILE)
        .await
        .expect("the first import");
    let moved_on = estate_with(
        "asset:hetzner-teamcity",
        &[("server_type", serde_json::json!("cx33"))],
    );

    let before = tables(&pool).await;
    let preview = assets::preview_import(&pool, &moved_on)
        .await
        .expect("the preview");
    assert_eq!(
        preview.changes.len(),
        1,
        "this preview has something to report, so it has a reason to write"
    );
    assert_eq!(
        tables(&pool).await,
        before,
        "the preview wrote something: assets, routes, activity lines, links"
    );
}

/// **The first import creates the whole estate with the file's own ids**, and
/// every created asset carries an origin line.
///
/// The real file, through the seam the two commands are shims over. What is
/// asserted is what a reader would look at afterwards: the tree is the one the
/// file describes, the ids are the file's, the environment and owner set at
/// the root are in force five levels down, a plain `2` arrived as a number,
/// the entry's prose arrived as a property, and the monitor names are on the
/// assets.
#[tokio::test]
async fn the_first_import_creates_the_real_estate_with_the_files_own_ids() {
    let pool = pool("assets-import-first").await;
    let written = assets::apply_import(&pool, ESTATE_FILE)
        .await
        .expect("the estate imports");
    let outcome = written.value;

    assert_eq!(outcome.assets_created, 23, "the file's assets");
    assert_eq!(outcome.routes_created, 9, "the file's routes");
    assert_eq!(outcome.properties_set, 0, "there was nothing to change");
    assert_eq!(outcome.properties_kept, 0);
    assert_eq!(outcome.monitors_kept, 7, "the file's monitor names");
    assert_eq!(
        outcome.monitors_linked, 0,
        "no adapter emits a monitor until M4.1, so no name resolves"
    );

    // One line is announced, and it is the run's rather than an asset's.
    assert_eq!(written.activity.len(), 1, "one line is announced, not 33");
    assert_eq!(written.activity[0].verb, "imported");
    assert_eq!(
        written.activity[0].entity_id, None,
        "the summary is the import's line, not an asset's"
    );
    assert_eq!(
        written.activity[0].detail["outcome"]["assets_created"],
        serde_json::json!(23)
    );

    let top = assets::tree(&pool, None).await.expect("the top level");
    assert_eq!(
        top.iter().map(|a| a.id.as_str()).collect::<Vec<_>>(),
        ["asset:knobas-estate"],
        "the estate is one tree with one root"
    );

    // Five levels down, by the file's own ids, with the path the file draws.
    let db = assets::get(&pool, "asset:db-jira")
        .await
        .expect("Jira's database is in the tree");
    assert_eq!(
        db.held_by
            .iter()
            .map(|row| row.name.as_str())
            .collect::<Vec<_>>(),
        [
            "knobas test estate",
            "Hetzner Cloud nbg1",
            "knobas-jira",
            "Docker engine (knobas-jira)",
            "knobas-jira-db",
        ],
        "the containment path is the file's, outermost first"
    );
    assert_eq!(
        db.effective_environment.map(|value| value.value),
        Some(Environment::Dev),
        "the environment set at the root is in force five levels down"
    );
    assert_eq!(
        db.effective_owner.map(|value| value.source_id),
        Some("asset:knobas-estate".to_owned()),
        "the owner is inherited from the root, and the pane can say from where"
    );

    // The scalars, translated. A JSON number is a number property; the entry's
    // own prose is a text property, because the model has no description
    // column; and a key the *type* declares takes the type's kind, which is
    // the negative control for the pair above.
    assert_eq!(
        property(&pool, "asset:hetzner-teamcity", "vcpu").await,
        Some(PropertyValue::Number { value: 2.0 })
    );
    assert!(
        matches!(
            property(&pool, "asset:hetzner-teamcity", "description").await,
            Some(PropertyValue::Text { value }) if value.contains("SSH")
        ),
        "the file's description is a property of the asset"
    );
    assert_eq!(
        property(&pool, "asset:hetzner-teamcity", "os").await,
        Some(text("ubuntu-24.04"))
    );

    // Every created asset carries its origin line, and it names the estate.
    let gitea = assets::get(&pool, "asset:knobas-gitea")
        .await
        .expect("the Gitea container");
    assert!(
        gitea.history.iter().any(|line| line.verb == "imported"
            && line.detail["estate"] == serde_json::json!("knobas test estate")),
        "the origin line is missing: {:?}",
        gitea
            .history
            .iter()
            .map(|line| line.verb.as_str())
            .collect::<Vec<_>>()
    );
    let without = rows(
        &pool,
        "select count(*) as n from knobas.asset a
          where not exists (select 1 from knobas.activity l
                             where l.entity_id = a.id and l.verb = 'imported')",
    )
    .await;
    assert_eq!(without, 0, "{without} imported assets carry no origin line");

    // The monitor names are on the assets, waiting for M4.1 to resolve them.
    assert_eq!(gitea.monitors, ["gitea"], "the name the file gave");

    // The routes, both ends resolved, carrying the file's own properties.
    let notebook = assets::get(&pool, "asset:notebook")
        .await
        .expect("the notebook");
    assert_eq!(
        notebook.exposes.len(),
        8,
        "the notebook exposes every forward but the reverse one"
    );
    let tunnel = notebook
        .exposes
        .iter()
        .find(|route| route.id == "route:tunnel-jira")
        .expect("Jira's forward");
    assert_eq!(tunnel.url, "http://127.0.0.1:8080/");
    assert_eq!(tunnel.target_name.as_deref(), Some("knobas-jira"));
    assert!(
        tunnel
            .properties
            .iter()
            .any(|property| property.key == "forward"),
        "the route carries the file's own properties: {:?}",
        tunnel.properties
    );
}

/// **A hand-edited property survives the next import, and the preview said it
/// would** -- while an untouched property the file changed is updated.
///
/// Two properties of one asset, changed by the file in the same breath, told
/// apart by nothing but who last wrote one of them. That is spec #427's rule
/// -- *"a hand edit is any activity line by the user on that property"* -- and
/// it is why the two are asserted side by side: a merge that kept everything,
/// or set everything, would satisfy half of this test.
#[tokio::test]
async fn a_hand_edited_property_survives_the_next_import_and_the_preview_said_it_would() {
    let pool = pool("assets-import-hand-edit").await;
    assets::apply_import(&pool, ESTATE_FILE)
        .await
        .expect("the first import");

    // The reader corrects the role by hand. Nobody touches the OS.
    assets::edit(
        &pool,
        "asset:hetzner-teamcity",
        &[AssetEdit::Property {
            key: "role".to_owned(),
            value: Some(text("teamcity, and the agent")),
        }],
    )
    .await
    .expect("the hand edit");

    // The file moves on, and changes both.
    let moved_on = estate_with(
        "asset:hetzner-teamcity",
        &[
            ("role", serde_json::json!("teamcity-only")),
            ("os", serde_json::json!("ubuntu-26.04")),
        ],
    );

    let preview = assets::preview_import(&pool, &moved_on)
        .await
        .expect("the preview");
    assert_eq!(
        preview.changes.len(),
        1,
        "one asset has something to change"
    );
    let change = &preview.changes[0];
    assert_eq!(change.id, "asset:hetzner-teamcity");
    assert_eq!(
        change
            .properties
            .iter()
            .map(|property| (property.key.as_str(), property.plan))
            .collect::<Vec<_>>(),
        [
            ("os", assets::PropertyPlan::Set),
            ("role", assets::PropertyPlan::Kept),
        ],
        "the preview says, per property, which value wins"
    );
    // And it says what it would be replacing, which is the sentence the dialog
    // draws: *the file says `teamcity-only` and it stays `teamcity, and the
    // agent`, because you typed that*.
    let role = &change.properties[1];
    assert_eq!(
        role.from,
        Some(text("teamcity, and the agent")),
        "the preview carries the value that is staying"
    );
    assert_eq!(role.to, text("teamcity-only"));
    assert_eq!(role.label, "role", "a custom key is labelled by itself");

    let outcome = assets::apply_import(&pool, &moved_on)
        .await
        .expect("the second import")
        .value;
    assert_eq!(outcome.properties_set, 1);
    assert_eq!(outcome.properties_kept, 1);
    assert_eq!(outcome.assets_created, 0);

    assert_eq!(
        property(&pool, "asset:hetzner-teamcity", "role").await,
        Some(text("teamcity, and the agent")),
        "the file undid a hand edit"
    );
    assert_eq!(
        property(&pool, "asset:hetzner-teamcity", "os").await,
        Some(text("ubuntu-26.04")),
        "an untouched property the file changed was not updated"
    );
    // The import's own line is not the reader's, which is what makes the rule
    // above decidable at all: the OS it has just set is still the file's to
    // change next time.
    let history = history(&pool, "asset:hetzner-teamcity").await;
    assert!(
        history
            .iter()
            .any(|(verb, detail)| verb == "edited" && detail["key"] == serde_json::json!("os")),
        "the property the import set has a line of its own: {history:?}"
    );
}

/// **A second import of the same file previews all-known and applies
/// nothing**, and the monitor names stay on the assets.
///
/// The idempotence criterion, from both ends: the preview says every entry is
/// already in the tree and nothing would change, and the apply that follows
/// leaves every table alone but the log -- which grows by the one summary
/// line, because *"I imported that file again and it changed nothing"* is a
/// fact about the estate and a log that recorded only the imports that did
/// something could not answer when the last one ran.
#[tokio::test]
async fn a_second_import_of_the_same_file_is_all_known_and_changes_nothing() {
    let pool = pool("assets-import-idempotent").await;
    assets::apply_import(&pool, ESTATE_FILE)
        .await
        .expect("the first import");

    let preview = assets::preview_import(&pool, ESTATE_FILE)
        .await
        .expect("the second preview");
    assert_eq!(
        preview.known.len(),
        32,
        "twenty-three assets and nine routes are all already in the tree"
    );
    assert!(preview.new.is_empty(), "{:?}", preview.new);
    assert!(preview.changes.is_empty(), "{:?}", preview.changes);
    assert!(preview.monitor_links.is_empty());
    assert_eq!(preview.name, "knobas test estate");

    let (assets_before, routes_before, activity_before, links_before) = tables(&pool).await;
    let outcome = assets::apply_import(&pool, ESTATE_FILE)
        .await
        .expect("the second import")
        .value;
    assert_eq!(
        outcome,
        assets::ImportOutcome::default(),
        "a second import of an unchanged file did something"
    );
    let (assets_after, routes_after, activity_after, links_after) = tables(&pool).await;
    assert_eq!((assets_after, routes_after), (assets_before, routes_before));
    assert_eq!(links_after, links_before);
    assert_eq!(
        activity_after,
        activity_before + 1,
        "exactly one line was written: the summary"
    );

    // And the names the first import kept are still there to be resolved.
    assert_eq!(
        assets::get(&pool, "asset:knobas-teamcity")
            .await
            .expect("the TeamCity container")
            .monitors,
        ["teamcity (tunnel)"]
    );
}

/// **A monitor the mirror holds becomes a link; one it does not stays a
/// name.**
///
/// Spec #427's import sentence in both halves. The mirror is seeded by hand
/// because no adapter emits the `monitor` kind until M4.1 -- the arrangement
/// `a_source_rooms_tile_lists_the_assets_its_own_monitors_watch` uses, and for
/// its reason: the statement is the real one, and a read that answered nothing
/// whatever the mirror held would pass every other test in this file.
///
/// The negative is the other six names, which resolve to nothing and are still
/// on their assets afterwards. Without it this would pass against an import
/// that drew a link and threw the name away.
#[tokio::test]
async fn a_monitor_the_mirror_holds_becomes_a_link_and_one_it_does_not_stays_a_name() {
    let pool = pool("assets-import-monitors").await;
    monitor(&pool, "kuma", "gitea").await;

    let preview = assets::preview_import(&pool, ESTATE_FILE)
        .await
        .expect("the preview");
    assert_eq!(
        preview
            .monitor_links
            .iter()
            .map(|link| (link.asset_id.as_str(), link.monitor_name.as_str()))
            .collect::<Vec<_>>(),
        [("asset:knobas-gitea", "gitea")],
        "the one name the mirror holds is the one link the preview offers"
    );

    let outcome = assets::apply_import(&pool, ESTATE_FILE)
        .await
        .expect("the import")
        .value;
    assert_eq!(outcome.monitors_linked, 1);
    assert_eq!(
        outcome.monitors_kept, 7,
        "every name is kept, linked or not"
    );
    assert_eq!(
        links_of(&pool, "asset:knobas-gitea").await,
        [(
            "monitored-by".to_owned(),
            "kuma:gitea".to_owned(),
            "monitor".to_owned(),
        )],
        "the resolved name is a monitored-by link"
    );
    assert_eq!(
        assets::get(&pool, "asset:knobas-jira")
            .await
            .expect("Jira's container")
            .monitors,
        ["jira (tunnel)"],
        "a name the mirror does not hold is kept, for the next import"
    );

    // Run it again: the link is already there and is not drawn twice, which is
    // what `knobas.link`'s unordered uniqueness would otherwise refuse in the
    // middle of a transaction that had already done its work.
    let again = assets::apply_import(&pool, ESTATE_FILE)
        .await
        .expect("the second import")
        .value;
    assert_eq!(again.monitors_linked, 0);
    assert_eq!(links_of(&pool, "asset:knobas-gitea").await.len(), 1);
}

/// **A name that resolves to nothing is reported by *every* preview, not only
/// the one that first put it on the asset.**
///
/// Spec #427's *"a name the mirror does not hold yet is kept on the asset and
/// resolved by the next import"* has a reader on the other end of it, and
/// issue #445 says where they read it: *a name the mirror lacks is kept and
/// **reported in the preview***. `changes` cannot be that report -- it lists
/// what an apply would *write*, so a name already on the asset is absent from
/// it by construction, and the second preview of an unchanged file has an
/// empty `changes` and an empty `monitor_links` while six of the file's seven
/// names still answer to nothing.
///
/// The positive control is `gitea`: the one name the mirror holds is a link
/// and is **not** in this list, so a list that simply echoed the file's names
/// would fail here.
#[tokio::test]
async fn a_name_the_mirror_lacks_is_reported_by_every_preview_and_not_only_the_first() {
    let pool = pool("assets-import-unresolved").await;
    monitor(&pool, "kuma", "gitea").await;

    let first = assets::preview_import(&pool, ESTATE_FILE)
        .await
        .expect("the first preview");
    assert_eq!(
        first
            .unresolved
            .iter()
            .map(|entry| (entry.asset_id.as_str(), entry.monitor_name.as_str()))
            .collect::<Vec<_>>(),
        [
            ("asset:hetzner-confluence", "knobas-confluence"),
            ("asset:hetzner-jira", "knobas-jira"),
            ("asset:hetzner-teamcity", "knobas-teamcity"),
            ("asset:knobas-confluence", "confluence (tunnel)"),
            ("asset:knobas-jira", "jira (tunnel)"),
            ("asset:knobas-teamcity", "teamcity (tunnel)"),
        ],
        "every name but the one the mirror holds, by asset and then by name"
    );

    assets::apply_import(&pool, ESTATE_FILE)
        .await
        .expect("the import");

    let again = assets::preview_import(&pool, ESTATE_FILE)
        .await
        .expect("the second preview");
    assert!(
        again.changes.is_empty() && again.monitor_links.is_empty(),
        "an unchanged file writes nothing the second time -- which is exactly \
         why the report cannot live in either of those lists"
    );
    assert_eq!(
        again.unresolved, first.unresolved,
        "the six names are still kept and still reported"
    );
    assert_eq!(
        again
            .unresolved
            .iter()
            .map(|entry| entry.asset_name.as_str())
            .collect::<Vec<_>>(),
        [
            "knobas-confluence",
            "knobas-jira",
            "knobas-teamcity",
            "knobas-confluence",
            "knobas-jira",
            "knobas-teamcity",
        ],
        "the asset each name is waiting on, named as the tree names it"
    );
}

/// A file may hang a new subtree under an asset **a person made by hand**, and
/// an asset the reader deleted comes back when a file still names it.
///
/// Two failures with one shape: the plan asks whether an id is already in the
/// estate, and both of these are ids the file's own `assets` list never
/// mentions or no longer answers for.
///
/// * The **parent** case is what `REFERENCED_ASSETS` exists for. An import
///   whose ordering only knew the file's own ids would leave such an asset
///   waiting for a parent that is never placed and refuse a legal file as a
///   cycle it does not have -- and the real estate file could not catch that,
///   because every parent in it is internal.
/// * The **deleted** case is `assets::delete`'s tombstone: it removes the
///   `knobas.asset` row and leaves the `knobas.entity` one marked, so an
///   import keeping the file's id meets its own tombstone. Reviving it is the
///   answer, and the assertion that it *is* revived rather than merely
///   re-inserted is `deleted_at` reading null -- a link drawn to it stops
///   being marked withdrawn and the launcher finds it again.
#[tokio::test]
async fn a_file_reaches_an_asset_made_by_hand_and_brings_a_deleted_one_back() {
    let pool = pool("assets-import-outside-ids").await;
    let by_hand = make(&pool, None, "site", "made by hand", &[]).await;

    let under_it = format!(
        r#"{{"name":"a wing","assets":[
             {{"id":"asset:wing","type":"vm","name":"wing","parent":"{}"}},
             {{"id":"asset:wing-docker","type":"container_engine","name":"docker",
               "parent":"asset:wing"}}],"routes":[]}}"#,
        by_hand.id
    );

    let preview = assets::preview_import(&pool, &under_it)
        .await
        .expect("a subtree under an asset the estate already holds");
    assert_eq!(
        preview
            .new
            .iter()
            .map(|entry| entry.id.as_str())
            .collect::<Vec<_>>(),
        ["asset:wing", "asset:wing-docker"],
        "the parent-first order reaches through an id the file does not describe"
    );
    assets::apply_import(&pool, &under_it)
        .await
        .expect("the subtree writes");
    assert_eq!(
        assets::get(&pool, "asset:wing")
            .await
            .expect("the new asset")
            .held_by
            .iter()
            .map(|row| row.name.as_str())
            .collect::<Vec<_>>(),
        ["made by hand"],
        "the file hung its subtree under the asset a person made"
    );

    // Now the reader deletes a leaf the import created, and imports again.
    assets::delete(&pool, "asset:wing-docker")
        .await
        .expect("a leaf deletes");
    let tombstoned = rows(
        &pool,
        "select count(*) as n from knobas.entity
          where id = 'asset:wing-docker' and deleted_at is not null",
    )
    .await;
    assert_eq!(tombstoned, 1, "delete leaves the entity row marked");

    let outcome = assets::apply_import(&pool, &under_it)
        .await
        .expect("a file that still names a deleted asset brings it back")
        .value;
    assert_eq!(
        outcome.assets_created, 1,
        "only the deleted one is new again"
    );
    assert_eq!(
        rows(
            &pool,
            "select count(*) as n from knobas.entity
              where id = 'asset:wing-docker' and deleted_at is null",
        )
        .await,
        1,
        "the entity row is revived, not left marked withdrawn beside a live asset"
    );
}

/// **A file whose assets hold each other is refused before a row is written**,
/// and named.
///
/// The reason this is a test and not a comment: the recursive CTEs behind
/// `recompute_paths` and the health rollup carry no depth cap, and they hold
/// today because `create` mints a fresh id and `move_to` walks the ancestors
/// first. An import writing `parent_id` out of a file is the third writer, and
/// a cycle reaching the table would make the path rewrite run forever -- a
/// failure no gate can wait out and no timeout here would catch. So it is
/// refused where it can still be a sentence.
///
/// Asserted on **both** commands, because a preview that ordered the file
/// happily would put an *Apply* button over a file that cannot be written.
#[tokio::test]
async fn a_file_whose_assets_hold_each_other_is_refused_before_a_row_is_written() {
    let pool = pool("assets-import-cycle").await;
    let looping = r#"{"name":"a loop","assets":[
        {"id":"asset:a","type":"site","name":"a","parent":"asset:b"},
        {"id":"asset:b","type":"site","name":"b","parent":"asset:a"}],"routes":[]}"#;

    let before = tables(&pool).await;
    let refusals = [
        assets::preview_import(&pool, looping)
            .await
            .expect_err("a cycle has no order"),
        assets::apply_import(&pool, looping)
            .await
            .expect_err("a cycle has no order"),
    ];
    for refusal in &refusals {
        assert_eq!(code(refusal), IpcErrorCode::Invalid);
        assert!(
            refusal.message.contains("asset:a") && refusal.message.contains("asset:b"),
            "the refusal names the assets that hold each other: {}",
            refusal.message
        );
    }
    assert_eq!(tables(&pool).await, before, "the refusal wrote something");
}

/// Every other way a file can be wrong, refused **by name and before a
/// write**.
///
/// One test rather than seven, because each is one line of the same rule: the
/// file is read whole and refused whole. The real file is the control at the
/// end -- without it, a preview that refused everything would pass all seven
/// cases and fail nothing.
#[tokio::test]
async fn a_file_that_is_not_an_estate_file_is_refused_and_says_why() {
    let pool = pool("assets-import-refusals").await;
    let before = tables(&pool).await;

    for (why, file, named) in [
        (
            "not JSON at all",
            "{ this is not json",
            "not an estate file",
        ),
        (
            "a misspelled key",
            r#"{"assets":[{"id":"asset:a","type":"site","name":"a","parnet":"asset:b"}]}"#,
            "parnet",
        ),
        (
            "a type nobody declares",
            r#"{"assets":[{"id":"asset:a","type":"kubernetes","name":"a"}]}"#,
            "kubernetes",
        ),
        (
            "a parent that is nowhere",
            r#"{"assets":[{"id":"asset:a","type":"site","name":"a","parent":"asset:gone"}]}"#,
            "asset:gone",
        ),
        (
            "a route landing on nothing",
            r#"{"assets":[{"id":"asset:a","type":"site","name":"a"}],
                "routes":[{"id":"route:r","asset":"asset:a","target":"asset:gone",
                           "name":"r","url":"https://r/"}]}"#,
            "asset:gone",
        ),
        (
            "an id in the wrong namespace",
            r#"{"assets":[{"id":"route:a","type":"site","name":"a"}]}"#,
            "namespace",
        ),
        (
            "one id on two entries",
            r#"{"assets":[{"id":"asset:a","type":"site","name":"a"},
                          {"id":"asset:a","type":"site","name":"b"}]}"#,
            "two entries",
        ),
    ] {
        let refusal = match assets::preview_import(&pool, file).await {
            Ok(preview) => panic!("{why} was accepted: {preview:?}"),
            Err(refusal) => refusal,
        };
        assert_eq!(code(&refusal), IpcErrorCode::Invalid, "{why}");
        assert!(
            refusal.message.contains(named),
            "{why}: the refusal does not name {named:?}: {}",
            refusal.message
        );
    }

    assert!(
        assets::preview_import(&pool, ESTATE_FILE).await.is_ok(),
        "the real estate file is not one of the seven"
    );
    assert_eq!(tables(&pool).await, before, "a refused preview wrote a row");
}

// ---------------------------------------------------------------------------
// The wiring: registered, named, and decoding.
// ---------------------------------------------------------------------------

/// Invoke `cmd` on a mock app that manages a `Lifecycle` with no pool.
///
/// Every asset command *that reads or writes* asks `lifecycle.pool()?` first,
/// so a call with nothing ready reaches the *body* and answers `not_ready`.
/// That is the marker for "registered and dispatched", as distinct from "no
/// such command" -- and it is only reachable because none of them declares the
/// state as an argument (carry-over §10.6(a)). `asset_types` is the one that
/// needs no pool and therefore **answers**, which is why the success value is
/// the response body rather than a placeholder.
fn invoke(cmd: &str, body: serde_json::Value) -> Result<serde_json::Value, String> {
    let app = tauri::test::mock_builder()
        .invoke_handler(tauri::generate_handler![
            knobas_app::commands::assets::asset_tree,
            knobas_app::commands::assets::get_asset,
            knobas_app::commands::assets::create_asset,
            knobas_app::commands::assets::edit_asset,
            knobas_app::commands::assets::move_asset,
            knobas_app::commands::assets::delete_asset,
            knobas_app::commands::assets::asset_types,
            knobas_app::commands::assets::context_assets,
            knobas_app::commands::assets::get_route,
            knobas_app::commands::assets::create_route,
            knobas_app::commands::assets::edit_route,
            knobas_app::commands::assets::delete_route,
            knobas_app::commands::assets::source_assets,
            knobas_app::commands::assets::preview_estate_import,
            knobas_app::commands::assets::apply_estate_import,
        ])
        .build(tauri::test::mock_context(tauri::test::noop_assets()))
        .expect("mock app");
    app.manage(knobas_app::Lifecycle::new());
    let webview: tauri::WebviewWindow<MockRuntime> =
        tauri::WebviewWindowBuilder::new(&app, "main", tauri::WebviewUrl::default())
            .build()
            .expect("mock webview");

    let outcome = tauri::test::get_ipc_response(
        &webview,
        tauri::webview::InvokeRequest {
            cmd: cmd.to_owned(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            url: LOCAL_ORIGIN.parse().expect("url"),
            body: body.into(),
            headers: Default::default(),
            invoke_key: tauri::test::INVOKE_KEY.to_string(),
        },
    )
    .map_err(|error| format!("{error:?}"));

    if let Err(rejection) = &outcome {
        assert!(
            !rejection.contains("not allowed"),
            "the permission check refused {cmd} before its arguments were decoded, \
             so this proves nothing about decoding: {rejection}"
        );
    }
    outcome.and_then(|answered| {
        answered
            .deserialize::<serde_json::Value>()
            .map_err(|error| format!("the answer is not JSON: {error}"))
    })
}

use tauri::Manager;

/// Every asset command is registered under the name the mirror invokes, and
/// its argument list decodes.
///
/// The mistake this catches is the one an append-only handler list invites:
/// adding a command and forgetting the list, which is a frontend failing at
/// run time with "command not found" against a Rust side that compiles.
///
/// Each command appears **twice** where it has an optional argument -- with it
/// and without it -- because an `Option` declared as a plain `String` would be
/// refused by name in the second call and by nothing in the first.
#[test]
fn every_asset_command_is_registered_and_its_arguments_decode() {
    for (cmd, args) in [
        ("asset_tree", serde_json::json!({})),
        (
            "asset_tree",
            serde_json::json!({ "parentId": "asset:hel1" }),
        ),
        ("asset_tree", serde_json::json!({ "parentId": null })),
        ("get_asset", serde_json::json!({ "assetId": "asset:7f2c" })),
        (
            "create_asset",
            serde_json::json!({ "typeId": "vm", "name": "vm-db-01" }),
        ),
        (
            "create_asset",
            serde_json::json!({
                "parentId": "asset:hel1", "typeId": "vm", "name": "vm-db-01",
                "properties": [["ip", { "kind": "text", "value": "10.0.0.4" }]],
            }),
        ),
        (
            "edit_asset",
            serde_json::json!({
                "assetId": "asset:7f2c",
                "edits": [
                    { "field": "name", "value": "vm-db-02" },
                    { "field": "status", "value": "warn" },
                    { "field": "environment", "value": null },
                    { "field": "owner", "value": "Björn" },
                    { "field": "property", "key": "ip",
                      "value": { "kind": "text", "value": "10.0.0.9" } },
                    { "field": "property", "key": "os", "value": null },
                ],
            }),
        ),
        (
            "move_asset",
            serde_json::json!({ "assetId": "asset:7f2c", "newParentId": "asset:hel1" }),
        ),
        (
            "move_asset",
            serde_json::json!({ "assetId": "asset:7f2c", "newParentId": null }),
        ),
        (
            "delete_asset",
            serde_json::json!({ "assetId": "asset:7f2c" }),
        ),
        ("context_assets", serde_json::json!({ "ctxId": "ctx:7f2c" })),
        ("get_route", serde_json::json!({ "routeId": "route:9a1b" })),
        (
            "create_route",
            serde_json::json!({
                "assetId": "asset:traefik", "name": "Gitea",
                "url": "https://gitea.local/",
            }),
        ),
        (
            "create_route",
            serde_json::json!({
                "assetId": "asset:traefik", "name": "Gitea",
                "url": "https://gitea.local/", "targetId": "asset:7f2c",
                "visibility": "public",
                "properties": [["cert_expires", { "kind": "date", "value": "2026-12-01" }]],
            }),
        ),
        (
            "create_route",
            serde_json::json!({
                "assetId": "asset:traefik", "name": "Gitea",
                "url": "https://gitea.local/", "targetId": null, "visibility": null,
            }),
        ),
        (
            "edit_route",
            serde_json::json!({
                "routeId": "route:9a1b",
                "edits": [
                    { "field": "name", "value": "Gitea (tunnel)" },
                    { "field": "url", "value": "https://gitea.local/" },
                    { "field": "target", "value": null },
                    { "field": "visibility", "value": "internal" },
                    { "field": "property", "key": "cert_expires",
                      "value": { "kind": "date", "value": "2026-12-01" } },
                ],
            }),
        ),
        (
            "delete_route",
            serde_json::json!({ "routeId": "route:9a1b" }),
        ),
        ("source_assets", serde_json::json!({ "sourceId": "kuma" })),
        (
            "preview_estate_import",
            serde_json::json!({ "file": ESTATE_FILE }),
        ),
        (
            "apply_estate_import",
            serde_json::json!({ "file": ESTATE_FILE }),
        ),
    ] {
        let rejection = invoke(cmd, args.clone()).expect_err("there is no pool yet");
        assert!(
            rejection.contains("not_ready"),
            "{cmd} did not reach its own body -- it is missing from the handler \
             list, or {args} does not decode: {rejection}"
        );
        assert!(
            !rejection.contains("state not managed"),
            "{cmd} declares managed state as an argument -- carry-over §10.6(a): \
             {rejection}"
        );
    }
}

/// **The type table answers before there is a database**, and the answer is the
/// whole table.
///
/// The command every other one here is unlike: it takes no state, so the loop
/// above's `not_ready` marker cannot say anything about it, and what stands in
/// its place is the answer itself. A create dialog that could not offer a type
/// until the pool came up would be a dialog that draws empty on a cold start.
///
/// The three assertions are the three things the dialog reads: the **count**
/// (spec #427's nineteen), the **schema in its declared order** -- which is
/// what tells a reader filling in a VM's `ip` that the backend wants text --
/// and the **suggestions**, which are story 17's *usual here*.
#[test]
fn the_type_table_answers_with_no_pool_and_carries_the_schema_and_the_suggestions() {
    let answered = invoke("asset_types", serde_json::json!({}))
        .expect("`asset_types` needs no pool and answers before bring-up");
    let types = answered.as_array().expect("a list of types");
    assert_eq!(types.len(), 19, "spec #427 names nineteen types");

    let vm = types
        .iter()
        .find(|entry| entry["id"] == "vm")
        .expect("`vm` is one of them");
    assert_eq!(vm["monogram"], "VM");
    assert_eq!(
        vm["properties"]
            .as_array()
            .expect("a schema")
            .iter()
            .map(|property| (
                property["key"].as_str().expect("a key"),
                property["kind"].as_str().expect("a kind")
            ))
            .collect::<Vec<_>>(),
        [
            ("hostname", "text"),
            ("ip", "text"),
            ("os", "text"),
            ("size", "text")
        ],
        "the schema arrives in its declared order, with the kind an editor needs"
    );
    assert!(
        vm["suggests"]
            .as_array()
            .expect("a suggestion list")
            .contains(&serde_json::json!("container_engine")),
        "a VM usually holds a container engine: {}",
        vm["suggests"]
    );
    // The escape hatch is the negative control: a table that answered with the
    // same list for every type would pass everything above.
    let custom = types
        .iter()
        .find(|entry| entry["id"] == "custom")
        .expect("`custom` is one of them");
    assert_eq!(custom["properties"], serde_json::json!([]));
    assert_eq!(custom["suggests"], serde_json::json!([]));
}

/// The control for the loop above: without it, that loop would pass just as
/// happily against a harness that answered `not_ready` to anything at all.
///
/// A required argument left out is refused **by name**, before any body runs,
/// and an edit whose tag the union does not carry is refused too -- which is
/// the assertion that `field` really is the discriminator and not decoration.
#[test]
fn an_argument_the_mirror_spells_differently_never_arrives() {
    let missing = invoke("get_asset", serde_json::json!({})).expect_err("`assetId` is required");
    assert!(
        missing.contains("assetId") || missing.contains("asset_id"),
        "the refusal must name the missing argument: {missing}"
    );
    assert!(
        !missing.contains("not_ready"),
        "the body ran despite an incomplete argument list: {missing}"
    );

    let wrong_key = invoke(
        "move_asset",
        serde_json::json!({ "asset": "asset:7f2c", "newParentId": null }),
    )
    .expect_err("`asset` is not the argument's name");
    assert!(!wrong_key.contains("not_ready"), "{wrong_key}");

    let unknown_edit = invoke(
        "edit_asset",
        serde_json::json!({
            "assetId": "asset:7f2c",
            "edits": [{ "field": "monogram", "value": "VM" }],
        }),
    )
    .expect_err("`monogram` is not a field an edit can carry");
    assert!(!unknown_edit.contains("not_ready"), "{unknown_edit}");

    let unknown_kind = invoke(
        "edit_asset",
        serde_json::json!({
            "assetId": "asset:7f2c",
            "edits": [{ "field": "property", "key": "ip",
                        "value": { "kind": "secret", "value": "hunter2" } }],
        }),
    )
    .expect_err("`secret` is a property kind knobas deliberately does not carry");
    assert!(!unknown_kind.contains("not_ready"), "{unknown_kind}");
}
