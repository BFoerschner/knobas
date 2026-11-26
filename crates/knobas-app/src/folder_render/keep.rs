//! Keeping what a folder renderer wrote into `{outdir}` (ADR-0046).
//!
//! The tool is the person's own program, but what it writes is made from
//! repository data: an archive it unpacked, names it took from files. So the
//! output is data, and this is the seam.
//!
//! [`keep`] reads the output folder once, without following a link, and
//! writes the regular files it holds into a folder knobas made, as regular
//! files with no executable bit:
//!
//! - a symlink, and on Windows any reparse point (a junction, a mount point,
//!   a cloud placeholder), is left out as [`LeftOutWhy::Link`] and never
//!   entered; a FIFO, socket or device as [`LeftOutWhy::Special`];
//! - an entry named `.git`, case aside, is left out whole: an editor opening
//!   the kept folder would run a generated repository's hooks;
//! - a name that is not UTF-8, a path [`copy::check_path`] refuses (the
//!   copy's own rules, on every platform), the second of two names in one
//!   folder alike but for case, and a path deeper than [`OUT_DEPTH`] or
//!   longer than [`OUT_PATH_BYTES`] are left out and said;
//! - an empty folder is not kept.
//!
//! A file that cannot be kept is left out and said, never a failed run: one
//! odd name should not cost a minute's generation. Past [`OUT_FILES`] files
//! or [`OUT_BYTES`] bytes nothing is kept.
//!
//! [`read`] reads a kept file by the manifest alone, opening it without
//! following a link. Nothing here canonicalizes or follows.

use std::collections::{HashMap, HashSet};
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use super::copy;

/// The most files one run may keep.
pub const OUT_FILES: usize = 20_000;
/// The most bytes one run may keep.
pub const OUT_BYTES: u64 = 1024 * 1024 * 1024;
/// The deepest path kept, in folders and the file.
pub const OUT_DEPTH: usize = 32;
/// The longest path kept, in bytes, from the generated folder's top.
pub const OUT_PATH_BYTES: usize = 480;
/// How many left-out entries are named; the rest are counted.
pub const LEFT_OUT_SHOWN: usize = 50;
/// The longest left-out path shown, in characters.
const LEFT_OUT_CHARS: usize = 200;

/// One kept file: its path from the generated folder's top, and its size.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export_to = "app.ts")]
pub struct MadeFile {
    pub path: String,
    pub size: u64,
}

/// Why an entry the tool wrote was not kept.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
#[ts(export_to = "app.ts", rename = "MadeLeftOutWhy")]
pub enum LeftOutWhy {
    /// A symlink or a reparse point: never followed.
    Link,
    /// A FIFO, socket or device.
    Special,
    /// A `.git` file or folder.
    Git,
    /// A name Windows does not take, or one that climbs.
    Name,
    NotUtf8,
    /// The same name as another in its folder but for case.
    CaseTwin,
    TooDeep,
}

/// One entry left out, its path made safe to show. `MadeLeftOut` in the
/// shell, beside the search's own `LeftOut`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export_to = "app.ts", rename = "MadeLeftOut")]
pub struct LeftOut {
    /// Lossy, control characters replaced, [`LEFT_OUT_CHARS`] at most.
    pub path: String,
    pub why: LeftOutWhy,
}

/// What a generated folder holds, as written beside it
/// (`<leaf>.files.json`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    /// Sorted by path bytes.
    pub files: Vec<MadeFile>,
    pub bytes: u64,
    /// The first [`LEFT_OUT_SHOWN`].
    pub left_out: Vec<LeftOut>,
    pub left_out_total: u32,
}

/// A generated folder: where it is kept and what it holds.
#[derive(Clone, Debug)]
pub struct Made {
    pub dir: PathBuf,
    pub manifest: Manifest,
    index: HashMap<String, usize>,
}

