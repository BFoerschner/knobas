//! Between the SPI's Rust types and the WIT world's, the guest half of
//! `knobas-wasm-host`'s `convert`: every type the host decodes is encoded
//! here in the form it decodes, and every type the host encodes is decoded
//! here.

use knobas_source::{ConnectionInfo, SourceError, SyncItem, WriteReceipt, pipeline};

use crate::Instance;
use crate::bindings::knobas::adapter::types as wit;

/// The instance the host constructed the component with.
pub(crate) fn instance(instance: wit::Instance) -> Instance {
    Instance {
        config: serde_json::from_str(&instance.config).unwrap_or(serde_json::Value::Null),
        id: instance.id,
        kind: instance.kind,
        display_name: instance.display_name,
        base_url: instance.base_url,
    }
}

/// The adapter's error, variant for variant.
pub(crate) fn to_wit(error: SourceError) -> wit::SourceError {
    match error {
        SourceError::Unauthorized { status } => wit::SourceError::Unauthorized(status),
        SourceError::Unreachable(detail) => wit::SourceError::Unreachable(detail),
        SourceError::Unconfirmed { status, message } => {
            wit::SourceError::Unconfirmed(wit::Fault { status, message })
        }
        SourceError::Protocol { status, message } => {
            wit::SourceError::Protocol(wit::Fault { status, message })
        }
        SourceError::Sink(detail) => wit::SourceError::Sink(detail),
        SourceError::Moved { base, current } => {
            wit::SourceError::Moved(wit::Moved { base, current })
        }
    }
}

/// A sink's error as the host sent it, so the adapter can return it as it
/// came: [`to_wit`] gives back the very same value.
pub(crate) fn from_wit(error: wit::SourceError) -> SourceError {
    match error {
        wit::SourceError::Unauthorized(status) => SourceError::Unauthorized { status },
        wit::SourceError::Unreachable(detail) => SourceError::Unreachable(detail),
        wit::SourceError::Unconfirmed(wit::Fault { status, message }) => {
            SourceError::Unconfirmed { status, message }
        }
        wit::SourceError::Protocol(wit::Fault { status, message }) => {
            SourceError::Protocol { status, message }
        }
        wit::SourceError::Sink(detail) => SourceError::Sink(detail),
        wit::SourceError::Moved(wit::Moved { base, current }) => {
            SourceError::Moved { base, current }
        }
    }
}

/// RFC 3339, in UTC, with as many fractional digits as the instant has.
fn timestamp(at: chrono::DateTime<chrono::Utc>) -> String {
    at.to_rfc3339_opts(chrono::SecondsFormat::AutoSi, true)
}

/// An item: the entity as `<namespace>:<key>`, the instant as RFC 3339,
/// the payload as its JSON text.
pub(crate) fn item(item: SyncItem) -> wit::SyncItem {
    wit::SyncItem {
        entity: item.entity.to_string(),
        kind: item.kind,
        title: item.title,
        body_text: item.body_text,
        author: item.author,
        updated_at: item.updated_at.map(timestamp),
        payload: item.payload.to_string(),
        web_url: item.web_url,
        deleted: item.deleted,
    }
}

pub(crate) fn connection_info(info: ConnectionInfo) -> wit::ConnectionInfo {
    wit::ConnectionInfo {
        account: info.account,
        server_version: info.server_version,
        secret_expires_at: info.secret_expires_at.map(timestamp),
        detail: info.detail,
        discovered: info.discovered.into_iter().collect(),
    }
}

pub(crate) fn run_chain_step(step: pipeline::RunChainStep) -> wit::RunChainStep {
    wit::RunChainStep {
        pipeline: step.step.pipeline,
        name: step.step.name,
        run: step.run.map(item),
    }
}

pub(crate) fn receipt(receipt: WriteReceipt) -> wit::WriteReceipt {
    wit::WriteReceipt {
        remote_id: receipt.remote_id,
    }
}

/// An SPI value as the serde JSON the host decodes.
pub(crate) fn json<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_string(value).expect("an SPI type serializes")
}

/// The serde JSON of the SPI type `what` names, as the host encoded it.
pub(crate) fn decode<T: serde::de::DeserializeOwned>(
    text: &str,
    what: &str,
) -> Result<T, SourceError> {
    serde_json::from_str(text)
        .map_err(|e| SourceError::protocol(format!("the host's {what} does not decode: {e}")))
}
