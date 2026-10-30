//! Reading code (issue #711, spec #701 workflow 11): the *Files* tab of a
//! repository's view (at its default branch) and a branch's (at that branch).
//!
//! The tab is deferred and lists the top directory; a directory and a file are
//! each a section of their own, named by an id that carries the path
//! (`files:src/lib`, `file:src/lib.rs`), read with `view(entity, section)`
//! and drawn in the same tab. Listings come from `contents/{path}?ref=`, files
//! from `raw/{path}?ref=` (Gitea 1.27.3's routes; `tests/live_gitea.rs`
//! re-asserts them against the seeded repository).
//!
//! A branch's and a repository's tree are read at the commit their tip is
//! (#2241): an id without a sha reads the tip first, and every id the answer
//! hands out names it (`files@{sha}:src`, `file@{sha}:src/lib.rs`), so a
//! file opened from the tree reads nothing but itself.

use knobas_source::view::{
    Block, Crumb, EntryKind, FileBytes, SourceView, Standing, TreeEntry, ViewSection,
};
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
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path(REPO))
        .and(header("Authorization", format!("token {TOKEN}").as_str()))
        .respond_with(ResponseTemplate::new(200).set_body_json(record(false)))
        .mount(&server)
        .await;
    for branch in ["main", "feature/PAY-231-sepa-retry"] {
        Mock::given(method("GET"))
            .and(path(format!(
                "{REPO}/branches/{}",
                branch.replace('/', "%2F")
            )))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "name": branch, "protected": false, "commit": { "id": tip(branch) }
            })))
            .mount(&server)
            .await;
    }
    server
}

/// The commit `branches/{at}` names as a branch's tip: 40 hex digits folded
/// from its name. A whole sha is its own.
fn tip(at: &str) -> String {
    if at.len() == 40 && at.chars().all(|c| c.is_ascii_hexdigit()) {
        return at.to_owned();
    }
    let fold = |seed: u64| {
        at.bytes().fold(seed, |h, b| {
            (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
        })
    };
    format!("{:016x}{:016x}{:08x}", fold(7), fold(8), fold(9) as u32)
}

/// Gitea's page of `path` at `at`'s commit, as its entries name it.
fn page_at(at: &str, path: &str) -> String {
    format!("{WEB}/src/commit/{}/{path}", tip(at))
}

/// `contents/{dir}?ref={at}` answers `listing`, at the commit `at` names
/// ([`tip`]), each entry with its page.
async fn listing(server: &MockServer, dir: &str, at: &str, mut listing: Value) {
    for entry in listing.as_array_mut().into_iter().flatten() {
        let path = entry["path"].as_str().unwrap_or_default().to_owned();
        entry["html_url"] = json!(page_at(at, &path));
    }
    if listing.is_object() {
        let path = listing["path"].as_str().unwrap_or_default().to_owned();
        listing["html_url"] = json!(page_at(at, &path));
    }
    let route = if dir.is_empty() {
        format!("{REPO}/contents")
    } else {
        format!("{REPO}/contents/{dir}")
    };
    Mock::given(method("GET"))
        .and(path(route))
        .and(query_param("ref", tip(at).as_str()))
        .and(header("Authorization", format!("token {TOKEN}").as_str()))
        .respond_with(ResponseTemplate::new(200).set_body_json(listing))
        .mount(server)
        .await;
}

/// `raw/{file}?ref={at}` answers `body`, and the file's own
/// `contents-ext/{file}?ref={at}` entry its size and [`blob`]`(file, at)`.
async fn raw(server: &MockServer, file: &str, at: &str, body: Vec<u8>) {
    own_entry(server, file, at, body.len() as u64).await;
    raw_only(server, file, at, body).await;
}

/// `raw/{file}?ref={at}` answers `body`, and nothing else is mounted.
async fn raw_only(server: &MockServer, file: &str, at: &str, body: Vec<u8>) {
    Mock::given(method("GET"))
        .and(path(format!("{REPO}/raw/{file}")))
        .and(query_param("ref", tip(at).as_str()))
        .and(header("Authorization", format!("token {TOKEN}").as_str()))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(body))
        .mount(server)
        .await;
}

