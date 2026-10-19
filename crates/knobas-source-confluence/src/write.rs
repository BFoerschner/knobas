//! The three writes ratified for Confluence: create a page, replace a page's
//! body, comment on a page (M3.2, issue #286).
//!
//! Deliberately free of [`WriteOp`]: this module takes the values already
//! unpacked, and [`crate::source`] -- which is where `impl Source for
//! ConfluenceSource` lives -- does the unpacking. That is not a style choice.
//! `knobas-sync/tests/it/write_choke_point.rs` refuses any production file that
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
/// reader is shown -- unless the page now holds `base_version + 1` with
/// exactly `body`, which is this write landed and met again (#1129): that is
/// answered as delivered.
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

    let sent = http
        .put_json(
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
        .await;
    match sent {
        Ok(_) => Ok(()),
        Err(error)
            if error.status() == Some(409)
                && holds_this_edit(http, &path, base_version + 1, body).await =>
        {
            Ok(())
        }
        Err(error) => Err(error),
    }
}

/// Whether the page now holds `version` with exactly `body`: a 409 over
/// that is this write's own edit, met again by a repeat after the first
/// `PUT` landed and its answer was lost (#1129) -- the transport's retry, or
/// the queue's later resend. Any other version or body is a real conflict,
/// and the refusal stands; so does any read that fails. A body Confluence
/// rewrote on the way in reads as another body, which keeps the refusal
/// rather than claims an edit that may not be this one.
async fn holds_this_edit(http: &ConfluenceHttp, path: &str, version: i64, body: &str) -> bool {
    #[derive(serde::Deserialize)]
    struct Held {
        version: HeldVersion,
        body: HeldBody,
    }
    #[derive(serde::Deserialize)]
    struct HeldVersion {
        number: i64,
    }
    #[derive(serde::Deserialize)]
    struct HeldBody {
        storage: HeldStorage,
    }
    #[derive(serde::Deserialize)]
    struct HeldStorage {
        value: String,
    }

    let query = [("expand", "body.storage,version".to_owned())];
    http.get_json::<Held>(path, &query)
        .await
        .is_ok_and(|held| held.version.number == version && held.body.storage.value == body)
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

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    use crate::http::{CREDENTIAL_RULES, ConfluenceHttp, credential};

    /// A Confluence that answers every request `503`, `Connection: close`,
    /// and counts them, on a std thread (this crate's tokio has no `net`). A
    /// `503` is retried by the transport unless the request opted out.
    fn busy() -> (ConfluenceHttp, Arc<AtomicUsize>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let hits = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&hits);
        std::thread::spawn(move || {
            use std::io::{Read, Write};
            for socket in listener.incoming() {
                let Ok(mut socket) = socket else { return };
                counter.fetch_add(1, Ordering::SeqCst);
                let mut buf = [0_u8; 8192];
                let _ = socket.read(&mut buf);
                let _ = socket.write_all(
                    b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\n\
                      Connection: close\r\n\r\n",
                );
            }
        });
        let auth = credential(&CREDENTIAL_RULES, Some(knobas_source::AuthMethod::Pat), None, Some("pat"))
            .expect("a PAT is enough");
        let http = ConfluenceHttp::new(
            &format!("http://{addr}"),
            &crate::ConfluenceConfig::default(),
            auth,
        )
        .expect("the client builds");
        (http, hits)
    }

    /// #1122: a page create and a comment are POSTs a resend could do twice,
    /// so each is sent once, busy answer or not.
    #[tokio::test]
    async fn a_create_or_a_comment_is_sent_once() {
        let (http, hits) = busy();
        super::create_page(&http, "PAY", "1", "Runbook", "<p>x</p>")
            .await
            .unwrap_err();
        assert_eq!(hits.load(Ordering::SeqCst), 1, "create");

        let (http, hits) = busy();
        super::comment(&http, "1", "bounded").await.unwrap_err();
        assert_eq!(hits.load(Ordering::SeqCst), 1, "comment");
    }

    /// A Confluence that answers each request with the next of `answers`
    /// (`(status, body)`), `Connection: close`, and records each request
    /// line. The request is read whole first, body included, so closing never
    /// resets a client still sending.
    fn scripted(answers: Vec<(u16, &'static str)>) -> (ConfluenceHttp, Arc<Mutex<Vec<String>>>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let record = Arc::clone(&seen);
        std::thread::spawn(move || {
            use std::io::{BufRead, BufReader, Read, Write};
            let mut answers = answers.into_iter();
            for socket in listener.incoming() {
                let Ok(mut socket) = socket else { return };
                let mut reader = BufReader::new(socket.try_clone().unwrap());
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                record.lock().unwrap().push(line.trim().to_owned());
                let mut length = 0;
                loop {
                    let mut header = String::new();
                    reader.read_line(&mut header).unwrap();
                    if header.trim().is_empty() {
                        break;
                    }
                    if let Some((name, value)) = header.split_once(':')
                        && name.eq_ignore_ascii_case("content-length")
                    {
                        length = value.trim().parse().unwrap();
                    }
                }
                let mut body = vec![0_u8; length];
                reader.read_exact(&mut body).unwrap();
                let (status, body) = answers.next().unwrap_or((500, ""));
                let answer = format!(
                    "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\n\
                     Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = socket.write_all(answer.as_bytes());
            }
        });
        let auth = credential(&CREDENTIAL_RULES, Some(knobas_source::AuthMethod::Pat), None, Some("pat"))
            .expect("a PAT is enough");
        let http = ConfluenceHttp::new(
            &format!("http://{addr}"),
            &crate::ConfluenceConfig::default(),
            auth,
        )
        .expect("the client builds");
        (http, seen)
    }

    const CURRENT: &str = r#"{"id":"98","type":"page","title":"Runbook"}"#;
    const CONFLICT: &str = r#"{"statusCode":409,"message":"Version must be incremented on update. Current version is: 8"}"#;

    /// #1129: the first `PUT` lands (the page goes to version 8) and its
    /// answer is lost to a gateway's `504`; the transport's repeat carries the
    /// same version 8 and Confluence refuses it, 409. The page now holds
    /// version 8 with this write's body, so it is this write, delivered.
    #[tokio::test]
    async fn a_page_update_whose_answer_was_lost_is_delivered() {
        let landed =
            r#"{"id":"98","version":{"number":8},"body":{"storage":{"value":"<p>new</p>"}}}"#;
        let (http, seen) = scripted(vec![
            (200, CURRENT),
            (504, ""),
            (409, CONFLICT),
            (200, landed),
        ]);
        super::update_page(&http, "98", 7, "<p>new</p>")
            .await
            .expect("the page holds this write's version");
        let puts = seen
            .lock()
            .unwrap()
            .iter()
            .filter(|l| l.starts_with("PUT"))
            .count();
        assert_eq!(puts, 2, "the lost one and the refused repeat");
    }

    /// Version 8 holds somebody else's body: a real conflict, and the 409
    /// stands.
    #[tokio::test]
    async fn a_page_at_the_next_version_with_another_body_is_still_refused() {
        let theirs =
            r#"{"id":"98","version":{"number":8},"body":{"storage":{"value":"<p>theirs</p>"}}}"#;
        let (http, _) = scripted(vec![
            (200, CURRENT),
            (504, ""),
            (409, CONFLICT),
            (200, theirs),
        ]);
        let error = super::update_page(&http, "98", 7, "<p>new</p>")
            .await
            .expect_err("the page holds somebody else's edit");
        assert_eq!(error.status(), Some(409), "{error:?}");
    }
}
