//! The schema check that keeps `testenv/hetzner/estate.json` honest.
//!
//! # What the file is
//!
//! `testenv/hetzner/estate.json` describes the **estate** (`CONTEXT.md`) as it
//! is provisioned: the notebook and its OrbStack Docker engine, the three
//! Hetzner servers with their containers and databases, and the tunnel's
//! forwards as routes. It is data about a real estate and the only asset
//! fixture there will be -- the M4 spec (#427) rules out anything
//! Tidewater-shaped for assets, because a made-up estate proves that the tree
//! draws and not that a real estate fits it.
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
//! [`every_hetzner_server_in_the_host_list_is_here_with_its_address`] and
//! [`every_compose_service_the_notebook_runs_is_recorded`] are the ones that
//! catch the drift that actually happens: a fourth server, or a new service in
//! the compose file, added by somebody who has never heard of this JSON. They
//! read `README.md`'s host-list table and `docker-compose.yml`'s service list
//! rather than a copy of them, so the day either grows a row the estate file
//! is red until it grows one too. Both parses are deliberately narrow and both
//! assert that they found something, so a rewrite that moves the table or the
//! services elsewhere fails loudly instead of silently checking nothing.
//!
//! # The type table
//!
//! [`TYPES`] is spec §12.1's built-in list as the M4 spec amends it (*runtime*
//! and *scenario* in place of the Flowrun-branded pair), spelled the way this
//! repository spells every other enumerated column value: lower case, snake
//! case, one word per part. The table itself lands in code with the asset model
//! (#428); when it does, this constant is the copy to delete, and the test
//! should read the types off it instead.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::net::Ipv4Addr;
use std::path::{Path, PathBuf};

use serde_json::Value;

/// The built-in asset types, by their id.
///
/// See the module docs: this is a copy of a table that does not exist in code
/// yet, and it goes when the table does.
const TYPES: &[&str] = &[
    "site",
    "hypervisor",
    "vm",
    "container_engine",
    "container",
    "service",
    "module",
    "runtime",
    "scenario",
    "step",
    "connector",
    "database_server",
    "database",
    "schema",
    "table",
    "reverse_proxy",
    "middleware",
    "network",
    "custom",
];

/// The four environments an asset may be in (spec §12.1).
const ENVIRONMENTS: &[&str] = &["dev", "stage", "prod", "shared"];

/// A compose service that is deliberately not an asset, and why.
///
/// `kuma-seed` is a one-shot job -- `profiles: ["seed"]` keeps it out of `up`
/// and `seed-kuma.sh` runs it with `compose run --rm`. It is a thing that is
/// *done to* the estate, not a thing the estate holds, so it has no asset and
/// no monitor. Every other service in that file runs somewhere and is recorded.
const NOT_ASSETS: &[&str] = &["kuma-seed"];

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
    serde_json::from_str(&read("testenv/hetzner/estate.json"))
        .expect("testenv/hetzner/estate.json is JSON")
}

/// The `assets` array, which the file must have and which must not be empty --
/// so that every test below is a claim about something.
fn assets(estate: &Value) -> &Vec<Value> {
    let assets = estate["assets"]
        .as_array()
        .expect("the estate file has an `assets` array");
    assert!(!assets.is_empty(), "the estate file describes no assets");
    assets
}

fn routes(estate: &Value) -> &Vec<Value> {
    let routes = estate["routes"]
        .as_array()
        .expect("the estate file has a `routes` array");
    assert!(!routes.is_empty(), "the estate file describes no routes");
    routes
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
        let id = id(entry);
        assert!(
            seen.insert(id),
            "`{id}` is the id of two entries in the estate file; an id is an \
             address and two things cannot share one"
        );
    }
}

