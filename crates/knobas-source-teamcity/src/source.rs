//! The [`Source`] implementation: what the sync engine and `test_source` hold.

use knobas_source::instance::SourceInstance;
use knobas_source::{
    ConnectionInfo, Cursor, Sink, Source, SourceDescriptor, SourceError, WriteOp, WriteReceipt,
};

use crate::client::{HttpRest, Rest, connection_info};
use crate::write;
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

impl TeamCitySource {
    /// The key half of an entity id this source owns.
    ///
    /// The namespace check is not defensive noise: two TeamCitys are
    /// `teamcity` and `teamcity-eu` (§4.1), and `buildType:Payout_Build` is a
    /// plausible key on both -- so a write that trusted the routing would
    /// start a build on the wrong server. The queue routes by namespace;
    /// reaching here with a foreign one is knobas' own bug.
    ///
    /// # Errors
    ///
    /// [`SourceError::Protocol`] -- a refusal, which the queue does not retry.
    fn key_of(&self, entity: &str) -> Result<String, SourceError> {
        let parsed = knobas_core::entity::EntityRef::parse(entity)
            .map_err(|error| SourceError::protocol(format!("teamcity: {error}")))?;
        if parsed.namespace != self.id {
            return Err(SourceError::protocol(format!(
                "teamcity: {entity:?} belongs to source {:?}, not to {:?}",
                parsed.namespace, self.id
            )));
        }
        Ok(parsed.key)
    }

    /// The build configuration a trigger names.
    ///
    /// # Errors
    ///
    /// As [`Self::key_of`], plus an id that is not a build configuration.
    fn build_config_id(&self, entity: &str) -> Result<String, SourceError> {
        let key = self.key_of(entity)?;
        key.strip_prefix("buildType:")
            .filter(|id| !id.is_empty())
            .map(str::to_owned)
            .ok_or_else(|| {
                SourceError::protocol(format!(
                    "teamcity: {entity:?} is not a build configuration -- a build is triggered \
                     from one, so the target is an id like \"{}:buildType:Payout_Build\"",
                    self.id
                ))
            })
    }

    /// The build a re-run names.
    ///
    /// # Errors
    ///
    /// As [`Self::key_of`], plus an id that is not a build.
    fn build_id(&self, entity: &str) -> Result<i64, SourceError> {
        let key = self.key_of(entity)?;
        key.strip_prefix("build:")
            .and_then(|id| id.parse::<i64>().ok())
            .ok_or_else(|| {
                SourceError::protocol(format!(
                    "teamcity: {entity:?} is not a build -- running one again needs the build \
                     itself, so the target is an id like \"{}:build:1187\"",
                    self.id
                ))
            })
    }
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

