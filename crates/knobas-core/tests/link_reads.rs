//! One place decides what a link *is*, as a test rather than as a convention
//! (issue #161).
//!
//! Migration `0007` (issue #41) put two populations in one table:
//! `knobas.link` now holds **proposals** as well as confirmed links, told apart
//! by `confirmed_at` -- `NULL` is a machine-made guess nobody accepted. The
//! migration answers that with two views whose predicates are each other's
//! negation, `knobas.confirmed_link` and `knobas.proposed_link`, so that "the
//! links panel shows confirmed links only, and the tray shows proposals only"
//! is a property of the schema rather than a `where` clause each reader has to
//! remember. #41's own words: *"the links panel showing an unconfirmed guess
//! would be a correctness bug, not a cosmetic one"*.
//!
//! A reader that goes to the base table instead gets both populations and no
//! error. That is the failure this scan makes impossible rather than merely
//! discouraged: **no shipping module may read `knobas.link` except the two that
//! own the distinction.** Everything else reads a view, and picking one is the
//! moment the question "confirmed, or proposed?" gets asked.
//!
//! ## What is checked, and why it is spelled this way
//!
//! Not "who names `knobas.link`" -- prose names it constantly, and must:
//! `link.rs` explains why it reads the view *instead of* the table, and this
//! file's own header does the same. A scan that forbade the name would forbid
//! the explanation. What it looks for is the table in a **read position**:
//! `from knobas.link` and `join knobas.link`. `insert into knobas.link` and
//! `update knobas.link` are how a link or a proposal comes to exist at all,
//! neither can be confused with the other population, and neither needle
//! matches them.
//!
//! `delete from knobas.link` does match, because the two words the needle wants
//! are adjacent there too, and the message it would print names the wrong
//! reason. Nothing in the tree hits it: a link is removed by a tombstone
//! (`link::unlink` writes `update knobas.link set deleted_at = now()`), and a
//! hard delete would want an argument of its own anyway. Whoever writes the
//! first one gets a report about seeing both populations and should read this
//! paragraph instead.
//!
//! Comments are scanned along with code, deliberately, and that is the whole
//! point: issue #161's actual defect was a **doc comment** in
//! `knobas-search/src/lists.rs` promising a future predicate over the base
//! table. Nothing executed it, and it would have been copied into something
//! that did. This is the mirror image of `knobas-sync`'s
//! `write_choke_point.rs`, which strips comments before matching because
//! *mentioning* a write op is not calling one; here, instructing a reader is
//! the offence.
//!
//! Whitespace is collapsed and the text lower-cased before matching, so a
//! clause broken across two lines of a multi-line SQL literal is still seen.
//! The character after the needle must not continue an identifier, so
//! `knobas.link_pair_idx` is not mistaken for the table.
//!
//! ## What it cannot see
//!
//! The needle wants `from` and the table next to each other in the source
//! text, so SQL that is assembled rather than written is out of its reach.
//! `knobas-search/src/sql.rs` is the one place in the tree that assembles it:
//!
//! ```text
//! let _ = write!(sql, "    from {}", corpus.relation);
//! ```
//!
//! `Corpus::relation` is a `&'static str` (`corpus.rs`: `relation:
//! "knobas.note n"`), so a corpus added later as `relation: "knobas.link l"`
//! would be a shipping read of the base table that this test walks past, in the
//! same crate #161's defect was in. The smart lists are not exposed that way --
//! `lists.rs` builds its SQL from `concat!` over whole clauses, so a future
//! `"    from knobas.confirmed_link l\n"` is one literal and the scan reads it
//! -- but a clause split across two `concat!` arguments would slip through as
//! well.
//!
//! Neither is worth a second needle today: there is no link corpus and no plan
//! for one, and a rule over string literals that merely start with the table's
//! name would report a table-name constant as a read. What the two cost is the
//! scope of the claim. This test covers SQL written as text, which is all of
//! the SQL there is right now.
//!
//! ## The three exemptions
//!
//! All three read the base table on purpose, and all three say why in place:
//!
//! * [`LINK`] -- `link::unlink` re-reads the row by id to tell "already gone"
//!   from "never existed", a question about the row rather than about which
//!   population it is in.
//! * [`SUGGEST`] -- detection's suppression reads the pair in both directions
//!   and **ignores every filter**, which is what makes a dismissal and an
//!   unlink the same fact to the detector (#40 story 12). Reading a view there
//!   would re-propose what the user threw away.
//! * [`ATTACH`] -- the sync engine's monitor-name resolution (issue #453) asks
//!   whether a pair already carries an **active row of any kind** before it
//!   inserts one. That is `link_pair_active_idx`'s own question (`0011`:
//!   unique over the unordered pair `where deleted_at is null`, which does not
//!   care about `confirmed_at`), and it is the *only* reader here that is
//!   asking about the index rather than about a population. Reading
//!   `knobas.confirmed_link` there would insert over `monitor_url_host`'s
//!   proposal (#478) and take the whole poll down with a unique violation
//!   every minute -- pinned, from the other side, by `knobas-sync`'s
//!   `a_proposal_over_the_same_pair_is_left_alone`. The first exemption
//!   outside `knobas-core`, and the reason it can be is that it reads the
//!   table to *avoid* deciding which population a row is in rather than to
//!   decide it.
//!
//! Growing this list is allowed; doing it without noticing is not. Each entry
//! is a decision a reviewer sees in a diff, the same treatment
//! `write_choke_point.rs` gives `HANDS_TO_THE_QUEUE`.
//!
//! ## Scope: `src/`, not `tests/`
//!
//! Deliberate, and the same call `write_choke_point.rs` makes. A test that
//! asserts on `knobas.link` directly is asking about the *table* -- that
//! detection wrote no confirmed row, that a tombstone is still there, that the
//! backfill filled `confirmed_at` -- and going through a view would hide the
//! very thing it is checking. What must not exist is a *shipping* read that
//! blurs the two populations, and that lives in `src/`.

