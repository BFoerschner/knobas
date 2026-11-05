//! Where the words appear in a hit's text (#2431, spec #2419, *The match's
//! place on the hit*).
//!
//! The statement hands back, for each hit on the page and only under
//! *Everything*, the text the row is indexed by: a page's or a ticket's
//! `body_text`, a note's body, a pipeline's stored latest run log with the
//! number of its first line. This module cuts it into the places a reader
//! finds again -- a page's sections and comments as the Docs search's marks
//! spell them (`knobas_source::docs::indexed`), a log's lines -- and keeps
//! each sentence or line that holds a typed word, the words flagged. A
//! section's heading and a comment's author are indexed too and sit only in
//! their marks, so a mark that holds a word is quoted as well (#2481).
//!
//! Words are matched as the Docs search marks them (`knobas_app::docs`'s
//! `stem`, prototype-6): lowercased, a word over four letters without one
//! `ing`, `es`, `ed` or `s` at its end, at the start of a word, the mark
//! running to the word's end. It is a reader's marking, not the index's
//! stemming: a row Postgres matched through a stem this rule misses has no
//! excerpt. So has a row matched by its title, its key or a page link's line,
//! and the preview says only that there is no passage to quote (#2481).

use knobas_source::docs::indexed::{self, Part};

use crate::types::{Excerpt, Segment};

/// The most excerpts one hit carries: what a preview pane shows without a
/// scroll, and a bound on what a log of matching lines sends.
pub const MAX: usize = 8;

/// The longest excerpt, in characters, before it is cut around its match.
const SPAN: usize = 180;

/// The places in `text` that hold one of `needles`, at most [`MAX`].
///
/// `first_line` is the number of `text`'s first line in a run log, and says
/// the text is one: every line is its own place, `line <n>`. Otherwise the
/// text is read for the Docs marks, and the title a source opens the text
/// with is not quoted (it is on screen).
#[must_use]
pub fn excerpts(
    title: &str,
    text: &str,
    first_line: Option<i64>,
    needles: &[String],
) -> Vec<Excerpt> {
    let stems: Vec<Vec<char>> = needles
        .iter()
        .map(|n| stem(n))
        .filter(|s| !s.is_empty())
        .collect();
    if stems.is_empty() || text.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    // The marking is passed unmade, so a full list marks no more of the text.
    let keep = |place: Option<String>,
                segments: &dyn Fn() -> Option<Vec<Segment>>,
                out: &mut Vec<Excerpt>| {
        if out.len() >= MAX {
            return;
        }
        let Some(segments) = segments() else {
            return;
        };
        if out
            .iter()
            .any(|e: &Excerpt| e.place == place && e.segments == segments)
        {
            return;
        }
        out.push(Excerpt { place, segments });
    };
    if let Some(first) = first_line {
        for (offset, line) in text.lines().enumerate() {
            let number = first.saturating_add(i64::try_from(offset).unwrap_or(i64::MAX));
            keep(
                Some(format!("line {number}")),
                &|| marked(line.trim(), &stems),
                &mut out,
            );
        }
        return out;
    }
    for run in indexed::read(text).runs {
        match &run.part {
            Part::Lead => {}
            Part::Section(heading) => {
                keep(
                    Some("Heading".to_owned()),
                    &|| marked(heading, &stems),
                    &mut out,
                );
            }
            // The line reads as its own place, so it names none.
            Part::Comment(by) => keep(None, &|| author_line(by, &stems), &mut out),
        }
        let place = match &run.part {
            Part::Lead => None,
            Part::Section(heading) => Some(heading.clone()),
            Part::Comment(by) if by.is_empty() => Some("Comment".to_owned()),
            Part::Comment(by) => Some(format!("Comment by {by}")),
        };
        let body = match run.part {
            Part::Lead => run
                .text
                .strip_prefix(title)
                .unwrap_or(&run.text)
                .trim()
                .to_owned(),
            _ => run.text,
        };
        for sentence in sentences(&body) {
            keep(place.clone(), &|| marked(&sentence, &stems), &mut out);
        }
    }
    out
}

