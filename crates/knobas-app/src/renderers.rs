//! User command renderers (#2262, spec #2234 step 5, ADR-0033): a program
//! the person names in Settings › Renderers, which turns a file's text or
//! bytes into SVG, HTML or an image.
//!
//! # One settings row
//!
//! The list is one `knobas.setting` row, [`RENDERERS_KEY`], a JSON array of
//! [`UserRenderer`], for the *Open in…* list's reason (`checkout::open`): a
//! short list a person types once is not a relation. No migration. A row
//! that does not parse reads as no renderers.
//!
//! # The rules of "Open in" (ADR-0016, amended by ADR-0033)
//!
//! - The program and every argument are what the person typed in settings,
//!   split into words by [`knobas_core::checkout::open::words`]; the webview
//!   names a renderer by its id and never sends a program. **No shell**: `;`,
//!   `|` and `$HOME` are literal text in an argument.
//! - The content (a file, a Markdown fence, the test pane's sample) goes on
//!   **stdin only**, never into an argument. It may come from the mirror;
//!   stdin is data the program reads, not a word it runs.
//! - The output is stdout, capped at [`OUTPUT_CAP`]; stderr is kept to
//!   [`STDERR_CAP`]. The run is killed at its timeout (1 to
//!   [`MAX_TIMEOUT_SECS`] seconds, [`DEFAULT_TIMEOUT_SECS`] unless set).
//! - It runs in the system temp directory, never in a checkout, with no
//!   `KNOBAS_*` variable, under `knobas_db::console`'s builder (no window on
//!   Windows).
//!
//! What the frontend does with the output (DOMPurify, a sandboxed frame) is
//! `app/src/lib/render/command.ts`.

use std::process::Stdio;
use std::time::Duration;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};

use crate::IpcError;
use crate::settings::{read_value, write_value};

/// The `knobas.setting` key holding the list.
pub const RENDERERS_KEY: &str = "render.commands";

/// The timeout a renderer gets until the person sets one.
pub const DEFAULT_TIMEOUT_SECS: u32 = 10;
/// The longest timeout a renderer may ask for.
pub const MAX_TIMEOUT_SECS: u32 = 120;
/// The most stdout a run may write; past it the run is stopped.
pub const OUTPUT_CAP: usize = 16 * 1024 * 1024;
/// How much stderr a failure shows; the rest is read and dropped.
pub const STDERR_CAP: usize = 64 * 1024;

/// What a renderer reads on stdin: the file's text, or its bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
#[ts(export_to = "app.ts")]
pub enum RendererInput {
    Text,
    Bytes,
}

/// What a renderer writes on stdout: SVG or HTML (drawn sanitized, in a
/// sandboxed frame), or an image's bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
#[ts(export_to = "app.ts")]
pub enum CommandOutput {
    Svg,
    Html,
    Image,
}

/// One saved command renderer (#2262), Settings › Renderers.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export_to = "app.ts")]
pub struct UserRenderer {
    /// Made from the name when first saved, kept through a rename.
    pub id: String,
    /// What the switcher shows.
    pub name: String,
    /// The program and its arguments, as typed: quotes group words, no shell.
    pub command: String,
    /// File globs it accepts (`*.puml`, `docs/*.uml`).
    pub patterns: Vec<String>,
    /// Markdown fence languages it accepts, lowercase.
    pub fences: Vec<String>,
    pub input: RendererInput,
    pub output: CommandOutput,
    /// Killed after this many seconds.
    pub timeout_secs: u32,
    /// Whether a diff of a file it draws offers a *Rendered* mode: the
    /// file drawn by it twice, before and after side by side (#2279). A
    /// row saved before the field existed reads as `false`.
    #[serde(default)]
    pub diff: bool,
}

/// A renderer as Settings › Renderers sends it: a new one has no id yet, and
/// no timeout is the default.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, ts_rs::TS)]
#[ts(export_to = "app.ts")]
pub struct UserRendererDraft {
    #[serde(default)]
    pub id: Option<String>,
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub patterns: Vec<String>,
    #[serde(default)]
    pub fences: Vec<String>,
    pub input: RendererInput,
    pub output: CommandOutput,
    #[serde(default)]
    pub timeout_secs: Option<u32>,
    #[serde(default)]
    pub diff: bool,
}

