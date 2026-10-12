//! A build configuration's source view against a wiremock TeamCity (#664,
//! ADR-0020).
//!
//! The mapping and the error classification live here. Whether the real
//! server answers these shapes is `tests/live_teamcity_seeded.rs`'s to say
//! (`the_seeded_configurations_view_lists_its_builds_parameters_and_steps`);
//! on disagreement this fake is what is wrong.
//!
//! The records are TeamCity's own shapes, as the seeded 2026.1.3 server
//! answered them on 2026-10-12 -- with one deliberate lie: the password
//! parameter here **carries its value**. A real TeamCity masks it; the view
//! must not depend on that, so the fake leaks it and the view must still
//! never draw it.

use knobas_source::instance::SourceInstance;
use knobas_source::view::{Block, Fact, SourceView, Tone, ViewSection};
use knobas_source::{AuthMethod, Source, SourceError};
use serde_json::json;
use wiremock::matchers::{header, method, path, query_param, query_param_contains};
use wiremock::{Mock, MockServer, ResponseTemplate};

const TOKEN: &str = "tidewater-tc-token";
const CONFIG: &str = "teamcity:buildType:Payout_IntegrationTests";
const CONFIG_PATH: &str = "/app/rest/buildTypes/id:Payout_IntegrationTests";
/// What the fake's password parameter holds, and what no view may contain.
const SECRET: &str = "s3pa-sandbox-key-never-drawn";

fn instance(base_url: String) -> SourceInstance {
    SourceInstance {
        id: "teamcity".to_owned(),
        kind: "teamcity".to_owned(),
        display_name: "Tidewater CI".to_owned(),
        base_url,
        auth: Some(AuthMethod::Pat),
        secret: Some(TOKEN.to_owned()),
        account: None,
        config: json!({}),
    }
}

fn config_record() -> serde_json::Value {
    json!({
        "id": "Payout_IntegrationTests",
        "name": "Integration Tests",
        "projectId": "Payout",
        "projectName": "Tidewater / Payout",
        "description": "Runs the SEPA suite against the sandbox",
        "paused": true,
        "parameters": { "property": [
            { "name": "env.SEPA_RETRIES", "value": "5", "inherited": false },
            { "name": "env.SEPA_SANDBOX_KEY", "value": SECRET, "inherited": false,
              "type": { "rawValue": "password display='hidden'" } },
            { "name": "tidewater.region", "value": "eu-central-1", "inherited": true }
        ]},
        "steps": { "step": [
            { "id": "RUNNER_7", "name": "Run the suite", "type": "simpleRunner", "disabled": false,
              "properties": { "property": [
                  { "name": "script.content", "value": "cargo test --test sepa\nexit 1" },
                  { "name": "use.custom.script", "value": "true" }
              ]}},
            { "id": "RUNNER_8", "name": "Report", "type": "simpleRunner", "disabled": true,
              "properties": { "property": [
                  { "name": "command.executable", "value": "./report.sh" },
                  { "name": "command.parameters", "value": "--format junit" }
              ]}}
        ]},
        "triggers": { "trigger": [
            { "id": "TRIGGER_1", "type": "vcsTrigger", "disabled": true,
              "properties": { "property": [
                  { "name": "branchFilter", "value": "+:feature/*" },
                  { "name": "quietPeriodMode", "value": "DO_NOT_USE" }
              ]}},
            { "id": "TRIGGER_2", "type": "schedulingTrigger", "disabled": false,
              "properties": { "property": [
                  { "name": "schedulingPolicy", "value": "daily" },
                  { "name": "hour", "value": "3" },
                  { "name": "minute", "value": "0" },
                  { "name": "branchFilter", "value": "+:<default>" }
              ]}}
        ]},
        "vcs-root-entries": { "vcs-root-entry": [
            { "id": "Payout_PayoutService", "checkout-rules": "+:src => src",
              "vcs-root": { "id": "Payout_PayoutService", "name": "Payout / payout-service",
                            "vcsName": "jetbrains.git",
                            "properties": { "property": [
                                { "name": "authMethod", "value": "PASSWORD" },
                                { "name": "branch", "value": "refs/heads/main" },
                                { "name": "secure:password" },
                                { "name": "url", "value": "http://gitea:3000/tidewater/payout-service.git" }
                            ]}}}
        ]}
    })
}

