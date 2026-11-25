//! One command's run (sections 7, 14, 19): the spawn at the checkout, its
//! environment, the pipes read into lines, the timeout, the stop and the kill.
//!
//! - **No shell.** The words go to `exec` as *Open in*'s do; a file's name is
//!   one argument whatever it holds.
//! - **The checkout, as the person.** It starts in the repository's top, or
//!   the folder it runs on where the command asks, and sees the working tree
//!   as it is.
//! - **The environment** is the app's less every `KNOBAS_*` variable; then the
//!   person's *Add to PATH* folders in front of `PATH` (the login shell's
//!   where they asked for it), the colour hints, and their own lines, which
//!   win.
//! - **Pipes, never a terminal.** Stdin is closed. Stdout and stderr are read
//!   by a thread each, 64 KiB at a time, into one [`Reader`], and sent in
//!   batches at most every 33 ms.
//! - **Everything it started dies with it** at the timeout, a stop and the
//!   app's quit: its process group on Unix, its tree on Windows.

use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use knobas_core::checkout::oneoff::{self as rules, At, OneOff, StartIn, Stdin};

use super::kept::Kept;
use super::lines::{Line, Reader, Stream};
use super::places::{self, Ctx, Disk, State};
use super::runs::{RunHandle, Runs};

/// What a command is run on.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export_to = "app.ts", rename = "OneOffTarget")]
pub enum Target {
    /// A file from the repository's top, at its selected lines, as the page
    /// shows it at a commit or branch (the run reads the working tree).
    File {
        path: String,
        #[serde(default)]
        #[ts(optional = nullable)]
        lines: Option<(u32, u32)>,
        #[serde(default)]
        #[ts(optional = nullable)]
        at: Option<String>,
    },
    /// A folder from the top, `""` for the top itself.
    Folder {
        path: String,
    },
    Repository,
    Commit {
        sha: String,
    },
    Range {
        base: String,
        head: String,
        #[serde(default)]
        #[ts(optional = nullable)]
        path: Option<String>,
    },
    Files {
        paths: Vec<String>,
    },
}

impl Target {
    /// What a person reads: the file, the folder, `the repository`.
    #[must_use]
    pub fn label(&self) -> String {
        match self {
            Self::File { path, .. } | Self::Folder { path } if !path.is_empty() => path.clone(),
            Self::Folder { .. } | Self::Repository => "the repository".to_owned(),
            Self::File { .. } => "a file".to_owned(),
            Self::Commit { sha } => format!("commit {}", &sha[..sha.len().min(7)]),
            Self::Range { base, head, .. } => format!("{base}..{head}"),
            Self::Files { paths } => format!("{} files", paths.len()),
        }
    }
}

/// How a run ended.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(tag = "how", rename_all = "snake_case")]
#[ts(export_to = "app.ts", rename = "OneOffEnded")]
pub enum Ended {
    Exit { code: i32 },
    Signal { signal: i32 },
    TimedOut { seconds: u32 },
    Stopped,
    NotStarted { reason: String },
}

/// What crosses on the run's channel: the new lines, the lines that changed,
/// and the counts, the last one carrying how it ended.
#[derive(Clone, Debug, Serialize, ts_rs::TS)]
#[ts(export_to = "app.ts", rename = "OneOffBatch")]
pub struct Batch {
    pub run: String,
    pub lines: Vec<Line>,
    pub changed: Vec<Line>,
    /// Lines printed, kept or not.
    pub total: u32,
    /// Lines in the middle no longer kept, and the first line after them.
    pub dropped: u32,
    pub tail_from: u32,
    /// Bytes printed.
    pub bytes: u64,
    pub findings: u32,
    pub elapsed_ms: u64,
    #[ts(optional = nullable)]
    pub ended: Option<Ended>,
    /// What it made that is not its lines, once it ended.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub result: Option<super::result::RunResult>,
}

