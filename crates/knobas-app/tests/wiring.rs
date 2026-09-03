//! What `run()` wires up: the command handler list, and the one plugin.
//!
//! Both live in files nothing else references, and both fail **at run time**
//! with no compile-time signal: a command missing from `generate_handler!` is
//! simply not there when the frontend calls it, and a permission missing from
//! `capabilities/default.json` denies a plugin command that exists.
//!
//! ## What the window is permitted to do -- `capabilities/default.json`
//!
//!
//! Tauri reads that file at build time and denies anything it does not name,
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
        vec![
            "core:default",
            "opener:allow-open-url",
            "notification:allow-is-permission-granted",
            "notification:allow-request-permission",
            "notification:allow-notify",
        ],
        "a permission was added or widened -- every entry here is a door the \
         webview can open, and `opener:default` in particular also grants \
         `reveal-item-in-dir` and `mailto:`/`tel:`, while `notification:default` \
         grants sixteen permissions where knobas makes three calls"
    );
}

/// The three notification commands the frontend actually calls, and no fourth.
///
/// `notification:default` is deliberately *not* used: it bundles sixteen
/// permissions -- channels, scheduling, cancelling, reading back what is on
/// screen, registering action types -- and `app/src/lib/inbox/notify.svelte.ts`
/// makes exactly three calls. The shortest way to acquire the other thirteen is
/// somebody "simplifying" this file into one word.
///
/// Stated as its own test beside the exact-list one above because the exact
/// list is about *width* and this is about *these three being present*: a
/// rewrite that dropped `allow-notify` and added something else would still be
/// a list of five.
#[test]
fn the_window_may_ask_about_notify_and_nothing_else_about_notifications() {
    let capability = capability();
    let granted: Vec<String> = capability["permissions"]
        .as_array()
        .expect("permissions is a list")
        .iter()
        .map(|entry| match entry {
            Value::String(identifier) => identifier.clone(),
            other => other["identifier"].as_str().unwrap_or_default().to_owned(),
        })
        .filter(|identifier| identifier.starts_with("notification:"))
        .collect();

    assert_eq!(
        granted,
        vec![
            "notification:allow-is-permission-granted",
            "notification:allow-request-permission",
            "notification:allow-notify",
        ],
        "these are the three plugin commands the notifier calls -- a missing \
         one is denied at run time with no build or test failure anywhere, and \
         an extra one is a door nothing asked for"
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

/// Each Tauri plugin's two halves are pinned to **one** version.
///
/// A plugin is a Rust crate and an npm package speaking one protocol over
/// `invoke`, and a mismatch fails at *run time* with Tauri's own "command not
/// found" -- not at build time, and not in any test that stubs the npm half,
/// which is every frontend test there is. `Cargo.toml` says the rule in prose
/// beside both pins; this is the check.
///
/// Exact pins (`=2.5.4`) rather than carets, for the same reason: a caret on
/// one side and a lockfile refresh on the other is exactly how the two drift.
#[test]
fn each_plugin_is_pinned_to_one_version_on_both_sides_of_the_bridge() {
    let manifest = include_str!("../Cargo.toml");
    let package = include_str!("../../../app/package.json");

    for plugin in ["opener", "notification"] {
        let crate_pin = manifest
            .lines()
            .find_map(|line| line.trim().strip_prefix(&format!("tauri-plugin-{plugin} = ")))
            .unwrap_or_else(|| panic!("`tauri-plugin-{plugin}` is not a dependency"))
            .trim()
            .trim_matches('"')
            .to_owned();
        let js_pin = package
            .lines()
            .find_map(|line| {
                line.trim()
                    .strip_prefix(&format!("\"@tauri-apps/plugin-{plugin}\": "))
            })
            .unwrap_or_else(|| panic!("`@tauri-apps/plugin-{plugin}` is not in app/package.json"))
            .trim()
            .trim_end_matches(',')
            .trim_matches('"')
            .to_owned();

        assert!(
            crate_pin.starts_with('='),
            "`tauri-plugin-{plugin} = {crate_pin:?}` is not an exact pin, so a \
             `cargo update` can move the Rust half away from the npm one"
        );
        assert_eq!(
            crate_pin.trim_start_matches('='),
            js_pin,
            "the two halves of the {plugin} plugin are on different versions; \
             they speak one protocol and the mismatch fails at run time with \
             \"command not found\""
        );
        assert!(
            !js_pin.starts_with('^') && !js_pin.starts_with('~'),
            "`@tauri-apps/plugin-{plugin}` is a range ({js_pin:?}), not a pin"
        );
    }
}

/// The notification plugin is registered in `run()`.
///
/// The same source scan as the opener above, for the same three-way lineup and
/// with the same worth: dropping the crate is a build error (`tauri-build`
/// resolves every identifier in `capabilities/` against the plugins it can
/// see), dropping the permissions is the two tests above, and what is left --
/// crate present, permissions present, nobody calling `.plugin(..)` -- produces
/// no compile error and no test failure anywhere else. What proves it is
/// reached is the signed bundle recorded in #290's PR body, where the
/// notification appeared.
#[test]
fn the_notification_plugin_is_registered() {
    let code = strip_comments(include_str!("../src/lib.rs"));
    assert!(
        code.contains(".plugin(tauri_plugin_notification::init())"),
        "the notification plugin is not registered in `run()`, so the three \
         `notification:*` grants name commands that do not exist -- and \
         nothing else in the tree fails"
    );
}

/// The stranded-timer sweep runs during bring-up, and **before the state
/// reaches `Ready`** (issue #278).
///
/// `tests/time_ipc.rs` proves the sweep closes a block at the last heartbeat.
/// Nothing there proves it is ever *called*: `spawn_bring_up` needs a Tauri
/// app handle and a real PostgreSQL provisioning, so the call itself has no
/// seam under test, and a sweep that exists and is never reached looks exactly
/// like one that works -- until a user quits with the clock running and comes
/// back to a timer that has been going all night.
///
/// So this reads the source, with comments stripped so that a sentence *about*
/// sweeping cannot stand in for the sweep. Position matters as much as
/// presence: the acceptance criterion is "before the shell's first read", and
/// the shell reads when `DbState::Ready` is announced.
#[test]
fn the_stranded_timer_sweep_runs_before_the_database_is_announced_ready() {
    let code = strip_comments(include_str!("../src/lib.rs"));
    let start = code
        .find("fn spawn_bring_up")
        .expect("bring-up is where a session's one-off work happens");
    let body = &code[start..];

    let swept = body
        .find("time::close_stranded")
        .expect("a timer that outlived the last process is never closed, so the strip draws a clock that ran all night");
    let ready = body
        .find("DbState::Ready")
        .expect("bring-up announces readiness");
    assert!(
        swept < ready,
        "the sweep runs after `DbState::Ready`, so the shell's first \
         `current_timer` can beat it and draw the stranded clock"
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

// ---------------------------------------------------------------------------
// The handler list.
// ---------------------------------------------------------------------------

/// Every `#[tauri::command]` in `src/commands/**` is in `run()`'s handler list.
///
/// # Why this is a scan, and what the compiler already covers
///
/// The two directions are not symmetric:
///
/// * **Registered but not a command** is a *compile* error. `generate_handler!`
///   expands each identifier into a path it has to resolve to a command, so a
///   name that is not one fails to build. Nothing is needed for that direction.
/// * **A command that is not registered** is nothing at all. It builds, it
///   lints, `tests/ipc.rs` still dispatches it -- because that file builds its
///   own `generate_handler!` list, which is a *copy* -- and the frontend's
///   `invoke` fails at run time with "command not found". A mutation run said
///   so: deleting `commands::entity::list_entities` from `run()` left the whole
///   suite green.
///
/// So this reads the source, with comments stripped so a doc comment naming a
/// command cannot stand in for registering it. It proves the name is in the
/// list; it does not prove the list is the one `Builder::invoke_handler` is
/// given, which is one line away in the same function.
#[test]
fn every_command_is_in_the_handler_list() {
    let registered = handler_list();
    assert!(
        registered.len() >= 8,
        "the handler list was not parsed -- it holds {} names",
        registered.len()
    );

    let mut missing = Vec::new();
    for (module, command) in declared_commands() {
        let path = format!("commands::{module}::{command}");
        if !registered.contains(&path) {
            missing.push(path);
        }
    }
    missing.sort();
    assert_eq!(
        missing,
        Vec::<String>::new(),
        "these commands exist and are never registered, so the frontend's \
         `invoke` fails at run time with \"command not found\" and nothing else \
         in the tree notices. Registered: {registered:?}"
    );
}

/// The identifiers inside `run()`'s `tauri::generate_handler![..]`.
fn handler_list() -> Vec<String> {
    let code = strip_comments(include_str!("../src/lib.rs"));
    let start = code
        .find("generate_handler![")
        .expect("run() builds a handler list");
    let rest = &code[start + "generate_handler![".len()..];
    let end = rest.find(']').expect("the handler list is closed");
    rest[..end]
        .split(',')
        .map(|entry| entry.trim().to_owned())
        .filter(|entry| !entry.is_empty())
        .collect()
}

/// Every `#[tauri::command]` in `src/commands/**`, as `(module, function)`.
fn declared_commands() -> Vec<(String, String)> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/commands");
    let mut found = Vec::new();

    for entry in std::fs::read_dir(&dir).expect("src/commands is readable") {
        let path = entry.expect("a directory entry").path();
        if path.extension().is_none_or(|extension| extension != "rs") {
            continue;
        }
        let module = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .expect("a module name")
            .to_owned();
        if module == "mod" {
            continue;
        }
        let code = strip_comments(&std::fs::read_to_string(&path).expect("a command module"));
        let mut rest = code.as_str();
        while let Some(at) = rest.find("#[tauri::command]") {
            rest = &rest[at + "#[tauri::command]".len()..];
            let name = rest
                .split("fn ")
                .nth(1)
                .and_then(|after| after.split(['(', '<', ' ']).next())
                .expect("a command declares a function after its attribute")
                .to_owned();
            found.push((module.clone(), name));
        }
    }

    assert!(
        found.len() >= 8,
        "no commands were found at all, so this test proves nothing"
    );
    found
}