/// Newest first, as `/app/rest/builds` answers: running, failed, canceled,
/// succeeded, queued-and-older shapes all in one page.
fn builds() -> serde_json::Value {
    json!({ "count": 4, "build": [
        { "id": 1190, "number": "1190", "state": "running", "status": "SUCCESS",
          "statusText": "Step 1/2", "branchName": "main",
          "queuedDate": "20260822T120000+0000", "startDate": "20260822T120010+0000" },
        { "id": 1187, "number": "1187", "state": "finished", "status": "FAILURE",
          "statusText": "Exit code 1", "branchName": "feature/PAY-231-sepa-retry",
          "queuedDate": "20260822T100500+0000", "startDate": "20260822T100600+0000",
          "finishDate": "20260822T101018+0000" },
        { "id": 1186, "number": "1186", "state": "finished", "status": "UNKNOWN",
          "statusText": "Canceled", "branchName": "main",
          "startDate": "20260822T090000+0000", "finishDate": "20260822T090038+0000" },
        { "id": 1185, "number": "1185", "state": "finished", "status": "SUCCESS",
          "statusText": "Tests passed: 42", "branchName": "main",
          "startDate": "20260821T090000+0000", "finishDate": "20260821T100205+0000" }
    ]})
}

/// A TeamCity that answers the configuration and its builds.
async fn teamcity() -> MockServer {
    let server = MockServer::start().await;
    let bearer = format!("Bearer {TOKEN}");
    Mock::given(method("GET"))
        .and(path(CONFIG_PATH))
        .and(header("Authorization", bearer.as_str()))
        // One request for every tab but the builds, with an explicit fields=.
        .and(query_param_contains(
            "fields",
            "parameters(property(name,value,inherited,type(rawValue)))",
        ))
        .and(query_param_contains("fields", "steps(step("))
        .and(query_param_contains("fields", "triggers(trigger("))
        .and(query_param_contains(
            "fields",
            "vcs-root-entries(vcs-root-entry(",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(config_record()))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/app/rest/builds"))
        .and(header("Authorization", bearer.as_str()))
        .and(query_param(
            "locator",
            "buildType:(id:Payout_IntegrationTests),branch:default:any,defaultFilter:false,count:20",
        ))
        .and(query_param_contains("fields", "build(id,number,state,status"))
        .respond_with(ResponseTemplate::new(200).set_body_json(builds()))
        .mount(&server)
        .await;
    server
}

fn source(server: &MockServer) -> Box<dyn Source> {
    match knobas_source_teamcity::build(instance(server.uri())) {
        Ok(source) => source,
        Err(e) => panic!("the adapter builds: {e:?}"),
    }
}

async fn whole_view(server: &MockServer) -> SourceView {
    source(server)
        .view(CONFIG, None)
        .await
        .expect("the read succeeds")
        .expect("TeamCity draws a build configuration")
}

fn section<'v>(view: &'v SourceView, id: &str) -> &'v ViewSection {
    view.sections
        .iter()
        .find(|s| s.id == id)
        .unwrap_or_else(|| panic!("no section {id:?} in {view:#?}"))
}

fn facts(block: &Block) -> &[Fact] {
    match block {
        Block::Facts { facts } => facts,
        other => panic!("expected facts, got {other:#?}"),
    }
}

fn fact<'f>(facts: &'f [Fact], label: &str) -> &'f Fact {
    facts
        .iter()
        .find(|f| f.label == label)
        .unwrap_or_else(|| panic!("no fact {label:?} in {facts:#?}"))
}

#[tokio::test]
async fn a_build_configuration_opens_on_its_overview_with_one_tab_per_part() {
    let server = teamcity().await;
    let view = whole_view(&server).await;
    let tabs: Vec<_> = view
        .sections
        .iter()
        .map(|s| (s.id.as_str(), s.title.as_str(), s.count, s.deferred))
        .collect();
    assert_eq!(
        tabs,
        [
            ("overview", "Overview", None, false),
            ("parameters", "Parameters", Some(3), false),
            ("steps", "Steps", Some(2), false),
            ("triggers", "Triggers", Some(2), false),
            ("vcs_roots", "VCS roots", Some(1), false),
        ]
    );
    // One configuration read and one builds read for the whole view.
    let asked = server.received_requests().await.expect("recorded");
    assert_eq!(asked.len(), 2, "{asked:#?}");
}

