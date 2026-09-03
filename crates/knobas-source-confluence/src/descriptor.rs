//! What knobas knows about this adapter before an instance of it exists.
//!
//! `list_adapters()` serves this template to the Add-source form, which is
//! *generated* from `config_schema` + `auth_methods` (spec §3a). Nothing
//! downstream carries a hardcoded Confluence table, which is why the kind
//! metadata (label, plural, monogram) lives here and not in the launcher.
//!
//! # The declared payload paths (#277)
//!
//! ADR-0007's destination: the adapter says **where** its records keep what
//! knobas reads, instead of every reader outside the adapter holding a little
//! table of per-source spellings. Confluence's answer is short, and most of it
//! is "nowhere" -- see [`payload_paths`].

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
        // `Write` since #286, and it is the only capability. `Capability::Search`
        // stays reserved for a server-side `Source::search` the SPI does not
        // have -- knobas' launcher answers from the local index either way, and
        // this adapter would be the first with a real server-side search to
        // declare if the SPI ever grows one. `Webhooks` likewise: Confluence DC
        // has them and knobas has no receiver.
        capabilities: vec![knobas_source::Capability::Write],
        adapter_version: crate::ADAPTER_VERSION.to_owned(),
        // Bearer PAT (DC >= 7.9) or Basic user+password (spec §3).
        auth_methods: vec![AuthMethod::Pat, AuthMethod::UserPassword],
        // Spec #272's Confluence set, ratified under ADR-0006 in one §10.8
        // entry (#286). `comment` is the SPI's existing op re-used with the
        // **page** as its container -- a reply on a page is the same act as a
        // reply on a ticket, and inventing a `CommentOnPage` would have made
        // the enum adapter-aware, which is what ADR-0006 rejects.
        //
        // The order is the order the action bar renders in, and it is
        // deliberate: the two a reader reaches for from a page detail come
        // first, and the one that makes a new page last.
        write_ops: vec![
            "comment".to_owned(),
            "update_page".to_owned(),
            "create_page".to_owned(),
        ],
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
        payload_paths: payload_paths(),
    }
}

