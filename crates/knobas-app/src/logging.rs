//! The app's log on disk (#747).
//!
//! Release builds are `windows_subsystem = "windows"` and launched from
//! Finder or the Start menu, so stderr goes nowhere; the 2026-10-13 hang on
//! Windows left no evidence at all. Everything `tracing` says now also lands in
//! a rotating file in Tauri's per-app log directory (`app_log_dir`:
//! `~/Library/Logs/<identifier>` on macOS, `%LOCALAPPDATA%\<identifier>\logs`
//! on Windows, the XDG data dir's `logs` on Linux), in release and dev builds.
//! Stderr stays on for `tauri dev`.
//!
//! ## Why not `tauri-plugin-log` or `tracing-appender`
//!
//! The app already speaks `tracing` end to end (the database and sync crates
//! too), and `tauri-plugin-log` is a `log`-crate sink that would need a bridge
//! and would add a plugin, its permission and an npm half for nothing the
//! webview needs. `tracing-appender` rotates by time only; the ticket asks for a
//! size cap, so the writer here is thirty lines of `std` and a test.
//!
//! ## What is one place
//!
//! * **Slow IPC commands and pool timeouts** are found by [`CommandLog`], a
//!   layer over the spans Tauri's own `tracing` feature wraps around every
//!   command (`ipc::request::handler{cmd}` and `ipc::request::run`), and
//!   [`note_pool_timeout`], called where a `sqlx::Error` becomes an `IpcError`.
//!   No command names itself.
//! * **Secrets** are scrubbed by [`redact`] on every write to the file and to
//!   stderr, so a call site that logs a URL or an error text by mistake is
//!   still safe.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tracing::Subscriber;
use tracing::field::{Field, Visit};
use tracing_subscriber::filter::FilterExt;
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::layer::{Context, SubscriberExt};
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::{EnvFilter, Layer};

/// The live file's name; rotated ones are `knobas.1.log` .. `knobas.<KEEP-1>.log`.
pub const LOG_NAME: &str = "knobas";
/// Size at which the live file is rotated.
pub const MAX_BYTES: u64 = 5 * 1024 * 1024;
/// Files kept, live one included: 5 x 5 MB.
pub const KEEP_FILES: usize = 5;
/// A command slower than this is logged at `warn`.
pub const SLOW_COMMAND: Duration = Duration::from_secs(1);

const HANDLER_SPAN: &str = "ipc::request::handler";
const RUN_SPAN: &str = "ipc::request::run";

// ---------------------------------------------------------------------------
// Rotation
// ---------------------------------------------------------------------------

/// An append-only log file that rotates when the next write would pass `cap`.
pub struct RotatingFile {
    dir: PathBuf,
    cap: u64,
    keep: usize,
    file: File,
    size: u64,
}

impl RotatingFile {
    /// # Errors
    ///
    /// If the directory cannot be created or the file opened.
    pub fn open(dir: &Path, cap: u64, keep: usize) -> io::Result<Self> {
        fs::create_dir_all(dir)?;
        let file = Self::open_live(dir)?;
        let size = file.metadata()?.len();
        Ok(Self {
            dir: dir.to_owned(),
            cap,
            keep: keep.max(1),
            file,
            size,
        })
    }

    fn open_live(dir: &Path) -> io::Result<File> {
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join(format!("{LOG_NAME}.log")))
    }

    fn rotated(&self, n: usize) -> PathBuf {
        self.dir.join(format!("{LOG_NAME}.{n}.log"))
    }

    fn rotate(&mut self) -> io::Result<()> {
        // Oldest first: the last slot is overwritten, each other moves up one.
        for n in (1..self.keep).rev() {
            let from = if n == 1 {
                self.dir.join(format!("{LOG_NAME}.log"))
            } else {
                self.rotated(n - 1)
            };
            if from.exists() {
                let _ = fs::remove_file(self.rotated(n));
                fs::rename(&from, self.rotated(n))?;
            }
        }
        if self.keep == 1 {
            let _ = fs::remove_file(self.dir.join(format!("{LOG_NAME}.log")));
        }
        self.file = Self::open_live(&self.dir)?;
        self.size = 0;
        Ok(())
    }
}

