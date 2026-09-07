//! The `Source` implementation: what the sync engine and `test_source` hold.

use knobas_source::instance::SourceInstance;
use knobas_source::{
    ConnectionInfo, Cursor, Sink, Source, SourceDescriptor, SourceError, SyncItem, WriteOp,
    WriteReceipt,
};

use crate::cursor::Position;
use crate::http::KumaHttp;
use crate::socket::KumaSocket;
use crate::{KumaConfig, descriptor, http, map, metrics, model, socket};

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
    /// The write half, or `None` for a source configured with only an API key
    /// (issue #452).
    ///
    /// The *presence of the channel* is what the descriptor advertises and
    /// what [`Source::write`] dispatches on, rather than a boolean beside it:
    /// two facts that must agree cannot disagree if there is only one.
    socket: Option<KumaSocket>,
}

/// The connection note (#326): how much this Kuma is watching, in one line.
///
/// A function rather than an inline `match`, so the singular arm is reachable
/// from a test: *Test connection* against any real instance answers a plural,
/// and an assertion on that answer alone would go on passing if the singular
/// were spelled "1 monitors".
fn roster_note(count: usize) -> String {
    match count {
        1 => "1 monitor".to_owned(),
        other => format!("{other} monitors"),
    }
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
    // The account is optional and its absence is not a fault: a Kuma with only
    // an API key is the ordinary configuration (spec #427, story 50) and reads
    // every monitor. What it does not get is a socket.io channel, and
    // therefore -- through `descriptor` -- any write op to offer.
    let socket = instance
        .account
        .map(|account| KumaSocket::new(&instance.base_url, &cfg, account))
        .transpose()?;
    Ok(Box::new(KumaSource {
        id: instance.id,
        display_name: instance.display_name,
        base_url: trimmed_base_url(&instance.base_url),
        http,
        socket,
    }))
}

/// Kuma's own monitor id, out of the entity id a write op names.
///
/// `kuma:8` → `8`, as a **number**: `pauseMonitor` is handed the id Kuma keeps
/// in its database and answers a string with *You do not own this monitor*,
/// which reads as a permissions problem rather than as a type one.
///
/// # Errors
///
/// [`SourceError::Protocol`] for a target that is not an entity id, or whose
/// key is not a monitor id -- both are writes nothing could ever deliver, and
/// saying so by name beats sending Kuma something it will refuse.
fn monitor_id(entity: &str) -> Result<i64, SourceError> {
    let parsed = knobas_core::entity::EntityRef::parse(entity).map_err(|error| {
        SourceError::protocol(format!("{entity:?} is not an entity id: {error}"))
    })?;
    parsed.key.parse::<i64>().map_err(|_| {
        SourceError::protocol(format!(
            "{entity:?} does not name an Uptime Kuma monitor: its key {:?} is not a monitor id",
            parsed.key
        ))
    })
}