/// Where a Confluence page keeps what knobas reads (#277, ADR-0007).
///
/// **A space is Confluence's project** (ADR-0010, spelled *space* in the UI),
/// so `space.key` and `space.name` are the project key and name -- and they
/// are the two paths the census resolves to list a source's projects. The
/// `space` object is expanded on every record this adapter emits
/// ([`crate::api::EXPAND`]), which is what makes the declaration true rather
/// than hopeful; `the_space_and_the_ancestors_are_where_their_readers_look` in
/// the live suite is where the real server confirms it, and contract battery
/// clause 6 refuses a path no item of the kind resolves.
///
/// **Everything else is a miss, and deliberately so.** A page has no status a
/// reader could act on, no priority, no assignee, no requested reviewers and
/// no merged flag: Confluence has none of those concepts, so this declares no
/// path for them and every reader misses. Declaring a path for a fact the
/// source does not have is the guess ADR-0007 forbids -- and clause 4 of the
/// battery is what catches a reader that grew a knobas-side fallback for one.
///
/// **`blocked_statuses` is empty for the same reason**, not by omission: a
/// page is never "stuck". A source with no status cannot have a blocked-like
/// one, so the set is empty rather than borrowing Jira's three names.
///
/// One entry, because this adapter emits one kind. A `comment` is not a kind
/// here -- it lives in its page's payload, the way a Jira comment lives in its
/// issue's -- so it needs no declaration.
fn payload_paths() -> Vec<knobas_source::KindPaths> {
    use knobas_source::{KindPaths, PayloadPath};
    vec![KindPaths {
        kind: crate::KIND_PAGE.to_owned(),
        project_key: vec![PayloadPath::of(["space", "key"])],
        project_name: vec![PayloadPath::of(["space", "name"])],
        ..KindPaths::default()
    }]
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
    /// here is exactly what the user is offered. A write op declared here that
    /// `write` refuses is an action that 404s, and an op `write` performs but
    /// that is missing here is an action the UI never draws; the battery holds
    /// both directions.
    ///
    /// The three names are checked **literally**, not through
    /// `WriteOp::identifier`: this declaration is the wire contract the
    /// frontend's `WriteOpPayload` and the queue's `PROJECTED_OPS` are keyed
    /// on, and a test that computed them from the enum would agree with a
    /// rename that broke every one of them (#286).
    #[test]
    fn the_template_declares_one_kind_and_the_three_page_writes() {
        let d = descriptor_template();
        assert_eq!(d.id, crate::ADAPTER_KIND);
        assert_eq!(d.adapter_kind, crate::ADAPTER_KIND);
        assert_eq!(d.capabilities, vec![Capability::Write]);
        assert_eq!(d.write_ops, vec!["comment", "update_page", "create_page"]);
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

    /// **ADR-0010 through #277's declaration**: a space is Confluence's
    /// project, and `space.key` / `space.name` are where the census resolves
    /// it. Both are expanded on every record this adapter emits, which is what
    /// makes the declaration true rather than hopeful -- contract battery
    /// clause 6 refuses a path no item of the kind resolves, and the live
    /// suite is where the real server answers.
    #[test]
    fn a_space_is_this_sources_project_and_the_declaration_says_where() {
        let d = descriptor_template();
        let paths = &d.payload_paths;
        assert_eq!(paths.len(), 1, "this adapter emits one kind: {paths:?}");
        assert_eq!(paths[0].kind, crate::KIND_PAGE);
        assert_eq!(
            paths[0].project_key,
            vec![knobas_source::PayloadPath::of(["space", "key"])]
        );
        assert_eq!(
            paths[0].project_name,
            vec![knobas_source::PayloadPath::of(["space", "name"])]
        );
        // Clause 1's own precondition, checked here too because a declaration
        // for a kind the adapter does not emit is one nothing ever reads.
        let kinds: Vec<&str> = d.entity_kinds.iter().map(|k| k.id.as_str()).collect();
        assert!(kinds.contains(&paths[0].kind.as_str()), "{kinds:?}");
    }

    /// **Everything Confluence does not have is a miss, and stays one.**
    ///
    /// A page has no status, no priority, no assignee, no requested reviewers
    /// and no merged flag, so this adapter declares no path for any of them --
    /// and `blocked_statuses` is empty because a source with no status cannot
    /// have a blocked-like one. Declaring a path for a fact the source does
    /// not have is the guess ADR-0007 forbids, and it is the kind of thing a
    /// later edit adds "for symmetry" with the Jira declaration next door.
    #[test]
    fn the_facts_confluence_does_not_have_are_declared_nowhere() {
        let paths = &descriptor_template().payload_paths[0];
        assert!(paths.status_name.is_empty(), "{:?}", paths.status_name);
        assert!(paths.priority.is_empty(), "{:?}", paths.priority);
        assert!(paths.assignee.is_empty(), "{:?}", paths.assignee);
        assert!(paths.reviewers.is_empty(), "{:?}", paths.reviewers);
        assert!(paths.merged.is_empty(), "{:?}", paths.merged);
        assert!(
            paths.blocked_statuses.is_empty(),
            "a page is never stuck, so it borrows no other product's names: {:?}",
            paths.blocked_statuses
        );
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
        // The action bar is drawn from this on the other side of the bridge, so
        // an op lost on the hop is an action the user is never offered.
        assert_eq!(
            back.write_ops,
            vec!["comment", "update_page", "create_page"]
        );
        assert_eq!(back.capabilities, vec![knobas_source::Capability::Write]);
        assert!(back.entity_kinds[0].full_sync_exhaustive);
        // The declaration travels with it: a reader resolves it on the other
        // side of the bridge, so a hop that dropped it would turn every
        // path-driven read into a silent miss.
        assert_eq!(
            back.payload_paths[0].project_key,
            vec![knobas_source::PayloadPath::of(["space", "key"])]
        );
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
