//! Native Messaging host CLI submode (`knobas-app native-messaging-host`, ADR-0061).
//!
//! Dispatched before Tauri initializes in `main.rs` so Firefox can spawn the
//! main knobas executable directly as its stdio Native Messaging host without
//! opening a window or bouncing a Dock icon.

use std::io;
use std::path::Path;

use crate::link_handoff::bridge;
use crate::link_handoff::protocol::{
    EXTENSION_ID, HOST_NAME, HostRequest, HostResponse, HostResponseBody, serve_stdio,
};

/// Explicit subcommand argument selecting Native Messaging stdio host mode.
pub const ARG: &str = "native-messaging-host";

/// Check whether `args` (excluding argv[0]) select Native Messaging host mode.
///
/// Matches either the explicit `native-messaging-host` argument or Firefox's
/// standard native-host invocation (`<path-to-manifest.json> <extension-id>`).
#[must_use]
pub fn is_native_host_invocation(args: &[String]) -> bool {
    if args.first().map(String::as_str) == Some(ARG) {
        return true;
    }
    if args.len() >= 2 && args[1] == EXTENSION_ID {
        let lower = args[0].to_ascii_lowercase();
        let expected_suffix = format!("{HOST_NAME}.json");
        if lower.ends_with(&expected_suffix) {
            return true;
        }
    }
    false
}

/// Dispatch one [`HostRequest`] to the running knobas instance via `endpoint_file`.
///
/// For `Ping`, an unreachable desktop instance answers `{ ok: true, type: "pong", running: false }`
/// so the Firefox extension can tell "host installed, app closed" from a broken install.
#[must_use]
pub fn dispatch_to_bridge(endpoint_file: Option<&Path>, request: HostRequest) -> HostResponse {
    let is_ping = matches!(request, HostRequest::Ping);
    let Some(path) = endpoint_file else {
        return if is_ping {
            HostResponse::pong(false)
        } else {
            HostResponse::error("knobas_not_running", "no profile directory available")
        };
    };
    let response = bridge::send_request(path, request);
    if is_ping
        && !response.ok
        && matches!(
            &response.body,
            HostResponseBody::Error { error, .. } if error == "knobas_not_running"
        )
    {
        return HostResponse::pong(false);
    }
    response
}

/// Entrypoint called from `main.rs` before starting Tauri.
///
/// Returns `Some(exit_code)` when started as the Firefox Native Messaging host,
/// or `None` for normal desktop launches.
#[must_use]
pub fn main_if_asked() -> Option<i32> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if !is_native_host_invocation(&args) {
        return None;
    }
    let endpoint = bridge::default_endpoint_file();
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut reader = stdin.lock();
    let mut writer = stdout.lock();
    let code = match serve_stdio(&mut reader, &mut writer, |req| {
        dispatch_to_bridge(endpoint.as_deref(), req)
    }) {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("knobas native-messaging-host: {error}");
            1
        }
    };
    Some(code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_explicit_flag_and_firefox_argv() {
        assert!(is_native_host_invocation(&[ARG.to_owned()]));
        assert!(is_native_host_invocation(&[
            "/Users/dev/Library/Application Support/Mozilla/NativeMessagingHosts/dev.knobas.desktop.json".to_owned(),
            EXTENSION_ID.to_owned(),
        ]));
        assert!(is_native_host_invocation(&[
            r"C:\Users\dev\AppData\Roaming\dev.knobas.desktop\native-messaging\dev.knobas.desktop.json".to_owned(),
            EXTENSION_ID.to_owned(),
        ]));
        assert!(!is_native_host_invocation(&[]));
        assert!(!is_native_host_invocation(&["--demo".to_owned()]));
        assert!(!is_native_host_invocation(&[
            "knobas://open?url=https%3A%2F%2Fjira.example".to_owned()
        ]));
        assert!(!is_native_host_invocation(&[
            "/tmp/dev.knobas.desktop.json".to_owned(),
            "other@example.com".to_owned(),
        ]));
    }
}