impl Made {
    /// The folder at `dir` holding `manifest`.
    #[must_use]
    pub fn new(dir: PathBuf, mut manifest: Manifest) -> Self {
        manifest
            .files
            .sort_unstable_by(|a, b| a.path.as_bytes().cmp(b.path.as_bytes()));
        let index = manifest
            .files
            .iter()
            .enumerate()
            .map(|(at, f)| (f.path.clone(), at))
            .collect();
        Self {
            dir,
            manifest,
            index,
        }
    }

    /// The kept file at `path`, when the manifest holds it.
    #[must_use]
    pub fn file(&self, path: &str) -> Option<&MadeFile> {
        self.index.get(path).map(|at| &self.manifest.files[*at])
    }

    /// The absolute path of a file the manifest holds.
    #[must_use]
    pub fn path_of(&self, path: &str) -> Option<PathBuf> {
        self.file(path)?;
        Some(
            path.split('/')
                .fold(self.dir.clone(), |at, seg| at.join(seg)),
        )
    }

    /// The folder moved to `dir` (a rename of the whole folder).
    #[must_use]
    pub fn moved_to(self, dir: PathBuf) -> Self {
        Self { dir, ..self }
    }
}

/// Why nothing was kept.
#[derive(Debug, PartialEq, Eq)]
pub enum KeepError {
    /// Past [`OUT_FILES`] or [`OUT_BYTES`]: the real totals.
    TooBig {
        files: u64,
        bytes: u64,
    },
    /// Not one regular file to keep.
    Nothing,
    Stopped,
    /// The kept folder could not be written.
    Io(String),
}

/// Why a kept file could not be read.
#[derive(Debug, PartialEq, Eq)]
pub enum ReadError {
    /// The manifest does not hold it, or it is no longer a plain file there.
    NotFound,
    Io(String),
}

/// Keeping's progress: files kept of the total.
pub type Progress = Arc<dyn Fn(u32, u32) + Send + Sync>;

/// The bounds [`keep`] holds to, smaller in tests.
#[derive(Clone, Copy, Debug)]
pub struct Caps {
    pub files: usize,
    pub bytes: u64,
}

impl Default for Caps {
    fn default() -> Self {
        Self {
            files: OUT_FILES,
            bytes: OUT_BYTES,
        }
    }
}

/// Keep the regular files under `out` in `into`, a folder this makes (it
/// must not exist yet). Runs on the blocking pool's thread it is called on.
///
/// # Errors
///
/// [`KeepError`]; on any error or a stop `into` is removed.
pub fn keep(
    out: &Path,
    into: &Path,
    progress: &Progress,
    cancel: &CancellationToken,
) -> Result<Made, KeepError> {
    keep_with(out, into, progress, cancel, Caps::default(), false)
}

/// [`keep`] with its bounds, and the byte copy forced in place of a rename
/// (`copy_only`, for the test that the two keep the same tree).
///
/// # Errors
///
/// As [`keep`].
pub fn keep_with(
    out: &Path,
    into: &Path,
    progress: &Progress,
    cancel: &CancellationToken,
    caps: Caps,
    copy_only: bool,
) -> Result<Made, KeepError> {
    let mut walked = walk(out, cancel)?;
    let total_bytes: u64 = walked.files.iter().map(|(_, size)| size).sum();
    if walked.files.len() > caps.files || total_bytes > caps.bytes {
        return Err(KeepError::TooBig {
            files: walked.files.len() as u64,
            bytes: total_bytes,
        });
    }
    if walked.files.is_empty() {
        return Err(KeepError::Nothing);
    }
    std::fs::create_dir_all(into.parent().unwrap_or(into))
        .and_then(|()| std::fs::create_dir(into))
        .map_err(|e| KeepError::Io(format!("{} could not be made: {e}", into.display())))?;
    let written = write(out, into, &mut walked, progress, cancel, caps, copy_only);
    match written {
        Ok(manifest) => Ok(Made::new(into.to_path_buf(), manifest)),
        Err(error) => {
            let _ = std::fs::remove_dir_all(into);
            Err(error)
        }
    }
}

