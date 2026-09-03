//! Confluence **storage format** in, plain text out.
//!
//! The storage format is XHTML with Confluence's own `ac:` and `ri:` elements
//! mixed in. `payload` keeps it verbatim (§3a), because a later, smarter
//! rendering must be able to re-project it without re-syncing -- that is the
//! next ticket. What `body_text` needs is the opposite: the words, and only
//! the words, because FTS indexes it and because roadmap §4 gotcha 7 says
//! markup must never reach the webview by being mistaken for text.
//!
//! # Why a hand-rolled scanner and not an HTML parser
//!
//! Storage format is not HTML: it is XML-shaped, it carries namespaced
//! elements no HTML parser models, and its CDATA sections hold the bodies of
//! code macros. A parser would either reject it or normalise it into a DOM
//! this crate would then have to walk back out. The job here is small and
//! total -- text between tags, entities decoded, block boundaries kept -- and
//! a scanner that never fails is worth more than a parser that sometimes
//! does, because the failure mode of a body that would not parse is a page
//! that is not searchable.
//!
//! # Order matters, and it is the security-relevant half
//!
//! Tags are stripped **before** entities are decoded. `&lt;script&gt;` in the
//! storage format is the *text* `<script>`, and it must come out as that text;
//! decoding first would manufacture a tag out of prose and then strip it,
//! silently deleting what the author wrote.

/// The storage format stripped to text.
///
/// Block-level elements become line breaks so `<h2>A</h2><p>B</p>` reads as
/// two lines rather than as `AB`; inline elements vanish so `a<em>b</em>c`
/// reads as `abc`. Runs of whitespace collapse, blank lines go, and the
/// result is trimmed -- what is left is what a person would have typed.
#[must_use]
pub(crate) fn to_text(storage: &str) -> String {
    let mut out = String::with_capacity(storage.len());
    let mut rest = storage;
    while let Some(open) = rest.find('<') {
        out.push_str(&rest[..open]);
        rest = &rest[open..];
        if let Some(tail) = rest.strip_prefix("<![CDATA[") {
            // The body of a code macro. Its content is text, verbatim: it is
            // inside CDATA precisely because it may contain `<`.
            let (inside, tail) = tail.split_once("]]>").unwrap_or((tail, ""));
            out.push_str(inside);
            rest = tail;
            continue;
        }
        if let Some(tail) = rest.strip_prefix("<!--") {
            rest = tail.split_once("-->").map_or("", |(_, tail)| tail);
            continue;
        }
        let Some((tag, tail)) = split_tag(rest) else {
            // A `<` that opens nothing: the rest of the document is text.
            out.push_str(rest);
            rest = "";
            break;
        };
        if is_block(&tag_name(tag)) {
            out.push('\n');
        }
        rest = tail;
    }
    out.push_str(rest);
    tidy(&decode_entities(&out))
}

/// Split `<…>` off the front of `rest`, respecting quoted attribute values.
///
/// `<img alt="a > b"/>` is one tag, and a scan to the first `>` would cut it
/// in half and spill `b"/>` into the text. Returns the tag's inside and what
/// follows it, or `None` for a `<` that never closes.
fn split_tag(rest: &str) -> Option<(&str, &str)> {
    let mut quote: Option<char> = None;
    for (i, ch) in rest.char_indices().skip(1) {
        match quote {
            Some(open) if ch == open => quote = None,
            Some(_) => {}
            None if ch == '"' || ch == '\'' => quote = Some(ch),
            None if ch == '>' => return Some((&rest[1..i], &rest[i + ch.len_utf8()..])),
            None => {}
        }
    }
    None
}

/// The element name inside a tag body, lowercased and without its `/`.
fn tag_name(tag: &str) -> String {
    tag.trim_start_matches('/')
        .split(|c: char| c.is_ascii_whitespace() || c == '/' || c == '>')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase()
}

