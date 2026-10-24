//! The `Source` implementation: what the sync engine and *Test connection*
//! hold.

use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::sync::{Mutex, OnceLock, PoisonError};

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
    /// Which token on which server this is, for [`Self::viewer`]'s cache: a
    /// hash of the instance id, the base URL and the secret, never the secret.
    viewer_key: u64,
}

/// Who each token is, by [`GiteaSource::viewer_key`], for the life of the
/// process (issue #705).
///
/// The app builds a source for every view it reads, so a cache on the
/// instance would last one read; this one outlives them. A login is cached
/// only once Gitea answered it. A user renamed in Gitea leaves the old login
/// here until restart, which only leaves the review actions enabled on their
/// own pull requests, and Gitea's refusal then says why (ADR-0004).
fn viewers() -> &'static Mutex<HashMap<u64, String>> {
    static VIEWERS: OnceLock<Mutex<HashMap<u64, String>>> = OnceLock::new();
    VIEWERS.get_or_init(Default::default)
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
    let viewer_key = {
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        (&instance.id, &instance.base_url, &instance.secret).hash(&mut hasher);
        hasher.finish()
    };
    let credential = client::credential(
        &client::CREDENTIAL_RULES,
        instance.auth,
        None,
        instance.secret.as_deref(),
    )?;
    let client = GiteaClient::new(&instance.base_url, &config, credential)?;
    Ok(Box::new(GiteaSource {
        id: instance.id,
        display_name: instance.display_name,
        config,
        client,
        viewer_key,
    }))
}