/// What pass one found.
struct Walked {
    /// Regular files, their paths from the top and their sizes, in walk order.
    files: Vec<(String, u64)>,
    left_out: Vec<LeftOut>,
    left_out_total: u32,
}

impl Walked {
    fn leave(&mut self, path: &str, why: LeftOutWhy) {
        self.left_out_total = self.left_out_total.saturating_add(1);
        if self.left_out.len() < LEFT_OUT_SHOWN {
            self.left_out.push(LeftOut {
                path: shown(path),
                why,
            });
        }
    }
}

/// A path as it may be shown: control characters replaced, cut to
/// [`LEFT_OUT_CHARS`].
fn shown(path: &str) -> String {
    let mut out: String = path
        .chars()
        .map(|c| if c.is_control() { '\u{fffd}' } else { c })
        .take(LEFT_OUT_CHARS)
        .collect();
    if path.chars().count() > LEFT_OUT_CHARS {
        out.push('…');
    }
    out
}

/// Whether the entry is a link of any kind: a symlink, or on Windows any
/// reparse point.
fn is_link(meta: &std::fs::Metadata) -> bool {
    if meta.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt as _;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        if meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return true;
        }
    }
    false
}

/// Pass one: every entry under `out`, each folder's entries in name-byte
/// order, by `symlink_metadata` alone.
fn walk(out: &Path, cancel: &CancellationToken) -> Result<Walked, KeepError> {
    let mut walked = Walked {
        files: Vec::new(),
        left_out: Vec::new(),
        left_out_total: 0,
    };
    // Folders to read: their path on disk and from the top ("" the top).
    let mut stack: Vec<(PathBuf, String)> = vec![(out.to_path_buf(), String::new())];
    while let Some((dir, rel)) = stack.pop() {
        if cancel.is_cancelled() {
            return Err(KeepError::Stopped);
        }
        let Ok(listed) = std::fs::read_dir(&dir) else {
            if rel.is_empty() {
                return Err(KeepError::Nothing);
            }
            continue;
        };
        let mut entries: Vec<std::fs::DirEntry> = listed.flatten().collect();
        entries.sort_by(|a, b| {
            a.file_name()
                .as_encoded_bytes()
                .cmp(b.file_name().as_encoded_bytes())
        });
        let mut seen: HashSet<String> = HashSet::with_capacity(entries.len());
        // Subfolders go on the stack after this folder's files, in reverse,
        // so they are walked in name order.
        let mut subfolders: Vec<(PathBuf, String)> = Vec::new();
        for entry in entries {
            let raw = entry.file_name();
            let lossy = raw.to_string_lossy();
            let joined = |name: &str| {
                if rel.is_empty() {
                    name.to_owned()
                } else {
                    format!("{rel}/{name}")
                }
            };
            // Never `metadata`: it follows a link.
            let Ok(meta) = std::fs::symlink_metadata(entry.path()) else {
                continue;
            };
            if is_link(&meta) {
                walked.leave(&joined(&lossy), LeftOutWhy::Link);
                continue;
            }
            let kind = meta.file_type();
            if !kind.is_dir() && !kind.is_file() {
                walked.leave(&joined(&lossy), LeftOutWhy::Special);
                continue;
            }
            if lossy.eq_ignore_ascii_case(".git") {
                walked.leave(&joined(&lossy), LeftOutWhy::Git);
                continue;
            }
            let Some(name) = raw.to_str() else {
                walked.leave(&joined(&lossy), LeftOutWhy::NotUtf8);
                continue;
            };
            let path = joined(name);
            if copy::check_path(&path).is_err() {
                walked.leave(&path, LeftOutWhy::Name);
                continue;
            }
            if !seen.insert(name.to_lowercase()) {
                walked.leave(&path, LeftOutWhy::CaseTwin);
                continue;
            }
            if path.split('/').count() > OUT_DEPTH || path.len() > OUT_PATH_BYTES {
                walked.leave(&path, LeftOutWhy::TooDeep);
                continue;
            }
            if kind.is_dir() {
                subfolders.push((entry.path(), path));
            } else {
                walked.files.push((path, meta.len()));
            }
        }
        stack.extend(subfolders.into_iter().rev());
    }
    Ok(walked)
}

