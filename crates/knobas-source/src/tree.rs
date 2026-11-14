//! Stored trees (the stored-trees pass of PR #2479, 2026-11-14; ADR-0041 and
//! ADR-0042 as amended that day): the file list of every branch of every
//! repository, kept in the mirror's `sync` schema so *Files* and *Go to
//! file* draw a branch's tree without asking the forge first.
//!
//! **Kept as git keeps it.** Each directory is one level, stored once by its
//! tree sha; a branch points at its root tree. Two branches that share a
//! directory share its level, so what is stored grows with what differs
//! between branches, not with how many there are.
//!
//! The adapter does three things, all optional and `None` by default:
//!
//! * says where its mirror records keep what the sync's tree pass plans from
//!   ([`TreeShape`], [`Source::tree_shape`](crate::Source::tree_shape));
//! * reads the levels of a commit's tree that the store does not hold
//!   ([`Source::read_tree`](crate::Source::read_tree));
//! * names the stored levels a section of its view draws, and draws it from
//!   them as its own read would ([`Source::tree_ask`](crate::Source::tree_ask),
//!   [`Source::tree_view`](crate::Source::tree_view)).
//!
//! Plain data, as every SPI type: nothing here holds a connection.

use std::collections::HashMap;

/// The most entries one branch's whole tree may hold to be stored, every
/// directory counted as an entry as Gitea's `total_count` counts it. Past
/// it nothing is stored for that tree, its reads stay the forge's, and the
/// tree says so.
pub const TREE_CAP: u64 = 100_000;

/// Where the mirror's records keep what the tree pass plans from, as paths
/// into each kind's stored payload (`payload #>> path`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeShape {
    /// The repository kind (`repo`), and where its record names its default
    /// branch.
    pub repository: &'static str,
    pub default_branch: &'static [&'static str],
    /// The branch kind (`branch`), and where its record names the branch and
    /// the commit at its head.
    pub branch: &'static str,
    pub branch_name: &'static [&'static str],
    pub branch_commit: &'static [&'static str],
    /// The pull request kind, where the adapter has one, and where its record
    /// names the branch it is opened from and its state, and the state that
    /// is open: what puts a branch ahead in the first fill.
    pub pull: Option<PullShape>,
}

/// Where a pull request's record names its head branch and its state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PullShape {
    pub kind: &'static str,
    pub head: &'static [&'static str],
    pub state: &'static [&'static str],
    pub open: &'static str,
}

/// One entry of a tree level as git holds it: its name in its directory,
/// its mode (`0o040000` a directory, `0o160000` a submodule, `0o120000` a
/// symlink, any other a file), its size in bytes (a file's; `0` otherwise),
/// and its object's sha.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TreeItem {
    pub name: String,
    pub mode: u32,
    pub size: u64,
    pub sha: String,
}

/// A directory's mode.
pub const MODE_TREE: u32 = 0o040_000;
/// A submodule's mode.
pub const MODE_COMMIT: u32 = 0o160_000;
/// A symlink's mode.
pub const MODE_SYMLINK: u32 = 0o120_000;

impl TreeItem {
    /// Whether this entry is a directory, whose level is stored by its sha.
    #[must_use]
    pub fn is_tree(&self) -> bool {
        self.mode == MODE_TREE
    }
}

/// One level of a tree: its sha and its entries, in git's order.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TreeLevel {
    pub sha: String,
    pub entries: Vec<TreeItem>,
}

/// What [`Source::read_tree`](crate::Source::read_tree) answers for a commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TreeRead {
    /// The commit's root tree, and every level under it the store did not
    /// hold: with the held ones, the whole tree.
    Read {
        root: String,
        levels: Vec<TreeLevel>,
        /// The requests the read sent.
        requests: u32,
    },
    /// The tree holds more than the cap allowed: `entries` at least.
    TooLarge { entries: u64, requests: u32 },
}

/// What the store holds that a read can build on: whether it holds the
/// level of a tree sha, and how many entries that tree holds in all (itself
/// and every level below). Handed to [`Source::read_tree`](crate::Source::read_tree)
/// by the tree pass.
pub trait HeldTrees: Send + Sync {
    /// The entries of the tree `sha` and every level below it, where the
    /// store holds it.
    fn held(&self, sha: &str) -> Option<u64>;
    /// Whether the store holds any level of this repository yet, and how
    /// many entries a tree of it held last (the estimate a read weighs a
    /// level-by-level walk against reading the whole tree by).
    fn estimate(&self) -> Option<u64>;
}

/// Where in a tree a section stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TreePlace {
    /// A directory by its path from the top, `""` the top.
    Dir(String),
    /// Every file of the tree (*Go to file*).
    Every,
}

