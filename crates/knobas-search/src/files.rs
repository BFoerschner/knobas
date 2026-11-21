//! **A repository found by its files**, in the launcher's answer (the
//! everything-search decisions, 2026-11-20, D8 to D10): the excerpt a file is
//! shown by, and the ranks the files give a repository among the picker's
//! names.

use knobas_core::code_text;
use knobas_files::FileHit;

use crate::types::{Excerpt, FileAt, Segment};

/// The rank of the `k`-th repository whose files' or folders' names hold
/// the words (D9 row 3): placed as a solid name of rank 0.5 is, after the
/// repositories named by the words and above one whose description says
/// them once, in the files' own order.
#[must_use]
pub(crate) fn named_rank(k: usize) -> f32 {
    0.5 - rank_step(k)
}

/// The rank of the `k`-th repository whose files' text alone holds the
/// words (D9 row 7): below every rank a statement gives, so it is told by
/// what found it and goes last in its kind.
#[must_use]
pub(crate) fn text_rank(k: usize) -> f32 {
    -2.0 - rank_step(k)
}

#[allow(clippy::cast_precision_loss)]
fn rank_step(k: usize) -> f32 {
    k.min(999) as f32 / 1_000.0
}

/// `text` as segments, the letters `terms` cover flagged
/// (`code_text::marks`).
#[must_use]
pub(crate) fn marked(text: &str, terms: &[String]) -> Vec<Segment> {
    let mut out: Vec<Segment> = Vec::new();
    let mut at = 0;
    for (start, end) in code_text::marks(text, terms) {
        if start > at {
            out.push(Segment {
                text: text[at..start].to_owned(),
                hit: false,
            });
        }
        out.push(Segment {
            text: text[start..end].to_owned(),
            hit: true,
        });
        at = end;
    }
    if at < text.len() {
        out.push(Segment {
            text: text[at..].to_owned(),
            hit: false,
        });
    }
    out
}

/// The excerpt a file is shown by (D10): its line, its indent kept so the
/// pane can drop what it shares with the lines around it, under
/// `<path>:<line>`; or, where its name alone found it, its path.
#[must_use]
pub(crate) fn excerpt(hit: &FileHit, terms: &[String]) -> Excerpt {
    match &hit.line {
        Some(line) => Excerpt {
            place: Some(format!("{}:{}", hit.path, line.number)),
            segments: marked(line.text.trim_end(), terms),
            file: Some(FileAt {
                path: hit.path.clone(),
                line: Some(line.number),
                before: line.before.clone(),
                after: line.after.clone(),
            }),
        },
        None => Excerpt {
            place: None,
            segments: marked(&hit.path, terms),
            file: Some(FileAt {
                path: hit.path.clone(),
                line: None,
                before: String::new(),
                after: String::new(),
            }),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn terms(words: &[&str]) -> Vec<String> {
        words.iter().map(|w| (*w).to_owned()).collect()
    }

    fn flat(segments: &[Segment]) -> String {
        segments
            .iter()
            .map(|s| {
                if s.hit {
                    format!("[{}]", s.text)
                } else {
                    s.text.clone()
                }
            })
            .collect()
    }

    #[test]
    fn a_files_line_is_its_excerpt_and_a_name_found_file_its_path() {
        let line = FileHit {
            path: "src/com/acme/adproxy/ADProxyServer.java".to_owned(),
            in_name: true,
            line: Some(knobas_files::Line {
                number: 7,
                before: "/** TLS */".to_owned(),
                text: "    public final class ADProxyServer {".to_owned(),
                after: "    private final AdProxyConfig config;".to_owned(),
            }),
        };
        let excerpt = excerpt(&line, &terms(&["adproxy"]));
        assert_eq!(
            excerpt.place.as_deref(),
            Some("src/com/acme/adproxy/ADProxyServer.java:7")
        );
        assert_eq!(
            flat(&excerpt.segments),
            "    public final class [ADProxy]Server {"
        );
        assert_eq!(excerpt.file.as_ref().unwrap().line, Some(7));
        let named = FileHit {
            path: "charts/ad-proxy/values.yaml".to_owned(),
            in_name: true,
            line: None,
        };
        let excerpt = super::excerpt(&named, &terms(&["adproxy"]));
        assert_eq!(excerpt.place, None);
        assert_eq!(flat(&excerpt.segments), "charts/[ad-proxy]/values.yaml");
    }

    #[test]
    fn named_ranks_stand_between_a_name_and_a_single_mention_and_text_ranks_below_all() {
        assert!(named_rank(0) < 1.0 && named_rank(0) > 0.4);
        assert!(named_rank(1) < named_rank(0));
        assert!(text_rank(0) < -1.0 && text_rank(1) < text_rank(0));
    }
}
