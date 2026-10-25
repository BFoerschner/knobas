//! The TeamCity endpoints, behind a small seam.
//!
//! [`Rest`] exists so the run in `sync.rs` can be tested against a fake that
//! answers locators the way TeamCity does, without a socket -- and so the wire
//! contract is verified in one place, against `knobas-mockd`.

use knobas_http::HttpClient;
use knobas_source::instance::SourceInstance;
use knobas_source::{ConnectionInfo, SourceError};

use crate::TeamCityConfig;
use crate::http;
use crate::rest::{
    ARTIFACT_FIELDS, BUILD_CHANGE_FIELDS, BUILD_CONFIG_VIEW_FIELDS, BUILD_FIELDS, BUILD_ID_FIELDS,
    BUILD_RECORD_FIELDS,
    BUILD_TYPE_FIELDS, BUILD_VIEW_FIELDS, Build, BuildRecord, BuildType, Changes, ConfigRecord,
    CurrentUser, FAILED_TEST_FIELDS, Files, ListEnvelope, Locator, PARAMETER_FIELDS,
    PROJECT_FIELDS, PermissionAssignments, Project, Property, RECENT_BUILD_FIELDS, SERVER_FIELDS,
    Server, TestOccurrences, USER_FIELDS,
};

/// One record, kept both as parsed fields and as the JSON it arrived as --
/// `SyncItem::payload` is the raw record verbatim (spec §3a).
#[derive(Debug, Clone)]
pub(crate) struct Rec<T> {
    pub raw: serde_json::Value,
    pub rec: T,
}

/// One answer to a collection request: the records, and whether the server
/// said there are more of them.
///
/// **The second half is the whole point** (issue #114). A page's *length* says
/// nothing on its own: `count:` is a request, a TeamCity may be configured to
/// serve fewer entries per list than were asked for, and a loaded one may
/// answer partially -- so a short page means "the query ran out" *or* "we were
/// cut off", with nothing in the row count to say which. Reading it as the
/// first is a run that reports success while its watermark advances past
/// records it never saw, which is the failure class ADR-0003 and this crate's
/// `sync` module exist to refuse, and it is the reading #81 removed from the
/// Gitea adapter's four walks.
///
/// [`Page::more`] is TeamCity's own answer to that question
/// ([`ListEnvelope::next_href`]), so the judgement moves off an assumption
/// this client makes about the server and onto a statement the server makes
/// about itself.
#[derive(Debug, Clone)]
pub(crate) struct Page<T> {
    pub items: Vec<Rec<T>>,
    /// Did the server report a page after this one?
    ///
    /// `nextHref`, reduced to the one bit this adapter reads. Nothing follows
    /// the href: it is an offset walk, and `/app/rest/builds` grows at the
    /// front, so an offset walked over it skips rows -- `sync::all_of` widens
    /// `count:` and re-reads from the top instead. Reducing it here is what
    /// keeps that structural rather than a habit.
    pub more: bool,
}

/// What one sync run and one connection test need from TeamCity.
#[async_trait::async_trait]
pub(crate) trait Rest: Send + Sync {
    async fn server(&self) -> Result<Server, SourceError>;
    /// Whom this credential authenticates as. `None` when the server serves
    /// the endpoint but names nobody.
    async fn current_user(&self) -> Result<CurrentUser, SourceError>;
    async fn build_types(&self) -> Result<Page<BuildType>, SourceError>;
    /// Every project on the server, with the one it sits in (#849): read once
    /// a run, and only by a run that needs the tree.
    async fn projects(&self) -> Result<Page<Project>, SourceError>;
    async fn builds(&self, locator: &Locator) -> Result<Page<Build>, SourceError>;
    /// Is there a build with this id on the server?
    ///
    /// `GET /app/rest/builds/id:{id}`, which interfaces §4.2 already lists.
    /// The only question this adapter asks whose **negative** answer is
    /// information rather than a failure, which is why it is a `bool` and not
    /// a record: `sync::refuse_a_replaced_server` needs to know that one
    /// build is gone, not what it said.
    ///
    /// A 404 is that answer. Every other status still raises -- a 403 means
    /// the credential may not read the build, not that the build is absent,
    /// and reading one as the other would accuse an innocent server of having
    /// been replaced.
    async fn build_exists(&self, id: i64) -> Result<bool, SourceError>;
    /// One build by id, as a run lists it: `GET /app/rest/builds/id:{id}`
    /// with [`BUILD_RECORD_FIELDS`], kept verbatim for
    /// [`crate::map::build_item`] (a refresh, ADR-0031).
    ///
    /// `None` is TeamCity's 404, a build it no longer has. Every other
    /// status raises, for [`Self::build_exists`]' reason: a 403 is a build
    /// the credential may not read, not one that is gone.
    async fn build(&self, id: i64) -> Result<Option<Rec<Build>>, SourceError>;
}

