//! One incremental (or full) sync run.
//!
//! The shape, and why each part is there:
//!
//! 1. `GET /rest/api/user/current` -- the credential check that a content
//!    search cannot be. A search may be answered anonymously with an empty
//!    result set, and the `page` kind claims `full_sync_exhaustive`, so an
//!    empty run reported `Ok` is the engine's licence to tombstone the whole
//!    mirror. This call is first for that reason, and the live suite pins the
//!    ordering so a later "optimization" cannot quietly remove the guard.
//! 2. **The ceiling probe**: one row, the same scope, `order by lastmodified
//!    desc`. It answers two questions at once -- what the newest page in this
//!    corpus was when the run started (the ceiling), and which UTC offset this
//!    instance renders timestamps in (the zone every CQL literal is read in).
//! 3. Read the cursor. Unreadable or absent ⇒ full sync (contract §4.1).
//! 4. Query `lastmodified >= watermark − 2 minutes order by lastmodified asc`,
//!    following `_links.next` until there is none.
//! 5. Drop what the overlap re-delivered, complete the comments the server did
//!    not expand, push the rest into the sink.
//! 6. **The mention walk** ([`SyncRun::mentions`]): one more query, `mention =
//!    currentUser() AND type in (page, comment)` over the same window, whose
//!    results are resolved to the pages they are on and pushed too.
//! 7. If nothing was pushed, hand back the cursor byte-identically. Otherwise
//!    advance the watermark to `min(max(previous, newest emitted), ceiling)`.
//!
//! # Why the mention walk is a second query and not a wider first one
//!
//! A mention usually lives in a **comment**, and a comment is separate content
//! in Confluence: posting one does not move the page's `lastmodified`. So the
//! page walk -- bounded by a page's own `lastmodified` -- can never reach a
//! comment posted on a page nobody has edited since, which is most pages. The
//! second query is bounded by the *comment's* timestamp instead, and that is
//! the only bound that reaches it (`crate::cql::build_mention_cql`).
//!
//! It emits the **page**, never the comment: this adapter declares one entity
//! kind, a comment belongs to its page's payload (`crate::KIND_PAGE`), and the
//! thing a person opens from the inbox is the page the discussion is on.
//!
//! **It does not move the watermark.** The two walks share one cursor, and the
//! records the mention walk sees are newer than the page edits the page walk
//! is bounded by -- advancing on a comment posted today would put every page
//! edited yesterday below the next run's lower bound and lose it for good.
//! What the mention walk does contribute is its `seen` entries, which is what
//! makes the *next* run skip the same comment and leaves an idle poll idle
//! (battery clause 2).
//!
//! # The ceiling, and what it is for (`CONTEXT.md`, *Watermark*)
//!
//! "Its **ceiling** is the newest position the run *witnessed* at its start,
//! which the watermark may never pass within that run."
//!
//! Concretely: a run is not instantaneous. While the walk is paging, somebody
//! edits a page; an `asc` ordering moves it toward the end, so the walk
//! usually reaches it, and its `version.when` is later than anything the run
//! saw at its start. Advancing the watermark to *that* would put the run's
//! whole duration below the next query's lower bound -- so every other edit
//! made during the run, by anyone, would never be offered again. Clamping to
//! the ceiling means the next run re-walks from just before run start.
//!
//! The ceiling is deliberately **witnessed, not clocked**: the newest
//! `lastmodified` the probe saw, not `now()`. A ceiling too low costs a
//! re-fetch; one too high loses work -- and `now()` is guaranteed too high the
//! moment the instance's clock and knobas' disagree.
//!
//! # What the ceiling does not close
//!
//! Offset paging over the field being ordered by can *step over* a row: a page
//! at the front of the order is edited, moves to the end, every row behind it
//! shifts down by one, and the walk's next offset lands one past where it
//! should. The stepped-over page keeps its old timestamp, so no incremental
//! query reaches it either -- only the next full sync does. That is the same
//! class the Jira adapter accepts with `startAt` and `ORDER BY updated ASC`
//! (contract §4.2), and
//! `a_page_edited_mid_walk_can_shift_a_row_past_the_offset` records it rather
//! than leaving the next reader to believe the walk is airtight.
//!
//! # The two things the cursor layer is owed
//!
//! [`ConfluenceCursor::since`] documents them and this is where they are
//! honoured; they fail in different places, so neither substitutes for the
//! other.
//!
//! * **`max(previous, …)`** on the watermark, so a re-delivered *older* page
//!   cannot drag the position backwards.
//! * **`seen_in_window` is every record the run saw**, skipped ones included.
//!   A run that reports only what it emitted forgets, every run, whatever it
//!   recognised that run, and an idle poll never settles. The watermark holds
//!   perfectly while that happens, which is why the sequence that shows it is
//!   *one edited page and then an idle poll*, never a full sync and then one.

use chrono::{DateTime, Utc};
use knobas_source::{Sink, SourceError};

use crate::ConfluenceConfig;
use crate::api::{ConfluenceApi, EXPAND, MENTION_EXPAND};
use crate::cql::{Order, build_cql, build_mention_cql};
use crate::cursor::{ConfluenceCursor, Seen};
use crate::map;
use crate::model::{Container, RawContent};
use crate::storage::Account;

/// A stop so a server that keeps handing out `_links.next` cannot spin a run
/// forever. At the default page size this is a quarter of a million pages.
///
/// Reaching it is a **failure**, never a quiet end to the walk. The one kind
/// this adapter emits claims `full_sync_exhaustive: true`, which is the
/// engine's licence to tombstone every row a `cursor: None` run did not
/// return -- so a truncated walk reported as `Ok` authorises deleting whatever
/// fell off the end. The query is `order by lastmodified asc`, so what falls
/// off the end is the *most recently modified* work: the pages someone is
/// reading today.
///
/// An incremental run gets the same refusal, and unconditionally: it is the
/// only thing that terminates a loop whose other exit is the server's own
/// "there is more".
const MAX_PAGES: u32 = 5_000;

/// How many comments one completion request asks for.
///
/// Larger than the search page size on purpose: comments are small, they are
/// not body-expanded content, and the cap that forces fifty on a search does
/// not apply. A page with a long discussion then costs one request, not four.
const COMMENT_PAGE_SIZE: u32 = 100;