/// What goes on stdin: text, or bytes in base64.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, ts_rs::TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export_to = "app.ts")]
pub enum RenderContent {
    Text { text: String },
    Bytes { base64: String },
}

/// How a run ended. A run that failed is an answer, not an error: the viewer
/// draws the source and what the command said.
///
/// - `drawn`: exit 0. `body` is the SVG or HTML as text, or for an image a
///   `data:` URL of the type its first bytes name.
/// - `failed`: a non-zero exit (`status` is `None` for a signal), with stderr.
/// - `timed_out`: stopped at its timeout, with the stderr it had written.
/// - `too_large`: stopped when stdout passed the cap.
/// - `not_started`: the program could not be started (not on `PATH`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, ts_rs::TS)]
#[serde(tag = "outcome", rename_all = "snake_case")]
#[ts(export_to = "app.ts")]
pub enum RenderRun {
    Drawn { output: CommandOutput, body: String },
    Failed { status: Option<i32>, stderr: String },
    TimedOut { seconds: u32, stderr: String },
    TooLarge { cap_bytes: u32 },
    NotStarted { reason: String },
}

/// The saved renderers, in the person's order.
///
/// # Errors
///
/// [`Internal`](crate::IpcErrorCode::Internal) if the read fails.
pub async fn list(pool: &PgPool) -> Result<Vec<UserRenderer>, IpcError> {
    Ok(read_value(pool, RENDERERS_KEY)
        .await?
        .and_then(|value| serde_json::from_value(value).ok())
        .unwrap_or_default())
}

/// Save the whole list, checked and trimmed, and answer it as stored.
///
/// # Errors
///
/// [`Invalid`](crate::IpcErrorCode::Invalid) for a renderer with no name or a
/// command that cannot be run, naming it;
/// [`Internal`](crate::IpcErrorCode::Internal) if the write fails.
pub async fn save(
    pool: &PgPool,
    drafts: &[UserRendererDraft],
) -> Result<Vec<UserRenderer>, IpcError> {
    let settled = settle(drafts)?;
    let value = serde_json::to_value(&settled).map_err(IpcError::internal)?;
    write_value(pool, RENDERERS_KEY, &value).await?;
    Ok(settled)
}

fn settle(drafts: &[UserRendererDraft]) -> Result<Vec<UserRenderer>, IpcError> {
    let mut out: Vec<UserRenderer> = Vec::with_capacity(drafts.len());
    for (index, draft) in drafts.iter().enumerate() {
        let name = draft.name.trim();
        if name.is_empty() {
            return Err(IpcError::invalid(format!(
                "renderer {} has no name",
                index + 1
            )));
        }
        let command = crate::settings::checked_command(name, &draft.command)?;
        let taken: Vec<&str> = out.iter().map(|r| r.id.as_str()).collect();
        let id = crate::slug::settled_id(draft.id.as_deref(), name, "renderer", &taken);
        let list = |items: &[String], lower: bool| -> Vec<String> {
            items
                .iter()
                .map(|item| item.trim())
                .filter(|item| !item.is_empty())
                .map(|item| {
                    if lower {
                        item.to_lowercase()
                    } else {
                        item.to_owned()
                    }
                })
                .collect()
        };
        out.push(UserRenderer {
            id,
            name: name.to_owned(),
            command: command.to_owned(),
            patterns: list(&draft.patterns, false),
            fences: list(&draft.fences, true),
            input: draft.input,
            output: draft.output,
            timeout_secs: draft
                .timeout_secs
                .unwrap_or(DEFAULT_TIMEOUT_SECS)
                .clamp(1, MAX_TIMEOUT_SECS),
            diff: draft.diff,
        });
    }
    Ok(out)
}

/// The `knobas.setting` key holding *Run pipelines on diffs* (#2270): whether
/// a diff runs each file's default pipeline on both sides before diffing.
/// A Settings › Renderers entry, so a row like the rest of them (review of
/// spec #2234, Standards 6); the split/unified toggle in a diff's own toolbar
/// stays the viewer's `localStorage`.
pub const DIFF_PIPELINES_KEY: &str = "render.diff_pipelines";