impl Write for RotatingFile {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.size > 0 && self.size + buf.len() as u64 > self.cap {
            self.rotate()?;
        }
        let n = self.file.write(buf)?;
        self.size += n as u64;
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

/// The file sink, attachable after the subscriber exists (the log directory
/// comes from the Tauri app handle, which is built after `run` starts).
/// Lines written before [`Sink::attach`] go to stderr only.
#[derive(Clone, Default)]
pub struct Sink(Arc<Mutex<Option<RotatingFile>>>);

impl Sink {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Start writing to `dir`.
    ///
    /// # Errors
    ///
    /// If the file cannot be opened; logging then stays stderr-only.
    pub fn attach(&self, dir: &Path, cap: u64, keep: usize) -> io::Result<()> {
        let file = RotatingFile::open(dir, cap, keep)?;
        *self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(file);
        Ok(())
    }
}

/// Scrubs each write, then hands it to `inner`.
pub struct Redacting<W>(W);

impl<W: Write> Write for Redacting<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let text = String::from_utf8_lossy(buf);
        self.0.write_all(redact(&text).as_bytes())?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

/// The file half of [`Sink`] as an `io::Write`.
pub struct SinkWriter(Sink);

impl Write for SinkWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut guard = (self.0)
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        match guard.as_mut() {
            Some(file) => file.write(buf),
            None => Ok(buf.len()),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        let mut guard = (self.0)
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        guard.as_mut().map_or(Ok(()), Write::flush)
    }
}

impl<'a> MakeWriter<'a> for Sink {
    type Writer = Redacting<SinkWriter>;
    fn make_writer(&'a self) -> Self::Writer {
        Redacting(SinkWriter(self.clone()))
    }
}

struct StderrMaker;

impl<'a> MakeWriter<'a> for StderrMaker {
    type Writer = Redacting<io::Stderr>;
    fn make_writer(&'a self) -> Self::Writer {
        Redacting(io::stderr())
    }
}

// ---------------------------------------------------------------------------
// Redaction
// ---------------------------------------------------------------------------

/// Words that mark the value after them as a secret.
const SECRET_KEYS: [&str; 9] = [
    "password",
    "passwd",
    "secret",
    "token",
    "authorization",
    "api_key",
    "apikey",
    "api-key",
    "credential",
];

const MASK: &str = "***";

fn is_ident(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'-'
}

fn ends_value(b: u8) -> bool {
    b.is_ascii_whitespace() || matches!(b, b'"' | b'\'' | b'&' | b',' | b';' | b'}' | b')' | b'\\')
}

/// Replace secrets in one log line (or chunk) with `***`.
///
/// Two shapes: `user:password@` in a URL, and `<secret word> = / : <value>`
/// (query strings, `Debug` structs, JSON, `Authorization: Bearer <value>`).
/// Deliberately blunt: a false positive costs one masked word in a log, a
/// false negative costs a credential on disk.
#[must_use]
pub fn redact(text: &str) -> String {
    scrub_keyed(&scrub_userinfo(text))
}

fn scrub_userinfo(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find("://") {
        let (head, tail) = rest.split_at(i + 3);
        out.push_str(head);
        let end = tail
            .find(|c: char| c.is_whitespace() || matches!(c, '/' | '?' | '#' | '"' | '\'' | '\\'))
            .unwrap_or(tail.len());
        let authority = &tail[..end];
        match authority.rfind('@') {
            Some(at) => {
                out.push_str(MASK);
                out.push_str(&authority[at..]);
            }
            None => out.push_str(authority),
        }
        rest = &tail[end..];
    }
    out.push_str(rest);
    out
}