/// Open `path` for reading without following a link, and check that what
/// was opened is a plain file.
fn open_plain(path: &Path) -> std::io::Result<std::fs::File> {
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.custom_flags(rustix::fs::OFlags::NOFOLLOW.bits().cast_signed());
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt as _;
        const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
        options.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    }
    let file = options.open(path)?;
    let meta = file.metadata()?;
    if !meta.is_file() || is_link(&meta) {
        return Err(std::io::Error::other("not a plain file"));
    }
    Ok(file)
}

/// The kept file made plain data: `0o644` on Unix, not read-only on Windows.
/// `meta` is the file's own, just read: on Windows nothing is written for a
/// file that is not read-only (the 2026-11-20 witness kept 2,005 files on
/// NTFS in 571 ms, one metadata read a file of it this).
fn make_plain(path: &Path, meta: &std::fs::Metadata) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let _ = meta;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o644))
    }
    #[cfg(not(unix))]
    {
        let mut permissions = meta.permissions();
        if permissions.readonly() {
            #[allow(
                clippy::permissions_set_readonly_false,
                reason = "Windows: clears the read-only attribute, which is what is meant"
            )]
            permissions.set_readonly(false);
            std::fs::set_permissions(path, permissions)?;
        }
        Ok(())
    }
}

/// Pass two: each file moved, or copied without following a link, into
/// `into`, its folders made by knobas.
fn write(
    out: &Path,
    into: &Path,
    walked: &mut Walked,
    progress: &Progress,
    cancel: &CancellationToken,
    caps: Caps,
    copy_only: bool,
) -> Result<Manifest, KeepError> {
    let total = u32::try_from(walked.files.len()).unwrap_or(u32::MAX);
    progress(0, total);
    let mut made_dirs: HashSet<String> = HashSet::new();
    let mut kept: Vec<MadeFile> = Vec::with_capacity(walked.files.len());
    let mut bytes: u64 = 0;
    let files = std::mem::take(&mut walked.files);
    for (n, (rel, _)) in files.iter().enumerate() {
        if cancel.is_cancelled() {
            return Err(KeepError::Stopped);
        }
        let on_disk = |base: &Path| rel.split('/').fold(base.to_path_buf(), |at, s| at.join(s));
        let (src, dst) = (on_disk(out), on_disk(into));
        if let Some((up, _)) = rel.rsplit_once('/')
            && !made_dirs.contains(up)
        {
            std::fs::create_dir_all(on_disk(into).parent().unwrap_or(into))
                .map_err(|e| KeepError::Io(format!("the folder {up} could not be made: {e}")))?;
            made_dirs.insert(up.to_owned());
        }
        let moved = !copy_only && std::fs::rename(&src, &dst).is_ok();
        if !moved {
            let copied = open_plain(&src).and_then(|mut from| {
                let mut to = std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&dst)?;
                std::io::copy(&mut from, &mut to).map(|_| ())
            });
            if copied.is_err() {
                // Gone, swapped for a link or unreadable since the walk.
                let _ = std::fs::remove_file(&dst);
                walked.leave(rel, LeftOutWhy::Link);
                continue;
            }
        }
        // What landed must be a plain file: a link swapped in after the walk
        // is moved as a link by the rename.
        let landed = std::fs::symlink_metadata(&dst);
        let plain = landed.as_ref().is_ok_and(|m| m.is_file() && !is_link(m));
        if !plain {
            if landed.as_ref().is_ok_and(|m| m.is_dir() && !is_link(m)) {
                let _ = std::fs::remove_dir_all(&dst);
            } else {
                let _ = std::fs::remove_file(&dst);
            }
            walked.leave(rel, LeftOutWhy::Link);
            continue;
        }
        let Ok(landed) = landed else { continue };
        make_plain(&dst, &landed)
            .map_err(|e| KeepError::Io(format!("{rel} could not be made plain: {e}")))?;
        let size = landed.len();
        bytes = bytes.saturating_add(size);
        if bytes > caps.bytes {
            return Err(KeepError::TooBig {
                files: files.len() as u64,
                bytes,
            });
        }
        kept.push(MadeFile {
            path: rel.clone(),
            size,
        });
        progress(u32::try_from(n + 1).unwrap_or(u32::MAX), total);
    }
    if kept.is_empty() {
        return Err(KeepError::Nothing);
    }
    Ok(Manifest {
        files: kept,
        bytes,
        left_out: std::mem::take(&mut walked.left_out),
        left_out_total: walked.left_out_total,
    })
}

