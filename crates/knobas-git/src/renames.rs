//! Which deleted file an added one is a rename of, as `git diff -M` pairs
//! them (`gitdiffcore(7)`, *diffcore-rename*): first by identical content,
//! then by name where one deleted and one added file share a file name,
//! then every remaining pair by how much of the one's content is in the
//! other, the best pairs first.
//!
//! The answer has to be git's own, digit for digit: a list says `R087` and a
//! patch `similarity index 87%`. So the measure is git's: the text cut into
//! pieces at each newline or 64 bytes, each piece hashed, and the bytes of
//! the pieces both sides hold counted against the larger side.

use std::collections::HashMap;

use gix::hash::ObjectId;

/// git's `MAX_SCORE`: a similarity of 100 %.
pub(crate) const MAX_SCORE: u32 = 60_000;
/// `-M` with no number: half alike.
const MINIMUM: u32 = MAX_SCORE / 2;
/// How far an added file is looked for among the deleted ones before the
/// inexact step is given up (`diff.renameLimit`'s default).
pub(crate) const RENAME_LIMIT: u64 = 1000;
/// Each added file keeps its four best candidates (`NUM_CANDIDATE_PER_DST`).
const CANDIDATES: usize = 4;
const HASHBASE: u32 = 107_927;

/// One side of a rename candidate.
pub(crate) struct File<'a> {
    pub path: &'a [u8],
    pub mode: u32,
    pub oid: ObjectId,
}

