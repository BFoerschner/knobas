//! Places and findings, from the design's cases and captured output of real
//! tools (`fixtures/`).

use std::collections::HashSet;
use std::path::Path;

use super::*;

struct Files(HashSet<&'static str>);

impl Exists for Files {
    fn exists(&mut self, relative: &str) -> bool {
        self.0.contains(relative)
    }
}

fn files() -> Files {
    Files(
        [
            "src/retry/window.ts",
            "orders/order-4711.xml",
            "scripts/coverage-report.py",
            "scripts/deploy.sh",
            "eslint.config.js",
            "src/main.rs",
            "packages/web/src/a.ts",
        ]
        .into_iter()
        .collect(),
    )
}

fn win<'a>(disk: &'a mut Files) -> Ctx<'a> {
    Ctx {
        root: "C:/src/payout-service".to_owned(),
        alias: None,
        cwd: String::new(),
        caseless: true,
        disk,
    }
}

fn links(text: &str, state: &mut State) -> Vec<(String, String, Option<u32>, Option<u32>)> {
    let mut disk = files();
    let mut ctx = win(&mut disk);
    find(text, &mut ctx, state)
        .into_iter()
        .map(|p| {
            let shown: String = text
                .encode_utf16()
                .skip(p.from as usize)
                .take((p.to - p.from) as usize)
                .map(|u| char::from_u32(u32::from(u)).unwrap_or('?'))
                .collect();
            (shown, p.path, p.line, p.col)
        })
        .collect()
}

fn l(text: &str) -> Vec<(String, String, Option<u32>, Option<u32>)> {
    links(text, &mut State::default())
}

fn one(
    shown: &str,
    path: &str,
    line: Option<u32>,
    col: Option<u32>,
) -> Vec<(String, String, Option<u32>, Option<u32>)> {
    vec![(shown.to_owned(), path.to_owned(), line, col)]
}

#[test]
fn each_shape_is_a_place() {
    assert_eq!(
        l("src/retry/window.ts:12:7: error: x"),
        one(
            "src/retry/window.ts:12:7",
            "src/retry/window.ts",
            Some(12),
            Some(7)
        )
    );
    assert_eq!(
        l(
            "C:\\src\\payout-service\\orders\\order-4711.xml:118: element ORDER_UNIT: Schemas validity error : bad"
        ),
        one(
            "C:\\src\\payout-service\\orders\\order-4711.xml:118",
            "orders/order-4711.xml",
            Some(118),
            None
        )
    );
    assert_eq!(
        l("src/retry/window.ts(27,21): error TS7006: x"),
        one(
            "src/retry/window.ts(27,21)",
            "src/retry/window.ts",
            Some(27),
            Some(21)
        )
    );
    assert_eq!(
        l("  File \"scripts/coverage-report.py\", line 41, in main"),
        one(
            "scripts/coverage-report.py\", line 41",
            "scripts/coverage-report.py",
            Some(41),
            None
        )
    );
    assert_eq!(
        l("  --> src/main.rs:4:9"),
        one("src/main.rs:4:9", "src/main.rs", Some(4), Some(9))
    );
    assert_eq!(
        l("In scripts/deploy.sh line 12:"),
        one(
            "scripts/deploy.sh line 12",
            "scripts/deploy.sh",
            Some(12),
            None
        )
    );
    // Case is ignored on Windows, and ./ is the top.
    assert_eq!(
        l("c:/SRC/payout-service/src/main.rs:1:1")[0].1,
        "src/main.rs"
    );
    assert_eq!(l("./src/main.rs:2")[0].1, "src/main.rs");
}

#[test]
fn what_is_not_a_file_inside_the_checkout_is_no_link() {
    for text in [
        "/etc/passwd:1:1: x",
        "../../etc/passwd:1: x",
        "src/../../x/src/main.rs:1: x",
        // A climb past the top names a file beside the checkout, never the checkout's own of that name.
        "../src/main.rs:1: x",
        "src/../../src/main.rs:1: x",
        "src/nope.ts:1:1: x",
        "    at Module._resolveFilename (node:internal/modules/cjs/loader:1249:15)",
        "C:/src/payout-service-other/src/main.rs:3:1",
        "12:30:01 started",
    ] {
        assert_eq!(l(text), vec![], "{text}");
    }
}

#[test]
fn eslint_stylish_a_path_then_rows() {
    let mut state = State::default();
    assert_eq!(
        links("C:\\src\\payout-service\\src\\retry\\window.ts", &mut state),
        one(
            "C:\\src\\payout-service\\src\\retry\\window.ts",
            "src/retry/window.ts",
            None,
            None
        )
    );
    assert_eq!(
        links(
            "  12:7   error    'attempts' is never reassigned  prefer-const",
            &mut state
        ),
        one("12:7", "src/retry/window.ts", Some(12), Some(7))
    );
    // Without the path line before it a row is nothing.
    assert_eq!(l("  12:7   error    x  y"), vec![]);
}