/// Whether this element ends a line of text.
///
/// The XHTML block elements a Confluence body actually uses, plus the `ac:`
/// elements that behave like blocks: a macro, a layout cell, a task. Anything
/// unrecognised is treated as **inline**, which is the direction that only
/// ever costs a missing line break -- treating an unknown inline element as a
/// block would chop a sentence in half in the search index.
fn is_block(name: &str) -> bool {
    matches!(
        name,
        "p" | "div"
            | "br"
            | "hr"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "li"
            | "ul"
            | "ol"
            | "dl"
            | "dt"
            | "dd"
            | "table"
            | "thead"
            | "tbody"
            | "tr"
            | "th"
            | "td"
            | "blockquote"
            | "pre"
            | "section"
            | "ac:layout"
            | "ac:layout-section"
            | "ac:layout-cell"
            | "ac:structured-macro"
            | "ac:rich-text-body"
            | "ac:plain-text-body"
            | "ac:task"
            | "ac:task-list"
    )
}

/// The XML entities a storage-format body carries, plus numeric references.
///
/// `&nbsp;` becomes an ordinary space rather than U+00A0: what this feeds is a
/// full-text index and a launcher excerpt, and a non-breaking space there is a
/// character no search will ever be typed with.
fn decode_entities(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        let Some((entity, tail)) = rest[1..]
            .find(';')
            .filter(|end| *end <= 12)
            .map(|end| (&rest[1..=end], &rest[end + 2..]))
        else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        match entity {
            "amp" => out.push('&'),
            "lt" => out.push('<'),
            "gt" => out.push('>'),
            "quot" => out.push('"'),
            "apos" | "#39" => out.push('\''),
            "nbsp" | "#160" => out.push(' '),
            _ => match numeric(entity) {
                Some(ch) => out.push(ch),
                // An entity knobas does not know is left exactly as it was
                // written: inventing a character for it would put a guess in
                // the search index, and `&hellip;` read as text is worse only
                // cosmetically.
                None => {
                    out.push('&');
                    out.push_str(entity);
                    out.push(';');
                }
            },
        }
        rest = tail;
    }
    out.push_str(rest);
    out
}

/// `&#8212;` / `&#x2014;` -- the numeric references a Confluence editor writes
/// for a dash, a quote or an ellipsis.
fn numeric(entity: &str) -> Option<char> {
    let digits = entity.strip_prefix('#')?;
    let code = match digits.strip_prefix(['x', 'X']) {
        Some(hex) => u32::from_str_radix(hex, 16).ok()?,
        None => digits.parse::<u32>().ok()?,
    };
    char::from_u32(code)
}

