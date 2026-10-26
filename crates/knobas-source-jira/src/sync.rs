//! One incremental (or full) sync run.
//!
//! The shape, and why each part is there:
//!
//! 1. `GET serverInfo` -- the server's UTC offset, because JQL date literals
//!    carry no zone.
//! 2. **The ceiling probe** ([`SyncRun::ceiling`], #1622): one row, the same
//!    scope, `ORDER BY updated DESC` -- the newest `updated` this source's
//!    corpus held when the run started.
//! 3. Read the cursor. Unreadable or absent ⇒ full sync (interfaces §4.1).
//!    Taken under another ticket scope (`projects` / `jql_filter`) ⇒ a full
//!    walk of this one, the views kept (#2181).
//! 4. Query `updated >= watermark − 2 minutes ORDER BY created ASC, id ASC`,
//!    paging with `startAt` until `total` is reached -- ordered by keys an
//!    edit cannot move (`id` breaks ties on `created`, #1720), each page
//!    asked from the last issue read so one that left the listing under the
//!    walk cannot hide another ([`crate::walk`], #1979).
//! 5. Drop what the overlap re-delivered, complete truncated comment/worklog
//!    containers, push the rest into the sink. A page's worklog completions
//!    run [`WORKLOG_READS_IN_FLIGHT`] at a time once the page is read; its
//!    items still reach the sink in walk order.
//! 6. If nothing was pushed, hand back the cursor byte-identically. Otherwise
//!    advance the watermark to `max(previous, min(newest updated emitted,
//!    ceiling))` ([`knobas_source::watermark::advanced`]) -- never to `now()`,
//!    because Jira's search index lags its own writes by seconds and an issue
//!    updated in that gap would be skipped forever.
//!
//! # The ceiling (`CONTEXT.md`, *Watermark*)
//!
//! A run is not instantaneous. An issue edited while the walk pages carries a
//! stamp later than anything the run saw at its start; advancing the
//! watermark to it would put the run's whole duration below the next query's
//! bound, and every other issue edited during the run, on a page the walk had
//! already passed, would never be offered again. Held at the ceiling, the next
//! run re-walks from just before the run started; the edited issue is in
//! `seen`, so that re-walk does not emit it twice. Witnessed by the probe, not
//! clocked: a ceiling too low costs a re-fetch, one too high loses work.
//!
//! # The two things the cursor layer is owed
//!
//! [`JiraCursor::since`] documents them and this is where they are honoured;
//! they fail in different places, so neither substitutes for the other.
//!
//! * **The previous watermark in [`Bounds`]**, so a re-delivered *older* item
//!   cannot drag the position backwards.
//! * **`seen_in_window` is every record the run saw**, skipped ones included --
//!   `seen` records what the *window* contained, not what crossed the SPI. A
//!   run that reports only what it emitted forgets, every run, whatever it
//!   recognised that run, and an idle poll never settles. The watermark holds
//!   perfectly while that happens, which is why the sequence that shows it is
//!   *one new issue and then an idle poll*, never a full sync and then one.
//!
//! # What "already delivered" means, and what it costs to get wrong
//!
//! An issue is recognised by [`knobas_source::fingerprint`] of the raw `/search` record and
//! **not** by its `updated` (issue #345, cursor version 2). `/search` reports
//! `updated` to the second, an issue changes in milliseconds, and this run is
//! fired after every landed write -- so keying on the timestamp dropped, for
//! ever, any change that landed in the same second as the one a run had just
//! recorded. The fingerprint is computed here, before the skip decision and before
//! `complete`, which is what keeps the comparison like-for-like across runs
//! without costing a request per issue.
//!
//! What that leaves, said plainly because it is #345's own shape: the
//! fingerprint covers what the skip decision sees, and `complete` then puts
//! more into what the sink receives. A change confined to a comment or worklog
//! beyond a truncated container, where the count does not move either, is
//! invisible to it, and if it also lands inside the second a previous run
//! recorded, that run skips it. That is narrower than the hole it replaced:
//! `(key, updated)` was blind to the same change and to every change that
//! needed no such coincidence.

use chrono::{DateTime, Utc};
use futures_util::{StreamExt, TryStreamExt, stream};
use knobas_http::{Offset, PageWalk, Walked};
use knobas_source::watermark::Bounds;
use knobas_source::{Sink, SourceError};

use crate::api::JiraApi;
use crate::cursor::JiraCursor;
use crate::jql::{build_ceiling_jql, build_jql};
use crate::map;
use crate::model::{RawIssue, SearchPage, ServerInfo};
use crate::walk::IssueWalk;
use crate::{JiraConfig, time::parse_jira_time};

/// The fields `/search` is asked for.
///
/// Explicit rather than Jira's `*navigable` default: a deterministic payload,
/// a smaller response, and no surprise when an instance changes its navigable
/// set. `comment` and `worklog` are in the list -- neither is navigable -- so
/// the common case costs one request per page instead of one per issue.
///
/// **Why these eighteen (issue #32).** This list used to stop after twelve,
/// and the reason it gave was the wrong way round: `knobas-mockd` validated
/// `fields=` against a closed set, so anything wider was a 400 plus a recorded
/// `UnknownField` violation, and the list "stopped where the certification
/// stopped". That makes a mock's coverage decide what production fetches. The
/// mock is there to certify the query, not to bound it, so mockd's set was
/// widened first (its deviation 5) and this follows.
///
/// The six that came back are `labels`, `parent`, `resolution`, `issuelinks`,
/// `timeoriginalestimate` and `timespent`. The reader is **`payload`**: spec
/// §3a keeps the raw record precisely so a later, smarter mapping can
/// re-project existing data without re-syncing, and the §3a generic detail
/// view renders out of it. So the TeamCity rule -- ask for nothing no reader
/// looks at -- lands differently here: a name left out of this list is not a
/// slightly larger response saved, it is data the mirror never holds, and
/// recovering it costs a full re-fetch of every issue in the source. `parent`
/// in particular is where epic membership lives, which is what the Contexts
/// work reads.
///
/// `components`, `fixVersions` and `timeestimate` are still out, and now for
/// their own reason rather than for mockd's: the Tidewater fixture records no
/// source for any of them, so mockd would have to invent one, and an invented
/// value reaches `payload` and the search index as if the dataset had said it
/// (the #28 ruling). They are one fixture field away, not one constant away.
///
/// Certified at the wire, not here: `tests/it/mockd.rs`'s
/// `a_synced_issue_carries_epic_membership_links_and_resolution` runs this
/// exact query against the mock and ends in `assert_no_violations()`, so a
/// name this instance would refuse fails there rather than silently thinning
/// the payload in production.
const BASE_FIELDS: &str = "summary,description,issuetype,status,priority,assignee,reporter,\
project,created,updated,labels,parent,resolution,issuelinks,timeoriginalestimate,timespent,\
comment,worklog";

/// A stop so a server that keeps reporting "more" cannot spin a run forever.
/// It counts requests, the walk's asks from further back included
/// (`crate::walk`); at the default page size it is about half a million
/// issues.
///
/// Reaching it is a **failure**, never a quiet end to the walk. The one kind
/// this adapter emits claims `full_sync_exhaustive: true`, which is the
/// engine's licence to tombstone every row a `cursor: None` run did not
/// return -- so a truncated walk reported as `Ok` authorises deleting whatever
/// fell off the end. The query is `ORDER BY created ASC`, so what falls off
/// the end is the *most recently filed* work: the tickets someone is looking
/// at today. And it is not a first-sync-only hazard, because any later
/// `cursor: None` run re-enters the same path against a populated mirror.
///
/// An incremental run gets the same refusal, and unconditionally. Two reasons,
/// the second of which was found by mutating the first away:
///
/// 1. The stop exists for a server that mispages -- one that keeps reporting
///    "more" without delivering it -- and "progress" measured against such a
///    server is progress against a number it is getting wrong, so advancing a
///    watermark on that basis skips whatever sat in the gap.
/// 2. **It is the only thing that terminates the loop.** The other exit is
///    the records read reaching `total`, and `total` is the very number the server is
///    inflating. Restricting the refusal to full syncs -- which looks like the
///    careful, narrower fix, since no sweep follows an incremental -- makes an
///    incremental against such a server run forever. Verified: the test below
///    does not terminate in 180 s with `&& cursor.is_none()` added here.
///
/// A source genuinely this large is narrowed with `projects` or `jql_filter`,
/// which is what the message says.
const MAX_PAGES: u32 = 5_000;

/// How many `GET /issue/{key}/worklog` a page's completion has in flight at
/// once (#2167). Data Center inlines at most 20 worklogs per issue in
/// `/search`, so every issue with more costs one read; one at a time, each
/// waiting its turn at the limiter, 200 such issues took a 1500-issue run
/// from 8.1 s to 45.2 s (#2163). Eight keeps the reads inside the default
/// burst of 40 (interfaces §4.1) with room for the walk's own pages.
const WORKLOG_READS_IN_FLIGHT: usize = 8;

/// The unit of the walk reading: the UI prints "120 of 300 issues".
const ISSUES: &str = "issues";

/// One issue of a page, read: whether the run delivers it (the overlap
/// re-delivered it if not) and its `updated`, the watermark's input.
struct PageIssue {
    raw: RawIssue,
    deliver: bool,
    updated: Option<DateTime<Utc>>,
}

pub(crate) struct SyncRun<'a> {
    pub api: &'a (dyn JiraApi + 'a),
    pub cfg: &'a JiraConfig,
    /// The instance id -- the `EntityRef` namespace (P10), not the adapter kind.
    pub source_id: &'a str,
    pub base_url: &'a str,
}

