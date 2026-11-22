//! A Java archive on the Code page (ux19, `docs/decisions/2026-11-18-a-jar-is-drawn-as-its-contents.md`): read once,
//! kept as a file, listed from its central directory, and read an entry at a time as the reader opens them.
//!
//! **One read of the archive.** Its bytes come from the clone at the file's commit (ADR-0041) or else from the
//! source, read past the 50 MiB byte cap up to [`ARCHIVE_CAP`] and outside the read cache (a 200 MB JAR held there
//! would push out every view). The listing is made from those bytes at once and answered; the bytes are then written
//! to a spool file in the background and dropped from memory, so what stays in memory per archive is its listing.
//! Later reads (an entry, a class, a nested JAR) seek in that file, through [`knobas_jar`]'s checks.
//!
//! **Kept by what the read is at.** An archive read at a whole sha is the same bytes every time: a second visit
//! answers its listing with no read. One read at a branch is read again when opened again. At most
//! [`KEEP`] archives are kept, the least lately used dropped with its nested ones and its spool file.

pub mod java;

use std::collections::HashMap;
use std::io::{Read, Seek, Write as _};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use knobas_jar::{ArchiveIndex, ClassOutline, Directory, Inner, ZipError, zip};
use sqlx::PgPool;

use crate::IpcError;

/// The most an archive read for this view may be.
pub const ARCHIVE_CAP: u64 = 1024 * 1024 * 1024;
/// The most one entry opened in the view unpacks to: the byte read's cap.
pub const ENTRY_CAP: u64 = knobas_source::view::BYTES_CAP;
/// How many archives (each with its nested ones) are kept.
pub const KEEP: usize = 8;

/// A spooled archive's file, removed when nothing holds it any more.
#[derive(Debug)]
struct Spool(PathBuf);

impl Drop for Spool {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Where an archive's bytes are: in memory until the spool file is written, then only there.
#[derive(Debug, Default)]
struct Backing {
    mem: Mutex<Option<Arc<Vec<u8>>>>,
    file: Mutex<Option<Arc<Spool>>>,
}

/// An `Arc<Vec<u8>>` read as a slice, so a `Cursor` over it is `Read + Seek` without a copy.
struct Shared(Arc<Vec<u8>>);
impl AsRef<[u8]> for Shared {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

trait ReadSeek: Read + Seek + Send {}
impl<T: Read + Seek + Send> ReadSeek for T {}

impl Backing {
    fn reader(&self) -> std::io::Result<Box<dyn ReadSeek>> {
        let file = self
            .file
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        if let Some(spool) = file {
            return Ok(Box::new(std::io::BufReader::with_capacity(
                64 * 1024,
                std::fs::File::open(&spool.0)?,
            )));
        }
        let mem = self
            .mem
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        match mem {
            Some(bytes) => Ok(Box::new(std::io::Cursor::new(Shared(bytes)))),
            None => Err(std::io::Error::other("the archive is no longer kept")),
        }
    }
}

/// One archive the view has open: a top-level one or one nested in it.
struct Opened {
    backing: Arc<Backing>,
    dir: Directory,
    /// Its file name (`app.jar`, `spring-core-6.1.2.jar`).
    name: String,
    index: Arc<ArchiveIndex>,
}

#[derive(Default)]
struct Store {
    dir: Option<PathBuf>,
    by_handle: HashMap<String, Arc<Opened>>,
    /// A read at a whole sha's key, to its handle.
    by_key: HashMap<String, String>,
    /// Top-level handles, the most lately used last.
    order: Vec<String>,
    next: u64,
}

fn store() -> &'static Mutex<Store> {
    static STORE: OnceLock<Mutex<Store>> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(Store::default()))
}

fn lock() -> std::sync::MutexGuard<'static, Store> {
    store()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// The folder spool files go in: one of this process's own under [`SPOOL_ROOT`], so two knobas on one machine (the
