//! The issue walk: `startAt` paging that an issue leaving the listing cannot
//! make step over another (#1979).
//!
//! The walk is ordered by `created`, then `id` (ruling A1, #1720), so an edit
//! cannot move a row. A row can still leave: an issue deleted, moved to a
//! project outside the scope, or edited out of a `jql_filter`. When it leaves
//! from a page already read, every row behind it shifts up by one, and plain
//! offset paging steps over the first unread issue. `ticket` is
//! `full_sync_exhaustive`, so a full sync that returned `Ok` over that hole
//! would license the sweep to tombstone a live ticket.
//!
//! **Why not a keyset.** JQL's `id > N` reads like one and is not, measured
//! on the seeded Jira 10.3 for #1979: `id` is an alias of `issuekey`, so the
//! range means "later in the same project as issue N" (`id > <OPS-77>` answers
//! none of the PAY issues filed after it), and an `N` that no longer exists is
//! refused outright (`A value with ID '0' does not exist for the field 'id'`).
//! A walk keyed on the last issue it read would then fail whenever that issue
//! was deleted, which is the very case it is for.
//!
//! **What this does instead.** Every request after the first starts one row
//! early, at the last issue read, and the page is accepted only when its
//! first row is an issue the walk has already read. Over an order no edit can
//! move, an issue the walk has read sorts before every issue it has not, so a
//! page that opens on one has nothing unread before it. When the first row is
//! one the walk has not read, rows have left behind the cursor; the walk asks
//! again from twice as far back (the start of the listing at worst, where
//! there is nothing before the first row) and goes on from there. Rows it
//! meets twice are handed out once. So a deletion under the walk costs a
//! request or two and never fails the run, which is the false failure ruling
//! A5's shrinking-`total` check paid (A5 as amended by #1979).
//!
//! An issue that *enters* the scope behind the cursor shifts rows the other
//! way: the walk meets one it has read again and hands it out once. The issue
//! that entered is delivered when it lands on a page, and missed otherwise,
//! which is the class the ceiling clamp already covers: its `updated` is
//! newer than the run's ceiling, so the next incremental run reads it.

use std::collections::HashSet;

use knobas_source::SourceError;

use crate::api::JiraApi;
use crate::model::{RawIssue, SearchPage};

/// What one `/search` may ask for: the configuration's own limit
/// ([`crate::JiraConfig::from_json`]), which Jira DC clamps at.
const MAX_RESULTS: u32 = 1_000;

/// What [`IssueWalk::next`] answers.
pub(crate) enum Step {
    /// The issues on this page the walk had not handed out before, and the
    /// page's own `total`. Never empty.
    Page(SearchPage),
    /// The listing ended.
    End,
    /// The next request would pass the cap. Whether that fails the run is
    /// the caller's.
    Capped,
}

/// One walk over a `/search` listing. See the module docs.
pub(crate) struct IssueWalk<'a> {
    api: &'a (dyn JiraApi + 'a),
    jql: &'a str,
    fields: &'a str,
    page_size: u32,
    /// Requests sent, the asks from further back included.
    cap: u32,
    requests: u32,
    /// Where the next unread issue sat in the listing, as of the last page.
    at: u32,
    /// Every issue handed out, by id.
    read: HashSet<String>,
    ended: bool,
}

impl<'a> IssueWalk<'a> {
    pub(crate) fn new(
        api: &'a (dyn JiraApi + 'a),
        jql: &'a str,
        fields: &'a str,
        page_size: u32,
        cap: u32,
    ) -> Self {
        Self {
            api,
            jql,
            fields,
            page_size,
            cap,
            requests: 0,
            at: 0,
            read: HashSet::new(),
            ended: false,
        }
    }

    /// The next page of issues not handed out before, the end, or the cap.
    ///
    /// # Errors
    ///
    /// The search's own.
    pub(crate) async fn next(&mut self) -> Result<Step, SourceError> {
        if self.ended {
            return Ok(Step::End);
        }
        // How far behind `at` the request starts: the one row that anchors
        // it, or nothing on the first page, where there is nothing behind.
        let mut back = u32::from(self.at > 0);
        loop {
            if self.requests >= self.cap {
                return Ok(Step::Capped);
            }
            self.requests += 1;
            let start = self.at - back;
            let limit = self.page_size.saturating_add(back).min(MAX_RESULTS);
            let mut page = self.api.search(self.jql, start, limit, self.fields).await?;
            let anchored = start == 0
                || page
                    .issues
                    .first()
                    .is_some_and(|first| self.read.contains(identity(first)));
            if !anchored {
                back = back.saturating_mul(2).min(self.at);
                continue;
            }
            let reached =
                start.saturating_add(u32::try_from(page.issues.len()).unwrap_or(u32::MAX));
            page.issues
                .retain(|issue| self.read.insert(identity(issue).to_owned()));
            if page.issues.is_empty() && reached <= self.at {
                // Nothing past where the walk stood: the listing ended there,
                // whatever its `total` claims.
                self.ended = true;
                return Ok(Step::End);
            }
            self.at = reached;
            self.ended = self.at >= page.total;
            if !page.issues.is_empty() {
                return Ok(Step::Page(page));
            }
            if self.ended {
                return Ok(Step::End);
            }
            // Only rows met again, after an issue entered behind the cursor:
            // anchor on the new position and ask on.
            back = 1;
        }
    }
}

/// What the walk tells two issues apart by: the numeric id, which a move to
/// another project leaves alone (the key changes), or the key on a record
/// without one.
fn identity(issue: &RawIssue) -> &str {
    issue.issue.id.as_deref().unwrap_or(&issue.issue.key)
}
