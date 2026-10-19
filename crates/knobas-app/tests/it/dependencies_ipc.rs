//! Dependencies (#871) at the seam its command is a shim over: the shared
//! layers migration `0036` stores, and the one read behind the top-bar opening
//! (`knobas_app::assets::dependencies`).
//!
//! Every test that reads the estate gets a **database of its own**: the read is
//! an answer about the whole database, so two tests sharing one would be each
//! other's fixture (`tests/it/assets_ipc.rs` gives the same reason).

use knobas_app::assets::dependencies::{self, DepItem, Dependencies};
use knobas_app::assets::{self, AssetEdit, AssetStatus, Environment, Visibility};
use knobas_app::sources::Registry;
use knobas_core::tone::Tone;
use sqlx::PgPool;

async fn pool(label: &str) -> PgPool {
    knobas_db::test_util::scratch_database(label)
        .await
        .pool(2)
        .await
        .expect("a pool on the scratch database")
}

/// A layer row, written by hand: there is no command that adds one until #873.
async fn add_layer(pool: &PgPool, id: &str, name: &str) -> Result<(), sqlx::Error> {
    sqlx::query("insert into knobas.dep_layer (id, name, position) values ($1, $2, 99)")
        .bind(id)
        .bind(name)
        .execute(pool)
        .await
        .map(|_| ())
}

/// A layer name is unique ignoring case and the spaces at either end
/// (R-11.13, ruling D14), and the database is what refuses the second one.
#[tokio::test]
async fn the_unique_index_refuses_a_duplicate_layer_name() {
    let pool = pool("deps_unique").await;
    for spelling in ["Services", "services", "  SERVICES ", " Services"] {
        let refused = add_layer(&pool, "again", spelling)
            .await
            .expect_err(&format!("{spelling:?} is the seeded Services again"));
        let constraint = refused
            .as_database_error()
            .and_then(|error| error.constraint().map(str::to_owned));
        assert_eq!(
            constraint.as_deref(),
            Some("dep_layer_name_idx"),
            "{spelling:?} must be refused by the name index, not something else: {refused}"
        );
    }
    add_layer(&pool, "service-mesh", "Service mesh")
        .await
        .expect("a different name is a new layer");
}

/// A kind sits in at most one layer: a second row for it is refused.
#[tokio::test]
async fn a_kind_sits_in_at_most_one_layer() {
    let pool = pool("deps_kind_once").await;
    let refused = sqlx::query(
        "insert into knobas.dep_layer_kind (kind, layer_id, position) values ('vm', 'services', 9)",
    )
    .execute(&pool)
    .await
    .expect_err("vm already sits in Machines");
    assert!(
        refused.to_string().contains("dep_layer_kind_pkey"),
        "{refused}"
    );
}

/// The default scheme is R-11.11's, top to bottom.
#[tokio::test]
async fn the_layers_are_seeded_with_the_default_scheme() {
    let pool = pool("deps_seed").await;
    let layers = dependencies::layers(&pool).await.expect("the layers");
    let drawn: Vec<(String, Vec<String>)> = layers
        .into_iter()
        .map(|layer| (layer.name, layer.kinds))
        .collect();
    let expected: Vec<(&str, Vec<&str>)> = vec![
        ("Work", vec!["ticket", "pr", "page"]),
        ("Code", vec!["repo"]),
        ("Delivery", vec!["pipeline"]),
        ("Entry points", vec!["route"]),
        (
            "Services",
            vec![
                "service",
                "reverse_proxy",
                "middleware",
                "runtime",
                "module",
                "custom",
            ],
        ),
        ("Containers", vec!["container_engine", "container"]),
        (
            "Data",
            vec!["database_server", "database", "schema", "table"],
        ),
        ("Machines", vec!["vm"]),
        (
            "Hardware and network",
            vec!["hypervisor", "site", "network"],
        ),
    ];
    let expected: Vec<(String, Vec<String>)> = expected
        .into_iter()
        .map(|(name, kinds)| {
            (
                name.to_owned(),
                kinds.into_iter().map(str::to_owned).collect(),
            )
        })
        .collect();
    assert_eq!(drawn, expected);
}

