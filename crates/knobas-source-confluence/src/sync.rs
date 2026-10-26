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
//! 4. **The positions sweep** ([`SyncRun::positions_sweep`], incremental runs
//!    only): every page's place among its siblings (#1138). A reorder makes
//!    no new version, so step 5 never sees it. Sent before the walk, so the
//!    places the cursor records are never newer than what the mirror holds.
//! 5. Query `lastmodified >= watermark − 2 minutes order by created asc, id
//!    asc`, each result asked from the last page read, until one offers no
//!    `_links.next` ([`crate::walk`], #1976).
//! 6. Drop what the overlap re-delivered, complete the comments the server did
//!    not expand, look up the users their mentions name ([`crate::users`]),
//!    push the rest into the sink.
//! 7. **The comment walk** ([`SyncRun::comment_walk`]): one more query,
//!    `type = comment` over the same window, whose results are resolved to
//!    the pages they are on and pushed too (#2073). Then the
//!    pages whose swept place differs from the cursor's record, less those
//!    already pushed, are read again whole and pushed.
//! 8. If nothing was pushed, hand back the cursor byte-identically. Otherwise
//!    advance the watermark to `max(previous, min(newest emitted, ceiling))`.
//!
//! # Why the comment walk is a second query and not a wider first one
//!
//! A comment is separate content in Confluence: posting or editing one does
//! not move the page's `lastmodified` (measured on Data Center 9.2.21, #2073).
//! So the page walk -- bounded by a page's own `lastmodified` -- can never
//! reach a comment posted on a page nobody has edited since, which is most
//! pages. The second query is bounded by the *comment's* timestamp instead,
//! and that is the only bound that reaches it
//! (`crate::cql::build_comment_cql`). It asks for every comment in scope,
//! not only the ones naming the account: until #2073 it was a mention walk,
//! and any other comment waited for the next page edit or full sync.
//!
//! It emits the **page**, never the comment: this adapter declares one entity
//! kind, a comment belongs to its page's payload (`crate::KIND_PAGE`), and the
//! thing a person opens from the inbox is the page the discussion is on.
//!
//! **It does not move the watermark.** The two walks share one cursor, and the
//! records the comment walk sees are newer than the page edits the page walk
//! is bounded by -- advancing on a comment posted today would put every page
//! edited yesterday below the next run's lower bound and lose it for good.
//! What the comment walk does contribute is its `seen` entries, which is what
//! makes the *next* run skip the same comment and leaves an idle poll idle
//! (battery clause 2). They are capped apart from the page walk's
//! ([`crate::cursor::COMMENT_SEEN_CAP`]): a comment stays in the window until
//! a page edit moves the watermark past it, so a busy discussion would
//! otherwise push the page walk's entries out of a shared cap.
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
//! # Why the walk is ordered by `created`
//!
//! The walk pages by offset, and offset paging over a field being edited can
//! *step over* a row: under `order by lastmodified asc`, a page at the front
//! of the order is edited, moves to the end, every row behind it shifts down
//! by one, and the next offset lands one past where it should. The ceiling
//! does not close that: the stepped-over page keeps its old timestamp, so no
//! incremental query reaches it, and a full sync's sweep tombstoned it
//! (#1976). `created`, then `id`, is an order no edit moves, the one the Jira
//! walk moved to in #1623. A deletion still shifts rows;
//! [`crate::walk`] asks each result from the last page read so it cannot
//! hide one. The time bound stays on `lastmodified`, which is what an edit
//! moves.
//!
//! # The two things the cursor layer is owed
//!
//! [`ConfluenceCursor::since`] documents them and this is where they are
//! honoured; they fail in different places, so neither substitutes for the
//! other.
//!
//! * **The previous watermark in [`Bounds`]**, so a re-delivered *older* page
//!   cannot drag the position backwards.
//! * **`seen_in_window` is every record the run saw**, skipped ones included.
//!   A run that reports only what it emitted forgets, every run, whatever it
//!   recognised that run, and an idle poll never settles. The watermark holds
//!   perfectly while that happens, which is why the sequence that shows it is
//!   *one edited page and then an idle poll*, never a full sync and then one.

use chrono::{DateTime, Utc};
use knobas_http::{NextLink, PageWalk, Walked};
use knobas_source::watermark::Bounds;
use knobas_source::{Sink, SourceError};

use crate::ConfluenceConfig;
use crate::api::{COMMENT_EXPAND, ConfluenceApi, EXPAND, POSITIONS_EXPAND};
use crate::cql::{Order, build_comment_cql, build_cql, build_ids_cql, build_positions_cql};
use crate::cursor::{ConfluenceCursor, Positions, Seen};
use crate::map;
use crate::model::RawContent;
use crate::storage::Account;
use crate::users::Directory;
use crate::walk::ContentWalk;

/// A stop so a server that keeps handing out `_links.next` cannot spin a run
/// forever. It counts requests, the page walk's asks from further back
/// included (`crate::walk`); at the default page size it is about a quarter
/// of a million pages.
///
/// Reaching it is a **failure**, never a quiet end to the walk. The one kind
/// this adapter emits claims `full_sync_exhaustive: true`, which is the
/// engine's licence to tombstone every row a `cursor: None` run did not
/// return -- so a truncated walk reported as `Ok` authorises deleting whatever
/// fell off the end. The walk is `order by created asc`, so what falls off
/// the end is the *most recently created* work: the pages someone is reading
/// today.
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

/// How many rows one request of the positions sweep asks for: Data Center
/// 9.2's own cap on a content search that expands no body (it answers
/// `"limit": 200` to a request for 1000, measured for #1138).
const SWEEP_PAGE_SIZE: u32 = 200;

/// One search and every page `_links.next` leads on to, with the
/// [`MAX_PAGES`] refusal in the one place it lives.
///
/// Every walk in this module but the page walk (the comment walk, the
/// positions sweep, the re-read of moved pages) is this one: the transport's
/// [`PageWalk`] over the [`NextLink`] protocol, which follows the link,
/// refuses one that is not rooted at the instance, and answers the cap. The
/// caller owns the rows; this names what they are when the cap refuses.
struct Walk<'a> {
    walk: PageWalk<NextLink, Fetch<'a>>,
    limit: u32,
    /// What the rows are, for the refusal message ("positions", "comment
    /// results").
    noun: &'a str,
}

/// The search for the first page, then whichever link the page before offered.
type Fetch<'a> = Box<
    dyn FnMut(
            Option<String>,
        ) -> std::pin::Pin<
            Box<
                dyn std::future::Future<Output = Result<crate::model::ContentPage, SourceError>>
                    + Send
                    + 'a,
            >,
        > + Send
        + 'a,
>;

impl<'a> Walk<'a> {
    fn new(
        api: &'a (dyn ConfluenceApi + 'a),
        cql: &'a str,
        limit: u32,
        expand: &'a str,
        noun: &'a str,
    ) -> Self {
        let fetch: Fetch<'a> = Box::new(move |link| {
            Box::pin(async move {
                match link {
                    None => api.search(cql, 0, limit, expand).await,
                    Some(link) => api.follow(&link).await,
                }
            })
        });
        Self {
            walk: PageWalk::new(
                NextLink {
                    product: "Confluence",
                },
                MAX_PAGES,
                fetch,
            ),
            limit,
            noun,
        }
    }

    /// The next page, or `None` once the last one has been handed out.
    ///
    /// The refusal is raised when the walk is about to *follow* a link past
    /// the stop, never as a quiet `None`: a truncated walk reported as
    /// finished is what [`MAX_PAGES`] exists to prevent. Because it is raised
    /// on the call after a page, the caller has handled that page's rows
    /// first.
    async fn next_page(&mut self) -> Result<Option<crate::model::ContentPage>, SourceError> {
        match self.walk.next().await? {
            Walked::Page(page) => Ok(Some(page)),
            Walked::End => Ok(None),
            Walked::Capped => Err(capped(self.noun, self.limit)),
        }
    }
}

/// The [`MAX_PAGES`] refusal, for the walk over `noun` in requests of `limit`.
fn capped(noun: &str, limit: u32) -> SourceError {
    SourceError::protocol(format!(
        "Confluence kept offering another page of {noun} after {MAX_PAGES} requests of \
         {limit}; refusing to report a partial walk as a completed sync. Narrow this source \
         with a space list."
    ))
}

/// The unit of the walk reading: the UI prints "120 of 300 pages".
const PAGES: &str = "pages";

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

