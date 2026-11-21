//! History: a merge base, and the commits of `base..head` as `git log`
//! lists them.

use std::collections::{BinaryHeap, HashMap};

use gix::hash::ObjectId;

use crate::read::parents;
use crate::{Answer, AskGit, Read, Repo};

impl Repo {
    /// `git merge-base --end-of-options <a> <b>`: the best common ancestor
    /// of the two commits and a newline, or [`Answer::Refused`] where they
    /// share none, or the clone lacks one of them.
    ///
    /// # Errors
    ///
    /// [`AskGit`] in a shallow clone, whose history only git reads as cut.
    pub fn merge_base(&self, a: &str, b: &str) -> Read {
        Ok(match self.merge_base_id(a, b)? {
            Some(id) => Answer::Out(format!("{id}\n").into_bytes()),
            None => Answer::Refused,
        })
    }

    pub(crate) fn merge_base_id(&self, a: &str, b: &str) -> Result<Option<ObjectId>, AskGit> {
        if self.shallow {
            return Err(AskGit::because("history in a shallow clone"));
        }
        let (Some(a), Some(b)) = (self.commit_id(a)?, self.commit_id(b)?) else {
            return Ok(None);
        };
        match self.repo.merge_base(a, b) {
            Ok(Some(id)) => Ok(Some(id.detach())),
            Ok(None) => Ok(None),
            Err(error) => Err(AskGit::because(format!("a merge base: {error}"))),
        }
    }

    /// `git log --no-color --max-count=<max> --format=%H%x1f%an%x1f%ar%x1f%s%x1e
    /// <base>..<head> --`: the commits `head` reaches and `base` does not,
    /// newest first as git orders them, each its sha, author, the author
    /// date as "3 weeks ago" against `now` (seconds since the epoch) and
    /// subject.
    ///
    /// The walk is git's own (`revision.c`'s `limit_list`): commits leave a
    /// list ordered by commit date, the ones a hidden commit reaches are
    /// marked on the way, and the walk goes on a little past the last shown
    /// commit in case an older date hides a newer one. Dates that tie keep
    /// the order they entered the list in, which is what a repository made
    /// by a script at one fixed date shows.
    ///
    /// # Errors
    ///
    /// [`AskGit`] in a shallow clone, for a commit with an `encoding` header
    /// (git re-encodes its text), and where `log.showSignature` is set.
    pub fn log(&self, base: &str, head: &str, max: usize, now: i64) -> Read {
        if self.shallow {
            return Err(AskGit::because("history in a shallow clone"));
        }
        if self.config_bool("log.showSignature").unwrap_or(false) {
            return Err(AskGit::because("log.showSignature"));
        }
        let (Some(base), Some(head)) = (self.commit_id(base)?, self.commit_id(head)?) else {
            return Ok(Answer::Refused);
        };
        let mut walk = Walk::new(self);
        let shown = walk.range(base, head)?;
        let mut out = Vec::new();
        for id in shown.into_iter().take(max) {
            let object = self.object(id)?;
            let commit = Commit::of(&object.data);
            if commit.encoding {
                return Err(AskGit::because("a commit with an encoding header"));
            }
            out.extend_from_slice(id.to_string().as_bytes());
            out.push(0x1f);
            if let Some(author) = &commit.author {
                out.extend_from_slice(author.name);
            }
            out.push(0x1f);
            if let Some(author) = &commit.author {
                out.extend_from_slice(relative(author.date, now).as_bytes());
            }
            out.push(0x1f);
            out.extend_from_slice(&subject(commit.message));
            out.extend_from_slice(b"\x1e\n");
        }
        Ok(Answer::Out(out))
    }

    /// A boolean of the repository's configuration, `None` where unset.
    pub(crate) fn config_bool(&self, key: &str) -> Option<bool> {
        self.repo.config_snapshot().try_boolean(key).ok().flatten()
    }
}

const SEEN: u8 = 1;
const UNINTERESTING: u8 = 2;
const ADDED: u8 = 4;
/// How many more hidden commits git looks at once nothing shown is left in
/// its list (`revision.c`'s `SLOP`).
const SLOP: i32 = 5;

#[derive(Default)]
struct Node {
    flags: u8,
    date: i64,
    /// Read once the commit is parsed.
    parents: Option<Vec<ObjectId>>,
    in_list: bool,
}

