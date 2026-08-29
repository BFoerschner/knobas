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
use axum::routing::{get, post};
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
        // M2's write-back (issue #43): triggering a build and re-running one
        // are the same request to TeamCity -- both put a build on the queue.
        .route("/app/rest/buildQueue", post(queue_build))
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

/// TeamCity's error envelope, as a real server serves it to a client that
/// asked for JSON.
///
/// ```json
/// {"errors":[{"message":"No build found by id '999999999'.",
///             "additionalMessage":"jetbrains.buildServer.server.rest.errors.NotFoundException: No build found by id '999999999'.",
///             "statusText":"Responding with error, status code: 404 (Not Found).",
///             "stackTrace":null}]}
/// ```
///
/// Transcribed from JetBrains' public instance (2026.2 EAP, build 238763),
/// read-only on 2026-08-29, on a 400, a 404 and a 406.
///
/// **This used to be `text/plain`**, in the shape `Error has occurred during
/// request processing (404).\n<message>\n`, on a comment asserting that a real
/// TeamCity answers errors as plain text "even to a client that asked for
/// JSON". It does not, and has not for a long time. That server
/// content-negotiates: with `Accept: application/json` -- which `knobas-http`
/// sets on every request -- the body is the envelope above; with
/// `application/xml`, with `*/*`, or with no `Accept` at all it is **XML**
/// (which is deviation 1 below, and that comment is the accurate one); and
/// only with an `Accept` the server cannot satisfy at all (`text/plain`,
/// `text/html`) is the answer a **406**, whose body is itself the envelope.
/// The plain-text form reached nobody either way, and `http::error_message` was
/// written to require it, so it parsed no error the adapter would ever be
/// handed and every failure rendered as a raw blob (issue #113).
///
/// That is the standing rule doing its work: where the fake and the server
/// disagree the **fake** is wrong. Leaving it would let the next reader derive
/// the same wrong shape from the same green suite.
///
/// `additionalMessage` and `statusText` are carried because the real server
/// carries them, and an adapter that lifted either would be reading noise it
/// should not -- which is a thing worth being able to fail on. `stackTrace` is
/// `null` for the same reason: it is a key a real answer has.
pub(crate) fn tc_error(status: StatusCode, message: impl Into<String>) -> Response {
    let message = message.into();
    let reason = status.canonical_reason().unwrap_or("Error");
    let body = json!({
        "errors": [{
            "message": message,
            "additionalMessage": format!(
                "jetbrains.buildServer.server.rest.errors.{}: {message}",
                exception_for(status)
            ),
            "statusText": format!(
                "Responding with error, status code: {} ({reason}).",
                status.as_u16()
            ),
            "stackTrace": Value::Null,
        }],
    });
    (
        status,
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        )],
        body.to_string(),
    )
        .into_response()
}

/// The class name a real TeamCity puts in front of `additionalMessage`.
///
/// `tc_error` is reached with seven statuses -- 404, 400 and 401 from the
/// routes and guards, 406 from the `Accept` guard, 501 from the unimplemented
/// fallback, and 500 and 429 from injected faults. Only the three whose real
/// class name was transcribed from a live server are named here; every other
/// status gets the generic `OperationException` rather than an invented class,
/// on the same rule as the fixture's people: a name that is not a real
/// TeamCity's is a name an adapter could come to depend on. The `FORBIDDEN`
/// arm rides with `UNAUTHORIZED` because they are one refusal to this mock,
/// which produces neither on its own.
fn exception_for(status: StatusCode) -> &'static str {
    match status {
        StatusCode::NOT_FOUND => "NotFoundException",
        StatusCode::BAD_REQUEST => "BadRequestException",
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => "AuthorizationFailedException",
        _ => "OperationException",
    }
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
        // No fixture configuration has prose, so `description` is normally the
        // null that means "known name, absent here";
        // `MockState::describe_build_type` is what puts one there. `paused` is
        // a genuine `false`: nothing in the fixture is paused, and a
        // configuration that is merely quiet is not a paused one.
        "description": bt.description,
        "paused": false,
    })
}