/// Every kind the seed places is one Dependencies knows, and every kind it
/// knows is placed: nothing starts in no layer.
#[tokio::test]
async fn the_seed_places_every_kind_once() {
    let pool = pool("deps_every_kind").await;
    let mut placed: Vec<String> = dependencies::layers(&pool)
        .await
        .expect("the layers")
        .into_iter()
        .flat_map(|layer| layer.kinds)
        .collect();
    placed.sort();
    let mut known: Vec<String> = dependencies::kinds().into_iter().map(|k| k.id).collect();
    known.sort();
    assert_eq!(placed, known);
    assert_eq!(known.len(), 22, "the 16 asset types and six other kinds");
}

async fn source(pool: &PgPool, id: &str) {
    crate::support::source_config(pool, id, id, id, &serde_json::json!({})).await;
}

/// One mirrored item: its entity row and its record.
async fn item(
    pool: &PgPool,
    id: &str,
    source: &str,
    kind: &str,
    title: &str,
    payload: serde_json::Value,
    minutes_ago: i32,
) {
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1, $2, $3)")
        .bind(id)
        .bind(kind)
        .bind(title)
        .execute(pool)
        .await
        .expect("the entity");
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload,
                                item_updated_at)
         values ($1, $2, $3, $4, $4, $5, now() - make_interval(mins => $6))",
    )
    .bind(id)
    .bind(source)
    .bind(kind)
    .bind(title)
    .bind(payload)
    .bind(minutes_ago)
    .execute(pool)
    .await
    .expect("the item");
}

async fn link(pool: &PgPool, from: &str, to: &str, relation: &str, confirmed: bool) {
    sqlx::query(
        "insert into knobas.link (from_id, to_id, relation, origin, created_by, confirmed_at,
                                  rule, rule_class, reason)
         values ($1, $2, $3, case when $4 then 'manual' else 'suggested' end, 'user',
                 case when $4 then now() end,
                 case when not $4 then 'key_mention' end, case when not $4 then 'exact_key' end,
                 case when not $4 then 'the key is in the title' end)",
    )
    .bind(from)
    .bind(to)
    .bind(relation)
    .bind(confirmed)
    .execute(pool)
    .await
    .expect("the link");
}

fn find<'a>(read: &'a Dependencies, id: &str) -> &'a DepItem {
    read.items
        .iter()
        .find(|item| item.id == id)
        .unwrap_or_else(|| panic!("{id} is not in the read"))
}

fn has(read: &Dependencies, from: &str, to: &str, relation: &str) -> bool {
    read.relations
        .iter()
        .any(|r| r.from == from && r.to == to && r.relation == relation)
}