    /// Perform one of the two writes M2 ratified for TeamCity, or refuse.
    ///
    /// The match unpacks and hands ids to [`crate::write`], which never names
    /// `WriteOp`: knobas has one outbound write path and
    /// `write_choke_point.rs` reads the tree to keep it that way.
    ///
    /// **Each op is checked against the kind of id it acts on.** A trigger is
    /// aimed at a build *configuration* (`teamcity:buildType:Payout_Build`)
    /// and a re-run at a *build* (`teamcity:build:1187`); the two key forms
    /// exist precisely so they cannot be confused (`map::build_key` /
    /// `map::build_config_key`), and sending `1187` as a configuration id
    /// would trigger whatever configuration happened to be called that.
    async fn write(&self, op: WriteOp) -> Result<WriteReceipt, SourceError> {
        match &op {
            WriteOp::TriggerBuild { entity } => {
                write::trigger(&self.rest, &self.build_config_id(entity)?).await?;
                Ok(WriteReceipt::none())
            }
            WriteOp::RerunBuild { entity } => {
                write::rerun(&self.rest, self.build_id(entity)?).await?;
                Ok(WriteReceipt::none())
            }
            WriteOp::Comment { .. }
            | WriteOp::Transition { .. }
            | WriteOp::CreateTicket { .. }
            | WriteOp::CreateBranch { .. }
            | WriteOp::CreatePullRequest { .. }
            | WriteOp::Approve { .. }
            | WriteOp::LogWork { .. }
            | WriteOp::CreatePage { .. }
            | WriteOp::UpdatePage { .. } => Err(SourceError::protocol(format!(
                "the TeamCity adapter does not support {:?}",
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
        assert_eq!(d.write_ops, vec!["trigger_build", "rerun_build"]);
        assert_eq!(d.capabilities, vec![knobas_source::Capability::Write]);
        // The claim the engine's tombstone sweep rests on travels with the
        // instance descriptor too, not only with the template.
        assert!(
            !d.entity_kinds.iter().any(|k| k.full_sync_exhaustive),
            "a TeamCity instance's full sync is a window for every kind it emits; \
             the sweep must not run after it"
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
        assert!(matches!(build(bad), Err(SourceError::Protocol { .. })));
    }

    /// Interfaces §3 "Missing": a configured source whose keychain item is
    /// gone reads as unauthorized, so the sources view offers *Re-enter*
    /// instead of a protocol error nobody can act on.
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
        i.base_url = "ci.example.com".to_owned();
        assert!(matches!(build(i), Err(SourceError::Protocol { .. })));
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

    /// Battery clause 5 from this adapter's side: an op the descriptor does not
    /// declare is refused **by name**, so a descriptor that drifted from its
    /// dispatch is diagnosable rather than merely broken.
    #[tokio::test]
    async fn an_op_this_adapter_does_not_declare_is_refused_by_name() {
        let source = built(instance(json!({})));
        for (op, name) in [
            (
                WriteOp::Comment {
                    entity: "teamcity-eu:build:1187".to_owned(),
                    body: "b".to_owned(),
                },
                "comment",
            ),
            (
                WriteOp::Transition {
                    entity: "teamcity-eu:build:1187".to_owned(),
                    status: "Done".to_owned(),
                },
                "transition",
            ),
            (
                WriteOp::Approve {
                    entity: "teamcity-eu:build:1187".to_owned(),
                    body: String::new(),
                },
                "approve",
            ),
        ] {
            let refused = source.write(op).await;
            let Err(SourceError::Protocol { message, .. }) = &refused else {
                panic!("{name} must be refused with Protocol, got {refused:?}");
            };
            assert!(message.contains(name), "{message}");
        }
    }

    /// The two key forms exist so a build and a configuration cannot be
    /// confused (`map::build_key` / `map::build_config_key`), and the write
    /// path is where confusing them would cost something: a trigger sent
    /// `1187` as a configuration id starts whatever is called that, and a
    /// re-run sent a configuration id has no build to read.
    #[tokio::test]
    async fn an_id_of_the_wrong_kind_is_refused_before_a_request_is_made() {
        let source = built(instance(json!({})));
        let refused = source
            .write(WriteOp::TriggerBuild {
                entity: "teamcity-eu:build:1187".to_owned(),
            })
            .await;
        let Err(SourceError::Protocol { message, .. }) = &refused else {
            panic!("a build handed to a trigger must be refused, got {refused:?}");
        };
        assert!(message.contains("not a build configuration"), "{message}");

        let refused = source
            .write(WriteOp::RerunBuild {
                entity: "teamcity-eu:buildType:Payout_Build".to_owned(),
            })
            .await;
        let Err(SourceError::Protocol { message, .. }) = &refused else {
            panic!("a configuration handed to a re-run must be refused, got {refused:?}");
        };
        assert!(message.contains("is not a build"), "{message}");
    }

    /// A write aimed at another instance is refused rather than performed
    /// here: `buildType:Payout_Build` is a plausible id on every TeamCity, so
    /// believing the routing is how a build starts on the wrong server.
    #[tokio::test]
    async fn a_write_for_another_source_is_refused_rather_than_performed_here() {
        let source = built(instance(json!({})));
        let refused = source
            .write(WriteOp::TriggerBuild {
                entity: "teamcity-us:buildType:Payout_Build".to_owned(),
            })
            .await;
        let Err(SourceError::Protocol { message, .. }) = &refused else {
            panic!("a foreign namespace must be refused, got {refused:?}");
        };
        assert!(message.contains("teamcity-us"), "{message}");
    }
}
