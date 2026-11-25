//! The placeholders' rules, ported from the design's 52 cases
//! (`run-commands-spike/core-test.mjs`) and the mutants section 5 names.

use std::path::Path;

use super::*;

const ROOT: &str = "/Users/mara/code/payout-service";
const SCRATCH: &str = "/Users/mara/Library/knobas/command-runs/r7";

fn at() -> At<'static> {
    At {
        file: Some("src/retry/window.ts"),
        dir: Some("src/retry"),
        ..At::root(Path::new(ROOT), Path::new(SCRATCH), "macos")
    }
}

fn filled(command: &str, at: &At<'_>) -> Vec<String> {
    fill(command, at).unwrap_or_else(|e| panic!("{command:?} must fill: {e}"))
}

fn refused(command: &str) -> String {
    check(command, Stdin::None, "macos")
        .expect_err(command)
        .to_string()
}

#[test]
fn the_kind_is_read_from_the_placeholders() {
    let kind = |c: &str| kind_of(c, Stdin::None).unwrap();
    assert_eq!(kind("eslint {file}"), Kind::File);
    assert_eq!(kind("eslint {relfile}"), Kind::File);
    assert_eq!(kind("prettier --check {files}"), Kind::File);
    assert_eq!(kind("git blame -L {line},{endline} -- x"), Kind::File);
    assert_eq!(kind("eslint {dir}"), Kind::Folder);
    assert_eq!(kind("ls {reldir}"), Kind::Folder);
    assert_eq!(kind("cargo clippy --all-targets"), Kind::Repository);
    assert_eq!(kind("tree {root}"), Kind::Repository);
    assert_eq!(kind("git show --stat {sha}"), Kind::Commit);
    assert_eq!(kind("git diff --stat {base} {head}"), Kind::Range);
    assert_eq!(kind("git diff {base}"), Kind::Range);
    // A file's command may hold {dir}, {root} and {sha}: it stays a file's.
    assert_eq!(kind("git blame {sha} -- {relfile}"), Kind::File);
    assert_eq!(
        kind("eslint --resolve-plugins-relative-to {dir} {file}"),
        Kind::File
    );
    // {base} before {sha}: a comparison's.
    assert_eq!(kind("git log {base}..{sha}"), Kind::Range);
    assert_eq!(kind_of("jq .", Stdin::Selection).unwrap(), Kind::File);
    assert_eq!(kind_of("xmllint -", Stdin::File).unwrap(), Kind::File);
}

#[test]
fn save_takes_a_plain_command_and_the_script_form_that_works() {
    for ok in [
        "eslint --max-warnings 0 {file}",
        "sh -c 'eslint \"$1\" | head -40' _ {file}",
        "eslint --config={root}/eslint.config.js {file}",
        "wsl -e ruff check {file}",
        "madge --image {out}.png {dir}",
        "git diff {base} {head} -- {relfile}",
        "bash -c 'echo hi'",
    ] {
        assert_eq!(check(ok, Stdin::None, "macos"), Ok(()), "{ok}");
    }
}

#[test]
fn path_is_refused_by_name_pointing_at_root_dir_and_file() {
    let said = refused("eslint {path}");
    assert!(
        said.contains("{root}") && said.contains("{dir}") && said.contains("{file}"),
        "{said}"
    );
}

#[test]
fn an_unknown_placeholder_is_refused_by_name() {
    assert!(refused("eslint {title}").starts_with("{title} is not something knobas fills in"));
    assert!(refused("x {author}").starts_with("{author}"));
}

/// S1: a path in a quoted script, and the script after a shell's flag.
#[test]
fn a_path_inside_a_script_is_refused_with_the_form_that_works() {
    let said = refused("sh -c \"eslint {file} | head\"");
    assert!(
        said.contains("quoted script") && said.contains("_ {file}"),
        "{said}"
    );
    assert!(refused("bash -c 'cat {dir}/x'").contains("{dir}"));
    assert!(refused("x 'a {out}'").contains("{out}"));
    assert!(refused("sh -c {file}").contains("script of sh -c"));
    assert!(refused("cmd /c {file}").contains("script of cmd /c"));
    assert!(refused("cmd.exe /K {relfile}").contains("/K"));
    assert!(refused("powershell -Command {file}").contains("-Command"));
    assert!(refused("pwsh -EncodedCommand {root}").contains("{root}"));
    // A shell further along is a shell too.
    assert!(refused("env FOO=1 sh -c {file}").contains("script of env"));
    assert!(refused("wsl -e bash -c {file}").contains("-c"));
    // A path as a word of its own after a non-script flag is fine.
    assert_eq!(
        check("eslint -c {root}/x.js {file}", Stdin::None, "macos"),
        Ok(())
    );
    // A word with whitespace but no path placeholder is fine.
    assert_eq!(
        check("sh -c 'echo {line} | cat' _", Stdin::None, "macos"),
        Ok(())
    );
}

