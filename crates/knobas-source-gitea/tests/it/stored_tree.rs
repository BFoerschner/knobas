//! Stored trees (the stored-trees pass of PR #2479, 2026-11-14): what the
//! adapter reads of a commit's tree for the store, and that a section drawn
//! from the stored levels is the section Gitea's own read draws.
//!
//! The fixture is one commit's tree in git's order, as Gitea answers it both
//! ways: recursive (`git/trees/{commit}?recursive=true`, every entry by its
//! path) and level by level (`git/trees/{sha}`, the root by the commit's
//! sha). Folders, a symlink, an executable, a submodule, a name outside
//! ASCII, a folder three deep.

use std::collections::HashMap;

use knobas_source::Source;
use knobas_source::tree::{
    HeldTrees, MODE_TREE, StoredTree, TreeAsk, TreeItem, TreePlace, TreeRead,
};
use serde_json::{Value, json};
use wiremock::matchers::{method, path, query_param, query_param_is_missing};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::support::{TOKEN, instance};

const REPO: &str = "/api/v1/repos/tidewater/payout-service";
const ENTITY: &str = "gitea:tidewater/payout-service";
const MAIN: &str = "gitea:tidewater/payout-service@refs/heads/main";
const WEB: &str = "https://gitea.example/tidewater/payout-service";
const COMMIT: &str = "c0ffee00c0ffee00c0ffee00c0ffee00c0ffee00";

/// A 40-digit object id, `n` its tail.
fn id(n: u32) -> String {
    format!("{n:040x}")
}

/// One entry: (name, type, mode, size, sha).
type Entry = (&'static str, &'static str, &'static str, u64, String);

/// Each folder's entries, in git's order, by its path; `""` the root.
fn fixture() -> Vec<(&'static str, String, Vec<Entry>)> {
    vec![
        (
            "",
            COMMIT.to_owned(),
            vec![
                (".github", "tree", "040000", 0, id(0xd1)),
                ("README.md", "blob", "100644", 120, id(0xf1)),
                ("docs", "tree", "040000", 0, id(0xd3)),
                ("link", "blob", "120000", 9, id(0xf2)),
                ("run.sh", "blob", "100755", 33, id(0xf3)),
                ("src", "tree", "040000", 0, id(0xd5)),
            ],
        ),
        (
            ".github",
            id(0xd1),
            vec![("workflows", "tree", "040000", 0, id(0xd2))],
        ),
        (
            ".github/workflows",
            id(0xd2),
            vec![("ci.yml", "blob", "100644", 410, id(0xf4))],
        ),
        (
            "docs",
            id(0xd3),
            vec![
                ("guide", "tree", "040000", 0, id(0xd4)),
                ("ünïcode name.md", "blob", "100644", 7, id(0xf5)),
            ],
        ),
        (
            "docs/guide",
            id(0xd4),
            vec![("a.md", "blob", "100644", 1, id(0xf6))],
        ),
        (
            "src",
            id(0xd5),
            vec![
                ("lib.rs", "blob", "100644", 2048, id(0xf7)),
                ("main.rs", "blob", "100644", 64, id(0xf8)),
                ("vendor", "commit", "160000", 0, id(0xf9)),
            ],
        ),
    ]
}

fn git_object(path: &str, (_, kind, mode, size, sha): &Entry) -> Value {
    json!({ "path": path, "type": kind, "mode": mode, "size": size, "sha": sha, "url": "" })
}

/// Every entry by its whole path, in git's recursive order.
fn recursive() -> Vec<Value> {
    let dirs = fixture();
    let by_path: HashMap<&str, &Vec<Entry>> = dirs.iter().map(|(p, _, e)| (*p, e)).collect();
    fn walk(dir: &str, by_path: &HashMap<&str, &Vec<Entry>>, out: &mut Vec<Value>) {
        for entry in by_path[dir] {
            let full = if dir.is_empty() {
                entry.0.to_owned()
            } else {
                format!("{dir}/{}", entry.0)
            };
            out.push(git_object(&full, entry));
            if entry.1 == "tree" {
                walk(&full, by_path, out);
            }
        }
    }
    let mut out = Vec::new();
    walk("", &by_path, &mut out);
    out
}

/// Gitea answering the fixture both ways, its repository record and `main`'s
/// tip at the commit. `total` is the recursive tree's `total_count`.
async fn gitea_with(total: Option<usize>) -> MockServer {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path(REPO))
        .respond_with(ResponseTemplate::new(200).set_body_json(record()))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("{REPO}/branches/main")))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            json!({ "name": "main", "protected": false, "commit": { "id": COMMIT } }),
        ))
        .mount(&server)
        .await;
    let every = recursive();
    Mock::given(method("GET"))
        .and(path(format!("{REPO}/git/trees/{COMMIT}")))
        .and(query_param("recursive", "true"))
        .and(query_param("page", "1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            // Gitea names the commit asked (its `ResolvedID`), not the tree.
            "sha": COMMIT, "url": "", "tree": every, "truncated": false, "page": 1,
            "total_count": total.unwrap_or(every.len())
        })))
        .mount(&server)
        .await;
    for (_, sha, entries) in fixture() {
        let tree: Vec<Value> = entries.iter().map(|e| git_object(e.0, e)).collect();
        Mock::given(method("GET"))
            .and(path(format!("{REPO}/git/trees/{sha}")))
            .and(query_param_is_missing("recursive"))
            .and(query_param("page", "1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "sha": sha, "url": "", "tree": tree, "truncated": false, "page": 1,
                "total_count": tree.len()
            })))
            .mount(&server)
            .await;
    }
    server
}