fn build_json(b: &TcBuild, base: &str, s: &MockState, types: &[TcBuildType]) -> Value {
    let triggerer = b
        .triggered_by
        .as_deref()
        .and_then(|id| knobas_source_mock::fixture().person(id));
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
        // `triggered` is the only place TeamCity names the person who started
        // a build, and it is what `knobas-source-teamcity` reads for
        // `SyncItem::author`.
        //
        // Who that is comes from the fixture and nowhere else: a person
        // invented here would flow straight into `author` and be indexed and
        // searched as if the dataset had said it. Where the fixture names
        // nobody the trigger is `vcs` with no `user` at all, which is exactly
        // what a real server serves for a branch build.
        "triggered": {
            "type": if triggerer.is_some() { "user" } else { "vcs" },
            "date": tc_date(b.start_date),
            "user": triggerer.map(|p| json!({ "username": p.username, "name": p.name })),
        },
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
        // Never paged, and so never a next page: this endpoint takes no
        // locator here, and a real TeamCity answers it whole -- 4,253 build
        // configurations in one response with no `nextHref`, measured
        // read-only against JetBrains' public instance on 2026-08-29.
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
    let (hits, more) = loc.apply(s.builds());
    let full = json!({
        "count": hits.len(),
        "href": format!("/app/rest/builds?locator={raw_locator}"),
        // Present exactly when the page came back filled, which is what a
        // real TeamCity answers and the only thing in the response that tells
        // a capped page from an exhausted query (issue #114). The href is the
        // offset continuation the real server serves, and it is one this
        // server itself accepts -- see [`continuation`].
        "nextHref": if more {
            json!(format!(
                "/app/rest/builds?locator={}",
                continuation(raw_locator, loc.start + hits.len())
            ))
        } else {
            Value::Null
        },
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
    /// `canceled:`, `None` when the locator did not name it — which is what
    /// leaves the default filter in charge of that facet.
    canceled: Option<Facet>,
    /// `failedToStart:`, same rule.
    failed_to_start: Option<Facet>,
}

/// What a facet dimension (`canceled:`, `failedToStart:`) asks for.
///
/// Three values and not a boolean, because `any` is the one the adapter sends
/// and it is **not** `true`: `canceled:true` asks for canceled builds and
/// nothing else, `canceled:any` says the dimension does not narrow at all, so
/// the page carries both classes. A mock that read `any` as `true` would serve
/// a page of nothing but canceled builds and an adapter would look correct
/// while asking the wrong question (issue #105).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Facet {
    Any,
    Only,
    Never,
}

impl Facet {
    fn parse(dimension: &str, value: &str) -> Result<Self, String> {
        match value {
            "any" => Ok(Self::Any),
            "true" => Ok(Self::Only),
            "false" => Ok(Self::Never),
            other => Err(format!("{dimension}:{other} is not true, false or any")),
        }
    }

    /// Does a build carrying (or not carrying) the flag pass this dimension?
    fn admits(self, flagged: bool) -> bool {
        match self {
            Self::Any => true,
            Self::Only => flagged,
            Self::Never => !flagged,
        }
    }
}

const SUPPORTED: &str = "sinceBuild:(id:N), state:queued|running|finished|any, \
     state:(queued:true,running:true,finished:true), buildType:X, buildType:(id:X), \
     count:N, start:N, defaultFilter:false, canceled:any|true|false, \
     failedToStart:any|true|false";

impl Locator {
    fn parse(raw: &str) -> Result<Self, String> {
        let mut out = Self {
            since_build: None,
            states: None,
            build_type: None,
            count: DEFAULT_COUNT,
            start: 0,
            default_filter: true,
            canceled: None,
            failed_to_start: None,
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
                "canceled" => out.canceled = Some(Facet::parse("canceled", value)?),
                "failedToStart" => {
                    out.failed_to_start = Some(Facet::parse("failedToStart", value)?);
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

    /// Does this locator admit `b`'s **facets** — canceled, failed-to-start?
    ///
    /// A separate question from [`Self::states`], and the one issue #105 turns
    /// on. TeamCity's default filter hides three classes — canceled,
    /// failed-to-start and personal — and it goes on hiding them **when
    /// `state:` is set**, which is why every query the TeamCity adapter used
    /// to emit items from silently missed the first two. mockd could not show
    /// that: it applied the default filter to the *states* alone, so a
    /// `state:finished` page here carried canceled builds that the real server
    /// hides. Part of how #105 survived a green suite.
    ///
    /// The dimensions re-open one facet each and leave the others alone;
    /// `defaultFilter:false` opens all of them at once. Measured read-only
    /// against JetBrains' public instance (2026.2 EAP) on 2026-08-29:
    /// `state:finished,canceled:any,count:100` answered one canceled build and
    /// no failed-to-start one, while `state:finished,defaultFilter:false,
    /// count:100` answered both.
    ///
    /// mockd has no personal builds — the Tidewater dataset has no vocabulary
    /// for one, and nothing in knobas asks for them — so that facet of the
    /// real filter is a documented absence here rather than a rule.
    fn admits_facets(&self, b: &TcBuild) -> bool {
        let canceled = self.canceled.unwrap_or(if self.default_filter {
            Facet::Never
        } else {
            Facet::Any
        });
        let failed_to_start = self.failed_to_start.unwrap_or(if self.default_filter {
            Facet::Never
        } else {
            Facet::Any
        });
        canceled.admits(b.canceled) && failed_to_start.admits(b.failed_to_start)
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

    /// Filters, orders **newest first**, then pages -- and says whether the
    /// paging left anything behind.
    ///
    /// The order is part of the contract, not a presentation detail:
    /// `/app/rest/builds` answers newest-first on a real server, so `count:1`
    /// is "the newest build" and a full page drops the *oldest* matches. An
    /// adapter reads both of those as meaning, and the vendored swagger cannot
    /// see either -- it validates the shape of a response, never the order of
    /// a collection. `start:`/`count:` page over this order, so they page the
    /// same way here as they do in production.
    ///
    /// The second half of the return is `nextHref`'s: **whether this page was
    /// filled to the number of rows the server put on it**, which is the one
    /// statement a real server makes about the rest of the collection. A fake
    /// that always answered "no more" taught adapters to infer the end from a
    /// page's *length* instead -- sound only against a server that serves
    /// exactly the `count:` it was asked for. That inference is issue #114, in
    /// the TeamCity adapter, and mockd answering `nextHref: null` on every
    /// truncated page is part of how it survived.
    ///
    /// **Filled, not "has a successor"** -- and the difference is measurable.
    /// Read-only against JetBrains' public instance, 2026-08-29, over a query
    /// with exactly 42 matches: `count:41` and `count:42` both answered a
    /// `nextHref`, `count:43` answered none. So a page filled to its limit is
    /// reported as continuing whether or not anything follows it, and only a
    /// page the server could not fill ends a collection. Reproduced rather
    /// than improved on: a mock that resolved the ambiguity the real server
    /// leaves would let an adapter depend on a promise TeamCity does not make.
    fn apply(&self, all: Vec<TcBuild>) -> (Vec<TcBuild>, bool) {
        let states = self.states();
        let mut hits: Vec<TcBuild> = all
            .into_iter()
            .filter(|b| states.contains(&b.state))
            .filter(|b| self.admits_facets(b))
            .filter(|b| {
                self.build_type
                    .as_ref()
                    .is_none_or(|t| &b.build_type_id == t)
            })
            .filter(|b| self.since_build.is_none_or(|n| b.id > n))
            .collect();
        // Sorted rather than reversed: `crate::state::MockState::builds`
        // happens to hand these over ascending, and a `reverse()` would depend
        // on that silently.
        hits.sort_by_key(|b| std::cmp::Reverse(b.id));
        let page: Vec<TcBuild> = hits.into_iter().skip(self.start).take(self.count).collect();
        let more = self.count > 0 && page.len() == self.count;
        (page, more)
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
/// The locator a filled page's `nextHref` names: the request's own, with
/// `start:` advanced past the rows just served.
///
/// **Replaced, not appended**, which is the difference between a continuation
/// and a 400. [`Locator::parse`] refuses a locator that names a dimension
/// twice, so appending `,start:N` to a request that already carried a `start:`
/// would produce a `nextHref` this very server rejects -- a link nothing can
/// follow, which is not what a real TeamCity serves and would quietly undo the
/// point of answering the field at all. Split at the top level so the commas
/// inside a nested value (`state:(queued:true,running:true)`) survive.
fn continuation(raw_locator: &str, start: usize) -> String {
    let kept: Vec<&str> = split_top_level(raw_locator)
        .unwrap_or_default()
        .into_iter()
        .filter(|p| !p.starts_with("start:"))
        .collect();
    if kept.is_empty() {
        format!("start:{start}")
    } else {
        format!("{},start:{start}", kept.join(","))
    }
}

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

// -- the build queue (issue #43) ---------------------------------------------

/// `POST /app/rest/buildQueue` -- put a build on the queue.
///
/// The one write TeamCity's REST API needs for both of M2's ratified ops:
/// **triggering and re-running are the same request**, differing only in how
/// the caller found the build configuration. A real server answers **200** with
/// the queued `Build`, whose `state` is `queued` and whose `id` is new.
///
/// `buildType.id` is the only field mockd requires, which is also the minimum a
/// real TeamCity accepts. A body without it is a 400 rather than a build of
/// something arbitrary -- a mock that guessed the configuration would let an
/// adapter ship a request that triggers whatever the server felt like.
async fn queue_build(State(s): State<Arc<MockState>>, req: Request) -> Response {
    let bytes = match axum::body::to_bytes(req.into_body(), 64 * 1024).await {
        Ok(bytes) => bytes,
        Err(_) => return tc_error(StatusCode::BAD_REQUEST, "Could not read the request body"),
    };
    let parsed: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    let requested = parsed
        .get("buildType")
        .and_then(|t| t.get("id"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|id| !id.is_empty());
    let Some(build_type_id) = requested else {
        return tc_error(
            StatusCode::BAD_REQUEST,
            "No build type specified. Please specify build type as buildType.id",
        );
    };
    if !s.build_types().iter().any(|t| t.id == build_type_id) {
        return tc_error(
            StatusCode::NOT_FOUND,
            format!("No build type found by id {build_type_id:?}."),
        );
    }
    if let Some(r) = fault(&s).await {
        return r;
    }

    // `branchName` is optional and TeamCity builds the default branch without
    // one -- which is what an adapter that does not model branches must be
    // able to rely on.
    let branch = parsed
        .get("branchName")
        .and_then(Value::as_str)
        .filter(|b| !b.trim().is_empty())
        .unwrap_or("refs/heads/main");

    let id = s.queue_build(build_type_id, branch);
    let queued = s.build(id).expect("the build was just queued");
    // The whole record, unprojected: this endpoint takes no `fields=`, and a
    // real server answers the created build in full.
    Json(build_json(&queued, &s.base_url(API), &s, &s.build_types())).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_page_reports_a_next_one_when_it_came_back_filled() {
        let builds: Vec<TcBuild> = crate::state::MockState::from_fixture().builds();
        let total = builds.len();
        assert!(total >= 3, "the fixture needs a few builds: {total}");

        let page = |locator: &str| {
            Locator::parse(locator)
                .unwrap_or_else(|e| panic!("{locator}: {e}"))
                .apply(builds.clone())
        };

        // Filled to the count asked for: a next page is reported whether or
        // not one is there. That second half is not mockd being lazy -- it is
        // what a real TeamCity answers, measured over a query with exactly 42
        // matches, where `count:42` still carried a `nextHref`.
        let (rows, more) = page(&format!("state:any,count:{}", total - 1));
        assert_eq!(rows.len(), total - 1);
        assert!(more, "a page that could not fit the rest reports the rest");
        let (rows, more) = page(&format!("state:any,count:{total}"));
        assert_eq!(rows.len(), total);
        assert!(
            more,
            "a page filled to its limit is not proof it is the last"
        );

        // Not filled: the one answer that ends a collection.
        let (rows, more) = page(&format!("state:any,count:{}", total + 1));
        assert_eq!(rows.len(), total);
        assert!(!more, "a page the server could not fill is the end");

        // ...and `start:` pages over the same order, so the continuation the
        // `nextHref` names actually leads somewhere.
        let (rows, more) = page(&format!("state:any,count:1,start:{}", total - 1));
        assert_eq!(rows.len(), 1);
        assert!(
            more,
            "TeamCity reports a next page off the page being filled, not off what remains"
        );
        let (rows, more) = page(&format!("state:any,count:2,start:{}", total - 1));
        assert_eq!(rows.len(), 1, "one row left after skipping the rest");
        assert!(!more);
    }

    /// A `nextHref` has to be followable, and following it twice has to work:
    /// the second hop is the one that would repeat `start:`.
    ///
    /// [`Locator::parse`] refuses a repeated dimension, so a continuation built
    /// by appending `,start:N` to the request's own locator is a link this
    /// server answers 400 to as soon as the request it continues already
    /// carried one. That is a fake advertising a page it will not serve --
    /// exactly the kind of gap issue #114 was, arriving from the other side.
    #[test]
    fn the_next_page_a_filled_one_names_is_a_locator_this_server_accepts() {
        let builds: Vec<TcBuild> = crate::state::MockState::from_fixture().builds();
        let total = builds.len();
        assert!(total >= 3, "the fixture needs a few builds: {total}");

        // Walk the whole collection one row at a time, following only what the
        // `nextHref` names, and never parse a locator this server would refuse.
        let mut locator = "state:any,count:1".to_owned();
        let mut seen = Vec::new();
        for _ in 0..total {
            let parsed = Locator::parse(&locator)
                .unwrap_or_else(|e| panic!("the continuation {locator:?} has to parse: {e}"));
            let (rows, more) = parsed.apply(builds.clone());
            assert_eq!(rows.len(), 1, "one row per hop, from {locator:?}");
            seen.push(rows[0].id);
            assert!(more, "there is still more after {locator:?}");
            locator = continuation(&locator, parsed.start + rows.len());
        }
        assert_eq!(seen.len(), total, "every row, once");
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), total, "and no row twice");

        // The dimension is replaced rather than repeated, and a nested value's
        // own commas are not a split point.
        assert_eq!(
            continuation("state:(queued:true,running:true),count:2,start:4", 6),
            "state:(queued:true,running:true),count:2,start:6"
        );
        assert_eq!(continuation("", 3), "start:3");
    }

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