#[test]
fn a_path_relative_to_the_working_directory_first() {
    let mut disk = files();
    let mut ctx = Ctx {
        cwd: "packages/web".to_owned(),
        ..win(&mut disk)
    };
    let places = find("src/a.ts:3:1: x", &mut ctx, &mut State::default());
    assert_eq!(places[0].path, "packages/web/src/a.ts");
    // Then the root.
    let places = find("src/main.rs:3:1", &mut ctx, &mut State::default());
    assert_eq!(places[0].path, "src/main.rs");
}

#[test]
fn a_path_as_wsl_sees_it_maps_back() {
    let mut disk = files();
    let mut ctx = Ctx {
        alias: Some("/mnt/c/src/payout-service".to_owned()),
        ..win(&mut disk)
    };
    let places = find(
        "/mnt/c/src/payout-service/src/main.rs:9:2: E501",
        &mut ctx,
        &mut State::default(),
    );
    assert_eq!(places[0].path, "src/main.rs");
}

fn problem(text: &str, state: &mut State) -> Option<Finding> {
    let mut disk = files();
    let mut ctx = win(&mut disk);
    read_line(text, &mut ctx, state).1
}

fn finding(severity: &str, said: &str, rule: &str) -> Option<Finding> {
    Some(Finding {
        severity: severity.to_owned(),
        said: said.to_owned(),
        rule: rule.to_owned(),
    })
}

#[test]
fn findings_from_each_tool() {
    let mut st = State::default();
    problem("C:\\src\\payout-service\\src\\retry\\window.ts", &mut st);
    assert_eq!(
        problem(
            "  12:7   error    'attempts' is never reassigned. Use 'const' instead   prefer-const",
            &mut st
        ),
        finding(
            "error",
            "'attempts' is never reassigned. Use 'const' instead",
            "prefer-const"
        )
    );
    assert_eq!(
        problem(
            "C:\\src\\payout-service\\orders\\order-4711.xml:118: element ORDER_UNIT: Schemas validity error : The value 'PCE' is not allowed.",
            &mut State::default()
        ),
        finding("error", "The value 'PCE' is not allowed.", "ORDER_UNIT")
    );
    assert_eq!(
        problem(
            "src/retry/window.ts(27,21): error TS7006: Parameter 'x' implicitly has an 'any' type.",
            &mut State::default()
        ),
        finding(
            "error",
            "Parameter 'x' implicitly has an 'any' type.",
            "TS7006"
        )
    );
    assert_eq!(
        problem(
            "src/retry/window.ts:3:1: F401 [*] `os` imported but unused",
            &mut State::default()
        ),
        finding("", "[*] `os` imported but unused", "F401")
    );
    assert_eq!(
        problem(
            "scripts/deploy.sh:12:5: warning: Double quote to prevent globbing. [SC2086]",
            &mut State::default()
        ),
        finding("warning", "Double quote to prevent globbing.", "SC2086")
    );
}

#[test]
fn a_frame_and_a_traceback_row_are_places_not_findings() {
    assert_eq!(
        problem(
            "    at Object.<anonymous> (C:\\src\\payout-service\\eslint.config.js:2:18)",
            &mut State::default()
        ),
        None
    );
    assert_eq!(
        problem(
            "  File \"scripts/coverage-report.py\", line 41, in main",
            &mut State::default()
        ),
        None
    );
}

#[test]
fn rustc_says_what_then_where() {
    let mut st = State::default();
    assert_eq!(problem("error[E0308]: mismatched types", &mut st), None);
    assert_eq!(
        problem("  --> src/main.rs:4:9", &mut st),
        finding("error", "mismatched types", "E0308")
    );
    assert_eq!(problem("  --> src/main.rs:4:9", &mut st), None);
    problem("warning: unused variable: `x`", &mut st);
    assert_eq!(
        problem("   --> src/main.rs:7:5", &mut st),
        finding("warning", "unused variable: `x`", "")
    );
}

#[test]
fn a_web_address_is_a_url_and_nothing_else_is() {
    let found = urls("see https://eslint.org/docs/rules/prefer-const. or http://x.io/a?b=1)");
    let hrefs: Vec<&str> = found.iter().map(|u| u.href.as_str()).collect();
    assert_eq!(
        hrefs,
        [
            "https://eslint.org/docs/rules/prefer-const",
            "http://x.io/a?b=1"
        ]
    );
    assert!(urls("file:///etc/passwd javascript:alert(1)").is_empty());
    // A namespace in xmllint's braces ends at the brace.
    let ns = urls("Element '{http://www.opentrans.org/XMLSchema/2.1}ORDER_UNIT': bad");
    assert_eq!(ns[0].href, "http://www.opentrans.org/XMLSchema/2.1");
}

#[test]
fn a_line_past_the_search_cap_is_not_searched() {
    let long = format!("src/main.rs:1:1 {}", "x".repeat(SEARCHED));
    assert_eq!(l(&long), vec![]);
}

