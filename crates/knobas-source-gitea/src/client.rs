//! The endpoints M1 reads, and the only module in this crate that speaks HTTP.
//!
//! Everything else is pure -- the key grammar, the cursor, the mapping -- and
//! the run is written against this one type, so the interesting logic (paging
//! and cursor discipline) needs no socket to be tested.
//!
//! # What lives here and what does not
//!
//! The transport is [`knobas_http`]'s, and that crate is **read-only for M1**
//! (interfaces §8 P8): rustls with the platform's root store, the retry budget,
//! `Retry-After`, the per-instance rate limiter, the `User-Agent`, and the one
//! fault mapping every adapter must agree on (401 **and** 403 →
//! [`SourceError::Unauthorized`]; connect/DNS/TLS/timeout →
//! [`SourceError::Unreachable`]; everything else → [`SourceError::Protocol`]).
//! [`knobas_http::HttpClient::send`] is the only way onto the wire -- the
//! [`knobas_http::Request`] the builder hands back deliberately has no `send`
//! of its own.
//!
//! Confining every mention of `knobas-http` to this one file is on purpose: a
//! change requested from the orchestrator then costs one file to apply, not a
//! sweep across the crate.
//!
//! List methods hand back `Vec<serde_json::Value>` rather than typed records:
//! the raw element *is* what `SyncItem::payload` must carry verbatim (spec
//! §3a), so it is kept and the typed view in [`crate::model`] is projected from
//! it. A field Gitea adds tomorrow lands in the payload either way.

use chrono::{DateTime, SecondsFormat, Utc};
use knobas_http::{Auth, HttpClient, HttpConfig, Method};
use knobas_source::{AuthMethod, SourceError};
use serde_json::Value;

use crate::config::GiteaConfig;

/// Interfaces §4.1: "Page sizes: … Gitea 50". Also Gitea's own default cap.
pub(crate) const PAGE_SIZE: u32 = 50;

/// Everything under `/api/v1`.
const API_ROOT: &str = "/api/v1";

/// Connect timeout, interfaces §4.1.
const CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
/// Whole-request timeout, interfaces §4.1.
const REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// The [`Auth`] the configured method means.
///
/// # Errors
///
/// [`SourceError::Unauthorized`] when the keychain holds no secret -- the
/// `missing_secret` state of interfaces §3, which the sources view offers
/// *Re-enter* for. [`SourceError::Protocol`] for a source configured with a
/// method this adapter does not speak, or with none at all: the descriptor
/// offers exactly [`AuthMethod::Pat`], and an anonymous Gitea would sync
/// whatever happens to be public rather than what the user asked for.
pub(crate) fn credential(
    auth: Option<AuthMethod>,
    secret: Option<&str>,
) -> Result<Auth, SourceError> {
    // The configuration is judged **before** the secret, and the order is the
    // point: `missing_secret` sends the user to *Re-enter*, a misconfigured
    // source sends them to the source's settings (§3, §10.2). Retyping a token
    // cannot fix a source with no auth method chosen.
    match auth {
        Some(AuthMethod::Pat) => {}
        Some(other) => {
            return Err(SourceError::Protocol(format!(
                "gitea: {other:?} authentication is not supported; this adapter authenticates \
                 with a personal access token"
            )));
        }
        None => {
            return Err(SourceError::Protocol(
                "gitea: this source has no authentication method configured; choose a personal \
                 access token"
                    .to_owned(),
            ));
        }
    }
    let secret = secret.ok_or(SourceError::Unauthorized)?;
    // Gitea: "API tokens must be prepended with `token` followed by a space"
    // (interfaces §4.2). The header spelling itself is `knobas-http`'s and is
    // pinned there; what this crate owns is picking the variant.
    Ok(Auth::GiteaToken(secret.to_owned()))
}

