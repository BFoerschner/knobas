//! What the TeamCity user may not write (#2040), against a wiremock TeamCity.
//!
//! The adapter reads `GET /app/rest/users/current/permissions` and the
//! project tree, and never sends a write to find out: a `POST
//! /app/rest/buildQueue` with a build type queues a build. Every test here
//! checks that nothing but `GET`s reached the server.
//!
//! The permission shapes are the ones the seeded 2026.1.3 server answered on
//! 2026-10-24: one assignment per permission and scope, `{"isGlobalScope":
//! true, "permission": {...}}` for a role at global scope, and
//! `{"isGlobalScope": false, "permission": {...}, "project": {"id": ...}}`
//! for one given on a project, which names the project it was given on and
//! not its subprojects. A server with per-project permissions off answers
//! every assignment `isGlobalScope`, since every role is global there.
//! Whether the real server still answers so is `tests/live_teamcity_seeded.rs`'s
//! to say (`a_viewer_may_run_builds_and_may_not_change_parameters`); on
//! disagreement this fake is what is wrong.

use knobas_source::grants::{RefusedWrite, WriteAsk};
use knobas_source::instance::SourceInstance;
use knobas_source::view::{Block, SourceView};
use knobas_source::{AuthMethod, Source};
use serde_json::json;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

const TOKEN: &str = "tidewater-tc-token";
const CONFIG: &str = "teamcity:buildType:Payout_IntegrationTests";
const BUILD: &str = "teamcity:build:11589";

fn source(server: &MockServer) -> Box<dyn Source> {
    let instance = SourceInstance {
        id: "teamcity".to_owned(),
        kind: "teamcity".to_owned(),
        display_name: "Tidewater CI".to_owned(),
        base_url: server.uri(),
        auth: Some(AuthMethod::Pat),
        secret: Some(TOKEN.to_owned()),
        account: None,
        config: json!({}),
    };
    match knobas_source_teamcity::build(instance) {
        Ok(source) => source,
        Err(e) => panic!("the adapter builds against wiremock: {e:?}"),
    }
}

fn ask(entity: &str, op: &str) -> WriteAsk {
    WriteAsk {
        entity: entity.to_owned(),
        op: op.to_owned(),
    }
}

/// One assignment as TeamCity answers it: at global scope, or on `project`.
fn assignment(permission: &str, project: Option<&str>) -> serde_json::Value {
    match project {
        None => json!({
            "permission": { "id": permission, "name": permission, "global": false },
            "isGlobalScope": true
        }),
        Some(project) => json!({
            "isGlobalScope": false,
            "permission": { "id": permission, "name": permission, "global": false },
            "project": { "id": project, "name": project }
        }),
    }
}

/// A TeamCity whose user holds `assignments`, whose configuration lives in
/// `Payout` under `Tidewater`, beside `Ledger`, and whose build 11589 is of
/// that configuration.
async fn teamcity(assignments: Vec<serde_json::Value>) -> MockServer {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/app/rest/users/current/permissions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "count": assignments.len(),
            "permissionAssignment": assignments,
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/app/rest/projects"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "count": 4,
            "project": [
                { "id": "_Root", "name": "<Root project>" },
                { "id": "Tidewater", "name": "Tidewater", "parentProjectId": "_Root" },
                { "id": "Payout", "name": "Payout", "parentProjectId": "Tidewater" },
                { "id": "Ledger", "name": "Ledger", "parentProjectId": "Tidewater" }
            ]
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/app/rest/buildTypes/id:Payout_IntegrationTests"))
        .and(query_param("fields", "projectId"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "projectId": "Payout" })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/app/rest/builds/id:11589"))
        .and(query_param("fields", "buildType(projectId)"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({ "buildType": { "projectId": "Payout" } })),
        )
        .mount(&server)
        .await;
    server
}

