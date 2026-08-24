//! The contract-violation model: what mockd records when a client asks for
//! something the vendored API contract does not define.
//!
//! A violation is not an error response — the response is separate, and is
//! whatever the real API would answer. It is the *record* that lets an
//! adapter's own test suite fail on a request the real server would have
//! shrugged at (or answered by accident).

use std::sync::{Arc, Mutex};

use axum::extract::{Request, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use chrono::{DateTime, Utc};

use crate::allowlist;
use crate::state::{MockFault, MockState};

/// One request that the vendored contract does not define, as mockd saw it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violation {
    pub kind: ViolationKind,
    /// Upper-case HTTP verb.
    pub method: String,
    /// The request path, with the `/rest/` prefix still on it — the violation
    /// is reported in the client's own spelling, not mockd's internal one.
    pub path: String,
    /// The raw query string, without the leading `?`.
    pub query: String,
    /// What was wrong, in words a reader of the failing test can act on.
    pub detail: String,
    pub at: DateTime<Utc>,
}

/// Why a request violated the contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViolationKind {
    /// The path is not in the contract at all (a Cloud-dialect path, say).
    UnknownPath,
    /// The path is in the contract, this verb on it is not.
    UnknownMethod,
    /// A query parameter the contract does not declare for this route.
    UnknownQueryParam,
    /// A `fields=`/`expand=` name outside the closed set mockd serves.
    UnknownField,
    /// Syntactically fine, but outside the JQL subset mockd implements.
    UnsupportedQuery,
    /// A header the API requires on every request was absent.
    MissingHeader,
    /// The contract defines this endpoint; mockd has no handler for it.
    Unimplemented,
}

/// Every [`Violation`] one mock server recorded, oldest first.
///
/// Cheap to share: [`record`](ViolationLog::record) takes `&self`, so the
/// router holds one behind an `Arc` and the test holds the same one.
#[derive(Debug, Default)]
pub struct ViolationLog(Mutex<Vec<Violation>>);

impl ViolationLog {
    pub fn record(&self, v: Violation) {
        self.lock().push(v);
    }

    /// Everything recorded so far, oldest first.
    pub fn snapshot(&self) -> Vec<Violation> {
        self.lock().clone()
    }

    pub fn clear(&self) {
        self.lock().clear();
    }

    /// Panics naming every recorded violation, or returns if there are none.
    ///
    /// `context` names the server (`"jira"`, `"teamcity"`), so a failure from
    /// a multi-server cluster says which one was misused.
    ///
    /// # Panics
    ///
    /// If any violation has been recorded.
    pub fn assert_empty(&self, context: &str) {
        let v = self.snapshot();
        if v.is_empty() {
            return;
        }
        let mut msg = format!("{context}: {} contract violation(s):", v.len());
        for x in &v {
            msg.push_str(&format!(
                "\n  {:?} {} {}?{}: {}",
                x.kind, x.method, x.path, x.query, x.detail
            ));
        }
        panic!("{msg}");
    }

    /// The same poisoning rationale as `MockSource::lock`: the guarded value is
    /// a plain `Vec` with no invariant to violate, and a test that already
    /// panicked should fail on its own assertion rather than on a second, less
    /// informative one.
    fn lock(&self) -> std::sync::MutexGuard<'_, Vec<Violation>> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }
}

// -- the Jira request guard -------------------------------------------------

/// The Jira error body, in the shape a real instance returns.
pub(crate) fn jira_error(status: StatusCode, message: impl Into<String>) -> Response {
    let body = serde_json::json!({ "errorMessages": [message.into()], "errors": {} });
    (status, axum::Json(body)).into_response()
}

/// The 401 a real Jira DC answers with when Seraph rejects the request.
pub(crate) fn seraph_401(message: &str) -> Response {
    let mut r = jira_error(StatusCode::UNAUTHORIZED, message);
    r.headers_mut().insert(
        "X-Seraph-LoginReason",
        HeaderValue::from_static("AUTHENTICATED_FAILED"),
    );
    r
}

