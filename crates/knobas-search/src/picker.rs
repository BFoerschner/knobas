//! Names found as a picker finds them (the fuzzy-search decisions,
//! 2026-11-19; `docs/decisions/2026-11-19-names-are-found-as-a-picker-finds-them.md`).
//!
//! PostgreSQL matches a name by its words, and a word only from its start, so
//! `proxy` never found `ADProxy` (one word to the parser, `adproxi`) and
//! `mapping service` never found `abcMappingService`. This module holds every
//! name the mirror has in memory and reads typed text against them with
//! frizbee, the matcher LazyVim's completion (blink.cmp) is built on: the
//! names that hold what was typed in one piece (**solid**), then, from four
//! typed letters, the names that hold its letters in order as an abbreviation
//! does (**loose**), and one typo only where nothing else matched.
//!
//! It is the one place that names a frizbee type. The launcher
//! ([`crate::Searcher::with_picker`]), the Docs search and the page filters'
//! `fuzzy_rank` command all read through it, so there is one set of rules.
//!
//! Pure but for [`Picker::reload`], which reads the names (and, once they are
//! whole, the mirror's words) and swaps them in.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Instant;

use frizbee::{Config, Matcher, Matching, Pattern, PatternConfig};

/// Loose (letters in order) only from this many typed letters, all words
/// together, and only for a word of this many letters and more.
pub const LOOSE_FROM: usize = 4;
/// A word takes one typo from this many letters, and two from
/// [`TWO_TYPOS_FROM`].
pub const TYPO_FROM: usize = 4;
/// A word takes two typos from this many letters.
pub const TWO_TYPOS_FROM: usize = 9;
/// The typo pass runs only when no row is solid and the loose pass kept fewer
/// rows than this, in all.
pub const TYPO_UNDER: usize = 5;
/// A loose row's score is at least this share of the best score its pass
/// gave, solid rows included.
pub const FLOOR: f64 = 0.85;
/// Solid candidates handed on a raw kind.
pub const SOLID_A_KIND: usize = 20;
/// Loose and typo rows a raw kind, the two passes together.
pub const LOOSE_A_KIND: usize = 10;
/// Loose candidates looked at, best first, before a pass stops.
pub const LOOSE_LOOKED_AT: usize = 3_000;

/// How one typed word is matched.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum How {
    /// A plain word: solid in one piece, loose as letters in order.
    Fuzzy,
    /// `'word`, `"a phrase"`, a left-out word, a key: in one piece only.
    Substring,
    /// `^word`: the name's start.
    Prefix,
    /// `word$`: the name's end.
    Suffix,
    /// `^word$`: the whole name.
    Exact,
}

#[derive(Clone, Debug)]
struct Atom {
    needle: String,
    how: How,
    negated: bool,
}

impl Atom {
    fn letters(&self) -> usize {
        self.needle.chars().count()
    }

    /// Read loosely: a plain word of [`LOOSE_FROM`] letters and more.
    fn loose(&self) -> bool {
        self.how == How::Fuzzy && !self.negated && self.letters() >= LOOSE_FROM
    }
}

/// Typed text as the picker reads it (D4): every word on its own, in any
/// order, the grammar's keys already taken out (`ParsedQuery::text`).
#[derive(Clone, Debug)]
pub struct Typed {
    atoms: Vec<Atom>,
    /// The letters of the words not left out.
    letters: usize,
}

/// A token shaped like a key: a ticket's (`PAY-231`), a `#` and digits, or
/// digits alone. Never read loosely (D4 rule 6): `sclpay-4988` found the
/// monitor `scale-payments-staging-49` by a typo.
fn key_shaped(token: &str) -> bool {
    let digits = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit());
    if digits(token) || token.strip_prefix('#').is_some_and(digits) {
        return true;
    }
    token.rsplit_once('-').is_some_and(|(project, number)| {
        digits(number)
            && project
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphabetic())
            && project.chars().all(|c| c.is_ascii_alphanumeric())
    })
}

impl Typed {
    /// `text` read as words, or `None` where the picker answers nothing: a
    /// text with the word `or` (PostgreSQL answers it, D4 rule 7), or one
    /// with no word to look for.
    #[must_use]
    pub fn read(text: &str) -> Option<Self> {
        let mut atoms = Vec::new();
        for token in crate::query::tokenize(text) {
            if token.eq_ignore_ascii_case("or") {
                return None;
            }
            let mut rest = token.as_str();
            let negated = rest.starts_with('-') || rest.starts_with('!');
            if negated {
                rest = &rest[1..];
            }
            let (needle, how) = if rest.starts_with('"') {
                (rest.trim_matches('"').to_owned(), How::Substring)
            } else {
                let prefix = rest.starts_with('^');
                let piece = !prefix && rest.starts_with('\'');
                if prefix || piece {
                    rest = &rest[1..];
                }
                let suffix = rest.len() > 1 && rest.ends_with('$');
                if suffix {
                    rest = &rest[..rest.len() - 1];
                }
                let how = match (prefix, suffix, piece) {
                    (true, true, _) => How::Exact,
                    (true, false, _) => How::Prefix,
                    (false, true, _) => How::Suffix,
                    (false, false, true) => How::Substring,
                    // Fuzzing a negated word leaves out far more than a
                    // person expects: frizbee, fzf and nucleo read it whole.
                    _ if negated => How::Substring,
                    _ if key_shaped(rest) => How::Substring,
                    _ => How::Fuzzy,
                };
                (rest.to_owned(), how)
            };
            if !needle.is_empty() {
                atoms.push(Atom {
                    needle,
                    how,
                    negated,
                });
            }
        }
        let letters = atoms
            .iter()
            .filter(|atom| !atom.negated)
            .map(Atom::letters)
            .sum();
        (letters > 0).then_some(Self { atoms, letters })
    }