async fn only_reads(server: &MockServer) {
    let asked = server.received_requests().await.expect("recorded");
    assert!(
        asked.iter().all(|r| r.method.as_str() == "GET"),
        "nothing but reads reached TeamCity: {asked:#?}"
    );
}

const CANNOT_CHANGE: &str =
    "This TeamCity user cannot change the settings of Payout: it lacks the Edit project permission";
const CANNOT_RUN: &str =
    "This TeamCity user cannot run builds in Payout: it lacks the Run build permission";

/// The measured case: `PROJECT_VIEWER` at global scope holds `run_build` and
/// not `edit_project`. *Run…*, a trigger and a re-run are the user's to
/// make; a parameter is not.
#[tokio::test]
async fn a_viewer_may_run_builds_and_may_not_change_parameters() {
    let server = teamcity(vec![
        assignment("view_project", None),
        assignment("run_build", None),
    ])
    .await;
    let refused = source(&server)
        .refused_writes(&[
            ask(CONFIG, "run_build"),
            ask(CONFIG, "trigger_build"),
            ask(CONFIG, "set_parameter"),
            ask(BUILD, "rerun_build"),
        ])
        .await
        .expect("the permissions read");
    assert_eq!(
        refused,
        [RefusedWrite {
            entity: CONFIG.to_owned(),
            op: "set_parameter".to_owned(),
            reason: CANNOT_CHANGE.to_owned(),
        }]
    );
    only_reads(&server).await;
}

/// A role given on a project holds in its subprojects, as TeamCity applies
/// it: `edit_project` on `Tidewater` holds for `Payout`. One given on a
/// sibling does not: `run_build` on `Ledger` runs nothing in `Payout`.
#[tokio::test]
async fn a_permission_on_an_ancestor_holds_and_one_on_a_sibling_does_not() {
    let server = teamcity(vec![
        assignment("edit_project", Some("Tidewater")),
        assignment("run_build", Some("Ledger")),
    ])
    .await;
    let refused = source(&server)
        .refused_writes(&[
            ask(CONFIG, "set_parameter"),
            ask(CONFIG, "run_build"),
            ask(BUILD, "rerun_build"),
        ])
        .await
        .expect("the permissions read");
    assert_eq!(
        refused,
        [
            RefusedWrite {
                entity: CONFIG.to_owned(),
                op: "run_build".to_owned(),
                reason: CANNOT_RUN.to_owned(),
            },
            RefusedWrite {
                entity: BUILD.to_owned(),
                op: "rerun_build".to_owned(),
                reason: CANNOT_RUN.to_owned(),
            },
        ]
    );
    only_reads(&server).await;
}

/// The root project is every project's ancestor: an assignment on `_Root` is
/// one at global scope.
#[tokio::test]
async fn a_permission_on_the_root_project_holds_everywhere() {
    let server = teamcity(vec![
        assignment("run_build", Some("_Root")),
        assignment("edit_project", Some("_Root")),
    ])
    .await;
    let refused = source(&server)
        .refused_writes(&[ask(CONFIG, "set_parameter"), ask(BUILD, "rerun_build")])
        .await
        .expect("the permissions read");
    assert_eq!(refused, []);
}

/// No verdict, rather than a guess: a permissions read TeamCity refuses is
/// an error, which the app reads as nothing refused.
#[tokio::test]
async fn a_permissions_read_that_fails_is_an_error() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/app/rest/users/current/permissions"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;
    let answer = source(&server)
        .refused_writes(&[ask(CONFIG, "run_build")])
        .await;
    assert!(answer.is_err(), "{answer:?}");
    only_reads(&server).await;
}

/// An op TeamCity's permissions do not govern here, or an entity of another
/// source, asks nothing and is answered nothing.
#[tokio::test]
async fn an_ask_it_does_not_govern_sends_nothing() {
    let server = teamcity(Vec::new()).await;
    let refused = source(&server)
        .refused_writes(&[
            ask(CONFIG, "comment"),
            ask("gitea:repo:tidewater/payments", "run_build"),
        ])
        .await
        .expect("nothing to read");
    assert_eq!(refused, []);
    let asked = server.received_requests().await.expect("recorded");
    assert!(asked.is_empty(), "{asked:#?}");
}

