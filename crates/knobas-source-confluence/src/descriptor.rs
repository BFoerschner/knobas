//! What knobas knows about this adapter before an instance of it exists.
//!
//! `list_adapters()` serves this template to the Add-source form, which is
//! *generated* from `config_schema` + `auth_methods` (spec §3a). Nothing
//! downstream carries a hardcoded Confluence table, which is why the kind
//! metadata (label, plural, monogram) lives here and not in the launcher.
//!
//! # The declared payload paths (#277) -- TODO, deliberately
//!
//! ADR-0007's destination is a descriptor that **declares where each of its
//! payload's readable facts lives**, so knobas stops learning each source's
//! shape. Issue #277 adds that per-kind field to `SourceDescriptor`; it had
//! not merged when this crate landed, so the declaration cannot be written
//! yet -- the field does not exist to write it into. What this adapter will
//! declare, in full, so it is one mechanical edit and not a fresh decision:
//!
//! | fact | path | note |
//! |---|---|---|
//! | project key | `space.key` | ADR-0010: a **space** is Confluence's project |
//! | project name | `space.name` | |
//! | status | *(miss)* | a page has no status a reader could act on |
//! | priority | *(miss)* | Confluence has none |
//! | assignee | *(miss)* | a page has no assignee |
//! | reviewer | *(miss)* | no review model |
//! | merged | *(miss)* | not a change-proposal kind |
//!
//! Every one of these but the first two is a **miss** in ADR-0007's sense: the
//! read finds nothing and contributes nothing, which is the failure a payload
//! read is allowed. Declaring a path for a fact Confluence does not have would
//! be the forbidden alternative -- a guess.

use knobas_source::{AuthMethod, KindInfo, SourceDescriptor};

/// The static, instance-free descriptor: `id == adapter_kind` (contract §4.2).
/// A configured instance's descriptor is this with `id` and `name` replaced by
/// the instance's own.
#[must_use]
pub fn descriptor_template() -> SourceDescriptor {
    SourceDescriptor {
        id: crate::ADAPTER_KIND.to_owned(),
        adapter_kind: crate::ADAPTER_KIND.to_owned(),
        name: "Confluence".to_owned(),
        // Read-only in this ticket, so **no** capabilities: `Capability::Write`
        // with no ops leaves the UI nothing to offer, and the battery rejects
        // that pair in both directions. `Capability::Search` stays reserved
        // for a server-side `Source::search` the SPI does not have -- knobas'
        // launcher answers from the local index either way, and this adapter
        // would be the first with a real server-side search to declare if the
        // SPI ever grows one.
        capabilities: Vec::new(),
        adapter_version: crate::ADAPTER_VERSION.to_owned(),
        // Bearer PAT (DC >= 7.9) or Basic user+password (spec §3).
        auth_methods: vec![AuthMethod::Pat, AuthMethod::UserPassword],
        // `CreatePage`, `UpdatePage` and `Comment` are spec #272's Confluence
        // set and are the *next* ticket's, each a §10.8-ratified growth of
        // `WriteOp` (ADR-0006). Until they exist, this adapter refuses every
        // op it is handed, which the battery checks.
        write_ops: Vec::new(),
        entity_kinds: vec![KindInfo {
            id: crate::KIND_PAGE.to_owned(),
            label: "Page".to_owned(),
            plural: "Pages".to_owned(),
            // `PG` and not `CO`: the monogram stands for the *kind*, and
            // knobas' own vocabulary already spells a page that way
            // (`app/src/lib/shell/kinds.ts`). Two sources emitting pages
            // should not chip differently.
            monogram: "PG".to_owned(),
            // ADR-0003: a `cursor: None` run walks the whole CQL result set --
            // `type = page`, narrowed by the configured spaces and by nothing
            // else, unbounded in time -- so every page the credential can see
            // in that scope is emitted, and the engine's sweep may tombstone
            // whatever did not come back. Contrast TeamCity, which emits the
            // newest N builds per configuration and is therefore `false`.
            //
            // The scope is the *source's*, not the wiki's, and that is not a
            // weakening: a space this source was never configured to sync has
            // no rows in the mirror for a sweep to tombstone.
            full_sync_exhaustive: true,
        }],
        config_schema: config_schema(),
    }
}