#[test]
fn files_stands_alone_and_out_is_once() {
    assert!(refused("x --in={files}").contains("stands alone"));
    assert!(refused("x {out}.png {outdir}").contains("one of them"));
    assert!(refused("x {out} {out}").contains("one of them"));
    assert!(refused("x {outdir} {outdir}").contains("one of them"));
}

/// S3: wsl reads its words through a shell without -e.
#[test]
fn wsl_with_a_path_needs_e() {
    assert!(refused("wsl ruff check {file}").contains("-e stands"));
    assert!(refused("C:\\Windows\\System32\\wsl.exe ruff {dir}").contains("-e"));
    assert_eq!(
        check("wsl --exec ruff check {file}", Stdin::None, "windows"),
        Ok(())
    );
    // No path: nothing for a shell to misread.
    assert_eq!(check("wsl cargo clippy", Stdin::None, "windows"), Ok(()));
}

#[test]
fn a_bare_shell_on_stdin_is_refused() {
    assert!(check("sh", Stdin::File, "linux").is_err());
    assert!(check("bash", Stdin::Selection, "linux").is_err());
    assert_eq!(check("jq .", Stdin::Selection, "linux"), Ok(()));
    assert_eq!(check("sh", Stdin::None, "linux"), Ok(()));
}

#[test]
fn unusable_words_say_so() {
    assert_eq!(check("  ", Stdin::None, "macos"), Err(Refusal::Empty));
    assert_eq!(
        check("x 'a", Stdin::None, "macos"),
        Err(Refusal::UnclosedQuote)
    );
    assert_eq!(
        check("x {file", Stdin::None, "macos"),
        Err(Refusal::UnclosedPlaceholder)
    );
}

#[test]
fn every_placeholder_fills_from_the_disk_the_page_or_the_run() {
    let at = At {
        lines: Some((12, 20)),
        sha: Some("abc1234"),
        base: Some("main"),
        head: Some("feature/retry-window"),
        branch: Some("feature/retry-window"),
        files: &[],
        ..at()
    };
    assert_eq!(
        filled("eslint --max-warnings 0 {file}", &at),
        [
            "eslint",
            "--max-warnings",
            "0",
            &format!("{ROOT}/src/retry/window.ts")
        ]
    );
    assert_eq!(
        filled("git blame -- {relfile}", &at),
        ["git", "blame", "--", "src/retry/window.ts"]
    );
    assert_eq!(
        filled("ls {dir} {reldir}", &at),
        ["ls", &format!("{ROOT}/src/retry"), "src/retry"]
    );
    assert_eq!(
        filled("x {root} {name}", &at),
        ["x", ROOT, "payout-service"]
    );
    assert_eq!(filled("x -L{line},{endline}", &at), ["x", "-L12,20"]);
    assert_eq!(filled("git show {sha}", &at), ["git", "show", "abc1234"]);
    assert_eq!(
        filled("git diff {base} {head} {branch}", &at),
        [
            "git",
            "diff",
            "main",
            "feature/retry-window",
            "feature/retry-window"
        ]
    );
    assert_eq!(
        filled("x {out}.png", &at),
        ["x", &format!("{SCRATCH}/out/output.png")]
    );
    assert_eq!(filled("x {outdir}", &at), ["x", &format!("{SCRATCH}/out")]);
}

#[test]
fn the_top_folder_is_dot_and_lines_default_to_one() {
    let top = At {
        dir: Some(""),
        ..At::root(Path::new(ROOT), Path::new(SCRATCH), "linux")
    };
    assert_eq!(
        filled("ruff check {reldir} {dir}", &top),
        ["ruff", "check", ".", ROOT]
    );
    assert_eq!(filled("x {line} {endline}", &top), ["x", "1", "1"]);
}

