//! Between the SPI's Rust types and the WIT world's.
//!
//! Every type the WIT world names as `json` is decoded here with the SPI's
//! own serde form, so a component's answer is held to exactly what a
//! compiled-in adapter's would be. A component that answers something that
//! does not decode has failed the call, and the call ends with
//! [`SourceError::Protocol`] naming what did not decode.

use knobas_core::entity::EntityRef;
use knobas_source::instance::SourceInstance;
use knobas_source::{ConnectionInfo, SourceError, SyncItem, WriteReceipt};

use crate::wit;

/// The instance the component is constructed with: everything but the
/// credential and the account, which the component never sees (ADR-0028).
pub(crate) fn instance(instance: &SourceInstance) -> wit::Instance {
    wit::Instance {
        id: instance.id.clone(),
        kind: instance.kind.clone(),
        display_name: instance.display_name.clone(),
        base_url: instance.base_url.clone(),
        config: instance.config.to_string(),
    }
}

/// A component's error, variant for variant.
pub(crate) fn error(error: wit::SourceError) -> SourceError {
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

/// The engine's error, as the component receives it from `sink.item`.
pub(crate) fn to_wit_error(error: &SourceError) -> wit::SourceError {
    match error {
        SourceError::Unauthorized { status } => wit::SourceError::Unauthorized(*status),
        SourceError::Unreachable(detail) => wit::SourceError::Unreachable(detail.clone()),
        SourceError::Unconfirmed { status, message } => wit::SourceError::Unconfirmed(wit::Fault {
            status: *status,
            message: message.clone(),
        }),
        SourceError::Protocol { status, message } => wit::SourceError::Protocol(wit::Fault {
            status: *status,
            message: message.clone(),
        }),
        SourceError::Sink(detail) => wit::SourceError::Sink(detail.clone()),
        SourceError::Moved { base, current } => wit::SourceError::Moved(wit::Moved {
            base: base.clone(),
            current: current.clone(),
        }),
    }
}

fn timestamp(
    text: Option<String>,
    what: &str,
) -> Result<Option<chrono::DateTime<chrono::Utc>>, SourceError> {
    text.map(|t| {
        chrono::DateTime::parse_from_rfc3339(&t)
            .map(|at| at.with_timezone(&chrono::Utc))
            .map_err(|e| SourceError::protocol(format!("{what} {t:?} is not RFC 3339: {e}")))
    })
    .transpose()
}

/// A component's sync item. The entity must parse as an `EntityRef`; its
/// namespace is the battery's to check, as it is for every adapter.
pub(crate) fn sync_item(item: wit::SyncItem) -> Result<SyncItem, SourceError> {
    let entity = EntityRef::parse(&item.entity).map_err(|e| {
        SourceError::protocol(format!(
            "item entity {:?} is not an entity id: {e}",
            item.entity
        ))
    })?;
    let payload = serde_json::from_str(&item.payload).map_err(|e| {
        SourceError::protocol(format!("item {} payload is not JSON: {e}", item.entity))
    })?;
    Ok(SyncItem {
        updated_at: timestamp(item.updated_at, "item updated-at")?,
        entity,
        kind: item.kind,
        title: item.title,
        body_text: item.body_text,
        author: item.author,
        payload,
        web_url: item.web_url,
        deleted: item.deleted,
    })
}

pub(crate) fn connection_info(info: wit::ConnectionInfo) -> Result<ConnectionInfo, SourceError> {
    Ok(ConnectionInfo {
        account: info.account,
        server_version: info.server_version,
        secret_expires_at: timestamp(info.secret_expires_at, "secret-expires-at")?,
        detail: info.detail,
        discovered: info.discovered.into_iter().collect(),
    })
}

pub(crate) fn run_chain_step(
    step: wit::RunChainStep,
) -> Result<knobas_source::pipeline::RunChainStep, SourceError> {
    Ok(knobas_source::pipeline::RunChainStep {
        step: knobas_source::pipeline::ChainStep {
            pipeline: step.pipeline,
            name: step.name,
        },
        run: step.run.map(sync_item).transpose()?,
    })
}

pub(crate) fn receipt(receipt: wit::WriteReceipt) -> WriteReceipt {
    WriteReceipt {
        remote_id: receipt.remote_id,
    }
}

/// Decode the serde JSON of the SPI type `what` names.
pub(crate) fn json<T: serde::de::DeserializeOwned>(
    text: &str,
    what: &str,
) -> Result<T, SourceError> {
    serde_json::from_str(text)
        .map_err(|e| SourceError::protocol(format!("the adapter's {what} does not decode: {e}")))
}

/// Encode an SPI value as the serde JSON the component reads.
pub(crate) fn to_json<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_string(value).expect("an SPI type serializes")
}

#[cfg(test)]
mod tests {
    use knobas_source::SourceDescriptor;

    /// A component's descriptor as `descriptor()` answers it, with one kind:
    /// `done` is the `done_statuses` entry, or no key at all.
    fn descriptor_text(done: Option<serde_json::Value>) -> String {
        let mut paths = serde_json::json!({
            "kind": "ticket",
            "status_name": [["status"]],
        });
        if let Some(done) = done {
            paths["done_statuses"] = done;
        }
        serde_json::json!({
            "id": "tickets",
            "adapter_kind": "tickets",
            "name": "Tickets",
            "capabilities": [],
            "adapter_version": "0.1.0",
            "auth_methods": [],
            "write_ops": [],
            "entity_kinds": [{
                "id": "ticket", "label": "Ticket", "plural": "Tickets",
                "monogram": "TK", "full_sync_exhaustive": true,
            }],
            "config_schema": { "type": "object" },
            "payload_paths": [paths],
        })
        .to_string()
    }

    /// An external adapter's done-like set reaches the host (#1501): the
    /// descriptor crosses the WIT as its serde JSON, so `adapter.wit` is
    /// unchanged, a component that declares the set is read with it, and one
    /// built before the field existed decodes as declaring none.
    #[test]
    fn a_components_done_like_set_decodes_and_one_without_it_declares_none() {
        let declared: SourceDescriptor = super::json(
            &descriptor_text(Some(serde_json::json!(["Shipped", "Scrapped"]))),
            "descriptor",
        )
        .unwrap();
        assert_eq!(
            declared.payload_paths[0].done_statuses,
            ["Shipped", "Scrapped"]
        );

        let older: SourceDescriptor = super::json(&descriptor_text(None), "descriptor").unwrap();
        assert!(older.payload_paths[0].done_statuses.is_empty());
    }
}
