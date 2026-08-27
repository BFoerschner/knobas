//! The self-description stream F's `list_adapters` serves and stream D's
//! Add-source form is generated from (spec §3a, interfaces §2.2).

use knobas_source::{AuthMethod, KindInfo, SourceDescriptor};

use crate::config::config_schema;
use crate::{ADAPTER_KIND, ADAPTER_VERSION, KIND_BUILD, KIND_BUILD_CONFIG};

/// One descriptor per compiled-in adapter kind: the Add-source form is
/// generated from `config_schema` + `auth_methods`, and the launcher reads
/// kind display metadata from `entity_kinds`, without instantiating an adapter
/// or touching the keychain.
#[must_use]
pub fn descriptor_template() -> SourceDescriptor {
    SourceDescriptor {
        id: ADAPTER_KIND.to_owned(),
        adapter_kind: ADAPTER_KIND.to_owned(),
        name: "TeamCity".to_owned(),
        // P12: M1 adapters declare no capabilities. `Capability::Search` is
        // reserved for a future `Source::search`, and this adapter is
        // read-only, so `Write` would have no ops to offer.
        capabilities: Vec::new(),
        adapter_version: ADAPTER_VERSION.to_owned(),
        // Bearer access token (TeamCity 2019.1+) or Basic user+password.
        auth_methods: vec![AuthMethod::Pat, AuthMethod::UserPassword],
        write_ops: Vec::new(),
        entity_kinds: vec![
            KindInfo {
                id: KIND_BUILD.to_owned(),
                label: "Build".to_owned(),
                plural: "Builds".to_owned(),
                monogram: "BU".to_owned(),
            },
            KindInfo {
                id: KIND_BUILD_CONFIG.to_owned(),
                label: "Build configuration".to_owned(),
                plural: "Build configurations".to_owned(),
                monogram: "BC".to_owned(),
            },
        ],
        // **The reason this field exists.** A TeamCity full sync fetches the
        // newest `builds_per_config` finished builds per configuration -- a
        // window, not the corpus. The engine's tombstone sweep deletes every
        // row of an exhaustive source whose `synced_at` predates the run, so
        // claiming exhaustiveness here would tombstone the entire mirrored
        // build history on every full sync. Jira, Gitea and the mock are the
        // `true` cases; this adapter is the `false` one.
        full_sync_exhaustive: false,
        config_schema: config_schema(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use knobas_source::Capability;

    /// The descriptor is the only thing the UI reads to render this source
    /// (spec §3a), so it is asserted rather than assumed.
    #[test]
    fn the_template_declares_two_kinds_and_no_writes() {
        let d = descriptor_template();
        assert_eq!(d.id, "teamcity");
        assert_eq!(d.adapter_kind, "teamcity");
        assert_eq!(d.name, "TeamCity");
        // P12: M1 adapters declare no capabilities, and read-only means
        // write_ops stays empty -- the battery enforces both directions.
        assert_eq!(d.capabilities, Vec::<Capability>::new());
        assert!(d.write_ops.is_empty());
        assert_eq!(d.auth_methods, [AuthMethod::Pat, AuthMethod::UserPassword]);
        let kinds: Vec<&str> = d.entity_kinds.iter().map(|k| k.id.as_str()).collect();
        assert_eq!(kinds, ["build", "build_config"]);
        for k in &d.entity_kinds {
            assert!(!k.label.is_empty() && !k.plural.is_empty());
            assert_eq!(k.monogram.chars().count(), 2, "monogram of {:?}", k.id);
        }
        assert_eq!(d.config_schema["type"], "object");
        assert_eq!(d.adapter_version, env!("CARGO_PKG_VERSION"));
    }

    /// The one descriptor field this adapter differs from every other M1
    /// adapter on, and the one with teeth: the sweep reads it, and `true` here
    /// would tombstone every build older than the newest `builds_per_config`
    /// on each full sync.
    #[test]
    fn the_full_sync_is_declared_a_window_not_the_corpus() {
        assert!(
            !descriptor_template().full_sync_exhaustive,
            "a TeamCity full sync fetches the newest builds_per_config builds per configuration, \
             so the engine must not sweep what it did not re-emit"
        );
    }

    /// The descriptor crosses IPC to the UI (§3a), so it has to survive as
    /// plain data -- monograms and the sweep flag included.
    #[test]
    fn the_template_survives_the_ipc_hop() {
        let json = serde_json::to_value(descriptor_template()).expect("plain serde data");
        assert_eq!(json["adapter_kind"], "teamcity");
        assert_eq!(json["full_sync_exhaustive"], false);
        assert_eq!(json["entity_kinds"][0]["monogram"], "BU");
        assert_eq!(json["entity_kinds"][1]["monogram"], "BC");
        assert_eq!(
            json["auth_methods"],
            serde_json::json!(["Pat", "UserPassword"])
        );
        assert_eq!(json["write_ops"], serde_json::json!([]));
    }
}
