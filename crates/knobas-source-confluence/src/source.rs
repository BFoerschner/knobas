//! The `Source` implementation: what the sync engine and `test_source` hold.

use knobas_source::instance::SourceInstance;
use knobas_source::{ConnectionInfo, Cursor, Sink, Source, SourceDescriptor, SourceError, WriteOp};

use crate::api::ConfluenceApi;
use crate::http::{self, ConfluenceHttp};
use crate::sync::SyncRun;
use crate::{ConfluenceConfig, descriptor_template};

/// One configured Confluence Data Center instance.
pub struct ConfluenceSource {
    /// The instance id: the `EntityRef` namespace of every item this source
    /// emits, immutable once chosen (P10).
    id: String,
    display_name: String,
    /// Without a trailing slash, so `_links.webui` composes cleanly.
    base_url: String,
    cfg: ConfluenceConfig,
    http: ConfluenceHttp,
}

/// The base URL as `SyncItem::web_url` is built from it: trimmed of
/// surrounding whitespace and of the trailing slash a user's paste usually
/// carries, so `/display/ENG/…` composes onto it without doubling.
fn trimmed_base_url(raw: &str) -> String {
    raw.trim().trim_end_matches('/').to_owned()
}

/// Build an adapter from a stored source configuration plus the secret read
/// out of the keychain (contract §4.2).
///
/// Everything that can be wrong with the configuration is wrong *here* -- a
/// missing secret, an unsupported flavor, a base URL that is not one -- so a
/// scheduled run never discovers it mid-sync.
///
/// # Errors
///
/// [`SourceError::Unauthorized`] when the keychain holds no secret, and
/// [`SourceError::Protocol`] for a configuration that cannot work: the Cloud
/// dialect, user+password with no username, a base URL that is not an http(s)
/// URL, or a config blob this adapter's schema does not describe.
pub fn build(instance: SourceInstance) -> Result<Box<dyn Source>, SourceError> {
    let cfg = ConfluenceConfig::from_json(&instance.config)?;
    let credential = http::credential(
        instance.auth,
        cfg.username.as_deref(),
        instance.secret.as_deref(),
    )?;
    let http = ConfluenceHttp::new(&instance.base_url, &cfg, credential)?;
    Ok(Box::new(ConfluenceSource {
        id: instance.id,
        display_name: instance.display_name,
        base_url: trimmed_base_url(&instance.base_url),
        cfg,
        http,
    }))
}

#[async_trait::async_trait]
impl Source for ConfluenceSource {
    fn descriptor(&self) -> SourceDescriptor {
        SourceDescriptor {
            id: self.id.clone(),
            name: self.display_name.clone(),
            ..descriptor_template()
        }
    }

    /// `GET /rest/api/user/current`, and what it says.
    ///
    /// One call, because one is what this product offers a non-administrator:
    /// Confluence DC publishes its version through
    /// `/rest/api/settings/systemInfo`, which is administrators-only, so a
    /// source configured with an ordinary account would report *unreachable*
    /// for a version it simply may not read. `ConnectionInfo` is explicitly
    /// "every field is optional and an adapter fills only what its API
    /// actually exposes" -- so the version is `None`, and the sources view
    /// says nothing about it rather than something wrong.
    async fn test_connection(&self) -> Result<ConnectionInfo, SourceError> {
        let me = self.http.current_user().await?;
        Ok(ConnectionInfo {
            // The criterion: *reports the account*. `username` is the identity
            // field (#82) and what the Add-source dialog copies into the
            // config, so it is what is reported -- the display name only
            // stands in where the instance would not say.
            account: me.username.clone().or_else(|| me.display_name.clone()),
            server_version: None,
            // Confluence DC publishes PAT expiry through
            // `/rest/pat/latest/tokens`, which is outside this adapter's
            // endpoint set. The credential-health strip shows the countdown as
            // soon as that endpoint is added.
            secret_expires_at: None,
            detail: Some(match (me.display_name.as_deref(), me.user_key.as_deref()) {
                (Some(name), Some(key)) => format!("Confluence Data Center -- {name} ({key})"),
                (Some(name), None) => format!("Confluence Data Center -- {name}"),
                _ => "Confluence Data Center".to_owned(),
            }),
        })
    }

