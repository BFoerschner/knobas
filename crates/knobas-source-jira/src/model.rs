//! Serde views of the six Data Center responses this adapter reads.
//!
//! Every issue and comment keeps **both** its raw [`serde_json::Value`] and a
//! typed view of the handful of fields the adapter uses. The raw half is what
//! `SyncItem::payload` carries (spec §3a: "raw payload kept", so a later,
//! smarter mapping can re-project existing data without re-syncing); the typed
//! half is what the mapping and the cursor read. Deriving both from one
//! `Deserialize` keeps them from drifting.
//!
//! Nothing here is `deny_unknown_fields`: a Jira instance carries hundreds of
//! custom fields and every upgrade adds more. That is the opposite of
//! [`crate::JiraConfig`], where an unknown key is knobas' own bug.
//!
//! Only fields something in this crate *reads* are modelled. Status, issue
//! type, reporter, labels and the custom fields are not missing -- they ride
//! along in [`RawIssue::raw`], which is what becomes `SyncItem::payload`, and
//! that is where the §3a generic detail view and any later re-mapping read
//! them from.

use serde::Deserialize;
use serde_json::Value;

/// `GET /rest/api/2/search` -- the classic `startAt`/`total` envelope
/// (WADL: `SearchResultsBean`). Cloud's `/search/jql` has no `total` and pages
/// with `nextPageToken` instead; that is the other dialect, refused in
/// [`crate::JiraConfig::from_json`].
///
/// The echoed `startAt`/`maxResults` are deliberately not modelled: the run
/// advances its own offset by how many issues actually arrived, which is
/// correct even when the server clamped `maxResults` to its own limit.
#[derive(Debug, Deserialize)]
pub(crate) struct SearchPage {
    #[serde(default)]
    pub total: u32,
    #[serde(default)]
    pub issues: Vec<RawIssue>,
}

/// One issue: the record as it arrived, plus the fields the adapter reads.
#[derive(Debug, Clone)]
pub(crate) struct RawIssue {
    pub raw: Value,
    pub issue: Issue,
}

impl<'de> Deserialize<'de> for RawIssue {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let raw = Value::deserialize(d)?;
        let issue = Issue::deserialize(&raw).map_err(serde::de::Error::custom)?;
        Ok(Self { raw, issue })
    }
}

impl RawIssue {
    /// Replace the comment container in both views, after the dedicated
    /// endpoint filled in what `/search` truncated.
    ///
    /// Both or neither: a `payload` holding all twelve comments beside a typed
    /// `total` still saying three would re-fetch the issue on every run, and a
    /// `body_text` built from one view beside a `payload` from the other is two
    /// answers to "what does this ticket say".
    pub(crate) fn set_comments(&mut self, comments: Vec<RawComment>) {
        let total = comments.len();
        let container = serde_json::json!({
            "startAt": 0,
            "maxResults": total,
            "total": total,
            "comments": comments.iter().map(|c| c.raw.clone()).collect::<Vec<_>>(),
        });
        if let Some(fields) = self.fields_mut() {
            fields.insert("comment".to_owned(), container);
            self.issue.fields.comment = Some(CommentContainer {
                total: Some(count(total)),
                comments,
            });
        }
    }

    /// Same for worklogs. No worklog field feeds `body_text` (interfaces §4.1
    /// lists title + description + comments), so the typed view exists only to
    /// answer "was this truncated?" -- after a completion, it is not.
    pub(crate) fn set_worklogs(&mut self, worklogs: Vec<Value>) {
        let total = worklogs.len();
        let container = serde_json::json!({
            "startAt": 0,
            "maxResults": total,
            "total": total,
            "worklogs": worklogs.clone(),
        });
        if let Some(fields) = self.fields_mut() {
            fields.insert("worklog".to_owned(), container);
            self.issue.fields.worklog = Some(WorklogContainer {
                total: Some(count(total)),
                worklogs,
            });
        }
    }