/// `contents-ext/{file}?ref={at}` answers the file's own entry, without
/// its bytes: `size`, and [`blob`]`(file, at)` as its sha.
async fn own_entry(server: &MockServer, file: &str, at: &str, size: u64) {
    Mock::given(method("GET"))
        .and(path(format!("{REPO}/contents-ext/{file}")))
        .and(query_param("ref", tip(at).as_str()))
        .and(header("Authorization", format!("token {TOKEN}").as_str()))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({ "file_contents": own(file, at, size) })),
        )
        .mount(server)
        .await;
}

/// The entry [`own_entry`] answers for `file` at `at`.
fn own(file: &str, at: &str, size: u64) -> Value {
    let name = file.rsplit('/').next().unwrap_or(file);
    let mut own = entry(name, file, "file", size);
    own["sha"] = json!(blob(file, at));
    own["html_url"] = json!(page_at(at, file));
    own
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

/// `id` as a tree read at `at` hands it out: naming `at`'s commit.
fn named(at: &str, id: &str) -> String {
    let sha = tip(at);
    match id.strip_prefix("files") {
        Some(rest) => format!("files@{sha}{rest}"),
        None => format!("file@{sha}{}", id.strip_prefix("file").unwrap_or(id)),
    }
}

/// [`crumbs`], each section as a tree read at `at` hands it out.
fn crumbs_at(at: &str, pairs: &[(&str, &str)]) -> Vec<Crumb> {
    pairs
        .iter()
        .map(|(name, section)| Crumb {
            name: (*name).into(),
            section: named(at, section),
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
    commit: Option<String>,
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
            commit,
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
            commit,
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
        crumbs_at("main", &[("main", "files")]),
        "the ref is the top crumb, opened at the tip"
    );
    let listed: Vec<_> = entries
        .iter()
        .map(|e| (e.name.as_str(), e.kind, e.size, e.section.clone()))
        .collect();
    let at = |id: &str| Some(named("main", id));
    assert_eq!(
        listed,
        [
            (".github", EntryKind::Dir, None, at("files:.github")),
            ("src", EntryKind::Dir, None, at("files:src")),
            (
                "Cargo.toml",
                EntryKind::File,
                Some(410),
                at("file:Cargo.toml")
            ),
            ("current", EntryKind::Symlink, Some(7), at("file:current")),
            ("README.md", EntryKind::File, Some(60), at("file:README.md")),
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
    let id = named("main", "files:src/lib");
    let section = read(&server, ENTITY, &id).await;
    let (trail, entries) = tree(&section);
    let short = &tip("main")[..10];
    assert_eq!(
        trail,
        crumbs_at(
            "main",
            &[
                (short, "files"),
                ("src", "files:src"),
                ("lib", "files:src/lib")
            ]
        ),
        "a repository's read at a sha names it by the sha"
    );
    let opens: Vec<_> = entries.iter().map(|e| e.section.clone()).collect();
    assert_eq!(
        opens,
        [
            Some(named("main", "files:src/lib/diff")),
            Some(named("main", "file:src/lib/view.rs"))
        ]
    );
    let asked = server.received_requests().await.unwrap();
    assert_eq!(
        asked.len(),
        1,
        "a directory at a sha reads its listing alone: {:?}",
        asked.iter().map(|r| r.url.path()).collect::<Vec<_>>()
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
    assert_eq!(url.as_deref(), Some(page_at("main", "gen").as_str()));

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
    let at = "feature/PAY-231-sepa-retry";
    assert_eq!(trail, crumbs_at(at, &[(at, "files")]));
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].section, Some(named(at, "file:retry.rs")));
    let [Block::Tree { url, .. }] = section.blocks.as_slice() else {
        panic!("one tree block");
    };
    assert_eq!(
        url.as_deref(),
        Some(format!("{WEB}/src/branch/feature/PAY-231-sepa-retry").as_str()),
        "a branch's pages are named by the branch"
    );
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
        crumbs_at(
            "main",
            &[
                ("main", "files"),
                ("src", "files:src"),
                ("main.rs", "file:src/main.rs")
            ]
        ),
        "a file read with no sha reads the tip first, and its trail names it"
    );
    assert_eq!(drawn.path, "src/main.rs");
    assert_eq!(drawn.text, text);
    assert_eq!(drawn.language.as_deref(), Some("rust"));
    assert_eq!(drawn.size, text.len() as u64);
    assert!(!drawn.truncated);
    assert!(!drawn.binary);
    assert_eq!(
        drawn.url.as_deref(),
        Some(page_at("main", "src/main.rs").as_str())
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

/// A Gitea from before `contents-ext` (1.24) answers its route 404: the
/// file's `contents` entry is read for the blob instead (#883).
#[tokio::test]
async fn a_gitea_without_contents_ext_draws_the_blob_from_contents() {
    let server = gitea().await;
    raw_only(&server, "src/main.rs", "main", b"fn main() {}\n".to_vec()).await;
    listing(
        &server,
        "src/main.rs",
        "main",
        own("src/main.rs", "main", 13),
    )
    .await;
    let source = source(&server);
    for _ in 0..2 {
        let view = source
            .view(ENTITY, Some(&named("main", "file:src/main.rs")))
            .await
            .unwrap()
            .unwrap();
        let drawn = file(view.sections.into_iter().next().unwrap());
        assert_eq!(drawn.text, "fn main() {}\n");
        assert_eq!(drawn.sha, Some(blob("src/main.rs", "main")));
    }
    let missing_route = server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|r| r.url.path().contains("/contents-ext/"))
        .count();
    assert_eq!(
        missing_route, 1,
        "a route found missing is not asked again (#2241)"
    );
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
        .and(query_param("ref", tip("main").as_str()))
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

/// `branches/main` answers whether it is protected and whether this account
/// may push to it anyway, over [`gitea`]'s own answer.
async fn branch_main(server: &MockServer, protected: bool, user_can_push: bool) {
    Mock::given(method("GET"))
        .and(path(format!("{REPO}/branches/main")))
        .and(header("Authorization", format!("token {TOKEN}").as_str()))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "name": "main", "protected": protected, "user_can_push": user_can_push,
            "commit": { "id": tip("main") }
        })))
        .with_priority(1)
        .mount(server)
        .await;
}

