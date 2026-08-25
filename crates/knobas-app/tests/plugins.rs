//! What the window is permitted to do -- `capabilities/default.json`.
//!
//! Tauri reads this file at build time and denies anything it does not name,
//! at **run time**, with no compile-time signal at all: an app whose capability
//! file lost a permission builds, lints and tests green, and *Open in browser*
//! silently stops working in the packaged build. There is nothing else in the
//! tree that references these identifiers, so this is the only thing standing
//! between an edit and that outcome.
//!
//! # What this proves, and what it does not
//!
//! It proves the file **grants what the frontend calls and nothing more**. It
//! does *not* prove Tauri enforces the scope -- that is Tauri's own code, it
//! runs in a real window, and the honest check for it is the bundled build
//! recorded in this task's PR body. The frontend's own refusal
//! (`app/src/lib/shell/open-external.ts`) is tested separately, and the two are
//! deliberately independent: this one holds if a future caller forgets the
//! helper, that one gives a person a message they can act on.

use serde_json::Value;

fn capability() -> Value {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("capabilities/default.json");
    let text = std::fs::read_to_string(&path).expect("capabilities/default.json is readable");
    serde_json::from_str(&text).expect("capabilities/default.json is valid JSON")
}

/// The permission `openUrl` needs, scoped to the two schemes the app allows.
#[test]
fn the_window_may_open_http_urls_and_only_http_urls() {
    let capability = capability();
    let permissions = capability["permissions"]
        .as_array()
        .expect("permissions is a list");

    let opener = permissions
        .iter()
        .find(|entry| entry["identifier"] == "opener:allow-open-url")
        .unwrap_or_else(|| {
            panic!(
                "`opener:allow-open-url` is not granted, so every `openUrl` call is denied at \
                 run time -- with no build or test failure anywhere. Entries: {permissions:?}"
            )
        });

    let allowed: Vec<&str> = opener["allow"]
        .as_array()
        .expect("the grant carries a scope")
        .iter()
        .filter_map(|entry| entry["url"].as_str())
        .collect();
    assert_eq!(
        allowed,
        vec!["http://*", "https://*"],
        "the opener scope is the second wall behind `openExternal`'s scheme check"
    );
}

/// `opener:default` is deliberately *not* used.
///
/// It bundles `allow-reveal-item-in-dir` -- a file-manager call knobas never
/// makes -- and `allow-default-urls`, which adds `mailto:` and `tel:` to what
/// an adapter's `web_url` could reach. Both are grants nothing asked for, and
/// the shortest way to acquire them is somebody "simplifying" this file.
#[test]
fn no_grant_reaches_further_than_the_app_does() {
    let capability = capability();
    let permissions = capability["permissions"]
        .as_array()
        .expect("permissions is a list");

    let granted: Vec<String> = permissions
        .iter()
        .map(|entry| match entry {
            Value::String(identifier) => identifier.clone(),
            other => other["identifier"].as_str().unwrap_or_default().to_owned(),
        })
        .collect();

    assert_eq!(
        granted,
        vec!["core:default", "opener:allow-open-url"],
        "a permission was added or widened -- every entry here is a door the \
         webview can open, and `opener:default` in particular also grants \
         `reveal-item-in-dir` and `mailto:`/`tel:`"
    );
}

/// The plugin is registered in `run()`.
///
/// # This one is a source scan, and here is exactly what that is worth
///
/// Three things have to line up: the crate is a dependency, `run()` calls
/// `.plugin(..)`, and the capability above names the permission. Two of the
/// three are already caught without this test:
///
/// * **Dropping the dependency is a build error.** `tauri-build` resolves
///   every identifier in `capabilities/` against the plugins it can see and
///   refuses an unknown one -- verified by removing the crate, not assumed:
///   `Permission opener:allow-open-url not found, expected one of core:...`.
/// * **Dropping the permission** is the two tests above.
///
/// What is left is "the crate is there, the permission is there, and nobody
/// calls `.plugin(..)`", which produces no compile error (an unused
/// dependency is not a warning) and no test failure anywhere else. The mock
/// runtime cannot see it either: `mock_context` builds an app with an empty
/// ACL, so `plugin:opener|open_url` is refused with the identical
/// `"not allowed. Plugin not found"` whether the plugin is registered or not
/// -- checked, rather than assumed, before settling for a scan.
///
/// So this reads the source, with comments stripped so that a sentence *about*
/// registering the plugin cannot stand in for registering it. It proves the
/// call is written; it does not prove it is reached. What proves that is the
/// bundled build in this task's PR body, where the button was clicked.
#[test]
fn the_opener_plugin_is_registered() {
    let code = strip_comments(include_str!("../src/lib.rs"));
    assert!(
        code.contains(".plugin(tauri_plugin_opener::init())"),
        "the opener plugin is not registered in `run()`, so `opener:allow-open-url` \
         grants a command that does not exist -- and nothing else in the tree fails"
    );
}

/// Rust source with its comments removed, string literals intact.
///
/// The file this scans documents the plugin in prose right next to the call,
/// so a raw-text scan would be satisfied by the documentation rather than by
/// the code -- which is how a lint stops catching things.
fn strip_comments(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    let mut chars = source.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                out.push(c);
                while let Some(s) = chars.next() {
                    out.push(s);
                    match s {
                        '\\' => {
                            if let Some(escaped) = chars.next() {
                                out.push(escaped);
                            }
                        }
                        '"' => break,
                        _ => {}
                    }
                }
            }
            '/' if chars.peek() == Some(&'/') => {
                for s in chars.by_ref() {
                    if s == '\n' {
                        out.push('\n');
                        break;
                    }
                }
            }
            '/' if chars.peek() == Some(&'*') => {
                chars.next();
                let mut depth = 1_usize;
                let mut previous = '\0';
                for s in chars.by_ref() {
                    match (previous, s) {
                        ('/', '*') => depth += 1,
                        ('*', '/') => {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        _ => {}
                    }
                    previous = s;
                }
                out.push(' ');
            }
            _ => out.push(c),
        }
    }
    out
}
