//! The endpoints M1 reads, and the only module in this crate that speaks HTTP.
//!
//! Everything else is pure -- the key grammar, the cursor, the mapping -- and
//! the run is written against this one type, so the interesting logic (paging
//! and cursor discipline) needs no socket to be tested.
//!
//! # What lives here and what does not
//!
//! The transport is [`knobas_http`]'s, and that crate changes only through the
//! orchestrator (interfaces §8 P8, §10.8): rustls with the platform's root
//! store, the retry budget, `Retry-After`, the per-instance rate limiter, the
//! `User-Agent`, and the one fault mapping every adapter must agree on (401
//! **and** 403 → [`SourceError::Unauthorized`], except a 403 this adapter's
//! [`forbidden_refusal`] reads as a refusal of the request (#722);
//! connect/DNS/TLS/timeout →
//! [`SourceError::Unreachable`]; everything else → [`SourceError::Protocol`]),
//! each carrying the status it came from ([`SourceError::status`], ADR-0004) --
//! which is what lets this crate tell a dead credential from a repository it
//! may not read. This adapter's own reading of a failing body is
//! [`error_message`], handed over as a [`knobas_http::BodyMessage`], and its
//! credential is the [`CredentialRules`] the transport's [`credential`] maps
//! the configured method under.
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
use knobas_http::{Auth, CredentialRules, HttpClient, HttpConfig, Method, TokenSpelling};
use knobas_source::{AuthMethod, SourceError};
use serde_json::Value;

use crate::config::GiteaConfig;

mod issue;
pub(crate) use issue::issues_path;
mod pull;
mod repo;

/// How many records a listing request asks for. Interfaces §4.1: "Page sizes:
/// … Gitea 50", which is also Gitea's own default cap.
///
/// **A request, not a guarantee -- and nothing may end a walk on it.** What the
/// server actually puts on a page is the server's to decide: `MAX_RESPONSE_ITEMS`
/// is configurable on a self-hosted instance, an endpoint may carry its own
/// maximum, and a loaded one may answer partially. So a page shorter than this
/// means "the collection ran out" *or* "we were capped", with nothing in the
/// answer to say which. No walk in `sync.rs` ends a *listing* on a short page:
/// it ends one on an **empty** page (`knobas_http::PageWalk`), and the two budgeted
/// walks stop on their own budget or watermark before they get that far. Nor
/// does `write.rs`'s 409 readback of open pull requests (#1625).
/// Reading a short page as the end is a run that reports success while its
/// watermark advances past records it never saw. Issue #81, ruled 2026-08-29.
/// `sync.rs` no longer **reads** this constant -- it does not import it, and
/// names it only in prose -- which is the shape that keeps the assumption from
/// growing back.
pub(crate) const PAGE_SIZE: u32 = 50;

/// Everything under `/api/v1`.
const API_ROOT: &str = "/api/v1";

/// Gitea's own count of the collection a response is part of, which it sets on
/// listing endpoints (`ctx.SetTotalCountHeader`). Read by
/// [`GiteaClient::issue_comments`], because that is the one read whose
/// completeness nothing else can check: the four paged walks ask for the next
/// page and find out. And by `GiteaClient::pull_commits` (#662), whose walk
/// stops on a page budget and must say how many commits that left out.
const TOTAL_COUNT: &str = "x-total-count";

/// One pull request's discussion, and what the server said it consists of.
///
/// The pair travels together because apart they are two facts about different
/// requests. [`crate::sync::fetch_comments`] compares them; see
/// [`GiteaClient::issue_comments`] for why that comparison is the only
/// completeness signal this endpoint offers.
pub(crate) struct Discussion {
    /// The comment records, verbatim, as [`crate::model::Comment`] is projected
    /// from.
    pub(crate) raw: Vec<Value>,
    /// `X-Total-Count`, when the server sent a readable one. `None` degrades to
    /// believing what arrived, which is what this adapter did before it asked.
    pub(crate) total: Option<usize>,
}

/// Connect timeout, interfaces §4.1.
const CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
/// Whole-request timeout, interfaces §4.1.
const REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

pub(crate) use knobas_http::credential;

