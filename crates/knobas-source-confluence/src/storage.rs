//! Confluence **storage format**: the scanning helpers the projection
//! ([`crate::blocks`]) and the mention lookup ([`crate::users`]) read it
//! with, and plain text into storage format for the writes.
//!
//! What `body_text` holds is not read here. Since #2435 it is the shared
//! search text over the page's projection (`knobas_source::docs::search_text`,
//! [`crate::map`]): search follows the drawing, so a macro's parameters and
//! the CDATA of a macro the reader does not draw as text are not searched.
//!
//! # Why a hand-rolled scanner and not an HTML parser
//!
//! Storage format is not HTML: it is XML-shaped, it carries namespaced
//! elements no HTML parser models, and its CDATA sections hold the bodies of
//! code macros. A scanner that never fails is worth more than a parser that
//! sometimes does, because the failure mode of a body that would not parse is
//! a page that is neither drawn nor searchable.
//!
//! # Order matters, and it is the security-relevant half
//!
//! Tags are split off **before** entities are decoded. `&lt;script&gt;` in
//! the storage format is the *text* `<script>`, and it must come out as that
//! text; decoding first would manufacture a tag out of prose and then strip
//! it, silently deleting what the author wrote.

// The tag and entity reads are the shared HTML reader's (spec #2432), so a
// tag splits and an entity decodes the same way in the projection and the
// mention lookup.
pub(crate) use knobas_source::docs::html::{attr, split_tag, tag_name};

/// Plain text in, **storage format** out -- the other direction, and the only
/// one a write takes (issue #286).
///
/// `WriteOp::Comment` carries what the reader typed, because it is the same op
/// Jira and Gitea take and their comment fields are plain text. Confluence's
/// is not: a body posted as `representation: "storage"` is XHTML, so a `<` in
/// somebody's prose would either be dropped by the wiki's own sanitizer or
/// stored as the start of an element they did not write. Translating here is
/// this adapter's job.
///
/// The rules, and they are deliberately few:
///
/// * `&`, `<` and `>` are escaped, `&` **first** -- the mirror image of
///   the projection's "split tags before decoding". Escaping `<` first would then
///   escape the `&` of the `&lt;` it just wrote and produce `&amp;lt;`.
/// * A blank line starts a new `<p>`; a single newline inside a paragraph
///   becomes `<br/>`. That is what the Confluence editor does with the same
///   keystrokes, so a comment reads back the way it was typed.
/// * Text that is only whitespace produces the empty string rather than an
///   empty `<p>`. Confluence refuses an empty comment with a 400 carrying its
///   own sentence, which is a better message than one knobas would invent.
///
/// No attribute is ever composed here, so `"` and `'` are left alone: they are
/// legal character data and escaping them would put `&quot;` in the reader's
/// prose.
#[must_use]
pub(crate) fn from_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 16);
    for paragraph in text.replace("\r\n", "\n").split("\n\n") {
        let lines: Vec<&str> = paragraph
            .split('\n')
            .map(str::trim_end)
            .skip_while(|line| line.trim().is_empty())
            .collect();
        // `skip_while` cannot reach a trailing blank run, so trim the tail too.
        let mut lines = lines.as_slice();
        while lines.last().is_some_and(|line| line.trim().is_empty()) {
            lines = &lines[..lines.len() - 1];
        }
        if lines.is_empty() {
            continue;
        }
        out.push_str("<p>");
        for (index, line) in lines.iter().enumerate() {
            if index > 0 {
                out.push_str("<br/>");
            }
            out.push_str(&escape(line));
        }
        out.push_str("</p>");
    }
    out
}

