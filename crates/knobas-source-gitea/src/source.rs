//! The `Source` implementation: what the sync engine and *Test connection*
//! hold.

use knobas_source::instance::{SourceInstance, validate_instance_id};
use knobas_source::{ConnectionInfo, Cursor, Sink, Source, SourceDescriptor, SourceError, WriteOp};

use crate::client::{self, GiteaClient};
use crate::config::GiteaConfig;
use crate::descriptor_template;

/// One configured Gitea instance.
pub struct GiteaSource {
    /// The instance id: the `EntityRef` namespace of every item this source
    /// emits, immutable once chosen (P10).
    pub(crate) id: String,
    display_name: String,
    pub(crate) config: GiteaConfig,
    pub(crate) client: GiteaClient,
}

/// Build an adapter for one configured source (ruling P6).
///
/// Everything that can be wrong with the configuration is wrong *here* -- an
/// unusable instance id, a missing secret, an auth method this adapter does not
/// speak, a base URL that is not one, a config blob its schema does not
/// describe -- so a scheduled run never discovers it mid-sync.
///
/// # Errors
///
/// [`SourceError::Unauthorized`] when the keychain holds no secret (interfaces
/// §3 `missing_secret`), and [`SourceError::Protocol`] for every configuration
/// failure.
pub fn build(instance: SourceInstance) -> Result<Box<dyn Source>, SourceError> {
    // The id is baked into every entity id, link and activity row this source
    // ever writes (P10), so an unusable one is refused at add time rather than
    // on every run afterwards. The rule is `knobas_source`'s own, not a copy:
    // a second copy is how an adapter passes its own suite and is then refused
    // by the engine on every real sync.
    validate_instance_id(&instance.id).map_err(|error| {
        SourceError::Protocol(format!(
            "gitea: instance id {:?} is unusable: {error}",
            instance.id
        ))
    })?;
    let config = GiteaConfig::from_json(&instance.config)?;
    let credential = client::credential(instance.auth, instance.secret.as_deref())?;
    let client = GiteaClient::new(&instance.base_url, &config, credential)?;
    Ok(Box::new(GiteaSource {
        id: instance.id,
        display_name: instance.display_name,
        config,
        client,
    }))
}

#[async_trait::async_trait]
impl Source for GiteaSource {
    fn descriptor(&self) -> SourceDescriptor {
        SourceDescriptor {
            id: self.id.clone(),
            name: self.display_name.clone(),
            ..descriptor_template()
        }
    }

    async fn test_connection(&self) -> Result<ConnectionInfo, SourceError> {
        // `/user` first: it is the call that fails with 401 on a bad
        // credential, and it is what the Add-source flow shows as
        // "connected as".
        let user = self.client.current_user().await?;
        // Best effort: `/version` says nothing about the credential, and an
        // instance that hides it is still perfectly usable.
        let server_version = self.client.version().await.ok().flatten();
        Ok(ConnectionInfo {
            account: user.login.filter(|l| !l.trim().is_empty()),
            detail: server_version
                .as_deref()
                .map(|version| format!("Gitea {version}")),
            server_version,
            // Gitea's personal access tokens do not expire, and the endpoint
            // that lists them needs basic auth, which this adapter does not
            // use -- so there is no countdown to report (spec §3).
            secret_expires_at: None,
        })
    }

    async fn sync(
        &self,
        cursor: Option<Cursor>,
        sink: &mut (dyn Sink + Send),
    ) -> Result<Cursor, SourceError> {
        crate::sync::run(self, cursor, sink).await
    }

