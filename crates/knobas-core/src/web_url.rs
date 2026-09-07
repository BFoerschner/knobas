//! The one spelling of "these two URLs name the same thing".
//!
//! A reader pastes a link from chat into the launcher and expects the entity
//! it names to open (spec #491 stories 9-11, 15-17). The mirror already holds
//! the address every adapter reported, in `sync.item.web_url`; what it did not
//! hold until issue #496 was a way to *find* a row by it. This module is that
//! rule, as SQL, in one place.
//!
//! # Why the rule is SQL and not Rust
//!
//! Matching happens on **both** sides -- the stored URL and the pasted one --
//! and it is backed by an expression index, so the rule exists in the database
//! whatever else happens. A Rust twin normalising the pasted side would be a
//! second implementation of one rule, in a different language, that nothing
//! forces to agree; `ancestor_path_read!` records the same reasoning in
//! `payload.rs`, and `knobas-core/tests/ancestor_path.rs` is written against
//! the database for it. So the pasted URL is bound as a parameter and put
//! through [`web_url_normalized!`] too, and the two sides cannot disagree
//! because they are the same characters.
//!
//! The migration that creates the index carries a **third** copy, because an
//! index definition cannot expand a Rust macro. That copy is not load-bearing
//! for correctness: if it drifted, the query would still answer correctly and
//! would merely stop using the index. It is load-bearing for *speed*, which is
//! why `crates/knobas-app/tests/url_resolve.rs` asks the planner whether the
//! shipped statement still reaches the index -- the same check
//! `the_view_still_reaches_the_fts_index` makes for the launcher's GIN index.
//!
//! # The rule
//!
//! Four normalisations, and no fifth (spec story 11):
//!
//! * **The fragment goes.** `#comment-42` addresses a place inside a page, not
//!   a different page.
//! * **A trailing slash goes**, from the *path* -- `…/PAY-231/` and
//!   `…/PAY-231` are one ticket.
//! * **The host is lower-cased**, with the scheme, because a host is
//!   case-insensitive and `JIRA.example` is typed by autocomplete more often
//!   than by hand.
//! * **The query stays, verbatim.** A Confluence
//!   `viewpage.action?pageId=42` link's identity *is* its query, so a
//!   normalisation that dropped it would resolve every page of an instance to
//!   whichever one was indexed first. This is the one place where doing less
//!   is the requirement rather than the shortcut.
//!
//! Nothing else: no percent-decoding, no default-port removal, no query
//! reordering, no `www.` folding. Each of those is a claim about what two URLs
//! mean, and the ticket asks for exact matching after the four rules above.
//!
//! # The failure direction is absence
//!
//! ADR-0007's discipline for a read that is not the adapter's: this one
//! **misses rather than guesses**. A value that is not an absolute URL --
//! anything without a `scheme://` -- normalises to SQL `null`, because both
//! `substring`s fail to match and `null || null` is `null`. A `null` index
//! entry equals nothing, so such a row is unreachable by paste rather than
//! reachable by accident, and `crates/knobas-core/tests/web_url.rs` pins that
//! direction across the shapes a mirror can hold.

/// SQL for the *trailing slash and fragment* half of the rule, over a URL
/// expression.
///
/// One `regexp_replace` does both, and keeps the query, because all three
/// live at the tail of a URL in a fixed order: optional slashes, an optional
/// `?query`, an optional `#fragment`, end of string. The pattern is anchored
/// on `$` and the engine takes the leftmost match, so on
/// `https://h/x?a=b/` the match starts at the `?` and the whole query --
/// trailing slash and all -- is put back by `\1`.
///
/// Not part of the public rule on its own; [`web_url_normalized!`] is what a
/// caller wants. It is a macro of its own only so that the expression appears
/// once rather than three times inside its sibling.
#[macro_export]
macro_rules! web_url_tail_trimmed {
    ($url:literal) => {
        concat!("regexp_replace(", $url, r", '/*(\?[^#]*)?(#.*)?$', '\1')")
    };
}

/// SQL for "this URL, as the resolver compares it" -- see the module docs for
/// the rule and for why it is not written in Rust.
///
/// `$url` is the URL expression as a **literal** (`"i.web_url"`, `"$1::text"`),
/// because that is what `concat!` folds: every statement built with this is a
/// `&'static str`, so nothing here can concatenate a value into SQL. The same
/// convention `ancestor_path_read!` follows.
///
/// The expression is `IMMUTABLE` throughout -- `regexp_replace`, `substring`,
/// `lower` and `||` all are -- which is what lets an index be built on it.
///
/// The two `substring`s split the URL at the end of its authority: the first
/// takes `scheme://host[:port]` and lower-cases it, the second takes
/// everything after it verbatim. Splitting rather than lower-casing the whole
/// string is the point -- a Jira key is upper-case (`/browse/PAY-231`), a
/// Gitea path is case-sensitive, and folding either would make two different
/// pages one.
#[macro_export]
macro_rules! web_url_normalized {
    ($url:literal) => {
        concat!(
            "(lower(substring(",
            $crate::web_url_tail_trimmed!($url),
            " from '^[^:/?#]+://[^/?#]*')) || substring(",
            $crate::web_url_tail_trimmed!($url),
            " from '^[^:/?#]+://[^/?#]*(.*)$'))"
        )
    };
}