/// The kept file `path` of `made`, at most `cap + 1` bytes of it: by the
/// manifest alone, every folder on the way checked to be a plain folder and
/// the file opened without following a link.
///
/// # Errors
///
/// [`ReadError::NotFound`] for a path the manifest does not hold, or one no
/// longer a plain file; [`ReadError::Io`] for a read that failed.
pub fn read(made: &Made, path: &str, cap: u64) -> Result<Vec<u8>, ReadError> {
    if made.file(path).is_none() {
        return Err(ReadError::NotFound);
    }
    let mut at = made.dir.clone();
    let segments: Vec<&str> = path.split('/').collect();
    for segment in &segments[..segments.len() - 1] {
        at.push(segment);
        let plain_dir = std::fs::symlink_metadata(&at).is_ok_and(|m| m.is_dir() && !is_link(&m));
        if !plain_dir {
            return Err(ReadError::NotFound);
        }
    }
    at.push(segments[segments.len() - 1]);
    let file = open_plain(&at).map_err(|error| match error.kind() {
        std::io::ErrorKind::NotFound => ReadError::NotFound,
        _ if error.to_string() == "not a plain file" => ReadError::NotFound,
        // `O_NOFOLLOW` on a link: ELOOP.
        _ if error
            .raw_os_error()
            .is_some_and(|code| code == 40 || code == 62) =>
        {
            ReadError::NotFound
        }
        _ => ReadError::Io(error.to_string()),
    })?;
    let mut bytes = Vec::new();
    file.take(cap.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|e| ReadError::Io(e.to_string()))?;
    Ok(bytes)
}

/// Whether every folder of `segments` under `made`'s top is a plain folder:
/// no link, no reparse point (what [`read`] walks before its file).
fn plain_folders(made: &Made, segments: &[&str]) -> bool {
    let mut at = made.dir.clone();
    for segment in segments {
        at.push(segment);
        if !std::fs::symlink_metadata(&at).is_ok_and(|m| m.is_dir() && !is_link(&m)) {
            return false;
        }
    }
    true
}

/// The kept file at `path` (from the generated folder's top) on disk, checked
/// as [`read`] checks it: the manifest holds it, every folder on the way is a
/// plain folder, and it is a plain file itself. What a command is handed for
/// a generated file (ADR-0060, amended 2026-11-25).
///
/// # Errors
///
/// [`ReadError::NotFound`] for anything else.
pub fn checked_file(made: &Made, path: &str) -> Result<PathBuf, ReadError> {
    if made.file(path).is_none() {
        return Err(ReadError::NotFound);
    }
    let segments: Vec<&str> = path.split('/').collect();
    let (_, folders) = segments.split_last().ok_or(ReadError::NotFound)?;
    if !plain_folders(made, folders) {
        return Err(ReadError::NotFound);
    }
    let at = segments.iter().fold(made.dir.clone(), |at, s| at.join(s));
    match std::fs::symlink_metadata(&at) {
        Ok(meta) if meta.is_file() && !is_link(&meta) => Ok(at),
        _ => Err(ReadError::NotFound),
    }
}

