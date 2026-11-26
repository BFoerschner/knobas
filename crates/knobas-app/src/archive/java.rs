//! *Decompile* on a class in the JAR view (ux19), as Fable ruled
//! (`docs/decisions/2026-11-18-a-jar-is-drawn-as-its-contents.md`): Vineflower, bundled beside the binary and pinned
//! by its SHA-256, run with a Java found on this machine. Nothing is downloaded at run time.
//!
//! - **Java** is looked for once (the first JAR opened, or Settings asking), never on a click: the path set in
//!   Settings, `JAVA_HOME`, macOS's `java_home`, `PATH`, then the usual install folders. A `java` counts only where
//!   `java -version` exits 0 and names a version within two seconds, which turns away macOS's stub. The candidates
//!   are asked four at once, and each `java`'s answer is kept for the session until the file changes (#2519).
//! - **The floor** is the bundled jar's own: the class file version of its main class (Vineflower 1.12 is Java 17).
//!   No Java: no button. Java too old: the button disabled with the reason.
//! - **A run**: the class and its nested classes written into a fresh folder, Vineflower into another, one JVM at a
//!   time (a new run stops the one before), `-Xmx512m`, 20 s, 8 MB in, 2 MB out, `JAVA_TOOL_OPTIONS` and its kin left
//!   out of its environment, both folders removed after. Past a bound is an error, never a cut answer.
//! - **Kept** on disk in the app's cache folder by a SHA-256 over the classes' bytes and the decompiler's name, 200 MB
//!   at most, the oldest dropped first. A failure is not kept.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex, OnceLock, PoisonError};
use std::time::Duration;

use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::PgPool;

use crate::IpcError;

/// Where Settings keeps a Java the person picked (`knobas.setting`).
pub const JAVA_PATH_KEY: &str = "java_path";
/// The pins of the bundled decompiler, `<sha256>  vineflower-<version>.jar`.
const PINS: &str = include_str!("../../decompiler/vineflower.sha256");
const TIMEOUT: Duration = Duration::from_secs(20);
const INPUT_CAP: u64 = 8 * 1024 * 1024;
const OUTPUT_CAP: usize = 2 * 1024 * 1024;
const CACHE_CAP: u64 = 200 * 1024 * 1024;
/// The options a run passes; part of the cache key.
const OPTIONS: &[&str] = &["-log=WARN"];

/// What the JAR view and Settings know about decompiling here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ts_rs::TS)]
#[ts(export_to = "app.ts")]
pub struct JavaRuntime {
    /// The Java found: its path and version; `null` for none.
    pub java: Option<FoundJava>,
    /// The decompiler bundled with this build (`Vineflower 1.12.0`); `null` in a build without it.
    pub decompiler: Option<String>,
    /// The Java release the decompiler needs (17).
    pub needs: Option<u16>,
    /// A Java was found, and the decompiler runs on it.
    pub can_decompile: bool,
    /// Why not, where there is a Java: `Java 11 found; Decompile needs Java 17`.
    pub reason: Option<String>,
    /// The Java the person set in Settings, if any.
    pub set_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ts_rs::TS)]
#[ts(export_to = "app.ts")]
pub struct FoundJava {
    pub path: String,
    /// As `java -version` names it (`21.0.4`).
    pub version: String,
    /// The release (`21`; `8` for `1.8.0_392`).
    pub release: u16,
}

/// The bundled decompiler: its jar and what it needs.
#[derive(Debug, Clone)]
struct Decompiler {
    jar: PathBuf,
    name: String,
    needs: u16,
}

static RUNTIME: OnceLock<tokio::sync::Mutex<Option<JavaRuntime>>> = OnceLock::new();
static DECOMPILER: OnceLock<Mutex<Option<Option<Decompiler>>>> = OnceLock::new();
/// The run going now, stopped by the next one.
static RUNNING: OnceLock<Mutex<Option<tokio::task::AbortHandle>>> = OnceLock::new();

/// The release a `java -version` line names: `21.0.4` is 21, `1.8.0_392` is 8.
#[must_use]
pub fn release_of(version: &str) -> Option<u16> {
    let mut parts = version
        .split(|c: char| !c.is_ascii_digit())
        .filter(|p| !p.is_empty());
    let first: u16 = parts.next()?.parse().ok()?;
    if first == 1 {
        parts.next()?.parse().ok()
    } else {
        Some(first)
    }
}

