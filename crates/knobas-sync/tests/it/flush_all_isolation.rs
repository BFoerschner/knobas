//! Whole-database flushes stay out of the files that assert a wait reason,
//! as a test rather than as a header (#620).
//!
//! `flush_all` reaches *every* configured source in a module's database, and
//! so does the real scheduler's flush task, which every tick wakes
//! (`crate::write_queue::flush_all`, from `scheduler.rs`). Each test harness's secret store holds only its own
//! source's credential, so a flush started by one test fails to build the
//! other tests' sources and re-reasons their due writes `unauthorized`. A
//! test in the same module that asserts `Unreachable` then reads back
//! `Unauthorized` -- but only when the flush lands between its write and its
//! reload, which an idle machine almost never does. This mix caused the
//! flake twice: once when `write_activity.rs` was split out of
//! `write_queue.rs`, and again in #609. Neither showed on an idle machine,
//! so review and `just check` cannot be relied on to catch a third.
//!
//! The rule: **no file in `tests/it/` that names the wait reason in code may
//! call `flush_all(` or `Scheduler::start(`.** Files are picked out by that
//! rule rather than by a list, so a new module that mixes the two is caught
//! as well. Comment lines are dropped first, the way `write_choke_point.rs`
//! does it: a header that explains the rule by naming both is not the mix.
//!
//! Every needle in this file is assembled with `concat!`, so the file does
//! not match itself.

use std::path::{Path, PathBuf};

/// The two spellings of a wait reason in code: the enum, and the row field.
const WAIT_REASON: [&str; 2] = [concat!("Wait", "Reason"), concat!("wait", "_reason")];

/// The two ways to flush every source: by hand, or by starting the real
/// scheduler, whose flush task does what `flush_all` does.
const FLUSHES_EVERYTHING: [&str; 2] = [concat!("flush_all", "("), concat!("Scheduler::start", "(")];

/// A file's code, with its comment lines removed.
///
/// The same filter as `write_choke_point.rs`'s: only whole lines that begin
/// with `//` are dropped, so a trailing comment never hides code.
fn code_only(source: &str) -> String {
    source
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn names_why_a_write_waits(code: &str) -> bool {
    WAIT_REASON.iter().any(|needle| code.contains(needle))
}

fn flushes_everything(code: &str) -> bool {
    FLUSHES_EVERYTHING
        .iter()
        .any(|needle| code.contains(needle))
}

/// Whether `source` both asserts on a wait reason and flushes every source.
///
/// Pure, so the rule can be checked against inline sources -- see
/// `the_rule_actually_rejects_a_flush_beside_a_wait_reason`.
fn offends(source: &str) -> bool {
    let code = code_only(source);
    names_why_a_write_waits(&code) && flushes_everything(&code)
}

#[test]
fn no_file_that_asserts_a_wait_reason_flushes_every_source() {
    let directory = tests_it();

    let mut scanned = 0_usize;
    let mut names_a_reason: Vec<String> = Vec::new();
    let mut flushes_all: Vec<String> = Vec::new();
    let mut offenders: Vec<String> = Vec::new();
    for path in rust_files(&directory) {
        let name = path
            .file_name()
            .expect("a file name")
            .to_string_lossy()
            .into_owned();
        scanned += 1;
        let source = std::fs::read_to_string(&path).expect("a readable source file");
        let code = code_only(&source);
        if names_why_a_write_waits(&code) {
            names_a_reason.push(name.clone());
        }
        if flushes_everything(&code) {
            flushes_all.push(name.clone());
        }
        // Through `offends` rather than the two lists above, so the scan runs
        // the function the self-test below pins.
        if offends(&source) {
            offenders.push(name);
        }
    }

    assert!(
        offenders.is_empty(),
        "these files assert a write's wait reason and also flush every source: \
         {offenders:?}\n\
         `flush_all` (and the real scheduler's flush task) fails to build \
         the other tests' sources and re-reasons their rows `unauthorized`, so a \
         wait-reason assertion in the same database flakes under load (#609, #620). \
         Move the flushing test to `write_queue_flush_all.rs`."
    );

    // Positive controls: a scan that matches nothing passes vacuously.
    assert!(
        scanned >= 10,
        "the scan read only {scanned} files -- it is not looking at tests/it/"
    );
    assert!(
        names_a_reason.iter().any(|name| name == "write_queue.rs"),
        "write_queue.rs no longer names a wait reason in code ({names_a_reason:?}) \
         -- the file was renamed, or the needle no longer matches"
    );
    assert!(
        flushes_all
            .iter()
            .any(|name| name == "write_queue_flush_all.rs"),
        "write_queue_flush_all.rs no longer flushes every source in code ({flushes_all:?}) \
         -- the file was renamed, or the needle no longer matches"
    );
}

/// The rule's teeth, on inline sources. Without this, an `offends` that
/// always answered `false` would leave the scan above green.
#[test]
fn the_rule_actually_rejects_a_flush_beside_a_wait_reason() {
    let asserts_a_reason = concat!(
        "assert_eq!(row.wait",
        "_reason, Some(Wait",
        "Reason::Unreachable));"
    );
    let by_hand = concat!("flusher::flush_all", "(&h.deps).await;");
    let by_scheduler = concat!("let s = Scheduler::start", "(deps);");

    assert!(
        offends(&format!("{asserts_a_reason}\n{by_hand}")),
        "a wait-reason assertion beside a flush by hand must be rejected"
    );
    assert!(
        offends(&format!("{asserts_a_reason}\n{by_scheduler}")),
        "a wait-reason assertion beside the real scheduler must be rejected"
    );
    assert!(
        offends(&format!(
            "{}\n{by_hand}",
            concat!("use knobas_core::write_queue::Wait", "Reason;")
        )),
        "importing the enum is naming it"
    );

    assert!(
        !offends(by_hand),
        "a flush with no wait reason in the file is the flush_all module's job"
    );
    assert!(
        !offends(asserts_a_reason),
        "a wait-reason assertion with no flush is what write_queue.rs is for"
    );
    assert!(
        !offends(&format!("{asserts_a_reason}\n    // {by_hand}")),
        "a flush named only in a comment is not a call"
    );
    assert!(
        !offends(&format!(
            "{}\n{by_hand}",
            concat!("//! Nothing here asserts a write's `wait", "_reason`.")
        )),
        "a header that names the wait reason is not code"
    );
}

/// This crate's `tests/it/`.
fn tests_it() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("it")
}

/// Every `.rs` file under `directory`, recursively.
fn rust_files(directory: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![directory.to_path_buf()];
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
