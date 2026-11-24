//! jq kept running for an open document (the XML view's design, 2026-11-23,
//! section 26): the JSON view's field asks the document a filter a key, and
//! a helper that has it read answers, so a key costs the filter alone.
//!
//! - **The helper is the app's own binary as a child** (`super::child`,
//!   [`SESSION_ARG`](super::SESSION_ARG)), as the jq step's is: a filter that
//!   builds a value too deep for the stack ends the child, never the app.
//!   The child reads the document once and keeps its values.
//! - **A filter has [`DEADLINE`]**: past it the child is ended and the answer
//!   says so; the next filter starts another, which reads the document again
//!   from what the session keeps. A child just started is given
//!   [`FIRST_DEADLINE`], which holds its read of the document.
//! - **A session ends** when the view closes it, and after [`IDLE`] with
//!   nothing asked. Each child is killed when its session drops.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

use serde::Serialize;
use tokio::io::{AsyncBufReadExt as _, AsyncReadExt as _, AsyncWriteExt as _, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout};

use super::jq::SessionAnswer;

/// How long a filter may run before its child is ended.
pub const DEADLINE: Duration = Duration::from_millis(1500);
/// How long a child just started may take: it reads the document first.
pub const FIRST_DEADLINE: Duration = Duration::from_secs(20);
/// How long a session lives with nothing asked.
pub const IDLE: Duration = Duration::from_secs(120);

/// A step of a jq path: a member's name or an item's place.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, ts_rs::TS)]
#[serde(untagged)]
#[ts(export_to = "api.ts")]
pub enum JsonPathStep {
    Key(String),
    Index(u32),
}

/// What a filter answered.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, ts_rs::TS)]
#[serde(tag = "state", rename_all = "snake_case")]
#[ts(export_to = "api.ts")]
pub enum JsonQueryAnswer {
    /// Its first values, compact (at most 500), how many it gave, and the
    /// paths of what it picked (at most 5,000; `null` where it makes new
    /// values).
    Answered {
        values: Vec<String>,
        count: u32,
        paths: Option<Vec<Vec<JsonPathStep>>>,
        ms: u32,
    },
    /// The filter does not parse, or fails on the document: jq's words.
    Failed { why: String, ms: u32 },
    /// It was stopped: past the deadline, or its helper ended.
    Stopped { why: String },
}

struct Running {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

struct Session {
    document: Arc<Vec<u8>>,
    running: Option<Running>,
    last: Instant,
}

/// The open sessions, by id.
#[derive(Default)]
pub struct JqSessions {
    sessions: Arc<std::sync::Mutex<HashMap<u32, Arc<tokio::sync::Mutex<Session>>>>>,
    next: AtomicU32,
}

impl JqSessions {
    /// Opens a session over `document`; answers its id. Nothing starts until
    /// the first filter.
    pub fn open(&self, document: Vec<u8>) -> u32 {
        let id = self.next.fetch_add(1, Ordering::Relaxed) + 1;
        let session = Session {
            document: Arc::new(document),
            running: None,
            last: Instant::now(),
        };
        self.map()
            .insert(id, Arc::new(tokio::sync::Mutex::new(session)));
        self.watch(id);
        id
    }

    /// Ends a session and its helper.
    pub fn close(&self, id: u32) {
        self.map().remove(&id);
    }

    /// Whether a session is open (tests).
    #[must_use]
    pub fn is_open(&self, id: u32) -> bool {
        self.map().contains_key(&id)
    }

    /// Whether a session's helper is running (tests): `None` for no session.
    pub async fn helper_running(&self, id: u32) -> Option<bool> {
        let session = self.map().get(&id).cloned()?;
        let mut session = session.lock().await;
        Some(match session.running.as_mut() {
            Some(running) => running.child.try_wait().ok().flatten().is_none(),
            None => false,
        })
    }

    fn map(&self) -> std::sync::MutexGuard<'_, HashMap<u32, Arc<tokio::sync::Mutex<Session>>>> {
        self.sessions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Ends the session after [`IDLE`] with nothing asked.
    fn watch(&self, id: u32) {
        let sessions = Arc::downgrade(&self.sessions);
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            return;
        };
        runtime.spawn(async move {
            loop {
                tokio::time::sleep(IDLE / 4).await;
                let Some(sessions) = sessions.upgrade() else {
                    return;
                };
                let mut map = sessions
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                let Some(session) = map.get(&id) else { return };
                // A session asked right now is not idle.
                let idle = session.try_lock().is_ok_and(|s| s.last.elapsed() >= IDLE);
                if idle {
                    map.remove(&id);
                    return;
                }
            }
        });
    }