/// *Run pipelines on diffs* as stored; on until set, and a row that is not a
/// boolean reads as on.
///
/// # Errors
///
/// [`Internal`](crate::IpcErrorCode::Internal) if the read fails.
pub async fn diff_pipelines(pool: &PgPool) -> Result<bool, IpcError> {
    Ok(read_value(pool, DIFF_PIPELINES_KEY)
        .await?
        .and_then(|value| value.as_bool())
        .unwrap_or(true))
}

/// Store *Run pipelines on diffs* and answer what is now stored.
///
/// # Errors
///
/// [`Internal`](crate::IpcErrorCode::Internal) if the write fails.
pub async fn set_diff_pipelines(pool: &PgPool, on: bool) -> Result<bool, IpcError> {
    write_value(pool, DIFF_PIPELINES_KEY, &serde_json::Value::Bool(on)).await?;
    Ok(on)
}

/// The `knobas.setting` key holding the default renderer per file pattern
/// (#2263).
pub const DEFAULTS_KEY: &str = "render.defaults";

/// One default renderer per file pattern (#2263): files `pattern` takes open
/// with `renderer`. Which of several matching patterns decides, and what
/// happens when the renderer is gone, is the frontend's
/// (`app/src/lib/render/defaults.ts`): the backend runs no built-in renderer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export_to = "app.ts")]
pub struct RendererDefault {
    /// A file glob (`*.md`, `docs/*.mmd`), matched without regard to case.
    pub pattern: String,
    /// The renderer's registry id: `source`, `markdown`, `command:<saved id>`.
    /// Empty when the row only sets a pipeline: the pipeline's output picks
    /// its renderer by its own name.
    pub renderer: String,
    /// A named pipeline's id (#2265, `file_pipeline::saved`): files the
    /// pattern takes open through it. By reference, so editing the pipeline
    /// changes every pattern that names it. A row stored before the field
    /// existed reads as none.
    #[serde(default)]
    pub pipeline: Option<String>,
}

/// The default renderers per file pattern, in the person's order.
///
/// # Errors
///
/// [`Internal`](crate::IpcErrorCode::Internal) if the read fails.
pub async fn defaults(pool: &PgPool) -> Result<Vec<RendererDefault>, IpcError> {
    Ok(read_value(pool, DEFAULTS_KEY)
        .await?
        .and_then(|value| serde_json::from_value(value).ok())
        .unwrap_or_default())
}

/// Save the whole list of defaults, trimmed, and answer it as stored.
///
/// # Errors
///
/// [`Invalid`](crate::IpcErrorCode::Invalid) for a row with no pattern, or
/// neither a renderer nor a pipeline, or a pattern listed twice, naming it;
/// [`Internal`](crate::IpcErrorCode::Internal) if the write fails.
pub async fn save_defaults(
    pool: &PgPool,
    list: &[RendererDefault],
) -> Result<Vec<RendererDefault>, IpcError> {
    let mut out: Vec<RendererDefault> = Vec::with_capacity(list.len());
    for (index, row) in list.iter().enumerate() {
        let pattern = row.pattern.trim();
        if pattern.is_empty() {
            return Err(IpcError::invalid(format!(
                "default {} has no file pattern",
                index + 1
            )));
        }
        let renderer = row.renderer.trim();
        let pipeline = row
            .pipeline
            .as_deref()
            .map(str::trim)
            .filter(|id| !id.is_empty());
        if renderer.is_empty() && pipeline.is_none() {
            return Err(IpcError::invalid(format!(
                "{pattern} names no renderer and no pipeline"
            )));
        }
        if out
            .iter()
            .any(|d| d.pattern.to_lowercase() == pattern.to_lowercase())
        {
            return Err(IpcError::invalid(format!(
                "{pattern} has a default already"
            )));
        }
        out.push(RendererDefault {
            pattern: pattern.to_owned(),
            renderer: renderer.to_owned(),
            pipeline: pipeline.map(str::to_owned),
        });
    }
    let value = serde_json::to_value(&out).map_err(IpcError::internal)?;
    write_value(pool, DEFAULTS_KEY, &value).await?;
    Ok(out)
}