/// The real transport.
#[derive(Debug)]
pub(crate) struct HttpRest {
    pub(crate) client: HttpClient,
}

impl HttpRest {
    /// # Errors
    ///
    /// Whatever [`http::credential`] and [`http::client`] report: an
    /// unauthenticatable configuration, a missing secret, or a base URL that
    /// is not one.
    pub(crate) fn new(
        instance: &SourceInstance,
        cfg: &TeamCityConfig,
    ) -> Result<Self, SourceError> {
        let credential = http::credential(
            &http::CREDENTIAL_RULES,
            instance.auth,
            http::nonblank_username(cfg),
            instance.secret.as_deref(),
        )?;
        Ok(Self {
            client: http::client(&instance.base_url, cfg, credential)?,
        })
    }

    /// One record, parsed **and** kept verbatim.
    ///
    /// Every response is decoded to `serde_json::Value` first and then into
    /// the typed shape, because `SyncItem::payload` is the raw record and
    /// re-serialising the typed one would silently drop every field this
    /// adapter does not model (spec §3a's re-mapping guarantee).
    async fn get_raw(
        &self,
        path: &str,
        query: &[(&str, &str)],
    ) -> Result<serde_json::Value, SourceError> {
        self.client.get_json::<serde_json::Value>(path, query).await
    }

    /// One collection request, decoded into its records **and** the server's
    /// own statement about whether there are more of them ([`Page`]).
    async fn list<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        query: &[(&str, &str)],
    ) -> Result<Page<T>, SourceError> {
        let body = self.get_raw(path, query).await?;
        let envelope: ListEnvelope = serde_json::from_value(body).map_err(|e| {
            SourceError::protocol(format!(
                "{path} did not answer with the collection envelope the TeamCity REST contract \
                 documents: {e}"
            ))
        })?;
        // Read before the elements are consumed, and never inferred from their
        // number: that inference is issue #114.
        let more = envelope.next_href.is_some();
        let items = envelope
            .items
            .into_iter()
            .map(|raw| {
                serde_json::from_value(raw.clone())
                    .map(|rec| Rec { raw, rec })
                    .map_err(|e| SourceError::protocol(format!("teamcity: {path}: {e}")))
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Page { items, more })
    }

    async fn one<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        query: &[(&str, &str)],
    ) -> Result<T, SourceError> {
        let body = self.get_raw(path, query).await?;
        serde_json::from_value(body)
            .map_err(|e| SourceError::protocol(format!("teamcity: {path}: {e}")))
    }
}

#[async_trait::async_trait]
impl Rest for HttpRest {
    async fn server(&self) -> Result<Server, SourceError> {
        self.one("app/rest/server", &[("fields", SERVER_FIELDS)])
            .await
    }

    async fn current_user(&self) -> Result<CurrentUser, SourceError> {
        self.one("app/rest/users/current", &[("fields", USER_FIELDS)])
            .await
    }

    async fn build_types(&self) -> Result<Page<BuildType>, SourceError> {
        self.list("app/rest/buildTypes", &[("fields", BUILD_TYPE_FIELDS)])
            .await
    }

    async fn projects(&self) -> Result<Page<Project>, SourceError> {
        self.list("app/rest/projects", &[("fields", PROJECT_FIELDS)])
            .await
    }

    async fn builds(&self, locator: &Locator) -> Result<Page<Build>, SourceError> {
        let rendered = locator.render();
        self.list(
            "app/rest/builds",
            &[("locator", rendered.as_str()), ("fields", BUILD_FIELDS)],
        )
        .await
    }

    async fn build_exists(&self, id: i64) -> Result<bool, SourceError> {
        presence(
            self.get_raw(
                &format!("app/rest/builds/id:{id}"),
                &[("fields", BUILD_ID_FIELDS)],
            )
            .await,
        )
    }

    async fn build(&self, id: i64) -> Result<Option<Rec<Build>>, SourceError> {
        let path = format!("app/rest/builds/id:{id}");
        let raw = match self.get_raw(&path, &[("fields", BUILD_RECORD_FIELDS)]).await {
            Ok(raw) => raw,
            Err(error) if error.status() == Some(404) => return Ok(None),
            Err(error) => return Err(error),
        };
        let rec = serde_json::from_value(raw.clone())
            .map_err(|e| SourceError::protocol(format!("teamcity: {path}: {e}")))?;
        Ok(Some(Rec { raw, rec }))
    }
}