    /// Asks session `id` `filter`.
    ///
    /// # Errors
    ///
    /// The session is not open (closed, or ended idle).
    pub async fn query(&self, id: u32, filter: &str) -> Result<JsonQueryAnswer, crate::IpcError> {
        let session =
            self.map().get(&id).cloned().ok_or_else(|| {
                crate::IpcError::not_found("The document's jq session has ended.")
            })?;
        let mut session = session.lock().await;
        session.last = Instant::now();
        let started = Instant::now();
        let fresh = session.running.is_none();
        if fresh {
            match start(&session.document).await {
                Ok(running) => session.running = Some(running),
                Err(why) => return Ok(JsonQueryAnswer::Stopped { why }),
            }
        }
        let running = session.running.as_mut().expect("started above");
        let deadline = if fresh { FIRST_DEADLINE } else { DEADLINE };
        let asked = tokio::time::timeout(deadline, ask(running, filter)).await;
        let ms = u32::try_from(started.elapsed().as_millis()).unwrap_or(u32::MAX);
        match asked {
            Ok(Ok(answer)) => Ok(answered(answer, ms)),
            Ok(Err(why)) => {
                // The helper ended (a stack overflowed): the next filter starts another.
                session.running = None;
                Ok(JsonQueryAnswer::Stopped { why })
            }
            Err(_) => {
                session.running = None;
                Ok(JsonQueryAnswer::Stopped {
                    why: format!(
                        "The filter ran past {:.1} s and was stopped.",
                        deadline.as_secs_f32()
                    ),
                })
            }
        }
    }
}

/// Starts a helper and hands it the document.
async fn start(document: &[u8]) -> Result<Running, String> {
    let program = super::child::program()?;
    let mut child = tokio::process::Command::new(program)
        .arg(super::SESSION_ARG)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|error| format!("jq could not start: {error}"))?;
    let mut stdin = child.stdin.take().ok_or("jq could not start: no stdin")?;
    let stdout = child.stdout.take().ok_or("jq could not start: no stdout")?;
    stdin
        .write_all(format!("{}\n", document.len()).as_bytes())
        .await
        .and(stdin.write_all(document).await)
        .and(stdin.flush().await)
        .map_err(|error| format!("jq could not be given the document: {error}"))?;
    Ok(Running {
        child,
        stdin,
        stdout: BufReader::new(stdout),
    })
}

/// One filter, one answer.
async fn ask(running: &mut Running, filter: &str) -> Result<SessionAnswer, String> {
    let line = serde_json::to_string(filter).map_err(|error| error.to_string())?;
    running
        .stdin
        .write_all(format!("{line}\n").as_bytes())
        .await
        .and(running.stdin.flush().await)
        .map_err(|_| ended())?;
    let mut header = String::new();
    let read = running
        .stdout
        .read_line(&mut header)
        .await
        .map_err(|_| ended())?;
    if read == 0 {
        return Err(ended());
    }
    let length: usize = header.trim().parse().map_err(|_| ended())?;
    let mut json = vec![0; length];
    running
        .stdout
        .read_exact(&mut json)
        .await
        .map_err(|_| ended())?;
    serde_json::from_slice(&json)
        .map_err(|error| format!("jq answered what knobas does not read: {error}"))
}

fn ended() -> String {
    "jq stopped on this filter: it built a value nested too deep, or the document is not JSON."
        .to_owned()
}

fn answered(answer: SessionAnswer, ms: u32) -> JsonQueryAnswer {
    if let Some(why) = answer.error {
        return JsonQueryAnswer::Failed { why, ms };
    }
    let step = |v: serde_json::Value| match v {
        serde_json::Value::String(s) => Some(JsonPathStep::Key(s)),
        serde_json::Value::Number(n) => n
            .as_u64()
            .and_then(|n| u32::try_from(n).ok())
            .map(JsonPathStep::Index),
        _ => None,
    };
    JsonQueryAnswer::Answered {
        values: answer.values,
        count: u32::try_from(answer.count).unwrap_or(u32::MAX),
        paths: answer.paths.map(|paths| {
            paths
                .into_iter()
                .filter_map(|p| p.into_iter().map(step).collect::<Option<Vec<_>>>())
                .collect()
        }),
        ms,
    }
}