/// The top-bar opening: every asset, route, repository, pipeline, open pull
/// request and linked ticket, with its health and the relations the stored
/// data holds -- and nothing else.
#[tokio::test]
async fn the_read_returns_the_whole_estate_with_health_and_relations() {
    let pool = pool("deps_read").await;
    for id in ["gitea", "teamcity", "jira", "kuma"] {
        source(&pool, id).await;
    }

    // The estate: a VM in prod holding a container that is down, and a route
    // the VM exposes that lands on the container. The VM is watched by a
    // monitor whose newest sample is a warning.
    let vm = assets::create(&pool, None, "vm", "vm-pay-01", &[])
        .await
        .unwrap()
        .value;
    assets::edit(
        &pool,
        &vm.id,
        &[
            AssetEdit::Environment {
                value: Some(Environment::Prod),
            },
            AssetEdit::Owner {
                value: Some("Payments".to_owned()),
            },
        ],
    )
    .await
    .unwrap();
    let container = assets::create(&pool, Some(&vm.id), "container", "payout-api", &[])
        .await
        .unwrap()
        .value;
    assets::edit(
        &pool,
        &container.id,
        &[AssetEdit::Status {
            value: AssetStatus::Down,
        }],
    )
    .await
    .unwrap();
    let route = assets::create_route(
        &pool,
        &vm.id,
        "payouts",
        "https://payouts.example",
        Some(&container.id),
        Visibility::Public,
        &[],
    )
    .await
    .unwrap()
    .value;
    item(
        &pool,
        "kuma:vm",
        "kuma",
        "monitor",
        "vm-pay-01 ping",
        serde_json::json!({}),
        0,
    )
    .await;
    link(&pool, &vm.id, "kuma:vm", "monitored-by", true).await;
    for (state, ago) in [("up", 3), ("warn", 1)] {
        sqlx::query(
            "insert into knobas.monitor_sample (entity_id, taken_at, state)
             values ('kuma:vm', now() - make_interval(mins => $1), $2)",
        )
        .bind(ago)
        .bind(state)
        .execute(&pool)
        .await
        .unwrap();
    }

    // The code: a repository with one open and one merged pull request.
    let repo = serde_json::json!({ "full_name": "payments/payouts", "name": "payouts" });
    item(
        &pool,
        "gitea:payments/payouts",
        "gitea",
        "repo",
        "payments/payouts",
        repo.clone(),
        0,
    )
    .await;
    item(
        &pool,
        "gitea:payments/payouts#142",
        "gitea",
        "pr",
        "SEPA retry",
        serde_json::json!({ "state": "open", "merged": false, "base": { "repo": repo } }),
        0,
    )
    .await;
    item(
        &pool,
        "gitea:payments/payouts#140",
        "gitea",
        "pr",
        "Old work",
        serde_json::json!({ "state": "closed", "merged": true, "base": { "repo": repo } }),
        0,
    )
    .await;

    // Delivery: a pipeline whose newest run failed after an older one passed,
    // and a pipeline that has never run.
    let config = |id: &str| serde_json::json!({ "id": id, "name": id, "projectId": "P" });
    item(
        &pool,
        "teamcity:Payout_Build",
        "teamcity",
        "build_config",
        "Payout_Build",
        config("Payout_Build"),
        0,
    )
    .await;
    item(
        &pool,
        "teamcity:Payout_Lint",
        "teamcity",
        "build_config",
        "Payout_Lint",
        config("Payout_Lint"),
        0,
    )
    .await;
    for (id, status, ago) in [("teamcity:1", "SUCCESS", 30), ("teamcity:2", "FAILURE", 5)] {
        item(
            &pool,
            id,
            "teamcity",
            "build",
            id,
            serde_json::json!({ "state": "finished", "status": status,
                                "buildTypeId": "Payout_Build",
                                "buildType": config("Payout_Build") }),
            ago,
        )
        .await;
    }

    // The work: one ticket linked to the container, one linked only by a
    // proposal, and one linked to nothing the read returns.
    let ticket = |key: &str| {
        serde_json::json!({ "key": key, "fields": { "summary": key, "status": { "name": "In Progress" },
                                                     "project": { "key": "PAY" } } })
    };
    for key in ["PAY-231", "PAY-232", "PAY-233"] {
        item(
            &pool,
            &format!("jira:{key}"),
            "jira",
            "ticket",
            key,
            ticket(key),
            0,
        )
        .await;
    }
    link(&pool, "jira:PAY-231", &container.id, "deployed-to", true).await;
    link(
        &pool,
        "jira:PAY-231",
        "gitea:payments/payouts#142",
        "implements",
        true,
    )
    .await;
    link(&pool, "jira:PAY-232", &vm.id, "mentions", false).await;
    link(&pool, "jira:PAY-233", "teamcity:1", "built-by", true).await;

    let read = dependencies::read(&pool, &Registry::for_process())
        .await
        .expect("the read");

    let mut ids: Vec<&str> = read.items.iter().map(|item| item.id.as_str()).collect();
    ids.sort_unstable();
    let mut expected = vec![
        vm.id.as_str(),
        container.id.as_str(),
        route.id.as_str(),
        "gitea:payments/payouts",
        "gitea:payments/payouts#142",
        "teamcity:Payout_Build",
        "teamcity:Payout_Lint",
        "jira:PAY-231",
    ];
    expected.sort_unstable();
    assert_eq!(
        ids, expected,
        "no merged PR, no run, no monitor, no ticket linked only by a proposal or to a run"
    );

    let vm_tile = find(&read, &vm.id);
    assert_eq!(vm_tile.kind, "vm");
    assert_eq!(
        vm_tile.health,
        Some(Tone::Warn),
        "its monitor's newest sample, not the down container under it"
    );
    assert_eq!(vm_tile.monitors, 1);
    assert_eq!(vm_tile.environment.as_deref(), Some("prod"));

    let container_tile = find(&read, &container.id);
    assert_eq!(container_tile.health, Some(Tone::Err));
    assert_eq!(
        container_tile.monitors, 0,
        "nothing watches it, so it draws hatched"
    );
    assert_eq!(
        container_tile.environment.as_deref(),
        Some("prod"),
        "inherited from the VM"
    );
    assert_eq!(
        container_tile.owner.as_deref(),
        Some("Payments"),
        "inherited from the VM"
    );

    let route_tile = find(&read, &route.id);
    assert_eq!(route_tile.kind, "route");
    assert_eq!(route_tile.health, None);
    assert_eq!(route_tile.environment.as_deref(), Some("prod"));

    let pr = find(&read, "gitea:payments/payouts#142");
    assert_eq!(pr.kind, "pr");
    assert_eq!(pr.health, Some(Tone::Info));
    assert_eq!(pr.state.as_deref(), Some("open"));

    let build = find(&read, "teamcity:Payout_Build");
    assert_eq!(build.kind, "pipeline");
    assert_eq!(build.health, Some(Tone::Err), "the newest run failed");
    let lint = find(&read, "teamcity:Payout_Lint");
    assert_eq!(
        (lint.health, lint.state.as_deref()),
        (None, None),
        "never ran"
    );

    let pay = find(&read, "jira:PAY-231");
    assert_eq!(pay.kind, "ticket");
    assert_eq!(pay.name, "PAY-231");
    assert_eq!(pay.state.as_deref(), Some("In Progress"));
    assert_eq!(find(&read, "gitea:payments/payouts").kind, "repo");

    assert!(has(&read, &container.id, &vm.id, "inside"));
    assert!(has(&read, &vm.id, &route.id, "exposes"));
    assert!(has(&read, &route.id, &container.id, "lands-on"));
    assert!(has(
        &read,
        "gitea:payments/payouts#142",
        "gitea:payments/payouts",
        "in-repository"
    ));
    assert!(has(&read, "jira:PAY-231", &container.id, "deployed-to"));
    assert!(has(
        &read,
        "jira:PAY-231",
        "gitea:payments/payouts#142",
        "implements"
    ));
    assert!(
        read.relations
            .iter()
            .all(|r| ids.contains(&r.from.as_str()) && ids.contains(&r.to.as_str())),
        "every relation joins two items of the read: {:?}",
        read.relations
    );
    assert_eq!(read.layers.len(), 9);
    assert_eq!(read.kinds.len(), 22);
}

