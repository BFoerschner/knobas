//! Serde views of the three Data Center responses this adapter reads.
//!
//! Every page and comment keeps **both** its raw [`serde_json::Value`] and a
//! typed view of the handful of fields the adapter uses. The raw half is what
//! `SyncItem::payload` carries (spec §3a: "raw payload kept", so a later,
//! smarter mapping -- the storage-format renderer of the next ticket -- can
//! re-project existing data without re-syncing); the typed half is what the
//! mapping and the cursor read. Deriving both from one `Deserialize` keeps
//! them from drifting.
//!
//! Nothing here is `deny_unknown_fields`: a Confluence response carries
//! `_expandable`, `extensions`, `metadata` and whatever the next version adds.
//! That is the opposite of [`crate::ConfluenceConfig`], where an unknown key
//! is knobas' own bug.

use serde::Deserialize;
use serde_json::Value;

/// `GET /rest/api/content/search` and
/// `GET /rest/api/content/{id}/child/comment` -- one envelope for both,
/// because Confluence uses one.
///
/// **There is no `total`.** A content search reports `size` (how many are in
/// *this* page) and, when there are more, a `_links.next`. So the walk is
/// "follow the link until there is none", never "count up to a total" -- and
/// an adapter that invented a total from `size` would stop after one page.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct ContentPage {
    #[serde(default)]
    pub results: Vec<RawContent>,
    #[serde(default, rename = "_links")]
    pub links: PageLinks,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub(crate) struct PageLinks {
    /// The next page, as a path and query rooted at the instance -- e.g.
    /// `/rest/api/content/search?cql=…&start=50&limit=50`. Absent on the last
    /// page, which is the only signal that the walk is done.
    #[serde(default)]
    pub next: Option<String>,
}

/// One page (or one comment): the record as it arrived, plus the fields the
/// adapter reads.
#[derive(Debug, Clone)]
pub(crate) struct RawContent {
    pub raw: Value,
    pub content: Content,
}

impl<'de> Deserialize<'de> for RawContent {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let raw = Value::deserialize(d)?;
        let content = Content::deserialize(&raw).map_err(serde::de::Error::custom)?;
        Ok(Self { raw, content })
    }
}

impl RawContent {
    /// The storage-format body, or `""` for a page with none.
    pub(crate) fn storage(&self) -> &str {
        self.content
            .body
            .as_ref()
            .and_then(|b| b.storage.as_ref())
            .and_then(|s| s.value.as_deref())
            .unwrap_or_default()
    }

    /// Replace the comment container in **both** views.
    ///
    /// Both or neither, for the reason the Jira adapter's `set_comments` gives:
    /// a `payload` holding all twelve comments beside a typed view still
    /// saying "truncated" would re-fetch the page on every run, and a
    /// `body_text` built from one view beside a `payload` from the other is
    /// two answers to "what does this page say".
    ///
    /// The container is written at `children.comment`, which is Confluence's
    /// **own** spelling -- `expand=children.comment` puts it exactly there --
    /// so a payload this completed and a payload the server expanded in full
    /// are the same shape. That is what lets the next ticket's renderer read
    /// one path, and it is the Confluence spelling of what Jira does at
    /// `fields.comment`.
    pub(crate) fn set_comments(&mut self, comments: Vec<RawContent>) {
        let size = comments.len();
        let container = serde_json::json!({
            "results": comments.iter().map(|c| c.raw.clone()).collect::<Vec<_>>(),
            "start": 0,
            "limit": size,
            "size": size,
        });
        if let Some(children) = self.children_mut() {
            children.insert("comment".to_owned(), container);
            self.content.children = Some(Children {
                comment: Some(CommentContainer {
                    results: comments,
                    links: PageLinks::default(),
                }),
            });
        }
    }

    /// The raw record's `children` object, created if the response had none.
    ///
    /// `None` only for a record that is not a JSON object at all, which cannot
    /// be reached: [`RawContent`]'s `Deserialize` builds [`Content`] from the
    /// same value, and that needs an object with an `id`. Returning an
    /// `Option` rather than asserting keeps the two views from ever being
    /// written apart -- the caller updates the typed half inside the same
    /// `if let`.
    fn children_mut(&mut self) -> Option<&mut serde_json::Map<String, Value>> {
        let object = self.raw.as_object_mut()?;
        let children = object
            .entry("children")
            .or_insert_with(|| Value::Object(serde_json::Map::new()));
        if !children.is_object() {
            *children = Value::Object(serde_json::Map::new());
        }
        children.as_object_mut()
    }
}

