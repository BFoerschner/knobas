//! The launcher's `?` card, checked against the parser it claims to describe.
//!
//! Ruling P2 puts the whole grammar in [`knobas_search::query`], which means
//! the card in `app/src/lib/launcher/` is documentation of code it cannot see.
//! A help card that lies is worse than none: someone types what it told them
//! to, gets a narrower result set than they asked for, and has no way to tell.
//!
//! So the card's table is data (`app/src/lib/launcher/syntax.json`), every row
//! carries a `probe`, and this file runs every probe through
//! [`knobas_search::parse`]. The assertions are **behavioural** -- what the
//! parser did with the string -- rather than a scan for the token's text
//! somewhere in the parser's source, which the token appearing in a comment
//! would satisfy.
//!
//! No database: a grammar is a string function, and
//! [`knobas_search::Vocabulary::fixture`] supplies the sources and kinds the
//! probes resolve against.

use std::collections::BTreeSet;

use knobas_search::{Prefix, Vocabulary, parse};

/// One row of the card, as `syntax.json` spells it.
#[derive(Debug, serde::Deserialize)]
struct Entry {
    token: String,
    insert: String,
    summary: String,
    probe: String,
    prefix: Option<String>,
    expect: String,
}

fn card() -> Vec<Entry> {
    let text = include_str!("../../../app/src/lib/launcher/syntax.json");
    serde_json::from_str(text).expect("syntax.json is a list of card entries")
}

/// The parser's own spelling of a prefix -- taken from serde, so it is the
/// wire spelling the card has to use rather than a third transcription.
fn wire(prefix: Prefix) -> String {
    serde_json::to_value(prefix)
        .expect("a prefix serializes")
        .as_str()
        .expect("as a string")
        .to_owned()
}

/// Every row of the card does what it says.
///
/// `recognised` means the parser honoured the token: nothing greyed out, and
/// the advertised prefix claimed. `reported` means the parser deliberately
/// refused it and said so -- which is the promise the card makes by drawing
/// that row greyed with a reason, and is exactly the state `author:` is in
/// while open question **E-Q1** is unanswered.
#[test]
fn every_row_of_the_help_card_does_what_it_says() {
    let vocab = Vocabulary::fixture();
    let rows = card();
    assert!(rows.len() >= 10, "the card walk found nothing: {rows:?}");

    for entry in &rows {
        let parsed = parse(&entry.probe, &vocab);
        let claimed = parsed.query.prefix.map(wire);

        assert_eq!(
            claimed.as_deref(),
            entry.prefix.as_deref(),
            "`{}` claims prefix {:?}; the parser made {:?} of {:?}",
            entry.token,
            entry.prefix,
            claimed,
            entry.probe
        );

        match entry.expect.as_str() {
            "recognised" => assert!(
                parsed.query.unknown_tokens.is_empty(),
                "the card offers `{}` as a filter, but the parser greys {:?} out: {:?}",
                entry.token,
                entry.probe,
                parsed.query.unknown_tokens
            ),
            "reported" => assert!(
                !parsed.query.unknown_tokens.is_empty(),
                "the card greys `{}` out with a reason, but the parser accepted {:?} \
                 -- so the card is telling people a working filter does nothing",
                entry.token,
                entry.probe
            ),
            other => panic!("`{}` has an unknown expectation {other:?}", entry.token),
        }
    }
}

/// Every prefix the grammar has is on the card.
///
/// Driven by [`Prefix::ALL`]-shaped enumeration over the real enum rather than
/// by a list written here, so a variant added to the grammar fails this test
/// until the card documents it. This is the coverage half; the test above is
/// the truthfulness half, and neither implies the other -- a card can be
/// entirely truthful about half a grammar.
#[test]
fn every_prefix_the_grammar_has_is_documented() {
    let documented: BTreeSet<String> = card().into_iter().filter_map(|e| e.prefix).collect();

    // Spelled out rather than iterated, because `Prefix` is a plain `enum`
    // with no `ALL`: the point is that adding a variant to it makes *this
    // line* fail to compile (a non-exhaustive match below), which is a
    // stronger signal than a missing set member.
    let all = [
        Prefix::Action,
        Prefix::Ticket,
        Prefix::Person,
        Prefix::Source,
        Prefix::Time,
        Prefix::Note,
        Prefix::List,
        Prefix::Asset,
        Prefix::Help,
    ];
    for prefix in all {
        // The compiler's half: a new variant makes this match non-exhaustive.
        match prefix {
            Prefix::Action
            | Prefix::Ticket
            | Prefix::Person
            | Prefix::Source
            | Prefix::Time
            | Prefix::Note
            | Prefix::List
            | Prefix::Asset
            | Prefix::Help => {}
        }
        assert!(
            documented.contains(&wire(prefix)),
            "{prefix:?} is a launcher prefix nobody documented: the `?` card lists {documented:?}"
        );
    }
}

/// Clicking a row never turns the grammar into a search word.
///
/// The card's rows are buttons that put their `insert` into the box (spec §4),
/// and `insert` differs from `probe` on purpose: half these rows are inserted
/// *incomplete* (`kind:`, `updated:`) for the person to finish typing, and the
/// parser reports an incomplete key rather than filtering on the empty string
/// -- deliberately, and pinned in `query.rs`'s own tests. So "the insert parses
/// clean" is the wrong property; **it was the first thing this test asserted,
/// and `kind:` failed it correctly.**
///
/// What actually matters is the failure that is invisible: a marker the parser
/// does not know is silently demoted to a *search term*, which widens the
/// result set with no chip, no grey token and nothing on screen to point at. A
/// single typo in this table (`kinds:` for `kind:`) is exactly that bug, and it
/// is what this pins.
#[test]
fn what_a_row_inserts_never_becomes_a_search_word() {
    let vocab = Vocabulary::fixture();
    for entry in card() {
        let inserted = entry.insert.trim();
        let parsed = parse(&entry.insert, &vocab);
        assert!(
            !parsed.query.text.contains(inserted),
            "clicking `{}` inserts {:?}, and the parser read it as search text {:?} \
             -- a filter that silently became a word",
            entry.token,
            entry.insert,
            parsed.query.text
        );
        assert!(
            !entry.summary.is_empty(),
            "`{}` has no explanation, which is the only reason the card exists",
            entry.token
        );
    }
}