fn scrub_keyed(text: &str) -> String {
    let lower = text.to_ascii_lowercase();
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut pos = 0;
    while pos < text.len() {
        let next = SECRET_KEYS
            .iter()
            .filter_map(|k| lower[pos..].find(k).map(|i| (pos + i, k.len())))
            .min_by_key(|(i, _)| *i);
        let Some((at, klen)) = next else { break };
        // The word ends the key (`access_token`, `credentials`), so
        // `KNOBAS_SECRET_STORE=memory` is not a secret.
        let mut i = at + klen;
        if bytes.get(i) == Some(&b's') {
            i += 1;
        }
        // `"key":` / `key=` / `key: `
        let mut j = i;
        while j < bytes.len() && matches!(bytes[j], b'"' | b'\'' | b'\\') {
            j += 1;
        }
        while j < bytes.len() && bytes[j] == b' ' {
            j += 1;
        }
        if j >= bytes.len() || !matches!(bytes[j], b'=' | b':') {
            out.push_str(&text[pos..i]);
            pos = i;
            continue;
        }
        j += 1;
        while j < bytes.len() && (bytes[j] == b' ' || matches!(bytes[j], b'"' | b'\'' | b'\\')) {
            j += 1;
        }
        let mut end = j;
        while end < bytes.len() && !ends_value(bytes[end]) {
            end += 1;
        }
        // `Bearer <value>` / `Basic <value>`: the word is not the secret.
        let word = lower[j..end].to_owned();
        if (word == "bearer" || word == "basic") && bytes.get(end) == Some(&b' ') {
            end += 1;
            while end < bytes.len() && !ends_value(bytes[end]) {
                end += 1;
            }
        }
        out.push_str(&text[pos..j]);
        if end > j {
            out.push_str(MASK);
        }
        pos = end.max(j);
    }
    out.push_str(&text[pos..]);
    out
}

// ---------------------------------------------------------------------------
// The IPC layer
// ---------------------------------------------------------------------------

struct Cmd(String);
struct Started(Instant);

thread_local! {
    /// The command whose `handler` span this thread opened last.
    ///
    /// Tauri's command macro (tauri-macros 2.6.3) opens `ipc::request::handler`
    /// -- the span that carries `cmd` -- in a block that ends before it opens
    /// `ipc::request::run`, so the run span is not its child and the scope
    /// cannot name the command. The two are created back to back on the same
    /// thread, which is what this hands across. A Tauri that nests them makes
    /// the scope lookup below answer instead; `a_command_through_the_real_pipeline_is_timed_by_name`
    /// fails if neither does.
    static PENDING_CMD: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}

struct CmdVisitor(Option<String>);

impl Visit for CmdVisitor {
    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "cmd" {
            self.0 = Some(value.to_owned());
        }
    }
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "cmd" && self.0.is_none() {
            self.0 = Some(format!("{value:?}").trim_matches('"').to_owned());
        }
    }
}

/// Times IPC commands, logs the slow ones, and remembers each command's name
/// on its spans for [`note_pool_timeout`].
pub struct CommandLog {
    threshold: Duration,
}

impl CommandLog {
    #[must_use]
    pub fn new(threshold: Duration) -> Self {
        Self { threshold }
    }

    /// The layer's own filter: only Tauri's two IPC spans.
    #[must_use]
    pub fn enabled(meta: &tracing::Metadata<'_>) -> bool {
        meta.is_span() && (meta.name() == HANDLER_SPAN || meta.name() == RUN_SPAN)
    }
}

fn command_in_scope<'a, S>(
    scope: impl Iterator<Item = tracing_subscriber::registry::SpanRef<'a, S>>,
) -> String
where
    S: LookupSpan<'a> + 'a,
{
    for span in scope {
        if let Some(cmd) = span.extensions().get::<Cmd>() {
            return cmd.0.clone();
        }
    }
    "(not in a command)".to_owned()
}

