//! The reads (D8): each term's files off the term dictionary by hand, a bit a
//! file of each segment, every term required, counted by repository.
//!
//! Not through a tantivy query: a prefix query has no bound and no way to
//! stop, and `test` begins 179,636 word lists of the measured corpus. Here a
//! term of four characters and more is read as every word it begins, until
//! [`crate::BOUND`] lists would be read across the field's segments; past
//! that, and for a term of three characters, it is read whole.

use std::collections::{HashMap, HashSet};

use knobas_core::code_text::{self, Query, TERM_WHOLE};
use tantivy::postings::TermInfo;
use tantivy::schema::{Field, IndexRecordOption, Value};
use tantivy::{
    DocAddress, DocId, DocSet, InvertedIndexReader, Searcher, SegmentOrdinal, TERMINATED,
    TantivyDocument,
};

use crate::{Fields, FileHit, FilesError, InRepo, Reads, Repos};

/// A bit a file of each segment.
type Bits = Vec<Vec<u64>>;

fn empty(searcher: &Searcher) -> Bits {
    searcher
        .segment_readers()
        .iter()
        .map(|segment| vec![0_u64; usize::try_from(segment.max_doc()).unwrap_or(0).div_ceil(64)])
        .collect()
}

fn set(
    inverted: &InvertedIndexReader,
    info: &TermInfo,
    bits: &mut [u64],
) -> Result<(), FilesError> {
    let mut postings = inverted.read_postings_from_terminfo(info, IndexRecordOption::Basic)?;
    let mut doc = postings.doc();
    while doc != TERMINATED {
        bits[usize::try_from(doc / 64).unwrap_or(usize::MAX)] |= 1_u64 << (doc % 64);
        doc = postings.advance();
    }
    Ok(())
}

/// The files whose `field` holds a word `term` begins (four characters and
/// more, within `bound` lists over every segment) or is (three, or past the
/// bound).
fn files_of(searcher: &Searcher, field: Field, term: &str, bound: u32) -> Result<Bits, FilesError> {
    let segments = searcher.segment_readers();
    if term.chars().count() > TERM_WHOLE {
        let mut left = bound;
        let mut bits = empty(searcher);
        let mut within = true;
        let mut upper = term.as_bytes().to_vec();
        upper.push(0xff);
        'segments: for (segment, bits) in segments.iter().zip(bits.iter_mut()) {
            let inverted = segment.inverted_index(field)?;
            let mut stream = inverted
                .terms()
                .range()
                .ge(term.as_bytes())
                .lt(&upper)
                .into_stream()?;
            while stream.advance() {
                if left == 0 {
                    within = false;
                    break 'segments;
                }
                left -= 1;
                set(&inverted, stream.value(), bits)?;
            }
        }
        if within {
            return Ok(bits);
        }
    }
    let mut bits = empty(searcher);
    for (segment, bits) in segments.iter().zip(bits.iter_mut()) {
        let inverted = segment.inverted_index(field)?;
        if let Some(info) = inverted.terms().get(term.as_bytes())? {
            set(&inverted, &info, bits)?;
        }
    }
    Ok(bits)
}

/// The files `field` holds every term in.
fn matched(
    searcher: &Searcher,
    field: Field,
    terms: &[String],
    bound: u32,
) -> Result<Bits, FilesError> {
    let mut all: Option<Bits> = None;
    for term in terms {
        let of_term = files_of(searcher, field, term, bound)?;
        all = Some(match all {
            None => of_term,
            Some(mut held) => {
                for (held, of_term) in held.iter_mut().zip(&of_term) {
                    for (word, other) in held.iter_mut().zip(of_term) {
                        *word &= *other;
                    }
                }
                held
            }
        });
    }
    Ok(all.unwrap_or_else(|| empty(searcher)))
}

/// A file: its path's length, then where it is, which orders a repository's
/// files shortest path first.
type At = (u64, SegmentOrdinal, DocId);

/// Of one repository: how many files matched, and the first `keep` of them.
#[derive(Default)]
struct Found {
    files: u32,
    first: Vec<At>,
}