/// The answer, serialized, is exactly what `app/src/lib/ipc/assets.ts`
/// declares for it.
#[test]
fn the_dependencies_shape_matches_its_typescript_mirror() {
    const MIRROR: &str = include_str!("../../../../app/src/lib/ipc/assets.ts");
    let read = Dependencies {
        layers: vec![dependencies::Layer {
            id: "work".to_owned(),
            name: "Work".to_owned(),
            kinds: vec!["ticket".to_owned()],
        }],
        kinds: dependencies::kinds(),
        items: vec![DepItem {
            id: "asset:x".to_owned(),
            kind: "vm".to_owned(),
            name: "vm-pay-01".to_owned(),
            health: None,
            state: None,
            monitors: 0,
            environment: None,
            owner: None,
        }],
        relations: vec![dependencies::DepRelation {
            from: "a".to_owned(),
            to: "b".to_owned(),
            relation: "inside".to_owned(),
        }],
    };
    let value = serde_json::to_value(&read).unwrap();
    knobas_sync::mirror::assert_shape(
        MIRROR,
        "Dependencies",
        &value,
        &["layers", "kinds", "items", "relations"],
    );
    knobas_sync::mirror::assert_shape(
        MIRROR,
        "DepLayer",
        &value["layers"][0],
        &["id", "name", "kinds"],
    );
    knobas_sync::mirror::assert_shape(
        MIRROR,
        "DepKind",
        &value["kinds"][0],
        &["id", "label", "monogram"],
    );
    knobas_sync::mirror::assert_shape(
        MIRROR,
        "DepItem",
        &value["items"][0],
        &[
            "id",
            "kind",
            "name",
            "health",
            "state",
            "monitors",
            "environment",
            "owner",
        ],
    );
    knobas_sync::mirror::assert_shape(
        MIRROR,
        "DepRelation",
        &value["relations"][0],
        &["from", "to", "relation"],
    );
}

