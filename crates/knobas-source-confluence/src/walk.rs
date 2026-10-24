//! The page walk: offset pages that a page leaving the listing cannot make
//! step over another (#1976).
//!
//! The walk is ordered by `created`, then `id` ([`crate::cql::Order`]), so an
//! edit cannot move a row. A row can still leave: a page deleted, or moved to
//! a space outside the source. When it leaves from a result page already
//! read, every row behind it shifts up by one, and following `_links.next`
//! (an offset, `start=N`) steps over the first unread page. `page` is
//! `full_sync_exhaustive`, so a full sync that returned `Ok` over that hole
//! would license the sweep to tombstone a live page.
//!
//! **Why not a keyset.** Data Center's CQL refuses a range on `id`
//! (`Operator '>' is not supported for field 'id'. Supported operators are :
//! =, !=, IN, NOT IN`, measured on the seeded 9.2 for #1976), and its time
//! literals are minute-resolution, which a keyset on `created` would need a
//! tiebreak for that CQL cannot express.
//!
//! **What this does instead** is the Jira walk's (`knobas-source-jira`'s
//! `walk` module), over Confluence's paging: every request after the first
//! starts one row early, at the last page read, and is accepted only when its
//! first row is a page the walk has already read. Over an order no edit can
//! move, a page the walk has read sorts before every page it has not, so a
//! result that opens on one has nothing unread before it. Otherwise rows have
//! left behind the cursor, and the walk asks again from twice as far back
//! (the start of the listing at worst). Rows met twice are handed out once.
//!
//! Where the listing ends is still the server's word: the walk ends on a
//! result with no `_links.next`, as [`knobas_http::NextLink`] does, and an
//! empty result *with* one is followed. The next offset is the one the server
//! reports (`start` plus its own `limit`, which a request past
//! [`crate::config::MAX_PAGE_SIZE`] is clamped to), not one counted from the
//! rows that arrived.

use std::collections::HashSet;

use knobas_http::Walked;
use knobas_source::SourceError;

use crate::api::ConfluenceApi;
use crate::config::MAX_PAGE_SIZE;
use crate::model::ContentPage;

/// One walk over a content search. See the module docs.
pub(crate) struct ContentWalk<'a> {
    api: &'a (dyn ConfluenceApi + 'a),
    cql: &'a str,
    expand: &'a str,
    page_size: u32,
    /// The most requests the walk sends ([`Walked::Capped`] past it).
    cap: u32,
    /// Requests sent, the asks from further back included.
    requests: u32,
    /// Where the next unread page sits in the listing, as of the last result.
    at: u32,
    /// Every page handed out, by content id.
    read: HashSet<String>,
    ended: bool,
}

impl<'a> ContentWalk<'a> {
    pub(crate) fn new(
        api: &'a (dyn ConfluenceApi + 'a),
        cql: &'a str,
        expand: &'a str,
        page_size: u32,
        cap: u32,
    ) -> Self {
        Self {
            api,
            cql,
            expand,
            page_size,
            cap,
            requests: 0,
            at: 0,
            read: HashSet::new(),
            ended: false,
        }
    }

    /// The next result of pages not handed out before, the end, or the cap.
    ///
    /// # Errors
    ///
    /// The search's own.
    pub(crate) async fn next(&mut self) -> Result<Walked<ContentPage>, SourceError> {
        // How far behind `at` the request starts: the one row that anchors
        // it, or nothing on the first request, where there is nothing behind.
        let mut back = u32::from(self.at > 0);
        while !self.ended {
            if self.requests >= self.cap {
                return Ok(Walked::Capped);
            }
            self.requests += 1;
            let start = self.at - back;
            let limit = self.page_size.saturating_add(back).min(MAX_PAGE_SIZE);
            let mut page = self.api.search(self.cql, start, limit, self.expand).await?;
            let anchored = start == 0
                || page
                    .results
                    .first()
                    .is_some_and(|first| self.read.contains(&first.content.id));
            if !anchored {
                back = back.saturating_mul(2).min(self.at);
                continue;
            }
            let arrived = u32::try_from(page.results.len()).unwrap_or(u32::MAX);
            match page.links.next {
                Some(_) => {
                    let at = page.start.unwrap_or(start);
                    self.at = at.saturating_add(page.limit.unwrap_or(arrived));
                }
                None => self.ended = true,
            }
            page.results
                .retain(|row| self.read.insert(row.content.id.clone()));
            if !page.results.is_empty() {
                return Ok(Walked::Page(page));
            }
            // Only rows met again, or an empty result the server says is not
            // the last: anchor on the new position and ask on.
            back = u32::from(self.at > 0);
        }
        Ok(Walked::End)
    }
}