impl SyncRun<'_> {
    pub(crate) async fn run(
        &self,
        cursor: Option<String>,
        sink: &mut (dyn Sink + Send),
    ) -> Result<String, SourceError> {
        let server: ServerInfo = self.api.server_info().await?;
        let offset = server.offset_secs();
        let ceiling = self.ceiling().await?;
        let parsed = cursor.as_deref().and_then(JiraCursor::parse);
        // A cursor taken under another ticket scope walks this one in full
        // (#2181); one written under this scope is the cursor as it was.
        let scope = crate::jql::scope_key(self.cfg);
        let scope_moved = parsed
            .as_ref()
            .is_some_and(|c| c.scope.as_deref() != Some(scope.as_str()));
        let previous = parsed.map(|c| c.under(&scope));
        let since = previous.as_ref().and_then(|c| c.since(offset));
        let jql = build_jql(self.cfg, since, offset);
        let fields = self.fields();

        let mut emitted: usize = 0;
        // Every record this run *saw* in the window -- see the module docs.
        // Skipped ones belong here as much as emitted ones.
        let mut seen_in_window: Vec<crate::cursor::Seen> = Vec::new();
        // Seeded from the previous position, so `None` here means neither
        // the run nor the previous cursor carries a stamp. That the new
        // position never falls below the previous one is `Bounds::previous`'s
        // job in `advanced`, which holds it after the ceiling clamp too.
        let mut watermark: Option<DateTime<Utc>> = previous.as_ref().and_then(|c| c.updated_to);

        // Paged so an issue leaving the listing cannot hide another
        // (`crate::walk`, #1979).
        let mut walk = IssueWalk::new(self.api, &jql, &fields, self.cfg.page_size, MAX_PAGES);
        // The last page's own total, which the refusal quotes and the walk
        // reading draws against (#1381).
        let mut total = 0;
        // Issues the walk has got through, delivered or skipped: the walk
        // reading's numerator (#1381).
        let mut walked: u64 = 0;
        loop {
            let page: SearchPage = match walk.next().await? {
                Walked::Page(page) => page,
                Walked::End => break,
                Walked::Capped => {
                    // Not a `break`: see the note on MAX_PAGES. Leaving the
                    // loop here and returning `Ok` would hand the engine a
                    // partial walk wearing a completed one's clothes.
                    return Err(SourceError::protocol(format!(
                        "the Jira search did not reach its own reported total of {total} \
                         issues within {MAX_PAGES} pages of {}; refusing to report a partial \
                         walk as a completed sync. Narrow this source with `projects` or \
                         `jql_filter`.",
                        self.cfg.page_size
                    )));
                }
            };
            total = page.total;
            let mut page_issues: Vec<PageIssue> = Vec::new();
            for raw in page.issues {
                let updated = raw
                    .issue
                    .fields
                    .updated
                    .as_deref()
                    .and_then(parse_jira_time);
                // Taken before `complete`, which is what makes the value
                // comparable across runs -- see the module docs, *What
                // "already delivered" means*.
                let fingerprint = knobas_source::fingerprint::of(&raw.raw);
                if let Some(u) = updated {
                    seen_in_window.push(crate::cursor::Seen {
                        k: raw.issue.key.clone(),
                        u,
                        h: fingerprint.clone(),
                    });
                }
                // The overlap exists so nothing is missed; this is what keeps
                // it from also meaning "everything arrives twice".
                let deliver = !previous
                    .as_ref()
                    .is_some_and(|c| c.already_delivered(&raw.issue.key, &fingerprint));
                page_issues.push(PageIssue {
                    raw,
                    deliver,
                    updated,
                });
            }
            // Before any of the page reaches the sink, so one that fails
            // fails the run with nothing after it pushed (#2167).
            let mut due: Vec<&mut RawIssue> = page_issues
                .iter_mut()
                .filter(|issue| issue.deliver)
                .map(|issue| &mut issue.raw)
                .collect();
            self.complete_worklogs(&mut due).await?;
            for PageIssue {
                mut raw,
                deliver,
                updated,
            } in page_issues
            {
                // Before each issue, over the search's own total, so an
                // incremental run that pushes nothing still moves the bar.
                // A total that shrank under the run never reads below `done`.
                sink.walked(walked, Some(u64::from(total).max(walked)), ISSUES)
                    .await;
                walked += 1;
                if !deliver {
                    continue;
                }
                self.complete_comments(&mut raw).await?;
                let item = self.item_of(&raw);
                // Sink failures are never swallowed (SPI doc on `Source::sync`).
                sink.item(item).await?;
                emitted += 1;
                if let Some(u) = updated {
                    watermark = Some(watermark.map_or(u, |w| w.max(u)));
                }
            }
        }

        // Once after the last issue (#1381).
        sink.walked(walked, Some(u64::from(total).max(walked)), ISSUES)
            .await;

        // The Jira views (#877), after the issues: their keys are the issues'
        // keys, and a view read first would name tickets this run had not yet
        // mirrored.
        // Bounded (#1700, ruling D6): every board's and favourite's keys are
        // paged through on a read, so an idle run within the hour skips it and
        // carries the last read's views forward. A source with no views to
        // read asks nothing, so it always "reads".
        let server_now = server.server_time.as_deref().and_then(parse_jira_time);
        let read_views = emitted > 0
            || crate::views::due(
                previous.as_ref().and_then(|c| c.board_read.as_ref()),
                server_now,
                self.cfg,
            );
        let delivered = previous
            .as_ref()
            .map(|c| c.views.as_slice())
            .unwrap_or_default();
        let mut views_emitted = 0_usize;
        let (now, board_read): (Vec<crate::cursor::SeenView>, _) = if read_views {
            let views = crate::views::read(self.api, self.cfg, self.base_url).await?;
            let now: Vec<crate::cursor::SeenView> = views
                .iter()
                .map(|view| crate::cursor::SeenView {
                    k: view.key.clone(),
                    h: knobas_source::fingerprint::of(&view.payload),
                })
                .collect();
            for (view, seen) in views.iter().zip(&now) {
                if delivered.contains(seen) {
                    continue;
                }
                sink.item(crate::views::item(self.source_id, view)).await?;
                views_emitted += 1;
            }
            for old in delivered {
                if !now.iter().any(|seen| seen.k == old.k) {
                    sink.item(crate::views::gone(self.source_id, &old.k))
                        .await?;
                    views_emitted += 1;
                }
            }
            (
                now,
                server_now
                    .filter(|_| crate::views::reads_any(self.cfg))
                    .map(|at| crate::cursor::BoardRead {
                        at,
                        scope: crate::views::scope(self.cfg),
                    }),
            )
        } else {
            (
                delivered.to_vec(),
                previous.as_ref().and_then(|c| c.board_read.clone()),
            )
        };

        // Battery clause 2: nothing happened, so the position did not move.
        // Returning the *input string* rather than a re-encoded struct makes
        // that byte-identical by construction.
        // A board read that moved the read stamp is a change to the cursor too,
        // or the hour would never restart.
        let stamp_moved =
            previous.as_ref().and_then(|c| c.board_read.as_ref()) != board_read.as_ref();
        if emitted == 0
            && views_emitted == 0
            && !stamp_moved
            && !scope_moved
            && let Some(cursor) = cursor
        {
            return Ok(cursor);
        }
        let bounds = Bounds {
            previous: previous.as_ref().and_then(|c| c.updated_to),
            ceiling,
        };
        let mut next = match (emitted, watermark) {
            // No issue moved: the issue half of the position is where it was.
            // Also where items arrived but neither they nor the previous
            // cursor carry a readable `updated`; advancing to anything would
            // be a guess. An issue with no `updated` at all is therefore
            // re-delivered on every run -- it never enters `seen`, because
            // `advanced` bounds that set by the timestamp such an issue does
            // not have. Real Jira always sets the field, and mockd synthesizes
            // it for the fixture's one null, so this is a tolerated shape and
            // not a supported one.
            (0, _) | (_, None) => previous.unwrap_or_else(|| JiraCursor::empty(offset)),
            (_, Some(w)) => JiraCursor::advanced(bounds, w, offset, &seen_in_window),
        };
        next.views = now;
        next.board_read = board_read;
        next.scope = Some(scope);
        Ok(next.encode())
    }

    /// The run-start probe (#1622): the newest `updated` in this source's
    /// scope, which the watermark may not pass this run. One row and one
    /// field; `None` for an empty corpus, which clamps nothing.
    async fn ceiling(&self) -> Result<Option<DateTime<Utc>>, SourceError> {
        let page = self
            .api
            .search(&build_ceiling_jql(self.cfg), 0, 1, "updated")
            .await?;
        Ok(page
            .issues
            .first()
            .and_then(|raw| raw.issue.fields.updated.as_deref())
            .and_then(parse_jira_time))
    }

    /// Fill in what `/search` truncated.
    ///
    /// A container that is absent is treated as truncated: a server that
    /// ignored `fields=comment` must not be read as "this issue has no
    /// comments" -- that would silently drop the discussion out of the search
    /// corpus.
    pub(crate) async fn complete(&self, raw: &mut RawIssue) -> Result<(), SourceError> {
        self.complete_comments(raw).await?;
        self.complete_worklogs(&mut [raw]).await
    }

    /// [`complete`](Self::complete)'s comment half, one issue at a time:
    /// Data Center inlines every comment, so it rarely asks anything.
    async fn complete_comments(&self, raw: &mut RawIssue) -> Result<(), SourceError> {
        let key = raw.issue.key.clone();

        let comments_truncated = match &raw.issue.fields.comment {
            None => true,
            Some(c) => c.total.unwrap_or(0) as usize > c.comments.len(),
        };
        if comments_truncated {
            let mut all = Vec::new();
            // No cap: the walk ends on the server's total or an empty page,
            // as it always has.
            let mut walk = PageWalk::new(Offset, u32::MAX, |start_at| {
                self.api.comments(&key, start_at, self.cfg.page_size)
            });
            while let Walked::Page(page) = walk.next().await? {
                all.extend(page.comments);
            }
            raw.set_comments(all);
        }
        Ok(())
    }

    /// [`complete`](Self::complete)'s worklog half, for a page of issues at
    /// once: every truncated or absent container is read,
    /// [`WORKLOG_READS_IN_FLIGHT`] at a time (#2167). The first failure in
    /// walk order is the answer, and the reads still in flight are dropped.
    async fn complete_worklogs(&self, raws: &mut [&mut RawIssue]) -> Result<(), SourceError> {
        let due: Vec<(usize, String)> = raws
            .iter()
            .enumerate()
            .filter(|(_, raw)| worklogs_truncated(raw))
            .map(|(at, raw)| (at, raw.issue.key.clone()))
            .collect();
        // Collected first: a stream holding the `map` closure across an
        // await is not provably `Send` to the `Source::sync` future.
        // No paging: the WADL declares no query parameters on this
        // resource. What the server returns is what M1 records;
        // `fields.worklog.total` keeps the truth visible. M3's time work
        // uses /rest/api/2/worklog/updated + /worklog/list instead.
        let reads: Vec<_> = due.iter().map(|(_, key)| self.api.worklogs(key)).collect();
        let pages: Vec<_> = stream::iter(reads)
            .buffered(WORKLOG_READS_IN_FLIGHT)
            .try_collect()
            .await?;
        for ((at, _), page) in due.iter().zip(pages) {
            raws[*at].set_worklogs(page.worklogs);
        }
        Ok(())
    }

    /// One completed record as the item this source emits for it: the walk's
    /// and a refresh's one mapping (#2097).
    pub(crate) fn item_of(&self, raw: &RawIssue) -> knobas_source::SyncItem {
        map::to_sync_item(
            self.source_id,
            self.base_url,
            self.cfg.epic_link_field.as_deref(),
            self.cfg.sprint_field.as_deref(),
            raw,
        )
    }

    pub(crate) fn fields(&self) -> String {
        let mut fields = BASE_FIELDS.to_owned();
        for extra in self
            .cfg
            .epic_link_field
            .iter()
            .chain(&self.cfg.sprint_field)
        {
            fields.push(',');
            fields.push_str(extra);
        }
        fields
    }
}