    /// The letters typed, the words left out not counted.
    #[must_use]
    pub fn letters(&self) -> usize {
        self.letters
    }

    fn positive(&self) -> impl Iterator<Item = &Atom> {
        self.atoms.iter().filter(|atom| !atom.negated)
    }

    /// The patterns of one pass: each plain word as `fuzzy` says when it is
    /// read loosely and in one piece when not, the operator words as typed.
    fn patterns(&self, fuzzy: Option<Typos>) -> Vec<Pattern> {
        self.atoms.iter().map(|atom| pattern(atom, fuzzy)).collect()
    }

    /// Whether `title` holds every word in one piece (D5's solid), and for a
    /// text of two or three letters each word at a word's start somewhere in
    /// it, unless `anywhere`.
    fn solid_fit(&self, title: &str, anywhere: bool) -> bool {
        anywhere
            || self.letters >= LOOSE_FROM
            || self
                .positive()
                .all(|atom| starts_a_word(title, &atom.needle))
    }

    /// Whether the launcher counts `title` **named** by what was typed: the
    /// solid patterns match it (D6), at a word's start for two or three
    /// letters. Under two letters nothing is.
    #[must_use]
    pub fn names(&self, title: &str) -> bool {
        if self.letters < 2 {
            return false;
        }
        let mut matcher = Matcher::from_patterns(&self.patterns(None), &Config::default());
        matcher.match_one(title, 0).is_some() && self.solid_fit(title, false)
    }

    /// The matched letters of `title` as pairs of start and end in UTF-16
    /// code units, ascending and never overlapping (D9): for a solid row the
    /// solid patterns' places, for a `loose` row each word's letters as the
    /// loose rules placed them (no typo first, then the typos allowed). None
    /// where nothing fits.
    #[must_use]
    pub fn marks(&self, title: &str, loose: bool) -> Vec<u32> {
        let mut at: Vec<u32> = Vec::new();
        if loose {
            for typos in [Typos::None, Typos::Allowed] {
                at.clear();
                let placed = self.positive().all(|atom| {
                    let mut one =
                        Matcher::from_patterns(&[pattern(atom, Some(typos))], &Config::default());
                    one.match_one_indices(title, 0).is_some_and(|found| {
                        at.extend(found.indices);
                        true
                    })
                });
                if placed {
                    break;
                }
                at.clear();
            }
        } else if self.names(title) {
            let mut matcher = Matcher::from_patterns(&self.patterns(None), &Config::default());
            if let Some(found) = matcher.match_one_indices(title, 0) {
                at = found.indices;
            }
        }
        utf16_runs(title, &at)
    }
}

/// Whether a pass reads loose words with typos.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Typos {
    None,
    Allowed,
}

/// The typos a word of `letters` may have where typos are allowed (D5).
fn typos_of(letters: usize) -> u16 {
    if letters < TYPO_FROM {
        0
    } else if letters < TWO_TYPOS_FROM {
        1
    } else {
        2
    }
}

/// One word's pattern: in one piece unless `fuzzy` asks for the loose
/// reading of a word that has one ([`Atom::loose`]).
fn pattern(atom: &Atom, fuzzy: Option<Typos>) -> Pattern {
    let matching = match atom.how {
        How::Fuzzy if fuzzy.is_some() && atom.loose() => Matching::Fuzzy,
        How::Fuzzy | How::Substring => Matching::Substring,
        How::Prefix => Matching::Prefix,
        How::Suffix => Matching::Suffix,
        How::Exact => Matching::Exact,
    };
    let typos = match fuzzy {
        Some(Typos::Allowed) if matching == Matching::Fuzzy => typos_of(atom.letters()),
        _ => 0,
    };
    Pattern::new(
        &atom.needle,
        PatternConfig::default()
            .matching(Some(matching))
            .max_typos(Some(typos)),
    )
    .negated(atom.negated)
}

/// Whether a word starts at byte `at` of `text` (D5): at its first character,
/// after anything that is not a letter or digit, at a capital after a small
/// letter, where a digit meets a letter, and at the last capital of a run
/// before a small letter (`ADProxy`'s `P`).
fn word_start(text: &str, at: usize) -> bool {
    let Some(prev) = text[..at].chars().next_back() else {
        return true;
    };
    let mut after = text[at..].chars();
    let Some(cur) = after.next() else {
        return false;
    };
    if !prev.is_alphanumeric() {
        return true;
    }
    if prev.is_lowercase() && cur.is_uppercase() {
        return true;
    }
    if (prev.is_alphabetic() && cur.is_numeric()) || (prev.is_numeric() && cur.is_alphabetic()) {
        return true;
    }
    prev.is_uppercase() && cur.is_uppercase() && after.next().is_some_and(char::is_lowercase)
}

/// Whether `needle` sits in `text` at a word's start somewhere, ignoring case
/// unless it holds a capital (smart case). Every place it occurs is looked
/// at: frizbee reports the one it scored best.
fn starts_a_word(text: &str, needle: &str) -> bool {
    let exact = needle.chars().any(char::is_uppercase);
    // ASCII, the usual case, without a lower-cased copy of every name: `pr`
    // is solid in thousands of names, and each is looked at here.
    if text.is_ascii() && needle.is_ascii() {
        let (hay, pin) = (text.as_bytes(), needle.as_bytes());
        if pin.is_empty() || pin.len() > hay.len() {
            return false;
        }
        return (0..=hay.len() - pin.len()).any(|at| {
            let here = &hay[at..at + pin.len()];
            (if exact {
                here == pin
            } else {
                here.eq_ignore_ascii_case(pin)
            }) && word_start(text, at)
        });
    }
    let (hay, pin) = if exact {
        (text.to_owned(), needle.to_owned())
    } else {
        (text.to_lowercase(), needle.to_lowercase())
    };
    // A letter whose small form is longer would put the offsets off: such a
    // name is read as it stands.
    if hay.len() != text.len() {
        return text.contains(needle);
    }
    let mut from = 0;
    while let Some(at) = hay[from..].find(&pin) {
        if word_start(text, from + at) {
            return true;
        }
        from += at + hay[from + at..].chars().next().map_or(1, char::len_utf8);
    }
    false
}

