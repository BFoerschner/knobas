//! The estate at the seam the six commands are shims over, and at the seam
//! Tauri dispatches through (issue #428).
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
use sqlx::PgPool;
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

    // An `up` under a `none` is still the worst of the two, because story 37
    // orders `up` above `none`. Read literally, and asserted so a later
    // reading of "worst" cannot change it silently.
    edit_one(
        &pool,
        &estate.redis.id,
        AssetEdit::Status {
            value: AssetStatus::Up,
        },
    )
    .await;
    let containers = assets::tree(&pool, Some(&estate.db.id))
        .await
        .expect("the containers");
    assert_eq!(find(&containers, "redis").health, AssetStatus::Up);

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
// The wiring: registered, named, and decoding.
// ---------------------------------------------------------------------------

/// Invoke `cmd` on a mock app that manages a `Lifecycle` with no pool.
///
/// Every asset command asks `lifecycle.pool()?` first, so a call with nothing
/// ready reaches the *body* and answers `not_ready`. That is the marker for
/// "registered and dispatched", as distinct from "no such command" -- and it
/// is only reachable because none of them declares the state as an argument
/// (carry-over §10.6(a)).
fn invoke(cmd: &str, body: serde_json::Value) -> Result<String, String> {
    let app = tauri::test::mock_builder()
        .invoke_handler(tauri::generate_handler![
            knobas_app::commands::assets::asset_tree,
            knobas_app::commands::assets::get_asset,
            knobas_app::commands::assets::create_asset,
            knobas_app::commands::assets::edit_asset,
            knobas_app::commands::assets::move_asset,
            knobas_app::commands::assets::delete_asset,
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
    outcome.map(|_| String::new())
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
