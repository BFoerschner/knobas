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

/// This router's key in [`MockState`]'s per-API base-URL map.
pub(crate) const API: &str = "jira";

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
        // M2's write-back set (issue #43). `POST /issue` is a literal route and
        // is registered *before* the templated `/issue/{issueIdOrKey}` for the
        // same reason `allowlist::lookup` prefers a literal: `api/2/issue` must
        // not resolve as an issue whose key is empty.
        .route("/rest/api/2/issue", post(create_issue))
        .route(
            "/rest/api/2/issue/{issueIdOrKey}/transitions",
            get(issue_transitions).merge(post(do_transition)),
        )
        .fallback(unimplemented)
        // A verb the WADL declares on a path mockd *does* serve (`POST
        // /search`, `PUT /myself`, `PUT`/`DELETE /issue/{key}`, `POST
        // .../worklog`) is not a method violation: the middleware already let
        // it through, because the contract has it. Without this, axum answers
        // its own 405 -- an `Allow` header describing mockd's routing table
        // rather than the contract, an empty body instead of the Jira error
        // shape, and no violation recorded at all. Send it to the same 501
        // fallback an unserved *path* gets.
        .method_not_allowed_fallback(unimplemented)
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            jira_guard,
        ))
        .with_state(state.clone())
        // Merged *after* the layer, which is what exempts `/__mock/*` from the
        // product middleware: axum applies a layer to the routes present when
        // it is added, never to ones merged in later.
        .merge(crate::admin::router(state))
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
        "baseUrl": s.base_url(API),
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
    let base = s.base_url(API);
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

/// This instance's **Epic Link** custom field — the classic Data Center
/// spelling of the relationship `parent` carries (deviation 13).
///
/// Every Jira DC instance has exactly one id for this field and the id differs
/// per instance, which is the whole reason
/// `JiraConfig::epic_link_field` exists as an option rather than a constant.
/// mockd is one instance, so it has one id, and this is it. No id would be
/// *realistic* -- a real instance's is whatever its Greenhopper provisioning
/// happened to allocate, and `10008` is only one plausible outcome of that --
/// so the id is chosen for **agreement** instead: it is already the value the
/// adapter's own goldens
/// (`knobas-source-jira/tests/golden/search-page.json`) and its descriptor
/// example use, so the two halves of the repo name one field.
///
/// It is exported so a test names *the mock's* field rather than hard-coding a
/// string that could stop meaning anything, the same reason [`crate::JIRA_TOKEN`]
/// is a constant.
///
/// **Exactly one id, not a pattern (issue #125).** Any other `customfield_*` is
/// still a 400 plus an `UnknownField` violation. A pattern would accept
/// `customfield_99999`, serve nothing under it, and let a mistyped
/// `epic_link_field` pass as a working configuration — which is the exact class
/// of bug deviation 5 exists to catch, one field id away from the one that
/// matters.
pub const EPIC_LINK_FIELD: &str = "customfield_10008";

/// Everything `*navigable` covers: the whole issue except the two collections
/// Jira also keeps off the default projection.
///
/// **Widened in issue #32.** The first ten are what M1 served, and for a while
/// they were also the whole of what `knobas-source-jira`'s `BASE_FIELDS`
/// asked for -- because asking for anything else was a 400 here. That is the
/// dependency the wrong way round: a mock's coverage was deciding what
/// production fetched, so the mirror's `payload` was missing epic membership,
/// links and resolution on every issue knobas had ever synced. The six below
/// close it. Each is served from the fixture and nothing here is invented
/// (see the transcription table in [`crate::state`]); the set stays *closed*,
/// so deviation 5 is widened rather than retired and a name mockd does not
/// serve is still a 400 plus an `UnknownField` violation.
///
/// **Widened again by one name in issue #125**: [`EPIC_LINK_FIELD`]. It is
/// navigable because a real instance's `*navigable` covers its custom fields
/// too, and it is here rather than in a set of its own because a reader asking
/// "what does this mock serve?" should find one answer.
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
    "labels",
    "parent",
    "resolution",
    "issuelinks",
    "timeoriginalestimate",
    "timespent",
    EPIC_LINK_FIELD,
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

/// The resolution a DC instance with the default workflow would report.
///
/// Jira sets a resolution when and only when an issue reaches a status in the
/// `done` category, so this is read off [`status_json`]'s own category rather
/// than off a second list of status names -- two lists would be two answers to
/// "is this issue finished".
fn resolution_json(status: &str) -> Value {
    if status_json(status)["statusCategory"]["key"] == "done" {
        json!({ "name": "Done", "id": "10000" })
    } else {
        Value::Null
    }
}