/// The version `java -version` prints on stderr: the quoted word of its first line not saying `Picked up`.
#[must_use]
pub fn version_of(stderr: &str) -> Option<String> {
    let line = stderr
        .lines()
        .find(|l| !l.starts_with("Picked up") && !l.trim().is_empty())?;
    let start = line.find('"')? + 1;
    let end = line[start..].find('"')? + start;
    Some(line[start..end].to_owned())
}

fn java_in(home: &Path) -> PathBuf {
    home.join("bin")
        .join(if cfg!(windows) { "java.exe" } else { "java" })
}

/// The `java` a path set in Settings names: the file itself, or a Java home's.
fn set_candidate(set: &str) -> PathBuf {
    let p = PathBuf::from(set);
    if p.is_dir() { java_in(&p) } else { p }
}

/// Every place a `java` may be, the person's own first.
fn candidates(set: Option<&str>) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(set) = set {
        out.push(set_candidate(set));
    }
    if let Some(home) = std::env::var_os("JAVA_HOME") {
        out.push(java_in(Path::new(&home)));
    }
    #[cfg(target_os = "macos")]
    if let Ok(found) = knobas_db::console::command("/usr/libexec/java_home")
        .stderr(std::process::Stdio::null())
        .output()
        && found.status.success()
    {
        let home = String::from_utf8_lossy(&found.stdout).trim().to_owned();
        if !home.is_empty() {
            out.push(java_in(Path::new(&home)));
        }
    }
    if let Some(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path) {
            out.push(dir.join(if cfg!(windows) { "java.exe" } else { "java" }));
        }
    }
    // A desktop app started from the Finder or the Start menu sees little of a shell's PATH: the usual installs.
    let mut roots: Vec<PathBuf> = Vec::new();
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" });
    if let Some(home) = &home {
        roots.extend(home_roots(Path::new(home)));
    }
    if cfg!(windows) {
        for base in [
            std::env::var_os("ProgramFiles"),
            std::env::var_os("ProgramFiles(x86)"),
        ]
        .into_iter()
        .flatten()
        {
            for vendor in [
                "Java",
                "Eclipse Adoptium",
                "Microsoft",
                "Zulu",
                "Amazon Corretto",
                "BellSoft",
                "Semeru",
            ] {
                roots.push(Path::new(&base).join(vendor));
            }
        }
    } else {
        roots.push(PathBuf::from("/Library/Java/JavaVirtualMachines"));
        roots.push(PathBuf::from("/usr/lib/jvm"));
        out.push(PathBuf::from("/opt/homebrew/opt/openjdk/bin/java"));
        out.push(PathBuf::from("/usr/local/opt/openjdk/bin/java"));
    }
    out.extend(javas_under(&roots));
    out.retain(|p| p.is_file());
    out.dedup();
    out
}

/// The folders under a person's home that tools fill with Javas, a Java home each: IntelliJ's downloads (`.jdks`, on
/// every platform; where Björn's Temurins sat on his PC, the ux19 Windows pass), and elsewhere SDKMAN!'s, mise's and
/// macOS's own per-user folder.
fn home_roots(home: &Path) -> Vec<PathBuf> {
    let mut roots = vec![home.join(".jdks")];
    if !cfg!(windows) {
        roots.push(home.join(".sdkman/candidates/java"));
        roots.push(home.join(".local/share/mise/installs/java"));
        roots.push(home.join("Library/Java/JavaVirtualMachines"));
    }
    roots
}

/// The `java` each folder in `roots` holds a Java home for (or a macOS bundle's `Contents/Home`), the newest-looking
/// name first in each; only those that are there.
fn javas_under(roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for root in roots {
        let Ok(dirs) = std::fs::read_dir(root) else {
            continue;
        };
        let mut found: Vec<PathBuf> = dirs.filter_map(Result::ok).map(|d| d.path()).collect();
        found.sort();
        found.reverse();
        for dir in found {
            out.push(java_in(&dir));
            out.push(java_in(&dir.join("Contents/Home")));
        }
    }
    out.retain(|p| p.is_file());
    out
}

/// `java -version`, within two seconds: its version, or `None` for a `java` that is no Java (macOS's stub).
async fn probe(java: &Path) -> Option<FoundJava> {
    let argv = vec![java.display().to_string(), "-version".to_owned()];
    let ended = crate::renderers::run_argv(
        &argv,
        Vec::new(),
        crate::renderers::Limits {
            timeout: Duration::from_secs(2),
            output_cap: 64 * 1024,
        },
        java_env(),
    )
    .await;
    let crate::renderers::Ended::Exited {
        status: Some(0),
        stdout,
        stderr,
    } = ended
    else {
        return None;
    };
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&stderr),
        String::from_utf8_lossy(&stdout)
    );
    let version = version_of(&text)?;
    Some(FoundJava {
        path: java.display().to_string(),
        release: release_of(&version)?,
        version,
    })
}