    /// The raw record's `fields` object, created if the response had none.
    ///
    /// `None` only for a record that is not a JSON object at all, which cannot
    /// be reached: [`RawIssue`]'s `Deserialize` builds [`Issue`] from the same
    /// value, and that needs an object with a `key` in it. Returning an
    /// `Option` rather than asserting keeps the two views from ever being
    /// written apart -- the callers update the typed half inside the same
    /// `if let`.
    fn fields_mut(&mut self) -> Option<&mut serde_json::Map<String, Value>> {
        let object = self.raw.as_object_mut()?;
        let fields = object
            .entry("fields")
            .or_insert_with(|| Value::Object(serde_json::Map::new()));
        if !fields.is_object() {
            *fields = Value::Object(serde_json::Map::new());
        }
        fields.as_object_mut()
    }
}

/// A container size as Jira reports it. Saturating rather than `as`: a
/// truncating cast on a pathological list would report *fewer* comments than
/// were fetched, and the completion check would loop.
fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct Issue {
    pub key: String,
    #[serde(default)]
    pub fields: Fields,
}

/// Jira's own field names are lowercase, so no rename is applied here.
#[derive(Debug, Clone, Default, Deserialize)]
pub(crate) struct Fields {
    pub summary: Option<String>,
    pub description: Option<String>,
    pub updated: Option<String>,
    pub assignee: Option<User>,
    pub comment: Option<CommentContainer>,
    pub worklog: Option<WorklogContainer>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct User {
    /// The Data Center username -- `author` per interfaces §4.1. Cloud has
    /// `accountId` here instead, which is another reason the two dialects are
    /// separate adapters' worth of work.
    pub name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct CommentContainer {
    /// How many the issue has. When it exceeds `comments.len()`, `/search`
    /// truncated and the dedicated endpoint has the rest.
    pub total: Option<u32>,
    #[serde(default)]
    pub comments: Vec<RawComment>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct WorklogContainer {
    pub total: Option<u32>,
    #[serde(default)]
    pub worklogs: Vec<Value>,
}

/// One comment: kept verbatim for `payload`, with its body lifted out for
/// `body_text`.
#[derive(Debug, Clone)]
pub(crate) struct RawComment {
    pub raw: Value,
    pub body: Option<String>,
}

impl<'de> Deserialize<'de> for RawComment {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let raw = Value::deserialize(d)?;
        let body = raw.get("body").and_then(Value::as_str).map(str::to_owned);
        Ok(Self { raw, body })
    }
}

/// `GET /rest/api/2/issue/{key}/comment` (WADL: comments-with-pagination).
#[derive(Debug, Deserialize)]
pub(crate) struct CommentPage {
    #[serde(default)]
    pub total: u32,
    #[serde(default)]
    pub comments: Vec<RawComment>,
}

/// `GET /rest/api/2/issue/{key}/worklog` (WADL: worklog-with-pagination).
///
/// The WADL declares **no query parameters** on this resource, so it cannot be
/// paged. The server's `total` is therefore not modelled: there is no second
/// request it could drive.
#[derive(Debug, Deserialize)]
pub(crate) struct WorklogPage {
    #[serde(default)]
    pub worklogs: Vec<Value>,
}

/// `GET /rest/api/2/serverInfo`.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ServerInfo {
    pub version: Option<String>,
    pub deployment_type: Option<String>,
    /// The server's own clock *with its offset* -- how the adapter learns which
    /// zone JQL date literals will be read in, without a timezone database.
    pub server_time: Option<String>,
}

impl ServerInfo {
    /// The server's UTC offset in seconds.
    ///
    /// A server that will not say falls back to
    /// [`crate::time::MIN_UTC_OFFSET_SECS`], **not** to zero. Zero looks like
    /// the neutral choice and is not: the JQL literal is read back in the
    /// server's own zone, so assuming an offset higher than the truth moves the
    /// query's lower bound *forward*. For a Jira at UTC-05 that silently skips
    /// five hours of edits on every run and never goes back for them, while the
    /// error in the other direction only re-reads work already done -- and
    /// upserts are idempotent. Over-fetching is recoverable; under-fetching is
    /// not, so the fallback is the lowest offset any real zone uses.
    pub(crate) fn offset_secs(&self) -> i32 {
        self.server_time
            .as_deref()
            .and_then(crate::time::parse_offset_secs)
            .unwrap_or(crate::time::MIN_UTC_OFFSET_SECS)
    }
}

/// `GET /rest/api/2/myself`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Myself {
    pub name: Option<String>,
    pub display_name: Option<String>,
}