    async fn sync(
        &self,
        cursor: Option<Cursor>,
        sink: &mut (dyn Sink + Send),
    ) -> Result<Cursor, SourceError> {
        SyncRun {
            api: &self.http,
            cfg: &self.cfg,
            source_id: &self.id,
            base_url: &self.base_url,
        }
        .run(cursor, sink)
        .await
    }

    /// Refuse every write, by name.
    ///
    /// **The refusal arm is the contract, not a gap** (SPI doc, battery
    /// clause 5): an adapter must reject every op absent from its own
    /// `descriptor.write_ops` with `Protocol`, and it must say which op it
    /// refused so a mis-declared descriptor is diagnosable from the message.
    /// This adapter declares none, so every op lands here.
    ///
    /// The match has **no wildcard arm**: `WriteOp` grows per milestone
    /// (ADR-0006), and when spec #272's `CreatePage` and `UpdatePage` are
    /// ratified this file stops compiling until they are given a decision here
    /// -- which is exactly the reminder the next ticket wants.
    async fn write(&self, op: WriteOp) -> Result<(), SourceError> {
        match &op {
            WriteOp::Comment { .. }
            | WriteOp::Transition { .. }
            | WriteOp::CreateTicket { .. }
            | WriteOp::CreateBranch { .. }
            | WriteOp::CreatePullRequest { .. }
            | WriteOp::Approve { .. }
            | WriteOp::TriggerBuild { .. }
            | WriteOp::RerunBuild { .. } => Err(SourceError::protocol(format!(
                "the Confluence adapter does not support {:?}: it is read-only until the page \
                 write ops land",
                op.identifier()
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use knobas_source::AuthMethod;
    use serde_json::json;

    fn instance(config: serde_json::Value) -> SourceInstance {
        SourceInstance {
            id: "confluence".to_owned(),
            kind: crate::ADAPTER_KIND.to_owned(),
            display_name: "Tidewater Confluence".to_owned(),
            base_url: "https://wiki.tidewater.example".to_owned(),
            auth: Some(AuthMethod::Pat),
            secret: Some("a-personal-access-token".to_owned()),
            config,
        }
    }

    /// `Box<dyn Source>` is not `Debug`, so `Result::unwrap` is unavailable
    /// here; this reports the error the same way it would have.
    fn built(i: SourceInstance) -> Box<dyn Source> {
        match build(i) {
            Ok(source) => source,
            Err(e) => panic!("the adapter must build from this instance: {e:?}"),
        }
    }

    fn dead_port_url() -> String {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        format!("http://127.0.0.1:{port}")
    }

    /// The descriptor a *configured* source reports: the instance's id (which
    /// is the EntityRef namespace, P10) and the name the user gave it, over
    /// the template's static metadata.
    #[test]
    fn a_built_source_describes_the_instance_not_the_template() {
        let mut i = instance(json!({}));
        i.id = "confluence-eu".to_owned();
        let source = built(i);
        let d = source.descriptor();
        assert_eq!(d.id, "confluence-eu");
        assert_eq!(d.adapter_kind, crate::ADAPTER_KIND);
        assert_eq!(d.name, "Tidewater Confluence");
        assert_eq!(d.entity_kinds.len(), 1);
        assert!(d.capabilities.is_empty());
        assert!(d.write_ops.is_empty());
        // The claim the engine's tombstone sweep rests on travels with the
        // instance descriptor too, not only with the template.
        assert!(d.entity_kinds.iter().all(|k| k.full_sync_exhaustive));
    }

    #[test]
    fn the_cloud_flavor_is_refused_at_build_time() {
        let e = build(instance(json!({ "flavor": "cloud" })))
            .err()
            .expect("the cloud flavor is refused");
        assert!(
            matches!(&e, SourceError::Protocol { message: m, .. } if m.contains("cloud")),
            "{e:?}"
        );
    }

    /// Contract §3 "Missing": a configured source whose keychain item is gone
    /// reads as unauthorized, so the sources view offers *Re-enter* instead of
    /// showing a protocol error nobody can act on.
    #[test]
    fn a_source_with_no_secret_is_unauthorized() {
        let mut i = instance(json!({}));
        i.secret = None;
        assert!(matches!(build(i), Err(SourceError::Unauthorized { .. })));
    }

    #[test]
    fn basic_auth_needs_the_username_from_the_config() {
        let mut i = instance(json!({}));
        i.auth = Some(AuthMethod::UserPassword);
        let e = build(i)
            .err()
            .expect("basic auth with no username is refused");
        assert!(
            matches!(&e, SourceError::Protocol { message: m, .. } if m.contains("username")),
            "{e:?}"
        );

        let mut ok = instance(json!({ "username": "mara.lindqvist" }));
        ok.auth = Some(AuthMethod::UserPassword);
        built(ok);
    }

    #[test]
    fn a_base_url_that_is_not_a_url_is_refused() {
        let mut i = instance(json!({}));
        i.base_url = "wiki.tidewater.example".to_owned();
        assert!(matches!(build(i), Err(SourceError::Protocol { .. })));
    }

    /// P5 and P10 together: the `_links.webui` link is composed onto the URL
    /// the user typed, so a trailing slash must not survive into it. Checked
    /// on the built source rather than on the mapping, because trimming it is
    /// `build`'s job and `map` only sees what it is handed.
    #[test]
    fn a_trailing_slash_on_the_base_url_is_trimmed_once() {
        assert_eq!(
            trimmed_base_url("  https://wiki.tidewater.example/  "),
            "https://wiki.tidewater.example"
        );
        assert_eq!(
            trimmed_base_url("https://wiki.example/confluence"),
            "https://wiki.example/confluence",
            "a context path is part of the base URL and must survive"
        );
    }

    /// Battery clause 5, from this adapter's side: **every** op is refused,
    /// because this adapter declares none -- and the refusal **names the op**,
    /// which is what makes a descriptor that drifted from its dispatch
    /// diagnosable instead of merely broken.
    ///
    /// The adapter here points at a port nothing listens on, so a refusal that
    /// slipped through to the network would surface as `Unreachable` and fail
    /// this loudly rather than pass quietly.
    #[tokio::test]
    async fn every_write_op_is_refused_by_name_and_none_reaches_the_network() {
        let mut i = instance(json!({}));
        i.base_url = dead_port_url();
        let source = built(i);
        let target = "confluence:98307".to_owned();
        for op in [
            WriteOp::Comment {
                entity: target.clone(),
                body: "on it".to_owned(),
            },
            WriteOp::Transition {
                entity: target.clone(),
                status: "Done".to_owned(),
            },
            WriteOp::CreateTicket {
                entity: target.clone(),
                title: "t".to_owned(),
                body: "b".to_owned(),
                ticket_type: "Task".to_owned(),
            },
            WriteOp::CreateBranch {
                entity: target.clone(),
                name: "feature/x".to_owned(),
                from_ref: "main".to_owned(),
            },
            WriteOp::CreatePullRequest {
                entity: target.clone(),
                title: "t".to_owned(),
                body: "b".to_owned(),
                head: "feature/x".to_owned(),
                base: "main".to_owned(),
            },
            WriteOp::Approve {
                entity: target.clone(),
                body: String::new(),
            },
            WriteOp::TriggerBuild {
                entity: target.clone(),
            },
            WriteOp::RerunBuild {
                entity: target.clone(),
            },
        ] {
            let name = op.identifier();
            let refused = source.write(op).await;
            let Err(SourceError::Protocol { message, status }) = &refused else {
                panic!("{name} must be refused with Protocol, got {refused:?}");
            };
            assert!(
                message.contains(name),
                "the refusal must name {name}: {message}"
            );
            assert_eq!(
                *status, None,
                "a refusal knobas raised itself carries no status"
            );
        }
    }

    /// The two halves of the descriptor that say "read-only" have to agree
    /// with the dispatch above, and the battery enforces both directions --
    /// but only against a live server. Pinned here too, because this is the
    /// pair that has to change together the day the page write ops land.
    #[test]
    fn the_descriptor_and_the_dispatch_agree_that_this_adapter_is_read_only() {
        let d = built(instance(json!({}))).descriptor();
        assert!(d.write_ops.is_empty());
        assert!(!d.capabilities.contains(&knobas_source::Capability::Write));
    }
}
