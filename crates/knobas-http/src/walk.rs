//! One walk over a page-sequence listing (#1315).
//!
//! Three built-in adapters read listings a page at a time, and each used to
//! own the loop: when to stop, what a page cap means, and what a short page
//! says. [`PageWalk`] owns all three; the adapter hands it a fetch (how to ask
//! for one page) and the [`Protocol`] its source speaks:
//!
//! * [`PageNumber`] -- `page` plus `limit` (Gitea).
//! * [`Offset`] -- `startAt` plus `total` (Jira).
//! * [`NextLink`] -- the server's own continuation link (Confluence).
//!
//! **Empty, not short.** Where the client builds the next request itself (page
//! number, offset), a page shorter than the limit asked for is not the end: a
//! server may cap a page below the limit, and reading a short page as the end
//! is a run that reports success while its watermark advances past records it
//! never saw (Gitea issue #81, ruled 2026-08-29). An empty page is the one
//! answer that cannot mean anything else, so it ends the walk -- or, where the
//! page reports a total, reaching it does. A [`NextLink`] walk ends where the
//! server stops offering a link, and an empty page there does not end it: the
//! link is the server's word that there is more.
//!
//! **The cap is a request budget**, counted in pages handed out. When the
//! walk would ask for a page past it, [`PageWalk::next`] answers
//! [`Walked::Capped`] -- never a quiet [`Walked::End`] -- so a walk that
//! promises a whole corpus can refuse a partial one, and a walk that promises
//! only a budget can stop. Which of the two it is stays the adapter's.
//!
//! **Not every walk is this one.** TeamCity's walk re-reads its listing from
//! the top with a wider count rather than asking for a next page (its
//! `sync::all_of`), so it stays its own. Gitea's pull-request discussion is
//! not paged at all (`client::issue_comments`).

use std::future::Future;

use knobas_source::SourceError;

/// What a page says about itself and the rest of its listing, as the walk
/// reads it. An adapter implements it on the page type its fetch answers.
pub trait Paged {
    /// How many records are on this page.
    fn rows(&self) -> usize;
    /// The size of the whole listing, where the page reports one (Jira's
    /// `total`, Gitea's `X-Total-Count`).
    fn total(&self) -> Option<usize> {
        None
    }
    /// The page's continuation link, where the protocol has one
    /// (Confluence's `_links.next`).
    fn next_link(&self) -> Option<&str> {
        None
    }
}

/// A plain page of records, as Gitea's listings answer.
impl<T> Paged for Vec<T> {
    fn rows(&self) -> usize {
        self.len()
    }
}

/// A page of records and the listing's size, as Gitea's label listing
/// answers with `X-Total-Count`.
impl<T> Paged for (Vec<T>, Option<usize>) {
    fn rows(&self) -> usize {
        self.0.len()
    }
    fn total(&self) -> Option<usize> {
        self.1
    }
}

/// How a page-sequence protocol asks for the page after the last.
pub trait Protocol {
    /// What one request is asked with: a page number, an offset, a link.
    type At;
    /// Whether an empty page ends the walk: true where the client builds the
    /// next request itself, so nothing but an empty page can say "no more".
    const EMPTY_ENDS: bool;
    /// What the first request is asked with.
    fn first(&self) -> Self::At;
    /// What the next request is asked with, after `pages` pages holding
    /// `rows` records in all, the last of them `page`; `None` where the
    /// protocol says the listing ended.
    ///
    /// # Errors
    ///
    /// A continuation the walk refuses to follow ([`NextLink`]).
    fn after(
        &self,
        pages: u32,
        rows: usize,
        page: &impl Paged,
    ) -> Result<Option<Self::At>, SourceError>;
}

/// `page=1, 2, 3, …` plus a `limit` (Gitea). Ends on an empty page, or where
/// the pages so far hold the total the last one reported.
#[derive(Debug, Clone, Copy)]
pub struct PageNumber;

impl Protocol for PageNumber {
    type At = u32;
    const EMPTY_ENDS: bool = true;
    fn first(&self) -> u32 {
        1
    }
    fn after(
        &self,
        pages: u32,
        rows: usize,
        page: &impl Paged,
    ) -> Result<Option<u32>, SourceError> {
        Ok((!reached(rows, page)).then_some(pages.saturating_add(1)))
    }
}

/// `startAt=0` and on by the records read, plus a `total` (Jira). Ends on an
/// empty page, or where the records read reach the total the last page
/// reported.
#[derive(Debug, Clone, Copy)]
pub struct Offset;

impl Protocol for Offset {
    type At = u32;
    const EMPTY_ENDS: bool = true;
    fn first(&self) -> u32 {
        0
    }
    fn after(
        &self,
        _pages: u32,
        rows: usize,
        page: &impl Paged,
    ) -> Result<Option<u32>, SourceError> {
        Ok((!reached(rows, page)).then(|| u32::try_from(rows).unwrap_or(u32::MAX)))
    }
}

