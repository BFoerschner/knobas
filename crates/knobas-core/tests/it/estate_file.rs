//! The schema check that keeps `testenv/estate.json` honest.
//!
//! # What the file is
//!
//! `testenv/estate.json` describes the **estate** (`CONTEXT.md`) as it runs:
//! the notebook, the Windows desktop, its WSL 2 VM and the Docker engine in
//! it, the containers and databases on that engine -- TeamCity, Jira and
//! Confluence among them, up around the clock since 2026-11-01 -- and the
//! ports the notebook reaches them on as routes. Until #634 it also held
//! three Hetzner servers, which were decommissioned; until 2026-11-01 the
//! engine was OrbStack's, on the notebook. It is data about a real
//! estate and the only asset fixture there will be -- the M4 spec (#427) rules
//! out anything Tidewater-shaped for assets, because a made-up estate proves
//! that the tree draws and not that a real estate fits it. (Knobas-app's
//! `tests/it/support/hcloud-estate.json` is not a second one: it is those three
//! servers as they stood when the hcloud recording was taken, the recording's
//! counterpart, and describes no estate.)
//!
//! It has three readers: the Import, `--demo`, and whoever is trying to
//! remember what runs where. All three are downstream of it being *true*, and
//! nothing else in the repository is in a position to notice when it stops
//! being true. Hence this file.
//!
//! # Why a test and not a JSON Schema
//!
//! Three of the four invariants below are referential -- a parent, a route's
//! asset, a route's target all have to name something else in the same file --
//! and a schema language that can express those is a language nobody here
//! reads. The fourth (the type table) is a list that lives in code. So the
//! check is a test, it runs in `just check` with no container and no database,
//! and it says what it wants in a sentence when it fails.
//!
//! # Why the two coverage tests read the real files
//!
//! [`every_compose_service_the_estate_runs_is_recorded`] and
//! [`the_notebook_states_the_address_its_ping_watches`] are the ones that catch
//! the drift that actually happens: a new service in the compose file, or a
//! seeded monitor moved, by somebody who has never heard of this JSON. They read
//! `docker-compose.yml`'s service list and `monitors.json` rather than a copy
//! of them, so the day either changes the estate file is red until it follows.
//! The parses are deliberately narrow and assert that they found something, so
//! a rewrite that moves the services elsewhere fails loudly instead of silently
//! checking nothing. (Until #634 a third read the Hetzner README's host-list
//! table; the servers and the table are gone.)
//!
//! A closed key vocabulary ([`ASSET_KEYS`], [`ROUTE_KEYS`]) covers the one
//! mistake none of the referential checks can see: a misspelled key reads as an
//! absent optional field, and an absent optional field is legal everywhere.
//!
//! # The type table
//!
//! Read off [`knobas_core::asset::TYPES`], which is where the built-in table
//! landed with the asset model (#428). This file carried a hand-written copy
//! of the list until then and said in its place that the copy was to be
//! deleted the moment there was a table to read; it was. So a type the estate
//! file names and the table does not is now a red gate rather than agreement
//! between two lists nobody diffs.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::net::Ipv4Addr;
use std::path::{Path, PathBuf};

use serde_json::Value;

/// The four environments an asset may be in (spec §12.1).
const ENVIRONMENTS: &[&str] = &["dev", "stage", "prod", "shared"];

/// A compose service that is deliberately not an asset, and why.
///
/// `kuma-seed` is a one-shot job -- `profiles: ["seed"]` keeps it out of `up`
/// and `seed-kuma.sh` runs it with `compose run --rm`. It is a thing that is
/// *done to* the estate, not a thing the estate holds, so it has no asset and
/// no monitor.
///
/// `forgejo` (#1195) is the Gitea live suite's second target, behind its own
/// profile: `just gitea-live forgejo` starts it and stops it again at the end
/// of the run. Between runs it is not there, so the estate does not hold it;
/// Gitea is the asset the notebook routes to and watches.
///
/// Every other service in that file runs somewhere and is recorded --
/// `docker-proxy` (#978), Kuma's read-only Docker API, among them since
/// 2026-11-01: it publishes no port and nothing routes to it, but it runs on
/// the WSL box's engine around the clock, and `just estate-live` holds every
/// running container there to this file.
const NOT_ASSETS: &[&str] = &["kuma-seed", "forgejo"];

