//! What knobas knows about this adapter before an instance of it exists.
//!
//! `list_adapters()` (interfaces §2.2) serves this template to the Add-source
//! form, which is *generated* from `config_schema` + `auth_methods` (spec §3a).
//! Nothing downstream carries a hardcoded Jira table, which is why the kind
//! metadata (label, plural, monogram) lives here and not in the launcher.

use knobas_source::{AuthMethod, KindInfo, SourceDescriptor};

/// The static, instance-free descriptor: `id == adapter_kind` (interfaces §4.2).
/// A configured instance's descriptor is this with `id` and `name` replaced by
/// the instance's own.
#[must_use]
pub fn descriptor_template() -> SourceDescriptor {
    SourceDescriptor {
        id: crate::ADAPTER_KIND.to_owned(),
        adapter_kind: crate::ADAPTER_KIND.to_owned(),
        name: "Jira".to_owned(),
        // P12: read-only in M1, so no capabilities at all. `Capability::Search`
        // is reserved for a future server-side `Source::search`.
        capabilities: Vec::new(),
        adapter_version: crate::ADAPTER_VERSION.to_owned(),
        // Bearer PAT (DC >= 8.14) or Basic user+password (spec §3).
        auth_methods: vec![AuthMethod::Pat, AuthMethod::UserPassword],
        // M1 is read-only toward every source (interfaces §4.1).
        write_ops: Vec::new(),
        entity_kinds: vec![KindInfo {
            id: crate::KIND_TICKET.to_owned(),
            label: "Ticket".to_owned(),
            plural: "Tickets".to_owned(),
            monogram: "JI".to_owned(),
            // A `cursor: None` run walks the whole JQL result set -- `project
            // in (…)` or the configured filter, unbounded in time -- so every
            // issue the credential can see is emitted, and the engine's sweep
            // may tombstone whatever did not come back. Contrast TeamCity,
            // which emits the newest N builds per configuration and is
            // therefore `false`. Jira emits one kind, so this adapter is
            // wholly exhaustive (ADR-0003 changed where the claim is written,
            // not what Jira claims).
            full_sync_exhaustive: true,
        }],
        config_schema: config_schema(),
    }
}