/// Run saved renderer `id` with `content` on stdin.
///
/// # Errors
///
/// [`Invalid`](crate::IpcErrorCode::Invalid) for an id that is not saved,
/// bytes that are not base64, or a saved command that no longer splits;
/// [`Internal`](crate::IpcErrorCode::Internal) if the read fails.
pub async fn run(pool: &PgPool, id: &str, content: RenderContent) -> Result<RenderRun, IpcError> {
    let renderer = list(pool)
        .await?
        .into_iter()
        .find(|r| r.id == id)
        .ok_or_else(|| {
            IpcError::invalid(format!(
                "'{id}' is not one of your renderers: see Settings › Renderers"
            ))
        })?;
    let argv = knobas_core::checkout::open::words(&renderer.command).map_err(|error| {
        IpcError::invalid(format!(
            "{}'s command cannot be run: {error}",
            renderer.name
        ))
    })?;
    let stdin = match content {
        RenderContent::Text { text } => text.into_bytes(),
        RenderContent::Bytes { base64 } => STANDARD
            .decode(base64.trim())
            .map_err(|error| IpcError::invalid(format!("the bytes are not base64: {error}")))?,
    };
    let limits = Limits {
        timeout: Duration::from_secs(u64::from(renderer.timeout_secs)),
        output_cap: OUTPUT_CAP,
    };
    let parent: Vec<_> = std::env::vars_os().collect();
    let ended = run_argv(&argv, stdin, limits, parent).await;
    Ok(match ended {
        Ended::Exited {
            status: Some(0),
            stdout,
            ..
        } => drawn(renderer.output, stdout),
        Ended::Exited { status, stderr, .. } => RenderRun::Failed {
            status,
            stderr: lossy(stderr),
        },
        Ended::TimedOut { stderr } => RenderRun::TimedOut {
            seconds: renderer.timeout_secs,
            stderr: lossy(stderr),
        },
        Ended::TooLarge => RenderRun::TooLarge {
            cap_bytes: u32::try_from(OUTPUT_CAP).unwrap_or(u32::MAX),
        },
        Ended::NotStarted(reason) => RenderRun::NotStarted { reason },
    })
}

fn lossy(bytes: Vec<u8>) -> String {
    String::from_utf8(bytes)
        .unwrap_or_else(|error| String::from_utf8_lossy(error.as_bytes()).into_owned())
}

fn drawn(output: CommandOutput, stdout: Vec<u8>) -> RenderRun {
    let body = match output {
        CommandOutput::Svg | CommandOutput::Html => lossy(stdout),
        CommandOutput::Image => format!(
            "data:{};base64,{}",
            image_type(&stdout),
            STANDARD.encode(&stdout)
        ),
    };
    RenderRun::Drawn { output, body }
}

/// The image type its first bytes name; SVG for text that opens with `<`.
fn image_type(bytes: &[u8]) -> &'static str {
    match bytes {
        [0x89, b'P', b'N', b'G', ..] => "image/png",
        [0xFF, 0xD8, 0xFF, ..] => "image/jpeg",
        [b'G', b'I', b'F', b'8', ..] => "image/gif",
        [
            b'R',
            b'I',
            b'F',
            b'F',
            _,
            _,
            _,
            _,
            b'W',
            b'E',
            b'B',
            b'P',
            ..,
        ] => "image/webp",
        _ if bytes.trim_ascii_start().starts_with(b"<") => "image/svg+xml",
        _ => "application/octet-stream",
    }
}

/// The scheme a command renderer's HTML frame is served from (#2304): its
/// own origin, so the app's `style-src 'self'` is not the frame's. A `srcdoc`
/// frame inherits the app's policy, and with it every `style` attribute and
/// `<style>` in the output was refused. The frame keeps `sandbox=""` (no
/// script, an opaque origin); the app's CSP names this scheme in `frame-src`
/// and nothing else changes in it.
pub const FRAME_SCHEME: &str = "knobas-render";

/// The frame document's own policy: inline styles and data images, nothing
/// else. No script runs and no request leaves the frame, whatever reaches it.
pub const FRAME_CSP: &str = "default-src 'none'; style-src 'unsafe-inline'; img-src data:";