/// What a start answers.
#[derive(Clone, Debug, Serialize, ts_rs::TS)]
#[ts(export_to = "app.ts", rename = "OneOffStarted")]
pub struct Started {
    pub run: String,
    pub argv: Vec<String>,
    pub cwd: String,
    #[ts(optional = nullable)]
    pub branch: Option<String>,
    #[ts(optional = nullable)]
    pub head: Option<String>,
    pub started_at: i64,
    /// The same command on the same thing was running: this is that run.
    pub already: bool,
}

/// A batch at most this often.
pub const BATCH_EVERY: Duration = Duration::from_millis(33);
/// A batch at most this many lines.
pub const BATCH_LINES: usize = 2000;
/// A batch about this many bytes.
pub const BATCH_BYTES: usize = 256 * 1024;

/// Where a run starts, already checked.
pub struct Placed {
    pub root: PathBuf,
    /// The file, the folder or the batch's files, from the root.
    pub file: Option<String>,
    pub files: Vec<String>,
    pub dir: Option<String>,
    pub lines: Option<(u32, u32)>,
    pub sha: Option<String>,
    pub base: Option<String>,
    pub head: Option<String>,
    pub branch: Option<String>,
    pub commit: Option<String>,
}

/// The environment a command runs in, over `parent`.
#[must_use]
pub fn environment(
    command: &OneOff,
    root: &Path,
    parent: impl IntoIterator<Item = (std::ffi::OsString, std::ffi::OsString)>,
    shell_path: Option<&str>,
) -> Vec<(std::ffi::OsString, std::ffi::OsString)> {
    use std::ffi::OsString;
    let mut env: Vec<(OsString, OsString)> = parent
        .into_iter()
        .filter(|(name, _)| !name.as_encoded_bytes().starts_with(b"KNOBAS_"))
        .collect();
    let set = |env: &mut Vec<(OsString, OsString)>, name: &str, value: OsString| {
        env.retain(|(n, _)| {
            if cfg!(windows) {
                !n.eq_ignore_ascii_case(name)
            } else {
                n != name
            }
        });
        env.push((OsString::from(name), value));
    };
    let path_name = "PATH";
    let base = shell_path.map(OsString::from).or_else(|| {
        env.iter()
            .find(|(n, _)| {
                if cfg!(windows) {
                    n.eq_ignore_ascii_case(path_name)
                } else {
                    n == path_name
                }
            })
            .map(|(_, v)| v.clone())
    });
    let root_text = root.to_string_lossy();
    let mut folders: Vec<PathBuf> = command
        .path
        .iter()
        .map(|f| PathBuf::from(f.replace("{root}", &root_text)))
        .collect();
    if let Some(base) = &base {
        folders.extend(std::env::split_paths(base));
    }
    if let Ok(joined) = std::env::join_paths(&folders) {
        set(&mut env, path_name, joined);
    }
    if command.colours {
        for (name, value) in [
            ("FORCE_COLOR", "1"),
            ("CLICOLOR_FORCE", "1"),
            ("CARGO_TERM_COLOR", "always"),
            ("PY_COLORS", "1"),
            ("TERM", "xterm-256color"),
        ] {
            set(&mut env, name, value.into());
        }
        env.retain(|(n, _)| n != "NO_COLOR");
    } else {
        set(&mut env, "NO_COLOR", "1".into());
        env.retain(|(n, _)| n != "FORCE_COLOR" && n != "CLICOLOR_FORCE");
    }
    if cfg!(windows) {
        set(&mut env, "NoDefaultCurrentDirectoryInExePath", "1".into());
    }
    for var in &command.env {
        set(&mut env, &var.name, var.value.clone().into());
    }
    env
}

