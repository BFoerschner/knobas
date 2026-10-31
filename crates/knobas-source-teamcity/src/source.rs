//! The [`Source`] implementation: what the sync engine and `test_source` hold.

use knobas_source::instance::SourceInstance;
use knobas_source::{
    ConnectionInfo, Cursor, RefreshAnswer, Sink, Source, SourceDescriptor, SourceError, SyncItem,
    WriteOp, WriteReceipt,
};

use crate::client::{HttpRest, Rest, connection_info};
use crate::rest::Locator;
use crate::write;
use crate::{TeamCityConfig, descriptor_template, sync};

/// One configured TeamCity instance.
#[derive(Debug)]
pub struct TeamCitySource {
    /// The instance id: the `EntityRef` namespace of every item this source
    /// emits, immutable once chosen (P10).
    id: String,
    display_name: String,
    /// The base URL this instance was configured with, trimmed of surrounding
    /// whitespace and of the trailing slash a paste usually carries, so a
    /// path composes onto it without doubling -- the shape `JiraSource` and
    /// `ConfluenceSource` already keep it in. Every build's `web_url` is
    /// composed from it (issue #495): the URL the user typed is the one
    /// reachable from the user's machine, and TeamCity's own `webUrl` names
    /// the server's configured *Server URL* instead.
    base_url: String,
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
        base_url: instance.base_url.trim().trim_end_matches('/').to_owned(),
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

    /// The configurations `config` is joined to by snapshot dependencies,
    /// itself included, in running order; empty for one in no chain (#857).
    async fn chain_of(&self, config: &str) -> Result<Vec<crate::chain::ChainType>, SourceError> {
        let mut types = self
            .rest
            .chain_types(&crate::chain::upstream_types(config))
            .await?;
        types.extend(
            self.rest
                .chain_types(&crate::chain::downstream_types(config))
                .await?,
        );
        Ok(crate::chain::order(types))
    }

    /// A chain configuration as the SPI's step: its pipeline id and name.
    fn chain_step(&self, t: crate::chain::ChainType) -> knobas_source::pipeline::ChainStep {
        knobas_source::pipeline::ChainStep {
            pipeline: knobas_core::entity::EntityRef::new(
                &self.id,
                &crate::map::build_config_key(&t.id),
            )
            .to_string(),
            name: t.name.unwrap_or(t.id),
        }
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
        sync::execute(
            &self.id,
            &self.base_url,
            &self.cfg,
            &self.rest,
            cursor,
            sink,
        )
        .await
    }

    /// One build read by id ([`sync::refresh`], ADR-0031, #2099): what a
    /// landed trigger, re-run or custom run is followed by, so the mirror
    /// holds the queued build its receipt names before the run after the
    /// write starts.
    ///
    /// A build configuration is [`RefreshAnswer::NotOffered`]: the one write
    /// aimed at it, a parameter set, changes nothing its item carries (the
    /// listing's id, name, project and description), so the run that follows
    /// the write is all it needs. An entity of another instance is refused,
    /// as a write's target is ([`Self::key_of`]).
    async fn refresh(&self, entity: &str) -> Result<RefreshAnswer, SourceError> {
        if !self.key_of(entity)?.starts_with("build:") {
            return Ok(RefreshAnswer::NotOffered);
        }
        let id = self.build_id(entity)?;
        sync::refresh(&self.id, &self.base_url, &self.cfg, &self.rest, id).await
    }