/// What rename detection reads of a file's content.
pub(crate) trait Contents {
    /// The file's size in bytes, `None` where it cannot be read.
    fn size(&mut self, file: &File<'_>) -> Option<u64>;
    /// Its bytes and whether git takes it for binary.
    fn data(&mut self, file: &File<'_>) -> Option<(std::rc::Rc<Vec<u8>>, bool)>;
}

/// The bytes of each piece of `data`, by the piece's hash, in hash order
/// (`diffcore-delta.c`'s `hash_chars`). In a text, the carriage return of a
/// CRLF is not counted.
fn pieces(data: &[u8], text: bool) -> Vec<(u32, u32)> {
    let mut counts: HashMap<u32, u32> = HashMap::new();
    let (mut accum1, mut accum2, mut n) = (0u32, 0u32, 0u32);
    let mut at = 0;
    while at < data.len() {
        let c = u32::from(data[at]);
        at += 1;
        if text && c == u32::from(b'\r') && data.get(at) == Some(&b'\n') {
            continue;
        }
        let old = accum1;
        accum1 = (accum1 << 7) ^ (accum2 >> 25);
        accum2 = (accum2 << 7) ^ (old >> 25);
        accum1 = accum1.wrapping_add(c);
        n += 1;
        if n < 64 && c != u32::from(b'\n') {
            continue;
        }
        let hash = accum1.wrapping_add(accum2.wrapping_mul(0x61)) % HASHBASE;
        *counts.entry(hash).or_default() += n;
        (accum1, accum2, n) = (0, 0, 0);
    }
    if n > 0 {
        let hash = accum1.wrapping_add(accum2.wrapping_mul(0x61)) % HASHBASE;
        *counts.entry(hash).or_default() += n;
    }
    let mut all: Vec<(u32, u32)> = counts.into_iter().collect();
    all.sort_unstable();
    all
}

/// How many bytes of the source are in the destination
/// (`diffcore_count_changes`'s `src_copied`).
fn copied(src: &[(u32, u32)], dst: &[(u32, u32)]) -> u64 {
    let mut total = 0u64;
    let mut d = 0;
    for &(hash, src_count) in src {
        while d < dst.len() && dst[d].0 < hash {
            d += 1;
        }
        let dst_count = if d < dst.len() && dst[d].0 == hash {
            d += 1;
            dst[d - 1].1
        } else {
            0
        };
        total += u64::from(src_count.min(dst_count));
    }
    total
}

/// One file's pieces, counted once.
#[derive(Default)]
struct Counted {
    pieces: HashMap<ObjectId, Option<std::rc::Rc<Vec<(u32, u32)>>>>,
}

impl Counted {
    fn of(
        &mut self,
        file: &File<'_>,
        contents: &mut dyn Contents,
    ) -> Option<std::rc::Rc<Vec<(u32, u32)>>> {
        if let Some(known) = self.pieces.get(&file.oid) {
            return known.clone();
        }
        let counted = contents
            .data(file)
            .map(|(data, binary)| std::rc::Rc::new(pieces(&data, !binary)));
        self.pieces.insert(file.oid, counted.clone());
        counted
    }
}

fn regular(mode: u32) -> bool {
    mode & 0o170_000 == 0o100_000
}

fn basename(path: &[u8]) -> &[u8] {
    path.rsplit(|b| *b == b'/').next().unwrap_or(path)
}

/// git's `estimate_similarity`: `0` to [`MAX_SCORE`].
fn similarity(
    src: &File<'_>,
    dst: &File<'_>,
    minimum: u32,
    contents: &mut dyn Contents,
    counted: &mut Counted,
) -> u32 {
    // Only regular files are compared; a link is a rename only where its
    // content is the same.
    if !regular(src.mode) || !regular(dst.mode) {
        return 0;
    }
    let (Some(src_size), Some(dst_size)) = (contents.size(src), contents.size(dst)) else {
        return 0;
    };
    let (max, base) = (src_size.max(dst_size), src_size.min(dst_size));
    // A size that changed too much to reach the minimum is not read.
    if max * u64::from(MAX_SCORE - minimum) < (max - base) * u64::from(MAX_SCORE) {
        return 0;
    }
    let (Some(a), Some(b)) = (counted.of(src, contents), counted.of(dst, contents)) else {
        return 0;
    };
    if dst_size == 0 {
        return 0;
    }
    u32::try_from(copied(&a, &b) * u64::from(MAX_SCORE) / max).unwrap_or(MAX_SCORE)
}

/// For each of `added`, the deleted file it is a rename of and the score:
/// `(place in added, place in deleted, score)`, at most one per added and
/// one per deleted file. `limit` is `diff.renameLimit` (`0`: none).
pub(crate) fn pair(
    deleted: &[File<'_>],
    added: &[File<'_>],
    limit: u64,
    contents: &mut dyn Contents,
) -> Vec<(usize, usize, u32)> {
    let mut of_added: Vec<Option<(usize, u32)>> = vec![None; added.len()];
    let mut used = vec![false; deleted.len()];

    // 1. The same content: the unused source of that content with the same
    //    file name, else the first unused one, looking at 100 at most.
    let mut by_content: HashMap<ObjectId, Vec<usize>> = HashMap::new();
    for (at, file) in deleted.iter().enumerate() {
        by_content.entry(file.oid).or_default().push(at);
    }
    for (d, target) in added.iter().enumerate() {
        let Some(sources) = by_content.get(&target.oid) else {
            continue;
        };
        let mut best: Option<(usize, i32)> = None;
        let mut looks = 100;
        for &s in sources {
            let source = &deleted[s];
            // Other than regular files only with the very same mode.
            if (!regular(source.mode) || !regular(target.mode)) && source.mode != target.mode {
                continue;
            }
            if used[s] {
                continue;
            }
            let score = 1 + i32::from(basename(source.path) == basename(target.path));
            if best.is_none_or(|(_, held)| score > held) {
                best = Some((s, score));
                if score == 2 {
                    break;
                }
            }
            looks -= 1;
            if looks == 0 {
                break;
            }
        }
        if let Some((s, _)) = best {
            of_added[d] = Some((s, MAX_SCORE));
            used[s] = true;
        }
    }

    let mut counted = Counted::default();
    let left = |of_added: &[Option<(usize, u32)>], used: &[bool]| {
        (
            of_added.iter().filter(|p| p.is_none()).count() as u64,
            used.iter().filter(|u| !**u).count() as u64,
        )
    };
    let (destinations, sources) = left(&of_added, &used);
    if destinations == 0 || sources == 0 {
        return paired(&of_added);
    }

    // 2. The same file name, once on each side: a rename where the two are
    //    three quarters alike.
    let minimum_by_name = MINIMUM + (MAX_SCORE - MINIMUM) / 2;
    let unique = |files: &[File<'_>], skip: &dyn Fn(usize) -> bool| {
        let mut names: HashMap<Vec<u8>, Option<usize>> = HashMap::new();
        for (at, file) in files.iter().enumerate() {
            if skip(at) {
                continue;
            }
            names
                .entry(basename(file.path).to_vec())
                .and_modify(|held| *held = None)
                .or_insert(Some(at));
        }
        names
    };
    let source_names = unique(deleted, &|at| used[at]);
    let target_names = unique(added, &|at| of_added[at].is_some());
    for (s, source) in deleted.iter().enumerate() {
        if used[s] {
            continue;
        }
        let name = basename(source.path);
        let (Some(Some(held)), Some(Some(d))) = (source_names.get(name), target_names.get(name))
        else {
            continue;
        };
        if *held != s || of_added[*d].is_some() {
            continue;
        }
        let score = similarity(source, &added[*d], minimum_by_name, contents, &mut counted);
        if score < minimum_by_name {
            continue;
        }
        of_added[*d] = Some((s, score));
        used[s] = true;
    }

    // 3. Every added file left against every deleted file left, unless
    //    that is more pairs than the limit squared.
    let (destinations, sources) = left(&of_added, &used);
    if destinations == 0 || sources == 0 {
        return paired(&of_added);
    }
    if limit > 0 && destinations * sources > limit * limit {
        return paired(&of_added);
    }
    // (score, same file name, added, deleted), `None` an empty slot.
    type Slot = Option<(u32, bool, usize, usize)>;
    // git's `score_compare`: empty slots last, then the higher score, then
    // the same file name.
    let before = |a: &Slot, b: &Slot| -> std::cmp::Ordering {
        match (a, b) {
            (None, None) => std::cmp::Ordering::Equal,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (Some(_), None) => std::cmp::Ordering::Less,
            (Some(a), Some(b)) => b.0.cmp(&a.0).then(b.1.cmp(&a.1)),
        }
    };
    let mut matrix: Vec<Slot> = Vec::new();
    for (d, target) in added.iter().enumerate() {
        if of_added[d].is_some() {
            continue;
        }
        let mut slots: [Slot; CANDIDATES] = [None; CANDIDATES];
        for (s, source) in deleted.iter().enumerate() {
            if used[s] {
                continue;
            }
            let candidate: Slot = Some((
                similarity(source, target, MINIMUM, contents, &mut counted),
                basename(source.path) == basename(target.path),
                d,
                s,
            ));
            // The worst slot gives way to a better candidate.
            let mut worst = 0;
            for i in 1..CANDIDATES {
                if before(&slots[i], &slots[worst]) == std::cmp::Ordering::Greater {
                    worst = i;
                }
            }
            if before(&slots[worst], &candidate) == std::cmp::Ordering::Greater {
                slots[worst] = candidate;
            }
        }
        matrix.extend_from_slice(&slots);
    }
    matrix.sort_by(before);
    for slot in matrix {
        let Some((score, _, d, s)) = slot else {
            break;
        };
        if score < MINIMUM {
            break;
        }
        if of_added[d].is_some() || used[s] {
            continue;
        }
        of_added[d] = Some((s, score));
        used[s] = true;
    }
    paired(&of_added)
}

fn paired(of_added: &[Option<(usize, u32)>]) -> Vec<(usize, usize, u32)> {
    of_added
        .iter()
        .enumerate()
        .filter_map(|(d, pair)| pair.map(|(s, score)| (d, s, score)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A piece ends at a newline or at 64 bytes, and the carriage return of
    /// a text's CRLF is not counted.
    #[test]
    fn pieces_are_lines_or_sixty_four_bytes() {
        let total = |p: &[(u32, u32)]| p.iter().map(|(_, n)| n).sum::<u32>();
        assert_eq!(total(&pieces(b"one\ntwo\n", true)), 8);
        assert_eq!(total(&pieces(b"one\r\ntwo\r\n", true)), 8);
        assert_eq!(total(&pieces(b"one\r\ntwo\r\n", false)), 10);
        assert_eq!(
            pieces(&[b'x'; 130], true)
                .iter()
                .map(|(_, n)| *n)
                .sum::<u32>(),
            130
        );
        assert_eq!(pieces(b"same\nsame\n", true), pieces(b"same\nsame\n", true));
        assert_eq!(
            copied(&pieces(b"a\nb\nc\n", true), &pieces(b"a\nc\nd\n", true)),
            4
        );
    }
}