async fn gitea() -> MockServer {
    gitea_with(None).await
}

fn record() -> Value {
    json!({
        "full_name": "tidewater/payout-service", "default_branch": "main",
        "html_url": WEB, "private": true, "internal": false, "empty": false,
        "mirror": false, "has_pull_requests": true,
        "stars_count": 0, "forks_count": 0, "size": 12
    })
}

fn source(server: &MockServer) -> Box<dyn Source> {
    knobas_source_gitea::build(instance(server.uri(), TOKEN, json!({}))).expect("builds")
}

/// What the store holds, by sha with each tree's entries in all, and the
/// repository's last tree's entries.
struct Held(HashMap<String, u64>, Option<u64>);

impl HeldTrees for Held {
    fn held(&self, sha: &str) -> Option<u64> {
        self.0.get(sha).copied()
    }
    fn estimate(&self) -> Option<u64> {
        self.1
    }
}

/// The tree requests the fake answered: (sha, recursive).
async fn tree_reads(server: &MockServer) -> Vec<(String, bool)> {
    server
        .received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .filter_map(|r| {
            let sha = r.url.path().split("/git/trees/").nth(1)?.to_owned();
            Some((sha, r.url.query().unwrap_or("").contains("recursive=true")))
        })
        .collect()
}

/// git's name for the root (`git rev-parse HEAD^{tree}`), as the adapter
/// answers it for the fixture with nothing held.
async fn root_key() -> String {
    let server = gitea().await;
    match source(&server)
        .read_tree(ENTITY, COMMIT, &Held(HashMap::new(), None), 100_000)
        .await
    {
        Ok(Some(TreeRead::Read { root, .. })) => root,
        other => panic!("{other:?}"),
    }
}

/// A repository the store holds nothing of is read whole, once: every
/// level, each named by the sha its folder's entry gives and the root by
/// git's hash of its entries, each in git's order.
#[tokio::test]
async fn a_repository_held_nowhere_is_read_whole_once() {
    let server = gitea().await;
    let read = source(&server)
        .read_tree(ENTITY, COMMIT, &Held(HashMap::new(), None), 100_000)
        .await
        .unwrap()
        .expect("Gitea stores trees");
    let TreeRead::Read {
        root,
        levels,
        requests,
    } = read
    else {
        panic!("read, not cut: {read:?}");
    };
    assert_eq!(requests, 1);
    assert_eq!(tree_reads(&server).await, [(COMMIT.to_owned(), true)]);
    assert_ne!(root, COMMIT, "the root is the tree's, not the commit's");
    assert_eq!(root.len(), 40);
    let by_sha: HashMap<String, Vec<String>> = levels
        .iter()
        .map(|l| {
            (
                l.sha.clone(),
                l.entries.iter().map(|e| e.name.clone()).collect(),
            )
        })
        .collect();
    assert_eq!(by_sha.len(), 6, "every folder once");
    for (dir, sha, entries) in fixture() {
        let key = if dir.is_empty() { root.clone() } else { sha };
        let names: Vec<String> = entries.iter().map(|e| e.0.to_owned()).collect();
        assert_eq!(by_sha[&key], names, "{dir:?} in git's order");
    }
    let top = &levels.iter().find(|l| l.sha == root).unwrap().entries;
    assert_eq!(top[3].mode, 0o120_000, "a symlink keeps its mode");
    assert_eq!(top[4].mode, 0o100_755);
    assert_eq!(top[1].size, 120);
    let src = &levels.iter().find(|l| l.sha == id(0xd5)).unwrap().entries;
    assert_eq!(src[2].mode, 0o160_000, "a submodule by its type");
}

