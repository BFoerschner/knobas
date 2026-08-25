//! The TeamCity REST router: the endpoints M1's build sync needs, in the shapes
//! the vendored swagger declares.
//!
//! TeamCity publishes no WADL, so there is no generated allowlist here — **the
//! route table is the allowlist**, and anything outside it falls through to a
//! 501 plus an [`Unimplemented`](crate::ViolationKind::Unimplemented)
//! violation, exactly as on the Jira side.
//!
//! ## Order of checks
//!
//! 1. `Accept: application/json` — absent or non-JSON ⇒ **406** (deviation 1);
//! 2. `Authorization` — absent ⇒ 401 (deviation 4, applied to TeamCity too);
//! 3. `fields=` and `locator=`, in the handler because both are route-specific;
//! 4. the injected [`MockFault`](crate::MockFault);
//! 5. serve.
//!
//! Steps 3 and 4 are in that order for the same reason the Jira guard validates
//! before faulting: a test that injected a fault wants the fault path exercised
//! by a *valid* request, and a malformed request must be recorded whatever the
//! server was told to pretend.

use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Path, Request, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde_json::{Value, json};

use crate::state::{MockFault, MockState};
use crate::tc_fields::{self, FieldSel};
use crate::tc_state::{TcBuild, TcBuildType, TcState, tc_date};
use crate::validate::{Violation, ViolationKind, record_violation};

/// The TeamCity version mockd claims to be — the line the vendored swagger was
/// extracted from.
const TC_VERSION: &str = "2025.07.3 (build 187654)";
const TC_BUILD_NUMBER: &str = "187654";

/// The account `GET /app/rest/users/current` reports. One fake company, one
/// seat: the same person Jira's `myself` returns.
const MYSELF: &str = "mara";

/// This router's key in [`MockState`]'s per-API base-URL map.
pub(crate) const API: &str = "teamcity";

/// TeamCity's own default page size when a locator does not give a `count`.
const DEFAULT_COUNT: usize = 100;

pub fn router(state: Arc<MockState>) -> Router {
    Router::new()
        .route("/app/rest/server", get(server))
        .route("/app/rest/users/current", get(current_user))
        .route("/app/rest/buildTypes", get(build_types))
        .route("/app/rest/builds", get(builds))
        .route("/app/rest/builds/{locator}", get(build_by_locator))
        .fallback(unimplemented)
        // Same reasoning as the Jira router: axum's own 405 would carry an
        // `Allow` header describing mockd's routing table, an empty body and no
        // violation at all. The route table is the TeamCity allowlist, so a
        // verb it does not have is as unserved as a path it does not have.
        .method_not_allowed_fallback(unimplemented)
        .layer(axum::middleware::from_fn_with_state(state.clone(), guard))
        .with_state(state.clone())
        // Merged *after* the layer, which is what exempts `/__mock/*` from the
        // product middleware: axum applies a layer to the routes present when
        // it is added, never to ones merged in later.
        .merge(crate::admin::router(state))
}

// -- errors, violations, the guard ------------------------------------------

/// TeamCity answers errors as plain text, not JSON, even to a client that asked
/// for JSON. Reproduced rather than tidied up: an adapter that blindly
/// `.json()`s an error response has a bug worth failing on.
pub(crate) fn tc_error(status: StatusCode, message: impl Into<String>) -> Response {
    let body = format!(
        "Error has occurred during request processing ({}).\n{}\n",
        status.as_u16(),
        message.into()
    );
    (
        status,
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("text/plain;charset=UTF-8"),
        )],
        body,
    )
        .into_response()
}

fn hint(mut r: Response, text: &'static str) -> Response {
    r.headers_mut()
        .insert("X-Mockd-Hint", HeaderValue::from_static(text));
    r
}

