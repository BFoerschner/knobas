//! What knobas knows about this adapter before an instance of it exists.
//!
//! `list_adapters()` serves this template to the Add-source form, which is
//! *generated* from `config_schema` + `auth_methods` (spec §3a). Nothing
//! downstream carries a hardcoded Kuma table: the `monitor` kind's label,
//! plural and monogram live here, and the launcher, the room tiles and the
//! chips read them from here.
//!
//! **No projects, and therefore no project rooms.** Uptime Kuma has no
//! container above a monitor that knobas mirrors -- monitor *groups* are out of
//! scope (spec #427, *Not in scope*: "monitor groups as projects") -- so
//! [`payload_paths`] declares no `project_key`, the census finds none, and the
//! switcher offers this source its own room and nothing under it. That is spec
//! #427's *The Kuma room and tiles* in full: "No switcher exception: the Kuma
//! source has its room; it declares no projects."

use knobas_source::{AuthMethod, KindInfo, SourceDescriptor};

/// The static, instance-free descriptor: `id == adapter_kind` (contract §4.2).
/// A configured instance's descriptor is this with `id` and `name` replaced by
/// the instance's own.
#[must_use]
pub fn descriptor_template() -> SourceDescriptor {
    SourceDescriptor {
        id: crate::ADAPTER_KIND.to_owned(),
        adapter_kind: crate::ADAPTER_KIND.to_owned(),
        name: "Uptime Kuma".to_owned(),
        // **None**, which is what a read-only adapter declares: `Search` is
        // reserved for a server-side `Source::search` the SPI does not have,
        // `Webhooks` for a receiver knobas does not have, and `Write` must be
        // accompanied by the ops it offers. Pause, resume and create are
        // socket.io and are the write half's ticket; the day they land, this
        // grows `Write` and a `write_ops` list together.
        capabilities: Vec::new(),
        adapter_version: crate::ADAPTER_VERSION.to_owned(),
        // One method, because `/metrics` accepts one thing: an API key, sent as
        // HTTP Basic with an empty username. There is no account password to
        // offer (spec #427, story 50).
        auth_methods: vec![AuthMethod::ApiToken],
        write_ops: Vec::new(),
        entity_kinds: vec![KindInfo {
            id: crate::KIND_MONITOR.to_owned(),
            label: "Monitor".to_owned(),
            plural: "Monitors".to_owned(),
            // `MO`. `MN` reads as *minute* on a chip beside a duration, and
            // knobas' own vocabulary spells the word *monitor* (`CONTEXT.md`,
            // **Monitor**).
            monogram: "MO".to_owned(),
            // ADR-0003: a `cursor: None` run emits every monitor `/metrics`
            // publishes, which is every monitor Kuma is running -- there is no
            // paging, no window and no budget, because the document is the
            // whole roster in one response. So the engine's post-full-sync
            // sweep is safe: a row whose `synced_at` predates the run really
            // did stop being published.
            //
            // It is not the *only* thing that retires a monitor, and it is not
            // the one that usually does. A scheduled poll resumes from the
            // stored cursor and so never sweeps (`knobas_sync`, limitation 3),
            // which is why the adapter tombstones a vanished monitor itself --
            // see `crate::cursor`.
            full_sync_exhaustive: true,
        }],
        config_schema: config_schema(),
        payload_paths: payload_paths(),
    }
}

/// Where a monitor keeps what knobas reads (#277, ADR-0007).
///
/// **One declaration: `state`.** A monitor's status in Uptime Kuma's own words
/// -- `up`, `down`, `pending`, `maintenance` -- is a status in exactly the
/// sense the mini board, the standup and the detail view mean, and
/// `crate::map` writes it at the top level of every monitor payload, tombstones
/// included. Declaring it is what lets a later reader ask for a monitor's state
/// without growing a `payload->>'state'` of its own, which is the coupling
/// ADR-0007 exists to prevent.
///
/// **Everything else is a miss, and deliberately.** A monitor has no priority,
/// no assignee, no reviewers and no merged flag -- nothing watches a URL on
/// somebody's behalf -- and no project, because Kuma's only container is a
/// monitor group and groups are out of scope (spec #427). Declaring a path for
/// a fact the source does not have is the guess ADR-0007 forbids.
///
/// **`blocked_statuses` is empty, and that is a decision rather than an
/// omission.** It names the statuses that mean *stuck waiting for someone*, and
/// the standup's blockers list is what reads it. A monitor that is `down` is
/// not blocked; it is broken, and M4.1's alerts are how it reaches a reader
/// (spec #427, *The inbox's sixth category*). Putting `down` here would file
/// every outage in the standup digest as a blocked ticket.
fn payload_paths() -> Vec<knobas_source::KindPaths> {
    use knobas_source::{KindPaths, PayloadPath};
    vec![KindPaths {
        kind: crate::KIND_MONITOR.to_owned(),
        status_name: vec![PayloadPath::of(["state"])],
        ..KindPaths::default()
    }]
}

