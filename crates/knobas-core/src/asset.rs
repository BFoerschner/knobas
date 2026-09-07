//! The built-in asset types (spec #427, stories 4 and 5; issue #428).
//!
//! Sixteen of them, in code and not in the database, and the reason is the
//! half of a type that SQL cannot hold: each one carries a **monogram** the
//! column rows draw and an **ordered typed-property schema** the pane shows
//! first. A `check (type_id in (…))` would be a second, partial copy of this
//! list -- the half that goes stale -- so `knobas.asset.type_id` is open text
//! and `knobas_app::assets::create` is the one door that refuses a type nobody
//! declared.
//!
//! # Why this is in `knobas-core` and not beside the store
//!
//! Everything else about an asset lives in `knobas_app::assets`, under the
//! §10.8 module-pair exception. This table does not, because it has a **second
//! reader**: `knobas-core`'s own `tests/estate_file.rs` checks that every type
//! the checked-in estate file names is one that exists, and `knobas-core`
//! cannot depend on `knobas-app`. That test shipped with #438 carrying a
//! hand-written copy of this list and said so in as many words -- *"this
//! constant is the copy to delete, and the test should read the types off it
//! instead"* -- which is what landing it here makes possible. It is the same
//! move `closed_vocabulary!` made from `knobas-sync`: to the crate both sides
//! depend on, so the list is one list.
//!
//! # The ids are wire values and never change
//!
//! Spec #427: *"Type ids travel in the share export and are stable from the
//! first release."* An asset imported on another machine carries `vm`, and a
//! rename here would make it a type that machine does not know. The
//! **labels** are display and may be reworded; the ids may not.
//!
//! # `custom` carries no typed properties, deliberately
//!
//! Story 7: *"an asset of type custom carries only custom properties, so that
//! a thing the list has no name for still fits"*. It is the escape hatch, and
//! a schema on it would be an opinion about a thing the list has no name for.
//! [`tests::custom_declares_no_typed_properties`] pins it, because an empty
//! slice is exactly what a careless edit adds a field to.
//!
//! # The third column: what usually goes here
//!
//! Each type carries [`AssetType::suggests`], the **child types
//! conventionally suggested** under it -- spec #427's type table, the third
//! column, and story 17's *"the type conventions suggesting what usually goes
//! here and any type allowed"*. #428 left it out because a table of
//! suggestions with no reader would have been untested prose; #429's create
//! dialog is that reader, and it draws the list as *usual here: ...* above a
//! picker that still offers all sixteen.
//!
//! **A suggestion is never a constraint.** [`crate::asset`] does not enforce
//! it and neither does `knobas_app::assets::create`: the estate is somebody's
//! real infrastructure, and a container held under a compose project that runs
//! on a VM elsewhere (story 16) is exactly the shape a constraint here would
//! have refused. What the list buys is the common case costing one click.
//!
//! # Why there is no low-code-runtime chain any more
//!
//! *runtime* is generic on purpose: spec #427 replaced a vendor-branded type
//! with it, so the tree can hold a low-code runtime instance by hand. The
//! three types that hung below it -- *scenario*, *step* and *connector* --
//! left the table with spec #491, because no adapter for such a system is
//! planned and a type nothing real is filed under is a chip in the create
//! dialog with nothing behind it. `knobas.asset.type_id` is open text, so a
//! row still carrying one of the three is read rather than refused; nothing
//! was migrated.

/// What a property value may be.
///
/// The four the spec names -- text, number, date, url -- and no fifth. The
/// *secret* kind is deferred with its keychain convention (spec #427, Out of
/// Scope), so there is nowhere in an asset a credential can be typed.
///
/// A tagged union on the wire, `{"kind":"number","value":8080}`, and the same
/// shape in the `properties` jsonb: the kind is a fact about the value and
/// storing it beside the value is what lets a custom key be read back as the
/// thing it was entered as. A bare JSON scalar would make `"8080"` and `8080`
/// the same property and would leave a date indistinguishable from a string.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PropertyKind {
    Text,
    Number,
    Date,
    Url,
}

impl PropertyKind {
    /// Every kind, for a walk that must not miss one.
    pub const ALL: [PropertyKind; 4] = [
        PropertyKind::Text,
        PropertyKind::Number,
        PropertyKind::Date,
        PropertyKind::Url,
    ];

    /// The serde spelling, which is also the wire value.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            PropertyKind::Text => "text",
            PropertyKind::Number => "number",
            PropertyKind::Date => "date",
            PropertyKind::Url => "url",
        }
    }
}

