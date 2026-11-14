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
/// 2026-11-13), not from Gitea's `contents`, which works out each entry's
/// last commit and took 9 s for a folder of 3,000 entries and never answered
/// one of 12,000. Where the whole tree at the commit is kept (*Go to file*
/// read it, [`whole`]) and holds the folder, it is drawn from it with no
/// request. Otherwise it is the folder's own level of the tree, read by its
/// tree's sha (`git/trees/{sha}`, a request a thousand entries, kept by that
/// sha, which cannot change), found from the folders read before it, the
/// whole tree, or level by level from the top ([`walk`]): the whole tree is
/// not waited for, at the repository's quota eighteen requests for 16,000
/// entries. A tree draws no last commit, so nothing the listing drew is lost.
/// One past the kept whole tree's cut says so (`read_alone`). Where Gitea
/// answers no tree, `contents` reads it, as before.
async fn directory(
    source: &GiteaSource,
    owner: &str,
    repo: &str,
    at: &At<'_>,
    dir: &str,
) -> Result<Block, SourceError> {
    let key: WholeKey = (
        source.viewer_key(),
        owner.to_owned(),
        repo.to_owned(),
        at.sha.to_owned(),
    );
    let kept = kept_whole(&key);
    if let Some(whole) = kept.as_ref().filter(|whole| whole.holds(dir)) {
        return Ok(listing(
            at,
            dir,
            whole.children(dir),
            whole.web(at.sha, dir),
            false,
        ));
    }
    let past_cut = kept.as_ref().is_some_and(|whole| whole.truncated);
    let (walked, html) = tokio::join!(
        walk(source, &key, kept.as_deref(), dir),
        html(source, owner, repo)
    );
    match walked {
        Ok(Some(entries)) => Ok(listing(
            at,
            dir,
            entries,
            web(html.as_deref(), at.sha, dir),
            past_cut,
        )),
        // No tree from Gitea, or no such folder on the way: `contents` answers, or says so as Gitea does.
        Ok(None) | Err(_) => listed(source, owner, repo, at, dir).await,
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

    /// The directory's page at the commit ([`web`]).
    fn web(&self, sha: &str, dir: &str) -> Option<String> {
        web(self.html.as_deref(), sha, dir)
    }
}

/// A directory's page at the commit, as `contents` names an entry's
/// (`{repo}/src/commit/{sha}/{dir}`), path segments escaped as Gitea
/// escapes them; `html` is the repository's page.
fn web(html: Option<&str>, sha: &str, dir: &str) -> Option<String> {
    let html = html?.trim_end_matches('/');
    if dir.is_empty() {
        return Some(format!("{html}/src/commit/{sha}"));
    }
    let path: Vec<String> = dir.split('/').map(escape_segment).collect();
    Some(format!("{html}/src/commit/{sha}/{}", path.join("/")))
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
    let (first, html) = tokio::join!(page(1), html(source, owner, repo));
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

/// What a level of a tree is kept under: the token and server, the
/// repository and the tree's sha.
type LevelKey = (u64, String, String, String);

/// How many levels are kept, and how many commits' folder shas.
const LEVELS_KEPT: usize = 64;
const SHAS_KEPT: usize = 16;

/// The levels read, and each commit's folders' tree shas as the levels
/// named them, for the life of the process.
#[derive(Default)]
struct Levels {
    kept: VecDeque<(LevelKey, Arc<Vec<GitEntry>>)>,
    shas: VecDeque<(WholeKey, HashMap<String, String>)>,
    /// Each repository's page, by token and server and repository.
    html: HashMap<(u64, String, String), Option<String>>,
}

fn levels() -> &'static Mutex<Levels> {
    static LEVELS: OnceLock<Mutex<Levels>> = OnceLock::new();
    LEVELS.get_or_init(Default::default)
}

/// The repository's page (its record's `html_url`), read once.
async fn html(source: &GiteaSource, owner: &str, repo: &str) -> Option<String> {
    let key = (source.viewer_key(), owner.to_owned(), repo.to_owned());
    if let Some(html) = levels()
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .html
        .get(&key)
    {
        return html.clone();
    }
    let record = source
        .client
        .repo_as::<RepoRecord>(owner, repo)
        .await
        .ok()?;
    let html = record.repo.html_url;
    levels()
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .html
        .insert(key, html.clone());
    html
}

/// The tree sha of `dir` at the commit `key` names, as a level read before
/// named it.
fn known_sha(key: &WholeKey, dir: &str) -> Option<String> {
    let held = levels().lock().unwrap_or_else(PoisonError::into_inner);
    held.shas
        .iter()
        .find(|(at, _)| at == key)
        .and_then(|(_, shas)| shas.get(dir).cloned())
}

/// One level of a tree, by its sha, kept by it; the folders it holds are
/// noted under `dir` at the commit `key` names.
async fn level(
    source: &GiteaSource,
    key: &WholeKey,
    tree: &str,
    dir: &str,
) -> Result<Arc<Vec<GitEntry>>, SourceError> {
    let (viewer, owner, repo, _) = key;
    let at: LevelKey = (*viewer, owner.clone(), repo.clone(), tree.to_owned());
    let kept = levels()
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .kept
        .iter()
        .find(|(k, _)| *k == at)
        .map(|(_, entries)| Arc::clone(entries));
    let entries = match kept {
        Some(entries) => entries,
        None => {
            let page = |page| {
                source
                    .client
                    .git_tree_level_as::<GitTree>(owner, repo, tree, page, TREE_PAGE)
            };
            let (entries, _) = pages(page(1).await?, LEVEL_PAGES, page).await?;
            let entries = Arc::new(entries);
            let mut held = levels().lock().unwrap_or_else(PoisonError::into_inner);
            held.kept.push_back((at, Arc::clone(&entries)));
            while held.kept.len() > LEVELS_KEPT {
                held.kept.pop_front();
            }
            entries
        }
    };
    let mut held = levels().lock().unwrap_or_else(PoisonError::into_inner);
    if !held.shas.iter().any(|(k, _)| k == key) {
        held.shas.push_back((key.clone(), HashMap::new()));
        while held.shas.len() > SHAS_KEPT {
            held.shas.pop_front();
        }
    }
    if let Some((_, shas)) = held.shas.iter_mut().find(|(k, _)| k == key) {
        for entry in entries.iter().filter(|e| e.kind() == EntryKind::Dir) {
            let path = if dir.is_empty() {
                entry.path.clone()
            } else {
                format!("{dir}/{}", entry.path)
            };
            shas.insert(path, entry.sha.clone());
        }
    }
    Ok(entries)
}

/// `dir`'s entries, its own level of the tree: from the deepest of its
/// folders whose tree sha a level read before or the kept whole tree names
/// (the top at worst, by the commit's sha), one level at a time down to
/// `dir`. `None` where a folder on the way is not there.
async fn walk(
    source: &GiteaSource,
    key: &WholeKey,
    whole: Option<&Whole>,
    dir: &str,
) -> Result<Option<Vec<(String, EntryKind, u64)>>, SourceError> {
    let parts: Vec<&str> = dir.split('/').filter(|part| !part.is_empty()).collect();
    let mut depth = parts.len();
    let mut tree = loop {
        if depth == 0 {
            break key.3.clone();
        }
        let at = parts[..depth].join("/");
        if let Some(found) =
            known_sha(key, &at).or_else(|| whole.and_then(|w| w.tree_sha(&at)).map(str::to_owned))
        {
            break found;
        }
        depth -= 1;
    };
    loop {
        let here = parts[..depth].join("/");
        let entries = level(source, key, &tree, &here).await?;
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

/// What a section of a branch's or repository's tree draws from a stored
/// tree (the stored-trees pass of PR #2479): the branch it is read through
/// (a repository's default, by its record), the commit its id names, and the
/// place. A file, and a commit's tree, are not drawn from it.
pub(super) fn stored_ask(
    source: &GiteaSource,
    owner: &str,
    repo: &str,
    of: Of<'_>,
    id: &str,
    payload: &serde_json::Value,
) -> Option<knobas_source::tree::TreeAsk> {
    use knobas_source::tree::{TreeAsk, TreePlace};
    let named = Named::parse(id)?;
    let place = match named.place {
        Place::Dir(dir) => TreePlace::Dir(dir.to_owned()),
        Place::Every => TreePlace::Every,
        Place::File(_) => return None,
    };
    let branch = match of {
        Of::Branch(branch) => branch.to_owned(),
        Of::Repository => serde_json::from_value::<RepoRecord>(payload.clone())
            .ok()?
            .default_branch()
            .to_owned(),
        Of::Commit(_) => return None,
    };
    Some(TreeAsk {
        branch: knobas_core::entity::EntityRef::new(
            &source.id,
            &crate::keys::branch_key(owner, repo, &branch),
        )
        .to_string(),
        commit: named.sha.map(str::to_owned),
        place,
    })
}

/// The section `id` of a branch's or repository's tree drawn from `tree`, as
/// [`read`] draws it from Gitea at the same commit: a directory's listing
/// from its stored level ([`listing`], the entries a level of `git/trees`
/// answers), every file from the stored levels in git's recursive order
/// ([`every`]). `None` where the id names another commit or the levels do
/// not hold the place.
pub(super) fn stored_view(
    of: Of<'_>,
    id: &str,
    tree: &knobas_source::tree::StoredTree,
    payload: &serde_json::Value,
) -> Option<ViewSection> {
    let named = Named::parse(id)?;
    if named.sha.is_some_and(|sha| sha != tree.commit) {
        return None;
    }
    let sha = tree.commit.as_str();
    let at = match (of, named.sha) {
        (Of::Branch(branch), _) => At {
            sha,
            name: branch.to_owned(),
            branch: Some(branch),
            in_ids: true,
        },
        (Of::Repository, Some(_)) => At {
            sha,
            name: super::short(sha),
            branch: None,
            in_ids: true,
        },
        (Of::Repository, None) => At {
            sha,
            name: serde_json::from_value::<RepoRecord>(payload.clone())
                .ok()?
                .default_branch()
                .to_owned(),
            branch: None,
            in_ids: true,
        },
        (Of::Commit(_), _) => return None,
    };
    let block = match named.place {
        Place::Dir(dir) => {
            let found = tree
                .dir(dir)?
                .iter()
                .map(|item| (item.name.clone(), stored_kind(item), item.size))
                .collect();
            listing(
                &at,
                dir,
                found,
                web(tree.repository_url.as_deref(), sha, dir),
                false,
            )
        }
        Place::Every => {
            let entries = tree
                .walk()?
                .into_iter()
                .filter(|(_, item)| {
                    !matches!(stored_kind(item), EntryKind::Dir | EntryKind::Submodule)
                })
                .map(|(path, item)| TreeEntry {
                    section: Some(at.id(Place::File(&path))),
                    kind: stored_kind(item),
                    size: Some(item.size),
                    name: path,
                })
                .collect();
            Block::Tree {
                trail: trail(&at, "", false),
                entries,
                truncated: false,
                url: None,
                paths: None,
                commit: Some(sha.to_owned()),
                read_alone: None,
            }
        }
        Place::File(_) => return None,
    };
    Some(ViewSection {
        id: id.to_owned(),
        title: "Files".to_owned(),
        count: None,
        deferred: false,
        blocks: vec![block],
    })
}

/// What a listing draws a stored entry as, by its mode, as [`GitEntry::kind`]
/// draws Gitea's.
fn stored_kind(item: &knobas_source::tree::TreeItem) -> EntryKind {
    use knobas_source::tree::{MODE_COMMIT, MODE_SYMLINK, MODE_TREE};
    match item.mode {
        MODE_TREE => EntryKind::Dir,
        MODE_COMMIT => EntryKind::Submodule,
        MODE_SYMLINK => EntryKind::Symlink,
        _ => EntryKind::File,
    }
}

impl GitEntry {
    /// The entry as a stored tree keeps it, under `name`: its mode as a
    /// number, a directory's and a submodule's by its type where Gitea's
    /// mode says otherwise.
    fn stored(&self, name: &str) -> knobas_source::tree::TreeItem {
        use knobas_source::tree::{MODE_COMMIT, MODE_TREE};
        let mode = match self.kind.as_str() {
            "tree" => MODE_TREE,
            "commit" => MODE_COMMIT,
            _ => u32::from_str_radix(&self.mode, 8).unwrap_or(0o100_644),
        };
        knobas_source::tree::TreeItem {
            name: name.to_owned(),
            mode,
            size: self.size,
            sha: self.sha.clone(),
        }
    }
}

/// The levels of the tree at `commit` the store does not hold (the
/// stored-trees pass of PR #2479), read the cheaper of two ways:
///
/// * **The whole tree** (`git/trees/{commit}?recursive=true`, a request a
///   thousand entries), for a repository the store holds nothing of yet,
///   and where a walk would cost more requests than it: every level is in
///   it, each directory's sha with it.
/// * **Level by level** (`git/trees/{sha}`, one level, no `recursive`): the
///   commit's root first, which names its sha, then only the levels under
///   it whose sha is not held, side by side. A branch pushed with one file
///   changed five folders down costs six requests, and one whose root is
///   held already (a push that changed nothing, a branch made from
///   another) costs one. A walk that knows of as many new levels still to
///   read as the whole tree has pages (by the repository's largest stored
///   tree, `held.estimate()`) reads the whole tree instead.
///
/// A tree of more than `cap` entries in all (each directory counted, as
/// Gitea's `total_count` counts) reads no further and says so.
pub(super) async fn read_tree(
    source: &GiteaSource,
    owner: &str,
    repo: &str,
    commit: &str,
    held: &dyn knobas_source::tree::HeldTrees,
    cap: u64,
) -> Result<knobas_source::tree::TreeRead, SourceError> {
    use knobas_source::tree::TreeRead;
    use std::sync::atomic::{AtomicU32, Ordering};
    let sent = AtomicU32::new(0);
    let level_page = |sha: String, page: u32| {
        sent.fetch_add(1, Ordering::Relaxed);
        async move {
            source
                .client
                .git_tree_level_as::<GitTree>(owner, repo, &sha, page, TREE_PAGE)
                .await
        }
    };
    let Some(estimate) = held.estimate() else {
        return whole_levels(source, owner, repo, commit, held, cap, &sent).await;
    };
    let first = level_page(commit.to_owned(), 1).await?;
    let (entries, cut) = pages(first, LEVEL_PAGES, |page| {
        level_page(commit.to_owned(), page)
    })
    .await?;
    if cut {
        return Ok(TreeRead::TooLarge {
            entries: entries.len() as u64,
            requests: sent.load(Ordering::Relaxed),
        });
    }
    let stored: Vec<knobas_source::tree::TreeItem> =
        entries.iter().map(|e| e.stored(&e.path)).collect();
    let root = tree_key(&stored)?;
    if held.held(&root).is_some() {
        return Ok(TreeRead::Read {
            root,
            levels: Vec::new(),
            requests: sent.load(Ordering::Relaxed),
        });
    }
    // The pages the whole tree would cost: a walk that knows of as many new
    // levels still to read is no cheaper, and reads the whole tree instead.
    let whole_pages = estimate.div_ceil(TREE_PAGE as u64).max(1);
    let mut total = entries.len() as u64;
    let mut levels = vec![knobas_source::tree::TreeLevel {
        sha: root.clone(),
        entries: stored,
    }];
    let mut seen: std::collections::HashSet<String> =
        std::collections::HashSet::from([root.clone()]);
    let mut owed: VecDeque<String> = VecDeque::new();
    let mut note = |entries: &[knobas_source::tree::TreeItem],
                    owed: &mut VecDeque<String>,
                    total: &mut u64| {
        for item in entries.iter().filter(|item| item.is_tree()) {
            match held.held(&item.sha) {
                Some(below) => *total += below,
                None => {
                    if seen.insert(item.sha.clone()) {
                        owed.push_back(item.sha.clone());
                    }
                }
            }
        }
    };
    note(&levels[0].entries, &mut owed, &mut total);
    while !owed.is_empty() {
        if total > cap {
            return Ok(TreeRead::TooLarge {
                entries: total,
                requests: sent.load(Ordering::Relaxed),
            });
        }
        if owed.len() as u64 >= whole_pages {
            return whole_levels(source, owner, repo, commit, held, cap, &sent).await;
        }
        let some: Vec<String> = (0..TREE_READS_AT_ONCE)
            .filter_map(|_| owed.pop_front())
            .collect();
        let reads = some
            .iter()
            .map(|sha| async {
                let first = level_page(sha.clone(), 1).await?;
                pages(first, LEVEL_PAGES, |page| level_page(sha.clone(), page)).await
            })
            .collect();
        let answers = super::rows::all_of(reads).await?;
        for (sha, (entries, cut)) in some.into_iter().zip(answers) {
            if cut {
                return Ok(TreeRead::TooLarge {
                    entries: total + entries.len() as u64,
                    requests: sent.load(Ordering::Relaxed),
                });
            }
            total += entries.len() as u64;
            let level = knobas_source::tree::TreeLevel {
                sha,
                entries: entries.iter().map(|e| e.stored(&e.path)).collect(),
            };
            note(&level.entries, &mut owed, &mut total);
            levels.push(level);
        }
    }
    if total > cap {
        return Ok(TreeRead::TooLarge {
            entries: total,
            requests: sent.load(Ordering::Relaxed),
        });
    }
    Ok(TreeRead::Read {
        root,
        levels,
        requests: sent.load(Ordering::Relaxed),
    })
}

/// The sha git names a tree of `entries` by: the hash of `tree <length>\0`
/// and each entry's mode (octal, no leading zero), a space, its name, a NUL
/// and its object's id as bytes; SHA-256 where the ids are 64 hex digits.
/// What a stored tree's root is kept by, since Gitea answers a tree asked
/// by a commit with the commit's sha (its `ResolvedID`), not the tree's.
/// Gitea's own shas name every level below it. An id that is not hex is
/// refused.
fn tree_key(entries: &[knobas_source::tree::TreeItem]) -> Result<String, SourceError> {
    use sha1::Digest as _;
    let wide = entries.first().is_some_and(|e| e.sha.len() == 64);
    let mut body = Vec::new();
    for item in entries {
        let id = unhex(&item.sha)
            .filter(|id| id.len() == if wide { 32 } else { 20 })
            .ok_or_else(|| {
                SourceError::protocol(format!("gitea: {:?} names no object id", item.sha))
            })?;
        body.extend_from_slice(format!("{:o} ", item.mode).as_bytes());
        body.extend_from_slice(item.name.as_bytes());
        body.push(0);
        body.extend_from_slice(&id);
    }
    let mut object = format!("tree {}\0", body.len()).into_bytes();
    object.extend_from_slice(&body);
    let digest: Vec<u8> = if wide {
        sha2::Sha256::digest(&object).to_vec()
    } else {
        sha1::Sha1::digest(&object).to_vec()
    };
    Ok(digest.iter().map(|b| format!("{b:02x}")).collect())
}

fn unhex(sha: &str) -> Option<Vec<u8>> {
    let bytes = sha.as_bytes();
    if !bytes.len().is_multiple_of(2) {
        return None;
    }
    bytes
        .chunks(2)
        .map(|pair| {
            let digit = |b: u8| (b as char).to_digit(16);
            Some((digit(pair[0])? << 4 | digit(pair[1])?) as u8)
        })
        .collect()
}

/// The whole tree at `commit` as levels: Gitea's recursive tree, page by
/// page, cut into each directory's level by the paths, each named by the
/// sha its directory's entry gives (the root by the answer's own). Levels
/// `held` holds are left out.
async fn whole_levels(
    source: &GiteaSource,
    owner: &str,
    repo: &str,
    commit: &str,
    held: &dyn knobas_source::tree::HeldTrees,
    cap: u64,
    sent: &std::sync::atomic::AtomicU32,
) -> Result<knobas_source::tree::TreeRead, SourceError> {
    use knobas_source::tree::{TreeLevel, TreeRead};
    use std::sync::atomic::Ordering;
    let page = |page| {
        sent.fetch_add(1, Ordering::Relaxed);
        source
            .client
            .git_tree_as::<GitTree>(owner, repo, commit, page, TREE_PAGE)
    };
    let first = page(1).await?;
    if let Some(total) = first.total_count
        && total as u64 > cap
    {
        return Ok(TreeRead::TooLarge {
            entries: total as u64,
            requests: sent.load(Ordering::Relaxed),
        });
    }
    let most = u32::try_from(cap.div_ceil(TREE_PAGE as u64))
        .unwrap_or(u32::MAX)
        .max(1);
    let (entries, cut) = pages(first, most, page).await?;
    if cut || entries.len() as u64 > cap {
        return Ok(TreeRead::TooLarge {
            entries: entries.len() as u64,
            requests: sent.load(Ordering::Relaxed),
        });
    }
    let top: Vec<knobas_source::tree::TreeItem> = entries
        .iter()
        .filter(|e| !e.path.contains('/'))
        .map(|e| e.stored(&e.path))
        .collect();
    let root = tree_key(&top)?;
    // Each directory's sha by its path; its entries in the order read,
    // which is its level's.
    let mut sha_of: HashMap<&str, &str> = HashMap::from([("", root.as_str())]);
    for entry in entries.iter().filter(|e| e.kind() == EntryKind::Dir) {
        sha_of.insert(entry.path.as_str(), entry.sha.as_str());
    }
    let mut order: Vec<&str> = vec![""];
    let mut by_dir: HashMap<&str, Vec<knobas_source::tree::TreeItem>> =
        HashMap::from([("", Vec::new())]);
    for entry in &entries {
        let (up, name) = entry
            .path
            .rsplit_once('/')
            .unwrap_or(("", entry.path.as_str()));
        if entry.kind() == EntryKind::Dir && !by_dir.contains_key(entry.path.as_str()) {
            by_dir.insert(entry.path.as_str(), Vec::new());
            order.push(entry.path.as_str());
        }
        by_dir.entry(up).or_default().push(entry.stored(name));
    }
    let mut kept: std::collections::HashSet<&str> = std::collections::HashSet::new();
    let mut levels = Vec::new();
    for dir in order {
        let Some(&sha) = sha_of.get(dir) else {
            continue;
        };
        if held.held(sha).is_some() || !kept.insert(sha) {
            continue;
        }
        levels.push(TreeLevel {
            sha: sha.to_owned(),
            entries: by_dir.remove(dir).unwrap_or_default(),
        });
    }
    Ok(TreeRead::Read {
        root,
        levels,
        requests: sent.load(Ordering::Relaxed),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stored tree's root is kept by the sha git names it by: a tree
    /// `git rev-parse HEAD^{tree}` answered, from the entries `git ls-tree`
    /// listed (a file, a symlink, an executable and a folder), and git's
    /// empty tree.
    #[test]
    fn a_roots_key_is_the_sha_git_names_the_tree_by() {
        use knobas_source::tree::TreeItem;
        let item = |mode: u32, sha: &str, name: &str| TreeItem {
            name: name.to_owned(),
            mode,
            size: 0,
            sha: sha.to_owned(),
        };
        let entries = [
            item(
                0o100_644,
                "78981922613b2afb6025042ff6bd878ac1994e85",
                "README.md",
            ),
            item(
                0o120_000,
                "42061c01a1c70097d1e4579f29a5adf40abdec95",
                "link",
            ),
            item(
                0o100_755,
                "1a2485251c33a70432394c93fb89330ef214bfc9",
                "run.sh",
            ),
            item(0o040_000, "d0b5bae9918c3ebcf05b92275ad04ce3f76de837", "src"),
        ];
        assert_eq!(
            tree_key(&entries).unwrap(),
            "fed491db6fd52d560d7eb87cbe0a23907013ac60"
        );
        assert_eq!(
            tree_key(&[]).unwrap(),
            "4b825dc642cb6eb9a060e54bf8d69288fbee4904"
        );
        assert!(tree_key(&[item(0o100_644, "not-hex", "x")]).is_err());
    }

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