/// Collapse whitespace, drop blank lines, trim.
fn tidy(raw: &str) -> String {
    raw.lines()
        .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The seeded fixture page, in the storage format
    /// `testenv/seed-atlassian-content.sh` builds for it -- the exact body the
    /// live suite reads back off the real server.
    const SEEDED: &str = "<h2>Backoff policy</h2><p>base 30 s, factor 2, max 5 attempts, \
                          jitter ±10 %. Transient codes: HTTP 503, <code>TEMP_UNAVAILABLE</code>. \
                          After the final attempt the payout is moved to the manual review queue \
                          (see PAY-231).</p><h2>Manual review queue</h2><p><em>SLA: to be \
                          defined.</em></p>";

    #[test]
    fn a_heading_and_a_paragraph_become_two_lines() {
        assert_eq!(
            to_text("<h2>Backoff policy</h2><p>base 30 s</p>"),
            "Backoff policy\nbase 30 s"
        );
    }

    /// Inline elements are not boundaries: a word wrapped in `<code>` is still
    /// the same word, and a line break inside it would split a search term.
    #[test]
    fn an_inline_element_leaves_the_words_joined() {
        assert_eq!(to_text("<p>set <code>MAX</code>=5</p>"), "set MAX=5");
        assert_eq!(to_text("un<em>break</em>able"), "unbreakable");
    }

    /// The whole seeded body, so the mapping's normalized text is pinned
    /// against the fixture rather than against an invented sample.
    #[test]
    fn the_seeded_page_body_reads_as_its_two_sections() {
        let text = to_text(SEEDED);
        assert_eq!(
            text,
            "Backoff policy\nbase 30 s, factor 2, max 5 attempts, jitter ±10 %. Transient \
             codes: HTTP 503, TEMP_UNAVAILABLE. After the final attempt the payout is moved to \
             the manual review queue (see PAY-231).\nManual review queue\nSLA: to be defined."
        );
        assert!(!text.contains('<'), "no markup survives: {text}");
    }

    /// gotcha 7, and the reason the order is what it is: escaped markup in the
    /// storage format is prose, and must come out as the prose it is rather
    /// than be decoded into a tag and then deleted.
    #[test]
    fn escaped_markup_is_text_and_survives_as_text() {
        assert_eq!(
            to_text("<p>write &lt;script&gt;alert(1)&lt;/script&gt; here</p>"),
            "write <script>alert(1)</script> here"
        );
        assert_eq!(to_text("<p>a &amp;lt; b</p>"), "a &lt; b");
    }

    /// An attribute value containing `>` is one tag, not one and a half. A
    /// scan to the first `>` spills the rest of the attribute into the text.
    #[test]
    fn a_greater_than_inside_an_attribute_does_not_end_the_tag() {
        assert_eq!(
            to_text("<p><img alt=\"a &gt; b\" title='x>y'/>after</p>"),
            "after"
        );
    }

    /// A code macro keeps its code: the body is inside CDATA precisely because
    /// it contains characters a tag scanner would otherwise eat.
    #[test]
    fn a_macro_body_in_cdata_is_kept_as_text() {
        let text = to_text(
            "<ac:structured-macro ac:name=\"code\"><ac:plain-text-body>\
             <![CDATA[if (a < b) { retry(); }]]></ac:plain-text-body></ac:structured-macro>",
        );
        assert_eq!(text, "if (a < b) { retry(); }");
    }

    #[test]
    fn a_comment_in_the_markup_contributes_nothing() {
        assert_eq!(to_text("<p>before<!-- a note -->after</p>"), "beforeafter");
        // An unterminated comment eats the rest rather than emitting markup.
        assert_eq!(to_text("<p>before<!-- forever"), "before");
    }

    /// The entities a Confluence editor actually writes, and the one it does
    /// not: an unknown entity is left alone rather than guessed at.
    #[test]
    fn entities_decode_and_an_unknown_one_is_left_alone() {
        assert_eq!(
            to_text("<p>a &amp; b &quot;c&quot; d&apos;e&nbsp;f &#8212; g &#x2014; h</p>"),
            "a & b \"c\" d'e f — g — h"
        );
        assert_eq!(to_text("<p>&hellip;</p>"), "&hellip;");
        // An `&` that opens nothing is an `&`, and what follows it is still
        // the author's words.
        assert_eq!(
            to_text("<p>100 &amp still counts</p>"),
            "100 &amp still counts"
        );
    }

    /// A list is lines, not one run-on sentence -- which is what makes a
    /// runbook's steps readable in the detail view and matchable in search.
    #[test]
    fn a_list_and_a_table_break_into_lines() {
        assert_eq!(
            to_text("<ul><li>first</li><li>second</li></ul>"),
            "first\nsecond"
        );
        assert_eq!(
            to_text("<table><tbody><tr><td>a</td><td>b</td></tr></tbody></table>"),
            "a\nb"
        );
    }

    /// A page with no body at all -- four of the five fixture pages, and every
    /// page somebody creates and does not fill in.
    #[test]
    fn an_empty_body_is_an_empty_string_and_never_a_panic() {
        assert_eq!(to_text(""), "");
        assert_eq!(to_text("   \n  "), "");
        assert_eq!(to_text("<p></p>"), "");
        // Malformed input is text, not a panic: a body that would not parse
        // must still be searchable.
        assert_eq!(to_text("<p>unclosed"), "unclosed");
        assert_eq!(to_text("a < b"), "a < b");
    }
}
