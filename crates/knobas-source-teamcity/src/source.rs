//! The [`Source`] implementation: what the sync engine and `test_source` hold.

use knobas_source::instance::SourceInstance;
use knobas_source::{ConnectionInfo, Cursor, Sink, Source, SourceDescriptor, SourceError, WriteOp};

use crate::client::{HttpRest, Rest, connection_info};
use crate::{TeamCityConfig, descriptor_template, sync};

/// One configured TeamCity instance.
#[derive(Debug)]
pub struct TeamCitySource {
    /// The instance id: the `EntityRef` namespace of every item this source
    /// emits, immutable once chosen (P10).
    id: String,
    display_name: String,
    cfg: TeamCityConfig,
    rest: HttpRest,
}

/// Config + secret ⇒ a live adapter. The scheduler and *Test connection* both
/// call this (interfaces §4.2).
///
/// Everything that can be wrong with the configuration is wrong *here* -- a
/// missing secret, an unknown config key, a base URL that is not one -- so a
/// scheduled run never discovers it mid-sync.
///
/// # Errors
///
/// [`SourceError::Unauthorized`] when the keychain holds no secret, and
/// [`SourceError::Protocol`] for a configuration that cannot work: an unknown
/// or out-of-range config key, user + password with no username, a base URL
/// that is not an http(s) URL, or an authentication method this adapter does
/// not declare.
pub fn build(instance: SourceInstance) -> Result<Box<dyn Source>, SourceError> {
    let cfg = TeamCityConfig::from_json(&instance.config)?;
    let rest = HttpRest::new(&instance, &cfg)?;
    Ok(Box::new(TeamCitySource {
        id: instance.id,
        display_name: instance.display_name,
        cfg,
        rest,
    }))
}

#[async_trait::async_trait]
impl Source for TeamCitySource {
    fn descriptor(&self) -> SourceDescriptor {
        SourceDescriptor {
            // The instance id, not the adapter kind: it is the namespace of
            // every item this instance emits (P10), and the contract battery
            // checks every item against it.
            id: self.id.clone(),
            name: if self.display_name.trim().is_empty() {
                "TeamCity".to_owned()
            } else {
                self.display_name.clone()
            },
            ..descriptor_template()
        }
    }

    async fn test_connection(&self) -> Result<ConnectionInfo, SourceError> {
        let server = self.rest.server().await?;
        // Second, and only after the version is in hand: naming the account is
        // the nicer half of the report, not the part worth failing over. A
        // TeamCity that serves `/app/rest/server` but not
        // `/app/rest/users/current` still connected.
        //
        // An authentication failure is *not* swallowed here, because it cannot
        // reach this line: `/app/rest/server` needs the same credential and
        // 401s first.
        let account = self.rest.current_user().await.ok();
        Ok(connection_info(&server, account.as_ref()))
    }

    async fn sync(
        &self,
        cursor: Option<Cursor>,
        sink: &mut (dyn Sink + Send),
    ) -> Result<Cursor, SourceError> {
        sync::execute(&self.id, &self.cfg, &self.rest, cursor, sink).await
    }

    async fn write(&self, op: WriteOp) -> Result<(), SourceError> {
        // M1 is read-only toward every source (interfaces §4.1); the
        // descriptor declares no write ops, so every op reaching here is
        // undeclared. Triggering a build is M2.
        Err(SourceError::Protocol(format!(
            "the TeamCity adapter is read-only in this version and does not support {:?}",
            op.identifier()
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use knobas_source::AuthMethod;
    use serde_json::json;

    fn instance(config: serde_json::Value) -> SourceInstance {
        SourceInstance {
            id: "teamcity-eu".to_owned(),
            kind: crate::ADAPTER_KIND.to_owned(),
            display_name: "Tidewater CI (EU)".to_owned(),
            base_url: "https://ci.example.com".to_owned(),
            auth: Some(AuthMethod::Pat),
            secret: Some("an-access-token".to_owned()),
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

    /// The descriptor an *instance* reports carries the instance's id, because
    /// that id is the namespace of everything it emits (P10).
    #[test]
    fn an_instance_describes_itself_not_the_template() {
        let d = built(instance(json!({ "project_ids": ["Payout"] }))).descriptor();
        assert_eq!(d.id, "teamcity-eu");
        assert_eq!(d.adapter_kind, "teamcity");
        assert_eq!(d.name, "Tidewater CI (EU)");
        assert!(d.write_ops.is_empty());
        assert!(d.capabilities.is_empty());
        // The claim the engine's tombstone sweep rests on travels with the
        // instance descriptor too, not only with the template.
        assert!(
            !d.full_sync_exhaustive,
            "a TeamCity instance's full sync is a window; the sweep must not run after it"
        );
    }

    /// A source someone left unnamed still has something to show in the list.
    #[test]
    fn a_blank_display_name_falls_back_to_the_adapter_name() {
        let mut i = instance(json!({}));
        i.display_name = "   ".to_owned();
        assert_eq!(built(i).descriptor().name, "TeamCity");
    }

    #[test]
    fn a_bad_config_fails_construction_rather_than_the_first_sync() {
        let mut bad = instance(json!({}));
        bad.config = json!({ "projects": ["Payout"] });
        assert!(matches!(build(bad), Err(SourceError::Protocol(_))));
    }

    /// Interfaces §3 "Missing": a configured source whose keychain item is
    /// gone reads as unauthorized, so the sources view offers *Re-enter*
    /// instead of a protocol error nobody can act on.
    #[test]
    fn a_source_with_no_secret_is_unauthorized() {
        let mut i = instance(json!({}));
        i.secret = None;
        assert!(matches!(build(i), Err(SourceError::Unauthorized)));
    }

    #[test]
    fn basic_auth_needs_the_username_from_the_config() {
        let mut i = instance(json!({}));
        i.auth = Some(AuthMethod::UserPassword);
        let e = build(i)
            .err()
            .expect("basic auth with no username is refused");
        assert!(
            matches!(&e, SourceError::Protocol(m) if m.contains("username")),
            "{e:?}"
        );

        let mut ok = instance(json!({ "username": "mara.lindqvist" }));
        ok.auth = Some(AuthMethod::UserPassword);
        built(ok);
    }

    #[test]
    fn a_base_url_that_is_not_a_url_is_refused() {
        let mut i = instance(json!({}));
        i.base_url = "ci.example.com".to_owned();
        assert!(matches!(build(i), Err(SourceError::Protocol(_))));
    }

    /// M1 is read-only toward every source: nothing is declared, so everything
    /// is refused -- which is also what battery clause 5 checks.
    #[tokio::test]
    async fn every_write_is_refused() {
        let refused = built(instance(json!({})))
            .write(WriteOp::Comment {
                entity: "teamcity-eu:build:1187".to_owned(),
                body: "not in M1".to_owned(),
            })
            .await;
        assert!(
            matches!(refused, Err(SourceError::Protocol(_))),
            "{refused:?}"
        );
    }

    /// Interfaces §4.1: the same classification from `test_connection` and
    /// from mid-`sync`. Both are exercised here against a port nothing listens
    /// on -- the mockd suite covers the 401 half against a live server.
    #[tokio::test]
    async fn an_unreachable_instance_is_unreachable_from_both_entry_points() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        drop(listener);
        let mut i = instance(json!({}));
        i.base_url = format!("http://127.0.0.1:{port}");
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