/// **A file opened from the tree costs two requests** (#2241): its own entry
/// and its bytes at the sha the id names. Nothing of the repository, nothing
/// of the branch, no grants probe, and no action beside it: *Commit change…*'s
/// standing is `refused_writes`'.
#[tokio::test]
async fn a_file_at_the_sha_its_id_names_reads_only_itself() {
    let server = gitea().await;
    raw(&server, "src/main.rs", "main", b"fn main() {}\n".to_vec()).await;
    branch_main(&server, true, false).await;
    for entity in [ENTITY, BRANCH] {
        let id = named("main", "file:src/main.rs");
        let section = read(&server, entity, &id).await;
        assert_eq!(section.blocks.len(), 1, "no action: {:#?}", section.blocks);
        assert_eq!(file(section).text, "fn main() {}\n");
    }
    let mut asked: Vec<String> = server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .map(|r| format!("{} {}", r.method, r.url.path()))
        .collect();
    asked.sort();
    let entry = format!("GET {REPO}/contents-ext/src/main.rs");
    let bytes = format!("GET {REPO}/raw/src/main.rs");
    assert_eq!(
        asked,
        [entry.clone(), entry, bytes.clone(), bytes],
        "only the file's own two reads, once per entity"
    );
}

/// #985 (R-8.6, story 94), now asked through `refused_writes` (#2241): a
/// commit onto a protected branch this account may not push to is refused
/// with the reason the flush refuses it with (#822's rule and sentence), so
/// the dialog can say it before the reader writes the message; one it may
/// push to, or an unprotected one, is not.
#[tokio::test]
async fn a_commit_onto_a_protected_branch_is_refused_with_the_flushs_reason() {
    for (protected, user_can_push, refused) in [
        (true, false, true),
        (true, true, false),
        (false, true, false),
    ] {
        let server = gitea().await;
        branch_main(&server, protected, user_can_push).await;
        let ask = knobas_source::grants::WriteAsk {
            entity: ENTITY.to_owned(),
            op: "commit_file".to_owned(),
            field: None,
        };
        let answer = source(&server)
            .refused_writes(std::slice::from_ref(&ask))
            .await
            .unwrap();
        let reasons: Vec<_> = answer.iter().map(|r| r.reason.as_str()).collect();
        if refused {
            assert_eq!(
                reasons,
                [
                    "main is a protected branch that this account may not push to: commit to a \
                     new branch and open a pull request instead"
                ]
            );
        } else {
            assert!(reasons.is_empty(), "protected {protected}: {reasons:?}");
        }
    }
}

