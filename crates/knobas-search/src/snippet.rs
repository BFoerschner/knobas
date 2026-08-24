//! Turning `ts_headline`'s output into segments.
//!
//! Postgres marks a headline's matches with configurable selectors. The
//! default is `<b>…</b>`, which is unusable here: the excerpt is raw source
//! text that every renderer has to escape, so the tags would arrive on screen
//! as literal `<b>`. Instead the selectors are two control characters that
//! cannot appear in prose, and Rust splits on them -- so no markup ever
//! crosses the bridge and the UI still knows what matched (carry-over D).

use crate::types::Segment;

/// Marks the start of a match in a `ts_headline` result.
pub const HIT_START: char = '\u{1}';
/// Marks the end of a match in a `ts_headline` result.
pub const HIT_STOP: char = '\u{2}';

/// The `ts_headline` options string, bound as a parameter rather than
/// interpolated (the sentinels are control characters; quoting them into SQL
/// is a needless escaping problem).
#[must_use]
pub fn headline_options() -> String {
    format!("MaxWords=18, MinWords=8, StartSel={HIT_START}, StopSel={HIT_STOP}")
}

/// Split a sentinel-marked headline into segments, dropping the sentinels.
///
/// A sentinel only ever moves the *state*; text is appended to the segment it
/// belongs to, and a new segment is opened only when a character's state
/// actually differs from the one before it. That is what keeps two adjacent
/// marks (`␁sepa␂␁retry␂`) one run rather than two, and empty segments out of
/// the output entirely.
///
/// Unbalanced sentinels -- which would mean a source document containing one
/// of the two control characters -- degrade to a mis-marked segment; no text
/// is ever lost or duplicated.
#[must_use]
pub fn segments(headline: &str) -> Vec<Segment> {
    let mut out: Vec<Segment> = Vec::new();
    let mut hit = false;
    for ch in headline.chars() {
        match ch {
            HIT_START => hit = true,
            HIT_STOP => hit = false,
            _ => match out.last_mut() {
                Some(last) if last.hit == hit => last.text.push(ch),
                _ => out.push(Segment {
                    text: String::from(ch),
                    hit,
                }),
            },
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(text: &str) -> Vec<(String, bool)> {
        segments(text)
            .into_iter()
            .map(|s| (s.text, s.hit))
            .collect()
    }

    #[test]
    fn splits_on_the_sentinels() {
        let headline = format!("the {HIT_START}sepa{HIT_STOP} batch");
        assert_eq!(
            plain(&headline),
            [
                ("the ".to_owned(), false),
                ("sepa".to_owned(), true),
                (" batch".to_owned(), false)
            ]
        );
    }

    #[test]
    fn handles_the_degenerate_shapes() {
        assert!(plain("").is_empty());
        assert_eq!(plain("no marks"), [("no marks".to_owned(), false)]);
        // Adjacent marks do not produce empty segments.
        let both = format!("{HIT_START}sepa{HIT_STOP}{HIT_START}retry{HIT_STOP}");
        assert_eq!(plain(&both), [("separetry".to_owned(), true)]);
        // Leading and trailing marks.
        let edges = format!("{HIT_START}sepa{HIT_STOP} retry");
        assert_eq!(
            plain(&edges),
            [("sepa".to_owned(), true), (" retry".to_owned(), false)]
        );
    }

    /// Source text is arbitrary and could in principle contain a sentinel
    /// byte. It must not desynchronise the walk into a panic or a lost tail.
    #[test]
    fn an_unbalanced_sentinel_loses_nothing() {
        let odd = format!("a{HIT_STOP}b{HIT_START}c");
        let text: String = segments(&odd).into_iter().map(|s| s.text).collect();
        assert_eq!(text, "abc");
    }
}