/// `Comment by <name>` with the words marked in the name alone; `None` when
/// the name holds none. The rest is this module's wording, which no row is
/// indexed by.
fn author_line(name: &str, stems: &[Vec<char>]) -> Option<Vec<Segment>> {
    let mut segments = marked(name, stems)?;
    segments.insert(
        0,
        Segment {
            text: "Comment by ".to_owned(),
            hit: false,
        },
    );
    Some(segments)
}

/// A needle as a reader matches it: lowercased, and a word longer than four
/// letters without one `ing`, `es`, `ed` or `s` at its end.
fn stem(needle: &str) -> Vec<char> {
    let lower: Vec<char> = needle.trim().chars().map(lower).collect();
    if lower.len() > 4 {
        for suffix in ["ing", "es", "ed", "s"] {
            let suffix: Vec<char> = suffix.chars().collect();
            if lower.ends_with(&suffix) {
                return lower[..lower.len() - suffix.len()].to_vec();
            }
        }
    }
    lower
}

/// One character lowercased to one character, so offsets survive.
fn lower(c: char) -> char {
    c.to_lowercase().next().unwrap_or(c)
}

/// A run's sentences: split after `.`, `!` or `?` and at line ends.
fn sentences(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in text.lines() {
        let mut current = String::new();
        let mut chars = line.chars().peekable();
        while let Some(c) = chars.next() {
            current.push(c);
            if matches!(c, '.' | '!' | '?') && chars.peek().is_some_and(|n| n.is_whitespace()) {
                out.push(std::mem::take(&mut current).trim().to_owned());
            }
        }
        out.push(current.trim().to_owned());
    }
    out.retain(|s| !s.is_empty());
    out
}