#[test]
fn a_value_the_target_lacks_runs_nothing() {
    let repo = At::root(Path::new(ROOT), Path::new(SCRATCH), "linux");
    assert_eq!(
        fill("eslint {file}", &repo),
        Err(Refusal::NoValue("file".to_owned()))
    );
    assert_eq!(
        fill("ls {dir}", &repo),
        Err(Refusal::NoValue("dir".to_owned()))
    );
    assert_eq!(
        fill("git show {sha}", &repo),
        Err(Refusal::NoValue("sha".to_owned()))
    );
    assert_eq!(
        fill("x {files}", &repo),
        Err(Refusal::NoValue("files".to_owned()))
    );
}

/// A file's name with spaces, a quote, `$(`, `;` and a newline is one argument.
#[test]
fn an_evil_file_name_is_one_argument() {
    let evil = "a b; rm -rf ~ $(x) 'q\"\n.ts";
    let at = At {
        file: Some(evil),
        ..at()
    };
    let argv = filled("eslint {file}", &at);
    assert_eq!(argv.len(), 2);
    assert_eq!(argv[1], format!("{ROOT}/{evil}"));
    assert_eq!(filled("git blame -- {relfile}", &at)[3], evil);
}

#[test]
fn a_relative_path_that_starts_with_a_dash_is_refused() {
    let at = At {
        file: Some("-rf"),
        dir: Some("--upload-pack=x"),
        ..at()
    };
    let said = fill("git blame {relfile}", &at).unwrap_err().to_string();
    assert!(
        said.contains("read as an option") && said.contains("{file}"),
        "{said}"
    );
    let said = fill("ls {reldir}", &at).unwrap_err().to_string();
    assert!(said.contains("{dir}"), "{said}");
    // The absolute path starts with the root, never a dash.
    assert_eq!(filled("git blame {file}", &at)[2], format!("{ROOT}/-rf"));
}

#[test]
fn a_git_name_is_held_to_its_grammar() {
    for (command, at) in [
        (
            "git show {sha}",
            At {
                sha: Some("--upload-pack=x"),
                ..at()
            },
        ),
        (
            "git show {sha}",
            At {
                sha: Some("abc12"),
                ..at()
            },
        ),
        (
            "git diff {base}",
            At {
                base: Some("-x"),
                ..at()
            },
        ),
        (
            "git diff {head}",
            At {
                head: Some("main^"),
                ..at()
            },
        ),
        (
            "git log {branch}",
            At {
                branch: Some("a b"),
                ..at()
            },
        ),
    ] {
        let said = fill(command, &at).unwrap_err().to_string();
        assert!(
            said.contains("not a name knobas passes"),
            "{command}: {said}"
        );
    }
    // A base may end in one caret.
    assert_eq!(
        filled(
            "git diff {base}",
            &At {
                base: Some("abc1234^"),
                ..at()
            }
        )[2],
        "abc1234^"
    );
}

#[test]
fn files_becomes_an_argument_a_file() {
    let files = ["a.ts".to_owned(), "src/b c.ts".to_owned()];
    let batch = At {
        files: &files,
        ..at()
    };
    assert_eq!(
        filled("prettier --check {files}", &batch),
        [
            "prettier",
            "--check",
            &format!("{ROOT}/a.ts"),
            &format!("{ROOT}/src/b c.ts")
        ]
    );
    // One file stands in for a batch of one.
    assert_eq!(
        filled("prettier {files}", &at())[1],
        format!("{ROOT}/src/retry/window.ts")
    );
}

#[test]
fn fill_checks_the_rules_save_checks() {
    assert_eq!(fill("eslint {path}", &at()), Err(Refusal::PathMeansTwo));
    assert!(matches!(
        fill("sh -c 'x {file}'", &at()),
        Err(Refusal::InScript(_))
    ));
}