impl HttpRest {
    /// `POST /app/rest/buildQueue` -- put one configuration's build on the
    /// queue (issue #43).
    ///
    /// Not on [`Rest`], deliberately. That trait is what `sync.rs` is written
    /// against and what its fake answers; a write on it would make every read
    /// fake implement a write nothing in a sync run may call. The write path
    /// has one implementation and holds the real client, so it lives here.
    ///
    /// [`Self::queue_run`] with no branch, no override and no option, whose
    /// body is the plain `{"buildType":{"id":…}}`, and which answers the
    /// queued build's id as a custom run does (#1949). It was dropped until
    /// then, so a trigger and a re-run left a build nothing named.
    ///
    /// **Sent once, never retried by the transport** (#1122), as
    /// [`Self::queue_run`] is: a POST that timed out may have queued its
    /// build, and a resend queues a second one. TeamCity may merge two
    /// identical queued builds, but only while the first still waits, so
    /// nothing on its side makes the resend safe. A failure comes back as the
    /// write's error.
    ///
    /// # Errors
    ///
    /// The [`SourceError`] the request maps to.
    pub(crate) async fn queue_build(
        &self,
        build_type_id: &str,
    ) -> Result<Option<i64>, SourceError> {
        self.queue_run(
            build_type_id,
            None,
            &[],
            crate::write::RunOptions::default(),
        )
        .await
    }

    /// `POST /app/rest/buildQueue` for a custom run (issue #669): the
    /// trigger's body with `branchName` when there is a branch,
    /// `properties` when there are overrides, and `triggeringOptions` with
    /// the options set (#852), none sent empty.
    ///
    /// Answers the queued build's id from TeamCity's answer, which is the
    /// queued build in full. A body without one is still a run TeamCity
    /// accepted -- `knobas-http` decided that from the status -- so it is
    /// `None`, not an error.
    ///
    /// # Errors
    ///
    /// The [`SourceError`] the request maps to.
    pub(crate) async fn queue_run(
        &self,
        build_type_id: &str,
        branch: Option<&str>,
        overrides: &[knobas_source::BuildParameter],
        options: crate::write::RunOptions,
    ) -> Result<Option<i64>, SourceError> {
        let mut body = serde_json::json!({ "buildType": { "id": build_type_id } });
        if let Some(branch) = branch {
            body["branchName"] = serde_json::Value::from(branch);
        }
        if !overrides.is_empty() {
            body["properties"] = serde_json::json!({ "property": overrides });
        }
        let mut triggering = serde_json::Map::new();
        if options.clean_checkout {
            triggering.insert("cleanSources".to_owned(), true.into());
        }
        if options.first_in_queue {
            triggering.insert("queueAtTop".to_owned(), true.into());
        }
        if !triggering.is_empty() {
            body["triggeringOptions"] = serde_json::Value::Object(triggering);
        }
        let request = self
            .client
            .request(knobas_http::Method::POST, "app/rest/buildQueue")
            .json(&body)
            .no_retry();
        let response = self.client.send(request).await?;
        let answer: Option<serde_json::Value> = response.json().await.ok();
        Ok(answer.and_then(|a| a.get("id").and_then(serde_json::Value::as_i64)))
    }

    /// `PUT /app/rest/buildTypes/id:{id}/parameters/{name}/value` -- one
    /// parameter's new value, as plain text (issue #668).
    ///
    /// **`Accept: text/plain`, replacing the client's default.** The value
    /// resource produces text only, and TeamCity answers a request that
    /// accepts nothing but JSON with **406, before it writes** (measured on
    /// the seeded 2026.1.3 server, 2026-10-12). The answer -- the value now
    /// held -- is dropped.
    ///
    /// The name is one path segment ([`crate::ids::parameter_segment`]):
    /// TeamCity's names are the user's, and a `/` or `?` in one must not move
    /// the request.
    ///
    /// # Errors
    ///
    /// The [`SourceError`] the request maps to, or a refusal of a name that
    /// cannot be one segment, before a request.
    pub(crate) async fn put_parameter(
        &self,
        build_type_id: &str,
        name: &str,
        value: &str,
    ) -> Result<(), SourceError> {
        let path = format!(
            "app/rest/buildTypes/id:{build_type_id}/parameters/{}/value",
            crate::ids::parameter_segment(name)?
        );
        let request = self
            .client
            .request(knobas_http::Method::PUT, &path)
            .header(
                knobas_http::header::ACCEPT,
                knobas_http::header::HeaderValue::from_static("text/plain"),
            )
            .text(value.to_owned());
        self.client.send(request).await.map(|_| ())
    }

