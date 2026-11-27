//! Authenticated local IPC bridge between the running knobas desktop app and
//! the short-lived Firefox Native Messaging host process (ADR-0061).
//!
//! - On Unix (macOS/Linux): binds a user-only `0o600` Unix domain socket.
//! - On Windows: binds a loopback TCP listener (`127.0.0.1:0`).
//! - Both transports require a per-session random bearer token (two v4 UUIDs,
//!   244 random bits) stored in `<profile_dir>/link-handoff.json` (mode `0o600`
//!   on Unix) and speak the same 4-byte LE length-prefixed JSON framing as
//!   Native Messaging.
//! - The app serves one request at a time. Each request carries the moment
//!   the host sent it, and the handler is told how long it has left before
//!   the host stops waiting ([`ANSWER_BUDGET`]), so an answer the host no
//!   longer reads is never acted on: Firefox has loaded the page by then.

use std::fs;
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
#[cfg(unix)]
use sha2::{Digest as _, Sha256};

use crate::link_handoff::protocol::{HostRequest, HostResponse, read_frame, write_frame};

/// Environment variable overriding the bridge endpoint descriptor file (for tests).
pub const BRIDGE_ENV: &str = "KNOBAS_LINK_HANDOFF_BRIDGE";

/// Endpoint descriptor filename inside a profile directory.
pub const BRIDGE_FILE: &str = "link-handoff.json";

const IO_TIMEOUT: Duration = Duration::from_secs(2);

/// How long after the host sent a request the app may still act on it: the
/// host's [`IO_TIMEOUT`] less a margin for writing the answer back. Past it
/// the host has answered `knobas_not_running` and Firefox loads the page, so
/// focusing knobas as well would open the link twice.
pub const ANSWER_BUDGET: Duration = Duration::from_millis(1500);

/// Descriptor written by the running knobas instance for the Native Messaging host.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BridgeEndpoint {
    #[serde(flatten)]
    pub transport: BridgeTransport,
    pub token: String,
    pub pid: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "transport", rename_all = "snake_case")]
pub enum BridgeTransport {
    Unix { path: String },
    Tcp { addr: String },
}

#[derive(Debug, Serialize, Deserialize)]
struct BridgeEnvelope {
    token: String,
    request: HostRequest,
    /// When the host sent this, in milliseconds since the Unix epoch. Absent
    /// from an older host, which then gets the whole [`ANSWER_BUDGET`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    sent_at_ms: Option<u64>,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

/// What is left of [`ANSWER_BUDGET`] for a request sent at `sent_at_ms`,
/// seen at `now_ms`. A clock that stepped back counts as no time passed.
#[must_use]
pub fn budget_left(sent_at_ms: Option<u64>, now_ms: u64) -> Duration {
    let waited = sent_at_ms.map_or(0, |sent| now_ms.saturating_sub(sent));
    ANSWER_BUDGET.saturating_sub(Duration::from_millis(waited))
}

/// Running local bridge server owned by the knobas desktop process.
pub struct BridgeServer {
    stop: Arc<AtomicBool>,
    endpoint_file: PathBuf,
    transport: BridgeTransport,
    worker: Option<JoinHandle<()>>,
}

impl BridgeServer {
    /// Start the local bridge server for `endpoint_file`, dispatching each
    /// authenticated [`HostRequest`] to `handler` with the time left before
    /// the host stops waiting for its answer ([`budget_left`]).
    ///
    /// # Errors
    ///
    /// Fails if neither a Unix socket (on Unix) nor a loopback TCP listener
    /// can be bound, or if the endpoint file cannot be written.
    pub fn start<F>(endpoint_file: PathBuf, handler: F) -> io::Result<Self>
    where
        F: Fn(HostRequest, Duration) -> HostResponse + Send + Sync + 'static,
    {
        if let Some(parent) = endpoint_file.parent() {
            fs::create_dir_all(parent)?;
        }
        let token = format!("{:x}{:x}", uuid::Uuid::new_v4(), uuid::Uuid::new_v4());
        let stop = Arc::new(AtomicBool::new(false));
        let handler = Arc::new(handler);

        #[cfg(unix)]
        let (transport, worker) = {
            let sock_path = unix_socket_path(&endpoint_file);
            let _ = fs::remove_file(&sock_path);
            match std::os::unix::net::UnixListener::bind(&sock_path) {
                Ok(listener) => {
                    use std::os::unix::fs::PermissionsExt as _;
                    let _ = fs::set_permissions(&sock_path, fs::Permissions::from_mode(0o600));
                    let transport = BridgeTransport::Unix {
                        path: sock_path.to_string_lossy().into_owned(),
                    };
                    let expected_token = token.clone();
                    let stop_flag = Arc::clone(&stop);
                    let worker = thread::Builder::new()
                        .name("link-handoff-bridge".into())
                        .spawn(move || {
                            accept_unix(listener, &stop_flag, &expected_token, &handler);
                        })?;
                    (transport, worker)
                }
                Err(_) => bind_tcp(&stop, &token, &handler)?,
            }
        };

        #[cfg(not(unix))]
        let (transport, worker) = bind_tcp(&stop, &token, &handler)?;

        let descriptor = BridgeEndpoint {
            transport: transport.clone(),
            token,
            pid: std::process::id(),
        };
        write_endpoint_file(&endpoint_file, &descriptor)?;

        Ok(Self {
            stop,
            endpoint_file,
            transport,
            worker: Some(worker),
        })
    }

