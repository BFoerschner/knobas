//! The self-description stream F's `list_adapters` serves and stream D's
//! Add-source form is generated from (spec §3a, interfaces §2.2).

use knobas_source::{AuthMethod, Capability, KindInfo, SourceDescriptor};

use crate::config::config_schema;
use crate::{
    ADAPTER_KIND, ADAPTER_VERSION, KIND_BUILD, KIND_BUILD_CONFIG, WRITE_OP_RERUN_BUILD,
    WRITE_OP_TRIGGER_BUILD,
};

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
        // M2 (issue #43): this adapter writes. `Capability::Search` stays
        // reserved for a future `Source::search`.
        capabilities: vec![Capability::Write],
        adapter_version: ADAPTER_VERSION.to_owned(),
        // Bearer access token (TeamCity 2019.1+) or Basic user+password.
        auth_methods: vec![AuthMethod::Pat, AuthMethod::UserPassword],
        // M2's ratified TeamCity set (issue #43, ADR-0006), and the whole of
        // what the action bar offers. The battery holds this and
        // `Capability::Write` to each other in both directions.
        write_ops: vec![
            WRITE_OP_TRIGGER_BUILD.to_owned(),
            WRITE_OP_RERUN_BUILD.to_owned(),
        ],
        entity_kinds: vec![
            KindInfo {
                id: KIND_BUILD.to_owned(),
                label: "Build".to_owned(),
                plural: "Builds".to_owned(),
                monogram: "BU".to_owned(),
                full_sync_exhaustive: false,
            },
            KindInfo {
                id: KIND_BUILD_CONFIG.to_owned(),
                label: "Build configuration".to_owned(),
                plural: "Build configurations".to_owned(),
                monogram: "BC".to_owned(),
                full_sync_exhaustive: false,
            },
        ],
        // **The reason that field exists.** A TeamCity full sync fetches the
        // newest `builds_per_config` finished builds per configuration -- a
        // window, not the corpus -- and the build configurations are read from
        // the same windowed walk. The engine's tombstone sweep deletes every
        // row of an exhaustive *kind* whose `synced_at` predates the run, so
        // claiming exhaustiveness on either kind above would tombstone the
        // entire mirrored build history on every full sync. Both kinds are
        // therefore `false`; ADR-0003 moved the claim onto the kind and left
        // TeamCity's answer unchanged.
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
    fn the_template_declares_two_kinds_and_two_write_ops() {
        let d = descriptor_template();
        assert_eq!(d.id, "teamcity");
        assert_eq!(d.adapter_kind, "teamcity");
        assert_eq!(d.name, "TeamCity");
        // The sources view renders its action bar from `write_ops` alone, so
        // this list is exactly what the user is offered (issue #43).
        assert_eq!(d.capabilities, vec![Capability::Write]);
        assert_eq!(d.write_ops, vec!["trigger_build", "rerun_build"]);
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
        for kind in descriptor_template().entity_kinds {
            assert!(
                !kind.full_sync_exhaustive,
                "a TeamCity full sync fetches the newest builds_per_config builds per \
                 configuration, so the engine must not sweep {:?} -- what it did not re-emit \
                 has not been deleted",
                kind.id
            );
        }
    }

    /// The descriptor crosses IPC to the UI (§3a), so it has to survive as
    /// plain data -- monograms and the sweep flag included.
    #[test]
    fn the_template_survives_the_ipc_hop() {
        let json = serde_json::to_value(descriptor_template()).expect("plain serde data");
        assert_eq!(json["adapter_kind"], "teamcity");
        assert_eq!(json["entity_kinds"][0]["monogram"], "BU");
        assert_eq!(json["entity_kinds"][0]["full_sync_exhaustive"], false);
        assert_eq!(json["entity_kinds"][1]["monogram"], "BC");
        assert_eq!(json["entity_kinds"][1]["full_sync_exhaustive"], false);
        assert_eq!(
            json["auth_methods"],
            serde_json::json!(["Pat", "UserPassword"])
        );
        assert_eq!(
            json["write_ops"],
            serde_json::json!(["trigger_build", "rerun_build"])
        );
    }
}