/// What `java -version` answered for a `java` asked this session, and the file as it was then (#2519).
#[derive(Clone)]
struct Probed {
    stamp: (std::time::SystemTime, u64),
    found: Option<FoundJava>,
}

/// Every `java` asked this session, by path: one replaced since (another modified time or length) is asked again.
static PROBED: LazyLock<Mutex<HashMap<PathBuf, Probed>>> = LazyLock::new(Mutex::default);

/// How many candidates are asked at once.
const ASKED_AT_ONCE: usize = 4;

fn stamp(java: &Path) -> Option<(std::time::SystemTime, u64)> {
    let meta = std::fs::metadata(java).ok()?;
    Some((meta.modified().ok()?, meta.len()))
}

/// [`probe`], answered from this session's asks where the `java` is unchanged; `again` (*Look again*) asks again a
/// `java` that was no Java, as one installed since may now be.
async fn probe_kept(java: &Path, again: bool) -> Option<FoundJava> {
    let stamp = stamp(java);
    if let Some(stamp) = stamp {
        let kept = PROBED
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(java)
            .filter(|kept| kept.stamp == stamp && !(again && kept.found.is_none()))
            .cloned();
        if let Some(kept) = kept {
            return kept.found;
        }
    }
    let found = probe(java).await;
    if let Some(stamp) = stamp {
        PROBED
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(
                java.to_path_buf(),
                Probed {
                    stamp,
                    found: found.clone(),
                },
            );
    }
    found
}

/// The first of `places` that is a Java new enough to run the decompiler (`needs`), else the first Java at all, so the
/// view can say which one it found and why it cannot decompile with it. A `java` 8 first in `PATH` no longer hides a
/// Java 21 in an install folder. The candidates are asked [`ASKED_AT_ONCE`] at a time and their answers taken in
/// order, so a slow or hanging one costs its two seconds once, not once per candidate before it (#2519).
async fn first_java(places: Vec<PathBuf>, needs: Option<u16>, again: bool) -> Option<FoundJava> {
    use futures_util::stream::{self, StreamExt};
    let mut answers = stream::iter(places)
        .map(|candidate| async move { probe_kept(&candidate, again).await })
        .buffered(ASKED_AT_ONCE);
    let mut older = None;
    while let Some(answer) = answers.next().await {
        if let Some(found) = answer {
            if needs.is_none_or(|n| found.release >= n) {
                return Some(found);
            }
            older.get_or_insert(found);
        }
    }
    older
}

/// The environment a Java is started with: knobas's own and the JVM's option variables left out.
fn java_env() -> Vec<(std::ffi::OsString, std::ffi::OsString)> {
    std::env::vars_os()
        .filter(|(k, _)| {
            !matches!(
                k.to_str(),
                Some("JAVA_TOOL_OPTIONS" | "_JAVA_OPTIONS" | "JDK_JAVA_OPTIONS")
            )
        })
        .collect()
}

/// The decompiler jar in `dirs`, checked against its pin, with the release its main class needs.
fn find_decompiler(dirs: &[PathBuf]) -> Option<Decompiler> {
    for line in PINS.lines() {
        let Some((sha, file)) = line.split_once("  ") else {
            continue;
        };
        for dir in dirs {
            let jar = dir.join(file.trim());
            let Ok(bytes) = std::fs::read(&jar) else {
                continue;
            };
            if format!("{:x}", Sha256::digest(&bytes)) != sha.trim() {
                tracing::warn!(jar = %jar.display(), "the decompiler jar does not match its pin; not used");
                continue;
            }
            let mut c = std::io::Cursor::new(&bytes[..]);
            let dir = knobas_jar::zip::read_directory(&mut c, 0, bytes.len() as u64).ok()?;
            let manifest = knobas_jar::entry(&dir, "META-INF/MANIFEST.MF")
                .and_then(|e| knobas_jar::zip::read_entry(&mut c, &dir, e, 1 << 20).ok())?;
            let fields = knobas_jar::meta::manifest(&String::from_utf8_lossy(&manifest));
            let field = |k: &str| fields.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone());
            let main = field("Main-Class")?.replace('.', "/") + ".class";
            let head = knobas_jar::entry(&dir, &main)
                .and_then(|e| knobas_jar::zip::read_head(&mut c, &dir, e, 8).ok())?;
            let (major, _) = knobas_jar::class::version(&head)?;
            let name = format!(
                "{} {}",
                field("Implementation-Name").unwrap_or_else(|| "Vineflower".to_owned()),
                field("Implementation-Version").unwrap_or_default()
            );
            return Some(Decompiler {
                jar,
                name: name.trim().to_owned(),
                needs: major.saturating_sub(44),
            });
        }
    }
    None
}