/// The repository root, from this crate's manifest directory.
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("the workspace root is two levels above this crate")
}

fn read(relative: &str) -> String {
    let path = root().join(relative);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

fn estate() -> Value {
    serde_json::from_str(&read("testenv/estate.json")).expect("testenv/estate.json is JSON")
}

/// One of the file's two arrays, which must be there and must not be empty --
/// so that every test below is a claim about something.
fn array<'a>(estate: &'a Value, key: &str) -> &'a Vec<Value> {
    let entries = estate[key]
        .as_array()
        .unwrap_or_else(|| panic!("the estate file has an `{key}` array"));
    assert!(!entries.is_empty(), "the estate file describes no {key}");
    entries
}

fn assets(estate: &Value) -> &Vec<Value> {
    array(estate, "assets")
}

fn routes(estate: &Value) -> &Vec<Value> {
    array(estate, "routes")
}

/// A required string field, named in the panic when it is missing so that a
/// typo in the file reads as a typo and not as a `None` unwrapped somewhere.
fn field<'a>(entry: &'a Value, key: &str) -> &'a str {
    entry[key].as_str().unwrap_or_else(|| {
        panic!(
            "every entry needs a string `{key}`; this one has none: {}",
            serde_json::to_string(entry).expect("an entry re-serialises")
        )
    })
}

fn id(entry: &Value) -> &str {
    field(entry, "id")
}

#[test]
fn every_id_is_unique() {
    let estate = estate();
    let mut seen: HashSet<&str> = HashSet::new();
    for entry in assets(&estate).iter().chain(routes(&estate)) {
        let entry_id = id(entry);
        assert!(
            seen.insert(entry_id),
            "`{entry_id}` is the id of two entries in the estate file; an id is \
             an address and two things cannot share one"
        );
    }
}

#[test]
fn every_id_is_a_slug_in_its_own_namespace() {
    let estate = estate();
    let slug = |id: &str, prefix: &str| {
        let rest = id
            .strip_prefix(prefix)
            .unwrap_or_else(|| panic!("`{id}` should be addressed as `{prefix}<slug>` (spec §2)"));
        assert!(
            !rest.is_empty()
                && rest
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
            "`{id}`'s slug is not lower-case, digits and hyphens; imported ids \
             keep the file's spelling, so a wobbly one is permanent"
        );
    };
    for asset in assets(&estate) {
        slug(id(asset), "asset:");
    }
    for route in routes(&estate) {
        slug(id(route), "route:");
    }
}

#[test]
fn every_parent_exists_and_the_assets_form_one_tree() {
    let estate = estate();
    let assets = assets(&estate);
    let ids: HashSet<&str> = assets.iter().map(id).collect();

    let mut parents: HashMap<&str, &str> = HashMap::new();
    let mut roots: Vec<&str> = Vec::new();
    for asset in assets {
        match asset.get("parent").and_then(Value::as_str) {
            Some(parent) => {
                assert!(
                    ids.contains(parent),
                    "`{}` names the parent `{parent}`, which is not in the file; \
                     containment is a parent field (ADR-0014) and a dangling one \
                     leaves the asset held by nobody",
                    id(asset)
                );
                parents.insert(id(asset), parent);
            }
            None => roots.push(id(asset)),
        }
    }

    assert_eq!(
        roots,
        vec!["asset:knobas-estate"],
        "the estate is one tree with one root, and the root is where the \
         environment and the owner are set"
    );

    // A parent that exists is not yet a tree: a cycle would satisfy every
    // assertion above and hang the Miller columns, and the import refuses one
    // at write time.
    for asset in assets {
        let start = id(asset);
        let mut walked: HashSet<&str> = HashSet::from([start]);
        let mut at = start;
        while let Some(&parent) = parents.get(at) {
            assert!(
                walked.insert(parent),
                "walking up from `{start}` comes back to `{parent}`: the assets \
                 hold each other in a cycle"
            );
            at = parent;
        }
    }
}