/// *New branch…* asked through `refused_writes` (#2243, as the repository's
/// toolbar draws it): refused in a mirror and in an empty repository, with
/// the toolbar's own sentences; offered in a plain one.
#[tokio::test]
async fn a_new_branch_is_refused_in_a_mirror_and_an_empty_repository() {
    for (mirror, empty, reason) in [
        (
            true,
            false,
            Some("A mirror: its branches come from upstream"),
        ),
        (false, true, Some("The repository is empty")),
        (false, false, None),
    ] {
        let server = gitea().await;
        let mut held = record(empty);
        held["mirror"] = json!(mirror);
        Mock::given(method("GET"))
            .and(path(REPO))
            .respond_with(ResponseTemplate::new(200).set_body_json(held))
            .with_priority(1)
            .mount(&server)
            .await;
        let ask = knobas_source::grants::WriteAsk {
            entity: ENTITY.to_owned(),
            op: "create_branch".to_owned(),
            field: None,
        };
        let answer = source(&server)
            .refused_writes(std::slice::from_ref(&ask))
            .await
            .unwrap();
        let reasons: Vec<_> = answer.iter().map(|r| r.reason.as_str()).collect();
        assert_eq!(
            reasons,
            reason.into_iter().collect::<Vec<_>>(),
            "mirror {mirror}, empty {empty}"
        );
    }
}

/// A directory takes no commit, and reads no branch for one once it knows
/// its sha.
#[tokio::test]
async fn a_directory_declares_no_commit() {
    let server = gitea().await;
    listing(
        &server,
        "",
        "main",
        json!([entry("a.rs", "a.rs", "file", 3)]),
    )
    .await;
    let section = read(&server, ENTITY, &named("main", "files")).await;
    assert!(
        section
            .blocks
            .iter()
            .all(|block| !matches!(block, Block::Actions { .. }))
    );
}

// -- the tree at a commit (#2239) ---------------------------------------------

const SHA: &str = "c90d11a3f5e2b7c4d9018e6a2b3c4d5e6f708192";

fn at_commit(sha: &str) -> String {
    format!("{ENTITY}@{sha}")
}

/// A commit's view reads the tree's sections at its sha: the file at that
/// commit, its page at the commit, and no *Commit change…*, since nothing is
/// committed onto a commit (so no branch is read for it either).
#[tokio::test]
async fn a_commit_reads_its_files_at_its_sha() {
    let server = gitea().await;
    raw(&server, "retry.rs", "main", b"tip\n".to_vec()).await;
    raw(&server, "retry.rs", SHA, b"then\n".to_vec()).await;
    let section = read(&server, &at_commit(SHA), "file:retry.rs").await;
    assert_eq!(section.blocks.len(), 1, "no action beside the file");
    let drawn = file(section);
    assert_eq!(drawn.text, "then\n");
    assert_eq!(drawn.sha, Some(blob("retry.rs", SHA)));
    assert_eq!(
        drawn.trail,
        crumbs(&[(SHA, "files"), ("retry.rs", "file:retry.rs")])
    );
    assert_eq!(
        drawn.url.as_deref(),
        Some(format!("{WEB}/src/commit/{SHA}/retry.rs").as_str())
    );
    let branch_reads = server
        .received_requests()
        .await
        .unwrap()
        .into_iter()
        .filter(|r| r.url.path().contains("/branches/"))
        .count();
    assert_eq!(branch_reads, 0, "a commit has no branch to commit onto");
}