/// The JSON Schema the Add-source form is generated from.
///
/// **The base URL and the API key are not in it, and must not be.** The base
/// URL is `SourceInstance::base_url`, which every source has and the form asks
/// for on its own; the key is the keychain secret the chosen `AuthMethod`
/// carries, and a schema property for it would put a credential in Postgres
/// (spec §14). What is left is the transport tuning -- which is why a form
/// generated from this asks for a URL, a key, and four numbers with defaults.
fn config_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
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

    /// One kind, no writes, one way in. A `write_ops` entry here is an action
    /// the UI renders and `write` refuses, and a capability without ops is a
    /// source that advertises actions it has none of -- the battery holds both
    /// directions, and this is the declaration it holds.
    #[test]
    fn the_template_declares_one_kind_and_no_writes() {
        let d = descriptor_template();
        assert_eq!(d.id, crate::ADAPTER_KIND);
        assert_eq!(d.adapter_kind, crate::ADAPTER_KIND);
        assert_eq!(d.name, "Uptime Kuma");
        assert!(d.capabilities.is_empty(), "{:?}", d.capabilities);
        assert!(d.write_ops.is_empty(), "{:?}", d.write_ops);
        assert_eq!(d.adapter_version, crate::ADAPTER_VERSION);
        assert_eq!(d.auth_methods, vec![AuthMethod::ApiToken]);
        let kinds: Vec<&str> = d.entity_kinds.iter().map(|k| k.id.as_str()).collect();
        assert_eq!(kinds, vec![crate::KIND_MONITOR]);
        assert_eq!(d.entity_kinds[0].label, "Monitor");
        assert_eq!(d.entity_kinds[0].plural, "Monitors");
        assert_eq!(d.entity_kinds[0].monogram, "MO");
    }

    /// ADR-0003: `/metrics` is the whole roster in one response, so a
    /// cursor-less run really does emit everything and the engine's sweep is
    /// safe. `false` here would leave a Kuma whose cursor was cleared holding
    /// monitors that no longer exist.
    #[test]
    fn a_full_sync_is_every_monitor_kuma_publishes() {
        assert!(
            descriptor_template()
                .entity_kinds
                .iter()
                .all(|k| k.full_sync_exhaustive)
        );
    }

    /// *The Kuma source has its room; it declares no projects* (spec #427).
    /// The switcher has no exception for it: a source with no `project_key`
    /// declaration contributes nothing to the census, and the census is what
    /// the project rooms are drawn from.
    #[test]
    fn a_monitor_belongs_to_no_project_and_the_declaration_says_so() {
        let d = descriptor_template();
        assert_eq!(d.payload_paths.len(), 1, "{:?}", d.payload_paths);
        let paths = &d.payload_paths[0];
        assert_eq!(paths.kind, crate::KIND_MONITOR);
        assert!(paths.project_key.is_empty(), "{:?}", paths.project_key);
        assert!(paths.project_name.is_empty(), "{:?}", paths.project_name);
    }

    /// The one declared fact, and the four a monitor does not have. A path
    /// declared for a fact the source lacks is the guess ADR-0007 forbids, and
    /// it is exactly the kind of thing a later edit adds "for symmetry" with
    /// the Jira declaration next door.
    #[test]
    fn the_state_is_declared_and_everything_else_is_a_miss() {
        let paths = &descriptor_template().payload_paths[0];
        assert_eq!(
            paths.status_name,
            vec![knobas_source::PayloadPath::of(["state"])]
        );
        assert!(paths.priority.is_empty(), "{:?}", paths.priority);
        assert!(paths.assignee.is_empty(), "{:?}", paths.assignee);
        assert!(paths.reviewers.is_empty(), "{:?}", paths.reviewers);
        assert!(paths.merged.is_empty(), "{:?}", paths.merged);
        assert!(
            paths.blocked_statuses.is_empty(),
            "a down monitor is broken, not blocked: {:?}",
            paths.blocked_statuses
        );
    }

    /// The Add-source form is generated from `config_schema` (spec §3a), so the
    /// schema and the struct the adapter parses must describe the same keys.
    /// Drift is a form field that is silently discarded, or a config key with
    /// no way to set it.
    #[test]
    fn the_schema_and_the_config_struct_agree() {
        let schema_keys: std::collections::BTreeSet<String> =
            descriptor_template().config_schema["properties"]
                .as_object()
                .expect("config_schema.properties is an object")
                .keys()
                .cloned()
                .collect();
        let struct_keys: std::collections::BTreeSet<String> =
            serde_json::to_value(crate::KumaConfig::default())
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
        crate::KumaConfig::from_json(&serde_json::Value::Object(object)).unwrap();
    }

    /// A schema property naming the API key would put the credential in
    /// Postgres. Property *names* and *titles*, not the whole serialized
    /// schema: a description may legitimately mention the key.
    #[test]
    fn the_schema_asks_for_no_secret() {
        let d = descriptor_template();
        for (key, prop) in d.config_schema["properties"].as_object().unwrap() {
            let title = prop["title"].as_str().unwrap_or_default().to_lowercase();
            for forbidden in ["password", "token", "secret", "key", "pat"] {
                assert!(!key.to_lowercase().contains(forbidden), "property {key}");
                assert!(
                    !title.contains(forbidden),
                    "property {key} is titled {title:?}"
                );
            }
        }
    }

    /// The descriptor crosses the IPC bridge as plain data (spec §3a): the
    /// launcher chips a monitor from the kind metadata on the other side, and
    /// a reader resolves the declaration there too.
    #[test]
    fn the_template_survives_a_json_round_trip() {
        let json = serde_json::to_string(&descriptor_template()).unwrap();
        let back: SourceDescriptor = serde_json::from_str(&json).unwrap();
        assert_eq!(back.adapter_kind, crate::ADAPTER_KIND);
        assert_eq!(back.entity_kinds[0].monogram, "MO");
        assert!(back.entity_kinds[0].full_sync_exhaustive);
        assert!(back.write_ops.is_empty());
        assert_eq!(
            back.payload_paths[0].status_name,
            vec![knobas_source::PayloadPath::of(["state"])]
        );
    }
}
