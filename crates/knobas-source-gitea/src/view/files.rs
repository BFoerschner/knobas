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
//! * `file:{path}` -- a file (`file:src/lib.rs`), read raw;
//! * `paths` -- every file of the tree, by its whole path (#2311): what
//!   *Go to file* filters, named by every listing (`Block::Tree::paths`).
//!
//! A branch's and a repository's tree are read at the commit the tip was
//! when the tree was first asked for, and the ids the tree hands out name
//! that sha (`file@{sha}:{path}`, [`Named::parse`], #2241): a file opened
//! from it costs its own entry and its bytes, side by side, and nothing for
//! the repository or the branch. A commit's view answers the plain sections
//! at its own sha (#2239). What a section naming a commit reads cannot
//! change, so the app keeps it for the session (ADR-0032); one that reads the
//! tip first moves, and is read again behind its answer (#2242).
//!
//! No section offers a write: *Commit change…*'s standing on the branch is
//! the source's answer to `refused_writes` (`crate::grants`, #2241), asked
//! once per branch and kept, not a read beside every file.
//!
//! A directory keeps its first [`ENTRY_CAP`] entries in order, and a file
//! [`BYTE_CAP`] bytes and [`LINE_CAP`] lines, cut at a line; either says it
//! was cut and where the rest is (ADR-0020 rule 6). A file with a NUL byte in
//! its first [`SNIFF`] bytes is binary (git's own test) and is drawn as its
//! size and a link. Beside its bytes, a file's own `contents-ext` entry is read
//! for its blob sha at the ref (#883), the base an edit of it commits against,
//! and its page.

use knobas_source::SourceError;
use knobas_source::view::{Block, Crumb, EntryKind, FileBytes, TreeEntry, ViewSection};

use super::rows::RepoRecord;
use crate::map;
use crate::source::GiteaSource;

/// The tab's id, and its top directory's.
pub(super) const FILES: &str = "files";

/// The most entries of one directory a view carries.
const ENTRY_CAP: usize = 1_000;
/// The most of a file a view carries, in bytes.
const BYTE_CAP: usize = 1_000_000;
/// The most of a file a view carries, in lines.
const LINE_CAP: usize = 10_000;
/// How far into a file a NUL byte makes it binary, as git looks.
const SNIFF: usize = 8_000;
/// The whole tree's id (#2311).
const PATHS: &str = "paths";
/// How many entries one page of Gitea's recursive tree is asked for: its
/// default and its most (`[api] DEFAULT_GIT_TREES_PER_PAGE`, 1000, by
/// Gitea's source). A server set lower answers fewer and says how many in
/// all, so its pages are read by the size of its first, up to [`TREE_PAGES`].
const TREE_PAGE: usize = 1_000;
/// How many pages of the recursive tree the whole tree reads at most: twenty
/// thousand entries, directories among them. A tree past it is cut there and
/// says so (`truncated`).
const TREE_PAGES: u32 = 20;
/// How many pages after the first are asked side by side.
const TREE_READS_AT_ONCE: usize = 4;

/// A place in the tree a section id names: a directory (`""` the top) or a
/// file. The one place an id is taken apart; [`Place::id`] builds them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Place<'a> {
    Dir(&'a str),
    File(&'a str),
    /// Every file below the top (#2311).
    Every,
}

/// A section id taken apart: where in the tree, and the commit it names
/// where it names one (#2241).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Named<'a> {
    place: Place<'a>,
    sha: Option<&'a str>,
}

