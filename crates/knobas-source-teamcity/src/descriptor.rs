//! The self-description stream F's `list_adapters` serves and stream D's
//! Add-source form is generated from (spec §3a, interfaces §2.2).

use knobas_source::{AuthMethod, Capability, KindInfo, SourceDescriptor};

use crate::config::config_schema;
use crate::{
    ADAPTER_KIND, ADAPTER_VERSION, KIND_BUILD, KIND_BUILD_CONFIG, WRITE_OP_RERUN_BUILD,
    WRITE_OP_RUN_BUILD, WRITE_OP_SET_PARAMETER, WRITE_OP_TRIGGER_BUILD,
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
        // M2 (issue #43): this adapter writes. And `Refresh` since #2099: a
        // build is read back by id after a write lands (ADR-0031); a build
        // configuration is not offered. `Capability::Search` stays reserved
        // for a future `Source::search`.
        capabilities: vec![Capability::Write, Capability::Refresh],
        adapter_version: ADAPTER_VERSION.to_owned(),
        // Bearer access token (TeamCity 2019.1+) or Basic user+password.
        auth_methods: vec![AuthMethod::Pat, AuthMethod::UserPassword],
        // One credential. The second one of envelope version 2 is Uptime
        // Kuma's alone (issue #452), and a form that offered account
        // fields here would be asking for something nothing reads.
        accepts_account: false,
        // A build's state is watched while it runs, so TeamCity may be polled
        // every 20 s (#1187): the incremental read asks only for builds
        // changed since the cursor, which is cheap on the server.
        min_sync_interval_secs: 20,
        // CI (#1193, ADR-0029): a build configuration is the pipeline, a
        // build the run, and every Builds-frame read is answered.
        roles: vec![knobas_source::role::RoleClaim::new(
            knobas_source::role::CI,
            [
                "run_log",
                "pipeline_settings",
                "older_runs",
                "run_tests",
                "test_history",
                "run_artifacts",
                "artifact_preview",
                "download",
                "run_chain",
            ],
        )],
        // M2's ratified TeamCity set (issue #43, ADR-0006) and the parameter
        // set and custom run the source views added (issues #668, #669,
        // §10.8). The battery holds this and `Capability::Write` to each
        // other in both directions.
        source_actions: Vec::new(),
        clone_from: Vec::new(),
        url_forms: Vec::new(),
        write_ops: vec![
            WRITE_OP_TRIGGER_BUILD.to_owned(),
            WRITE_OP_RERUN_BUILD.to_owned(),
            WRITE_OP_SET_PARAMETER.to_owned(),
            WRITE_OP_RUN_BUILD.to_owned(),
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
///
/// **The Builds frame** (ADR-0022, #848): a configuration is a pipeline and
/// a build is a run. Both name the configuration's id -- the configuration
/// as its own `id`, the build as `buildTypeId` -- which is what joins a run to
/// its pipeline. A pipeline is called its `name` and grouped by its
/// `projectName`; a run carries its `number`, `branchName`, `startDate` and
/// `finishDate`, all of them in the record `BUILD_FIELDS` already fetches.
/// TeamCity writes its dates in ISO 8601's basic form (`20260822T102000+0000`).
///
/// **How a run was started** (#961) is `triggered.type`: `vcs` a change,
/// `schedule` a schedule, `user` a person, whose account is
/// `triggered.user.username`. Any other type (`buildType`, a finished
/// dependency) is a miss, which the shell draws as no trigger.
///
/// **Where a run ran and what it is doing now** (#1004) are `agent.name`,
/// which a build has from the moment an agent takes it, and
/// `running-info.currentStageText`, which TeamCity serves only while the
/// build runs.
///
/// **A run's test summary** (#1448) is `testOccurrences.failed`, `.passed`
/// and `.ignored`. Asked for by name, TeamCity serves all three on a build
/// that ran tests, zeros included, and no `testOccurrences` on one that ran
/// none.
///
/// **A pipeline's chain of groups** (#849) is its `projectPath`, which the
/// sync writes beside the record from `/app/rest/projects`
/// (`crate::projects`): every project from below the root down to the
/// configuration's own, by name. The same chain, by `id` and `name`, is the
/// configuration's `project_path` (#1019), so a project pattern or
/// `project:` naming a parent project takes in its subprojects' configurations,
/// as TeamCity's own project page and a configured `project_ids` entry do.
///
/// **A build ran on its revisions** (#710): `revisions.revision[].version`,
/// one full sha per VCS root, which is what the detail of a pull request, a
/// branch or a commit finds the builds of its commit by.
///
/// **No blocked-like and no done-like set** (#1501): a build or a
/// configuration has an outcome and a state chip, not a workflow status, so
/// no job context anchored on one is archived by its status.
fn payload_paths() -> Vec<knobas_source::KindPaths> {
    use knobas_core::payload::Frame;
    use knobas_source::{
        KindPaths, ListPath, ParentPath, PayloadPath, ProjectPath, Trigger, TriggerPath,
        TriggerValue,
    };
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
            revision: vec![ListPath {
                at: PayloadPath::of(["revisions", "revision"]),
                entry: PayloadPath::of(["version"]),
            }],
            frame: Some(Frame::Run),
            pipeline: vec![PayloadPath::of(["buildTypeId"])],
            number: vec![PayloadPath::of(["number"])],
            branch: vec![PayloadPath::of(["branchName"])],
            started: vec![PayloadPath::of(["startDate"])],
            finished: vec![PayloadPath::of(["finishDate"])],
            trigger: vec![TriggerPath {
                at: PayloadPath::of(["triggered", "type"]),
                values: vec![
                    TriggerValue::of("vcs", Trigger::Change),
                    TriggerValue::of("schedule", Trigger::Schedule),
                    TriggerValue::of("user", Trigger::Person),
                    TriggerValue::of("snapshotDependency", Trigger::Chain),
                ],
            }],
            started_by: vec![PayloadPath::of(["triggered", "user", "username"])],
            agent: vec![PayloadPath::of(["agent", "name"])],
            current_step: vec![PayloadPath::of(["running-info", "currentStageText"])],
            tests_failed: vec![PayloadPath::of(["testOccurrences", "failed"])],
            tests_passed: vec![PayloadPath::of(["testOccurrences", "passed"])],
            tests_ignored: vec![PayloadPath::of(["testOccurrences", "ignored"])],
            ..KindPaths::default()
        },
        KindPaths {
            kind: KIND_BUILD_CONFIG.to_owned(),
            project_key: vec![PayloadPath::of(["projectId"])],
            project_name: vec![PayloadPath::of(["projectName"])],
            project_path: vec![ProjectPath {
                at: PayloadPath::of(["projectPath"]),
                key: vec![PayloadPath::of(["id"])],
                name: vec![PayloadPath::of(["name"])],
            }],
            frame: Some(Frame::Pipeline),
            name: vec![PayloadPath::of(["name"])],
            group: vec![PayloadPath::of(["projectName"])],
            group_path: vec![ListPath {
                at: PayloadPath::of(["projectPath"]),
                entry: PayloadPath::of(["name"]),
            }],
            pipeline: vec![PayloadPath::of(["id"])],
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
    let value = |value: &str, tone: Tone| StateValue::of(value, tone);
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
    fn the_template_declares_two_kinds_and_its_write_ops() {
        let d = descriptor_template();
        assert_eq!(d.id, "teamcity");
        assert_eq!(d.adapter_kind, "teamcity");
        assert_eq!(d.name, "TeamCity");
        // The sources view renders its action bar from `write_ops` alone, so
        // this list is exactly what the user is offered (issue #43).
        assert_eq!(d.capabilities, vec![Capability::Write, Capability::Refresh]);
        assert_eq!(
            d.write_ops,
            vec!["trigger_build", "rerun_build", "set_parameter", "run_build"]
        );
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
            serde_json::json!(["trigger_build", "rerun_build", "set_parameter", "run_build"])
        );
    }

    /// The Builds frame (#848): a configuration is a pipeline named by its
    /// `name` in its project, a build is a run of the configuration whose id
    /// it names, with its number, branch and both dates. Read off the records
    /// the shapes `BUILD_TYPE_FIELDS` and `BUILD_FIELDS` ask for.
    #[test]
    fn a_configuration_is_a_pipeline_and_a_build_is_a_run_of_it() {
        use knobas_core::payload::{Frame, parse_instant, resolve_string};
        let declared = descriptor_template().payload_paths;
        let of = |kind: &str| declared.iter().find(|p| p.kind == kind).cloned().unwrap();
        let (run, pipeline) = (of(KIND_BUILD), of(KIND_BUILD_CONFIG));
        assert_eq!(run.frame, Some(Frame::Run));
        assert_eq!(pipeline.frame, Some(Frame::Pipeline));
        let config = serde_json::json!({
            "id": "Payout_IntegrationTests", "name": "IntegrationTests",
            "projectId": "Payout", "projectName": "Payout"
        });
        let build = serde_json::json!({
            "id": 7, "number": "1187", "buildTypeId": "Payout_IntegrationTests",
            "state": "finished", "status": "FAILURE",
            "branchName": "feature/PAY-231-sepa-retry",
            "startDate": "20260822T100500+0000", "finishDate": "20260822T100917+0000"
        });
        assert_eq!(
            resolve_string(&config, &pipeline.name).as_deref(),
            Some("IntegrationTests")
        );
        assert_eq!(
            resolve_string(&config, &pipeline.group).as_deref(),
            Some("Payout")
        );
        // The chain of projects the sync writes beside the record (#849),
        // outermost first, by name; a record without one has no chain.
        let nested = serde_json::json!({
            "id": "Payout_Deploy_Prod", "name": "Deploy to prod",
            "projectPath": [
                { "id": "Payments", "name": "Payments" },
                { "id": "Payments_Payouts", "name": "Payouts" },
                { "id": "Payments_Payouts_Deploy", "name": "Deploy" }
            ]
        });
        assert_eq!(
            knobas_core::payload::resolve_list(&nested, &pipeline.group_path),
            ["Payments", "Payouts", "Deploy"]
        );
        assert!(knobas_core::payload::resolve_list(&config, &pipeline.group_path).is_empty());
        // The same chain is the configuration's projects (#1019), by id and
        // name, so a parent project takes in its subprojects.
        let chain: Vec<(String, Option<String>)> =
            knobas_core::payload::resolve_project_path(&nested, &pipeline.project_path)
                .into_iter()
                .map(|p| (p.key, p.name))
                .collect();
        let named = |key: &str, name: &str| (key.to_owned(), Some(name.to_owned()));
        assert_eq!(
            chain,
            [
                named("Payments", "Payments"),
                named("Payments_Payouts", "Payouts"),
                named("Payments_Payouts_Deploy", "Deploy")
            ]
        );
        assert!(
            knobas_core::payload::resolve_project_path(&config, &pipeline.project_path).is_empty()
        );
        assert_eq!(
            resolve_string(&config, &pipeline.pipeline),
            resolve_string(&build, &run.pipeline),
            "a run names its pipeline as the pipeline names itself"
        );
        assert!(resolve_string(&build, &run.pipeline).is_some());
        assert_eq!(resolve_string(&build, &run.number).as_deref(), Some("1187"));
        assert_eq!(
            resolve_string(&build, &run.branch).as_deref(),
            Some("feature/PAY-231-sepa-retry")
        );
        let started = parse_instant(&resolve_string(&build, &run.started).unwrap()).unwrap();
        let finished = parse_instant(&resolve_string(&build, &run.finished).unwrap()).unwrap();
        assert_eq!((finished - started).num_seconds(), 257);
        // A queued build has no number and neither date yet.
        let queued = serde_json::json!({ "id": 8, "buildTypeId": "Payout_IntegrationTests", "state": "queued" });
        assert_eq!(resolve_string(&queued, &run.number), None);
        assert_eq!(resolve_string(&queued, &run.started), None);
    }

    /// How a build was started (#961), off `triggered` as `BUILD_FIELDS`
    /// asks for it: a VCS trigger is a change with nobody, a person names
    /// their account, a schedule is a schedule, a step a chain queued is the
    /// chain (#1146), and a finished dependency is a miss.
    #[test]
    fn a_build_says_how_it_was_started_and_by_whom() {
        use knobas_core::payload::{Trigger, resolve_string, resolve_trigger};
        let declared = descriptor_template().payload_paths;
        let run = declared.iter().find(|p| p.kind == KIND_BUILD).unwrap();
        let build =
            |triggered: serde_json::Value| serde_json::json!({ "id": 7, "triggered": triggered });
        let vcs = build(serde_json::json!({ "type": "vcs" }));
        assert_eq!(resolve_trigger(&vcs, &run.trigger), Some(Trigger::Change));
        assert_eq!(resolve_string(&vcs, &run.started_by), None);
        let user = build(serde_json::json!({ "type": "user", "user": { "username": "mara" } }));
        assert_eq!(resolve_trigger(&user, &run.trigger), Some(Trigger::Person));
        assert_eq!(
            resolve_string(&user, &run.started_by).as_deref(),
            Some("mara")
        );
        let nightly = build(serde_json::json!({ "type": "schedule" }));
        assert_eq!(
            resolve_trigger(&nightly, &run.trigger),
            Some(Trigger::Schedule)
        );
        let chained = build(serde_json::json!({ "type": "snapshotDependency" }));
        assert_eq!(
            resolve_trigger(&chained, &run.trigger),
            Some(Trigger::Chain)
        );
        let finished_dependency = build(serde_json::json!({ "type": "buildType" }));
        assert_eq!(resolve_trigger(&finished_dependency, &run.trigger), None);
        let config = declared
            .iter()
            .find(|p| p.kind == KIND_BUILD_CONFIG)
            .unwrap();
        assert!(config.trigger.is_empty() && config.started_by.is_empty());
    }

    /// The agent a build ran on and what a running one is doing now (#1004),
    /// off `agent(name)` and `running-info(currentStageText)` as
    /// `BUILD_FIELDS` asks for them; a queued build has neither, a finished
    /// one no step, and a configuration declares neither.
    #[test]
    fn a_build_names_its_agent_and_what_it_is_doing_now() {
        use knobas_core::payload::resolve_string;
        let declared = descriptor_template().payload_paths;
        let run = declared.iter().find(|p| p.kind == KIND_BUILD).unwrap();
        let running = serde_json::json!({
            "id": 7, "state": "running", "agent": { "name": "agent-linux-03" },
            "running-info": { "currentStageText": "step 3/5 `cargo test`" }
        });
        assert_eq!(
            resolve_string(&running, &run.agent).as_deref(),
            Some("agent-linux-03")
        );
        assert_eq!(
            resolve_string(&running, &run.current_step).as_deref(),
            Some("step 3/5 `cargo test`")
        );
        let finished =
            serde_json::json!({ "id": 8, "state": "finished", "agent": { "name": "a1" } });
        assert_eq!(resolve_string(&finished, &run.agent).as_deref(), Some("a1"));
        assert_eq!(resolve_string(&finished, &run.current_step), None);
        let queued = serde_json::json!({ "id": 9, "state": "queued" });
        assert_eq!(resolve_string(&queued, &run.agent), None);
        let config = declared
            .iter()
            .find(|p| p.kind == KIND_BUILD_CONFIG)
            .unwrap();
        assert!(config.agent.is_empty() && config.current_step.is_empty());
    }

    /// A build's test summary (#1448, R-9.6), off `testOccurrences(failed,
    /// passed,ignored)` as `BUILD_FIELDS` asks for it. TeamCity serves all
    /// three, zeros included, on a build that ran tests, and no
    /// `testOccurrences` at all on one that ran none (measured on
    /// teamcity.jetbrains.com, 2026.2.1, 2026-10-21); a configuration declares
    /// none.
    #[test]
    fn a_build_carries_its_test_counts_and_one_without_tests_none() {
        use knobas_core::payload::resolve_number;
        let declared = descriptor_template().payload_paths;
        let run = declared.iter().find(|p| p.kind == KIND_BUILD).unwrap();
        let tested = serde_json::json!({
            "id": 7, "state": "finished",
            "testOccurrences": { "failed": 1, "passed": 41, "ignored": 2 }
        });
        assert_eq!(resolve_number(&tested, &run.tests_failed), Some(1.0));
        assert_eq!(resolve_number(&tested, &run.tests_passed), Some(41.0));
        assert_eq!(resolve_number(&tested, &run.tests_ignored), Some(2.0));
        let untested = serde_json::json!({ "id": 8, "state": "finished" });
        assert_eq!(resolve_number(&untested, &run.tests_failed), None);
        assert_eq!(resolve_number(&untested, &run.tests_passed), None);
        assert_eq!(resolve_number(&untested, &run.tests_ignored), None);
        let config = declared
            .iter()
            .find(|p| p.kind == KIND_BUILD_CONFIG)
            .unwrap();
        assert!(
            config.tests_failed.is_empty()
                && config.tests_passed.is_empty()
                && config.tests_ignored.is_empty()
        );
    }

    /// A build ran on every revision it lists (#710): one sha per VCS root,
    /// read as `BUILD_FIELDS` asks for them; a build with no revision ran on
    /// none, and a configuration declares none.
    #[test]
    fn a_build_ran_on_each_of_its_revisions() {
        let declared = descriptor_template().payload_paths;
        let of = |kind: &str| {
            declared
                .iter()
                .find(|p| p.kind == kind)
                .map(|p| p.revision.clone())
                .unwrap_or_default()
        };
        let build = serde_json::json!({
            "id": 7,
            "revisions": { "revision": [
                { "version": "c90d11a3f5e2b7c4d9018e6a2b3c4d5e6f708192" },
                { "version": "1111111111111111111111111111111111111111" }
            ] }
        });
        assert_eq!(
            knobas_core::payload::resolve_list(&build, &of(KIND_BUILD)),
            [
                "c90d11a3f5e2b7c4d9018e6a2b3c4d5e6f708192",
                "1111111111111111111111111111111111111111"
            ]
        );
        let bare = serde_json::json!({ "id": 8, "revisions": { "count": 0 } });
        assert!(knobas_core::payload::resolve_list(&bare, &of(KIND_BUILD)).is_empty());
        assert!(of(KIND_BUILD_CONFIG).is_empty());
    }

    /// The role this adapter fills (#1193, ADR-0029), held to its minimum
    /// on the descriptor alone: the install-time check, which the battery
    /// repeats as its clause 10.
    #[test]
    fn the_template_claims_the_ci_role_and_fills_it() {
        let d = descriptor_template();
        let roles: Vec<&str> = d.roles.iter().map(|c| c.role.as_str()).collect();
        assert_eq!(roles, [knobas_source::role::CI]);
        assert_eq!(knobas_source::role::check(&d), Ok(()));
    }
}