/// With levels held, a branch is walked from its root and only the levels
/// not held are read, one level each; a submodule is no level.
#[tokio::test]
async fn a_walk_reads_only_the_levels_not_held() {
    let server = gitea().await;
    let held = Held(HashMap::from([(id(0xd1), 2), (id(0xd3), 4)]), Some(10_000));
    let read = source(&server)
        .read_tree(ENTITY, COMMIT, &held, 100_000)
        .await
        .unwrap()
        .unwrap();
    let TreeRead::Read {
        root,
        levels,
        requests,
    } = read
    else {
        panic!("{read:?}")
    };
    assert_eq!(root, root_key().await, "the same root key either way");
    assert_eq!(requests, 2);
    assert_eq!(
        tree_reads(&server).await,
        [(COMMIT.to_owned(), false), (id(0xd5), false)]
    );
    let shas: Vec<&str> = levels.iter().map(|l| l.sha.as_str()).collect();
    assert_eq!(shas, [root.as_str(), id(0xd5).as_str()]);
}

/// A root the store holds already (a push that changed nothing below, a
/// branch made from another) costs its one request.
#[tokio::test]
async fn a_held_root_costs_one_request() {
    let root = root_key().await;
    let server = gitea().await;
    let read = source(&server)
        .read_tree(
            ENTITY,
            COMMIT,
            &Held(HashMap::from([(root.clone(), 14)]), Some(14)),
            100_000,
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        read,
        TreeRead::Read {
            root,
            levels: Vec::new(),
            requests: 1
        }
    );
}

/// A walk that would read more levels than the whole tree has pages reads
/// the whole tree instead.
#[tokio::test]
async fn a_walk_dearer_than_the_whole_tree_reads_the_whole_tree() {
    let server = gitea().await;
    let read = source(&server)
        .read_tree(
            ENTITY,
            COMMIT,
            &Held(HashMap::from([(id(0xff), 1)]), Some(1)),
            100_000,
        )
        .await
        .unwrap()
        .unwrap();
    let TreeRead::Read {
        levels, requests, ..
    } = read
    else {
        panic!("{read:?}")
    };
    assert_eq!(levels.len(), 6);
    assert_eq!(
        tree_reads(&server).await,
        [(COMMIT.to_owned(), false), (COMMIT.to_owned(), true)]
    );
    assert_eq!(requests, 2);
}

/// A tree past the cap is not read past its first page, and says how large.
#[tokio::test]
async fn a_tree_past_the_cap_is_not_read() {
    let server = gitea_with(Some(150_000)).await;
    let read = source(&server)
        .read_tree(ENTITY, COMMIT, &Held(HashMap::new(), None), 100_000)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        read,
        TreeRead::TooLarge {
            entries: 150_000,
            requests: 1
        }
    );
}

/// The stored levels the fixture reads to.
async fn stored() -> StoredTree {
    let server = gitea().await;
    let TreeRead::Read { root, levels, .. } = source(&server)
        .read_tree(ENTITY, COMMIT, &Held(HashMap::new(), None), 100_000)
        .await
        .unwrap()
        .unwrap()
    else {
        panic!("read")
    };
    StoredTree {
        commit: COMMIT.to_owned(),
        root,
        levels: levels.into_iter().map(|l| (l.sha, l.entries)).collect(),
        repository_url: Some(WEB.to_owned()),
    }
}

