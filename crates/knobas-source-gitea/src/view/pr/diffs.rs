//! A pull request's `.diff`, kept for the session at what it stands at (spec
//! #2234 step 1, ADR-0032 as amended by #2321).
//!
//! *Files* cannot be a fixed view: it carries the reviews' threads, which move
//! (ADR-0032: a fixed view carries nothing that moves). Its diff does not move
//! while it stands at the same merge base and head, and it is the one
//! expensive read. So the built source keeps the text here, beside the app's
//! read cache rather than in it, and drops it with the source: a rebuild (a new
//! address or credential, a removal) starts empty. One entry per pull request;
//! a new head replaces the old one. Bounded by bytes, oldest out first; a diff
//! larger than the bound is used and not kept.
//!
//! **What the `.diff` stands at is not the record's head.** The record names
//! the head branch's tip at once; the `.diff` and the commits listing follow
//! `refs/pull/N/head`, which Forgejo moves through a queue a second or two
//! after a push (#1195; measured on Forgejo 17 and Gitea 1.27 in #2321: the
//! listing's newest commit and the `.diff` moved in the same sample on both,
//! the record up to 1.8 s ahead on Forgejo). So the key is the record's merge
//! base and the listing's newest commit, and `PullView::files` keeps a diff
//! only when that commit is the record's head both before and after the
//! `.diff` was read.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex, PoisonError};

/// How many bytes of diff text a built source keeps.
pub(crate) const BOUND: usize = 32 * 1024 * 1024;

/// What a kept diff stands at: the pull request, its merge base and the head
/// its commits listing ends on, both whole shas.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DiffKey {
    owner: String,
    repo: String,
    number: u64,
    merge_base: String,
    head: String,
}

impl DiffKey {
    /// The key of a diff at `merge_base` and `head`, or `None` when either
    /// is not a whole sha: then nothing says what the diff stands at.
    pub(super) fn of(
        owner: &str,
        repo: &str,
        number: u64,
        merge_base: &str,
        head: &str,
    ) -> Option<Self> {
        (crate::view::is_full_sha(merge_base) && crate::view::is_full_sha(head))
            .then(|| Self::new(owner, repo, number, merge_base, head))
    }

    fn new(owner: &str, repo: &str, number: u64, merge_base: &str, head: &str) -> Self {
        Self {
            owner: owner.to_owned(),
            repo: repo.to_owned(),
            number,
            merge_base: merge_base.to_ascii_lowercase(),
            head: head.to_ascii_lowercase(),
        }
    }

    fn same_pull(&self, owner: &str, repo: &str, number: u64) -> bool {
        self.number == number && self.owner == owner && self.repo == repo
    }
}

/// The kept diffs of one built source.
#[derive(Debug, Default)]
pub(crate) struct PullDiffs {
    kept: Mutex<Kept>,
}

#[derive(Debug, Default)]
struct Kept {
    /// Least recently used first.
    entries: VecDeque<(DiffKey, Arc<str>)>,
    bytes: usize,
}

impl Kept {
    fn remove_at(&mut self, at: usize) -> Option<(DiffKey, Arc<str>)> {
        let entry = self.entries.remove(at)?;
        self.bytes -= entry.1.len();
        Some(entry)
    }
}

impl PullDiffs {
    fn lock(&self) -> std::sync::MutexGuard<'_, Kept> {
        self.kept.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Whether a diff of this pull request is kept at `head`, at whatever
    /// merge base.
    pub(crate) fn holds_at(&self, owner: &str, repo: &str, number: u64, head: &str) -> bool {
        self.lock().entries.iter().any(|(key, _)| {
            key.same_pull(owner, repo, number) && key.head.eq_ignore_ascii_case(head)
        })
    }

    /// The diff kept at `key`, now the most recently used.
    pub(crate) fn get(&self, key: &DiffKey) -> Option<Arc<str>> {
        let mut kept = self.lock();
        let at = kept.entries.iter().position(|(k, _)| k == key)?;
        let entry = kept.remove_at(at)?;
        let text = Arc::clone(&entry.1);
        kept.bytes += text.len();
        kept.entries.push_back(entry);
        Some(text)
    }

    /// Keep `text` at `key`, in place of any diff of the same pull request;
    /// the least recently used go until it fits. A diff over the bound is not
    /// kept.
    pub(crate) fn put(&self, key: DiffKey, text: Arc<str>) {
        let mut kept = self.lock();
        if let Some(at) = kept
            .entries
            .iter()
            .position(|(k, _)| k.same_pull(&key.owner, &key.repo, key.number))
        {
            kept.remove_at(at);
        }
        if text.len() > BOUND {
            return;
        }
        while kept.bytes + text.len() > BOUND {
            if kept.remove_at(0).is_none() {
                break;
            }
        }
        kept.bytes += text.len();
        kept.entries.push_back((key, text));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(number: u64, head: char) -> DiffKey {
        DiffKey::new(
            "o",
            "r",
            number,
            &"b".repeat(40),
            &head.to_string().repeat(40),
        )
    }

    fn text(bytes: usize) -> Arc<str> {
        "x".repeat(bytes).into()
    }

    /// A diff is answered at its own ends only, and a new head of the same
    /// pull request takes its place.
    #[test]
    fn a_diff_is_kept_at_its_ends_and_a_new_head_replaces_it() {
        let diffs = PullDiffs::default();
        let a = "a".repeat(40);
        assert!(!diffs.holds_at("o", "r", 1, &a));
        diffs.put(key(1, 'a'), "one".into());
        assert!(diffs.holds_at("o", "r", 1, &a) && !diffs.holds_at("o", "r", 2, &a));
        assert!(!diffs.holds_at("o", "r", 1, &"c".repeat(40)));
        assert_eq!(diffs.get(&key(1, 'a')).as_deref(), Some("one"));
        assert_eq!(diffs.get(&key(1, 'c')), None);

        diffs.put(key(1, 'c'), "two".into());
        assert_eq!(diffs.get(&key(1, 'a')), None, "the old head is gone");
        assert_eq!(diffs.get(&key(1, 'c')).as_deref(), Some("two"));
        assert_eq!(diffs.lock().bytes, 3);
    }

    /// Past the bound the least recently used go first; a diff over the bound
    /// is not kept and drops nothing else.
    #[test]
    fn the_bound_drops_the_least_recently_used_first() {
        let diffs = PullDiffs::default();
        diffs.put(key(1, 'a'), text(BOUND / 2));
        diffs.put(key(2, 'a'), text(BOUND / 2));
        diffs.get(&key(1, 'a'));
        diffs.put(key(3, 'a'), text(BOUND / 4));
        let a = "a".repeat(40);
        assert!(diffs.holds_at("o", "r", 1, &a), "used last, kept");
        assert!(!diffs.holds_at("o", "r", 2, &a), "used least, dropped");
        assert!(diffs.holds_at("o", "r", 3, &a));
        assert_eq!(diffs.lock().bytes, BOUND / 2 + BOUND / 4);

        diffs.put(key(4, 'a'), text(BOUND + 1));
        assert!(!diffs.holds_at("o", "r", 4, &a));
        assert!(diffs.holds_at("o", "r", 1, &a) && diffs.holds_at("o", "r", 3, &a));
    }
}
