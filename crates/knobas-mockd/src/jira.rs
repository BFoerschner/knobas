//! The Jira Data Center router: the endpoints mockd serves, in the shapes the
//! vendored WADL declares.
//!
//! Every route here is also in the generated allowlist — the middleware runs
//! first and refuses anything the contract does not define, so a handler only
//! ever sees a request the real server would have accepted.

use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Path, Request, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::FixedOffset;
use serde_json::{Value, json};

use crate::jql::parse_jql;
use crate::state::{JiraComment, JiraIssue, JiraWorklog, MockState, jira_date};
use crate::validate::{jira_error, jira_guard, unimplemented, unknown_field, unsupported_query};

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
        .route("/rest/api/2/search", get(search))
        .route("/rest/api/2/issue/{issueIdOrKey}", get(issue))
        .route(
            "/rest/api/2/issue/{issueIdOrKey}/comment",
            get(issue_comments).merge(post(post_comment)),
        )
        .route(
            "/rest/api/2/issue/{issueIdOrKey}/worklog",
            get(issue_worklogs),
        )
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

// -- field / expand projection ----------------------------------------------

/// Which `fields=` names to serialise.
///
/// Deviation 5: mockd validates these against a closed set, where real Jira
/// ignores names it does not know.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FieldSel {
    names: Vec<String>,
}

/// Everything `*navigable` covers: the whole issue except the two collections
/// Jira also keeps off the default projection.
const NAVIGABLE: &[&str] = &[
    "summary",
    "description",
    "issuetype",
    "status",
    "priority",
    "assignee",
    "reporter",
    "project",
    "created",
    "updated",
];
const NON_NAVIGABLE: &[&str] = &["comment", "worklog"];

impl FieldSel {
    /// `None` (no `fields=`) is Jira's `*navigable` default.
    fn parse(raw: Option<&str>) -> Result<Self, String> {
        let Some(raw) = raw.map(str::trim).filter(|s| !s.is_empty()) else {
            return Ok(Self {
                names: NAVIGABLE.iter().map(|s| (*s).to_owned()).collect(),
            });
        };
        let mut names = Vec::new();
        for part in raw.split(',').map(str::trim).filter(|s| !s.is_empty()) {
            match part {
                "*all" => names.extend(
                    NAVIGABLE
                        .iter()
                        .chain(NON_NAVIGABLE)
                        .map(|s| (*s).to_owned()),
                ),
                "*navigable" => names.extend(NAVIGABLE.iter().map(|s| (*s).to_owned())),
                n if NAVIGABLE.contains(&n) || NON_NAVIGABLE.contains(&n) => {
                    names.push(n.to_owned());
                }
                other => return Err(other.to_owned()),
            }
        }
        Ok(Self { names })
    }

    fn has(&self, name: &str) -> bool {
        self.names.iter().any(|n| n == name)
    }
}

/// Which `expand=` names were asked for.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ExpandSel {
    names: Vec<String>,
}

const EXPANDABLE: &[&str] = &[
    "renderedFields",
    "names",
    "schema",
    "changelog",
    "transitions",
];

impl ExpandSel {
    fn parse(raw: Option<&str>) -> Result<Self, String> {
        let mut names = Vec::new();
        for part in raw
            .unwrap_or_default()
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            if EXPANDABLE.contains(&part) {
                names.push(part.to_owned());
            } else {
                return Err(part.to_owned());
            }
        }
        Ok(Self { names })
    }

    fn has(&self, name: &str) -> bool {
        self.names.iter().any(|n| n == name)
    }
}

// -- issue serialisation ----------------------------------------------------

fn status_json(status: &str) -> Value {
    // (status id, category key, category name) — the mapping a DC instance with
    // the default workflow would report.
    let (id, cat_key, cat_name) = match status {
        "To Do" => ("10000", "new", "To Do"),
        "In Progress" => ("3", "indeterminate", "In Progress"),
        "In Review" => ("10002", "indeterminate", "In Progress"),
        "Done" => ("10001", "done", "Done"),
        _ => ("10003", "indeterminate", "In Progress"),
    };
    json!({
        "name": status,
        "id": id,
        "statusCategory": { "key": cat_key, "name": cat_name },
    })
}