/// The abbreviated issue Jira nests inside `parent` and inside each
/// `issuelinks` entry: identity plus the handful of fields it carries.
fn issue_ref_json(base: &str, r: &crate::state::JiraIssueRef) -> Value {
    json!({
        "id": r.id.to_string(),
        "key": r.key,
        "self": format!("{base}/rest/api/2/issue/{}", r.id),
        "fields": {
            "summary": r.summary,
            "issuetype": { "name": r.issue_type, "subtask": false },
            "status": status_json(&r.status),
            "priority": r.priority.as_ref().map_or(Value::Null, |p| json!({ "name": p })),
        },
    })
}

/// One `issuelinks` entry. The link *type* is the same object at both ends;
/// which of `inwardIssue` / `outwardIssue` is present is what says which end
/// this is, and exactly one of them ever is.
fn issue_link_json(base: &str, l: &crate::state::JiraLink) -> Value {
    let mut out = json!({
        "id": l.id.to_string(),
        "self": format!("{base}/rest/api/2/issueLink/{}", l.id),
        "type": {
            "id": "10000",
            "name": "Blocks",
            "inward": "is blocked by",
            "outward": "blocks",
        },
    });
    let side = if l.inward {
        "inwardIssue"
    } else {
        "outwardIssue"
    };
    out[side] = issue_ref_json(base, &l.other);
    out
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
    if fields.has("labels") {
        // Always empty: the dataset names no labels, and mockd inventing some
        // would put them in `payload` and in the search index (the #28 ruling).
        f.insert("labels".into(), json!([]));
    }
    if fields.has("parent") {
        // Absent, not null, where there is no epic -- which is how Jira
        // serves an issue with no parent, and why a reader can test for the
        // key rather than having to distinguish null from missing.
        if let Some(p) = &issue.parent {
            f.insert("parent".into(), issue_ref_json(base, p));
        }
    }
    if fields.has(EPIC_LINK_FIELD) {
        // The same fixture `epic`, in the classic Data Center spelling: the
        // Epic Link custom field carries the epic's **key**, where `parent`
        // nests an abbreviated issue. Null rather than absent where there is no
        // epic -- a custom field a request named is always in the answer, and
        // that is the one shape difference from `parent` above.
        f.insert(
            EPIC_LINK_FIELD.into(),
            issue
                .parent
                .as_ref()
                .map_or(Value::Null, |p| json!(p.key.clone())),
        );
    }
    if fields.has("resolution") {
        f.insert("resolution".into(), resolution_json(&issue.status));
    }
    if fields.has("issuelinks") {
        f.insert(
            "issuelinks".into(),
            Value::Array(
                issue
                    .links
                    .iter()
                    .map(|l| issue_link_json(base, l))
                    .collect(),
            ),
        );
    }
    if fields.has("timeoriginalestimate") {
        f.insert(
            "timeoriginalestimate".into(),
            issue
                .original_estimate_secs
                .map_or(Value::Null, |s| json!(s)),
        );
    }
    if fields.has("timespent") {
        f.insert(
            "timespent".into(),
            issue.time_spent_secs().map_or(Value::Null, |s| json!(s)),
        );
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

/// A paging parameter, or the message naming why the value is not one.
///
/// Deviation 3 applied to values rather than names: swallowing `startAt=abc`
/// as a 200 from row 0 would certify an adapter whose cursor serialises
/// non-integrally into paging from the start on every run.
fn num_param(q: &HashMap<String, String>, key: &str, default: u32) -> Result<u32, String> {
    match q.get(key) {
        None => Ok(default),
        Some(raw) => raw
            .parse::<u32>()
            .map_err(|_| format!("{key}={raw:?} is not a non-negative integer")),
    }
}

/// `startAt` and `maxResults` together, so every paged endpoint refuses the
/// same values for the same reason.
fn paging(q: &HashMap<String, String>) -> Result<(u32, u32), String> {
    Ok((
        num_param(q, "startAt", 0)?,
        num_param(q, "maxResults", DEFAULT_MAX_RESULTS)?,
    ))
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
    let base = s.base_url(API);
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
    let (start, asked) = match paging(&q) {
        Ok(v) => v,
        Err(m) => return unsupported_query(&s, &req, &m),
    };
    let start = start as usize;
    // The server caps the page size and reports the value it actually used: an
    // adapter that reads "fewer rows than I asked for" as "last page" would
    // otherwise truncate a sync.
    let max = asked.min(s.max_results_cap());
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
        &s.base_url(API),
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
    let (start, asked) = match paging(&q) {
        Ok(v) => v,
        Err(m) => return unsupported_query(&s, &req, &m),
    };
    let Some(mut i) = find_issue(&s, &id_or_key) else {
        return no_such_issue(&id_or_key);
    };
    if newest_first {
        i.comments.reverse();
    }
    let start = start as usize;
    let max = asked.min(s.max_results_cap());
    Json(comments_envelope(
        &s.base_url(API),
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
    Json(worklogs_envelope(&s.base_url(API), &i, s.server_offset())).into_response()
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
            &s.base_url(API),
            &after,
            created,
            s.server_offset(),
        )),
    )
        .into_response()
}

// -- the M2 write-back set (issue #43) --------------------------------------

/// `GET /rest/api/2/issue/{key}/transitions` -- what this issue's workflow
/// offers **from where it stands now**.
///
/// Interfaces §5: the available set is fetched, never assumed. mockd's
/// workflow is [`MockState::jira_transitions`]; the point of it having shape at
/// all is that an adapter which believed every status reachable would pass here
/// and fail against a real Jira.
async fn issue_transitions(
    State(s): State<Arc<MockState>>,
    Path(id_or_key): Path<String>,
) -> Response {
    let Some(i) = find_issue(&s, &id_or_key) else {
        return no_such_issue(&id_or_key);
    };
    let transitions: Vec<Value> = MockState::jira_transitions(&i.status)
        .iter()
        .map(|(id, name, to)| {
            json!({
                "id": id,
                "name": name,
                "to": status_json(to),
            })
        })
        .collect();
    Json(json!({ "expand": "transitions", "transitions": transitions })).into_response()
}

/// `POST /rest/api/2/issue/{key}/transitions` -- perform one.
///
/// A real Jira answers **204 with no body**, which is what makes this endpoint
/// worth pinning: an adapter that insisted on decoding a response would work
/// against a mock that invented one and fail against the real thing.
async fn do_transition(
    State(s): State<Arc<MockState>>,
    Path(id_or_key): Path<String>,
    body: Option<Json<Value>>,
) -> Response {
    let Some(i) = find_issue(&s, &id_or_key) else {
        return no_such_issue(&id_or_key);
    };
    // Jira takes the transition by **id**, under `transition.id`, and the id
    // is a string in its own responses.
    let id = body
        .as_ref()
        .and_then(|Json(v)| v.get("transition"))
        .and_then(|t| t.get("id"))
        .map(|id| match id {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        })
        .unwrap_or_default();
    if id.is_empty() {
        return jira_error(
            StatusCode::BAD_REQUEST,
            "Transition id is required, as transition.id",
        );
    }
    match s.transition_issue(&i.key, &id) {
        None => no_such_issue(&id_or_key),
        Some(Err(())) => jira_error(
            StatusCode::BAD_REQUEST,
            format!(
                "It is not possible to perform this transition on {} from status {:?}",
                i.key, i.status
            ),
        ),
        Some(Ok(_)) => StatusCode::NO_CONTENT.into_response(),
    }
}

/// `POST /rest/api/2/issue` -- create one.
///
/// A real Jira answers **201** with `{id, key, self}` and nothing else: the
/// created issue is not echoed back, so an adapter that wanted the whole record
/// would have to re-read it.
async fn create_issue(State(s): State<Arc<MockState>>, body: Option<Json<Value>>) -> Response {
    let fields = body.as_ref().and_then(|Json(v)| v.get("fields"));
    let text = |name: &str| -> Option<&str> {
        fields
            .and_then(|f| f.get(name))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|v| !v.is_empty())
    };
    let nested = |name: &str, inner: &str| -> Option<&str> {
        fields
            .and_then(|f| f.get(name))
            .and_then(|v| v.get(inner))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|v| !v.is_empty())
    };

    // Every one of these is required by a real Jira, and each is refused
    // separately so a test can tell which field went missing.
    let Some(project) = nested("project", "key") else {
        return jira_error(StatusCode::BAD_REQUEST, "project is required");
    };
    let Some(summary) = text("summary") else {
        return jira_error(
            StatusCode::BAD_REQUEST,
            "You must specify a summary of the issue.",
        );
    };
    let Some(issue_type) = nested("issuetype", "name") else {
        return jira_error(StatusCode::BAD_REQUEST, "issue type is required");
    };
    let description = text("description");

    let reporter = knobas_source_mock::fixture()
        .person(MYSELF)
        .expect("the fixture has Mara")
        .username
        .clone();
    let Some(created) = s.create_issue(project, summary, description, issue_type, &reporter) else {
        return jira_error(
            StatusCode::BAD_REQUEST,
            format!("project: A value with ID {project:?} does not exist for the field 'project'."),
        );
    };
    let base = s.base_url(API);
    (
        StatusCode::CREATED,
        Json(json!({
            "id": created.id.to_string(),
            "key": created.key,
            "self": format!("{base}/rest/api/2/issue/{}", created.id),
        })),
    )
        .into_response()
}
