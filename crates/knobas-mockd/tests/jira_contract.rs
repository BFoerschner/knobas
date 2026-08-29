//! The Jira fidelity gate: every body mockd serves is validated against the
//! JSON Schema the pinned WADL embeds for that response, and against a golden
//! snapshot of the body itself.
//!
//! Two halves because either alone lets a regression through. The schema
//! catches **shape** drift (a `String` where the contract says integer, an
//! `additionalProperties: false` object that grew a key) but says nothing about
//! whether the number is right; the golden catches **content** drift (a field
//! that quietly stopped being populated) but would happily pin a body that the
//! contract forbids.

mod common;

use common::{golden, normalise_base_url};
use knobas_mockd::{allowlist::response_schema, spawn_mock_jira};

/// The endpoints mockd serves, as `(verb, WADL template, request path,
/// golden name)`. One table, used by both halves, so a new endpoint cannot be
/// added to one gate and forgotten in the other.
const SERVED: &[(&str, &str, &str, &str)] = &[
    (
        "GET",
        "api/2/serverInfo",
        "/rest/api/2/serverInfo",
        "server_info",
    ),
    ("GET", "api/2/myself", "/rest/api/2/myself", "myself"),
    (
        "GET",
        "api/2/search",
        "/rest/api/2/search?jql=&fields=*all&maxResults=100",
        "search_all",
    ),
    (
        "GET",
        "api/2/issue/{issueIdOrKey}",
        "/rest/api/2/issue/PAY-231?fields=*all",
        "issue_pay_231",
    ),
    (
        "GET",
        "api/2/issue/{issueIdOrKey}/comment",
        "/rest/api/2/issue/PAY-231/comment",
        "comments_pay_231",
    ),
    (
        "GET",
        "api/2/issue/{issueIdOrKey}/worklog",
        "/rest/api/2/issue/PAY-231/worklog",
        "worklogs_pay_231",
    ),
    // The read half of M2's transition write-back (issue #43). PAY-231 is
    // `In Progress`, whose workflow offers two moves and does *not* offer
    // `Done` -- so the golden is also the record of the workflow having shape.
    (
        "GET",
        "api/2/issue/{issueIdOrKey}/transitions",
        "/rest/api/2/issue/PAY-231/transitions",
        "transitions_pay_231",
    ),
];

fn assert_conforms(schema_src: &str, instance: &serde_json::Value, what: &str) {
    let schema: serde_json::Value = serde_json::from_str(schema_src).unwrap();
    let validator = jsonschema::validator_for(&schema)
        .unwrap_or_else(|e| panic!("{what}: schema does not compile: {e}"));
    let errors: Vec<String> = validator
        .iter_errors(instance)
        .map(|e| format!("{} at {}", e, e.instance_path()))
        .collect();
    assert!(
        errors.is_empty(),
        "{what} violates its WADL schema:\n  {}",
        errors.join("\n  ")
    );
}

async fn get(base: &str, path: &str) -> serde_json::Value {
    reqwest::Client::new()
        .get(format!("{base}{path}"))
        .header(
            "Authorization",
            format!("Bearer {}", knobas_mockd::JIRA_TOKEN),
        )
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap()
}

#[tokio::test]
async fn every_served_response_conforms_to_the_wadl_schema() {
    let s = spawn_mock_jira().await;
    let b = s.base_url();
    for (verb, template, path, _) in SERVED {
        let schema = response_schema(verb, template)
            .unwrap_or_else(|| panic!("no embedded schema for {verb} {template}"));
        let body = get(&b, path).await;
        assert_conforms(schema, &body, path);
    }
    s.assert_no_violations();
}

#[tokio::test]
async fn every_served_response_matches_its_golden() {
    let s = spawn_mock_jira().await;
    let b = s.base_url();
    for (_, _, path, name) in SERVED {
        let mut body = get(&b, path).await;
        normalise_base_url(&mut body, &b);
        golden("jira", name, &body);
    }
    s.assert_no_violations();
}