/// Jira's human-readable duration: `16200` -> `"4h 30m"`.
fn time_spent(seconds: u64) -> String {
    let (h, m) = (seconds / 3600, (seconds % 3600) / 60);
    match (h, m) {
        (0, m) => format!("{m}m"),
        (h, 0) => format!("{h}h"),
        (h, m) => format!("{h}h {m}m"),
    }
}

fn comment_json(base: &str, issue: &JiraIssue, c: &JiraComment, off: FixedOffset) -> Value {
    let author = user_json(base, &c.author);
    json!({
        "self": format!("{base}/rest/api/2/issue/{}/comment/{}", issue.id, c.id),
        "id": c.id.to_string(),
        "author": author,
        "body": c.body,
        "updateAuthor": user_json(base, &c.author),
        "created": jira_date(c.created, off),
        "updated": jira_date(c.updated, off),
    })
}

fn worklog_json(base: &str, issue: &JiraIssue, w: &JiraWorklog, off: FixedOffset) -> Value {
    json!({
        "self": format!("{base}/rest/api/2/issue/{}/worklog/{}", issue.id, w.id),
        "author": user_json(base, &w.author),
        "updateAuthor": user_json(base, &w.author),
        "comment": w.comment,
        "created": jira_date(w.started, off),
        "updated": jira_date(w.started, off),
        "started": jira_date(w.started, off),
        "timeSpent": time_spent(w.time_spent_seconds),
        "timeSpentSeconds": w.time_spent_seconds,
        "id": w.id.to_string(),
        "issueId": issue.id.to_string(),
    })
}

/// The comment collection in the paginated envelope the standalone endpoint
/// also returns. `start`/`max` page it; the `fields=comment` caller passes the
/// whole list.
pub(crate) fn comments_envelope(
    base: &str,
    issue: &JiraIssue,
    off: FixedOffset,
    start: usize,
    max: usize,
) -> Value {
    let page: Vec<Value> = issue
        .comments
        .iter()
        .skip(start)
        .take(max)
        .map(|c| comment_json(base, issue, c, off))
        .collect();
    json!({
        "startAt": start,
        "maxResults": max,
        "total": issue.comments.len(),
        "comments": page,
    })
}

/// The worklog collection. The endpoint takes no pagination parameters (the
/// WADL declares none), but the response schema is still the paginated
/// envelope — Jira's own quirk, reproduced rather than tidied up.
pub(crate) fn worklogs_envelope(base: &str, issue: &JiraIssue, off: FixedOffset) -> Value {
    let all: Vec<Value> = issue
        .worklogs
        .iter()
        .map(|w| worklog_json(base, issue, w, off))
        .collect();
    json!({
        "startAt": 0,
        "maxResults": all.len(),
        "total": all.len(),
        "worklogs": all,
    })
}

/// One issue in the DC wire shape. `/search` and `/issue/{key}` both go through
/// here, so the two cannot disagree about a shape.
pub(crate) fn issue_json(
    issue: &JiraIssue,
    base: &str,
    off: FixedOffset,
    fields: &FieldSel,
    expand: &ExpandSel,
) -> Value {
    let mut f = serde_json::Map::new();
    if fields.has("summary") {
        f.insert("summary".into(), json!(issue.summary));
    }
    if fields.has("description") {
        f.insert("description".into(), json!(issue.description));
    }
    if fields.has("issuetype") {
        f.insert(
            "issuetype".into(),
            json!({ "name": issue.issue_type, "subtask": false }),
        );
    }
    if fields.has("status") {
        f.insert("status".into(), status_json(&issue.status));
    }
    if fields.has("priority") {
        f.insert(
            "priority".into(),
            issue
                .priority
                .as_ref()
                .map_or(Value::Null, |p| json!({ "name": p })),
        );
    }
    if fields.has("assignee") {
        f.insert(
            "assignee".into(),
            issue
                .assignee
                .as_ref()
                .map_or(Value::Null, |a| user_json(base, a)),
        );
    }
    if fields.has("reporter") {
        f.insert("reporter".into(), user_json(base, &issue.reporter));
    }
    if fields.has("project") {
        f.insert(
            "project".into(),
            json!({ "key": issue.project, "name": issue.project }),
        );
    }
    if fields.has("created") {
        f.insert("created".into(), json!(jira_date(issue.created, off)));
    }
    if fields.has("updated") {
        f.insert("updated".into(), json!(jira_date(issue.updated, off)));
    }
    if fields.has("comment") {
        f.insert(
            "comment".into(),
            comments_envelope(base, issue, off, 0, issue.comments.len()),
        );
    }
    if fields.has("worklog") {
        f.insert("worklog".into(), worklogs_envelope(base, issue, off));
    }

    let mut out = json!({
        "expand": expand.names.join(","),
        "id": issue.id.to_string(),
        "self": format!("{base}/rest/api/2/issue/{}", issue.id),
        "key": issue.key,
        "fields": Value::Object(f),
    });
    if expand.has("renderedFields") {
        // Deviation 7: no wiki rendering — the description comes back verbatim.
        out["renderedFields"] = json!({ "description": issue.description });
    }
    if expand.has("names") {
        out["names"] = json!({ "summary": "Summary", "updated": "Updated" });
    }
    if expand.has("schema") {
        out["schema"] = json!({});
    }
    if expand.has("changelog") {
        out["changelog"] = json!({ "startAt": 0, "maxResults": 0, "total": 0, "histories": [] });
    }
    if expand.has("transitions") {
        out["transitions"] = json!([]);
    }
    out
}