/// `sentence` as segments with every needle that starts a word marked to
/// that word's end, cut to [`SPAN`] around its first mark; `None` when it
/// holds none.
fn marked(sentence: &str, stems: &[Vec<char>]) -> Option<Vec<Segment>> {
    let chars: Vec<char> = sentence.chars().collect();
    let lowered: Vec<char> = chars.iter().copied().map(lower).collect();
    let mut hit = vec![false; chars.len()];
    let mut any = false;
    for stem in stems {
        let mut i = 0;
        while i + stem.len() <= lowered.len() {
            let at_word = i == 0 || !lowered[i - 1].is_alphanumeric();
            if at_word && lowered[i..i + stem.len()] == **stem {
                let mut end = i + stem.len();
                while end < chars.len() && chars[end].is_alphanumeric() {
                    end += 1;
                }
                hit[i..end].iter_mut().for_each(|h| *h = true);
                any = true;
                i = end;
            } else {
                i += 1;
            }
        }
    }
    if !any {
        return None;
    }
    let first = hit.iter().position(|h| *h).unwrap_or(0);
    let (start, lead) = if chars.len() > SPAN && first > 60 {
        let from = chars[..first - 50]
            .iter()
            .rposition(|c| c.is_whitespace())
            .map_or(first - 50, |p| p + 1);
        (from, true)
    } else {
        (0, false)
    };
    let end = (start + SPAN).min(chars.len());
    let mut out: Vec<Segment> = Vec::new();
    if lead {
        out.push(Segment {
            text: "…".to_owned(),
            hit: false,
        });
    }
    for i in start..end {
        match out.last_mut() {
            Some(last) if last.hit == hit[i] => last.text.push(chars[i]),
            _ => out.push(Segment {
                text: chars[i].to_string(),
                hit: hit[i],
            }),
        }
    }
    if end < chars.len() {
        match out.last_mut() {
            Some(last) if !last.hit => last.text.push('…'),
            _ => out.push(Segment {
                text: "…".to_owned(),
                hit: false,
            }),
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat(excerpts: &[Excerpt]) -> Vec<(Option<&str>, String)> {
        excerpts
            .iter()
            .map(|e| {
                (
                    e.place.as_deref(),
                    e.segments
                        .iter()
                        .map(|s| {
                            if s.hit {
                                format!("[{}]", s.text)
                            } else {
                                s.text.clone()
                            }
                        })
                        .collect(),
                )
            })
            .collect()
    }

    fn needles(words: &[&str]) -> Vec<String> {
        words.iter().map(|w| (*w).to_owned()).collect()
    }

    #[test]
    fn a_stemmed_word_is_marked_to_its_end_and_only_at_a_word_start() {
        let out = excerpts(
            "T",
            "T\nThe payouts retried. Prepayout stays.",
            None,
            &needles(&["payout", "retries"]),
        );
        assert_eq!(
            flat(&out),
            vec![(None, "The [payouts] [retried].".to_owned())]
        );
    }

    /// #2481: `fts` indexes a comment's author line, so a row found by that
    /// name alone has to quote it, or the preview has nothing to say.
    #[test]
    fn a_comment_whose_author_holds_the_word_is_quoted_with_the_name_marked() {
        let out = excerpts(
            "T",
            "T\nThe payouts retried.\n\u{1f}Priya Nair\nLooks wrong.",
            None,
            &needles(&["priya"]),
        );
        assert_eq!(
            flat(&out),
            vec![(None, "Comment by [Priya] Nair".to_owned())]
        );
    }

    /// The name is what is marked: `Comment by` is this module's wording and
    /// no row is indexed by it.
    #[test]
    fn the_words_comment_by_are_never_a_match() {
        let text = "T\n\u{1f}Priya\nLooks wrong.";
        assert!(excerpts("T", text, None, &needles(&["comment"])).is_empty());
        assert!(excerpts("T", text, None, &needles(&["by"])).is_empty());
    }

    #[test]
    fn one_author_of_many_comments_is_quoted_once_and_the_comment_text_keeps_its_place() {
        let out = excerpts(
            "T",
            "T\n\u{1f}Priya\nCap the retry.\n\u{1f}Priya\nStill failing.",
            None,
            &needles(&["priya", "retry"]),
        );
        assert_eq!(
            flat(&out),
            vec![
                (None, "Comment by [Priya]".to_owned()),
                (Some("Comment by Priya"), "Cap the [retry].".to_owned()),
            ]
        );
    }

    /// A comment no name was recorded for has no author line to quote.
    #[test]
    fn a_comment_by_nobody_quotes_only_its_text() {
        let out = excerpts("T", "T\n\u{1f}\nCap the retry.", None, &needles(&["retry"]));
        assert_eq!(
            flat(&out),
            vec![(Some("Comment"), "Cap the [retry].".to_owned())]
        );
    }

    /// The same cause as the author line: a heading is indexed and sits only
    /// in its mark.
    #[test]
    fn a_section_whose_heading_holds_the_word_is_quoted_with_the_word_marked() {
        let out = excerpts(
            "T",
            "T\n\u{1e}Backoff policy\nWait, then go again.",
            None,
            &needles(&["backoff"]),
        );
        assert_eq!(
            flat(&out),
            vec![(Some("Heading"), "[Backoff] policy".to_owned())]
        );
    }

    #[test]
    fn a_log_names_each_line_by_its_number_in_the_whole_log() {
        let out = excerpts(
            "x",
            "ok\nboom AC04\nok\nAC04 again",
            Some(40),
            &needles(&["ac04"]),
        );
        assert_eq!(
            flat(&out),
            vec![
                (Some("line 41"), "boom [AC04]".to_owned()),
                (Some("line 43"), "[AC04] again".to_owned()),
            ]
        );
    }

    #[test]
    fn a_long_sentence_is_cut_around_its_first_mark() {
        let long = format!("{} needle {}", "word ".repeat(40), "tail ".repeat(40));
        let out = excerpts("x", &long, None, &needles(&["needle"]));
        let text: String = out[0].segments.iter().map(|s| s.text.as_str()).collect();
        assert!(text.starts_with('…') && text.ends_with('…'), "{text}");
        assert!(text.chars().count() <= SPAN + 2, "{text}");
        assert!(out[0].segments.iter().any(|s| s.hit && s.text == "needle"));
    }

    #[test]
    fn no_more_than_max_and_no_needle_no_excerpt() {
        let log = "hit\n".repeat(30);
        assert_eq!(excerpts("x", &log, Some(1), &needles(&["hit"])).len(), MAX);
        assert!(excerpts("x", &log, Some(1), &needles(&["miss"])).is_empty());
        assert!(excerpts("x", &log, Some(1), &[]).is_empty());
    }
}