/// The Docs editor's plain text in, storage format out (#862): what a section
/// edit and a new page are written in (R-10.13, R-10.16).
///
/// `## ` (one to six `#`) starts a heading, `- ` or `* ` a list item, a blank
/// line a new paragraph, and a single newline inside one is a `<br/>`.
/// Everything else is text, escaped as [`from_text`] escapes it: no other
/// markup is read, so what a reader typed arrives as typed.
#[must_use]
pub(crate) fn from_plain(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 32);
    let mut paragraph: Vec<&str> = Vec::new();
    let mut items: Vec<&str> = Vec::new();
    let flush = |out: &mut String, paragraph: &mut Vec<&str>, items: &mut Vec<&str>| {
        if !paragraph.is_empty() {
            out.push_str("<p>");
            out.push_str(
                &paragraph
                    .iter()
                    .map(|l| escape(l))
                    .collect::<Vec<_>>()
                    .join("<br/>"),
            );
            out.push_str("</p>");
            paragraph.clear();
        }
        if !items.is_empty() {
            out.push_str("<ul>");
            for item in items.iter() {
                out.push_str("<li>");
                out.push_str(&escape(item));
                out.push_str("</li>");
            }
            out.push_str("</ul>");
            items.clear();
        }
    };
    let text = text.replace("\r\n", "\n");
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            flush(&mut out, &mut paragraph, &mut items);
            continue;
        }
        let hashes = trimmed.len() - trimmed.trim_start_matches('#').len();
        if (1..=6).contains(&hashes)
            && let Some(heading) = trimmed[hashes..].strip_prefix(' ')
        {
            flush(&mut out, &mut paragraph, &mut items);
            out.push_str(&format!(
                "<h{hashes}>{}</h{hashes}>",
                escape(heading.trim())
            ));
            continue;
        }
        if let Some(item) = trimmed
            .strip_prefix("- ")
            .or_else(|| trimmed.strip_prefix("* "))
        {
            if !paragraph.is_empty() {
                let mut none = Vec::new();
                flush(&mut out, &mut paragraph, &mut none);
            }
            items.push(item.trim());
            continue;
        }
        if !items.is_empty() {
            let mut none = Vec::new();
            flush(&mut out, &mut none, &mut items);
        }
        paragraph.push(trimmed);
    }
    flush(&mut out, &mut paragraph, &mut items);
    out
}

/// A panel macro (`info`, `note`, `warning`, `tip`) around `body`, which is
/// already storage format, with a `title` parameter when `title` is not empty
/// (*Edit section*, #975).
#[must_use]
pub(crate) fn panel(name: &str, title: &str, body: &str) -> String {
    let title = if title.is_empty() {
        String::new()
    } else {
        format!(
            "<ac:parameter ac:name=\"title\">{}</ac:parameter>",
            escape(title)
        )
    };
    format!(
        "<ac:structured-macro ac:name=\"{name}\" ac:schema-version=\"1\">{title}\
         <ac:rich-text-body>{body}</ac:rich-text-body></ac:structured-macro>"
    )
}

