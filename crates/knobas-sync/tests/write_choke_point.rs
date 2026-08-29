//! One outbound write path, as a test rather than as a convention (issue #42,
//! story 23).
//!
//! `Source::write` is the single seam through which everything knobas sends to
//! a source passes, and the write queue **wraps** it: the queue is what decides
//! send, pend or hold. A queue that can be bypassed is not a queue, so this
//! reads the tree and fails if a second call site appears -- the same device
//! `knobas-search`'s `sql_containment.rs` uses to confine runtime SQL, and
//! `app/src/lib/shell/house-rules.test.ts` uses on the frontend.
//!
//! ## What is checked, and why it is spelled this way
//!
//! Not "who calls `.write(`" -- that phrase also spells an `RwLock` guard and
//! an `OpenOptions` builder, and a scan that has to tell those apart is a scan
//! that can be fooled. Instead: **which production files so much as name
//! `WriteOp`.** Nothing can call `Source::write` without one, so naming it is
//! the necessary condition, and it is unambiguous.
//!
//! Three kinds of file may name it, and they are different things:
//!
//! 1. **The choke point** -- `knobas_sync::write_queue`, which is the only
//!    place that *originates* a write.
//! 2. **The SPI's own conformance harness** -- `knobas_source::contract`, whose
//!    job is to hand an adapter a write and check it is refused. It certifies
//!    adapters; it is not an application write path.
//! 3. **Anything that declares or implements the trait** -- the SPI itself, the
//!    four adapters, and the sync engine's `Observed` decorator. These *are*
//!    `Source::write`, or forward it; none of them decides that a write should
//!    happen.
//!
//! Everything else is an offender. A new feature that wants to write to a
//! source has to go through `knobas_sync::write_queue::submit`, and the way it
//! finds that out is this test rather than a reviewer noticing.
//!
//! ## Scope: `src/`, not `tests/`
//!
//! Deliberate. An adapter's own test suite calls `write` to prove the adapter
//! refuses an op it never declared -- that is the contract battery's clause,
//! not a bypass of the queue. What must not exist is a *shipping* write path
//! beside the queue, and that lives in `src/`.

use std::path::{Path, PathBuf};

/// The one module allowed to originate a write, relative to `crates/`.
const CHOKE_POINT: &str = "knobas-sync/src/write_queue.rs";

/// The SPI's conformance harness: it writes in order to certify that an
/// adapter refuses, which is the battery's job and nobody else's.
const BATTERY: &str = "knobas-source/src/contract.rs";

/// A file's code, with its comment lines removed.
///
/// *Mentioning* the write op in prose is not calling it -- `knobas-core`'s
/// store documents the payload it holds and cannot call the SPI at all, since
/// `knobas-source` depends on it rather than the reverse. Only whole lines
/// that begin with `//` are dropped, never the tail of a code line: a trailing
/// comment naming the op sits on a line a reviewer should be reading anyway,
/// and stripping from the first `//` would also cut a URL in a string literal
/// -- a false *negative*, which is the direction this test may not fail in.
fn code_only(source: &str) -> String {
    source
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// A file names the trait rather than merely calling it.
///
/// `impl Source for` covers every adapter and the sync engine's progress
/// decorator; `trait Source` covers the SPI's own declaration.
fn implements_the_trait(source: &str) -> bool {
    source.contains("impl Source for") || source.contains("trait Source")
}

/// Whether `source`, living at `relative`, is a second write path.
///
/// Pure, so the rule itself can be checked against inputs that do not have to
/// exist in the tree -- see `the_rule_actually_rejects_a_second_call_site`.
fn offends(relative: &str, source: &str, needle: &str) -> bool {
    let code = code_only(source);
    if !code.contains(needle) {
        return false;
    }
    if relative == CHOKE_POINT || relative == BATTERY {
        return false;
    }
    !implements_the_trait(&code)
}

#[test]
fn only_the_write_queue_may_send_to_a_source() {
    // Assembled at run time so that nothing in this file is itself the string
    // being hunted -- a test that reported itself is one nobody can keep green.
    let needle = concat!("Write", "Op");
    let crates = workspace_crates();

    let mut scanned = 0_usize;
    let mut named_it: Vec<String> = Vec::new();
    let mut offenders: Vec<String> = Vec::new();
    for path in rust_files(&crates) {
        let relative = path
            .strip_prefix(&crates)
            .expect("a path under crates/")
            .to_string_lossy()
            .replace('\\', "/");
        scanned += 1;
        let source = std::fs::read_to_string(&path).expect("a readable source file");
        if code_only(&source).contains(needle) {
            named_it.push(relative.clone());
        }
        if offends(&relative, &source, needle) {
            offenders.push(relative);
        }
    }

    assert!(
        offenders.is_empty(),
        "these files reach the source write path without going through the write \
         queue: {offenders:?}\n\
         Every outbound write goes through `knobas_sync::write_queue::submit`, \
         which is what decides send, pend or hold. See issue #42 and ADR-0006."
    );

    // Three positive controls, because the failure mode of a scan is passing
    // vacuously.
    assert!(
        scanned > 50,
        "the scan walked only {scanned} files -- it is not looking at the workspace"
    );
    assert!(
        crates.join(CHOKE_POINT).exists(),
        "the choke point moved: this test is now checking nothing"
    );
    assert!(
        named_it.len() >= 5,
        "only {named_it:?} name the write op -- the needle no longer matches the SPI"
    );
}

/// The rule's teeth, checked directly: a file that names the write op and does
/// not implement the trait is rejected, wherever it lives.
///
/// Without this, deleting the `offends` body and returning `false` would leave
/// the scan above green -- it would report an empty offender list, which is
/// exactly what it reports when the tree is clean.
#[test]
fn the_rule_actually_rejects_a_second_call_site() {
    let needle = concat!("Write", "Op");
    let calls_it = "async fn post(&self) { self.source.write(WriteOp::Comment { .. }).await }";

    assert!(
        offends("knobas-app/src/commands/entity.rs", calls_it, needle),
        "a command that writes to a source directly must be rejected"
    );
    assert!(
        offends("knobas-sync/src/scheduler.rs", calls_it, needle),
        "not even this crate's other modules may originate a write"
    );
    assert!(
        !offends(CHOKE_POINT, calls_it, needle),
        "the queue itself is the write path"
    );
    assert!(
        !offends(
            "knobas-source-jira/src/source.rs",
            "impl Source for JiraSource { async fn write(&self, op: WriteOp) {} }",
            needle,
        ),
        "an adapter implementing the trait is not a second path"
    );
    assert!(
        !offends("knobas-search/src/sql.rs", "fn build() {}", needle),
        "a file that never names the write op cannot call it"
    );
    assert!(
        !offends(
            "knobas-core/src/write_queue.rs",
            "/// The serialized `WriteOp`.\npub payload: serde_json::Value,",
            needle,
        ),
        "documenting the op is not calling it"
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