/// The band of the corpus a walk reads, and the identity it reads it as.
///
/// One type because the four travel together and always will: the comment
/// walk reads the *same* cursor the page walk is bounded by, in the same zone,
/// as the same account -- and a run that passed three of the four would be a
/// second answer to "where is this source up to".
struct Window<'a> {
    /// The lower bound of the `lastmodified >=` clause, or `None` for a full
    /// sync.
    since: Option<DateTime<Utc>>,
    /// The UTC offset every CQL literal in this run is rendered in.
    offset_secs: i32,
    /// The position the run started from, which is what recognises a record
    /// the overlap re-delivered.
    previous: Option<&'a ConfluenceCursor>,
    /// Who the credential is, for resolving a mention's user key
    /// ([`crate::storage::Account`]).
    me: Option<Account<'a>>,
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
        let me = account(&whoami);
        // Who each mention key names, asked once per key for the whole run
        // (`crate::users`, #953).
        let mut users = Directory::new(&whoami);

        let witnessed = self.run_start().await?;
        let offset = witnessed.offset_secs;
        // A cursor taken under another space list is no cursor (#2181): the
        // run syncs this list in full, as it does an unreadable one.
        let scope = crate::cql::scope_key(self.cfg);
        let previous = cursor
            .as_deref()
            .and_then(ConfluenceCursor::parse)
            .filter(|c| c.scope.as_deref() == Some(scope.as_str()));
        let cursor = cursor.filter(|_| previous.is_some());
        let since = previous.as_ref().and_then(|c| c.since(offset));
        let cql = build_cql(self.cfg, since, offset, Order::Ascending);
        // The positions sweep goes **before** the walk, so the places the
        // cursor records are never newer than the ones the mirror received.
        // Swept after, a drag made once the walk had delivered a page would be
        // recorded as delivered, and that page is not re-read: the mirror
        // would keep the old place for good. Swept before, the same drag only
        // shows up as a difference on the next run, which re-reads the page.
        let swept = match previous {
            Some(_) => Some(self.positions_sweep(sink).await?),
            None => None,
        };

        let mut emitted: usize = 0;
        // The entity ids this run has already handed to the sink, so the
        // comment walk does not re-deliver a page the page walk just sent --
        // one wasted request and one wasted upsert per commented page on
        // every full sync, otherwise.
        let mut delivered: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        // Every record this run *saw* in the window -- see the module docs.
        // Skipped ones belong here as much as emitted ones.
        let mut seen_in_window: Vec<Seen> = Vec::new();
        // The comment walk's share of it, kept apart because it is capped
        // apart (`ConfluenceCursor::advanced`).
        let mut comments_seen: Vec<Seen> = Vec::new();
        // The comments each page the page walk sent carried, so the comment
        // walk can tell a comment that went out with its page from one
        // posted after the walk read that page.
        let mut sent_threads: Threads = Threads::new();
        // Seeded from the previous position, so `None` here means neither
        // the run nor the previous cursor carries a stamp. That the new
        // position never falls below the previous one is `Bounds::previous`'s
        // job in `advanced`, which holds it after the ceiling clamp too.
        let mut watermark: Option<DateTime<Utc>> = previous.as_ref().and_then(|c| c.modified_to);
        // Every walked page's place among its siblings. A full sync walks
        // every page, so this is the whole map and it needs no sweep (#1138).
        let mut walked_positions = Positions::new();

        // Paged so a page leaving the listing cannot hide another
        // (`crate::walk`, #1976).
        let mut walk = ContentWalk::new(self.api, &cql, EXPAND, self.cfg.page_size, MAX_PAGES);
        // The walk reading (#1381): pages got through, over the result's
        // `totalSize` where the API reports one and over nothing otherwise.
        // The latest page's figure wins, so a total that moved under the run
        // is the one drawn; it never reads below `walked`.
        //
        // On an incremental run the sweep's rows come first (#1389): the walk
        // counts on from them, over both totals, so the bar the sweep filled
        // is the one the walk finishes rather than a second one from zero.
        let swept_rows = swept.as_ref().map_or(0, |s| s.len() as u64);
        let reading = |walked: u64, of: Option<u64>| {
            (swept_rows + walked, of.map(|n| swept_rows + n.max(walked)))
        };
        let mut walked: u64 = 0;
        let mut of: Option<u64> = None;
        loop {
            let page = match walk.next().await? {
                Walked::Page(page) => page,
                Walked::End => break,
                Walked::Capped => return Err(capped("results", self.cfg.page_size)),
            };
            of = page.total_size.or(of);
            for mut raw in page.results {
                let (done, shown_of) = reading(walked, of);
                sink.walked(done, shown_of, PAGES).await;
                walked += 1;
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
                if let Some(position) = map::position_of(&raw.raw) {
                    walked_positions.insert(raw.content.id.clone(), position);
                }
                // The overlap exists so nothing is missed; this is what keeps
                // it from also meaning "everything arrives twice".
                if previous
                    .as_ref()
                    .is_some_and(|c| c.already_delivered(&raw.content.id, number))
                {
                    continue;
                }
                let item = self.mapped(&mut raw, me, &mut users).await?;
                // Sink failures are never swallowed (SPI doc on `Source::sync`).
                delivered.insert(raw.content.id.clone());
                sent_threads.insert(raw.content.id.clone(), thread(&raw));
                sink.item(item).await?;
                emitted += 1;
                if let Some(u) = when {
                    watermark = Some(watermark.map_or(u, |w| w.max(u)));
                }
            }
        }

        // Once after the last page, so a run that pushes nothing still ends
        // its bar at the end.
        let (done, shown_of) = reading(walked, of);
        sink.walked(done, shown_of, PAGES).await;

        // A full sync has just delivered every page with its whole
        // discussion, so the comment walk owes it only the `seen` entries the
        // next run's window will hold: it starts where that window will.
        // Unbounded, it would read every comment the wiki ever had.
        let comments_since = since.or_else(|| {
            watermark.and_then(|w| {
                let held = witnessed.ceiling.map_or(w, |ceiling| w.min(ceiling));
                knobas_source::watermark::since(Some(held), offset, offset)
            })
        });
        emitted += self
            .comment_walk(
                &Window {
                    since: comments_since,
                    offset_secs: offset,
                    previous: previous.as_ref(),
                    me,
                },
                &mut delivered,
                &sent_threads,
                &mut comments_seen,
                &mut users,
                sink,
            )
            .await?;

        // The places among siblings (#1138). A full sync read them off the
        // walk; an incremental run sweeps them, because a reorder makes no
        // new version and no `lastmodified` bound reaches it.
        let positions = match swept {
            None => walked_positions,
            Some(swept) => {
                let known = previous.as_ref().and_then(|c| c.positions.as_ref());
                let moved = ids_to_reread(known, &swept, &delivered);
                emitted += self
                    .reread(&moved, me, &mut delivered, &mut users, sink)
                    .await?;
                swept
                    .into_iter()
                    .filter_map(|(id, position)| position.map(|p| (id, p)))
                    .collect()
            }
        };

        match (emitted, watermark) {
            // Battery clause 2: nothing happened, so the position did not
            // move. Returning the *input string* rather than a re-encoded
            // struct makes that byte-identical by construction. A placed page
            // deleted since stays in the input's `positions` until the next
            // run that delivers something; it costs a few bytes, never a
            // request.
            (0, _) => Ok(cursor.unwrap_or_else(|| {
                ConfluenceCursor::empty(offset)
                    .with_positions(positions)
                    .with_scope(scope)
                    .encode()
            })),
            // Items arrived, but neither they nor the previous cursor carry a
            // readable `version.when`; advancing to anything would be a guess.
            // The places are recorded all the same, or the pages just re-read
            // would be re-read on every run.
            (_, None) => Ok(previous
                .unwrap_or_else(|| ConfluenceCursor::empty(offset))
                .with_positions(positions)
                .with_scope(scope)
                .encode()),
            // The ceiling clamp and the never-backwards floor are the shared
            // watermark's (#1622): an edit made *during* the walk does not
            // carry the position past the run's start, and a ceiling older
            // than the previous watermark (its page deleted, or moved out of
            // the configured spaces) does not drag it back.
            (_, Some(w)) => {
                let bounds = Bounds {
                    previous: previous.as_ref().and_then(|c| c.modified_to),
                    ceiling: witnessed.ceiling,
                };
                Ok(
                    ConfluenceCursor::advanced(bounds, w, offset, &seen_in_window, &comments_seen)
                        .with_positions(positions)
                        .with_scope(scope)
                        .encode(),
                )
            }
        }
    }

    /// The positions sweep (#1138): every page in scope, with its place among
    /// its siblings (`None` for title order) and nothing else.
    ///
    /// **Why every run reads it.** Moving a page among its siblings changes
    /// `extensions.position` and nothing else on Data Center 9.2: not the
    /// page's version, not its `lastmodified`, not its parent's. No bounded
    /// query reaches it and the instance offers no change feed an ordinary
    /// account may read (`rest/api/audit` is an administrator's). So the
    /// bound on a reorder reaching the Docs tree is one sync interval, and the
    /// price is `ceil(pages / 200)` body-less requests per incremental run.
    ///
    /// **It is the first part of the run's walk reading** (#1389): a reading
    /// of `pages` before each row, over the result's `totalSize` like the
    /// page walk's, so a large space shows the bar moving through the sweep
    /// and not only through the window after it. The page walk counts on
    /// from the rows swept here.
    async fn positions_sweep(
        &self,
        sink: &mut (dyn Sink + Send),
    ) -> Result<Vec<(String, Option<i64>)>, SourceError> {
        let cql = build_positions_cql(self.cfg);
        let mut swept = Vec::new();
        let mut walk = Walk::new(
            self.api,
            &cql,
            SWEEP_PAGE_SIZE,
            POSITIONS_EXPAND,
            "positions",
        );
        let mut of: Option<u64> = None;
        while let Some(page) = walk.next_page().await? {
            of = page.total_size.or(of);
            for raw in &page.results {
                let done = swept.len() as u64;
                sink.walked(done, of.map(|n| n.max(done)), PAGES).await;
                swept.push((raw.content.id.clone(), map::position_of(&raw.raw)));
            }
        }
        Ok(swept)
    }

    /// Read the pages the sweep found moved again, whole, and push them.
    /// Answers how many it pushed.
    ///
    /// Through a search by id rather than one request per page: a reorder
    /// renumbers every sibling, so the pages come in groups. They do not move
    /// the watermark -- a reorder is not an edit, and their `version.when` is
    /// as old as it was -- and they are not `seen`, for the same reason.
    async fn reread(
        &self,
        moved: &[&str],
        me: Option<Account<'_>>,
        delivered: &mut std::collections::BTreeSet<String>,
        users: &mut Directory,
        sink: &mut (dyn Sink + Send),
    ) -> Result<usize, SourceError> {
        let mut emitted = 0;
        let size = usize::try_from(self.cfg.page_size).unwrap_or(1).max(1);
        for group in moved.chunks(size) {
            let cql = build_ids_cql(group);
            let mut walk = Walk::new(self.api, &cql, self.cfg.page_size, EXPAND, "re-read pages");
            while let Some(page) = walk.next_page().await? {
                for mut raw in page.results {
                    if !delivered.insert(raw.content.id.clone()) {
                        continue;
                    }
                    let item = self.mapped(&mut raw, me, users).await?;
                    sink.item(item).await?;
                    emitted += 1;
                }
            }
        }
        Ok(emitted)
    }

    /// The comment walk: every comment created or edited since the cursor,
    /// emitted as the **page** it is on (#2073).
    ///
    /// Answers how many items it pushed. The module docs carry why this is a
    /// second query and why it must not move the watermark; the three
    /// decisions that live here:
    ///
    /// * **`seen` is fed from the comment CQL returned**, and the skip is
    ///   checked against the same identity. That is what makes an idle poll
    ///   idle: the page a comment resolves to keeps its own old
    ///   `version.when` and would fall out of the `seen` window immediately,
    ///   so deduplicating on the *page* would re-emit it on every run for
    ///   ever. Deduplicating on the comment cannot: its timestamp is newer
    ///   than the watermark, so it stays in `seen` exactly as long as the
    ///   query keeps returning it. Those entries go to `comments_seen`, which
    ///   the cursor caps apart from the page walk's, so a run that brought in
    ///   hundreds of comments cannot evict page-walk entries and re-deliver a
    ///   page ([`crate::cursor::COMMENT_SEEN_CAP`]).
    /// * **The page is fetched whole** rather than mapped from the thin
    ///   `COMMENT_EXPAND` record, so a page a comment led to and a page the
    ///   ordinary walk delivered are mapped from the same shape -- and the new
    ///   comment is in the payload where every other comment is.
    /// * **A page the page walk already sent is not sent again**, and a
    ///   comment its copy did not carry -- posted after the walk read the
    ///   page -- is left out of `seen`, so the next run sends the page with
    ///   it. Recorded, the entry would claim a delivery that never happened.
    /// * **A failure here fails the run**, and is not swallowed into a partial
    ///   success: a source that answered pages and not comments would
    ///   otherwise go quietly deaf to every discussion, and a stuck source is
    ///   the direction a person can see and act on.
    async fn comment_walk(
        &self,
        window: &Window<'_>,
        delivered: &mut std::collections::BTreeSet<String>,
        sent_threads: &Threads,
        comments_seen: &mut Vec<Seen>,
        users: &mut Directory,
        sink: &mut (dyn Sink + Send),
    ) -> Result<usize, SourceError> {
        let cql = build_comment_cql(self.cfg, window.since, window.offset_secs);
        let mut emitted: usize = 0;
        let mut walk = Walk::new(
            self.api,
            &cql,
            self.cfg.page_size,
            COMMENT_EXPAND,
            "comment results",
        );
        while let Some(page) = walk.next_page().await? {
            for raw in &page.results {
                let version = raw.content.version.as_ref();
                let number = version.and_then(|v| v.number);
                let page_id = raw.content.page_it_is_on();
                let missed = page_id
                    .as_ref()
                    .and_then(|p| sent_threads.get(p))
                    .is_some_and(|t| !t.contains(&(raw.content.id.clone(), number)));
                if !missed
                    && let Some(u) = version
                        .and_then(|v| v.when.as_deref())
                        .and_then(crate::time::parse_time)
                {
                    comments_seen.push(Seen {
                        i: raw.content.id.clone(),
                        n: number,
                        u,
                    });
                }
                if window
                    .previous
                    .is_some_and(|c| c.already_delivered(&raw.content.id, number))
                {
                    continue;
                }
                let Some(page_id) = page_id else {
                    continue;
                };
                if !delivered.insert(page_id.clone()) {
                    continue;
                }
                let mut target = self.api.content(&page_id, EXPAND).await?;
                let item = self.mapped(&mut target, window.me, users).await?;
                sink.item(item).await?;
                emitted += 1;
            }
        }
        Ok(emitted)
    }

    /// **A refresh** (ADR-0031, #2096): the page with content id `id`, read
    /// by id and mapped as a run maps it.
    ///
    /// The same three steps the comment walk takes for the page a comment led
    /// to: `GET /rest/api/content/{id}` with [`EXPAND`], the comments it did
    /// not expand completed, the mentions' users looked up. And the same
    /// identity first, for the reason the run asks it first: a mention is
    /// resolved against the credential's own key, and a read made as nobody
    /// must fail rather than answer a page mapped without one.
    ///
    /// By id and not by CQL, which is what lets it follow a write at once:
    /// the search index lags a write, the content endpoint does not.
    ///
    /// * `404` is [`Gone`](knobas_source::RefreshAnswer::Gone): deleted, or out
    ///   of the account's sight, which no run would deliver either. So is a
    ///   page whose status is not `current` (in the trash, a draft) and one in
    ///   a space this source is not configured for: a run's `type = page`
    ///   search answers neither, and a full sync would tombstone it.
    /// * A record that is not a page (a blog post, a comment) is
    ///   [`NotOffered`](knobas_source::RefreshAnswer::NotOffered): this
    ///   adapter emits one kind.
    /// * Every other failure is the error, never `Gone`.
    pub(crate) async fn refresh(
        &self,
        id: &str,
    ) -> Result<knobas_source::RefreshAnswer, SourceError> {
        use knobas_source::RefreshAnswer;
        let whoami = self.api.current_user().await?;
        let me = account(&whoami);
        let mut raw = match self.api.content(id, EXPAND).await {
            Ok(raw) => raw,
            Err(error) if error.status() == Some(404) => return Ok(RefreshAnswer::Gone),
            Err(error) => return Err(error),
        };
        if raw.content.content_type.as_deref() != Some("page") {
            return Ok(RefreshAnswer::NotOffered);
        }
        // A record that names no status is read as current, as the run reads
        // every search row it is handed; a space key is matched without
        // case, so a key typed `eng` that CQL scopes to `ENG` is not read as
        // gone -- the wrong way, a page wrongly gone is a page tombstoned.
        let current = raw.raw["status"].as_str().is_none_or(|s| s == "current");
        let in_scope = self.cfg.spaces.is_empty()
            || raw.raw["space"]["key"]
                .as_str()
                .is_some_and(|key| self.cfg.spaces.iter().any(|s| s.eq_ignore_ascii_case(key)));
        if !current || !in_scope {
            return Ok(RefreshAnswer::Gone);
        }
        let item = self
            .mapped(&mut raw, me, &mut Directory::new(&whoami))
            .await?;
        Ok(RefreshAnswer::Item(item))
    }

    /// One page record as the item every path emits it: its comments
    /// completed, its mentions' users looked up, mapped. The page walk, the
    /// re-read of moved pages, the comment walk and the refresh all pass
    /// here, so the four cannot map one page four ways.
    async fn mapped(
        &self,
        raw: &mut RawContent,
        me: Option<Account<'_>>,
        users: &mut Directory,
    ) -> Result<knobas_source::SyncItem, SourceError> {
        self.complete(raw).await?;
        users.annotate(self.api, raw).await;
        Ok(map::to_sync_item(self.source_id, self.base_url, raw, me))
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
        let page = self.api.search(&probe, 0, 1, "version").await?;
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

/// The credential's own account as a mention is resolved against it
/// ([`Account`]), or `None` for a server that would not say its username.
fn account(whoami: &crate::model::CurrentUser) -> Option<Account<'_>> {
    whoami.username.as_deref().map(|username| Account {
        username,
        user_key: whoami.user_key.as_deref(),
    })
}