/// What a section of a view draws from the store, as the adapter names it
/// ([`Source::tree_ask`](crate::Source::tree_ask)): the branch whose stored
/// tree it reads, the commit the section names where it names one (else the
/// branch's head as the mirror holds it), and where in the tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeAsk {
    /// The branch's entity id.
    pub branch: String,
    pub commit: Option<String>,
    pub place: TreePlace,
}

/// The stored levels a section draws: the commit and its root tree, every
/// level the section's place needs (a directory's way down from the top, or
/// all of them for [`TreePlace::Every`]) by sha, and the repository's page
/// as the mirror holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredTree {
    pub commit: String,
    pub root: String,
    pub levels: HashMap<String, Vec<TreeItem>>,
    pub repository_url: Option<String>,
}

impl StoredTree {
    /// The entries of the directory at `dir` (`""` the top), walked down
    /// from the root by the levels held; `None` where a step is not a
    /// directory of the tree or its level is not here.
    #[must_use]
    pub fn dir(&self, dir: &str) -> Option<&[TreeItem]> {
        let mut level = self.levels.get(&self.root)?;
        for part in dir.split('/').filter(|part| !part.is_empty()) {
            let next = level
                .iter()
                .find(|item| item.name == part && item.is_tree())?;
            level = self.levels.get(&next.sha)?;
        }
        Some(level)
    }

    /// Every entry below the top by its whole path, in git's recursive
    /// order (`git ls-tree -r -t`: a directory before what it holds, each
    /// level in its own order), as Gitea's recursive tree answers it;
    /// `None` where a level under the root is not here.
    #[must_use]
    pub fn walk(&self) -> Option<Vec<(String, &TreeItem)>> {
        let mut out = Vec::new();
        let mut stack: Vec<(String, &[TreeItem], usize)> =
            vec![(String::new(), self.levels.get(&self.root)?.as_slice(), 0)];
        while let Some((dir, level, at)) = stack.pop() {
            let Some(item) = level.get(at) else {
                continue;
            };
            stack.push((dir.clone(), level, at + 1));
            let path = if dir.is_empty() {
                item.name.clone()
            } else {
                format!("{dir}/{}", item.name)
            };
            if item.is_tree() {
                let below = self.levels.get(&item.sha)?;
                out.push((path.clone(), item));
                stack.push((path, below.as_slice(), 0));
            } else {
                out.push((path, item));
            }
        }
        Some(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(name: &str, mode: u32, sha: &str) -> TreeItem {
        TreeItem {
            name: name.to_owned(),
            mode,
            size: 1,
            sha: sha.to_owned(),
        }
    }

    fn tree() -> StoredTree {
        StoredTree {
            commit: "c".into(),
            root: "r".into(),
            levels: HashMap::from([
                (
                    "r".to_owned(),
                    vec![
                        item("a", MODE_TREE, "ta"),
                        item("b.txt", 0o100_644, "b1"),
                        item("z", MODE_TREE, "tz"),
                    ],
                ),
                (
                    "ta".to_owned(),
                    vec![item("deep", MODE_TREE, "td"), item("x", 0o100_644, "x1")],
                ),
                ("td".to_owned(), vec![item("y", MODE_SYMLINK, "y1")]),
                ("tz".to_owned(), vec![item("ta-again", MODE_TREE, "ta")]),
            ]),
            repository_url: None,
        }
    }

    #[test]
    fn a_walk_lists_a_directory_before_what_it_holds_in_each_levels_order() {
        let tree = tree();
        let paths: Vec<String> = tree.walk().unwrap().into_iter().map(|(p, _)| p).collect();
        assert_eq!(
            paths,
            [
                "a",
                "a/deep",
                "a/deep/y",
                "a/x",
                "b.txt",
                "z",
                "z/ta-again",
                "z/ta-again/deep",
                "z/ta-again/deep/y",
                "z/ta-again/x"
            ]
        );
    }

    #[test]
    fn a_directory_is_found_by_its_way_down_and_a_missing_step_is_none() {
        let tree = tree();
        assert_eq!(tree.dir("").unwrap().len(), 3);
        assert_eq!(tree.dir("z/ta-again/deep").unwrap()[0].name, "y");
        assert!(tree.dir("b.txt").is_none(), "a file is no directory");
        assert!(tree.dir("nope").is_none());
        let mut cut = tree.clone();
        cut.levels.remove("td");
        assert!(cut.dir("a/deep").is_none());
        assert!(cut.walk().is_none(), "a level missing is no whole walk");
    }
}