#[test]
fn every_route_is_exposed_by_an_asset_and_lands_on_one() {
    let estate = estate();
    let ids: HashSet<&str> = assets(&estate).iter().map(id).collect();
    for route in routes(&estate) {
        let route_id = id(route);
        let exposed_by = field(route, "asset");
        assert!(
            ids.contains(exposed_by),
            "`{route_id}` is exposed by `{exposed_by}`, which is not an asset in this file"
        );
        // A route may have no target -- an endpoint that lands on nothing
        // knobas knows is still a route -- but a named one has to resolve.
        if let Some(target) = route.get("target").and_then(Value::as_str) {
            assert!(
                ids.contains(target),
                "`{route_id}` targets `{target}`, which is not an asset in this \
                 file; `reachable via` is computed from the target end and reads \
                 nothing at all when it dangles"
            );
        }
        assert!(
            field(route, "url").contains("://"),
            "`{route_id}` needs a url with a scheme"
        );
    }
}

#[test]
fn every_type_is_in_the_type_table() {
    let estate = estate();
    let table: HashSet<&str> = knobas_core::asset::TYPES
        .iter()
        .map(|declared| declared.id)
        .collect();
    for asset in assets(&estate) {
        let asset_type = field(asset, "type");
        assert!(
            table.contains(asset_type),
            "`{}` is of type `{asset_type}`, which is not one of the built-in \
             types {table:?}; user-defined types are deferred and `custom` is \
             what a thing the list has no name for gets",
            id(asset)
        );
    }
}

/// Every parent-and-child pair the real estate uses is a pair the type table
/// **suggests**.
///
/// The conventions `AssetType::suggests` carries are what the create dialog
/// offers as *usual here* (story 17, #429), and the only estate there is to
/// answer to is this one (ADR-0013: the real container is the witness). So the
/// two are checked against each other here rather than against a diagram: a
/// shape that occurs in provisioned infrastructure and is not offered is a
/// convention that has drifted from the thing it describes.
///
/// It runs on **types**, not on assets, so the six distinct pairs this file
/// holds are six assertions and not twenty-odd. (Seven until #634: the
/// Hetzner site under the root and the servers directly under that site went
/// with the servers. The table still suggests both pairs, and the floor below
/// fell with the estate, not with the table. The sixth, a `custom` under the
/// `site`, is the notebook since it stopped holding a VM on 2026-11-01.) It is one-directional on
/// purpose: a type may suggest a pair the estate has no instance of -- the
/// design draws chains this infrastructure does not reach into -- and only the
/// other direction is a drift.
#[test]
fn every_pair_the_real_estate_holds_is_a_pair_the_type_table_suggests() {
    let estate = estate();
    let assets = assets(&estate);
    let type_of: HashMap<&str, &str> = assets
        .iter()
        .map(|asset| (id(asset), field(asset, "type")))
        .collect();

    let mut pairs: BTreeSet<(&str, &str)> = BTreeSet::new();
    for asset in assets {
        let Some(parent) = asset.get("parent").and_then(Value::as_str) else {
            continue;
        };
        let holder = type_of[parent];
        pairs.insert((holder, field(asset, "type")));
    }
    assert!(
        pairs.len() >= 5,
        "the estate has stopped describing a tree of more than one shape: {pairs:?}"
    );

    for (holder, held) in pairs {
        let declared = knobas_core::asset::find(holder).expect("a type in the table");
        assert!(
            declared.suggests.contains(&held),
            "the estate holds a {held:?} inside a {holder:?} and the type table \
             does not suggest it, so creating that pair in the Tree means \
             hunting for the type: {:?}",
            declared.suggests
        );
    }
}