impl Found {
    /// Count `at`, and keep it if it is among the first `keep`: unordered
    /// until [`by_repo`] sorts them once.
    fn add(&mut self, at: At, keep: usize) {
        self.files += 1;
        if self.first.len() < keep {
            self.first.push(at);
        } else if let Some((index, last)) =
            self.first.iter().enumerate().max_by_key(|(_, kept)| **kept)
            && at < *last
        {
            self.first[index] = at;
        }
    }
}

/// Every live file set in `bits`, by repository: each repository's count
/// and its first `keep` files. Only repository `only`'s where it is given.
fn by_repo(
    searcher: &Searcher,
    bits: &Bits,
    keep: usize,
    only: Option<u64>,
) -> Result<HashMap<u64, Found>, FilesError> {
    let mut found: HashMap<u64, Found> = HashMap::new();
    for (ord, (segment, bits)) in searcher.segment_readers().iter().zip(bits).enumerate() {
        let ord = SegmentOrdinal::try_from(ord).unwrap_or(SegmentOrdinal::MAX);
        let repo = segment.fast_fields().u64("repo")?;
        let plen = segment.fast_fields().u64("plen")?;
        let alive = segment.alive_bitset();
        for (at, word) in bits.iter().enumerate() {
            let mut word = *word;
            while word != 0 {
                let doc = DocId::try_from(at).unwrap_or(DocId::MAX) * 64 + word.trailing_zeros();
                word &= word - 1;
                if alive.is_some_and(|alive| !alive.is_alive(doc)) {
                    continue;
                }
                let no = repo.first(doc).unwrap_or(u64::MAX);
                if only.is_some_and(|only| only != no) {
                    continue;
                }
                found
                    .entry(no)
                    .or_default()
                    .add((plen.first(doc).unwrap_or(u64::MAX), ord, doc), keep);
            }
        }
    }
    for found in found.values_mut() {
        found.first.sort_unstable();
    }
    Ok(found)
}

/// The files the index holds of each repository.
pub(crate) fn held(searcher: &Searcher) -> Result<HashMap<u64, u32>, FilesError> {
    let mut held: HashMap<u64, u32> = HashMap::new();
    for segment in searcher.segment_readers() {
        let repo = segment.fast_fields().u64("repo")?;
        let alive = segment.alive_bitset();
        for doc in 0..segment.max_doc() {
            if alive.is_some_and(|alive| !alive.is_alive(doc)) {
                continue;
            }
            *held.entry(repo.first(doc).unwrap_or(u64::MAX)).or_default() += 1;
        }
    }
    Ok(held)
}

/// A file as shown: its path, whether the path found it, and its line where
/// its text is read and holds the words. `None` where the query has a phrase
/// and no line of the file holds it (D8 rule 11).
fn shown(
    searcher: &Searcher,
    fields: &Fields,
    at: At,
    in_name: bool,
    asked: &Query,
    reads: Reads,
) -> Result<Option<FileHit>, FilesError> {
    let (_, ord, doc) = at;
    let stored: TantivyDocument = searcher.doc(DocAddress::new(ord, doc))?;
    let path = stored
        .get_first(fields.path)
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .to_owned();
    let line = if reads.text {
        stored
            .get_first(fields.text)
            .and_then(|value| value.as_str())
            .and_then(|text| code_text::best_line(text, asked))
    } else {
        None
    };
    if !asked.phrases.is_empty() && line.is_none() {
        return Ok(None);
    }
    Ok(Some(FileHit {
        path,
        in_name,
        line,
    }))
}

/// How many files of a repository a phrase's line is looked for in.
const PHRASE_FILES: usize = 8;