// -- #873: the layer writes and the saved views ------------------------------

fn names(layers: &[dependencies::Layer]) -> Vec<String> {
    layers.iter().map(|layer| layer.name.clone()).collect()
}

/// A layer's name stays unique on every write that sets one: adding and
/// renaming both refuse a name already taken, by case and outer spaces, with
/// R-11.13's sentence; a blank name is refused as invalid.
#[tokio::test]
async fn adding_and_renaming_a_layer_keep_the_names_unique() {
    let pool = pool("deps_layer_names").await;
    let added = dependencies::create_layer(&pool, "  Service mesh ")
        .await
        .expect("a new name is a new layer");
    let mesh = added.last().expect("the new layer is last");
    assert_eq!(mesh.name, "Service mesh", "stored trimmed");
    assert!(mesh.kinds.is_empty());

    let refused = dependencies::create_layer(&pool, " services ")
        .await
        .expect_err("Services is taken");
    assert_eq!(refused.code, knobas_app::IpcErrorCode::Conflict);
    assert_eq!(
        refused.message,
        "There is already a layer called services. Names are unique, so a name always means the same things."
    );

    let refused = dependencies::rename_layer(&pool, &mesh.id, "WORK")
        .await
        .expect_err("Work is taken");
    assert_eq!(refused.code, knobas_app::IpcErrorCode::Conflict);

    let refused = dependencies::rename_layer(&pool, &mesh.id, "   ")
        .await
        .expect_err("blank");
    assert_eq!(refused.code, knobas_app::IpcErrorCode::Invalid);
    assert_eq!(refused.message, "A layer needs a name.");

    // Renaming a layer to its own name in another case is not a clash with itself.
    let renamed = dependencies::rename_layer(&pool, "work", "WORK")
        .await
        .expect("its own name");
    assert_eq!(renamed[0].name, "WORK");

    let missing = dependencies::rename_layer(&pool, "nope", "Nope")
        .await
        .expect_err("no such layer");
    assert_eq!(missing.code, knobas_app::IpcErrorCode::NotFound);
}