    /// Perform one of the two writes M2 ratified for TeamCity, or the
    /// parameter set (#668) or custom run (#669) the source views added, or
    /// refuse.
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
    /// would trigger whatever configuration happened to be called that. A
    /// parameter set and a custom run are aimed at a configuration too, and its
    /// id must also be one TeamCity could have issued, since it goes into a
    /// path or a locator.
    async fn write(&self, op: WriteOp) -> Result<WriteReceipt, SourceError> {
        match &op {
            WriteOp::TriggerBuild { entity } => {
                let queued = write::trigger(&self.rest, &self.build_config_id(entity)?).await?;
                Ok(queued_build(queued))
            }
            WriteOp::RerunBuild { entity } => {
                let queued = write::rerun(&self.rest, self.build_id(entity)?).await?;
                Ok(queued_build(queued))
            }
            WriteOp::SetParameter {
                entity,
                name,
                value,
                base,
            } => {
                let config = self.build_config_id(entity)?;
                let config = crate::ids::config_id(entity, &config)?;
                write::set_parameter(&self.rest, config, name, value, base.as_deref()).await?;
                Ok(WriteReceipt::none())
            }
            WriteOp::RunBuild {
                entity,
                branch,
                parameters,
                clean_checkout,
                first_in_queue,
            } => {
                let config = self.build_config_id(entity)?;
                let config = crate::ids::config_id(entity, &config)?;
                let options = write::RunOptions {
                    clean_checkout: *clean_checkout,
                    first_in_queue: *first_in_queue,
                };
                let queued =
                    write::run(&self.rest, config, branch.as_deref(), parameters, options).await?;
                Ok(queued_build(queued))
            }
            WriteOp::Comment { .. }
            | WriteOp::Transition { .. }
            | WriteOp::CreateTicket { .. }
            | WriteOp::CreateBranch { .. }
            | WriteOp::CreatePullRequest { .. }
            | WriteOp::Approve { .. }
            | WriteOp::LogWork { .. }
            | WriteOp::CreatePage { .. }
            | WriteOp::UpdatePage { .. }
            | WriteOp::PauseMonitor { .. }
            | WriteOp::ResumeMonitor { .. }
            | WriteOp::CreateMonitor { .. }
            | WriteOp::MergePullRequest { .. }
            | WriteOp::SubmitReview { .. }
            | WriteOp::DeleteBranch { .. }
            | WriteOp::EditPullRequest { .. }
            | WriteOp::UpdatePullRequestBranch { .. }
            | WriteOp::CommitFile { .. }
            | WriteOp::SourceAction { .. }
            | WriteOp::ReplyToThread { .. }
            | WriteOp::ResolveThread { .. }
            | WriteOp::RequestReviewers { .. } => Err(SourceError::protocol(format!(
                "the TeamCity adapter does not support {:?}",
                op.identifier()
            ))),
        }
    }

    /// Refused: TeamCity has no workflow (#498).
    ///
    /// A build's status is something that *happened to it* -- queued, running,
    /// then whatever the agent reported -- and nothing a person moves it to.
    /// The two write ops this adapter declares say the same thing from the
    /// other side: it can start a build and run one again, and neither is a
    /// transition of an existing record.
    ///
    /// **Refused rather than answered empty**: an empty list is a workflow
    /// with no moves left, and this is a source with no workflow at all. A
    /// build's detail draws no select either way, but not for the same reason
    /// -- an empty answer is a ticket with nothing to move to, while this
    /// adapter is never asked at all, the read being gated on the descriptor's
    /// `transition` op it does not declare. Contract battery clause 8 is what
    /// holds the refusal in place for any other caller.
    async fn reachable_transitions(&self, entity: &str) -> Result<Vec<String>, SourceError> {
        Err(SourceError::protocol(format!(
            "the TeamCity adapter has no workflow, so there are no reachable transitions to read \
             for {entity:?}"
        )))
    }

    /// The asks the user's TeamCity permissions refuse (#2040), read from
    /// `/app/rest/users/current/permissions` and never probed: see
    /// [`crate::grants`]. An entity of another source is not answered.
    async fn refused_writes(
        &self,
        asks: &[knobas_source::grants::WriteAsk],
    ) -> Result<Vec<knobas_source::grants::RefusedWrite>, SourceError> {
        let keyed: Vec<_> = asks
            .iter()
            .filter_map(|a| self.key_of(&a.entity).ok().map(|key| (a, key)))
            .collect();
        crate::grants::refused(&self.rest, &keyed).await
    }

    /// A build configuration's overview, parameters, steps, triggers and VCS
    /// roots (#664), and a build's overview, changes, failed tests, log tail,
    /// artifacts and parameters (#665) (ADR-0020). See [`crate::view`].
    async fn view(
        &self,
        entity: &str,
        section: Option<&str>,
    ) -> Result<Option<knobas_source::view::SourceView>, SourceError> {
        let key = self.key_of(entity)?;
        let viewer = crate::view::Viewer {
            source_id: &self.id,
            base_url: &self.base_url,
            rest: &self.rest,
        };
        crate::view::draw(&viewer, entity, &key, section).await
    }