#[test]
fn a_path_for_cmd_may_not_hold_what_cmd_reads_again() {
    let win = |file| At {
        file: Some(file),
        os: "windows",
        ..at()
    };
    for bad in ["50%off.ts", "a!b.ts", "a^b.ts", "a\"b.ts"] {
        for program in ["cmd", "CMD.EXE", "eslint.cmd", "C:\\tools\\lint.bat"] {
            let command = format!("{program} {{file}}");
            assert!(
                matches!(fill(&command, &win(bad)), Err(Refusal::CmdChars { .. })),
                "{command} {bad}"
            );
        }
        // Not cmd: the standard library quotes it.
        assert!(fill("eslint.exe {file}", &win(bad)).is_ok());
        // Not Windows: no cmd.
        assert!(
            fill(
                "eslint.cmd {file}",
                &At {
                    file: Some(bad),
                    ..at()
                }
            )
            .is_ok()
        );
    }
    assert!(fill("eslint.cmd {file}", &win("plain name.ts")).is_ok());
}

#[test]
fn paths_as_wsl_sees_them() {
    assert_eq!(
        wsl_path("C:\\src\\app\\a.py").as_deref(),
        Some("/mnt/c/src/app/a.py")
    );
    assert_eq!(wsl_path("d:/x").as_deref(), Some("/mnt/d/x"));
    assert_eq!(wsl_path("C:\\").as_deref(), Some("/mnt/c"));
    assert_eq!(
        wsl_path("\\\\wsl$\\Ubuntu\\home\\dev\\app\\a.py").as_deref(),
        Some("/home/dev/app/a.py")
    );
    assert_eq!(
        wsl_path("\\\\wsl.localhost\\Ubuntu-24.04\\home\\dev").as_deref(),
        Some("/home/dev")
    );
    assert_eq!(wsl_path("/home/dev"), None);
    assert_eq!(wsl_path("relative/x"), None);
    let at = At {
        root: Path::new("C:\\src\\app"),
        file: Some("a.py"),
        wsl: true,
        os: "windows",
        ..at()
    };
    // On a Mac the root joins with '/', so the drive form still maps.
    assert_eq!(
        filled("wsl -e ruff check {file}", &at)[4],
        "/mnt/c/src/app/a.py"
    );
    assert_eq!(filled("wsl -e ruff check {relfile}", &at)[4], "a.py");
}

#[test]
fn shown_quotes_what_a_person_must_quote() {
    let argv = [
        "eslint".to_owned(),
        "/a b/c.ts".to_owned(),
        String::new(),
        "x\"y".to_owned(),
    ];
    assert_eq!(shown(&argv), "eslint \"/a b/c.ts\" \"\" \"x\\\"y\"");
}

#[test]
fn globs_match_a_name_or_a_whole_path() {
    assert!(named("*.ts", "src/retry/window.ts"));
    assert!(named("*.TS", "src/retry/window.ts"));
    assert!(named("orders/*.xml", "orders/order-4711.xml"));
    assert!(!named("orders/*.xml", "src/order.xml"));
    assert!(!named("*.ts", "src/retry/window.tsx"));
    assert!(named("?akefile", "Makefile"));
}

fn command(text: &str) -> OneOff {
    settle(
        &[Draft {
            name: "x".to_owned(),
            command: text.to_owned(),
            ..Draft::default()
        }],
        "macos",
    )
    .unwrap()
    .remove(0)
}

#[test]
fn a_menu_offers_only_what_applies() {
    let top = ["package.json".to_owned(), "src".to_owned()];
    let facts = Facts {
        repo: "gitea:a/b",
        top: Some(&top),
    };
    let mut eslint = command("eslint {file}");
    eslint.files = vec!["*.ts".to_owned(), "*.tsx".to_owned()];
    eslint.repo_holds = vec!["package.json".to_owned()];
    assert!(eslint.takes_file("src/a.ts", &facts));
    assert!(!eslint.takes_file("src/a.rs", &facts));
    assert!(!eslint.takes_folder("src", None, &facts));
    assert!(!eslint.takes_repository(&facts));
    let rust_top = ["Cargo.toml".to_owned()];
    assert!(!eslint.takes_file(
        "src/a.ts",
        &Facts {
            repo: "x",
            top: Some(&rust_top)
        }
    ));
    // An unread top counts as met.
    assert!(eslint.takes_file(
        "src/a.ts",
        &Facts {
            repo: "x",
            top: None
        }
    ));
    // An empty list takes everything.
    let any = command("wc -l {file}");
    assert!(any.takes_file("whatever.bin", &facts));

    let mut folder = command("ruff check {dir}");
    folder.has = vec!["*.py".to_owned()];
    let py = ["a.py".to_owned()];
    let none = ["a.ts".to_owned()];
    assert!(folder.takes_folder("scripts", Some(&py), &facts));
    assert!(!folder.takes_folder("scripts", Some(&none), &facts));
    assert!(folder.takes_folder("scripts", None, &facts));
    folder.folders = vec!["scripts".to_owned()];
    assert!(!folder.takes_folder("src", Some(&py), &facts));

    let mut repo = command("cargo clippy");
    assert!(repo.takes_repository(&facts));
    repo.repos = vec!["gitea:other".to_owned()];
    assert!(!repo.takes_repository(&facts));
    assert!(command("git show {sha}").takes_commit(&facts));
    assert!(command("git diff {base} {head}").takes_range(&facts));
    assert!(!command("git diff {base} {head}").takes_commit(&facts));
}