/// Which [`Auth`] each configured method means for Gitea, and what a source
/// that cannot authenticate reads as. The mapping itself, and the order it
/// judges in, are [`credential`]'s.
///
/// The descriptor offers exactly [`AuthMethod::Pat`]: an anonymous Gitea
/// would sync whatever happens to be public rather than what the user asked
/// for.
pub(crate) const CREDENTIAL_RULES: CredentialRules = CredentialRules {
    // Gitea: "API tokens must be prepended with `token` followed by a space"
    // (interfaces §4.2). Forgejo keeps the spelling.
    pat: TokenSpelling::Token,
    // What `descriptor_template().auth_methods` declares and the Add-source
    // form offers; anything else is refused rather than guessed at.
    methods: &[AuthMethod::Pat],
    unconfigured: "gitea: this source has no authentication method configured; choose a \
                   personal access token",
    unsupported: |method| {
        format!(
            "gitea: {method:?} authentication is not supported; this adapter authenticates \
             with a personal access token"
        )
    },
};

/// The sentence inside Gitea's error envelope, for `knobas-http` to build the
/// message out of ([`knobas_http::BodyMessage`], ADR-0004).
///
/// A failing Gitea answers `{"message": "…", "url": "…"}`, the `url` being a
/// link to its own API documentation and of no use to anyone reading a sync
/// failure. `None` for anything else, which keeps the raw excerpt `knobas-http`
/// would have built -- a body this could not read is still the most informative
/// thing knobas has.
fn error_message(body: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(body.trim()).ok()?;
    let message = value.get("message")?.as_str()?.trim();
    (!message.is_empty()).then(|| message.to_owned())
}

/// The one 403 Gitea answers about the **token** rather than the request: a
/// token lacking the scope the endpoint needs (`token does not have at least
/// one of required scope(s), required=[…], token scope=…`, measured on 1.27.3,
/// 2026-10-13). A new token with that scope is what fixes it.
const SCOPE_REFUSAL: &str = "token does not have at least one of required scope(s)";

/// How Gitea's 403 is read ([`knobas_http::ForbiddenReading`], issue #722):
/// **a refusal of the request, carrying Gitea's sentence**, unless it is the
/// scope refusal.
///
/// Gitea refuses a bad token with **401** (`invalid username, password or
/// token`), never 403, and answers 403 for things no token can fix: a write to
/// a mirror (`Git Repository is a mirror.`), a delete of a protected branch
/// (`branch protected`) or of the default one (`can not delete default or pull
/// request target branch`), each measured on 1.27.3 and witnessed by
/// `tests/live_gitea.rs`. Read as `Unauthorized`, such a write waited for a new
/// token that would have been refused the same way, and the source was shown
/// needing one. So the exception is the one 403 about the token, not a list of
/// refusals: a refusal Gitea adds tomorrow is read right without a release.
///
/// A body that is not Gitea's envelope -- a proxy's page -- is not read at
/// all: what it refuses is unknown, and the token is the conservative guess
/// (a needless wait, never a dropped write).
fn forbidden_refusal(body: &str) -> Option<String> {
    let message = error_message(body)?;
    (!message.contains(SCOPE_REFUSAL)).then_some(message)
}