/// Where the bundled jar may be: the app's resources, and in a development build the crate's own folder.
pub fn decompiler_dirs(resources: Option<PathBuf>) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(r) = resources {
        dirs.push(r.join("decompiler"));
    }
    if cfg!(debug_assertions) {
        dirs.push(Path::new(env!("CARGO_MANIFEST_DIR")).join("decompiler"));
    }
    dirs
}

fn decompiler(dirs: &[PathBuf]) -> Option<Decompiler> {
    let cell = DECOMPILER.get_or_init(|| Mutex::new(None));
    let mut held = cell
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if held.is_none() {
        *held = Some(find_decompiler(dirs));
    }
    held.clone().flatten()
}

async fn set_path(pool: Option<&PgPool>) -> Option<String> {
    let pool = pool?;
    let stored: Option<serde_json::Value> =
        sqlx::query_scalar("select value from knobas.setting where key = $1")
            .bind(JAVA_PATH_KEY)
            .fetch_optional(pool)
            .await
            .ok()?;
    stored
        .and_then(|v| v.as_str().map(str::to_owned))
        .filter(|s| !s.is_empty())
}

/// What decompiling here stands at: looked for once, again when `again`.
pub async fn runtime(pool: Option<&PgPool>, dirs: &[PathBuf], again: bool) -> JavaRuntime {
    let cell = RUNTIME.get_or_init(|| tokio::sync::Mutex::new(None));
    let mut held = cell.lock().await;
    if !again && let Some(known) = held.as_ref() {
        return known.clone();
    }
    let set = set_path(pool).await;
    let looked = set.clone();
    // The candidates and the decompiler jar (read whole and hashed against its pin, the first time) on a blocking
    // thread, never on the async runtime's (#2519).
    let dirs = dirs.to_vec();
    let (places, tool) =
        tokio::task::spawn_blocking(move || (candidates(looked.as_deref()), decompiler(&dirs)))
            .await
            .unwrap_or_default();
    // A Java the person set is the one used, too old or not (the view says why); otherwise the first new enough.
    let own = match set.as_deref() {
        Some(set) => probe_kept(&set_candidate(set), again).await,
        None => None,
    };
    let java = match own {
        Some(own) => Some(own),
        None => first_java(places, tool.as_ref().map(|d| d.needs), again).await,
    };
    let (can, reason) = match (&java, &tool) {
        (None, _) => (false, None),
        (Some(_), None) => (false, Some("this build carries no decompiler".to_owned())),
        (Some(j), Some(d)) if j.release < d.needs => (
            false,
            Some(format!(
                "Java {} found; Decompile needs Java {}",
                j.release, d.needs
            )),
        ),
        (Some(_), Some(_)) => (true, None),
    };
    let answer = JavaRuntime {
        java,
        decompiler: tool.as_ref().map(|d| d.name.clone()),
        needs: tool.as_ref().map(|d| d.needs),
        can_decompile: can,
        reason,
        set_path: set,
    };
    *held = Some(answer.clone());
    answer
}

/// Stores the Java Settings names (`None` clears it) and looks again.
///
/// # Errors
///
/// `invalid` for a path that is not absolute, `internal` where it cannot be stored.
pub async fn set_java_path(
    pool: &PgPool,
    dirs: &[PathBuf],
    path: Option<&str>,
) -> Result<JavaRuntime, IpcError> {
    let path = path.map(str::trim).filter(|p| !p.is_empty());
    if let Some(p) = path
        && !Path::new(p).is_absolute()
    {
        return Err(IpcError::invalid(format!(
            "the Java must be an absolute path, not {p:?}"
        )));
    }
    sqlx::query("insert into knobas.setting (key, value) values ($1, $2) on conflict (key) do update set value = excluded.value, updated_at = now()")
        .bind(JAVA_PATH_KEY)
        .bind(serde_json::Value::String(path.unwrap_or_default().to_owned()))
        .execute(pool)
        .await
        .map_err(IpcError::internal)?;
    Ok(runtime(Some(pool), dirs, true).await)
}