#[tokio::test]
async fn a_commit_lists_its_directories_at_its_sha() {
    let server = gitea().await;
    listing(
        &server,
        "",
        SHA,
        json!([entry("retry.rs", "retry.rs", "file", 5)]),
    )
    .await;
    let section = read(&server, &at_commit(SHA), "files").await;
    let (trail, entries) = tree(&section);
    assert_eq!(trail, crumbs(&[(SHA, "files")]));
    assert_eq!(entries[0].section.as_deref(), Some("file:retry.rs"));
}

/// What the app may keep (ADR-0032): a commit's tree and diff at a whole
/// sha, and a branch's or repository's tree read at the sha its id names
/// (#2241). A branch's tree by its tip, and a repository's branch list, move
/// (#2242). A short sha is only unique today, a commit's overview carries
/// checks that move, and a whole view a toolbar.
#[tokio::test]
async fn what_names_a_whole_sha_is_fixed_and_what_reads_a_tip_moves() {
    use knobas_source::view::Standing::{Fixed, Live, Moving};
    let leak = |id: String| -> &'static str { Box::leak(id.into_boxed_str()) };
    let server = gitea().await;
    let source = source(&server);
    let sha256 = "a".repeat(64);
    for (entity, section, standing) in [
        (at_commit(SHA), Some("file:src/lib.rs"), Fixed),
        (at_commit(SHA), Some("files:src"), Fixed),
        (at_commit(SHA), Some("files"), Fixed),
        (at_commit(SHA), Some("diff"), Fixed),
        (at_commit(&sha256), Some("files"), Fixed),
        (at_commit(SHA), Some("overview"), Live),
        (at_commit(SHA), None, Live),
        (at_commit(&SHA[..12]), Some("file:src/lib.rs"), Live),
        (at_commit(&"g".repeat(40)), Some("files"), Live),
        (BRANCH.to_owned(), Some("file:src/lib.rs"), Moving),
        (ENTITY.to_owned(), Some("file:src/lib.rs"), Moving),
        (ENTITY.to_owned(), Some("files"), Moving),
        (ENTITY.to_owned(), Some("branches"), Moving),
        (ENTITY.to_owned(), Some("overview"), Live),
        (ENTITY.to_owned(), None, Live),
        (BRANCH.to_owned(), None, Live),
        (
            BRANCH.to_owned(),
            Some(leak(named("main", "file:src/lib.rs"))),
            Fixed,
        ),
        (
            ENTITY.to_owned(),
            Some(leak(named("main", "files:src"))),
            Fixed,
        ),
        (
            ENTITY.to_owned(),
            Some(leak(format!("files@{}", &SHA[..12]))),
            Live,
        ),
        (
            "other:tidewater/payout-service@x".to_owned(),
            Some("files"),
            Live,
        ),
    ] {
        assert_eq!(
            source.view_standing(&entity, section),
            standing,
            "{entity} {section:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// The byte read (#2256): a file whole at the commit a file block names, or
// refused with its size over the cap, never cut.

/// A file block names the commit it was read at, for the byte read to take
/// back: a README's images are read at the README's commit.
#[tokio::test]
async fn a_file_names_the_commit_it_was_read_at() {
    let server = gitea().await;
    let at = "feature/PAY-231-sepa-retry";
    raw(&server, "README.md", at, b"# hi\n".to_vec()).await;
    let drawn = file(read(&server, BRANCH, &named(at, "file:README.md")).await);
    assert_eq!(drawn.commit, Some(tip(at)));
}

/// The bytes at the commit handed back, whole, from one raw read: no
/// repository, branch or entry read beside it.
#[tokio::test]
async fn the_byte_read_answers_a_file_whole_at_the_commit_handed_back() {
    let server = gitea().await;
    let sha = tip("feature/PAY-231-sepa-retry");
    let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
    png.extend(std::iter::repeat_n(7u8, 2_000));
    raw_only(&server, "docs/img/logo.png", &sha, png.clone()).await;
    let read = source(&server)
        .file_bytes(BRANCH, "docs/img/logo.png", Some(&sha), 10_000)
        .await
        .expect("read")
        .expect("Gitea reads bytes");
    assert_eq!(read, FileBytes::Read(png));
    let asked = server.received_requests().await.unwrap();
    assert_eq!(
        asked
            .iter()
            .map(|r| r.url.path().to_owned())
            .collect::<Vec<_>>(),
        vec![format!("{REPO}/raw/docs/img/logo.png")]
    );
}

/// Over the cap the file is refused with its whole size, and nothing of it
/// is answered; at the cap exactly it is read.
#[tokio::test]
async fn a_file_over_the_cap_is_refused_with_its_size_never_cut() {
    let server = gitea().await;
    let sha = tip("main");
    raw_only(&server, "big.pdf", &sha, vec![1u8; 2_001]).await;
    let source = source(&server);
    let over = source
        .file_bytes(ENTITY, "big.pdf", Some(&sha), 2_000)
        .await
        .expect("read")
        .expect("Gitea reads bytes");
    assert_eq!(over, FileBytes::TooLarge { size: 2_001 });
    let at = source
        .file_bytes(ENTITY, "big.pdf", Some(&sha), 2_001)
        .await
        .expect("read")
        .expect("Gitea reads bytes");
    assert_eq!(at, FileBytes::Read(vec![1u8; 2_001]));
}

/// A refused file whose answer had no length (a compressing proxy) takes
/// its size from its own entry.
#[tokio::test]
async fn a_refused_file_without_a_length_asks_gitea_its_size() {
    use std::io::Write as _;
    let server = gitea().await;
    let text = "z".repeat(50_000);
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    gz.write_all(text.as_bytes()).unwrap();
    Mock::given(method("GET"))
        .and(path(format!("{REPO}/raw/big.txt")))
        .and(query_param("ref", tip("main").as_str()))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Content-Encoding", "gzip")
                .set_body_bytes(gz.finish().unwrap()),
        )
        .mount(&server)
        .await;
    own_entry(&server, "big.txt", "main", text.len() as u64).await;
    let refused = source(&server)
        .file_bytes(ENTITY, "big.txt", Some(&tip("main")), 10_000)
        .await
        .expect("read")
        .expect("Gitea reads bytes");
    assert_eq!(refused, FileBytes::TooLarge { size: 50_000 });
}

/// With no commit handed back a branch's file is read at the branch, and a
/// repository's at its default branch.
#[tokio::test]
async fn without_a_commit_the_byte_read_is_at_the_entitys_reference() {
    let server = gitea().await;
    Mock::given(method("GET"))
        .and(path(format!("{REPO}/raw/a.txt")))
        .and(query_param("ref", "feature/PAY-231-sepa-retry"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(b"branch".to_vec()))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("{REPO}/raw/a.txt")))
        .and(query_param("ref", "main"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(b"main".to_vec()))
        .mount(&server)
        .await;
    let source = source(&server);
    let read = |entity: &'static str| {
        let source = &source;
        async move {
            source
                .file_bytes(entity, "a.txt", None, 100)
                .await
                .unwrap()
                .unwrap()
        }
    };
    assert_eq!(read(BRANCH).await, FileBytes::Read(b"branch".to_vec()));
    assert_eq!(read(ENTITY).await, FileBytes::Read(b"main".to_vec()));
}

/// Fixed only at a whole sha: one handed back, or a commit entity's own.
#[test]
fn a_byte_read_is_fixed_only_at_a_whole_sha() {
    let source =
        knobas_source_gitea::build(instance("http://127.0.0.1:9".into(), TOKEN, json!({})))
            .expect("builds");
    let sha = tip("main");
    let commit = format!("{ENTITY}@{sha}");
    assert_eq!(
        source.bytes_standing(BRANCH, "a.png", Some(&sha)),
        Standing::Fixed
    );
    assert_eq!(source.bytes_standing(BRANCH, "a.png", None), Standing::Live);
    assert_eq!(
        source.bytes_standing(ENTITY, "a.png", Some("main")),
        Standing::Live
    );
    assert_eq!(
        source.bytes_standing(&commit, "a.png", None),
        Standing::Fixed
    );
    assert_eq!(
        source.bytes_standing(&commit[..commit.len() - 30], "a.png", None),
        Standing::Live
    );
}

// ---------------------------------------------------------------------------
// The whole tree (#2311): every file at a commit, for *Go to file*, from
// Gitea's recursive tree (`git/trees/{sha}?recursive=true`), paged by a
// thousand and counted by its `total_count`.

/// One entry of a recursive tree as `git/trees` answers it.
fn git_entry(path: &str, kind: &str, mode: &str) -> Value {
    json!({ "path": path, "type": kind, "mode": mode, "size": 3, "sha": "0".repeat(40), "url": "" })
}

/// `git/trees/{sha}?recursive=true&page={page}` answers `entries` of `total`.
async fn git_tree(server: &MockServer, sha: &str, page: u32, entries: Vec<Value>, total: usize) {
    Mock::given(method("GET"))
        .and(path(format!("{REPO}/git/trees/{sha}")))
        .and(query_param("recursive", "true"))
        .and(query_param("page", page.to_string().as_str()))
        .and(query_param("per_page", "1000"))
        .and(header("Authorization", format!("token {TOKEN}").as_str()))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "sha": sha, "url": "", "tree": entries,
            "truncated": total > 1000, "page": page, "total_count": total
        })))
        .mount(server)
        .await;
}