/// One entry of git's date-ordered list: newest first, and among equal
/// dates the one that entered first.
struct Entry {
    date: i64,
    seq: u64,
    id: ObjectId,
}

impl PartialEq for Entry {
    fn eq(&self, other: &Self) -> bool {
        self.seq == other.seq
    }
}
impl Eq for Entry {}
impl PartialOrd for Entry {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Entry {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.date
            .cmp(&other.date)
            .then_with(|| other.seq.cmp(&self.seq))
    }
}

struct Walk<'a> {
    clone: &'a Repo,
    nodes: HashMap<ObjectId, Node>,
    list: BinaryHeap<Entry>,
    seq: u64,
    /// The entries of `list` not marked uninteresting.
    interesting: usize,
}

impl<'a> Walk<'a> {
    fn new(clone: &'a Repo) -> Self {
        Self {
            clone,
            nodes: HashMap::new(),
            list: BinaryHeap::new(),
            seq: 0,
            interesting: 0,
        }
    }

    /// The commit parsed: its date and parents known.
    fn parse(&mut self, id: ObjectId) -> Result<(), AskGit> {
        if self.nodes.get(&id).is_some_and(|n| n.parents.is_some()) {
            return Ok(());
        }
        let object = self.clone.object(id)?;
        if object.kind != gix::objs::Kind::Commit {
            return Err(AskGit::because("a parent that is no commit"));
        }
        let date = Commit::of(&object.data).committed;
        let found: Vec<ObjectId> = parents(&object.data).collect();
        let node = self.nodes.entry(id).or_default();
        node.date = date;
        node.parents = Some(found);
        Ok(())
    }

    fn insert(&mut self, id: ObjectId) {
        let node = self.nodes.entry(id).or_default();
        node.in_list = true;
        if node.flags & UNINTERESTING == 0 {
            self.interesting += 1;
        }
        self.seq += 1;
        self.list.push(Entry {
            date: node.date,
            seq: self.seq,
            id,
        });
    }

    fn mark(&mut self, id: ObjectId) -> bool {
        let node = self.nodes.entry(id).or_default();
        if node.flags & UNINTERESTING != 0 {
            return false;
        }
        node.flags |= UNINTERESTING;
        if node.in_list {
            self.interesting -= 1;
        }
        true
    }

    /// `mark_parents_uninteresting`: the parents of `id`, and on through
    /// every commit already parsed.
    fn mark_parents(&mut self, id: ObjectId) {
        let mut pending: Vec<ObjectId> = self
            .nodes
            .get(&id)
            .and_then(|n| n.parents.clone())
            .unwrap_or_default();
        while let Some(next) = pending.pop() {
            if self.mark(next)
                && let Some(more) = self.nodes.get(&next).and_then(|n| n.parents.as_ref())
            {
                pending.extend(more.iter().copied());
            }
        }
    }

    /// `process_parents` with a list.
    fn process_parents(&mut self, id: ObjectId) -> Result<(), AskGit> {
        let node = self.nodes.entry(id).or_default();
        if node.flags & ADDED != 0 {
            return Ok(());
        }
        node.flags |= ADDED;
        let hidden = node.flags & UNINTERESTING != 0;
        let found = node.parents.clone().unwrap_or_default();
        for parent in found {
            if hidden {
                self.mark(parent);
                // A parent a hidden commit names and the clone lacks is
                // passed over, as git passes over it.
                if self.parse(parent).is_err() {
                    continue;
                }
                if self
                    .nodes
                    .get(&parent)
                    .and_then(|n| n.parents.as_ref())
                    .is_some_and(|p| !p.is_empty())
                {
                    self.mark_parents(parent);
                }
            } else {
                self.parse(parent)?;
            }
            let node = self.nodes.entry(parent).or_default();
            if node.flags & SEEN == 0 {
                node.flags |= SEEN;
                self.insert(parent);
            }
        }
        Ok(())
    }