#[test]
fn every_id_is_a_slug_in_its_own_namespace() {
    let estate = estate();
    let slug = |id: &str, prefix: &str| {
        let rest = id.strip_prefix(prefix).unwrap_or_else(|| {
            panic!("`{id}` should be addressed as `{prefix}<slug>` (spec §2)")
        });
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
        let id = id(route);
        let exposed_by = field(route, "asset");
        assert!(
            ids.contains(exposed_by),
            "`{id}` is exposed by `{exposed_by}`, which is not an asset in this file"
        );
        // A route may have no target -- an endpoint that lands on nothing
        // knobas knows is still a route -- but a named one has to resolve.
        if let Some(target) = route.get("target").and_then(Value::as_str) {
            assert!(
                ids.contains(target),
                "`{id}` targets `{target}`, which is not an asset in this file; \
                 `reachable via` is computed from the target end and reads \
                 nothing at all when it dangles"
            );
        }
        assert!(
            field(route, "url").contains("://"),
            "`{id}` needs a url with a scheme"
        );
    }
}

#[test]
fn every_type_is_in_the_type_table() {
    let estate = estate();
    let table: HashSet<&str> = TYPES.iter().copied().collect();
    for asset in assets(&estate) {
        let asset_type = field(asset, "type");
        assert!(
            table.contains(asset_type),
            "`{}` is of type `{asset_type}`, which is not one of the built-in \
             types {TYPES:?}; user-defined types are deferred and `custom` is \
             what a thing the list has no name for gets",
            id(asset)
        );
    }
}

#[test]
fn the_environment_and_the_owner_are_set_at_the_root_and_nowhere_else() {
    let estate = estate();
    for asset in assets(&estate) {
        let id = id(asset);
        let inherited = |key: &str| asset.get(key).and_then(Value::as_str);
        if id == "asset:knobas-estate" {
            assert_eq!(inherited("environment"), Some("dev"));
            assert!(
                inherited("owner").is_some_and(|owner| !owner.is_empty()),
                "the root sets the owner every asset inherits"
            );
        } else {
            assert_eq!(
                (inherited("environment"), inherited("owner")),
                (None, None),
                "`{id}` sets an environment or an owner of its own. That is \
                 allowed by the model and wrong here: this estate is one \
                 environment with one owner, and setting either again is how \
                 the inheritance stops being witnessed by the file."
            );
        }
        if let Some(environment) = inherited("environment") {
            assert!(
                ENVIRONMENTS.contains(&environment),
                "`{id}`'s environment `{environment}` is not one of {ENVIRONMENTS:?}"
            );
        }
    }
}

/// The servers named in this directory's README, read out of its host-list
/// table: `| `knobas-teamcity` | cx23 | ... |`.
fn host_list() -> Vec<String> {
    let readme = read("testenv/hetzner/README.md");
    let names: Vec<String> = readme
        .lines()
        .filter_map(|line| line.strip_prefix("| `knobas-"))
        .filter_map(|rest| rest.split_once('`'))
        .map(|(name, _)| format!("knobas-{name}"))
        .collect();
    assert_eq!(
        names.len(),
        3,
        "the host-list table in testenv/hetzner/README.md should have one row \
         per server and this parse found {names:?}; if the table moved, fix \
         this parse rather than deleting the check"
    );
    names
}

#[test]
fn every_hetzner_server_in_the_host_list_is_here_with_its_address() {
    let estate = estate();
    let assets = assets(&estate);
    for server in host_list() {
        let asset = assets
            .iter()
            .find(|asset| field(asset, "name") == server)
            .unwrap_or_else(|| {
                panic!(
                    "the README's host list names the server `{server}` and no \
                     asset in the estate file is called that"
                )
            });
        assert_eq!(
            field(asset, "type"),
            "vm",
            "`{server}` is a cloud server and belongs in the tree as a vm"
        );
        let address = asset["properties"]["ipv4"]
            .as_str()
            .unwrap_or_else(|| panic!("`{server}` needs its public `ipv4` property"));
        address.parse::<Ipv4Addr>().unwrap_or_else(|e| {
            panic!("`{server}`'s ipv4 property `{address}` is not an address: {e}")
        });
        assert!(
            asset["properties"]["server_type"].is_string(),
            "`{server}` needs its Hetzner `server_type`"
        );
    }
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
         file declares the notebook's services and the products', so a count \
         this low means the parse broke, not that the environment shrank"
    );
    services
}

#[test]
fn every_compose_service_the_notebook_runs_is_recorded() {
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
