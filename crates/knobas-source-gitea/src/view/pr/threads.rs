//! A pull request's line comments placed on its diff (issue #709, spec #701
//! workflow 6): each review's comments read, grouped into one thread per
//! line, and each thread on the line it was written about -- or, where that
//! line is gone or reads otherwise since, apart as outdated.
//!
//! Gitea 1.27 answers a line comment's line as `position` (the new side's
//! number) or `original_position` (the old side's), the other `0` -- measured
//! against the real server, which takes the same pair as `new_position` /
//! `old_position` on the way in. It keeps that number when a later push moves
//! the line, and the API carries no *outdated* flag, so the adapter decides:
//! the comment's `diff_hunk` ends on the line as it read when commented, and a
//! line of the current diff that no longer reads so is not the line it was
//! about (ADR-0007: the source's answer, never the shell's guess).

use std::future::Future;
use std::pin::Pin;
use std::task::Poll;

use knobas_source::ReviewSide;
use knobas_source::view::{DiffFile, DiffLine, FileThread, LineComment, LineThread};

use super::super::login;
use super::Review;
use crate::model::UserRef;

/// One line comment as Gitea answers it, with the review it came in.
pub(super) struct Written {
    path: String,
    side: ReviewSide,
    line: u32,
    /// The commented line's text when it was written: the `diff_hunk`'s last
    /// line, without its marker. `None` where Gitea sent no hunk.
    was: Option<String>,
    comment: LineComment,
    /// Gitea's id for the comment, which resolve and unresolve name (#2277).
    id: Option<u64>,
    /// Who resolved the conversation this comment opens, as Gitea keeps it
    /// on the comment (`resolver`); `None` while open.
    resolver: Option<String>,
}

/// The fields of Gitea's `PullReviewComment` placing reads.
#[derive(Debug, serde::Deserialize)]
pub(super) struct ReviewLineComment {
    #[serde(default)]
    id: Option<u64>,
    #[serde(default)]
    resolver: Option<UserRef>,
    #[serde(default)]
    user: Option<UserRef>,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    position: u32,
    #[serde(default)]
    original_position: u32,
    #[serde(default)]
    diff_hunk: Option<String>,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    created_at: Option<String>,
}

/// The reviews whose line comments are read, with the id they are read by:
/// those Gitea counts any for, and not a draft (`PENDING`), which is nobody's
/// yet and only its author sees.
pub(super) fn with_comments(reviews: &[Review]) -> Vec<(u64, &Review)> {
    reviews
        .iter()
        .filter(|r| r.comments_count > 0 && r.state.as_deref() != Some("PENDING"))
        .filter_map(|r| Some((r.id?, r)))
        .collect()
}

/// `raw`, the comments of `review`, as placing reads them. A comment Gitea
/// gave no path or no line is not one a diff can hold, and is left out.
pub(super) fn written(review: &Review, raw: Vec<serde_json::Value>) -> Vec<Written> {
    raw.into_iter()
        .filter_map(|value| serde_json::from_value::<ReviewLineComment>(value).ok())
        .filter_map(|c| {
            let path = c.path.filter(|p| !p.is_empty())?;
            let (side, line) = match (c.position, c.original_position) {
                (0, 0) => return None,
                (0, old) => (ReviewSide::Old, old),
                (new, _) => (ReviewSide::New, new),
            };
            let author = login(c.user.as_ref())
                .or_else(|| login(review.user.as_ref()))
                .unwrap_or_else(|| "someone".to_owned());
            Some(Written {
                path,
                side,
                line,
                was: c.diff_hunk.as_deref().and_then(last_line),
                id: c.id,
                resolver: c
                    .resolver
                    .as_ref()
                    .map(|r| login(Some(r)).unwrap_or_else(|| "someone".to_owned())),
                comment: LineComment {
                    author,
                    body: c.body.unwrap_or_default(),
                    at: c.created_at,
                },
            })
        })
        .collect()
}

/// The text of a hunk's last line of the file -- what the comment was written
/// on -- without its `+`, `-` or ` ` marker. A `\ No newline` note and the
/// `@@` header are not lines of the file.
///
/// `lines()` drops a CRLF file's `\r` here as the shared diff parser does for
/// the line it is compared with, so such a file compares like any other.
fn last_line(hunk: &str) -> Option<String> {
    let line = hunk
        .lines()
        .rev()
        .find(|l| !l.starts_with('\\') && !l.starts_with("@@"))?;
    Some(match line.as_bytes().first() {
        Some(b'+' | b'-' | b' ') => line[1..].to_owned(),
        _ => line.to_owned(),
    })
}