/// app and a windowless backend, or two of either) never empty or overwrite each other's. One shared folder, emptied
/// by each process at its first use and filled with files numbered per process, read one process's spool file as
/// another's: "does not match its checksum: the archive is corrupt" (the ux23 gate's loaded suite, 2026-11-20).
fn spool_dir() -> PathBuf {
    let mut s = lock();
    if let Some(dir) = &s.dir {
        return dir.clone();
    }
    let root = std::env::temp_dir().join(SPOOL_ROOT);
    sweep_spools(&root);
    let at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let dir = root.join(format!("{}-{at}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    s.dir = Some(dir.clone());
    dir
}

/// Where every process's spool folder is made. Not `knobas-archives`, the one folder of the knobas before it, which
/// such a knobas still empties whole at its first use.
const SPOOL_ROOT: &str = "knobas-archive-spool";

/// Remove what an earlier knobas left (a quit, a crash): each spool folder under `root` last written more than twelve
/// hours ago, as the folder renders' scratch is swept (`folder_render::copy::sweep`), and the earlier knobas'
/// `knobas-archives` once it is as old.
fn sweep_spools(root: &Path) {
    let old = std::time::SystemTime::now() - std::time::Duration::from_secs(12 * 3600);
    let stale = |path: &Path| {
        std::fs::metadata(path)
            .and_then(|m| m.modified())
            .is_ok_and(|at| at < old)
    };
    let legacy = std::env::temp_dir().join("knobas-archives");
    if stale(&legacy) {
        let _ = std::fs::remove_dir_all(&legacy);
    }
    for entry in std::fs::read_dir(root).into_iter().flatten().flatten() {
        if stale(&entry.path()) {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

/// `size` bytes in MiB, to one place where it is not whole.
fn mebibytes(size: u64) -> String {
    #[allow(clippy::cast_precision_loss)]
    let mib = size as f64 / (1024.0 * 1024.0);
    if mib.fract() == 0.0 {
        format!("{mib:.0} MiB")
    } else {
        format!("{mib:.1} MiB")
    }
}

/// A number for each spool file of this run.
static SPOOLED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn gone(handle: &str) -> IpcError {
    IpcError::not_found(format!("archive {handle} is no longer open; open it again"))
}

fn opened(handle: &str) -> Result<Arc<Opened>, IpcError> {
    let mut s = lock();
    let found = s
        .by_handle
        .get(handle)
        .cloned()
        .ok_or_else(|| gone(handle))?;
    let top = handle.split('/').next().unwrap_or(handle).to_owned();
    if let Some(at) = s.order.iter().position(|h| *h == top) {
        let h = s.order.remove(at);
        s.order.push(h);
    }
    Ok(found)
}

fn zip_error(error: &ZipError) -> IpcError {
    match error {
        ZipError::Io(io) => IpcError::internal(format!("reading the archive: {io}")),
        other => IpcError::invalid(other.to_string()),
    }
}

/// Keeps `opened` under a new handle: a top-level one at the end of the order, dropping the oldest past [`KEEP`].
fn keep(opened: Opened, parent: Option<&str>, key: Option<String>) -> Arc<ArchiveIndex> {
    let mut s = lock();
    s.next += 1;
    let handle = match parent {
        Some(p) => format!("{p}/{}", s.next),
        None => format!("a{}", s.next),
    };
    let mut index = (*opened.index).clone();
    index.handle.clone_from(&handle);
    let index = Arc::new(index);
    let opened = Opened {
        index: Arc::clone(&index),
        ..opened
    };
    s.by_handle.insert(handle.clone(), Arc::new(opened));
    if parent.is_none() {
        if let Some(key) = key {
            s.by_key.insert(key, handle.clone());
        }
        s.order.push(handle);
        while s.order.len() > KEEP {
            let old = s.order.remove(0);
            let prefix = format!("{old}/");
            s.by_handle
                .retain(|h, _| *h != old && !h.starts_with(&prefix));
            s.by_key.retain(|_, h| *h != old && !h.starts_with(&prefix));
        }
    }
    index
}

/// Lists `bytes` as an archive named `name`, keeps them, and spools them to disk behind the answer.
async fn take_in(
    bytes: Vec<u8>,
    name: String,
    key: Option<String>,
) -> Result<Arc<ArchiveIndex>, IpcError> {
    let bytes = Arc::new(bytes);
    let backing = Arc::new(Backing {
        mem: Mutex::new(Some(Arc::clone(&bytes))),
        file: Mutex::new(None),
    });
    let listed = {
        let bytes = Arc::clone(&bytes);
        let name = name.clone();
        tokio::task::spawn_blocking(move || -> Result<(Directory, ArchiveIndex), ZipError> {
            let mut c = std::io::Cursor::new(Shared(bytes));
            let len = c.get_ref().0.len() as u64;
            let dir = zip::read_directory(&mut c, 0, len)?;
            let index = knobas_jar::index(&mut c, &dir, &name);
            Ok((dir, index))
        })
        .await
        .map_err(IpcError::internal)?
        .map_err(|e| zip_error(&e))?
    };
    let index = keep(
        Opened {
            backing: Arc::clone(&backing),
            dir: listed.0,
            name,
            index: Arc::new(listed.1),
        },
        None,
        key,
    );
    let spool = spool_dir().join(format!("{}.jar", index.handle));
    // Behind the answer: the bytes to a file, then out of memory.
    tokio::task::spawn_blocking(move || {
        if std::fs::write(&spool, bytes.as_slice()).is_ok() {
            *backing
                .file
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(Arc::new(Spool(spool)));
            *backing
                .mem
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
        }
    });
    Ok(index)
}

/// `bytes` listed as the archive `name`, kept under a new handle (a test's door, and the fake's).
///
/// # Errors
///
/// `invalid` for bytes that are no archive, with why.
pub async fn open_bytes(bytes: Vec<u8>, name: &str) -> Result<ArchiveIndex, IpcError> {
    take_in(bytes, name.to_owned(), None)
        .await
        .map(|i| (*i).clone())
}

/// An archive a folder renderer made (ADR-0046), listed: `read` hands its bytes, read only where no listing is kept under
/// `key`. A generated folder is never rewritten, so its file is the same bytes for as long as its token names it.
///
/// # Errors
///
/// As `read`, and `invalid` for bytes that are no archive, with why.
pub async fn open_kept<F>(key: String, name: &str, read: F) -> Result<ArchiveIndex, IpcError>
where
    F: std::future::Future<Output = Result<Vec<u8>, IpcError>>,
{
    let kept = lock().by_key.get(&key).cloned();
    if let Some(handle) = kept
        && let Ok(o) = opened(&handle)
    {
        return Ok((*o.index).clone());
    }
    let bytes = read.await?;
    take_in(bytes, name.to_owned(), Some(key))
        .await
        .map(|i| (*i).clone())
}

/// Opens the archive at `path` in `entity_id`'s tree, at `commit` (a file block's, handed back) or the entity's own
/// reference: its listing, under a handle the other reads name it by.
///
/// # Errors
///
/// `invalid` for a file over [`ARCHIVE_CAP`] or one that is no archive (saying why), `not_found` for no file there,
/// and otherwise as the byte read.
pub async fn open(
    pool: &PgPool,
    secrets: &Arc<dyn knobas_secrets::SecretStore>,
    registry: &Arc<dyn knobas_sync::scheduler::AdapterRegistry>,
    live: &crate::sources::live::LiveSources,
    entity_id: &str,
    path: &str,
    commit: Option<&str>,
) -> Result<ArchiveIndex, IpcError> {
    let fixed = commit.filter(|c| knobas_core::checkout::open::is_full_sha(c));
    let key = fixed.map(|c| format!("{entity_id}\u{1f}{c}\u{1f}{path}"));
    if let Some(key) = &key {
        let kept = lock().by_key.get(key).cloned();
        if let Some(handle) = kept
            && let Ok(o) = opened(&handle)
        {
            return Ok((*o.index).clone());
        }
    }
    let name = path.rsplit('/').next().unwrap_or(path).to_owned();
    let too_large = |size: u64| {
        IpcError::invalid(format!(
            "{path} is {}; knobas reads an archive up to {}",
            mebibytes(size),
            mebibytes(ARCHIVE_CAP)
        ))
    };
    // The working tree's archive (ux29, the Files tab on *Working tree*) or the index's (Local changes' staged half, a
    // comparison's side): read from the checkout, never the source, and kept by no key, so a rebuilt JAR is read again.
    if let Some(end) = commit
        .filter(|c| *c == crate::checkout::working::END || *c == crate::checkout::working::INDEX)
    {
        use crate::checkout::working::Bytes;
        return match crate::checkout::working_file_bytes(pool, entity_id, end, path, ARCHIVE_CAP)
            .await?
        {
            Bytes::Read(bytes) => take_in(bytes, name, None).await.map(|i| (*i).clone()),
            Bytes::TooLarge(size) => Err(too_large(size)),
            Bytes::NotThere => Err(IpcError::not_found(format!(
                "git lists no file {path} in {}",
                if end == crate::checkout::working::INDEX {
                    "the index"
                } else {
                    "the working tree"
                }
            ))),
        };
    }
    if let Some(commit) = commit {
        match crate::checkout::local_file_bytes(pool, entity_id, commit, path, ARCHIVE_CAP).await {
            Some(Ok(bytes)) => return take_in(bytes, name, key).await.map(|i| (*i).clone()),
            Some(Err(size)) => return Err(too_large(size)),
            None => {}
        }
    }
    let lent = live
        .source(pool, secrets, registry, entity_id)
        .await
        .map_err(|e| crate::sources::to_ipc(&e, None))?;
    let read = lent
        .source
        .file_bytes(entity_id, path, commit, ARCHIVE_CAP)
        .await
        .map_err(|error| match error {
            knobas_source::SourceError::Protocol {
                status: Some(404), ..
            } => IpcError::not_found(format!("{entity_id} holds no file {path}")),
            other => crate::sources::to_ipc(&crate::sources::SourcesError::Source(other), None),
        })?;
    match read {
        Some(knobas_source::view::FileBytes::Read(bytes)) => {
            take_in(bytes, name, key).await.map(|i| (*i).clone())
        }
        Some(knobas_source::view::FileBytes::TooLarge { size }) => Err(too_large(size)),
        None => Err(IpcError::invalid(format!(
            "{entity_id}'s source reads no file bytes"
        ))),
    }
}

/// Runs `work` on an open archive's reader off the async threads.
async fn with_reader<T: Send + 'static>(
    handle: &str,
    work: impl FnOnce(&mut dyn ReadSeek, &Opened) -> Result<T, IpcError> + Send + 'static,
) -> Result<T, IpcError> {
    let o = opened(handle)?;
    tokio::task::spawn_blocking(move || {
        let mut r = o
            .backing
            .reader()
            .map_err(|e| IpcError::internal(format!("reading the archive: {e}")))?;
        work(&mut *r, &o)
    })
    .await
    .map_err(IpcError::internal)?
}

fn entry_of<'a>(o: &'a Opened, name: &str) -> Result<&'a knobas_jar::Entry, IpcError> {
    knobas_jar::entry(&o.dir, name)
        .ok_or_else(|| IpcError::not_found(format!("{} holds no {name}", o.name)))
}

/// The archive `entry` inside the open archive `handle`: its listing, under a handle of its own. One stored (a
/// Spring Boot fat JAR's libraries) is read in place; one deflated (a WAR's) is unpacked to a spool file first.
///
/// # Errors
///
/// `not_found` for a handle no longer kept or an entry not in it, `invalid` for one refused (its name, a bomb, its
/// size) or that is no archive.
pub async fn nested(handle: &str, entry: &str) -> Result<ArchiveIndex, IpcError> {
    let key = format!("{handle}\u{1f}{entry}");
    let kept = lock().by_key.get(&key).cloned();
    if let Some(h) = kept
        && let Ok(o) = opened(&h)
    {
        return Ok((*o.index).clone());
    }
    let wanted = entry.to_owned();
    let spool = spool_dir();
    let parent_handle = handle.to_owned();
    let made = with_reader(handle, move |r, o| {
        let e = entry_of(o, &wanted)?;
        let name = wanted.rsplit('/').next().unwrap_or(&wanted).to_owned();
        let (backing, base, len) =
            match knobas_jar::inner(r, &o.dir, e).map_err(|e| zip_error(&e))? {
                Inner::InPlace { base, len } => (Arc::clone(&o.backing), base, len),
                Inner::Packed => {
                    let n = SPOOLED.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    let path = spool.join(format!("{}-n{n}.jar", parent_handle.replace('/', "-")));
                    let file = std::fs::File::create(&path)
                        .map_err(|e| IpcError::internal(format!("unpacking {name}: {e}")))?;
                    let spooled = Spool(path.clone());
                    let mut w = std::io::BufWriter::new(file);
                    let n = zip::copy_entry(r, &o.dir, e, knobas_jar::NESTED_CAP, &mut w)
                        .map_err(|e| zip_error(&e))?;
                    w.flush()
                        .map_err(|e| IpcError::internal(format!("unpacking {name}: {e}")))?;
                    drop(w);
                    (
                        Arc::new(Backing {
                            mem: Mutex::new(None),
                            file: Mutex::new(Some(Arc::new(spooled))),
                        }),
                        0,
                        n,
                    )
                }
            };
        let mut inner_r = backing
            .reader()
            .map_err(|e| IpcError::internal(format!("reading {name}: {e}")))?;
        let dir = zip::read_directory(&mut inner_r, base, len).map_err(|e| zip_error(&e))?;
        let index = knobas_jar::index(&mut inner_r, &dir, &name);
        Ok(Opened {
            backing,
            dir,
            name,
            index: Arc::new(index),
        })
    })
    .await?;
    let index = keep(made, Some(handle), None);
    lock().by_key.insert(key, index.handle.clone());
    Ok((*index).clone())
}

/// One entry's unpacked bytes, at most [`ENTRY_CAP`].
///
/// # Errors
///
/// As [`nested`].
pub async fn entry_bytes(handle: &str, entry: &str) -> Result<Vec<u8>, IpcError> {
    let wanted = entry.to_owned();
    with_reader(handle, move |r, o| {
        let e = entry_of(o, &wanted)?;
        zip::read_entry(r, &o.dir, e, ENTRY_CAP).map_err(|e| zip_error(&e))
    })
    .await
}

/// A class's outline.
///
/// # Errors
///
/// As [`entry_bytes`], and `invalid` for bytes that are no class file.
pub async fn class(handle: &str, entry: &str) -> Result<ClassOutline, IpcError> {
    let bytes = entry_bytes(handle, entry).await?;
    knobas_jar::class::outline(&bytes).map_err(|e| IpcError::invalid(format!("{entry}: {e}")))
}

/// A top-level class and its nested classes (`Outer$*.class` beside it), each `(name, bytes)`, sorted by name, at
/// most `cap` bytes together: what the decompiler is handed for `entry` (an inner class decompiles as its outer).
///
/// # Errors
///
/// As [`entry_bytes`]; `invalid` past `cap`.
pub async fn class_family(
    handle: &str,
    entry: &str,
    cap: u64,
) -> Result<(String, Vec<(String, Vec<u8>)>), IpcError> {
    let wanted = entry.to_owned();
    with_reader(handle, move |r, o| {
        let folder_end = wanted.rfind('/').map_or(0, |i| i + 1);
        let file = &wanted[folder_end..];
        let outer = file
            .split('$')
            .next()
            .unwrap_or(file)
            .trim_end_matches(".class");
        let top = format!("{}{outer}.class", &wanted[..folder_end]);
        let prefix = format!("{}{outer}$", &wanted[..folder_end]);
        let mut family: Vec<&knobas_jar::Entry> = o
            .dir
            .entries
            .iter()
            .filter(|e| {
                (e.name == top || (e.name.starts_with(&prefix) && e.name.ends_with(".class")))
                    && !e.unsafe_name
            })
            .collect();
        family.sort_by(|a, b| a.name.cmp(&b.name));
        family.dedup_by(|a, b| a.name == b.name);
        if !family.iter().any(|e| e.name == top) {
            return Err(IpcError::not_found(format!("{} holds no {top}", o.name)));
        }
        let mut total = 0u64;
        let mut out = Vec::new();
        for e in family {
            total += e.size;
            if total > cap {
                return Err(IpcError::invalid(format!(
                    "{top} and its nested classes are over {}",
                    mebibytes(cap)
                )));
            }
            let bytes = zip::read_entry(r, &o.dir, e, cap).map_err(|e| zip_error(&e))?;
            let base = e.name[folder_end..].to_owned();
            out.push((base, bytes));
        }
        Ok((top, out))
    })
    .await
}

/// Writes the archive (`entry` `None`) or one entry of it into `dir` under a free name; answers the file's path.
///
/// # Errors
///
/// As [`entry_bytes`], and `internal` where the file cannot be written.
pub async fn download(handle: &str, entry: Option<&str>, dir: &Path) -> Result<String, IpcError> {
    let o = opened(handle)?;
    let name = crate::downloads::safe_name(entry.unwrap_or(&o.name));
    let (path, file) = crate::downloads::create_free(dir, &name)
        .await
        .map_err(|(p, e)| IpcError::internal(format!("{}: {e}", p.display())))?;
    let wanted = entry.map(str::to_owned);
    let target = path.clone();
    let written = tokio::task::spawn_blocking(move || -> Result<(), IpcError> {
        let mut file = std::io::BufWriter::new(
            file.try_into_std()
                .map_err(|_| IpcError::internal("the download's file"))?,
        );
        let mut r = o
            .backing
            .reader()
            .map_err(|e| IpcError::internal(format!("reading the archive: {e}")))?;
        match wanted {
            Some(name) => {
                let e = entry_of(&o, &name)?;
                zip::copy_entry(&mut r, &o.dir, e, u64::MAX, &mut file)
                    .map_err(|e| zip_error(&e))?;
            }
            None => {
                r.seek(std::io::SeekFrom::Start(o.dir.base))
                    .map_err(IpcError::internal)?;
                std::io::copy(&mut r.take(o.dir.len), &mut file).map_err(IpcError::internal)?;
            }
        }
        file.flush().map_err(IpcError::internal)
    })
    .await
    .map_err(IpcError::internal)?;
    if let Err(error) = written {
        let _ = tokio::fs::remove_file(&target).await;
        return Err(error);
    }
    Ok(path.display().to_string())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn tiny_jar(entries: &[(&str, &[u8])]) -> Vec<u8> {
        // Stored entries: enough for the store's own paths; `knobas-jar`'s tests hold the reader to real archives.
        let mut out = Vec::new();
        let mut central = Vec::new();
        for (name, bytes) in entries {
            let local = out.len() as u32;
            let crc = crc32(bytes);
            out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
            out.extend_from_slice(&[20, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
            out.extend_from_slice(&crc.to_le_bytes());
            out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
            out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
            out.extend_from_slice(&(name.len() as u16).to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(name.as_bytes());
            out.extend_from_slice(bytes);
            central.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
            central.extend_from_slice(&[20, 0, 20, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
            central.extend_from_slice(&crc.to_le_bytes());
            central.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
            central.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
            central.extend_from_slice(&(name.len() as u16).to_le_bytes());
            central.extend_from_slice(&[0; 12]);
            central.extend_from_slice(&local.to_le_bytes());
            central.extend_from_slice(name.as_bytes());
        }
        let at = out.len() as u32;
        out.extend_from_slice(&central);
        out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
        out.extend_from_slice(&[0; 4]);
        out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
        out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
        out.extend_from_slice(&(central.len() as u32).to_le_bytes());
        out.extend_from_slice(&at.to_le_bytes());
        out.extend_from_slice(&[0, 0]);
        out
    }

    fn crc32(bytes: &[u8]) -> u32 {
        let mut crc = !0u32;
        for b in bytes {
            crc ^= u32::from(*b);
            for _ in 0..8 {
                crc = if crc & 1 == 1 {
                    (crc >> 1) ^ 0xEDB8_8320
                } else {
                    crc >> 1
                };
            }
        }
        !crc
    }

    /// A spool folder is this process's own, and what an earlier knobas left is swept only once it is twelve hours old:
    /// another knobas running beside this one keeps its spool files.
    #[cfg(unix)]
    #[test]
    fn a_spool_folder_is_the_process_own_and_only_a_stale_one_is_swept() {
        let root = tempfile::tempdir().unwrap();
        let (stale, beside) = (root.path().join("1-1"), root.path().join("2-2"));
        for dir in [&stale, &beside] {
            std::fs::create_dir_all(dir).unwrap();
            std::fs::write(dir.join("1.jar"), b"spooled").unwrap();
        }
        let long_ago = std::time::SystemTime::now() - std::time::Duration::from_secs(13 * 3600);
        std::fs::File::open(&stale)
            .unwrap()
            .set_modified(long_ago)
            .unwrap();
        sweep_spools(root.path());
        assert!(!stale.exists(), "an earlier knobas' folder goes");
        assert_eq!(std::fs::read(beside.join("1.jar")).unwrap(), b"spooled");
        let ours = spool_dir();
        let name = ours.file_name().unwrap().to_string_lossy().into_owned();
        assert!(
            name.starts_with(&format!("{}-", std::process::id())),
            "{name}"
        );
        assert_eq!(ours.parent().unwrap().file_name().unwrap(), SPOOL_ROOT);
    }

    /// The store is the process's: its tests take turns, so one's evictions do not reach another's handles.
    pub(crate) static SERIAL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

    #[tokio::test]
    async fn an_archive_opens_reads_its_entries_and_its_nested_jar() {
        let _turn = SERIAL.lock().await;
        let lib = tiny_jar(&[("x/Y.txt", b"inside")]);
        let jar = tiny_jar(&[("a.txt", b"hello"), ("BOOT-INF/lib/lib-1.0.jar", &lib)]);
        let ix = open_bytes(jar, "app.jar").await.unwrap();
        assert_eq!(ix.names, "BOOT-INF/lib/lib-1.0.jar\na.txt");
        assert_eq!(ix.kind, "boot-jar");
        // Each entry's CRC as declared, aligned with its name: what a comparison tells a changed entry by.
        assert_eq!(ix.crcs, vec![crc32(&lib), crc32(b"hello")]);
        assert_eq!(entry_bytes(&ix.handle, "a.txt").await.unwrap(), b"hello");
        let inner = nested(&ix.handle, "BOOT-INF/lib/lib-1.0.jar")
            .await
            .unwrap();
        assert!(inner.handle.starts_with(&format!("{}/", ix.handle)));
        assert_eq!(
            entry_bytes(&inner.handle, "x/Y.txt").await.unwrap(),
            b"inside"
        );
        // The same nested JAR again is the one kept.
        assert_eq!(
            nested(&ix.handle, "BOOT-INF/lib/lib-1.0.jar")
                .await
                .unwrap()
                .handle,
            inner.handle
        );
        assert!(
            matches!(entry_bytes(&ix.handle, "nope").await, Err(e) if e.code == crate::IpcErrorCode::NotFound)
        );

        let downloads = tempfile::tempdir().unwrap();
        let saved = download(&ix.handle, Some("a.txt"), downloads.path())
            .await
            .unwrap();
        assert_eq!(std::fs::read(&saved).unwrap(), b"hello");
        let whole = download(&inner.handle, None, downloads.path())
            .await
            .unwrap();
        assert_eq!(std::fs::read(&whole).unwrap(), lib);
    }

    #[tokio::test]
    async fn bytes_that_are_no_archive_are_refused_with_why() {
        let _turn = SERIAL.lock().await;
        let refused = open_bytes(b"not an archive, plainly".to_vec(), "x.jar")
            .await
            .unwrap_err();
        assert_eq!(refused.code, crate::IpcErrorCode::Invalid);
        assert!(
            refused.message.contains("not a zip archive"),
            "{}",
            refused.message
        );
    }

    #[tokio::test]
    async fn only_the_last_archives_are_kept() {
        let _turn = SERIAL.lock().await;
        let mut handles = Vec::new();
        for i in 0..=KEEP {
            let name = format!("f{i}.txt");
            handles.push(
                open_bytes(tiny_jar(&[(name.as_str(), b"x")]), "k.jar")
                    .await
                    .unwrap()
                    .handle,
            );
        }
        assert!(
            matches!(entry_bytes(&handles[0], "f0.txt").await, Err(e) if e.code == crate::IpcErrorCode::NotFound)
        );
        assert_eq!(
            entry_bytes(&handles[KEEP], &format!("f{KEEP}.txt"))
                .await
                .unwrap(),
            b"x"
        );
    }
}