// -- GET /rest/api/2/search -------------------------------------------------

fn num_param(q: &HashMap<String, String>, key: &str, default: u32) -> u32 {
    q.get(key)
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(default)
}

/// Jira's own default page size when the client does not ask for one.
const DEFAULT_MAX_RESULTS: u32 = 50;

/// The query string as a map. The middleware has already refused any key the
/// contract does not declare, so a handler only ever sees legal names.
fn query_map(req: &Request) -> HashMap<String, String> {
    axum::extract::Query::<HashMap<String, String>>::try_from_uri(req.uri())
        .map(|q| q.0)
        .unwrap_or_default()
}

/// `fields=` and `expand=`, or the first name that is not in the closed set.
fn selections(q: &HashMap<String, String>) -> Result<(FieldSel, ExpandSel), String> {
    let fields = FieldSel::parse(q.get("fields").map(String::as_str))?;
    let expand = ExpandSel::parse(q.get("expand").map(String::as_str))?;
    Ok((fields, expand))
}

async fn search(State(s): State<Arc<MockState>>, req: Request) -> Response {
    let q = query_map(&req);
    let base = s.base_url();
    let off = s.server_offset();

    let jql = match parse_jql(q.get("jql").map(String::as_str).unwrap_or_default(), off) {
        Ok(j) => j,
        Err(e) => return unsupported_query(&s, &req, &e.to_string()),
    };
    let (fields, expand) = match selections(&q) {
        Ok(v) => v,
        Err(bad) => return unknown_field(&s, &req, &bad),
    };

    let mut hits: Vec<JiraIssue> = s
        .issues()
        .into_iter()
        .filter(|i| jql.updated_gte.is_none_or(|t| i.updated >= t))
        .filter(|i| jql.projects.is_empty() || jql.projects.contains(&i.project))
        .collect();
    // Ties broken by id so a page boundary is never ambiguous.
    hits.sort_by(|a, b| match a.updated.cmp(&b.updated) {
        std::cmp::Ordering::Equal => a.id.cmp(&b.id),
        other if jql.order_by_updated_desc => other.reverse(),
        other => other,
    });

    let total = hits.len();
    let start = num_param(&q, "startAt", 0) as usize;
    // The server caps the page size and reports the value it actually used: an
    // adapter that reads "fewer rows than I asked for" as "last page" would
    // otherwise truncate a sync.
    let max = num_param(&q, "maxResults", DEFAULT_MAX_RESULTS).min(s.max_results_cap());
    let page: Vec<Value> = hits
        .iter()
        .skip(start)
        .take(max as usize)
        .map(|i| issue_json(i, &base, off, &fields, &expand))
        .collect();

    Json(json!({
        "expand": "schema,names",
        "startAt": start,
        "maxResults": max,
        "total": total,
        "issues": page,
    }))
    .into_response()
}