/// Put each comment in `written` on its file of `files`: one thread per line
/// and side, the replies oldest first; the threads in the file's order, the
/// outdated after the rest. A comment on a file this diff no longer holds is
/// outdated by definition and answered apart, threaded the same way, for the
/// diff's own outdated list: never dropped (the #709 ruling).
///
/// A line missing from a file the cap cut may be past the cut rather than
/// gone, so there it is not called outdated: the source does not know, and
/// the shell draws the thread apart as on a line it does not show.
pub(super) fn place(files: &mut [DiffFile], written: Vec<Written>) -> Vec<FileThread> {
    let mut gone: Vec<FileThread> = Vec::new();
    for comment in written {
        let Some(file) = files.iter_mut().find(|f| f.path == comment.path) else {
            match gone.iter_mut().find(|g| {
                g.path == comment.path
                    && g.thread.side == comment.side
                    && g.thread.line == comment.line
            }) {
                Some(g) => join(&mut g.thread, comment),
                None => gone.push(FileThread {
                    path: comment.path.clone(),
                    thread: opened(comment, true),
                }),
            }
            continue;
        };
        let current = line_of(file, comment.side, comment.line);
        let outdated = match (current, &comment.was) {
            (None, _) => !file.truncated,
            (Some(line), Some(was)) => line.text != *was,
            (Some(_), None) => false,
        };
        match file
            .threads
            .iter_mut()
            .find(|t| t.side == comment.side && t.line == comment.line && t.outdated == outdated)
        {
            Some(thread) => join(thread, comment),
            None => file.threads.push(opened(comment, outdated)),
        }
    }
    for file in files.iter_mut() {
        for thread in &mut file.threads {
            oldest_first(&mut thread.comments);
            // Gitea places a reply by line on the current head (#2277): an
            // outdated thread's line is not that line, so it takes none.
            thread.replies = thread.id.is_some() && !thread.outdated;
        }
        file.threads
            .sort_by_key(|t| (t.outdated, t.line, t.side == ReviewSide::New));
    }
    for g in &mut gone {
        oldest_first(&mut g.thread.comments);
    }
    gone.sort_by(|a, b| {
        (&a.path, a.thread.line, a.thread.side == ReviewSide::New).cmp(&(
            &b.path,
            b.thread.line,
            b.thread.side == ReviewSide::New,
        ))
    });
    gone
}

/// A thread opened by `comment`. Its id and its resolver are the first
/// comment's, which [`join`] keeps the oldest.
fn opened(comment: Written, outdated: bool) -> LineThread {
    LineThread {
        side: comment.side,
        line: comment.line,
        outdated,
        id: comment.id.map(|id| id.to_string()),
        resolved_by: comment.resolver,
        replies: false,
        comments: vec![comment.comment],
    }
}

/// `comment` added to `thread`. Gitea keeps a conversation's resolution on
/// the comment that opened it (the fact note of 2026-10-30, #2253), and its
/// ids grow with time, so the thread's id and resolver are those of its
/// lowest-numbered comment: the one the web page resolves.
fn join(thread: &mut LineThread, comment: Written) {
    let first = thread.id.as_deref().and_then(|id| id.parse::<u64>().ok());
    if let Some(id) = comment.id
        && first.is_none_or(|first| id < first)
    {
        thread.id = Some(id.to_string());
        thread.resolved_by = comment.resolver;
    }
    thread.comments.push(comment.comment);
}

/// A thread's replies oldest first. Stable: equal instants, and comments
/// with none (last), keep Gitea's order.
fn oldest_first(comments: &mut [LineComment]) {
    comments.sort_by_key(|c| {
        let at =
            c.at.as_deref()
                .and_then(|at| chrono::DateTime::parse_from_rfc3339(at).ok());
        (at.is_none(), at)
    });
}

/// The line numbered `line` on `side` of `file`'s hunks: a removed or kept
/// line's old number, an added or kept line's new one.
fn line_of(file: &DiffFile, side: ReviewSide, line: u32) -> Option<&DiffLine> {
    file.hunks
        .iter()
        .flat_map(|h| &h.lines)
        .find(|l| match side {
            ReviewSide::Old => l.old == Some(line),
            ReviewSide::New => l.new == Some(line),
        })
}

/// Every future in `reads`, side by side in the task that awaits this, in
/// order; the first error ends it. A view costs about its slowest read, not
/// their sum (#703), and a pull request with several reviewed rounds asks one
/// read per review. `tokio::try_join!` takes a fixed count; this takes a list
/// and spawns nothing.
pub(super) async fn all<F, T, E>(reads: Vec<F>) -> Result<Vec<T>, E>
where
    F: Future<Output = Result<T, E>>,
{
    let mut reads: Vec<Pin<Box<F>>> = reads.into_iter().map(Box::pin).collect();
    let mut done: Vec<Option<T>> = reads.iter().map(|_| None).collect();
    std::future::poll_fn(|cx| {
        let mut waiting = false;
        for (read, slot) in reads.iter_mut().zip(done.iter_mut()) {
            if slot.is_some() {
                continue;
            }
            match read.as_mut().poll(cx) {
                Poll::Ready(Ok(value)) => *slot = Some(value),
                Poll::Ready(Err(error)) => return Poll::Ready(Err(error)),
                Poll::Pending => waiting = true,
            }
        }
        if waiting {
            Poll::Pending
        } else {
            Poll::Ready(Ok(()))
        }
    })
    .await?;
    Ok(done.into_iter().flatten().collect())
}
