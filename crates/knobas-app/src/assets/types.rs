//! The built-in asset types (spec #427, story 4 and 5; issue #428).
//!
//! Nineteen of them, in code and not in the database, and the reason is the
//! half of a type that SQL cannot hold: each one carries a **monogram** the
//! column rows draw and an **ordered typed-property schema** the pane shows
//! first. A `check (type_id in (…))` would be a second, partial copy of this
//! list -- the half that goes stale -- so `knobas.asset.type_id` is open text
//! and [`super::create`] is the one door that refuses a type nobody declared.
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
//! # What is not here yet
//!
//! The **child types conventionally suggested** (spec #427's type table, the
//! third column). Nothing in #428 creates through the UI -- #429 owns the
//! create dialog and the suggestion list it draws -- and a table of
//! suggestions with no reader would be untested prose. The ids above are what
//! it will be written against.
//!
//! *runtime* and *scenario* are generic on purpose: spec #427 replaced the
//! Flowrun-branded types with them so the tree can hold an Orchestra instance
//! by hand until an instance is reachable.

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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TypedProperty {
    /// The key it is stored under in `knobas.asset.properties`.
    pub key: &'static str,
    /// What the pane calls it.
    pub label: &'static str,
    /// What a value for it must be.
    pub kind: PropertyKind,
}

/// One built-in asset type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
}

/// A shorthand so the table below reads as a table.
const fn p(key: &'static str, label: &'static str, kind: PropertyKind) -> TypedProperty {
    TypedProperty { key, label, kind }
}

use PropertyKind::{Date as D, Number as N, Text as T, Url as U};

/// The nineteen built-in types, in spec #427's own order.
///
/// The order is the one story 4 lists them in -- outermost thing first, down to
/// the smallest -- so a reader scanning the list reads the estate top to
/// bottom. `custom` is last because it is the escape hatch rather than a level.
pub const TYPES: &[AssetType] = &[
    AssetType {
        id: "site",
        label: "Site",
        monogram: "SI",
        properties: &[p("location", "Location", T), p("provider", "Provider", T)],
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
    },
    AssetType {
        id: "container-engine",
        label: "Container engine",
        monogram: "CE",
        properties: &[p("version", "Version", T), p("socket", "Socket", T)],
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
    },
    AssetType {
        id: "module",
        label: "Module",
        monogram: "MD",
        properties: &[p("version", "Version", T), p("repository", "Repository", U)],
    },
    AssetType {
        id: "runtime",
        label: "Runtime",
        monogram: "RT",
        properties: &[p("url", "URL", U), p("version", "Version", T)],
    },
    AssetType {
        id: "scenario",
        label: "Scenario",
        monogram: "SC",
        properties: &[p("path", "Path", T), p("last_run", "Last run", D)],
    },
    AssetType {
        id: "step",
        label: "Step",
        monogram: "SP",
        properties: &[p("position", "Position", N), p("action", "Action", T)],
    },
    AssetType {
        id: "connector",
        label: "Connector",
        monogram: "CN",
        properties: &[p("protocol", "Protocol", T), p("target", "Target", T)],
    },
    AssetType {
        id: "database-server",
        label: "Database server",
        monogram: "DS",
        properties: &[
            p("engine", "Engine", T),
            p("version", "Version", T),
            p("host", "Host", T),
            p("port", "Port", N),
        ],
    },
    AssetType {
        id: "database",
        label: "Database",
        monogram: "DB",
        properties: &[p("engine", "Engine", T), p("size_mb", "Size (MB)", N)],
    },
    AssetType {
        id: "schema",
        label: "Schema",
        monogram: "SM",
        properties: &[p("owner", "Owner", T)],
    },
    AssetType {
        id: "table",
        label: "Table",
        monogram: "TB",
        properties: &[p("rows", "Rows", N)],
    },
    AssetType {
        id: "reverse-proxy",
        label: "Reverse proxy",
        monogram: "RP",
        properties: &[
            p("config_path", "Config path", T),
            p("upstreams", "Upstreams", T),
        ],
    },
    AssetType {
        id: "middleware",
        label: "Middleware",
        monogram: "MW",
        properties: &[
            p("protocol", "Protocol", T),
            p("config_path", "Config path", T),
        ],
    },
    AssetType {
        id: "network",
        label: "Network",
        monogram: "NW",
        properties: &[p("cidr", "CIDR", T), p("gateway", "Gateway", T)],
    },
    AssetType {
        id: "custom",
        label: "Custom",
        monogram: "CU",
        // Story 7: the escape hatch declares nothing. See the module docs.
        properties: &[],
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

    /// Story 4 names nineteen types, and this is the list.
    #[test]
    fn the_table_holds_the_nineteen_types_the_spec_names() {
        let ids: Vec<&str> = TYPES.iter().map(|t| t.id).collect();
        assert_eq!(
            ids,
            [
                "site",
                "hypervisor",
                "vm",
                "container-engine",
                "container",
                "service",
                "module",
                "runtime",
                "scenario",
                "step",
                "connector",
                "database-server",
                "database",
                "schema",
                "table",
                "reverse-proxy",
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

    #[test]
    fn find_answers_for_a_declared_type_and_misses_for_anything_else() {
        assert_eq!(find("vm").map(|t| t.label), Some("VM"));
        for unknown in ["", "VM", "flowrun-scenario", "asset", "site "] {
            assert!(find(unknown).is_none(), "{unknown:?} is not a type");
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