impl<'a> Named<'a> {
    /// What `id` names, or `None` for an id that is not this tab's.
    ///
    /// A branch's and a repository's tree are read at the commit their tip
    /// was when the tree was first asked for, and every id the tree hands
    /// out names it: `files@{sha}`, `files@{sha}:{dir}`, `file@{sha}:{path}`.
    /// A read of such an id sends no request for the branch, and stands at a
    /// fixed commit (ADR-0032). An id without one -- the tab's own `files`,
    /// or a deep link's guess -- reads the tip first.
    fn parse(id: &'a str) -> Option<Self> {
        if let Some(rest) = id.strip_prefix(PATHS) {
            let sha = match rest.strip_prefix('@') {
                Some(sha) if super::full_sha(sha) => Some(sha),
                None if rest.is_empty() => None,
                _ => return None,
            };
            return Some(Self {
                place: Place::Every,
                sha,
            });
        }
        let (dir, rest) = if let Some(rest) = id.strip_prefix(FILES) {
            (true, rest)
        } else {
            (false, id.strip_prefix("file")?)
        };
        let (sha, rest) = match rest.strip_prefix('@') {
            Some(at) => {
                let (sha, tail) = at.split_once(':').map_or((at, None), |(s, t)| (s, Some(t)));
                if !super::full_sha(sha) {
                    return None;
                }
                (Some(sha), tail)
            }
            None if rest.is_empty() => (None, None),
            None => (None, Some(rest.strip_prefix(':')?)),
        };
        let place = match (dir, rest) {
            (true, None) => Place::Dir(""),
            // `files:` with nothing after it is not the top's spelling.
            (true, Some(dir)) if !dir.is_empty() => Place::Dir(dir),
            (false, Some(path)) => Place::File(path),
            _ => return None,
        };
        Some(Self { place, sha })
    }
}

impl Place<'_> {
    /// The section id that opens this place, naming `sha` where given.
    fn id(self, sha: Option<&str>) -> String {
        let at = sha.map(|sha| format!("@{sha}")).unwrap_or_default();
        match self {
            Self::Dir("") => format!("{FILES}{at}"),
            Self::Dir(dir) => format!("{FILES}{at}:{dir}"),
            Self::File(path) => format!("file{at}:{path}"),
            Self::Every => format!("{PATHS}{at}"),
        }
    }
}

/// Whether `section` is one of this tab's: the tab itself, a directory or a
/// file.
pub(super) fn owns(section: &str) -> bool {
    Named::parse(section).is_some()
}

/// Whether `section` is one of this tab's and names the commit it is read at.
pub(super) fn names_sha(section: &str) -> bool {
    Named::parse(section).is_some_and(|named| named.sha.is_some())
}

/// What the entity a section is read through reads at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Of<'a> {
    /// A branch, by its name.
    Branch(&'a str),
    /// A repository: its default branch.
    Repository,
    /// A commit, which its entity names (its ids name no sha).
    Commit(&'a str),
}

/// Where one read stands: the commit, the name the trail starts at, the
/// branch a page is named by (none at a commit), and whether ids name the
/// sha.
struct At<'a> {
    sha: &'a str,
    name: String,
    branch: Option<&'a str>,
    in_ids: bool,
}

