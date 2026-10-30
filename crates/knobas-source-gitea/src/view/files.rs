//! The *Files* tab of a repository's and a branch's view (#711, spec #701
//! workflow 11): the tree at a ref, one directory or one file at a time.
//!
//! The tab is deferred and its own read is the top directory. Every directory
//! and file below is a section of its own, named by an id that carries its
//! path, which the entry that opens it names and the shell reads in the same
//! tab ([`knobas_source::view::Block::Tree`]):
//!
//! * `files` -- the top directory;
//! * `files:{dir}` -- a directory below it (`files:src/lib`);
//! * `file:{path}` -- a file (`file:src/lib.rs`), read raw.
//!
//! A commit's view answers the same sections at its sha (#2239), with no
//! *Commit change…*: nothing is committed onto a commit. What a section reads
//! there cannot change, so the app keeps it for the session (ADR-0032).
//!
//! A directory keeps its first [`ENTRY_CAP`] entries in order, and a file
//! [`BYTE_CAP`] bytes and [`LINE_CAP`] lines, cut at a line; either says it
//! was cut and where the rest is (ADR-0020 rule 6). A file with a NUL byte in
//! its first [`SNIFF`] bytes is binary (git's own test) and is drawn as its
//! size and a link. Beside its bytes, a file's own `contents-ext` entry is read
//! for its blob sha at the ref (#883), the base an edit of it commits against,
//! and its branch for whether a commit onto it is taken (#985).

use knobas_source::SourceError;
use knobas_source::view::{Block, Crumb, EntryKind, TreeEntry, ViewAction, ViewSection};

use super::rows::RepoRecord;
use crate::map;
use crate::source::GiteaSource;

/// The tab's id, and its top directory's.
pub(super) const FILES: &str = "files";
/// A directory below the top: `files:{dir}`.
const DIR_PREFIX: &str = "files:";
/// A file: `file:{path}`.
const FILE_PREFIX: &str = "file:";

/// The most entries of one directory a view carries.
const ENTRY_CAP: usize = 1_000;
/// The most of a file a view carries, in bytes.
const BYTE_CAP: usize = 1_000_000;
/// The most of a file a view carries, in lines.
const LINE_CAP: usize = 10_000;
/// How far into a file a NUL byte makes it binary, as git looks.
const SNIFF: usize = 8_000;