#[tokio::test]
async fn the_overview_says_where_it_lives_whether_it_is_paused_and_how_it_has_been_doing() {
    let server = teamcity().await;
    let view = whole_view(&server).await;
    let overview = section(&view, "overview");
    let facts = facts(&overview.blocks[0]);

    assert_eq!(fact(facts, "Project").value, "Tidewater / Payout");
    let paused = fact(facts, "Paused");
    assert_eq!(paused.value, "yes");
    assert_eq!(paused.tone, Some(Tone::Warn), "paused is something wrong");
    // The newest *finished* build -- the running one has no outcome yet.
    let last = fact(facts, "Last build");
    assert_eq!(last.value, "#1187 failed");
    assert_eq!(last.tone, Some(Tone::Err));
    // Over the finished builds that have an outcome: 1185 passed, 1187
    // failed; 1186 was canceled and says nothing about health.
    assert_eq!(fact(facts, "Success rate").value, "50 % of 2 finished");

    assert!(
        overview.blocks.iter().any(|b| matches!(b,
            Block::Text { text, markdown: false } if text == "Runs the SEPA suite against the sandbox")),
        "the description is drawn: {overview:#?}"
    );

    let rows = overview
        .blocks
        .iter()
        .find_map(|b| match b {
            Block::Entities { title, rows } if title.as_deref() == Some("Recent builds") => {
                Some(rows)
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("no recent builds in {overview:#?}"));
    let drawn: Vec<_> = rows
        .iter()
        .map(|r| {
            (
                r.chip.as_deref(),
                r.tone,
                r.title.as_str(),
                r.entity_id.as_deref(),
            )
        })
        .collect();
    assert_eq!(
        drawn,
        [
            (
                Some("#1190"),
                Some(Tone::Info),
                "Step 1/2",
                Some("teamcity:build:1190")
            ),
            (
                Some("#1187"),
                Some(Tone::Err),
                "Exit code 1",
                Some("teamcity:build:1187")
            ),
            (
                Some("#1186"),
                Some(Tone::Muted),
                "Canceled",
                Some("teamcity:build:1186")
            ),
            (
                Some("#1185"),
                Some(Tone::Ok),
                "Tests passed: 42",
                Some("teamcity:build:1185")
            ),
        ]
    );
    let failed = &rows[1];
    let meta = failed.meta.as_deref().expect("a row has a meta line");
    assert!(
        meta.starts_with("feature/PAY-231-sepa-retry · 4m 18s · ") && meta.ends_with(" ago"),
        "branch · duration · age: {meta}"
    );
    assert!(
        rows[3]
            .meta
            .as_deref()
            .unwrap_or_default()
            .starts_with("main · 1h 02m · "),
        "{:?}",
        rows[3].meta
    );
    assert_eq!(
        failed.url.as_deref(),
        Some(
            format!(
                "{}/buildConfiguration/Payout_IntegrationTests/1187",
                server.uri()
            )
            .as_str()
        ),
        "the build's own page, composed from the configured base URL"
    );
}

#[tokio::test]
async fn parameters_mark_the_inherited_and_never_carry_a_passwords_value() {
    let server = teamcity().await;
    let view = whole_view(&server).await;
    let parameters = section(&view, "parameters");
    let [
        Block::Settings {
            entity,
            write_op,
            values,
            ..
        },
    ] = parameters.blocks.as_slice()
    else {
        panic!("parameters are one settings block: {parameters:#?}");
    };
    assert_eq!(entity, CONFIG, "an edit targets the configuration");
    assert_eq!(write_op, "set_parameter");
    let drawn: Vec<_> = values
        .iter()
        .map(|s| {
            (
                s.name.as_str(),
                s.value.as_str(),
                s.inherited,
                s.secret,
                s.editable,
            )
        })
        .collect();
    assert_eq!(
        drawn,
        [
            ("env.SEPA_RETRIES", "5", false, false, false),
            ("env.SEPA_SANDBOX_KEY", "", false, true, false),
            ("tidewater.region", "eu-central-1", true, false, false),
        ]
    );

    let serialized = serde_json::to_string(&view).expect("serializes");
    assert!(
        !serialized.contains(SECRET),
        "a password's value crossed into the view, though TeamCity sent it: {serialized}"
    );
}

#[tokio::test]
async fn steps_show_their_runner_and_their_script() {
    let server = teamcity().await;
    let view = whole_view(&server).await;
    let steps = section(&view, "steps");
    assert_eq!(
        steps.blocks.len(),
        4,
        "facts then script, per step: {steps:#?}"
    );

    let first = facts(&steps.blocks[0]);
    assert_eq!(fact(first, "Step").value, "Run the suite");
    assert_eq!(fact(first, "Runner").value, "Command Line");
    assert!(
        matches!(&steps.blocks[1], Block::Log { text, truncated: false } if text == "cargo test --test sepa\nexit 1"),
        "the script is a log block, monospace and whole: {:#?}",
        steps.blocks[1]
    );

    let second = facts(&steps.blocks[2]);
    assert_eq!(fact(second, "Step").value, "Report");
    let disabled = fact(second, "Disabled");
    assert_eq!(disabled.tone, Some(Tone::Muted));
    assert!(
        matches!(&steps.blocks[3], Block::Log { text, .. } if text == "./report.sh --format junit"),
        "an executable runner shows its command line: {:#?}",
        steps.blocks[3]
    );
}

#[tokio::test]
async fn triggers_and_vcs_roots_say_when_and_from_where() {
    let server = teamcity().await;
    let view = whole_view(&server).await;

    let triggers = section(&view, "triggers");
    let vcs = facts(&triggers.blocks[0]);
    assert_eq!(fact(vcs, "Trigger").value, "VCS");
    assert_eq!(fact(vcs, "Branch filter").value, "+:feature/*");
    assert_eq!(fact(vcs, "Disabled").tone, Some(Tone::Muted));
    let schedule = facts(&triggers.blocks[1]);
    assert_eq!(fact(schedule, "Trigger").value, "Schedule");
    assert_eq!(fact(schedule, "When").value, "daily at 03:00");
    assert!(
        !schedule.iter().any(|f| f.label == "Disabled"),
        "an enabled trigger says nothing about it: {schedule:#?}"
    );

    let roots = section(&view, "vcs_roots");
    let root = facts(&roots.blocks[0]);
    assert_eq!(fact(root, "Root").value, "Payout / payout-service");
    assert_eq!(fact(root, "Type").value, "Git");
    assert_eq!(
        fact(root, "URL").value,
        "http://gitea:3000/tidewater/payout-service.git"
    );
    assert_eq!(fact(root, "Default branch").value, "refs/heads/main");
    assert_eq!(fact(root, "Checkout rules").value, "+:src => src");
    assert!(
        !root
            .iter()
            .any(|f| f.label.contains("password") || f.label == "authMethod"),
        "a root's credentials are not drawn: {root:#?}"
    );
}

/// A section asked for on its own is read on its own: the parameters tab does
/// not cost a builds read.
#[tokio::test]
async fn one_section_is_read_without_the_others() {
    let server = teamcity().await;
    let view = source(&server)
        .view(CONFIG, Some("parameters"))
        .await
        .expect("the read succeeds")
        .expect("drawn");
    let ids: Vec<_> = view.sections.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(ids, ["parameters"]);
    let asked = server.received_requests().await.expect("recorded");
    assert!(
        asked.iter().all(|r| r.url.path() == CONFIG_PATH),
        "only the configuration was read: {asked:#?}"
    );

    let refused = source(&server).view(CONFIG, Some("log")).await;
    assert!(
        matches!(&refused, Err(SourceError::Protocol { message, .. }) if message.contains("log")),
        "{refused:?}"
    );
}

/// A build has no view in this ticket (#665 draws it), and answers `None`
/// without a request, so its detail draws the mirror as before.
#[tokio::test]
async fn a_build_draws_no_view_yet() {
    let server = teamcity().await;
    let answer = source(&server)
        .view("teamcity:build:1187", None)
        .await
        .expect("no error");
    assert!(answer.is_none(), "{answer:?}");
    assert!(
        server
            .received_requests()
            .await
            .expect("recorded")
            .is_empty()
    );
}

/// A refusal is the error it maps to, status and all, so the detail can say
/// *Showing the mirror* and why (ADR-0020 rule 2).
#[tokio::test]
async fn a_configuration_the_server_does_not_know_is_its_404() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/app/rest/buildTypes/id:Gone"))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({
            "errors": [{ "message": "No build type nor template is found by id 'Gone'." }]
        })))
        .mount(&server)
        .await;
    let failed = source(&server).view("teamcity:buildType:Gone", None).await;
    let Err(error) = failed else {
        panic!("a 404 must be an error, got {failed:?}");
    };
    assert_eq!(error.status(), Some(404), "{error:?}");
}

/// An id that could leave its path segment is refused before any request.
#[tokio::test]
async fn a_configuration_id_that_is_not_one_is_refused_unsent() {
    let server = teamcity().await;
    for bad in [
        "teamcity:buildType:../server",
        "teamcity:buildType:a,b",
        "teamcity:buildType:",
    ] {
        let refused = source(&server).view(bad, None).await;
        assert!(
            matches!(refused, Err(SourceError::Protocol { .. })),
            "{bad}: {refused:?}"
        );
    }
    assert!(
        server
            .received_requests()
            .await
            .expect("recorded")
            .is_empty()
    );
}