/// The HTTP status behind a failure, when the failure carries one.
///
/// `knobas-http` builds a protocol message as `HTTP <status>: <body excerpt>`
/// and hands back no structured status, so this reads it back out of the
/// message it produced. Two decisions in this crate need it and cannot be
/// taken without it: a 409 from `/commits` means "this repository has no
/// commits yet", and a 404 on one repository means "skip it", neither of which
/// is the same as the generic protocol failure they arrive as.
///
/// A structured status on `SourceError` would be better and is worth
/// requesting; `knobas-http` is read-only for M1 (P8), so this is the local
/// half. It is pinned by a test that builds its input with
/// [`knobas_http::status_error`] rather than with a hand-written string, so
/// the day that format changes this crate fails its own suite instead of
/// silently reclassifying every 409 and 404.
///
/// 401 and 403 deliberately have no status here: `knobas-http` maps both to
/// [`SourceError::Unauthorized`], which carries nothing. That is what
/// [`is_repo_scoped`] exists to handle.
pub(crate) fn status_of(error: &SourceError) -> Option<u16> {
    let SourceError::Protocol(message) = error else {
        return None;
    };
    message
        .strip_prefix("HTTP ")?
        .split([' ', ':'])
        .next()?
        .parse()
        .ok()
}

/// Whether a failure is about **one repository** rather than about the source.
///
/// Ruling B4: a 403 or 404 on a single repository skips that repository; every
/// repository refusing us is a fact about the token's scope and raises. The
/// run's identity preflight (`GET /user`) has already proved the credential
/// itself works, which is what makes that reading available at all.
///
/// 403 arrives as a bare [`SourceError::Unauthorized`] because `knobas-http`
/// maps 401 and 403 alike, so both are read here as repository-scoped. A
/// credential revoked *mid-run* therefore looks like every remaining
/// repository refusing us -- which raises, since none of them can be walked.
pub(crate) fn is_repo_scoped(error: &SourceError) -> bool {
    match error {
        SourceError::Unauthorized => true,
        // Never a sink failure and never a connectivity failure: those are
        // about the run, not about one repository.
        other => status_of(other) == Some(404),
    }
}

/// One configured Gitea instance's API access.
#[derive(Debug)]
pub(crate) struct GiteaClient {
    http: HttpClient,
}

impl GiteaClient {
    /// Build the client for one configured source.
    ///
    /// # Errors
    ///
    /// [`SourceError::Protocol`] if the base URL is not an http(s) URL or the
    /// client cannot be built -- both are configuration failures, and both
    /// surface when the source is saved rather than mid-sync.
    pub(crate) fn new(
        base_url: &str,
        config: &GiteaConfig,
        auth: Auth,
    ) -> Result<Self, SourceError> {
        let http = HttpClient::new(HttpConfig {
            base_url: base_url.trim().to_owned(),
            adapter_kind: crate::ADAPTER_KIND.to_owned(),
            adapter_version: crate::ADAPTER_VERSION.to_owned(),
            auth,
            requests_per_second: config.rate_limit_per_sec,
            burst: config.rate_burst(),
            connect_timeout: CONNECT_TIMEOUT,
            request_timeout: REQUEST_TIMEOUT,
        })?;
        Ok(Self { http })
    }

    /// `GET /api/v1/user` -- the authenticated account.
    pub(crate) async fn current_user(&self) -> Result<crate::model::UserRef, SourceError> {
        self.get_json("/user", &[]).await
    }

    /// `GET /api/v1/version` -- the server version, for the Add-source report.
    pub(crate) async fn version(&self) -> Result<Option<String>, SourceError> {
        let body: Value = self.get_json("/version", &[]).await?;
        Ok(body
            .get("version")
            .and_then(Value::as_str)
            .map(str::to_owned))
    }

    /// `GET /api/v1/repos/search` -- every repository the token can see.
    pub(crate) async fn search_repos(&self, page: u32) -> Result<Vec<Value>, SourceError> {
        #[derive(serde::Deserialize)]
        struct Envelope {
            #[serde(default)]
            data: Option<Vec<Value>>,
        }
        let (limit, page) = page_params(page);
        // The only endpoint here wrapped in `{ ok, data }`.
        let body: Envelope = self
            .get_json(
                "/repos/search",
                &[
                    ("sort", "updated".to_owned()),
                    ("order", "desc".to_owned()),
                    limit,
                    page,
                ],
            )
            .await?;
        body.data.ok_or_else(|| {
            SourceError::Protocol("gitea: /repos/search returned no data array".to_owned())
        })
    }

    pub(crate) async fn get_repo(&self, owner: &str, repo: &str) -> Result<Value, SourceError> {
        self.get_json(
            &format!("/repos/{}/{}", segment(owner)?, segment(repo)?),
            &[],
        )
        .await
    }