    /// A configuration's VCS roots, triggers and parameters (#851), off the
    /// record its source view reads and scrubbed the same way.
    async fn pipeline_settings(
        &self,
        entity: &str,
    ) -> Result<Option<knobas_source::pipeline::PipelineSettings>, SourceError> {
        let config = self.build_config_id(entity)?;
        let config = crate::ids::config_id(entity, &config)?;
        let record = self.rest.build_config(config).await?;
        let mut settings = crate::view::pipeline_settings(&record);
        settings.chain = self
            .chain_of(config)
            .await?
            .into_iter()
            .map(|t| self.chain_step(t))
            .collect();
        Ok(Some(settings))
    }

    /// The chain the build is in (#857): its configuration's chain, each step
    /// with its build out of this build's own chain -- what it built on, what
    /// built on it -- else the step's newest build on this build's branch.
    async fn run_chain(
        &self,
        run: &str,
    ) -> Result<Option<Vec<knobas_source::pipeline::RunChainStep>>, SourceError> {
        let id = self.build_id(run)?;
        let build = self.rest.build_branch(id).await?;
        let Some(config) = build.build_type_id.clone() else {
            return Ok(Some(Vec::new()));
        };
        let steps = self.chain_of(&config).await?;
        if steps.is_empty() {
            return Ok(Some(Vec::new()));
        }
        let mut builds = self
            .rest
            .builds_at(&crate::chain::upstream_builds(id))
            .await?;
        builds.extend(
            self.rest
                .builds_at(&crate::chain::downstream_builds(id))
                .await?,
        );
        let picked = crate::chain::pick(
            &steps,
            &builds,
            |b| b.rec.build_type_id.as_deref(),
            |b| b.rec.id,
        );
        let mut out = Vec::with_capacity(steps.len());
        for (step, found) in steps.into_iter().zip(picked) {
            let found = match found {
                Some(b) => Some(b.clone()),
                None => self
                    .rest
                    .builds_at(&crate::chain::newest_on_branch(
                        &step.id,
                        build.branch_name.as_deref(),
                        build.default_branch == Some(true),
                    ))
                    .await?
                    .into_iter()
                    .next(),
            };
            out.push(knobas_source::pipeline::RunChainStep {
                step: self.chain_step(step),
                run: found
                    .map(|b| crate::map::build_item(&self.id, &self.base_url, &b.raw, &b.rec)),
            });
        }
        Ok(Some(out))
    }

    /// A configuration's builds older than `before` (#851): the classes the
    /// sync's per-configuration query emits -- canceled, failed to start and
    /// every branch, personal builds kept out -- so an older run is the item
    /// the sync would have made of it.
    async fn older_runs(
        &self,
        entity: &str,
        before: &str,
        count: u32,
    ) -> Result<Option<Vec<SyncItem>>, SourceError> {
        let config = self.build_config_id(entity)?;
        let config = crate::ids::config_id(entity, &config)?;
        let before = self.build_id(before)?;
        let count = count.clamp(1, knobas_source::pipeline::OLDER_RUNS_MAX);
        let page = self
            .rest
            .builds(&Locator {
                build_type_id: Some(config.to_owned()),
                until_build_id: Some(before),
                canceled_any: true,
                failed_to_start_any: true,
                branch_any: true,
                // One more than asked: TeamCity answers `before` itself too.
                count: count + 1,
                ..Locator::default()
            })
            .await?;
        Ok(Some(
            page.items
                .iter()
                .filter(|b| b.rec.id != before)
                .take(count as usize)
                .map(|b| crate::map::build_item(&self.id, &self.base_url, &b.raw, &b.rec))
                .collect(),
        ))
    }

    /// A range of a build's whole log, streamed, and its steps (#854).
    async fn run_log(
        &self,
        entity: &str,
        from: Option<u64>,
        count: u32,
    ) -> Result<Option<knobas_source::run::RunLog>, SourceError> {
        let id = self.build_id(entity)?;
        let count = count.clamp(1, knobas_source::run::LOG_LINES_MAX);
        Ok(Some(self.rest.run_log(id, from, count).await?))
    }

