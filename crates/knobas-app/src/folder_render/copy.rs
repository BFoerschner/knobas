//! The copy of a folder a folder renderer is given (ADR-0044): written by
//! knobas, from the clone's objects or the forge, into a folder of its own
//! under the system temp directory, never the checkout's working folder.
//!
//! # Names from the mirror are checked before anything is written
//!
//! A file's name is the mirror's, and it becomes a path on this disk, so
//! every one is checked first and the copy is refused, naming the path,
//! when one is not a plain name here: `..`, an absolute path, a backslash, a
//! colon (an NTFS stream), a character Windows refuses or a control
//! character, a device name (`CON`, `nul.txt`), a name ending in a dot or a
//! space, or two names in one folder that differ only by case (on a disk
//! that ignores case one would overwrite the other). The same rules hold on
//! every platform, so a folder renders on the Mac exactly when it would on
//! Windows.
//!
//! # What the copy holds
//!
//! What git's objects hold at the commit: LFS files are their pointers, line
//! endings are as committed. A symlink is a plain file holding the link's
//! text, a submodule an empty folder, and an executable file keeps its bit
//! on Unix. Deep paths need nothing on Windows: Rust's `std::fs` writes a
//! path past 260 characters in its `\\?\` form itself.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use tokio_util::sync::CancellationToken;

/// The most files a copy from the clone may hold.
pub const CLONE_FILES: usize = 20_000;
/// The most bytes a copy from the clone may hold.
pub const CLONE_BYTES: u64 = 1024 * 1024 * 1024;
/// The most files a copy read from the forge may hold.
pub const FORGE_FILES: usize = 1_000;
/// The most bytes a copy read from the forge may hold.
pub const FORGE_BYTES: u64 = 64 * 1024 * 1024;
/// How many files are read at once.
pub const AT_ONCE: usize = 8;

/// What an entry of the folder is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ItemKind {
    File,
    Symlink,
    Dir,
    Submodule,
}

/// One entry of the folder, its path from the folder (`a/b.txt`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    pub rel: String,
    pub kind: ItemKind,
    pub size: u64,
    pub executable: bool,
    /// The blob's sha where the clone gave one: what it is read by, and what
    /// the cache key is made of.
    pub blob: Option<String>,
}

/// The name the copy's own folder gets: the folder's when it is a plain
/// name (`[A-Za-z0-9][A-Za-z0-9._-]{0,63}`, not ending in a dot, not a device
/// name), else `folder`. A tool that names a project after its folder sees
/// the project's name; a name that could be read as anything else is never
/// an argument (ADR-0016, as ADR-0034 checks a branch name).
#[must_use]
pub fn copy_name(folder: &str) -> String {
    let last = folder.trim_matches('/').rsplit('/').next().unwrap_or("");
    let plain = !last.is_empty()
        && last.len() <= 64
        && last.as_bytes()[0].is_ascii_alphanumeric()
        && last
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
        && !last.ends_with('.')
        && !is_device(last);
    if plain {
        last.to_owned()
    } else {
        "folder".to_owned()
    }
}

/// A Windows device name, with or without an extension (`CON`, `nul.txt`,
/// `COM1`).
fn is_device(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or(name).to_ascii_uppercase();
    matches!(
        stem.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
    ) || ((stem.starts_with("COM") || stem.starts_with("LPT"))
        && stem.len() == 4
        && stem.as_bytes()[3].is_ascii_digit()
        && stem.as_bytes()[3] != b'0')
}

/// Why `rel` cannot be written as a path of the copy, or `Ok`.
///
/// # Errors
///
/// The reason, in words.
pub fn check_path(rel: &str) -> Result<(), String> {
    if rel.is_empty() || rel.starts_with('/') {
        return Err("is not a path inside the folder".to_owned());
    }
    for segment in rel.split('/') {
        if segment.is_empty() || segment == "." || segment == ".." {
            return Err("climbs out of the folder or has an empty name".to_owned());
        }
        if let Some(c) = segment
            .chars()
            .find(|c| c.is_control() || matches!(c, '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|'))
        {
            return Err(format!(
                "holds {c:?}, which Windows does not take in a name"
            ));
        }
        if segment.ends_with('.') || segment.ends_with(' ') {
            return Err("ends in a dot or a space, which Windows drops".to_owned());
        }
        if is_device(segment) {
            return Err(format!("{segment} is a device name on Windows"));
        }
    }
    Ok(())
}