/// The cache key of a run: the classes' names and bytes, the decompiler and its options.
#[must_use]
pub fn cache_key(classes: &[(String, Vec<u8>)], decompiler: &str) -> String {
    let mut h = Sha256::new();
    for (name, bytes) in classes {
        h.update((name.len() as u64).to_le_bytes());
        h.update(name.as_bytes());
        h.update((bytes.len() as u64).to_le_bytes());
        h.update(bytes);
    }
    h.update(decompiler.as_bytes());
    for o in OPTIONS {
        h.update(o.as_bytes());
    }
    format!("{:x}", h.finalize())
}

/// Drops the oldest files of `dir` until it holds at most `cap` bytes.
fn trim_cache(dir: &Path, cap: u64) {
    let Ok(read) = std::fs::read_dir(dir) else {
        return;
    };
    let mut files: Vec<(std::time::SystemTime, u64, PathBuf)> = read
        .filter_map(Result::ok)
        .filter_map(|e| {
            let m = e.metadata().ok()?;
            m.is_file().then(|| {
                (
                    m.modified().unwrap_or(std::time::UNIX_EPOCH),
                    m.len(),
                    e.path(),
                )
            })
        })
        .collect();
    let mut total: u64 = files.iter().map(|f| f.1).sum();
    files.sort();
    for (_, len, path) in files {
        if total <= cap {
            break;
        }
        if std::fs::remove_file(&path).is_ok() {
            total -= len;
        }
    }
}

/// The source Vineflower writes for `top` (`com/x/Foo.class`) and its nested classes, `classes` as
/// [`super::class_family`] answers them; kept in `cache`.
///
/// # Errors
///
/// `invalid` where no Java or decompiler can run (the reason said), past a bound, or where the decompiler fails (its
/// last words said); `cancelled`-like `invalid` where a newer run stopped this one.
pub async fn decompile(
    rt: &JavaRuntime,
    dirs: &[PathBuf],
    cache: &Path,
    top: &str,
    classes: Vec<(String, Vec<u8>)>,
) -> Result<String, IpcError> {
    let (Some(java), true) = (rt.java.as_ref(), rt.can_decompile) else {
        return Err(IpcError::invalid(
            rt.reason
                .clone()
                .unwrap_or_else(|| "no Java found on this machine".to_owned()),
        ));
    };
    let tool =
        decompiler(dirs).ok_or_else(|| IpcError::invalid("this build carries no decompiler"))?;
    if classes.iter().map(|(_, b)| b.len() as u64).sum::<u64>() > INPUT_CAP {
        return Err(IpcError::invalid(format!(
            "{top} and its nested classes are over 8 MiB"
        )));
    }
    let key = cache_key(&classes, &tool.name);
    let kept = cache.join(format!("{key}.java"));
    if let Ok(text) = tokio::fs::read_to_string(&kept).await {
        // Touched, so the oldest-first trim keeps what is read.
        let _ = std::fs::File::options()
            .append(true)
            .open(&kept)
            .and_then(|f| f.set_modified(std::time::SystemTime::now()));
        return Ok(text);
    }
    let java_path = java.path.clone();
    let jar = tool.jar.clone();
    let wanted = top
        .rsplit('/')
        .next()
        .unwrap_or(top)
        .trim_end_matches(".class")
        .to_owned();
    let work = tokio::spawn(async move { run(&java_path, &jar, &wanted, classes).await });
    {
        let cell = RUNNING.get_or_init(|| Mutex::new(None));
        let mut running = cell
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(before) = running.replace(work.abort_handle()) {
            before.abort();
        }
    }
    let text = match work.await {
        Ok(result) => result?,
        Err(e) if e.is_cancelled() => {
            return Err(IpcError::invalid("stopped: another class was decompiled"));
        }
        Err(e) => return Err(IpcError::internal(e)),
    };
    if tokio::fs::create_dir_all(cache).await.is_ok()
        && tokio::fs::write(&kept, &text).await.is_ok()
    {
        let dir = cache.to_owned();
        tokio::task::spawn_blocking(move || trim_cache(&dir, CACHE_CAP));
    }
    Ok(text)
}

/// Removes its folder when dropped, a run stopped midway included.
struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