/// One declared property of a type.
///
/// Serialized as it stands: `asset_types` puts the table on the wire so the
/// create dialog and the pane's property editor read *one* list. Without it
/// the frontend would carry a second copy of sixteen types -- and, worse,
/// would have to guess which kind an unfilled typed property takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub struct TypedProperty {
    /// The key it is stored under in `knobas.asset.properties`.
    pub key: &'static str,
    /// What the pane calls it.
    pub label: &'static str,
    /// What a value for it must be.
    pub kind: PropertyKind,
}

/// One built-in asset type.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub struct AssetType {
    /// The `knobas.asset.type_id` value. A wire value -- see the module docs.
    pub id: &'static str,
    /// What one of them is called.
    pub label: &'static str,
    /// Two characters, for the fixed-width chip a column row draws.
    pub monogram: &'static str,
    /// The typed properties, **in the order the pane shows them**. The order is
    /// the declaration's; it is not sorted anywhere.
    pub properties: &'static [TypedProperty],
    /// The child types conventionally suggested under one of these, by id, in
    /// the order the create dialog offers them.
    ///
    /// A **suggestion and not a constraint** -- see the module docs. Empty is
    /// a real answer: nothing usually goes inside a table, a network or a
    /// `custom`, and an empty list is what makes the dialog say *any type*
    /// rather than name one at random.
    ///
    /// Every id here is one [`TYPES`] declares, which
    /// [`tests::every_suggestion_is_a_type_that_exists`] holds it to: a typo
    /// would be a chip in the dialog that creates a type
    /// `knobas_app::assets::create` refuses.
    pub suggests: &'static [&'static str],
}

/// A shorthand so the table below reads as a table.
const fn p(key: &'static str, label: &'static str, kind: PropertyKind) -> TypedProperty {
    TypedProperty { key, label, kind }
}

// No `Date as D`: since spec #491 dropped *scenario* and its `last_run`, no
// built-in type declares a date. The kind itself stays -- a custom property
// may be one -- and `knobas_app::assets` exercises the declared-date arm
// against a type it builds by hand.
use PropertyKind::{Number as N, Text as T, Url as U};