/// Whether the records read reach the total the page reported.
fn reached(rows: usize, page: &impl Paged) -> bool {
    page.total().is_some_and(|total| rows >= total)
}

/// The server's own continuation link (Confluence's `_links.next`). The first
/// request is asked with `None`, every later one with the link the page
/// before offered. Ends where a page offers none; an empty page with a link
/// is followed.
///
/// A link that is not a path rooted at the instance is refused: an absolute
/// URL names a host the client was not built for, and appending it to the
/// base URL would make a nonsense request rather than a refusal.
#[derive(Debug, Clone, Copy)]
pub struct NextLink {
    /// Who offered the link, for the refusal: `"Confluence"`.
    pub product: &'static str,
}

impl Protocol for NextLink {
    type At = Option<String>;
    const EMPTY_ENDS: bool = false;
    fn first(&self) -> Option<String> {
        None
    }
    fn after(
        &self,
        _pages: u32,
        _rows: usize,
        page: &impl Paged,
    ) -> Result<Option<Option<String>>, SourceError> {
        let Some(link) = page.next_link() else {
            return Ok(None);
        };
        if !link.starts_with('/') {
            return Err(SourceError::protocol(format!(
                "{} answered with a continuation link this adapter cannot follow: {link:?} is \
                 not a path rooted at the instance. Refusing rather than reporting a walk that \
                 stopped early as a completed one.",
                self.product
            )));
        }
        Ok(Some(Some(link.to_owned())))
    }
}

/// What [`PageWalk::next`] answers.
#[derive(Debug)]
pub enum Walked<T> {
    /// The next page, never an empty one where the protocol ends on empty.
    Page(T),
    /// The listing ended where the protocol says it does.
    End,
    /// The walk would ask for a page past its cap. The pages handed out are
    /// not the whole listing; whether that fails the run is the caller's.
    Capped,
}

/// Where the walk stands between two pages.
enum State<At> {
    /// The next request is asked with this.
    Ask(At),
    /// The last page offered a continuation the walk will not follow. Raised
    /// on the call after that page, so the caller has handled its records.
    Refused(SourceError),
    Ended,
}

/// One walk over a page-sequence listing: the loop, the cap and the end rule,
/// for a fetch the adapter supplies. See the module docs.
pub struct PageWalk<P: Protocol, F> {
    protocol: P,
    fetch: F,
    state: State<P::At>,
    cap: u32,
    pages: u32,
    rows: usize,
}

impl<P: Protocol, F> std::fmt::Debug for PageWalk<P, F> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PageWalk")
            .field("cap", &self.cap)
            .field("pages", &self.pages)
            .field("rows", &self.rows)
            .finish_non_exhaustive()
    }
}

impl<P, F, Fut, T> PageWalk<P, F>
where
    P: Protocol,
    F: FnMut(P::At) -> Fut,
    Fut: Future<Output = Result<T, SourceError>>,
    T: Paged,
{
    /// A walk of at most `cap` pages, asking `fetch` for each.
    pub fn new(protocol: P, cap: u32, fetch: F) -> Self {
        let first = protocol.first();
        Self {
            protocol,
            fetch,
            state: State::Ask(first),
            cap,
            pages: 0,
            rows: 0,
        }
    }

    /// The next page, the end of the listing, or the cap.
    ///
    /// The cap is answered when the walk would ask for a page past it, so
    /// the caller has handled the last page's records first; the same holds
    /// for a refused continuation. After an error the walk is over.
    ///
    /// # Errors
    ///
    /// The fetch's, or a continuation [`NextLink`] refuses.
    pub async fn next(&mut self) -> Result<Walked<T>, SourceError> {
        let at = match std::mem::replace(&mut self.state, State::Ended) {
            State::Ended => return Ok(Walked::End),
            state if self.pages >= self.cap => {
                self.state = state;
                return Ok(Walked::Capped);
            }
            State::Refused(error) => return Err(error),
            State::Ask(at) => at,
        };
        let page = (self.fetch)(at).await?;
        if P::EMPTY_ENDS && page.rows() == 0 {
            return Ok(Walked::End);
        }
        self.pages += 1;
        self.rows += page.rows();
        self.state = match self.protocol.after(self.pages, self.rows, &page) {
            Ok(Some(at)) => State::Ask(at),
            Ok(None) => State::Ended,
            Err(error) => State::Refused(error),
        };
        Ok(Walked::Page(page))
    }

    /// The records on every page handed out so far.
    pub fn rows(&self) -> usize {
        self.rows
    }
}