/// The JSON Schema the Add-source form is generated from.
///
/// Every property carries a `default` the form can submit unchanged, and the
/// key set is pinned against [`crate::JiraConfig`]'s own serialization by this
/// module's tests -- a schema key with no struct field is a form input that is
/// silently discarded.
fn config_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "flavor": {
                "type": "string", "enum": ["datacenter", "cloud"], "default": "datacenter",
                "title": "Deployment",
                "description": "Data Center / Server speaks REST v2. Cloud is not supported yet."
            },
            "projects": {
                "type": "array", "default": [], "title": "Projects",
                "description": "Project keys to sync, e.g. PAY. Leave empty to sync everything this account can see.",
                "items": { "type": "string", "pattern": "^[A-Z][A-Z0-9_]{0,31}$" }
            },
            "jql_filter": {
                "type": ["string", "null"], "default": null, "title": "JQL filter",
                "description": "Alternative to Projects: any JQL, without an ORDER BY."
            },
            "username": {
                "type": ["string", "null"], "default": null, "title": "Username",
                "description": "Only for user + password authentication."
            },
            "epic_link_field": {
                "type": ["string", "null"], "default": null, "title": "Epic Link field id",
                "description": "e.g. customfield_10008. Only for classic projects: epic membership on a next-gen project is synced already, via fields.parent."
            },
            "page_size": {
                "type": "integer", "minimum": 1, "maximum": 1000, "default": 100,
                "title": "Issues per request"
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

    /// P12: M1 adapters declare no capabilities and no write ops. The contract
    /// battery reads both, and the sources view renders its action bar from
    /// `write_ops` -- an accidental entry here is an action that 404s.
    #[test]
    fn the_template_is_read_only_and_self_describing() {
        let d = descriptor_template();
        assert_eq!(d.id, crate::ADAPTER_KIND);
        assert_eq!(d.adapter_kind, crate::ADAPTER_KIND);
        assert!(d.capabilities.is_empty(), "{:?}", d.capabilities);
        assert!(d.write_ops.is_empty(), "{:?}", d.write_ops);
        assert_eq!(d.adapter_version, crate::ADAPTER_VERSION);
        assert_eq!(
            d.auth_methods,
            vec![
                knobas_source::AuthMethod::Pat,
                knobas_source::AuthMethod::UserPassword
            ]
        );
        let kinds: Vec<&str> = d.entity_kinds.iter().map(|k| k.id.as_str()).collect();
        assert_eq!(kinds, vec![crate::KIND_TICKET]);
        assert_eq!(d.entity_kinds[0].monogram, "JI");
        assert_eq!(d.entity_kinds[0].plural, "Tickets");
    }

    /// A `cursor: None` Jira sync walks the whole JQL result set, so the
    /// engine's post-full-sync sweep is safe: anything whose `synced_at`
    /// predates the run really did stop coming back. `false` here would leave
    /// hard-deleted issues in the mirror forever; `true` on a windowed adapter
    /// (TeamCity) would tombstone its history every run, which is why this is
    /// a per-adapter claim and not a default.
    #[test]
    fn a_full_sync_is_the_whole_corpus() {
        let d = descriptor_template();
        assert!(
            d.entity_kinds.iter().all(|k| k.full_sync_exhaustive),
            "every kind Jira emits is walked in full"
        );
    }

    /// The claim above is one the contract battery *cannot* check -- it would
    /// have to know the remote corpus. What makes it true is a single property
    /// of the query renderer: a `cursor: None` run puts **no time bound** in
    /// its JQL, so the result set is every issue the credential can see.
    ///
    /// Pinned here, beside the claim, rather than in `jql`'s own tests: it is
    /// the descriptor that becomes a lie if the renderer ever gains a bound,
    /// and the engine acts on the descriptor -- it sweeps every row whose
    /// `synced_at` predates a full run, so a windowed full sync would tombstone
    /// live issues. The paging half (that the run walks `total`, not just the
    /// first page) is task 5's and is checked there.
    #[test]
    fn the_full_sync_claim_rests_on_a_query_with_no_time_bound() {
        assert!(
            descriptor_template()
                .entity_kinds
                .iter()
                .all(|k| k.full_sync_exhaustive)
        );
        for config in [
            serde_json::json!({}),
            serde_json::json!({ "projects": ["PAY", "OPS"] }),
            serde_json::json!({ "jql_filter": "labels = sepa" }),
        ] {
            let cfg = crate::JiraConfig::from_json(&config).unwrap();
            let jql = crate::jql::build_jql(&cfg, None, 7_200).to_lowercase();
            for bound in [
                "updated >=",
                "updated>=",
                "created >=",
                "updated <",
                "created <",
            ] {
                assert!(
                    !jql.contains(bound),
                    "a full sync claiming to be exhaustive must not bound time, \
                     but {config} renders {jql:?}"
                );
            }
        }
    }

    /// The Add-source form is generated from `config_schema` (spec §3a), so the
    /// schema and the struct the adapter parses must describe the same keys.
    /// Drift here is a form field that is silently discarded, or a config key
    /// with no way to set it.
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
            serde_json::to_value(crate::JiraConfig::default())
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
        crate::JiraConfig::from_json(&serde_json::Value::Object(object)).unwrap();
    }

    /// The descriptor crosses the IPC bridge as plain data (spec §3a).
    #[test]
    fn the_template_survives_a_json_round_trip() {
        let json = serde_json::to_string(&descriptor_template()).unwrap();
        let back: knobas_source::SourceDescriptor = serde_json::from_str(&json).unwrap();
        assert_eq!(back.adapter_kind, crate::ADAPTER_KIND);
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