/// A build that is gone has no project to read: that ask is no verdict, and
/// the others are still answered.
#[tokio::test]
async fn an_entity_whose_project_cannot_be_read_is_no_verdict() {
    let server = teamcity(Vec::new()).await;
    let refused = source(&server)
        .refused_writes(&[
            ask("teamcity:build:404", "rerun_build"),
            ask(CONFIG, "run_build"),
        ])
        .await
        .expect("the permissions read");
    assert_eq!(
        refused,
        [RefusedWrite {
            entity: CONFIG.to_owned(),
            op: "run_build".to_owned(),
            reason: CANNOT_RUN.to_owned(),
        }]
    );
}

// --- The configuration's own view (ADR-0020) --------------------------------

/// The configuration's record as the view reads it, with one parameter.
async fn with_config(server: &MockServer) {
    Mock::given(method("GET"))
        .and(path("/app/rest/buildTypes/id:Payout_IntegrationTests"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "Payout_IntegrationTests",
            "name": "Integration Tests",
            "projectId": "Payout",
            "projectName": "Tidewater / Payout",
            "parameters": { "property": [
                { "name": "env.SEPA_RETRIES", "value": "5", "inherited": false }
            ]}
        })))
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path("/app/rest/builds"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "count": 0, "build": [] })))
        .mount(server)
        .await;
}

fn editable(view: &SourceView) -> Vec<bool> {
    view.sections
        .iter()
        .flat_map(|s| &s.blocks)
        .filter_map(|b| match b {
            Block::Settings { values, .. } => Some(values.iter().map(|v| v.editable)),
            _ => None,
        })
        .flatten()
        .collect()
}

/// *Run…* and the parameters follow the configuration's project permissions
/// (#2040 criterion 3): a viewer is offered *Run…* and no parameter edit; a
/// user with neither right is offered *Run…* disabled, saying why.
#[tokio::test]
async fn the_view_draws_what_the_user_may_not_write_so() {
    let viewer = teamcity(vec![assignment("run_build", None)]).await;
    with_config(&viewer).await;
    let view = source(&viewer)
        .view(CONFIG, None)
        .await
        .expect("the read")
        .expect("drawn");
    assert_eq!(view.actions[0].write_op, "run_build");
    assert_eq!(view.actions[0].disabled, None);
    assert_eq!(editable(&view), [false]);
    only_reads(&viewer).await;

    let nobody = teamcity(vec![assignment("view_project", None)]).await;
    with_config(&nobody).await;
    let view = source(&nobody)
        .view(CONFIG, None)
        .await
        .expect("the read")
        .expect("drawn");
    assert_eq!(view.actions[0].disabled.as_deref(), Some(CANNOT_RUN));
    assert_eq!(editable(&view), [false]);

    let editor = teamcity(vec![
        assignment("run_build", Some("Payout")),
        assignment("edit_project", Some("Payout")),
    ])
    .await;
    with_config(&editor).await;
    let view = source(&editor)
        .view(CONFIG, Some("parameters"))
        .await
        .expect("the read")
        .expect("drawn");
    assert_eq!(editable(&view), [true]);
}

/// A permissions read that fails draws the view as before: no verdict is not
/// a refusal.
#[tokio::test]
async fn a_view_whose_permissions_read_fails_draws_as_before() {
    let server = MockServer::builder().start().await;
    with_config(&server).await;
    let view = source(&server)
        .view(CONFIG, None)
        .await
        .expect("the read")
        .expect("drawn");
    assert_eq!(view.actions[0].disabled, None);
    assert_eq!(editable(&view), [true]);
}