/// The sixteen built-in types, in spec #427's own order.
///
/// The order is the one story 4 lists them in -- outermost thing first, down to
/// the smallest -- so a reader scanning the list reads the estate top to
/// bottom. `custom` is last because it is the escape hatch rather than a level.
///
/// # Where the `suggests` lists come from
///
/// Three buckets, in descending order of how much they answer to:
///
/// * **Design §12.1's two chains** -- *site > hypervisor > VM > engine >
///   container > runtime* -- §12.1 drew three more links below *runtime*, and
///   spec #491 dropped the types they named -- *and database server >
///   database > schema > table*. Every link in both is a suggestion here, and
///   [`tests::the_two_chains_the_design_draws_are_each_a_link_at_a_time`] reads
///   them back.
/// * **What the real estate actually holds.** `testenv/hetzner/estate.json`
///   describes provisioned infrastructure, and it holds three pairs the chains
///   do not draw -- a site under a site, a VM directly under a site, and a
///   database inside a container. `knobas-core`'s own `tests/estate_file.rs`
///   asserts that every parent-and-child pair in that file is suggested here,
///   so the conventions answer to an estate that exists (ADR-0013) rather than
///   to a diagram.
///
/// * **The neighbouring types §12.1's prose names, and the few this estate
///   will plainly grow into.** A service or a database server on a VM, a
///   middleware behind a reverse proxy, a module under a service. And four
///   the chains do not draw and the estate has no instance
///   of yet, each argued rather than assumed: a `network` under a `site`
///   (a network belongs to the place it is provisioned in, and there is
///   nowhere else in the table to hang one), a `reverse_proxy` on a `vm` (the
///   estate runs one and will hold it once it is described), and a `service`
///   inside a `container` and behind a `reverse_proxy` -- the two ways a
///   service is actually reached here. These are the judgement calls, and
///   they are suggestions: the dialog offers all sixteen whatever is listed,
///   and nothing in the table is a constraint.
pub const TYPES: &[AssetType] = &[
    AssetType {
        id: "site",
        label: "Site",
        monogram: "SI",
        properties: &[p("location", "Location", T), p("provider", "Provider", T)],
        suggests: &["site", "hypervisor", "vm", "network"],
    },
    AssetType {
        id: "hypervisor",
        label: "Hypervisor",
        monogram: "HV",
        properties: &[
            p("hostname", "Hostname", T),
            p("ip", "IP", T),
            p("os", "OS", T),
        ],
        suggests: &["vm"],
    },
    AssetType {
        id: "vm",
        label: "VM",
        monogram: "VM",
        properties: &[
            p("hostname", "Hostname", T),
            p("ip", "IP", T),
            p("os", "OS", T),
            p("size", "Size", T),
        ],
        suggests: &[
            "container_engine",
            "service",
            "database_server",
            "reverse_proxy",
        ],
    },
    AssetType {
        id: "container_engine",
        label: "Container engine",
        monogram: "CE",
        properties: &[p("version", "Version", T), p("socket", "Socket", T)],
        suggests: &["container"],
    },
    AssetType {
        id: "container",
        label: "Container",
        monogram: "CT",
        properties: &[
            p("image", "Image", T),
            p("ports", "Ports", T),
            p("restart_policy", "Restart policy", T),
        ],
        suggests: &["service", "database", "runtime"],
    },
    AssetType {
        id: "service",
        label: "Service",
        monogram: "SV",
        properties: &[
            p("url", "URL", U),
            p("port", "Port", N),
            p("health_path", "Health path", T),
        ],
        suggests: &["module"],
    },
    AssetType {
        id: "module",
        label: "Module",
        monogram: "MD",
        properties: &[p("version", "Version", T), p("repository", "Repository", U)],
        suggests: &[],
    },
    AssetType {
        id: "runtime",
        label: "Runtime",
        monogram: "RT",
        properties: &[p("url", "URL", U), p("version", "Version", T)],
        // Nothing: *scenario*, *step* and *connector* were what went inside
        // one, and they left the table with spec #491.
        suggests: &[],
    },
    AssetType {
        id: "database_server",
        label: "Database server",
        monogram: "DS",
        properties: &[
            p("engine", "Engine", T),
            p("version", "Version", T),
            p("host", "Host", T),
            p("port", "Port", N),
        ],
        suggests: &["database"],
    },
    AssetType {
        id: "database",
        label: "Database",
        monogram: "DB",
        properties: &[p("engine", "Engine", T), p("size_mb", "Size (MB)", N)],
        suggests: &["schema"],
    },
    AssetType {
        id: "schema",
        label: "Schema",
        monogram: "SM",
        properties: &[p("owner", "Owner", T)],
        suggests: &["table"],
    },
    AssetType {
        id: "table",
        label: "Table",
        monogram: "TB",
        properties: &[p("rows", "Rows", N)],
        suggests: &[],
    },
    AssetType {
        id: "reverse_proxy",
        label: "Reverse proxy",
        monogram: "RP",
        properties: &[
            p("config_path", "Config path", T),
            p("upstreams", "Upstreams", T),
        ],
        suggests: &["middleware", "service"],
    },
    AssetType {
        id: "middleware",
        label: "Middleware",
        monogram: "MW",
        properties: &[
            p("protocol", "Protocol", T),
            p("config_path", "Config path", T),
        ],
        suggests: &[],
    },
    AssetType {
        id: "network",
        label: "Network",
        monogram: "NW",
        properties: &[p("cidr", "CIDR", T), p("gateway", "Gateway", T)],
        suggests: &[],
    },
    AssetType {
        id: "custom",
        label: "Custom",
        monogram: "CU",
        // Story 7: the escape hatch declares nothing. See the module docs.
        properties: &[],
        suggests: &[],
    },
];

