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

/// Who a storage body's user links are resolved against.
///
/// Confluence writes a mention as `<ac:link><ri:user ri:userkey="…"/></ac:link>`
/// -- a **key**, not a name -- so a body_text built by stripping tags carries
/// no trace of who was mentioned. This is the one identity the adapter can
/// resolve a key to, and it is the only one the inbox asks about: the account
/// the source is configured with, read from `/rest/api/user/current` at the
/// start of every run ([`crate::model::CurrentUser`]).
///
/// ADR-0007's shape, in one sentence: **one named read, and a miss where the
/// shape is absent.** A link naming somebody else's key resolves to nothing
/// rather than to a guess -- which is the direction that costs a mention
/// knobas never claims, instead of claiming somebody else's as mine.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Account<'a> {
    /// The username contract §4.1 spells `author` with, and the string the
    /// inbox's mention rule and `@me` match against.
    pub username: &'a str,
    /// `userKey` -- the stable id a Confluence editor writes into a mention.
    /// `None` on an instance that did not report one, which makes every
    /// key-shaped link a miss.
    pub user_key: Option<&'a str>,
}

/// The storage format stripped to text.
///
/// Block-level elements become line breaks so `<h2>A</h2><p>B</p>` reads as
/// two lines rather than as `AB`; inline elements vanish so `a<em>b</em>c`
/// reads as `abc`. Runs of whitespace collapse, blank lines go, and the
/// result is trimmed -- what is left is what a person would have typed.
///
/// **One element does not vanish: `ri:user`.** A mention is markup, and
/// stripping it would delete the only thing that says the body names somebody
/// -- so it is rendered back to the `@name` a person typed to make it, which
/// is the spelling `knobas_core::inbox`'s mention rule reads and the spelling
/// that module already documents Confluence as using. `ri:username` renders
/// whoever it names; `ri:userkey` renders only [`Account`]'s own key, because
/// that is the one key this adapter can resolve. See [`Account`] for the
/// direction the misses fall in.
#[must_use]
pub(crate) fn to_text(storage: &str, me: Option<Account<'_>>) -> String {
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
        let name = tag_name(tag);
        if name == "ri:user" {
            if let Some(mentioned) = mentioned_name(tag, me) {
                // A trailing space, always: the mention rule requires the
                // username not be followed by another name character, and a
                // `<ac:plain-text-link-body>` right behind the link would
                // otherwise run its text straight onto the name and hide the
                // mention. `tidy` collapses the space away where nothing
                // follows.
                out.push('@');
                out.push_str(mentioned);
                out.push(' ');
            }
        } else if is_block(&name) {
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

/// Which name a `ri:user` element renders as, if any.
///
/// Two attributes, in the order Confluence prefers them: `ri:username` names
/// the account outright and is rendered whoever it is, and `ri:userkey` is
/// rendered only when it is [`Account`]'s own -- an unresolvable key is a
/// miss, never a guess.
fn mentioned_name<'a>(tag: &'a str, me: Option<Account<'a>>) -> Option<&'a str> {
    if let Some(name) = attr(tag, "ri:username").filter(|name| !name.is_empty()) {
        return Some(name);
    }
    let me = me?;
    let key = attr(tag, "ri:userkey").filter(|key| !key.is_empty())?;
    (Some(key) == me.user_key).then_some(me.username)
}

/// One attribute's value out of a tag body, or `None` for an attribute that is
/// absent or unquoted.
///
/// The name must be **preceded by whitespace and followed by `=`**, because
/// `ri:username` contains `ri:user` and a substring search for one would find
/// the other. Whitespace and not "or the start of the tag": the first token in
/// a tag body is the element name, never an attribute, so requiring the space
/// costs nothing and needs no second case. Deliberately total: a malformed tag
/// yields no attribute rather than an error, which is this module's whole
/// discipline (a body that would not parse is a page that is not searchable).
fn attr<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let mut rest = tag;
    loop {
        let at = rest.find(name)?;
        let before_ok = at > 0
            && rest[..at]
                .chars()
                .next_back()
                .is_some_and(|c| c.is_ascii_whitespace());
        let after = &rest[at + name.len()..];
        let value = after.trim_start();
        if before_ok && value.starts_with('=') {
            let value = value[1..].trim_start();
            let quote = value.chars().next().filter(|c| *c == '"' || *c == '\'')?;
            let value = &value[quote.len_utf8()..];
            return value.find(quote).map(|end| &value[..end]);
        }
        rest = after;
    }
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

/// Plain text in, **storage format** out -- the other direction, and the only
/// one a write takes (issue #286).
///
/// `WriteOp::Comment` carries what the reader typed, because it is the same op
/// Jira and Gitea take and their comment fields are plain text. Confluence's
/// is not: a body posted as `representation: "storage"` is XHTML, so a `<` in
/// somebody's prose would either be dropped by the wiki's own sanitizer or
/// stored as the start of an element they did not write. Translating here is
/// this adapter's job, and it is the same reasoning as [`to_text`]'s in
/// reverse.
///
/// The rules, and they are deliberately few:
///
/// * `&`, `<` and `>` are escaped, `&` **first** -- the mirror image of
///   [`to_text`]'s "strip before decoding". Escaping `<` first would then
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

    /// A body with no `ri:user` in it renders identically whoever is asking,
    /// so every test that is not about a mention reads it with no account --
    /// and shadowing the name here keeps that claim in one place rather than
    /// in thirty call sites.
    fn to_text(storage: &str) -> String {
        super::to_text(storage, None)
    }

    /// The account the seeded Confluence's admin is, as
    /// `/rest/api/user/current` reports it.
    fn me() -> Account<'static> {
        Account {
            username: "knobas",
            user_key: Some("ff8080818f2a1b4c018f2a1c9d0e0001"),
        }
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
            super::to_text(
                "<p><ac:link><ri:user ri:username=\"knobas\" /></ac:link> can you add the \
                 SLA?</p>",
                Some(me())
            ),
            "@knobas can you add the SLA?"
        );
        assert_eq!(
            super::to_text(
                "<p><ac:link><ri:user ri:userkey=\"ff8080818f2a1b4c018f2a1c9d0e0001\" \
                 /></ac:link> can you add the SLA?</p>",
                Some(me())
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
            super::to_text(
                "<p><ac:link><ri:user ri:username=\"mara.lindqvist\" /></ac:link> ping</p>",
                Some(me())
            ),
            "@mara.lindqvist ping"
        );
        assert_eq!(
            super::to_text(
                "<p><ac:link><ri:user ri:userkey=\"2c9080f0-somebody-else\" /></ac:link> \
                 ping</p>",
                Some(me())
            ),
            "ping",
            "an unresolvable key is a miss, not a guess"
        );
        // And with no account at all -- a source whose `username` was never
        // filled in -- a key resolves to nothing while a name still renders.
        assert_eq!(
            super::to_text(
                "<p><ac:link><ri:user ri:userkey=\"ff8080818f2a1b4c018f2a1c9d0e0001\" \
                 /></ac:link> ping</p>",
                None
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
                super::to_text(&format!("<p>before{tag}after</p>"), Some(me())),
                "beforeafter",
                "{tag} must render as nothing"
            );
        }
        // A `<` that never closes is the text it was and nothing is
        // rendered from it -- the module's own rule, unchanged by mentions,
        // and the reason a truncated body is searchable rather than empty.
        assert_eq!(
            super::to_text("<p>before<ri:user ri:username=\"knobas\"", Some(me())),
            "before<ri:user ri:username=\"knobas\""
        );
    }

    /// The rendered name is **delimited**, because the mention rule requires
    /// the username not be followed by another name character. A mention whose
    /// link carries a plain-text body would otherwise read as `@knobasknobas`
    /// and match nobody.
    #[test]
    fn a_rendered_mention_is_delimited_from_whatever_follows_it() {
        let text = super::to_text(
            "<p><ac:link><ri:user ri:username=\"knobas\" /><ac:plain-text-link-body>\
             <![CDATA[knobas]]></ac:plain-text-link-body></ac:link>?</p>",
            Some(me()),
        );
        assert_eq!(text, "@knobas knobas?");
    }
}
