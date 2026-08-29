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
    BUILD_FIELDS, BUILD_ID_FIELDS, BUILD_TYPE_FIELDS, Build, BuildType, CurrentUser, ListEnvelope,
    Locator, SERVER_FIELDS, Server, USER_FIELDS,
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
}

/// The real transport.
#[derive(Debug)]
pub(crate) struct HttpRest {
    client: HttpClient,
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
            instance.auth,
            cfg.username.as_deref(),
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
    /// The answer is dropped: TeamCity replies with the queued `Build`, and
    /// nothing in M2 has anywhere to put it -- `Source::write` answers `()`.
    /// What matters is that the request was accepted, which `knobas-http` has
    /// already decided by the time this returns.
    ///
    /// # Errors
    ///
    /// The [`SourceError`] the request maps to.
    pub(crate) async fn queue_build(&self, build_type_id: &str) -> Result<(), SourceError> {
        let request = self
            .client
            .request(knobas_http::Method::POST, "app/rest/buildQueue")
            .json(&serde_json::json!({ "buildType": { "id": build_type_id } }));
        self.client.send(request).await.map(|_| ())
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
    /// fake that constructs `more` itself, and `tests/mockd.rs` reads
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
        // TeamCity exposes no token expiry over REST.
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
