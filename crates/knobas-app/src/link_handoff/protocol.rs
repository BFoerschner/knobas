//! Firefox Native Messaging stdio framing and JSON protocol (ADR-0061).
//!
//! Wire format follows Mozilla's WebExtension Native Messaging spec:
//! each message is a 4-byte unsigned 32-bit little-endian byte length followed
//! by that many UTF-8 JSON bytes, capped at 1 MiB.

use std::io::{self, Read, Write};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::link_handoff::LinkHandoffRule;

/// Native Messaging host name registered with Firefox (`^\w+(\.\w+)*$`).
pub const HOST_NAME: &str = "dev.knobas.desktop";

/// Gecko add-on ID of the in-repo Firefox WebExtension (`allowed_extensions`).
pub const EXTENSION_ID: &str = "link-handoff@knobas.dev";

/// Wire protocol version reported by `ping` and `get_rules`.
pub const PROTOCOL_VERSION: u32 = 1;

/// Mozilla Native Messaging maximum message size (1 MiB).
pub const MAX_MESSAGE_BYTES: usize = 1_048_576;

/// Incoming Native Messaging envelope: optional correlation `id` plus typed request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HostEnvelope {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<Value>,
    #[serde(flatten)]
    pub request: HostRequest,
}

/// Requests accepted by the Native Messaging host (and local bridge).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HostRequest {
    Ping,
    GetRules,
    CheckBypass {
        url: String,
        #[serde(default = "default_consume")]
        consume: bool,
    },
    Handoff {
        url: String,
    },
}

fn default_consume() -> bool {
    true
}

/// Result of a `handoff` request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum HandoffAction {
    /// A short-lived *Open in browser* bypass matched and was consumed.
    Bypass,
    /// knobas focused its window and navigated to the native entity view.
    Opened {
        target: HandoffOpenedTarget,
        source_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        entity_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        entity_kind: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        url: Option<String>,
    },
    /// URL was not claimed by knobas; browser should continue loading normally.
    Ignore {
        reason: IgnoreReason,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        source_id: Option<String>,
    },
}

/// Which surface inside knobas opened a handed-off URL.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HandoffOpenedTarget {
    Entity,
    Site,
}

/// Why knobas declined to claim a handed-off URL.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IgnoreReason {
    NoMatchingSource,
    NotKnownItem,
    SiteDisabled,
}

/// Response returned by the Native Messaging host (and local bridge).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HostResponse {
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<Value>,
    #[serde(flatten)]
    pub body: HostResponseBody,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum HostResponseBody {
    Pong {
        #[serde(rename = "type")]
        kind: PongKind,
        protocol_version: u32,
        running: bool,
    },
    Rules {
        #[serde(rename = "type")]
        kind: RulesKind,
        protocol_version: u32,
        rules: Vec<LinkHandoffRule>,
    },
    Bypass {
        #[serde(rename = "type")]
        kind: BypassKind,
        bypassed: bool,
    },
    Handoff {
        #[serde(rename = "type")]
        kind: HandoffKind,
        #[serde(flatten)]
        outcome: HandoffAction,
    },
    Error {
        error: String,
        message: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PongKind {
    Pong,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RulesKind {
    Rules,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BypassKind {
    Bypass,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HandoffKind {
    Handoff,
}

impl HostResponse {
    #[must_use]
    pub fn pong(running: bool) -> Self {
        Self {
            ok: true,
            id: None,
            body: HostResponseBody::Pong {
                kind: PongKind::Pong,
                protocol_version: PROTOCOL_VERSION,
                running,
            },
        }
    }

    #[must_use]
    pub fn rules(rules: Vec<LinkHandoffRule>) -> Self {
        Self {
            ok: true,
            id: None,
            body: HostResponseBody::Rules {
                kind: RulesKind::Rules,
                protocol_version: PROTOCOL_VERSION,
                rules,
            },
        }
    }

    #[must_use]
    pub fn bypass(bypassed: bool) -> Self {
        Self {
            ok: true,
            id: None,
            body: HostResponseBody::Bypass {
                kind: BypassKind::Bypass,
                bypassed,
            },
        }
    }

    #[must_use]
    pub fn handoff(outcome: HandoffAction) -> Self {
        Self {
            ok: true,
            id: None,
            body: HostResponseBody::Handoff {
                kind: HandoffKind::Handoff,
                outcome,
            },
        }
    }

    #[must_use]
    pub fn error(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            ok: false,
            id: None,
            body: HostResponseBody::Error {
                error: code.into(),
                message: message.into(),
            },
        }
    }

    #[must_use]
    pub fn with_id(mut self, id: Option<Value>) -> Self {
        self.id = id;
        self
    }
}

/// Read one 4-byte LE length-prefixed frame. Returns `Ok(None)` on clean EOF
/// before the length header; errors on truncated headers, truncated payloads,
/// or lengths exceeding [`MAX_MESSAGE_BYTES`].
pub fn read_frame<R: Read>(reader: &mut R) -> io::Result<Option<Vec<u8>>> {
    let mut header = [0_u8; 4];
    let mut read = 0;
    while read < 4 {
        match reader.read(&mut header[read..])? {
            0 if read == 0 => return Ok(None),
            0 => {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "truncated native messaging length prefix",
                ));
            }
            n => read += n,
        }
    }
    let len = u32::from_le_bytes(header) as usize;
    if len == 0 || len > MAX_MESSAGE_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("native messaging frame length {len} outside 1..={MAX_MESSAGE_BYTES}"),
        ));
    }
    let mut buf = vec![0_u8; len];
    reader.read_exact(&mut buf)?;
    Ok(Some(buf))
}