    pub(crate) async fn branches(
        &self,
        owner: &str,
        repo: &str,
        page: u32,
    ) -> Result<Vec<Value>, SourceError> {
        let (limit, page) = page_params(page);
        self.get_json(
            &format!("/repos/{}/{}/branches", segment(owner)?, segment(repo)?),
            &[limit, page],
        )
        .await
    }

    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "wired in task 6: pull requests and their discussion"
        )
    )]
    pub(crate) async fn pulls(
        &self,
        owner: &str,
        repo: &str,
        page: u32,
    ) -> Result<Vec<Value>, SourceError> {
        let (limit, page) = page_params(page);
        self.get_json(
            &format!("/repos/{}/{}/pulls", segment(owner)?, segment(repo)?),
            // `state` defaults to `open`; a merged pull request is exactly the
            // one the ticket-to-PR story ends on, so `all` is required.
            &[
                ("state", "all".to_owned()),
                ("sort", "recentupdate".to_owned()),
                limit,
                page,
            ],
        )
        .await
    }

    /// Pull-request discussion. Gitea keeps it on the issue with the same index.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "wired in task 6: pull requests and their discussion"
        )
    )]
    pub(crate) async fn issue_comments(
        &self,
        owner: &str,
        repo: &str,
        index: u64,
    ) -> Result<Vec<Value>, SourceError> {
        self.get_json(
            &format!(
                "/repos/{}/{}/issues/{index}/comments",
                segment(owner)?,
                segment(repo)?
            ),
            &[],
        )
        .await
    }

    /// `GET /repos/{o}/{r}/commits`. `stat`, `verification` and `files` all
    /// default to **true** in Gitea; turning them off is the difference between
    /// a listing and a diff dump.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "wired in task 7: incremental commits per moved branch"
        )
    )]
    pub(crate) async fn commits(
        &self,
        owner: &str,
        repo: &str,
        branch: &str,
        since: Option<DateTime<Utc>>,
        page: u32,
    ) -> Result<Vec<Value>, SourceError> {
        let (limit, page) = page_params(page);
        let mut query = vec![
            ("sha", branch.to_owned()),
            ("stat", "false".to_owned()),
            ("verification", "false".to_owned()),
            ("files", "false".to_owned()),
            limit,
            page,
        ];
        if let Some(since) = since {
            query.push(("since", since.to_rfc3339_opts(SecondsFormat::Secs, true)));
        }
        let path = format!("/repos/{}/{}/commits", segment(owner)?, segment(repo)?);
        match self.get_json(&path, &query).await {
            // 409 is Gitea's "this repository has no commits yet". A freshly
            // created repository is not a failure.
            Err(error) if status_of(&error) == Some(409) => Ok(Vec::new()),
            other => other,
        }
    }

    /// `GET <base>/api/v1/<path>?<query>`, decoded as `T`.
    ///
    /// The query values are owned because every one of them is computed;
    /// borrowing would only move the temporaries to the call sites.
    async fn get_json<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        query: &[(&str, String)],
    ) -> Result<T, SourceError> {
        let path = format!("{API_ROOT}{path}");
        let request = self.http.request(Method::GET, &path).query(query);
        let response = self.http.send(request).await?;
        response.json::<T>().await.map_err(|error| {
            SourceError::Protocol(format!(
                "gitea: {path} answered an unreadable body: {error}"
            ))
        })
    }
}

/// `("limit", "50"), ("page", "<n>")`.
fn page_params(page: u32) -> ((&'static str, String), (&'static str, String)) {
    (("limit", PAGE_SIZE.to_string()), ("page", page.to_string()))
}