/// Every item checked, and no two names of one folder alike but for case.
///
/// # Errors
///
/// The first path refused and why.
pub fn check_all(items: &[Item]) -> Result<(), String> {
    let mut seen: HashMap<String, &str> = HashMap::with_capacity(items.len());
    for item in items {
        check_path(&item.rel).map_err(|why| format!("{} {why}", item.rel))?;
        if let Some(other) = seen.insert(item.rel.to_lowercase(), &item.rel) {
            return Err(format!(
                "{} and {other} differ only by case, and one would overwrite the other",
                item.rel
            ));
        }
    }
    Ok(())
}

/// Where every run's folders are made: short, so a deep tree stays under
/// what tools that are not long-path aware reach.
#[must_use]
pub fn root() -> PathBuf {
    std::env::temp_dir().join("knobas-fr")
}

/// Remove what an earlier run left under [`root`] (a quit mid-run, a
/// crash): each folder last written more than twelve hours ago. A running
/// knobas never holds one that long (the longest run is ten minutes and a
/// copy). `remove_dir_all` does not follow a link the tool made.
pub fn sweep(root: &Path) {
    let Ok(listed) = std::fs::read_dir(root) else {
        return;
    };
    let old = std::time::SystemTime::now() - std::time::Duration::from_secs(12 * 3600);
    for entry in listed.flatten() {
        let stale = entry
            .metadata()
            .and_then(|m| m.modified())
            .is_ok_and(|at| at < old);
        if stale {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

/// One run's folders: `in/<name>` the copy, `out` empty for `{out}` (or
/// `{outdir}`, under the store's staging when `out_root` was given), `cwd`
/// empty for the program to start in. Removed when dropped.
#[derive(Debug)]
pub struct Scratch {
    pub dir: PathBuf,
    pub copy: PathBuf,
    pub out: PathBuf,
    pub cwd: PathBuf,
    /// The staging folder `out` is in, when it is not under `dir`.
    pub staged: Option<PathBuf>,
}

static NEXT: AtomicU64 = AtomicU64::new(0);

impl Scratch {
    /// Fresh folders under `root` for a copy named `name`; `out` under
    /// `out_root` when given (a folder kind's `{outdir}`, in the store's
    /// staging so keeping is a rename per file, ADR-0046).
    ///
    /// # Errors
    ///
    /// The file system's.
    pub fn make(root: &Path, name: &str, out_root: Option<&Path>) -> std::io::Result<Self> {
        std::fs::create_dir_all(root)?;
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let leaf = format!("{}-{n}", std::process::id());
        let dir = root.join(&leaf);
        let _ = std::fs::remove_dir_all(&dir);
        let staged = out_root.map(|at| at.join(&leaf));
        if let Some(staged) = &staged {
            let _ = std::fs::remove_dir_all(staged);
        }
        let copy = dir.join("in").join(name);
        let out = staged.as_ref().unwrap_or(&dir).join("out");
        let cwd = dir.join("cwd");
        let made = Self {
            dir,
            copy,
            out,
            cwd,
            staged,
        };
        for at in [&made.copy, &made.out, &made.cwd] {
            std::fs::create_dir_all(at)?;
        }
        Ok(made)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
        if let Some(staged) = &self.staged {
            let _ = std::fs::remove_dir_all(staged);
        }
    }
}

/// Where a file's bytes come from: the clone's objects or the forge.
#[async_trait::async_trait]
pub trait Bytes: Send + Sync + 'static {
    /// The bytes of `item`, whole.
    async fn read(&self, item: &Item) -> Result<Vec<u8>, String>;
}

/// The copy's progress: files written of the total.
pub type Progress = Arc<dyn Fn(u32, u32) + Send + Sync>;

/// Write `items` into `scratch.copy`, [`AT_ONCE`] read at a time, telling
/// `progress` as files land. Stops at `cancel`.
///
/// # Errors
///
/// The first path refused or a file that could not be read or written,
/// naming it; `"stopped"` when cancelled.
pub async fn write(
    scratch: &Scratch,
    items: Vec<Item>,
    bytes: Arc<dyn Bytes>,
    progress: Progress,
    cancel: CancellationToken,
) -> Result<(), String> {
    check_all(&items)?;
    let base = scratch.copy.clone();
    // Folders first, all of them, so the writers race on nothing.
    let dirs: HashSet<PathBuf> = items
        .iter()
        .flat_map(|item| {
            let own =
                matches!(item.kind, ItemKind::Dir | ItemKind::Submodule).then(|| item.rel.as_str());
            let up = item.rel.rsplit_once('/').map(|(up, _)| up);
            own.into_iter().chain(up)
        })
        .map(|rel| base.join(rel))
        .collect();
    tokio::task::spawn_blocking(move || dirs.iter().try_for_each(std::fs::create_dir_all))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| format!("the copy's folders could not be made: {e}"))?;

    let files: Vec<Item> = items
        .into_iter()
        .filter(|i| matches!(i.kind, ItemKind::File | ItemKind::Symlink))
        .collect();
    let total = u32::try_from(files.len()).unwrap_or(u32::MAX);
    progress(0, total);
    let done = Arc::new(AtomicU64::new(0));
    let mut queue = files.into_iter();
    let mut running = tokio::task::JoinSet::new();
    let start = |running: &mut tokio::task::JoinSet<Result<(), String>>, item: Item| {
        let (bytes, base, done, progress) =
            (bytes.clone(), base.clone(), done.clone(), progress.clone());
        running.spawn(async move {
            let read = bytes
                .read(&item)
                .await
                .map_err(|why| format!("{}: {why}", item.rel))?;
            let to = base.join(&item.rel);
            tokio::fs::write(&to, read)
                .await
                .map_err(|e| format!("{} could not be written: {e}", item.rel))?;
            #[cfg(unix)]
            if item.executable {
                use std::os::unix::fs::PermissionsExt as _;
                let _ =
                    tokio::fs::set_permissions(&to, std::fs::Permissions::from_mode(0o755)).await;
            }
            let n = done.fetch_add(1, Ordering::Relaxed) + 1;
            progress(u32::try_from(n).unwrap_or(u32::MAX), total);
            Ok(())
        });
    };
    for item in queue.by_ref().take(AT_ONCE) {
        start(&mut running, item);
    }
    loop {
        let next = tokio::select! {
            () = cancel.cancelled() => {
                running.abort_all();
                return Err("stopped".to_owned());
            }
            next = running.join_next() => next,
        };
        let Some(joined) = next else { break };
        match joined {
            Ok(Ok(())) => {}
            Ok(Err(why)) => {
                running.abort_all();
                return Err(why);
            }
            Err(error) => {
                running.abort_all();
                return Err(error.to_string());
            }
        }
        if let Some(item) = queue.next() {
            start(&mut running, item);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_copy_is_named_after_a_plain_folder_name_only() {
        assert_eq!(copy_name("apps/order-flow"), "order-flow");
        assert_eq!(copy_name("v1.2_beta"), "v1.2_beta");
        for odd in [
            "",
            "a/-rf",
            "a/.hidden",
            "a/x y",
            "a/CON",
            "a/end.",
            "a/ü",
            "a/$(id)",
        ] {
            assert_eq!(copy_name(odd), "folder", "{odd}");
        }
        assert_eq!(copy_name(&"a".repeat(65)), "folder");
    }

    /// Every name that is not a plain path here is refused before anything
    /// is written, on every platform.
    #[test]
    fn a_name_that_is_no_plain_path_is_refused() {
        for ok in [
            "a.txt",
            "a/b/c.rs",
            "Ärger/ü.md",
            "com0",
            "COM10.txt",
            "console/x",
        ] {
            assert!(check_path(ok).is_ok(), "{ok}");
        }
        for bad in [
            "../x",
            "a/../../x",
            "/etc/passwd",
            "a//b",
            "./a",
            "a\\b",
            "a:stream",
            "nul.txt",
            "a/CON",
            "con.d/x",
            "LPT1",
            "end.",
            "space ",
            "tab\there",
            "q?",
            "pipe|",
        ] {
            assert!(check_path(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn two_names_alike_but_for_case_are_refused() {
        let item = |rel: &str| Item {
            rel: rel.to_owned(),
            kind: ItemKind::File,
            size: 1,
            executable: false,
            blob: None,
        };
        assert!(check_all(&[item("a/Read.md"), item("b/read.md")]).is_ok());
        let refused = check_all(&[item("a/Read.md"), item("a/README.md"), item("a/readme.md")]);
        assert!(refused.is_err());
        let why = check_all(&[item("x/A"), item("x/a")]).unwrap_err();
        assert!(why.contains("differ only by case"), "{why}");
    }

    struct Fixed(HashMap<String, Vec<u8>>);

    #[async_trait::async_trait]
    impl Bytes for Fixed {
        async fn read(&self, item: &Item) -> Result<Vec<u8>, String> {
            self.0
                .get(&item.rel)
                .cloned()
                .ok_or_else(|| "not there".to_owned())
        }
    }

    /// The copy holds each file at its path, the folders and a submodule's
    /// empty folder, tells its progress to the last file, and is gone when
    /// the run's folders are dropped.
    #[tokio::test]
    async fn the_copy_is_written_and_removed() {
        let root = std::env::temp_dir().join(format!("knobas-fr-test-{}", std::process::id()));
        let scratch = Scratch::make(&root, "proj", None).unwrap();
        let items = vec![
            Item {
                rel: "a.txt".into(),
                kind: ItemKind::File,
                size: 1,
                executable: true,
                blob: None,
            },
            Item {
                rel: "sub".into(),
                kind: ItemKind::Dir,
                size: 0,
                executable: false,
                blob: None,
            },
            Item {
                rel: "sub/b.txt".into(),
                kind: ItemKind::File,
                size: 1,
                executable: false,
                blob: None,
            },
            Item {
                rel: "mod".into(),
                kind: ItemKind::Submodule,
                size: 0,
                executable: false,
                blob: None,
            },
            Item {
                rel: "link".into(),
                kind: ItemKind::Symlink,
                size: 1,
                executable: false,
                blob: None,
            },
        ];
        let bytes: Arc<dyn Bytes> = Arc::new(Fixed(HashMap::from([
            ("a.txt".to_owned(), b"A".to_vec()),
            ("sub/b.txt".to_owned(), b"B".to_vec()),
            ("link".to_owned(), b"a.txt".to_vec()),
        ])));
        let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        let told = seen.clone();
        write(
            &scratch,
            items,
            bytes,
            Arc::new(move |n, of| told.lock().unwrap().push((n, of))),
            CancellationToken::new(),
        )
        .await
        .unwrap();
        assert_eq!(std::fs::read(scratch.copy.join("a.txt")).unwrap(), b"A");
        assert_eq!(std::fs::read(scratch.copy.join("sub/b.txt")).unwrap(), b"B");
        assert!(scratch.copy.join("mod").is_dir());
        let link = std::fs::symlink_metadata(scratch.copy.join("link")).unwrap();
        assert!(link.is_file(), "a symlink is copied as a plain file");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            let mode = std::fs::metadata(scratch.copy.join("a.txt"))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o111, 0o111);
        }
        assert_eq!(seen.lock().unwrap().last(), Some(&(3, 3)));
        assert!(std::fs::read_dir(&scratch.out).unwrap().next().is_none());
        let dir = scratch.dir.clone();
        drop(scratch);
        assert!(!dir.exists());
        let _ = std::fs::remove_dir_all(root);
    }

    /// A refused name writes nothing at all.
    #[tokio::test]
    async fn a_refused_name_writes_nothing() {
        let root = std::env::temp_dir().join(format!("knobas-fr-test2-{}", std::process::id()));
        let scratch = Scratch::make(&root, "folder", None).unwrap();
        let items = vec![
            Item {
                rel: "ok.txt".into(),
                kind: ItemKind::File,
                size: 1,
                executable: false,
                blob: None,
            },
            Item {
                rel: "../escape".into(),
                kind: ItemKind::File,
                size: 1,
                executable: false,
                blob: None,
            },
        ];
        let bytes: Arc<dyn Bytes> = Arc::new(Fixed(HashMap::new()));
        let why = write(
            &scratch,
            items,
            bytes,
            Arc::new(|_, _| {}),
            CancellationToken::new(),
        )
        .await
        .unwrap_err();
        assert!(why.contains("../escape"), "{why}");
        assert!(std::fs::read_dir(&scratch.copy).unwrap().next().is_none());
        assert!(!root.join("in").join("escape").exists());
        drop(scratch);
        let _ = std::fs::remove_dir_all(root);
    }
}