/// Write one 4-byte LE length-prefixed frame and flush.
pub fn write_frame<W: Write>(writer: &mut W, payload: &[u8]) -> io::Result<()> {
    if payload.is_empty() || payload.len() > MAX_MESSAGE_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "native messaging frame length {} outside 1..={MAX_MESSAGE_BYTES}",
                payload.len()
            ),
        ));
    }
    let len = u32::try_from(payload.len())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "frame length overflow"))?;
    writer.write_all(&len.to_le_bytes())?;
    writer.write_all(payload)?;
    writer.flush()
}

/// Run the Native Messaging request/response loop over `reader` and `writer`
/// until `reader` reaches clean EOF.
pub fn serve_stdio<R, W, F>(reader: &mut R, writer: &mut W, mut handle: F) -> io::Result<()>
where
    R: Read,
    W: Write,
    F: FnMut(HostRequest) -> HostResponse,
{
    while let Some(raw) = read_frame(reader)? {
        let response = match serde_json::from_slice::<HostEnvelope>(&raw) {
            Ok(envelope) => handle(envelope.request).with_id(envelope.id),
            Err(error) => {
                // Preserve `id` if the frame was valid JSON carrying one.
                let id = serde_json::from_slice::<Value>(&raw)
                    .ok()
                    .and_then(|v| v.get("id").cloned());
                HostResponse::error("invalid_request", format!("malformed request: {error}"))
                    .with_id(id)
            }
        };
        let encoded = serde_json::to_vec(&response)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        write_frame(writer, &encoded)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    fn framed(value: &Value) -> Vec<u8> {
        let mut out = Vec::new();
        let bytes = serde_json::to_vec(value).unwrap();
        write_frame(&mut out, &bytes).unwrap();
        out
    }

    #[test]
    fn stdio_loop_echoes_ids_and_handles_multiple_frames() {
        let mut input = Vec::new();
        input.extend_from_slice(&framed(&serde_json::json!({ "id": 1, "type": "ping" })));
        input.extend_from_slice(&framed(
            &serde_json::json!({ "id": "req-2", "type": "check_bypass", "url": "https://jira.example/browse/PAY-1" }),
        ));
        input.extend_from_slice(&framed(
            &serde_json::json!({ "id": 3, "type": "unknown_op" }),
        ));

        let mut reader = Cursor::new(input);
        let mut output = Vec::new();
        serve_stdio(&mut reader, &mut output, |req| match req {
            HostRequest::Ping => HostResponse::pong(true),
            HostRequest::CheckBypass { consume, .. } => {
                assert!(consume, "consume defaults to true");
                HostResponse::bypass(true)
            }
            _ => HostResponse::error("unexpected", "unexpected"),
        })
        .unwrap();

        let mut out_reader = Cursor::new(output);
        let r1: Value =
            serde_json::from_slice(&read_frame(&mut out_reader).unwrap().unwrap()).unwrap();
        let r2: Value =
            serde_json::from_slice(&read_frame(&mut out_reader).unwrap().unwrap()).unwrap();
        let r3: Value =
            serde_json::from_slice(&read_frame(&mut out_reader).unwrap().unwrap()).unwrap();
        assert!(read_frame(&mut out_reader).unwrap().is_none());

        assert_eq!(r1["ok"], true);
        assert_eq!(r1["id"], 1);
        assert_eq!(r1["type"], "pong");
        assert_eq!(r1["protocol_version"], PROTOCOL_VERSION);
        assert_eq!(r2["id"], "req-2");
        assert_eq!(r2["type"], "bypass");
        assert_eq!(r2["bypassed"], true);
        assert_eq!(r3["ok"], false);
        assert_eq!(r3["id"], 3);
        assert_eq!(r3["error"], "invalid_request");
    }

    #[test]
    fn oversized_and_truncated_frames_are_refused() {
        let mut over = Cursor::new(((MAX_MESSAGE_BYTES as u32) + 1).to_le_bytes().to_vec());
        assert!(read_frame(&mut over).is_err());

        let mut trunc = Cursor::new(vec![10, 0, 0, 0, b'{', b'}']);
        assert!(read_frame(&mut trunc).is_err());

        let mut half_hdr = Cursor::new(vec![1, 0]);
        assert!(read_frame(&mut half_hdr).is_err());
    }
}