// -- GET/POST /rest/api/2/issue/{issueIdOrKey}[/comment|/worklog] -----------

/// `{issueIdOrKey}` really is either: an adapter that kept the numeric `id`
/// from a search response and fetched by it must work.
fn find_issue(s: &MockState, id_or_key: &str) -> Option<JiraIssue> {
    s.issue(id_or_key).or_else(|| {
        s.issues()
            .into_iter()
            .find(|i| i.id.to_string() == id_or_key)
    })
}

fn no_such_issue(id_or_key: &str) -> Response {
    jira_error(
        StatusCode::NOT_FOUND,
        format!("Issue does not exist or you do not have permission to see it: {id_or_key}"),
    )
}

async fn issue(
    State(s): State<Arc<MockState>>,
    Path(id_or_key): Path<String>,
    req: Request,
) -> Response {
    let q = query_map(&req);
    let (fields, expand) = match selections(&q) {
        Ok(v) => v,
        Err(bad) => return unknown_field(&s, &req, &bad),
    };
    let Some(i) = find_issue(&s, &id_or_key) else {
        return no_such_issue(&id_or_key);
    };
    Json(issue_json(
        &i,
        &s.base_url(),
        s.server_offset(),
        &fields,
        &expand,
    ))
    .into_response()
}

async fn issue_comments(
    State(s): State<Arc<MockState>>,
    Path(id_or_key): Path<String>,
    req: Request,
) -> Response {
    let q = query_map(&req);
    // `expand` is declared by the WADL; mockd serves the same body either way,
    // so only its spelling is checked.
    if let Err(bad) = ExpandSel::parse(q.get("expand").map(String::as_str)) {
        return unknown_field(&s, &req, &bad);
    }
    let newest_first = match q.get("orderBy").map(String::as_str) {
        None | Some("created") | Some("+created") => false,
        Some("-created") => true,
        Some(other) => {
            return unsupported_query(
                &s,
                &req,
                &format!("orderBy={other:?} is not supported; use created or -created"),
            );
        }
    };
    let Some(mut i) = find_issue(&s, &id_or_key) else {
        return no_such_issue(&id_or_key);
    };
    if newest_first {
        i.comments.reverse();
    }
    let start = num_param(&q, "startAt", 0) as usize;
    let max = num_param(&q, "maxResults", DEFAULT_MAX_RESULTS).min(s.max_results_cap());
    Json(comments_envelope(
        &s.base_url(),
        &i,
        s.server_offset(),
        start,
        max as usize,
    ))
    .into_response()
}

async fn issue_worklogs(
    State(s): State<Arc<MockState>>,
    Path(id_or_key): Path<String>,
) -> Response {
    let Some(i) = find_issue(&s, &id_or_key) else {
        return no_such_issue(&id_or_key);
    };
    Json(worklogs_envelope(&s.base_url(), &i, s.server_offset())).into_response()
}

/// The M2 write-back path, built now because it costs nothing (interfaces §5).
/// **No M1 adapter may call it** — every M1 adapter declares `write_ops: []`.
async fn post_comment(
    State(s): State<Arc<MockState>>,
    Path(id_or_key): Path<String>,
    body: Option<Json<Value>>,
) -> Response {
    let Some(i) = find_issue(&s, &id_or_key) else {
        return no_such_issue(&id_or_key);
    };
    let text = body
        .as_ref()
        .and_then(|Json(v)| v.get("body"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    if text.is_empty() {
        return jira_error(StatusCode::BAD_REQUEST, "Comment body must not be empty");
    }
    // mockd authenticates as Mara, matching `myself`.
    let author = knobas_source_mock::fixture()
        .person(MYSELF)
        .expect("the fixture has Mara")
        .username
        .clone();
    let Some(id) = s.add_comment(&i.key, &author, text) else {
        return no_such_issue(&id_or_key);
    };
    let after = s.issue(&i.key).expect("the issue was just commented on");
    let created = after
        .comments
        .iter()
        .find(|c| c.id == id)
        .expect("the comment that was just added");
    (
        StatusCode::CREATED,
        Json(comment_json(
            &s.base_url(),
            &after,
            created,
            s.server_offset(),
        )),
    )
        .into_response()
}