#[test]
fn a_saved_list_is_checked_trimmed_and_keeps_ids() {
    let drafts = [
        Draft {
            id: Some("eslint".to_owned()),
            name: " ESLint ".to_owned(),
            command: " eslint {file} ".to_owned(),
            files: vec!["*.ts  *.tsx".to_owned()],
            timeout_secs: Some(9000),
            env: vec![
                EnvVar {
                    name: "NODE_OPTIONS".to_owned(),
                    value: "--max-old-space-size=4096".to_owned(),
                },
                EnvVar::default(),
            ],
            path: vec!["{root}/node_modules/.bin".to_owned(), " ".to_owned()],
            ..Draft::default()
        },
        Draft {
            name: "ESLint".to_owned(),
            command: "eslint {dir}".to_owned(),
            timeout_secs: Some(0),
            ..Draft::default()
        },
        Draft {
            name: "Ruff!".to_owned(),
            command: "ruff check {file}".to_owned(),
            ..Draft::default()
        },
    ];
    let saved = settle(&drafts, "macos").unwrap();
    assert_eq!(saved[0].id, "eslint");
    assert_eq!(saved[0].name, "ESLint");
    assert_eq!(saved[0].command, "eslint {file}");
    assert_eq!(saved[0].files, ["*.ts", "*.tsx"]);
    assert_eq!(saved[0].timeout_secs, MAX_TIMEOUT);
    assert_eq!(saved[0].env.len(), 1);
    assert_eq!(saved[0].path, ["{root}/node_modules/.bin"]);
    assert!(saved[0].colours);
    assert_eq!(saved[1].id, "eslint-2");
    assert_eq!(saved[1].timeout_secs, DEFAULT_TIMEOUT);
    assert_eq!(saved[2].id, "ruff");
}

#[test]
fn a_list_with_an_unusable_entry_is_refused_by_name() {
    let one = |draft: Draft| settle(&[draft], "macos").unwrap_err().to_string();
    assert_eq!(
        one(Draft {
            name: " ".to_owned(),
            command: "x".to_owned(),
            ..Draft::default()
        }),
        "command 1 has no name"
    );
    assert!(
        one(Draft {
            name: "E".to_owned(),
            command: "eslint {path}".to_owned(),
            ..Draft::default()
        })
        .starts_with("E cannot be run: {path}")
    );
    for bad in ["KNOBAS_DB_URL", "knobas_x", "1A", "A-B", "A=B"] {
        let said = one(Draft {
            name: "E".to_owned(),
            command: "x".to_owned(),
            env: vec![EnvVar {
                name: bad.to_owned(),
                value: "v".to_owned(),
            }],
            ..Draft::default()
        });
        assert!(said.contains("environment line"), "{bad}: {said}");
    }
    let said = one(Draft {
        name: "E".to_owned(),
        command: "x".to_owned(),
        path: vec!["{file}/bin".to_owned()],
        ..Draft::default()
    });
    assert!(said.contains("PATH folder"), "{said}");
}

#[test]
fn a_saved_command_reads_back_with_its_defaults() {
    let json = r#"{"id":"x","name":"X","command":"x {file}"}"#;
    let read: OneOff = serde_json::from_str(json).unwrap();
    assert_eq!(read.timeout_secs, DEFAULT_TIMEOUT);
    assert!(read.colours);
    assert_eq!(read.stdin, Stdin::None);
    assert_eq!(read.kind(), Kind::File);
}