/// The typed half. Only fields something in this crate *reads* are here;
/// labels, restrictions, metadata and the rest ride along in
/// [`RawContent::raw`], which is what becomes `SyncItem::payload`.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct Content {
    /// Confluence's content id -- **the entity key**. It is assigned once and
    /// survives a rename, a move and a space change, which is the whole reason
    /// the criterion names it rather than the title.
    pub id: String,
    /// `page` or `comment`. Read by the mention walk and nowhere else: a
    /// comment's [`container`](Self::container) is the page it hangs off,
    /// while a **page's** container is its *space* -- so the mention walk must
    /// know which record it is holding before it reads a container id, or it
    /// would follow a space id as though it were a page's.
    #[serde(default, rename = "type")]
    pub content_type: Option<String>,
    /// What this content hangs off, when `expand=container` asked. The page,
    /// for a comment.
    #[serde(default)]
    pub container: Option<Container>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub body: Option<Body>,
    #[serde(default)]
    pub version: Option<Version>,
    #[serde(default)]
    pub history: Option<History>,
    #[serde(default)]
    pub children: Option<Children>,
    #[serde(default, rename = "_links")]
    pub links: Option<ContentLinks>,
}

/// `expand=container`: for a comment, the page it is on.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct Container {
    /// A [`Value`] and not a `String`, and this is not fussiness: a **page's**
    /// container is its space, and a space's `id` comes back as a JSON
    /// *number*. Typed as a string, that record would fail to deserialize and
    /// take the whole mention walk's response down with it -- an adapter that
    /// refuses a run because a page has a space.
    #[serde(default)]
    pub id: Option<Value>,
}