    /// The commits of `base..head`, in the order `git log` shows them.
    fn range(&mut self, base: ObjectId, head: ObjectId) -> Result<Vec<ObjectId>, AskGit> {
        self.parse(base)?;
        self.parse(head)?;
        // `^base` is handled first: marked, and its parents with it.
        self.mark(base);
        self.mark_parents(base);
        for id in [base, head] {
            let node = self.nodes.entry(id).or_default();
            if node.flags & SEEN == 0 {
                node.flags |= SEEN;
                self.insert(id);
            }
        }
        let mut shown: Vec<ObjectId> = Vec::new();
        let mut slop = SLOP;
        let mut date = i64::MAX;
        while let Some(entry) = self.list.pop() {
            let id = entry.id;
            let node = self.nodes.entry(id).or_default();
            node.in_list = false;
            if node.flags & UNINTERESTING == 0 {
                self.interesting -= 1;
            }
            self.process_parents(id)?;
            let node = self.nodes.entry(id).or_default();
            if node.flags & UNINTERESTING != 0 {
                self.mark_parents(id);
                slop = match self.list.peek() {
                    None => 0,
                    Some(next) if date <= next.date => SLOP,
                    Some(_) if self.interesting > 0 => SLOP,
                    Some(_) => slop - 1,
                };
                if slop == 0 {
                    break;
                }
                continue;
            }
            date = node.date;
            shown.push(id);
        }
        // A commit shown and then found hidden is left out at the end.
        shown.retain(|id| {
            self.nodes
                .get(id)
                .is_some_and(|n| n.flags & UNINTERESTING == 0)
        });
        Ok(shown)
    }
}

/// What `git log` reads of a commit.
struct Commit<'a> {
    author: Option<Ident<'a>>,
    /// The committer's date: what the walk orders by. `0` where unreadable.
    committed: i64,
    encoding: bool,
    message: &'a [u8],
}

struct Ident<'a> {
    name: &'a [u8],
    date: i64,
}

impl<'a> Commit<'a> {
    fn of(data: &'a [u8]) -> Self {
        let mut commit = Commit {
            author: None,
            committed: 0,
            encoding: false,
            message: b"",
        };
        let mut rest = data;
        loop {
            let end = rest.iter().position(|b| *b == b'\n');
            let line = &rest[..end.unwrap_or(rest.len())];
            let next = end.map_or(&rest[rest.len()..], |at| &rest[at + 1..]);
            if line.is_empty() {
                commit.message = next;
                break;
            }
            if let Some(author) = line.strip_prefix(b"author ") {
                if commit.author.is_none() {
                    commit.author = ident(author);
                }
            } else if let Some(committer) = line.strip_prefix(b"committer ") {
                commit.committed = committer_date(committer);
            } else if line.starts_with(b"encoding ") {
                commit.encoding = true;
            }
            if end.is_none() {
                break;
            }
            rest = next;
        }
        commit
    }
}

/// git's `split_ident_line`: the name up to the blanks before `<`, and the
/// date after `>`. `None` where the line has no `<...>`.
fn ident(line: &[u8]) -> Option<Ident<'_>> {
    let open = line.iter().position(|b| *b == b'<')?;
    let close = open + line[open..].iter().position(|b| *b == b'>')?;
    let mut end = open;
    while end > 0 && is_space(line[end - 1]) {
        end -= 1;
    }
    let after = &line[close + 1..];
    let digits: Vec<u8> = after
        .iter()
        .copied()
        .skip_while(|b| is_space(*b))
        .take_while(u8::is_ascii_digit)
        .collect();
    let date = std::str::from_utf8(&digits)
        .ok()
        .and_then(|d| d.parse::<i64>().ok())
        .unwrap_or(0);
    Some(Ident {
        name: &line[..end],
        date,
    })
}

/// git's `parse_commit_date`: the number after the committer's last `>`.
fn committer_date(line: &[u8]) -> i64 {
    let Some(close) = line.iter().rposition(|b| *b == b'>') else {
        return 0;
    };
    let digits: Vec<u8> = line[close + 1..]
        .iter()
        .copied()
        .skip_while(|b| *b == b' ')
        .take_while(u8::is_ascii_digit)
        .collect();
    std::str::from_utf8(&digits)
        .ok()
        .and_then(|d| d.parse::<i64>().ok())
        .unwrap_or(0)
}

/// git's `isspace`: a blank, a tab, a newline or a carriage return.
pub(crate) fn is_space(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\n' | b'\r')
}

