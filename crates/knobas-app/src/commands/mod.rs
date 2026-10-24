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
//! **No command issues a statement of its own** (#1870). #1659 named the
//! `_inner` functions that still held their SQL or a resolution rule, and
//! #1870 moved each below: the Builds frame's pins, *This job* and the rule
//! that links a run to its commit into `knobas_core::builds` (its budgeted
//! live read into `sources::crud::shared_commits`), the detail view into
//! `knobas_core::detail`, the paste resolver into `knobas_core::web_url`,
//! Code's facts into `knobas_core::code_facts`, *Viewed* into
//! `knobas_core::viewed`, drawing a link into `knobas_core::link`, an action
//! item's ticket look-up into `crate::protocol`, a saved view's layout into
//! `crate::ticket_views` and the first run into `crate::settings`.
//! `no_command_issues_its_own_statement` keeps it so. What is also settled:
//! who am I (`identity_of`), the declared paths and the stored declarations
//! are each derived once per command, through `identity_of`,
//! `sources::paths::declared_paths` and [`search::declare`] /
//! [`search::searcher`], not re-read in a body. (`docs_tree` still lists the
//! sources twice, once for the paths and once in `sources::crud::fillers`.)

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

/// What the mirror tests of this directory read off `lib.rs` and the barrels.
#[cfg(test)]
pub(crate) mod mirror {
    /// Every `commands::<module>::<name>` line of `lib.rs`'s handler list, so a
    /// command added there is checked without a second list to keep.
    pub(crate) fn registered(module: &str) -> Vec<&'static str> {
        let prefix = format!("commands::{module}::");
        include_str!("../lib.rs")
            .lines()
            .filter_map(|line| line.trim().strip_prefix(prefix.as_str()))
            .map(|rest| {
                let end = rest
                    .find(|c: char| !c.is_alphanumeric() && c != '_')
                    .unwrap_or(rest.len());
                &rest[..end]
            })
            .filter(|name| !name.is_empty())
            .collect()
    }

    /// The command names a barrel calls as `invoke<...>("name"`, comment lines
    /// skipped so a doc comment cannot stand in for a call.
    pub(crate) fn invoked(mirror: &'static str) -> Vec<&'static str> {
        let mut names = Vec::new();
        for (at, _) in mirror.match_indices("invoke<") {
            if in_comment(mirror, at) {
                continue;
            }
            // The name follows the `>(` that closes the type argument. A site
            // this cannot read is a panic, never a skipped line: a skipped
            // `invoke` would let a ghost command through the reverse check.
            let rest = &mirror[at..];
            let name = rest
                .split_once(">(")
                .and_then(|(_, after)| after.trim_start().strip_prefix('"'))
                .and_then(|quoted| quoted.split_once('"'))
                .map(|(name, _)| name);
            let Some(name) = name else {
                panic!(
                    "an `invoke<` call site whose command name is not a double-quoted \
                     literal right after `>(`: {}",
                    rest.lines().next().unwrap_or_default()
                );
            };
            names.push(name);
        }
        names
    }

    /// Whether the line holding byte `at` is a comment line.
    fn in_comment(mirror: &str, at: usize) -> bool {
        let line_start = mirror[..at].rfind('\n').map_or(0, |i| i + 1);
        let lead = mirror[line_start..at].trim_start();
        lead.starts_with("//") || lead.starts_with('*') || lead.starts_with("/*")
    }

    /// The `invoke<..>("name", ..)` call site, from `invoke<` to its `);`,
    /// comment lines skipped as in [`invoked`]: a doc comment that names the
    /// command, or shows a call, is not the call (#1984).
    pub(crate) fn invocation(mirror: &'static str, name: &str) -> &'static str {
        let quoted = format!("\"{name}\"");
        mirror
            .match_indices("invoke<")
            .filter(|&(at, _)| !in_comment(mirror, at))
            .map(|(at, _)| &mirror[at..])
            .find(|rest| {
                rest.split_once(">(")
                    .is_some_and(|(_, after)| after.trim_start().starts_with(&quoted))
            })
            .map(|rest| &rest[..rest.find(");").unwrap_or(rest.len())])
            .unwrap_or_else(|| panic!("{name} is not invoked from the mirror"))
    }

    /// The object keys an [`invocation`] sends, nested payload fields
    /// included: every identifier after the call's `>(` whose previous
    /// non-space character is `{` or `,` and whose next is `:`, `,` or `}`,
    /// which is the `key: value` form and the shorthand both (#2008).
    ///
    /// Tauri reads the key, so a test that asks whether the call text
    /// *contains* an argument name passes on `offset: when.offsetMinutes`,
    /// which sends no `offsetMinutes`. The next-character rule keeps out a
    /// generic's `B` in `<A, B>` and a value's `b` in `f(a, b)`.
    pub(crate) fn keys(invocation: &str) -> Vec<&str> {
        let args = invocation.split_once(">(").map_or(invocation, |(_, a)| a);
        let is_ident = |c: char| c.is_alphanumeric() || c == '_' || c == '$';
        let mut found = Vec::new();
        let mut at = 0;
        while let Some(start) = args[at..].find(is_ident).map(|i| at + i) {
            let end = args[start..]
                .find(|c: char| !is_ident(c))
                .map_or(args.len(), |i| start + i);
            let before = args[..start].trim_end().chars().next_back();
            let after = args[end..].trim_start().chars().next();
            if matches!(before, Some('{' | ','))
                && matches!(after, Some(':' | ',' | '}'))
            {
                found.push(&args[start..end]);
            }
            at = end;
        }
        found
    }

    mod tests {
        use super::keys;

        /// The key side only: a name that appears as a value is not sent
        /// (#2008). Tauri reads the key, so `offset: when.offsetMinutes` sends
        /// no `offsetMinutes`, and `changes: edits` sends no `edits`.
        #[test]
        fn the_keys_of_a_call_are_its_key_side_names() {
            let call = r#"invoke<Draft | null>("worklog_draft", {
    entityId,
    day: when.day,
    offset: when.offsetMinutes,
  }"#;
            assert_eq!(keys(call), ["entityId", "day", "offset"]);
            assert_eq!(
                keys(r#"invoke<AssetRow>("edit_asset", { assetId, changes: edits }"#),
                ["assetId", "changes"]
            );
        }

        /// A nested payload's fields are keys too, and the generic's, the
        /// command name and a call inside a value are not.
        #[test]
        fn the_keys_of_a_call_include_nested_fields_and_nothing_else() {
            let call = r#"invoke<Pair<{ a: A }, B>>("start_timer", {
    target: { kind: "entity", id },
    note: label(a, b),
    inRoom
  }"#;
            assert_eq!(keys(call), ["target", "kind", "id", "note", "inRoom"]);
            assert!(keys(r#"invoke<void>("stop_timer""#).is_empty());
        }
    }

    /// Both directions: every registered command is invoked, every invoked
    /// command is registered.
    pub(crate) fn assert_mirrors(module: &str, mirror: &'static str, barrel: &str, min: usize) {
        let registered = registered(module);
        assert!(
            registered.len() >= min,
            "only {} {module} commands found in the handler list -- the parse is wrong, not the list",
            registered.len()
        );
        let invoked = invoked(mirror);
        for command in &registered {
            assert!(
                invoked.contains(command),
                "{command} is in the handler list but {barrel} has no invoke<..>(\"{command}\" call"
            );
        }
        for name in &invoked {
            assert!(
                registered.contains(name),
                "{barrel} invokes {name}, which is not in the handler list under commands::{module}"
            );
        }
    }
}

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
        let mut checked = 0_usize;
        let mut entity_files = 0_usize;

        for path in command_modules() {
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

    /// Every command module's file: `src/commands/*.rs` and one level down,
    /// since a module that is a directory (`entity/`, #1316) keeps its
    /// commands in the files beside its `mod.rs`.
    ///
    /// This file is left out. It declares no commands -- it is re-exports and
    /// these tests -- and the assertions here quote the very spellings they
    /// forbid, so a scan that matched its own error messages could only be
    /// kept green by weakening the pattern.
    fn command_modules() -> Vec<std::path::PathBuf> {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/commands");
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
        paths.retain(|path| {
            path.extension().is_some_and(|extension| extension == "rs")
                && *path != dir.join("mod.rs")
        });
        paths
    }

    /// No command module issues a statement of its own (#1870).
    ///
    /// A command is a shim (this module's doc): a statement, a transaction or
    /// the rule they carry belongs in `knobas-core` or a behaviour module of
    /// this crate, with its own tests, where a change to it is found by the
    /// module that owns the behaviour rather than in a command body. #1659
    /// named the exceptions and #1870 moved them; this keeps the next one
    /// from arriving unnoticed.
    ///
    /// The issue's search spelled a statement `sqlx::query*` and a
    /// transaction `.begin()`. A statement has more spellings than the first
    /// (`sqlx::raw_sql`, `sqlx::QueryBuilder`, `use sqlx::{query_as, ..}`),
    /// so every `sqlx::` path must name one of the two items a shim has a
    /// use for -- the pool it is handed and the error it maps -- and a
    /// connection is neither begun nor acquired.
    #[test]
    fn no_command_issues_its_own_statement() {
        const SHIM_ITEMS: [&str; 2] = ["PgPool", "Error"];
        let mut checked = 0_usize;
        for path in command_modules() {
            let source = std::fs::read_to_string(&path).expect("a command module");
            let code = strip_comments(&source);
            let mut rest = code.as_str();
            while let Some(at) = rest.find(concat!("sqlx", "::")) {
                rest = &rest[at + "sqlx::".len()..];
                let item: String = rest
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect();
                assert!(
                    SHIM_ITEMS.contains(&item.as_str()),
                    "{} spells `sqlx::{item}`: a command issues no statement of its \
                     own, and a shim names no sqlx item but {SHIM_ITEMS:?}. Move the \
                     read or write into `knobas-core` or the behaviour module that \
                     owns it, and call that (#1870).",
                    path.display()
                );
            }
            for spelling in [concat!(".begin", "()"), concat!(".acquire", "()")] {
                assert!(
                    !code.contains(spelling),
                    "{} spells `{spelling}`: a command opens no connection or \
                     transaction of its own. Move it into `knobas-core` or the \
                     behaviour module that owns it, and call that (#1870).",
                    path.display()
                );
            }
            checked += 1;
        }
        // Twenty-four today (#1870); a floor well under that, so a walk that
        // stopped descending into `entity/` fails here rather than passing.
        assert!(
            checked >= 20,
            "only {checked} command modules were scanned -- the walk missed \
             most of them, so this test proves little"
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
