//! The TeamCity fidelity gate.
//!
//! Two halves, like the Jira one — but only one of them can run today.
//!
//! * **Goldens** pin the bodies mockd serves. Unblocked, and running.
//! * **Schemas** validate them against `testenv/specs/teamcity.json`. That
//!   document could not be vendored on the development machine (JetBrains
//!   publishes no static spec; the only source is a running server behind a
//!   human first-start wizard, and the host had 13 GiB free on a 98 %-full
//!   volume). The blocker is recorded in `testenv/specs/README.md`, per the
//!   plan's own fallback for exactly this case, and P11(b) already blesses
//!   golden-only validation for TeamCity.
//!
//! The schema half is written and **self-arming**: it looks for the document
//! and starts asserting the moment one is vendored, with no code change. And
//! [`the_teamcity_swagger_blocker_is_recorded`] fails if the README ever stops
//! saying why it is skipped, so the skip cannot be quietly forgotten.

mod common;

use common::{golden, normalise_base_url};
use knobas_mockd::spawn_mock_teamcity;

/// `(request path, golden name, swagger definition)`. One table for both
/// halves, so an endpoint cannot be added to one gate and missed in the other.
const SERVED: &[(&str, &str, &str)] = &[
    ("/app/rest/server", "server", "Server"),
    (
        "/app/rest/users/current?fields=$long",
        "users_current",
        "User",
    ),
    (
        "/app/rest/buildTypes?fields=$long",
        "build_types",
        "BuildTypes",
    ),
    (
        "/app/rest/builds?locator=state:any,count:100&fields=$long",
        "builds_all",
        "Builds",
    ),
    (
        "/app/rest/builds/id:1187?fields=$long",
        "build_1187",
        "Build",
    ),
];

fn swagger_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testenv/specs/teamcity.json")
}

async fn tc(base: &str, path: &str) -> serde_json::Value {
    reqwest::Client::new()
        .get(format!("{base}{path}"))
        .header("Accept", "application/json")
        .header(
            "Authorization",
            format!("Bearer {}", knobas_mockd::TEAMCITY_TOKEN),
        )
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap()
}

#[tokio::test]
async fn every_served_response_matches_its_golden() {
    let s = spawn_mock_teamcity().await;
    let b = s.base_url();
    for (path, name, _) in SERVED {
        let mut body = tc(&b, path).await;
        normalise_base_url(&mut body, &b);
        golden("teamcity", name, &body);
    }
    s.assert_no_violations();
}

/// Swagger 2.0 `definitions` are draft-04 JSON Schema. To validate against
/// `#/definitions/Builds`, the validator needs a document carrying both the
/// reference and the definitions it resolves against.
fn schema_for(doc: &serde_json::Value, definition: &str) -> serde_json::Value {
    serde_json::json!({
        "$ref": format!("#/definitions/{definition}"),
        "definitions": doc["definitions"],
    })
}

#[tokio::test]
async fn teamcity_responses_conform_to_the_vendored_swagger() {
    let path = swagger_path();
    let Ok(raw) = std::fs::read_to_string(&path) else {
        // Not a silent pass: the companion test below fails if the README stops
        // recording why this document is missing.
        eprintln!(
            "SKIP teamcity_responses_conform_to_the_vendored_swagger: {} is not vendered yet \
             (see testenv/specs/README.md, and run `./fetch.sh --teamcity`)",
            path.display()
        );
        return;
    };
    let doc: serde_json::Value = serde_json::from_str(&raw).expect("teamcity.json must be JSON");

    let s = spawn_mock_teamcity().await;
    let b = s.base_url();
    for (request, _, definition) in SERVED {
        let body = tc(&b, request).await;
        let schema = schema_for(&doc, definition);
        let validator = jsonschema::validator_for(&schema)
            .unwrap_or_else(|e| panic!("#/definitions/{definition} does not compile: {e}"));
        let errs: Vec<String> = validator
            .iter_errors(&body)
            .map(|e| format!("{} at {}", e, e.instance_path()))
            .collect();
        assert!(
            errs.is_empty(),
            "{request} violates #/definitions/{definition}:\n  {}",
            errs.join("\n  ")
        );
    }
    s.assert_no_violations();
}

#[test]
fn the_teamcity_swagger_blocker_is_recorded() {
    // The one thing that must never happen quietly: the schema half skipping
    // while nothing in the tree says why. Either the document is vendored, or
    // the README explains that it is not.
    let readme = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testenv/specs/README.md"),
    )
    .expect("testenv/specs/README.md");
    if swagger_path().exists() {
        // Its *own* row, not just the word somewhere in the file: every other
        // vendored document already says "vendored", so a whole-file search
        // would pass without anyone touching the TeamCity row at all.
        let row = readme
            .lines()
            .find(|l| l.starts_with("| `teamcity.json` |"))
            .expect("README.md must have a `teamcity.json` row");
        // Negatively, not positively: the row's blocked spelling is "**BLOCKED,
        // not vendored**", so a `contains("vendored")` check passes on the very
        // text it is supposed to reject.
        assert!(
            !row.contains("BLOCKED") && !row.contains("not vendored"),
            "teamcity.json is present but its README row still says it is not: {row:?}. \
             Flip it to `vendored — <version>, <date>` and re-pin SHA256SUMS."
        );
        return;
    }
    assert!(
        readme.contains("TeamCity blocker"),
        "teamcity.json is absent and README.md no longer records why. A skipped fidelity gate \
         with no written reason is how a gate stays off forever."
    );
}