/// The kept folder at `path` (`""` the top) on disk: one the manifest holds a
/// file under, every folder on the way plain.
///
/// # Errors
///
/// [`ReadError::NotFound`] for a folder it holds no file in, or a link.
pub fn checked_folder(made: &Made, path: &str) -> Result<PathBuf, ReadError> {
    if path.is_empty() {
        return Ok(made.dir.clone());
    }
    let under = format!("{path}/");
    if !made
        .manifest
        .files
        .iter()
        .any(|f| f.path.starts_with(&under))
    {
        return Err(ReadError::NotFound);
    }
    let segments: Vec<&str> = path.split('/').collect();
    if !plain_folders(made, &segments) {
        return Err(ReadError::NotFound);
    }
    Ok(segments.iter().fold(made.dir.clone(), |at, s| at.join(s)))
}

/// What one kept file is on disk now: its size and modified time, `None`
/// where it is gone or no longer a plain file.
pub type Stamp = Vec<Option<(u64, std::time::SystemTime)>>;

/// The manifest's files as they are on disk now, in the manifest's order.
#[must_use]
pub fn stamp(made: &Made) -> Stamp {
    made.manifest
        .files
        .iter()
        .map(|file| {
            let at = file
                .path
                .split('/')
                .fold(made.dir.clone(), |at, s| at.join(s));
            std::fs::symlink_metadata(&at)
                .ok()
                .filter(|m| m.is_file() && !is_link(m))
                .map(|m| (m.len(), m.modified().unwrap_or(std::time::UNIX_EPOCH)))
        })
        .collect()
}

