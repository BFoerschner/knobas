//! One run's lines as knobas keeps them (section 19): every line up to
//! 200,000 lines and 16 MiB, then the first 2,000 and the newest, the middle
//! gone and counted. A line keeps its number however many before it went.

use std::collections::{BTreeSet, VecDeque};

use super::lines::Line;

/// What one run keeps.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub lines: usize,
    pub bytes: usize,
    /// The first lines, kept whatever follows.
    pub head: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            lines: 200_000,
            bytes: 16 * 1024 * 1024,
            head: 2_000,
        }
    }
}

/// A run's lines.
#[derive(Debug, Default)]
pub struct Kept {
    head: Vec<Line>,
    tail: VecDeque<Line>,
    /// Lines between the head and the tail no longer kept.
    dropped: u32,
    next: u32,
    bytes: usize,
    limits: Limits,
    /// Lines below `sent` that changed since the last batch.
    dirty: BTreeSet<u32>,
    sent: u32,
}

impl Kept {
    #[must_use]
    pub fn new(limits: Limits) -> Self {
        Self {
            limits,
            ..Self::default()
        }
    }

    /// The number the next line gets.
    #[must_use]
    pub const fn next(&self) -> u32 {
        self.next
    }

    /// How many lines the run printed, kept or not.
    #[must_use]
    pub const fn total(&self) -> u32 {
        self.next
    }

    /// How many lines in the middle are not kept.
    #[must_use]
    pub const fn dropped(&self) -> u32 {
        self.dropped
    }

    /// The first line number after the gap: the tail's first.
    #[must_use]
    pub fn tail_from(&self) -> u32 {
        u32::try_from(self.head.len()).unwrap_or(u32::MAX) + self.dropped
    }

    /// The bytes the kept lines weigh.
    #[must_use]
    pub const fn bytes(&self) -> usize {
        self.bytes
    }

    #[must_use]
    pub fn kept(&self) -> usize {
        self.head.len() + self.tail.len()
    }

    /// Adds `line`, answering its number; drops from the middle past the
    /// limits.
    pub fn push(&mut self, mut line: Line) -> u32 {
        let n = self.next;
        line.n = n;
        self.next += 1;
        self.bytes += line.weight();
        if self.tail.is_empty() && self.dropped == 0 && self.head.len() < self.limits.head {
            self.head.push(line);
        } else {
            self.tail.push_back(line);
        }
        self.trim();
        n
    }

    fn trim(&mut self) {
        while (self.kept() > self.limits.lines || self.bytes > self.limits.bytes)
            && self.tail.len() > 1
        {
            if let Some(gone) = self.tail.pop_front() {
                self.bytes = self.bytes.saturating_sub(gone.weight());
                self.dirty.remove(&gone.n);
                self.dropped += 1;
            }
        }
    }

    /// Line `n`, if it is kept.
    #[must_use]
    pub fn get(&self, n: u32) -> Option<&Line> {
        let head = u32::try_from(self.head.len()).unwrap_or(u32::MAX);
        if n < head {
            return self.head.get(n as usize);
        }
        let from = self.tail_from();
        if n < from {
            return None;
        }
        self.tail.get((n - from) as usize)
    }

    pub fn get_mut(&mut self, n: u32) -> Option<&mut Line> {
        let head = u32::try_from(self.head.len()).unwrap_or(u32::MAX);
        if n < head {
            return self.head.get_mut(n as usize);
        }
        let from = self.tail_from();
        if n < from {
            return None;
        }
        self.tail.get_mut((n - from) as usize)
    }

    /// Line `n` changed: sent again with the next batch if it was sent.
    pub fn touch(&mut self, n: u32) {
        if n < self.sent {
            self.dirty.insert(n);
        }
    }

    /// A line's weight changed from `before` to `after`.
    pub fn reweigh(&mut self, before: usize, after: usize) {
        self.bytes = self.bytes.saturating_sub(before) + after;
        self.trim();
    }

    /// Line `n` ended: nothing to do but mark it changed.
    pub fn ended(&mut self, n: u32) {
        self.touch(n);
    }

    /// The lines not yet sent, at most `max_lines` and about `max_bytes`, and
    /// the lines sent before that changed since.
    pub fn take(&mut self, max_lines: usize, max_bytes: usize) -> (Vec<Line>, Vec<Line>) {
        let changed: Vec<Line> = std::mem::take(&mut self.dirty)
            .into_iter()
            .filter_map(|n| self.get(n).cloned())
            .collect();
        let mut new = Vec::new();
        let mut bytes = 0;
        let mut n = self.sent;
        while n < self.next && new.len() < max_lines && bytes < max_bytes {
            match self.get(n) {
                Some(line) => {
                    bytes += line.weight();
                    new.push(line.clone());
                    n += 1;
                }
                None => n = self.tail_from().max(n + 1),
            }
        }
        self.sent = n;
        (new, changed)
    }