/// The argument vector and the working directory of `command` at `placed`.
///
/// # Errors
///
/// The refusal, in the person's terms.
pub fn words_at(
    command: &OneOff,
    placed: &Placed,
    scratch: &Path,
) -> Result<(Vec<String>, PathBuf), String> {
    let at = At {
        root: &placed.root,
        file: placed.file.as_deref(),
        files: &placed.files,
        dir: placed.dir.as_deref(),
        lines: placed.lines,
        sha: placed.sha.as_deref(),
        base: placed.base.as_deref(),
        head: placed.head.as_deref(),
        branch: placed.branch.as_deref(),
        scratch,
        stdin: command.stdin,
        os: std::env::consts::OS,
        wsl: command.wsl_paths && cfg!(windows),
    };
    let argv = rules::fill(&command.command, &at).map_err(|e| e.to_string())?;
    let cwd = match command.start_in {
        StartIn::Root => placed.root.clone(),
        StartIn::Target => {
            let rel = placed
                .dir
                .clone()
                .or_else(|| {
                    placed
                        .file
                        .as_deref()
                        .map(|f| f.rsplit_once('/').map_or("", |(d, _)| d).to_owned())
                })
                .unwrap_or_default();
            rel.split('/')
                .filter(|p| !p.is_empty())
                .fold(placed.root.clone(), |path, part| path.join(part))
        }
    };
    Ok((argv, cwd))
}

/// Where a run's result is: its stdout, or what it wrote to `{out}` or
/// `{outdir}`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wants {
    Stdout,
    Out,
    Outdir,
}

/// What the run's thread is handed.
pub struct Spawn {
    pub argv: Vec<String>,
    pub cwd: PathBuf,
    pub env: Vec<(std::ffi::OsString, std::ffi::OsString)>,
    pub timeout: Duration,
    pub stdin: Option<Vec<u8>>,
    /// For the place finder: the checkout, and the working directory from it.
    pub root: PathBuf,
    pub cwd_rel: String,
    pub wants: Wants,
    pub shows: knobas_core::checkout::oneoff::Shows,
    /// The checkout as WSL sees it, where the command was handed WSL's paths:
    /// the paths it prints map back to the checkout.
    pub alias: Option<String>,
    /// The extension of the file it ran on, for *Code, as the file it ran on*.
    pub ext: Option<String>,
}

enum Chunk {
    Bytes(Stream, Vec<u8>),
    Eof,
}

/// Starts `spawn` for `handle`, answering once the program started or did
/// not; the run goes on in threads of its own and ends into `handle`.
pub fn launch(runs: &Arc<Runs>, handle: &Arc<RunHandle>, spawn: Spawn) {
    let Some((program, arguments)) = spawn.argv.split_first() else {
        runs.end(
            handle,
            Ended::NotStarted {
                reason: "the command has no program in it".to_owned(),
            },
        );
        return;
    };
    let mut built = knobas_db::console::command(program);
    built
        .env_clear()
        .envs(spawn.env.iter().map(|(n, v)| (n, v)))
        .args(arguments)
        .current_dir(&spawn.cwd)
        .stdin(if spawn.stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut built, 0);
    let mut child = match built.spawn() {
        Ok(child) => child,
        Err(error) => {
            let reason = if error.kind() == std::io::ErrorKind::NotFound {
                format!("{program} was not found: is it on the PATH the command sees?")
            } else {
                format!("{program} could not be started: {error}")
            };
            runs.end(handle, Ended::NotStarted { reason });
            return;
        }
    };
    let pid = child.id();
    handle.started(pid);
    #[cfg(windows)]
    handle.set_job(super::win::Job::for_child(&child));
    let (tx, rx) = mpsc::sync_channel::<Chunk>(64);
    let mut readers = Vec::new();
    for (stream, pipe) in [
        (
            Stream::Out,
            child
                .stdout
                .take()
                .map(|p| Box::new(p) as Box<dyn std::io::Read + Send>),
        ),
        (
            Stream::Err,
            child
                .stderr
                .take()
                .map(|p| Box::new(p) as Box<dyn std::io::Read + Send>),
        ),
    ] {
        let Some(mut pipe) = pipe else { continue };
        let tx = tx.clone();
        readers.push(std::thread::spawn(move || {
            let mut buf = vec![0_u8; 64 * 1024];
            loop {
                match pipe.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        if tx.send(Chunk::Bytes(stream, buf[..n].to_vec())).is_err() {
                            break;
                        }
                    }
                }
            }
            let _ = tx.send(Chunk::Eof);
        }));
    }
    drop(tx);
    if let (Some(bytes), Some(mut stdin)) = (spawn.stdin, child.stdin.take()) {
        std::thread::spawn(move || {
            use std::io::Write as _;
            let _ = stdin.write_all(&bytes);
        });
    }
    let runs = Arc::clone(runs);
    let handle = Arc::clone(handle);
    std::thread::spawn(move || {
        let raw = Raw::new(spawn.wants, spawn.shows);
        let ended = consume(
            &handle,
            &mut child,
            &rx,
            spawn.timeout,
            (&spawn.root, &spawn.cwd_rel, spawn.alias.as_deref()),
            &raw,
        );
        handle.kill(Some(pid));
        let _ = child.wait();
        for reader in readers {
            let _ = reader.join();
        }
        let result = result_of(
            &handle,
            &ended,
            spawn.wants,
            spawn.shows,
            spawn.ext.as_deref(),
            raw,
        );
        handle.set_result(result);
        runs.end(&handle, ended);
    });
}