/// The manifest's files whose stamp moved since `before`: changed, gone, or
/// swapped for something that is no plain file. A file a program added beside
/// them is no change: it is never drawn (Fable, 2026-11-25).
#[must_use]
pub fn changed_since(made: &Made, before: &Stamp) -> Vec<String> {
    stamp(made)
        .iter()
        .zip(before)
        .zip(&made.manifest.files)
        .filter(|((now, was), _)| now != was)
        .map(|(_, file)| file.path.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn none() -> Progress {
        Arc::new(|_, _| {})
    }

    fn put(base: &Path, rel: &str, body: &[u8]) {
        let path = base.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, body).unwrap();
    }

    fn paths(made: &Made) -> Vec<&str> {
        made.manifest
            .files
            .iter()
            .map(|f| f.path.as_str())
            .collect()
    }

    fn whys(made: &Made) -> Vec<(&str, LeftOutWhy)> {
        made.manifest
            .left_out
            .iter()
            .map(|l| (l.path.as_str(), l.why))
            .collect()
    }

    fn keep_it(out: &Path, into: &Path) -> Result<Made, KeepError> {
        keep(out, into, &none(), &CancellationToken::new())
    }

    /// The place: `out` the tool's, `into` knobas's (not made yet).
    fn place() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("out");
        std::fs::create_dir(&out).unwrap();
        let into = dir.path().join("kept");
        (dir, out, into)
    }

    #[cfg(unix)]
    #[test]
    fn a_link_is_left_out_and_never_followed() {
        let (_dir, out, into) = place();
        put(&out, "index.md", b"# hi");
        std::os::unix::fs::symlink("/etc/hosts", out.join("link.txt")).unwrap();
        std::os::unix::fs::symlink("/", out.join("dir")).unwrap();
        let made = keep_it(&out, &into).unwrap();
        assert_eq!(paths(&made), ["index.md"]);
        assert_eq!(
            whys(&made),
            [("dir", LeftOutWhy::Link), ("link.txt", LeftOutWhy::Link)]
        );
        assert!(!into.join("link.txt").exists() && !into.join("dir").exists());
    }

    #[test]
    fn a_git_folder_is_left_out_whole() {
        let (_dir, out, into) = place();
        put(&out, "a.md", b"a");
        put(&out, ".git/config", b"[core]\n\tfsmonitor = evil\n");
        put(&out, "sub/.GIT/HEAD", b"ref");
        put(&out, "sub/b.md", b"b");
        let made = keep_it(&out, &into).unwrap();
        assert_eq!(paths(&made), ["a.md", "sub/b.md"]);
        assert_eq!(
            whys(&made),
            [(".git", LeftOutWhy::Git), ("sub/.GIT", LeftOutWhy::Git)]
        );
        assert!(!into.join(".git").exists() && !into.join("sub/.GIT").exists());
    }

    #[cfg(unix)]
    #[test]
    fn names_windows_refuses_are_left_out_on_every_platform() {
        let (_dir, out, into) = place();
        put(&out, "ok.md", b"ok");
        for bad in ["a:b", "nul.txt", "end."] {
            put(&out, bad, b"x");
        }
        let made = keep_it(&out, &into).unwrap();
        assert_eq!(paths(&made), ["ok.md"]);
        assert_eq!(
            whys(&made),
            [
                ("a:b", LeftOutWhy::Name),
                ("end.", LeftOutWhy::Name),
                ("nul.txt", LeftOutWhy::Name)
            ]
        );
    }

    #[test]
    fn two_names_alike_but_for_case_keep_the_first() {
        let (_dir, out, into) = place();
        put(&out, "Readme.md", b"first");
        std::fs::write(out.join("README.md"), b"second").unwrap();
        let names: Vec<_> = std::fs::read_dir(&out).unwrap().flatten().collect();
        if names.len() < 2 {
            // The volume folds case: there is only one file.
            return;
        }
        let made = keep_it(&out, &into).unwrap();
        // Name-byte order: `README.md` before `Readme.md`.
        assert_eq!(paths(&made), ["README.md"]);
        assert_eq!(whys(&made), [("Readme.md", LeftOutWhy::CaseTwin)]);
    }

    #[test]
    fn more_files_than_the_cap_keeps_nothing_and_says_how_many() {
        let (_dir, out, into) = place();
        for n in 0..5 {
            put(&out, &format!("f{n}.txt"), b"12345");
        }
        let caps = Caps {
            files: 4,
            bytes: 1000,
        };
        let too_many = keep_with(&out, &into, &none(), &CancellationToken::new(), caps, false);
        assert_eq!(
            too_many.unwrap_err(),
            KeepError::TooBig {
                files: 5,
                bytes: 25
            }
        );
        assert!(!into.exists());
        let caps = Caps {
            files: 10,
            bytes: 20,
        };
        let too_big = keep_with(&out, &into, &none(), &CancellationToken::new(), caps, false);
        assert_eq!(
            too_big.unwrap_err(),
            KeepError::TooBig {
                files: 5,
                bytes: 25
            }
        );
        assert!(!into.exists());
    }

    #[cfg(unix)]
    #[test]
    fn nothing_but_links_is_no_output() {
        let (_dir, out, into) = place();
        std::os::unix::fs::symlink("/etc/hosts", out.join("a")).unwrap();
        std::fs::create_dir(out.join("empty")).unwrap();
        assert_eq!(keep_it(&out, &into).unwrap_err(), KeepError::Nothing);
        assert!(!into.exists());
    }

    #[cfg(unix)]
    #[test]
    fn a_kept_file_is_plain_data() {
        use std::os::unix::fs::PermissionsExt as _;
        let (_dir, out, into) = place();
        put(&out, "run.sh", b"#!/bin/sh\necho hi\n");
        std::fs::set_permissions(out.join("run.sh"), std::fs::Permissions::from_mode(0o755))
            .unwrap();
        let made = keep_it(&out, &into).unwrap();
        let mode = std::fs::metadata(into.join("run.sh"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o644);
        assert_eq!(made.manifest.files[0].size, 18);
    }

    #[cfg(unix)]
    #[test]
    fn a_read_takes_only_what_the_manifest_holds() {
        let (dir, out, into) = place();
        put(&out, "a.md", b"aaa");
        put(&out, "sub/b.md", b"bbb");
        std::fs::write(dir.path().join("x"), b"outside").unwrap();
        let made = keep_it(&out, &into).unwrap();
        assert_eq!(read(&made, "a.md", 100).unwrap(), b"aaa");
        assert_eq!(read(&made, "a.md", 1).unwrap(), b"aa", "cap + 1 bytes");
        assert_eq!(read(&made, "../x", 100), Err(ReadError::NotFound));
        assert_eq!(read(&made, "/etc/hosts", 100), Err(ReadError::NotFound));
        assert_eq!(read(&made, "sub", 100), Err(ReadError::NotFound));
        // A file swapped for a link after keeping, and a folder swapped for one.
        std::fs::remove_file(into.join("a.md")).unwrap();
        std::os::unix::fs::symlink(dir.path().join("x"), into.join("a.md")).unwrap();
        assert_eq!(read(&made, "a.md", 100), Err(ReadError::NotFound));
        std::fs::rename(into.join("sub"), dir.path().join("sub-real")).unwrap();
        std::os::unix::fs::symlink(dir.path().join("sub-real"), into.join("sub")).unwrap();
        assert_eq!(read(&made, "sub/b.md", 100), Err(ReadError::NotFound));
    }

    #[test]
    fn a_stop_mid_keeping_leaves_no_folder() {
        let (_dir, out, into) = place();
        for n in 0..20 {
            put(&out, &format!("d/f{n}.txt"), b"x");
        }
        let cancel = CancellationToken::new();
        let stop = cancel.clone();
        let progress: Progress = Arc::new(move |done, _| {
            if done == 5 {
                stop.cancel();
            }
        });
        let stopped = keep(&out, &into, &progress, &cancel);
        assert_eq!(stopped.unwrap_err(), KeepError::Stopped);
        assert!(!into.exists());
    }

    #[test]
    fn the_copy_path_keeps_the_same_tree_as_the_rename_path() {
        let build = |out: &Path| {
            put(out, "README.md", b"# r");
            put(out, "a.txt", b"a");
            put(out, "a/b/c.md", b"c");
            put(out, "img/x.png", b"\x89PNG");
            put(out, ".git/HEAD", b"x");
        };
        let (_d1, out1, into1) = place();
        build(&out1);
        let renamed = keep_it(&out1, &into1).unwrap();
        let (_d2, out2, into2) = place();
        build(&out2);
        let copied = keep_with(
            &out2,
            &into2,
            &none(),
            &CancellationToken::new(),
            Caps::default(),
            true,
        )
        .unwrap();
        assert_eq!(renamed.manifest, copied.manifest);
        // Sorted by path bytes: `a.txt` before `a/b/c.md`.
        assert_eq!(
            paths(&renamed),
            ["README.md", "a.txt", "a/b/c.md", "img/x.png"]
        );
        assert_eq!(read(&copied, "a/b/c.md", 10).unwrap(), b"c");
        // The rename moved; the copy left the source.
        assert!(!out1.join("a.txt").exists() && out2.join("a.txt").exists());
    }

    #[test]
    fn a_long_left_out_path_is_cut_and_its_control_characters_replaced() {
        let long = "x".repeat(300);
        let cut = shown(&long);
        assert_eq!(cut.chars().count(), LEFT_OUT_CHARS + 1);
        assert_eq!(shown("a\u{7}b"), "a\u{fffd}b");
    }
}
