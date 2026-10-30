//! A step in a child process (#2314): the jq step runs in the app's own
//! binary, started with [`ARG`], under the runner command steps use
//! (`crate::renderers::run_argv`, ADR-0033's limits).
//!
//! jaq evaluates, writes, compares and drops values by recursion, and a value
//! built deep by iteration (`reduce range(1e6) as $_ (0; [.])`) overflows the
//! stack below any hook jaq offers. A Rust stack overflow aborts the process,
//! so in the app's process it quit the app. In a child it ends the child, and
//! the step fails with what the child said. The timeout is a real kill too: a
//! filter that loops without writing is gone at its deadline instead of
//! holding a worker thread until the app exits.
//!
//! # The child
//!
//! `knobas --knobas-pipeline-step <cap>`. On stdin: the step as JSON, its
//! length in decimal first and a newline after it, then the input bytes. On
//! stdout: the output. It exits [`EXIT_ERROR`] with the step's error on stderr,
//! or [`EXIT_TOO_LARGE`] at the cap. It never drops what the step built: it
//! exits from the step's thread once the output is written.

use std::io::{Read as _, Write as _};
use std::path::PathBuf;
use std::sync::OnceLock;
use std::sync::atomic::AtomicBool;

use super::steps::{self, Failure};
use super::{Limits, PipeStep, WORKER_STACK};
use crate::renderers::{Ended, run_argv};

/// The first argument that makes the app binary a step's child.
pub const ARG: &str = "--knobas-pipeline-step";
/// The step failed; its error is on stderr.
const EXIT_ERROR: i32 = 3;
/// The step's output passed the cap.
const EXIT_TOO_LARGE: i32 = 4;
/// What Windows exits a process with when its stack overflows.
const WINDOWS_STACK_OVERFLOW: i32 = 0xC000_00FD_u32.cast_signed();

static HELPER: OnceLock<PathBuf> = OnceLock::new();

/// Run steps' children from `program` instead of this process's own binary.
/// For tests, whose binary is the test harness: point it at
/// `env!("CARGO_BIN_EXE_knobas-app")`. The first call wins.
#[cfg(feature = "test-util")]
pub fn use_helper(program: impl Into<PathBuf>) {
    let _ = HELPER.set(program.into());
}

/// The app's own binary. On Linux, `/proc/self/exe`: the running image even
/// after a package update replaced or removed the file at its path (where
/// `current_exe` answers `… (deleted)`). Elsewhere the path the process
/// started from: on macOS the bundle's binary; an app moved or replaced while
/// running fails the step with the reason, or runs the newer binary, whose
/// refusal of a step it cannot read fails the step too, never the app.
fn program() -> Result<PathBuf, String> {
    match HELPER.get() {
        Some(program) => Ok(program.clone()),
        None if cfg!(target_os = "linux") => Ok(PathBuf::from("/proc/self/exe")),
        None => std::env::current_exe()
            .map_err(|error| format!("the app's own binary cannot be found: {error}")),
    }
}

/// `step` over `input` in a child, within `limits`.
pub(super) async fn run(step: &PipeStep, input: &[u8], limits: Limits) -> Result<Vec<u8>, Failure> {
    let program = program().map_err(Failure::Error)?;
    let json = serde_json::to_vec(step).map_err(|error| Failure::Error(error.to_string()))?;
    let mut stdin = Vec::with_capacity(json.len() + input.len() + 16);
    stdin.extend_from_slice(format!("{}\n", json.len()).as_bytes());
    stdin.extend_from_slice(&json);
    stdin.extend_from_slice(input);
    let argv = [
        program.to_string_lossy().into_owned(),
        ARG.to_owned(),
        limits.cap.to_string(),
    ];
    let ended = run_argv(
        &argv,
        stdin,
        crate::renderers::Limits {
            timeout: limits.timeout,
            output_cap: usize::try_from(limits.cap).unwrap_or(usize::MAX),
        },
        std::env::vars_os().collect(),
    )
    .await;
    match ended {
        Ended::Exited {
            status: Some(0),
            stdout,
            ..
        } => Ok(stdout),
        Ended::Exited {
            status: Some(EXIT_ERROR),
            stderr,
            ..
        } => Err(Failure::Error(
            String::from_utf8_lossy(&stderr).trim().to_owned(),
        )),
        Ended::Exited {
            status: Some(EXIT_TOO_LARGE),
            ..
        }
        | Ended::TooLarge => Err(Failure::TooLarge),
        Ended::Exited { status, stderr, .. } => Err(Failure::Error(crashed(status, &stderr))),
        Ended::TimedOut { .. } => Err(Failure::Stopped),
        Ended::NotStarted(reason) => Err(Failure::Error(reason)),
    }
}

