//! Roadmap §4 gotcha 2, as a test rather than as a convention.
//!
//! sqlx 0.9 will not execute a runtime-built `String` unless it is wrapped in
//! the crate's assert-safe marker type. That wrapper is the injection speed
//! bump, and gotcha 2 confines it to *one reviewed query-builder module*. This
//! is what keeps that true when the next feature is in a hurry: `src/sql.rs` is
//! the module, and no other file in this crate may so much as name the type.
//!
//! The needle is assembled at run time from two halves, and no comment in this
//! file spells the name out -- the scan walks this file too, and a test that
//! reported itself would be a test nobody could keep green.
//!
//! Confined to *this crate*, not to the workspace: interfaces §10.2 records the
//! two other legitimate uses (`knobas-db`'s `create database`, where the
//! database name cannot be a bind, and a `knobas-sync` test's setup). Gotcha 2
//! asks for runtime SQL to live in reviewed places, not for the type to be
//! unused. This crate's builder is the only one of the three on a path a user's
//! keystrokes reach, which is why it gets a test and they get a review.

use std::path::{Path, PathBuf};

/// Directories of this crate that are scanned, relative to its manifest.
const SCANNED: &[&str] = &["src", "tests", "benches"];

/// The file allowed to name it.
const ALLOWED: &str = "sql.rs";

#[test]
fn the_runtime_sql_wrapper_appears_only_in_the_query_builder() {
    let needle = concat!("AssertSql", "Safe");
    let crate_root = Path::new(env!("CARGO_MANIFEST_DIR"));

    let mut scanned = 0_usize;
    let mut offenders: Vec<PathBuf> = Vec::new();
    for directory in SCANNED {
        let root = crate_root.join(directory);
        if !root.exists() {
            continue;
        }
        for path in rust_files(&root) {
            scanned += 1;
            if path.file_name().is_some_and(|name| name == ALLOWED) {
                continue;
            }
            let source = std::fs::read_to_string(&path).expect("a readable source file");
            if source.contains(needle) {
                offenders.push(path);
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "dynamic SQL escaped {ALLOWED}: {offenders:?}"
    );
    // A scan that walked nothing would pass silently, which is the one way this
    // test can rot into decoration.
    assert!(scanned > 3, "the scan found only {scanned} source files");
    assert!(
        crate_root
            .join("src")
            .join(ALLOWED)
            .exists_and_names(needle),
        "{ALLOWED} no longer names the wrapper: either it moved, or this test \
         is now checking nothing"
    );
}

/// Every `.rs` file under `root`, recursively.
fn rust_files(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![root.to_path_buf()];
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

/// Small helper so the positive control reads as one assertion.
trait NamesTheWrapper {
    fn exists_and_names(&self, needle: &str) -> bool;
}

impl NamesTheWrapper for PathBuf {
    fn exists_and_names(&self, needle: &str) -> bool {
        std::fs::read_to_string(self).is_ok_and(|source| source.contains(needle))
    }
}