impl At<'_> {
    fn id(&self, place: Place<'_>) -> String {
        place.id(self.in_ids.then_some(self.sha))
    }

    /// Gitea's page as an entry's `html_url` names it at the sha, named by
    /// the branch where the read is a branch's.
    fn page(&self, html_url: Option<&str>) -> Option<String> {
        let url = html_url.filter(|url| !url.is_empty())?;
        Some(match self.branch {
            Some(branch) => url.replacen(
                &format!("/src/commit/{}", self.sha),
                &format!("/src/branch/{}", map::url_path(branch)),
                1,
            ),
            None => url.to_owned(),
        })
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

/// A branch's tip, as `branches/{name}` answers it.
#[derive(Debug, serde::Deserialize)]
struct Tip {
    #[serde(default)]
    commit: Option<super::rows::TipCommit>,
}

/// The section `id` names, read through an entity of `of`: a directory's
/// listing or a file, at the commit the id names, else at the branch's tip,
/// which is read first; the ids the answer hands out name the commit, so
/// the next read through them is the commit's.
pub(super) async fn read(
    source: &GiteaSource,
    owner: &str,
    repo: &str,
    of: Of<'_>,
    id: &str,
) -> Result<ViewSection, SourceError> {
    let unnamed = || SourceError::protocol(format!("gitea: {id:?} names no place in the tree"));
    let named = Named::parse(id).ok_or_else(unnamed)?;
    match named.place {
        Place::Dir("") | Place::Every => {}
        Place::Dir(path) | Place::File(path) => crate::client::GiteaClient::tree_path(path)?,
    }
    let resolved: String;
    let at = match (of, named.sha) {
        (Of::Commit(sha), None) => At {
            sha,
            name: sha.to_owned(),
            branch: None,
            in_ids: false,
        },
        (Of::Commit(_), Some(_)) => return Err(unnamed()),
        (Of::Branch(branch), Some(sha)) => At {
            sha,
            name: branch.to_owned(),
            branch: Some(branch),
            in_ids: true,
        },
        (Of::Repository, Some(sha)) => At {
            sha,
            name: super::short(sha),
            branch: None,
            in_ids: true,
        },
        (Of::Branch(_) | Of::Repository, None) => {
            let name = match of {
                Of::Branch(branch) => branch.to_owned(),
                _ => {
                    let record: RepoRecord = source.client.repo_as(owner, repo).await?;
                    record.default_branch().to_owned()
                }
            };
            let tip: Tip = source.client.branch_as(owner, repo, &name).await?;
            resolved = tip
                .commit
                .map(|c| c.id)
                .filter(|sha| super::full_sha(sha))
                .ok_or_else(|| {
                    SourceError::protocol(format!("gitea: {name:?} answered no tip commit"))
                })?;
            At {
                sha: resolved.as_str(),
                name,
                // A repository's pages name the commit, as its later reads'
                // do: those know no branch.
                branch: match of {
                    Of::Branch(branch) => Some(branch),
                    _ => None,
                },
                in_ids: true,
            }
        }
    };
    let block = match named.place {
        Place::File(path) => file(source, owner, repo, &at, path).await?,
        Place::Dir(dir) => directory(source, owner, repo, &at, dir).await?,
        Place::Every => every(source, owner, repo, &at).await?,
    };
    Ok(ViewSection {
        // The id asked, as every section answers (the contract battery):
        // what the tree hands out names the sha.
        id: id.to_owned(),
        title: "Files".to_owned(),
        count: None,
        // Its own read is the read the deferral waited for (rule 3).
        deferred: false,
        blocks: vec![block],
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
    /// Gitea's page of the entry at the ref read.
    #[serde(default)]
    html_url: Option<String>,
}

async fn directory(
    source: &GiteaSource,
    owner: &str,
    repo: &str,
    at: &At<'_>,
    dir: &str,
) -> Result<Block, SourceError> {
    let listing: Vec<Listed> = source
        .client
        .contents_at_as(owner, repo, dir, at.sha)
        .await?;
    // The directory's own page is any entry's, its name taken off.
    let url = listing.iter().find_map(|listed| {
        listed
            .html_url
            .as_deref()?
            .rsplit_once('/')
            .map(|(up, _)| up.to_owned())
    });
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
                EntryKind::Dir => (None, Some(at.id(Place::Dir(&path)))),
                EntryKind::Submodule => (None, None),
                EntryKind::File | EntryKind::Symlink => {
                    (Some(listed.size), Some(at.id(Place::File(&path))))
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
        url: at.page(url.as_deref()),
        paths: Some(at.id(Place::Every)),
    })
}

/// One entry of Gitea's recursive tree.
#[derive(Debug, serde::Deserialize)]
struct GitEntry {
    path: String,
    #[serde(rename = "type", default)]
    kind: String,
    #[serde(default)]
    mode: String,
    #[serde(default)]
    size: u64,
}

/// One page of `git/trees/{sha}?recursive=true`.
#[derive(Debug, serde::Deserialize)]
struct GitTree {
    #[serde(default)]
    tree: Vec<GitEntry>,
    /// Every entry of the tree, all pages together (measured on Gitea 1.27.3
    /// and Forgejo 17, #2311): what says how many pages to read. The
    /// answer's own `truncated` is no help: on the last page Gitea says
    /// false and Forgejo true.
    #[serde(default)]
    total_count: Option<usize>,
}

/// Every file of the tree at `at` (#2311), by its whole path, in git's order:
/// page by page of Gitea's recursive tree, the first read alone (it says how
/// many there are), the rest side by side, at most [`TREE_PAGES`]. A tree
/// past them is cut and says so. Directories and submodules are left out; a
/// symlink is a file, as in a listing.
async fn every(
    source: &GiteaSource,
    owner: &str,
    repo: &str,
    at: &At<'_>,
) -> Result<Block, SourceError> {
    let first: GitTree = source
        .client
        .git_tree_as(owner, repo, at.sha, 1, TREE_PAGE)
        .await?;
    // No count: a page short of full is the last one, so another is read
    // only after a full one, one at a time.
    let total = first.total_count;
    let mut pages = vec![first];
    match total {
        Some(total) => {
            // The server's own page, where it holds fewer than asked.
            let size = pages[0].tree.len().clamp(1, TREE_PAGE);
            let wanted = u32::try_from(total.div_ceil(size))
                .unwrap_or(u32::MAX)
                .min(TREE_PAGES);
            let rest: Vec<u32> = (2..=wanted).collect();
            for some in rest.chunks(TREE_READS_AT_ONCE) {
                let reads = some
                    .iter()
                    .map(|page| {
                        source
                            .client
                            .git_tree_as::<GitTree>(owner, repo, at.sha, *page, TREE_PAGE)
                    })
                    .collect();
                pages.extend(super::rows::all_of(reads).await?);
            }
        }
        None => {
            let mut page = 1;
            while page < TREE_PAGES && pages.last().is_some_and(|p| p.tree.len() >= TREE_PAGE) {
                page += 1;
                pages.push(
                    source
                        .client
                        .git_tree_as(owner, repo, at.sha, page, TREE_PAGE)
                        .await?,
                );
            }
        }
    }
    let read: usize = pages.iter().map(|p| p.tree.len()).sum();
    let truncated = match total {
        Some(total) => read < total,
        None => {
            pages.len() as u32 == TREE_PAGES
                && pages.last().is_some_and(|p| p.tree.len() >= TREE_PAGE)
        }
    };
    let entries = pages
        .into_iter()
        .flat_map(|page| page.tree)
        .filter(|entry| entry.kind == "blob")
        .map(|entry| {
            let kind = if entry.mode == "120000" {
                EntryKind::Symlink
            } else {
                EntryKind::File
            };
            TreeEntry {
                section: Some(at.id(Place::File(&entry.path))),
                name: entry.path,
                kind,
                size: Some(entry.size),
            }
        })
        .collect();
    Ok(Block::Tree {
        trail: trail(at, "", false),
        entries,
        truncated,
        url: None,
        paths: None,
    })
}

async fn file(
    source: &GiteaSource,
    owner: &str,
    repo: &str,
    at: &At<'_>,
    path: &str,
) -> Result<Block, SourceError> {
    // The file's own entry (its blob sha, #883, its size where a cut read
    // cannot say, and its page) and its bytes, side by side: both are read
    // at one commit, which no push moves (#2241), so neither can be newer
    // than the other.
    let (own, raw) = tokio::try_join!(
        source
            .client
            .file_entry_as::<Listed>(owner, repo, path, at.sha),
        source
            .client
            .raw_capped(owner, repo, path, at.sha, BYTE_CAP),
    )?;
    let binary = raw.bytes.iter().take(SNIFF).any(|b| *b == 0);
    let (text, truncated) = if binary {
        (String::new(), false)
    } else {
        capped(&raw.bytes, raw.cut)
    };
    // Cut, and the answer had no length (a compressing proxy): the file's
    // own entry says, rather than the megabyte that was read.
    let size = raw.size.unwrap_or(own.size);
    Ok(Block::File {
        trail: trail(at, path, true),
        path: path.to_owned(),
        text,
        language: language_of(path).map(str::to_owned),
        size,
        truncated,
        binary,
        url: at.page(own.html_url.as_deref()),
        sha: own.sha,
        // The byte read of this file, or of one beside it, takes it back.
        commit: Some(at.sha.to_owned()),
    })
}

/// The bytes of the file at `path`, read at `at` (a commit sha or a branch
/// name), whole or refused with its size where it is over `cap` (#2256):
/// one raw read, and a refusal whose answer gave no length asks the file's
/// own entry for it.
pub(super) async fn bytes(
    source: &GiteaSource,
    owner: &str,
    repo: &str,
    at: &str,
    path: &str,
    cap: u64,
) -> Result<FileBytes, SourceError> {
    crate::client::GiteaClient::tree_path(path)?;
    let cap_bytes = usize::try_from(cap).unwrap_or(usize::MAX);
    let raw = source
        .client
        .raw_whole(owner, repo, path, at, cap_bytes)
        .await?;
    if !raw.cut {
        return Ok(FileBytes::Read(raw.bytes));
    }
    let size = match raw.size {
        Some(size) => size,
        None => {
            source
                .client
                .file_entry_as::<Listed>(owner, repo, path, at)
                .await?
                .size
        }
    };
    Ok(FileBytes::TooLarge { size })
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
fn trail(at: &At<'_>, path: &str, is_file: bool) -> Vec<Crumb> {
    let mut crumbs = vec![Crumb {
        name: at.name.clone(),
        section: at.id(Place::Dir("")),
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
            section: at.id(place),
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