impl GiteaSource {
    /// The login the token belongs to, or `None` where Gitea would not say.
    ///
    /// Asked once per token and server for the life of the process
    /// ([`viewers`]): every whole pull request view wants it (#705), and one
    /// `GET /user` per view, read-ahead included, would be a request spent on
    /// an answer that does not change. A failed read is not cached, and is not
    /// the view's failure: the review actions stay enabled and Gitea's
    /// refusal, should it come, is the write queue's to show (ADR-0004).
    pub(crate) async fn viewer(&self) -> Option<String> {
        if let Some(login) = viewers()
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&self.viewer_key)
        {
            return Some(login.clone());
        }
        let user = self.client.current_user().await.ok()?;
        let login = user.login.filter(|l| !l.trim().is_empty())?;
        viewers()
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(self.viewer_key, login.clone());
        Some(login)
    }

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
    pub(crate) fn key_of(&self, entity: &str) -> Result<crate::keys::GiteaKey, SourceError> {
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

    /// The branch a write acts on: its repository and its name.
    ///
    /// # Errors
    ///
    /// As [`Self::key_of`], plus an id that names anything but a branch.
    fn branch_of(&self, entity: &str) -> Result<(String, String, String), SourceError> {
        match self.key_of(entity)? {
            crate::keys::GiteaKey::Branch {
                owner,
                repo,
                branch,
            } => Ok((owner, repo, branch)),
            _ => Err(SourceError::protocol(format!(
                "gitea: {entity:?} is not a branch -- this operation is performed on one, so the \
                 target is an id like \"{}:owner/repo@refs/heads/feature/x\"",
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

    /// The issue a write or a read acts on (#1206).
    ///
    /// # Errors
    ///
    /// As [`Self::key_of`], plus an id that names anything but an issue.
    fn issue_of(&self, entity: &str) -> Result<(String, String, u64), SourceError> {
        match self.key_of(entity)? {
            crate::keys::GiteaKey::Issue {
                owner,
                repo,
                number,
            } => Ok((owner, repo, number)),
            _ => Err(SourceError::protocol(format!(
                "gitea: {entity:?} is not an issue -- this operation is performed on one, so the \
                 target is an id like \"{}:owner/repo/issues/7\"",
                self.id
            ))),
        }
    }

    /// What a comment is written on: a pull request or an issue, whose
    /// discussions Gitea keeps on one endpoint.
    ///
    /// # Errors
    ///
    /// As [`Self::key_of`], plus an id that names neither.
    fn discussion_of(&self, entity: &str) -> Result<(String, String, u64), SourceError> {
        match self.key_of(entity)? {
            crate::keys::GiteaKey::Pr {
                owner,
                repo,
                number,
            }
            | crate::keys::GiteaKey::Issue {
                owner,
                repo,
                number,
            } => Ok((owner, repo, number)),
            _ => Err(SourceError::protocol(format!(
                "gitea: {entity:?} is not a pull request or an issue -- a comment is written on \
                 one, so the target is an id like \"{}:owner/repo#142\"",
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

    /// Perform one of the four writes M2 ratified for Gitea, the merge the
    /// source views added (#667), the review the pull request's toolbar
    /// writes (#705), its edit, close and reopen (#706), the branch delete
    /// (#707), the branch update (#708) or a file commit (#822), or refuse.
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
                let (owner, repo, index) = self.discussion_of(entity)?;
                write::comment(&self.client, &owner, &repo, index, body).await?;
                Ok(WriteReceipt::none())
            }
            WriteOp::Approve { entity, body } => {
                let (owner, repo, index) = self.pull_of(entity)?;
                write::approve(&self.client, &owner, &repo, index, body).await?;
                Ok(WriteReceipt::none())
            }
            WriteOp::MergePullRequest {
                entity,
                style,
                delete_branch,
                message,
            } => {
                let (owner, repo, index) = self.pull_of(entity)?;
                let how = write::Merge {
                    style: *style,
                    delete_branch: *delete_branch,
                    message: message.as_deref(),
                };
                write::merge(&self.client, &owner, &repo, index, &how).await?;
                Ok(WriteReceipt::none())
            }
            WriteOp::UpdatePullRequestBranch { entity, style } => {
                let (owner, repo, index) = self.pull_of(entity)?;
                write::update_branch(&self.client, &owner, &repo, index, *style).await?;
                Ok(WriteReceipt::none())
            }
            WriteOp::SubmitReview {
                entity,
                verdict,
                body,
                comments,
            } => {
                let (owner, repo, index) = self.pull_of(entity)?;
                write::submit_review(&self.client, &owner, &repo, index, *verdict, body, comments)
                    .await?;
                Ok(WriteReceipt::none())
            }
            WriteOp::DeleteBranch { entity } => {
                let (owner, repo, branch) = self.branch_of(entity)?;
                write::delete_branch(&self.client, &owner, &repo, &branch).await?;
                Ok(WriteReceipt::none())
            }
            WriteOp::EditPullRequest {
                entity,
                title,
                body,
                state,
            } => {
                let (owner, repo, index) = self.pull_of(entity)?;
                write::edit_pull_request(
                    &self.client,
                    &owner,
                    &repo,
                    index,
                    title.as_deref(),
                    body.as_deref(),
                    *state,
                )
                .await?;
                Ok(WriteReceipt::none())
            }
            WriteOp::CommitFile {
                repo: target,
                branch,
                new_branch,
                path,
                content,
                message,
                base_sha,
            } => {
                let (owner, repo) = self.repo_of(target)?;
                let commit = write::Commit {
                    branch,
                    new_branch: new_branch.as_deref(),
                    path,
                    content,
                    message,
                    base_sha,
                };
                let made = write::commit_file(&self.client, &owner, &repo, &commit).await?;
                Ok(made.map_or_else(WriteReceipt::none, WriteReceipt::id))
            }
            WriteOp::SourceAction { entity, id, fields } => {
                // The id first: one this adapter does not declare is refused
                // whatever it targets (battery clause 11).
                if id != crate::SOURCE_ACTION_ADD_LABEL {
                    return Err(SourceError::protocol(format!(
                        "gitea: {id:?} is not a source action this adapter declares"
                    )));
                }
                let (owner, repo, index) = self.pull_of(entity)?;
                let given = fields.get(crate::ADD_LABEL_FIELD);
                let label = given
                    .and_then(|label| label.parse::<u64>().ok())
                    .ok_or_else(|| {
                        SourceError::protocol(format!(
                            "gitea: {id} needs a label id, got {given:?}"
                        ))
                    })?;
                write::add_label(&self.client, &owner, &repo, index, label).await?;
                Ok(WriteReceipt::none())
            }
            WriteOp::Transition { entity, status } => {
                let (owner, repo, index) = self.issue_of(entity)?;
                write::transition(&self.client, &owner, &repo, index, status).await?;
                Ok(WriteReceipt::none())
            }
            WriteOp::CreateTicket {
                entity,
                title,
                body,
                ticket_type: _,
            } => {
                let (owner, repo) = self.repo_of(entity)?;
                let made = write::create_ticket(&self.client, &owner, &repo, title, body).await?;
                // The issue's **key**, not its bare number (#1754): a create's
                // receipt is the local part of what it made, so the app takes
                // `gitea:<key>` as this write's ticket, as it takes Jira's.
                Ok(made.map_or_else(WriteReceipt::none, |number| {
                    WriteReceipt::id(crate::keys::issue_key(&owner, &repo, number))
                }))
            }
            WriteOp::TriggerBuild { .. }
            | WriteOp::RerunBuild { .. }
            | WriteOp::LogWork { .. }
            | WriteOp::CreatePage { .. }
            | WriteOp::UpdatePage { .. }
            | WriteOp::PauseMonitor { .. }
            | WriteOp::ResumeMonitor { .. }
            | WriteOp::CreateMonitor { .. }
            | WriteOp::SetParameter { .. }
            | WriteOp::RunBuild { .. } => Err(SourceError::protocol(format!(
                "gitea: {:?} is not an operation this adapter supports",
                op.identifier()
            ))),
        }
    }

    /// An issue's workflow (#1206): its two states, so from where it stands
    /// the one move is to the other. Read live, as Jira's is, so a select is
    /// never built from a state the mirror has not caught up with.
    ///
    /// Refused for anything but an issue: a pull request is closed and
    /// reopened by `edit_pull_request`, and has a third state, merged, that
    /// no move reaches.
    async fn reachable_transitions(&self, entity: &str) -> Result<Vec<String>, SourceError> {
        let (owner, repo, number) = self.issue_of(entity)?;
        let issue = self.client.issue(&owner, &repo, number).await?;
        let state = issue["state"]
            .as_str()
            .and_then(crate::model::issue_state)
            .ok_or_else(|| {
                SourceError::protocol(format!(
                    "gitea: {entity:?} answered no state an issue has: {}",
                    issue["state"]
                ))
            })?;
        Ok(crate::model::ISSUE_STATES
            .into_iter()
            .filter(|other| *other != state)
            .map(str::to_owned)
            .collect())
    }

    /// A commit's overview and diff; `None` for the kinds whose views are
    /// later tickets' (ADR-0020, #661). See [`crate::view`].
    async fn view(
        &self,
        entity: &str,
        section: Option<&str>,
    ) -> Result<Option<knobas_source::view::SourceView>, SourceError> {
        crate::view::draw(self, entity, section).await
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
        assert_eq!(d.entity_kinds.len(), 5);
        assert_eq!(d.write_ops, crate::descriptor_template().write_ops);
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
                (crate::KIND_TICKET, false),
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
                WriteOp::LogWork {
                    entity: "gitea:tidewater/payout-service/issues/7".to_owned(),
                    seconds: 60,
                    started: chrono::Utc::now(),
                    comment: String::new(),
                },
                "log_work",
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
        let issue = "gitea:tidewater/payout-service/issues/7";

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
            (
                WriteOp::Approve {
                    entity: issue.to_owned(),
                    body: String::new(),
                },
                "not a pull request",
            ),
            (
                WriteOp::Transition {
                    entity: pull.to_owned(),
                    status: "closed".to_owned(),
                },
                "not an issue",
            ),
            (
                WriteOp::Transition {
                    entity: issue.to_owned(),
                    status: "Done".to_owned(),
                },
                "open or closed",
            ),
            (
                WriteOp::CreateTicket {
                    entity: issue.to_owned(),
                    title: "t".to_owned(),
                    body: String::new(),
                    ticket_type: "Task".to_owned(),
                },
                "not a repository",
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