/// Each page the page walk sent, by id, with the comments its copy carried
/// as `(comment id, version number)`.
type Threads =
    std::collections::BTreeMap<String, std::collections::BTreeSet<(String, Option<u64>)>>;

/// The comments a page's record carries, as [`Threads`] holds them.
fn thread(raw: &RawContent) -> std::collections::BTreeSet<(String, Option<u64>)> {
    raw.content
        .children
        .as_ref()
        .and_then(|c| c.comment.as_ref())
        .map(|c| {
            c.results
                .iter()
                .map(|r| {
                    (
                        r.content.id.clone(),
                        r.content.version.as_ref().and_then(|v| v.number),
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The swept pages whose place differs from the one the mirror last received,
/// less the ones this run already delivered fresh (#1138).
///
/// With no recorded places at all -- a cursor from before #1138 -- every
/// placed page counts as moved, since nothing says it did not.
fn ids_to_reread<'s>(
    known: Option<&Positions>,
    swept: &'s [(String, Option<i64>)],
    delivered: &std::collections::BTreeSet<String>,
) -> Vec<&'s str> {
    swept
        .iter()
        .filter(|(id, position)| match known {
            Some(known) => known.get(id).copied() != *position,
            None => position.is_some(),
        })
        .map(|(id, _)| id.as_str())
        .filter(|id| !delivered.contains(*id))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use knobas_source::SyncItem;
    use knobas_source::contract::{VecSink, Walking};
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
        /// What `type = comment` answers: comment records carrying the page
        /// they are on in `container`, bounded and ordered by their own
        /// `version.when` the way the page rows are. Empty in every test that
        /// is not about comments, which is how the comment walk stays
        /// invisible to them.
        comment_hits: Mutex<Vec<Value>>,
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
        edit_mid_walk: Mutex<Option<Vec<(&'static str, &'static str)>>>,
        /// Pages deleted the moment the walk asks for its second page.
        delete_mid_walk: Mutex<Option<Vec<&'static str>>>,
        /// Places among siblings that change the moment the comment walk is
        /// asked for: a drag made after the page walk has delivered the page
        /// (#1138).
        reorder_on_comments: Mutex<Option<Vec<(String, i64)>>>,
        /// A comment that lands on a page the moment the comment walk is
        /// asked for: posted after the page walk had read that page.
        comment_on_comments: Mutex<Option<(&'static str, Value)>>,
        /// The users `rest/api/user?key=` knows, by key (#953); any other key
        /// answers `404`.
        users: std::collections::BTreeMap<String, Value>,
        /// Set to fail every user lookup with this instead.
        user_fault: Option<SourceError>,
        /// Answer every search page with the result's `totalSize`, the way a
        /// server that reports one does (#1381).
        reports_total: bool,
    }

    impl Fake {
        fn new(pages: Vec<Value>) -> Self {
            Self {
                pages: Mutex::new(pages),
                comment_hits: Mutex::new(Vec::new()),
                detached_comments: Mutex::new(std::collections::BTreeMap::new()),
                calls: Mutex::new(Vec::new()),
                fault: None,
                identity_fault: None,
                edit_mid_walk: Mutex::new(None),
                delete_mid_walk: Mutex::new(None),
                reorder_on_comments: Mutex::new(None),
                comment_on_comments: Mutex::new(None),
                users: std::collections::BTreeMap::new(),
                user_fault: None,
                reports_total: false,
            }
        }

        /// The page envelope, carrying `totalSize` when this fake reports one.
        fn envelope_with_reported_total(
            &self,
            mut page: crate::model::ContentPage,
            cql: &str,
            expand: &str,
        ) -> crate::model::ContentPage {
            if self.reports_total {
                let all = self.matching(cql, expand, 0, usize::MAX).0.len();
                page.total_size = Some(all as u64);
            }
            page
        }

        /// What changes the moment the walk asks for a second page: the
        /// mid-run edit (the page jumps to the end of a `lastmodified` order)
        /// and the mid-run deletion.
        fn between_pages(&self) {
            let mut pages = self.pages.lock().unwrap();
            for (id, when) in self
                .edit_mid_walk
                .lock()
                .unwrap()
                .take()
                .unwrap_or_default()
            {
                for p in pages.iter_mut() {
                    if p["id"] == json!(id) {
                        p["version"]["when"] = json!(when);
                        p["version"]["number"] = json!(99);
                    }
                }
            }
            if let Some(ids) = self.delete_mid_walk.lock().unwrap().take() {
                pages.retain(|p| !ids.iter().any(|id| p["id"] == json!(id)));
            }
        }

        /// The records `type = comment` will answer with.
        fn with_comment_hits(mut self, rows: Vec<Value>) -> Self {
            self.comment_hits = Mutex::new(rows);
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
            let pages = if is_comment_query(cql) {
                self.comment_hits.lock().unwrap().clone()
            } else {
                self.pages.lock().unwrap().clone()
            };
            let bound = cql
                .split_once("lastmodified >= \"")
                .map(|(_, rest)| rest.split('"').next().unwrap_or_default().to_owned());
            // The re-read of reordered pages (#1138) names them by id.
            let named: Option<Vec<String>> = cql.split_once("id in (").map(|(_, rest)| {
                rest.split(')')
                    .next()
                    .unwrap_or_default()
                    .split(',')
                    .map(|id| id.trim().to_owned())
                    .collect()
            });
            // A space list, as the server reads it: a row outside it is not
            // answered. A row naming no space (a comment hit) is left in.
            let spaces: Option<Vec<String>> = cql.split_once("space in (").map(|(_, rest)| {
                rest.split(')')
                    .next()
                    .unwrap_or_default()
                    .split(", ")
                    .map(|key| key.trim_matches('"').to_owned())
                    .collect()
            });
            let mut rows: Vec<Value> = pages
                .into_iter()
                .filter(|p| match (&spaces, p["space"]["key"].as_str()) {
                    (Some(keys), Some(key)) => keys.iter().any(|k| k == key),
                    _ => true,
                })
                .filter(|p| {
                    named
                        .as_ref()
                        .is_none_or(|ids| ids.iter().any(|id| p["id"] == json!(id)))
                })
                .filter(|p| match &bound {
                    // The fake reads the literal as UTC, which is the zone
                    // every timestamp in these tests is written in.
                    Some(literal) => when_of(p) >= format!("{literal}:00Z").replace(' ', "T"),
                    None => true,
                })
                .collect();
            // By what the query orders by, as the server would.
            if cql.contains("order by created") {
                rows.sort_by_key(created_of);
            } else {
                rows.sort_by_key(when_of);
            }
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

    /// `order by created asc, id asc`: the creation stamp, then the id as
    /// the number it is. A page with no `history` sorts on its id alone.
    fn created_of(page: &Value) -> (String, u64) {
        (
            page["history"]["createdDate"]
                .as_str()
                .unwrap_or_default()
                .to_owned(),
            page["id"]
                .as_str()
                .and_then(|id| id.parse().ok())
                .unwrap_or_default(),
        )
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
            Ok(serde_json::from_value(json!({ "username": "knobas", "userKey": MY_KEY })).unwrap())
        }

        async fn search(
            &self,
            cql: &str,
            start: u32,
            limit: u32,
            expand: &str,
        ) -> Result<crate::model::ContentPage, SourceError> {
            self.calls.lock().unwrap().push(format!("search {cql}"));
            if start > 0 {
                self.between_pages();
            }
            if let Some(fault) = &self.fault {
                return Err(clone_fault(fault));
            }
            assert!(
                cql.contains("type = page") || is_comment_query(cql),
                "every query this adapter sends is the page walk's or the comment walk's: {cql}"
            );
            if is_comment_query(cql)
                && let Some((page_id, comment)) = self.comment_on_comments.lock().unwrap().take()
            {
                a_comment_lands_on(self, page_id, comment);
            }
            if is_comment_query(cql)
                && let Some(places) = self.reorder_on_comments.lock().unwrap().take()
            {
                let mut pages = self.pages.lock().unwrap();
                for (id, position) in places {
                    if let Some(p) = pages.iter_mut().find(|p| p["id"] == json!(id)) {
                        p["extensions"] = json!({ "position": position });
                    }
                }
            }
            let (rows, more) = self.matching(cql, expand, start as usize, limit as usize);
            let mut page = self.envelope_with_reported_total(
                envelope(
                    rows,
                    more.then(|| next_link(cql, expand, start + limit, limit)),
                ),
                cql,
                expand,
            );
            // Where this result starts and how long it was asked to be, as
            // the server echoes them.
            (page.start, page.limit) = (Some(start), Some(limit));
            Ok(page)
        }

        async fn follow(
            &self,
            path_and_query: &str,
        ) -> Result<crate::model::ContentPage, SourceError> {
            self.calls
                .lock()
                .unwrap()
                .push(format!("follow {path_and_query}"));
            self.between_pages();
            let (cql, expand, start, limit) = parse_link(path_and_query);
            let (rows, more) = self.matching(&cql, &expand, start, limit);
            let (start, limit) = (
                u32::try_from(start + limit).unwrap_or(u32::MAX),
                u32::try_from(limit).unwrap_or(u32::MAX),
            );
            Ok(self.envelope_with_reported_total(
                envelope(rows, more.then(|| next_link(&cql, &expand, start, limit))),
                &cql,
                &expand,
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
                .ok_or_else(|| SourceError::Protocol {
                    status: Some(404),
                    message: format!("No content found with id: {id}"),
                })?;
            if !expand.contains("children.comment")
                && let Some(object) = row.as_object_mut()
            {
                object.remove("children");
            }
            Ok(serde_json::from_value(row).expect("the fake holds well-formed records"))
        }

        async fn user_by_key(&self, key: &str) -> Result<Value, SourceError> {
            self.calls.lock().unwrap().push(format!("user {key}"));
            if let Some(fault) = &self.user_fault {
                return Err(clone_fault(fault));
            }
            self.users.get(key).cloned().ok_or(SourceError::Protocol {
                status: Some(404),
                message: format!("No user found with key: {key}"),
            })
        }
    }

    /// Whether a CQL is the comment walk's, read the way the fake's `search`
    /// has to read it: by the scope only that walk sends.
    fn is_comment_query(cql: &str) -> bool {
        cql.starts_with("type = comment")
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

    /// The `userKey` this fake's `/rest/api/user/current` reports -- the key
    /// a mention in a body is written with, and the one thing that lets
    /// `body_text` carry a name at all.
    const MY_KEY: &str = "ff8080818f2a1b4c018f2a1c9d0e0001";

    /// A comment in the storage format a Confluence **editor** writes a
    /// mention in: a user link carrying the key and no name anywhere.
    fn mentioning_comment(id: &str, when: &str) -> Value {
        json!({
            "id": id,
            "type": "comment",
            "body": { "storage": { "value": format!(
                "<p><ac:link><ri:user ri:userkey=\"{MY_KEY}\" /></ac:link> can you add the \
                 SLA?</p>"), "representation": "storage" } },
            "version": { "number": 1, "when": when, "by": { "username": "priya" } }
        })
    }

    /// A comment nobody is named in, by `by`, saying `text`.
    fn plain_comment(id: &str, when: &str, number: u64, by: &str, text: &str) -> Value {
        json!({
            "id": id,
            "type": "comment",
            "body": { "storage": { "value": format!("<p>{text}</p>"), "representation": "storage" } },
            "version": { "number": number, "when": when, "by": { "username": by } }
        })
    }

    /// The thin record `type = comment` answers for a comment:
    /// `COMMENT_EXPAND`'s fields and nothing else.
    fn comment_hit(comment: &Value, page_id: &str) -> Value {
        json!({
            "id": comment["id"],
            "type": "comment",
            "container": { "id": page_id, "type": "page" },
            "version": { "number": comment["version"]["number"], "when": comment["version"]["when"] }
        })
    }

    /// Hang `comment` off page `page_id` -- replacing the comment with its id
    /// if the page has one, which is an edit -- and set the comment query's
    /// answer for it. The page's own `version` is left alone: that is what a
    /// comment does to its page on Confluence (#2073).
    fn a_comment_lands_on(fake: &Fake, page_id: &str, comment: Value) {
        let mut pages = fake.pages.lock().unwrap();
        let page = pages
            .iter_mut()
            .find(|p| p["id"] == json!(page_id))
            .expect("the corpus has the page");
        let thread = page["children"]["comment"]["results"]
            .as_array_mut()
            .expect("an expanded thread");
        thread.retain(|c| c["id"] != comment["id"]);
        thread.push(comment.clone());
        let size = thread.len();
        page["children"]["comment"]["size"] = json!(size);
        drop(pages);
        let mut hits = fake.comment_hits.lock().unwrap();
        hits.retain(|h| h["id"] != comment["id"]);
        hits.push(comment_hit(&comment, page_id));
    }

    /// Hang a mentioning comment off the fake's oldest page and register the
    /// comment query's answer for it. Answers the page's id.
    ///
    /// **The oldest page on purpose**: the whole reason the comment walk
    /// exists is that a comment does not move its page's `lastmodified`, so
    /// the page this reaches must be one no incremental page walk would.
    fn a_mention_arrives(fake: &Fake, when: &str) -> String {
        let id = fake.pages.lock().unwrap()[0]["id"]
            .as_str()
            .expect("a page id")
            .to_owned();
        a_comment_lands_on(fake, &id, mentioning_comment("900", when));
        id
    }

    /// The thread of page `id` in an item's payload, as `(author, text)`.
    fn thread_of(items: &[SyncItem], id: &str) -> Vec<(String, String)> {
        let item = items
            .iter()
            .find(|i| i.entity.key == id)
            .unwrap_or_else(|| panic!("page {id} was not delivered: {:?}", ids(items)));
        item.payload["children"]["comment"]["results"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|c| {
                (
                    c["version"]["by"]["username"]
                        .as_str()
                        .unwrap_or_default()
                        .to_owned(),
                    c["body"]["storage"]["value"]
                        .as_str()
                        .unwrap_or_default()
                        .to_owned(),
                )
            })
            .collect()
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

    /// A server that never stops offering `_links.next` ends every walk in an
    /// error naming what it was walking, after exactly [`MAX_PAGES`] pages --
    /// never in a quiet `None` the engine would read as a finished corpus.
    #[tokio::test]
    async fn a_walk_that_never_ends_is_refused_after_max_pages() {
        struct Endless;
        #[async_trait::async_trait]
        impl ConfluenceApi for Endless {
            async fn current_user(&self) -> Result<crate::model::CurrentUser, SourceError> {
                unimplemented!()
            }
            async fn search(
                &self,
                _cql: &str,
                _start: u32,
                _limit: u32,
                _expand: &str,
            ) -> Result<crate::model::ContentPage, SourceError> {
                Ok(envelope(Vec::new(), Some("/next".to_owned())))
            }
            async fn follow(&self, _p: &str) -> Result<crate::model::ContentPage, SourceError> {
                Ok(envelope(Vec::new(), Some("/next".to_owned())))
            }
            async fn comments(
                &self,
                _id: &str,
                _start: u32,
                _limit: u32,
            ) -> Result<crate::model::ContentPage, SourceError> {
                unimplemented!()
            }
            async fn content(
                &self,
                _id: &str,
                _expand: &str,
            ) -> Result<crate::model::RawContent, SourceError> {
                unimplemented!()
            }
            async fn user_by_key(&self, _key: &str) -> Result<Value, SourceError> {
                unimplemented!()
            }
        }
        let mut walk = Walk::new(&Endless, "type = page", 50, EXPAND, "positions");
        let mut pages = 0;
        let refusal = loop {
            match walk.next_page().await {
                Ok(Some(_)) => pages += 1,
                Ok(None) => panic!("an endless walk ended quietly after {pages} pages"),
                Err(e) => break e.to_string(),
            }
        };
        assert_eq!(pages, MAX_PAGES);
        assert!(refusal.contains("positions"), "{refusal}");
    }

    /// A full sync walks every page of the configured scope and emits each
    /// once -- which is what the descriptor's `full_sync_exhaustive: true`
    /// claims, and what licenses the engine's sweep.
    #[tokio::test]
    async fn a_full_sync_walks_every_page_to_the_last_result() {
        let fake = Fake::new(a_corpus());
        // Two per page, each after the first asked from the last page read
        // and one longer (`crate::walk`): `@0` 100-101, `@1` 101-103, `@3`
        // 103-104, which offers no next link.
        let (items, cursor) = run(&fake, &cfg(json!({ "page_size": 2 })), None)
            .await
            .unwrap();
        assert_eq!(ids(&items), vec!["100", "101", "102", "103", "104"]);
        let walk = fake
            .calls()
            .iter()
            .filter(|c| c.ends_with("order by created asc, id asc"))
            .count();
        assert_eq!(walk, 3, "{:?}", fake.calls());
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

    /// A drag in Confluence's page tree, as the server records it: the
    /// siblings gain `extensions.position` and nothing else -- no new version,
    /// no new `lastmodified` (measured on Data Center 9.2, #1138).
    fn reorder(fake: &Fake, positions: &[(&str, i64)]) {
        let mut pages = fake.pages.lock().unwrap();
        for (id, position) in positions {
            let page = pages
                .iter_mut()
                .find(|p| p["id"] == json!(id))
                .expect("a page the corpus holds");
            page["extensions"] = json!({ "position": position });
        }
    }

    fn sorted(mut ids: Vec<String>) -> Vec<String> {
        ids.sort();
        ids
    }

    /// **A reorder alone reaches the next incremental run** (#1138). Moving a
    /// page among its siblings makes no new version, so the page walk's
    /// `lastmodified` bound never returns the siblings; the positions sweep
    /// does, and the siblings whose place changed are delivered again, whole,
    /// with their new `position`. An idle poll after that is idle.
    #[tokio::test]
    async fn a_reorder_alone_reaches_the_next_incremental_run() {
        let fake = Fake::new(a_corpus());
        let (_, first) = run(&fake, &cfg(json!({})), None).await.unwrap();

        reorder(&fake, &[("102", 0), ("101", 1)]);
        let (items, second) = run(&fake, &cfg(json!({})), Some(first.clone()))
            .await
            .unwrap();
        assert_eq!(sorted(ids(&items)), vec!["101", "102"]);
        for item in &items {
            let expected = if item.entity.key == "102" { 0 } else { 1 };
            assert_eq!(
                item.payload["position"],
                json!(expected),
                "{}",
                item.entity.key
            );
            assert!(
                item.payload["body"]["storage"]["value"].is_string(),
                "the page is delivered whole, not as a positions row"
            );
        }
        assert_eq!(
            watermark(&second),
            watermark(&first),
            "a reorder is not an edit, and does not move the watermark"
        );

        let (idle, third) = run(&fake, &cfg(json!({})), Some(second.clone()))
            .await
            .unwrap();
        assert!(idle.is_empty(), "{:?}", ids(&idle));
        assert_eq!(third, second, "byte-identical");

        // A second drag, among siblings that already had places.
        reorder(&fake, &[("101", 0), ("102", 1)]);
        let (items, _) = run(&fake, &cfg(json!({})), Some(second)).await.unwrap();
        assert_eq!(sorted(ids(&items)), vec!["101", "102"]);
    }

    /// A page the walk delivers and that is dragged before the run ends still
    /// reaches its new place on the next run. The cursor has to record the
    /// place the mirror received, never a newer one: a sweep sent after the
    /// walk would record the drag as already delivered, and the delivered
    /// page is not re-read, so the mirror would keep the old place for good.
    #[tokio::test]
    async fn a_drag_after_the_walk_delivered_the_page_reaches_the_next_run() {
        let fake = Fake::new(a_corpus());
        let (_, first) = run(&fake, &cfg(json!({})), None).await.unwrap();

        for p in fake.pages.lock().unwrap().iter_mut() {
            if p["id"] == json!("104") {
                p["version"]["when"] = json!("2026-08-22T10:50:00.000Z");
                p["version"]["number"] = json!(4);
            }
        }
        *fake.reorder_on_comments.lock().unwrap() = Some(vec![("104".to_owned(), 0)]);
        let (edited, second) = run(&fake, &cfg(json!({})), Some(first)).await.unwrap();
        assert_eq!(ids(&edited), vec!["104"]);
        assert_eq!(
            edited[0].payload["position"],
            json!(null),
            "delivered before the drag"
        );

        let (items, _) = run(&fake, &cfg(json!({})), Some(second)).await.unwrap();
        assert_eq!(
            ids(&items),
            vec!["104"],
            "the drag is re-read on the next run"
        );
        assert_eq!(items[0].payload["position"], json!(0));
    }

    /// A full sync reads every page's place off the walk itself, so it sends
    /// no positions sweep and re-reads nothing; the next idle poll's sweep
    /// finds the places it recorded.
    #[tokio::test]
    async fn a_full_sync_records_the_places_without_a_sweep() {
        let fake = Fake::new(a_corpus());
        reorder(&fake, &[("102", 0), ("101", 1)]);
        let (items, first) = run(&fake, &cfg(json!({})), None).await.unwrap();
        assert_eq!(items.len(), 5);
        assert!(
            !fake
                .calls()
                .iter()
                .any(|c| c.ends_with("order by created asc")),
            "{:?}",
            fake.calls()
        );
        let (idle, again) = run(&fake, &cfg(json!({})), Some(first.clone()))
            .await
            .unwrap();
        assert!(idle.is_empty(), "{:?}", ids(&idle));
        assert_eq!(again, first);
    }

    /// A cursor written before #1138 carries no places. Its first run cannot
    /// tell a reorder from none, so it re-reads every page that has a place,
    /// once, rather than leaving them as stale as they may already be.
    #[tokio::test]
    async fn a_cursor_without_places_re_reads_the_placed_pages_once() {
        let fake = Fake::new(a_corpus());
        reorder(&fake, &[("102", 0), ("101", 1)]);
        let (_, first) = run(&fake, &cfg(json!({})), None).await.unwrap();
        let mut old: Value = serde_json::from_str(&first).unwrap();
        old.as_object_mut().unwrap().remove("positions");
        let (items, second) = run(&fake, &cfg(json!({})), Some(old.to_string()))
            .await
            .unwrap();
        assert_eq!(sorted(ids(&items)), vec!["101", "102"]);
        let (idle, _) = run(&fake, &cfg(json!({})), Some(second)).await.unwrap();
        assert!(idle.is_empty(), "{:?}", ids(&idle));
    }

    /// **The ceiling.** The watermark may never pass the newest position the
    /// run witnessed at its *start* (`CONTEXT.md`, *Watermark*).
    ///
    /// A run is not instantaneous. While the walk is paging, somebody edits a
    /// page the walk has not reached yet; the walk reads it with a
    /// `version.when` later than anything the run saw at its start.
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
        // 104, the last in the walk's order, is edited the moment the walk
        // asks for its second page, to well past the ceiling -- and the walk,
        // which is still paging, reads it so.
        *fake.edit_mid_walk.lock().unwrap() = Some(vec![("104", "2026-08-22T11:59:00.000Z")]);

        let (items, cursor) = run(&fake, &cfg(json!({ "page_size": 2 })), None)
            .await
            .unwrap();
        assert!(
            items
                .iter()
                .any(|i| i.entity.key == "104" && i.payload["version"]["number"] == json!(99)),
            "the walk reads the edit: {:?}",
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

    /// **A page edited mid-walk shifts no row past the walk** (#1976). Over
    /// `order by lastmodified asc` with offset pages, page 100 -- on the page
    /// already read -- is edited, moves to the end, every row behind it shifts
    /// down by one, and the next offset lands one past page 102. `page` is
    /// `full_sync_exhaustive`, so the run's `Ok` let the sweep tombstone it.
    /// The walk orders by `created`, then `id`, which an edit cannot move.
    #[tokio::test]
    async fn a_page_edited_mid_walk_shifts_no_row_past_the_walk() {
        let fake = Fake::new(a_corpus());
        *fake.edit_mid_walk.lock().unwrap() = Some(vec![("100", "2026-08-22T11:59:00.000Z")]);
        let (items, _) = run(&fake, &cfg(json!({ "page_size": 2 })), None)
            .await
            .unwrap();
        assert_eq!(ids(&items), vec!["100", "101", "102", "103", "104"]);
    }

    /// **Why the order matters and not only the anchor.** Over three pages,
    /// both pages already read are edited after the first result. Over
    /// `lastmodified` they move to the end, behind 102, and the next request,
    /// asked from the last page read, opens on 100: a page the walk has read,
    /// which is where an anchored result says nothing unread is before it. It
    /// is wrong only because an edit moved the row, which `created` rules out.
    #[tokio::test]
    async fn pages_read_and_edited_mid_walk_hide_no_unread_one() {
        let fake = Fake::new(a_corpus().into_iter().take(3).collect());
        *fake.edit_mid_walk.lock().unwrap() = Some(vec![
            ("100", "2026-08-22T11:58:00.000Z"),
            ("101", "2026-08-22T11:59:00.000Z"),
        ]);
        let (items, _) = run(&fake, &cfg(json!({ "page_size": 2 })), None)
            .await
            .unwrap();
        assert_eq!(ids(&items), vec!["100", "101", "102"]);
    }

    /// **A page deleted mid-walk hides no other** (#1976). Page 100, already
    /// read, is deleted after the first page, so offset 2 of the four left
    /// starts at page 103 and an offset walk steps over 102. The walk asks
    /// from the last page read, finds the answer opens on 102, which it has
    /// not read, and asks again from further back.
    #[tokio::test]
    async fn a_page_deleted_mid_walk_hides_no_other() {
        let fake = Fake::new(a_corpus());
        *fake.delete_mid_walk.lock().unwrap() = Some(vec!["100"]);
        let (items, _) = run(&fake, &cfg(json!({ "page_size": 2 })), None)
            .await
            .unwrap();
        assert_eq!(ids(&items), vec!["100", "101", "102", "103", "104"]);
    }

    /// **Two pages deleted mid-walk** shift the rows by two, past the one
    /// row each request starts early by: offset 1 of the three left starts at
    /// 103. The result does not open on a page the walk has read, so it is
    /// asked again from the start, and 102 is on it.
    #[tokio::test]
    async fn two_pages_deleted_mid_walk_hide_no_other() {
        let fake = Fake::new(a_corpus());
        *fake.delete_mid_walk.lock().unwrap() = Some(vec!["100", "101"]);
        let (items, _) = run(&fake, &cfg(json!({ "page_size": 2 })), None)
            .await
            .unwrap();
        assert_eq!(ids(&items), vec!["100", "101", "102", "103", "104"]);
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

    /// **The criterion (#287), end to end at this seam.** A comment naming
    /// the account arrives on a page nobody has edited in months; the run
    /// after it delivers *that page*, and its `body_text` carries the `@name`
    /// `knobas_core::inbox`'s mention rule reads.
    ///
    /// Everything the ticket rests on is in the one assertion: the page walk
    /// cannot reach this page (its `version.when` is two months below the
    /// bound), the comment query can, the comment's user **key** is resolved
    /// to a name, and the entity is the page rather than the comment.
    #[tokio::test]
    async fn a_comment_that_mentions_me_delivers_the_page_it_is_on() {
        let fake = Fake::new(a_corpus());
        let (_, first) = run(&fake, &cfg(json!({})), None).await.unwrap();
        let page_id = a_mention_arrives(&fake, "2026-09-03T09:00:00.000Z");

        let (items, _) = run(&fake, &cfg(json!({})), Some(first)).await.unwrap();
        assert_eq!(
            ids(&items),
            vec![page_id.clone()],
            "the comment walk delivers the page the comment is on: {:?}",
            fake.calls()
        );
        assert!(
            items[0].body_text.contains("@knobas"),
            "the mention has to reach body_text as a name, or no inbox rule can \
             see it: {:?}",
            items[0].body_text
        );
        // The page was fetched whole by id -- the comment record itself
        // carries no body, and mapping from it would mirror an empty page.
        assert!(
            fake.calls().contains(&format!("content {page_id}")),
            "{:?}",
            fake.calls()
        );
        // **And the item is dated by the comment**, which is what puts it
        // inside `knobas_core::inbox::WINDOW_DAYS`. The page's own edit is two
        // months old; an item dated by that would be mirrored, searchable and
        // *absent from the inbox* -- the one failure that would make the whole
        // walk pointless.
        assert_eq!(
            items[0].updated_at.map(|t| t.to_rfc3339()).as_deref(),
            Some("2026-09-03T09:00:00+00:00"),
            "the mention's own date, not the page's last edit"
        );
    }

    /// Battery clause 2 **after a comment**, which is the case the ordinary
    /// idle test cannot reach: the page the comment resolved to keeps its own
    /// old `version.when`, so it falls out of the `seen` window at once. Only
    /// the *comment's* entry keeps the next run from re-delivering the page,
    /// and on every poll for ever.
    #[tokio::test]
    async fn a_delivered_comment_settles_and_the_next_poll_returns_the_same_bytes() {
        let fake = Fake::new(a_corpus());
        let (_, first) = run(&fake, &cfg(json!({})), None).await.unwrap();
        a_mention_arrives(&fake, "2026-09-03T09:00:00.000Z");

        let (items, second) = run(&fake, &cfg(json!({})), Some(first)).await.unwrap();
        assert_eq!(items.len(), 1, "{:?}", ids(&items));
        let (items, third) = run(&fake, &cfg(json!({})), Some(second.clone()))
            .await
            .unwrap();
        assert!(
            items.is_empty(),
            "the comment was delivered once: {:?}",
            ids(&items)
        );
        assert_eq!(third, second, "and the position did not move");
    }

    /// **The comment walk never moves the watermark.** The comment is dated
    /// twelve days after the newest page edit; a run that advanced to it would
    /// put every page edited in between below the next query's lower bound and
    /// lose those edits for good.
    #[tokio::test]
    async fn a_comment_newer_than_every_page_leaves_the_watermark_where_it_was() {
        let fake = Fake::new(a_corpus());
        let (_, first) = run(&fake, &cfg(json!({})), None).await.unwrap();
        assert_eq!(watermark(&first), "2026-08-22T10:40:00Z");
        a_mention_arrives(&fake, "2026-09-03T09:00:00.000Z");

        let (_, second) = run(&fake, &cfg(json!({})), Some(first)).await.unwrap();
        assert_eq!(
            watermark(&second),
            "2026-08-22T10:40:00Z",
            "the page walk's position is the watermark, and a comment is not a page edit"
        );
    }

    /// A page the page walk already delivered this run is not fetched and sent
    /// a second time by the comment walk -- which on a full sync would be one
    /// wasted request and one wasted upsert per commented page.
    #[tokio::test]
    async fn a_commented_page_the_walk_already_sent_is_not_sent_twice() {
        let fake = Fake::new(a_corpus());
        let page_id = a_mention_arrives(&fake, "2026-09-03T09:00:00.000Z");
        let (items, _) = run(&fake, &cfg(json!({})), None).await.unwrap();
        assert_eq!(
            ids(&items).iter().filter(|id| **id == page_id).count(),
            1,
            "{:?}",
            ids(&items)
        );
        assert!(
            !fake.calls().contains(&format!("content {page_id}")),
            "a full sync already has the page: {:?}",
            fake.calls()
        );
    }

    /// A comment posted **after the page walk read its page** in the same
    /// run: the page went out without it, so the comment's `seen` entry would
    /// claim a delivery that never happened and no later run would bring it.
    /// The page is not sent twice in that run; the next one sends it with the
    /// comment, and the one after is idle.
    #[tokio::test]
    async fn a_comment_posted_after_the_page_walk_sent_its_page_reaches_the_next_run() {
        let fake = Fake::new(a_corpus());
        let (_, first) = run(&fake, &cfg(json!({})), None).await.unwrap();
        for p in fake.pages.lock().unwrap().iter_mut() {
            if p["id"] == json!("101") {
                p["version"] = json!({ "number": 2, "when": "2026-09-02T00:00:00.000Z" });
            }
        }
        *fake.comment_on_comments.lock().unwrap() = Some((
            "101",
            plain_comment("960", "2026-09-03T09:00:00.000Z", 1, "mara", "late"),
        ));
        let (edited, second) = run(&fake, &cfg(json!({})), Some(first)).await.unwrap();
        assert_eq!(ids(&edited), vec!["101"], "the page walk's copy, once");
        assert!(
            thread_of(&edited, "101").is_empty(),
            "read before the comment"
        );

        let (items, third) = run(&fake, &cfg(json!({})), Some(second)).await.unwrap();
        assert_eq!(ids(&items), vec!["101"], "{:?}", fake.calls());
        assert_eq!(
            thread_of(&items, "101"),
            vec![("mara".to_owned(), "<p>late</p>".to_owned())]
        );
        let (idle, fourth) = run(&fake, &cfg(json!({})), Some(third.clone()))
            .await
            .unwrap();
        assert!(idle.is_empty(), "{:?}", ids(&idle));
        assert_eq!(fourth, third);
    }

    /// A comment record the walk cannot place produces nothing, and takes the
    /// run down with it in neither direction.
    ///
    /// Three shapes: a comment whose container the server did not expand, a
    /// **page** record -- whose container is its *space*, so a walk reading a
    /// container id blindly would fetch a space id as a page -- and a type
    /// this adapter never asked for.
    #[tokio::test]
    async fn a_comment_record_the_walk_cannot_place_contributes_nothing() {
        let fake = Fake::new(a_corpus()).with_comment_hits(vec![
            json!({ "id": "900", "type": "comment",
                    "version": { "number": 1, "when": "2026-09-03T09:00:00.000Z" } }),
            json!({ "id": "901", "type": "blogpost",
                    "container": { "id": 98305, "key": "ENG", "type": "space" },
                    "version": { "number": 1, "when": "2026-09-03T09:00:00.000Z" } }),
            json!({ "id": "902",
                    "version": { "number": 1, "when": "2026-09-03T09:00:00.000Z" } }),
        ]);
        let (_, first) = run(&fake, &cfg(json!({})), None).await.unwrap();
        let (items, _) = run(&fake, &cfg(json!({})), Some(first)).await.unwrap();
        assert!(items.is_empty(), "{:?}", ids(&items));
    }

    /// **#2073's bug, at this seam.** Somebody else comments on a page nobody
    /// has edited in weeks, naming nobody; the next incremental run delivers
    /// that page with the comment in its thread. The page's own version is
    /// untouched, so the page walk cannot be what found it.
    #[tokio::test]
    async fn a_comment_anybody_posts_reaches_the_next_run_on_a_page_nobody_edited() {
        let fake = Fake::new(a_corpus());
        let (_, first) = run(&fake, &cfg(json!({})), None).await.unwrap();
        let comment = plain_comment("910", "2026-09-03T09:00:00.000Z", 1, "mara", "looks fine");
        a_comment_lands_on(&fake, "101", comment);

        let (items, _) = run(&fake, &cfg(json!({})), Some(first)).await.unwrap();
        assert_eq!(ids(&items), vec!["101"], "{:?}", fake.calls());
        assert_eq!(
            thread_of(&items, "101"),
            vec![("mara".to_owned(), "<p>looks fine</p>".to_owned())]
        );
        assert_eq!(
            items[0].payload["version"]["number"],
            json!(1),
            "the page itself was not edited"
        );
    }

    /// A comment **edited** in Confluence is a new version of the comment and
    /// nothing at all to its page: the next run delivers the page again with
    /// the new text, and the one after is idle.
    #[tokio::test]
    async fn a_comment_edited_in_confluence_reaches_the_next_run_and_then_settles() {
        let fake = Fake::new(a_corpus());
        let (_, first) = run(&fake, &cfg(json!({})), None).await.unwrap();
        a_comment_lands_on(
            &fake,
            "102",
            plain_comment("920", "2026-09-03T09:00:00.000Z", 1, "knobas", "ship it"),
        );
        let (_, second) = run(&fake, &cfg(json!({})), Some(first)).await.unwrap();

        a_comment_lands_on(
            &fake,
            "102",
            plain_comment("920", "2026-09-04T08:13:00.000Z", 2, "knobas", "meh"),
        );
        let (items, third) = run(&fake, &cfg(json!({})), Some(second)).await.unwrap();
        assert_eq!(ids(&items), vec!["102"], "{:?}", fake.calls());
        assert_eq!(
            thread_of(&items, "102"),
            vec![("knobas".to_owned(), "<p>meh</p>".to_owned())]
        );

        let (idle, fourth) = run(&fake, &cfg(json!({})), Some(third.clone()))
            .await
            .unwrap();
        assert!(idle.is_empty(), "{:?}", ids(&idle));
        assert_eq!(
            fourth, third,
            "the idle poll hands the cursor back as it came"
        );
    }

    /// **The sync after a `Comment` write**, at this seam: the comment the
    /// account itself just posted mentions nobody, and the run that follows
    /// the write delivers the page with it in the thread -- before #2073 only
    /// the next page edit did.
    #[tokio::test]
    async fn the_run_after_a_comment_write_delivers_the_page_with_that_comment() {
        let fake = Fake::new(a_corpus());
        let (_, before) = run(&fake, &cfg(json!({})), None).await.unwrap();
        // What `write::comment` leaves on the server: a comment by the
        // credential's own account.
        a_comment_lands_on(
            &fake,
            "100",
            plain_comment(
                "930",
                "2026-09-03T09:00:00.000Z",
                1,
                "knobas",
                "does the SLA hold?",
            ),
        );
        let (items, _) = run(&fake, &cfg(json!({})), Some(before)).await.unwrap();
        assert_eq!(ids(&items), vec!["100"]);
        assert_eq!(
            thread_of(&items, "100"),
            vec![("knobas".to_owned(), "<p>does the SLA hold?</p>".to_owned())]
        );
    }

    /// **Hundreds of comments across many pages, then an idle poll.** Every
    /// comment stays in the window until a page edit moves the watermark past
    /// it, so a run that brought in 600 of them over 300 pages records 600
    /// `seen` entries beside the page walk's. In one cap of
    /// [`SEEN_CAP`](knobas_source::watermark::SEEN_CAP) the newest 500 win,
    /// the oldest comments and the page edit are forgotten, and the next poll
    /// re-delivers their pages -- every poll, until a page edit.
    #[tokio::test]
    async fn hundreds_of_comments_across_many_pages_settle_on_the_next_idle_poll() {
        let at = |base: &str, step: chrono::Duration| base.parse::<DateTime<Utc>>().unwrap() + step;
        let stamp = |t: DateTime<Utc>| t.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
        let space: Vec<Value> = (0..300)
            .map(|i| {
                let when = at("2026-07-01T00:00:00Z", chrono::Duration::minutes(i));
                page(&format!("{}", 1000 + i), "p", &stamp(when), 1)
            })
            .collect();
        let fake = Fake::new(space);
        let (_, first) = run(&fake, &cfg(json!({})), None).await.unwrap();

        // One page edit, which moves the watermark, then 600 comments after it.
        {
            let mut pages = fake.pages.lock().unwrap();
            pages[0]["version"] = json!({ "number": 2, "when": "2026-08-30T00:00:00.000Z" });
        }
        for j in 0..600 {
            let when = stamp(at("2026-09-01T00:00:00Z", chrono::Duration::seconds(j)));
            let page_id = format!("{}", 1000 + j % 300);
            let comment = plain_comment(&format!("{}", 5000 + j), &when, 1, "priya", "+1");
            a_comment_lands_on(&fake, &page_id, comment);
        }
        let (items, second) = run(&fake, &cfg(json!({})), Some(first)).await.unwrap();
        assert_eq!(items.len(), 300, "every commented page, once");
        assert_eq!(watermark(&second), "2026-08-30T00:00:00Z");

        let (idle, third) = run(&fake, &cfg(json!({})), Some(second.clone()))
            .await
            .unwrap();
        assert!(
            idle.is_empty(),
            "{} pages re-delivered: {:?}",
            idle.len(),
            ids(&idle).iter().take(5).collect::<Vec<_>>()
        );
        assert_eq!(third, second, "and the position did not move");
    }

    /// A full sync has just delivered every page with its whole discussion, so
    /// its comment walk starts where the next run's window will: two minutes
    /// before the new watermark, the band whose `seen` entries keep that run
    /// idle. Unbounded it would read every comment the wiki ever had.
    #[tokio::test]
    async fn a_full_sync_reads_only_the_comments_the_next_window_holds() {
        let fake = Fake::new(a_corpus());
        a_comment_lands_on(
            &fake,
            "100",
            plain_comment("940", "2026-07-03T00:00:00.000Z", 1, "mara", "old news"),
        );
        let (_, first) = run(&fake, &cfg(json!({})), None).await.unwrap();
        let comment_queries: Vec<String> = fake
            .calls()
            .into_iter()
            .filter(|c| c.starts_with("search type = comment"))
            .collect();
        assert_eq!(
            comment_queries,
            vec![
                "search type = comment AND lastmodified >= \"2026-08-22 10:38\" order by lastmodified asc"
            ],
        );
        let (idle, again) = run(&fake, &cfg(json!({})), Some(first.clone()))
            .await
            .unwrap();
        assert!(idle.is_empty(), "{:?}", ids(&idle));
        assert_eq!(again, first);
    }

    /// A comment on a **blog post** is in `type = comment` too, now that the
    /// walk asks for every comment: its container is the blog post, and
    /// following it would fetch the post and mirror it as a page, a kind the
    /// descriptor never declared. Only a comment whose container says `page`
    /// leads anywhere.
    #[tokio::test]
    async fn a_comment_on_a_blog_post_leads_nowhere() {
        let mut corpus = a_corpus();
        let mut post = page("700", "Release notes", "2026-07-01T00:00:00.000Z", 1);
        post["type"] = json!("blogpost");
        corpus.push(post);
        let fake = Fake::new(corpus);
        let (_, first) = run(&fake, &cfg(json!({})), None).await.unwrap();
        fake.comment_hits.lock().unwrap().push(json!({
            "id": "950", "type": "comment",
            "container": { "id": "700", "type": "blogpost" },
            "version": { "number": 1, "when": "2026-09-03T09:00:00.000Z" }
        }));
        let (items, _) = run(&fake, &cfg(json!({})), Some(first)).await.unwrap();
        assert!(items.is_empty(), "{:?}", ids(&items));
        assert!(
            !fake.calls().contains(&"content 700".to_owned()),
            "{:?}",
            fake.calls()
        );
    }

    /// A configured space list reaches the query. Without this the option
    /// would parse, validate, appear in the form -- and mirror the whole wiki.
    #[tokio::test]
    async fn the_configured_space_list_reaches_every_query_the_run_sends() {
        let fake = Fake::new(a_corpus());
        let scoped = cfg(json!({ "spaces": ["ENG"] }));
        let (_, cursor) = run(&fake, &scoped, None).await.unwrap();
        // An incremental run adds the positions sweep (#1138).
        run(&fake, &scoped, Some(cursor)).await.unwrap();
        let searches: Vec<String> = fake
            .calls()
            .into_iter()
            .filter(|c| c.starts_with("search "))
            .collect();
        assert_eq!(
            searches.len(),
            7,
            "the probe, the page walk and the comment walk, twice, and one positions sweep: \
             {searches:?}"
        );
        for search in &searches {
            assert!(
                search.contains("space in (\"ENG\")"),
                "the ceiling probe is scoped like the walk, or the clamp never binds: {search}"
            );
        }
    }

    /// [`page`] in the space `key`.
    fn page_in(key: &str, id: &str, when: &str) -> Value {
        let mut p = page(id, &format!("{key} {id}"), when, 1);
        p["space"]["key"] = json!(key);
        p
    }

    /// Two old pages, one per space, and a newer one in ENG.
    fn two_spaces() -> Vec<Value> {
        vec![
            page_in("ENG", "101", "2026-08-18T10:00:00.000Z"),
            page_in("OPS", "102", "2026-08-19T10:00:00.000Z"),
            page_in("ENG", "103", "2026-08-22T11:48:00.000Z"),
        ]
    }

    /// #2181: a space added to the scope brings in its pages older than the
    /// watermark. The cursor records the space list it was taken under, and
    /// a run under another one syncs the new scope in full.
    #[tokio::test]
    async fn a_space_added_to_the_scope_brings_its_older_pages() {
        let fake = Fake::new(two_spaces());
        let eng = cfg(json!({ "spaces": ["ENG"] }));
        let (first, cursor) = run(&fake, &eng, None).await.unwrap();
        assert_eq!(ids(&first), ["101", "103"]);
        let (idle, _) = run(&fake, &eng, Some(cursor.clone())).await.unwrap();
        assert!(idle.is_empty(), "{:?}", ids(&idle));

        let both = cfg(json!({ "spaces": ["ENG", "OPS"] }));
        let (widened, after) = run(&fake, &both, Some(cursor)).await.unwrap();
        assert!(
            ids(&widened).contains(&"102".to_owned()),
            "102 is older than the watermark: {:?}",
            ids(&widened)
        );
        let (idle, again) = run(&fake, &both, Some(after.clone())).await.unwrap();
        assert!(idle.is_empty(), "{:?}", ids(&idle));
        assert_eq!(again, after);
    }

    /// #2181: a cursor written before it recorded a space list matches none,
    /// so the first run after the upgrade syncs in full -- which is what
    /// brings in a space added before this build.
    #[tokio::test]
    async fn a_cursor_without_a_recorded_space_list_syncs_in_full() {
        let fake = Fake::new(two_spaces());
        let every = cfg(json!({}));
        let (_, cursor) = run(&fake, &every, None).await.unwrap();
        let mut legacy: Value = serde_json::from_str(&cursor).unwrap();
        assert_eq!(legacy["scope"], json!(""));
        legacy.as_object_mut().unwrap().remove("scope");
        let (again, _) = run(&fake, &every, Some(legacy.to_string())).await.unwrap();
        assert_eq!(ids(&again), ["101", "102", "103"]);
    }

    /// #2181: an edit outside the space list keeps the incremental position.
    #[tokio::test]
    async fn an_edit_outside_the_space_list_keeps_the_position() {
        let fake = Fake::new(two_spaces());
        let (_, cursor) = run(&fake, &cfg(json!({ "spaces": ["ENG"] })), None)
            .await
            .unwrap();
        let edited = cfg(json!({ "spaces": ["ENG"], "rate_per_sec": 2, "page_size": 10 }));
        let (idle, again) = run(&fake, &edited, Some(cursor.clone())).await.unwrap();
        assert!(idle.is_empty(), "{:?}", ids(&idle));
        assert_eq!(again, cursor);
    }

    /// #2181, ruled: a space taken out of the list is neither read nor
    /// tombstoned. Its pages stay in the mirror as last seen; the engine
    /// sweeps only a cursor-less run (ADR-0003).
    #[tokio::test]
    async fn a_space_taken_out_of_the_list_is_neither_read_nor_tombstoned() {
        let fake = Fake::new(two_spaces());
        let (_, cursor) = run(&fake, &cfg(json!({ "spaces": ["ENG", "OPS"] })), None)
            .await
            .unwrap();
        let eng = cfg(json!({ "spaces": ["ENG"] }));
        let (narrowed, after) = run(&fake, &eng, Some(cursor)).await.unwrap();
        assert_eq!(ids(&narrowed), ["101", "103"], "the rest, in full");
        assert!(narrowed.iter().all(|i| !i.deleted));
        let (idle, _) = run(&fake, &eng, Some(after)).await.unwrap();
        assert!(idle.is_empty(), "{:?}", ids(&idle));
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

    // -- the run's walk (#1381) -------------------------------------------------

    async fn walk_of(fake: &Fake, cursor: Option<String>) -> (Walking, String) {
        let mut sink = Walking::default();
        let next = SyncRun {
            api: fake,
            cfg: &cfg(json!({ "page_size": 2 })),
            source_id: "confluence",
            base_url: "http://127.0.0.1:8090",
        }
        .run(cursor, &mut sink)
        .await
        .expect("the run succeeds");
        (sink, next)
    }

    /// Where the API reports `totalSize`, the run reports `n of N pages` over
    /// it, before each page and once after the last -- across several result
    /// pages, and still to the end when an incremental run pushes nothing.
    #[tokio::test]
    async fn a_run_reports_its_walk_over_the_total_size_even_when_it_pushes_nothing() {
        let mut fake = Fake::new(a_corpus());
        fake.reports_total = true;
        let of_five = |done| (done, Some(5), "pages".to_owned());

        let (first, cursor) = walk_of(&fake, None).await;
        assert_eq!(first.items.len(), 5);
        assert_eq!(
            first.readings,
            (0..=5).map(of_five).collect::<Vec<_>>(),
            "0 to N, one reading per page and one after the last"
        );

        let (idle, _) = walk_of(&fake, Some(cursor)).await;
        assert!(
            idle.items.is_empty(),
            "idle run emitted {}",
            idle.items.len()
        );
        let last = idle.readings.last().expect("the idle run still reports");
        assert_eq!(last.1, Some(last.0), "the idle walk ends at its own total");
        assert_eq!(last.2, "pages");
        assert!(idle.readings.len() >= 2, "{:?}", idle.readings);
    }

    /// A server that reports no `totalSize` is read without a total: the
    /// count moves, `of` stays `None`, and the walk is unchanged. On an
    /// incremental run the count runs on through the sweep into the window
    /// one step at a time (#1389).
    #[tokio::test]
    async fn a_run_over_a_server_with_no_total_size_reports_the_count_alone() {
        let fake = Fake::new(a_corpus());
        let count_to = |last| {
            (0..=last)
                .map(|done| (done, None, "pages".to_owned()))
                .collect::<Vec<_>>()
        };
        let (run, cursor) = walk_of(&fake, None).await;
        assert_eq!(run.items.len(), 5);
        assert_eq!(run.readings, count_to(5));

        let (idle, _) = walk_of(&fake, Some(cursor)).await;
        let last = idle.readings.last().expect("the idle run reports").0;
        assert!(last > 5, "the five swept pages, then the window: {last}");
        assert_eq!(idle.readings, count_to(last));
    }

    /// An incremental run over a space larger than one sweep request (#1389)
    /// reports the positions sweep and the page walk as one walk of `pages`:
    /// the sweep's rows over its `totalSize`, then the window's rows on top,
    /// over both totals. The count never goes back and the unit never
    /// changes, so the bar the sweep fills is the one the walk finishes.
    #[tokio::test]
    async fn the_positions_sweep_moves_the_reading_and_the_walk_carries_on_from_it() {
        // 450 pages an hour apart: three sweep requests of 200, and a window
        // (the watermark less two minutes) that holds only the newest page.
        let space: Vec<Value> = (0..450)
            .map(|i| {
                let when = format!("2026-08-{:02}T{:02}:00:00.000Z", 1 + i / 24, i % 24);
                page(&format!("{}", 1000 + i), "p", &when, 1)
            })
            .collect();
        let mut fake = Fake::new(space);
        fake.reports_total = true;
        let (_, cursor) = walk_of(&fake, None).await;
        let before = fake.calls().len();

        let (idle, _) = walk_of(&fake, Some(cursor)).await;

        // The sweep's query is the only one ordered by creation alone.
        let sweeps = fake.calls()[before..]
            .iter()
            .filter(|c| c.contains("created") && !c.contains("id asc"))
            .count();
        assert_eq!(sweeps, 3, "the sweep reads 450 pages in three requests");
        assert!(idle.items.is_empty());
        let pages = |done, of| (done, Some(of), "pages".to_owned());
        let mut expected: Vec<_> = (0..450).map(|done| pages(done, 450)).collect();
        expected.extend([pages(450, 451), pages(451, 451)]);
        assert_eq!(
            idle.readings, expected,
            "0..450 of 450 through the sweep, then the window's one page on top"
        );
    }

    // -- who a mention names (#953) -------------------------------------------

    /// A page whose body says `body`, edited by `tomasz` (key `k-tw`).
    fn page_saying(id: &str, when: &str, body: &str) -> Value {
        let mut p = page(id, id, when, 1);
        p["body"]["storage"]["value"] = json!(body);
        p["version"]["by"] = json!({ "username": "tomasz", "userKey": "k-tw" });
        p
    }

    fn mention(key: &str) -> String {
        format!("<ac:link><ri:user ri:userkey=\"{key}\" /></ac:link>")
    }

    fn user_calls(fake: &Fake) -> Vec<String> {
        fake.calls()
            .into_iter()
            .filter(|c| c.starts_with("user "))
            .collect()
    }

    fn mention_names(item: &SyncItem) -> Vec<Option<String>> {
        use knobas_source::docs::{DocBlock, Inline};
        let doc = crate::blocks::doc_content("page", &item.payload).expect("a page projects");
        let mut blocks: Vec<DocBlock> = doc.blocks;
        for c in doc.comments {
            blocks.extend(c.blocks);
        }
        knobas_source::docs::inlines_mut(&mut blocks)
            .into_iter()
            .filter_map(|i| match i {
                Inline::Mention { name, .. } => Some(name.clone()),
                _ => None,
            })
            .collect()
    }

    /// Every key resolves, each looked up once per run: a key two pages share
    /// costs one request, and a key the record names (the editor) or the
    /// credential's own costs none.
    #[tokio::test]
    async fn every_mention_key_resolves_with_one_lookup_per_key_per_run() {
        let mara = mention("k-mara");
        let (tw, me) = (mention("k-tw"), mention(MY_KEY));
        let mut second = page_saying("2", "2026-08-02T10:00:00.000Z", "<p>none</p>");
        second["children"]["comment"]["results"] = json!([{
            "id": "77", "type": "comment",
            "body": { "storage": { "value": format!("<p>{mara} again</p>") } },
            "version": { "number": 1, "when": "2026-08-02T10:00:00.000Z", "by": { "username": "priya" } }
        }]);
        let mut fake = Fake::new(vec![
            page_saying(
                "1",
                "2026-08-01T10:00:00.000Z",
                &format!("<p>{mara} {tw} {me}</p>"),
            ),
            second,
        ]);
        fake.users.insert(
            "k-mara".into(),
            json!({ "type": "known", "userKey": "k-mara", "username": "mara", "displayName": "Mara Lindqvist" }),
        );
        let (items, _) = run(&fake, &cfg(json!({})), None).await.unwrap();
        assert_eq!(user_calls(&fake), vec!["user k-mara"]);
        assert_eq!(
            mention_names(&items[0]),
            vec![Some("Mara Lindqvist".into()), None, None],
            "mara by lookup; the editor and the credential carry no display name here"
        );
        assert_eq!(
            mention_names(&items[1]),
            vec![Some("Mara Lindqvist".into())]
        );
        assert_eq!(
            items[0].payload["mentioned_users"]
                .as_array()
                .map(|u| u.iter().map(|u| u["userKey"].clone()).collect::<Vec<_>>()),
            Some(vec![json!(MY_KEY), json!("k-mara")]),
            "the credential's own key is kept too, so the reader names it"
        );
        assert_eq!(
            items[0].payload["mentioned_users"][1],
            json!({ "userKey": "k-mara", "username": "mara", "displayName": "Mara Lindqvist" }),
            "only what names a mention is kept of the user record"
        );
    }

    /// A key Confluence does not know is asked once, stays a miss and leaves
    /// the payload as the server sent it.
    #[tokio::test]
    async fn a_key_nobody_answers_to_is_asked_once_and_stays_a_miss() {
        let gone = mention("k-gone");
        let fake = Fake::new(vec![
            page_saying("1", "2026-08-01T10:00:00.000Z", &format!("<p>{gone}</p>")),
            page_saying("2", "2026-08-02T10:00:00.000Z", &format!("<p>{gone}</p>")),
        ]);
        let (items, _) = run(&fake, &cfg(json!({})), None).await.unwrap();
        assert_eq!(user_calls(&fake), vec!["user k-gone"]);
        assert_eq!(mention_names(&items[0]), vec![None]);
        assert!(items[0].payload.get("mentioned_users").is_none());
    }

    /// A lookup that fails for any other reason does not fail the sync, and
    /// ends the lookups for the run rather than asking again per key.
    #[tokio::test]
    async fn a_failing_lookup_stops_the_lookups_and_the_pages_still_sync() {
        let mut fake = Fake::new(vec![
            page_saying(
                "1",
                "2026-08-01T10:00:00.000Z",
                &format!("<p>{}</p>", mention("k-a")),
            ),
            page_saying(
                "2",
                "2026-08-02T10:00:00.000Z",
                &format!("<p>{}</p>", mention("k-b")),
            ),
        ]);
        fake.user_fault = Some(SourceError::protocol("HTTP 429 after the retry budget"));
        let (items, _) = run(&fake, &cfg(json!({})), None).await.unwrap();
        assert_eq!(ids(&items), vec!["1", "2"]);
        assert_eq!(user_calls(&fake), vec!["user k-a"]);
    }

    // -- the refresh: one page read by id (ADR-0031, #2096) -------------------

    async fn refresh_of(
        fake: &Fake,
        cfg: &ConfluenceConfig,
        id: &str,
    ) -> Result<knobas_source::RefreshAnswer, SourceError> {
        SyncRun {
            api: fake,
            cfg,
            source_id: "confluence",
            base_url: "http://127.0.0.1:8090",
        }
        .refresh(id)
        .await
    }

    fn refreshed(answer: knobas_source::RefreshAnswer) -> SyncItem {
        match answer {
            knobas_source::RefreshAnswer::Item(item) => item,
            other => panic!("expected the page, got {other:?}"),
        }
    }

    fn gone(answer: knobas_source::RefreshAnswer) {
        assert!(
            matches!(answer, knobas_source::RefreshAnswer::Gone),
            "expected gone, got {answer:?}"
        );
    }

    /// **A refreshed page is the item a run emits for it**, field for field:
    /// its comments completed where the read did not expand them, and its
    /// mentions named, by the same mapping.
    #[tokio::test]
    async fn a_refreshed_page_is_the_item_a_run_emits_for_it() {
        let mut p = page_saying(
            "104",
            "2026-08-22T10:40:00.000Z",
            &format!("<p>ask {}</p>", mention("k-mara")),
        );
        p.as_object_mut().unwrap().remove("children");
        let mut fake = Fake::new(vec![p, page("105", "Other", "2026-08-23T00:00:00.000Z", 1)]);
        fake.users.insert(
            "k-mara".to_owned(),
            json!({ "type": "known", "userKey": "k-mara", "username": "mara", "displayName": "Mara Lind" }),
        );
        fake.detached_comments.lock().unwrap().insert(
            "104".to_owned(),
            vec![json!({
                "id": "98320",
                "type": "comment",
                "body": { "storage": { "value": "<p>can you add the SLA?</p>" } },
                "version": { "number": 1, "when": "2026-08-23T09:00:00.000Z", "by": { "username": "mara" } }
            })],
        );
        let cfg = cfg(json!({}));
        let (items, _) = run(&fake, &cfg, None).await.unwrap();
        let synced = items.iter().find(|i| i.entity.key == "104").unwrap();

        let item = refreshed(refresh_of(&fake, &cfg, "104").await.unwrap());
        assert_eq!(item.entity, synced.entity);
        assert_eq!(item.kind, synced.kind);
        assert_eq!(item.title, synced.title);
        assert_eq!(item.body_text, synced.body_text);
        assert_eq!(item.author, synced.author);
        assert_eq!(item.updated_at, synced.updated_at);
        assert_eq!(item.web_url, synced.web_url);
        assert_eq!(item.payload, synced.payload);
        assert!(!item.deleted);
        assert_eq!(
            item.payload["children"]["comment"]["results"][0]["id"], "98320",
            "the comment the read did not expand is completed"
        );
        assert_eq!(mention_names(&item), vec![Some("Mara Lind".to_owned())]);
    }

    /// **One read by id**, not a search: a page nobody's index has caught up
    /// with yet is read as Confluence holds it (CQL lags a write, the content
    /// endpoint does not).
    #[tokio::test]
    async fn a_refresh_reads_the_page_by_id_and_searches_nothing() {
        let fake = Fake::new(a_corpus());
        refreshed(refresh_of(&fake, &cfg(json!({})), "104").await.unwrap());
        assert_eq!(fake.calls(), vec!["current_user", "content 104"]);
    }

    /// **A page Confluence answers `404` for is gone**: deleted, or out of
    /// the account's sight, which a run would not deliver either.
    #[tokio::test]
    async fn a_page_confluence_answers_404_for_is_gone() {
        let fake = Fake::new(a_corpus());
        let answer = refresh_of(&fake, &cfg(json!({})), "999").await.unwrap();
        gone(answer);
    }

    /// **A page in the trash is gone**: no run's `type = page` search answers
    /// one, so the mirror must not hold it either.
    #[tokio::test]
    async fn a_trashed_page_is_gone() {
        let mut p = page(
            "104",
            "SEPA payout retry design",
            "2026-08-22T10:40:00.000Z",
            3,
        );
        p["status"] = json!("trashed");
        let fake = Fake::new(vec![p]);
        let answer = refresh_of(&fake, &cfg(json!({})), "104").await.unwrap();
        gone(answer);
    }

    /// **A page moved out of the configured spaces is gone**, as a full sync
    /// would tombstone it; the same page with no space list is the page.
    #[tokio::test]
    async fn a_page_outside_the_configured_spaces_is_gone() {
        let fake = Fake::new(a_corpus());
        let answer = refresh_of(&fake, &cfg(json!({ "spaces": ["OPS"] })), "104")
            .await
            .unwrap();
        gone(answer);
        refreshed(
            refresh_of(&fake, &cfg(json!({ "spaces": ["OPS", "ENG"] })), "104")
                .await
                .unwrap(),
        );
    }

    /// **A record that is not a page is not offered**: this adapter emits one
    /// kind, and a blog post or a comment is not an item of it.
    #[tokio::test]
    async fn a_record_that_is_not_a_page_is_not_offered() {
        let mut p = page("300", "Release notes", "2026-08-22T10:40:00.000Z", 1);
        p["type"] = json!("blogpost");
        let fake = Fake::new(vec![p]);
        let answer = refresh_of(&fake, &cfg(json!({})), "300").await.unwrap();
        assert!(
            matches!(answer, knobas_source::RefreshAnswer::NotOffered),
            "{answer:?}"
        );
    }

    /// **A failed read is an error, never gone** (SPI doc on
    /// `Source::refresh`): a refused credential tombstones nothing.
    #[tokio::test]
    async fn a_failed_read_is_an_error_and_never_gone() {
        let mut fake = Fake::new(a_corpus());
        fake.fault = Some(SourceError::Unauthorized { status: Some(401) });
        let error = refresh_of(&fake, &cfg(json!({})), "104").await.unwrap_err();
        assert!(
            matches!(error, SourceError::Unauthorized { .. }),
            "{error:?}"
        );

        let mut fake = Fake::new(a_corpus());
        fake.fault = Some(SourceError::Protocol {
            status: Some(500),
            message: "HTTP 500".into(),
        });
        let error = refresh_of(&fake, &cfg(json!({})), "104").await.unwrap_err();
        assert!(matches!(error, SourceError::Protocol { .. }), "{error:?}");
    }
}