fn every(section: &ViewSection) -> (Vec<(String, EntryKind, Option<String>)>, bool) {
    match section.blocks.as_slice() {
        [Block::Tree { entries, truncated, paths, .. }] => {
            assert_eq!(*paths, None, "the whole tree names no further read");
            (
                entries
                    .iter()
                    .map(|e| (e.name.clone(), e.kind, e.section.clone()))
                    .collect(),
                *truncated,
            )
        }
        other => panic!("one tree block, not {other:#?}"),
    }
}

/// Every listing names the read of the whole tree at its own commit.
#[tokio::test]
async fn a_listing_names_the_whole_tree_at_its_commit() {
    let server = gitea().await;
    listing(&server, "", "main", json!([entry("a.rs", "a.rs", "file", 1)])).await;
    let section = read(&server, ENTITY, "files").await;
    let [Block::Tree { paths, .. }] = section.blocks.as_slice() else {
        panic!("one tree block");
    };
    assert_eq!(paths.as_deref(), Some(format!("paths@{}", tip("main")).as_str()));

    listing(&server, "", SHA, json!([entry("a.rs", "a.rs", "file", 1)])).await;
    let section = read(&server, &at_commit(SHA), "files").await;
    let [Block::Tree { paths, .. }] = section.blocks.as_slice() else {
        panic!("one tree block");
    };
    assert_eq!(paths.as_deref(), Some("paths"), "a commit's ids name no sha");
}