/// `%s`: the message's first paragraph, its lines joined by one blank
/// (`pretty.c`'s `format_subject` after `skip_blank_lines`).
fn subject(message: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut started = false;
    for line in message.split_inclusive(|b| *b == b'\n') {
        let mut len = line.len();
        while len > 0 && is_space(line[len - 1]) {
            len -= 1;
        }
        if len == 0 {
            if started {
                break;
            }
            continue;
        }
        if started {
            out.push(b' ');
        }
        started = true;
        out.extend_from_slice(&line[..len]);
    }
    out
}

/// `%ar`: git's `show_date_relative`.
fn relative(time: i64, now: i64) -> String {
    if now < time {
        return "in the future".to_owned();
    }
    let plural =
        |n: i64, one: &str, many: &str| format!("{n} {} ago", if n == 1 { one } else { many });
    let mut diff = now - time;
    if diff < 90 {
        return plural(diff, "second", "seconds");
    }
    diff = (diff + 30) / 60;
    if diff < 90 {
        return plural(diff, "minute", "minutes");
    }
    diff = (diff + 30) / 60;
    if diff < 36 {
        return plural(diff, "hour", "hours");
    }
    diff = (diff + 12) / 24;
    if diff < 14 {
        return plural(diff, "day", "days");
    }
    if diff < 70 {
        return plural((diff + 3) / 7, "week", "weeks");
    }
    if diff < 365 {
        return plural((diff + 15) / 30, "month", "months");
    }
    if diff < 1825 {
        let total = (diff * 12 * 2 + 365) / (365 * 2);
        let (years, months) = (total / 12, total % 12);
        if months == 0 {
            return plural(years, "year", "years");
        }
        return format!(
            "{years} {}, {months} {} ago",
            if years == 1 { "year" } else { "years" },
            if months == 1 { "month" } else { "months" }
        );
    }
    plural((diff + 183) / 365, "year", "years")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The steps of git's relative date, at each of its edges.
    #[test]
    fn a_relative_date_reads_as_gits() {
        let ago = |seconds: i64| relative(1_000_000_000 - seconds, 1_000_000_000);
        assert_eq!(ago(-5), "in the future");
        assert_eq!(ago(1), "1 second ago");
        assert_eq!(ago(89), "89 seconds ago");
        assert_eq!(ago(90), "2 minutes ago");
        assert_eq!(ago(60 * 89), "89 minutes ago");
        assert_eq!(ago(60 * 90), "2 hours ago");
        assert_eq!(ago(3600 * 35), "35 hours ago");
        assert_eq!(ago(3600 * 36), "2 days ago");
        assert_eq!(ago(86400 * 13), "13 days ago");
        assert_eq!(ago(86400 * 14), "2 weeks ago");
        assert_eq!(ago(86400 * 69), "10 weeks ago");
        assert_eq!(ago(86400 * 70), "2 months ago");
        assert_eq!(ago(86400 * 364), "12 months ago");
        assert_eq!(ago(86400 * 365), "1 year ago");
        assert_eq!(ago(86400 * 400), "1 year, 1 month ago");
        assert_eq!(ago(86400 * 800), "2 years, 2 months ago");
        assert_eq!(ago(86400 * 1825), "5 years ago");
        assert_eq!(ago(86400 * 4000), "11 years ago");
    }

    /// The subject is the first paragraph on one line, blank lines before it
    /// skipped and each line's trailing blanks dropped.
    #[test]
    fn a_subject_is_the_first_paragraph_on_one_line() {
        assert_eq!(subject(b"\n\n  one  \ntwo\t\n\nbody\n"), b"  one two");
        assert_eq!(subject(b"only"), b"only");
        assert_eq!(subject(b""), b"");
        assert_eq!(subject(b"a\r\nb\r\n\r\nc"), b"a b");
    }

    /// A name ends before the blanks in front of its address; a line with
    /// no address has no name.
    #[test]
    fn an_author_is_split_as_git_splits_it() {
        let author =
            ident(b"Mara  Okafor  <mara@example.test> 1700000000 +0100").expect("an ident");
        assert_eq!(author.name, b"Mara  Okafor");
        assert_eq!(author.date, 1_700_000_000);
        assert!(ident(b"nobody 1700000000 +0000").is_none());
        assert_eq!(committer_date(b"A <a@b> 12 +0000"), 12);
        assert_eq!(committer_date(b"A a@b 12 +0000"), 0);
    }
}
