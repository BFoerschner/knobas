//! The Jira Data Center router: the endpoints mockd serves, in the shapes the
//! vendored WADL declares.
//!
//! Every route here is also in the generated allowlist — the middleware runs
//! first and refuses anything the contract does not define, so a handler only
//! ever sees a request the real server would have accepted.

use std::sync::Arc;

use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use serde_json::{Value, json};

use crate::state::{MockState, jira_date};
use crate::validate::{jira_guard, unimplemented};

/// The Jira version mockd claims to be — the same one the pinned WADL
/// documents, so an adapter that gates on it sees a consistent story.
const JIRA_VERSION: &str = "9.17.0";
const JIRA_BUILD_NUMBER: u64 = 917_000;

/// The account `GET /rest/api/2/myself` reports. The whole fixture is written
/// from Mara's seat (`mockups/shared/dataset.md`), so she is who mockd
/// authenticates as.
const MYSELF: &str = "mara";

pub fn router(state: Arc<MockState>) -> Router {
    Router::new()
        .route("/rest/api/2/serverInfo", get(server_info))
        .route("/rest/api/2/myself", get(myself))
        .fallback(unimplemented)
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            jira_guard,
        ))
        .with_state(state)
}

/// A Jira user object, as the WADL's `user` definition declares it.
pub(crate) fn user_json(base: &str, username: &str) -> Value {
    let person = knobas_source_mock::fixture()
        .people
        .iter()
        .find(|p| p.username == username);
    let display = person.map_or_else(|| username.to_owned(), |p| p.name.clone());
    json!({
        "self": format!("{base}/rest/api/2/user?username={username}"),
        "name": username,
        "key": format!("JIRAUSER{}", 10_100 + person_index(username)),
        "emailAddress": format!("{username}@tidewater.example"),
        "avatarUrls": avatar_urls(base, username),
        "displayName": display,
        "active": true,
        "timeZone": "Europe/Berlin",
    })
}

fn person_index(username: &str) -> u64 {
    knobas_source_mock::fixture()
        .people
        .iter()
        .position(|p| p.username == username)
        .unwrap_or(0) as u64
}

fn avatar_urls(base: &str, username: &str) -> Value {
    let mut m = serde_json::Map::new();
    for size in [16, 24, 32, 48] {
        m.insert(
            format!("{size}x{size}"),
            Value::String(format!(
                "{base}/secure/useravatar?size={size}&ownerId={username}"
            )),
        );
    }
    Value::Object(m)
}

async fn server_info(State(s): State<Arc<MockState>>) -> Json<Value> {
    let off = s.server_offset();
    // A fixed build date so the response is byte-stable across runs; the
    // server *time* moves with the mock's own clock, never the wall clock.
    let build_date = chrono::DateTime::parse_from_rfc3339("2026-05-14T10:00:00Z")
        .expect("a literal RFC 3339 timestamp")
        .with_timezone(&chrono::Utc);
    Json(json!({
        "baseUrl": s.base_url(),
        "version": JIRA_VERSION,
        "versionNumbers": [9, 17, 0],
        "deploymentType": "Server",
        "buildNumber": JIRA_BUILD_NUMBER,
        "buildDate": jira_date(build_date, off),
        "serverTime": jira_date(s.now(), off),
        "scmInfo": "mockd",
        "serverTitle": "Tidewater Jira (mockd)",
    }))
}

async fn myself(State(s): State<Arc<MockState>>) -> Json<Value> {
    let base = s.base_url();
    let username = knobas_source_mock::fixture()
        .person(MYSELF)
        .expect("the fixture has Mara")
        .username
        .clone();
    let mut u = user_json(&base, &username);
    // `myself` carries a little more than the shared `user` definition does.
    if let Some(o) = u.as_object_mut() {
        o.insert("deleted".into(), Value::Bool(false));
        o.insert("locale".into(), Value::String("en_GB".into()));
    }
    Json(u)
}
