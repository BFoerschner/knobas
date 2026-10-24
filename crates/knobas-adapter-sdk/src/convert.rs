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

#[cfg(test)]
mod tests {
    use chrono::TimeZone;
    use knobas_source::{EntityRef, SourceError, SyncItem};

    use super::wit;

    /// Every `SourceError`, each with what it carries.
    fn every_error() -> Vec<SourceError> {
        vec![
            SourceError::Unauthorized { status: Some(403) },
            SourceError::Unauthorized { status: None },
            SourceError::Unreachable("dns: no such host".into()),
            SourceError::Unconfirmed {
                status: Some(504),
                message: "gateway timeout after the send".into(),
            },
            SourceError::Protocol {
                status: Some(422),
                message: "no such status".into(),
            },
            SourceError::Protocol {
                status: None,
                message: "the adapter's own".into(),
            },
            SourceError::Sink("the mirror is gone".into()),
            SourceError::Moved {
                base: "b7e1".into(),
                current: "# Crane telemetry".into(),
            },
        ]
    }

    /// A sink's error crosses back out of the adapter as it came in, so an
    /// adapter that returns it with `?` returns the host's own error.
    #[test]
    fn a_sink_error_returned_as_it_came_reaches_the_host_unchanged() {
        for error in every_error() {
            let text = format!("{error:?}");
            assert_eq!(format!("{:?}", super::from_wit(super::to_wit(error))), text);
        }
    }

    /// The WIT variant the host decodes each error from, statuses included.
    #[test]
    fn each_error_is_its_own_wit_variant() {
        let wit: Vec<String> = every_error()
            .into_iter()
            .map(|e| format!("{:?}", super::to_wit(e)))
            .collect();
        assert_eq!(
            wit,
            [
                "SourceError::Unauthorized(Some(403))",
                "SourceError::Unauthorized(None)",
                "SourceError::Unreachable(\"dns: no such host\")",
                "SourceError::Unconfirmed(Fault { status: Some(504), message: \"gateway timeout after the send\" })",
                "SourceError::Protocol(Fault { status: Some(422), message: \"no such status\" })",
                "SourceError::Protocol(Fault { status: None, message: \"the adapter's own\" })",
                "SourceError::Sink(\"the mirror is gone\")",
                "SourceError::Moved(Moved { base: \"b7e1\", current: \"# Crane telemetry\" })",
            ]
        );
    }

    /// An item as the host's `sync_item` decodes it: the entity as
    /// `<namespace>:<key>`, the instant in UTC with its fraction kept, the
    /// payload as its JSON text.
    #[test]
    fn an_item_crosses_as_its_entity_id_its_rfc_3339_instant_and_its_payload_text() {
        let at = chrono::Utc
            .with_ymd_and_hms(2026, 10, 18, 9, 0, 0)
            .unwrap()
            + chrono::Duration::milliseconds(250);
        let item = super::item(SyncItem {
            entity: EntityRef::new("surveys", "survey-1"),
            kind: "survey".into(),
            title: "Quay crane 4 inspection".into(),
            body_text: "hoist brake worn".into(),
            author: Some("harbourmaster".into()),
            updated_at: Some(at),
            payload: serde_json::json!({ "key": "survey-1" }),
            web_url: Some("https://surveys.invalid/1".into()),
            deleted: true,
        });
        assert_eq!(item.entity, "surveys:survey-1");
        assert_eq!(item.updated_at.as_deref(), Some("2026-10-18T09:00:00.250Z"));
        assert_eq!(item.payload, r#"{"key":"survey-1"}"#);
        assert_eq!(item.web_url.as_deref(), Some("https://surveys.invalid/1"));
        assert!(item.deleted);
    }

    /// The instance's config arrives as text and reaches the adapter as the
    /// value it spells.
    #[test]
    fn the_instance_config_reaches_the_adapter_as_its_value() {
        let instance = super::instance(wit::Instance {
            id: "surveys".into(),
            kind: "wasm-fixture".into(),
            display_name: "Surveys".into(),
            base_url: "https://surveys.invalid".into(),
            config: r#"{"fault":"none","download_mib":2}"#.into(),
        });
        assert_eq!(instance.id, "surveys");
        assert_eq!(instance.config["download_mib"], 2);
        assert_eq!(instance.config["fault"], "none");
    }
}