    /// Stop the accept loop and remove the endpoint file and socket.
    pub fn stop(mut self) {
        self.stop_inner();
    }

    fn stop_inner(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        // Unblock `accept()` immediately with a dummy local connect.
        match &self.transport {
            #[cfg(unix)]
            BridgeTransport::Unix { path } => {
                let _ = std::os::unix::net::UnixStream::connect(path);
            }
            #[cfg(not(unix))]
            BridgeTransport::Unix { .. } => {}
            BridgeTransport::Tcp { addr } => {
                let _ = std::net::TcpStream::connect(addr);
            }
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        let _ = fs::remove_file(&self.endpoint_file);
        if let BridgeTransport::Unix { path } = &self.transport {
            let _ = fs::remove_file(path);
        }
    }
}

impl Drop for BridgeServer {
    fn drop(&mut self) {
        self.stop_inner();
    }
}

fn bind_tcp<F>(
    stop: &Arc<AtomicBool>,
    token: &str,
    handler: &Arc<F>,
) -> io::Result<(BridgeTransport, JoinHandle<()>)>
where
    F: Fn(HostRequest, Duration) -> HostResponse + Send + Sync + 'static,
{
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    let addr = listener.local_addr()?.to_string();
    let expected_token = token.to_owned();
    let stop_flag = Arc::clone(stop);
    let handler = Arc::clone(handler);
    let worker = thread::Builder::new()
        .name("link-handoff-bridge".into())
        .spawn(move || {
            for incoming in listener.incoming() {
                if stop_flag.load(Ordering::SeqCst) {
                    break;
                }
                let Ok(mut stream) = incoming else {
                    continue;
                };
                let _ = stream.set_read_timeout(Some(IO_TIMEOUT));
                let _ = stream.set_write_timeout(Some(IO_TIMEOUT));
                handle_connection(&mut stream, &expected_token, &*handler);
            }
        })?;
    Ok((BridgeTransport::Tcp { addr }, worker))
}

#[cfg(unix)]
fn accept_unix<F>(
    listener: std::os::unix::net::UnixListener,
    stop: &AtomicBool,
    expected_token: &str,
    handler: &Arc<F>,
) where
    F: Fn(HostRequest, Duration) -> HostResponse + Send + Sync + 'static,
{
    for incoming in listener.incoming() {
        if stop.load(Ordering::SeqCst) {
            break;
        }
        let Ok(mut stream) = incoming else {
            continue;
        };
        let _ = stream.set_read_timeout(Some(IO_TIMEOUT));
        let _ = stream.set_write_timeout(Some(IO_TIMEOUT));
        handle_connection(&mut stream, expected_token, &**handler);
    }
}

fn handle_connection<S, F>(stream: &mut S, expected_token: &str, handler: &F)
where
    S: io::Read + io::Write,
    F: Fn(HostRequest, Duration) -> HostResponse,
{
    let Ok(Some(raw)) = read_frame(stream) else {
        return;
    };
    let response = match serde_json::from_slice::<BridgeEnvelope>(&raw) {
        Ok(env) if constant_time_eq(env.token.as_bytes(), expected_token.as_bytes()) => {
            let left = budget_left(env.sent_at_ms, now_ms());
            if left.is_zero() {
                HostResponse::error("not_ready", "knobas could not answer in time")
            } else {
                handler(env.request, left)
            }
        }
        Ok(_) => HostResponse::error("unauthorized", "bridge token mismatch"),
        Err(error) => HostResponse::error(
            "invalid_request",
            format!("malformed bridge payload: {error}"),
        ),
    };
    if let Ok(encoded) = serde_json::to_vec(&response) {
        let _ = write_frame(stream, &encoded);
    }
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0_u8;
    for (&x, &y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

#[cfg(unix)]
fn unix_socket_path(endpoint_file: &Path) -> PathBuf {
    let beside = endpoint_file.with_file_name("lh.sock");
    // macOS `sockaddr_un.sun_path` is 104 bytes including NUL.
    if beside.as_os_str().len() < 96 {
        return beside;
    }
    let digest = Sha256::digest(endpoint_file.to_string_lossy().as_bytes());
    let hex: String = digest.iter().take(8).map(|b| format!("{b:02x}")).collect();
    PathBuf::from(format!("/tmp/knobas-lh-{hex}.sock"))
}

fn write_endpoint_file(path: &Path, descriptor: &BridgeEndpoint) -> io::Result<()> {
    let json = serde_json::to_vec(descriptor)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    let staging = path.with_extension(format!("json.{}.tmp", std::process::id()));
    let _ = fs::remove_file(&staging);
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    // Made user-only, never readable by others for a moment first: the token
    // in it is what lets a process drive handoffs.
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    let mut file = options.open(&staging)?;
    file.write_all(&json)?;
    file.sync_all()?;
    drop(file);
    fs::rename(&staging, path)
}

/// Send one request to the running knobas desktop instance described by
/// `endpoint_file`, returning `knobas_not_running` if unavailable.
#[must_use]
pub fn send_request(endpoint_file: &Path, request: HostRequest) -> HostResponse {
    let raw = match fs::read_to_string(endpoint_file) {
        Ok(raw) => raw,
        Err(_) => {
            return HostResponse::error("knobas_not_running", "knobas is not running");
        }
    };
    let Ok(endpoint) = serde_json::from_str::<BridgeEndpoint>(&raw) else {
        return HostResponse::error("knobas_not_running", "bridge descriptor is unreadable");
    };
    let envelope = BridgeEnvelope {
        token: endpoint.token,
        request,
        sent_at_ms: Some(now_ms()),
    };
    let Ok(payload) = serde_json::to_vec(&envelope) else {
        return HostResponse::error("internal", "failed to encode request");
    };

    let answer = match endpoint.transport {
        #[cfg(unix)]
        BridgeTransport::Unix { path } => {
            std::os::unix::net::UnixStream::connect(&path).and_then(|mut stream| {
                stream.set_read_timeout(Some(IO_TIMEOUT))?;
                stream.set_write_timeout(Some(IO_TIMEOUT))?;
                write_frame(&mut stream, &payload)?;
                read_frame(&mut stream)
            })
        }
        #[cfg(not(unix))]
        BridgeTransport::Unix { .. } => Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "unix sockets unsupported",
        )),
        BridgeTransport::Tcp { addr } => {
            let parsed: Result<std::net::SocketAddr, _> = addr.parse();
            match parsed {
                Ok(sock_addr) => std::net::TcpStream::connect_timeout(&sock_addr, IO_TIMEOUT)
                    .and_then(|mut stream| {
                        stream.set_read_timeout(Some(IO_TIMEOUT))?;
                        stream.set_write_timeout(Some(IO_TIMEOUT))?;
                        write_frame(&mut stream, &payload)?;
                        read_frame(&mut stream)
                    }),
                Err(error) => Err(io::Error::new(io::ErrorKind::InvalidInput, error)),
            }
        }
    };

    match answer {
        Ok(Some(bytes)) => serde_json::from_slice(&bytes).unwrap_or_else(|error| {
            HostResponse::error("internal", format!("bad response: {error}"))
        }),
        _ => HostResponse::error("knobas_not_running", "knobas is not running"),
    }
}

/// Default endpoint file path for the Native Messaging host process.
#[must_use]
pub fn default_endpoint_file() -> Option<PathBuf> {
    if let Some(override_path) = std::env::var_os(BRIDGE_ENV)
        && !override_path.is_empty()
    {
        return Some(PathBuf::from(override_path));
    }
    Some(
        dirs::data_dir()?
            .join(crate::APP_IDENTIFIER)
            .join(BRIDGE_FILE),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stream that reads `input` and records what is written.
    struct Duplex {
        input: io::Cursor<Vec<u8>>,
        output: Vec<u8>,
    }

    impl io::Read for Duplex {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            self.input.read(buf)
        }
    }

    impl io::Write for Duplex {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.output.write(buf)
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    fn serve(sent_at_ms: Option<u64>) -> (HostResponse, Option<Duration>) {
        let envelope = BridgeEnvelope {
            token: "t".into(),
            request: HostRequest::Ping,
            sent_at_ms,
        };
        let mut input = Vec::new();
        write_frame(&mut input, &serde_json::to_vec(&envelope).unwrap()).unwrap();
        let mut stream = Duplex {
            input: io::Cursor::new(input),
            output: Vec::new(),
        };
        let seen = std::cell::Cell::new(None);
        handle_connection(&mut stream, "t", &|_req, left| {
            seen.set(Some(left));
            HostResponse::pong(true)
        });
        let raw = read_frame(&mut io::Cursor::new(stream.output))
            .unwrap()
            .unwrap();
        (serde_json::from_slice(&raw).unwrap(), seen.get())
    }

    #[test]
    fn the_budget_shrinks_with_the_wait_and_never_grows_on_a_clock_step() {
        assert_eq!(budget_left(None, 10_000), ANSWER_BUDGET);
        assert_eq!(
            budget_left(Some(10_000), 10_400),
            ANSWER_BUDGET - Duration::from_millis(400)
        );
        assert_eq!(budget_left(Some(10_000), 12_000), Duration::ZERO);
        assert_eq!(budget_left(Some(10_000), 9_000), ANSWER_BUDGET);
    }

    #[test]
    fn a_request_the_host_stopped_waiting_for_never_reaches_the_handler() {
        let (fresh, left) = serve(Some(now_ms()));
        assert!(fresh.ok);
        assert!(left.is_some_and(|left| left > Duration::ZERO && left <= ANSWER_BUDGET));

        let (stale, left) = serve(Some(now_ms() - 5_000));
        assert!(!stale.ok);
        assert_eq!(left, None, "a late request must not be acted on");
    }
}