#[test]
fn the_disk_refuses_a_link_out_and_a_folder() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("secret.txt"), "x").unwrap();
    std::fs::create_dir(root.path().join("src")).unwrap();
    std::fs::write(root.path().join("src/a.ts"), "x").unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(
        outside.path().join("secret.txt"),
        root.path().join("out.txt"),
    )
    .unwrap();
    let mut disk = Disk::new(root.path());
    assert!(disk.exists("src/a.ts"));
    assert!(!disk.exists("src"));
    assert!(!disk.exists("nope.ts"));
    #[cfg(unix)]
    assert!(!disk.exists("out.txt"));
}

/// Captured output of the real tools, run on the WSL box against
/// `fixtures/repo` (`fixtures/README.md` says how): every place the file
/// names becomes a link, and the findings are counted.
#[test]
fn captured_output_of_real_tools() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/oneoff/places/fixtures");
    let mut checked = 0;
    for (file, cwd, want_places, want_findings) in FIXTURES {
        let text =
            std::fs::read_to_string(dir.join(file)).unwrap_or_else(|e| panic!("{file}: {e}"));
        let mut disk = Files(FIXTURE_FILES.iter().copied().collect());
        let mut ctx = Ctx {
            root: "/home/dev/runcmd/fixrepo".to_owned(),
            alias: None,
            cwd: (*cwd).to_owned(),
            caseless: false,
            disk: &mut disk,
        };
        let mut state = State::default();
        let (mut places, mut findings) = (0, 0);
        let mut kept = super::super::kept::Kept::default();
        let mut reader = super::super::lines::Reader::default();
        reader.write(&mut kept, super::super::lines::Stream::Out, text.as_bytes());
        reader.finish(&mut kept);
        for line in kept.all() {
            let (p, f, _) = read_line(&line.text, &mut ctx, &mut state);
            places += p.iter().filter(|p| p.line.is_some()).count();
            findings += usize::from(f.is_some());
        }
        assert_eq!((places, findings), (*want_places, *want_findings), "{file}");
        checked += 1;
    }
    assert!(checked >= 6);
}

/// The files of the repository the fixtures were captured on.
const FIXTURE_FILES: [&str; 7] = [
    "src/retry/window.js",
    "src/retry/window.ts",
    "scripts/report.py",
    "scripts/deploy.sh",
    "orders/order-4711.xml",
    "orders/broken.xml",
    "crate/src/main.rs",
];

/// Each capture, the folder it ran in, and the places with a line and the
/// findings it holds, counted by hand from the file.
const FIXTURES: &[(&str, &str, usize, usize)] = &[
    ("eslint-stylish.txt", "", 4, 4),
    ("eslint-stylish-abs.txt", "", 4, 4),
    ("tsc.txt", "", 2, 2),
    ("ruff.txt", "", 3, 3),
    ("ruff-full.txt", "", 3, 3),
    ("shellcheck-gcc.txt", "", 5, 5),
    ("shellcheck-tty.txt", "", 8, 5),
    ("xmllint-schema.txt", "", 2, 2),
    ("xmllint-broken.txt", "", 1, 1),
    ("python-traceback.txt", "", 2, 0),
    ("rustc.txt", "crate", 1, 1),
];

/// Björn, 2026-11-25: a generated folder kept under the profile on macOS is
/// under `Application Support`, so every absolute path a tool printed there
/// held a space and was no link. The run's own root reads as one word.
#[test]
fn a_root_whose_name_holds_a_space_or_brackets_is_one_path() {
    let mut disk = files();
    for root in [
        "/Users/mara/Library/Application Support/dev.knobas.desktop/kept/orders.site.1",
        "C:/Program Files (x86)/kept [1]/orders.site.1",
    ] {
        let mut ctx = Ctx {
            root: root.to_owned(),
            alias: None,
            cwd: String::new(),
            caseless: root.starts_with("C:"),
            disk: &mut disk,
        };
        let printed = format!(
            "{root}/scripts/deploy.sh:3:8: note: Double quote to prevent globbing. [SC2086]"
        );
        let places = find(&printed, &mut ctx, &mut State::default());
        assert_eq!(places.len(), 1, "{printed}");
        let place = &places[0];
        assert_eq!(
            (place.path.as_str(), place.line, place.col),
            ("scripts/deploy.sh", Some(3), Some(8))
        );
        assert_eq!(place.from, 0);
        // Backslashes, as a Windows tool prints the same root.
        let back = printed.replace('/', "\\");
        let places = find(&back, &mut ctx, &mut State::default());
        assert_eq!(places.len(), 1, "{back}");
        assert_eq!(places[0].path, "scripts/deploy.sh");
        // Another folder with a space in it is still no path.
        let other = "/Users/mara/Library/Application Support/other/scripts/deploy.sh:3:8: x";
        assert!(
            find(other, &mut ctx, &mut State::default())
                .iter()
                .all(|p| p.from > 0)
        );
    }
}