impl Container {
    /// The container's id **as a content id**, which is what a comment's is.
    /// A space's numeric id is not one and reads as absent.
    pub(crate) fn content_id(&self) -> Option<&str> {
        self.id.as_ref()?.as_str()
    }
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct Body {
    #[serde(default)]
    pub storage: Option<Storage>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct Storage {
    #[serde(default)]
    pub value: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct Version {
    /// The version number, which increments on every edit. Half of the
    /// cursor's `(id, version)` identity -- and the half that makes two edits
    /// inside one second distinguishable, which a timestamp at CQL's minute
    /// resolution cannot be.
    #[serde(default)]
    pub number: Option<u64>,
    /// When this version was made: the value CQL's `lastmodified` matches.
    #[serde(default)]
    pub when: Option<String>,
    #[serde(default)]
    pub by: Option<User>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct History {
    #[serde(default, rename = "createdBy")]
    pub created_by: Option<User>,
}

/// A user inside a content record -- who made a version, who created a page.
///
/// **Username only.** Contract §4.1 spells `author` as "the source's username
/// string", and that is what `@me` and *My items* resolve against; a display
/// name in that field would make the identity filter match nobody. The display
/// name rides along in `payload` like everything else this crate does not
/// read.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct User {
    /// The Data Center username. Cloud has `accountId` here instead, which is
    /// one more reason the two dialects are separate adapters' worth of work.
    #[serde(default)]
    pub username: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct Children {
    #[serde(default)]
    pub comment: Option<CommentContainer>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct CommentContainer {
    #[serde(default)]
    pub results: Vec<RawContent>,
    #[serde(default, rename = "_links")]
    pub links: PageLinks,
}

impl CommentContainer {
    /// Whether the server had more comments than it sent.
    ///
    /// A `_links.next` is the only thing that says so -- there is no total to
    /// compare against -- which is why an **absent** container is treated as
    /// truncated too by the sync run: "the server sent no comments" and "the
    /// server was not asked for comments" look identical from here, and
    /// reading the second as the first would silently drop a page's whole
    /// discussion out of the search corpus.
    pub(crate) fn truncated(&self) -> bool {
        self.links.next.is_some()
    }
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ContentLinks {
    /// The path a human opens, relative to the instance -- `/display/ENG/…`
    /// for a page. P5: *Open in browser* renders from `SyncItem::web_url` and
    /// nothing else, and this is where Confluence says what that is.
    #[serde(default)]
    pub webui: Option<String>,
}

/// `GET /rest/api/user/current`.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct CurrentUser {
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default, rename = "displayName")]
    pub display_name: Option<String>,
    /// The stable key behind the username. Reported in
    /// `ConnectionInfo::detail` rather than as the account, because the
    /// account is the thing a person recognises and `username` is what the
    /// identity convention (#82) stores.
    #[serde(default, rename = "userKey")]
    pub user_key: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn a_page() -> RawContent {
        serde_json::from_value(json!({
            "id": "98307",
            "type": "page",
            "title": "SEPA payout retry design",
            "space": { "key": "ENG", "name": "Engineering" },
            "body": { "storage": { "value": "<p>hello</p>", "representation": "storage" } },
            "version": {
                "number": 3,
                "when": "2026-08-22T12:40:00.000+02:00",
                "by": { "username": "knobas", "displayName": "knobas" }
            },
            "history": { "createdBy": { "username": "mara.lindqvist" } },
            "ancestors": [{ "id": "65537", "title": "Engineering" }],
            "_links": { "webui": "/display/ENG/SEPA+payout+retry+design" },
            "_expandable": { "container": "/rest/api/space/ENG" }
        }))
        .expect("a content record")
    }

    /// A search page reports `size` and a next link, never a total, so the
    /// walk is link-driven. The envelope is asserted with both halves present
    /// and both absent, because the absent case is the last page.
    #[test]
    fn the_envelope_is_a_next_link_and_never_a_total() {
        let page: ContentPage = serde_json::from_value(json!({
            "results": [a_page().raw],
            "start": 0, "limit": 1, "size": 1,
            "_links": {
                "next": "/rest/api/content/search?cql=type%3Dpage&start=1&limit=1",
                "base": "http://127.0.0.1:8090"
            }
        }))
        .unwrap();
        assert_eq!(page.results.len(), 1);
        assert_eq!(
            page.links.next.as_deref(),
            Some("/rest/api/content/search?cql=type%3Dpage&start=1&limit=1")
        );

        let last: ContentPage =
            serde_json::from_value(json!({ "results": [], "size": 0, "_links": {} })).unwrap();
        assert!(last.links.next.is_none(), "the last page has no next link");
        // A response with no `_links` at all is the last page too, not a parse
        // failure: the walk must end rather than abort.
        let bare: ContentPage = serde_json::from_value(json!({ "results": [] })).unwrap();
        assert!(bare.links.next.is_none());
    }

    /// The typed view reads the fields the mapping and the cursor need; the
    /// raw view keeps everything, including what this crate has never heard
    /// of. That is §3a's re-mapping guarantee, and it is what the next
    /// ticket's renderer reads.
    #[test]
    fn both_views_are_built_from_one_record() {
        let page = a_page();
        assert_eq!(page.content.id, "98307");
        assert_eq!(
            page.content.title.as_deref(),
            Some("SEPA payout retry design")
        );
        assert_eq!(page.storage(), "<p>hello</p>");
        let version = page.content.version.as_ref().unwrap();
        assert_eq!(version.number, Some(3));
        assert_eq!(
            version.when.as_deref(),
            Some("2026-08-22T12:40:00.000+02:00")
        );
        assert_eq!(
            page.content.links.as_ref().unwrap().webui.as_deref(),
            Some("/display/ENG/SEPA+payout+retry+design")
        );
        assert_eq!(page.raw["space"]["key"], "ENG");
        assert_eq!(page.raw["ancestors"][0]["title"], "Engineering");
        assert_eq!(page.raw["_expandable"]["container"], "/rest/api/space/ENG");
    }

    /// A Confluence upgrade adds fields to every one of these records.
    /// Rejecting an unknown key would make the adapter fail on the next
    /// upgrade -- the opposite of the config struct, where an unknown key is a
    /// bug.
    #[test]
    fn unknown_fields_do_not_break_parsing() {
        let page: RawContent = serde_json::from_value(json!({
            "id": "1", "brandNewTopLevel": 7, "version": { "number": 1, "somethingNew": true }
        }))
        .unwrap();
        assert_eq!(page.content.id, "1");
        assert_eq!(page.raw["brandNewTopLevel"], 7);
        assert_eq!(page.storage(), "", "a page with no body reads as no body");
    }

    /// Completion rewrites both views, at Confluence's own `children.comment`
    /// path -- so a payload this filled in and a payload the server expanded
    /// are indistinguishable to everything downstream.
    #[test]
    fn setting_comments_rewrites_the_payload_at_confluences_own_path() {
        let mut page = a_page();
        let comment: RawContent = serde_json::from_value(json!({
            "id": "98320",
            "type": "comment",
            "body": { "storage": { "value": "<p>@Mara can you add the SLA?</p>" } }
        }))
        .unwrap();
        page.set_comments(vec![comment]);
        assert_eq!(page.raw["children"]["comment"]["results"][0]["id"], "98320");
        assert_eq!(page.raw["children"]["comment"]["size"], 1);
        // The typed half moved with it, so the completion check does not
        // re-fetch this page's comments on every run.
        let container = page
            .content
            .children
            .as_ref()
            .unwrap()
            .comment
            .as_ref()
            .unwrap();
        assert_eq!(container.results.len(), 1);
        assert!(!container.truncated());
    }

    /// A record with no `children` object at all still gets one: the search
    /// response has none until something expands it.
    #[test]
    fn comments_can_be_set_on_a_record_that_had_no_children_object() {
        let mut page: RawContent = serde_json::from_value(json!({ "id": "1" })).unwrap();
        page.set_comments(Vec::new());
        assert_eq!(page.raw["children"]["comment"]["size"], 0);
        assert!(
            page.raw["children"]["comment"]["results"]
                .as_array()
                .is_some_and(std::vec::Vec::is_empty),
            "a page with no comments carries an empty container, not a missing one"
        );
    }

    /// The truncation signal is the next link and nothing else -- there is no
    /// total to compare a length against.
    #[test]
    fn a_comment_container_is_truncated_exactly_when_it_has_a_next_link() {
        let full: CommentContainer =
            serde_json::from_value(json!({ "results": [], "size": 0, "_links": {} })).unwrap();
        assert!(!full.truncated());
        let more: CommentContainer = serde_json::from_value(json!({
            "results": [], "size": 0,
            "_links": { "next": "/rest/api/content/1/child/comment?start=100" }
        }))
        .unwrap();
        assert!(more.truncated());
    }

    /// The mention walk's two fields, and the trap they exist for: a
    /// comment's container is the **page**, a page's container is its
    /// **space**, and the ids are from different namespaces. Reading a
    /// container without reading the type would follow a space id as a page's.
    #[test]
    fn a_comments_container_is_its_page_and_a_pages_container_is_its_space() {
        let comment: RawContent = serde_json::from_value(json!({
            "id": "98320", "type": "comment",
            "container": { "id": "98307", "type": "page", "title": "SEPA payout retry design" },
            "version": { "number": 1, "when": "2026-09-03T10:00:00.000Z" }
        }))
        .unwrap();
        assert_eq!(comment.content.content_type.as_deref(), Some("comment"));
        assert_eq!(
            comment
                .content
                .container
                .as_ref()
                .and_then(Container::content_id),
            Some("98307")
        );

        let page: RawContent = serde_json::from_value(json!({
            "id": "98307", "type": "page",
            "container": { "id": 98305, "key": "ENG", "type": "space" }
        }))
        .unwrap();
        assert_eq!(page.content.content_type.as_deref(), Some("page"));
        // A space id is a **number** in this response. It reads as no content
        // id at all, and -- the half that matters -- the record still parses:
        // typed as a string it would not, and one page with a space would fail
        // the whole walk.
        assert!(
            page.content
                .container
                .as_ref()
                .is_some_and(|c| c.content_id().is_none())
        );
    }

    #[test]
    fn the_current_user_response_names_the_account() {
        let me: CurrentUser = serde_json::from_value(json!({
            "type": "known", "username": "knobas", "userKey": "2c9…",
            "displayName": "knobas", "profilePicture": { "path": "/images/x.png" }
        }))
        .unwrap();
        assert_eq!(me.username.as_deref(), Some("knobas"));
        assert_eq!(me.display_name.as_deref(), Some("knobas"));
        assert_eq!(me.user_key.as_deref(), Some("2c9…"));
    }
}