    /// `GET /app/rest/buildTypes/id:{id}/parameters/{name}` -- one parameter
    /// as the configuration holds it now, own or inherited, with the fields
    /// the view draws it from (issue #668: `set_parameter`'s check against
    /// the value the reader edited).
    ///
    /// # Errors
    ///
    /// Whatever the request maps to, a 404 for a configuration or parameter
    /// that is gone included, or a refusal of a name that cannot be one
    /// segment, before a request.
    pub(crate) async fn parameter(
        &self,
        build_type_id: &str,
        name: &str,
    ) -> Result<Property, SourceError> {
        self.one(
            &format!(
                "app/rest/buildTypes/id:{build_type_id}/parameters/{}",
                crate::ids::parameter_segment(name)?
            ),
            &[("fields", PARAMETER_FIELDS)],
        )
        .await
    }

    /// Which build configuration a build belonged to.
    ///
    /// `GET /app/rest/builds/id:{id}` with an explicit `fields=` like every
    /// other read this adapter makes -- asking for `buildType(id)` and nothing
    /// else, because a re-run needs one string and a bare request would drag
    /// the whole build record back.
    ///
    /// # Errors
    ///
    /// [`SourceError::Protocol`] when the build exists and names no
    /// configuration; otherwise whatever the request maps to, a 404 for a
    /// build that is gone included.
    pub(crate) async fn build_type_of(&self, id: i64) -> Result<String, SourceError> {
        let body = self
            .get_raw(
                &format!("app/rest/builds/id:{id}"),
                &[("fields", "buildType(id)")],
            )
            .await?;
        body.get("buildType")
            .and_then(|t| t.get("id"))
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| {
                SourceError::protocol(format!(
                    "teamcity: build {id} names no build configuration, so there is nothing to \
                     run again"
                ))
            })
    }
}

impl HttpRest {
    /// `GET /app/rest/users/current/permissions` -- what the credential's
    /// user may do, and where (#2040). Asked with no locator: the whole list
    /// is one answer, and its `permission:(id:…)` form is refused.
    ///
    /// # Errors
    ///
    /// Whatever the request maps to.
    pub(crate) async fn permissions(&self) -> Result<PermissionAssignments, SourceError> {
        self.one("app/rest/users/current/permissions", &[]).await
    }

    /// The project a configuration lives in (#2040).
    ///
    /// # Errors
    ///
    /// Whatever the request maps to, and an answer naming no project.
    pub(crate) async fn config_project(&self, id: &str) -> Result<String, SourceError> {
        let body = self
            .get_raw(
                &format!("app/rest/buildTypes/id:{id}"),
                &[("fields", "projectId")],
            )
            .await?;
        project_named(body.get("projectId"), id)
    }

    /// The project a build's configuration lives in (#2040).
    ///
    /// # Errors
    ///
    /// As [`Self::config_project`].
    pub(crate) async fn build_project(&self, id: i64) -> Result<String, SourceError> {
        let body = self
            .get_raw(
                &format!("app/rest/builds/id:{id}"),
                &[("fields", "buildType(projectId)")],
            )
            .await?;
        project_named(
            body.get("buildType").and_then(|t| t.get("projectId")),
            &id.to_string(),
        )
    }
}

fn project_named(value: Option<&serde_json::Value>, of: &str) -> Result<String, SourceError> {
    value
        .and_then(serde_json::Value::as_str)
        .filter(|p| !p.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| SourceError::protocol(format!("teamcity: {of} names no project")))
}

impl HttpRest {
    /// `GET /app/rest/buildTypes/id:{id}` -- one build configuration with its
    /// parameters, steps, triggers and VCS roots, for its source view (#664).
    ///
    /// Not on [`Rest`], for [`Self::queue_build`]'s reason: that trait is the
    /// sync run's, and its fake has no business answering a view.
    ///
    /// # Errors
    ///
    /// Whatever the request maps to, a 404 for a configuration that is gone
    /// included.
    pub(crate) async fn build_config(&self, id: &str) -> Result<ConfigRecord, SourceError> {
        self.one(
            &format!("app/rest/buildTypes/id:{id}"),
            &[("fields", BUILD_CONFIG_VIEW_FIELDS)],
        )
        .await
    }