/// A stdout that is the run's result and not its lines: kept as bytes.
pub struct Raw {
    /// `None` until the first chunk of stdout says which it is.
    is_raw: std::cell::Cell<Option<bool>>,
    wants: Wants,
    shows: knobas_core::checkout::oneoff::Shows,
    bytes: std::cell::RefCell<Vec<u8>>,
    over: std::cell::Cell<u64>,
}

impl Raw {
    fn new(wants: Wants, shows: knobas_core::checkout::oneoff::Shows) -> Self {
        Self {
            is_raw: std::cell::Cell::new(None),
            wants,
            shows,
            bytes: std::cell::RefCell::new(Vec::new()),
            over: std::cell::Cell::new(0),
        }
    }

    /// Whether `first`, the first stdout chunk, makes stdout the result: a
    /// command that writes no file and is set to draw a result, or whose
    /// stdout is not text.
    fn decide(&self, first: &[u8]) -> bool {
        use knobas_core::checkout::oneoff::Shows;
        if let Some(known) = self.is_raw.get() {
            return known;
        }
        let raw = self.wants == Wants::Stdout
            && (matches!(
                self.shows,
                Shows::Image
                    | Shows::Pdf
                    | Shows::Page
                    | Shows::Json
                    | Shows::Markdown
                    | Shows::Code
                    | Shows::Diff
            ) || matches!(
                super::result::sniff(&first[..first.len().min(8192)]),
                super::result::Kind::Image { .. }
                    | super::result::Kind::Pdf
                    | super::result::Kind::Bytes
            ));
        self.is_raw.set(Some(raw));
        raw
    }

    fn take(&self, chunk: &[u8]) {
        let mut bytes = self.bytes.borrow_mut();
        let room = usize::try_from(super::result::RESULT_CAP)
            .unwrap_or(usize::MAX)
            .saturating_sub(bytes.len());
        if chunk.len() > room {
            bytes.extend_from_slice(&chunk[..room]);
            self.over.set(self.over.get() + (chunk.len() - room) as u64);
        } else {
            bytes.extend_from_slice(chunk);
        }
    }
}