/// The answer to `knobas-render://localhost/<document>`: the document is the
/// path, base64url without padding (the shell sanitized it first,
/// `render/frame.ts`), served as HTML under [`FRAME_CSP`]. Stateless: the URL
/// is the document. A path that is no document is a 400 with no body.
#[must_use]
pub fn frame_response(path: &str) -> tauri::http::Response<Vec<u8>> {
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    let document = URL_SAFE_NO_PAD
        .decode(path.trim_start_matches('/'))
        .ok()
        .filter(|bytes| !bytes.is_empty());
    let built = match document {
        Some(body) => tauri::http::Response::builder()
            .status(200)
            .header("content-type", "text/html; charset=utf-8")
            .header("content-security-policy", FRAME_CSP)
            .header("x-content-type-options", "nosniff")
            .header("cache-control", "no-store")
            .body(body),
        None => tauri::http::Response::builder().status(400).body(Vec::new()),
    };
    built.unwrap_or_else(|_| tauri::http::Response::new(Vec::new()))
}

/// How long a run may take and how much it may write.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Limits {
    pub timeout: Duration,
    pub output_cap: usize,
}

/// A pipeline step's limits (#2264) as a run's: one type of limits reaches
/// [`run_argv`], whoever starts the program.
impl From<crate::file_pipeline::Limits> for Limits {
    fn from(step: crate::file_pipeline::Limits) -> Self {
        Self {
            timeout: step.timeout,
            output_cap: usize::try_from(step.cap).unwrap_or(usize::MAX),
        }
    }
}

/// How a program ended.
#[derive(Debug)]
pub(crate) enum Ended {
    Exited {
        status: Option<i32>,
        stdout: Vec<u8>,
        stderr: Vec<u8>,
    },
    TimedOut {
        stderr: Vec<u8>,
    },
    TooLarge,
    NotStarted(String),
}

/// Start `argv[0]` with the rest as its arguments -- **no shell** -- write
/// `stdin` to it, and collect stdout and stderr within `limits`.
/// The environment is `parent`'s less every `KNOBAS_*` variable.
pub(crate) async fn run_argv(
    argv: &[String],
    stdin: Vec<u8>,
    limits: Limits,
    parent: Vec<(std::ffi::OsString, std::ffi::OsString)>,
) -> Ended {
    let Some((program, arguments)) = argv.split_first() else {
        return Ended::NotStarted("the command has no program in it".to_owned());
    };
    let mut built = knobas_db::console::command(program);
    knobas_db::console::scrub_knobas_env(&mut built, parent)
        .args(arguments)
        .current_dir(std::env::temp_dir())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // Its own process group, so a timeout or the cap stops everything it
    // started (a wrapper script's `java`, a `sh -c` pipeline), not only it.
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut built, 0);
    let mut command = tokio::process::Command::from(built);
    command.kill_on_drop(true);
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => return Ended::NotStarted(format!("{program} could not be started: {error}")),
    };
    let (Some(mut input), Some(stdout), Some(stderr)) =
        (child.stdin.take(), child.stdout.take(), child.stderr.take())
    else {
        return Ended::NotStarted("the program's streams could not be opened".to_owned());
    };
    // A program that exits without reading its stdin closes the pipe; the
    // write fails then, and that is not the program's failure.
    let writer = tokio::spawn(async move {
        let _ = input.write_all(&stdin).await;
        drop(input);
    });
    let stderr_task = tokio::spawn(read_capped(stderr, STDERR_CAP, false));
    let collected = tokio::time::timeout(limits.timeout, async {
        let out = read_capped(stdout, limits.output_cap, true).await;
        match out {
            Some(stdout) => Some((stdout, child.wait().await)),
            None => None,
        }
    })
    .await;
    writer.abort();
    let ended = match collected {
        Err(_) => {
            stop(&mut child).await;
            Ended::TimedOut { stderr: Vec::new() }
        }
        Ok(None) => {
            stop(&mut child).await;
            stderr_task.abort();
            return Ended::TooLarge;
        }
        Ok(Some((stdout, status))) => Ended::Exited {
            status: status.ok().and_then(|s| s.code()),
            stdout,
            stderr: Vec::new(),
        },
    };
    // The child is gone, so its stderr is at its end (or a grandchild holds
    // it: give up after a moment rather than wait on it).
    let stderr = tokio::time::timeout(Duration::from_millis(500), stderr_task)
        .await
        .ok()
        .and_then(Result::ok)
        .flatten()
        .unwrap_or_default();
    match ended {
        Ended::TimedOut { .. } => Ended::TimedOut { stderr },
        Ended::Exited { status, stdout, .. } => Ended::Exited {
            status,
            stdout,
            stderr,
        },
        other => other,
    }
}

