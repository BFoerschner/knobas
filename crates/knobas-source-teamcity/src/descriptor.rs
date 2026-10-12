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
        // One credential. The second one of envelope version 2 is Uptime
        // Kuma's alone (issue #452), and a form that offered account
        // fields here would be asking for something nothing reads.
        accepts_account: false,
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
        payload_paths: payload_paths(),
    }
}

/// Where a TeamCity record keeps what knobas reads (#277, ADR-0007).
///
/// **The project, twice, because a build and a configuration name it in two
/// places.** A build's record carries the `buildType` it ran, and names the
/// project on that; a build *configuration*'s record **is** that `buildType`
/// object, so it names the project at the top level, in the same two words one
/// level up. That difference used to be a `case when i.kind = 'build_config'`
/// guard inside knobas' own SQL (#232, ruled at triage because a top-level
/// `projectId` is a less distinctive path than a container-scoped one and a
/// future adapter's incidental one must not silently open a room). It is two
/// declarations now, and the guard is the declaration being **per kind**: no
/// other adapter's records can reach these paths, because no other adapter
/// declares them.
///
/// **No status.** A build has an *outcome* -- `SUCCESS`, `FAILURE` -- and a
/// `state`, and neither is the status a ticket stands in: the mini board's
/// columns are a workflow's statuses and a build has no workflow. The inbox's
/// failed-build rule reads the outcome directly and is not one of the reads
/// this declaration serves. Declaring the outcome as a `status_name` would put
/// `FAILURE` on the mini board as a column, which is not a thing anybody can
/// move a build to.
///
/// **A build's parent is its configuration** (ADR-0018, #597), read off the
/// same nested `buildType` object the project is: it *is* the configuration's
/// own record shape, so the names declared for it resolve on a
/// `build_config` record's root too, which is how `under:` returns the
/// configuration itself. It answers to its `name` first -- the name a path
/// line shows -- and to its `id`, which is what build parameters and logs
/// name. Not the composed `<project> / <name>` title the launcher shows: no
/// record carries that string, and a build's title keeps it anyway. A
/// configuration has no parent; its project is a project, not a parent.
fn payload_paths() -> Vec<knobas_source::KindPaths> {
    use knobas_source::{KindPaths, ParentPath, PayloadPath};
    vec![
        KindPaths {
            kind: KIND_BUILD.to_owned(),
            state: build_state(),
            project_key: vec![PayloadPath::of(["buildType", "projectId"])],
            project_name: vec![PayloadPath::of(["buildType", "projectName"])],
            parent: vec![ParentPath {
                kind: KIND_BUILD_CONFIG.to_owned(),
                at: PayloadPath::of(["buildType"]),
                names: vec![PayloadPath::of(["name"]), PayloadPath::of(["id"])],
            }],
            ..KindPaths::default()
        },
        KindPaths {
            kind: KIND_BUILD_CONFIG.to_owned(),
            project_key: vec![PayloadPath::of(["projectId"])],
            project_name: vec![PayloadPath::of(["projectName"])],
            ..KindPaths::default()
        },
    ]
}

/// A build's state chip (#666): `running` or `queued` while it is in
/// flight, else its outcome in TeamCity's own spelling -- `SUCCESS`,
/// `FAILURE`, or `UNKNOWN` for a cancelled one. A finished build's `state` is
/// `finished`, which the first candidate does not map, so it falls through to
/// `status`. Not a `status_name` (see above): a chip is drawn, not a column.
fn build_state() -> Vec<knobas_source::StatePath> {
    use knobas_source::{PayloadPath, StatePath, StateValue, Tone};
    let value = |value: &str, tone: Tone| StateValue {
        value: serde_json::json!(value),
        tone,
        text: None,
    };
    vec![
        StatePath {
            at: PayloadPath::of(["state"]),
            values: vec![value("running", Tone::Info), value("queued", Tone::Info)],
        },
        StatePath {
            at: PayloadPath::of(["status"]),
            values: vec![
                value("SUCCESS", Tone::Ok),
                value("FAILURE", Tone::Err),
                value("UNKNOWN", Tone::Muted),
            ],
        },
    ]
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

    /// A build sits in the configuration it ran (ADR-0018, #597), and reads
    /// it off the `buildType` object TeamCity nests in every build record. The
    /// configuration answers to its name first -- the title TeamCity gives it,
    /// and the name a path line shows -- and to its id, which is what build
    /// parameters and logs name. A configuration has no parent.
    #[test]
    fn a_build_declares_its_configuration_as_parent_by_name_and_id() {
        use knobas_source::{ParentPath, PayloadPath};
        let declared = |kind: &str| {
            descriptor_template()
                .payload_paths
                .into_iter()
                .find(|paths| paths.kind == kind)
                .map(|paths| paths.parent)
                .unwrap_or_default()
        };
        assert_eq!(
            declared("build"),
            vec![ParentPath {
                kind: "build_config".to_owned(),
                at: PayloadPath::of(["buildType"]),
                names: vec![PayloadPath::of(["name"]), PayloadPath::of(["id"])],
            }]
        );
        assert!(declared("build_config").is_empty());
    }

    /// A build's chip (#666): in flight it says so, finished it says its
    /// outcome in TeamCity's spelling. A configuration draws none.
    #[test]
    fn a_build_draws_running_or_its_outcome() {
        use knobas_source::{StateChip, Tone};
        let state = |kind: &str| {
            descriptor_template()
                .payload_paths
                .into_iter()
                .find(|p| p.kind == kind)
                .map(|p| p.state)
                .unwrap_or_default()
        };
        let build = state("build");
        let chip =
            |payload: serde_json::Value| knobas_core::payload::resolve_state(&payload, &build);
        let drawn = |text: &str, tone: Tone| {
            Some(StateChip {
                text: text.to_owned(),
                tone,
            })
        };
        assert_eq!(
            chip(serde_json::json!({ "state": "running", "status": "SUCCESS" })),
            drawn("running", Tone::Info)
        );
        assert_eq!(
            chip(serde_json::json!({ "state": "queued" })),
            drawn("queued", Tone::Info)
        );
        assert_eq!(
            chip(serde_json::json!({ "state": "finished", "status": "SUCCESS" })),
            drawn("SUCCESS", Tone::Ok)
        );
        assert_eq!(
            chip(serde_json::json!({ "state": "finished", "status": "FAILURE" })),
            drawn("FAILURE", Tone::Err)
        );
        assert_eq!(
            chip(serde_json::json!({ "state": "finished", "status": "UNKNOWN" })),
            drawn("UNKNOWN", Tone::Muted)
        );
        assert!(state("build_config").is_empty());
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