#[async_trait::async_trait]
impl Source for KumaSource {
    /// The template under this instance's own id and name -- **plus the write
    /// half when this instance has an account** (issue #452).
    ///
    /// See `descriptor::for_instance` for why this adapter is the one whose
    /// descriptor depends on a credential.
    fn descriptor(&self) -> SourceDescriptor {
        descriptor::for_instance(&self.id, &self.display_name, self.socket.is_some())
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
            detail: Some(roster_note(count)),
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

    /// Pause or resume one monitor, over the socket.io channel the account
    /// opens (issue #452); everything else refused by name.
    ///
    /// **The no-account arm comes first**, and it is a different sentence from
    /// the unsupported-op one: a source with only an API key declares an empty
    /// `write_ops`, so nothing should ever offer a pause on it -- and if
    /// something did, "this source has no account" is the fact a reader can
    /// act on. `Protocol` and not `Unauthorized`: the credential this source
    /// *has* is fine and re-entering it changes nothing (ADR-0004), so the
    /// write is refused rather than parked waiting for a human.
    ///
    /// The `match` is exhaustive with no wildcard, the device
    /// `WriteOp::identifier` uses: a new SPI variant must stop this adapter
    /// compiling until somebody decides whether Kuma performs it.
    ///
    /// **`WriteReceipt::none()`**, because Kuma's answer carries nothing to
    /// keep: `{"ok":true,"msg":"successPaused"}` names no record and mints no
    /// id -- what changed is the monitor's own state, and the next poll reads
    /// it back.
    async fn write(&self, op: WriteOp) -> Result<WriteReceipt, SourceError> {
        let Some(socket) = self.socket.as_ref() else {
            return Err(SourceError::protocol(format!(
                "this Uptime Kuma source has no account configured, so it declares no write ops \
                 and cannot perform {:?} -- add the account to its credential",
                op.identifier()
            )));
        };
        let (event, entity) = match &op {
            WriteOp::PauseMonitor { entity } => (socket::PAUSE_EVENT, entity),
            WriteOp::ResumeMonitor { entity } => (socket::RESUME_EVENT, entity),
            WriteOp::Comment { .. }
            | WriteOp::Transition { .. }
            | WriteOp::CreateTicket { .. }
            | WriteOp::CreateBranch { .. }
            | WriteOp::CreatePullRequest { .. }
            | WriteOp::Approve { .. }
            | WriteOp::TriggerBuild { .. }
            | WriteOp::RerunBuild { .. }
            | WriteOp::LogWork { .. }
            | WriteOp::CreatePage { .. }
            | WriteOp::UpdatePage { .. } => {
                return Err(SourceError::protocol(format!(
                    "the Uptime Kuma adapter offers pause and resume, so it cannot perform {:?}",
                    op.identifier()
                )));
            }
        };
        socket
            .call(event, &serde_json::json!(monitor_id(entity)?))
            .await?;
        Ok(WriteReceipt::none())
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
            account: None,
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

    /// The one arm no live instance reaches: a Kuma watching exactly one thing.
    #[test]
    fn the_roster_note_counts_in_english() {
        assert_eq!(roster_note(0), "0 monitors");
        assert_eq!(roster_note(1), "1 monitor");
        assert_eq!(roster_note(8), "8 monitors");
    }

    /// A write's target is a monitor id, and everything else is refused
    /// before Kuma is asked.
    ///
    /// The direction that matters is the second one: `pauseMonitor` answers a
    /// non-numeric id with *You do not own this monitor*, so an entity id that
    /// slipped through would surface as a permissions problem on a monitor
    /// nobody can find.
    #[test]
    fn a_writes_target_is_a_monitor_id_or_it_is_refused_here() {
        assert_eq!(monitor_id("kuma:8").unwrap(), 8);
        assert_eq!(monitor_id("kuma-eu:31").unwrap(), 31);
        for refused in [
            // Not an entity id at all.
            "8",
            "",
            // An entity id whose key is not a monitor id -- a Jira ticket
            // amended onto a Kuma write, or a tombstone key knobas invented.
            "kuma:PAY-231",
            "kuma:8a",
            "kuma:",
        ] {
            assert!(
                matches!(monitor_id(refused), Err(SourceError::Protocol { .. })),
                "{refused:?} must not read as a monitor id"
            );
        }
    }

    /// The write half, on and off, at the seam the sync engine holds.
    ///
    /// `build` is what the registry calls, so this is the whole of issue
    /// #452's "adding the account flips both": the same configuration, the
    /// same key, one field more.
    #[tokio::test]
    async fn an_account_is_what_gives_an_instance_its_write_ops() {
        let key_only = build(instance(serde_json::json!({}), Some("uk1_secret"))).unwrap();
        assert!(key_only.descriptor().write_ops.is_empty());

        let with_account = build(SourceInstance {
            account: Some(knobas_source::instance::Account {
                username: "knobas".to_owned(),
                password: "knobas-dev".to_owned(),
            }),
            ..instance(serde_json::json!({}), Some("uk1_secret"))
        })
        .unwrap();
        assert_eq!(
            with_account.descriptor().write_ops,
            ["pause_monitor", "resume_monitor"]
        );

        // And the refusal a key-only source gives names the remedy. No
        // network: the arm is taken before a session is opened.
        let refused = key_only
            .write(WriteOp::PauseMonitor {
                entity: "kuma:8".to_owned(),
            })
            .await;
        let message = match refused {
            Err(SourceError::Protocol { message, .. }) => message,
            other => panic!("expected Protocol, got {other:?}"),
        };
        assert!(message.contains("no account"), "{message}");
    }

    /// An op this adapter does not perform is refused with a **different**
    /// sentence from the one a missing account gets -- the two are different
    /// faults with different remedies, and a reader who cannot tell them apart
    /// will go looking for an account they already have.
    #[tokio::test]
    async fn an_op_kuma_does_not_perform_says_so_rather_than_blaming_the_account() {
        let with_account = build(SourceInstance {
            account: Some(knobas_source::instance::Account {
                username: "knobas".to_owned(),
                password: "knobas-dev".to_owned(),
            }),
            ..instance(serde_json::json!({}), Some("uk1_secret"))
        })
        .unwrap();
        let refused = with_account
            .write(WriteOp::Comment {
                entity: "kuma:8".to_owned(),
                body: "nothing to say to a monitor".to_owned(),
            })
            .await;
        let message = match refused {
            Err(SourceError::Protocol { message, .. }) => message,
            other => panic!("expected Protocol, got {other:?}"),
        };
        assert!(message.contains("comment"), "{message}");
        assert!(!message.contains("no account"), "{message}");
    }

    /// The base URL a monitor's link is composed onto: the trailing slash a
    /// paste carries would otherwise double.
    #[test]
    fn the_base_url_loses_its_trailing_slash() {
        assert_eq!(trimmed_base_url("  http://x:3001/ "), "http://x:3001");
        assert_eq!(trimmed_base_url("http://x:3001"), "http://x:3001");
    }
}