/// Whether the letters at `starts` (bytes of character starts, ascending)
/// sit in `text` as an abbreviation does (D5 rules 3 and 4): the first at a
/// word's start, and every stretch skipped either inside one word, or the
/// rest of a word and then only spaces and signs up to the next, and then
/// only where the word left behind gave two letters at least.
fn shaped(text: &str, starts: &[usize]) -> bool {
    let Some(&first) = starts.first() else {
        return false;
    };
    if !word_start(text, first) {
        return false;
    }
    // The letters placed in the word being read, since the last step to
    // another word.
    let mut piece = 1;
    for pair in starts.windows(2) {
        let end = pair[0] + text[pair[0]..].chars().next().map_or(1, char::len_utf8);
        if end >= pair[1] {
            piece += 1;
            continue;
        }
        let skipped = &text[end..pair[1]];
        match skipped.find(char::is_whitespace) {
            None => piece += 1,
            Some(space) => {
                if piece < 2 || skipped[space..].chars().any(char::is_alphanumeric) {
                    return false;
                }
                piece = 1;
            }
        }
    }
    true
}

/// Frizbee's positions (bytes, any order, a multi-byte character's every
/// byte) as the character starts they hold, ascending.
fn char_starts(text: &str, indices: &[u32]) -> Vec<usize> {
    let mut starts: Vec<usize> = indices
        .iter()
        .map(|&at| at as usize)
        .filter(|&at| at < text.len() && text.is_char_boundary(at))
        .collect();
    starts.sort_unstable();
    starts.dedup();
    starts
}

/// Byte positions as runs of `[start, end)` in UTF-16 code units, flattened.
fn utf16_runs(text: &str, indices: &[u32]) -> Vec<u32> {
    let starts = char_starts(text, indices);
    let mut runs: Vec<u32> = Vec::new();
    let mut next = starts.iter().peekable();
    let mut unit: u32 = 0;
    for (at, ch) in text.char_indices() {
        let width = u32::try_from(ch.len_utf16()).unwrap_or(2);
        if next.peek().is_some_and(|&&s| s == at) {
            next.next();
            match runs.last_mut() {
                Some(end) if *end == unit => *end = unit + width,
                _ => {
                    runs.push(unit);
                    runs.push(unit + width);
                }
            }
        }
        unit += width;
    }
    runs
}

/// What a row is worth to the launcher's merge (D5): `score` over sixteen a
/// letter typed, one at most. A loose or typo row carries it less one, so
/// every solid row is above every loose one and the sign tells them apart:
/// below zero always, so a loose fit that reaches the cap (`paysched` scores
/// 136 over eight letters) is never read as a solid row of rank zero.
#[allow(clippy::cast_precision_loss)]
fn worth(score: u16, letters: usize, loose: bool) -> f32 {
    let rank = (f32::from(score) / (16.0 * letters.max(1) as f32)).min(1.0);
    if loose {
        (rank - 1.0).min(-LOOSE_BELOW)
    } else {
        rank
    }
}

/// How far under zero a loose row's worth is at its best.
const LOOSE_BELOW: f32 = 1.0 / 1024.0;

/// How [`pick_over`] keeps rows: the caps a kind, and whether a text of one
/// to three letters is kept anywhere in a name (a page's own list, D12) or
/// only at a word's start (the launcher, where ten rows a kind are all
/// there is).
struct Rules {
    solid_a_kind: usize,
    loose_a_kind: usize,
    anywhere: bool,
}

/// One row the rules kept: its index in the list read, its score, whether it
/// is loose.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Kept {
    index: u32,
    score: u16,
    loose: bool,
}