/// `Accept: application/json`, then a credential. Everything route-specific
/// happens in the handler.
async fn guard(
    State(state): State<Arc<MockState>>,
    req: Request,
    next: axum::middleware::Next,
) -> Response {
    let accept = req
        .headers()
        .get(header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    // `*/*` deliberately does not count: it is what reqwest sends by default,
    // so this is precisely the guard that forces the header to be explicit.
    let wants_json = accept
        .split(',')
        .any(|p| p.trim().split(';').next().unwrap_or_default() == "application/json");
    if !wants_json {
        record_violation(
            &state,
            &req,
            ViolationKind::MissingHeader,
            format!(
                "Accept: {accept:?} is not application/json; real TeamCity would answer XML here"
            ),
        );
        return hint(
            tc_error(
                StatusCode::NOT_ACCEPTABLE,
                "TeamCity serves XML without an explicit JSON Accept header",
            ),
            "knobas-mockd deviation 1: send Accept: application/json on every TeamCity request; \
             real TeamCity would have answered XML, which no knobas adapter parses",
        );
    }

    let credential = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .trim();
    let has_credential = credential.split_once(' ').is_some_and(|(scheme, value)| {
        (scheme.eq_ignore_ascii_case("Bearer") || scheme.eq_ignore_ascii_case("Basic"))
            && !value.trim().is_empty()
    });
    if !has_credential {
        record_violation(
            &state,
            &req,
            ViolationKind::MissingHeader,
            "no Authorization: Bearer/Basic header".to_owned(),
        );
        return tc_error(StatusCode::UNAUTHORIZED, "Authentication required");
    }

    next.run(req).await
}

/// The fallback for anything the route table does not have.
async fn unimplemented(State(state): State<Arc<MockState>>, req: Request) -> Response {
    state.violations().record(Violation {
        kind: ViolationKind::Unimplemented,
        method: req.method().as_str().to_ascii_uppercase(),
        path: req.uri().path().to_owned(),
        query: req.uri().query().unwrap_or_default().to_owned(),
        detail: "no mock handler for this TeamCity path".to_owned(),
        at: chrono::Utc::now(),
    });
    hint(
        tc_error(
            StatusCode::NOT_IMPLEMENTED,
            format!("Not implemented by mockd: {}", req.uri().path()),
        ),
        "knobas-mockd implements only the M1 TeamCity read subset (interfaces doc section 5); \
         the route table is the allowlist, so this path is not one an M1 adapter may call",
    )
}

fn unsupported(state: &MockState, req: &Request, message: String) -> Response {
    record_violation(state, req, ViolationKind::UnsupportedQuery, message.clone());
    tc_error(StatusCode::BAD_REQUEST, message)
}

fn unknown_field(state: &MockState, req: &Request, name: &str) -> Response {
    let message = format!(
        "Field {name:?} is not one mockd serves. Real TeamCity drops names it does not know; \
         mockd refuses them (deviation 6), because a typo that silently drops a field from a \
         sync is worth a 400."
    );
    record_violation(state, req, ViolationKind::UnknownField, message.clone());
    tc_error(StatusCode::BAD_REQUEST, message)
}

// -- request plumbing --------------------------------------------------------

fn query_map(req: &Request) -> HashMap<String, String> {
    axum::extract::Query::<HashMap<String, String>>::try_from_uri(req.uri())
        .map(|q| q.0)
        .unwrap_or_default()
}

/// `fields=`, parsed. `required` is true for the two collection endpoints,
/// where TeamCity's default projection is so thin that an adapter which forgot
/// the parameter would sync empty objects and never notice.
fn field_sel(q: &HashMap<String, String>, required: bool) -> Result<Option<Vec<FieldSel>>, String> {
    match q
        .get("fields")
        .map(String::as_str)
        .filter(|s| !s.is_empty())
    {
        None if required => Err(
            "fields= is required on this endpoint; TeamCity's default projection is not one \
             any knobas adapter should rely on"
                .to_owned(),
        ),
        None => Ok(None),
        Some(raw) => tc_fields::parse(raw).map(Some),
    }
}

/// Applies `sel` to `full`, or the name that is not in it.
fn project(full: Value, sel: Option<&Vec<FieldSel>>) -> Result<Value, String> {
    match sel {
        None => Ok(strip(full)),
        Some(s) => tc_fields::project(&full, s),
    }
}

fn strip(v: Value) -> Value {
    tc_fields::project(&v, &tc_fields::parse("$long").expect("$long parses")).expect("$long")
}

/// Runs the injected fault, or returns `None` to carry on.
async fn fault(s: &MockState) -> Option<Response> {
    match s.fault() {
        MockFault::None => None,
        MockFault::Unauthorized => Some(tc_error(
            StatusCode::UNAUTHORIZED,
            "Authentication required (injected by mockd)",
        )),
        MockFault::ServerError => Some(tc_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Internal server error (injected by mockd)",
        )),
        MockFault::RateLimited { retry_after_secs } => {
            let mut r = tc_error(
                StatusCode::TOO_MANY_REQUESTS,
                "Rate limit exceeded (injected by mockd)",
            );
            if let Ok(v) = HeaderValue::from_str(&retry_after_secs.to_string()) {
                r.headers_mut().insert(header::RETRY_AFTER, v);
            }
            Some(r)
        }
        MockFault::Timeout { hang_ms } => {
            tokio::time::sleep(std::time::Duration::from_millis(hang_ms)).await;
            None
        }
    }
}