#[test]
fn the_environment_and_the_owner_are_set_at_the_root_and_nowhere_else() {
    let estate = estate();
    for asset in assets(&estate) {
        let asset_id = id(asset);
        // `own`, not `inherited`: this reads the asset's own field and walks no
        // parent. Which ancestor an asset inherits from is the model's job
        // (ADR-0014) and is not what this file records.
        let own = |key: &str| asset.get(key).and_then(Value::as_str);
        // First, so that a typo is what this assertion sees. Behind the
        // placement rule below it could never fail: the root's environment is
        // asserted equal to `dev` and every other asset's to nothing.
        if let Some(environment) = own("environment") {
            assert!(
                ENVIRONMENTS.contains(&environment),
                "`{asset_id}`'s environment `{environment}` is not one of {ENVIRONMENTS:?}"
            );
        }
        if asset_id == "asset:knobas-estate" {
            assert_eq!(
                own("environment"),
                Some("dev"),
                "the root sets the environment every asset inherits, and this \
                 estate is the development one"
            );
            assert!(
                own("owner").is_some_and(|owner| !owner.is_empty()),
                "the root sets the owner every asset inherits"
            );
        } else {
            assert_eq!(
                (own("environment"), own("owner")),
                (None, None),
                "`{asset_id}` sets an environment or an owner of its own. That \
                 is allowed by the model and wrong here: this estate is one \
                 environment with one owner, and setting either again is how \
                 the inheritance stops being witnessed by the file."
            );
        }
    }
}

/// Whether `descendant` is `ancestor`, or is held by it however deep.
///
/// The walk is bounded by the number of assets so that a cycle fails here as a
/// wrong answer rather than hanging the gate;
/// [`every_parent_exists_and_the_assets_form_one_tree`] is what reports it.
fn is_held_by(assets: &[Value], ancestor: &str, descendant: &str) -> bool {
    let mut at = descendant;
    for _ in 0..=assets.len() {
        if at == ancestor {
            return true;
        }
        let Some(entry) = assets.iter().find(|asset| id(asset) == at) else {
            return false;
        };
        match entry.get("parent").and_then(Value::as_str) {
            Some(parent) => at = parent,
            None => return false,
        }
    }
    false
}

/// **The notebook states the address its ping watches** (#634).
///
/// Until #634 the three Hetzner servers carried their public `ip`, and the
/// three `ping` monitors watched those addresses: that is what put a ping
/// check inside `monitor_url_host`'s reach on the real estate (#479). With the
/// servers gone, the one ping left is `notebook`, on the notebook's Tailscale
/// address, which the Kuma on the WSL box pings (until 2026-11-01, the address
/// OrbStack's containers reached the notebook at) -- and the notebook states
/// it as its `ip`.
///
/// `ip` and not `ipv4`: the type schema's own word for this field
/// (`knobas_core::asset::TYPES`, a hypervisor's and a vm's; the notebook is
/// `custom` and carries it as a custom property), and what
/// `monitor_url_host` reads. A property the schema does not declare is legal,
/// so nothing but this line would notice the file spelling it its own way.
/// Read against `monitors.json` rather than a copy, so the two cannot drift
/// apart without this going red.
#[test]
fn the_notebook_states_the_address_its_ping_watches() {
    let estate = estate();
    let notebook = assets(&estate)
        .iter()
        .find(|asset| id(asset) == "asset:notebook")
        .expect("the estate holds the notebook");
    let address = notebook["properties"]["ip"]
        .as_str()
        .expect("the notebook needs its `ip` property");
    address
        .parse::<Ipv4Addr>()
        .unwrap_or_else(|e| panic!("the notebook's ip `{address}` is not an address: {e}"));

    let seeded: Value = serde_json::from_str(&read("testenv/monitors.json"))
        .expect("testenv/monitors.json is JSON");
    let pings: Vec<&Value> = seeded
        .as_array()
        .expect("testenv/monitors.json is an array of monitors")
        .iter()
        .filter(|monitor| monitor["type"] == "ping")
        .collect();
    let [ping] = pings.as_slice() else {
        panic!("the seed creates exactly one ping, the notebook's: {pings:?}");
    };
    assert_eq!(
        field(ping, "name"),
        "notebook",
        "the one ping the seed creates"
    );
    assert_eq!(
        ping["hostname"].as_str(),
        Some(address),
        "the `notebook` ping watches an address the notebook does not state"
    );
    assert!(
        notebook["monitors"]
            .as_array()
            .is_some_and(|names| names.iter().any(|name| name == "notebook")),
        "and the notebook names it, so the import draws the link"
    );
}

