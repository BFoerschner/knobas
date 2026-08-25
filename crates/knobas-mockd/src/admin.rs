//! `/__mock/*` — the out-of-band control plane.
//!
//! In-process, a test reaches [`MockState`] directly and never needs this. In
//! **compose** mode there is no such handle: mockd is a container and the thing
//! driving it is a shell script or another process entirely. Every mutator the
//! typed API offers therefore has an HTTP twin here, and
//! `GET /__mock/violations` is how a compose-mode end-to-end run asserts the
//! same fidelity an in-process test gets from `assert_no_violations()`.
//!
//! The admin router is **merged after** each product router's middleware layer,
//! so `/__mock/*` needs no `Authorization` and no `Accept: application/json`,
//! and can never record a violation against itself: it is not part of any
//! vendored contract, so there is nothing for it to violate.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{Value, json};

use crate::state::{MockFault, MockState};

pub fn router(state: Arc<MockState>) -> Router {
    Router::new()
        .route("/__mock/health", get(health))
        .route("/__mock/reset", post(reset))
        .route("/__mock/fault", post(fault))
        .route("/__mock/config", post(config))
        .route("/__mock/jira/issue/{key}/touch", post(touch))
        .route("/__mock/violations", get(violations).delete(clear))
        .with_state(state)
}

/// The compose healthcheck. `fixture_today` is in the body so a bring-up script
/// can tell a stale image from a current one without shelling into it.
async fn health() -> Json<Value> {
    Json(json!({
        "ok": true,
        "apis": ["jira", "teamcity"],
        "fixture_today": knobas_source_mock::fixture()
            .today
            .to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
    }))
}

async fn reset(State(s): State<Arc<MockState>>) -> Json<Value> {
    s.reset();
    Json(json!({ "ok": true }))
}

async fn fault(State(s): State<Arc<MockState>>, Json(f): Json<MockFault>) -> Json<Value> {
    s.set_fault(f.clone());
    Json(json!({ "ok": true, "fault": f }))
}

/// The knobs a compose-mode driver can turn. Unknown keys are refused rather
/// than ignored: a driver that misspells one would otherwise believe it had
/// configured something.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    jira_max_results_cap: Option<u32>,
    server_offset_secs: Option<i32>,
}

async fn config(State(s): State<Arc<MockState>>, body: Json<Value>) -> Response {
    let cfg: Config = match serde_json::from_value(body.0) {
        Ok(c) => c,
        Err(e) => return (StatusCode::BAD_REQUEST, format!("{e}\n")).into_response(),
    };
    if let Some(cap) = cfg.jira_max_results_cap {
        s.set_max_results_cap(cap);
    }
    if let Some(secs) = cfg.server_offset_secs {
        let Some(off) = chrono::FixedOffset::east_opt(secs) else {
            return (
                StatusCode::BAD_REQUEST,
                format!("server_offset_secs={secs} is not a valid UTC offset\n"),
            )
                .into_response();
        };
        s.set_server_offset(off);
    }
    Json(json!({ "ok": true })).into_response()
}

async fn touch(State(s): State<Arc<MockState>>, Path(key): Path<String>) -> Response {
    // 404 rather than a cheerful 200: `touch_issue` is a deliberate no-op for a
    // key the fixture does not have, and a driver that typed one needs to see
    // that nothing happened.
    if s.issue(&key).is_none() {
        return (
            StatusCode::NOT_FOUND,
            format!("no issue {key} in the fixture\n"),
        )
            .into_response();
    }
    s.touch_issue(&key);
    Json(json!({ "ok": true, "key": key, "updated": s.issue(&key).map(|i| i.updated) }))
        .into_response()
}

async fn violations(State(s): State<Arc<MockState>>) -> Json<Value> {
    Json(json!(s.violations().snapshot()))
}

async fn clear(State(s): State<Arc<MockState>>) -> Json<Value> {
    s.violations().clear();
    Json(json!({ "ok": true }))
}