    /// The newest `count` builds of one configuration, in every state and on
    /// every branch, newest first -- the view's *Recent builds* (#664).
    ///
    /// `defaultFilter:false` and `branch:default:any`: the sync's item queries
    /// keep personal builds out (see [`Locator::default_filter`]), but a list
    /// of what a configuration has been doing lately is exactly the question
    /// "every build, whatever its state" that the probe asks, and nothing
    /// here becomes an item.
    ///
    /// # Errors
    ///
    /// Whatever the request maps to.
    pub(crate) async fn recent_builds(
        &self,
        build_type_id: &str,
        count: u32,
    ) -> Result<Vec<Build>, SourceError> {
        let locator = Locator {
            build_type_id: Some(build_type_id.to_owned()),
            branch_any: true,
            default_filter: Some(false),
            count,
            ..Locator::default()
        }
        .render();
        let page = self
            .list::<Build>(
                "app/rest/builds",
                &[
                    ("locator", locator.as_str()),
                    ("fields", RECENT_BUILD_FIELDS),
                ],
            )
            .await?;
        Ok(page.items.into_iter().map(|r| r.rec).collect())
    }
}

impl HttpRest {
    /// `GET /app/rest/buildTypes?locator=<locator>` with
    /// [`CHAIN_TYPE_FIELDS`](crate::chain::CHAIN_TYPE_FIELDS): the
    /// configurations a chain locator names, each with what it depends on
    /// (#857). Not on [`Rest`], for [`Self::build_config`]'s reason.
    ///
    /// # Errors
    ///
    /// Whatever the request maps to.
    pub(crate) async fn chain_types(
        &self,
        locator: &str,
    ) -> Result<Vec<crate::chain::ChainType>, SourceError> {
        #[derive(serde::Deserialize)]
        struct Listing {
            #[serde(rename = "buildType", default)]
            build_type: Vec<crate::chain::ChainType>,
        }
        let listing: Listing = self
            .one(
                "app/rest/buildTypes",
                &[
                    ("locator", locator),
                    ("fields", crate::chain::CHAIN_TYPE_FIELDS),
                ],
            )
            .await?;
        Ok(listing.build_type)
    }

    /// `GET /app/rest/builds?locator=<locator>` with [`BUILD_FIELDS`], each
    /// build kept verbatim for [`crate::map::build_item`] (#857).
    ///
    /// # Errors
    ///
    /// Whatever the request maps to.
    pub(crate) async fn builds_at(&self, locator: &str) -> Result<Vec<Rec<Build>>, SourceError> {
        let page = self
            .list::<Build>(
                "app/rest/builds",
                &[("locator", locator), ("fields", BUILD_FIELDS)],
            )
            .await?;
        Ok(page.items)
    }

    /// One build's configuration and branch, and whether that is the
    /// default branch (#857).
    ///
    /// # Errors
    ///
    /// Whatever the request maps to, a 404 for a build that is gone included.
    pub(crate) async fn build_branch(&self, id: i64) -> Result<Build, SourceError> {
        self.one(
            &format!("app/rest/builds/id:{id}"),
            &[("fields", crate::chain::BUILD_BRANCH_FIELDS)],
        )
        .await
    }
}

impl HttpRest {
    /// `GET /app/rest/builds/id:{id}` -- one build for its source view
    /// (#665). Not on [`Rest`], for [`Self::build_config`]'s reason.
    ///
    /// # Errors
    ///
    /// Whatever the request maps to, a 404 for a build that is gone included.
    pub(crate) async fn build_record(&self, id: i64) -> Result<BuildRecord, SourceError> {
        self.one(
            &format!("app/rest/builds/id:{id}"),
            &[("fields", BUILD_VIEW_FIELDS)],
        )
        .await
    }

    /// `GET /app/rest/changes?locator=build:(id:{id})` -- the commits a build
    /// built (#665).
    ///
    /// # Errors
    ///
    /// Whatever the request maps to.
    pub(crate) async fn build_changes(&self, id: i64) -> Result<Changes, SourceError> {
        let locator = format!("build:(id:{id})");
        self.one(
            "app/rest/changes",
            &[
                ("locator", locator.as_str()),
                ("fields", BUILD_CHANGE_FIELDS),
            ],
        )
        .await
    }