/// **Every compose service runs on the WSL box's engine** (#634, #2366).
///
/// The fact the set comparison in
/// [`every_compose_service_the_estate_runs_is_recorded`] cannot carry: where
/// it runs. Until #634 the README's host list put TeamCity, Jira and Confluence
/// on three Hetzner servers and a test read that table; until 2026-11-01 every
/// service ran in OrbStack's Docker on the notebook; now every one runs on the
/// WSL box's engine, so every container recording a compose service hangs off
/// `asset:wsl-docker`.
#[test]
fn every_compose_service_runs_on_the_wsl_boxs_engine() {
    let estate = estate();
    let assets = assets(&estate);
    let mut checked = 0;
    for asset in assets {
        if asset["properties"]["compose_service"].as_str().is_none() {
            continue;
        }
        assert!(
            is_held_by(assets, "asset:wsl-docker", id(asset)),
            "`{}` records a compose service and is not held by the WSL box's \
             Docker engine, which is the only engine this estate has",
            id(asset)
        );
        checked += 1;
    }
    assert!(
        checked >= 8,
        "only {checked} assets record a compose service; the parse has broken \
         rather than the estate having shrunk"
    );
}

/// The services `testenv/docker-compose.yml` declares, read out of the file.
///
/// The parse is the narrow one the shape allows: the keys indented by exactly
/// two spaces inside the top-level `services:` block, which is how every
/// service in that file is written and how a new one would be.
fn compose_services() -> BTreeSet<String> {
    let compose = read("testenv/docker-compose.yml");
    let body = compose
        .split_once("\nservices:\n")
        .expect("testenv/docker-compose.yml has a top-level `services:` block")
        .1;
    let mut services = BTreeSet::new();
    for line in body.lines() {
        // The block ends at the next top-level key (`volumes:`).
        if !line.starts_with(' ') && !line.trim().is_empty() && !line.starts_with('#') {
            break;
        }
        if let Some(name) = line
            .strip_prefix("  ")
            .filter(|rest| !rest.starts_with([' ', '#']))
            .and_then(|rest| rest.strip_suffix(':'))
        {
            services.insert(name.to_owned());
        }
    }
    assert!(
        services.len() >= 8,
        "this parse found only {services:?} in testenv/docker-compose.yml; that \
         file declares Kuma's services and the products', so a count \
         this low means the parse broke, not that the environment shrank"
    );
    services
}

#[test]
fn every_compose_service_the_estate_runs_is_recorded() {
    let estate = estate();
    let recorded: BTreeSet<String> = assets(&estate)
        .iter()
        .filter_map(|asset| asset["properties"]["compose_service"].as_str())
        .map(str::to_owned)
        .collect();
    let expected: BTreeSet<String> = compose_services()
        .into_iter()
        .filter(|service| !NOT_ASSETS.contains(&service.as_str()))
        .collect();
    assert_eq!(
        recorded, expected,
        "the estate file's `compose_service` properties and the compose file's \
         services have drifted apart. A service that runs is an asset; one that \
         is a job rather than a thing goes in NOT_ASSETS with its reason."
    );
}