/// The whole tree is its files, each by its whole path and opened by the
/// section the tree would name it by; directories and submodules are not
/// files, a symlink is drawn as one.
#[tokio::test]
async fn the_whole_tree_lists_every_file_by_its_path() {
    let server = gitea().await;
    let sha = tip("main");
    git_tree(
        &server,
        &sha,
        1,
        vec![
            git_entry("README.md", "blob", "100644"),
            git_entry("src", "tree", "040000"),
            git_entry("src/lib.rs", "blob", "100644"),
            git_entry("src/run", "blob", "100755"),
            git_entry("current", "blob", "120000"),
            git_entry("vendor", "commit", "160000"),
        ],
        6,
    )
    .await;
    let section = read(&server, ENTITY, &format!("paths@{sha}")).await;
    let (files, truncated) = every(&section);
    let at = |p: &str| Some(format!("file@{sha}:{p}"));
    assert_eq!(
        files,
        [
            ("README.md".to_owned(), EntryKind::File, at("README.md")),
            ("src/lib.rs".to_owned(), EntryKind::File, at("src/lib.rs")),
            ("src/run".to_owned(), EntryKind::File, at("src/run")),
            ("current".to_owned(), EntryKind::Symlink, at("current")),
        ]
    );
    assert!(!truncated);
    let asked: Vec<_> = server
        .received_requests()
        .await
        .unwrap()
        .into_iter()
        .map(|r| r.url.path().to_owned())
        .collect();
    assert_eq!(asked, [format!("{REPO}/git/trees/{sha}")], "one read, no tip");

    // A commit's own tree, by its plain id.
    git_tree(&server, SHA, 1, vec![git_entry("x.rs", "blob", "100644")], 1).await;
    let section = read(&server, &at_commit(SHA), "paths").await;
    assert_eq!(
        every(&section).0,
        [("x.rs".to_owned(), EntryKind::File, Some("file:x.rs".to_owned()))]
    );
}

