//! The contract-violation model: what mockd records when a client asks for
//! something the vendored API contract does not define.
//!
//! A violation is not an error response — the response is separate, and is
//! whatever the real API would answer. It is the *record* that lets an
//! adapter's own test suite fail on a request the real server would have
//! shrugged at (or answered by accident).

use std::sync::Mutex;

use chrono::{DateTime, Utc};

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
