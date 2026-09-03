//! The `Source` implementation: what the sync engine and `test_source` hold.

use knobas_source::instance::SourceInstance;
use knobas_source::{
    ConnectionInfo, Cursor, Sink, Source, SourceDescriptor, SourceError, WriteOp, WriteReceipt,
};

use crate::api::JiraApi;
use crate::http::{self, JiraHttp};
use crate::sync::SyncRun;
use crate::write;
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

impl JiraSource {
    /// The issue key inside an entity id this source owns.
    ///
    /// Two refusals, and both are about a write going somewhere it was not
    /// meant to. An id that does not parse cannot name anything; an id in
    /// **another source's namespace** would otherwise have its key half posted
    /// to *this* Jira, which is how a `jira-eu:PAY-231` reaches the wrong
    /// instance and moves the wrong ticket. The queue routes by namespace
    /// already, so reaching here with a foreign one is knobas' own bug -- and
    /// the adapter refuses it rather than trusting the routing.
    ///
    /// # Errors
    ///
    /// [`SourceError::Protocol`], which the queue reads as a refusal rather
    /// than something to retry: nothing about waiting makes a wrong id right.
    fn issue_key(&self, entity: &str) -> Result<String, SourceError> {
        let parsed = knobas_core::entity::EntityRef::parse(entity)
            .map_err(|error| SourceError::protocol(error.to_string()))?;
        if parsed.namespace != self.id {
            return Err(SourceError::protocol(format!(
                "{entity:?} belongs to source {:?}, not to {:?}",
                parsed.namespace, self.id
            )));
        }
        Ok(parsed.key)
    }

    /// The project key a create targets -- the same id grammar, whose key half
    /// is a project (`jira:PAY`) rather than an issue.
    ///
    /// knobas does not mirror Jira projects, so this names a container that has
    /// no entity row. That is deliberate and is what the queue's hold detection
    /// is built for: a create projects on liveness alone, and an unmirrored
    /// container is `live: false` at queue time and at flush time alike, so it
    /// never holds.
    ///
    /// # Errors
    ///
    /// As [`Self::issue_key`], plus a key that is not a project key -- Jira
    /// project keys are `[A-Z][A-Z0-9_]*`, and `jira:PAY-231` reaching here
    /// means a create was aimed at a *ticket*, which would otherwise be sent as
    /// a project that does not exist.
    fn project_key(&self, entity: &str) -> Result<String, SourceError> {
        let key = self.issue_key(entity)?;
        let looks_like_a_project = !key.is_empty()
            && key.starts_with(|c: char| c.is_ascii_uppercase())
            && key
                .chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_');
        if !looks_like_a_project {
            return Err(SourceError::protocol(format!(
                "{entity:?} is not a Jira project -- a ticket is created in a project, so the \
                 target is an id like \"{}:PAY\"",
                self.id
            )));
        }
        Ok(key)
    }
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