async fn run(
    java: &str,
    jar: &Path,
    wanted: &str,
    classes: Vec<(String, Vec<u8>)>,
) -> Result<String, IpcError> {
    static RUNS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = RUNS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let scratch =
        Scratch(std::env::temp_dir().join(format!("knobas-decompile-{}-{n}", std::process::id())));
    let (input, output) = (scratch.0.join("in"), scratch.0.join("out"));
    std::fs::create_dir_all(&input)
        .and_then(|()| std::fs::create_dir_all(&output))
        .map_err(IpcError::internal)?;
    for (name, bytes) in &classes {
        // Base names only: a name with a separator or `..` is not written.
        if name.contains(['/', '\\']) || name.contains("..") || name.is_empty() {
            return Err(IpcError::invalid(format!("{name:?} is no class file name")));
        }
        std::fs::write(input.join(name), bytes).map_err(IpcError::internal)?;
    }
    let mut argv = vec![
        java.to_owned(),
        "-Xmx512m".to_owned(),
        "-Djava.awt.headless=true".to_owned(),
        "-jar".to_owned(),
        jar.display().to_string(),
    ];
    argv.extend(OPTIONS.iter().map(|o| (*o).to_owned()));
    argv.push(input.display().to_string());
    argv.push(output.display().to_string());
    let ended = crate::renderers::run_argv(
        &argv,
        Vec::new(),
        crate::renderers::Limits {
            timeout: TIMEOUT,
            output_cap: 1024 * 1024,
        },
        java_env(),
    )
    .await;
    let last_words = |stderr: &[u8]| {
        String::from_utf8_lossy(stderr)
            .lines()
            .rev()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("")
            .to_owned()
    };
    match ended {
        crate::renderers::Ended::Exited {
            status: Some(0), ..
        } => {}
        crate::renderers::Ended::Exited { stderr, .. } => {
            return Err(IpcError::invalid(format!(
                "the decompiler failed: {}",
                last_words(&stderr)
            )));
        }
        crate::renderers::Ended::TimedOut { .. } => {
            return Err(IpcError::invalid(
                "the decompiler took over 20 seconds and was stopped",
            ));
        }
        crate::renderers::Ended::TooLarge => {
            return Err(IpcError::invalid(
                "the decompiler wrote too much and was stopped",
            ));
        }
        crate::renderers::Ended::NotStarted(why) => return Err(IpcError::invalid(why)),
    }
    let file = format!("{wanted}.java");
    let found = find_output(&output, &file, 0)
        .ok_or_else(|| IpcError::invalid(format!("the decompiler wrote no {file}")))?;
    let meta = std::fs::symlink_metadata(&found).map_err(IpcError::internal)?;
    if !meta.is_file() {
        return Err(IpcError::invalid(format!(
            "the decompiler's {file} is not a plain file"
        )));
    }
    if meta.len() > OUTPUT_CAP as u64 {
        return Err(IpcError::invalid(format!(
            "{file} as decompiled is over 2 MiB"
        )));
    }
    std::fs::read_to_string(&found).map_err(IpcError::internal)
}