/// **Every container carries the origin key the Docker importer matches it on**
/// (#510, spec #491 story 67).
///
/// The unique thing this can say, and it is the reason it exists: a
/// container's origin key is two values that every other line of the same
/// file already implies, so it can be gated here with no engine running. Its
/// `container_name` is its own `name`, and its `docker_context` is the one on
/// the engine it hangs off. So a hand edit that renames a container and forgets
/// the property, or moves it to another engine, is red here rather than red on
/// a live recipe an hour later, or silent.
///
/// `CONTEXT.md`, **Origin key**: *"the docker context plus the container name
/// for a container, since a container's id changes on every recreate and its
/// name does not."* Both parts are **properties**, because that is what
/// `assets::Producer::origin_key` matches on -- a name is a column and the
/// planner does not read columns -- and the same two are what
/// `assets::docker` writes into every entry it produces. Half a key matches
/// nothing on either side, so a container missing one of them would preview as
/// *new* and the import would make a second copy of it.
#[test]
fn every_container_carries_the_context_and_the_name_it_is_matched_on() {
    let estate = estate();
    let assets = assets(&estate);
    let context_of: HashMap<&str, &str> = assets
        .iter()
        .filter(|asset| field(asset, "type") == "container_engine")
        .map(|engine| {
            let context = engine["properties"]["docker_context"]
                .as_str()
                .unwrap_or_else(|| {
                    panic!(
                        "the container engine `{}` carries no `docker_context`. \
                         It is what the Docker importer spawns `docker \
                         --context` with, and an engine without one is an \
                         engine whose containers no import can read.",
                        id(engine)
                    )
                });
            (id(engine), context)
        })
        .collect();
    assert!(
        !context_of.is_empty(),
        "this parse found no container engine; if the type id moved, fix the \
         parse rather than deleting the check"
    );

    let mut checked = 0;
    for asset in assets {
        if field(asset, "type") != "container" {
            continue;
        }
        let entry = id(asset);
        assert_eq!(
            asset["properties"]["container_name"].as_str(),
            Some(field(asset, "name")),
            "`{entry}`'s `container_name` and its name say different things. \
             The property is half the origin key and the name is what a person \
             reads; a container is one thing and they are the same string."
        );
        let engine = field(asset, "parent");
        let context = context_of.get(engine).unwrap_or_else(|| {
            panic!("`{entry}` hangs off `{engine}`, which is not a container engine")
        });
        assert_eq!(
            asset["properties"]["docker_context"].as_str(),
            Some(*context),
            "`{entry}` says it is reached through a different docker context \
             from the engine `{engine}` it runs on. The importer reads it \
             through the engine's, so the container's would match nothing."
        );
        checked += 1;
    }
    assert!(
        checked >= 8,
        "only {checked} containers were checked; this file has carried more \
         than that since M4.0, so the parse has broken rather than the estate \
         having shrunk"
    );
}

/// What an asset entry may say, and what a route entry may say.
///
/// A closed vocabulary is the only thing that catches the mistake none of the
/// tests above can see: a **misspelled key**. `"monitor"` for `"monitors"`,
/// `"parnet"` for `"parent"`, `"targets"` for `"target"` -- every one of those
/// reads as an absent optional field, and an absent optional field is legal
/// everywhere it appears. The import would then draw no `monitored-by` link,
/// hang the asset off the root and land the route on nothing, all without a
/// word from the gate.
const ASSET_KEYS: &[&str] = &[
    "id",
    "type",
    "name",
    "parent",
    "description",
    "environment",
    "owner",
    "properties",
    "monitors",
];
// No `monitors` here: a monitor is "attached to an asset by a `monitored-by`
// link" (`CONTEXT.md`, *Monitor*), so a route that grew one would name a
// monitor the import has nowhere to hang. The products' routes share their
// names with the monitors of the containers they land on, and it is those
// containers that carry them.
const ROUTE_KEYS: &[&str] = &[
    "id",
    "asset",
    "target",
    "name",
    "url",
    "description",
    "properties",
];