/// A place in the tree a section id names: a directory (`""` the top) or a
/// file. The one place an id is taken apart; [`Place::id`] builds them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Place<'a> {
    Dir(&'a str),
    File(&'a str),
}

impl<'a> Place<'a> {
    /// The place `id` names, or `None` for an id that is not this tab's.
    fn parse(id: &'a str) -> Option<Self> {
        if id == FILES {
            return Some(Self::Dir(""));
        }
        if let Some(dir) = id.strip_prefix(DIR_PREFIX) {
            return Some(Self::Dir(dir));
        }
        id.strip_prefix(FILE_PREFIX).map(Self::File)
    }

    /// The section id that opens this place.
    fn id(self) -> String {
        match self {
            Self::Dir("") => FILES.to_owned(),
            Self::Dir(dir) => format!("{DIR_PREFIX}{dir}"),
            Self::File(path) => format!("{FILE_PREFIX}{path}"),
        }
    }
}

/// Whether `section` is one of this tab's: the tab itself, a directory or a
/// file.
pub(super) fn owns(section: &str) -> bool {
    Place::parse(section).is_some()
}

/// Where a section is read: a branch's tip, or a commit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum At<'a> {
    Branch(&'a str),
    Commit(&'a str),
}

impl<'a> At<'a> {
    /// The ref Gitea is asked at, which the trail's first step is named by.
    fn name(self) -> &'a str {
        match self {
            Self::Branch(name) | Self::Commit(name) => name,
        }
    }

    /// The part of a Gitea page's path that says where: `branch/{name}` or
    /// `commit/{sha}`.
    fn page(self) -> String {
        match self {
            Self::Branch(name) => format!("branch/{}", map::url_path(name)),
            Self::Commit(sha) => format!("commit/{}", map::url_path(sha)),
        }
    }
}

/// The deferred stub the whole view carries.
pub(super) fn tab() -> ViewSection {
    ViewSection {
        id: FILES.to_owned(),
        title: "Files".to_owned(),
        count: None,
        deferred: true,
        blocks: Vec::new(),
    }
}

/// The section `id` names, read at `at`: a directory's listing or a file.
pub(super) async fn read(
    source: &GiteaSource,
    record: &RepoRecord,
    owner: &str,
    repo: &str,
    at: At<'_>,
    id: &str,
) -> Result<ViewSection, SourceError> {
    let blocks = match Place::parse(id) {
        Some(Place::File(path)) => {
            let mut blocks = vec![file(source, record, owner, repo, at, path).await?];
            if let At::Branch(branch) = at {
                blocks.extend(commit(source, owner, repo, branch).await);
            }
            blocks
        }
        // `files:` with nothing after it is not the top's spelling.
        Some(Place::Dir(dir)) if id == FILES || !dir.is_empty() => {
            vec![directory(source, record, owner, repo, at, dir).await?]
        }
        _ => {
            return Err(SourceError::protocol(format!(
                "gitea: {id:?} names no place in the tree"
            )));
        }
    };
    Ok(ViewSection {
        id: id.to_owned(),
        title: "Files".to_owned(),
        count: None,
        // Its own read is the read the deferral waited for (rule 3).
        deferred: false,
        blocks,
    })
}

/// A file's *Commit change…* (#985, R-8.6): whether a commit straight onto
/// `branch` is taken, read from `GET branches/{branch}` by #822's own rule,
/// and disabled with the sentence the flush would refuse it with
/// ([`crate::write::PushGuard::refusal`]). A commit to a new branch is always
/// taken, so the action speaks for the branch only. A branch Gitea does not
/// answer for declares nothing: the flush is still the backstop, and the
/// reader is not told a reason knobas did not read.
///
/// The shell's dialog builds the commit itself; the old shell has no payload
/// for `commit_file` and so draws no button for it.
async fn commit(source: &GiteaSource, owner: &str, repo: &str, branch: &str) -> Option<Block> {
    let guard: crate::write::PushGuard = source
        .client
        .branch_as(owner, repo, branch)
        .await
        .inspect_err(|error| {
            tracing::debug!(%error, branch, "gitea: no protection read for a file's commit");
        })
        .ok()?;
    Some(Block::Actions {
        actions: vec![ViewAction {
            disabled: guard.refusal(branch),
            ..ViewAction::new(crate::WRITE_OP_COMMIT_FILE, "Commit change…")
        }],
    })
}

/// One entry of a listing, or a file's own entry, as `contents` answers it.
#[derive(Debug, serde::Deserialize)]
struct Listed {
    name: String,
    #[serde(rename = "type", default)]
    kind: String,
    #[serde(default)]
    size: u64,
    /// The blob sha, for a file: what an edit of it is made against.
    #[serde(default)]
    sha: Option<String>,
}

async fn directory(
    source: &GiteaSource,
    record: &RepoRecord,
    owner: &str,
    repo: &str,
    at: At<'_>,
    dir: &str,
) -> Result<Block, SourceError> {
    let listing: Vec<Listed> = source
        .client
        .contents_at_as(owner, repo, dir, at.name())
        .await?;
    let mut entries: Vec<TreeEntry> = listing
        .into_iter()
        .map(|listed| {
            let path = if dir.is_empty() {
                listed.name.clone()
            } else {
                format!("{dir}/{}", listed.name)
            };
            let kind = match listed.kind.as_str() {
                "dir" => EntryKind::Dir,
                "symlink" => EntryKind::Symlink,
                "submodule" => EntryKind::Submodule,
                _ => EntryKind::File,
            };
            let (size, section) = match kind {
                EntryKind::Dir => (None, Some(Place::Dir(&path).id())),
                EntryKind::Submodule => (None, None),
                EntryKind::File | EntryKind::Symlink => {
                    (Some(listed.size), Some(Place::File(&path).id()))
                }
            };
            TreeEntry {
                name: listed.name,
                kind,
                size,
                section,
            }
        })
        .collect();
    entries.sort_by(|a, b| {
        (b.kind == EntryKind::Dir)
            .cmp(&(a.kind == EntryKind::Dir))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            .then_with(|| a.name.cmp(&b.name))
    });
    let truncated = entries.len() > ENTRY_CAP;
    entries.truncate(ENTRY_CAP);
    Ok(Block::Tree {
        trail: trail(at, dir, false),
        entries,
        truncated,
        url: page(record, at, dir),
    })
}

async fn file(
    source: &GiteaSource,
    record: &RepoRecord,
    owner: &str,
    repo: &str,
    at: At<'_>,
    path: &str,
) -> Result<Block, SourceError> {
    // The file's own entry first: its blob sha at `at` (#883), and its size
    // where a cut read cannot say. Then its bytes. Read in this order, a push
    // between the two leaves an older blob beside newer text, and an edit
    // made against it is held as moved (#822). The other order, or both at
    // once, could draw a newer blob beside older text, and an edit of that
    // text would land over the push unseen.
    let own: Listed = source
        .client
        .file_entry_as(owner, repo, path, at.name())
        .await?;
    let raw = source
        .client
        .raw_capped(owner, repo, path, at.name(), BYTE_CAP)
        .await?;
    let binary = raw.bytes.iter().take(SNIFF).any(|b| *b == 0);
    let (text, truncated) = if binary {
        (String::new(), false)
    } else {
        capped(&raw.bytes, raw.cut)
    };
    // Cut, and the answer had no length (a compressing proxy): the file's
    // own entry says, rather than the megabyte that was read.
    let size = raw.size.unwrap_or(own.size);
    let url = page(record, at, path);
    Ok(Block::File {
        trail: trail(at, path, true),
        path: path.to_owned(),
        text,
        language: language_of(path).map(str::to_owned),
        size,
        truncated,
        binary,
        url,
        sha: own.sha,
    })
}

/// The Gitea page of `path` (a directory or a file; `""` the top) at `at`.
fn page(record: &RepoRecord, at: At<'_>, path: &str) -> Option<String> {
    let mut tail = format!("/src/{}", at.page());
    if !path.is_empty() {
        tail.push('/');
        tail.push_str(&map::url_path(path));
    }
    record.web(&tail)
}

/// A file's text within both caps, and whether either cut it. `cut` says the
/// read already stopped past [`BYTE_CAP`].
fn capped(bytes: &[u8], cut: bool) -> (String, bool) {
    let mut kept = bytes;
    let mut truncated = cut || bytes.len() > BYTE_CAP;
    if truncated {
        let head = &bytes[..BYTE_CAP.min(bytes.len())];
        // At the last whole line; a cap inside the first line keeps it cut.
        kept = head
            .iter()
            .rposition(|b| *b == b'\n')
            .map_or(head, |at| &head[..=at]);
    }
    if let Some(end) = kept
        .iter()
        .enumerate()
        .filter(|(_, b)| **b == b'\n')
        .nth(LINE_CAP - 1)
        .map(|(at, _)| at)
        && end + 1 < kept.len()
    {
        kept = &kept[..=end];
        truncated = true;
    }
    (String::from_utf8_lossy(kept).into_owned(), truncated)
}

/// The steps from the tree's top (named by the ref) to `path`: a directory's
/// each, and where `is_file`, the file's own last.
fn trail(at: At<'_>, path: &str, is_file: bool) -> Vec<Crumb> {
    let mut crumbs = vec![Crumb {
        name: at.name().to_owned(),
        section: FILES.to_owned(),
    }];
    if path.is_empty() {
        return crumbs;
    }
    let segments: Vec<&str> = path.split('/').collect();
    let mut so_far = String::new();
    for (index, segment) in segments.iter().enumerate() {
        if !so_far.is_empty() {
            so_far.push('/');
        }
        so_far.push_str(segment);
        let place = if is_file && index + 1 == segments.len() {
            Place::File(&so_far)
        } else {
            Place::Dir(&so_far)
        };
        crumbs.push(Crumb {
            name: (*segment).to_owned(),
            section: place.id(),
        });
    }
    crumbs
}

/// The language a file's name says it is in, where it is one a reader would
/// name; `None` otherwise. A hint only: nothing is highlighted (#701).
fn language_of(path: &str) -> Option<&'static str> {
    let name = path.rsplit('/').next().unwrap_or(path).to_ascii_lowercase();
    match name.as_str() {
        "dockerfile" => return Some("dockerfile"),
        "makefile" | "gnumakefile" => return Some("makefile"),
        "justfile" => return Some("just"),
        _ => {}
    }
    let (_, ext) = name.rsplit_once('.')?;
    Some(match ext {
        "rs" => "rust",
        "md" | "markdown" | "mdown" | "mkd" => "markdown",
        "ts" | "tsx" | "mts" | "cts" => "typescript",
        "js" | "jsx" | "mjs" | "cjs" => "javascript",
        "svelte" => "svelte",
        "py" => "python",
        "go" => "go",
        "java" => "java",
        "kt" | "kts" => "kotlin",
        "rb" => "ruby",
        "c" | "h" => "c",
        "cc" | "cpp" | "cxx" | "hpp" | "hh" => "cpp",
        "cs" => "csharp",
        "swift" => "swift",
        "sh" | "bash" | "zsh" => "shell",
        "sql" => "sql",
        "json" => "json",
        "yaml" | "yml" => "yaml",
        "toml" => "toml",
        "xml" => "xml",
        "html" | "htm" => "html",
        "css" => "css",
        "scss" => "scss",
        "gradle" => "gradle",
        "tf" => "terraform",
        "proto" => "protobuf",
        "txt" => "text",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_is_cut_at_whole_lines() {
        assert_eq!(capped(b"a\nb\n", false), ("a\nb\n".to_owned(), false));
        assert_eq!(
            capped(b"no newline", false),
            ("no newline".to_owned(), false)
        );
        // A read cut past the cap keeps the lines that fit.
        let mut long = b"one\n".to_vec();
        long.extend(std::iter::repeat_n(b'x', BYTE_CAP + 5));
        assert_eq!(capped(&long, true), ("one\n".to_owned(), true));
    }

    #[test]
    fn languages_are_named_by_extension_or_name() {
        assert_eq!(language_of("src/lib.rs"), Some("rust"));
        assert_eq!(language_of("README.MD"), Some("markdown"));
        assert_eq!(language_of("build/Dockerfile"), Some("dockerfile"));
        assert_eq!(language_of("LICENSE"), None);
        assert_eq!(language_of("a.unknownext"), None);
    }
}