pub(crate) struct SyncRun<'a> {
    pub api: &'a (dyn ConfluenceApi + 'a),
    pub cfg: &'a ConfluenceConfig,
    /// The instance id -- the `EntityRef` namespace (P10), not the adapter
    /// kind.
    pub source_id: &'a str,
    pub base_url: &'a str,
}

/// What the run-start probe witnessed.
///
/// Named for `CONTEXT.md`'s **Watermark** entry -- "the newest position the
/// run *witnessed* at its start" -- rather than for a synonym of its own: this
/// is the module that most needs the glossary's word for the thing, and a
/// second word for it here is how the next reader comes to believe there are
/// two concepts.
struct RunStart {
    /// The newest `version.when` in this source's corpus when the run started,
    /// or `None` for a corpus with no page in it at all.
    ceiling: Option<DateTime<Utc>>,
    /// The UTC offset the instance renders timestamps in -- the zone every CQL
    /// literal will be read back in.
    offset_secs: i32,
}

impl SyncRun<'_> {
    pub(crate) async fn run(
        &self,
        cursor: Option<String>,
        sink: &mut (dyn Sink + Send),
    ) -> Result<String, SourceError> {
        // First, and not only for the identity: a content search can be
        // answered anonymously with an empty page, and this call cannot. The
        // identity is now read off it too -- a Confluence mention carries a
        // user **key**, and this is the one call that says which key the
        // credential is. The *server's* answer and not the configured
        // `username`, because it is the server that owns the key/name pairing;
        // `test_connection` fills the configuration in from this same call, so
        // the two agree by construction.
        let whoami = self.api.current_user().await?;
        let me = whoami.username.as_deref().map(|username| Account {
            username,
            user_key: whoami.user_key.as_deref(),
        });

        let witnessed = self.run_start().await?;
        let offset = witnessed.offset_secs;
        let previous = cursor.as_deref().and_then(ConfluenceCursor::parse);
        let since = previous.as_ref().and_then(|c| c.since(offset));
        let cql = build_cql(self.cfg, since, offset, Order::Ascending);

        let mut emitted: usize = 0;
        // The entity ids this run has already handed to the sink, so the
        // mention walk does not re-deliver a page the page walk just sent --
        // one wasted request and one wasted upsert per mentioning page on
        // every full sync, otherwise.
        let mut delivered: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        // Every record this run *saw* in the window -- see the module docs.
        // Skipped ones belong here as much as emitted ones.
        let mut seen_in_window: Vec<Seen> = Vec::new();
        // Seeded from the previous position, which is what makes the new one
        // `max(previous, newest emitted)` rather than "newest emitted".
        let mut watermark: Option<DateTime<Utc>> = previous.as_ref().and_then(|c| c.modified_to);

        let mut next: Option<String> = None;
        let mut pages: u32 = 0;
        loop {
            let page = match &next {
                None => self.api.search(&cql, self.cfg.page_size, EXPAND).await?,
                Some(link) => self.api.follow(link).await?,
            };
            for mut raw in page.results {
                let version = raw.content.version.as_ref();
                let number = version.and_then(|v| v.number);
                let when = version
                    .and_then(|v| v.when.as_deref())
                    .and_then(crate::time::parse_time);
                if let Some(u) = when {
                    seen_in_window.push(Seen {
                        i: raw.content.id.clone(),
                        n: number,
                        u,
                    });
                }
                // The overlap exists so nothing is missed; this is what keeps
                // it from also meaning "everything arrives twice".
                if previous
                    .as_ref()
                    .is_some_and(|c| c.already_delivered(&raw.content.id, number))
                {
                    continue;
                }
                self.complete(&mut raw).await?;
                let item = map::to_sync_item(self.source_id, self.base_url, &raw, me);
                // Sink failures are never swallowed (SPI doc on `Source::sync`).
                delivered.insert(raw.content.id.clone());
                sink.item(item).await?;
                emitted += 1;
                if let Some(u) = when {
                    watermark = Some(watermark.map_or(u, |w| w.max(u)));
                }
            }
            let Some(link) = page.links.next else { break };
            pages += 1;
            if pages >= MAX_PAGES {
                // Not a `break`: leaving the loop here and returning `Ok`
                // would hand the engine a partial walk wearing a completed
                // one's clothes, and the sweep would tombstone the rest.
                return Err(SourceError::protocol(format!(
                    "Confluence kept offering another page of results after {MAX_PAGES} of \
                     {}; refusing to report a partial walk as a completed sync. Narrow this \
                     source with a space list.",
                    self.cfg.page_size
                )));
            }
            next = Some(link);
        }

        emitted += self
            .mentions(
                since,
                offset,
                previous.as_ref(),
                me,
                &mut delivered,
                &mut seen_in_window,
                sink,
            )
            .await?;

        match (emitted, watermark) {
            // Battery clause 2: nothing happened, so the position did not
            // move. Returning the *input string* rather than a re-encoded
            // struct makes that byte-identical by construction.
            (0, _) => Ok(cursor.unwrap_or_else(|| ConfluenceCursor::empty(offset).encode())),
            // Items arrived, but neither they nor the previous cursor carry a
            // readable `version.when`; advancing to anything would be a guess.
            (_, None) => Ok(cursor.unwrap_or_else(|| ConfluenceCursor::empty(offset).encode())),
            (_, Some(w)) => {
                // The ceiling clamp. An edit made *during* the walk carries a
                // stamp later than anything the run saw at its start;
                // advancing to it would put the run's own duration below the
                // next query's lower bound and hide every other edit made
                // while it ran.
                let clamped = witnessed.ceiling.map_or(w, |ceiling| w.min(ceiling));
                // ...but never **backwards**. The ceiling is the newest page
                // in the corpus *now*, and that can be older than the
                // watermark -- the page which set it was deleted, or moved out
                // of the configured spaces. Clamping to it unconditionally
                // would then drag the position back, which is the one thing
                // the `max(previous, …)` rule above exists to prevent
                // ([`ConfluenceCursor::since`], condition 1). The cost of
                // refusing is a ceiling that does not bind on that one run;
                // the cost of accepting is a source that re-delivers its whole
                // recent history on every poll and never settles.
                let floor = previous.as_ref().and_then(|c| c.modified_to);
                let clamped = floor.map_or(clamped, |previous| clamped.max(previous));
                Ok(ConfluenceCursor::advanced(clamped, offset, &seen_in_window).encode())
            }
        }
    }

    /// The mention walk: every page and comment that names this account since
    /// the cursor, emitted as the **page** it is on.
    ///
    /// Answers how many items it pushed. The module docs carry why this is a
    /// second query and why it must not move the watermark; the three
    /// decisions that live here:
    ///
    /// * **`seen` is fed from the record CQL returned**, comment or page, and
    ///   the skip is checked against the same identity. That is what makes an
    ///   idle poll idle: the page a comment resolves to keeps its own old
    ///   `version.when` and would fall out of the `seen` window immediately,
    ///   so deduplicating on the *page* would re-emit it on every run for
    ///   ever. Deduplicating on the comment cannot: its timestamp is newer
    ///   than the watermark, so it stays in `seen` exactly as long as the
    ///   query keeps returning it.
    /// * **The page is fetched whole** rather than mapped from the thin
    ///   `MENTION_EXPAND` record, so a mentioning page and a page the ordinary
    ///   walk delivered are mapped from the same shape -- and the comment
    ///   carrying the mention is in the payload where every other comment is.
    /// * **A failure here fails the run**, and is not swallowed into a partial
    ///   success. `mention` is a CQL field Confluence Data Center has had for
    ///   as long as CQL, and `tests/live_confluence_seeded.rs` is the witness
    ///   that this instance answers it; an instance that does not would leave
    ///   the source stuck rather than quietly mention-blind, which is the
    ///   direction a person can see and act on.
    async fn mentions(
        &self,
        since: Option<DateTime<Utc>>,
        offset_secs: i32,
        previous: Option<&ConfluenceCursor>,
        me: Option<Account<'_>>,
        delivered: &mut std::collections::BTreeSet<String>,
        seen_in_window: &mut Vec<Seen>,
        sink: &mut (dyn Sink + Send),
    ) -> Result<usize, SourceError> {
        let cql = build_mention_cql(self.cfg, since, offset_secs);
        let mut emitted: usize = 0;
        let mut next: Option<String> = None;
        let mut requests: u32 = 0;
        loop {
            let page = match &next {
                None => {
                    self.api
                        .search(&cql, self.cfg.page_size, MENTION_EXPAND)
                        .await?
                }
                Some(link) => self.api.follow(link).await?,
            };
            for raw in &page.results {
                let version = raw.content.version.as_ref();
                let number = version.and_then(|v| v.number);
                if let Some(u) = version
                    .and_then(|v| v.when.as_deref())
                    .and_then(crate::time::parse_time)
                {
                    seen_in_window.push(Seen {
                        i: raw.content.id.clone(),
                        n: number,
                        u,
                    });
                }
                if previous.is_some_and(|c| c.already_delivered(&raw.content.id, number)) {
                    continue;
                }
                let Some(page_id) = mentioning_page(&raw.content) else {
                    continue;
                };
                if !delivered.insert(page_id.clone()) {
                    continue;
                }
                let mut target = self.api.content(&page_id, EXPAND).await?;
                self.complete(&mut target).await?;
                let item = map::to_sync_item(self.source_id, self.base_url, &target, me);
                sink.item(item).await?;
                emitted += 1;
            }
            let Some(link) = page.links.next else { break };
            requests += 1;
            if requests >= MAX_PAGES {
                return Err(SourceError::protocol(format!(
                    "Confluence kept offering another page of mention results after \
                     {MAX_PAGES}; refusing to report a partial walk as a completed sync."
                )));
            }
            next = Some(link);
        }
        Ok(emitted)
    }

    /// The run-start probe: the newest page in this source's scope, and the
    /// zone this instance renders timestamps in.
    ///
    /// One row, ordered the other way, over the same scope -- **the same
    /// scope** matters: a ceiling taken from a space this source does not sync
    /// would be a clamp that never binds.
    ///
    /// A corpus with no page in it yields no ceiling and no offset, and the
    /// offset then falls back to [`crate::time::MIN_UTC_OFFSET_SECS`] rather
    /// than to UTC, for the reason that constant gives: guessing the zone high
    /// skips edits permanently, guessing it low only re-reads them. An empty
    /// corpus has nothing to re-read, so the fallback costs nothing at all --
    /// and the moment a page exists, the probe answers.
    async fn run_start(&self) -> Result<RunStart, SourceError> {
        let probe = build_cql(self.cfg, None, 0, Order::Descending);
        let page = self.api.search(&probe, 1, "version").await?;
        let when = page
            .results
            .first()
            .and_then(|c| c.content.version.as_ref())
            .and_then(|v| v.when.as_deref());
        Ok(RunStart {
            ceiling: when.and_then(crate::time::parse_time),
            offset_secs: when
                .and_then(crate::time::parse_offset_secs)
                .unwrap_or(crate::time::MIN_UTC_OFFSET_SECS),
        })
    }

    /// Fill in the comments the search did not expand.
    ///
    /// A container that is **absent** is treated as truncated: a server that
    /// ignored `expand=children.comment.body.storage` must not be read as
    /// "this page has no comments" -- that would silently drop the discussion
    /// out of the search corpus, and Confluence's deep-expand support is
    /// exactly the kind of thing a version bump changes.
    ///
    /// A container that is present and carries a `_links.next` is truncated in
    /// the ordinary sense, and is re-fetched whole rather than continued: the
    /// dedicated endpoint's own paging is simpler than splicing two shapes,
    /// and a page with more than a hundred comments is not the common case.
    async fn complete(&self, raw: &mut RawContent) -> Result<(), SourceError> {
        let truncated = raw
            .content
            .children
            .as_ref()
            .and_then(|c| c.comment.as_ref())
            .is_none_or(crate::model::CommentContainer::truncated);
        if !truncated {
            return Ok(());
        }
        let id = raw.content.id.clone();
        let mut all: Vec<RawContent> = Vec::new();
        let mut start: u32 = 0;
        loop {
            let page = self.api.comments(&id, start, COMMENT_PAGE_SIZE).await?;
            let returned = u32::try_from(page.results.len()).unwrap_or(u32::MAX);
            all.extend(page.results);
            if page.links.next.is_none() || returned == 0 {
                break;
            }
            start += returned;
        }
        raw.set_comments(all);
        Ok(())
    }
}