impl<S> Layer<S> for CommandLog
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_new_span(
        &self,
        attrs: &tracing::span::Attributes<'_>,
        id: &tracing::span::Id,
        ctx: Context<'_, S>,
    ) {
        let Some(span) = ctx.span(id) else { return };
        match attrs.metadata().name() {
            HANDLER_SPAN => {
                let mut visitor = CmdVisitor(None);
                attrs.record(&mut visitor);
                if let Some(cmd) = visitor.0 {
                    PENDING_CMD.with(|p| *p.borrow_mut() = Some(cmd.clone()));
                    span.extensions_mut().insert(Cmd(cmd));
                }
            }
            RUN_SPAN => {
                let cmd = PENDING_CMD
                    .with(|p| p.borrow_mut().take())
                    .unwrap_or_else(|| command_in_scope::<S>(span.scope()));
                let mut ext = span.extensions_mut();
                ext.insert(Cmd(cmd));
                ext.insert(Started(Instant::now()));
            }
            _ => {}
        }
    }

    fn on_close(&self, id: tracing::span::Id, ctx: Context<'_, S>) {
        let slow = {
            let Some(span) = ctx.span(&id) else { return };
            let ext = span.extensions();
            match (ext.get::<Started>(), ext.get::<Cmd>()) {
                (Some(started), Some(cmd)) if started.0.elapsed() > self.threshold => {
                    Some((cmd.0.clone(), started.0.elapsed()))
                }
                _ => None,
            }
        };
        if let Some((cmd, took)) = slow {
            tracing::warn!(
                command = %cmd,
                duration_ms = u64::try_from(took.as_millis()).unwrap_or(u64::MAX),
                "slow ipc command"
            );
        }
    }
}

/// Record that a pool acquire timed out, if `error` says so, naming the IPC
/// command it happened in. Called where a `sqlx::Error` becomes an `IpcError`,
/// which every command's failure passes -- so no command names itself.
///
/// The command is read from the current span's chain in the registry rather
/// than from a layer callback: an event emitted from inside `on_event` loses
/// its per-layer filter state and reaches no file.
pub(crate) fn note_pool_timeout(error: &sqlx::Error) {
    if !matches!(error, sqlx::Error::PoolTimedOut) {
        return;
    }
    let command = tracing::Span::current()
        .with_subscriber(|(id, dispatch)| {
            let registry = dispatch.downcast_ref::<tracing_subscriber::Registry>()?;
            Some(command_in_scope(registry.span(id)?.scope()))
        })
        .flatten()
        .unwrap_or_else(|| "(not in a command)".to_owned());
    tracing::error!(%command, "database pool acquire timed out");
}

// ---------------------------------------------------------------------------
// Assembly
// ---------------------------------------------------------------------------

/// The subscriber: stderr (optional) and the file, both at `filter`, plus the
/// IPC layer. Separate from [`init`] so tests build the same stack.
pub fn subscriber(
    sink: &Sink,
    stderr: bool,
    filter: &str,
    threshold: Duration,
) -> impl Subscriber + Send + Sync {
    let env = || EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(filter));
    // No spans in the printed lines: Tauri's `tracing` feature wraps setup in a
    // span whose field is the whole `App` (its config, its state), which the
    // default format would print in front of every line inside it.
    let events_only = || tracing_subscriber::filter::filter_fn(|meta| !meta.is_span());
    let stderr_layer = stderr.then(|| {
        tracing_subscriber::fmt::layer()
            .with_target(false)
            .with_writer(StderrMaker)
            .with_filter(env().and(events_only()))
    });
    let file_layer = tracing_subscriber::fmt::layer()
        .with_target(false)
        .with_ansi(false)
        .with_writer(sink.clone())
        .with_filter(env().and(events_only()));
    let ipc_layer = CommandLog::new(threshold)
        .with_filter(tracing_subscriber::filter::filter_fn(CommandLog::enabled));
    tracing_subscriber::registry()
        .with(stderr_layer)
        .with(file_layer)
        .with(ipc_layer)
}