/// D5 over `titles`: the solid rows, then the loose, then one typo, each kind
/// (`kind_of`) capped, the rows `keep` refuses left out before any cap.
fn pick_over(
    typed: &Typed,
    titles: &[String],
    kind_of: &dyn Fn(usize) -> usize,
    keep: &dyn Fn(usize) -> bool,
    rules: &Rules,
) -> Vec<Kept> {
    let mut out: Vec<Kept> = Vec::new();
    if typed.letters < 2 && !rules.anywhere {
        return out;
    }
    let config = Config::default();
    let mut solid_of: HashMap<usize, usize> = HashMap::new();
    let mut is_solid = vec![false; titles.len()];
    let mut solid = 0_usize;

    let mut matcher = Matcher::from_patterns(&typed.patterns(None), &config);
    for found in matcher.match_list(titles) {
        let index = found.index as usize;
        if !keep(index) || !typed.solid_fit(&titles[index], rules.anywhere) {
            continue;
        }
        is_solid[index] = true;
        solid += 1;
        let held = solid_of.entry(kind_of(index)).or_default();
        if *held < rules.solid_a_kind {
            *held += 1;
            out.push(Kept {
                index: found.index,
                score: found.score,
                loose: false,
            });
        }
    }

    let fuzzy: Vec<&Atom> = typed.atoms.iter().filter(|atom| atom.loose()).collect();
    if typed.letters < LOOSE_FROM || fuzzy.is_empty() {
        return out;
    }
    let mut loose_of: HashMap<usize, usize> = HashMap::new();
    let mut taken = vec![false; titles.len()];
    let mut loose_kept = 0_usize;
    for typos in [Typos::None, Typos::Allowed] {
        if typos == Typos::Allowed && (solid > 0 || loose_kept >= TYPO_UNDER) {
            break;
        }
        let mut matcher = Matcher::from_patterns(&typed.patterns(Some(typos)), &config);
        let found = matcher.match_list(titles);
        let Some(best) = found.first().map(|m| f64::from(m.score)) else {
            continue;
        };
        let mut singles: Vec<(Matcher, usize)> = fuzzy
            .iter()
            .map(|atom| {
                let wanted = match typos {
                    Typos::None => atom.letters(),
                    Typos::Allowed => atom.letters() - usize::from(typos_of(atom.letters())),
                };
                (
                    Matcher::from_patterns(&[pattern(atom, Some(typos))], &config),
                    wanted,
                )
            })
            .collect();
        let mut looked = 0_usize;
        for m in found {
            let index = m.index as usize;
            // Sorted best first: past the floor, every row after is too.
            if f64::from(m.score) < best * FLOOR {
                break;
            }
            if is_solid[index] || taken[index] || !keep(index) {
                continue;
            }
            looked += 1;
            if looked > LOOSE_LOOKED_AT {
                break;
            }
            let title = &titles[index];
            // Every typed letter placed, less the typos allowed (frizbee
            // scores the best local alignment, which may leave letters out
            // even with no typo allowed: `adprx` scored 62 on "Harden auth
            // [pr]o[x]y"), and placed as an abbreviation is.
            let fits = singles.iter_mut().all(|(single, wanted)| {
                single
                    .match_one_indices(title.as_str(), 0)
                    .is_some_and(|placed| {
                        let starts = char_starts(title, &placed.indices);
                        starts.len() >= *wanted && shaped(title, &starts)
                    })
            });
            if !fits {
                continue;
            }
            let held = loose_of.entry(kind_of(index)).or_default();
            if *held < rules.loose_a_kind {
                *held += 1;
                taken[index] = true;
                if typos == Typos::None {
                    loose_kept += 1;
                }
                out.push(Kept {
                    index: m.index,
                    score: m.score,
                    loose: true,
                });
            }
        }
    }
    out
}

/// One name the picker chose for the launcher: its entity and what it is
/// worth to the merge (negative for a loose or typo row).
#[derive(Clone, Debug, PartialEq)]
pub struct Pick {
    pub entity_id: String,
    pub rank: f32,
}

/// The names chosen for one typed text (D5): at most [`SOLID_A_KIND`] solid
/// and [`LOOSE_A_KIND`] loose rows of each raw kind, best first.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Picked {
    pub rows: Vec<Pick>,
}

impl Picked {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// The ids, for the statement that reads the rows (D8).
    #[must_use]
    pub fn ids(&self) -> Vec<String> {
        self.rows.iter().map(|row| row.entity_id.clone()).collect()
    }

    /// The ranks, beside [`Self::ids`].
    #[must_use]
    pub fn ranks(&self) -> Vec<f32> {
        self.rows.iter().map(|row| row.rank).collect()
    }
}

/// What a pick is narrowed by before any kind is cut: the sources a chip or
/// `source:` asked for, all where empty (D6).
#[derive(Clone, Debug, Default)]
pub struct Limits {
    pub sources: Vec<String>,
}

/// One row of a list ranked by [`rank`] (D12): its index in the list handed
/// in, whether it is loose, and its marks as [`Typed::marks`] gives them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ranked {
    pub index: u32,
    pub loose: bool,
    pub marks: Vec<u32>,
}

/// D4 and D5 over a list a page holds (D12), best first: solid rows, then at
/// most [`LOOSE_A_KIND`] loose ones. Unlike the launcher's names, a text of
/// one to three letters keeps a row that holds it anywhere, as the page
/// filters always have. Nothing for a text the picker reads as nothing.
#[must_use]
pub fn rank(text: &str, haystacks: &[String]) -> Vec<Ranked> {
    let Some(typed) = Typed::read(text) else {
        return Vec::new();
    };
    let rules = Rules {
        solid_a_kind: usize::MAX,
        loose_a_kind: LOOSE_A_KIND,
        anywhere: true,
    };
    pick_over(&typed, haystacks, &|_| 0, &|_| true, &rules)
        .into_iter()
        .map(|kept| Ranked {
            index: kept.index,
            loose: kept.loose,
            marks: typed.marks(&haystacks[kept.index as usize], kept.loose),
        })
        .collect()
}

/// The names held: parallel to `titles`, which frizbee reads, one record a
/// row, newest first (`item_updated_at` descending, nulls last, then the
/// id), so frizbee's order among equal scores is the newest first.
#[derive(Default)]
struct Names {
    titles: Vec<String>,
    ids: Vec<String>,
    /// Each row's kind, an index into `kinds`.
    kind: Vec<u16>,
    kinds: Vec<Arc<str>>,
    source: Vec<Arc<str>>,
}

/// What a [`Picker`] holds between reloads.
#[derive(Default)]
struct Held {
    names: Names,
    /// The words the mirror holds, sorted (D10): empty until the words'
    /// fill has marked them whole.
    words: Vec<String>,
}

/// What one reload read: logged as the fills log.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Loaded {
    pub names: usize,
    pub words: usize,
    pub millis: u64,
}

/// Every name of the mirror, held in memory and read by frizbee (D2, D3).
///
/// A cheap clone: everything is behind one `Arc`. [`Self::default`] holds
/// nothing and answers nothing, which is what a searcher without one does
/// too. [`Self::reload`] builds the new names aside and swaps them in, so a
/// search never waits on a reload.
#[derive(Clone, Default)]
pub struct Picker {
    held: Arc<RwLock<Arc<Held>>>,
    /// A reload runs; a request meanwhile sets [`Self::again`].
    running: Arc<AtomicBool>,
    again: Arc<AtomicBool>,
}