/// One path segment, refused unless it is a name Gitea could have issued.
///
/// Owner and repository names reach here from configuration as well as from the
/// API, and a `..` in either would walk out of `/api/v1`.
fn segment(value: &str) -> Result<&str, SourceError> {
    let ok = !value.is_empty()
        && value.len() <= 100
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        && value != "."
        && value != "..";
    ok.then_some(value).ok_or_else(|| {
        SourceError::Protocol(format!(
            "gitea: {value:?} is not a usable owner or repository name"
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use knobas_http::StatusCode;

    const SECRET: &str = "tidewater-pat-not-in-any-log";

    /// Gitea's own spelling of the header is `Authorization: token <pat>`
    /// (interfaces §4.2), which `knobas-http` renders for
    /// [`Auth::GiteaToken`]. What this crate owns is picking that variant.
    #[test]
    fn a_pat_becomes_a_gitea_token_credential() {
        let got = credential(Some(AuthMethod::Pat), Some(SECRET)).unwrap();
        assert!(
            matches!(got, Auth::GiteaToken(ref token) if token == SECRET),
            "{got:?}"
        );
    }

    /// Interfaces §3 `missing_secret`: a configured source whose keychain entry
    /// is gone reads as unauthorized, which is what puts *Re-enter* on screen.
    #[test]
    fn a_missing_secret_is_unauthorized() {
        assert!(matches!(
            credential(Some(AuthMethod::Pat), None),
            Err(SourceError::Unauthorized)
        ));
    }

    /// A source that could not authenticate even with a secret in hand has a
    /// configuration problem, and reporting `Unauthorized` for it would send
    /// the user to retype a token that was never the issue.
    #[test]
    fn a_configuration_problem_outranks_a_missing_secret() {
        for method in [
            None,
            Some(AuthMethod::UserPassword),
            Some(AuthMethod::OAuth),
            Some(AuthMethod::ApiToken),
        ] {
            for secret in [None, Some(SECRET)] {
                let error = credential(method, secret).unwrap_err();
                assert!(
                    matches!(error, SourceError::Protocol(ref m) if m.contains("token")),
                    "{method:?}/{}: {error:?}",
                    secret.is_some()
                );
            }
        }
    }

    /// Spec §14: the secret exists in one place. `Debug` is the leak that gets
    /// forgotten -- a `dbg!` on the client would otherwise put a PAT in a log.
    #[test]
    fn debug_never_prints_the_secret() {
        let auth = credential(Some(AuthMethod::Pat), Some(SECRET)).unwrap();
        assert!(!format!("{auth:?}").contains(SECRET));
        let client = GiteaClient::new(
            "https://gitea.tidewater.example",
            &GiteaConfig::default(),
            auth,
        )
        .unwrap();
        let shown = format!("{client:?}");
        assert!(!shown.contains(SECRET), "{shown}");
        assert!(shown.contains("gitea.tidewater.example"), "{shown}");
    }

    /// A base URL the user mistyped must fail when the source is saved, not
    /// mid-sync, and it must name the string it could not read.
    #[test]
    fn a_base_url_that_is_not_a_url_is_refused() {
        let error = GiteaClient::new(
            "gitea.example.com",
            &GiteaConfig::default(),
            Auth::GiteaToken(SECRET.to_owned()),
        )
        .unwrap_err();
        assert!(
            matches!(error, SourceError::Protocol(ref m) if m.contains("gitea.example.com")),
            "{error:?}"
        );
    }

    /// The coupling that makes 409-is-empty and 404-is-a-skip work.
    ///
    /// The input is built with `knobas_http::status_error` -- the function that
    /// actually produces these errors -- rather than with a hand-written
    /// string, so this fails the day that message format changes instead of
    /// this crate silently treating every 409 as a hard failure and every
    /// missing repository as a fatal one.
    #[test]
    fn the_status_behind_a_protocol_failure_is_recoverable() {
        for status in [
            StatusCode::NOT_FOUND,
            StatusCode::CONFLICT,
            StatusCode::BAD_REQUEST,
            StatusCode::INTERNAL_SERVER_ERROR,
        ] {
            let error = knobas_http::status_error(status, "{\"message\":\"nope\"}");
            assert_eq!(
                status_of(&error),
                Some(status.as_u16()),
                "{status} -> {error:?}"
            );
        }
        // 401 and 403 are `Unauthorized`, which carries no status at all.
        for status in [StatusCode::UNAUTHORIZED, StatusCode::FORBIDDEN] {
            assert_eq!(
                status_of(&knobas_http::status_error(status, "")),
                None,
                "{status}"
            );
        }
        // Nothing else carries a status either, and none of them may be read
        // as one: a sink failure that parsed as a 404 would be swallowed as a
        // skipped repository.
        for other in [
            SourceError::Unreachable("connection refused".to_owned()),
            SourceError::Sink("pool closed".to_owned()),
            SourceError::Protocol("gitea: /user answered an unreadable body".to_owned()),
        ] {
            assert_eq!(status_of(&other), None, "{other:?}");
        }
    }

    /// Ruling B4: 403 and 404 are about one repository; everything else is
    /// about the run and must abort it.
    #[test]
    fn only_a_refusal_or_an_absence_is_scoped_to_one_repository() {
        assert!(is_repo_scoped(&SourceError::Unauthorized));
        assert!(is_repo_scoped(&knobas_http::status_error(
            StatusCode::NOT_FOUND,
            ""
        )));
        for other in [
            knobas_http::status_error(StatusCode::CONFLICT, ""),
            knobas_http::status_error(StatusCode::INTERNAL_SERVER_ERROR, ""),
            SourceError::Unreachable("connection refused".to_owned()),
            SourceError::Sink("pool closed".to_owned()),
        ] {
            assert!(!is_repo_scoped(&other), "{other:?}");
        }
    }

    /// Owner and repository names reach the path builder from configuration as
    /// well as from the API.
    #[test]
    fn a_path_segment_cannot_walk_out_of_the_api_root() {
        for good in ["tidewater", "payout-service", "docs.v2", "a_b", "A1"] {
            assert_eq!(segment(good).unwrap(), good);
        }
        for bad in [
            "",
            "..",
            ".",
            "../../admin",
            "a/b",
            "a?b",
            "a b",
            "a#b",
            "%2e%2e",
        ] {
            assert!(segment(bad).is_err(), "{bad:?} must be refused");
        }
        assert!(segment(&"a".repeat(101)).is_err());
    }

    /// Interfaces §4.1 fixes the page size at 50; the limit and the page number
    /// travel together so a call cannot send one without the other.
    #[test]
    fn paging_parameters_carry_the_contracts_page_size() {
        assert_eq!(PAGE_SIZE, 50);
        let (limit, page) = page_params(3);
        assert_eq!(limit, ("limit", "50".to_owned()));
        assert_eq!(page, ("page", "3".to_owned()));
    }
}

/// What each endpoint actually puts on the wire.
///
/// These pin the four query facts this stream verified against Gitea's
/// published OpenAPI document (1.27) and would otherwise only find out about
/// from the real container: `stat`/`verification`/`files` default to **true**,
/// `state` defaults to `open`, `sort=recentupdate` is the spelling, and
/// `/repos/search` is the one `{ok,data}`-wrapped answer. Each mock *requires*
/// the parameters, so dropping one makes the request miss and the test fail --
/// which is the only way an omitted default is visible without a live server.
#[cfg(test)]
mod wire_tests {
    use super::*;
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    async fn client_for(server: &MockServer) -> GiteaClient {
        GiteaClient::new(
            &server.uri(),
            &GiteaConfig::default(),
            Auth::GiteaToken("tidewater-pat".to_owned()),
        )
        .expect("the client builds")
    }

    /// `stat`, `verification` and `files` default to **true** in Gitea, which
    /// turns a commit listing into a diff dump. The mock demands all three, so
    /// dropping any one of them fails this test.
    #[tokio::test]
    async fn a_commit_listing_turns_off_the_projections_gitea_defaults_to_on() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/repos/tidewater/payout-service/commits"))
            .and(query_param("sha", "main"))
            .and(query_param("stat", "false"))
            .and(query_param("verification", "false"))
            .and(query_param("files", "false"))
            .and(query_param("limit", "50"))
            .and(query_param("page", "1"))
            .and(query_param("since", "2026-08-22T11:42:00Z"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                { "sha": "c90d11" }
            ])))
            .mount(&server)
            .await;
        let since = DateTime::parse_from_rfc3339("2026-08-22T11:42:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let got = client_for(&server)
            .await
            .commits("tidewater", "payout-service", "main", Some(since), 1)
            .await
            .expect("the mock answers only the fully-parameterised request");
        assert_eq!(got.len(), 1);
        assert_eq!(got[0]["sha"], "c90d11");
    }

    /// A run with no watermark yet must not send `since=` at all: an absent
    /// parameter is "from the beginning", and sending an empty one is a 400.
    #[tokio::test]
    async fn a_first_commit_listing_sends_no_since_at_all() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/repos/tidewater/payout-service/commits"))
            .and(|request: &wiremock::Request| {
                !request.url.query_pairs().any(|(key, _)| key == "since")
            })
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(&server)
            .await;
        client_for(&server)
            .await
            .commits("tidewater", "payout-service", "main", None, 1)
            .await
            .expect("no watermark means no since parameter");
    }

    /// `state` defaults to `open`, and a merged pull request is exactly the one
    /// the ticket-to-PR story ends on.
    #[tokio::test]
    async fn a_pull_listing_asks_for_every_state_most_recently_updated_first() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/repos/tidewater/payout-service/pulls"))
            .and(query_param("state", "all"))
            .and(query_param("sort", "recentupdate"))
            .and(query_param("limit", "50"))
            .and(query_param("page", "2"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                { "number": 142 }
            ])))
            .mount(&server)
            .await;
        let got = client_for(&server)
            .await
            .pulls("tidewater", "payout-service", 2)
            .await
            .expect("the mock answers only the fully-parameterised request");
        assert_eq!(got[0]["number"], 142);
    }

    /// The only endpoint here wrapped in `{ ok, data }`. Unwrapping it wrongly
    /// would leave the run with no repositories and no error.
    #[tokio::test]
    async fn the_repository_search_envelope_is_unwrapped() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/repos/search"))
            .and(query_param("sort", "updated"))
            .and(query_param("order", "desc"))
            .and(query_param("limit", "50"))
            .and(query_param("page", "1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "ok": true,
                "data": [{ "full_name": "tidewater/payout-service" }]
            })))
            .mount(&server)
            .await;
        let got = client_for(&server)
            .await
            .search_repos(1)
            .await
            .expect("the envelope is unwrapped");
        assert_eq!(got.len(), 1);
        assert_eq!(got[0]["full_name"], "tidewater/payout-service");
    }

    /// A body without the envelope is a protocol failure rather than "no
    /// repositories": silently syncing nothing is how a whole source quietly
    /// disappears from the mirror.
    #[tokio::test]
    async fn a_repository_search_without_a_data_array_is_a_protocol_failure() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/repos/search"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({ "ok": false })),
            )
            .mount(&server)
            .await;
        let error = client_for(&server).await.search_repos(1).await.unwrap_err();
        assert!(
            matches!(error, SourceError::Protocol(ref m) if m.contains("data array")),
            "{error:?}"
        );
    }

    /// 409 is Gitea's "this repository has no commits yet". A freshly created
    /// repository is not a failed sync.
    #[tokio::test]
    async fn an_empty_repository_answers_no_commits_rather_than_failing() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/repos/tidewater/fresh/commits"))
            .respond_with(
                ResponseTemplate::new(409)
                    .set_body_json(serde_json::json!({ "message": "Git Repository is empty." })),
            )
            .mount(&server)
            .await;
        let got = client_for(&server)
            .await
            .commits("tidewater", "fresh", "main", None, 1)
            .await
            .expect("an empty repository is not a failure");
        assert!(got.is_empty());
    }

    /// Gitea keeps a pull request's discussion on the **issue** with the same
    /// index -- there is no `/pulls/{n}/comments`. Ruling B1 grants this fifth
    /// read endpoint; the path is the part nobody guesses right.
    #[tokio::test]
    async fn pull_request_discussion_is_read_from_the_issue_with_the_same_index() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(
                "/api/v1/repos/tidewater/payout-service/issues/142/comments",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                { "id": 1, "body": "Should the jitter be bounded?" }
            ])))
            .mount(&server)
            .await;
        let got = client_for(&server)
            .await
            .issue_comments("tidewater", "payout-service", 142)
            .await
            .expect("the discussion is on the issue path");
        assert_eq!(got.len(), 1);
        assert_eq!(got[0]["body"], "Should the jitter be bounded?");
    }

    /// Every other status still fails: only 409 is forgiven, and only there.
    #[tokio::test]
    async fn a_broken_commit_listing_still_fails() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/repos/tidewater/broken/commits"))
            .respond_with(ResponseTemplate::new(500).set_body_string("boom"))
            .mount(&server)
            .await;
        let error = client_for(&server)
            .await
            .commits("tidewater", "broken", "main", None, 1)
            .await
            .unwrap_err();
        assert!(matches!(error, SourceError::Protocol(_)), "{error:?}");
    }
}