/// The three characters that are markup in character data, escaped in the one
/// order that is not self-defeating.
fn escape(raw: &str) -> String {
    raw.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The `body_text` a page with this body and no title gets
    /// (`knobas_source::docs::search_text` over the projection, #2435), the
    /// sync having looked up the credential's own key -- what
    /// [`crate::users::Directory`] always knows -- when `me` is set. A body
    /// with no `ri:user` in it reads the same either way.
    fn body_text(storage: &str, me: bool) -> String {
        let mut people = crate::blocks::People::new();
        if me {
            people.insert(
                "ff8080818f2a1b4c018f2a1c9d0e0001".to_owned(),
                crate::blocks::Person {
                    username: Some("knobas".to_owned()),
                    display_name: Some("knobas".to_owned()),
                },
            );
        }
        let content = knobas_source::docs::DocContent {
            version: None,
            editor_name: None,
            labels: Vec::new(),
            blocks: crate::blocks::to_blocks(storage, &people),
            comments: Vec::new(),
            sections: Vec::new(),
            history: Vec::new(),
        };
        knobas_source::docs::search_text("", "", &content)
    }

    fn to_text(storage: &str) -> String {
        body_text(storage, false)
    }

    // -- plain text out to the wiki (issue #286) ---------------------------

    /// The whole point of the direction: what a reader types is **character
    /// data**, and a wiki that received it as markup would either eat it or
    /// store an element nobody wrote. `&` is escaped first, so the escapes
    /// this function writes are not escaped again.
    #[test]
    fn a_comment_is_escaped_before_it_becomes_markup() {
        assert_eq!(
            from_text("use <script> & the a > b case"),
            "<p>use &lt;script&gt; &amp; the a &gt; b case</p>"
        );
        // Not `&amp;lt;`: the order is what keeps this from doubling.
        assert!(!from_text("<b>").contains("&amp;lt;"));
    }

    /// A round trip through both directions is the pair's own check: the words
    /// a reader typed come back as the words they typed, brackets included.
    #[test]
    fn what_a_reader_typed_survives_both_directions() {
        let typed = "3 < 4 && \"quoted\" stays";
        assert_eq!(to_text(&from_text(typed)), typed);
    }

    /// Blank line, new paragraph; single newline, a break -- which is what the
    /// Confluence editor does with the same keystrokes.
    #[test]
    fn blank_lines_are_paragraphs_and_single_newlines_are_breaks() {
        assert_eq!(
            from_text("first\nsecond\n\nthird"),
            "<p>first<br/>second</p><p>third</p>"
        );
        // A Windows newline is the same keystroke.
        assert_eq!(from_text("a\r\n\r\nb"), "<p>a</p><p>b</p>");
        // Runs of blank lines are one break, and leading and trailing ones
        // produce no empty paragraph.
        assert_eq!(from_text("\n\n  \na\n\n\n\nb\n\n"), "<p>a</p><p>b</p>");
    }

    /// Nothing typed is nothing sent -- **not** an empty `<p>`. Confluence
    /// refuses an empty comment with its own sentence, which is a better
    /// message than one knobas would compose.
    #[test]
    fn text_that_is_only_whitespace_produces_no_markup() {
        assert_eq!(from_text(""), "");
        assert_eq!(from_text("   \n\n \t \n"), "");
    }

    /// Quotes and apostrophes are legal character data here, and escaping them
    /// would put `&quot;` in somebody's prose. No attribute is ever composed
    /// by this function, which is what makes that safe.
    #[test]
    fn quotes_are_left_alone_because_no_attribute_is_ever_composed() {
        assert_eq!(from_text("she said \"no\""), "<p>she said \"no\"</p>");
    }

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
            "\u{1e}Backoff policy\nbase 30 s"
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
            "\u{1e}Backoff policy\nbase 30 s, factor 2, max 5 attempts, jitter ±10 %. Transient \
             codes: HTTP 503, TEMP_UNAVAILABLE. After the final attempt the payout is moved to \
             the manual review queue (see PAY-231).\n\u{1e}Manual review queue\nSLA: to be defined."
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

    /// **The detection rule, stated as a test.** A Confluence mention is
    /// `<ac:link><ri:user …/></ac:link>`, and what reaches the mirror's
    /// `body_text` is the `@name` a person typed to make it -- which is what
    /// `knobas_core::inbox`'s mention rule reads.
    ///
    /// Both spellings, because a Data Center editor writes the key and the
    /// REST API accepts the name: `ri:username` names the account outright,
    /// `ri:userkey` is resolved against the one key this adapter knows.
    #[test]
    fn a_user_link_renders_as_the_at_name_the_mention_rule_reads() {
        assert_eq!(
            body_text(
                "<p><ac:link><ri:user ri:username=\"knobas\" /></ac:link> can you add the \
                 SLA?</p>",
                true
            ),
            "@knobas can you add the SLA?"
        );
        assert_eq!(
            body_text(
                "<p><ac:link><ri:user ri:userkey=\"ff8080818f2a1b4c018f2a1c9d0e0001\" \
                 /></ac:link> can you add the SLA?</p>",
                true
            ),
            "@knobas can you add the SLA?"
        );
    }

    /// **The negative control, and the one that matters.** A link naming
    /// somebody *else* renders as somebody else -- a renderer that resolved
    /// every key to the configured account would put every colleague's
    /// mentions in my inbox, which is the failure this pins.
    ///
    /// An unresolvable key renders as **nothing**: ADR-0007's miss direction,
    /// and the cost is a mention of somebody knobas is not configured as.
    #[test]
    fn a_link_that_is_not_mine_never_renders_as_me() {
        assert_eq!(
            body_text(
                "<p><ac:link><ri:user ri:username=\"mara.lindqvist\" /></ac:link> ping</p>",
                true
            ),
            "@mara.lindqvist ping"
        );
        assert_eq!(
            body_text(
                "<p><ac:link><ri:user ri:userkey=\"2c9080f0-somebody-else\" /></ac:link> \
                 ping</p>",
                true
            ),
            "ping",
            "an unresolvable key is a miss, not a guess"
        );
        // And with no account at all -- a source whose `username` was never
        // filled in -- a key resolves to nothing while a name still renders.
        assert_eq!(
            body_text(
                "<p><ac:link><ri:user ri:userkey=\"ff8080818f2a1b4c018f2a1c9d0e0001\" \
                 /></ac:link> ping</p>",
                false
            ),
            "ping"
        );
    }

    /// A malformed or empty user link contributes nothing and takes nothing
    /// down with it: an element with no attribute at all, an attribute with no
    /// value, an unquoted value, an empty value, and the `ri:username`
    /// substring hazard -- `attr` must not read `ri:username`'s value when it
    /// was asked for `ri:userkey`, nor find `ri:userkey` inside a longer
    /// attribute name.
    #[test]
    fn a_malformed_user_link_is_a_miss_and_the_body_still_reads() {
        for tag in [
            "<ri:user/>",
            "<ri:user ri:userkey/>",
            "<ri:user ri:userkey=/>",
            "<ri:user ri:userkey=ff8080818f2a1b4c018f2a1c9d0e0001/>",
            "<ri:user ri:userkey=\"\"/>",
            "<ri:user ri:username=\"\"/>",
            "<ri:user data-ri:userkey=\"ff8080818f2a1b4c018f2a1c9d0e0001\"/>",
        ] {
            assert_eq!(
                body_text(&format!("<p>before{tag}after</p>"), true),
                "beforeafter",
                "{tag} must render as nothing"
            );
        }
        // A `<` that never closes is the text it was and nothing is
        // rendered from it -- the module's own rule, unchanged by mentions,
        // and the reason a truncated body is searchable rather than empty.
        assert_eq!(
            body_text("<p>before<ri:user ri:username=\"knobas\"", true),
            "before<ri:user ri:username=\"knobas\""
        );
    }

    /// The rendered name is **delimited**, because the mention rule requires
    /// the username not be followed by another name character. A mention whose
    /// link carries a plain-text body would otherwise read as `@knobasknobas`
    /// and match nobody.
    #[test]
    fn a_rendered_mention_is_delimited_from_whatever_follows_it() {
        let text = body_text(
            "<p><ac:link><ri:user ri:username=\"knobas\" /><ac:plain-text-link-body>\
             <![CDATA[knobas]]></ac:plain-text-link-body></ac:link>?</p>",
            true,
        );
        // The link's body is the name the reader draws, not more words.
        assert_eq!(text, "@knobas ?");
    }

    #[test]
    fn plain_text_becomes_headings_lists_and_paragraphs() {
        assert_eq!(
            from_plain("Intro <b>\nsecond line\n\n## Steps\n- one & two\n* three\nAfter."),
            "<p>Intro &lt;b&gt;<br/>second line</p><h2>Steps</h2>\
             <ul><li>one &amp; two</li><li>three</li></ul><p>After.</p>"
        );
        assert_eq!(from_plain("  \n "), "");
        assert_eq!(from_plain("#hashtag"), "<p>#hashtag</p>");
    }
}