/// Install the global subscriber. Returns the sink to attach the file to once
/// the log directory is known.
#[must_use]
pub fn init() -> Sink {
    let sink = Sink::new();
    // A second install (a test harness, say) is not an error worth dying for.
    let _ = tracing::subscriber::set_global_default(subscriber(&sink, true, "info", SLOW_COMMAND));
    sink
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read_all(dir: &Path) -> String {
        let mut names: Vec<_> = fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect();
        names.sort();
        names
            .iter()
            .map(|p| fs::read_to_string(p).unwrap())
            .collect()
    }

    #[test]
    fn a_slow_command_is_logged_with_its_name() {
        let dir = tempfile::tempdir().unwrap();
        let sink = Sink::new();
        sink.attach(dir.path(), MAX_BYTES, KEEP_FILES).unwrap();
        let sub = subscriber(&sink, false, "info", Duration::from_millis(20));
        tracing::subscriber::with_default(sub, || {
            let handler = tracing::debug_span!("ipc::request::handler", cmd = "list_entities");
            let _h = handler.enter();
            let run = tracing::debug_span!("ipc::request::run");
            {
                let _r = run.enter();
                std::thread::sleep(Duration::from_millis(60));
            }
            drop(run);
            let fast = tracing::debug_span!("ipc::request::run");
            drop(fast);
        });
        let log = read_all(dir.path());
        assert_eq!(log.matches("slow ipc command").count(), 1, "{log}");
        assert!(log.contains("list_entities"), "{log}");
        assert!(log.contains("duration_ms"), "{log}");
    }

    #[test]
    fn a_pool_timeout_is_logged_with_the_running_command() {
        let dir = tempfile::tempdir().unwrap();
        let sink = Sink::new();
        sink.attach(dir.path(), MAX_BYTES, KEEP_FILES).unwrap();
        let sub = subscriber(&sink, false, "info", SLOW_COMMAND);
        tracing::subscriber::with_default(sub, || {
            let handler = tracing::debug_span!("ipc::request::handler", cmd = "save_note");
            let _h = handler.enter();
            let run = tracing::debug_span!("ipc::request::run");
            let _r = run.enter();
            note_pool_timeout(&sqlx::Error::RowNotFound);
            note_pool_timeout(&sqlx::Error::PoolTimedOut);
        });
        let log = read_all(dir.path());
        assert_eq!(log.matches("pool acquire timed out").count(), 1, "{log}");
        assert!(log.contains("save_note"), "{log}");
    }

    #[test]
    fn the_file_rotates_at_the_cap_and_keeps_the_newest() {
        let dir = tempfile::tempdir().unwrap();
        let mut file = RotatingFile::open(dir.path(), 100, 3).unwrap();
        for i in 0..10 {
            file.write_all(format!("{i}{}\n", "x".repeat(39)).as_bytes())
                .unwrap();
        }
        let mut names: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        assert_eq!(names, ["knobas.1.log", "knobas.2.log", "knobas.log"]);
        for name in &names {
            let len = fs::metadata(dir.path().join(name)).unwrap().len();
            assert!(len <= 100, "{name} is {len} bytes");
        }
        let live = fs::read_to_string(dir.path().join("knobas.log")).unwrap();
        assert!(
            live.starts_with('8'),
            "newest lines are in the live file: {live}"
        );
    }

    #[test]
    fn secrets_are_redacted_before_they_reach_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let sink = Sink::new();
        sink.attach(dir.path(), MAX_BYTES, KEEP_FILES).unwrap();
        tracing::subscriber::with_default(subscriber(&sink, false, "info", SLOW_COMMAND), || {
            tracing::info!(
                url = "https://bjoern:hunter2@jira.example.com/rest?x=1",
                "calling"
            );
            tracing::warn!("GET /x?api_key=abc123&page=2 failed");
            tracing::info!(header = "Authorization: Bearer eyJhbGciOi", "sent");
            tracing::info!("KNOBAS_SECRET_STORE=memory is set");
            tracing::info!(config = ?("a", "password: \"s3cret\""), "config");
        });
        let log = read_all(dir.path());
        for leaked in ["hunter2", "abc123", "eyJhbGciOi", "s3cret"] {
            assert!(!log.contains(leaked), "{leaked} leaked: {log}");
        }
        assert!(log.contains("jira.example.com"), "{log}");
        assert!(log.contains("page=2"), "{log}");
        assert!(log.contains("KNOBAS_SECRET_STORE=memory"), "{log}");
    }

    #[test]
    fn redact_leaves_ordinary_text_alone() {
        let line = "sync finished source=jira items=12 token expired";
        assert_eq!(redact(line), line);
    }
}
