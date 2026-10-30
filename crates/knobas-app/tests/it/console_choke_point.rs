//! Every console program knobas starts goes through `knobas_db::console`
//! (#2232), as a test rather than as a convention.
//!
//! The release app has no console, so on Windows a program started with a bare
//! `Command::new` opens a window. The Code view's git reads did, on every
//! window focus, and each window took the focus back. `console::command` asks
//! for no window; this reads the tree and fails on a production
//! `Command::new` anywhere else, the device `write_choke_point.rs` uses for
//! the write queue.
//!
//! Production is a file under a crate's `src/` up to its `#[cfg(test)]` test
//! module. Three files may name it anyway:
//!
//! 1. **The builder**, `knobas-db/src/console.rs`.
//! 2. **"Open in…"**, `knobas-app/src/checkout/open.rs`. It starts the reader's
//!    editor or terminal, which they asked to see.
//! 3. **The test support** `knobas-db/src/test_util.rs`, compiled only for
//!    tests (`test-util`).

use std::path::{Path, PathBuf};

const NEEDLE: &str = concat!("Command::", "new(");

const MAY_NAME_IT: &[&str] = &[
    "knobas-db/src/console.rs",
    "knobas-app/src/checkout/open.rs",
    "knobas-db/src/test_util.rs",
];

/// `source` without its test module and its comment lines.
fn production(source: &str) -> String {
    let lines: Vec<&str> = source.lines().collect();
    let end = lines
        .windows(2)
        .position(|pair| pair[0].trim() == "#[cfg(test)]" && pair[1].starts_with("mod tests"))
        .unwrap_or(lines.len());
    lines[..end]
        .iter()
        .filter(|line| !line.trim_start().starts_with("//"))
        .copied()
        .collect::<Vec<_>>()
        .join("\n")
}

fn offends(relative: &str, source: &str) -> bool {
    !MAY_NAME_IT.contains(&relative) && production(source).contains(NEEDLE)
}

#[test]
fn every_console_program_is_started_through_the_builder() {
    let crates = workspace_crates();
    let mut scanned = 0_usize;
    let mut offenders: Vec<String> = Vec::new();
    for path in rust_files(&crates) {
        let relative = path
            .strip_prefix(&crates)
            .expect("a path under crates/")
            .to_string_lossy()
            .replace('\\', "/");
        scanned += 1;
        let source = std::fs::read_to_string(&path).expect("a readable source file");
        if offends(&relative, &source) {
            offenders.push(relative);
        }
    }

    assert!(
        offenders.is_empty(),
        "these files start a program without `knobas_db::console::command`, so on \
         Windows it opens a window: {offenders:?}"
    );
    assert!(
        scanned > 50,
        "the scan walked only {scanned} files: it is not looking at the workspace"
    );
    for allowed in MAY_NAME_IT {
        assert!(
            crates.join(allowed).exists(),
            "{allowed} moved: its exemption now covers nothing"
        );
    }
}

#[test]
fn the_rule_rejects_a_bare_start_and_spares_tests() {
    let bare = "fn read() { let c = tokio::process::Command::new(git); }";
    assert!(offends("knobas-app/src/checkout/working_tree.rs", bare));
    assert!(!offends("knobas-app/src/checkout/open.rs", bare));

    let in_tests = "fn read() {}\n#[cfg(test)]\nmod tests {\n    fn f() { std::process::Command::new(\"git\"); }\n}";
    assert!(!offends(
        "knobas-app/src/checkout/working_tree.rs",
        in_tests
    ));

    let commented = "// std::process::Command::new(\"git\") would open a window\nfn read() {}";
    assert!(!offends(
        "knobas-app/src/checkout/working_tree.rs",
        commented
    ));
}

fn workspace_crates() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates/ is this crate's parent")
        .to_path_buf()
}

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
