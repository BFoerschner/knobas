//! The three writes ratified for Confluence: create a page, replace a page's
//! body, comment on a page (M3.2, issue #286).
//!
//! Deliberately free of [`WriteOp`]: this module takes the values already
//! unpacked, and [`crate::source`] -- which is where `impl Source for
//! ConfluenceSource` lives -- does the unpacking. That is not a style choice.
//! `knobas-sync/tests/write_choke_point.rs` refuses any production file that
//! *names* the op enum without implementing the trait, so a dispatch module
//! that named it would read as a second outbound write path.
//!
//! [`WriteOp`]: knobas_source::WriteOp
//!
//! # The endpoints, and what a real Confluence does with them
//!
//! | write | request | body |
//! | --- | --- | --- |
//! | create page | `POST rest/api/content` | `{"type":"page","title":…,"space":{"key":…},"ancestors":[{"id":…}],"body":{"storage":{…}}}` |
//! | update page | `GET rest/api/content/{id}`, then `PUT` the same | `{"id":…,"type":"page","title":…,"version":{"number":<base+1>},"body":{"storage":{…}}}` |
//! | comment | `POST rest/api/content` | `{"type":"comment","container":{"id":…,"type":"page"},"body":{"storage":{…}}}` |
//!
//! Atlassian publishes no machine-readable specification for this product, so
//! none of that is checkable against a mock: `tests/live_confluence_seeded.rs`
//! against the seeded container is the witness (ADR-0013).
//!
//! # The content `PUT` **replaces** the record
//!
//! It is not a patch. A request that omitted `title` would either be refused
//! or blank the page's title, so [`update_page`] reads the record first and
//! sends the title back unchanged. That read is also where the page's `type`
//! comes from, so a blog post edited by id is written back as a blog post
//! rather than silently converted into a page.
//!
//! # `version.number` is the concurrency check, and it is the backstop
//!
//! Confluence accepts a `PUT` only when the version sent is exactly one past
//! the version it holds, and answers 409 otherwise. knobas therefore sends
//! `base_version + 1` -- `base_version` being the version the reader's edit
//! was made against -- and lets the server refuse. That refusal is the
//! *second* line: the write queue holds an `update_page` whose target moved
//! after it was queued (`knobas_core::write_queue::project`), and this catches
//! only the window between the last sync and the flush, which the mirror
//! cannot see. Both directions matter, and neither is sufficient alone
//! (issue #286, ADR-0012).

use knobas_source::SourceError;

use crate::http::ConfluenceHttp;

/// Create a page under `parent`, and report the id Confluence gave it.
///
/// `space` is the key (`ENG`) and `parent` the content id of the page it goes
/// under. Both are sent: Confluence infers the space from the ancestor when it
/// can, but an explicit key is what makes a mistyped parent a 400 naming the
/// space rather than a page that quietly landed somewhere else.
///
/// `body` is **storage format** and is sent verbatim. The caller composed it;
/// an adapter that re-wrapped it here would make the round trip lossy for
/// every caller that already had the dialect right.
///
/// # Errors
///
/// [`SourceError::Protocol`] if the create succeeded but Confluence did not
/// name the page it made -- a write reported as done with nothing to point at,
/// which is the refusal `knobas-source-jira`'s `create_ticket` makes for the
/// same reason; otherwise whatever the request maps to.
pub(crate) async fn create_page(
    http: &ConfluenceHttp,
    space: &str,
    parent: &str,
    title: &str,
    body: &str,
) -> Result<String, SourceError> {
    let response = http
        .post_json(
            "rest/api/content",
            &serde_json::json!({
                "type": "page",
                "title": title,
                "space": { "key": space },
                "ancestors": [{ "id": parent }],
                "body": { "storage": { "value": body, "representation": "storage" } },
            }),
        )
        .await?;
    created_id(response, "the new page").await
}

/// Replace a page's whole body, guarded by the version the edit was made
/// against.
///
/// Two requests, and the first one is the point twice over: the content `PUT`
/// replaces the record, so the title and the content type have to be read
/// before they can be sent back unchanged.
///
/// # Errors
///
/// [`SourceError::Protocol`] when the read does not answer with a title -- a
/// `PUT` composed without one would blank the page -- and whatever the two
/// requests map to otherwise. A 409 from the version check arrives as a
/// refusal carrying Confluence's own sentence (ADR-0004), which is what the
/// reader is shown.
pub(crate) async fn update_page(
    http: &ConfluenceHttp,
    id: &str,
    base_version: i64,
    body: &str,
) -> Result<(), SourceError> {
    #[derive(serde::Deserialize)]
    struct Current {
        #[serde(default)]
        title: Option<String>,
        #[serde(default, rename = "type")]
        content_type: Option<String>,
    }

    let path = format!("rest/api/content/{id}");
    let current: Current = http.get_json(&path, &[]).await?;
    let Some(title) = current.title.filter(|t| !t.trim().is_empty()) else {
        return Err(SourceError::protocol(format!(
            "Confluence answered for content {id} without a title, and the content PUT replaces \
             the record: sending an edit now would blank the page's title. Refusing instead."
        )));
    };

    http.put_json(
        &path,
        &serde_json::json!({
            "id": id,
            // What the record says it is, never a hardcoded "page": a blog
            // post reached by id must be written back as a blog post.
            "type": current.content_type.as_deref().unwrap_or("page"),
            "title": title,
            "version": { "number": base_version + 1 },
            "body": { "storage": { "value": body, "representation": "storage" } },
        }),
    )
    .await
    .map(|_| ())
}

/// Comment on a page, and report the id Confluence gave the comment.
///
/// `text` is **what the reader typed**, not storage format. `WriteOp::Comment`
/// is shared with Jira and Gitea, whose comment fields take plain text, so the
/// dialect translation is this adapter's job and happens in
/// [`crate::storage::from_text`] -- which is also what stops a `<` in somebody's
/// prose reaching the wiki as markup.
///
/// # Errors
///
/// [`SourceError::Protocol`] if Confluence accepted the comment without naming
/// it; otherwise whatever the request maps to. An empty comment is refused by
/// Confluence itself with a 400 carrying its own sentence.
pub(crate) async fn comment(
    http: &ConfluenceHttp,
    page_id: &str,
    text: &str,
) -> Result<String, SourceError> {
    let response = http
        .post_json(
            "rest/api/content",
            &serde_json::json!({
                "type": "comment",
                "container": { "id": page_id, "type": "page" },
                "body": {
                    "storage": {
                        "value": crate::storage::from_text(text),
                        "representation": "storage",
                    },
                },
            }),
        )
        .await?;
    created_id(response, "the comment").await
}

/// The `id` out of a create's answer, or the refusal that a write reported as
/// done with nothing to point at deserves.
async fn created_id(response: knobas_http::Response, what: &str) -> Result<String, SourceError> {
    let status = response.status();
    let created: serde_json::Value = response.json().await.map_err(|error| {
        SourceError::protocol(format!(
            "Confluence answered {status} to a create but not with the shape the REST v1 API \
             documents: {error}"
        ))
    })?;
    created
        .get("id")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| {
            SourceError::protocol(format!(
                "Confluence accepted {what} but did not say what id it gave it"
            ))
        })
}