/// Which page a mention-walk result is *on*, or `None` for a record this walk
/// cannot place.
///
/// **The type is read first, and that is the whole point.** A comment's
/// `container` is the page it hangs off; a **page's** container is its *space*,
/// whose id is from another namespace entirely. A walk that read a container id
/// without asking what it was holding would fetch a space id as a page and
/// either 404 or, worse, mirror something that is not the page.
///
/// A record whose type the walk never asked for -- a blog post, an attachment,
/// a type a later Confluence adds -- contributes nothing: this adapter emits
/// one kind, and the alternative is inventing a page for something that has
/// none.
fn mentioning_page(content: &crate::model::Content) -> Option<String> {
    match content.content_type.as_deref() {
        Some("comment") => content
            .container
            .as_ref()
            .and_then(Container::content_id)
            .map(str::to_owned),
        Some("page") => Some(content.id.clone()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use knobas_source::SyncItem;
    use knobas_source::contract::VecSink;
    use serde_json::{Value, json};
    use std::sync::Mutex;

    /// A fake Confluence.
    ///
    /// It answers from a list of pages it holds, honouring the CQL's
    /// `lastmodified >=` bound, its ordering and the limit -- because those
    /// three are what the run's correctness rests on, and a fake that ignored
    /// them would agree with any implementation. It records every call, so the
    /// tests can assert the *order* of them, which is where the anonymous-search
    /// hazard lives.
    ///
    /// It is not a witness for what Confluence does: that is
    /// `tests/live_confluence_seeded.rs` (ADR-0013). It is a witness for what
    /// **this run** does with what a server says.
    struct Fake {
        pages: Mutex<Vec<Value>>,
        /// What `mention = currentUser()` answers: comment records carrying
        /// the page they are on in `container`, and page records naming the
        /// account in their own body. Empty in every test that is not about
        /// mentions, which is how the mention walk stays invisible to them.
        mentions: Mutex<Vec<Value>>,
        /// Comments the search does *not* expand, by content id -- the
        /// completion path's input.
        detached_comments: Mutex<std::collections::BTreeMap<String, Vec<Value>>>,
        calls: Mutex<Vec<String>>,
        /// Set to fail every call, `current_user` included.
        fault: Option<SourceError>,
        /// Set to fail **only** `current_user`, leaving the search answering
        /// normally -- the shape of the server this run's call ordering exists
        /// for: one that reads a content search anonymously and answers it
        /// with an empty page. `fault` cannot express that, because it refuses
        /// the search too, and a run refused by its *search* would end in
        /// `Err` with the identity check deleted.
        identity_fault: Option<SourceError>,
        /// Pages whose `version.when` moves forward the moment the walk asks
        /// for its second page -- the mid-run edit the ceiling exists for.
        edit_mid_walk: Mutex<Option<(String, String)>>,
    }

    impl Fake {
        fn new(pages: Vec<Value>) -> Self {
            Self {
                pages: Mutex::new(pages),
                mentions: Mutex::new(Vec::new()),
                detached_comments: Mutex::new(std::collections::BTreeMap::new()),
                calls: Mutex::new(Vec::new()),
                fault: None,
                identity_fault: None,
                edit_mid_walk: Mutex::new(None),
            }
        }

        /// The records `mention = currentUser()` will answer with.
        fn mentioning(mut self, rows: Vec<Value>) -> Self {
            self.mentions = Mutex::new(rows);
            self
        }

        fn calls(&self) -> Vec<String> {
            self.calls.lock().unwrap().clone()
        }

        /// The rows a CQL asks for, ordered and bounded the way the server
        /// would order and bound them.
        ///
        /// `expand` is honoured for the **comment container**, and that is not
        /// decoration: it is the only thing offline that can tell whether the
        /// adapter still asks for the discussion. A fake that answered the
        /// same rows whatever it was asked would agree with an adapter that
        /// had stopped asking, and the cost -- one extra request per page,
        /// every run, for ever -- is invisible in a payload assertion because
        /// the completion path fills the container in either way.
        fn matching(
            &self,
            cql: &str,
            expand: &str,
            start: usize,
            limit: usize,
        ) -> (Vec<Value>, bool) {
            let pages = if is_mention_query(cql) {
                self.mentions.lock().unwrap().clone()
            } else {
                self.pages.lock().unwrap().clone()
            };
            let bound = cql
                .split_once("lastmodified >= \"")
                .map(|(_, rest)| rest.split('"').next().unwrap_or_default().to_owned());
            let mut rows: Vec<Value> = pages
                .into_iter()
                .filter(|p| match &bound {
                    // The fake reads the literal as UTC, which is the zone
                    // every timestamp in these tests is written in.
                    Some(literal) => when_of(p) >= format!("{literal}:00Z").replace(' ', "T"),
                    None => true,
                })
                .collect();
            rows.sort_by_key(when_of);
            if cql.contains("desc") {
                rows.reverse();
            }
            let total = rows.len();
            let mut page: Vec<Value> = rows.into_iter().skip(start).take(limit).collect();
            if !expand.contains("children.comment") {
                for row in &mut page {
                    if let Some(object) = row.as_object_mut() {
                        object.remove("children");
                    }
                }
            }
            let more = start + page.len() < total;
            (page, more)
        }
    }

    fn when_of(page: &Value) -> String {
        page["version"]["when"]
            .as_str()
            .unwrap_or_default()
            .to_owned()
    }

    /// One page record, as the server would answer it with the expansions
    /// [`EXPAND`] asks for.
    fn page(id: &str, title: &str, when: &str, number: u64) -> Value {
        json!({
            "id": id,
            "type": "page",
            "title": title,
            "space": { "key": "ENG", "name": "Engineering" },
            "body": { "storage": { "value": format!("<p>{title}</p>"), "representation": "storage" } },
            "version": { "number": number, "when": when, "by": { "username": "knobas" } },
            "ancestors": [{ "id": "1", "title": "Engineering" }],
            "children": { "comment": { "results": [], "size": 0, "_links": {} } },
            "_links": { "webui": format!("/display/ENG/{id}") }
        })
    }

    /// The same, but without the expanded comment container -- what a server
    /// that will not expand that deeply answers.
    fn page_without_comments(id: &str, title: &str, when: &str, number: u64) -> Value {
        let mut p = page(id, title, when, number);
        p.as_object_mut().unwrap().remove("children");
        p
    }

    #[async_trait::async_trait]
    impl ConfluenceApi for Fake {
        async fn current_user(&self) -> Result<crate::model::CurrentUser, SourceError> {
            self.calls.lock().unwrap().push("current_user".to_owned());
            if let Some(fault) = self.identity_fault.as_ref().or(self.fault.as_ref()) {
                return Err(clone_fault(fault));
            }
            Ok(serde_json::from_value(json!({ "username": "knobas" })).unwrap())
        }

        async fn search(
            &self,
            cql: &str,
            limit: u32,
            expand: &str,
        ) -> Result<crate::model::ContentPage, SourceError> {
            self.calls.lock().unwrap().push(format!("search {cql}"));
            if let Some(fault) = &self.fault {
                return Err(clone_fault(fault));
            }
            assert!(
                cql.contains("type = page") || is_mention_query(cql),
                "every query this adapter sends is the page walk's or the mention walk's: {cql}"
            );
            let (rows, more) = self.matching(cql, expand, 0, limit as usize);
            Ok(envelope(
                rows,
                more.then(|| next_link(cql, expand, limit, limit)),
            ))
        }

        async fn follow(
            &self,
            path_and_query: &str,
        ) -> Result<crate::model::ContentPage, SourceError> {
            self.calls
                .lock()
                .unwrap()
                .push(format!("follow {path_and_query}"));
            // The mid-run edit: the moment the walk asks for a second page,
            // one of the pages is modified and jumps to the end of the order.
            if let Some((id, when)) = self.edit_mid_walk.lock().unwrap().take() {
                let mut pages = self.pages.lock().unwrap();
                for p in pages.iter_mut() {
                    if p["id"] == json!(id.clone()) {
                        p["version"]["when"] = json!(when.clone());
                        p["version"]["number"] = json!(99);
                    }
                }
            }
            let (cql, expand, start, limit) = parse_link(path_and_query);
            let (rows, more) = self.matching(&cql, &expand, start, limit);
            let (start, limit) = (
                u32::try_from(start + limit).unwrap_or(u32::MAX),
                u32::try_from(limit).unwrap_or(u32::MAX),
            );
            Ok(envelope(
                rows,
                more.then(|| next_link(&cql, &expand, start, limit)),
            ))
        }

        async fn comments(
            &self,
            id: &str,
            _start: u32,
            _limit: u32,
        ) -> Result<crate::model::ContentPage, SourceError> {
            self.calls.lock().unwrap().push(format!("comments {id}"));
            let held = self.detached_comments.lock().unwrap();
            let rows = held.get(id).cloned().unwrap_or_default();
            Ok(envelope(rows, None))
        }

        async fn content(
            &self,
            id: &str,
            expand: &str,
        ) -> Result<crate::model::RawContent, SourceError> {
            self.calls.lock().unwrap().push(format!("content {id}"));
            if let Some(fault) = &self.fault {
                return Err(clone_fault(fault));
            }
            let pages = self.pages.lock().unwrap().clone();
            let mut row = pages
                .into_iter()
                .find(|p| p["id"] == json!(id))
                .ok_or_else(|| SourceError::protocol(format!("no content {id}")))?;
            if !expand.contains("children.comment") {
                if let Some(object) = row.as_object_mut() {
                    object.remove("children");
                }
            }
            Ok(serde_json::from_value(row).expect("the fake holds well-formed records"))
        }
    }

    /// Whether a CQL is the mention walk's, read the way the fake's `search`
    /// has to read it: by the clause only that walk sends.
    fn is_mention_query(cql: &str) -> bool {
        cql.contains("mention = currentUser()")
    }

    fn clone_fault(fault: &SourceError) -> SourceError {
        match fault {
            SourceError::Unauthorized { status } => SourceError::Unauthorized { status: *status },
            other => SourceError::protocol(other.to_string()),
        }
    }

    fn envelope(rows: Vec<Value>, next: Option<String>) -> crate::model::ContentPage {
        let mut links = serde_json::Map::new();
        if let Some(next) = next {
            links.insert("next".to_owned(), json!(next));
        }
        serde_json::from_value(json!({
            "results": rows,
            "size": 0,
            "_links": Value::Object(links)
        }))
        .expect("the fake builds a well-formed envelope")
    }

    /// The continuation link Confluence builds: its own cql, expand, start and
    /// limit. The expansions ride along, which is what makes the walk's later
    /// pages carry what its first one did.
    fn next_link(cql: &str, expand: &str, start: u32, limit: u32) -> String {
        format!(
            "/rest/api/content/search?cql={}&start={start}&limit={limit}&expand={expand}",
            cql.replace(' ', "%20")
        )
    }

    fn parse_link(link: &str) -> (String, String, usize, usize) {
        let query = link.split_once('?').expect("a link has a query").1;
        let (mut cql, mut expand) = (String::new(), String::new());
        let (mut start, mut limit) = (0usize, 50usize);
        for pair in query.split('&') {
            let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
            match key {
                "cql" => cql = value.replace("%20", " "),
                "expand" => expand = value.to_owned(),
                "start" => start = value.parse().unwrap_or(0),
                "limit" => limit = value.parse().unwrap_or(50),
                _ => {}
            }
        }
        (cql, expand, start, limit)
    }

    fn cfg(value: Value) -> ConfluenceConfig {
        ConfluenceConfig::from_json(&value).expect("a valid configuration")
    }

    async fn run(
        fake: &Fake,
        cfg: &ConfluenceConfig,
        cursor: Option<String>,
    ) -> Result<(Vec<SyncItem>, String), SourceError> {
        let mut sink = VecSink(Vec::new());
        let next = SyncRun {
            api: fake,
            cfg,
            source_id: "confluence",
            base_url: "http://127.0.0.1:8090",
        }
        .run(cursor, &mut sink)
        .await?;
        Ok((sink.0, next))
    }

    fn ids(items: &[SyncItem]) -> Vec<String> {
        items.iter().map(|i| i.entity.key.clone()).collect()
    }

    fn watermark(cursor: &str) -> String {
        serde_json::from_str::<Value>(cursor).unwrap()["modified_to"]
            .as_str()
            .unwrap_or_else(|| panic!("not a cursor this adapter wrote: {cursor}"))
            .to_owned()
    }

    fn a_corpus() -> Vec<Value> {
        vec![
            page("100", "On-call handbook", "2026-07-02T00:00:00.000Z", 1),
            page(
                "101",
                "Payments architecture overview",
                "2026-08-11T00:00:00.000Z",
                1,
            ),
            page(
                "102",
                "Ledger reconciliation runbook",
                "2026-08-19T00:00:00.000Z",
                1,
            ),
            page("103", "Standup protocols", "2026-08-21T00:00:00.000Z", 1),
            page(
                "104",
                "SEPA payout retry design",
                "2026-08-22T10:40:00.000Z",
                3,
            ),
        ]
    }

    /// **The anonymous-search hazard.** A content search may be answered
    /// without a credential and answer an empty result set; the `page` kind
    /// claims `full_sync_exhaustive`, so an empty run reported `Ok` is the
    /// engine's licence to tombstone the mirror. The run therefore asks *who
    /// the credential is* first, and that call cannot answer emptily.
    #[tokio::test]
    async fn the_credential_is_checked_before_anything_that_could_answer_emptily() {
        let fake = Fake::new(a_corpus());
        let (_, _) = run(&fake, &cfg(json!({})), None).await.unwrap();
        assert_eq!(
            fake.calls().first().map(String::as_str),
            Some("current_user"),
            "the run's first call must be the one a bad credential cannot pass: {:?}",
            fake.calls()
        );
    }

    /// A full sync walks every page of the configured scope and emits each
    /// once -- which is what the descriptor's `full_sync_exhaustive: true`
    /// claims, and what licenses the engine's sweep.
    #[tokio::test]
    async fn a_full_sync_walks_every_page_following_the_next_link() {
        let fake = Fake::new(a_corpus());
        // Two per page, so the walk has to follow three continuation links.
        let (items, cursor) = run(&fake, &cfg(json!({ "page_size": 2 })), None)
            .await
            .unwrap();
        assert_eq!(ids(&items), vec!["100", "101", "102", "103", "104"]);
        let follows = fake
            .calls()
            .iter()
            .filter(|c| c.starts_with("follow "))
            .count();
        assert_eq!(follows, 2, "{:?}", fake.calls());
        // The watermark is the newest page's own timestamp, never a clock
        // reading -- `CONTEXT.md`'s Watermark rule.
        assert_eq!(watermark(&cursor), "2026-08-22T10:40:00Z");
    }

    /// Battery clause 2, at this seam: an incremental run over an unchanged
    /// corpus emits nothing and hands back the **byte-identical** cursor. The
    /// engine reads a changed cursor as "something happened" and writes an
    /// activity line for it, so a fresh cursor here is 288 lies a day.
    #[tokio::test]
    async fn an_idle_incremental_run_emits_nothing_and_returns_the_same_bytes() {
        let fake = Fake::new(a_corpus());
        let (_, cursor) = run(&fake, &cfg(json!({})), None).await.unwrap();
        let (items, again) = run(&fake, &cfg(json!({})), Some(cursor.clone()))
            .await
            .unwrap();
        assert!(items.is_empty(), "{:?}", ids(&items));
        assert_eq!(again, cursor);
    }

    /// One edited page, then an idle poll -- **the only shape that shows a
    /// `seen` set which forgot what the run skipped.** Full-sync-then-idle is
    /// stable under that bug, because a full sync skips nothing.
    #[tokio::test]
    async fn one_edit_then_an_idle_poll_settles() {
        let fake = Fake::new(a_corpus());
        let (_, first) = run(&fake, &cfg(json!({})), None).await.unwrap();

        fake.pages.lock().unwrap().push(page(
            "105",
            "Retry budget notes",
            "2026-08-22T10:41:30.000Z",
            1,
        ));
        let (items, second) = run(&fake, &cfg(json!({})), Some(first)).await.unwrap();
        assert_eq!(
            ids(&items),
            vec!["105"],
            "the overlap re-delivers 104, and `seen` recognises it"
        );
        assert_eq!(watermark(&second), "2026-08-22T10:41:30Z");

        let (idle, third) = run(&fake, &cfg(json!({})), Some(second.clone()))
            .await
            .unwrap();
        assert!(idle.is_empty(), "{:?}", ids(&idle));
        assert_eq!(third, second, "byte-identical");
    }

    /// **The ceiling.** The watermark may never pass the newest position the
    /// run witnessed at its *start* (`CONTEXT.md`, *Watermark*).
    ///
    /// A run is not instantaneous. While the walk is paging, somebody edits a
    /// page; the walk reaches it (an `asc` ordering moves it toward the end)
    /// and its `version.when` is later than anything the run saw at its start.
    /// Advancing the watermark to *that* would put the whole run's own
    /// duration below the next query's lower bound -- so every other edit made
    /// during the run, by anyone, is never offered again. Clamping to the
    /// ceiling means the next run re-walks from just before run start.
    ///
    /// Both halves are asserted: the watermark stops at the ceiling, **and**
    /// an edit made during the run is still delivered afterwards. Without the
    /// second half a clamp that merely returned a smaller number would pass.
    #[tokio::test]
    async fn the_watermark_never_passes_the_ceiling_the_run_witnessed() {
        let fake = Fake::new(a_corpus());
        // 100 is edited the moment the walk asks for its second page, so it
        // moves from the front of the order to well past the ceiling -- and
        // the walk, which is still paging, reaches it at the end.
        *fake.edit_mid_walk.lock().unwrap() =
            Some(("100".to_owned(), "2026-08-22T11:59:00.000Z".to_owned()));

        let (items, cursor) = run(&fake, &cfg(json!({ "page_size": 2 })), None)
            .await
            .unwrap();
        assert!(
            items.iter().any(|i| i.entity.key == "100"),
            "the walk does reach the page that moved: {:?}",
            ids(&items)
        );
        assert_eq!(
            watermark(&cursor),
            "2026-08-22T10:40:00Z",
            "the watermark stops at the newest page the run witnessed at its start, not at the \
             edit that happened underneath it"
        );

        // The half that makes the clamp matter: an edit made *during* that run
        // which the walk never saw. Its stamp is after the ceiling and before
        // the mid-run edit -- exactly the band an unclamped watermark would
        // have skipped over.
        for p in fake.pages.lock().unwrap().iter_mut() {
            if p["id"] == json!("101") {
                p["version"]["when"] = json!("2026-08-22T11:30:00.000Z");
                p["version"]["number"] = json!(2);
            }
        }
        let (after, _) = run(&fake, &cfg(json!({})), Some(cursor)).await.unwrap();
        assert_eq!(
            ids(&after),
            vec!["101"],
            "the next run is offered the edit that happened during the previous one; with the \
             watermark at 11:59 its lower bound would have been 11:57 and this would be lost"
        );
    }

    /// The limitation the ceiling does **not** close, written down where the
    /// next reader meets it: offset paging over a field that is being edited
    /// can step over a row.
    ///
    /// A page at the front of an `asc` order is edited mid-walk and moves to
    /// the end; every row behind it shifts down by one, and the walk's next
    /// offset lands one past where it should. The stepped-over page keeps its
    /// old timestamp, so no later incremental query reaches it either -- only
    /// the next full sync does. This is the same class the Jira adapter
    /// accepts with `startAt` and `ORDER BY updated ASC` (contract §4.2), and
    /// it is recorded rather than hidden: a reader who believes the walk is
    /// airtight would build a stronger guarantee on top of it.
    #[tokio::test]
    async fn a_page_edited_mid_walk_can_shift_a_row_past_the_offset() {
        let fake = Fake::new(a_corpus());
        *fake.edit_mid_walk.lock().unwrap() =
            Some(("100".to_owned(), "2026-08-22T11:59:00.000Z".to_owned()));
        let (items, _) = run(&fake, &cfg(json!({ "page_size": 2 })), None)
            .await
            .unwrap();
        assert!(
            !items.iter().any(|i| i.entity.key == "102"),
            "the row behind the moved one is stepped over -- the accepted limitation: {:?}",
            ids(&items)
        );
        // And the same walk with nothing moving under it misses nothing, so
        // the assertion above is about the edit and not about the paging.
        let quiet = Fake::new(a_corpus());
        let (all, _) = run(&quiet, &cfg(json!({ "page_size": 2 })), None)
            .await
            .unwrap();
        assert_eq!(ids(&all), vec!["100", "101", "102", "103", "104"]);
    }

    /// **The clamp never drags the position backwards.**
    ///
    /// The ceiling is the newest page in the corpus *at run start*, and that
    /// can be **older** than the watermark: the page which set the watermark
    /// was deleted, or moved out of the configured spaces. Clamping to it
    /// unconditionally would then move the position back, and a source whose
    /// newest page was deleted would re-deliver its recent history on every
    /// poll for ever -- battery clause 2 failing in steady state.
    ///
    /// Witnessed as a sequence, because a single run cannot show it: sync,
    /// delete the newest page, sync again. The second run's ceiling is the
    /// *second*-newest page, and the position stays where it was.
    #[tokio::test]
    async fn the_ceiling_clamp_never_moves_the_position_backwards() {
        let fake = Fake::new(a_corpus());
        let (_, first) = run(&fake, &cfg(json!({})), None).await.unwrap();
        assert_eq!(watermark(&first), "2026-08-22T10:40:00Z");

        // The newest page is gone, so the corpus's newest is now 08-21 -- and
        // a page edited into the overlap window arrives to make the run emit
        // something at all.
        fake.pages
            .lock()
            .unwrap()
            .retain(|p| p["id"] != json!("104"));
        fake.pages.lock().unwrap().push(page(
            "105",
            "Retry budget notes",
            "2026-08-22T10:39:30.000Z",
            1,
        ));

        let (items, second) = run(&fake, &cfg(json!({})), Some(first.clone()))
            .await
            .unwrap();
        assert_eq!(ids(&items), vec!["105"]);
        assert_eq!(
            watermark(&second),
            "2026-08-22T10:40:00Z",
            "the ceiling is now older than the watermark, and the position stays put rather \
             than being dragged back to it"
        );
    }

    /// The completion path: a server that did not expand the comments is not
    /// a page with no comments. Reading the second as the first would drop a
    /// page's whole discussion out of the search corpus.
    #[tokio::test]
    async fn comments_the_search_did_not_expand_are_fetched_and_land_in_the_payload() {
        let fake = Fake::new(vec![page_without_comments(
            "104",
            "SEPA payout retry design",
            "2026-08-22T10:40:00.000Z",
            3,
        )]);
        fake.detached_comments.lock().unwrap().insert(
            "104".to_owned(),
            vec![json!({
                "id": "98320",
                "type": "comment",
                "body": { "storage": { "value": "<p>@Mara can you add the SLA?</p>" } }
            })],
        );
        let (items, _) = run(&fake, &cfg(json!({})), None).await.unwrap();
        let it = &items[0];
        assert_eq!(
            it.payload["children"]["comment"]["results"][0]["id"],
            "98320"
        );
        assert!(
            it.body_text.contains("@Mara can you add the SLA?"),
            "the discussion reaches what FTS indexes: {:?}",
            it.body_text
        );
        assert!(
            fake.calls().iter().any(|c| c == "comments 104"),
            "{:?}",
            fake.calls()
        );
    }

    /// The other direction: a page whose comments the search *did* expand
    /// costs no second request. Without this the completion path would be an
    /// N+1 on every page of every run, and nothing would notice.
    #[tokio::test]
    async fn an_expanded_comment_container_is_not_fetched_again() {
        let fake = Fake::new(a_corpus());
        run(&fake, &cfg(json!({})), None).await.unwrap();
        assert!(
            !fake.calls().iter().any(|c| c.starts_with("comments ")),
            "{:?}",
            fake.calls()
        );
    }

    /// A configured space list reaches the query. Without this the option
    /// would parse, validate, appear in the form -- and mirror the whole wiki.
    #[tokio::test]
    async fn the_configured_space_list_reaches_every_query_the_run_sends() {
        let fake = Fake::new(a_corpus());
        run(&fake, &cfg(json!({ "spaces": ["ENG"] })), None)
            .await
            .unwrap();
        let searches: Vec<String> = fake
            .calls()
            .into_iter()
            .filter(|c| c.starts_with("search "))
            .collect();
        assert_eq!(
            searches.len(),
            3,
            "the probe, the page walk and the mention walk: {searches:?}"
        );
        for search in &searches {
            assert!(
                search.contains("space in (\"ENG\")"),
                "the ceiling probe is scoped like the walk, or the clamp never binds: {search}"
            );
        }
    }

    /// Sink failures are the sink's to raise and the adapter's to propagate:
    /// a sink that has lost its database has no use for the remaining pages,
    /// and a fresh cursor after a partial write would skip what it dropped.
    #[tokio::test]
    async fn a_sink_failure_abandons_the_run() {
        struct Failing;
        #[async_trait::async_trait]
        impl Sink for Failing {
            async fn item(&mut self, _item: SyncItem) -> Result<(), SourceError> {
                Err(SourceError::Sink("sink is down".into()))
            }
        }
        let fake = Fake::new(a_corpus());
        let got = SyncRun {
            api: &fake,
            cfg: &cfg(json!({})),
            source_id: "confluence",
            base_url: "http://127.0.0.1:8090",
        }
        .run(None, &mut Failing)
        .await;
        assert!(matches!(got, Err(SourceError::Sink(_))), "{got:?}");
    }

    /// An empty corpus is a legitimate answer, not a failure -- a wiki with no
    /// page in the configured space yet. It emits nothing and hands back a
    /// cursor the next run can read.
    #[tokio::test]
    async fn an_empty_corpus_syncs_to_nothing_and_still_answers_a_cursor() {
        let fake = Fake::new(Vec::new());
        let (items, cursor) = run(&fake, &cfg(json!({})), None).await.unwrap();
        assert!(items.is_empty());
        let parsed = ConfluenceCursor::parse(&cursor).unwrap_or_else(|| {
            panic!("the cursor must be one this adapter can read back: {cursor}")
        });
        // **The zone, when the probe could not answer one.** A corpus with no
        // page in it renders no timestamp, so the offset every later CQL
        // literal is read in has to be guessed -- and the guess is UTC-12, not
        // UTC. Guessing the offset *high* moves the query's lower bound
        // forward and skips edits permanently; guessing it low only re-reads
        // work upserts absorb. `time.rs` pins the rule; this pins that the
        // sync run is the caller that applies it, which is where a plausible
        // `unwrap_or(0)` would live.
        assert_eq!(parsed.tz_offset_secs, crate::time::MIN_UTC_OFFSET_SECS);
    }

    /// **The hazard the call ordering exists for, witnessed rather than
    /// asserted about.**
    ///
    /// A server that reads a content search anonymously answers it `200` with
    /// an empty page when the credential cannot be resolved. The `page` kind
    /// claims `full_sync_exhaustive`, so a run that reported `Ok` there would
    /// be the engine's licence to tombstone the entire mirror. What stops it
    /// is that `current_user` runs first **and its answer is obeyed**.
    ///
    /// Neither of this file's other two tests can see that second half:
    ///
    /// * `the_credential_is_checked_before_anything_that_could_answer_emptily`
    ///   runs against a fake that answers everything, so it pins *which call
    ///   is first* and not *what happens when that call says no*;
    /// * `a_refused_credential_ends_the_run_with_nothing_in_the_sink` sets
    ///   `fault`, which refuses the search as well -- so the run would still
    ///   end in `Err` with the identity check deleted outright.
    ///
    /// Nor can the live suite: this container 401s the search too (the header
    /// of `tests/live_confluence_seeded.rs` records that), so there also the
    /// search is what refuses the run. This test is the only place the
    /// tolerated-failure direction is visible, which is why the fake needs a
    /// fault that stops at the identity call.
    #[tokio::test]
    async fn a_search_that_answers_emptily_to_a_refused_credential_still_ends_in_an_error() {
        let mut fake = Fake::new(Vec::new());
        fake.identity_fault = Some(SourceError::Unauthorized { status: Some(401) });
        let mut sink = VecSink(Vec::new());
        let got = SyncRun {
            api: &fake,
            cfg: &cfg(json!({})),
            source_id: "confluence",
            base_url: "http://127.0.0.1:8090",
        }
        .run(None, &mut sink)
        .await;
        assert!(
            matches!(got, Err(SourceError::Unauthorized { .. })),
            "an unresolvable credential over a search that answers emptily must end the run, \
             not hand the sweep an empty exhaustive corpus: {got:?}"
        );
        assert!(sink.0.is_empty(), "{:?}", sink.0.len());
    }

    /// A credential the server refuses is refused **before** anything is
    /// emitted, and the fault keeps its class all the way out.
    #[tokio::test]
    async fn a_refused_credential_ends_the_run_with_nothing_in_the_sink() {
        let mut fake = Fake::new(a_corpus());
        fake.fault = Some(SourceError::Unauthorized { status: Some(401) });
        let mut sink = VecSink(Vec::new());
        let got = SyncRun {
            api: &fake,
            cfg: &cfg(json!({})),
            source_id: "confluence",
            base_url: "http://127.0.0.1:8090",
        }
        .run(None, &mut sink)
        .await;
        assert!(
            matches!(got, Err(SourceError::Unauthorized { .. })),
            "{got:?}"
        );
        assert!(sink.0.is_empty());
    }
}