// -- serialisation -----------------------------------------------------------
//
// Every serialiser emits *every* key its object can have, using `null` for the
// ones this instance does not: that is what gives `fields=` a stable set of
// known names to validate against, and `tc_fields::project` drops the nulls
// again, which is TeamCity's own omit-when-absent wire shape.

fn build_type_json(bt: &TcBuildType, base: &str) -> Value {
    json!({
        "id": bt.id,
        "name": bt.name,
        "projectName": bt.project_name,
        "projectId": bt.project_id,
        "href": format!("/app/rest/buildTypes/id:{}", bt.id),
        "webUrl": format!("{base}/viewType.html?buildTypeId={}", bt.id),
    })
}

fn build_json(b: &TcBuild, base: &str, s: &MockState, types: &[TcBuildType]) -> Value {
    let queued = b.state == TcState::Queued;
    let running = b.state == TcState::Running;
    let running_info = running.then(|| {
        json!({
            "percentageComplete": b.percentage_complete,
            "elapsedSeconds": (s.now() - b.start_date).num_seconds().max(0),
            "currentStageText": b.current_stage_text,
        })
    });
    json!({
        "id": b.id,
        "buildTypeId": b.build_type_id,
        "number": b.number,
        "status": b.status.as_str(),
        "state": b.state.as_str(),
        "running": running.then_some(true),
        "percentageComplete": running.then_some(b.percentage_complete).flatten(),
        "branchName": b.branch_name,
        "href": format!("/app/rest/builds/id:{}", b.id),
        "webUrl": format!("{base}/viewLog.html?buildId={}&buildTypeId={}", b.id, b.build_type_id),
        "statusText": b.status_text,
        "queuedDate": tc_date(b.start_date),
        "startDate": (!queued).then(|| tc_date(b.start_date)),
        "finishDate": b.finish_date.map(tc_date),
        "buildType": types
            .iter()
            .find(|t| t.id == b.build_type_id)
            .map(|t| build_type_json(t, base)),
        "running-info": running_info,
    })
}

fn server_json(s: &MockState) -> Value {
    let base = s.base_url(API);
    json!({
        "version": TC_VERSION,
        "versionMajor": 2025,
        "versionMinor": 7,
        "buildNumber": TC_BUILD_NUMBER,
        // Fixed so the body is byte-stable; `currentTime` is the mock's own
        // clock, never the wall clock.
        "buildDate": tc_date(
            chrono::DateTime::parse_from_rfc3339("2026-07-04T09:00:00Z")
                .expect("a literal RFC 3339 timestamp")
                .with_timezone(&chrono::Utc),
        ),
        "startTime": tc_date(knobas_source_mock::fixture().today - chrono::Duration::days(1)),
        "currentTime": tc_date(s.now()),
        "internalId": "mockd",
        "role": "main_node",
        "webUrl": base,
    })
}