#[test]
fn no_entry_says_anything_in_a_key_the_file_does_not_define() {
    let estate = estate();
    let check = |entry: &Value, allowed: &[&str]| {
        let object = entry
            .as_object()
            .unwrap_or_else(|| panic!("every entry is an object; this is {entry}"));
        for key in object.keys() {
            assert!(
                allowed.contains(&key.as_str()),
                "`{}` carries a key `{key}`, which is not one of {allowed:?}. A \
                 key nothing reads is a fact nobody gets: if it is a typo, fix \
                 it; if it is new, add it here and teach the import about it.",
                id(entry)
            );
        }
    };
    for asset in assets(&estate) {
        check(asset, ASSET_KEYS);
    }
    for route in routes(&estate) {
        check(route, ROUTE_KEYS);
    }
}

#[test]
fn a_monitor_is_named_by_a_name() {
    let estate = estate();
    let mut named = 0;
    for entry in assets(&estate) {
        let Some(monitors) = entry.get("monitors") else {
            continue;
        };
        let monitors = monitors.as_array().unwrap_or_else(|| {
            panic!(
                "`{}`'s `monitors` is a list of Uptime Kuma monitor names, and \
                 this is {monitors}",
                id(entry)
            )
        });
        assert!(
            !monitors.is_empty(),
            "`{}` has an empty `monitors`; leave the key out instead, so that \
             \"nothing watches this\" and \"somebody meant to fill this in\" do \
             not read alike",
            id(entry)
        );
        for monitor in monitors {
            assert!(
                monitor.as_str().is_some_and(|name| !name.trim().is_empty()),
                "`{}` names the monitor {monitor}; a monitor is named by its \
                 Uptime Kuma name, which is what the import resolves it by",
                id(entry)
            );
            named += 1;
        }
    }
    // The file exists partly to carry these; none at all means the key was
    // renamed and every assertion above skipped.
    assert!(
        named >= 5,
        "only {named} monitors are named in the estate file"
    );
}

/// The estate file's monitor names and `testenv/monitors.json`'s must agree,
/// because a name is the only thing joining them.
///
/// `monitors.json` is what `seed-kuma.sh` puts in the local Uptime Kuma, and
/// the import (#445) resolves an asset's `monitors` entry against a *live*
/// monitor by its name. So a rename on either side draws no `monitored-by`
/// link and reports an unresolved name -- which looks like a monitor that has
/// not synced yet, not like a typo, and is therefore the kind of thing nobody
/// chases. It has already happened once: #438 wrote these names before #441
/// created the monitors, and all seven disagreed.
///
/// One direction only. Every name the estate uses must be seeded; the reverse
/// is false by design, because the `canary` (`testenv/canary.sh`) watches
/// nothing and belongs to no asset.
#[test]
fn every_monitor_the_estate_names_is_one_the_seed_creates() {
    let seeded: Value = serde_json::from_str(&read("testenv/monitors.json"))
        .expect("testenv/monitors.json is JSON");
    let seeded: BTreeSet<&str> = seeded
        .as_array()
        .expect("testenv/monitors.json is an array of monitors")
        .iter()
        .map(|m| field(m, "name"))
        .collect();
    // A narrow parse that found nothing would pass every assertion below
    // without checking anything.
    assert!(
        seeded.len() >= 2,
        "testenv/monitors.json seeds only {} monitor(s); has it moved?",
        seeded.len()
    );

    let estate = estate();
    for entry in assets(&estate) {
        let Some(monitors) = entry.get("monitors").and_then(Value::as_array) else {
            continue;
        };
        for monitor in monitors {
            let name = monitor.as_str().unwrap_or_default();
            assert!(
                seeded.contains(name),
                "`{}` names the monitor {name:?}, which testenv/monitors.json does \
                 not seed. The seeded names are {seeded:?}. A name is all that \
                 joins the two files (../README.md, \"Monitors\"); rename in both \
                 or the import resolves nothing.",
                id(entry)
            );
        }
    }
}
