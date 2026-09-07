//! The `Source` implementation: what the sync engine and *Test connection*
//! hold.

use knobas_source::instance::{SourceInstance, validate_instance_id};
use knobas_source::{
    ConnectionInfo, Cursor, Sink, Source, SourceDescriptor, SourceError, WriteOp, WriteReceipt,
};

use crate::client::{self, GiteaClient};
use crate::config::GiteaConfig;
use crate::descriptor_template;
use crate::write;

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
        SourceError::protocol(format!(
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

impl GiteaSource {
    /// The key half of an entity id this source owns, parsed into Gitea's own
    /// key grammar.
    ///
    /// The namespace check is not defensive noise: two Giteas are `gitea` and
    /// `gitea-eu` (§4.1), and `tidewater/payout-service#142` is a plausible key
    /// on both -- so a write that trusted the routing would approve a pull
    /// request on the wrong server. The queue routes by namespace; reaching
    /// here with a foreign one is knobas' own bug and is refused rather than
    /// performed.
    ///
    /// # Errors
    ///
    /// [`SourceError::Protocol`] -- a refusal, which the queue does not retry,
    /// because nothing about waiting makes a wrong id right.
    fn key_of(&self, entity: &str) -> Result<crate::keys::GiteaKey, SourceError> {
        let parsed = knobas_core::entity::EntityRef::parse(entity)
            .map_err(|error| SourceError::protocol(format!("gitea: {error}")))?;
        if parsed.namespace != self.id {
            return Err(SourceError::protocol(format!(
                "gitea: {entity:?} belongs to source {:?}, not to {:?}",
                parsed.namespace, self.id
            )));
        }
        crate::keys::parse_key(&parsed.key).ok_or_else(|| {
            SourceError::protocol(format!(
                "gitea: {entity:?} is not an id this adapter issued"
            ))
        })
    }

    /// The repository a write acts in.
    ///
    /// # Errors
    ///
    /// As [`Self::key_of`], plus an id that names a branch, a commit or a pull
    /// request rather than a repository.
    fn repo_of(&self, entity: &str) -> Result<(String, String), SourceError> {
        match self.key_of(entity)? {
            crate::keys::GiteaKey::Repo { owner, repo } => Ok((owner, repo)),
            _ => Err(SourceError::protocol(format!(
                "gitea: {entity:?} is not a repository -- this operation is performed in one, so \
                 the target is an id like \"{}:owner/repo\"",
                self.id
            ))),
        }
    }

    /// The pull request a write acts on.
    ///
    /// # Errors
    ///
    /// As [`Self::key_of`], plus an id that names anything but a pull request.
    fn pull_of(&self, entity: &str) -> Result<(String, String, u64), SourceError> {
        match self.key_of(entity)? {
            crate::keys::GiteaKey::Pr {
                owner,
                repo,
                number,
            } => Ok((owner, repo, number)),
            _ => Err(SourceError::protocol(format!(
                "gitea: {entity:?} is not a pull request -- this operation is performed on one, \
                 so the target is an id like \"{}:owner/repo#142\"",
                self.id
            ))),
        }
    }
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
            // No connection note: the version is `server_version`'s to say,
            // and a note is for what nothing else on the report says (#326).
            detail: None,
            server_version,
            // Gitea's personal access tokens do not expire, and the endpoint
            // that lists them needs basic auth, which this adapter does not
            // use -- so there is no countdown to report (spec §3).
            secret_expires_at: None,
            // Nothing per-instance for the dialog to fill: every Gitea config key
            // is one the reader chooses, not one the server owns.
            discovered: std::collections::BTreeMap::new(),
        })
    }

    async fn sync(
        &self,
        cursor: Option<Cursor>,
        sink: &mut (dyn Sink + Send),
    ) -> Result<Cursor, SourceError> {
        crate::sync::run(self, cursor, sink).await
    }

    /// Perform one of the four writes M2 ratified for Gitea, or refuse.
    ///
    /// The match unpacks and hands values to [`crate::write`], which never
    /// names `WriteOp`: knobas has one outbound write path and
    /// `write_choke_point.rs` reads the tree to keep it that way.
    ///
    /// **Each op is checked against the shape of id it acts on**, not merely
    /// against the id parsing. A branch is created *in a repository* and an
    /// approval is given *on a pull request*; letting `gitea:owner/repo#142`
    /// through to `create_branch` would build the path
    /// `/repos/owner/repo#142/branches`, which is a 404 on a good day and a
    /// different repository on a bad one.
    async fn write(&self, op: WriteOp) -> Result<WriteReceipt, SourceError> {
        match &op {
            WriteOp::CreateBranch {
                entity,
                name,
                from_ref,
            } => {
                let (owner, repo) = self.repo_of(entity)?;
                write::create_branch(&self.client, &owner, &repo, name, from_ref).await?;
                Ok(WriteReceipt::none())
            }
            WriteOp::CreatePullRequest {
                entity,
                title,
                body,
                head,
                base,
            } => {
                let (owner, repo) = self.repo_of(entity)?;
                write::create_pull_request(&self.client, &owner, &repo, title, body, head, base)
                    .await?;
                Ok(WriteReceipt::none())
            }
            WriteOp::Comment { entity, body } => {
                let (owner, repo, index) = self.pull_of(entity)?;
                write::comment(&self.client, &owner, &repo, index, body).await?;
                Ok(WriteReceipt::none())
            }
            WriteOp::Approve { entity, body } => {
                let (owner, repo, index) = self.pull_of(entity)?;
                write::approve(&self.client, &owner, &repo, index, body).await?;
                Ok(WriteReceipt::none())
            }
            WriteOp::Transition { .. }
            | WriteOp::CreateTicket { .. }
            | WriteOp::TriggerBuild { .. }
            | WriteOp::RerunBuild { .. }
            | WriteOp::LogWork { .. }
            | WriteOp::CreatePage { .. }
            | WriteOp::UpdatePage { .. }
            | WriteOp::PauseMonitor { .. }
            | WriteOp::ResumeMonitor { .. }
            | WriteOp::CreateMonitor { .. } => Err(SourceError::protocol(format!(
                "gitea: {:?} is not an operation this adapter supports",
                op.identifier()
            ))),
        }
    }

    /// Refused: Gitea has no workflow (#498).
    ///
    /// A pull request and an issue have **states**, not a workflow -- open,
    /// closed, merged are what happened to the thing rather than a
    /// configurable set of moves somebody may take next -- and Gitea offers no
    /// endpoint that answers "what can this move to from here". Which is why
    /// this adapter declares no `transition` write op either: the two are one
    /// fact, and the contract battery holds them together.
    ///
    /// **Refused rather than answered empty**, and the difference is what a
    /// reader sees. An empty list is a workflow with nowhere left to go, which
    /// is a real state a real Jira ticket can be in; this is a source that was
    /// asked the wrong question. The shell reads the refusal as "draw no
    /// select" and an empty answer as "draw a select with nothing in it".
    async fn reachable_transitions(&self, entity: &str) -> Result<Vec<String>, SourceError> {
        Err(SourceError::protocol(format!(
            "gitea: this adapter has no workflow, so there are no reachable transitions to read \
             for {entity:?}"
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
            account: None,
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
        assert_eq!(
            d.write_ops,
            vec!["create_branch", "create_pull_request", "comment", "approve"]
        );
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
                matches!(error, SourceError::Protocol { message: ref m, .. } if m.contains(bad)),
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
            Err(SourceError::Unauthorized { .. })
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
                matches!(error, SourceError::Protocol { message: ref m, .. } if m.contains("token")),
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
            matches!(error, SourceError::Protocol { message: ref m, .. } if m.contains("owner")),
            "{error:?}"
        );
    }

    /// Battery clause 5 from this adapter's side: an op the descriptor does not
    /// declare is refused **by name**, so a descriptor that drifted from its
    /// dispatch is diagnosable rather than merely broken.
    ///
    /// The base URL here is a real hostname nothing resolves to, so a refusal
    /// that leaked through to the network would surface as `Unreachable` and
    /// fail this rather than pass.
    #[tokio::test]
    async fn an_op_this_adapter_does_not_declare_is_refused_by_name() {
        let source = build(instance()).unwrap();
        for (op, name) in [
            (
                WriteOp::Transition {
                    entity: "gitea:tidewater/payout-service#142".to_owned(),
                    status: "Done".to_owned(),
                },
                "transition",
            ),
            (
                WriteOp::CreateTicket {
                    entity: "gitea:tidewater/payout-service".to_owned(),
                    title: "t".to_owned(),
                    body: String::new(),
                    ticket_type: "Task".to_owned(),
                },
                "create_ticket",
            ),
            (
                WriteOp::TriggerBuild {
                    entity: "gitea:tidewater/payout-service".to_owned(),
                },
                "trigger_build",
            ),
            (
                WriteOp::RerunBuild {
                    entity: "gitea:tidewater/payout-service".to_owned(),
                },
                "rerun_build",
            ),
        ] {
            let refused = source.write(op).await.unwrap_err();
            assert!(
                matches!(refused, SourceError::Protocol { message: ref m, .. } if m.contains(name)),
                "{name}: {refused:?}"
            );
        }
    }

    /// A write aimed at another instance is refused rather than performed here
    /// with its key half: `tidewater/payout-service#142` is a plausible key on
    /// every Gitea, so believing the routing is how an approval lands on the
    /// wrong server.
    #[tokio::test]
    async fn a_write_for_another_source_is_refused_rather_than_performed_here() {
        let source = build(instance()).unwrap();
        let refused = source
            .write(WriteOp::Approve {
                entity: "gitea-eu:tidewater/payout-service#142".to_owned(),
                body: String::new(),
            })
            .await
            .unwrap_err();
        assert!(
            matches!(refused, SourceError::Protocol { message: ref m, .. } if m.contains("gitea-eu")),
            "{refused:?}"
        );
    }

    /// Each op is checked against the **shape** of id it acts on, before any
    /// path is built. A pull-request id handed to `create_branch` would compose
    /// `/repos/tidewater/payout-service#142/branches`; a repository id handed
    /// to `approve` would compose a review path with no index at all.
    #[tokio::test]
    async fn an_id_of_the_wrong_shape_is_refused_before_a_path_is_built() {
        let source = build(instance()).unwrap();
        let repo = "gitea:tidewater/payout-service";
        let pull = "gitea:tidewater/payout-service#142";
        let branch = "gitea:tidewater/payout-service@refs/heads/main";

        for (op, expected) in [
            (
                WriteOp::CreateBranch {
                    entity: pull.to_owned(),
                    name: "feature/x".to_owned(),
                    from_ref: "main".to_owned(),
                },
                "not a repository",
            ),
            (
                WriteOp::CreatePullRequest {
                    entity: branch.to_owned(),
                    title: "t".to_owned(),
                    body: String::new(),
                    head: "feature/x".to_owned(),
                    base: "main".to_owned(),
                },
                "not a repository",
            ),
            (
                WriteOp::Comment {
                    entity: repo.to_owned(),
                    body: "on what?".to_owned(),
                },
                "not a pull request",
            ),
            (
                WriteOp::Approve {
                    entity: repo.to_owned(),
                    body: String::new(),
                },
                "not a pull request",
            ),
        ] {
            let refused = source.write(op).await.unwrap_err();
            assert!(
                matches!(refused, SourceError::Protocol { message: ref m, .. } if m.contains(expected)),
                "{expected}: {refused:?}"
            );
        }
    }
}
