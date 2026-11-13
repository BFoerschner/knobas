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

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use knobas_source::SourceError;
use knobas_source::view::{
    Block, Crumb, EntryKind, FileBytes, TreeEntry, TreeSections, ViewSection,
};

use knobas_source::file_text::{BYTE_CAP, capped, is_binary, language_of};

use super::rows::RepoRecord;
use crate::map;
use crate::source::GiteaSource;

/// The tab's id, and its top directory's.
pub(super) const FILES: &str = "files";

/// The most entries of one directory a view carries: as many as the whole
/// tree holds (the big-folder pass, 2026-11-13; it was a thousand, and a
/// folder of 3,000 drew its first thousand with nothing said in the tab).
const ENTRY_CAP: usize = 20_000;
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
                Some(sha) if super::is_full_sha(sha) => Some(sha),
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
                if !super::is_full_sha(sha) {
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

/// How this tab names a directory and a file by their path, at the tip
/// (#2298): what a deep link reads before any listing names its sections,
/// the plain spellings of [`Place::id`] that [`Named::parse`] reads.
pub(crate) fn tree_sections() -> TreeSections {
    TreeSections {
        dir: format!("{FILES}:"),
        file: "file:".to_owned(),
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
                .filter(|sha| super::is_full_sha(sha))
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

/// A directory's listing at `at` (the big-folder pass of PR #2479,
/// 2026-11-13): drawn from the whole tree at the commit ([`whole`], read once
/// and kept by its sha, the read *Go to file* filters), not from Gitea's
/// `contents`, which works out each entry's last commit and took 9 s for a
/// folder of 3,000 entries and never answered one of 12,000. The tree draws
/// no last commit, so nothing the listing drew is lost. A directory the whole
/// tree's cut left out is read on its own, level by level by the sha of each
/// tree ([`walk`]), as fast, and says so (`read_alone`). Where Gitea answers no
/// tree at all, `contents` reads it, as before.
async fn directory(
    source: &GiteaSource,
    owner: &str,
    repo: &str,
    at: &At<'_>,
    dir: &str,
) -> Result<Block, SourceError> {
    let Ok(whole) = whole(source, owner, repo, at.sha).await else {
        return listed(source, owner, repo, at, dir).await;
    };
    if whole.holds(dir) {
        return Ok(listing(
            at,
            dir,
            whole.children(dir),
            whole.web(at.sha, dir),
            false,
        ));
    }
    match walk(source, owner, repo, at.sha, &whole, dir).await? {
        Some(entries) => Ok(listing(at, dir, entries, whole.web(at.sha, dir), true)),
        // No such directory along the way: `contents` says so as Gitea does.
        None => listed(source, owner, repo, at, dir).await,
    }
}

/// A directory's listing as `contents` answers it: what [`directory`] falls
/// back to where Gitea answers no tree.
async fn listed(
    source: &GiteaSource,
    owner: &str,
    repo: &str,
    at: &At<'_>,
    dir: &str,
) -> Result<Block, SourceError> {
    let answered: Vec<Listed> = source
        .client
        .contents_at_as(owner, repo, dir, at.sha)
        .await?;
    // The directory's own page is any entry's, its name taken off.
    let url = answered.iter().find_map(|listed| {
        listed
            .html_url
            .as_deref()?
            .rsplit_once('/')
            .map(|(up, _)| up.to_owned())
    });
    let entries = answered
        .into_iter()
        .map(|listed| {
            let kind = match listed.kind.as_str() {
                "dir" => EntryKind::Dir,
                "symlink" => EntryKind::Symlink,
                "submodule" => EntryKind::Submodule,
                _ => EntryKind::File,
            };
            (listed.name, kind, listed.size)
        })
        .collect();
    Ok(listing(at, dir, entries, url, false))
}

/// A directory's block from its entries (name, kind, size), however read:
/// sorted directories first and by name as the shell draws them, at most
/// [`ENTRY_CAP`], each naming the section that opens it at `at`.
fn listing(
    at: &At<'_>,
    dir: &str,
    found: Vec<(String, EntryKind, u64)>,
    url: Option<String>,
    alone: bool,
) -> Block {
    let mut entries: Vec<TreeEntry> = found
        .into_iter()
        .map(|(name, kind, size)| {
            let path = if dir.is_empty() {
                name.clone()
            } else {
                format!("{dir}/{name}")
            };
            let (size, section) = match kind {
                EntryKind::Dir => (None, Some(at.id(Place::Dir(&path)))),
                EntryKind::Submodule => (None, None),
                EntryKind::File | EntryKind::Symlink => {
                    (Some(size), Some(at.id(Place::File(&path))))
                }
            };
            TreeEntry {
                name,
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
    Block::Tree {
        trail: trail(at, dir, false),
        entries,
        truncated,
        url: at.page(url.as_deref()),
        paths: Some(at.id(Place::Every)),
        commit: Some(at.sha.to_owned()),
        read_alone: alone.then_some(true),
    }
}

/// One entry of Gitea's tree, recursive (its whole path) or one level (its
/// name).
#[derive(Debug, Clone, serde::Deserialize)]
struct GitEntry {
    path: String,
    #[serde(rename = "type", default)]
    kind: String,
    #[serde(default)]
    mode: String,
    #[serde(default)]
    size: u64,
    /// The object's sha: a tree's is what its own level is read by.
    #[serde(default)]
    sha: String,
}

impl GitEntry {
    /// What a listing draws it as: a `tree` a directory, a `commit` a
    /// submodule, a blob of mode `120000` a symlink, any other blob a file.
    fn kind(&self) -> EntryKind {
        match (self.kind.as_str(), self.mode.as_str()) {
            ("tree", _) => EntryKind::Dir,
            ("commit", _) => EntryKind::Submodule,
            (_, "120000") => EntryKind::Symlink,
            _ => EntryKind::File,
        }
    }
}

/// One page of `git/trees/{sha}`, recursive or one level.
#[derive(Debug, serde::Deserialize)]
struct GitTree {
    /// Gitea answers `null` for a page past the last.
    #[serde(default, deserialize_with = "null_as_empty")]
    tree: Vec<GitEntry>,
    /// Every entry of the tree, all pages together (measured on Gitea 1.27.3
    /// and Forgejo 17, #2311): what says how many pages to read. The
    /// answer's own `truncated` is no help: on the last page Gitea says
    /// false and Forgejo true.
    #[serde(default)]
    total_count: Option<usize>,
}

fn null_as_empty<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<GitEntry>, D::Error> {
    use serde::Deserialize as _;
    Ok(Option::<Vec<GitEntry>>::deserialize(d)?.unwrap_or_default())
}

/// The whole tree at one commit, as Gitea's recursive tree answered it, cut
/// at [`TREE_PAGES`] pages: what *Go to file* filters and every directory it
/// holds whole is drawn from.
struct Whole {
    /// Every entry read, in git's order: a directory before what it holds,
    /// and what it holds together.
    entries: Vec<GitEntry>,
    /// The read stopped at [`TREE_PAGES`] with entries left.
    truncated: bool,
    /// Each directory's entries, by their place in `entries`; `""` the top.
    children: HashMap<String, Vec<usize>>,
    /// The repository's page (`html_url`), which a directory's is under.
    html: Option<String>,
}

impl Whole {
    fn new(entries: Vec<GitEntry>, truncated: bool, html: Option<String>) -> Self {
        let mut children: HashMap<String, Vec<usize>> = HashMap::new();
        for (at, entry) in entries.iter().enumerate() {
            let up = entry.path.rsplit_once('/').map_or("", |(up, _)| up);
            children.entry(up.to_owned()).or_default().push(at);
        }
        Self {
            entries,
            truncated,
            children,
            html,
        }
    }

    /// Whether `dir` is here with all it holds. In git's order a directory's
    /// entries follow it together, so where the read was cut the only ones
    /// it cut short are the last entry's directories; one not read at all
    /// is past the cut.
    fn holds(&self, dir: &str) -> bool {
        let here = dir.is_empty() || self.children.contains_key(dir) || self.is_dir(dir);
        if !self.truncated {
            return here;
        }
        let Some(last) = self.entries.last() else {
            return false;
        };
        here && !dir.is_empty() && last.path != dir && !last.path.starts_with(&format!("{dir}/"))
    }

    fn is_dir(&self, dir: &str) -> bool {
        let up = dir.rsplit_once('/').map_or("", |(up, _)| up);
        self.children.get(up).is_some_and(|at| {
            at.iter()
                .any(|&i| self.entries[i].path == dir && self.entries[i].kind() == EntryKind::Dir)
        })
    }

    /// `dir`'s entries as (name, kind, size).
    fn children(&self, dir: &str) -> Vec<(String, EntryKind, u64)> {
        self.children
            .get(dir)
            .map(|at| {
                at.iter()
                    .map(|&i| {
                        let entry = &self.entries[i];
                        let name = entry.path.rsplit_once('/').map_or(&*entry.path, |(_, n)| n);
                        (name.to_owned(), entry.kind(), entry.size)
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The sha of the tree at `dir`, where the read holds its entry.
    fn tree_sha(&self, dir: &str) -> Option<&str> {
        let up = dir.rsplit_once('/').map_or("", |(up, _)| up);
        self.children.get(up)?.iter().find_map(|&i| {
            let entry = &self.entries[i];
            (entry.path == dir && entry.kind() == EntryKind::Dir && !entry.sha.is_empty())
                .then_some(entry.sha.as_str())
        })
    }

    /// The directory's page at the commit, as `contents` names an entry's
    /// (`{repo}/src/commit/{sha}/{dir}`), path segments escaped as Gitea
    /// escapes them.
    fn web(&self, sha: &str, dir: &str) -> Option<String> {
        let html = self.html.as_deref()?.trim_end_matches('/');
        if dir.is_empty() {
            return Some(format!("{html}/src/commit/{sha}"));
        }
        let path: Vec<String> = dir.split('/').map(escape_segment).collect();
        Some(format!("{html}/src/commit/{sha}/{}", path.join("/")))
    }
}

/// A path segment as Go's `url.PathEscape` writes it, which Gitea's
/// `html_url` uses: unreserved characters and `$&+,:;=@` stay, the rest is
/// percent-encoded byte by byte.
fn escape_segment(segment: &str) -> String {
    let mut out = String::with_capacity(segment.len());
    for byte in segment.bytes() {
        if byte.is_ascii_alphanumeric() || b"-_.~$&+,:;=@".contains(&byte) {
            out.push(char::from(byte));
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// What [`whole`] keeps a tree under: the token and server, the repository
/// and the commit, which cannot change.
type WholeKey = (u64, String, String, String);

/// How many whole trees are kept: a few repositories' current commit.
const WHOLES_KEPT: usize = 8;

/// The whole trees read, for the life of the process (the app builds a
/// source for each view it reads, so one on the instance would last one read).
#[derive(Default)]
struct Wholes {
    /// The trees read, newest last.
    kept: VecDeque<(WholeKey, Arc<Whole>)>,
    /// One lock per tree being read, so two reads of it at once send its
    /// pages once.
    reading: HashMap<WholeKey, Arc<futures_util::lock::Mutex<()>>>,
}

fn wholes() -> &'static Mutex<Wholes> {
    static WHOLES: OnceLock<Mutex<Wholes>> = OnceLock::new();
    WHOLES.get_or_init(Default::default)
}

fn kept_whole(key: &WholeKey) -> Option<Arc<Whole>> {
    let held = wholes().lock().unwrap_or_else(PoisonError::into_inner);
    held.kept
        .iter()
        .find(|(at, _)| at == key)
        .map(|(_, whole)| Arc::clone(whole))
}

/// The whole tree at `sha`, kept by it: read page by page of Gitea's
/// recursive tree, the first alone (it says how many there are), the rest
/// side by side, at most [`TREE_PAGES`], beside the repository's record for
/// its page. A failed read is not kept.
async fn whole(
    source: &GiteaSource,
    owner: &str,
    repo: &str,
    sha: &str,
) -> Result<Arc<Whole>, SourceError> {
    let key: WholeKey = (
        source.viewer_key(),
        owner.to_owned(),
        repo.to_owned(),
        sha.to_owned(),
    );
    if let Some(whole) = kept_whole(&key) {
        return Ok(whole);
    }
    let reading = {
        let mut held = wholes().lock().unwrap_or_else(PoisonError::into_inner);
        Arc::clone(held.reading.entry(key.clone()).or_default())
    };
    let _one = reading.lock().await;
    // Another read of it may have kept it while this one waited.
    if let Some(whole) = kept_whole(&key) {
        return Ok(whole);
    }
    let read = read_whole(source, owner, repo, sha).await;
    let mut held = wholes().lock().unwrap_or_else(PoisonError::into_inner);
    held.reading.remove(&key);
    let whole = Arc::new(read?);
    held.kept.push_back((key, Arc::clone(&whole)));
    while held.kept.len() > WHOLES_KEPT {
        held.kept.pop_front();
    }
    Ok(whole)
}

async fn read_whole(
    source: &GiteaSource,
    owner: &str,
    repo: &str,
    sha: &str,
) -> Result<Whole, SourceError> {
    let page = |page| {
        source
            .client
            .git_tree_as::<GitTree>(owner, repo, sha, page, TREE_PAGE)
    };
    let (first, record) = tokio::join!(page(1), source.client.repo_as::<RepoRecord>(owner, repo));
    let html = record.ok().and_then(|record| record.repo.html_url);
    let (entries, truncated) = pages(first?, TREE_PAGES, page).await?;
    Ok(Whole::new(entries, truncated, html))
}

/// Every page of a tree after `first`, which says how many there are, side by
/// side [`TREE_READS_AT_ONCE`] at a time, at most `most`; with no count a page
/// short of full is the last, so another is read only after a full one. The
/// entries, and whether the read stopped at `most` with entries left.
async fn pages<F, R>(
    first: GitTree,
    most: u32,
    page: F,
) -> Result<(Vec<GitEntry>, bool), SourceError>
where
    F: Fn(u32) -> R,
    R: std::future::Future<Output = Result<GitTree, SourceError>>,
{
    let total = first.total_count;
    let mut pages = vec![first];
    match total {
        Some(total) => {
            // The server's own page, where it holds fewer than asked.
            let size = pages[0].tree.len().clamp(1, TREE_PAGE);
            let wanted = u32::try_from(total.div_ceil(size))
                .unwrap_or(u32::MAX)
                .min(most);
            let rest: Vec<u32> = (2..=wanted).collect();
            for some in rest.chunks(TREE_READS_AT_ONCE) {
                let reads = some.iter().map(|n| page(*n)).collect();
                pages.extend(super::rows::all_of(reads).await?);
            }
        }
        None => {
            let mut at = 1;
            while at < most && pages.last().is_some_and(|p| p.tree.len() >= TREE_PAGE) {
                at += 1;
                pages.push(page(at).await?);
            }
        }
    }
    let read: usize = pages.iter().map(|p| p.tree.len()).sum();
    let truncated = match total {
        Some(total) => read < total,
        None => {
            pages.len() as u32 == most && pages.last().is_some_and(|p| p.tree.len() >= TREE_PAGE)
        }
    };
    Ok((pages.into_iter().flat_map(|p| p.tree).collect(), truncated))
}

/// How many pages one level of a tree reads at most: a directory of up to
/// twenty thousand entries, as the whole tree.
const LEVEL_PAGES: u32 = TREE_PAGES;

/// `dir`'s entries, read past the whole tree's cut: from the deepest of its
/// directories the whole tree holds (the top at worst, by the commit's sha),
/// one level at a time by each tree's sha, down to `dir`. `None` where a
/// directory on the way is not there.
async fn walk(
    source: &GiteaSource,
    owner: &str,
    repo: &str,
    sha: &str,
    whole: &Whole,
    dir: &str,
) -> Result<Option<Vec<(String, EntryKind, u64)>>, SourceError> {
    let parts: Vec<&str> = dir.split('/').filter(|part| !part.is_empty()).collect();
    // The deepest directory on the way whose tree the whole tree names.
    let mut depth = parts.len();
    let mut tree = loop {
        if depth == 0 {
            break sha.to_owned();
        }
        if let Some(found) = whole.tree_sha(&parts[..depth].join("/")) {
            break found.to_owned();
        }
        depth -= 1;
    };
    loop {
        let level = |page| {
            source
                .client
                .git_tree_level_as::<GitTree>(owner, repo, &tree, page, TREE_PAGE)
        };
        let (entries, _) = pages(level(1).await?, LEVEL_PAGES, level).await?;
        if depth == parts.len() {
            return Ok(Some(
                entries
                    .iter()
                    .map(|entry| (entry.path.clone(), entry.kind(), entry.size))
                    .collect(),
            ));
        }
        let Some(next) = entries
            .iter()
            .find(|entry| entry.path == parts[depth] && entry.kind() == EntryKind::Dir)
        else {
            return Ok(None);
        };
        tree = next.sha.clone();
        depth += 1;
    }
}

/// Every file of the tree at `at` (#2311), by its whole path, in git's order:
/// the whole tree ([`whole`]), which directories are drawn from too. A tree
/// past [`TREE_PAGES`] is cut and says so. Directories and submodules are
/// left out; a symlink is a file, as in a listing.
async fn every(
    source: &GiteaSource,
    owner: &str,
    repo: &str,
    at: &At<'_>,
) -> Result<Block, SourceError> {
    let whole = whole(source, owner, repo, at.sha).await?;
    let entries = whole
        .entries
        .iter()
        .filter(|entry| entry.kind == "blob")
        .map(|entry| TreeEntry {
            section: Some(at.id(Place::File(&entry.path))),
            name: entry.path.clone(),
            kind: entry.kind(),
            size: Some(entry.size),
        })
        .collect();
    Ok(Block::Tree {
        trail: trail(at, "", false),
        entries,
        truncated: whole.truncated,
        url: None,
        paths: None,
        commit: Some(at.sha.to_owned()),
        read_alone: None,
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
    let binary = is_binary(&raw.bytes);
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

#[cfg(test)]
mod tests {
    use super::*;

    /// #2298: the scheme the shell builds a deep link's ids by is the one
    /// this tab parses, at the tip.
    #[test]
    fn the_declared_tree_sections_name_the_places_this_tab_reads() {
        let tree = tree_sections();
        for dir in ["src", "src/psp/gateway"] {
            assert_eq!(
                Named::parse(&format!("{}{dir}", tree.dir)),
                Some(Named {
                    place: Place::Dir(dir),
                    sha: None
                })
            );
        }
        for path in ["README.md", "src/psp/gateway/sepa/codes.rs"] {
            assert_eq!(
                Named::parse(&format!("{}{path}", tree.file)),
                Some(Named {
                    place: Place::File(path),
                    sha: None
                })
            );
        }
    }
}