    async fn write(&self, op: WriteOp) -> Result<(), SourceError> {
        // Interfaces §4.1: M1 is read-only toward every source. The descriptor
        // declares no write ops, so refusing here is the contract, not a gap;
        // write-back is M2's identity release.
        Err(SourceError::Protocol(format!(
            "gitea: {:?} is not supported -- this adapter is read-only in M1",
            op.identifier()
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use knobas_source::AuthMethod;

    fn instance() -> SourceInstance {
        SourceInstance {
            id: "gitea".to_owned(),
            kind: "gitea".to_owned(),
            display_name: "Tidewater Git".to_owned(),
            base_url: "https://gitea.tidewater.example".to_owned(),
            auth: Some(AuthMethod::Pat),
            secret: Some("tidewater-pat".to_owned()),
            config: serde_json::json!({}),
        }
    }

    /// The descriptor an instance reports is the *instance's*, not the
    /// template's: the id is the `EntityRef` namespace of everything it emits,
    /// and the name is what the sources view lists.
    #[test]
    fn an_instance_describes_itself_and_not_the_template() {
        let source = build(SourceInstance {
            id: "gitea-eu".to_owned(),
            ..instance()
        })
        .unwrap();
        let d = source.descriptor();
        assert_eq!(d.id, "gitea-eu");
        assert_eq!(d.adapter_kind, "gitea");
        assert_eq!(d.name, "Tidewater Git");
        // Everything else is still the template's.
        assert_eq!(d.entity_kinds.len(), 4);
        assert!(d.write_ops.is_empty());
        // Including the 2026-08-25 budget ruling -- an instance cannot quietly
        // re-grant the sweep the template refuses it.
        assert_eq!(
            d.entity_kinds
                .iter()
                .map(|k| (k.id.as_str(), k.full_sync_exhaustive))
                .collect::<Vec<_>>(),
            vec![
                (crate::KIND_REPO, true),
                (crate::KIND_BRANCH, true),
                (crate::KIND_PR, false),
                (crate::KIND_COMMIT, false),
            ]
        );
    }

    /// An id that cannot be an `EntityRef` namespace fails when the source is
    /// added, not on every run afterwards -- and the rule is the SPI's, so an
    /// id the engine refuses is refused here too.
    #[test]
    fn an_unusable_instance_id_is_refused_at_build_time() {
        for bad in ["", "gitea:eu", "monitor", "Gitea", "gitea_eu"] {
            let error = build(SourceInstance {
                id: bad.to_owned(),
                ..instance()
            })
            .err()
            .unwrap_or_else(|| panic!("{bad:?} must be refused"));
            assert!(
                matches!(error, SourceError::Protocol(ref m) if m.contains(bad)),
                "{bad:?} -> {error:?}"
            );
        }
        // And the two shapes the contract names are accepted.
        for good in ["gitea", "gitea-eu"] {
            build(SourceInstance {
                id: good.to_owned(),
                ..instance()
            })
            .unwrap_or_else(|_| panic!("{good:?} is a valid instance id"));
        }
    }

    #[test]
    fn a_missing_secret_cannot_authenticate() {
        assert!(matches!(
            build(SourceInstance {
                secret: None,
                ..instance()
            }),
            Err(SourceError::Unauthorized)
        ));
    }

    #[test]
    fn an_unsupported_auth_method_is_refused_at_build_time() {
        for auth in [
            None,
            Some(AuthMethod::UserPassword),
            Some(AuthMethod::OAuth),
        ] {
            let error = build(SourceInstance { auth, ..instance() }).err().unwrap();
            assert!(
                matches!(error, SourceError::Protocol(ref m) if m.contains("token")),
                "{auth:?} -> {error:?}"
            );
        }
    }

    /// A config blob the schema does not describe is refused at add time, with
    /// the offending key named.
    #[test]
    fn a_config_the_schema_does_not_describe_is_refused_at_build_time() {
        let error = build(SourceInstance {
            config: serde_json::json!({ "owner": "tidewater" }),
            ..instance()
        })
        .err()
        .unwrap();
        assert!(
            matches!(error, SourceError::Protocol(ref m) if m.contains("owner")),
            "{error:?}"
        );
    }

    /// M1 is read-only toward every source, and the refusal must name the op
    /// so the failure reads as a decision rather than a bug.
    #[tokio::test]
    async fn every_write_is_refused_without_reaching_the_network() {
        let source = build(instance()).unwrap();
        let refused = source
            .write(WriteOp::Comment {
                entity: "gitea:tidewater/payout-service#142".to_owned(),
                body: "no".to_owned(),
            })
            .await
            .unwrap_err();
        assert!(
            matches!(refused, SourceError::Protocol(ref m) if m.contains("comment")),
            "{refused:?}"
        );
    }
}