/// What a child that died without answering said about it.
fn crashed(status: Option<i32>, stderr: &[u8]) -> String {
    let said = String::from_utf8_lossy(stderr);
    if said.contains("overflowed its stack") || status == Some(WINDOWS_STACK_OVERFLOW) {
        return "jq ran out of stack: the filter built a value nested too deep to write".to_owned();
    }
    let last = said.lines().rev().find(|line| !line.trim().is_empty());
    match (last, status) {
        (Some(line), _) => format!("the step stopped abnormally: {}", line.trim()),
        (None, Some(code)) => format!("the step stopped abnormally with exit code {code}"),
        (None, None) => "the step stopped abnormally, killed by a signal".to_owned(),
    }
}

/// The child's `main`, when the process was started as one: its exit code.
/// `None` when it was not, and the app should start.
#[must_use]
pub fn main_if_asked() -> Option<i32> {
    let mut args = std::env::args_os().skip(1);
    if args.next()? != ARG {
        return None;
    }
    let cap = args
        .next()
        .and_then(|cap| cap.to_str()?.parse::<u64>().ok())
        .unwrap_or(super::SIZE_CAP);
    Some(child(cap))
}

fn child(cap: u64) -> i32 {
    let mut stdin = Vec::new();
    if let Err(error) = std::io::stdin().read_to_end(&mut stdin) {
        return said(&format!("the step's input could not be read: {error}"));
    }
    let Some((step, input)) = framed(stdin) else {
        return said("the step's input is not framed as the app frames it");
    };
    let worker = std::thread::Builder::new()
        .name("knobas-pipeline-step".to_owned())
        .stack_size(WORKER_STACK)
        .spawn(move || {
            let stop = AtomicBool::new(false);
            let code = match steps::run(&step, &input, cap, &stop) {
                Ok(output) => {
                    let mut out = std::io::stdout().lock();
                    if out.write_all(&output).and_then(|()| out.flush()).is_ok() {
                        0
                    } else {
                        EXIT_ERROR
                    }
                }
                Err(Failure::TooLarge) => EXIT_TOO_LARGE,
                Err(Failure::Error(error)) => said(&error),
                Err(Failure::Stopped | Failure::Busy) => said("the step was stopped"),
            };
            // Exit here: what the step built is never dropped, and dropping a
            // deep value recurses as writing one does.
            std::process::exit(code)
        });
    match worker {
        Ok(worker) => {
            let _ = worker.join();
            said("the step's thread ended without an answer")
        }
        Err(error) => said(&format!("no thread for the step: {error}")),
    }
}

fn said(error: &str) -> i32 {
    let _ = writeln!(std::io::stderr(), "{error}");
    EXIT_ERROR
}

/// The step and the input bytes, as [`run`] writes them.
fn framed(stdin: Vec<u8>) -> Option<(PipeStep, Vec<u8>)> {
    let newline = stdin.iter().position(|&byte| byte == b'\n')?;
    let length: usize = std::str::from_utf8(&stdin[..newline]).ok()?.parse().ok()?;
    let start = newline + 1;
    let end = start.checked_add(length)?;
    let step = serde_json::from_slice(stdin.get(start..end)?).ok()?;
    let mut stdin = stdin;
    Some((step, stdin.split_off(end)))
}
