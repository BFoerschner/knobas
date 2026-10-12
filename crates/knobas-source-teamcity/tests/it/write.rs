//! `set_parameter` against a wiremock TeamCity (#668, spec #660 story 19).
//!
//! The request's shape and the error classification live here. Whether the
//! real server takes it is `tests/live_teamcity_seeded.rs`'s to say
//! (`a_seeded_parameter_set_through_the_adapter_reads_back_in_the_next_view`);
//! on disagreement this fake is what is wrong.

use knobas_source::instance::SourceInstance;
use knobas_source::{AuthMethod, Source, SourceError, WriteOp};
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

/// A set with no `base`, as for a secret: no read before the `PUT`.
fn set(entity: &str, name: &str, value: &str) -> WriteOp {
    WriteOp::SetParameter {
        entity: entity.to_owned(),
        name: name.to_owned(),
        value: value.to_owned(),
        base: None,
    }
}

/// A set edited against `base`, as the view's every drawn value is.
fn set_from(name: &str, base: &str, value: &str) -> WriteOp {
    WriteOp::SetParameter {
        entity: CONFIG.to_owned(),
        name: name.to_owned(),
        value: value.to_owned(),
        base: Some(base.to_owned()),
    }
}

/// The parameter resource answering `body`, as the seeded 2026.1.3 server
/// answers `GET .../parameters/{name}?fields=name,value,inherited,type(rawValue)`.
async fn holding(server: &MockServer, name: &str, body: serde_json::Value) {
    Mock::given(method("GET"))
        .and(path(format!(
            "/app/rest/buildTypes/id:Payout_IntegrationTests/parameters/{name}"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(server)
        .await;
}

/// The `PUT`s a write made.
async fn puts(server: &MockServer) -> Vec<String> {
    server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|r| r.method.as_str() == "PUT")
        .map(|r| String::from_utf8(r.body.clone()).unwrap())
        .collect()
}

/// The one request a set made: verb, path as sent, content type, accept, body.
async fn only_request(server: &MockServer) -> (String, String, String, String, String) {
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1, "{requests:#?}");
    let r = &requests[0];
    assert_eq!(
        r.headers.get_all("accept").iter().count(),
        1,
        "one Accept, not the client's default beside the request's own"
    );
    let head = |name: &str| {
        r.headers
            .get(name)
            .map(|v| v.to_str().unwrap().to_owned())
            .unwrap_or_default()
    };
    (
        r.method.to_string(),
        r.url.path().to_owned(),
        head("content-type"),
        head("accept"),
        String::from_utf8(r.body.clone()).unwrap(),
    )
}

/// Story 19: a value goes to the parameter's `value` resource as the plain
/// text it is -- not JSON, not quoted -- and asks for plain text back.
///
/// The `Accept` is the half that matters on a real server and that no fake
/// enforces by itself: the client's default `application/json` makes TeamCity
/// answer **406 and write nothing** (measured on the seeded 2026.1.3 server,
/// 2026-10-12), so this asserts the header the request carries rather than
/// trusting the mock to refuse.
#[tokio::test]
async fn a_parameter_is_put_as_plain_text_on_its_value_resource() {
    let server = MockServer::start().await;
    Mock::given(method("PUT"))
        .and(path(
            "/app/rest/buildTypes/id:Payout_IntegrationTests/parameters/env.SEPA_RETRIES/value",
        ))
        .and(header("Authorization", format!("Bearer {TOKEN}").as_str()))
        // TeamCity answers the value it now holds, as text.
        .respond_with(ResponseTemplate::new(200).set_body_string("7"))
        .mount(&server)
        .await;
    source(&server)
        .write(set(CONFIG, "env.SEPA_RETRIES", "7"))
        .await
        .expect("a declared op is performed");

    let (verb, path, content_type, accept, body) = only_request(&server).await;
    assert_eq!(verb, "PUT");
    assert_eq!(
        path,
        "/app/rest/buildTypes/id:Payout_IntegrationTests/parameters/env.SEPA_RETRIES/value"
    );
    assert!(content_type.starts_with("text/plain"), "{content_type}");
    assert_eq!(accept, "text/plain", "TeamCity answers 406 to any other");
    assert_eq!(body, "7", "the value verbatim, not a JSON string");
}

/// A value is sent byte for byte: spaces, quotes, newlines and an empty
/// string are values a parameter can hold, and none of them may be trimmed
/// or escaped on the way.
#[tokio::test]
async fn a_value_is_sent_byte_for_byte_and_an_empty_one_too() {
    for value in ["", "  two words  ", "line one\nline \"two\"", "%{x}%"] {
        let server = MockServer::start().await;
        Mock::given(method("PUT"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;
        source(&server)
            .write(set(CONFIG, "env.NOTE", value))
            .await
            .expect("performed");
        let (.., body) = only_request(&server).await;
        assert_eq!(body, value);
    }
}

/// A name is one path segment whatever it holds: a `/` or a `?` in a name
/// must not move the request to another resource.
#[tokio::test]
async fn a_name_stays_one_path_segment() {
    let server = MockServer::start().await;
    Mock::given(method("PUT"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    source(&server)
        .write(set(CONFIG, "odd/name?x=1 #y", "v"))
        .await
        .expect("performed");
    let (_, path, ..) = only_request(&server).await;
    assert_eq!(
        path,
        "/app/rest/buildTypes/id:Payout_IntegrationTests/parameters/odd%2Fname%3Fx%3D1%20%23y/value"
    );
}

/// A 403 -- the account may not edit this project -- is `Unauthorized`, as
/// every other 403 in this adapter is (interfaces §4.1); a 404 for a
/// configuration that is gone is a refusal carrying TeamCity's sentence.
#[tokio::test]
async fn a_refusal_is_classified_as_elsewhere_in_the_adapter() {
    let server = MockServer::start().await;
    Mock::given(method("PUT"))
        .respond_with(
            ResponseTemplate::new(403).set_body_string(
                "You do not have enough permissions to edit project with id: Payout",
            ),
        )
        .mount(&server)
        .await;
    let error = source(&server)
        .write(set(CONFIG, "env.SEPA_RETRIES", "7"))
        .await
        .expect_err("refused");
    assert!(
        matches!(error, SourceError::Unauthorized { status: Some(403) }),
        "{error:?}"
    );

    let server = MockServer::start().await;
    Mock::given(method("PUT"))
        .respond_with(
            ResponseTemplate::new(404).set_body_json(json!({ "errors": [{
                "message": "No build type nor template is found by id 'Payout_IntegrationTests'."
            }]})),
        )
        .mount(&server)
        .await;
    let error = source(&server)
        .write(set(CONFIG, "env.SEPA_RETRIES", "7"))
        .await
        .expect_err("refused");
    assert_eq!(error.status(), Some(404), "{error:?}");
    assert!(
        matches!(&error, SourceError::Protocol { message, .. } if message.contains("No build type")),
        "{error:?}"
    );
}

/// A parameter belongs to a **build configuration**: a build's id, another
/// source's id, an id TeamCity could not have issued and an empty name are
/// all refused before a request leaves.
#[tokio::test]
async fn a_target_that_is_no_configuration_is_refused_before_a_request() {
    let server = MockServer::start().await;
    for (entity, name) in [
        ("teamcity:build:1187", "env.SEPA_RETRIES"),
        (
            "teamcity-eu:buildType:Payout_IntegrationTests",
            "env.SEPA_RETRIES",
        ),
        ("teamcity:buildType:Payout/../x", "env.SEPA_RETRIES"),
        (CONFIG, ""),
        (CONFIG, ".."),
    ] {
        let error = source(&server)
            .write(set(entity, name, "7"))
            .await
            .expect_err("refused");
        assert!(
            matches!(error, SourceError::Protocol { status: None, .. }),
            "{entity} {name:?}: {error:?}"
        );
    }
    assert!(server.received_requests().await.unwrap().is_empty());
}

/// The concurrency check (#668): a colleague changed the parameter between
/// the reader's edit and the flush. The mirror cannot see it -- a
/// configuration's record holds no parameters -- so the adapter reads the
/// parameter just before the `PUT` and refuses, naming what it holds now,
/// rather than overwrite it silently. Nothing is written.
#[tokio::test]
async fn a_parameter_changed_since_the_edit_is_refused_with_its_value_now() {
    let server = MockServer::start().await;
    holding(
        &server,
        "env.SEPA_RETRIES",
        json!({ "name": "env.SEPA_RETRIES", "value": "9", "inherited": false }),
    )
    .await;
    Mock::given(method("PUT"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    let error = source(&server)
        .write(set_from("env.SEPA_RETRIES", "5", "7"))
        .await
        .expect_err("refused");
    assert!(
        matches!(&error, SourceError::Protocol { status: None, message }
            if message.contains("changed since you edited") && message.contains("\"9\"")),
        "{error:?}"
    );
    assert!(
        puts(&server).await.is_empty(),
        "the colleague's value stays"
    );
}

/// The other direction: the value is still the one edited against, so the
/// write goes -- and so it does when it already holds the new value, which
/// is this write having landed before an answer was lost (ADR-0012).
#[tokio::test]
async fn a_parameter_still_as_edited_or_already_set_is_written() {
    for now in ["5", "7"] {
        let server = MockServer::start().await;
        holding(
            &server,
            "env.SEPA_RETRIES",
            json!({ "name": "env.SEPA_RETRIES", "value": now, "inherited": false }),
        )
        .await;
        Mock::given(method("PUT"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;
        source(&server)
            .write(set_from("env.SEPA_RETRIES", "5", "7"))
            .await
            .unwrap_or_else(|e| panic!("holding {now:?}: {e:?}"));
        assert_eq!(puts(&server).await, ["7"], "holding {now:?}");
    }
}

/// An inherited value is compared too: the edit was against what the
/// configuration inherits, and a project-level change since is a change.
#[tokio::test]
async fn an_inherited_value_changed_above_is_refused() {
    let server = MockServer::start().await;
    holding(
        &server,
        "tidewater.region",
        json!({ "name": "tidewater.region", "value": "us-east-1", "inherited": true }),
    )
    .await;
    let error = source(&server)
        .write(set_from("tidewater.region", "eu-central-1", "eu-west-1"))
        .await
        .expect_err("refused");
    assert!(
        matches!(&error, SourceError::Protocol { message, .. } if message.contains("us-east-1")),
        "{error:?}"
    );
    assert!(puts(&server).await.is_empty());
}

/// The refusal names the value **as the view would draw it**: a secret's
/// value and a URL's credential stay out of it as they stay out of the view.
#[tokio::test]
async fn a_refusal_never_names_what_the_view_would_not_draw() {
    let server = MockServer::start().await;
    holding(
        &server,
        "vcs.mirror",
        json!({ "name": "vcs.mirror", "value": "https://ci:mirror-token@git.example/q.git" }),
    )
    .await;
    holding(
        &server,
        "env.KEY",
        json!({ "name": "env.KEY", "value": "now-a-secret-value",
                "type": { "rawValue": "password display='hidden'" } }),
    )
    .await;
    for (name, base) in [
        ("vcs.mirror", "https://git.example/p.git"),
        ("env.KEY", "plain before"),
    ] {
        let error = source(&server)
            .write(set_from(name, base, "x"))
            .await
            .expect_err("refused");
        let text = format!("{error:?}");
        assert!(text.contains("changed since you edited"), "{text}");
        assert!(
            !text.contains("mirror-token") && !text.contains("now-a-secret-value"),
            "{text}"
        );
    }
    assert!(puts(&server).await.is_empty());
}