/// Whether a failure is about **one repository** rather than about the source.
///
/// Ruling B4: a 403 or 404 on a single repository skips that repository; every
/// repository refusing us is a fact about the token or its user and raises --
/// as `Unauthorized` for Gitea's scope sentence, as a refusal carrying Gitea's
/// sentence for any other 403 ([`forbidden_refusal`], #722). The
/// run's identity preflight (`GET /user`) has already proved the credential
/// itself works, which is what makes that reading available at all.
///
/// **Amended by #728:** a repository from the run's own listing whose read
/// answers 404, and whose `GET /repos/{owner}/{repo}` then answers 404 as
/// well, was deleted since the listing and is skipped even on a full sync; a
/// repository the user configured keeps B4 as written. The sync module docs
/// carry the reasoning.
///
/// **A 401 is not repository-scoped, and never was**: a credential the server
/// rejected is a fact about the run, not about the object it was asked for.
/// Until ADR-0004 this could not be said here -- `knobas-http` mapped 401 and
/// 403 onto one indistinguishable [`SourceError::Unauthorized`], so both had to
/// be read as repository-scoped and `sync::credential_still_good` re-ran the
/// identity probe to recover the difference by measuring. The status is carried
/// now, so task 5's original classification -- 403/404 skip, 401 fatal -- is
/// simply what this reads.
///
/// A failure carrying no status at all is not repository-scoped either, and
/// that is the same rule rather than a second one: a sink failure, a DNS blip,
/// a keychain entry that is gone and a body that would not decode are all about
/// the run.
pub(crate) fn is_repo_scoped(error: &SourceError) -> bool {
    matches!(error.status(), Some(403 | 404))
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
            body_message: Some(error_message),
            forbidden: Some(forbidden_refusal),
            max_body: knobas_http::MAX_BODY,
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
    ///
    /// **Paged over `id` ascending, a key no push can move (#1624).** This
    /// was `sort=updated&order=desc`, and a push into a repository not yet
    /// read moved it onto a page already read: the run walked another one
    /// twice, this one not at all, returned `Ok`, and handed the sweep a
    /// corpus without it. An id never changes and a new repository takes a
    /// higher one, so a push moves nothing. Two things still move a row
    /// across a page boundary. A repository appearing below the walk (shared
    /// with the token, say) repeats a row, and `sync::select_repos` walks a
    /// repeated row once. A repository deleted, or unshared, from a page
    /// already read hides the first row of the next page (#1979), and
    /// `sync::select_repos` lists twice on a full sync for it, since this
    /// endpoint offers no keyset. Gitea 1.27.3 offers `id`
    /// (`swagger.v1.json`, `repoSearch`; spec #1618 ruling A1 asked for it
    /// where offered), and `live_gitea` asserts the order it answers.
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
                    ("sort", "id".to_owned()),
                    ("order", "asc".to_owned()),
                    limit,
                    page,
                ],
            )
            .await?;
        body.data.ok_or_else(|| {
            SourceError::protocol("gitea: /repos/search returned no data array".to_owned())
        })
    }

    pub(crate) async fn get_repo(&self, owner: &str, repo: &str) -> Result<Value, SourceError> {
        self.get_json(
            &format!("/repos/{}/{}", segment(owner)?, segment(repo)?),
            &[],
        )
        .await
    }

    /// `GET /repos/{owner}/{repo}/teams` -- the organization teams that hold
    /// the repository (#966), read whole: Gitea does not page it.
    ///
    /// `Some(vec![])` where Gitea says the repository has no teams to have:
    /// a user's repository answers `405 repo is not owned by an
    /// organization` (1.27.3, measured 2026-10-17). `None` where it would not
    /// say: a 403 (a token without organization scope) or a 404. Either costs
    /// the teams and nothing more, so neither ends the walk; anything else
    /// is the failure it is.
    pub(crate) async fn repo_teams(
        &self,
        owner: &str,
        repo: &str,
    ) -> Result<Option<Vec<Value>>, SourceError> {
        let teams: Result<Option<Vec<Value>>, SourceError> = self
            .get_json(
                &format!("/repos/{}/{}/teams", segment(owner)?, segment(repo)?),
                &[],
            )
            .await;
        match teams {
            Ok(teams) => Ok(Some(teams.unwrap_or_default())),
            Err(error) if error.status() == Some(405) => Ok(Some(Vec::new())),
            Err(error) if matches!(error.status(), Some(403 | 404)) => Ok(None),
            Err(error) => Err(error),
        }
    }

    pub(crate) async fn branches(
        &self,
        owner: &str,
        repo: &str,
        page: u32,
    ) -> Result<(Vec<Value>, Option<usize>), SourceError> {
        let (limit, page) = page_params(page);
        // An empty repository's listing is `200 null`, not `[]` (Gitea
        // 1.27.3, measured 2026-10-12): no branches, which is an empty page.
        // The count beside it is `X-Total-Count`, the whole listing's size
        // when this page was read (61 on a 61-branch page 1, 1.27.3, #1716).
        let (batch, total): (Option<Vec<Value>>, _) = self
            .get_json_counted(
                &format!("/repos/{}/{}/branches", segment(owner)?, segment(repo)?),
                &[limit, page],
            )
            .await?;
        Ok((batch.unwrap_or_default(), total))
    }

    /// `GET /repos/{owner}/{repo}/git/refs/heads` -- the name of every branch,
    /// in one answer (#1716).
    ///
    /// Read from git rather than the branch table [`Self::branches`] pages
    /// over, so it is one snapshot that a push or a deletion cannot reorder
    /// between two pages. Measured on 1.27.3 (2026-10-22): all 60 of a
    /// 60-branch repository in one array, `limit`/`page` ignored and no
    /// `Link`; an array for a single branch too; `404` where the repository
    /// has no branch at all, which is an empty list here.
    pub(crate) async fn branch_names(
        &self,
        owner: &str,
        repo: &str,
    ) -> Result<Vec<String>, SourceError> {
        #[derive(serde::Deserialize)]
        struct Ref {
            #[serde(rename = "ref")]
            name: String,
        }
        let refs: Result<Vec<Ref>, SourceError> = self
            .get_json(
                &format!(
                    "/repos/{}/{}/git/refs/heads",
                    segment(owner)?,
                    segment(repo)?
                ),
                &[],
            )
            .await;
        match refs {
            Ok(refs) => Ok(refs
                .into_iter()
                .filter_map(|r| r.name.strip_prefix("refs/heads/").map(str::to_owned))
                .collect()),
            Err(error) if error.status() == Some(404) => Ok(Vec::new()),
            Err(error) => Err(error),
        }
    }

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

    /// Pull-request discussion, whole. Gitea keeps it on the issue with the
    /// same index.
    ///
    /// # Why this one request is not the truncation issue #131 expected
    ///
    /// #131 read this as a listing asked without a `limit`, and therefore
    /// answered `DEFAULT_PAGING_NUM` records -- thirty on a stock install --
    /// with nothing to say the rest existed. It is not, and the pinned
    /// container (interfaces §4.2 makes it this adapter's contract source)
    /// says so three ways, measured on Gitea **1.27.2**, 2026-08-29:
    ///
    /// * **Its own OpenAPI document declares no paging here.**
    ///   `swagger.v1.json`'s `issueGetComments` takes `since` and `before` and
    ///   nothing else. The *repository-wide* `issueGetRepoComments`
    ///   (`/repos/{o}/{r}/issues/comments`) declares `page` and `limit` -- so
    ///   the difference is the endpoint, not the instance.
    /// * **`limit` and `page` are ignored.** A discussion of 51 comments came
    ///   back whole for no query at all, for `limit=50&page=1`, for
    ///   `limit=2&page=1` and for `limit=50&page=9`. On the same server in the
    ///   same minute, the repository-wide endpoint answered `limit=2` with two
    ///   records of 104. Paging *this* one would therefore not walk a
    ///   discussion -- it would re-read the same discussion until the walk ran
    ///   out of budget, and fold every comment into `body_text` once per
    ///   request.
    /// * **`X-Total-Count` is sent, and it is the truth.** It equalled the
    ///   number of records in the body on every pull request in the fixture and
    ///   under every one of those queries -- including `#142`, where it read 2
    ///   and the pull request's own `comments` field read 3. So the record
    ///   count is a completeness signal and `PullRequest::comments` is not.
    ///
    /// So the request stays one request, sends no `limit` and no `page` --
    /// parameters this endpoint does not have -- and carries the server's own
    /// count back with it. [`crate::sync::fetch_comments`] is what refuses a
    /// discussion the server says it did not send in full: the assumption is
    /// checked on every read rather than trusted, which is what turns "a Gitea
    /// that starts paging this endpoint" from a silent corpus-wide truncation
    /// into a run that stops and names it.
    pub(crate) async fn issue_comments(
        &self,
        owner: &str,
        repo: &str,
        index: u64,
    ) -> Result<Discussion, SourceError> {
        let (raw, total) = self
            .get_json_counted(
                &format!(
                    "/repos/{}/{}/issues/{index}/comments",
                    segment(owner)?,
                    segment(repo)?
                ),
                &[],
            )
            .await?;
        Ok(Discussion { raw, total })
    }

    /// `GET /repos/{o}/{r}/commits`. `stat`, `verification` and `files` all
    /// default to **true** in Gitea; turning them off is the difference between
    /// a listing and a diff dump.
    ///
    /// `since` and `until` are both server-side and inclusive (git's
    /// `--since`/`--until` on the committer date, which is `created`);
    /// `live_gitea` asserts both. `until` bounds a catch-up's window from
    /// above (#1624), so it does not re-list the commits the walk it
    /// continues already delivered.
    pub(crate) async fn commits(
        &self,
        owner: &str,
        repo: &str,
        branch: &str,
        (since, until): (Option<DateTime<Utc>>, Option<DateTime<Utc>>),
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
        if let Some(until) = until {
            query.push(("until", until.to_rfc3339_opts(SecondsFormat::Secs, true)));
        }
        let path = format!("/repos/{}/{}/commits", segment(owner)?, segment(repo)?);
        match self.get_json(&path, &query).await {
            // 409 is Gitea's "this repository has no commits yet". A freshly
            // created repository is not a failure.
            Err(error) if error.status() == Some(409) => Ok(Vec::new()),
            other => other,
        }
    }

    /// `GET /repos/{o}/{r}/git/commits/{sha}` -- one commit, with its
    /// parents, for its source view (#661). The projections Gitea defaults to
    /// on are turned off, as for [`Self::commits`]: the view reads its diff
    /// from [`Self::commit_diff`], which is the text every git server prints.
    pub(crate) async fn commit(
        &self,
        owner: &str,
        repo: &str,
        sha: &str,
    ) -> Result<crate::model::CommitDetail, SourceError> {
        self.get_json(
            &format!(
                "/repos/{}/{}/git/commits/{}",
                segment(owner)?,
                segment(repo)?,
                object_id(sha)?
            ),
            &[
                ("stat", "false".to_owned()),
                ("verification", "false".to_owned()),
                ("files", "false".to_owned()),
            ],
        )
        .await
    }

    /// `GET /repos/{o}/{r}/git/commits/{sha}.diff` -- the commit's change as
    /// `git diff` prints it, for [`knobas_source::diff`] to parse.
    ///
    /// A refusal is the error it maps to, never an empty text: an empty diff
    /// would draw *no changes* over a commit that has some.
    pub(crate) async fn commit_diff(
        &self,
        owner: &str,
        repo: &str,
        sha: &str,
    ) -> Result<String, SourceError> {
        self.get_text(&format!(
            "/repos/{}/{}/git/commits/{}.diff",
            segment(owner)?,
            segment(repo)?,
            object_id(sha)?
        ))
        .await
    }

    /// `GET <base>/api/v1/<path>`, its body as text: a `.diff`, which is not
    /// JSON.
    async fn get_text(&self, path: &str) -> Result<String, SourceError> {
        self.get_text_with(path, &[]).await
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
        self.get_json_counted(path, query)
            .await
            .map(|(body, _)| body)
    }

    /// The same request, keeping Gitea's `X-Total-Count` alongside the body.
    ///
    /// The header exists only while the response does, and
    /// [`knobas_http::HttpClient::send`] is the one place a response is ever
    /// held -- so reading it has to happen here, between the send and the
    /// decode, or not at all. Nothing in `knobas-http` changes for it:
    /// `send` already hands back the whole response (§10.8's frozen crate is
    /// untouched).
    ///
    /// `None` when the header is absent or unreadable, which is the honest
    /// answer for a server that does not send one and degrades to exactly the
    /// behaviour this adapter had before it read one.
    async fn get_json_counted<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        query: &[(&str, String)],
    ) -> Result<(T, Option<usize>), SourceError> {
        let path = format!("{API_ROOT}{path}");
        let request = self.http.request(Method::GET, &path).query(query);
        let response = self.http.send(request).await?;
        let total = response
            .headers()
            .get(TOTAL_COUNT)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.trim().parse::<usize>().ok());
        let body = response.json::<T>().await.map_err(|error| {
            SourceError::protocol(format!(
                "gitea: {path} answered an unreadable body: {error}"
            ))
        })?;
        Ok((body, total))
    }
}