    /// `GET /app/rest/testOccurrences?locator=build:(id:{id}),status:FAILURE,count:100`
    /// -- a build's failed tests, the first hundred (#665).
    ///
    /// # Errors
    ///
    /// Whatever the request maps to.
    pub(crate) async fn failed_tests(&self, id: i64) -> Result<TestOccurrences, SourceError> {
        let locator = format!("build:(id:{id}),status:FAILURE,count:{FAILED_TESTS}");
        self.one(
            "app/rest/testOccurrences",
            &[
                ("locator", locator.as_str()),
                ("fields", FAILED_TEST_FIELDS),
            ],
        )
        .await
    }

    /// `GET /app/rest/builds/id:{id}/artifacts` -- a build's top-level
    /// artifacts (#665); TeamCity's own `.teamcity` directory is hidden.
    ///
    /// # Errors
    ///
    /// Whatever the request maps to.
    pub(crate) async fn artifacts(&self, id: i64) -> Result<Files, SourceError> {
        self.one(
            &format!("app/rest/builds/id:{id}/artifacts"),
            &[("fields", ARTIFACT_FIELDS)],
        )
        .await
    }

    /// The last `lines` lines of a build's log, and whether the log was
    /// longer (#665).
    ///
    /// `GET /downloadBuildLog.html?buildId={id}` -- not `/app/rest` at all:
    /// the REST log resource is POST-only (it *adds* a message), and this is
    /// the URL behind the build log page's *Download* link, which answers a
    /// bearer token like the REST paths do (testenv/seed-teamcity-builds.sh).
    ///
    /// **Read as a stream and never held whole.** A log can run to hundreds
    /// of megabytes; only the last [`LOG_TAIL_BYTES`] are kept while it
    /// arrives, and the tail is cut from those.
    ///
    /// # Errors
    ///
    /// Whatever the request maps to; a body that breaks off mid-way is
    /// [`SourceError::Unreachable`].
    pub(crate) async fn log_tail(
        &self,
        id: i64,
        lines: usize,
    ) -> Result<(String, bool), SourceError> {
        let request = self
            .client
            .request(knobas_http::Method::GET, "downloadBuildLog.html")
            .query(&[("buildId", id)]);
        // Streamed: a long log takes longer than a request may (#1012).
        let mut response = self
            .client
            .send_streaming(request, knobas_http::STALL_TIMEOUT)
            .await?;
        let mut kept: Vec<u8> = Vec::new();
        let mut cut = false;
        while let Some(chunk) = response.chunk().await.map_err(|error| {
            SourceError::Unreachable(format!(
                "teamcity: the log of build {id} broke off: {error}"
            ))
        })? {
            kept.extend_from_slice(&chunk);
            if kept.len() > 2 * LOG_TAIL_BYTES {
                kept.drain(..kept.len() - LOG_TAIL_BYTES);
                cut = true;
            }
        }
        Ok(tail(&kept, cut, lines))
    }
}

/// How many failed tests *Tests* lists (the ticket's `count:100`).
pub(crate) const FAILED_TESTS: u32 = 100;

/// How much of a log's end is kept while it streams: room for the tail's
/// lines at any sane width, and a ceiling on what one tab can cost.
pub(crate) const LOG_TAIL_BYTES: usize = 512 * 1024;

/// The last `lines` lines of `bytes`, and whether anything came before them.
///
/// `cut` says `bytes` is already the end of something longer: its first line
/// is then a fragment, and it is dropped. Invalid UTF-8 is replaced, never an
/// error -- a build log is whatever the build printed.
pub(crate) fn tail(bytes: &[u8], cut: bool, lines: usize) -> (String, bool) {
    let text = String::from_utf8_lossy(bytes);
    let mut all: Vec<&str> = text.lines().collect();
    if cut && !all.is_empty() {
        all.remove(0);
    }
    let truncated = cut || all.len() > lines;
    let kept = &all[all.len().saturating_sub(lines)..];
    (kept.join("\n"), truncated)
}

/// What a by-id fetch's answer means: present, absent, or neither.
///
/// A free function rather than a `match` inside [`Rest::build_exists`] so it
/// can be asserted per status without an HTTP server. That is not a stylistic
/// preference: `sync::refuse_a_replaced_server` turns `Ok(false)` into an
/// accusation that the server was replaced, so the set of answers that reach
/// `Ok(false)` is exactly as load-bearing as the refusal it feeds, and the
/// refusal has a test that dies when it is removed. This did not.
///
/// **Only a 404.** It is the one status that says the build is not there.
/// A 403 says the credential may not read it -- a different claim, and the
/// one that would resurrect issue #91's false refusal through a new door, on
/// a healthy server whose token was narrowed after the fact. A 500 or a
/// timeout say nothing at all, and a run that cannot get an answer must fail
/// rather than guess at one. `status()` rather than a message match: ADR-0004
/// carries the status precisely so nobody has to parse prose to tell a 404
/// from a 403.
///
/// Deliberately narrower than the Gitea adapter's `matches!(error.status(),
/// Some(403 | 404))`, which reads both as absence because a repository the
/// credential cannot see is one it cannot mirror either. Here the two
/// statuses answer different questions, so they get different answers.
fn presence(answer: Result<serde_json::Value, SourceError>) -> Result<bool, SourceError> {
    match answer {
        Ok(_) => Ok(true),
        Err(error) if error.status() == Some(404) => Ok(false),
        Err(error) => Err(error),
    }
}