/// Whether a record's worklog container needs reading whole: it carries
/// fewer than its `total`, or it is absent, which is never read as "no
/// worklogs" (see [`SyncRun::complete`]).
fn worklogs_truncated(raw: &RawIssue) -> bool {
    match &raw.issue.fields.worklog {
        None => true,
        Some(w) => w.total.unwrap_or(0) as usize > w.worklogs.len(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // `super::*` already brings in Sink, SourceError, JiraApi, SearchPage and
    // ServerInfo; the fake also has to answer the other three calls.
    use crate::model::{CommentPage, Myself, WorklogPage};
    use knobas_source::contract::VecSink;
    use std::collections::HashMap;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// An issue as `/search` would return it, with a comment container that may
    /// claim more comments than it carries.
    fn issue(key: &str, updated: &str, inline: usize, total: usize) -> serde_json::Value {
        serde_json::json!({
            "key": key,
            "fields": {
                "summary": format!("{key} summary"),
                "updated": updated,
                "comment": {
                    "startAt": 0, "maxResults": inline, "total": total,
                    "comments": (0..inline).map(|i| serde_json::json!({
                        "id": format!("{key}-c{i}"), "body": format!("inline {i}")
                    })).collect::<Vec<_>>()
                },
                "worklog": { "startAt": 0, "maxResults": 0, "total": 0, "worklogs": [] }
            }
        })
    }

    struct FakeApi {
        issues: Vec<serde_json::Value>,
        comments: HashMap<String, Vec<serde_json::Value>>,
        worklogs: HashMap<String, Vec<serde_json::Value>>,
        /// The zone `serverInfo` reports and JQL literals are resolved in.
        /// Defaults to `+02:00` like `knobas-mockd`, and deliberately not to
        /// UTC: a fake on UTC cannot tell a watermark rendered in the server's
        /// zone from one rendered in UTC, which is the trap this adapter's
        /// whole `serverInfo` call exists to avoid.
        offset_secs: i32,
        /// Minutes the server clock stands past 11:48 UTC.
        clock_minutes: i64,
        /// Extra `total` the fake claims beyond what it will ever return -- a
        /// server whose index shrank under the run.
        phantom_total: u32,
        /// A server that never stops promising more: every page carries one
        /// fresh issue and a `total` the walk can never reach.
        endless: bool,
        unauthorized: bool,
        /// Edits made while the walk pages: `(key, updated)` applied to every
        /// page after the first, so an issue the walk has not reached yet
        /// arrives edited and one it already passed does not.
        edited_mid_walk: Vec<(&'static str, &'static str)>,
        /// Milliseconds a key's worklog read takes, so reads can overlap and
        /// finish out of walk order.
        worklog_ms: HashMap<String, u64>,
        /// The key whose worklog read fails.
        failing_worklogs: Option<&'static str>,
        /// Worklog reads under way now, and the most there ever were.
        worklogs_in_flight: AtomicUsize,
        most_worklogs_in_flight: AtomicUsize,
        calls: Mutex<Vec<String>>,
    }

    impl Default for FakeApi {
        fn default() -> Self {
            Self {
                issues: Vec::new(),
                comments: HashMap::new(),
                worklogs: HashMap::new(),
                offset_secs: 2 * 3600,
                clock_minutes: 0,
                phantom_total: 0,
                endless: false,
                unauthorized: false,
                edited_mid_walk: Vec::new(),
                worklog_ms: HashMap::new(),
                failing_worklogs: None,
                worklogs_in_flight: AtomicUsize::new(0),
                most_worklogs_in_flight: AtomicUsize::new(0),
                calls: Mutex::new(Vec::new()),
            }
        }
    }

    impl FakeApi {
        fn calls(&self) -> Vec<String> {
            self.calls.lock().unwrap().clone()
        }
        fn count(&self, prefix: &str) -> usize {
            self.calls()
                .iter()
                .filter(|c| c.starts_with(prefix))
                .count()
        }
        fn searches(&self) -> Vec<String> {
            self.calls()
                .into_iter()
                .filter(|c| c.starts_with("search@"))
                .collect()
        }
        /// Rewrite one issue's `updated`, the way an edit upstream would.
        fn edit(&mut self, key: &str, updated: &str) {
            self.field_of(key)["updated"] = serde_json::json!(updated);
        }

        /// Rewrite one issue's assignee and **leave `updated` where it is** --
        /// the shape issue #345 is about, since `/search` renders two changes
        /// inside one second as one stamp. See
        /// [`two_changes_inside_one_second_are_two_versions_and_both_are_delivered`].
        fn reassign(&mut self, key: &str, name: &str) {
            self.field_of(key)["assignee"] = serde_json::json!({
                "name": name, "displayName": name, "active": true
            });
        }

        fn field_of(&mut self, key: &str) -> &mut serde_json::Value {
            let hit = self
                .issues
                .iter_mut()
                .find(|i| i["key"] == key)
                .unwrap_or_else(|| panic!("no {key} in the fake"));
            &mut hit["fields"]
        }
    }

    /// The fake honours the one JQL clause the run depends on. Without it the
    /// incremental tests would prove nothing.
    ///
    /// The literal is resolved **in the server's zone**, never in UTC -- that
    /// is what a real Jira does, what mockd does, and the only reason the
    /// adapter reads `serverInfo.serverTime` at all.
    fn since_of(jql: &str, offset_secs: i32) -> Option<chrono::DateTime<chrono::Utc>> {
        use chrono::TimeZone;
        let rest = jql.split("updated >= \"").nth(1)?;
        let literal = rest.split('"').next()?;
        let naive = chrono::NaiveDateTime::parse_from_str(literal, "%Y-%m-%d %H:%M").ok()?;
        let zone = chrono::FixedOffset::east_opt(offset_secs)?;
        zone.from_local_datetime(&naive)
            .single()
            .map(|t| t.with_timezone(&chrono::Utc))
    }

    /// Whether `issue` is inside the query's `project in (…)` clause, as Jira
    /// reads it: a query without one names every project.
    fn in_projects(jql: &str, issue: &serde_json::Value) -> bool {
        let Some(rest) = jql.split("project in (").nth(1) else {
            return true;
        };
        let list = rest.split(')').next().unwrap_or_default();
        let project = issue["key"]
            .as_str()
            .and_then(|key| key.split('-').next())
            .unwrap_or_default();
        list.split(", ")
            .any(|quoted| quoted.trim_matches('"') == project)
    }

    #[async_trait::async_trait]
    impl JiraApi for FakeApi {
        async fn server_info(&self) -> Result<ServerInfo, SourceError> {
            self.calls.lock().unwrap().push("serverInfo".to_owned());
            let (sign, mins) = if self.offset_secs < 0 {
                ('-', -self.offset_secs / 60)
            } else {
                ('+', self.offset_secs / 60)
            };
            Ok(serde_json::from_value(serde_json::json!({
                "version": "9.17.0", "deploymentType": "Server",
                // The same instant (11:48 UTC) on whatever wall clock the zone
                // reads: a zone change is not an hour passing.
                "serverTime": format!(
                    "{}{sign}{:02}{:02}",
                    (chrono::NaiveDate::from_ymd_opt(2026, 8, 22).unwrap().and_hms_opt(11, 48, 0).unwrap()
                        + chrono::Duration::seconds(i64::from(self.offset_secs))
                        + chrono::Duration::minutes(self.clock_minutes))
                    .format("%Y-%m-%dT%H:%M:%S.000"),
                    mins / 60,
                    mins % 60
                )
            }))
            .unwrap())
        }

        async fn myself(&self) -> Result<Myself, SourceError> {
            Ok(serde_json::from_value(serde_json::json!({ "name": "mara.lindqvist" })).unwrap())
        }

        /// A Jira Software with no boards: it answers, and the call is counted.
        async fn boards(
            &self,
            _project: Option<&str>,
            _start_at: u32,
            _max: u32,
        ) -> Result<crate::views::BoardPage, SourceError> {
            self.calls.lock().unwrap().push("boards".to_owned());
            Ok(
                serde_json::from_value(serde_json::json!({ "values": [], "isLast": true }))
                    .unwrap(),
            )
        }

        /// A sync run never asks for the field table -- discovery is
        /// `test_connection`'s, once per *Test connection*. An empty answer
        /// here is therefore not a fixture gap: a fake that returned fields
        /// would be describing a call these tests would never see made.
        ///
        /// The call is still **recorded**, and that is the point: every
        /// assertion below reads `calls` as an exact sequence, so a sync run
        /// that started probing the field table would show up as a `"field"`
        /// nobody expected rather than as one extra silent request per run.
        async fn fields(&self) -> Result<Vec<crate::model::FieldMeta>, SourceError> {
            self.calls.lock().unwrap().push("field".to_owned());
            Ok(Vec::new())
        }

        async fn search(
            &self,
            jql: &str,
            start_at: u32,
            max_results: u32,
            _fields: &str,
        ) -> Result<SearchPage, SourceError> {
            if self.unauthorized {
                return Err(SourceError::unauthorized());
            }
            // The ceiling probe: the newest issue, as the walk would first
            // see it. Recorded apart from the walk's pages so the page
            // sequences below read as the walk alone.
            if jql.ends_with("ORDER BY updated DESC") {
                self.calls.lock().unwrap().push("ceiling".to_owned());
                let newest = self
                    .issues
                    .iter()
                    .filter(|i| in_projects(jql, i))
                    .max_by_key(|i| {
                    i["fields"]["updated"]
                        .as_str()
                        .and_then(crate::time::parse_jira_time)
                });
                return Ok(serde_json::from_value(serde_json::json!({
                    "startAt": 0, "maxResults": max_results,
                    "total": self.issues.len(), "issues": newest.into_iter().collect::<Vec<_>>()
                }))
                .unwrap());
            }
            self.calls
                .lock()
                .unwrap()
                .push(format!("search@{start_at}"));
            // A listing without end, two rows a page so a page asked from
            // the last issue read still brings a new one.
            if self.endless {
                let row = |n: u32| issue(&format!("PAY-{n}"), "2026-08-22T11:48:00.000+0000", 0, 0);
                return Ok(serde_json::from_value(serde_json::json!({
                    "startAt": start_at, "maxResults": max_results, "total": u32::MAX,
                    "issues": [row(start_at), row(start_at + 1)]
                }))
                .unwrap());
            }
            let since = since_of(jql, self.offset_secs);
            let matching: Vec<serde_json::Value> = self
                .issues
                .iter()
                .filter(|i| in_projects(jql, i))
                .filter(|i| {
                    let updated = i["fields"]["updated"]
                        .as_str()
                        .and_then(crate::time::parse_jira_time);
                    match (since, updated) {
                        (Some(s), Some(u)) => u >= s,
                        (Some(_), None) => false,
                        (None, _) => true,
                    }
                })
                .cloned()
                .collect();
            let mut window: Vec<serde_json::Value> = matching
                .iter()
                .skip(start_at as usize)
                .take(max_results as usize)
                .cloned()
                .collect();
            if start_at > 0 {
                for (key, updated) in &self.edited_mid_walk {
                    for hit in window.iter_mut().filter(|i| i["key"] == *key) {
                        hit["fields"]["updated"] = serde_json::json!(updated);
                    }
                }
            }
            Ok(serde_json::from_value(serde_json::json!({
                "startAt": start_at, "maxResults": max_results,
                "total": matching.len() as u32 + self.phantom_total, "issues": window
            }))
            .unwrap())
        }

        async fn comments(
            &self,
            key: &str,
            start_at: u32,
            max_results: u32,
        ) -> Result<CommentPage, SourceError> {
            self.calls
                .lock()
                .unwrap()
                .push(format!("comments:{key}@{start_at}"));
            let all = self.comments.get(key).cloned().unwrap_or_default();
            let window: Vec<serde_json::Value> = all
                .iter()
                .skip(start_at as usize)
                .take(max_results as usize)
                .cloned()
                .collect();
            Ok(serde_json::from_value(serde_json::json!({
                "startAt": start_at, "maxResults": max_results,
                "total": all.len(), "comments": window
            }))
            .unwrap())
        }

        async fn worklogs(&self, key: &str) -> Result<WorklogPage, SourceError> {
            self.calls.lock().unwrap().push(format!("worklogs:{key}"));
            let now = self.worklogs_in_flight.fetch_add(1, Ordering::SeqCst) + 1;
            self.most_worklogs_in_flight
                .fetch_max(now, Ordering::SeqCst);
            if let Some(ms) = self.worklog_ms.get(key) {
                tokio::time::sleep(std::time::Duration::from_millis(*ms)).await;
            }
            self.worklogs_in_flight.fetch_sub(1, Ordering::SeqCst);
            if self.failing_worklogs == Some(key) {
                return Err(SourceError::Unreachable(format!(
                    "{key}'s worklogs timed out"
                )));
            }
            let all = self.worklogs.get(key).cloned().unwrap_or_default();
            Ok(serde_json::from_value(serde_json::json!({
                "startAt": 0, "maxResults": all.len(), "total": all.len(), "worklogs": all
            }))
            .unwrap())
        }
    }

    struct FailingSink;
    #[async_trait::async_trait]
    impl Sink for FailingSink {
        async fn item(&mut self, _item: knobas_source::SyncItem) -> Result<(), SourceError> {
            Err(SourceError::Sink("sink is down".to_owned()))
        }
    }

    fn cfg(value: serde_json::Value) -> crate::JiraConfig {
        crate::JiraConfig::from_json(&value).unwrap()
    }

    async fn run(
        api: &dyn JiraApi,
        cfg: &crate::JiraConfig,
        cursor: Option<String>,
        sink: &mut (dyn Sink + Send),
    ) -> Result<String, SourceError> {
        SyncRun {
            api,
            cfg,
            source_id: "jira",
            base_url: "https://jira.example",
        }
        .run(cursor, sink)
        .await
    }

    /// One run, returning the keys it emitted and the cursor it handed back.
    async fn keys_of(
        api: &dyn JiraApi,
        cfg: &crate::JiraConfig,
        cursor: Option<String>,
    ) -> (Vec<String>, String) {
        let mut sink = VecSink(Vec::new());
        let next = run(api, cfg, cursor, &mut sink)
            .await
            .expect("run succeeds");
        (sink.0.iter().map(|i| i.entity.key.clone()).collect(), next)
    }

    /// [`issue`] with an assignee, which is what §4.1's `author` maps from on
    /// Jira (`crate::map`).
    fn assigned(key: &str, updated: &str, name: &str) -> serde_json::Value {
        let mut raw = issue(key, updated, 0, 0);
        raw["fields"]["assignee"] = serde_json::json!({
            "name": name, "displayName": name, "active": true
        });
        raw
    }

    fn five_issues() -> Vec<serde_json::Value> {
        vec![
            issue("PAY-219", "2026-08-18T10:00:00.000+0000", 0, 0),
            issue("OPS-77", "2026-08-19T10:00:00.000+0000", 0, 0),
            issue("PAY-236", "2026-08-20T10:00:00.000+0000", 0, 0),
            issue("PAY-228", "2026-08-21T16:05:00.000+0000", 0, 0),
            issue("PAY-231", "2026-08-22T11:48:00.000+0000", 2, 2),
        ]
    }

    /// The board read is bounded (#1700, ruling D6): an idle run within the
    /// hour asks Jira for no board; a run that emitted an issue, or one an hour
    /// on, does.
    #[tokio::test]
    async fn an_idle_run_within_the_hour_reads_no_board() {
        let mut api = FakeApi {
            issues: five_issues(),
            ..FakeApi::default()
        };
        let config = cfg(serde_json::json!({ "boards": "synced", "projects": ["PAY"] }));
        let (_, cursor) = keys_of(&api, &config, None).await;
        let first = api.count("boards");
        assert!(first > 0, "the first run reads the boards");

        api.clock_minutes = 59;
        let (idle, again) = keys_of(&api, &config, Some(cursor.clone())).await;
        assert!(idle.is_empty(), "{idle:?}");
        assert_eq!(
            api.count("boards"),
            first,
            "no board request within the hour"
        );
        assert_eq!(again, cursor);
    }

    #[tokio::test]
    async fn a_run_that_emitted_an_issue_reads_the_boards() {
        let mut api = FakeApi {
            issues: five_issues(),
            ..FakeApi::default()
        };
        let config = cfg(serde_json::json!({ "boards": "synced", "projects": ["PAY"] }));
        let (_, cursor) = keys_of(&api, &config, None).await;
        let first = api.count("boards");

        api.issues
            .push(issue("PAY-300", "2026-08-22T11:50:00.000+0000", 0, 0));
        api.clock_minutes = 2;
        let (emitted, _) = keys_of(&api, &config, Some(cursor)).await;
        assert_eq!(emitted, ["PAY-300"]);
        assert!(
            api.count("boards") > first,
            "an emitted issue reads the boards"
        );
    }

    #[tokio::test]
    async fn an_idle_run_an_hour_on_reads_the_boards_and_restarts_the_hour() {
        let mut api = FakeApi {
            issues: five_issues(),
            ..FakeApi::default()
        };
        let config = cfg(serde_json::json!({ "boards": "synced", "projects": ["PAY"] }));
        let (_, cursor) = keys_of(&api, &config, None).await;
        let first = api.count("boards");

        api.clock_minutes = 60;
        let (idle, hourly) = keys_of(&api, &config, Some(cursor.clone())).await;
        assert!(idle.is_empty(), "{idle:?}");
        let second = api.count("boards");
        assert!(second > first, "an hour on, the boards are read");
        assert_ne!(
            hourly, cursor,
            "the read is recorded, or the hour never restarts"
        );

        api.clock_minutes = 90;
        let (_, later) = keys_of(&api, &config, Some(hourly.clone())).await;
        assert_eq!(api.count("boards"), second, "thirty minutes after the read");
        assert_eq!(later, hourly);
    }

    #[tokio::test]
    async fn a_changed_boards_setting_is_read_at_once() {
        let api = FakeApi {
            issues: five_issues(),
            ..FakeApi::default()
        };
        let (_, cursor) = keys_of(
            &api,
            &cfg(serde_json::json!({ "boards": "synced", "projects": ["PAY"] })),
            None,
        )
        .await;
        let first = api.count("boards");
        keys_of(
            &api,
            &cfg(serde_json::json!({ "boards": "all", "projects": ["PAY"] })),
            Some(cursor),
        )
        .await;
        assert!(api.count("boards") > first);
    }

    /// #2181: a project added to the scope brings in its tickets older than
    /// the watermark. The cursor records the scope it was taken under, and a
    /// run under another scope walks the new one in full.
    #[tokio::test]
    async fn a_project_added_to_the_scope_brings_its_older_tickets() {
        let api = FakeApi {
            issues: five_issues(),
            ..FakeApi::default()
        };
        let pay = cfg(serde_json::json!({ "projects": ["PAY"] }));
        let (first, cursor) = keys_of(&api, &pay, None).await;
        assert!(!first.contains(&"OPS-77".to_owned()), "{first:?}");
        let (idle, _) = keys_of(&api, &pay, Some(cursor.clone())).await;
        assert!(idle.is_empty(), "{idle:?}");

        let both = cfg(serde_json::json!({ "projects": ["PAY", "OPS"] }));
        let (widened, after) = keys_of(&api, &both, Some(cursor)).await;
        assert!(
            widened.contains(&"OPS-77".to_owned()),
            "OPS-77 is older than the watermark: {widened:?}"
        );
        let (idle, again) = keys_of(&api, &both, Some(after.clone())).await;
        assert!(idle.is_empty(), "settled under the new scope: {idle:?}");
        assert_eq!(again, after);
    }

    /// #2181: a scope change that brings nothing still records the new scope,
    /// or every later run would walk it in full again.
    #[tokio::test]
    async fn a_scope_change_that_emits_nothing_still_records_the_scope() {
        let api = FakeApi {
            issues: five_issues(),
            ..FakeApi::default()
        };
        let quiet = |projects: &[&str]| {
            cfg(serde_json::json!({
                "projects": projects, "boards": "none", "favourite_filters": false
            }))
        };
        let (_, cursor) = keys_of(&api, &quiet(&["PAY"]), None).await;
        let (none, after) = keys_of(&api, &quiet(&["NEW"]), Some(cursor.clone())).await;
        assert!(none.is_empty(), "{none:?}");
        assert_ne!(after, cursor);
        let recorded = crate::cursor::JiraCursor::parse(&after).unwrap().scope;
        assert_eq!(recorded.as_deref(), Some("project in (\"NEW\")"));
    }

    /// #2181, the same for a changed JQL filter: the filter is the scope.
    #[tokio::test]
    async fn a_changed_jql_filter_walks_the_new_filter_in_full() {
        let api = FakeApi {
            issues: five_issues(),
            ..FakeApi::default()
        };
        let (_, cursor) = keys_of(&api, &cfg(serde_json::json!({ "jql_filter": "a" })), None).await;
        let (again, _) = keys_of(
            &api,
            &cfg(serde_json::json!({ "jql_filter": "b" })),
            Some(cursor),
        )
        .await;
        assert_eq!(again.len(), 5, "{again:?}");
    }

    /// #2181: an edit that leaves the scope alone keeps the incremental
    /// position -- the rate, the page size and the *Boards and filters*
    /// setting are not the ticket scope.
    #[tokio::test]
    async fn an_edit_outside_the_scope_keeps_the_position() {
        let api = FakeApi {
            issues: five_issues(),
            ..FakeApi::default()
        };
        let (_, cursor) = keys_of(&api, &cfg(serde_json::json!({ "projects": ["PAY"] })), None).await;
        let edited = cfg(serde_json::json!({
            "projects": ["PAY"], "rate_per_sec": 3, "page_size": 50, "favourite_filters": false
        }));
        let (idle, _) = keys_of(&api, &edited, Some(cursor)).await;
        assert!(idle.is_empty(), "{idle:?}");
    }

    /// #2181, ruled: narrowing the scope walks what is left in full and says
    /// nothing of what was taken out. The removed project's tickets stay in
    /// the mirror as last seen -- this run emits no tombstone for them, and
    /// the engine sweeps only a cursor-less run (ADR-0003).
    #[tokio::test]
    async fn a_project_taken_out_of_the_scope_is_neither_read_nor_tombstoned() {
        let api = FakeApi {
            issues: five_issues(),
            ..FakeApi::default()
        };
        let both = cfg(serde_json::json!({ "projects": ["PAY", "OPS"] }));
        let (_, cursor) = keys_of(&api, &both, None).await;
        let pay = cfg(serde_json::json!({ "projects": ["PAY"] }));
        let mut sink = VecSink(Vec::new());
        let after = run(&api, &pay, Some(cursor), &mut sink).await.unwrap();
        let keys: Vec<(&str, bool)> = sink
            .0
            .iter()
            .map(|i| (i.entity.key.as_str(), i.deleted))
            .collect();
        assert!(
            !keys.iter().any(|(k, _)| k.starts_with("OPS-")),
            "{keys:?}"
        );
        assert!(
            keys.contains(&("PAY-219", false)),
            "the rest is walked in full: {keys:?}"
        );
        let (idle, _) = keys_of(&api, &pay, Some(after)).await;
        assert!(idle.is_empty(), "{idle:?}");
    }

    /// Each page after the first is asked from the last issue read, one row
    /// early, and one row longer (`crate::walk`): `@1` answers PAY-236 again
    /// and two new ones, `@3` PAY-228 again and the last.
    #[tokio::test]
    async fn pages_with_start_at_until_the_total_is_reached() {
        let api = FakeApi {
            issues: five_issues(),
            ..FakeApi::default()
        };
        let mut sink = VecSink(Vec::new());
        run(
            &api,
            &cfg(serde_json::json!({ "page_size": 2 })),
            None,
            &mut sink,
        )
        .await
        .unwrap();
        assert_eq!(sink.0.len(), 5);
        assert_eq!(api.searches(), vec!["search@0", "search@1", "search@3"]);
        // Same corpus, one page: identical set of keys.
        let mut one_page = VecSink(Vec::new());
        run(&api, &cfg(serde_json::json!({})), None, &mut one_page)
            .await
            .unwrap();
        let keys = |s: &VecSink| {
            let mut k: Vec<String> = s.0.iter().map(|i| i.entity.key.clone()).collect();
            k.sort();
            k
        };
        assert_eq!(keys(&sink), keys(&one_page));
    }

    /// The `ticket` kind claims `full_sync_exhaustive: true`, and the engine
    /// tombstones every row of such a kind that a full sync did not return. A
    /// run that stopped after the first page would therefore not merely sync
    /// less -- it would delete the rest of the corpus from the mirror. So the
    /// claim's paging
    /// half is pinned separately from the "which requests went out" assertion
    /// above: *every* key arrives, whatever the page size.
    #[tokio::test]
    async fn a_full_sync_walks_the_whole_total_not_just_the_first_page() {
        let api = FakeApi {
            issues: five_issues(),
            ..FakeApi::default()
        };
        for page_size in [1, 2, 3, 4, 5, 100] {
            let (mut keys, _) = keys_of(
                &api,
                &cfg(serde_json::json!({ "page_size": page_size })),
                None,
            )
            .await;
            keys.sort();
            assert_eq!(
                keys,
                vec!["OPS-77", "PAY-219", "PAY-228", "PAY-231", "PAY-236"],
                "page_size {page_size} did not deliver the whole corpus"
            );
        }
    }

    /// `total` is a live number and can shrink under the run. An empty page
    /// must end the loop, or the adapter spins forever against a server that
    /// keeps saying "there are more".
    #[tokio::test]
    async fn an_empty_page_ends_the_run() {
        let api = FakeApi {
            issues: Vec::new(),
            ..FakeApi::default()
        };
        let mut sink = VecSink(Vec::new());
        let cursor = run(&api, &cfg(serde_json::json!({})), None, &mut sink)
            .await
            .unwrap();
        assert!(sink.0.is_empty());
        assert_eq!(api.count("search@"), 1);
        // A full sync that found nothing still hands back a readable cursor.
        let parsed = crate::cursor::JiraCursor::parse(&cursor).unwrap();
        assert_eq!(parsed.updated_to, None);
    }

    /// `/search` truncates the comment container; the container says so, and
    /// the dedicated endpoint has the rest.
    #[tokio::test]
    async fn completes_a_truncated_comment_container() {
        let api = FakeApi {
            issues: vec![issue("PAY-231", "2026-08-22T11:48:00.000+0000", 1, 3)],
            comments: HashMap::from([(
                "PAY-231".to_owned(),
                (0..3)
                    .map(|i| {
                        serde_json::json!({ "id": format!("c{i}"), "body": format!("full {i}") })
                    })
                    .collect(),
            )]),
            ..FakeApi::default()
        };
        let mut sink = VecSink(Vec::new());
        run(&api, &cfg(serde_json::json!({})), None, &mut sink)
            .await
            .unwrap();
        let item = &sink.0[0];
        assert_eq!(
            item.payload["fields"]["comment"]["comments"]
                .as_array()
                .unwrap()
                .len(),
            3
        );
        assert!(item.body_text.contains("full 2"), "{}", item.body_text);
        assert_eq!(api.count("comments:PAY-231"), 1);
    }

    /// The comment endpoint pages like `/search` does, and a discussion longer
    /// than one page is exactly the case the completion exists for. Stopping
    /// after the first page would put a *shorter* comment list in `payload`
    /// than the container's own `total` claims -- and the next run would
    /// re-fetch the issue forever, because the container would still look
    /// truncated.
    #[tokio::test]
    async fn completion_pages_the_comment_endpoint_too() {
        let api = FakeApi {
            issues: vec![issue("PAY-231", "2026-08-22T11:48:00.000+0000", 0, 5)],
            comments: HashMap::from([(
                "PAY-231".to_owned(),
                (0..5)
                    .map(|i| {
                        serde_json::json!({ "id": format!("c{i}"), "body": format!("full {i}") })
                    })
                    .collect(),
            )]),
            ..FakeApi::default()
        };
        let mut sink = VecSink(Vec::new());
        run(
            &api,
            &cfg(serde_json::json!({ "page_size": 2 })),
            None,
            &mut sink,
        )
        .await
        .unwrap();
        assert_eq!(
            api.calls()
                .iter()
                .filter(|c| c.starts_with("comments:"))
                .collect::<Vec<_>>(),
            vec![
                "comments:PAY-231@0",
                "comments:PAY-231@2",
                "comments:PAY-231@4"
            ]
        );
        let item = &sink.0[0];
        assert_eq!(
            item.payload["fields"]["comment"]["comments"]
                .as_array()
                .unwrap()
                .len(),
            5
        );
        assert!(item.body_text.contains("full 4"), "{}", item.body_text);
    }

    /// An issue whose comments all fit costs no extra request. This is the
    /// difference between one call per run and one call per issue.
    #[tokio::test]
    async fn leaves_a_complete_container_alone() {
        let api = FakeApi {
            issues: vec![issue("PAY-231", "2026-08-22T11:48:00.000+0000", 2, 2)],
            ..FakeApi::default()
        };
        let mut sink = VecSink(Vec::new());
        run(&api, &cfg(serde_json::json!({})), None, &mut sink)
            .await
            .unwrap();
        assert_eq!(api.count("comments:"), 0);
        assert!(sink.0[0].body_text.contains("inline 1"));
    }

    /// A server that ignored `fields=comment` returns no container at all --
    /// the adapter must not read that as "no comments".
    #[tokio::test]
    async fn fetches_comments_when_the_container_is_absent() {
        let api = FakeApi {
            issues: vec![serde_json::json!({
                "key": "PAY-231",
                "fields": { "summary": "s", "updated": "2026-08-22T11:48:00.000+0000" }
            })],
            comments: HashMap::from([(
                "PAY-231".to_owned(),
                vec![serde_json::json!({ "id": "c0", "body": "fetched" })],
            )]),
            worklogs: HashMap::from([(
                "PAY-231".to_owned(),
                vec![serde_json::json!({ "id": "w0", "timeSpentSeconds": 900 })],
            )]),
            ..FakeApi::default()
        };
        let mut sink = VecSink(Vec::new());
        run(&api, &cfg(serde_json::json!({})), None, &mut sink)
            .await
            .unwrap();
        assert_eq!(api.count("comments:PAY-231"), 1);
        assert_eq!(api.count("worklogs:PAY-231"), 1);
        assert!(sink.0[0].body_text.contains("fetched"));
        assert_eq!(
            sink.0[0].payload["fields"]["worklog"]["worklogs"][0]["id"],
            "w0"
        );
    }

    /// `n` issues on one page whose worklog containers inline 20 of 25, the
    /// Data Center cut (#2167); each key's read takes longer the earlier it
    /// is in the walk, so reads finish in the reverse of walk order.
    fn truncated_worklogs(n: usize) -> FakeApi {
        let keys: Vec<String> = (0..n).map(|i| format!("PAY-{}", 300 + i)).collect();
        let worklog = |key: &str, i: usize| serde_json::json!({ "id": format!("{key}-w{i}") });
        FakeApi {
            issues: keys
                .iter()
                .enumerate()
                .map(|(i, key)| {
                    let mut raw = issue(key, &format!("2026-08-22T11:{i:02}:00.000+0000"), 0, 0);
                    raw["fields"]["worklog"] = serde_json::json!({
                        "startAt": 0, "maxResults": 20, "total": 25,
                        "worklogs": (0..20).map(|w| worklog(key, w)).collect::<Vec<_>>()
                    });
                    raw
                })
                .collect(),
            worklogs: keys
                .iter()
                .map(|key| (key.clone(), (0..25).map(|w| worklog(key, w)).collect()))
                .collect(),
            worklog_ms: keys
                .iter()
                .enumerate()
                .map(|(i, key)| (key.clone(), 2 * (n - i) as u64))
                .collect(),
            ..FakeApi::default()
        }
    }

    /// A page's truncated worklog containers are completed concurrently,
    /// never more than eight at a time, and its items still reach the sink
    /// whole and in walk order (#2167). One at a time, 200 such tickets took
    /// a 1500-ticket run from 8.1 s to 45.2 s (#2163).
    #[tokio::test]
    async fn a_pages_truncated_worklogs_are_completed_eight_at_a_time_in_walk_order() {
        let api = truncated_worklogs(12);
        let walk_order: Vec<String> = api
            .issues
            .iter()
            .map(|i| i["key"].as_str().unwrap().to_owned())
            .collect();
        let mut sink = VecSink(Vec::new());
        run(&api, &cfg(serde_json::json!({})), None, &mut sink)
            .await
            .unwrap();

        let most = api.most_worklogs_in_flight.load(Ordering::SeqCst);
        assert!(most > 1, "the worklog reads never overlapped");
        assert!(most <= 8, "{most} worklog reads were in flight at once");
        assert_eq!(api.count("worklogs:"), 12);
        let keys: Vec<&str> = sink.0.iter().map(|i| i.entity.key.as_str()).collect();
        assert_eq!(keys, walk_order);
        for item in &sink.0 {
            let worklog = &item.payload["fields"]["worklog"];
            assert_eq!(
                worklog["worklogs"].as_array().unwrap().len(),
                25,
                "{}",
                item.entity.key
            );
            assert_eq!(
                worklog["worklogs"][24]["id"],
                format!("{}-w24", item.entity.key)
            );
        }
    }

    /// One worklog completion that fails fails the run with its own error,
    /// and nothing after it on the page reaches the sink (#2167).
    #[tokio::test]
    async fn a_failing_worklog_completion_fails_the_run_and_stops_the_page() {
        let mut api = truncated_worklogs(10);
        api.failing_worklogs = Some("PAY-303");
        let mut sink = VecSink(Vec::new());
        let err = run(&api, &cfg(serde_json::json!({})), None, &mut sink)
            .await
            .expect_err("a failed completion fails the run");
        assert!(
            matches!(&err, SourceError::Unreachable(m) if m == "PAY-303's worklogs timed out"),
            "{err:?}"
        );
        let after: Vec<&str> = sink
            .0
            .iter()
            .map(|i| i.entity.key.as_str())
            .filter(|key| *key >= "PAY-303")
            .collect();
        assert!(after.is_empty(), "{after:?} reached the sink");
    }

    /// Battery clause 2, and the reason the cursor carries a `seen` set: the
    /// two-minute overlap re-fetches, and the run must still emit nothing and
    /// return the *same bytes*.
    ///
    /// Necessary and **not** sufficient on its own -- a full sync skips
    /// nothing, so this sequence is stable even with the `seen_in_window`
    /// defect. See
    /// [`one_new_issue_then_an_idle_poll_settles`](fn@one_new_issue_then_an_idle_poll_settles).
    #[tokio::test]
    async fn an_idle_incremental_emits_nothing_and_returns_the_same_cursor() {
        let api = FakeApi {
            issues: five_issues(),
            ..FakeApi::default()
        };
        let config = cfg(serde_json::json!({}));
        let mut first = VecSink(Vec::new());
        let cursor = run(&api, &config, None, &mut first).await.unwrap();
        assert_eq!(first.0.len(), 5);

        let mut second = VecSink(Vec::new());
        let again = run(&api, &config, Some(cursor.clone()), &mut second)
            .await
            .unwrap();
        assert!(second.0.is_empty(), "{:?}", second.0);
        assert_eq!(
            again, cursor,
            "an idle run must hand back the cursor it was given"
        );
    }

    /// The sequence that actually certifies cursor discipline: **one new issue
    /// and then an idle poll**, twice over.
    ///
    /// Full-sync-then-idle passes even when the run hands
    /// [`JiraCursor::advanced`](crate::cursor::JiraCursor::advanced) only what
    /// it *emitted*, because a full sync skips nothing and `seen` therefore
    /// comes out the same either way. This one does not: run 2 skips PAY-231
    /// as already delivered, and if that pair drops out of `seen`, run 3 --
    /// whose window still reaches back over it -- emits it again. The watermark
    /// holds perfectly the whole time, so nothing watching the cursor's
    /// timestamp can see the failure.
    ///
    /// The edits are one minute apart on purpose: the overlap is two minutes,
    /// so each run's window still contains the pair the previous run skipped.
    #[tokio::test]
    async fn one_new_issue_then_an_idle_poll_settles() {
        let mut api = FakeApi {
            issues: five_issues(),
            ..FakeApi::default()
        };
        let config = cfg(serde_json::json!({}));

        let (full, c1) = keys_of(&api, &config, None).await;
        assert_eq!(full.len(), 5);

        // One issue edited: it is the only thing that comes back, and PAY-231
        // -- still inside the two-minute window -- is recognised, not re-sent.
        api.edit("PAY-236", "2026-08-22T11:49:00.000+0000");
        let (second, c2) = keys_of(&api, &config, Some(c1)).await;
        assert_eq!(second, vec!["PAY-236".to_owned()]);

        // A second edit, one minute on. PAY-231 *and* PAY-236 are both still in
        // the window; both must be recognised. This is the assertion that goes
        // red when `advanced` is fed only the emitted pairs.
        api.edit("OPS-77", "2026-08-22T11:50:00.000+0000");
        let (third, c3) = keys_of(&api, &config, Some(c2)).await;
        assert_eq!(
            third,
            vec!["OPS-77".to_owned()],
            "a pair skipped as already-delivered must stay in `seen`, or it comes back \
             unrecognised on the next run"
        );

        // And now nothing at all changed: the run is idle and says so.
        let (idle, c4) = keys_of(&api, &config, Some(c3.clone())).await;
        assert!(idle.is_empty(), "{idle:?}");
        assert_eq!(
            c4, c3,
            "an idle poll must hand back the cursor it was given"
        );
    }

    /// The trap the `serverInfo` call exists for, at the layer that sends the
    /// query.
    ///
    /// JQL date literals carry no zone and are read in the **server's**. A
    /// watermark rendered in UTC and sent to a `+02:00` server therefore names
    /// an instant two hours earlier, and the query returns two extra hours of
    /// issues on every incremental run. That is invisible in the emitted items
    /// as long as everything in the extra band is still in `seen` -- so the
    /// issue here sits *outside* the two-minute overlap the cursor remembers
    /// and inside the two-hour band the mistake would open. Under the correct
    /// rendering the idle run does not see it at all; under a UTC rendering it
    /// comes back unrecognised and is emitted.
    ///
    /// `time.rs` pins the renderer itself. This pins that the run hands it the
    /// zone the server reported rather than a zero.
    #[tokio::test]
    async fn the_watermark_literal_is_read_in_the_servers_zone() {
        let mut issues = five_issues();
        // 11:46Z is the floor an idle run really queries from; 09:46Z is the
        // floor a UTC-rendered literal would open on a +02:00 server.
        issues.push(issue("PAY-500", "2026-08-22T10:30:00.000+0000", 0, 0));
        let api = FakeApi {
            issues,
            ..FakeApi::default()
        };
        let config = cfg(serde_json::json!({}));
        let (full, cursor) = keys_of(&api, &config, None).await;
        assert_eq!(full.len(), 6);
        // Outside the overlap window, so the cursor does not remember it -- the
        // only thing keeping it out of the next run is the query's lower bound.
        let seen: Vec<String> = crate::cursor::JiraCursor::parse(&cursor)
            .unwrap()
            .seen
            .into_iter()
            .map(|s| s.k)
            .collect();
        assert!(!seen.contains(&"PAY-500".to_owned()), "{seen:?}");

        let (idle, again) = keys_of(&api, &config, Some(cursor.clone())).await;
        assert!(
            idle.is_empty(),
            "the query floor reached back further than the server's zone puts it: {idle:?}"
        );
        assert_eq!(again, cursor);
    }

    /// `total` is a live number: an issue can leave the result set while the
    /// run is paging it, and a server can simply be wrong. The loop stops on
    /// the page that brings nothing past where the walk stood rather than on
    /// the arithmetic, because `startAt` does not advance over rows that were
    /// not returned -- so a run trusting `total` alone re-asks for the same
    /// offset forever.
    #[tokio::test]
    async fn a_page_with_nothing_past_the_walk_ends_it_while_total_still_claims_more() {
        let api = FakeApi {
            issues: five_issues(),
            phantom_total: 40,
            ..FakeApi::default()
        };
        let (keys, _) = keys_of(&api, &cfg(serde_json::json!({})), None).await;
        assert_eq!(keys.len(), 5);
        // One page with the five issues, then one asked from the last of them
        // that holds it alone and ends the walk.
        assert_eq!(api.searches(), vec!["search@0", "search@4"]);
    }

    /// Battery clause 2 says an idle run returns the cursor it was handed
    /// **byte-identically**, and the run keeps that promise by handing back the
    /// input string rather than re-encoding the struct. Re-encoding looks
    /// equivalent and is not: the envelope carries the zone its watermark was
    /// taken in, so the first idle poll after the server crossed a daylight
    /// saving boundary would come back with a different `tz_offset_secs` and a
    /// window widened to 65 minutes -- a changed cursor for a run in which
    /// nothing happened, which is exactly the activity line the clause exists
    /// to suppress.
    #[tokio::test]
    async fn an_idle_run_after_the_server_changed_zone_returns_the_same_bytes() {
        let mut api = FakeApi {
            issues: five_issues(),
            offset_secs: 2 * 3600,
            ..FakeApi::default()
        };
        let config = cfg(serde_json::json!({}));
        let (full, cursor) = keys_of(&api, &config, None).await;
        assert_eq!(full.len(), 5);

        // Winter time: the server is now an hour west of where the cursor was
        // written, so the next run widens its overlap. Nothing changed upstream.
        api.offset_secs = 3600;
        let (idle, again) = keys_of(&api, &config, Some(cursor.clone())).await;
        assert!(idle.is_empty(), "{idle:?}");
        assert_eq!(
            again, cursor,
            "a run that emitted nothing must return the bytes it was handed, zone change included"
        );
    }

    /// A walk that hits the page cap **fails**; it never comes back as a
    /// completed sync.
    ///
    /// This is the one that matters most, and it is the one mockd structurally
    /// cannot see: the fixture is seven issues, so the cap is a real-server-only
    /// failure mode. The `ticket` kind claims `full_sync_exhaustive: true`,
    /// which is the engine's licence to tombstone every row of that kind a
    /// `cursor: None` run did not return. An `Ok` here is therefore not "we
    /// synced a bit less" -- it is authorisation to delete everything past the
    /// cap. And because
    /// the query is `ORDER BY created ASC`, what falls off the end is the
    /// newest work, so the rows offered up for deletion are exactly the ones
    /// someone is looking at today.
    ///
    /// The assertion is the *outcome*, not the emission: items do reach the
    /// sink before the cap, and that is fine. What must not happen is the run
    /// telling the engine it saw the whole corpus.
    #[tokio::test]
    async fn a_walk_that_hits_the_page_cap_fails_instead_of_reporting_a_completed_sync() {
        let api = FakeApi {
            endless: true,
            ..FakeApi::default()
        };
        let mut sink = VecSink(Vec::new());
        let got = run(&api, &cfg(serde_json::json!({})), None, &mut sink)
            .await
            .unwrap_err();
        assert!(
            matches!(&got, SourceError::Protocol { message: m, .. } if m.contains("partial walk")),
            "{got:?}"
        );
        // The cap fired where it is meant to, rather than the run ending for
        // some other reason that happens to look the same.
        assert_eq!(api.count("search@"), MAX_PAGES as usize);
        assert!(
            !sink.0.is_empty(),
            "the items before the cap are still delivered; it is the verdict that changes"
        );
    }

    /// An incremental run gets the same refusal.
    ///
    /// Tempting to let it through: no sweep follows an incremental and the
    /// items were delivered. (Under `ORDER BY created ASC` the watermark would
    /// not even mark where the walk stopped: the newest `updated` emitted can
    /// sit past issues it never reached.) That narrower fix is worse than it
    /// looks, for a reason the mutation check turned up rather than the
    /// argument: with
    /// `&& cursor.is_none()` on the cap, **this test does not terminate**. The
    /// loop's only other exit is the records read reaching `total`, and `total` is the number
    /// the server is inflating -- so an incremental against a mispaging server
    /// has no stop at all. (Measured: no completion in 180 s, against 1.7 s for
    /// the whole lib suite.)
    ///
    /// The refusal is therefore unconditional, and this pins it rather than
    /// leaving it to a comment.
    #[tokio::test]
    async fn an_incremental_run_that_hits_the_page_cap_fails_too() {
        let settled = FakeApi {
            issues: five_issues(),
            ..FakeApi::default()
        };
        let (_, cursor) = keys_of(&settled, &cfg(serde_json::json!({})), None).await;

        let api = FakeApi {
            endless: true,
            ..FakeApi::default()
        };
        let mut sink = VecSink(Vec::new());
        let got = run(&api, &cfg(serde_json::json!({})), Some(cursor), &mut sink)
            .await
            .unwrap_err();
        assert!(
            matches!(&got, SourceError::Protocol { message: m, .. } if m.contains("partial walk")),
            "{got:?}"
        );
    }

    /// The widening is for the run *after* a zone change and no other. A run
    /// whose zone did not move reaches back exactly the two-minute overlap.
    ///
    /// `seen` is filtered to that same two minutes, so anything the window
    /// reaches beyond it comes back unrecognised and is emitted -- on every
    /// poll, forever. A permanently widened window is therefore not a harmless
    /// over-fetch: it is battery clause 2 failing in steady state, for any
    /// source with an edit between three and sixty-five minutes old.
    #[tokio::test]
    async fn an_idle_poll_does_not_reach_back_further_than_the_overlap() {
        let mut issues = five_issues();
        // Inside the daylight-saving widening, outside the overlap -- so the
        // cursor does not remember it and only an over-wide floor returns it.
        issues.push(issue("PAY-502", "2026-08-22T11:00:00.000+0000", 0, 0));
        let api = FakeApi {
            issues,
            ..FakeApi::default()
        };
        let config = cfg(serde_json::json!({}));
        let (full, cursor) = keys_of(&api, &config, None).await;
        assert_eq!(full.len(), 6);
        let (idle, again) = keys_of(&api, &config, Some(cursor.clone())).await;
        assert!(
            idle.is_empty(),
            "an idle poll reached past the two-minute overlap: {idle:?}"
        );
        assert_eq!(again, cursor);
    }

    /// The one run after the server's UTC offset changed reaches back an hour
    /// further than the overlap, and it is the **current** offset that decides
    /// that -- comparing the cursor's zone against itself would make the
    /// widening unreachable.
    ///
    /// `cursor.rs` pins that [`JiraCursor::since`] widens; this pins that the
    /// run gives it something to compare against. The issue in the seam is
    /// outside the two-minute overlap and therefore not in `seen`, so the only
    /// thing that can bring it back is the wider floor -- and the watermark
    /// must not follow it backwards.
    #[tokio::test]
    async fn the_run_after_a_zone_change_widens_its_window_once() {
        let mut issues = five_issues();
        // 48 minutes before the watermark: inside the 65-minute widening,
        // outside the 2-minute overlap.
        issues.push(issue("PAY-501", "2026-08-22T11:00:00.000+0000", 0, 0));
        let mut api = FakeApi {
            issues,
            offset_secs: 2 * 3600,
            ..FakeApi::default()
        };
        let config = cfg(serde_json::json!({}));
        let (_, cursor) = keys_of(&api, &config, None).await;

        // Same corpus, unchanged; only the server's zone moved.
        api.offset_secs = 3600;
        let (widened, advanced) = keys_of(&api, &config, Some(cursor)).await;
        assert_eq!(
            widened,
            vec!["PAY-501".to_owned()],
            "the run after a zone change must re-read the daylight-saving seam"
        );
        assert_eq!(
            crate::cursor::JiraCursor::parse(&advanced)
                .unwrap()
                .updated_to,
            crate::time::parse_jira_time("2026-08-22T11:48:00.000+0000"),
            "re-reading the seam must not drag the watermark back into it"
        );
    }

    /// The other half: something really changed, and exactly that comes back.
    #[tokio::test]
    async fn a_changed_issue_is_the_only_thing_the_next_run_emits() {
        let mut api = FakeApi {
            issues: five_issues(),
            ..FakeApi::default()
        };
        let config = cfg(serde_json::json!({}));
        let mut first = VecSink(Vec::new());
        let cursor = run(&api, &config, None, &mut first).await.unwrap();

        // PAY-228 edited after the watermark.
        api.edit("PAY-228", "2026-08-22T12:30:00.000+0000");
        let mut second = VecSink(Vec::new());
        let advanced = run(&api, &config, Some(cursor.clone()), &mut second)
            .await
            .unwrap();
        let keys: Vec<&str> = second.0.iter().map(|i| i.entity.key.as_str()).collect();
        assert_eq!(keys, vec!["PAY-228"]);
        assert_ne!(advanced, cursor);

        // …and the watermark is the newest `updated` delivered, not `now()`.
        let parsed = crate::cursor::JiraCursor::parse(&advanced).unwrap();
        assert_eq!(
            parsed.updated_to,
            crate::time::parse_jira_time("2026-08-22T12:30:00.000+0000")
        );
    }

    /// **Two changes inside one second are two versions, and both are
    /// delivered** (issue #345).
    ///
    /// This is the fix's own test, and its fixture is the sequence a real
    /// installation runs constantly: something changes, a sync records the
    /// issue, and something else changes it again before that second is out.
    /// `knobas_app::sources::write_queue::after_landing` runs a sync after every
    /// landed write, so *comment through knobas, then reassign in Jira* is
    /// exactly this shape -- measured on Jira DC 10.3.24 with 88 ms between the
    /// two.
    ///
    /// **The stamp does not move between the two changes, and that is the
    /// point.** `/search` reports `updated` to the second while an issue
    /// changes in milliseconds, so a run cannot see a difference in the
    /// timestamp even though there is one. While `seen` was keyed on
    /// `(key, updated)` the second change was therefore dropped before the
    /// sink -- and dropped **for ever**, because `updated` never moves again on
    /// its own and no later incremental run reaches it. Only
    /// `knobas_sync::run_backfill` recovered it, and nothing knew to run one.
    ///
    /// What it cost downstream is why this is not a cosmetic miss: `author` is
    /// the assignee on Jira (`crate::map`), and `sync.live_item.author` is what
    /// the standup digest's mirror half, the inbox's author matching (#82) and
    /// every `@me` filter key on.
    ///
    /// The last poll is here so the assertion cannot be satisfied by an adapter
    /// that simply stopped recognising anything: once the change is delivered
    /// the source must go **quiet** again, which is battery clause 2.
    #[tokio::test]
    async fn two_changes_inside_one_second_are_two_versions_and_both_are_delivered() {
        let mut issues = five_issues();
        // Its own stamp is the newest in the fixture, so it sets the watermark
        // and the whole question is `seen`, never the query's lower bound.
        issues[4] = assigned("PAY-231", "2026-08-22T11:48:00.000+0000", "mara.lindqvist");
        let mut api = FakeApi {
            issues,
            ..FakeApi::default()
        };
        let config = cfg(serde_json::json!({}));

        let mut first = VecSink(Vec::new());
        let cursor = run(&api, &config, None, &mut first).await.unwrap();
        assert_eq!(
            first
                .0
                .iter()
                .find(|i| i.entity.key == "PAY-231")
                .map(|i| i.author.clone()),
            Some(Some("mara.lindqvist".to_owned())),
            "the run that records the issue is what puts the old assignee in the mirror"
        );

        // The second change, inside the second that run recorded: the assignee
        // moves and `updated` -- as `/search` renders it -- does not.
        api.reassign("PAY-231", "knobas");

        let mut second = VecSink(Vec::new());
        let advanced = run(&api, &config, Some(cursor.clone()), &mut second)
            .await
            .unwrap();
        assert_eq!(
            second
                .0
                .iter()
                .map(|i| i.entity.key.as_str())
                .collect::<Vec<_>>(),
            vec!["PAY-231"],
            "the reassigned issue is delivered although its `updated` is unchanged"
        );
        assert_eq!(
            second.0[0].author.as_deref(),
            Some("knobas"),
            "and it carries the new assignee, which is what every `@me` reader needs"
        );
        assert_ne!(advanced, cursor, "the cursor records the new record");

        // ...and the source settles: nothing changed since, so nothing is
        // emitted and the position is handed back byte-identically.
        let mut third = VecSink(Vec::new());
        let settled = run(&api, &config, Some(advanced.clone()), &mut third)
            .await
            .unwrap();
        assert!(
            third.0.is_empty(),
            "an idle poll after the delivery must emit nothing: {:?}",
            third.0.iter().map(|i| &i.entity.key).collect::<Vec<_>>()
        );
        assert_eq!(settled, advanced, "byte-identical");
    }

    /// **An unchanged record inside the overlap window is still skipped**
    /// (battery clause 2, and the half issue #345's fix must not break).
    ///
    /// The overlap re-delivers the last two minutes on every run by design, and
    /// `seen` is the only thing keeping that from meaning "every poll emits the
    /// recent corpus". Giving the identity a fingerprint could have broken it in one
    /// specific way -- a field that differed between two reads of an unchanged
    /// issue would make every poll re-emit everything -- so the property is
    /// asserted here rather than left to follow from its sibling.
    ///
    /// **The window is open, not closed**, which is what makes this a test: the
    /// query *does* return PAY-231 and the run chooses to skip it. The witness
    /// is the cursor's own `seen`, which is built from what the pages returned,
    /// so PAY-231 appearing in it is proof the query answered with it.
    ///
    /// Offline this can only pin the rule. Whether a real Jira's `/search`
    /// answers an untouched issue byte-identically twice is a question for the
    /// server, and `tests/live_jira_seeded.rs`'s repeated idle poll is where it
    /// is asked (ADR-0013).
    #[tokio::test]
    async fn an_unchanged_record_inside_the_overlap_window_is_still_skipped() {
        let mut issues = five_issues();
        issues[4] = assigned("PAY-231", "2026-08-22T11:48:00.000+0000", "mara.lindqvist");
        let mut api = FakeApi {
            issues,
            ..FakeApi::default()
        };
        let config = cfg(serde_json::json!({}));
        let cursor = keys_of(&api, &config, None).await.1;

        // Something else inside the window, so the run is not idle and the skip
        // below is a decision rather than an absence of work.
        api.edit("PAY-228", "2026-08-22T11:49:00.000+0000");

        let (keys, advanced) = keys_of(&api, &config, Some(cursor)).await;
        assert_eq!(
            keys,
            vec!["PAY-228"],
            "PAY-231 reached the run untouched and was skipped before the sink"
        );
        let parsed = crate::cursor::JiraCursor::parse(&advanced).unwrap();
        assert!(
            parsed.seen.iter().any(|s| s.k == "PAY-231"
                && Some(s.u) == crate::time::parse_jira_time("2026-08-22T11:48:00.000+0000")),
            "`seen` records what the window contained, so PAY-231 in it is proof the query \
             answered with it: {:?}",
            parsed.seen
        );
    }

    /// **The first run after the upgrade that dropped the changelog re-delivers
    /// the overlap window once, and nothing more** (#2166, spec #2163).
    ///
    /// A cursor recorded before #2165 holds fingerprints of records that
    /// carried `changelog`; the same tickets now arrive without it. The ones
    /// inside the overlap window fingerprint differently and are delivered
    /// again, once. The ones outside it are never asked for, so the run is an
    /// incremental one, and the poll after it is idle and hands the cursor
    /// back byte-identically. The version a cursor from before the upgrade
    /// carries is still read: `cursor::tests::a_v4_envelope_without_the_board_read_stamp_decodes`
    /// pins `CURSOR_VERSION` at 4.
    #[tokio::test]
    async fn the_run_after_the_changelog_left_redelivers_only_its_overlap_window() {
        let mut issues = five_issues();
        issues.push(issue("PAY-232", "2026-08-22T11:47:30.000+0000", 0, 0));
        for raw in &mut issues {
            raw["changelog"] = serde_json::json!({
                "startAt": 0, "total": 1,
                "histories": [{"id": "1", "items": [{"field": "status", "toString": "Done"}]}]
            });
        }
        let mut api = FakeApi {
            issues,
            ..FakeApi::default()
        };
        let config = cfg(serde_json::json!({}));
        let (full, before) = keys_of(&api, &config, None).await;
        assert_eq!(full.len(), 6, "the run before the upgrade delivers all six");

        for raw in &mut api.issues {
            raw.as_object_mut().unwrap().remove("changelog");
        }
        let (mut keys, after) = keys_of(&api, &config, Some(before)).await;
        keys.sort();
        assert_eq!(
            keys,
            vec!["PAY-231", "PAY-232"],
            "only the two tickets inside the two-minute overlap come back"
        );

        let (idle, settled) = keys_of(&api, &config, Some(after.clone())).await;
        assert!(idle.is_empty(), "the next poll is idle: {idle:?}");
        assert_eq!(settled, after, "byte-identical");
    }

    /// The watermark is `max(previous, newest emitted)`, never just "newest
    /// emitted".
    ///
    /// An item can enter the window carrying an `updated` *older* than the
    /// current watermark -- an issue whose project was added to the scope, or
    /// one the credential could not see until a permission changed. Taking the
    /// newest emitted alone drags the position backwards onto it, and the next
    /// query's window widens to everything since. That is recoverable exactly
    /// while `seen` can hold the whole widened window; past
    /// [`SEEN_CAP`](knobas_source::watermark::SEEN_CAP) it is re-delivery on every poll,
    /// forever.
    #[tokio::test]
    async fn the_watermark_never_moves_backwards() {
        let mut api = FakeApi {
            issues: five_issues(),
            ..FakeApi::default()
        };
        let config = cfg(serde_json::json!({}));
        let (_, cursor) = keys_of(&api, &config, None).await;
        let before = crate::cursor::JiraCursor::parse(&cursor)
            .unwrap()
            .updated_to;
        assert_eq!(
            before,
            crate::time::parse_jira_time("2026-08-22T11:48:00.000+0000")
        );

        // Newly visible, and older than the watermark -- but inside the window,
        // so the query returns it and the run has never seen the pair.
        api.issues
            .push(issue("PAY-999", "2026-08-22T11:47:00.000+0000", 0, 0));
        let (keys, advanced) = keys_of(&api, &config, Some(cursor)).await;
        assert_eq!(keys, vec!["PAY-999".to_owned()]);
        assert_eq!(
            crate::cursor::JiraCursor::parse(&advanced)
                .unwrap()
                .updated_to,
            before,
            "emitting an item older than the watermark must not move the watermark back to it"
        );
    }

    /// **A ceiling below the previous watermark does not drag it back**
    /// (#1622). PAY-231, which set the watermark at 11:48, is deleted, so the
    /// probe's newest is PAY-999 at 11:47, newly visible inside the window.
    /// Emitting it must leave the watermark at 11:48: clamping to the 11:47
    /// ceiling would move the position backwards, which only
    /// `Bounds::previous` prevents here.
    #[tokio::test]
    async fn a_ceiling_below_the_previous_watermark_does_not_move_it_back() {
        let mut api = FakeApi {
            issues: five_issues(),
            ..FakeApi::default()
        };
        let config = cfg(serde_json::json!({}));
        let (_, cursor) = keys_of(&api, &config, None).await;
        let before = crate::cursor::JiraCursor::parse(&cursor)
            .unwrap()
            .updated_to;

        api.issues.retain(|i| i["key"] != "PAY-231");
        api.issues
            .push(issue("PAY-999", "2026-08-22T11:47:00.000+0000", 0, 0));
        let (keys, advanced) = keys_of(&api, &config, Some(cursor)).await;
        assert_eq!(keys, vec!["PAY-999".to_owned()]);
        assert_eq!(
            crate::cursor::JiraCursor::parse(&advanced)
                .unwrap()
                .updated_to,
            before,
            "the 11:47 ceiling does not pull the 11:48 watermark back"
        );
    }

    /// **The ceiling** (#1622). While the walk pages, PAY-3 (not reached yet)
    /// is edited and arrives on the second page stamped 11:44:30, past the
    /// newest issue the probe saw at the run's start (11:20). PAY-1, on the
    /// page already walked, is edited at 11:30 in the same minutes.
    ///
    /// Advancing to 11:44:30 would put the next query's bound at 11:42 and
    /// lose PAY-1's edit for good. Held at the ceiling, the next run queries
    /// from 11:18, delivers PAY-1 and recognises PAY-3, which it already has.
    #[tokio::test]
    async fn an_issue_edited_during_the_walk_does_not_carry_the_watermark_past_the_ceiling() {
        let mut api = FakeApi {
            issues: vec![
                issue("PAY-1", "2026-08-22T11:00:00.000+0000", 0, 0),
                issue("PAY-2", "2026-08-22T11:10:00.000+0000", 0, 0),
                issue("PAY-3", "2026-08-22T11:20:00.000+0000", 0, 0),
            ],
            edited_mid_walk: vec![("PAY-3", "2026-08-22T11:44:30.000+0000")],
            ..FakeApi::default()
        };
        let config = cfg(serde_json::json!({ "page_size": 2 }));
        let (keys, cursor) = keys_of(&api, &config, None).await;
        assert_eq!(keys, vec!["PAY-1", "PAY-2", "PAY-3"]);
        assert_eq!(
            crate::cursor::JiraCursor::parse(&cursor)
                .unwrap()
                .updated_to,
            crate::time::parse_jira_time("2026-08-22T11:20:00.000+0000"),
            "the watermark is held at the ceiling the run witnessed at its start"
        );

        // The world after that run: both edits landed.
        api.edited_mid_walk.clear();
        api.edit("PAY-3", "2026-08-22T11:44:30.000+0000");
        api.edit("PAY-1", "2026-08-22T11:30:00.000+0000");
        let (keys, _) = keys_of(&api, &config, Some(cursor)).await;
        assert_eq!(
            keys,
            vec!["PAY-1"],
            "the edit behind the walk is delivered; the one it saw is recognised"
        );
    }

    /// What upstream does to the listing between two of the walk's pages.
    type Upstream = Box<dyn FnOnce(&mut Vec<serde_json::Value>) + Send>;

    /// A server whose listing **changes under the walk** (#1623). It sorts by
    /// the keys the JQL's `ORDER BY` names, as Jira does, and once it has
    /// served the walk `n` pages it applies `between[n]` before answering the
    /// next, so an edit or a deletion lands between two requests the way a
    /// colleague's does. Every call that is not `/search` goes to a plain
    /// [`FakeApi`].
    ///
    /// `issues` is kept in **index order**: rows the sort keys leave tied come
    /// back in it, and an edit moves the edited issue to its end. That is what
    /// the seeded Jira does (#1720, measured on 10.3.24): an edit reindexes the
    /// issue, a delete plus an add, and a group tied on `created` comes back in
    /// document order.
    struct LiveIndex {
        issues: Mutex<Vec<serde_json::Value>>,
        between: Mutex<HashMap<u32, Upstream>>,
        served: Mutex<u32>,
        /// The most rows one answer holds, whatever `maxResults` asked:
        /// Jira's own `jira.search.views.default.max`.
        clamp: u32,
        rest: FakeApi,
    }

    impl LiveIndex {
        fn new(
            issues: Vec<serde_json::Value>,
            between: impl IntoIterator<Item = (u32, Upstream)>,
        ) -> Self {
            Self {
                issues: Mutex::new(issues),
                between: Mutex::new(between.into_iter().collect()),
                served: Mutex::new(0),
                clamp: u32::MAX,
                rest: FakeApi::default(),
            }
        }

        /// This server, answering at most `clamp` rows a page.
        fn clamped(self, clamp: u32) -> Self {
            Self { clamp, ..self }
        }
    }

    fn stamp(issue: &serde_json::Value, field: &str) -> Option<chrono::DateTime<chrono::Utc>> {
        issue["fields"][field]
            .as_str()
            .and_then(crate::time::parse_jira_time)
    }

    /// Jira's numeric issue id, which `ORDER BY id` sorts on.
    fn id_of(issue: &serde_json::Value) -> u64 {
        issue["id"]
            .as_str()
            .and_then(|id| id.parse().ok())
            .expect("a filed issue has a numeric id")
    }

    /// [`issue`] with the `created` stamp an ordering over it reads, and an id
    /// that follows the key's number, as Jira's do for issues filed in turn.
    fn filed(key: &str, created: &str, updated: &str) -> serde_json::Value {
        let mut raw = issue(key, updated, 0, 0);
        raw["fields"]["created"] = serde_json::json!(created);
        let number: u64 = key
            .rsplit('-')
            .next()
            .and_then(|n| n.parse().ok())
            .expect("a key ends in its number");
        raw["id"] = serde_json::json!((10_000 + number).to_string());
        raw
    }

    /// PAY-1 to PAY-5, created a day apart and last updated ten minutes
    /// apart, in the same order: before anything changes, both orderings
    /// agree.
    fn five_filed() -> Vec<serde_json::Value> {
        (1..=5)
            .map(|n| {
                filed(
                    &format!("PAY-{n}"),
                    &format!("2026-08-0{n}T09:00:00.000+0000"),
                    &format!("2026-08-22T11:{}0:00.000+0000", n - 1),
                )
            })
            .collect()
    }

    /// An edit upstream: `key`'s `updated` moves to `updated`, and the issue,
    /// reindexed, moves to the end of the index order.
    fn edit_to(key: &'static str, updated: &'static str) -> Upstream {
        Box::new(move |issues| {
            let at = issues
                .iter()
                .position(|i| i["key"] == key)
                .unwrap_or_else(|| panic!("no {key} to edit"));
            let mut hit = issues.remove(at);
            hit["fields"]["updated"] = serde_json::json!(updated);
            issues.push(hit);
        })
    }

    /// A deletion upstream.
    fn delete(keys: &'static [&'static str]) -> Upstream {
        Box::new(move |issues| issues.retain(|i| !keys.iter().any(|k| i["key"] == *k)))
    }

    #[async_trait::async_trait]
    impl JiraApi for LiveIndex {
        async fn server_info(&self) -> Result<ServerInfo, SourceError> {
            self.rest.server_info().await
        }
        async fn myself(&self) -> Result<Myself, SourceError> {
            self.rest.myself().await
        }
        async fn fields(&self) -> Result<Vec<crate::model::FieldMeta>, SourceError> {
            self.rest.fields().await
        }
        async fn search(
            &self,
            jql: &str,
            start_at: u32,
            max_results: u32,
            _fields: &str,
        ) -> Result<SearchPage, SourceError> {
            let page = {
                let mut issues = self.issues.lock().unwrap();
                if jql.ends_with("ORDER BY updated DESC") {
                    let newest = issues.iter().max_by_key(|i| stamp(i, "updated")).cloned();
                    serde_json::json!({
                        "startAt": 0, "maxResults": max_results, "total": issues.len(),
                        "issues": newest.into_iter().collect::<Vec<_>>()
                    })
                } else {
                    let mut served = self.served.lock().unwrap();
                    if let Some(change) = self.between.lock().unwrap().remove(&served) {
                        change(&mut issues);
                    }
                    *served += 1;
                    let keys: Vec<&str> = jql
                        .rsplit_once("ORDER BY ")
                        .map_or("", |(_, order)| order)
                        .split(", ")
                        .map(|key| match key {
                            "created ASC" => "created",
                            "updated ASC" => "updated",
                            "id ASC" => "id",
                            _ => panic!("an order this server does not model: {jql}"),
                        })
                        .collect();
                    let since = since_of(jql, self.rest.offset_secs);
                    let mut matching: Vec<serde_json::Value> = issues
                        .iter()
                        .filter(|i| since.is_none_or(|s| stamp(i, "updated") >= Some(s)))
                        .cloned()
                        .collect();
                    // Stable, so what the keys leave tied stays in index order.
                    matching.sort_by(|a, b| {
                        keys.iter().fold(std::cmp::Ordering::Equal, |o, &key| {
                            o.then_with(|| match key {
                                "id" => id_of(a).cmp(&id_of(b)),
                                stamp_key => stamp(a, stamp_key).cmp(&stamp(b, stamp_key)),
                            })
                        })
                    });
                    let window: Vec<serde_json::Value> = matching
                        .iter()
                        .skip(start_at as usize)
                        .take(max_results.min(self.clamp) as usize)
                        .cloned()
                        .collect();
                    serde_json::json!({
                        "startAt": start_at, "maxResults": max_results.min(self.clamp),
                        "total": matching.len(), "issues": window
                    })
                }
            };
            Ok(serde_json::from_value(page).unwrap())
        }
        async fn comments(
            &self,
            key: &str,
            start_at: u32,
            max_results: u32,
        ) -> Result<CommentPage, SourceError> {
            self.rest.comments(key, start_at, max_results).await
        }
        async fn worklogs(&self, key: &str) -> Result<WorklogPage, SourceError> {
            self.rest.worklogs(key).await
        }
    }

    /// **An edit cannot move a row across a page boundary** (#1623, ADPT-1).
    /// Five issues, two to a page; between the first page and the second,
    /// PAY-1 -- on the page already read -- is edited. Over `updated` that
    /// edit moves PAY-1 to the end and shifts every row behind it up by one,
    /// so PAY-3, the first unread issue, lands on the page already read and no
    /// request returns it: `total` is unchanged, the walk ends `Ok`, and the
    /// engine's sweep tombstones a live ticket. Over `created` nothing moves.
    #[tokio::test]
    async fn an_issue_edited_behind_the_walk_does_not_shift_an_unread_one_past_it() {
        let api = LiveIndex::new(
            five_filed(),
            [(1, edit_to("PAY-1", "2026-08-22T11:50:00.000+0000"))],
        );
        let (keys, cursor) = keys_of(&api, &cfg(serde_json::json!({ "page_size": 2 })), None).await;
        assert_eq!(keys, vec!["PAY-1", "PAY-2", "PAY-3", "PAY-4", "PAY-5"]);
        assert_eq!(
            crate::cursor::JiraCursor::parse(&cursor)
                .unwrap()
                .updated_to,
            crate::time::parse_jira_time("2026-08-22T11:40:00.000+0000"),
            "held at the ceiling: PAY-1's edit came after the run started"
        );
    }

    /// **Why the order matters and not only the anchored pages** (#1979).
    /// Three issues; both read on the first page are edited after it. Over
    /// `updated` they move to the end, behind PAY-3, and the next page, asked
    /// from the last issue read, opens on PAY-1: an issue the walk has read,
    /// which is where an anchored page says nothing unread is before it. That
    /// is wrong only because an edit moved the row, which `created` rules out.
    #[tokio::test]
    async fn issues_read_and_edited_behind_the_walk_hide_no_unread_one() {
        let api = LiveIndex::new(
            five_filed().into_iter().take(3).collect(),
            [(
                1,
                Box::new(|issues: &mut Vec<serde_json::Value>| {
                    edit_to("PAY-1", "2026-08-22T11:50:00.000+0000")(issues);
                    edit_to("PAY-2", "2026-08-22T11:51:00.000+0000")(issues);
                }) as Upstream,
            )],
        );
        let (keys, _) = keys_of(&api, &cfg(serde_json::json!({ "page_size": 2 })), None).await;
        assert_eq!(keys, vec!["PAY-1", "PAY-2", "PAY-3"]);
    }

    /// **An edit cannot move a row out of a tie group either** (#1720). Five
    /// issues filed in one import share one `created` stamp, so `created`
    /// leaves the whole group to Jira's own tiebreak, the index order. Between
    /// the first page and the second PAY-1, on the page already read, is
    /// edited and reindexed to the end of the group; without a second sort key
    /// PAY-3 shifts onto the page already read and no request returns it, with
    /// `total` unchanged. `id` breaks the tie on a value nothing changes.
    #[tokio::test]
    async fn an_issue_edited_in_a_tie_group_does_not_shift_an_unread_one_past_it() {
        let imported = (1..=5)
            .map(|n| {
                filed(
                    &format!("PAY-{n}"),
                    "2026-08-01T09:00:00.000+0000",
                    &format!("2026-08-22T11:{}0:00.000+0000", n - 1),
                )
            })
            .collect();
        let api = LiveIndex::new(
            imported,
            [(1, edit_to("PAY-1", "2026-08-22T11:50:00.000+0000"))],
        );
        let (keys, _) = keys_of(&api, &cfg(serde_json::json!({ "page_size": 2 })), None).await;
        assert_eq!(keys, vec!["PAY-1", "PAY-2", "PAY-3", "PAY-4", "PAY-5"]);
    }

    /// **A deletion behind the walk hides nothing and fails nothing** (#1979,
    /// ruling A5 as amended). PAY-1 is deleted after the first page; offset 2
    /// of the four left starts at PAY-4, so plain offset paging steps over
    /// PAY-3, and A5's shrinking-total check failed the run for it. The walk
    /// asks from the last issue read, finds the page opens on PAY-3, which it
    /// has not read, and asks again from the start: every issue arrives once,
    /// and the run is `Ok`.
    #[tokio::test]
    async fn an_issue_deleted_behind_the_walk_hides_no_unread_one() {
        let api = LiveIndex::new(five_filed(), [(1, delete(&["PAY-1"]))]);
        let (keys, _) = keys_of(&api, &cfg(serde_json::json!({ "page_size": 2 })), None).await;
        assert_eq!(keys, vec!["PAY-1", "PAY-2", "PAY-3", "PAY-4", "PAY-5"]);
    }

    /// **Two deletions behind the walk** shift the rows by two, past the one
    /// row each request starts early by: offset 1 of the three left starts at
    /// PAY-4. The page does not open on an issue the walk has read, so it is
    /// asked again from the start, and PAY-3 is on it.
    #[tokio::test]
    async fn two_issues_deleted_behind_the_walk_hide_no_unread_one() {
        let api = LiveIndex::new(five_filed(), [(1, delete(&["PAY-1", "PAY-2"]))]);
        let (keys, _) = keys_of(&api, &cfg(serde_json::json!({ "page_size": 2 })), None).await;
        assert_eq!(keys, vec!["PAY-1", "PAY-2", "PAY-3", "PAY-4", "PAY-5"]);
    }

    /// The same when the deletion leaves the page the walk asks next empty:
    /// four issues read of five, two of them deleted, and offset 3 of the
    /// three left holds nothing. Empty is not the end here; the walk asks from
    /// further back until a page opens on an issue it has read, or on the
    /// first row, and PAY-5 is on it.
    #[tokio::test]
    async fn a_deletion_that_empties_the_next_page_hides_no_unread_one() {
        let api = LiveIndex::new(five_filed(), [(2, delete(&["PAY-1", "PAY-2"]))]);
        let (keys, _) = keys_of(&api, &cfg(serde_json::json!({ "page_size": 2 })), None).await;
        assert_eq!(keys, vec!["PAY-1", "PAY-2", "PAY-3", "PAY-4", "PAY-5"]);
    }

    /// An issue entering the scope **behind** the walk -- reopened under a
    /// `resolution = Unresolved` filter, say -- shifts every row the other
    /// way, so the next page opens on issues already read. They are handed
    /// out once, and the walk goes on to the end.
    #[tokio::test]
    async fn an_issue_entering_behind_the_walk_repeats_no_issue() {
        let api = LiveIndex::new(
            five_filed(),
            [(
                1,
                Box::new(|issues: &mut Vec<serde_json::Value>| {
                    issues.push(filed(
                        "PAY-0",
                        "2026-07-31T09:00:00.000+0000",
                        "2026-08-22T11:45:00.000+0000",
                    ));
                }) as Upstream,
            )],
        );
        let (keys, _) = keys_of(&api, &cfg(serde_json::json!({ "page_size": 2 })), None).await;
        assert_eq!(keys, vec!["PAY-1", "PAY-2", "PAY-3", "PAY-4", "PAY-5"]);
    }

    /// **A page asked from far back can stop short of the walk** and still
    /// end nothing (merge review of #2006). Twelve issues, two a page, on a
    /// server that clamps every answer at three rows (Jira's
    /// `jira.search.views.default.max`). Eight are read when PAY-1 to PAY-4
    /// are deleted; the walk steps back to offset 0, and the three rows there
    /// (PAY-5 to PAY-7) are all read and end short of where it stood. That is
    /// the clamp, not the listing's end: the walk goes on from PAY-7, and
    /// PAY-9 to PAY-12 arrive.
    #[tokio::test]
    async fn a_step_back_answered_short_of_the_walk_ends_nothing() {
        let twelve = (1..=12)
            .map(|n| {
                filed(
                    &format!("PAY-{n}"),
                    &format!("2026-08-{n:02}T09:00:00.000+0000"),
                    &format!("2026-08-22T10:{n:02}:00.000+0000"),
                )
            })
            .collect();
        let api =
            LiveIndex::new(twelve, [(4, delete(&["PAY-1", "PAY-2", "PAY-3", "PAY-4"]))]).clamped(3);
        let (keys, _) = keys_of(&api, &cfg(serde_json::json!({ "page_size": 2 })), None).await;
        let want: Vec<String> = (1..=12).map(|n| format!("PAY-{n}")).collect();
        assert_eq!(keys, want);
    }

    /// The listing emptied under the walk: four issues read, all five
    /// deleted. The walk steps back to offset 0, finds nothing there, and
    /// ends `Ok` with what it read.
    #[tokio::test]
    async fn a_listing_emptied_under_the_walk_ends_it() {
        let all = &["PAY-1", "PAY-2", "PAY-3", "PAY-4", "PAY-5"];
        let api = LiveIndex::new(five_filed(), [(2, delete(all))]);
        let (keys, _) = keys_of(&api, &cfg(serde_json::json!({ "page_size": 2 })), None).await;
        assert_eq!(keys, vec!["PAY-1", "PAY-2", "PAY-3", "PAY-4"]);
    }

    /// **A deletion and a filing between two pages hide nothing** (#1979).
    /// After the first page PAY-1, already read, is deleted and PAY-6 filed,
    /// so the total stays 5: a check on the total sees nothing, and offset 2
    /// of the listing that is left starts at PAY-4. The walk asks from where
    /// it stood, sees the page does not open on an issue it has read, and
    /// asks again from further back.
    #[tokio::test]
    async fn an_issue_deleted_while_another_is_filed_hides_no_unread_one() {
        let api = LiveIndex::new(
            five_filed(),
            [(
                1,
                Box::new(|issues: &mut Vec<serde_json::Value>| {
                    issues.retain(|i| i["key"] != "PAY-1");
                    issues.push(filed(
                        "PAY-6",
                        "2026-08-22T11:45:00.000+0000",
                        "2026-08-22T11:45:00.000+0000",
                    ));
                }) as Upstream,
            )],
        );
        let (keys, _) = keys_of(&api, &cfg(serde_json::json!({ "page_size": 2 })), None).await;
        assert_eq!(
            keys,
            vec!["PAY-1", "PAY-2", "PAY-3", "PAY-4", "PAY-5", "PAY-6"]
        );
    }

    /// A total that **grew** is not a hole: over `created`, an issue filed
    /// during the walk joins the end and is read there.
    #[tokio::test]
    async fn a_total_that_grew_between_pages_is_read_to_its_end() {
        let api = LiveIndex::new(
            five_filed(),
            [(
                1,
                Box::new(|issues: &mut Vec<serde_json::Value>| {
                    issues.push(filed(
                        "PAY-6",
                        "2026-08-22T11:45:00.000+0000",
                        "2026-08-22T11:45:00.000+0000",
                    ));
                }) as Upstream,
            )],
        );
        let (keys, _) = keys_of(&api, &cfg(serde_json::json!({ "page_size": 2 })), None).await;
        assert_eq!(
            keys,
            vec!["PAY-1", "PAY-2", "PAY-3", "PAY-4", "PAY-5", "PAY-6"]
        );
    }

    /// Interfaces §4.1: "an unrecognised version means full sync".
    #[tokio::test]
    async fn an_unreadable_cursor_falls_back_to_a_full_sync() {
        let api = FakeApi {
            issues: five_issues(),
            ..FakeApi::default()
        };
        let mut sink = VecSink(Vec::new());
        run(
            &api,
            &cfg(serde_json::json!({})),
            Some("tidewater-v1".to_owned()),
            &mut sink,
        )
        .await
        .unwrap();
        assert_eq!(sink.0.len(), 5);
    }

    /// `comment` and `worklog` are not in Jira's `*navigable` set, so a
    /// `/search` that does not name them returns no container -- which this
    /// adapter reads as "truncated" and completes with a request per issue.
    /// Asking for them up front is the difference between one request per page
    /// and one per issue, and nothing else in the suite would notice their
    /// removal: the completion path would quietly pick up the slack.
    #[test]
    fn the_field_list_asks_for_the_two_containers_that_would_otherwise_cost_a_request_each() {
        let config = cfg(serde_json::json!({}));
        let api = FakeApi::default();
        let fields = SyncRun {
            api: &api,
            cfg: &config,
            source_id: "jira",
            base_url: "https://jira.example",
        }
        .fields();
        let names: Vec<&str> = fields.split(',').collect();
        assert!(names.contains(&"comment"), "{fields}");
        assert!(names.contains(&"worklog"), "{fields}");
        // The fields the mapping itself reads (interfaces §4.1 normalization).
        for required in ["summary", "description", "updated", "assignee"] {
            assert!(
                names.contains(&required),
                "{required} missing from {fields}"
            );
        }
    }

    /// The other direction, and the half nothing else can see: a name that is
    /// **out** of the list, and the reason it is out.
    ///
    /// The positive half of #32's widening is certified at the wire --
    /// `tests/it/mockd.rs` runs this exact query against the mock and ends in
    /// `assert_no_violations()`, which is the only thing that can say the
    /// server would accept it. What no wire test can say is why a name is
    /// absent, and that is what went wrong the first time: the list stopped at
    /// twelve because mockd's closed set stopped there, so a mock's coverage
    /// was silently deciding what production fetched.
    ///
    /// Each row below is paired with its own reason, on the #33 precedent,
    /// because the reasons differ and a shared message would be false about
    /// at least one of them. **The fix for any of these is a fixture field,
    /// not this list**: adding the name alone makes mockd invent the value,
    /// and an invented value reaches `payload` and the search index as if the
    /// dataset had said it (the #28 ruling).
    #[test]
    fn the_field_list_asks_for_nothing_the_fixture_cannot_answer() {
        let config = cfg(serde_json::json!({}));
        let api = FakeApi::default();
        let run = SyncRun {
            api: &api,
            cfg: &config,
            source_id: "jira",
            base_url: "https://jira.example",
        };
        let fields = run.fields();
        let names: Vec<&str> = fields.split(',').collect();

        for (missing, why) in [
            (
                "components",
                "no ticket in fixtures/tidewater/work.json names a component, so mockd would \
                 have to invent one",
            ),
            (
                "fixVersions",
                "the fixture records no releases at all, so mockd would have to invent one",
            ),
            (
                "timeestimate",
                "the fixture records an estimate and worklogs but no *remaining* estimate, and \
                 deriving one would be arithmetic mockd made up",
            ),
        ] {
            assert!(
                !names.contains(&missing),
                "{missing} is in {fields}: {why}. mockd answers it with 400 + an UnknownField \
                 violation, so tests/it/mockd.rs fails too -- but fix it by giving the fixture the \
                 field, not by widening mockd's set around an invented value."
            );
        }

        // ...and the widened six, each named with what its absence costs.
        // Absence is not a smaller response here: `payload` is the reader
        // (spec §3a), so a name left out is data the mirror never holds and
        // a full re-fetch of the source is the only way back.
        for (needed, cost) in [
            ("parent", "epic membership, which the Contexts work reads"),
            ("issuelinks", "the blocks/blocked-by graph"),
            ("resolution", "whether a done issue was fixed or dropped"),
            ("labels", "every label-driven view and filter"),
            ("timeoriginalestimate", "the estimate side of time tracking"),
            ("timespent", "the logged side of time tracking"),
        ] {
            assert!(
                names.contains(&needed),
                "{needed} is missing from {fields}, so the mirror loses {cost} for every issue \
                 -- and gets it back only by re-syncing the whole source (#32)"
            );
        }

        // No whitespace and no duplicates: this goes into a query string
        // verbatim, and a repeated name is a widening that was applied twice.
        assert!(!fields.contains(' '), "{fields}");
        let mut sorted = names.clone();
        sorted.sort_unstable();
        let before = sorted.len();
        sorted.dedup();
        assert_eq!(before, sorted.len(), "a name appears twice in {fields}");
    }

    /// The configured Epic Link custom field rides along in `fields=`, which is
    /// the whole of what the option does -- it preserves epic membership in
    /// `payload` for a classic DC project (gap 3). Dropped, the option is a
    /// form input that silently does nothing.
    ///
    /// This asserts the *query*. The round trip -- the response, and the value
    /// landing in `sync.item.payload` -- is
    /// `knobas-app/tests/it/adapter_to_mirror.rs`'s
    /// `a_classic_projects_epic_link_reaches_the_stored_payload` (issue #125).
    /// Both halves, because either alone is green while the join is broken.
    #[test]
    fn the_epic_link_field_is_appended_to_the_field_list() {
        let config = cfg(serde_json::json!({ "epic_link_field": "customfield_10008" }));
        let api = FakeApi::default();
        let fields = SyncRun {
            api: &api,
            cfg: &config,
            source_id: "jira",
            base_url: "https://jira.example",
        }
        .fields();
        assert!(
            fields.split(',').any(|n| n == "customfield_10008"),
            "{fields}"
        );
    }

    /// The Sprint field rides the same parameter (#811), beside Epic Link.
    #[test]
    fn the_sprint_field_is_appended_to_the_field_list() {
        let config = cfg(serde_json::json!({
            "epic_link_field": "customfield_10008", "sprint_field": "customfield_10104"
        }));
        let api = FakeApi::default();
        let fields = SyncRun {
            api: &api,
            cfg: &config,
            source_id: "jira",
            base_url: "https://jira.example",
        }
        .fields();
        let names: Vec<&str> = fields.split(',').collect();
        assert!(names.contains(&"customfield_10104"), "{fields}");
        assert!(names.contains(&"customfield_10008"), "{fields}");
    }

    /// Battery clause 6: a sink error aborts the run rather than being
    /// swallowed, and it keeps its own variant.
    #[tokio::test]
    async fn a_sink_failure_aborts_the_run() {
        let api = FakeApi {
            issues: five_issues(),
            ..FakeApi::default()
        };
        let got = run(&api, &cfg(serde_json::json!({})), None, &mut FailingSink)
            .await
            .unwrap_err();
        assert!(matches!(got, SourceError::Sink(_)), "{got:?}");
    }

    /// Battery clause 4: the classification raised mid-sync is the one the user
    /// actually sees, because `test_connection` ran once when the source was
    /// added.
    #[tokio::test]
    async fn an_auth_failure_mid_sync_keeps_its_classification() {
        let api = FakeApi {
            unauthorized: true,
            issues: five_issues(),
            ..FakeApi::default()
        };
        let mut sink = VecSink(Vec::new());
        let got = run(&api, &cfg(serde_json::json!({})), None, &mut sink)
            .await
            .unwrap_err();
        assert!(matches!(got, SourceError::Unauthorized { .. }), "{got:?}");
    }

    /// Jira's golden records -- the two issues of a real `/search` page and
    /// an epic read whole -- fingerprint as they did before #1314 moved the
    /// algorithm from this crate into `knobas_source::fingerprint`. The
    /// literals are what this crate's own copy gave them, run on that copy. A
    /// value that moves here would re-deliver every issue in the overlap
    /// window once after the upgrade.
    #[test]
    fn golden_records_keep_their_fingerprints() {
        let page: serde_json::Value =
            serde_json::from_str(include_str!("../tests/golden/search-page.json")).unwrap();
        let issues = page["issues"].as_array().unwrap();
        assert_eq!(issues[0]["key"], "PAY-231");
        assert_eq!(
            knobas_source::fingerprint::of(&issues[0]),
            "bf376862bfa5eae0"
        );
        assert_eq!(issues[1]["key"], "PAY-240");
        assert_eq!(
            knobas_source::fingerprint::of(&issues[1]),
            "d7d2e69253c3cee3"
        );
        assert_eq!(knobas_source::fingerprint::of(&page), "4b19d7a7866cebd1");
        let epic: serde_json::Value =
            serde_json::from_str(include_str!("../tests/golden/issue-epic.json")).unwrap();
        assert_eq!(knobas_source::fingerprint::of(&epic), "58f840f0d1e66aea");
    }
}