use std::path::{Path, PathBuf};

/// `link::unlink`'s existence probe, relative to `crates/`.
const LINK: &str = "knobas-core/src/link.rs";

/// Detection's unfiltered, both-directions suppression, relative to `crates/`.
const SUGGEST: &str = "knobas-core/src/suggest.rs";

/// The sync engine's monitor-name resolution, relative to `crates/` -- the
/// reader that asks `link_pair_active_idx`'s question rather than a
/// population's (issue #453). See the header.
const ATTACH: &str = "knobas-sync/src/attach.rs";

/// The two spellings of the table in a read position.
const READS: &[&str] = &["from knobas.link", "join knobas.link"];

/// `source` with whitespace runs collapsed to one space and everything
/// lower-cased.
///
/// A multi-line SQL literal breaks `from` and the table name across lines about
/// as often as it keeps them together, so matching the raw text would find one
/// spelling and miss the other -- a false *negative*, the direction this scan
/// may not fail in.
fn flattened(source: &str) -> String {
    source
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// Whether `source` reads the base table.
///
/// The character after the needle may not continue an identifier: the schema
/// carries `link_pair_idx`, `link_proposed_idx` and `link_origin_chk`, and a
/// prefix match would report all of them.
fn reads_the_base_table(source: &str) -> bool {
    let flat = flattened(source);
    READS.iter().any(|needle| {
        flat.match_indices(needle).any(|(at, _)| {
            !flat[at + needle.len()..]
                .chars()
                .next()
                .is_some_and(|next| next.is_ascii_alphanumeric() || next == '_')
        })
    })
}

/// Whether `source`, living at `relative`, blurs the two populations.
///
/// Pure, so the rule can be checked against inputs that do not have to exist in
/// the tree -- see `the_rule_actually_rejects_a_read_of_the_base_table`.
fn offends(relative: &str, source: &str) -> bool {
    !EXEMPT.contains(&relative) && reads_the_base_table(source)
}

/// The three modules allowed to read the base table, in the header's order.
///
/// A list rather than a chain of `!=`, now that there are three: growing it is
/// allowed and is meant to be one line a reviewer sees, which is
/// `write_choke_point.rs`' `HANDS_TO_THE_QUEUE` treatment.
const EXEMPT: &[&str] = &[LINK, SUGGEST, ATTACH];

#[test]
fn only_the_exempt_modules_read_the_base_table() {
    let crates = workspace_crates();

    let mut scanned = 0_usize;
    let mut offenders: Vec<String> = Vec::new();
    let mut exempt_still_read: Vec<&str> = Vec::new();
    for path in rust_files(&crates) {
        let relative = path
            .strip_prefix(&crates)
            .expect("a path under crates/")
            .to_string_lossy()
            .replace('\\', "/");
        scanned += 1;
        let source = std::fs::read_to_string(&path).expect("a readable source file");
        if let Some(exempt) = EXEMPT.iter().find(|allowed| **allowed == relative)
            && reads_the_base_table(&source)
        {
            exempt_still_read.push(*exempt);
        }
        if offends(&relative, &source) {
            offenders.push(relative);
        }
    }

    assert!(
        offenders.is_empty(),
        "these files read `knobas.link` directly, so they see proposals and confirmed links \
         alike: {offenders:?}\n\
         Read `knobas.confirmed_link` for links the user agreed to, or `knobas.proposed_link` \
         for the suggestion tray's -- migration 0007 made them each other's negation so that \
         neither reader can drift onto the other's rows. A doc comment counts: it is what the \
         next implementer copies. See issue #161 and issue #41."
    );

    // Positive controls, because the failure mode of a scan is passing
    // vacuously.
    assert!(
        scanned > 50,
        "the scan walked only {scanned} files -- it is not looking at the workspace"
    );
    assert!(
        exempt_still_read.len() == EXEMPT.len(),
        "only {exempt_still_read:?} of the {} exempt modules still read the base table -- \
         either the read moved and the exemption is now a hole, or the matcher stopped matching \
         and this test is checking nothing",
        EXEMPT.len()
    );
}

/// The rule's teeth, checked directly.
///
/// Without this, `offends` could be gutted to `false` and the scan above would
/// still report an empty offender list -- which is exactly what it reports when
/// the tree is clean.
#[test]
fn the_rule_actually_rejects_a_read_of_the_base_table() {
    let planned = "//! and exists (select 1 from knobas.link l where l.from_id = i.entity_id)";

    assert!(
        offends("knobas-search/src/lists.rs", planned),
        "a smart list that probes the base table would count unconfirmed guesses as links"
    );
    assert!(
        offends("knobas-app/src/commands/entity.rs", planned),
        "not even a command in the shell may decide for itself what a link is"
    );
    assert!(
        !offends(LINK, planned),
        "the module that owns the read is the one place it is allowed"
    );
    assert!(
        !offends(SUGGEST, planned),
        "detection's suppression reads the table on purpose"
    );
    assert!(
        !offends(ATTACH, planned),
        "the monitor-name resolution reads the table to ask the unique index's own question"
    );

    // A read broken across lines is the normal shape of a multi-line SQL
    // literal in this workspace, and it must not be the way past the rule.
    assert!(
        offends(
            "knobas-search/src/lists.rs",
            "select 1\n            from\n              knobas.link l"
        ),
        "a `from` and its table on two lines is still a read of the base table"
    );
    assert!(
        offends(
            "knobas-app/src/entity.rs",
            "select e.* from knobas.entity e JOIN KNOBAS.LINK l on l.to_id = e.id"
        ),
        "joining the base table reads it, and case does not take it out of the rule"
    );

    // What the rule must *not* catch, or it becomes a rule people route around.
    assert!(
        !offends(
            "knobas-search/src/lists.rs",
            "//! `knobas.link` holds proposals too, so this reads knobas.confirmed_link \
             -- see migration 0007."
        ),
        "explaining why the base table is not read must stay sayable"
    );
    assert!(
        !offends(
            "knobas-core/src/note.rs",
            "insert into knobas.link (from_id, to_id, relation) values ($1, $2, $3)"
        ),
        "writing a link is not reading one, and a write cannot confuse the two populations"
    );
    assert!(
        offends(
            "knobas-core/src/note.rs",
            "delete from knobas.link where id = $1"
        ),
        "a hard delete is caught too, because `from knobas.link` is in it -- see the header: \
         the report names the wrong reason, and nothing in the tree hits it"
    );
    assert!(
        !offends(
            "knobas-db/src/lib.rs",
            "create index link_pair_idx on knobas.link_pair (from_id)"
        ),
        "an identifier that merely starts with the table's name is not the table"
    );
    assert!(
        !offends(
            "knobas-core/src/link.rs",
            "select id from knobas.confirmed_link where id = $1"
        ),
        "a file that reads a view is what this rule is asking for"
    );
}

/// `crates/`, found from this crate's manifest.
fn workspace_crates() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates/ is this crate's parent")
        .to_path_buf()
}

/// Every `.rs` file under each crate's `src/`.
fn rust_files(crates: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack: Vec<PathBuf> = std::fs::read_dir(crates)
        .expect("a readable crates directory")
        .map(|entry| entry.expect("a readable entry").path().join("src"))
        .filter(|src| src.is_dir())
        .collect();
    while let Some(directory) = stack.pop() {
        for entry in std::fs::read_dir(&directory).expect("a readable directory") {
            let path = entry.expect("a readable entry").path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                found.push(path);
            }
        }
    }
    found
}