/// Reordering takes every layer once; removing one sends its kinds to no layer;
/// placing a kind moves it, or takes it out of every layer.
#[tokio::test]
async fn layers_reorder_remove_and_place_kinds() {
    let pool = pool("deps_layer_writes").await;
    let seeded = dependencies::layers(&pool).await.expect("layers");
    let mut ids: Vec<String> = seeded.iter().map(|layer| layer.id.clone()).collect();
    ids.swap(0, 1);
    let ordered = dependencies::order_layers(&pool, &ids)
        .await
        .expect("a permutation");
    assert_eq!(names(&ordered)[..2], ["Code".to_owned(), "Work".to_owned()]);

    let refused = dependencies::order_layers(&pool, &ids[1..])
        .await
        .expect_err("one layer missing");
    assert_eq!(refused.code, knobas_app::IpcErrorCode::Invalid);

    let placed = dependencies::place_kind(&pool, "vm", Some("services"))
        .await
        .expect("vm into Services");
    let services = placed.iter().find(|l| l.id == "services").unwrap();
    assert_eq!(services.kinds.last().map(String::as_str), Some("vm"));
    assert!(
        placed
            .iter()
            .find(|l| l.id == "machines")
            .unwrap()
            .kinds
            .is_empty()
    );

    let unplaced = dependencies::place_kind(&pool, "network", None)
        .await
        .expect("network into no layer");
    assert!(
        unplaced
            .iter()
            .all(|l| !l.kinds.iter().any(|k| k == "network"))
    );

    let refused = dependencies::place_kind(&pool, "spaceship", Some("services"))
        .await
        .expect_err("not a kind");
    assert_eq!(refused.code, knobas_app::IpcErrorCode::Invalid);
    let refused = dependencies::place_kind(&pool, "vm", Some("nope"))
        .await
        .expect_err("not a layer");
    assert_eq!(refused.code, knobas_app::IpcErrorCode::NotFound);

    let left = dependencies::delete_layer(&pool, "data")
        .await
        .expect("removed");
    assert!(left.iter().all(|l| l.id != "data"));
    let read = dependencies::read(&pool, &Registry::for_process())
        .await
        .expect("the read");
    for kind in ["database_server", "database", "schema", "table"] {
        assert!(
            read.layers
                .iter()
                .all(|l| !l.kinds.iter().any(|k| k == kind)),
            "{kind} sits in no layer once Data is gone"
        );
    }
    let missing = dependencies::delete_layer(&pool, "data")
        .await
        .expect_err("gone");
    assert_eq!(missing.code, knobas_app::IpcErrorCode::NotFound);
}

/// The four built-in views are seeded, *This ticket* opens from a ticket and
/// *Everything* from the top bar; *Everything* hides Work.
#[tokio::test]
async fn the_built_in_views_are_seeded_with_their_defaults() {
    use dependencies::{Group, Tiles};
    let pool = pool("deps_views_seed").await;
    let read = dependencies::views(&pool).await.expect("views");
    let named: Vec<&str> = read.views.iter().map(|v| v.name.as_str()).collect();
    assert_eq!(
        named,
        [
            "Everything",
            "This ticket",
            "Runtime by environment",
            "Code and delivery"
        ]
    );
    assert_eq!(read.opens_from_ticket, "ticket");
    assert_eq!(read.opens_from_top, "everything");
    let everything = &read.views[0].settings;
    assert!(everything.layers["work"].hidden);
    assert_eq!(everything.layers["services"].group, Group::Environment);
    assert_eq!(everything.layers["containers"].group, Group::Parent);
    assert_eq!(everything.tiles, Tiles::Auto);
    assert_eq!(everything.fold_at, 24);
    assert!(read.views[1].settings.layers.is_empty());
}