/// What a run made that is not its lines, once it ended.
fn result_of(
    handle: &RunHandle,
    ended: &Ended,
    wants: Wants,
    shows: knobas_core::checkout::oneoff::Shows,
    ext: Option<&str>,
    raw: Raw,
) -> Option<super::result::RunResult> {
    use super::result::{self as r, RunResult};
    let ok = matches!(ended, Ended::Exit { code: 0 });
    match wants {
        Wants::Out if ok => {
            let (result, path) = r::of_out(&handle.scratch.join("out"), shows);
            handle.keep_file(path);
            Some(result)
        }
        Wants::Outdir if ok => {
            let (result, made) =
                r::of_outdir(&handle.scratch.join("out"), &handle.scratch.join("kept"));
            let _ = std::fs::remove_dir_all(handle.scratch.join("out"));
            handle.keep_made(made);
            Some(result)
        }
        Wants::Out | Wants::Outdir => None,
        Wants::Stdout => {
            if raw.is_raw.get() != Some(true) {
                return None;
            }
            if raw.over.get() > 0 {
                return Some(RunResult::Refused {
                    why: format!(
                        "it printed more than the {} MiB a result may have",
                        r::RESULT_CAP / 1024 / 1024
                    ),
                });
            }
            let bytes = raw.bytes.into_inner();
            let (kind, overruled) = r::detect(&bytes, shows, ext);
            let result = RunResult::One {
                kind,
                size: bytes.len() as u64,
                name: "stdout".to_owned(),
                overruled,
            };
            handle.keep_bytes(bytes);
            Some(result)
        }
    }
}

/// Reads the run's chunks until it ends, stopped, timed out or exited with
/// both pipes closed.
fn consume(
    handle: &RunHandle,
    child: &mut std::process::Child,
    rx: &mpsc::Receiver<Chunk>,
    timeout: Duration,
    (root, cwd_rel, alias): (&Path, &str, Option<&str>),
    raw: &Raw,
) -> Ended {
    let started = Instant::now();
    let deadline = started + timeout;
    let mut reader = Reader::default();
    let mut disk = Disk::new(root);
    let mut state = State::default();
    let at = Where {
        root: root.to_string_lossy().replace('\\', "/"),
        alias: alias.map(str::to_owned),
        cwd: cwd_rel.to_owned(),
    };
    let mut eofs = 0;
    let mut exited: Option<(std::process::ExitStatus, Instant)> = None;
    let mut next_batch = started + BATCH_EVERY;
    loop {
        let wait = next_batch.saturating_duration_since(Instant::now());
        match rx.recv_timeout(wait) {
            Ok(Chunk::Bytes(stream, bytes)) => {
                handle.add_bytes(bytes.len());
                if stream == Stream::Out && raw.decide(&bytes) {
                    raw.take(&bytes);
                } else {
                    let ended = handle.with_kept(|kept| reader.write(kept, stream, &bytes).ended);
                    place(handle, &ended, &mut disk, &mut state, &at);
                }
            }
            Ok(Chunk::Eof) => eofs += 1,
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => eofs = 2,
        }
        let now = Instant::now();
        if now >= next_batch {
            handle.flush(started, None);
            next_batch = now + BATCH_EVERY;
        }
        if handle.stop_asked() {
            handle.kill(Some(child.id()));
            return finish(
                handle,
                &mut reader,
                &mut disk,
                &mut state,
                &at,
                Ended::Stopped,
            );
        }
        if now >= deadline {
            handle.kill(Some(child.id()));
            let seconds = u32::try_from(timeout.as_secs()).unwrap_or(u32::MAX);
            return finish(
                handle,
                &mut reader,
                &mut disk,
                &mut state,
                &at,
                Ended::TimedOut { seconds },
            );
        }
        if exited.is_none()
            && let Ok(Some(status)) = child.try_wait()
        {
            exited = Some((status, now));
        }
        if let Some((status, when)) = exited {
            // Both pipes closed, or a child it left holds them: half a second,
            // then what it started is killed and the run is over.
            if eofs >= 2 || now.duration_since(when) > Duration::from_millis(500) {
                while let Ok(Chunk::Bytes(stream, bytes)) = rx.try_recv() {
                    handle.add_bytes(bytes.len());
                    if stream == Stream::Out && raw.decide(&bytes) {
                        raw.take(&bytes);
                    } else {
                        let ended =
                            handle.with_kept(|kept| reader.write(kept, stream, &bytes).ended);
                        place(handle, &ended, &mut disk, &mut state, &at);
                    }
                }
                let ended = ended_of(status);
                return finish(handle, &mut reader, &mut disk, &mut state, &at, ended);
            }
        }
    }
}