    /// Perform one of the three writes M2 ratified for Jira, or refuse.
    ///
    /// The match unpacks and hands the values to [`crate::write`], which never
    /// names `WriteOp` -- knobas has one outbound write path and
    /// `write_choke_point.rs` reads the tree to keep it that way, so an
    /// adapter's dispatch lives with its `impl Source` and nowhere else.
    ///
    /// **The refusal arm is the contract, not a gap** (SPI doc, battery clause
    /// 5): an adapter must reject every op absent from its own
    /// `descriptor.write_ops` with `Protocol`, and it must say which op it
    /// refused so a mis-declared descriptor is diagnosable from the message.
    async fn write(&self, op: WriteOp) -> Result<WriteReceipt, SourceError> {
        match &op {
            WriteOp::Comment { entity, body } => {
                write::comment(&self.http, &self.issue_key(entity)?, body).await?;
                Ok(WriteReceipt::none())
            }
            WriteOp::Transition { entity, status } => {
                write::transition(&self.http, &self.issue_key(entity)?, status).await?;
                Ok(WriteReceipt::none())
            }
            WriteOp::CreateTicket {
                entity,
                title,
                body,
                ticket_type,
            } => {
                let created = write::create_ticket(
                    &self.http,
                    &self.project_key(entity)?,
                    title,
                    body,
                    ticket_type,
                )
                .await?;
                // Dropped, and the value is in having asked for it: a Jira
                // that accepted the create without naming what it made is a
                // write reported as done with nothing to point at, which
                // `create_ticket` refuses rather than reports as success.
                //
                // Still dropped now that `Source::write` can carry an id back
                // (#280). A created ticket is a *mirrored entity*: the start-
                // work flow already finds it by reading the mirror, which is
                // the reading that survives a re-send, and putting the key in
                // the receipt as well would be a second answer to a question
                // that already has one. The receipt exists for what the
                // mirror cannot name -- see `WriteReceipt`.
                let _ = created;
                Ok(WriteReceipt::none())
            }
            WriteOp::LogWork {
                entity,
                started,
                seconds,
                comment,
            } => {
                let id = write::log_work(
                    &self.http,
                    &self.issue_key(entity)?,
                    *started,
                    *seconds,
                    comment,
                )
                .await?;
                // **The one receipt in knobas.** A worklog is not a mirrored
                // entity and two worklogs of the same length on the same day
                // are indistinguishable from outside, so the id Jira just
                // answered with is the only way the local copy can ever name
                // the row it stands for (issue #280).
                Ok(WriteReceipt::id(id))
            }
            WriteOp::CreateBranch { .. }
            | WriteOp::CreatePullRequest { .. }
            | WriteOp::Approve { .. }
            | WriteOp::TriggerBuild { .. }
            | WriteOp::RerunBuild { .. } => Err(SourceError::protocol(format!(
                "the Jira adapter does not support {:?}",
                op.identifier()
            ))),
        }
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
        assert_eq!(d.capabilities, vec![knobas_source::Capability::Write]);
        assert_eq!(
            d.write_ops,
            vec!["comment", "transition", "create_ticket", "log_work"]
        );
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

    /// Battery clause 5, from this adapter's side: an op the descriptor does
    /// not declare is refused rather than attempted, and the refusal **names
    /// the op** -- which is what makes a descriptor that drifted from its
    /// dispatch diagnosable instead of merely broken.
    ///
    /// The adapter here points at a port nothing listens on, so a refusal that
    /// slipped through to the network would surface as `Unreachable` and fail
    /// this loudly rather than pass quietly.
    #[tokio::test]
    async fn an_op_this_adapter_does_not_declare_is_refused_by_name() {
        let mut i = instance(json!({}));
        i.base_url = dead_port_url();
        let source = built(i);
        for (op, name) in [
            (
                WriteOp::CreateBranch {
                    entity: "jira:PAY-231".to_owned(),
                    name: "feature/x".to_owned(),
                    from_ref: "main".to_owned(),
                },
                "create_branch",
            ),
            (
                WriteOp::CreatePullRequest {
                    entity: "jira:PAY-231".to_owned(),
                    title: "t".to_owned(),
                    body: "b".to_owned(),
                    head: "feature/x".to_owned(),
                    base: "main".to_owned(),
                },
                "create_pull_request",
            ),
            (
                WriteOp::Approve {
                    entity: "jira:PAY-231".to_owned(),
                    body: String::new(),
                },
                "approve",
            ),
            (
                WriteOp::TriggerBuild {
                    entity: "jira:PAY-231".to_owned(),
                },
                "trigger_build",
            ),
            (
                WriteOp::RerunBuild {
                    entity: "jira:PAY-231".to_owned(),
                },
                "rerun_build",
            ),
        ] {
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

    /// A write aimed at another instance's namespace is refused rather than
    /// posted here with its key half. Two Jiras are `jira` and `jira-eu`
    /// (§4.1), and the key `PAY-231` exists on both -- so believing the routing
    /// is how a ticket moves on the wrong server.
    #[tokio::test]
    async fn a_write_for_another_source_is_refused_rather_than_posted_here() {
        let mut i = instance(json!({}));
        i.base_url = dead_port_url();
        let source = built(i);
        let refused = source
            .write(WriteOp::Comment {
                entity: "jira-eu:PAY-231".to_owned(),
                body: "meant for the other one".to_owned(),
            })
            .await;
        let Err(SourceError::Protocol { message, .. }) = &refused else {
            panic!("a foreign namespace must be refused, got {refused:?}");
        };
        assert!(message.contains("jira-eu"), "{message}");
        assert!(message.contains("jira"), "{message}");
    }

    /// A create is aimed at a **project**, so an id naming a ticket is a
    /// mistake the adapter can see: `PAY-231` is not a project key, and sending
    /// it would ask Jira to file the ticket in a project that does not exist.
    #[tokio::test]
    async fn a_create_aimed_at_a_ticket_rather_than_a_project_is_refused() {
        let mut i = instance(json!({}));
        i.base_url = dead_port_url();
        let source = built(i);
        let refused = source
            .write(WriteOp::CreateTicket {
                entity: "jira:PAY-231".to_owned(),
                title: "a new one".to_owned(),
                body: String::new(),
                ticket_type: "Task".to_owned(),
            })
            .await;
        let Err(SourceError::Protocol { message, .. }) = &refused else {
            panic!("a create aimed at a ticket must be refused, got {refused:?}");
        };
        assert!(message.contains("not a Jira project"), "{message}");
    }

    /// An id that does not parse names nothing, and nothing about waiting makes
    /// it parse -- so it is a refusal, which the queue does not retry.
    #[tokio::test]
    async fn an_entity_id_that_is_not_one_is_refused() {
        let mut i = instance(json!({}));
        i.base_url = dead_port_url();
        let source = built(i);
        let refused = source
            .write(WriteOp::Comment {
                entity: "PAY-231".to_owned(),
                body: "no namespace".to_owned(),
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