/// Kill a run that is still going: on Unix its whole process group (it leads
/// one, `process_group(0)`), then the child itself, reaped. Windows kills the
/// child alone.
async fn stop(child: &mut tokio::process::Child) {
    #[cfg(unix)]
    if let Some(pid) = child
        .id()
        .and_then(|id| i32::try_from(id).ok())
        .and_then(rustix::process::Pid::from_raw)
    {
        // The child has not been reaped (no `wait` finished), so its pid is
        // still the group's id and not anyone else's.
        let _ = rustix::process::kill_process_group(pid, rustix::process::Signal::KILL);
    }
    let _ = child.kill().await;
}

/// Everything `reader` writes up to `cap` bytes. Past the cap: `None` when
/// `refuse`, else the first `cap` bytes, the rest read and dropped so the
/// writer never blocks.
async fn read_capped(
    mut reader: impl AsyncRead + Unpin,
    cap: usize,
    refuse: bool,
) -> Option<Vec<u8>> {
    let mut kept = Vec::new();
    let mut chunk = vec![0_u8; 64 * 1024];
    loop {
        let n = reader.read(&mut chunk).await.ok()?;
        if n == 0 {
            return Some(kept);
        }
        let room = cap.saturating_sub(kept.len());
        if n > room {
            if refuse {
                return None;
            }
            kept.extend_from_slice(&chunk[..room]);
        } else {
            kept.extend_from_slice(&chunk[..n]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The frame's own origin (#2304): the document in the path comes back
    /// as HTML under a policy of its own, inline styles and data images only,
    /// so no script and no request leaves it; a path that is no document is
    /// a 400 with nothing in it.
    #[test]
    fn a_frame_document_is_served_under_its_own_policy() {
        let doc = "<!doctype html><p style=\"color: red\">é</p>";
        let path = format!("/{}", base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(doc));
        let answer = frame_response(&path);
        assert_eq!(answer.status(), 200);
        assert_eq!(answer.body().as_slice(), doc.as_bytes());
        let header = |name: &str| answer.headers().get(name).and_then(|v| v.to_str().ok()).map(str::to_owned);
        assert_eq!(header("content-type").as_deref(), Some("text/html; charset=utf-8"));
        assert_eq!(
            header("content-security-policy").as_deref(),
            Some("default-src 'none'; style-src 'unsafe-inline'; img-src data:")
        );
        assert_eq!(header("x-content-type-options").as_deref(), Some("nosniff"));

        // The app frames that scheme and nothing else changes in its policy:
        // its own styles stay `'self'` only.
        let conf = include_str!("../tauri.conf.json");
        assert!(conf.contains("frame-src 'self' knobas-render: http://knobas-render.localhost;"));
        assert!(conf.contains("style-src 'self';") && !conf.contains("unsafe-inline"));

        for bad in ["/not base64!", "/", "/%2F%2F"] {
            let refused = frame_response(bad);
            assert_eq!(refused.status(), 400, "{bad}");
            assert!(refused.body().is_empty());
        }
    }

    /// A pipeline step's limits are a run's: the same timeout, the cap as the
    /// output cap, one past `usize` held at its most.
    #[test]
    fn a_pipeline_steps_limits_are_a_runs_limits() {
        let step = crate::file_pipeline::Limits {
            cap: 7,
            timeout: Duration::from_millis(1500),
        };
        let run = Limits::from(step);
        assert_eq!((run.timeout, run.output_cap), (Duration::from_millis(1500), 7));
        let huge = Limits::from(crate::file_pipeline::Limits {
            cap: u64::MAX,
            timeout: Duration::ZERO,
        });
        assert_eq!(huge.output_cap, usize::try_from(u64::MAX).unwrap_or(usize::MAX));
    }

    fn argv(words: &[&str]) -> Vec<String> {
        words.iter().map(|w| (*w).to_owned()).collect()
    }

    /// A renderer saved before #2279 has no `diff` field: it reads as one
    /// that does not draw diffs, so the stored list still parses.
    #[test]
    fn a_renderer_stored_without_diff_reads_as_not_drawing_diffs() {
        let stored = r#"{"id":"plantuml","name":"PlantUML","command":"plantuml -tsvg -pipe","patterns":["*.puml"],"fences":[],"input":"text","output":"svg","timeout_secs":10}"#;
        let renderer: UserRenderer = serde_json::from_str(stored).expect("a pre-#2279 row parses");
        assert!(!renderer.diff);
    }

    /// A run that writes past the cap is stopped and refused, not cut.
    #[tokio::test]
    async fn output_past_the_cap_is_too_large() {
        let ended = run_argv(
            &argv(&["yes"]),
            Vec::new(),
            Limits {
                timeout: Duration::from_secs(5),
                output_cap: 1000,
            },
            std::env::vars_os().collect(),
        )
        .await;
        assert!(matches!(ended, Ended::TooLarge), "{ended:?}");
    }

    /// A run stopped at its timeout takes what it started with it: a
    /// wrapper's background child is killed too, not left running.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_timed_out_run_takes_its_children_with_it() {
        let marker = std::env::temp_dir().join(format!("knobas-2262-group-{}", std::process::id()));
        let _ = std::fs::remove_file(&marker);
        let ended = run_argv(
            &argv(&[
                "sh",
                "-c",
                "sleep 60 & echo $! > \"$0\"; wait",
                marker.to_str().unwrap(),
            ]),
            Vec::new(),
            Limits {
                timeout: Duration::from_secs(1),
                output_cap: OUTPUT_CAP,
            },
            std::env::vars_os().collect(),
        )
        .await;
        assert!(matches!(ended, Ended::TimedOut { .. }), "{ended:?}");
        let pid = std::fs::read_to_string(&marker).expect("the child wrote its pid");
        let _ = std::fs::remove_file(&marker);
        let pid = pid.trim();
        let alive = || {
            std::process::Command::new("kill")
                .args(["-0", pid])
                .stderr(Stdio::null())
                .status()
                .is_ok_and(|status| status.success())
        };
        // Killed, then reaped by init: give the reaping a moment.
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        while alive() && std::time::Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        assert!(!alive(), "the background sleep {pid} outlived its renderer");
    }

    /// A run gets no `KNOBAS_*` variable (the database URL under `just dev`).
    #[tokio::test]
    async fn a_run_inherits_no_knobas_variable() {
        let parent = vec![
            ("PATH".into(), std::env::var_os("PATH").expect("a PATH")),
            ("KNOBAS_RENDER_TEST_2262".into(), "hunter2".into()),
        ];
        let ended = run_argv(
            &argv(&["env"]),
            Vec::new(),
            Limits {
                timeout: Duration::from_secs(5),
                output_cap: OUTPUT_CAP,
            },
            parent,
        )
        .await;
        let Ended::Exited { stdout, .. } = ended else {
            panic!("{ended:?}")
        };
        let printed = String::from_utf8(stdout).unwrap();
        assert!(!printed.contains("KNOBAS_RENDER_TEST_2262"), "{printed}");
        assert!(printed.contains("PATH="), "{printed}");
    }

    #[test]
    fn an_image_type_is_read_from_its_first_bytes() {
        assert_eq!(image_type(b"\x89PNG\r\n"), "image/png");
        assert_eq!(image_type(b"\xFF\xD8\xFF\xE0"), "image/jpeg");
        assert_eq!(image_type(b"GIF89a"), "image/gif");
        assert_eq!(image_type(b"RIFF\0\0\0\0WEBPVP8 "), "image/webp");
        assert_eq!(image_type(b"  <svg/>"), "image/svg+xml");
        assert_eq!(image_type(b"??"), "application/octet-stream");
    }
}
