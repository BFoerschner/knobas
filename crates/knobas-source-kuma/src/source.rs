//! The `Source` implementation: what the sync engine and `test_source` hold.

use knobas_source::instance::SourceInstance;
use knobas_source::{
    ConnectionInfo, Cursor, Sink, Source, SourceDescriptor, SourceError, SyncItem, WriteOp,
    WriteReceipt,
};

use crate::cursor::Position;
use crate::http::KumaHttp;
use crate::{KumaConfig, descriptor_template, http, map, metrics, model};

/// One configured Uptime Kuma instance.
pub struct KumaSource {
    /// The instance id: the `EntityRef` namespace of every item this source
    /// emits, immutable once chosen (P10).
    id: String,
    display_name: String,
    /// Without a trailing slash, so a monitor's `/dashboard/<id>` composes
    /// cleanly onto it.
    base_url: String,
    http: KumaHttp,
}

/// The base URL as `SyncItem::web_url` is built from it: trimmed of
/// surrounding whitespace and of the trailing slash a paste usually carries.
fn trimmed_base_url(raw: &str) -> String {
    raw.trim().trim_end_matches('/').to_owned()
}

/// Build an adapter from a stored source configuration plus the API key read
/// out of the keychain (contract §4.2).
///
/// Everything that can be wrong with the configuration is wrong *here* -- a
/// missing key, an authentication method `/metrics` does not speak, a base URL
/// that is not one -- so a scheduled run never discovers it mid-sync.
///
/// # Errors
///
/// [`SourceError::Unauthorized`] when the keychain holds no key, and
/// [`SourceError::Protocol`] for a configuration that cannot work: no
/// authentication method, one this adapter does not support, a base URL that is
/// not an http(s) URL, or a config blob this adapter's schema does not
/// describe.
pub fn build(instance: SourceInstance) -> Result<Box<dyn Source>, SourceError> {
    let cfg = KumaConfig::from_json(&instance.config)?;
    let credential = http::credential(instance.auth, instance.secret.as_deref())?;
    let http = KumaHttp::new(&instance.base_url, &cfg, credential)?;
    Ok(Box::new(KumaSource {
        id: instance.id,
        display_name: instance.display_name,
        base_url: trimmed_base_url(&instance.base_url),
        http,
    }))
}

#[async_trait::async_trait]
impl Source for KumaSource {
    fn descriptor(&self) -> SourceDescriptor {
        SourceDescriptor {
            id: self.id.clone(),
            name: self.display_name.clone(),
            ..descriptor_template()
        }
    }

    /// `GET /metrics`, and what it says.
    ///
    /// The same request a sync makes, which is the point: this adapter has one
    /// endpoint, so *Test connection* exercises exactly the path a run will
    /// take and a key that passes here cannot fail there for a reason about the
    /// credential.
    ///
    /// **No account.** An API key belongs to the instance rather than to a
    /// person -- Kuma mints it in its own settings, and `/metrics` never says
    /// whose it is -- so `account` is `None` and the sources view says nothing
    /// about it rather than something wrong. `ConnectionInfo` is explicitly
    /// "every field is optional and an adapter fills only what its API actually
    /// exposes".
    ///
    /// **The version and the roster size** are what it *can* say: `app_version`
    /// is a gauge in the same document, and the monitor count is the one number
    /// that tells a key with no monitors behind it from a key pointed at the
    /// wrong Kuma.
    async fn test_connection(&self) -> Result<ConnectionInfo, SourceError> {
        let body = self.http.metrics().await?;
        let samples = metrics::parse(&body);
        let count = model::monitors(&samples).len();
        Ok(ConnectionInfo {
            account: None,
            server_version: samples
                .iter()
                .find(|s| s.name == model::APP_VERSION)
                .and_then(|s| s.label("version"))
                .map(str::to_owned),
            // Kuma's API keys can carry an expiry, but `/metrics` does not
            // publish it and this adapter has no other channel. The
            // credential-health strip shows a countdown the day one exists.
            secret_expires_at: None,
            detail: Some(match count {
                1 => "1 monitor".to_owned(),
                other => format!("{other} monitors"),
            }),
            // Nothing per-instance for the dialog to fill: this adapter's
            // config keys are transport tuning, all of them the reader's own
            // choices rather than ids the server mints (#297).
            discovered: std::collections::BTreeMap::new(),
        })
    }

