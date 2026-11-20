//! Source scans that hold decisions §3 and §4 (ADR-0048): a site's view is
//! wry's, never Tauri's, and is handed no IPC, no scheme and no script but
//! the named ones.

use std::fs;
use std::path::{Path, PathBuf};

fn rust_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).expect("a source folder") {
            let path = entry.expect("an entry").path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

/// `text` with `//` comments removed, line by line, so a comment that names
/// a call is no call.
fn code(text: &str) -> String {
    text.lines()
        .map(|line| match line.find("//") {
            Some(at) => &line[..at],
            None => line,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn site_src() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

/// The wry builder calls that would hand a page knobas's IPC, a scheme of
/// knobas's, or a script at document creation.
const NEVER: &[&str] = &[
    "with_ipc_handler",
    "with_custom_protocol",
    "with_asynchronous_custom_protocol",
    "with_initialization_script",
    "with_initialization_script_for_main_only",
];

#[test]
fn a_site_is_handed_no_ipc_no_scheme_and_no_script_at_load() {
    let mut found = Vec::new();
    for file in rust_files(&site_src()) {
        let text = code(&fs::read_to_string(&file).expect("readable"));
        for name in NEVER {
            if text.contains(name) {
                found.push(format!("{}: {name}", file.display()));
            }
        }
    }
    assert!(
        found.is_empty(),
        "a site's view must not get these: {found:#?}"
    );
}

/// The scripts knobas may run in a site: `scripts.rs`'s named constants.
const SCRIPTS: &[&str] = &["DIRTY_PROBE"];

#[test]
fn a_site_runs_only_the_named_scripts() {
    let mut found = Vec::new();
    for file in rust_files(&site_src()) {
        let text = code(&fs::read_to_string(&file).expect("readable"));
        for call in ["evaluate_script(", "evaluate_script_with_callback("] {
            let mut rest = text.as_str();
            while let Some(at) = rest.find(call) {
                let after = rest[at + call.len()..].trim_start();
                let argument: String = after
                    .chars()
                    .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_' || *ch == ':')
                    .collect();
                let named = SCRIPTS
                    .iter()
                    .any(|name| argument == *name || argument.ends_with(&format!("::{name}")));
                if !named {
                    found.push(format!("{}: {call}{argument}…", file.display()));
                }
                rest = &rest[at + call.len()..];
            }
        }
    }
    assert!(
        found.is_empty(),
        "only the named scripts run in a site: {found:#?}"
    );
}

#[test]
fn no_web_view_in_the_app_is_made_by_tauri_but_the_two_windows() {
    let app = Path::new(env!("CARGO_MANIFEST_DIR")).join("../knobas-app/src");
    let allowed = ["capture.rs", "bundle.rs"];
    let mut found = Vec::new();
    for root in [app, site_src()] {
        for file in rust_files(&root) {
            if allowed.iter().any(|name| file.ends_with(name)) {
                continue;
            }
            let text = code(&fs::read_to_string(&file).expect("readable"));
            for name in ["add_child", "WebviewBuilder", "WebviewUrl::External"] {
                if text.contains(name) {
                    found.push(format!("{}: {name}", file.display()));
                }
            }
        }
    }
    assert!(
        found.is_empty(),
        "a site is a raw wry view, never Tauri's: {found:#?}"
    );
}

#[test]
fn the_scan_sees_what_it_looks_for() {
    // The scan's own check: a comment is no call, a call is one.
    assert!(!code("// with_ipc_handler").contains("with_ipc_handler"));
    assert!(code("b.with_ipc_handler(x) // no").contains("with_ipc_handler"));
}