/// `file` inside `dir`, a plain file and no link, at most eight folders down.
fn find_output(dir: &Path, file: &str, depth: u8) -> Option<PathBuf> {
    if depth > 8 {
        return None;
    }
    let entries = std::fs::read_dir(dir).ok()?;
    let mut folders = Vec::new();
    for e in entries.filter_map(Result::ok) {
        // `DirEntry::file_type` does not follow a link: a link is neither a file nor a folder here, so none is read
        // or walked into.
        let kind = e.file_type().ok()?;
        if kind.is_file() && e.file_name() == file {
            return Some(e.path());
        }
        if kind.is_dir() {
            folders.push(e.path());
        }
    }
    folders
        .into_iter()
        .find_map(|f| find_output(&f, file, depth + 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The real path, where this machine has a Java and the fetched Vineflower (`scripts/fetch-vineflower.sh`): a class
    /// and its nested classes decompiled, then answered from the cache. Skipped, saying why, where it cannot run.
    #[tokio::test]
    async fn a_class_decompiles_with_the_java_found_here_and_is_kept() {
        let dirs = decompiler_dirs(None);
        let rt = runtime(None, &dirs, true).await;
        if !rt.can_decompile {
            eprintln!(
                "skipped: no decompiling here ({:?}, {:?}, {:?})",
                rt.java, rt.decompiler, rt.reason
            );
            return;
        }
        assert_eq!(
            rt.needs,
            Some(17),
            "Vineflower 1.12's main class is Java 17's"
        );
        let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("../knobas-jar/tests/fixtures");
        let mut classes: Vec<(String, Vec<u8>)> = std::fs::read_dir(&fixtures)
            .unwrap()
            .filter_map(Result::ok)
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n == "Shelf.class" || n.starts_with("Shelf$"))
            .map(|n| {
                let bytes = std::fs::read(fixtures.join(&n)).unwrap();
                (n, bytes)
            })
            .collect();
        classes.sort();
        let cache = tempfile::tempdir().unwrap();
        let started = std::time::Instant::now();
        let text = decompile(
            &rt,
            &dirs,
            cache.path(),
            "com/example/demo/Shelf.class",
            classes.clone(),
        )
        .await
        .unwrap();
        let cold = started.elapsed();
        assert!(
            text.contains("public abstract class Shelf<T extends Comparable<T>>"),
            "{text}"
        );
        assert!(
            text.contains("public static enum Colour") || text.contains("enum Colour"),
            "{text}"
        );
        let started = std::time::Instant::now();
        assert_eq!(
            decompile(
                &rt,
                &dirs,
                cache.path(),
                "com/example/demo/Shelf.class",
                classes
            )
            .await
            .unwrap(),
            text
        );
        let kept = started.elapsed();
        eprintln!("decompiled cold in {cold:?}, from the cache in {kept:?}");
        assert!(kept < cold);
    }

    /// IntelliJ's downloads (`~/.jdks`) are looked in on every platform: on Björn's PC both his Javas were there
    /// (Temurin 24 and 25), and none in `PATH`, `JAVA_HOME` or Program Files, so the JAR view offered no *Decompile*.
    #[test]
    fn a_java_intellij_downloaded_is_found_under_the_home() {
        let home = tempfile::tempdir().unwrap();
        let exe = if cfg!(windows) { "java.exe" } else { "java" };
        for name in ["temurin-24.0.2", "temurin-25"] {
            let bin = home.path().join(".jdks").join(name).join("bin");
            std::fs::create_dir_all(&bin).unwrap();
            std::fs::write(bin.join(exe), b"").unwrap();
        }
        // A folder that is no Java home is passed over.
        std::fs::create_dir_all(home.path().join(".jdks/notes")).unwrap();
        let found = javas_under(&home_roots(home.path()));
        assert_eq!(
            found,
            vec![
                home.path().join(".jdks/temurin-25/bin").join(exe),
                home.path().join(".jdks/temurin-24.0.2/bin").join(exe),
            ],
            "the newest-looking name first, only the Java homes"
        );
    }

    /// A Java too old found first is passed over for a newer one further on; with none new enough, the first is named.
    #[cfg(unix)]
    #[tokio::test]
    async fn the_first_java_new_enough_is_taken() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let fake = |name: &str, version: &str| {
            let path = dir.path().join(name);
            std::fs::write(
                &path,
                format!("#!/bin/sh\necho 'openjdk version \"{version}\" 2024-07-16' >&2\n"),
            )
            .unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
            path
        };
        let (old, new, newer) = (
            fake("j11", "11.0.2"),
            fake("j21", "21.0.4"),
            fake("j25", "25"),
        );
        let found = first_java(vec![old.clone(), new.clone(), newer], Some(17), false)
            .await
            .unwrap();
        assert_eq!((found.release, found.path), (21, new.display().to_string()));
        let found = first_java(vec![old.clone()], Some(17), false).await.unwrap();
        assert_eq!(
            found.release, 11,
            "no Java new enough: the one found, to say why not"
        );
        assert_eq!(
            first_java(vec![old, dir.path().join("none")], None, false)
                .await
                .unwrap()
                .release,
            11
        );
    }

    /// Settings › Renderers' look for a Java (#2519): the candidates are asked at once, not one after another, the first
    /// new enough in their order still wins, and a `java` asked once this session is not started again unless it
    /// changed; a look again asks again those that were no Java.
    #[cfg(unix)]
    #[tokio::test]
    async fn javas_are_asked_at_once_and_once_a_session() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("asked.log");
        let fake = |name: &str, body: &str| {
            let path = dir.path().join(name);
            std::fs::write(
                &path,
                format!("#!/bin/sh\necho {name} >> '{}'\nsleep 1\n{body}\n", log.display()),
            )
            .unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
            path
        };
        let says = |v: &str| format!("echo 'openjdk version \"{v}\" 2024-07-16' >&2");
        let places = vec![
            fake("stub", "exit 1"),
            fake("j11", &says("11.0.2")),
            fake("j21", &says("21.0.4")),
        ];
        let asked = || {
            let mut lines: Vec<String> = std::fs::read_to_string(&log)
                .unwrap_or_default()
                .lines()
                .map(str::to_owned)
                .collect();
            lines.sort();
            lines
        };

        let started = std::time::Instant::now();
        let found = first_java(places.clone(), Some(17), false).await.unwrap();
        let took = started.elapsed();
        assert_eq!(found.path, places[2].display().to_string());
        assert!(
            took < Duration::from_millis(2500),
            "three Javas of a second each asked at once, not in turn: {took:?}"
        );
        assert_eq!(asked(), ["j11", "j21", "stub"]);

        let started = std::time::Instant::now();
        let again = first_java(places.clone(), Some(17), false).await.unwrap();
        assert_eq!(again, found);
        assert!(started.elapsed() < Duration::from_millis(500), "answered from the session");
        assert_eq!(asked(), ["j11", "j21", "stub"], "none started again");

        // A look again asks only what was no Java; a `java` replaced since is asked again.
        std::fs::write(
            &places[1],
            format!("#!/bin/sh\necho j11 >> '{}'\n{}\n", log.display(), says("17.0.9")),
        )
        .unwrap();
        let file = std::fs::File::options().append(true).open(&places[1]).unwrap();
        file.set_modified(std::time::SystemTime::now() + Duration::from_secs(5))
            .unwrap();
        let looked = first_java(places.clone(), Some(17), true).await.unwrap();
        assert_eq!(
            (looked.release, looked.path),
            (17, places[1].display().to_string()),
            "the replaced one, now new enough and first in order"
        );
        assert_eq!(asked(), ["j11", "j11", "j21", "stub", "stub"]);
    }

    #[test]
    fn versions_and_releases_from_java_version() {
        let modern = "openjdk version \"21.0.4\" 2024-07-16 LTS\nOpenJDK Runtime Environment\n";
        assert_eq!(version_of(modern).as_deref(), Some("21.0.4"));
        let picked =
            "Picked up JAVA_TOOL_OPTIONS: -Dfile.encoding=UTF-8\njava version \"1.8.0_392\"\n";
        assert_eq!(version_of(picked).as_deref(), Some("1.8.0_392"));
        assert_eq!(release_of("1.8.0_392"), Some(8));
        assert_eq!(release_of("21.0.4"), Some(21));
        assert_eq!(release_of("17"), Some(17));
        assert_eq!(release_of("27-ea"), Some(27));
        assert_eq!(
            version_of("The operation couldn't be completed. Unable to locate a Java Runtime."),
            None
        );
    }

    #[test]
    fn the_cache_key_is_the_classes_and_the_decompiler() {
        let a = vec![
            ("A.class".to_owned(), vec![1, 2]),
            ("A$B.class".to_owned(), vec![3]),
        ];
        let mut b = a.clone();
        assert_eq!(
            cache_key(&a, "Vineflower 1.12.0"),
            cache_key(&b, "Vineflower 1.12.0")
        );
        assert_ne!(
            cache_key(&a, "Vineflower 1.12.0"),
            cache_key(&a, "Vineflower 1.13.0")
        );
        b[1].1 = vec![4];
        assert_ne!(
            cache_key(&a, "Vineflower 1.12.0"),
            cache_key(&b, "Vineflower 1.12.0")
        );
        // A byte moved from one name to the next is another key.
        let c = vec![
            ("A.class".to_owned(), vec![1]),
            ("A$B.class".to_owned(), vec![2, 3]),
        ];
        assert_ne!(cache_key(&a, "x"), cache_key(&c, "x"));
    }

    #[test]
    fn the_cache_drops_its_oldest_first() {
        let dir = tempfile::tempdir().unwrap();
        for (i, name) in ["old", "mid", "new"].iter().enumerate() {
            let p = dir.path().join(name);
            std::fs::write(&p, vec![0u8; 100]).unwrap();
            let f = std::fs::File::options().append(true).open(&p).unwrap();
            f.set_modified(std::time::UNIX_EPOCH + Duration::from_secs(1000 + i as u64))
                .unwrap();
        }
        trim_cache(dir.path(), 200);
        assert!(!dir.path().join("old").exists());
        assert!(dir.path().join("mid").exists() && dir.path().join("new").exists());
    }

    #[test]
    fn output_is_found_only_as_a_plain_file() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("com/x")).unwrap();
        std::fs::write(dir.path().join("com/x/Foo.java"), "class Foo {}").unwrap();
        assert_eq!(
            find_output(dir.path(), "Foo.java", 0),
            Some(dir.path().join("com/x/Foo.java"))
        );
        assert_eq!(find_output(dir.path(), "Bar.java", 0), None);
        #[cfg(unix)]
        {
            let other = tempfile::tempdir().unwrap();
            std::fs::write(other.path().join("Baz.java"), "secret").unwrap();
            std::os::unix::fs::symlink(other.path().join("Baz.java"), dir.path().join("Baz.java"))
                .unwrap();
            assert_eq!(
                find_output(dir.path(), "Baz.java", 0),
                None,
                "a link is not followed"
            );
        }
    }
}