    /// A build's tests, failed first (#855).
    async fn run_tests(
        &self,
        entity: &str,
    ) -> Result<Option<knobas_source::run::RunTests>, SourceError> {
        let id = self.build_id(entity)?;
        Ok(Some(self.rest.run_tests(id).await?))
    }

    /// One test's last ten outcomes in the build's configuration (#855).
    async fn test_history(
        &self,
        entity: &str,
        test: &str,
    ) -> Result<Option<Vec<knobas_source::run::TestOutcome>>, SourceError> {
        let id = self.build_id(entity)?;
        Ok(Some(self.rest.test_history(&self.id, id, test).await?))
    }

    /// One folder of a build's artifacts (#856).
    async fn run_artifacts(
        &self,
        entity: &str,
        path: &str,
    ) -> Result<Option<Vec<knobas_source::run::ArtifactEntry>>, SourceError> {
        let id = self.build_id(entity)?;
        Ok(Some(self.rest.run_artifacts(id, path).await?))
    }

    /// The head of one text artifact (#856).
    async fn artifact_preview(
        &self,
        entity: &str,
        path: &str,
    ) -> Result<Option<knobas_source::run::ArtifactPreview>, SourceError> {
        let id = self.build_id(entity)?;
        Ok(Some(self.rest.artifact_preview(id, path).await?))
    }

    /// A log, a file, a folder or every artifact, streamed with the stored
    /// credential (D8, #856).
    async fn download(
        &self,
        entity: &str,
        target: &knobas_source::run::DownloadTarget,
        sink: &mut dyn knobas_source::run::DownloadSink,
    ) -> Result<bool, SourceError> {
        let id = self.build_id(entity)?;
        self.rest.download(id, target, sink).await?;
        Ok(true)
    }
}

/// The receipt of a write that put a build on the queue: the queued build's
/// key `build:<id>` (#1949), the local part of the entity id the build is
/// mirrored under, as [`WriteReceipt::remote_id`] states. A trigger, a re-run
/// and a custom run answer it alike; `None` where TeamCity's answer named no
/// id.
fn queued_build(queued: Option<i64>) -> WriteReceipt {
    queued.map_or_else(WriteReceipt::none, |id| {
        WriteReceipt::id(crate::map::build_key(id))
    })
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
            account: None,
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
        assert_eq!(
            d.write_ops,
            vec!["trigger_build", "rerun_build", "set_parameter", "run_build"]
        );
        assert_eq!(
            d.capabilities,
            vec![
                knobas_source::Capability::Write,
                knobas_source::Capability::Refresh
            ]
        );
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

    /// TeamCity's form lets a blank username through, and the transport uses
    /// a username as given: a whitespace one is no username, refused as the
    /// configuration problem it is rather than sent as Basic `"  "`.
    #[test]
    fn a_blank_username_is_no_username() {
        let mut blank = instance(json!({ "username": "  " }));
        blank.auth = Some(AuthMethod::UserPassword);
        assert!(matches!(
            build(blank),
            Err(SourceError::Protocol { message, .. }) if message.contains("username")
        ));
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
        let mut i = instance(json!({}));
        i.base_url = knobas_mockd::refused_url();
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

    /// **A build configuration is not offered** (#2099): the app runs the
    /// source after a parameter set instead, and nothing is asked of the
    /// server -- this instance's base URL answers nothing.
    #[tokio::test]
    async fn a_build_configuration_is_not_offered_a_refresh() {
        let source = built(instance(json!({})));
        let answer = source
            .refresh("teamcity-eu:buildType:Payout_Build")
            .await
            .expect("answered without a request");
        assert!(matches!(answer, RefreshAnswer::NotOffered), "{answer:?}");
    }

    /// **A refresh of another instance's item, or of a build key that names
    /// no build, is refused** before anything is read: `build:1187` exists
    /// on every TeamCity, so trusting the routing would read the wrong one.
    #[tokio::test]
    async fn a_refresh_for_another_source_or_a_malformed_build_is_refused() {
        let source = built(instance(json!({})));
        for entity in ["teamcity-us:build:1187", "teamcity-eu:build:latest"] {
            let refused = source.refresh(entity).await;
            assert!(
                matches!(refused, Err(SourceError::Protocol { .. })),
                "{entity}: {refused:?}"
            );
        }
    }
}