/// Save as a new view, save changes, rename, choose which opens first, and
/// delete: a deleted default falls back to the first view, and the last view
/// is never deleted.
#[tokio::test]
async fn views_are_saved_renamed_chosen_and_deleted() {
    use dependencies::{DepViewSettings, Group, LayerInView, Tiles};
    let pool = pool("deps_views_writes").await;
    let mut settings = DepViewSettings::default();
    settings.layers.insert(
        "services".to_owned(),
        LayerInView {
            hidden: false,
            folded: true,
            group: Group::Owner,
        },
    );
    settings.environments = vec!["prod".to_owned()];
    settings.problems = true;
    settings.tiles = Tiles::Compact;
    settings.fold_at = 50;

    let mine = dependencies::save_view(&pool, None, "  My view ", &settings)
        .await
        .expect("saved as new");
    assert_eq!(mine.name, "My view");
    assert_eq!(mine.settings, settings);

    settings.problems = false;
    let saved = dependencies::save_view(&pool, Some(&mine.id), "Mine", &settings)
        .await
        .expect("saved changes and renamed");
    assert_eq!(saved.id, mine.id);
    assert_eq!(saved.name, "Mine");

    let blank = dependencies::save_view(&pool, Some(&mine.id), " ", &settings)
        .await
        .expect_err("blank");
    assert_eq!(blank.code, knobas_app::IpcErrorCode::Invalid);
    let missing = dependencies::save_view(&pool, Some("nope"), "x", &settings)
        .await
        .expect_err("no such view");
    assert_eq!(missing.code, knobas_app::IpcErrorCode::NotFound);

    let read = dependencies::set_view_defaults(&pool, &mine.id, "runtime")
        .await
        .expect("defaults");
    assert_eq!(read.opens_from_ticket, mine.id);
    assert_eq!(read.opens_from_top, "runtime");
    assert_eq!(read.views.last().unwrap().settings, settings);
    let refused = dependencies::set_view_defaults(&pool, "nope", "runtime")
        .await
        .expect_err("no such view");
    assert_eq!(refused.code, knobas_app::IpcErrorCode::NotFound);

    let read = dependencies::delete_view(&pool, &mine.id)
        .await
        .expect("deleted");
    assert_eq!(
        read.opens_from_ticket, "everything",
        "falls back to the first view"
    );
    assert_eq!(read.views.len(), 4);
    for id in ["everything", "ticket", "runtime"] {
        dependencies::delete_view(&pool, id).await.expect("deleted");
    }
    let last = dependencies::delete_view(&pool, "code")
        .await
        .expect_err("the last view");
    assert_eq!(last.code, knobas_app::IpcErrorCode::Conflict);
}

/// A settings object that does not read as one is refused before it is stored.
#[test]
fn view_settings_refuse_an_unknown_group() {
    let parsed: Result<dependencies::DepViewSettings, _> =
        serde_json::from_value(serde_json::json!({
            "layers": {"work": {"hidden": false, "folded": false, "group": "colour"}},
            "environments": [], "problems": false, "tiles": "auto", "fold_at": 24
        }));
    assert!(parsed.is_err());
}

/// The views' answer, serialized, is what `app/src/lib/ipc/assets.ts` declares.
#[test]
fn the_views_shape_matches_its_typescript_mirror() {
    const MIRROR: &str = include_str!("../../../../app/src/lib/ipc/assets.ts");
    let mut settings = dependencies::DepViewSettings::default();
    settings
        .layers
        .insert("work".to_owned(), dependencies::LayerInView::default());
    let read = dependencies::DepViews {
        views: vec![dependencies::DepView {
            id: "everything".to_owned(),
            name: "Everything".to_owned(),
            settings,
        }],
        opens_from_ticket: "everything".to_owned(),
        opens_from_top: "everything".to_owned(),
    };
    let value = serde_json::to_value(&read).unwrap();
    knobas_sync::mirror::assert_shape(
        MIRROR,
        "DepViews",
        &value,
        &["views", "opens_from_ticket", "opens_from_top"],
    );
    knobas_sync::mirror::assert_shape(
        MIRROR,
        "DepView",
        &value["views"][0],
        &["id", "name", "settings"],
    );
    knobas_sync::mirror::assert_shape(
        MIRROR,
        "DepViewSettings",
        &value["views"][0]["settings"],
        &["layers", "environments", "problems", "tiles", "fold_at"],
    );
    knobas_sync::mirror::assert_shape(
        MIRROR,
        "LayerInView",
        &value["views"][0]["settings"]["layers"]["work"],
        &["hidden", "folded", "group"],
    );
}
