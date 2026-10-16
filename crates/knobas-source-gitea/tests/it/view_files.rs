//! Reading code (issue #711, spec #701 workflow 11): the *Files* tab of a
//! repository's view (at its default branch) and a branch's (at that branch).
//!
//! The tab is deferred and lists the top directory; a directory and a file are
//! each a section of their own, named by an id that carries the path
//! (`files:src/lib`, `file:src/lib.rs`), read with `view(entity, section)`
//! and drawn in the same tab. Listings come from `contents/{path}?ref=`, files
//! from `raw/{path}?ref=` (Gitea 1.27.3's routes; `tests/live_gitea.rs`
//! re-asserts them against the seeded repository).

use knobas_source::view::{Block, Crumb, EntryKind, SourceView, TreeEntry, ViewSection};
use knobas_source::{Source, SourceError};
use serde_json::{Value, json};
use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::support::{TOKEN, instance};

const REPO: &str = "/api/v1/repos/tidewater/payout-service";
const ENTITY: &str = "gitea:tidewater/payout-service";
const BRANCH: &str = "gitea:tidewater/payout-service@refs/heads/feature/PAY-231-sepa-retry";
const WEB: &str = "https://gitea.example/tidewater/payout-service";

fn record(empty: bool) -> Value {
    json!({
        "full_name": "tidewater/payout-service", "default_branch": "main",
        "html_url": WEB, "private": true, "internal": false, "empty": empty,
        "mirror": false, "has_pull_requests": true,
        "stars_count": 0, "forks_count": 0, "size": 12
    })
}

fn entry(name: &str, path: &str, kind: &str, size: u64) -> Value {
    json!({ "name": name, "path": path, "type": kind, "size": size })
}

async fn gitea() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(REPO))
        .and(header("Authorization", format!("token {TOKEN}").as_str()))
        .respond_with(ResponseTemplate::new(200).set_body_json(record(false)))
        .mount(&server)
        .await;
    server
}

/// `contents/{dir}?ref={at}` answers `listing`.
async fn listing(server: &MockServer, dir: &str, at: &str, listing: Value) {
    let route = if dir.is_empty() {
        format!("{REPO}/contents")
    } else {
        format!("{REPO}/contents/{dir}")
    };
    Mock::given(method("GET"))
        .and(path(route))
        .and(query_param("ref", at))
        .and(header("Authorization", format!("token {TOKEN}").as_str()))
        .respond_with(ResponseTemplate::new(200).set_body_json(listing))
        .mount(server)
        .await;
}

/// `raw/{file}?ref={at}` answers `body`, and the file's own
/// `contents/{file}?ref={at}` entry its size and [`blob`]`(file, at)`.
async fn raw(server: &MockServer, file: &str, at: &str, body: Vec<u8>) {
    own_entry(server, file, at, body.len() as u64).await;
    Mock::given(method("GET"))
        .and(path(format!("{REPO}/raw/{file}")))
        .and(query_param("ref", at))
        .and(header("Authorization", format!("token {TOKEN}").as_str()))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(body))
        .mount(server)
        .await;
}

/// `contents/{file}?ref={at}` answers the file's own entry: `size`, and
/// [`blob`]`(file, at)` as its sha.
async fn own_entry(server: &MockServer, file: &str, at: &str, size: u64) {
    let name = file.rsplit('/').next().unwrap_or(file);
    let mut own = entry(name, file, "file", size);
    own["sha"] = json!(blob(file, at));
    listing(server, file, at, own).await;
}