fn current_user_json() -> Value {
    let p = knobas_source_mock::fixture()
        .person(MYSELF)
        .expect("the fixture has Mara");
    json!({
        "id": 1,
        "username": p.username,
        "name": p.name,
        // Deliberately the address Jira's `myself` serves: one fake company,
        // one seat. See the deviations list in lib.rs.
        "email": format!("{}@tidewater.example", p.username),
        "href": "/app/rest/users/id:1",
    })
}

// -- handlers ----------------------------------------------------------------

async fn server(State(s): State<Arc<MockState>>, req: Request) -> Response {
    let q = query_map(&req);
    let sel = match field_sel(&q, false) {
        Ok(v) => v,
        Err(m) => return unsupported(&s, &req, m),
    };
    if let Some(r) = fault(&s).await {
        return r;
    }
    match project(server_json(&s), sel.as_ref()) {
        Ok(v) => Json(v).into_response(),
        Err(name) => unknown_field(&s, &req, &name),
    }
}

async fn current_user(State(s): State<Arc<MockState>>, req: Request) -> Response {
    let q = query_map(&req);
    let sel = match field_sel(&q, false) {
        Ok(v) => v,
        Err(m) => return unsupported(&s, &req, m),
    };
    if let Some(r) = fault(&s).await {
        return r;
    }
    match project(current_user_json(), sel.as_ref()) {
        Ok(v) => Json(v).into_response(),
        Err(name) => unknown_field(&s, &req, &name),
    }
}

async fn build_types(State(s): State<Arc<MockState>>, req: Request) -> Response {
    let q = query_map(&req);
    let sel = match field_sel(&q, true) {
        Ok(v) => v,
        Err(m) => return unsupported(&s, &req, m),
    };
    if let Some(r) = fault(&s).await {
        return r;
    }
    let base = s.base_url(API);
    let types = s.build_types();
    let full = json!({
        "count": types.len(),
        "href": "/app/rest/buildTypes",
        "nextHref": Value::Null,
        "buildType": types.iter().map(|t| build_type_json(t, &base)).collect::<Vec<_>>(),
    });
    match project(full, sel.as_ref()) {
        Ok(v) => Json(v).into_response(),
        Err(name) => unknown_field(&s, &req, &name),
    }
}

async fn builds(State(s): State<Arc<MockState>>, req: Request) -> Response {
    let q = query_map(&req);
    let sel = match field_sel(&q, true) {
        Ok(v) => v,
        Err(m) => return unsupported(&s, &req, m),
    };
    let raw_locator = q.get("locator").map(String::as_str).unwrap_or_default();
    let loc = match Locator::parse(raw_locator) {
        Ok(l) => l,
        Err(m) => return unsupported(&s, &req, m),
    };
    if let Some(r) = fault(&s).await {
        return r;
    }
    let base = s.base_url(API);
    let types = s.build_types();
    let hits: Vec<TcBuild> = loc.apply(s.builds());
    let full = json!({
        "count": hits.len(),
        "href": format!("/app/rest/builds?locator={raw_locator}"),
        "nextHref": Value::Null,
        "build": hits.iter().map(|b| build_json(b, &base, &s, &types)).collect::<Vec<_>>(),
    });
    match project(full, sel.as_ref()) {
        Ok(v) => Json(v).into_response(),
        Err(name) => unknown_field(&s, &req, &name),
    }
}