    /// Whether lines wait to be sent.
    #[must_use]
    pub fn behind(&self) -> bool {
        self.sent < self.next || !self.dirty.is_empty()
    }

    /// The kept lines numbered `from` to `from + count`, in order.
    #[must_use]
    pub fn range(&self, from: u32, count: u32) -> Vec<Line> {
        let end = from.saturating_add(count).min(self.next);
        let mut out = Vec::new();
        let mut n = from;
        while n < end {
            match self.get(n) {
                Some(line) => {
                    out.push(line.clone());
                    n += 1;
                }
                None => n = self.tail_from().max(n + 1),
            }
        }
        out
    }

    /// Every kept line, for a copy or a save.
    pub fn all(&self) -> impl Iterator<Item = &Line> {
        self.head.iter().chain(self.tail.iter())
    }

    /// Lets go of the lines, keeping the counts: a run that lost its lines
    /// to the memory cap.
    pub fn forget(&mut self) {
        self.head = Vec::new();
        self.tail = VecDeque::new();
        self.dirty.clear();
        self.dropped = self.next;
        self.bytes = 0;
        self.sent = self.next;
    }
}

#[cfg(test)]
mod tests {
    use super::super::lines::{Line, Stream};
    use super::*;

    fn line(text: &str) -> Line {
        let mut kept = Kept::default();
        let mut reader = super::super::lines::Reader::default();
        reader.write(&mut kept, Stream::Out, format!("{text}\n").as_bytes());
        kept.get(0).unwrap().clone()
    }

    #[test]
    fn past_the_limit_the_middle_goes_and_numbers_stay() {
        let mut kept = Kept::new(Limits {
            lines: 10,
            bytes: usize::MAX,
            head: 3,
        });
        for i in 0..25 {
            kept.push(line(&format!("l{i}")));
        }
        assert_eq!(kept.total(), 25);
        assert_eq!(kept.kept(), 10);
        assert_eq!(kept.dropped(), 15);
        assert_eq!(kept.get(2).unwrap().text, "l2");
        assert!(kept.get(3).is_none());
        assert!(kept.get(17).is_none());
        assert_eq!(kept.get(18).unwrap().text, "l18");
        assert_eq!(kept.get(24).unwrap().n, 24);
        let range: Vec<u32> = kept.range(0, 25).iter().map(|l| l.n).collect();
        assert_eq!(range, [0, 1, 2, 18, 19, 20, 21, 22, 23, 24]);
    }

    #[test]
    fn the_byte_cap_drops_from_the_middle_too() {
        let one = line("x").weight();
        let mut kept = Kept::new(Limits {
            lines: usize::MAX,
            bytes: one * 5,
            head: 2,
        });
        for _ in 0..9 {
            kept.push(line("x"));
        }
        assert_eq!(kept.kept(), 5);
        assert!(kept.bytes() <= one * 5);
    }

    #[test]
    fn a_batch_takes_the_new_lines_then_the_changed() {
        let mut kept = Kept::default();
        for i in 0..5 {
            kept.push(line(&format!("l{i}")));
        }
        let (new, changed) = kept.take(3, usize::MAX);
        assert_eq!(new.iter().map(|l| l.n).collect::<Vec<_>>(), [0, 1, 2]);
        assert!(changed.is_empty());
        kept.touch(1);
        kept.touch(4);
        let (new, changed) = kept.take(10, usize::MAX);
        assert_eq!(new.iter().map(|l| l.n).collect::<Vec<_>>(), [3, 4]);
        assert_eq!(changed.iter().map(|l| l.n).collect::<Vec<_>>(), [1]);
        assert!(!kept.behind());
    }

    #[test]
    fn lines_dropped_before_they_were_sent_are_skipped() {
        let mut kept = Kept::new(Limits {
            lines: 4,
            bytes: usize::MAX,
            head: 2,
        });
        for i in 0..10 {
            kept.push(line(&format!("l{i}")));
        }
        let (new, _) = kept.take(100, usize::MAX);
        assert_eq!(new.iter().map(|l| l.n).collect::<Vec<_>>(), [0, 1, 8, 9]);
    }
}