/// Validates every Jira request against the generated contract tables, then
/// applies the configured fault, then dispatches.
///
/// The order is fixed, because it decides which failure an adapter sees:
///
/// 1. strip `/rest/` — absent ⇒ [`ViolationKind::UnknownPath`], 404;
/// 2. [`allowlist::lookup`] — `NotFound` ⇒ 404, `MethodNotAllowed` ⇒ 405 + `Allow`;
/// 3. every query key must be declared for the route ⇒ else 400;
/// 4. `Authorization` must be present and non-empty ⇒ else Seraph 401;
/// 5. the configured [`MockFault`];
/// 6. dispatch (an unhandled contract path falls through to a 501).
///
/// Validation runs **before** the fault so that a malformed request under an
/// injected fault is still logged: a test that set `Unauthorized` wants the 401
/// path exercised by a *valid* request.
pub(crate) async fn jira_guard(
    State(state): State<Arc<MockState>>,
    req: Request,
    next: Next,
) -> Response {
    let method = req.method().as_str().to_ascii_uppercase();
    let path = req.uri().path().to_owned();
    let query = req.uri().query().unwrap_or_default().to_owned();
    let record = |kind: ViolationKind, detail: String| {
        state.violations().record(Violation {
            kind,
            method: method.clone(),
            path: path.clone(),
            query: query.clone(),
            detail,
            at: Utc::now(),
        });
    };

    // 1. The contract's paths are all relative to `/rest/`.
    let Some(rest) = path.strip_prefix("/rest/") else {
        record(
            ViolationKind::UnknownPath,
            "every Jira REST path lives under /rest/".to_owned(),
        );
        return jira_error(StatusCode::NOT_FOUND, format!("No such resource: {path}"));
    };

    // 2. Path and verb.
    let allowed_query = match allowlist::lookup(&method, rest) {
        allowlist::Lookup::Allowed { query } => query,
        allowlist::Lookup::MethodNotAllowed { allowed } => {
            record(
                ViolationKind::UnknownMethod,
                format!(
                    "{method} is not declared for {rest}; allowed: {}",
                    allowed.join(", ")
                ),
            );
            let mut r = jira_error(
                StatusCode::METHOD_NOT_ALLOWED,
                format!("{method} is not supported for {path}"),
            );
            let allow = allowed.join(", ");
            if let Ok(v) = HeaderValue::from_str(&allow) {
                r.headers_mut().insert(header::ALLOW, v);
            }
            return r;
        }
        allowlist::Lookup::NotFound => {
            record(
                ViolationKind::UnknownPath,
                format!("{rest} is not in testenv/specs/jira-dc-rest.wadl"),
            );
            return jira_error(StatusCode::NOT_FOUND, format!("No such resource: {path}"));
        }
    };

    // 3. Query parameters. Stricter than real Jira on purpose (deviation 3).
    let undeclared: Vec<&str> = query
        .split('&')
        .filter(|kv| !kv.is_empty())
        .map(|kv| kv.split_once('=').map_or(kv, |(k, _)| k))
        .filter(|k| !allowed_query.contains(k))
        .collect();
    if !undeclared.is_empty() {
        record(
            ViolationKind::UnknownQueryParam,
            format!(
                "{rest} declares no query parameter(s) {}; the WADL allows: {}",
                undeclared.join(", "),
                allowed_query.join(", ")
            ),
        );
        return jira_error(
            StatusCode::BAD_REQUEST,
            format!("Unknown query parameter(s): {}", undeclared.join(", ")),
        );
    }

    // 4. Credentials, on every endpoint (deviation 4).
    let credential = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .trim();
    let has_credential = credential
        .strip_prefix("Bearer ")
        .or_else(|| credential.strip_prefix("Basic "))
        .is_some_and(|c| !c.trim().is_empty());
    if !has_credential {
        record(
            ViolationKind::MissingHeader,
            "no Authorization: Bearer/Basic header".to_owned(),
        );
        return seraph_401("You do not have the permission to see the specified issue.");
    }

    // 5. The injected fault, if any.
    match state.fault() {
        MockFault::None => {}
        MockFault::Unauthorized => {
            return seraph_401("You do not have the permission to see the specified issue.");
        }
        MockFault::ServerError => {
            return jira_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Internal server error (injected by mockd)",
            );
        }
        MockFault::RateLimited { retry_after_secs } => {
            let mut r = jira_error(
                StatusCode::TOO_MANY_REQUESTS,
                "Rate limit exceeded (injected by mockd)",
            );
            if let Ok(v) = HeaderValue::from_str(&retry_after_secs.to_string()) {
                r.headers_mut().insert(header::RETRY_AFTER, v);
            }
            return r;
        }
        // Hang, then answer normally: what a client with its own timeout has
        // to survive is the wait, not a special status code.
        MockFault::Timeout { hang_ms } => {
            tokio::time::sleep(std::time::Duration::from_millis(hang_ms)).await;
        }
    }

    // 6. Dispatch.
    next.run(req).await
}

/// The fallback for a path the contract declares and mockd does not serve.
pub(crate) async fn unimplemented(State(state): State<Arc<MockState>>, req: Request) -> Response {
    state.violations().record(Violation {
        kind: ViolationKind::Unimplemented,
        method: req.method().as_str().to_ascii_uppercase(),
        path: req.uri().path().to_owned(),
        query: req.uri().query().unwrap_or_default().to_owned(),
        detail: "no mock handler for this contract path".to_owned(),
        at: Utc::now(),
    });
    let mut r = jira_error(
        StatusCode::NOT_IMPLEMENTED,
        format!("Not implemented by mockd: {}", req.uri().path()),
    );
    r.headers_mut().insert(
        "X-Mockd-Hint",
        HeaderValue::from_static(
            // Header values must be ASCII, so the section sign is spelled out.
            "knobas-mockd implements only the M1 read subset (interfaces doc section 5); \
             this path exists in Jira DC but has no mock handler",
        ),
    );
    r
}

/// Records one violation against `req`, so every handler spells the fields the
/// same way.
fn record_violation(state: &MockState, req: &Request, kind: ViolationKind, detail: String) {
    state.violations().record(Violation {
        kind,
        method: req.method().as_str().to_ascii_uppercase(),
        path: req.uri().path().to_owned(),
        query: req.uri().query().unwrap_or_default().to_owned(),
        detail,
        at: Utc::now(),
    });
}

/// A JQL query outside the subset mockd implements: 400, plus the violation
/// that makes an adapter's own test fail.
pub(crate) fn unsupported_query(state: &MockState, req: &Request, message: &str) -> Response {
    record_violation(
        state,
        req,
        ViolationKind::UnsupportedQuery,
        message.to_owned(),
    );
    jira_error(StatusCode::BAD_REQUEST, message)
}

/// A `fields=`/`expand=` name outside the closed set (deviation 5).
pub(crate) fn unknown_field(state: &MockState, req: &Request, name: &str) -> Response {
    let message = format!(
        "Field or expand name {name:?} is not one mockd serves. Real Jira ignores names it \
         does not know; mockd refuses them, because a typo that silently drops a field from \
         a sync is worth a 400."
    );
    record_violation(state, req, ViolationKind::UnknownField, message.clone());
    jira_error(StatusCode::BAD_REQUEST, message)
}
