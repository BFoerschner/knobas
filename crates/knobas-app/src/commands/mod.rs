//! The M1 IPC surface, mirrored in `app/src/lib/ipc/`.
//!
//! One module per owning stream (interfaces §2), which is what keeps seven
//! branches off each other's toes: `app` and `entity` are stream D's, `sources`
//! is stream F's, `search` is stream E's; `assets`, `backup` and `time` are
//! the modules added since. This file and the
//! `generate_handler!` list in `crate::run` are the only shared surfaces, both
//! append-only and orchestrator-owned.
//!
//! Most commands are a thin shim: take the arguments Tauri deserialised, call
//! the crate that owns the behaviour, convert the failure into an
//! [`IpcError`](crate::IpcError). When a command grows a policy, that policy
//! belongs in a crate below it, with its own tests.
//!
//! **The exceptions, named rather than denied** (#1659): some `_inner`
//! functions here still hold their own SQL or a resolution rule, and are not
//! yet moved below. `entity/builds.rs` (`with_commits`, `JOB_PIPELINES`),
//! `entity/detail.rs` (`DETAIL`, `RESOLVE_URL`), `entity/standup.rs`
//! (`ticket_titled`), `entity/board.rs` (`saved_view_board_laned`) and
//! `entity/links.rs` (`create_link_inner`'s two-way transaction) are the ones;
//! each has a `backlog` ticket to move it (#1870 to #1874). What is already settled: who am I
//! (`identity_of`), the declared paths and the stored declarations are derived
//! once per command, through `identity_of`, `sources::paths::declared_paths`
//! and [`search::declare`] / [`search::searcher`], not re-read in a body.

pub mod app;
pub mod assets;
pub mod backup;
pub mod entity;
pub mod search;
pub mod sources;
pub mod time;

/// The most rows one list read hands the bridge, whatever limit the webview
/// sends (#1643): a frontend arithmetic slip would otherwise serialise a whole
/// table through the IPC payload. `list_sync_runs`, `room_suggestions` and
/// `list_entities` read under it.
pub const ROW_CEILING: u32 = 500;

#[cfg(test)]
mod tests {
    /// No command anywhere in this directory takes `State<'_, AppState>`.
    ///
    /// Carry-over §10.6(a). `AppState` is managed only once PostgreSQL is up,
    /// and a `#[tauri::command]` resolves **every** argument before its body
    /// runs -- so a command declaring it is rejected by Tauri during bring-up
    /// with the bare string `"state not managed"`: no code, nothing the
    /// frontend can branch on, and `IpcErrorCode::NotReady` unreachable.
    /// `State<'_, Lifecycle>` plus `lifecycle.pool()?` is the shape; that is
    /// the one place `not_ready` comes from.
    ///
    /// # Why this is a source scan and not a type-level check
    ///
    /// Because **it does not fail to compile.** `AppState` is `pub` at the
    /// crate root and `tauri::State<'r, T>` requires only
    /// `Send + Sync + 'static`, so the wrong argument builds clean and passes
    /// `clippy -D warnings`. An earlier version of this PR's own notes claimed
    /// otherwise; the disproof was already in its mutation log, where
    /// reintroducing the argument produced "8 passed, 2 failed" -- a count a
    /// compile error cannot produce.
    ///
    /// `tests/it/ipc.rs` catches it at runtime, but only for the one command each
    /// test names. This is the class-level pin: a *new* command with the wrong
    /// argument fails here, whoever writes it and whichever stream owns the
    /// file.
    #[test]
    fn no_command_takes_the_app_state_directly() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/commands");
        let mut checked = 0_usize;
        let mut entity_files = 0_usize;

        // One level down as well: a module that is a directory (`entity/`,
        // #1316) keeps its commands in the files beside its `mod.rs`.
        let mut paths = Vec::new();
        for entry in std::fs::read_dir(&dir).expect("src/commands is readable") {
            let path = entry.expect("a directory entry").path();
            if path.is_dir() {
                for inner in std::fs::read_dir(&path).expect("a command module directory") {
                    paths.push(inner.expect("a directory entry").path());
                }
            } else {
                paths.push(path);
            }
        }

        for path in paths {
            if path.extension().is_none_or(|extension| extension != "rs") {
                continue;
            }
            // This file declares no commands -- it is re-exports and this
            // test. It is skipped because the assertion below quotes the very
            // signature it forbids, and a scan that matched its own error
            // message could only be kept green by weakening the pattern.
            if path == dir.join("mod.rs") {
                continue;
            }
            checked += 1;
            if path
                .parent()
                .is_some_and(|parent| parent.ends_with("entity"))
            {
                entity_files += 1;
            }
            let source = std::fs::read_to_string(&path).expect("a command module");
            let code = strip_comments(&source);

            // Every `tauri::State` argument is written `State<'_, T>`, so the
            // lifetime is what distinguishes a real argument from prose or
            // from an `Arc<AppState>` field.
            let mut rest = code.as_str();
            while let Some(at) = rest.find("State<'") {
                rest = &rest[at + "State<'".len()..];
                let end = rest.find('>').unwrap_or(rest.len());
                assert!(
                    !rest[..end].contains("AppState"),
                    "{} declares a command argument of `State<'_, AppState>`. \
                     That builds and lints clean and fails at *run time*, during \
                     bring-up, with Tauri's bare \"state not managed\" -- see \
                     carry-over §10.6(a). Take `State<'_, Lifecycle>` and call \
                     `lifecycle.pool()?` instead.",
                    path.display()
                );
            }
        }

        assert!(
            checked >= 4,
            "only {checked} command modules were scanned -- the walk found \
             nothing, so this test proves nothing"
        );
        // A module that is a directory (`entity/`, #1316) is walked too.
        assert!(
            entity_files > 1,
            "the walk did not reach `entity/`'s files, so its commands go unscanned"
        );
    }

    /// Rust source with its comments removed, string and char literals intact.
    ///
    /// The modules here document the very rule above, so a scan over raw text
    /// would fire on the doc comment that explains it.
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
}
