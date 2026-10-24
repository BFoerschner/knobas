//! `run_build` against a wiremock TeamCity (#669, spec #660 story 21).
//!
//! The request's shape and the error classification live here. Whether the
//! real server builds the branch with the overrides is
//! `tests/live_teamcity_seeded.rs`'s to say
//! (`a_run_with_an_override_is_built_on_its_branch_and_synced_next`); on
//! disagreement this fake is what is wrong.

use knobas_source::instance::SourceInstance;
use knobas_source::{AuthMethod, BuildParameter, Source, SourceError, WriteOp};
use serde_json::json;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const TOKEN: &str = "tidewater-tc-token";
const CONFIG: &str = "teamcity:buildType:Payout_IntegrationTests";

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

fn run(entity: &str, branch: Option<&str>, parameters: &[(&str, &str)]) -> WriteOp {
    WriteOp::RunBuild {
        entity: entity.to_owned(),
        branch: branch.map(str::to_owned),
        parameters: parameters
            .iter()
            .map(|(name, value)| BuildParameter {
                name: (*name).to_owned(),
                value: (*value).to_owned(),
            })
            .collect(),
        clean_checkout: false,
        first_in_queue: false,
    }
}

/// The queue answering as the seeded 2026.1.3 server does: the queued build,
/// in full, with the id it was given.
async fn queue_answers(server: &MockServer) {
    Mock::given(method("POST"))
        .and(path("/app/rest/buildQueue"))
        .and(header("Authorization", format!("Bearer {TOKEN}").as_str()))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": 2214,
            "buildTypeId": "Payout_IntegrationTests",
            "state": "queued",
            "branchName": "feature/PAY-231-sepa-retry"
        })))
        .mount(server)
        .await;
}

/// The one body the queue was sent, as JSON.
async fn only_body(server: &MockServer) -> serde_json::Value {
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1, "{requests:#?}");
    serde_json::from_slice(&requests[0].body).expect("a JSON body")
}

/// Story 21: the queued body carries the branch as `branchName` and the
/// overrides -- exactly the ones the op carries, in order -- as
/// `properties`, the shape TeamCity's own *Run custom build* sends. The
/// receipt names the queued build.
#[tokio::test]
async fn a_run_queues_its_branch_and_only_its_overrides() {
    let server = MockServer::builder().start().await;
    queue_answers(&server).await;
    let receipt = source(&server)
        .write(run(
            CONFIG,
            Some("feature/PAY-231-sepa-retry"),
            &[("env.SEPA_RETRIES", "9"), ("tidewater.region", "")],
        ))
        .await
        .expect("queued");
    assert_eq!(
        only_body(&server).await,
        json!({
            "buildType": { "id": "Payout_IntegrationTests" },
            "branchName": "feature/PAY-231-sepa-retry",
            "properties": { "property": [
                { "name": "env.SEPA_RETRIES", "value": "9" },
                { "name": "tidewater.region", "value": "" }
            ] }
        })
    );
    assert_eq!(receipt.remote_id.as_deref(), Some("build:2214"));
}

/// No branch and no override is the plain trigger's body: TeamCity builds the
/// default branch with the configuration's own values. Neither key is sent
/// empty -- an empty `branchName` is not the default branch.
#[tokio::test]
async fn a_run_without_a_branch_or_overrides_sends_neither() {
    let server = MockServer::builder().start().await;
    queue_answers(&server).await;
    source(&server)
        .write(run(CONFIG, None, &[]))
        .await
        .expect("queued");
    assert_eq!(
        only_body(&server).await,
        json!({ "buildType": { "id": "Payout_IntegrationTests" } })
    );
}

/// A trigger and a re-run name the build they queued, as a custom run does
/// (#1949): by its key `build:<id>`, the local part of its entity id. The
/// re-run asks which configuration 1187 belongs to first, and queues that.
#[tokio::test]
async fn a_trigger_and_a_re_run_name_the_build_they_queued() {
    let server = MockServer::builder().start().await;
    queue_answers(&server).await;
    Mock::given(method("GET"))
        .and(path("/app/rest/builds/id:1187"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({ "buildType": { "id": "Payout_IntegrationTests" } })),
        )
        .mount(&server)
        .await;
    for op in [
        WriteOp::TriggerBuild {
            entity: CONFIG.to_owned(),
        },
        WriteOp::RerunBuild {
            entity: "teamcity:build:1187".to_owned(),
        },
    ] {
        let label = format!("{op:?}");
        let receipt = source(&server).write(op).await.expect("queued");
        assert_eq!(receipt.remote_id.as_deref(), Some("build:2214"), "{label}");
    }
}

