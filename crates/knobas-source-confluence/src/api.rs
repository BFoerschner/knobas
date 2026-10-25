//! The six calls this adapter's sync makes.
//!
//! A trait rather than a direct dependency on [`crate::http::ConfluenceHttp`],
//! because the interesting part of this adapter is paging, the ceiling and
//! cursor discipline -- and none of those needs a socket to be tested. The
//! tests in [`crate::sync`] use a fake; the **witness** is
//! `tests/live_confluence_seeded.rs` against the real container (ADR-0013).
//!
//! # The endpoint set, and every parameter it sends
//!
//! | Call | Path | Parameters |
//! |---|---|---|
//! | [`current_user`](ConfluenceApi::current_user) | `rest/api/user/current` | *(none)* |
//! | [`search`](ConfluenceApi::search) | `rest/api/content/search` | `cql`, `start`, `limit`, `expand` |
//! | [`follow`](ConfluenceApi::follow) | whatever `_links.next` said | *(the server's own)* |
//! | [`comments`](ConfluenceApi::comments) | `rest/api/content/{id}/child/comment` | `limit`, `start`, `expand` |
//! | [`content`](ConfluenceApi::content) | `rest/api/content/{id}` | `expand` |
//! | [`user_by_key`](ConfluenceApi::user_by_key) | `rest/api/user` | `key` |
//!
//! Atlassian publishes no machine-readable specification for this product, so
//! there is no mock that can record an invented parameter as a violation the
//! way `knobas-mockd` does for Jira. That is exactly why ADR-0013 makes the
//! real container the witness: the live suite calls every one of these against
//! Confluence itself.
//!
//! **`start` is sent by the page walk alone** (#1976): it asks each result
//! from the last page it read (`crate::walk`). The other walks follow
//! `_links.next`, which carries the server's own offset -- see
//! [`ConfluenceHttp::get_link`](crate::http::ConfluenceHttp::get_link) -- and
//! start at `0`.

use knobas_source::SourceError;

use crate::model::{ContentPage, CurrentUser, RawContent};

/// What the **comment walk** asks a record to carry.
///
/// Deliberately thin: it needs the id, the `version` its cursor entry is made
/// of, and the `container` a comment hangs off. The page it resolves to is
/// then fetched whole with [`EXPAND`], so a page a comment led to and a page
/// the ordinary walk delivered are mapped from byte-identical records rather
/// than from two shapes that have to agree.
pub(crate) const COMMENT_EXPAND: &str = "version,container";

/// What the **positions sweep** asks a record to carry (#1138): its place
/// among its siblings, and nothing else -- no body, so Data Center answers up
/// to 200 rows a request rather than 50.
pub(crate) const POSITIONS_EXPAND: &str = "extensions.position";

/// What a page's record is asked to carry.
///
/// * `body.storage` -- the storage format **verbatim**, which is what the
///   payload keeps and the next ticket renders.
/// * `ancestors` -- the path to the page, which the launcher shows.
/// * `space` -- ADR-0010's project for this source; a space is what a Jira
///   project is, in Confluence's own word.
/// * `version` -- `number` and `when`: the cursor's identity and the value
///   CQL's `lastmodified` matches.
/// * `history` -- who created the page, for the pages nobody has edited since.
/// * `children.comment.body.storage` -- the discussion, in one request instead
///   of one per page. A server that will not expand this deeply is not a
///   failure: [`crate::sync`] completes what it did not get, which is the
///   Jira adapter's `complete` under another name.
/// * `children.comment.version` and `children.comment.history` -- **when each
///   of those comments was written, and by whom.** Two tickets asked for this
///   expansion for two different reasons and both are kept, because they are
///   the same fact read twice.
///
///   *When* is #287's: `body_text` has always carried the discussion, but
///   without its dates the record could not say when it last changed, and a
///   page whose only recent event is a comment would be dated by an edit a
///   year old. Jira answers this natively -- a comment moves an issue's
///   `fields.updated` -- and this is what lets a Confluence page answer the
///   same question ([`crate::map`]).
///
///   *By whom* is #286's: the page's own record has carried its author since
///   #284 and a comment's had none, so the detail's comment section could
///   render the words and nothing else. `history` is the creator fallback for
///   a comment nobody has edited since, exactly as it is for a page.
///
///   These are the same expansions [`ConfluenceApi::comments`] asks for on the
///   completion path, which is what makes a comment that arrived with its page
///   and one that arrived through completion the same shape downstream. A
///   server that will not expand them falls back to the page's own stamp and
///   to no byline, which is the behaviour before they were asked for.
///
/// * `metadata.labels` -- the page's labels, which the Docs reader draws in
///   its header (#859). A server that will not expand them reads as a page
///   with no labels.
pub const EXPAND: &str = concat!(
    "body.storage,ancestors,space,version,history,metadata.labels,",
    "children.comment.body.storage,children.comment.version,children.comment.history"
);