impl std::fmt::Debug for Picker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let held = self.held();
        f.debug_struct("Picker")
            .field("names", &held.names.titles.len())
            .field("words", &held.words.len())
            .finish()
    }
}

/// Clears `running` when a reload ends, however it ends.
struct Running<'a>(&'a AtomicBool);

impl Drop for Running<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

impl Picker {
    fn held(&self) -> Arc<Held> {
        match self.held.read() {
            Ok(held) => Arc::clone(&held),
            Err(poisoned) => Arc::clone(&poisoned.into_inner()),
        }
    }

    /// Whether any name is held: a picker that holds none answers as none.
    #[must_use]
    pub fn holds_names(&self) -> bool {
        !self.held().names.titles.is_empty()
    }

    /// Read every name of the mirror and its words, and swap them in (D3).
    ///
    /// A request while a reload runs is kept and runs once after it: this
    /// call then returns at once with `None`, and the running one reads
    /// again before it returns.
    ///
    /// # Errors
    ///
    /// [`sqlx::Error`] if a read fails; what was held stays.
    pub async fn reload(&self, pool: &sqlx::PgPool) -> Result<Option<Loaded>, sqlx::Error> {
        self.again.store(true, Ordering::SeqCst);
        if self
            .running
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Ok(None);
        }
        let _running = Running(&self.running);
        let mut loaded = Loaded::default();
        while self.again.swap(false, Ordering::SeqCst) {
            let started = Instant::now();
            let held = load(pool).await?;
            loaded = Loaded {
                names: held.names.titles.len(),
                words: held.words.len(),
                millis: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
            };
            let held = Arc::new(held);
            match self.held.write() {
                Ok(mut slot) => *slot = held,
                Err(poisoned) => *poisoned.into_inner() = held,
            }
        }
        Ok(Some(loaded))
    }

    /// The names `text` picks (D5), narrowed by `limits` before each raw
    /// kind is cut. Nothing for a text under two letters, with an `or`, or
    /// of left-out words only, and nothing from a picker that holds none.
    #[must_use]
    pub fn pick(&self, text: &str, limits: &Limits) -> Picked {
        let held = self.held();
        let Some(typed) = Typed::read(text) else {
            return Picked::default();
        };
        let names = &held.names;
        let keep = |index: usize| {
            limits.sources.is_empty()
                || limits
                    .sources
                    .iter()
                    .any(|source| *source == *names.source[index])
        };
        let rules = Rules {
            solid_a_kind: SOLID_A_KIND,
            loose_a_kind: LOOSE_A_KIND,
            anywhere: false,
        };
        let kept = pick_over(
            &typed,
            &names.titles,
            &|index| usize::from(names.kind[index]),
            &keep,
            &rules,
        );
        Picked {
            rows: kept
                .into_iter()
                .map(|kept| Pick {
                    entity_id: names.ids[kept.index as usize].clone(),
                    rank: worth(kept.score, typed.letters, kept.loose),
                })
                .collect(),
        }
    }

    /// [`Typed::marks`] of `text` on `title`, solid.
    #[must_use]
    pub fn marks(&self, text: &str, title: &str) -> Vec<u32> {
        Typed::read(text).map_or_else(Vec::new, |typed| typed.marks(title, false))
    }

    /// The kinds the held names fall into, for a test.
    #[cfg(any(test, feature = "test-util"))]
    #[must_use]
    pub fn kinds(&self) -> Vec<String> {
        self.held()
            .names
            .kinds
            .iter()
            .map(|kind| kind.to_string())
            .collect()
    }

    /// A picker holding `rows` (`(entity id, kind, source, title)`, newest
    /// first), for a test that reads no database.
    #[cfg(any(test, feature = "test-util"))]
    #[must_use]
    pub fn holding(rows: &[(&str, &str, &str, &str)]) -> Self {
        let picker = Self::default();
        let mut names = Names::default();
        let mut kinds: HashMap<String, u16> = HashMap::new();
        for (id, kind, source, title) in rows {
            push_name(&mut names, &mut kinds, id, kind, source, title);
        }
        *picker.held.write().expect("a new lock") = Arc::new(Held {
            names,
            words: Vec::new(),
        });
        picker
    }
}

#[cfg(any(test, feature = "test-util"))]
fn push_name(
    names: &mut Names,
    kinds: &mut HashMap<String, u16>,
    id: &str,
    kind: &str,
    source: &str,
    title: &str,
) {
    let next = u16::try_from(names.kinds.len()).unwrap_or(u16::MAX);
    let index = *kinds.entry(kind.to_owned()).or_insert_with(|| {
        names.kinds.push(Arc::from(kind));
        next
    });
    let source = names
        .source
        .iter()
        .rev()
        .find(|held| ***held == *source)
        .cloned()
        .unwrap_or_else(|| Arc::from(source));
    names.ids.push(id.to_owned());
    names.kind.push(index);
    names.source.push(source);
    names.titles.push(title.to_owned());
}

/// The names' read (D3): every row of `sync.item`, its title, kind and
/// source, off `item_ticket_brief_idx` as an index-only scan. Liveness is
/// not read here: the statement that reads the chosen rows drops what is
/// withdrawn or switched off (D8).
pub const NAMES_SQL: &str = names_sql!();

macro_rules! names_sql {
    () => {
        "select i.entity_id, i.kind, i.source_id, i.title, i.item_updated_at
  from sync.item i"
    };
}
use names_sql;

/// [`NAMES_SQL`]'s plan, for the tests that hold it to an index-only scan
/// and no worker.
#[cfg(any(test, feature = "test-util"))]
pub const NAMES_PLAN_SQL: &str = concat!("explain (costs off) ", names_sql!());