    /// One `GET /metrics`, folded into monitors, compared against the cursor.
    ///
    /// **Every run reads the whole document** -- there is nothing else to read,
    /// and no way to ask Kuma for a subset. What the cursor decides is whether
    /// anything is *emitted*: an unchanged corpus emits nothing and hands the
    /// cursor straight back, and any difference at all emits every monitor plus
    /// a tombstone for each one that has gone. [`crate::cursor`] is where that
    /// is argued out, including why the deletion is reported here rather than
    /// left to the engine's sweep.
    async fn sync(
        &self,
        cursor: Option<Cursor>,
        sink: &mut (dyn Sink + Send),
    ) -> Result<Cursor, SourceError> {
        let body = self.http.metrics().await?;
        let monitors = model::monitors(&metrics::parse(&body));
        let items: Vec<SyncItem> = monitors
            .iter()
            .map(|monitor| map::to_sync_item(&self.id, &self.base_url, monitor))
            .collect();
        let current = Position::of(
            items
                .iter()
                .map(|item| (item.entity.key.as_str(), item.title.as_str(), &item.payload)),
        );

        let previous = cursor.as_deref().and_then(Position::parse);
        if let (Some(raw), Some(before)) = (cursor.as_ref(), previous.as_ref())
            && *before == current
        {
            // The cursor the caller handed in, byte for byte, rather than a
            // re-encoding of an equal position: the engine compares the two
            // strings to decide whether the run is worth an activity line.
            return Ok(raw.clone());
        }

        for item in items {
            sink.item(item).await?;
        }
        if let Some(before) = previous.as_ref() {
            for (id, name) in before.vanished(&current) {
                sink.item(map::tombstone(&self.id, id, name)).await?;
            }
        }
        Ok(current.encode())
    }

    /// Refused, by name, whatever it is.
    ///
    /// This adapter declares **no** write ops, so there is no arm to take and
    /// no variant to miss: the SPI's rule is that an op absent from
    /// `descriptor.write_ops` is rejected with `Protocol` rather than
    /// attempted, and every op is absent from an empty list. The exhaustive
    /// `match` its siblings carry exists so that a *new* `WriteOp` variant
    /// forces a decision in an adapter that performs some of them; here the
    /// decision is already made and would be the same for every future
    /// variant.
    ///
    /// Pause, resume and create monitor are Kuma's socket.io channel and a
    /// ticket of their own (spec #427, *The Kuma adapter*). When they land,
    /// this grows a real dispatch and the descriptor grows `Capability::Write`
    /// with them.
    async fn write(&self, op: WriteOp) -> Result<WriteReceipt, SourceError> {
        Err(SourceError::protocol(format!(
            "the Uptime Kuma adapter is read-only and declares no write ops, so it cannot perform \
             {:?}",
            op.identifier()
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use knobas_source::AuthMethod;

    fn instance(config: serde_json::Value, secret: Option<&str>) -> SourceInstance {
        SourceInstance {
            id: "kuma".to_owned(),
            kind: crate::ADAPTER_KIND.to_owned(),
            display_name: "Uptime Kuma".to_owned(),
            base_url: "http://127.0.0.1:3001/".to_owned(),
            auth: Some(AuthMethod::ApiToken),
            secret: secret.map(str::to_owned),
            config,
        }
    }

    /// A source that cannot work is refused when it is saved, not on the first
    /// scheduled run.
    #[test]
    fn a_configuration_that_cannot_work_is_refused_at_build_time() {
        assert!(matches!(
            build(instance(serde_json::json!({}), None)),
            Err(SourceError::Unauthorized { status: None })
        ));
        assert!(matches!(
            build(instance(
                serde_json::json!({ "rate_per_sec": 0 }),
                Some("uk1_secret")
            )),
            Err(SourceError::Protocol { .. })
        ));
        let mut bad_url = instance(serde_json::json!({}), Some("uk1_secret"));
        bad_url.base_url = "127.0.0.1:3001".to_owned();
        assert!(matches!(build(bad_url), Err(SourceError::Protocol { .. })));
    }

    /// The instance's own id and name ride on the descriptor; everything else
    /// is the template's.
    #[test]
    fn the_instances_descriptor_is_the_template_under_its_own_name() {
        let mut two = instance(serde_json::json!({}), Some("uk1_secret"));
        two.id = "kuma-eu".to_owned();
        two.display_name = "Kuma EU".to_owned();
        let d = build(two).unwrap().descriptor();
        assert_eq!(d.id, "kuma-eu");
        assert_eq!(d.name, "Kuma EU");
        assert_eq!(d.adapter_kind, crate::ADAPTER_KIND);
        assert_eq!(d.entity_kinds[0].id, crate::KIND_MONITOR);
    }

    /// The base URL a monitor's link is composed onto: the trailing slash a
    /// paste carries would otherwise double.
    #[test]
    fn the_base_url_loses_its_trailing_slash() {
        assert_eq!(trimmed_base_url("  http://x:3001/ "), "http://x:3001");
        assert_eq!(trimmed_base_url("http://x:3001"), "http://x:3001");
    }
}