async fn build_by_locator(
    State(s): State<Arc<MockState>>,
    Path(locator): Path<String>,
    req: Request,
) -> Response {
    let q = query_map(&req);
    let sel = match field_sel(&q, false) {
        Ok(v) => v,
        Err(m) => return unsupported(&s, &req, m),
    };
    let Some(id) = locator
        .strip_prefix("id:")
        .and_then(|n| n.parse::<u64>().ok())
    else {
        return unsupported(
            &s,
            &req,
            format!("build locator {locator:?} is not supported; mockd serves id:<number>"),
        );
    };
    if let Some(r) = fault(&s).await {
        return r;
    }
    let Some(b) = s.build(id) else {
        return tc_error(StatusCode::NOT_FOUND, format!("No build found by id {id}"));
    };
    let full = build_json(&b, &s.base_url(API), &s, &s.build_types());
    match project(full, sel.as_ref()) {
        Ok(v) => Json(v).into_response(),
        Err(name) => unknown_field(&s, &req, &name),
    }
}

// -- the build locator -------------------------------------------------------

/// The locator dimensions mockd supports. Anything else is a 400 plus an
/// [`UnsupportedQuery`](ViolationKind::UnsupportedQuery) violation: an adapter
/// that sends a dimension mockd ignores would be tested against a filter that
/// silently did nothing.
#[derive(Debug, PartialEq, Eq)]
struct Locator {
    since_build: Option<u64>,
    /// `None` when the locator did not say — which is what makes
    /// `default_filter` observable.
    states: Option<Vec<TcState>>,
    build_type: Option<String>,
    count: usize,
    start: usize,
    default_filter: bool,
}

const SUPPORTED: &str = "sinceBuild:(id:N), state:queued|running|finished|any, \
     state:(queued:true,running:true,finished:true), buildType:X, buildType:(id:X), \
     count:N, start:N, defaultFilter:false";

impl Locator {
    fn parse(raw: &str) -> Result<Self, String> {
        let mut out = Self {
            since_build: None,
            states: None,
            build_type: None,
            count: DEFAULT_COUNT,
            start: 0,
            default_filter: true,
        };
        let mut seen: Vec<&str> = Vec::new();
        for item in split_top_level(raw)? {
            let (key, value) = item
                .split_once(':')
                .ok_or_else(|| format!("locator part {item:?} is not <dimension>:<value>"))?;
            if seen.contains(&key) {
                return Err(if key == "state" {
                    "locator dimension \"state\" appears more than once. Real TeamCity does not \
                     accept state:running,state:queued -- the combined in-flight filter is \
                     state:(queued:true,running:true)."
                        .to_owned()
                } else {
                    format!("locator dimension {key:?} appears more than once")
                });
            }
            seen.push(key);
            match key {
                "sinceBuild" => {
                    out.since_build = Some(
                        value
                            .strip_prefix('(')
                            .and_then(|v| v.strip_suffix(')'))
                            .and_then(|v| v.strip_prefix("id:"))
                            .and_then(|n| n.parse().ok())
                            .ok_or_else(|| {
                                format!(
                                    "sinceBuild:{value} is not supported; mockd serves \
                                     sinceBuild:(id:N)"
                                )
                            })?,
                    );
                }
                "state" => out.states = Some(parse_states(value)?),
                "buildType" => {
                    let id = value
                        .strip_prefix('(')
                        .and_then(|v| v.strip_suffix(')'))
                        .map_or(Ok(value), |inner| {
                            inner.strip_prefix("id:").ok_or_else(|| {
                                format!(
                                    "buildType:{value} is not supported; mockd serves \
                                     buildType:X and buildType:(id:X)"
                                )
                            })
                        })?;
                    out.build_type = Some(id.to_owned());
                }
                "count" => {
                    out.count = value
                        .parse()
                        .map_err(|_| format!("count:{value} is not a non-negative integer"))?;
                }
                "start" => {
                    out.start = value
                        .parse()
                        .map_err(|_| format!("start:{value} is not a non-negative integer"))?;
                }
                "defaultFilter" => {
                    out.default_filter = value
                        .parse()
                        .map_err(|_| format!("defaultFilter:{value} is not true or false"))?;
                }
                other => {
                    return Err(format!(
                        "locator dimension {other:?} is not one mockd supports. Supported: \
                         {SUPPORTED}"
                    ));
                }
            }
        }
        Ok(out)
    }