impl GiteaClient {
    /// `POST <base>/api/v1/<path>` with `body` as JSON, keeping the response.
    ///
    /// M2's write-back (issue #43). The response is handed back rather than
    /// decoded because Gitea's four writes answer four different records
    /// (`Branch`, `PullRequest`, `Comment`, `PullReview`) and none of the four
    /// callers needs one -- what they need is that the write landed, which
    /// `knobas-http` has already decided by the time this returns.
    ///
    /// # Errors
    ///
    /// The [`SourceError`] the failure maps to, with Gitea's own sentence in
    /// the message where it sent one ([`error_message`]).
    pub(crate) async fn post(
        &self,
        path: &str,
        body: &Value,
    ) -> Result<knobas_http::Response, SourceError> {
        let request = self
            .http
            .request(Method::POST, &format!("{API_ROOT}{path}"))
            .json(body);
        self.http.send(request).await
    }

    /// [`Self::post`], sent **once** ([`knobas_http::Request::no_retry`],
    /// #1122): for a write Gitea would accept a second time and do again -- a
    /// comment, an approval, a review. A POST that timed out may have landed,
    /// and a resend would be a second comment. The writes that keep
    /// [`Self::post`] are refused the second time by Gitea itself: a branch
    /// that exists (409), a pull request already open for the head and base
    /// (409), a merge already made (405).
    ///
    /// # Errors
    ///
    /// As [`Self::post`], and a transient failure is returned rather than
    /// retried.
    pub(crate) async fn post_once(
        &self,
        path: &str,
        body: &Value,
    ) -> Result<knobas_http::Response, SourceError> {
        let request = self
            .http
            .request(Method::POST, &format!("{API_ROOT}{path}"))
            .json(body)
            .no_retry();
        self.http.send(request).await
    }