/// What a section of a branch's or repository's tree asks of the store: the
/// branch (a repository's default, by its record), the commit its id names,
/// and the place; a file and a compare ask nothing.
#[tokio::test]
async fn a_tree_section_names_its_branch_commit_and_place() {
    let server = gitea().await;
    let source = source(&server);
    assert_eq!(
        source.tree_ask(ENTITY, Some("files"), &record()),
        Some(TreeAsk {
            branch: MAIN.to_owned(),
            commit: None,
            place: TreePlace::Dir(String::new()),
        })
    );
    assert_eq!(
        source.tree_ask(
            MAIN,
            Some(&format!("files@{COMMIT}:docs/guide")),
            &json!({})
        ),
        Some(TreeAsk {
            branch: MAIN.to_owned(),
            commit: Some(COMMIT.to_owned()),
            place: TreePlace::Dir("docs/guide".to_owned()),
        })
    );
    assert_eq!(
        source
            .tree_ask(ENTITY, Some(&format!("paths@{COMMIT}")), &record())
            .map(|ask| ask.place),
        Some(TreePlace::Every)
    );
    assert_eq!(
        source.tree_ask(ENTITY, Some(&format!("file@{COMMIT}:README.md")), &record()),
        None
    );
    assert_eq!(source.tree_ask(ENTITY, Some("overview"), &record()), None);
    let commit = format!("{ENTITY}@{COMMIT}");
    assert_eq!(source.tree_ask(&commit, Some("files"), &json!({})), None);
    assert!(tree_reads(&server).await.is_empty(), "pure");
}

/// **Same answer**: every directory and *Go to file* drawn from the stored
/// levels is, byte for byte on the wire, what Gitea's own read of the same
/// section at the same commit draws: entries, kinds, sizes, ids, trail, page.
#[tokio::test]
async fn a_section_drawn_from_the_store_is_the_section_gitea_draws() {
    let tree = stored().await;
    let sections = [
        (ENTITY, "files".to_owned()),
        (ENTITY, format!("files@{COMMIT}:docs")),
        (ENTITY, format!("files@{COMMIT}:docs/guide")),
        (ENTITY, format!("files@{COMMIT}:.github/workflows")),
        (ENTITY, format!("files@{COMMIT}:src")),
        (ENTITY, format!("paths@{COMMIT}")),
        (MAIN, "files".to_owned()),
        (MAIN, format!("files@{COMMIT}:docs/guide")),
        (MAIN, format!("paths@{COMMIT}")),
    ];
    for (entity, section) in sections {
        // A fresh fake each, so no read is kept from the one before.
        let server = gitea().await;
        let source = source(&server);
        let read = source.view(entity, Some(&section)).await.unwrap().unwrap();
        let payload = if entity == ENTITY {
            record()
        } else {
            json!({})
        };
        let drawn = source
            .tree_view(entity, Some(&section), &payload, &tree)
            .unwrap_or_else(|| panic!("{entity} {section} drawn from the store"));
        assert_eq!(
            serde_json::to_value(&drawn).unwrap(),
            serde_json::to_value(&read).unwrap(),
            "{entity} {section}"
        );
    }
}

/// A section naming another commit, or a place the levels do not hold, is
/// not drawn from them.
#[tokio::test]
async fn the_store_draws_only_its_own_commit_and_places() {
    let tree = stored().await;
    let server = gitea().await;
    let source = source(&server);
    let other = "1".repeat(40);
    assert!(
        source
            .tree_view(
                ENTITY,
                Some(&format!("files@{other}:docs")),
                &record(),
                &tree
            )
            .is_none()
    );
    assert!(
        source
            .tree_view(
                ENTITY,
                Some(&format!("files@{COMMIT}:nope")),
                &record(),
                &tree
            )
            .is_none()
    );
    assert!(
        source
            .tree_view(
                ENTITY,
                Some(&format!("files@{COMMIT}:README.md")),
                &record(),
                &tree
            )
            .is_none()
    );
    let mut cut = tree.clone();
    cut.levels.remove(&id(0xd4));
    assert!(
        source
            .tree_view(ENTITY, Some(&format!("paths@{COMMIT}")), &record(), &cut)
            .is_none()
    );
    assert!(
        cut.levels
            .values()
            .flatten()
            .any(|i: &TreeItem| i.mode == MODE_TREE)
    );
}