#[async_trait::async_trait]
pub(crate) trait ConfluenceApi: Send + Sync {
    /// `GET /rest/api/user/current` -- who the credential is.
    ///
    /// **The first call of every sync run**, and not only of
    /// `test_connection`. A content search is allowed to be asked
    /// anonymously on an instance with anonymous access, and answers an empty
    /// result set rather than a 401; the `page` kind claims
    /// `full_sync_exhaustive`, so an empty run reported `Ok` is the engine's
    /// licence to tombstone every page in the mirror. This call is the one
    /// that cannot answer emptily.
    async fn current_user(&self) -> Result<CurrentUser, SourceError>;
    /// `GET /rest/api/content/search?cql=…&start=…&limit=…&expand=…`.
    async fn search(
        &self,
        cql: &str,
        start: u32,
        limit: u32,
        expand: &str,
    ) -> Result<ContentPage, SourceError>;
    /// The `_links.next` of a page this run already received.
    async fn follow(&self, path_and_query: &str) -> Result<ContentPage, SourceError>;
    /// `GET /rest/api/content/{id}/child/comment` -- the completion path, for
    /// a server that did not expand the comments with the page.
    async fn comments(&self, id: &str, start: u32, limit: u32) -> Result<ContentPage, SourceError>;
    /// `GET /rest/api/content/{id}?expand=…` -- one record by id.
    ///
    /// The comment walk's second half: a comment new or edited since the
    /// cursor is found by CQL, and the **page** it hangs off is what becomes
    /// the item, fetched here. A search cannot do it -- `id = <n>` is CQL, but
    /// a comment on a page nobody edited is precisely the case whose page the
    /// walk's own bound excludes.
    async fn content(&self, id: &str, expand: &str) -> Result<RawContent, SourceError>;
    /// `GET /rest/api/user?key=…` -- the user a mention's `ri:userkey` names
    /// (#953), as Confluence's own user record. Data Center has no bulk form
    /// of this call, so [`crate::users::Directory`] asks once per key per run.
    async fn user_by_key(&self, key: &str) -> Result<serde_json::Value, SourceError>;
}

/// The real endpoints.
///
/// The content id is interpolated into the comment path. Ids come from
/// Confluence's own responses and are decimal, so no escaping is required; an
/// id that is not one would 404 and be reported as
/// [`SourceError::Protocol`].
#[async_trait::async_trait]
impl ConfluenceApi for crate::http::ConfluenceHttp {
    async fn current_user(&self) -> Result<CurrentUser, SourceError> {
        self.get_json("rest/api/user/current", &[]).await
    }

    async fn search(
        &self,
        cql: &str,
        start: u32,
        limit: u32,
        expand: &str,
    ) -> Result<ContentPage, SourceError> {
        self.get_json(
            "rest/api/content/search",
            &[
                ("cql", cql.to_owned()),
                ("start", start.to_string()),
                ("limit", limit.to_string()),
                ("expand", expand.to_owned()),
            ],
        )
        .await
    }

    async fn follow(&self, path_and_query: &str) -> Result<ContentPage, SourceError> {
        self.get_link(path_and_query).await
    }

    async fn comments(&self, id: &str, start: u32, limit: u32) -> Result<ContentPage, SourceError> {
        self.get_json(
            &format!("rest/api/content/{id}/child/comment"),
            &[
                ("start", start.to_string()),
                ("limit", limit.to_string()),
                ("expand", "body.storage,version,history".to_owned()),
            ],
        )
        .await
    }

    async fn content(&self, id: &str, expand: &str) -> Result<RawContent, SourceError> {
        self.get_json(
            &format!("rest/api/content/{id}"),
            &[("expand", expand.to_owned())],
        )
        .await
    }

    async fn user_by_key(&self, key: &str) -> Result<serde_json::Value, SourceError> {
        self.get_json("rest/api/user", &[("key", key.to_owned())])
            .await
    }
}