    /// `PATCH <base>/api/v1/<path>` with `body` as JSON, keeping the response:
    /// [`Self::post`]'s twin for an edit (issue #706).
    ///
    /// # Errors
    ///
    /// As [`Self::post`].
    pub(crate) async fn patch(
        &self,
        path: &str,
        body: &Value,
    ) -> Result<knobas_http::Response, SourceError> {
        let request = self
            .http
            .request(Method::PATCH, &format!("{API_ROOT}{path}"))
            .json(body);
        self.http.send(request).await
    }

    /// One path segment of a write, refused unless it is a name Gitea could
    /// have issued -- the same guard the reads use, reachable from
    /// [`crate::write`].
    ///
    /// # Errors
    ///
    /// [`SourceError::Protocol`] for a name that could walk out of `/api/v1`.
    pub(crate) fn path_segment(value: &str) -> Result<&str, SourceError> {
        segment(value)
    }
}

/// `("limit", "50"), ("page", "<n>")`.
fn page_params(page: u32) -> ((&'static str, String), (&'static str, String)) {
    (("limit", PAGE_SIZE.to_string()), ("page", page.to_string()))
}

/// A commit's object id, refused unless it is 4 to 64 hex characters --
/// git's shortest abbreviation up to a SHA-256 id (40 for SHA-1) -- which is
/// also nothing that could leave the path it is put in.
fn object_id(value: &str) -> Result<&str, SourceError> {
    let ok = (4..=64).contains(&value.len()) && value.chars().all(|c| c.is_ascii_hexdigit());
    ok.then_some(value)
        .ok_or_else(|| SourceError::protocol(format!("gitea: {value:?} is not a commit id")))
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
        SourceError::protocol(format!(
            "gitea: {value:?} is not a usable owner or repository name"
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use knobas_http::StatusCode;

    const SECRET: &str = "tidewater-pat-not-in-any-log";

    /// The Add-source form is generated from the descriptor's `auth_methods`
    /// (spec §3a), and the transport refuses any method the rules leave out.
    /// The two lists drifting apart either offers a method that is then
    /// refused, or accepts one nobody can choose.
    #[test]
    fn the_credential_rules_speak_exactly_what_the_form_offers() {
        assert_eq!(
            CREDENTIAL_RULES.methods,
            crate::descriptor_template().auth_methods.as_slice()
        );
    }

    /// Spec §14: the secret exists in one place. `Debug` is the leak that gets
    /// forgotten -- a `dbg!` on the client would otherwise put a PAT in a log.
    #[test]
    fn debug_never_prints_the_secret() {
        let auth =
            credential(&CREDENTIAL_RULES, Some(AuthMethod::Pat), None, Some(SECRET)).unwrap();
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
            matches!(error, SourceError::Protocol { message: ref m, .. } if m.contains("gitea.example.com")),
            "{error:?}"
        );
    }

    /// Gitea's error envelope, read where the body still exists so the message
    /// carries the sentence and not the JSON document around it.
    ///
    /// Asserted through `knobas_http::status_error` -- the function that
    /// actually consults the hook -- so the `HTTP <status>: ` prefix this crate
    /// does not own is pinned where it comes from.
    #[test]
    fn giteas_error_envelope_becomes_the_message() {
        let error = knobas_http::status_error(
            StatusCode::NOT_FOUND,
            r#"{"message":"user redirect does not exist [name: tidewatr]","url":"https://gitea.example/api/swagger"}"#,
            Some(error_message),
        );
        assert!(
            matches!(&error, SourceError::Protocol { message, .. }
                     if message == "HTTP 404 Not Found: user redirect does not exist \
                                    [name: tidewatr]"),
            "the url to Gitea's own swagger page helps nobody reading a sync failure: {error:?}"
        );

        // Anything that is not the envelope reads as nothing, which keeps the
        // raw excerpt knobas-http would have built.
        for body in [
            "",
            "<html>502 Bad Gateway</html>",
            r#"{"url":"https://gitea.example/api/swagger"}"#,
            r#"{"message":"   "}"#,
            r#"{"message":42}"#,
        ] {
            assert_eq!(error_message(body), None, "{body:?}");
        }
    }

    /// Ruling B4: 403 and 404 are about one repository; everything else is
    /// about the run and must abort it -- **401 included**.
    ///
    /// The 401 is the line ADR-0004 moved. It used to arrive here as the same
    /// bare `Unauthorized` a 403 did, so it had to be read as repository-scoped
    /// and `sync::credential_still_good` re-measured to recover the difference.
    /// The status is carried now, and a credential the server rejected ends the
    /// run without a second request.
    #[test]
    fn only_a_refusal_or_an_absence_is_scoped_to_one_repository() {
        for status in [StatusCode::FORBIDDEN, StatusCode::NOT_FOUND] {
            assert!(
                is_repo_scoped(&knobas_http::status_error(status, "", None)),
                "{status}"
            );
        }
        // A dead credential is the run's verdict, not one repository's.
        assert!(!is_repo_scoped(&knobas_http::status_error(
            StatusCode::UNAUTHORIZED,
            "",
            None
        )));
        // And so is a source with no secret in the keychain at all, which
        // carries no status to be mistaken for a refusal of one repository.
        assert!(!is_repo_scoped(&SourceError::unauthorized()));
        for other in [
            knobas_http::status_error(StatusCode::CONFLICT, "", None),
            knobas_http::status_error(StatusCode::INTERNAL_SERVER_ERROR, "", None),
            SourceError::Unreachable("connection refused".to_owned()),
            SourceError::Sink("pool closed".to_owned()),
            SourceError::protocol("gitea: /user answered an unreadable body"),
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
        let server = MockServer::builder().start().await;
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
            .commits(
                "tidewater",
                "payout-service",
                "main",
                (Some(since), None),
                1,
            )
            .await
            .expect("the mock answers only the fully-parameterised request");
        assert_eq!(got.len(), 1);
        assert_eq!(got[0]["sha"], "c90d11");
    }

    /// A run with no watermark yet must not send `since=` at all: an absent
    /// parameter is "from the beginning", and sending an empty one is a 400.
    #[tokio::test]
    async fn a_first_commit_listing_sends_no_since_at_all() {
        let server = MockServer::builder().start().await;
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
            .commits("tidewater", "payout-service", "main", (None, None), 1)
            .await
            .expect("no watermark means no since parameter");
    }

    /// `state` defaults to `open`, and a merged pull request is exactly the one
    /// the ticket-to-PR story ends on.
    #[tokio::test]
    async fn a_pull_listing_asks_for_every_state_most_recently_updated_first() {
        let server = MockServer::builder().start().await;
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
        let server = MockServer::builder().start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/repos/search"))
            .and(query_param("sort", "id"))
            .and(query_param("order", "asc"))
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
        let server = MockServer::builder().start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/repos/search"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({ "ok": false })),
            )
            .mount(&server)
            .await;
        let error = client_for(&server).await.search_repos(1).await.unwrap_err();
        assert!(
            matches!(error, SourceError::Protocol { message: ref m, .. } if m.contains("data array")),
            "{error:?}"
        );
    }

    /// 409 is Gitea's "this repository has no commits yet". A freshly created
    /// repository is not a failed sync.
    #[tokio::test]
    async fn an_empty_repository_answers_no_commits_rather_than_failing() {
        let server = MockServer::builder().start().await;
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
            .commits("tidewater", "fresh", "main", (None, None), 1)
            .await
            .expect("an empty repository is not a failure");
        assert!(got.is_empty());
    }

    /// Gitea keeps a pull request's discussion on the **issue** with the same
    /// index -- there is no `/pulls/{n}/comments`. Ruling B1 grants this fifth
    /// read endpoint; the path is the part nobody guesses right.
    ///
    /// The mock also demands that **no** `limit` and **no** `page` are sent
    /// (issue #131). This endpoint declares neither in Gitea's own OpenAPI
    /// document and ignores both -- measured on the pinned container -- so
    /// sending them would be a request the API has no parameter for, and the
    /// next reader would take it as proof that the discussion pages.
    #[tokio::test]
    async fn pull_request_discussion_is_read_from_the_issue_with_the_same_index() {
        let server = MockServer::builder().start().await;
        Mock::given(method("GET"))
            .and(path(
                "/api/v1/repos/tidewater/payout-service/issues/142/comments",
            ))
            .and(|request: &wiremock::Request| request.url.query().is_none())
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                { "id": 1, "body": "Should the jitter be bounded?" }
            ])))
            .mount(&server)
            .await;
        let got = client_for(&server)
            .await
            .issue_comments("tidewater", "payout-service", 142)
            .await
            .expect("the discussion is read from the issue path, unparameterised");
        assert_eq!(got.raw.len(), 1);
        assert_eq!(got.raw[0]["body"], "Should the jitter be bounded?");
    }

    /// `X-Total-Count` is the discussion's only completeness signal, and it has
    /// to survive the trip from the wire to `sync::fetch_comments` -- the one
    /// seam between the response and the check, which no other test crosses.
    /// Without it the check reads `None` on every discussion and passes
    /// everything, which is the shape of a guard that is not there at all.
    #[tokio::test]
    async fn the_servers_own_count_of_a_discussion_reaches_the_caller() {
        let server = MockServer::builder().start().await;
        Mock::given(method("GET"))
            .and(path(
                "/api/v1/repos/tidewater/payout-service/issues/142/comments",
            ))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!([{ "id": 1, "body": "one of five" }]))
                    .insert_header("X-Total-Count", "5"),
            )
            .mount(&server)
            .await;
        let got = client_for(&server)
            .await
            .issue_comments("tidewater", "payout-service", 142)
            .await
            .expect("a header the adapter reads is not a failure");
        assert_eq!((got.raw.len(), got.total), (1, Some(5)));
    }

    /// A server that sends no such header -- an older Gitea, a proxy that
    /// strips it -- leaves the count unknown, and unknown must mean "believe
    /// what arrived" rather than "assume zero". Reading a missing header as 0
    /// would be harmless; reading an unparseable one as 0 too is what keeps a
    /// malformed header from failing every run.
    #[tokio::test]
    async fn a_discussion_with_no_count_header_is_no_count_at_all() {
        let server = MockServer::builder().start().await;
        Mock::given(method("GET"))
            .and(path(
                "/api/v1/repos/tidewater/payout-service/issues/142/comments",
            ))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!([{ "id": 1, "body": "one" }]))
                    .insert_header("X-Total-Count", "not a number"),
            )
            .mount(&server)
            .await;
        let got = client_for(&server)
            .await
            .issue_comments("tidewater", "payout-service", 142)
            .await
            .expect("an unreadable header is not a protocol failure");
        assert_eq!(got.total, None);
    }

    /// Every other status still fails: only 409 is forgiven, and only there.
    #[tokio::test]
    async fn a_broken_commit_listing_still_fails() {
        let server = MockServer::builder().start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/repos/tidewater/broken/commits"))
            .respond_with(ResponseTemplate::new(500).set_body_string("boom"))
            .mount(&server)
            .await;
        let error = client_for(&server)
            .await
            .commits("tidewater", "broken", "main", (None, None), 1)
            .await
            .unwrap_err();
        assert!(matches!(error, SourceError::Protocol { .. }), "{error:?}");
    }
}