/// The blob sha [`own_entry`] gives `file` at `at`: 40 hex digits folded
/// from both names, so a sha read at the wrong ref shows.
fn blob(file: &str, at: &str) -> String {
    let fold = |seed: u64| {
        format!("{file}@{at}").bytes().fold(seed, |h, b| {
            (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
        })
    };
    format!("{:016x}{:016x}{:08x}", fold(1), fold(2), fold(3) as u32)
}

fn source(server: &MockServer) -> Box<dyn Source> {
    knobas_source_gitea::build(instance(server.uri(), TOKEN, json!({}))).expect("builds")
}

async fn read(server: &MockServer, entity: &str, section: &str) -> ViewSection {
    let view: SourceView = source(server)
        .view(entity, Some(section))
        .await
        .unwrap_or_else(|e| panic!("reading {section:?} failed: {e}"))
        .expect("drawn");
    assert_eq!(
        view.sections.len(),
        1,
        "a section read answers that section"
    );
    let only = view.sections.into_iter().next().unwrap();
    assert_eq!(only.id, section, "the answer is the section asked for");
    assert_eq!(only.title, "Files", "it is drawn under the same tab");
    only
}

fn crumbs(pairs: &[(&str, &str)]) -> Vec<Crumb> {
    pairs
        .iter()
        .map(|(name, section)| Crumb {
            name: (*name).into(),
            section: (*section).into(),
        })
        .collect()
}

fn tree(section: &ViewSection) -> (&[Crumb], &[TreeEntry]) {
    match section.blocks.as_slice() {
        [Block::Tree { trail, entries, .. }] => (trail, entries),
        other => panic!("one tree block, not {other:#?}"),
    }
}

struct Drawn {
    trail: Vec<Crumb>,
    path: String,
    text: String,
    language: Option<String>,
    size: u64,
    truncated: bool,
    binary: bool,
    url: Option<String>,
    sha: Option<String>,
}

fn file(section: ViewSection) -> Drawn {
    match section.blocks.into_iter().next() {
        Some(Block::File {
            trail,
            path,
            text,
            language,
            size,
            truncated,
            binary,
            url,
            sha,
        }) => Drawn {
            trail,
            path,
            text,
            language,
            size,
            truncated,
            binary,
            url,
            sha,
        },
        other => panic!("one file block, not {other:#?}"),
    }
}

/// The tab sits last on the repository's view, deferred, and reads nothing
/// of the tree until it is opened.
#[tokio::test]
async fn a_repo_view_has_a_deferred_files_tab() {
    let server = gitea().await;
    // Everything else the whole view reads answers empty; the tab is what is
    // asked about here.
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .mount(&server)
        .await;
    let view = source(&server)
        .view(ENTITY, None)
        .await
        .expect("read")
        .expect("drawn");
    let files = view.sections.last().expect("tabs");
    assert_eq!(
        (
            files.id.as_str(),
            files.title.as_str(),
            files.count,
            files.deferred
        ),
        ("files", "Files", None, true)
    );
    assert!(files.blocks.is_empty());
    // The overview lists the top directory once, for its README; nothing
    // below it is listed and no file is read until the tab is opened.
    let tree_reads: Vec<String> = server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .map(|r| r.url.path().to_owned())
        .filter(|p| {
            p.starts_with(&format!("{REPO}/contents")) || p.starts_with(&format!("{REPO}/raw"))
        })
        .collect();
    assert_eq!(
        tree_reads,
        [format!("{REPO}/contents")],
        "the overview's README lookup only"
    );
}

/// The top directory at the default branch: directories first, then the
/// rest, each by name; a submodule cannot be opened here.
#[tokio::test]
async fn the_top_directory_lists_directories_first() {
    let server = gitea().await;
    listing(
        &server,
        "",
        "main",
        json!([
            entry("README.md", "README.md", "file", 60),
            entry("src", "src", "dir", 0),
            entry("Cargo.toml", "Cargo.toml", "file", 410),
            entry("vendor", "vendor", "submodule", 0),
            entry(".github", ".github", "dir", 0),
            entry("current", "current", "symlink", 7),
        ]),
    )
    .await;
    let section = read(&server, ENTITY, "files").await;
    let (trail, entries) = tree(&section);
    assert_eq!(
        trail,
        crumbs(&[("main", "files")]),
        "the ref is the top crumb"
    );
    let listed: Vec<_> = entries
        .iter()
        .map(|e| (e.name.as_str(), e.kind, e.size, e.section.as_deref()))
        .collect();
    assert_eq!(
        listed,
        [
            (".github", EntryKind::Dir, None, Some("files:.github")),
            ("src", EntryKind::Dir, None, Some("files:src")),
            (
                "Cargo.toml",
                EntryKind::File,
                Some(410),
                Some("file:Cargo.toml")
            ),
            ("current", EntryKind::Symlink, Some(7), Some("file:current")),
            (
                "README.md",
                EntryKind::File,
                Some(60),
                Some("file:README.md")
            ),
            ("vendor", EntryKind::Submodule, None, None),
        ]
    );
}

/// A directory below the top, opened by the id its parent's entry named:
/// its trail leads back up one directory at a time.
#[tokio::test]
async fn a_nested_directory_is_read_by_the_section_its_entry_names() {
    let server = gitea().await;
    listing(
        &server,
        "src/lib",
        "main",
        json!([
            entry("view.rs", "src/lib/view.rs", "file", 900),
            entry("diff", "src/lib/diff", "dir", 0),
        ]),
    )
    .await;
    let section = read(&server, ENTITY, "files:src/lib").await;
    let (trail, entries) = tree(&section);
    assert_eq!(
        trail,
        crumbs(&[
            ("main", "files"),
            ("src", "files:src"),
            ("lib", "files:src/lib")
        ])
    );
    let opens: Vec<_> = entries.iter().map(|e| e.section.as_deref()).collect();
    assert_eq!(
        opens,
        [Some("files:src/lib/diff"), Some("file:src/lib/view.rs")]
    );
}

/// A directory past 1,000 entries keeps the first 1,000 in order, says it
/// was cut, and names its page on Gitea, where the rest is.
#[tokio::test]
async fn a_directory_over_the_entry_cap_is_cut_at_it() {
    let server = gitea().await;
    let many: Vec<Value> = (0..1_001)
        .map(|n| {
            entry(
                &format!("f{n:04}.txt"),
                &format!("gen/f{n:04}.txt"),
                "file",
                1,
            )
        })
        .collect();
    listing(&server, "gen", "main", Value::Array(many)).await;
    let section = read(&server, ENTITY, "files:gen").await;
    let [
        Block::Tree {
            entries,
            truncated,
            url,
            ..
        },
    ] = section.blocks.as_slice()
    else {
        panic!("one tree block");
    };
    assert!(*truncated);
    assert_eq!(entries.len(), 1_000);
    assert_eq!(entries.last().map(|e| e.name.as_str()), Some("f0999.txt"));
    assert_eq!(
        url.as_deref(),
        Some(format!("{WEB}/src/branch/main/gen").as_str())
    );

    // Exactly at the cap is not cut.
    let at: Vec<Value> = (0..1_000)
        .map(|n| entry(&format!("g{n:04}"), &format!("all/g{n:04}"), "file", 1))
        .collect();
    listing(&server, "all", "main", Value::Array(at)).await;
    let section = read(&server, ENTITY, "files:all").await;
    let [
        Block::Tree {
            entries, truncated, ..
        },
    ] = section.blocks.as_slice()
    else {
        panic!("one tree block");
    };
    assert!(!*truncated);
    assert_eq!(entries.len(), 1_000);
}

/// A branch's tab is the same tree at that branch.
#[tokio::test]
async fn a_branch_reads_its_tree_at_the_branch() {
    let server = gitea().await;
    listing(
        &server,
        "",
        "feature/PAY-231-sepa-retry",
        json!([entry("retry.rs", "retry.rs", "file", 30)]),
    )
    .await;
    let section = read(&server, BRANCH, "files").await;
    let (trail, entries) = tree(&section);
    assert_eq!(trail, crumbs(&[("feature/PAY-231-sepa-retry", "files")]));
    assert_eq!(entries.len(), 1);
}

/// A file's text, with the trail to it, a language hint from its name, its
/// size and its page on Gitea.
#[tokio::test]
async fn a_file_is_read_raw_at_the_ref() {
    let server = gitea().await;
    let text = "fn main() {\n    println!(\"hi\");\n}\n";
    raw(&server, "src/main.rs", "main", text.as_bytes().to_vec()).await;
    let drawn = file(read(&server, ENTITY, "file:src/main.rs").await);
    assert_eq!(
        drawn.trail,
        crumbs(&[
            ("main", "files"),
            ("src", "files:src"),
            ("main.rs", "file:src/main.rs")
        ])
    );
    assert_eq!(drawn.path, "src/main.rs");
    assert_eq!(drawn.text, text);
    assert_eq!(drawn.language.as_deref(), Some("rust"));
    assert_eq!(drawn.size, text.len() as u64);
    assert!(!drawn.truncated);
    assert!(!drawn.binary);
    assert_eq!(
        drawn.url.as_deref(),
        Some(format!("{WEB}/src/branch/main/src/main.rs").as_str())
    );
    assert_eq!(
        drawn.sha,
        Some(blob("src/main.rs", "main")),
        "the blob an edit is made against, from the file's contents entry"
    );
}

/// A branch's file is drawn with the blob the branch holds, not the default
/// branch's (#883): the base an edit on that branch is made against.
#[tokio::test]
async fn a_branch_file_carries_the_blob_at_the_branch() {
    let server = gitea().await;
    let at = "feature/PAY-231-sepa-retry";
    raw(&server, "retry.rs", "main", b"old\n".to_vec()).await;
    raw(&server, "retry.rs", at, b"new\n".to_vec()).await;
    let drawn = file(read(&server, BRANCH, "file:retry.rs").await);
    assert_eq!(drawn.text, "new\n");
    assert_eq!(drawn.sha, Some(blob("retry.rs", at)));
    assert_ne!(blob("retry.rs", at), blob("retry.rs", "main"));
}

/// A Markdown file says so, so the shell can draw it rendered.
#[tokio::test]
async fn a_markdown_file_is_hinted_as_markdown() {
    let server = gitea().await;
    raw(&server, "docs/README.MD", "main", b"# hi\n".to_vec()).await;
    let drawn = file(read(&server, ENTITY, "file:docs/README.MD").await);
    assert_eq!(drawn.language.as_deref(), Some("markdown"));
}

/// Over 10,000 lines: the first 10,000, and `truncated`.
#[tokio::test]
async fn a_file_over_the_line_cap_is_cut_at_it() {
    let server = gitea().await;
    let text: String = (1..=10_001).map(|n| format!("line {n}\n")).collect();
    raw(&server, "big.txt", "main", text.clone().into_bytes()).await;
    let drawn = file(read(&server, ENTITY, "file:big.txt").await);
    assert!(drawn.truncated);
    assert_eq!(drawn.text.lines().count(), 10_000);
    assert_eq!(drawn.text.lines().last(), Some("line 10000"));
    assert_eq!(
        drawn.size,
        text.len() as u64,
        "the size is the whole file's"
    );

    // Exactly at the cap is not cut.
    let at: String = (1..=10_000).map(|n| format!("line {n}\n")).collect();
    raw(&server, "at.txt", "main", at.clone().into_bytes()).await;
    let drawn = file(read(&server, ENTITY, "file:at.txt").await);
    assert!(!drawn.truncated);
    assert_eq!(drawn.text, at);
}

/// Over 1 MB: cut at the last line that fits, and `truncated`, with the
/// whole file's size.
#[tokio::test]
async fn a_file_over_the_byte_cap_is_cut_at_a_line_within_it() {
    let server = gitea().await;
    // 2,000 lines of 997 bytes: 2 MB, well under the line cap, and the
    // cap falls inside line 1,004, so only whole lines survive the cut.
    let line = format!("{}\n", "x".repeat(996));
    let text = line.repeat(2_000);
    raw(&server, "wide.txt", "main", text.clone().into_bytes()).await;
    let drawn = file(read(&server, ENTITY, "file:wide.txt").await);
    assert!(drawn.truncated);
    assert_eq!(drawn.text, line.repeat(1_003), "whole lines within 1 MB");
    assert_eq!(drawn.size, text.len() as u64);
    assert!(drawn.url.is_some(), "the rest is on Gitea");
}

/// A cut file whose answer carried no length -- Gitea behind a proxy that
/// compresses, which `reqwest` undoes and so drops the header -- takes its
/// size from the file's own `contents` entry, never from the bytes read.
#[tokio::test]
async fn a_cut_file_without_a_length_asks_gitea_its_size() {
    use std::io::Write as _;
    let server = gitea().await;
    let line = format!("{}\n", "y".repeat(996));
    let text = line.repeat(3_000);
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    gz.write_all(text.as_bytes()).unwrap();
    let body = gz.finish().unwrap();
    Mock::given(method("GET"))
        .and(path(format!("{REPO}/raw/big.txt")))
        .and(query_param("ref", "main"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Content-Encoding", "gzip")
                .set_body_bytes(body),
        )
        .mount(&server)
        .await;
    own_entry(&server, "big.txt", "main", text.len() as u64).await;
    let drawn = file(read(&server, ENTITY, "file:big.txt").await);
    assert!(drawn.truncated);
    assert_eq!(drawn.text, line.repeat(1_003));
    assert_eq!(drawn.size, text.len() as u64, "the whole file's size");
    assert_eq!(drawn.sha, Some(blob("big.txt", "main")));
}

/// A file with a NUL byte is binary: no text, its size, its page.
#[tokio::test]
async fn a_binary_file_says_so_with_its_size() {
    let server = gitea().await;
    let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
    png.extend(std::iter::repeat_n(7u8, 2_000));
    raw(&server, "logo.png", "main", png.clone()).await;
    let drawn = file(read(&server, ENTITY, "file:logo.png").await);
    assert!(drawn.binary);
    assert!(drawn.text.is_empty());
    assert_eq!(drawn.size, png.len() as u64);
    assert!(drawn.url.is_some());
}

/// A path that would walk out of the route is refused before anything is
/// asked.
#[tokio::test]
async fn a_path_that_walks_out_is_refused() {
    let server = gitea().await;
    for section in ["file:../secrets", "files:src/../..", "file:", "files:a//b"] {
        let error = source(&server)
            .view(ENTITY, Some(section))
            .await
            .expect_err(section);
        assert!(
            matches!(error, SourceError::Protocol { .. }),
            "{section}: {error:?}"
        );
    }
    let asked: Vec<_> = server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .map(|r| r.url.path().to_owned())
        .filter(|p| p != REPO)
        .collect();
    assert!(asked.is_empty(), "{asked:?}");
}