/// What the names' read is planned under, for its transaction alone: no
/// parallel worker (D14). Nobody waits on the read, and on Windows a worker
/// is a process, 31 to 37 ms to start on Björn's PC.
pub const LOAD_PLANNING: &str = "select set_config('max_parallel_workers_per_gather', '0', true)";

async fn load(pool: &sqlx::PgPool) -> Result<Held, sqlx::Error> {
    type Row = (
        String,
        String,
        String,
        String,
        Option<chrono::DateTime<chrono::Utc>>,
    );
    let mut tx = pool.begin().await?;
    sqlx::query(LOAD_PLANNING).execute(&mut *tx).await?;
    let mut rows: Vec<Row> = sqlx::query_as(NAMES_SQL).fetch_all(&mut *tx).await?;
    let words: Vec<String> = Vec::new();
    tx.commit().await?;
    rows.sort_by(|a, b| match (a.4, b.4) {
        (Some(x), Some(y)) => y.cmp(&x).then_with(|| a.0.cmp(&b.0)),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => a.0.cmp(&b.0),
    });
    let mut names = Names::default();
    let mut kinds: HashMap<String, u16> = HashMap::new();
    let mut sources: HashMap<String, Arc<str>> = HashMap::new();
    names.titles.reserve(rows.len());
    for (id, kind, source, title, _) in rows {
        let next = u16::try_from(names.kinds.len()).unwrap_or(u16::MAX);
        let kind = *kinds.entry(kind).or_insert_with_key(|kind| {
            names.kinds.push(Arc::from(kind.as_str()));
            next
        });
        let source = sources
            .entry(source)
            .or_insert_with_key(|source| Arc::from(source.as_str()))
            .clone();
        names.ids.push(id);
        names.kind.push(kind);
        names.source.push(source);
        names.titles.push(title);
    }
    Ok(Held { names, words })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list(titles: &[&str]) -> Vec<String> {
        titles.iter().map(|title| (*title).to_owned()).collect()
    }

    /// The kept rows' titles, best first, as `(title, loose)`.
    fn kept(text: &str, titles: &[&str]) -> Vec<(String, bool)> {
        let titles = list(titles);
        let typed = Typed::read(text).expect("a text");
        let rules = Rules {
            solid_a_kind: SOLID_A_KIND,
            loose_a_kind: LOOSE_A_KIND,
            anywhere: false,
        };
        pick_over(&typed, &titles, &|_| 0, &|_| true, &rules)
            .into_iter()
            .map(|kept| (titles[kept.index as usize].clone(), kept.loose))
            .collect()
    }

    fn found(text: &str, titles: &[&str]) -> Vec<String> {
        kept(text, titles)
            .into_iter()
            .map(|(title, _)| title)
            .collect()
    }

    fn one(text: &str, title: &str) -> Option<(u16, Vec<u32>)> {
        let mut matcher = Matcher::from_patterns(
            &Typed::read(text).expect("a text").patterns(None),
            &Config::default(),
        );
        matcher
            .match_one_indices(title, 0)
            .map(|found| (found.score, found.indices))
    }

    /// **D1: the version's behaviour the rules rely on**, pinned. A new
    /// frizbee that scores or places otherwise fails here first.
    #[test]
    fn frizbee_scores_and_places_as_pinned() {
        assert_eq!(one("proxy", "acme/ADProxy").map(|f| f.0), Some(76));
        assert_eq!(one("proxy", "acme/auth-proxy").map(|f| f.0), Some(84));
        assert_eq!(one("PROXY", "acme/ADProxy"), None);
        assert_eq!(one("PROXY", "acme/auth-proxy"), None);
        // Positions are byte offsets, descending.
        assert_eq!(
            one("proxy", "acme/ADProxy").map(|f| f.1),
            Some(vec![11, 10, 9, 8, 7])
        );
        // A multi-byte letter is every one of its bytes.
        assert_eq!(
            one("größe", "Die Größe ändern").map(|f| f.1),
            Some(vec![10, 9, 8, 7, 6, 5, 4])
        );
        // With no typo allowed the alignment may still leave typed letters
        // out: why D5's rule 2 counts them.
        let mut loose = Matcher::from_patterns(
            &Typed::read("adprx")
                .expect("a text")
                .patterns(Some(Typos::None)),
            &Config::default(),
        );
        let placed = loose
            .match_one_indices("Harden auth proxy the runbook", 0)
            .expect("frizbee matches it");
        assert_eq!(placed.score, 62);
        assert_eq!(placed.indices.len(), 3);
    }

    /// **D4: how typed text is read**, a row a rule, each with a name that
    /// matches and one that does not.
    #[test]
    fn typed_text_is_read_as_decided() {
        let names = |text: &str, title: &str| Typed::read(text).expect("a text").names(title);
        // 1. Several words, each on its own, in any order.
        assert!(names("service mapping", "acme/abcMappingService"));
        assert!(!names("service routing", "acme/abcMappingService"));
        // 2. Smart case.
        assert!(names("Proxy", "acme/ADProxy"));
        assert!(!names("Proxy", "acme/auth-proxy"));
        assert!(!names("PROXY", "acme/ADProxy"));
        // 3. `'word`, `^word`, `word$`, `^word$`.
        assert!(names("'proxy", "acme/ADProxy"));
        assert!(!names("'prxy", "acme/ADProxy"));
        assert!(names("^acme", "acme/ADProxy"));
        assert!(!names("^proxy", "acme/ADProxy"));
        assert!(names("proxy$", "acme/ADProxy"));
        assert!(!names("acme$", "acme/ADProxy"));
        assert!(names("^acme/adproxy$", "acme/ADProxy"));
        assert!(!names("^acme/adprox$", "acme/ADProxy"));
        // 4. `!word` and `-word` leave a name out.
        assert!(!names("acme !proxy", "acme/ADProxy"));
        assert!(!names("acme -proxy", "acme/ADProxy"));
        assert!(names("acme -proxy", "acme/abcMappingService"));
        // 5. A phrase is one piece, spaces included.
        assert!(names("\"auth proxy\"", "Harden auth proxy"));
        assert!(!names("\"proxy auth\"", "Harden auth proxy"));
        // 6. A key is never fuzzed.
        assert_eq!(
            found("sclpay-4988", &["scale-payments-staging-49"]),
            Vec::<String>::new()
        );
        assert_eq!(
            found("sclpay-4988", &["SCLPAY-4988 retry"]),
            ["SCLPAY-4988 retry"]
        );
        assert_eq!(found("4988", &["scale 4 9 8 8"]), Vec::<String>::new());
        // 7. `or`: nothing.
        assert!(Typed::read("proxy or mapping").is_none());
        // 8. Under two letters, nothing.
        assert_eq!(found("p", &["proxy"]), Vec::<String>::new());
        assert!(!names("p", "proxy"));
        // Left-out words alone are nothing to look for.
        assert!(Typed::read("-proxy !auth").is_none());
        // 10. Accents: smart case folds case, not accents.
        assert!(names("größe", "Die Größe ändern"));
        // A capital makes the word exact (rule 2), so `Größe` does not find
        // `größe`: the list's rule 10 said it would, which rule 2 and
        // frizbee's smart case both refuse.
        assert!(!names("Größe", "die größe"));
        assert!(!names("Größe", "die grösse"));
        assert!(!names("grosse", "Die Größe ändern"));
        assert!(!names("andern", "Die Größe ändern"));
    }

    /// **The names the brief asks for**: camelCase, an acronym before a word,
    /// `snake_case`, `kebab-case`, digits, an accent, a typo within the
    /// budget, several words in any order.
    #[test]
    fn the_name_table() {
        let titles = [
            "acme/ADProxy",
            "acme/abcMappingService",
            "HTTPServer",
            "my_snake_case",
            "kebab-case-thing",
            "apiV2Client",
            "http2server",
            "Größe",
            "payout scheduler",
            "improve the docs",
        ];
        for (text, first) in [
            ("proxy", "acme/ADProxy"),
            ("mapping service", "acme/abcMappingService"),
            ("service mapping", "acme/abcMappingService"),
            ("mappingservice", "acme/abcMappingService"),
            ("mapsvc", "acme/abcMappingService"),
            ("snake", "my_snake_case"),
            ("case snake", "my_snake_case"),
            ("kebab", "kebab-case-thing"),
            ("v2", "apiV2Client"),
            ("client", "apiV2Client"),
            ("2server", "http2server"),
            ("größe", "Größe"),
            ("shceduler", "payout scheduler"),
            ("adprx", "acme/ADProxy"),
        ] {
            assert_eq!(
                found(text, &titles).first().map(String::as_str),
                Some(first),
                "{text}"
            );
        }
        // Both servers are solid; `http2server`'s `s` starts a part too.
        assert!(found("server", &titles).contains(&"HTTPServer".to_owned()));
        // Two or three letters only where a part of a name starts.
        assert_eq!(found("pro", &titles), ["acme/ADProxy"]);
        assert_eq!(found("adp", &titles), ["acme/ADProxy"]);
        // Three letters are never loose: `srv` is no piece of either server,
        // and `srvr`, four, finds them by their letters in order.
        assert!(found("srv", &titles).is_empty());
        assert!(found("srvr", &titles).contains(&"HTTPServer".to_owned()));
    }

    /// **D5: which names answer**, one test a rule, each with the junk row
    /// it exists to drop.
    #[test]
    fn solid_rows_of_two_or_three_letters_start_a_part_of_the_name() {
        // `pro` finds `ADProxy`'s `P` (an acronym's end) and not `improve`.
        assert_eq!(
            found("pro", &["improve the docs", "acme/ADProxy"]),
            ["acme/ADProxy"]
        );
        // Any place it occurs: the second `sys` starts a part.
        assert_eq!(
            found("sys", &["Scale test / Web / DesignSystem"]),
            ["Scale test / Web / DesignSystem"]
        );
        assert_eq!(found("sys", &["ecosystems"]), Vec::<String>::new());
        // Four letters and more are solid anywhere.
        assert_eq!(found("prov", &["improve the docs"]), ["improve the docs"]);
    }

    #[test]
    fn a_word_under_four_letters_is_never_loose() {
        // `prx` holds letters in order of `proxy`, but three letters are solid
        // or nothing, even in a text of four letters and more.
        assert_eq!(found("prx", &["acme/proxy"]), Vec::<String>::new());
        assert_eq!(found("acme prx", &["acme/proxy"]), Vec::<String>::new());
        assert_eq!(
            found("acme prxy", &["acme/proxy"]),
            [String::from("acme/proxy")]
        );
    }

    #[test]
    fn the_floor_drops_a_loose_row_far_below_the_best() {
        // The junk is shaped (every letter placed, inside one word, the
        // first at its start), so only the floor tells it from a fit: alone
        // it is kept, beside a row that holds the word it is dropped.
        let junk = "axxxxxxbxxxxxxcxxxxxxd";
        assert_eq!(found("abcd", &[junk]), [junk]);
        assert_eq!(found("abcd", &["abcd tool", junk]), ["abcd tool"]);
    }

    #[test]
    fn every_typed_letter_is_placed() {
        // Rule 2: frizbee places `[pr]o[x]y` with `a` and `d` left out.
        assert_eq!(
            found("adprx", &["Harden auth proxy the runbook", "acme/ADProxy"]),
            ["acme/ADProxy"]
        );
    }

    #[test]
    fn the_first_placed_letter_starts_a_word() {
        // Rule 3: `obs` inside `jobs` is no abbreviation of it.
        assert_eq!(
            found("obsstack", &["jobs stacked", "observability-stack"]),
            ["observability-stack"]
        );
    }

    #[test]
    fn a_skipped_stretch_crosses_a_word_only_from_two_letters() {
        // Rule 4: `mapsvc` scattered over a sentence is no fit, and the word
        // left behind must have given two letters at least.
        let prose = "automatic production service";
        assert_eq!(found("mapsvc", &[prose]), Vec::<String>::new());
        assert_eq!(found("apsvc", &[prose, "apps service"]), ["apps service"]);
        // `m` alone in `my`, then `apsvc`: one letter left behind.
        assert_eq!(found("mapsvc", &["my apps service"]), Vec::<String>::new());
        assert_eq!(
            found("mapsvc", &["acme/abcMappingService"]),
            ["acme/abcMappingService"]
        );
    }

    #[test]
    fn the_typo_pass_runs_only_where_nothing_else_answered() {
        // Two swapped letters: no order of letters finds them, a typo does.
        assert_eq!(
            kept("shceduler", &["payout scheduler"]),
            [("payout scheduler".to_owned(), true)]
        );
        // A solid row: no typo pass, so the typo's row stays out, though it
        // would score above the solid one (82 against a mid-word 74).
        assert_eq!(found("payuot", &["zzpayuotzz", "payout"]), ["zzpayuotzz"]);
        assert_eq!(found("payuot", &["payout"]), ["payout"]);
        // Fewer than five loose rows: the typo pass adds its row (`pyasched`
        // scores 114 with its typo, at the floor of the loose row's 134).
        assert_eq!(
            kept("paysched", &["payments scheduled", "pyasched"]),
            [
                ("payments scheduled".to_owned(), true),
                ("pyasched".to_owned(), true)
            ]
        );
        // Five loose rows: no typo pass, so the typo's row stays out.
        let five = [
            "payments scheduled one",
            "payments scheduled two",
            "payments scheduled three",
            "payments scheduled four",
            "payments scheduled five",
            "pyasched",
        ];
        let loose = found("paysched", &five);
        assert_eq!(loose.len(), 5, "{loose:?}");
        assert!(!loose.iter().any(|title| title == "pyasched"));
    }

    #[test]
    fn a_kind_holds_ten_loose_rows_and_twenty_solid() {
        let titles: Vec<String> = (0..40).map(|n| format!("payout scheduler {n}")).collect();
        let refs: Vec<&str> = titles.iter().map(String::as_str).collect();
        assert_eq!(found("paysched", &refs).len(), LOOSE_A_KIND);
        assert_eq!(found("payout", &refs).len(), SOLID_A_KIND);
    }

    #[test]
    fn ranks_put_every_solid_row_above_every_loose_one() {
        assert!((worth(76, 5, false) - 0.95).abs() < 1e-6);
        assert!((worth(240, 5, false) - 1.0).abs() < 1e-6);
        assert!(worth(77, 6, true) < 0.0);
        assert!(worth(1, 5, false) > worth(u16::MAX, 5, true));
        // A loose fit at the cap is still below zero.
        assert!(worth(136, 8, true) < 0.0);
    }

    #[test]
    fn a_pick_narrows_by_source_before_a_kind_is_cut() {
        let picker = Picker::holding(&[
            ("gitea:1", "repo", "gitea", "acme/ADProxy"),
            ("forgejo:1", "repo", "forgejo", "tidewater/ADProxy"),
        ]);
        let all = picker.pick("proxy", &Limits::default());
        assert_eq!(all.ids(), ["gitea:1", "forgejo:1"]);
        let one = picker.pick(
            "proxy",
            &Limits {
                sources: vec!["forgejo".to_owned()],
            },
        );
        assert_eq!(one.ids(), ["forgejo:1"]);
        // A picker that holds nothing answers nothing.
        assert!(
            Picker::default()
                .pick("proxy", &Limits::default())
                .is_empty()
        );
        assert!(!Picker::default().holds_names());
    }

    /// **D9: the marks**, in UTF-16 code units of the title.
    #[test]
    fn marks_are_utf16_runs() {
        let typed = |text: &str| Typed::read(text).expect("a text");
        assert_eq!(typed("größe").marks("Die Größe ändern", false), [4, 9]);
        assert_eq!(
            typed("service mapping").marks("acme/abcMappingService", false),
            [8, 22]
        );
        assert_eq!(
            typed("proxy acme").marks("acme/ADProxy", false),
            [0, 4, 7, 12]
        );
        // A loose row: each placed letter, neighbours merged.
        assert_eq!(
            typed("mapsvc").marks("acme/abcMappingService", true),
            [8, 10, 11, 12, 15, 16, 18, 19, 20, 21]
        );
        // An emoji before the match is two code units.
        assert_eq!(typed("proxy").marks("🚀 proxy", false), [3, 8]);
        // A row the solid patterns do not match has none.
        assert!(
            typed("mapsvc")
                .marks("acme/abcMappingService", false)
                .is_empty()
        );
        assert!(typed("proxy").marks("routing", false).is_empty());
    }

    /// **D12: a page's own list**, one to three letters anywhere.
    #[test]
    fn rank_keeps_short_text_anywhere() {
        let list = list(&["improve the docs", "acme/ADProxy", "routing"]);
        let ranked = rank("pro", &list);
        assert_eq!(
            ranked.iter().map(|r| r.index).collect::<Vec<_>>(),
            [0, 1],
            "{ranked:?}"
        );
        let one = rank("p", &list);
        assert_eq!(one.len(), 2);
        let loose = rank("adprx", &list);
        assert_eq!(loose.len(), 1);
        assert!(loose[0].loose);
        assert!(!loose[0].marks.is_empty());
        assert!(rank("or", &list).is_empty());
    }
}