/// The type `id` names, or [`None`] for a word no type carries.
#[must_use]
pub fn find(id: &str) -> Option<&'static AssetType> {
    TYPES.iter().find(|declared| declared.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    /// Story 4 named nineteen types; spec #491 dropped three of them, and
    /// this is what is left.
    ///
    /// Spelled lower snake case, the way this repository spells every other
    /// enumerated column value, and the way `testenv/hetzner/estate.json`
    /// already spells them -- #438 chose the spelling and left the table to
    /// this ticket, so a hyphen here would have made the checked-in estate
    /// unimportable.
    #[test]
    fn the_table_holds_the_sixteen_types_the_spec_names() {
        let ids: Vec<&str> = TYPES.iter().map(|t| t.id).collect();
        assert_eq!(
            ids,
            [
                "site",
                "hypervisor",
                "vm",
                "container_engine",
                "container",
                "service",
                "module",
                "runtime",
                "database_server",
                "database",
                "schema",
                "table",
                "reverse_proxy",
                "middleware",
                "network",
                "custom",
            ]
        );
    }

    /// A monogram is drawn into a fixed-width chip, so two characters is an
    /// invariant -- the rule `knobas_core::entity`'s own monogram test states.
    ///
    /// Distinct as well as two characters: the chip is the *only* thing a
    /// column row says about a type, so two types sharing one would be two
    /// things a reader cannot tell apart at the width the Tree draws them.
    #[test]
    fn every_monogram_is_two_characters_and_no_two_types_share_one() {
        let mut seen = BTreeSet::new();
        for declared in TYPES {
            assert_eq!(
                declared.monogram.chars().count(),
                2,
                "{:?} has monogram {:?}",
                declared.id,
                declared.monogram
            );
            assert!(
                seen.insert(declared.monogram),
                "{:?} reuses the monogram {:?}",
                declared.id,
                declared.monogram
            );
            assert!(!declared.label.trim().is_empty(), "{:?}", declared.id);
        }
    }

    /// Two types may share a property *key* -- `hostname` is a VM's and a
    /// hypervisor's -- but one type may not declare the same key twice, and a
    /// key is what the jsonb bag is keyed on.
    #[test]
    fn no_type_declares_one_key_twice() {
        for declared in TYPES {
            let mut seen = BTreeSet::new();
            for property in declared.properties {
                assert!(
                    seen.insert(property.key),
                    "{:?} declares {:?} twice",
                    declared.id,
                    property.key
                );
                assert!(!property.label.trim().is_empty(), "{:?}", property.key);
            }
        }
    }

    /// Story 7, and the reason it is asserted rather than left to the reader:
    /// an empty slice is exactly what a careless edit adds a field to.
    #[test]
    fn custom_declares_no_typed_properties() {
        let custom = find("custom").expect("`custom` is in the table");
        assert!(custom.properties.is_empty());
        assert!(
            TYPES.iter().filter(|t| t.properties.is_empty()).count() == 1,
            "`custom` is the only type with no typed properties"
        );
    }

    /// A suggestion nothing declares is a chip in the create dialog that
    /// mints a type `knobas_app::assets::create` refuses -- a failure the
    /// reader meets after choosing, with a message about a word they never
    /// typed.
    #[test]
    fn every_suggestion_is_a_type_that_exists() {
        let declared: BTreeSet<&str> = TYPES.iter().map(|t| t.id).collect();
        for asset_type in TYPES {
            let mut seen = BTreeSet::new();
            for suggestion in asset_type.suggests {
                assert!(
                    declared.contains(suggestion),
                    "{:?} suggests {suggestion:?}, which is not a type",
                    asset_type.id
                );
                assert!(
                    seen.insert(suggestion),
                    "{:?} suggests {suggestion:?} twice",
                    asset_type.id
                );
            }
        }
    }

    /// Design §12.1's two chains, read back a link at a time.
    ///
    /// The pairs and not the count: a table that suggested every type under
    /// every type would satisfy a count and would tell a reader nothing, and
    /// `custom` below is the other half of that -- the escape hatch suggests
    /// nothing, the way it declares no properties.
    #[test]
    fn the_two_chains_the_design_draws_are_each_a_link_at_a_time() {
        let chains = [
            // site > hypervisor > VM > engine > container > runtime
            // (§12.1 drew three more links below *runtime*; spec #491 dropped
            // the types they named)
            [
                "site",
                "hypervisor",
                "vm",
                "container_engine",
                "container",
                "runtime",
            ]
            .as_slice(),
            // database server > database > schema > table
            ["database_server", "database", "schema", "table"].as_slice(),
        ];
        for chain in chains {
            for pair in chain.windows(2) {
                let (holder, held) = (pair[0], pair[1]);
                let declared = find(holder).expect("a type in the chain");
                assert!(
                    declared.suggests.contains(&held),
                    "{holder:?} does not suggest {held:?}, which §12.1's chain \
                     puts directly inside it: {:?}",
                    declared.suggests
                );
            }
        }
        assert!(
            find("custom")
                .expect("`custom` is in the table")
                .suggests
                .is_empty(),
            "the escape hatch suggests nothing, the way it declares nothing"
        );
        assert!(
            find("table")
                .expect("`table` is in the table")
                .suggests
                .is_empty(),
            "the chain ends at a table and the list says so"
        );
    }

    #[test]
    fn find_answers_for_a_declared_type_and_misses_for_anything_else() {
        assert_eq!(find("vm").map(|t| t.label), Some("VM"));
        // An id this build never had.
        for unknown in ["", "VM", "tape-library", "asset", "site "] {
            assert!(find(unknown).is_none(), "{unknown:?} is not a type");
        }
        // And an id this build *dropped*, which is the other way a word gets
        // here: spec #491 took these three out of the table, and a row in an
        // older database still carries one. The two cases are named apart on
        // purpose -- `knobas_app::assets` has a test for what such a row reads
        // as, and it would say nothing if a dropped id and an invented one
        // were the same case.
        for dropped in ["scenario", "step", "connector"] {
            assert!(find(dropped).is_none(), "{dropped:?} left the table");
        }
    }

    /// The kind list and its spellings are one fact; the mirror is pinned
    /// against `ALL`, so a fifth kind cannot arrive without the mirror hearing
    /// about it.
    #[test]
    fn every_property_kind_spells_itself_the_way_serde_does() {
        for kind in PropertyKind::ALL {
            let json = serde_json::to_value(kind).unwrap();
            assert_eq!(json, serde_json::json!(kind.as_str()));
        }
        assert_eq!(PropertyKind::ALL.len(), 4);
    }
}