/// [`crate::FileIndex::ask`].
#[allow(clippy::too_many_arguments)]
pub(crate) fn ask(
    searcher: &Searcher,
    fields: &Fields,
    asked: &Query,
    reads: Reads,
    most: usize,
    bound: u32,
    repos: &Repos,
) -> Result<Vec<InRepo>, FilesError> {
    let keep = if asked.phrases.is_empty() {
        1
    } else {
        PHRASE_FILES
    };
    let by_text = if reads.text {
        by_repo(
            searcher,
            &matched(searcher, fields.body, &asked.terms, bound)?,
            keep,
            None,
        )?
    } else {
        HashMap::new()
    };
    let by_name = if reads.names {
        by_repo(
            searcher,
            &matched(searcher, fields.ptok, &asked.terms, bound)?,
            keep,
            None,
        )?
    } else {
        HashMap::new()
    };
    let mut nos: Vec<u64> = by_name
        .keys()
        .chain(by_text.keys())
        .copied()
        .collect::<HashSet<u64>>()
        .into_iter()
        .filter(|no| repos.contains_key(no))
        .collect();
    let count =
        |found: &HashMap<u64, Found>, no: &u64| found.get(no).map_or(0, |found| found.files);
    // D9: named by its files first, more of them first (none is last); then
    // more files by text; a favourite; the newer; the number, so the order is
    // one.
    nos.sort_by(|a, b| {
        let (ra, rb) = (&repos[a], &repos[b]);
        count(&by_name, b)
            .cmp(&count(&by_name, a))
            .then_with(|| count(&by_text, b).cmp(&count(&by_text, a)))
            .then_with(|| rb.favourite.cmp(&ra.favourite))
            .then_with(|| rb.updated.cmp(&ra.updated))
            .then_with(|| a.cmp(b))
    });
    let mut out = Vec::with_capacity(most.min(nos.len()));
    for no in nos {
        if out.len() >= most {
            break;
        }
        let named = by_name
            .get(&no)
            .map(|found| found.first.as_slice())
            .unwrap_or_default();
        let texted = by_text
            .get(&no)
            .map(|found| found.first.as_slice())
            .unwrap_or_default();
        let candidates = named.iter().map(|at| (*at, true)).chain(
            texted
                .iter()
                .filter(|at| !named.contains(at))
                .map(|at| (*at, false)),
        );
        let mut best = None;
        for (at, in_name) in candidates {
            if let Some(hit) = shown(searcher, fields, at, in_name, asked, reads)? {
                best = Some(hit);
                break;
            }
        }
        if let Some(best) = best {
            out.push(InRepo {
                no,
                by_text: count(&by_text, &no),
                by_name: count(&by_name, &no),
                best,
            });
        }
    }
    Ok(out)
}

/// [`crate::FileIndex::in_repo`].
pub(crate) fn in_repo(
    searcher: &Searcher,
    fields: &Fields,
    no: u64,
    asked: &Query,
    reads: Reads,
    most: usize,
    bound: u32,
) -> Result<crate::InRepoFiles, FilesError> {
    let every = usize::MAX;
    let named: Vec<At> = if reads.names {
        let bits = matched(searcher, fields.ptok, &asked.terms, bound)?;
        by_repo(searcher, &bits, every, Some(no))?
            .remove(&no)
            .map(|found| found.first)
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    let texted: Vec<At> = if reads.text {
        let bits = matched(searcher, fields.body, &asked.terms, bound)?;
        by_repo(searcher, &bits, every, Some(no))?
            .remove(&no)
            .map(|found| found.first)
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    let named_set: HashSet<At> = named.iter().copied().collect();
    let mut files: Vec<(At, bool)> = named.iter().map(|at| (*at, true)).collect();
    files.extend(
        texted
            .iter()
            .filter(|at| !named_set.contains(at))
            .map(|at| (*at, false)),
    );
    let total = u32::try_from(files.len()).unwrap_or(u32::MAX);
    let by_name = u32::try_from(named.len()).unwrap_or(u32::MAX);
    let by_text = u32::try_from(texted.len()).unwrap_or(u32::MAX);
    let mut out = Vec::with_capacity(most.min(files.len()));
    for (at, in_name) in files {
        if out.len() >= most {
            break;
        }
        if let Some(hit) = shown(searcher, fields, at, in_name, asked, reads)? {
            out.push(hit);
        }
    }
    Ok(crate::InRepoFiles {
        files: out,
        by_text,
        by_name,
        total,
    })
}
