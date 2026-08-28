//! The `Source` implementation: what the sync engine and `test_source` hold.

use knobas_source::instance::SourceInstance;
use knobas_source::{ConnectionInfo, Cursor, Sink, Source, SourceDescriptor, SourceError, WriteOp};

use crate::api::JiraApi;
use crate::http::{self, JiraHttp};
use crate::sync::SyncRun;
use crate::{JiraConfig, descriptor_template};

/// One configured Jira Data Center instance.
pub struct JiraSource {
    /// The instance id: the `EntityRef` namespace of every item this source
    /// emits, immutable once chosen (P10).
    id: String,
    display_name: String,
    /// Without a trailing slash, so `/browse/<key>` composes cleanly.
    base_url: String,
    cfg: JiraConfig,
    http: JiraHttp,
}

/// The base URL as `SyncItem::web_url` is built from it: trimmed of
/// surrounding whitespace and of the trailing slash a user's paste usually
/// carries, so `/browse/PAY-231` composes onto it without doubling.
fn trimmed_base_url(raw: &str) -> String {
    raw.trim().trim_end_matches('/').to_owned()
}

/// Build an adapter from a stored source configuration plus the secret stream F
/// read out of the keychain (interfaces §4.2).
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
    let cfg = JiraConfig::from_json(&instance.config)?;
    let credential = http::credential(
        instance.auth,
        cfg.username.as_deref(),
        instance.secret.as_deref(),
    )?;
    let http = JiraHttp::new(&instance.base_url, &cfg, credential)?;
    Ok(Box::new(JiraSource {
        id: instance.id,
        display_name: instance.display_name,
        base_url: trimmed_base_url(&instance.base_url),
        cfg,
        http,
    }))
}

#[async_trait::async_trait]
impl Source for JiraSource {
    fn descriptor(&self) -> SourceDescriptor {
        SourceDescriptor {
            id: self.id.clone(),
            name: self.display_name.clone(),
            ..descriptor_template()
        }
    }

    async fn test_connection(&self) -> Result<ConnectionInfo, SourceError> {
        // `myself` first: it is the call that fails with 401 on a bad
        // credential, and it is what the Add-source flow shows as "connected
        // as".
        let me = self.http.myself().await?;
        let server = self.http.server_info().await?;
        let version = server.version.clone();
        Ok(ConnectionInfo {
            account: me.name.or(me.display_name),
            server_version: version.clone(),
            // Jira DC does publish PAT expiry, but only through
            // /rest/pat/latest/tokens, which is outside M1's endpoint set (it
            // is not in mockd's Jira subset). The credential-health strip shows
            // the countdown as soon as that endpoint is added.
            secret_expires_at: None,
            detail: Some(format!(
                "{} {}",
                server.deployment_type.unwrap_or_else(|| "Jira".to_owned()),
                version.unwrap_or_else(|| "(unknown version)".to_owned())
            )),
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

    async fn write(&self, op: WriteOp) -> Result<(), SourceError> {
        // M1 is read-only toward every source (interfaces §4.1); the descriptor
        // declares no write ops, so refusing here is the contract, not a gap.
        // Jira write-back (transition, comment, create) is M2.
        Err(SourceError::protocol(format!(
            "the Jira adapter is read-only in this version and does not support {:?}",
            op.identifier()
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use knobas_source::AuthMethod;
    use knobas_source::instance::SourceInstance;
    use serde_json::json;

    fn instance(config: serde_json::Value) -> SourceInstance {
        SourceInstance {
            id: "jira".to_owned(),
            kind: crate::ADAPTER_KIND.to_owned(),
            display_name: "Tidewater Jira".to_owned(),
            base_url: "https://jira.tidewater.example".to_owned(),
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
    /// is the EntityRef namespace, P10) and the name the user gave it, over the
    /// template's static metadata.
    #[test]
    fn a_built_source_describes_the_instance_not_the_template() {
        let mut i = instance(json!({}));
        i.id = "jira-eu".to_owned();
        let source = built(i);
        let d = source.descriptor();
        assert_eq!(d.id, "jira-eu");
        assert_eq!(d.adapter_kind, crate::ADAPTER_KIND);
        assert_eq!(d.name, "Tidewater Jira");
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

    /// Interfaces §3 "Missing": a configured source whose keychain item is gone
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
        i.base_url = "jira.tidewater.example".to_owned();
        assert!(matches!(build(i), Err(SourceError::Protocol { .. })));
    }

    /// P5 and P10 together: the `/browse/<key>` link is built from the URL the
    /// user typed, so a trailing slash must not survive into it. Checked on the
    /// built source rather than on the mapping, because trimming it is `build`'s
    /// job and `map` only sees what it is handed.
    #[test]
    fn a_trailing_slash_on_the_base_url_is_trimmed_once() {
        let mut i = instance(json!({}));
        i.base_url = "https://jira.tidewater.example/".to_owned();
        let cfg = JiraConfig::from_json(&i.config).unwrap();
        let credential =
            http::credential(i.auth, cfg.username.as_deref(), i.secret.as_deref()).unwrap();
        let source = JiraSource {
            id: i.id.clone(),
            display_name: i.display_name.clone(),
            base_url: trimmed_base_url(&i.base_url),
            cfg,
            http: JiraHttp::new(&i.base_url, &JiraConfig::default(), credential).unwrap(),
        };
        assert_eq!(source.base_url, "https://jira.tidewater.example");
    }

    /// M1 is read-only toward every source: nothing is declared, so everything
    /// is refused -- which is also what battery clause 5 checks.
    #[tokio::test]
    async fn every_write_is_refused() {
        let source = built(instance(json!({})));
        let refused = source
            .write(WriteOp::Comment {
                entity: "jira:PAY-231".to_owned(),
                body: "not in M1".to_owned(),
            })
            .await;
        assert!(
            matches!(refused, Err(SourceError::Protocol { .. })),
            "{refused:?}"
        );
    }

    /// Interfaces §4.1: the same classification from `test_connection` and from
    /// mid-`sync`. Both are exercised here against a port nothing listens on.
    #[tokio::test]
    async fn an_unreachable_instance_is_unreachable_from_both_entry_points() {
        let mut i = instance(json!({}));
        i.base_url = dead_port_url();
        let source = built(i);
        let connect = source.test_connection().await;
        assert!(
            matches!(connect, Err(SourceError::Unreachable(_))),
            "{connect:?}"
        );
        let synced = source
            .sync(None, &mut knobas_source::contract::VecSink(Vec::new()))
            .await;
        assert!(
            matches!(synced, Err(SourceError::Unreachable(_))),
            "{synced:?}"
        );
    }
}