/// Gitea pages the recursive tree by a thousand: every page its count names
/// is read.
#[tokio::test]
async fn the_whole_tree_reads_every_page() {
    let server = gitea().await;
    let sha = tip("main");
    let page = |from: usize, to: usize| -> Vec<Value> {
        (from..to)
            .map(|n| git_entry(&format!("f{n:05}"), "blob", "100644"))
            .collect()
    };
    git_tree(&server, &sha, 1, page(0, 1000), 2_500).await;
    git_tree(&server, &sha, 2, page(1000, 2000), 2_500).await;
    git_tree(&server, &sha, 3, page(2000, 2500), 2_500).await;
    let section = read(&server, ENTITY, &format!("paths@{sha}")).await;
    let (files, truncated) = every(&section);
    assert_eq!(files.len(), 2_500);
    assert_eq!(files[1_999].0, "f01999");
    assert_eq!(files[2_499].0, "f02499");
    assert!(!truncated, "every page was read");
}

/// A tree past the most pages knobas reads is cut there and says so; what
/// Go to file does with it is the shell's (it says it searched the first).
#[tokio::test]
async fn a_whole_tree_past_the_page_cap_is_cut_and_says_so() {
    let server = gitea().await;
    let sha = tip("main");
    let thousand: Vec<Value> = (0..1000)
        .map(|n| git_entry(&format!("f{n:04}"), "blob", "100644"))
        .collect();
    Mock::given(method("GET"))
        .and(path(format!("{REPO}/git/trees/{sha}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "sha": sha, "tree": thousand, "truncated": true, "page": 1, "total_count": 90_000
        })))
        .mount(&server)
        .await;
    let section = read(&server, ENTITY, &format!("paths@{sha}")).await;
    let (files, truncated) = every(&section);
    assert!(truncated);
    assert_eq!(files.len(), 20_000, "twenty pages of a thousand");
    let reads = server.received_requests().await.unwrap().len();
    assert_eq!(reads, 20);
}

/// The whole tree at a sha is kept (ADR-0032); by the tip it moves.
#[tokio::test]
async fn the_whole_tree_at_a_sha_is_fixed() {
    use knobas_source::view::Standing::{Fixed, Moving};
    let server = gitea().await;
    let source = source(&server);
    let at = format!("paths@{SHA}");
    assert_eq!(source.view_standing(ENTITY, Some(&at)), Fixed);
    assert_eq!(source.view_standing(BRANCH, Some(&at)), Fixed);
    assert_eq!(source.view_standing(&at_commit(SHA), Some("paths")), Fixed);
    assert_eq!(source.view_standing(ENTITY, Some("paths")), Moving);
}