    /// The states this locator selects.
    ///
    /// Real TeamCity's `defaultFilter` returns only finished, non-personal,
    /// non-canceled builds. Reproducing it exactly is the entire reason
    /// interfaces §4.2 requires an unconditional in-flight poll each run.
    fn states(&self) -> Vec<TcState> {
        self.states.clone().unwrap_or({
            if self.default_filter {
                vec![TcState::Finished]
            } else {
                vec![TcState::Queued, TcState::Running, TcState::Finished]
            }
        })
    }

    fn apply(&self, all: Vec<TcBuild>) -> Vec<TcBuild> {
        let states = self.states();
        all.into_iter()
            .filter(|b| states.contains(&b.state))
            .filter(|b| {
                self.build_type
                    .as_ref()
                    .is_none_or(|t| &b.build_type_id == t)
            })
            .filter(|b| self.since_build.is_none_or(|n| b.id > n))
            .skip(self.start)
            .take(self.count)
            .collect()
    }
}

fn parse_states(value: &str) -> Result<Vec<TcState>, String> {
    let all = [TcState::Queued, TcState::Running, TcState::Finished];
    if let Some(inner) = value.strip_prefix('(').and_then(|v| v.strip_suffix(')')) {
        let mut out = Vec::new();
        for part in split_top_level(inner)? {
            let (name, flag) = part
                .split_once(':')
                .ok_or_else(|| format!("state part {part:?} is not <state>:<true|false>"))?;
            let Some(st) = all.iter().copied().find(|s| s.as_str() == name) else {
                return Err(format!(
                    "state:{name} is not one of queued, running, finished"
                ));
            };
            match flag {
                "true" => out.push(st),
                "false" => {}
                other => return Err(format!("state:({name}:{other}) is not true or false")),
            }
        }
        return Ok(out);
    }
    if value == "any" {
        return Ok(all.to_vec());
    }
    all.iter()
        .copied()
        .find(|s| s.as_str() == value)
        .map(|s| vec![s])
        .ok_or_else(|| {
            format!(
                "state:{value} is not supported; mockd serves \
                 state:queued|running|finished|any and state:(queued:true,running:true)"
            )
        })
}

/// Splits on commas that are not inside parentheses.
fn split_top_level(raw: &str) -> Result<Vec<&str>, String> {
    let mut out = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    for (i, c) in raw.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth = depth
                    .checked_sub(1)
                    .ok_or_else(|| format!("unbalanced ')' in locator {raw:?}"))?;
            }
            ',' if depth == 0 => {
                out.push(&raw[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    if depth != 0 {
        return Err(format!("unbalanced '(' in locator {raw:?}"));
    }
    let tail = &raw[start..];
    if !tail.is_empty() {
        out.push(tail);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_locator_is_finished_only() {
        assert_eq!(
            Locator::parse("").unwrap().states(),
            vec![TcState::Finished]
        );
        assert_eq!(
            Locator::parse("defaultFilter:false").unwrap().states(),
            vec![TcState::Queued, TcState::Running, TcState::Finished]
        );
    }

    #[test]
    fn the_nested_state_spelling_parses_and_the_repeated_one_does_not() {
        assert_eq!(
            Locator::parse("state:(queued:true,running:true)")
                .unwrap()
                .states(),
            vec![TcState::Queued, TcState::Running]
        );
        let err = Locator::parse("state:running,state:queued").unwrap_err();
        assert!(err.contains("state:(queued:true,running:true)"), "{err}");
    }

    #[test]
    fn a_false_flag_deselects() {
        assert_eq!(
            Locator::parse("state:(queued:true,running:false)")
                .unwrap()
                .states(),
            vec![TcState::Queued]
        );
    }

    #[test]
    fn commas_inside_parentheses_do_not_split() {
        assert_eq!(
            split_top_level("sinceBuild:(id:5),state:(a:true,b:true),count:2").unwrap(),
            ["sinceBuild:(id:5)", "state:(a:true,b:true)", "count:2"]
        );
        assert!(split_top_level("state:(a:true").is_err());
    }
}