/// The JSON Schema the Add-source form is generated from.
///
/// Every property carries a `default` the form can submit unchanged, and the
/// key set is pinned against [`crate::ConfluenceConfig`]'s own serialization
/// by this module's tests -- a schema key with no struct field is a form input
/// that is silently discarded.
fn config_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "flavor": {
                "type": "string", "enum": ["datacenter", "cloud"], "default": "datacenter",
                "title": "Deployment",
                "description": "Data Center / Server speaks REST v1 and CQL. Cloud is not supported yet."
            },
            "spaces": {
                "type": "array", "default": [], "title": "Spaces",
                "description": "Space keys to sync, e.g. ENG. Leave empty to sync every space this account can see.",
                "items": { "type": "string", "pattern": "^~?[A-Za-z0-9_.-]+$" }
            },
            "username": {
                "type": ["string", "null"], "default": null, "title": "Username",
                "description": "Your Confluence account. Filled in by Test connection; used for @me and My items. Also the login name for user + password authentication."
            },
            "page_size": {
                "type": "integer", "minimum": 1, "maximum": 50, "default": 50,
                "title": "Pages per request",
                "description": "Confluence clamps a content search to 50 results once a body is expanded, and this adapter always expands one."
            },
            "rate_per_sec": {
                "type": "integer", "minimum": 1, "default": 5, "title": "Requests per second"
            },
            "rate_burst": {
                "type": "integer", "minimum": 1, "default": 10, "title": "Request burst"
            },
            "connect_timeout_secs": {
                "type": "integer", "minimum": 1, "default": 10, "title": "Connect timeout (s)"
            },
            "request_timeout_secs": {
                "type": "integer", "minimum": 1, "default": 30, "title": "Request timeout (s)"
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use knobas_source::Capability;

    /// The sources view renders its action bar from `write_ops`, so what is
    /// here is exactly what the user is offered -- and for this ticket that is
    /// nothing. A write op declared here that `write` refuses is an action
    /// that 404s; the battery holds both directions.
    #[test]
    fn the_template_declares_one_kind_and_no_writes() {
        let d = descriptor_template();
        assert_eq!(d.id, crate::ADAPTER_KIND);
        assert_eq!(d.adapter_kind, crate::ADAPTER_KIND);
        assert!(d.capabilities.is_empty(), "{:?}", d.capabilities);
        assert!(
            !d.capabilities.contains(&Capability::Write),
            "declaring Write with no ops leaves the UI nothing to offer"
        );
        assert!(d.write_ops.is_empty(), "{:?}", d.write_ops);
        assert_eq!(d.adapter_version, crate::ADAPTER_VERSION);
        assert_eq!(
            d.auth_methods,
            vec![AuthMethod::Pat, AuthMethod::UserPassword]
        );
        let kinds: Vec<&str> = d.entity_kinds.iter().map(|k| k.id.as_str()).collect();
        assert_eq!(kinds, vec![crate::KIND_PAGE]);
        assert_eq!(d.entity_kinds[0].monogram, "PG");
        assert_eq!(d.entity_kinds[0].plural, "Pages");
    }

    /// ADR-0003: a `cursor: None` Confluence sync walks the whole configured
    /// scope, so the engine's post-full-sync sweep is safe -- anything whose
    /// `synced_at` predates the run really did stop coming back. `false` here
    /// would leave deleted pages in the mirror forever.
    #[test]
    fn a_full_sync_is_the_whole_corpus_of_the_configured_scope() {
        assert!(
            descriptor_template()
                .entity_kinds
                .iter()
                .all(|k| k.full_sync_exhaustive)
        );
    }

    /// The claim above is one the contract battery *cannot* check -- it would
    /// have to know the remote corpus. What makes it true is a single property
    /// of the query renderer: a `cursor: None` run puts **no time bound** in
    /// its CQL, so the result set is every page the credential can see in
    /// scope.
    ///
    /// Pinned here, beside the claim, rather than in `cql`'s own tests: it is
    /// the descriptor that becomes a lie if the renderer ever gains a bound,
    /// and the engine acts on the descriptor -- it sweeps every row whose
    /// `synced_at` predates a full run, so a windowed full sync would tombstone
    /// live pages.
    #[test]
    fn the_full_sync_claim_rests_on_a_query_with_no_time_bound() {
        for config in [
            serde_json::json!({}),
            serde_json::json!({ "spaces": ["ENG", "OPS"] }),
        ] {
            let cfg = crate::ConfluenceConfig::from_json(&config).unwrap();
            let cql = crate::cql::build_cql(&cfg, None, 7200, crate::cql::Order::Ascending);
            assert!(
                !cql.to_lowercase().contains("lastmodified >="),
                "a full sync claiming to be exhaustive must not bound time, but {config} \
                 renders {cql:?}"
            );
        }
    }

    /// The Add-source form is generated from `config_schema` (spec §3a), so
    /// the schema and the struct the adapter parses must describe the same
    /// keys. Drift here is a form field that is silently discarded, or a
    /// config key with no way to set it.
    #[test]
    fn the_schema_and_the_config_struct_agree() {
        let d = descriptor_template();
        let schema_keys: std::collections::BTreeSet<String> = d.config_schema["properties"]
            .as_object()
            .expect("config_schema.properties is an object")
            .keys()
            .cloned()
            .collect();
        let struct_keys: std::collections::BTreeSet<String> =
            serde_json::to_value(crate::ConfluenceConfig::default())
                .unwrap()
                .as_object()
                .unwrap()
                .keys()
                .cloned()
                .collect();
        assert_eq!(schema_keys, struct_keys);
    }

    /// Every default the form would submit has to parse back.
    #[test]
    fn the_schema_defaults_are_a_valid_configuration() {
        let d = descriptor_template();
        let mut object = serde_json::Map::new();
        for (key, prop) in d.config_schema["properties"].as_object().unwrap() {
            object.insert(
                key.clone(),
                prop.get("default")
                    .unwrap_or_else(|| panic!("{key} has no default for the form to submit"))
                    .clone(),
            );
        }
        crate::ConfluenceConfig::from_json(&serde_json::Value::Object(object)).unwrap();
    }

    /// The descriptor crosses the IPC bridge as plain data (spec §3a).
    #[test]
    fn the_template_survives_a_json_round_trip() {
        let json = serde_json::to_string(&descriptor_template()).unwrap();
        let back: SourceDescriptor = serde_json::from_str(&json).unwrap();
        assert_eq!(back.adapter_kind, crate::ADAPTER_KIND);
        assert!(back.write_ops.is_empty());
        assert!(back.entity_kinds[0].full_sync_exhaustive);
    }

    /// A schema property that named a password would put a secret in the DB.
    ///
    /// Property *names* and *titles*, not the whole serialized schema: the
    /// `username` field's help text legitimately says "user + password", and a
    /// substring scan over descriptions would forbid explaining the auth
    /// method the schema exists to support.
    #[test]
    fn the_schema_asks_for_no_secret() {
        let d = descriptor_template();
        for (key, prop) in d.config_schema["properties"].as_object().unwrap() {
            let title = prop["title"].as_str().unwrap_or_default().to_lowercase();
            for forbidden in ["password", "token", "secret", "pat"] {
                assert!(!key.to_lowercase().contains(forbidden), "property {key}");
                assert!(
                    !title.contains(forbidden),
                    "property {key} is titled {title:?}"
                );
            }
        }
    }
}
