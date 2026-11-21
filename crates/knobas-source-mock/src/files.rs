//! The fixture repositories' files (the everything-search decisions,
//! 2026-11-20, D16): `fixtures/tidewater/files/<owner>/<name>/…`, compiled in
//! by `build.rs`, answered as the trees, blobs and archives a forge would
//! answer for the repository's head, so the stored-trees pass and the files
//! pass run against the mock in tests and in `just demo`.
//!
//! **Git's own shas.** A blob's and a tree's sha are the ones `git hash-object`
//! and `git write-tree` give the same bytes, so a checkout of the seeded
//! repository holds the blobs by the shas the mock names. The head's commit
//! is the mock's own (`git` would mix in an author and a date), the same on
//! every run.
//!
//! **Every branch of a repository holds its files**, as every branch the
//! seed cuts from `main` does (`testenv/seed-gitea.sh`): a tree read at any
//! commit of a repository with files answers its one tree.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use knobas_source::tree::{HeldTrees, MODE_TREE, TreeItem, TreeLevel, TreeRead};
use sha1::{Digest as _, Sha1};

/// `(owner, repository, path, bytes)`, every fixture file, by path.
static FILES: &[(&str, &str, &str, &[u8])] = include!(concat!(env!("OUT_DIR"), "/files.rs"));

/// A regular file's mode.
const MODE_FILE: u32 = 0o100_644;

/// One repository's files at its head.
#[derive(Debug)]
pub struct Tree {
    /// The commit the mock's default branch stands at.
    pub commit: String,
    /// The root tree's sha.
    pub root: String,
    /// Every level, the root first.
    pub levels: Vec<TreeLevel>,
    /// Every file's bytes, by its path.
    pub files: BTreeMap<&'static str, &'static [u8]>,
    /// The tree's entries in all, a directory counted as one.
    pub entries: u64,
}

impl Tree {
    /// The levels `held` does not hold, as a read from the forge answers
    /// them: one request for the whole tree.
    #[must_use]
    pub fn read(&self, held: &dyn HeldTrees, cap: u64) -> TreeRead {
        if self.entries > cap {
            return TreeRead::TooLarge {
                entries: self.entries,
                requests: 1,
            };
        }
        TreeRead::Read {
            root: self.root.clone(),
            levels: self
                .levels
                .iter()
                .filter(|level| held.held(&level.sha).is_none())
                .cloned()
                .collect(),
            requests: 1,
        }
    }
}

enum Node {
    File(&'static [u8]),
    Dir(BTreeMap<&'static str, Node>),
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Git's sha of an object of `kind` holding `body`.
fn object_sha(kind: &str, body: &[u8]) -> [u8; 20] {
    let mut hasher = Sha1::new();
    hasher.update(format!("{kind} {}\0", body.len()).as_bytes());
    hasher.update(body);
    hasher.finalize().into()
}

/// Git's order in a tree: by name, a directory's name read with a `/` after it.
fn git_order(a: &(&str, &Node), b: &(&str, &Node)) -> std::cmp::Ordering {
    let key = |(name, node): &(&str, &Node)| {
        let mut key = name.as_bytes().to_vec();
        if matches!(node, Node::Dir(_)) {
            key.push(b'/');
        }
        key
    };
    key(a).cmp(&key(b))
}

/// Writes `dir`'s level and every level below it into `levels`, answering
/// its sha and its entries in all.
fn write_level(dir: &BTreeMap<&'static str, Node>, levels: &mut Vec<TreeLevel>) -> ([u8; 20], u64) {
    let mut children: Vec<(&str, &Node)> = dir.iter().map(|(name, node)| (*name, node)).collect();
    children.sort_by(git_order);
    let mut body = Vec::new();
    let mut entries = Vec::new();
    let mut below = Vec::new();
    let mut total = 0_u64;
    for (name, node) in children {
        let (mode, sha, size) = match node {
            Node::File(bytes) => (MODE_FILE, object_sha("blob", bytes), bytes.len() as u64),
            Node::Dir(inner) => {
                let (sha, inner_total) = write_level(inner, &mut below);
                total += inner_total;
                (MODE_TREE, sha, 0)
            }
        };
        total += 1;
        body.extend_from_slice(format!("{mode:o} {name}\0").as_bytes());
        body.extend_from_slice(&sha);
        entries.push(TreeItem {
            name: name.to_owned(),
            mode,
            size,
            sha: hex(&sha),
        });
    }
    let sha = object_sha("tree", &body);
    levels.push(TreeLevel {
        sha: hex(&sha),
        entries,
    });
    levels.extend(below);
    (sha, total)
}

fn trees() -> &'static BTreeMap<&'static str, Tree> {
    static TREES: OnceLock<BTreeMap<&'static str, Tree>> = OnceLock::new();
    TREES.get_or_init(|| {
        let mut roots: BTreeMap<&'static str, BTreeMap<&'static str, Node>> = BTreeMap::new();
        let mut files: BTreeMap<&'static str, BTreeMap<&'static str, &'static [u8]>> =
            BTreeMap::new();
        for &(_owner, repository, path, bytes) in FILES {
            files.entry(repository).or_default().insert(path, bytes);
            let mut dir = roots.entry(repository).or_default();
            let mut parts = path.split('/').peekable();
            while let Some(part) = parts.next() {
                if parts.peek().is_none() {
                    dir.insert(part, Node::File(bytes));
                } else {
                    let Node::Dir(inner) = dir
                        .entry(part)
                        .or_insert_with(|| Node::Dir(BTreeMap::new()))
                    else {
                        unreachable!("a fixture path names a file as a folder: {path}");
                    };
                    dir = inner;
                }
            }
        }
        roots
            .into_iter()
            .map(|(repository, root)| {
                let mut levels = Vec::new();
                let (sha, entries) = write_level(&root, &mut levels);
                let root = hex(&sha);
                let commit = hex(&object_sha(
                    "commit",
                    format!("tree {root}\n\nThe fixture's files of {repository}\n").as_bytes(),
                ));
                (
                    repository,
                    Tree {
                        commit,
                        root,
                        levels,
                        files: files.remove(repository).unwrap_or_default(),
                        entries,
                    },
                )
            })
            .collect()
    })
}

/// The files of the fixture's repository `name`, where it has any.
#[must_use]
pub fn of(name: &str) -> Option<&'static Tree> {
    trees().get(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_folder_sorts_as_its_name_with_a_slash_as_git_writes_it() {
        // `a/x` and `a.txt`: git puts `a.txt` first, since `.` sorts before
        // the `/` it reads after a folder's name (`git write-tree`).
        let dir = BTreeMap::from([
            ("a", Node::Dir(BTreeMap::from([("x", Node::File(b"x\n"))]))),
            ("a.txt", Node::File(b"t\n")),
        ]);
        let mut levels = Vec::new();
        let (sha, entries) = write_level(&dir, &mut levels);
        assert_eq!(hex(&sha), "83b112ebc915000c439da77b96edf5b56261cf19");
        assert_eq!(entries, 3);
        let names: Vec<&str> = levels[0].entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["a.txt", "a"]);
    }
}