/// The run options (#852, R-9.7): a clean checkout and first in the queue
/// cross as TeamCity's own `triggeringOptions` -- `cleanSources` and
/// `queueAtTop`, the keys its *Run custom build* dialog sends -- and only the
/// ones the reader ticked, so a run with neither is the body above.
#[tokio::test]
async fn a_runs_options_cross_as_triggering_options_and_only_when_ticked() {
    for (clean, first, expected) in [
        (
            true,
            true,
            Some(json!({ "cleanSources": true, "queueAtTop": true })),
        ),
        (true, false, Some(json!({ "cleanSources": true }))),
        (false, true, Some(json!({ "queueAtTop": true }))),
        (false, false, None),
    ] {
        let server = MockServer::builder().start().await;
        queue_answers(&server).await;
        let WriteOp::RunBuild {
            entity,
            branch,
            parameters,
            ..
        } = run(CONFIG, None, &[])
        else {
            unreachable!()
        };
        source(&server)
            .write(WriteOp::RunBuild {
                entity,
                branch,
                parameters,
                clean_checkout: clean,
                first_in_queue: first,
            })
            .await
            .expect("queued");
        let body = only_body(&server).await;
        assert_eq!(
            body.get("triggeringOptions").cloned(),
            expected,
            "clean {clean}, first {first}: {body}"
        );
    }
}

/// A queue that accepted the run but answered nothing readable still queued
/// it: the write succeeded, and only the receipt is empty.
#[tokio::test]
async fn an_answer_without_an_id_is_still_a_queued_run() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path("/app/rest/buildQueue"))
        .respond_with(ResponseTemplate::new(200).set_body_string("queued"))
        .mount(&server)
        .await;
    let receipt = source(&server)
        .write(run(CONFIG, Some("main"), &[]))
        .await
        .expect("queued");
    assert_eq!(receipt.remote_id, None);
}

/// #1122: a POST to the queue that failed may still have queued its build,
/// and a resend queues a second one. So a trigger and a custom run are each
/// sent once, even on a busy answer the transport retries for a read.
#[tokio::test]
async fn a_run_is_put_on_the_queue_once_even_on_a_busy_answer() {
    for op in [
        WriteOp::TriggerBuild {
            entity: CONFIG.to_owned(),
        },
        run(CONFIG, Some("main"), &[]),
    ] {
        let server = MockServer::builder().start().await;
        Mock::given(method("POST"))
            .and(path("/app/rest/buildQueue"))
            .respond_with(ResponseTemplate::new(503))
            .mount(&server)
            .await;
        let label = format!("{op:?}");
        let error = source(&server).write(op).await.expect_err("503");
        assert_eq!(error.status(), Some(503), "{label}");
        assert_eq!(
            server.received_requests().await.unwrap().len(),
            1,
            "{label}"
        );
    }
}

/// A run belongs to a **build configuration**, with a branch that is a name
/// and overrides that have one: a build's id, another source's id, an id
/// TeamCity could not have issued, an empty or blank branch and an unnamed
/// parameter are all refused before a request leaves.
#[tokio::test]
async fn a_run_that_names_nothing_runnable_is_refused_before_a_request() {
    let server = MockServer::builder().start().await;
    for op in [
        run("teamcity:build:1187", None, &[]),
        run("teamcity-eu:buildType:Payout_IntegrationTests", None, &[]),
        run("teamcity:buildType:Payout/../x", None, &[]),
        run(CONFIG, Some(""), &[]),
        run(CONFIG, Some("  "), &[]),
        run(CONFIG, None, &[("", "9")]),
    ] {
        let error = source(&server)
            .write(op.clone())
            .await
            .expect_err("refused");
        assert!(
            matches!(error, SourceError::Protocol { status: None, .. }),
            "{op:?}: {error:?}"
        );
    }
    assert!(server.received_requests().await.unwrap().is_empty());
}

/// Refusals are classified as everywhere else in the adapter: an account
/// that may not run the configuration is `Unauthorized`, and a configuration
/// that is gone is a refusal carrying TeamCity's own sentence.
#[tokio::test]
async fn a_refused_run_is_classified_as_elsewhere_in_the_adapter() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(403).set_body_string(
            "You do not have enough permissions to run builds in project with id: Payout",
        ))
        .mount(&server)
        .await;
    let error = source(&server)
        .write(run(CONFIG, Some("main"), &[("env.SEPA_RETRIES", "9")]))
        .await
        .expect_err("refused");
    assert!(
        matches!(error, SourceError::Unauthorized { status: Some(403) }),
        "{error:?}"
    );

    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(404).set_body_json(json!({ "errors": [{
                "message": "No build type nor template is found by id 'Payout_IntegrationTests'."
            }]})),
        )
        .mount(&server)
        .await;
    let error = source(&server)
        .write(run(CONFIG, None, &[]))
        .await
        .expect_err("refused");
    assert_eq!(error.status(), Some(404), "{error:?}");
    assert!(
        matches!(&error, SourceError::Protocol { message, .. } if message.contains("No build type")),
        "{error:?}"
    );
}