/// Where a run's printed paths are read against.
struct Where {
    root: String,
    alias: Option<String>,
    cwd: String,
}

fn finish(
    handle: &RunHandle,
    reader: &mut Reader,
    disk: &mut Disk,
    state: &mut State,
    at: &Where,
    ended: Ended,
) -> Ended {
    let last = handle.with_kept(|kept| reader.finish(kept).ended);
    place(handle, &last, disk, state, at);
    ended
}

/// The places, finding and addresses of each line that ended.
fn place(handle: &RunHandle, ended: &[u32], disk: &mut Disk, state: &mut State, at: &Where) {
    if ended.is_empty() {
        return;
    }
    let texts: Vec<(u32, String)> = handle.with_kept(|kept| {
        ended
            .iter()
            .filter_map(|n| kept.get(*n).map(|l| (*n, l.text.clone())))
            .collect()
    });
    let mut found = Vec::with_capacity(texts.len());
    for (n, text) in texts {
        let mut ctx = Ctx {
            root: at.root.clone(),
            alias: at.alias.clone(),
            cwd: at.cwd.clone(),
            caseless: cfg!(windows),
            disk,
        };
        let (places, finding, urls) = places::read_line(&text, &mut ctx, state);
        if !places.is_empty() || finding.is_some() || !urls.is_empty() {
            found.push((n, places, finding, urls));
        }
    }
    if found.is_empty() {
        return;
    }
    let findings = found.iter().filter(|(_, _, f, _)| f.is_some()).count();
    handle.with_kept(|kept: &mut Kept| {
        for (n, places, finding, urls) in found {
            if let Some(line) = kept.get_mut(n) {
                let before = line.weight();
                line.places = places;
                line.finding = finding;
                line.urls = urls;
                let after = line.weight();
                kept.reweigh(before, after);
                kept.touch(n);
            }
        }
    });
    handle.add_findings(findings);
}

fn ended_of(status: std::process::ExitStatus) -> Ended {
    if let Some(code) = status.code() {
        return Ended::Exit { code };
    }
    #[cfg(unix)]
    if let Some(signal) = std::os::unix::process::ExitStatusExt::signal(&status) {
        return Ended::Signal { signal };
    }
    Ended::Exit { code: -1 }
}

/// Kills the process group `pid` leads (Unix), or the tree under it
/// (Windows). "No such process" is the usual answer once it ended.
pub fn kill_group(pid: Option<u32>) {
    let Some(pid) = pid else { return };
    #[cfg(unix)]
    if let Some(pid) = i32::try_from(pid)
        .ok()
        .and_then(rustix::process::Pid::from_raw)
    {
        let _ = rustix::process::kill_process_group(pid, rustix::process::Signal::KILL);
    }
    #[cfg(windows)]
    {
        let _ = knobas_db::console::command("taskkill")
            .args(["/T", "/F", "/PID", &pid.to_string()])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

/// The stdin a command reads (section 7, step 8): nothing, the file as it is
/// in the working tree, or its selected lines. `None` closes stdin.
#[must_use]
pub fn stdin_of(stdin: Stdin, placed: &Placed) -> Option<Vec<u8>> {
    let file = placed.file.as_deref()?;
    let path = file
        .split('/')
        .fold(placed.root.clone(), |path, part| path.join(part));
    match stdin {
        Stdin::None => None,
        Stdin::File => std::fs::read(path).ok(),
        Stdin::Selection => {
            let text = std::fs::read(path).ok()?;
            let text = String::from_utf8_lossy(&text);
            let (first, last) = placed.lines.unwrap_or((1, u32::MAX));
            let picked: Vec<&str> = text
                .split_inclusive('\n')
                .skip(first.saturating_sub(1) as usize)
                .take((last.saturating_sub(first) as usize).saturating_add(1))
                .collect();
            Some(picked.concat().into_bytes())
        }
    }
}