/// What the Add-source flow shows after *Test connection* (P4).
///
/// `user` is optional because `/app/rest/users/current` is one request more
/// than the version needs: a server that refuses it should still report the
/// version it reached rather than failing the whole test.
pub(crate) fn connection_info(server: &Server, user: Option<&CurrentUser>) -> ConnectionInfo {
    ConnectionInfo {
        // "Connected as …" is what tells a user a wrong-account token from a
        // working one, so the username wins over the display name: it is the
        // spelling TeamCity itself uses everywhere else.
        account: user.and_then(|u| u.username.clone().or_else(|| u.name.clone())),
        server_version: server.version.clone(),
        // TeamCity publishes no token expiry over REST, so the credential
        // health strip shows no countdown for this source.
        secret_expires_at: None,
        detail: server.build_number.as_ref().map(|b| format!("build {b}")),
        // Nothing per-instance for the dialog to fill: TeamCity's ids are the
        // locators the config already spells out.
        discovered: std::collections::BTreeMap::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The seam issue #114's fix hangs on: the server's own `nextHref` reaches
    /// [`Page::more`], over HTTP, through the decode.
    ///
    /// Both halves of the new rule are pinned elsewhere and **neither crosses
    /// this line**. `sync`'s unit tests hand a [`Page`] to `last_page` from a
    /// fake that constructs `more` itself, and `tests/it/mockd.rs` reads
    /// `nextHref` off the wire with a bare `reqwest`. Between them sit two
    /// lines -- the `envelope.next_href.is_some()` in [`HttpRest::list`] and
    /// [`ListEnvelope`]'s `nextHref` rename -- and deleting either puts every
    /// walk back on the page-length assumption #114 removed, silently, with
    /// the rest of the suite green. `BUILD_FIELDS` naming the field is pinned
    /// as a string in `rest.rs`; that the answer to it is *read* is pinned
    /// here.
    ///
    /// Against mockd, which answers the field the way a real TeamCity does:
    /// present exactly when the page came back filled to the rows served.
    #[tokio::test]
    async fn the_next_href_a_server_sends_reaches_page_more() {
        let mock = knobas_mockd::spawn_mock_teamcity().await;
        let instance = SourceInstance {
            id: "teamcity".to_owned(),
            kind: crate::ADAPTER_KIND.to_owned(),
            display_name: "Tidewater CI".to_owned(),
            base_url: mock.base_url(),
            auth: Some(knobas_source::AuthMethod::Pat),
            secret: Some(knobas_mockd::TEAMCITY_TOKEN.to_owned()),
            account: None,
            config: serde_json::json!({}),
        };
        let rest = HttpRest::new(&instance, &TeamCityConfig::default())
            .expect("the adapter builds against mockd");
        let page = async |count: u32| {
            rest.builds(&Locator {
                default_filter: Some(false),
                count,
                ..Locator::default()
            })
            .await
            .expect("mockd answers")
        };

        // Wider than the corpus: the one answer that ends a collection.
        let all = page(100).await;
        let total = u32::try_from(all.items.len()).expect("a small fixture");
        assert!(total >= 2, "the fixture needs more than one build: {total}");
        assert!(
            !all.more,
            "a page the server could not fill is the end of its collection"
        );

        // Filled *exactly*, which is the case a page's own length cannot tell
        // from an exhausted query -- and the reason `more` has to be read.
        let exact = page(total).await;
        assert_eq!(
            u32::try_from(exact.items.len()).expect("a small page"),
            total
        );
        assert!(
            exact.more,
            "a page filled to its limit is not proof it is the last, and `nextHref` is where \
             the server says so"
        );

        let short = page(total - 1).await;
        assert_eq!(
            u32::try_from(short.items.len()).expect("a small page"),
            total - 1
        );
        assert!(short.more, "a truncated page reports the rest");

        mock.assert_no_violations();
    }

    /// A 404 is the only answer read as "that build is not on this server",
    /// because `sync::refuse_a_replaced_server` reads that as a replaced
    /// server and refuses the run.
    ///
    /// The contract amendment for issue #91 states it as binding: "A `403` is
    /// *not* read as absence -- the credential may not read the build, which
    /// is not the same claim -- so only a 404 refuses." Widening the match to
    /// `error.status().is_some()`, or to `Some(403 | 404)` as the Gitea
    /// adapter has it, is caught here and nowhere else: every other test in
    /// this crate reaches [`presence`] through a fake or through mockd, and
    /// neither can answer a by-id fetch with a status that is not 200 or 404.
    #[test]
    fn only_a_404_is_read_as_absence() {
        assert!(
            presence(Ok(serde_json::json!({ "id": 6_520_991 }))).expect("a 200 is an answer"),
            "the build is there"
        );
        assert!(
            !presence(Err(SourceError::Protocol {
                status: Some(404),
                message: "No build found by id '999999999'.".to_owned(),
            }))
            .expect("a 404 is an answer"),
            "the build is not there"
        );
        for refused in [
            // The credential may not read this build. Reading it as absence
            // would accuse a healthy server of having been replaced.
            SourceError::Unauthorized { status: Some(403) },
            SourceError::Unauthorized { status: Some(401) },
            SourceError::Protocol {
                status: Some(500),
                message: "boom".to_owned(),
            },
            // No status at all: a transport failure answers nothing.
            SourceError::Unreachable("connect timed out".to_owned()),
        ] {
            let described = format!("{refused:?}");
            let raised = presence(Err(refused));
            assert!(
                raised.is_err(),
                "{described} says nothing about whether the build exists, so it must raise \
                 rather than become an answer: {raised:?}"
            );
        }
    }

    /// The log tail is the last lines, says when there was more, and drops
    /// the broken first line of a stream it already cut.
    #[test]
    fn a_log_tail_is_the_last_lines_and_says_when_it_was_cut() {
        let log: String = (1..=10).map(|n| format!("line {n}\n")).collect();
        assert_eq!(
            tail(log.as_bytes(), false, 3),
            ("line 8\nline 9\nline 10".to_owned(), true)
        );
        assert!(
            !tail(log.as_bytes(), false, 10).1,
            "exactly the cap is the whole log"
        );
        assert_eq!(tail(log.as_bytes(), false, 11).0.lines().count(), 10);
        // A stream already cut mid-line: the fragment is not a line.
        assert_eq!(
            tail(b"ne 4\nline 5\nline 6\n", true, 400),
            ("line 5\nline 6".to_owned(), true)
        );
        assert_eq!(tail(b"", false, 400), (String::new(), false));
        assert_eq!(tail(b"ok \xff\n", false, 400).0, "ok \u{fffd}");
    }

    fn server() -> Server {
        Server {
            version: Some("2025.07.2".to_owned()),
            build_number: Some("189456".to_owned()),
        }
    }

    #[test]
    fn the_server_and_user_records_become_a_connection_report() {
        let info = connection_info(
            &server(),
            Some(&CurrentUser {
                username: Some("mara".to_owned()),
                name: Some("Mara Lindqvist".to_owned()),
            }),
        );
        assert_eq!(info.server_version.as_deref(), Some("2025.07.2"));
        assert_eq!(
            info.account.as_deref(),
            Some("mara"),
            "the username, not the display name: it is what a wrong-account token is spotted by"
        );
        // TeamCity lists a user's tokens with their `expirationTime`, but
        // nothing says which token authenticated the request, so the
        // adapter reports none (#1513).
        assert_eq!(info.secret_expires_at, None);
        assert_eq!(info.detail.as_deref(), Some("build 189456"));
    }

    /// Every field of `ConnectionInfo` is optional (P4), and a server that
    /// says less must not make *Test connection* fail.
    #[test]
    fn a_server_that_names_nobody_still_reports_a_connection() {
        let info = connection_info(&server(), None);
        assert_eq!(info.account, None);
        assert_eq!(info.server_version.as_deref(), Some("2025.07.2"));

        let bare = connection_info(
            &Server {
                version: None,
                build_number: None,
            },
            Some(&CurrentUser {
                username: None,
                name: Some("Mara Lindqvist".to_owned()),
            }),
        );
        assert_eq!(
            bare.account.as_deref(),
            Some("Mara Lindqvist"),
            "the display name is the fallback, not nothing"
        );
        assert_eq!(bare.server_version, None);
        assert_eq!(bare.detail, None);
    }
}
