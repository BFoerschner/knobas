//! The **app**, end to end, against the seeded real Atlassian pair (issues
//! #276 and #287, ADR-0013) -- the half of each certification that an
//! adapter's own live suite cannot reach.
//!
//! `knobas-source-jira/tests/live_jira_seeded.rs` certifies the adapter: the
//! wire, the payload, the cursor. Two of M3.0's claims are not about the
//! adapter at all, and both are asserted here through the real engine over a
//! real database:
//!
//! * **Credential health.** "A wrong PAT yields the credential-health path" is
//!   a claim about `knobas.source_config.auth_state` and the `source:health`
//!   event the sources view renders from -- three crates downstream of the
//!   `SourceError` the adapter raises. And on this server it carries a real
//!   hazard with it: an unresolvable bearer token searches **anonymously**, and
//!   an anonymous `/rest/api/2/search` answers `200` with `total: 0`. The
//!   `ticket` kind claims `full_sync_exhaustive: true`, so a run that reported
//!   that as a completed sync would hand the engine a licence to tombstone
//!   every ticket in the mirror. [`a_revoked_pat_reaches_the_credential_health_surface_and_the_mirror_survives`]
//!   is that whole sentence, measured.
//! * **A worklog, all the way to PAY-231 and back** (issue #280). The write
//!   queue is what logs a day's blocks, and the id Jira answers with is what
//!   the local copy carries -- a value that exists for exactly the length of
//!   one POST response, and that nothing downstream could recover if the
//!   settle dropped it. [`a_days_work_is_logged_to_pay_231_and_comes_back_in_the_mirror`]
//!   asserts it at Jira, on the copy, and in the next sync's mirrored payload.
//! * **The three write ops through the write queue.** `tests/mockd.rs` calls
//!   `Source::write` directly; the queue is what the *app* calls, and what
//!   turns an adapter's refusal into the `refused` row the pending-writes panel
//!   shows. Both halves are asserted against **Jira's own answer**, never
//!   against knobas' mirror of it.
//!
//! The Confluence half (#287) is the M3.2 exit criterion, and it is here for
//! the same reason: *a comment that mentions me becomes an inbox item* is a
//! claim about the adapter, the sync engine, the inbox derivation and the
//! action filter together -- four crates, none of which can witness it alone.
//! [`a_comment_that_mentions_me_becomes_an_inbox_mention`] is that sentence,
//! measured.
//!
//! Confluence's credential health is here for the third time over the same
//! reason and with a different answer (#317). `live_confluence_seeded.rs`
//! holds the wire end -- a bearer token this Confluence cannot resolve is a
//! clean 401 on the content search too (#284), so Jira's anonymous-200 hazard
//! does not arise -- and what it cannot hold is the `auth_state` column, the
//! `source:health` event and a mirror to survive the refusal.
//! [`a_revoked_confluence_pat_reaches_the_credential_health_surface_and_the_mirror_survives`]
//! is those three, and its doc comment says at length what it does *not*
//! witness.
//!
//! `#[ignore]`d, so `just check` runs none of it. `just atlassian-live` from
//! the repo root stands the pair up, seeds it, runs this, and tears it down
//! again; inside a licence window already open:
//!
//! ```text
//! cd testenv && eval "$(./seed --env)"
//! cd .. && env -u RUSTUP_TOOLCHAIN cargo test -p knobas-app \
//!     --test atlassian_live -- --ignored --nocapture --test-threads=1
//! ```
//!
//! # What this file writes, and what it takes away
//!
//! It is the suite that writes: a personal access token, a comment on PAY-231,
//! one transition of PAY-240 and back, one new ticket in `PAY`, one worklog on
//! PAY-231, and -- for the Confluence half -- one comment on a seeded page,
//! **one edit of a seeded page's body** (#342), one page under the standup
//! parent, and one Confluence personal access token.
//! Every one of them is undone when the test ends, passing or panicking alike,
//! by a `Drop` that checks rather than assumes -- [`Litter`], [`Pat`] (which
//! guards a token at either product), [`Mention`], [`Edited`] and
//! [`Protocol`]. What a
//! *killed* run left behind is cleared before the next one takes a baseline:
//! [`Env::clear_leftovers`] deletes every issue and revokes every token
//! carrying [`LITTER_LABEL`], [`Wiki::clear_leftovers`] deletes every
//! comment whose body carries it, and [`Wiki::clear_leftover_tokens`] revokes
//! every Confluence token named after it. The **full** restore is
//! `knobas-source-jira`'s live suite's `Seeded::clear_leftovers`, which works
//! from `seed-state.json` rather than from a label and so also puts back a
//! comment or a status this suite left behind -- either suite's leftovers are
//! the other's to clear, because whichever runs next is the one that can.
//!
//! The three things it cannot take away are the `PAY` key counter -- Jira
//! never rewinds one, so a created ticket costs the project one key for ever
//! -- the `updated` stamps of what it touched, and **an edited page's version
//! history**: Confluence has no undo, so [`Edited`]'s restore is one more
//! version on top of the edit rather than a removal of it, and the page comes
//! out of a *successful* run at `version.number + 2` with its `version.when`
//! on the run's own clock. The bytes are the seed's again; the history is not,
//! and [`Edited`] says at length what does and does not read it. None of the
//! three is fixture content, and the environment is torn down at the end of
//! the window regardless.
//!
//! **Never a wrong password.** A real Jira counts failed password logins per
//! account and answers `403 AUTHENTICATION_DENIED` -- to the *correct* password
//! too -- once an account has failed a few. Everything here that needs a
//! credential Jira will not accept uses a **bearer token**, which is not a
//! login attempt; `Seeded::refused_source` in the adapter's live suite carries
//! the full reasoning.

mod live_digest;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use knobas_app::sources::{Registry, SourcesState};
use knobas_secrets::{MemoryStore, Secret, SecretStore};
use knobas_source::AuthMethod;
use knobas_sync::SyncTrigger;
use knobas_sync::health::AuthState;
use knobas_sync::scheduler::{Scheduler, SchedulerDeps, SyncEvents};
use live_digest::{Connections, Ending, day_window, sync};
use serde_json::json;

/// The source id, which is also the `EntityRef` namespace every mirrored
/// ticket is in (P10).
const JIRA: &str = "jira";

/// The label everything this suite writes is marked with, spelled the same as
/// `knobas-source-jira/tests/live_jira_seeded.rs`'s. A marker, so a person
/// looking at the server can tell whose litter it is; the restore that matters
/// is that suite's, and it works from `seed-state.json`.
const LITTER_LABEL: &str = "knobas-live-suite";

/// How long one HTTP exchange with the container may take.
const REQUEST_BUDGET: Duration = Duration::from_secs(30);

/// How long Jira's search index may lag a write made through the REST API --
/// the create is read back by JQL, which is an index read.
///
/// [`the_three_write_ops_go_through_the_queue_and_come_back_from_jira`] waits
/// on the *mirror* to this budget rather than on the index (#325), which is a
/// second use and not the same claim: what it needs is a re-mirror that has
/// finished, and the index is upstream of when a sync run first sees the
/// transition that starts one. One number for both because the lag it is
/// waiting through is the same lag, and `knobas-source-jira`'s own live suite
/// spends the same 60 seconds on it.
const INDEX_BUDGET: Duration = Duration::from_secs(60);

/// The issue a comment is posted on: the fixture's own story, and the one
/// whose worklog M3.1 will add to.
const COMMENTED: &str = "PAY-231";

/// The issue that is transitioned. `PAY-240` is the fixture's **To Do** task,
/// so the legal move is out of To Do and the guard's undo is back into it --
/// and it is the issue `tests/start_work_live.rs` uses against mockd for the
/// same reason.
const TRANSITIONED: &str = "PAY-240";

/// A status the template's workflow does not have at all.
///
/// Not a status the issue merely cannot reach *from here*: this workflow
/// ("Software Simplified Workflow for Project `<KEY>`") offers all four of its
/// statuses from every one of them, the issue's own included -- which is
/// itself a divergence from mockd, whose fixture workflow does not. So in
/// **this** project the only way to witness a refusal against the real server
/// is a status that is not in the workflow, and `seed-state.json`'s
/// `jira.statuses` is what this is checked against rather than assumed.
///
/// It is the weaker of the two refusals, and #522 is why there are two: a
/// status that exists nowhere on the instance can be refused by an adapter
/// that never read a workflow at all. See [`NARROW_UNREACHABLE_FROM_FIRST`]
/// for the refusal of a status that exists in its project and is merely out of
/// reach from where the ticket stands.
const UNREACHABLE_STATUS: &str = "Blocked";

/// The project a ticket is created in, and the type it is created as -- one
/// the template's issue type scheme has.
const CREATE_PROJECT: &str = "PAY";
const CREATE_TYPE: &str = "Task";

/// **The narrowing workflow's states, and what each of them offers** (issue
/// #522).
///
/// `NARROW` is the seed's second Jira project, from Jira Core's
/// process-management template, and unlike the Simplified workflow the two
/// Tidewater projects share, its workflow reaches a **proper subset** of its
/// own statuses from every one of them. Walked on the real server on
/// 2026-09-08 (Jira 10.3.24), the whole of it:
///
/// | standing       | offers                    |
/// |----------------|---------------------------|
/// | `Open`         | `In Progress`             |
/// | `In Progress`  | `Under Review`, `Cancelled` |
/// | `Under Review` | `Approved`, `Rejected`    |
/// | `Approved`     | `Done`                    |
/// | `Done`         | nothing -- terminal       |
/// | `Cancelled`    | `Open`                    |
/// | `Rejected`     | `In Progress`             |
///
/// The two rows below are the two this suite stands a ticket in. They are
/// **constants and not a read**, deliberately: the thing under test is a read
/// of `/transitions`, so an expectation taken from `/transitions` would assert
/// that a value equals itself. The *denominator* -- the seven statuses the
/// project has -- does come off the server, through `seed-state.json`'s
/// `jira.narrowing.statuses`, which the seed read off
/// `GET /rest/api/2/project/NARROW/statuses`: a different endpoint. So
/// "answered exactly this, and this is fewer than the project has" is one
/// claim checked against a constant and one against the server.
const NARROW_FIRST_STATE: &str = "Open";
const NARROW_FROM_FIRST: [&str; 1] = ["In Progress"];
const NARROW_SECOND_STATE: &str = "In Progress";
const NARROW_FROM_SECOND: [&str; 2] = ["Under Review", "Cancelled"];

/// A status the narrowing project **has** and a ticket standing in
/// [`NARROW_FIRST_STATE`] cannot reach.
///
/// This is the refusal the reachable-transition read exists to stop offering,
/// and the one [`UNREACHABLE_STATUS`] cannot witness: `"Blocked"` is a status
/// that exists nowhere on this Jira, so refusing it says only that the adapter
/// will not invent a transition. `"Done"` is one of the seven statuses this
/// project's own workflow has, and is still not somewhere an `Open` ticket may
/// go -- which is the shape a person actually hits.
const NARROW_UNREACHABLE_FROM_FIRST: &str = "Done";

// -- the environment --------------------------------------------------------

struct Env {
    url: String,
    user: String,
    password: String,
    http: reqwest::Client,
    /// `jira.statuses` from `seed-state.json`: what the seeded workflow offers.
    statuses: Vec<String>,
    /// `jira.narrowing` from `seed-state.json`: the second project, whose
    /// workflow does not reach every status from every status.
    narrowing: Narrowing,
}

/// The seed's narrowing-workflow project, as `seed-state.json` records it.
#[derive(Clone)]
struct Narrowing {
    key: String,
    /// The issue type the seed checked this project's scheme has, and the one
    /// a ticket is filed as below.
    issue_type: String,
    /// Every status the project's workflow for [`Narrowing::issue_type`] has,
    /// off `GET /rest/api/2/project/<KEY>/statuses` -- not off the
    /// `/transitions` read this suite is about. That endpoint answers per issue
    /// type, and the seed records the row for the type the ticket below is
    /// filed as rather than the first row, so this really is the denominator
    /// the ticket's own workflow is measured against.
    statuses: Vec<String>,
}

fn env() -> Env {
    let need = |key: &str| {
        std::env::var(key)
            .ok()
            .map(|v| v.trim().to_owned())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| {
                panic!(
                    "{key} is not set -- this suite needs testenv's seeded Jira. From the repo \
                     root: `just atlassian-live`; or, inside a licence window already open, \
                     `eval \"$(cd testenv && ./seed --env)\"`"
                )
            })
    };
    let state = std::env::var("KNOBAS_JIRA_SEED_STATE")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testenv/seed-state.json")
        });
    let raw = std::fs::read_to_string(&state).unwrap_or_else(|e| {
        panic!(
            "{}: {e} -- run `./seed-atlassian-content.sh`",
            state.display()
        )
    });
    let whole: serde_json::Value = serde_json::from_str(&raw).expect("seed-state.json is JSON");
    let statuses: Vec<String> = whole["jira"]["statuses"][CREATE_PROJECT]
        .as_array()
        .unwrap_or_else(|| {
            panic!(
                "{}: no jira.statuses.{CREATE_PROJECT} -- run `./seed-atlassian-content.sh`",
                state.display()
            )
        })
        .iter()
        .filter_map(|s| s.as_str().map(str::to_owned))
        .collect();
    let narrowing = &whole["jira"]["narrowing"];
    let strings = |v: &serde_json::Value| -> Vec<String> {
        v.as_array()
            .into_iter()
            .flatten()
            .filter_map(|s| s.as_str().map(str::to_owned))
            .collect()
    };
    let narrowing = Narrowing {
        key: narrowing["key"]
            .as_str()
            .unwrap_or_else(|| {
                panic!(
                    "{}: no jira.narrowing.key -- the narrowing-workflow project is what gives \
                     the reachable-transition read a live witness; run \
                     `./seed-atlassian-content.sh`",
                    state.display()
                )
            })
            .to_owned(),
        issue_type: narrowing["issue_type"]
            .as_str()
            .unwrap_or_else(|| panic!("{}: no jira.narrowing.issue_type", state.display()))
            .to_owned(),
        statuses: strings(&narrowing["statuses"]),
    };
    Env {
        url: need("KNOBAS_JIRA_URL").trim_end_matches('/').to_owned(),
        user: need("KNOBAS_JIRA_USER"),
        password: need("KNOBAS_JIRA_PASSWORD"),
        http: client(),
        statuses,
        narrowing,
    }
}

fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(REQUEST_BUDGET)
        .build()
        .expect("a reqwest client with a timeout")
}

impl Env {
    /// One raw request as the seed's admin, answering status and body -- what
    /// every assertion below is made against, because the criterion is what
    /// **Jira** holds and not what knobas thinks it does.
    async fn api(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<serde_json::Value>,
    ) -> (u16, serde_json::Value) {
        api(
            &self.http,
            &self.url,
            &self.user,
            &self.password,
            method,
            path,
            body,
        )
        .await
    }

    async fn issue(&self, key: &str, fields: &str) -> serde_json::Value {
        let (status, body) = self
            .api(
                reqwest::Method::GET,
                &format!("rest/api/2/issue/{key}?fields={fields}"),
                None,
            )
            .await;
        assert_eq!(status, 200, "GET issue {key}: {body}");
        body
    }

    async fn status_at_jira(&self, key: &str) -> String {
        self.issue(key, "status").await["fields"]["status"]["name"]
            .as_str()
            .expect("a status name")
            .to_owned()
    }

    /// The keys a JQL query answers with.
    async fn jql(&self, jql: &str) -> Vec<String> {
        let encoded: String = url_encode(jql);
        let (status, body) = self
            .api(
                reqwest::Method::GET,
                &format!("rest/api/2/search?jql={encoded}&maxResults=100&fields=key"),
                None,
            )
            .await;
        assert_eq!(status, 200, "JQL {jql:?} answered {body}");
        body["issues"]
            .as_array()
            .map(|rows| {
                rows.iter()
                    .filter_map(|row| row["key"].as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Wait until Jira's **search index** answers `jql` with `expect`.
    ///
    /// The adapter enumerates through `/rest/api/2/search`, which reads Lucene
    /// and not the database, and Jira updates that index *asynchronously*
    /// after a write. So a sync fired the instant a REST write returns 204 can
    /// mirror the row as it was before the write -- which is the race #325
    /// records against a different assertion, and which made
    /// `a_seeded_days_work_is_what_the_digest_lists_under_yesterday` fail one
    /// run in two with two of its three producers present and the mirror's
    /// missing.
    ///
    /// So: any live assertion that writes through the REST API and then asks
    /// the *mirror* about it has to wait for the index first, and this is
    /// where it waits. Bounded, and it fails loudly rather than syncing
    /// anyway -- a test that quietly went on with stale data is the thing
    /// being fixed, not a cheaper version of it.
    async fn indexed(&self, jql: &str, expect: &str) {
        const EVERY: Duration = Duration::from_millis(250);
        const CAP: usize = 60;
        for attempt in 0..CAP {
            if self.jql(jql).await.iter().any(|key| key == expect) {
                if attempt > 0 {
                    println!(
                        "SEEDED search index caught up after {}ms: {jql}",
                        attempt * 250
                    );
                }
                return;
            }
            tokio::time::sleep(EVERY).await;
        }
        panic!(
            "Jira's search index never answered {jql:?} with {expect} in {}s. The write landed \
             (its own status was asserted above); what did not is the index the adapter reads.",
            CAP * 250 / 1000
        );
    }

    /// Delete every issue and revoke every token a **killed** run left behind
    /// -- the ones no `Drop` ever reached. Read-only in the ordinary case.
    async fn clear_leftovers(&self) {
        for key in self.jql(&format!("labels = {LITTER_LABEL}")).await {
            let (status, body) = self
                .api(
                    reqwest::Method::DELETE,
                    &format!("rest/api/2/issue/{key}"),
                    None,
                )
                .await;
            assert_eq!(status, 204, "deleting the leftover issue {key}: {body}");
            println!("live suite: deleted leftover issue {key}");
        }
        let (status, body) = self
            .api(reqwest::Method::GET, "rest/pat/latest/tokens", None)
            .await;
        assert_eq!(status, 200, "listing personal access tokens: {body}");
        for id in token_ids(&body, |name| name.starts_with(LITTER_LABEL)) {
            let (status, body) = self
                .api(
                    reqwest::Method::DELETE,
                    &format!("rest/pat/latest/tokens/{id}"),
                    None,
                )
                .await;
            assert_eq!(
                status, JIRA_TOKENS.revoked,
                "revoking the leftover token {id}: {body}"
            );
            println!("live suite: revoked leftover personal access token {id}");
        }
    }

    /// A personal access token at this Jira, revoked when the guard drops.
    async fn pat(&self) -> Pat {
        Pat::issue(&self.url, &self.user, &self.password, JIRA_TOKENS).await
    }
}

/// Free-standing so `Drop`'s cleanup thread, which has no [`Env`], can use it.
async fn api(
    http: &reqwest::Client,
    url: &str,
    user: &str,
    password: &str,
    method: reqwest::Method,
    path: &str,
    body: Option<serde_json::Value>,
) -> (u16, serde_json::Value) {
    let mut request = http
        .request(method.clone(), format!("{url}/{path}"))
        .header("Accept", "application/json")
        .basic_auth(user, Some(password));
    if let Some(body) = body {
        request = request.json(&body);
    }
    let response = request
        .send()
        .await
        .unwrap_or_else(|e| panic!("{method} {path}: {e}"));
    let status = response.status().as_u16();
    let text = response.text().await.unwrap_or_default();
    let json = serde_json::from_str(&text).unwrap_or(serde_json::Value::String(text));
    (status, json)
}

/// Percent-encode what a JQL clause may contain. Deliberately tiny: the only
/// queries here are composed from constants and from keys Jira itself
/// answered.
fn url_encode(raw: &str) -> String {
    raw.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            other => format!("%{other:02X}"),
        })
        .collect()
}

// -- what the suite writes, and gives back -----------------------------------

/// What one product answers to the two personal-access-token calls this suite
/// makes.
///
/// Data rather than a range, because this file's rule is that a status is a
/// **measured fact about a server** and every assertion over one cites where it
/// was measured. `2xx` would pass on a Confluence that started answering `202`
/// to a create, and the whole point of a live suite is that it does not.
#[derive(Clone, Copy)]
struct TokenStatuses {
    created: u16,
    revoked: u16,
}

/// Jira's, measured by #276's run of this suite.
const JIRA_TOKENS: TokenStatuses = TokenStatuses {
    created: 201,
    revoked: 204,
};

/// Confluence's, measured by #317's run against `atlassian/confluence:9.2.21`
/// -- the same numbers, from the same `/rest/pat/latest/tokens` REST API.
///
/// Written down as a *measurement* because until that run nothing in this repo
/// had ever called this endpoint on a Confluence: both adapters' notes call it
/// "outside this adapter's endpoint set", which is a statement about the
/// adapter and not about the server. The same run confirmed the two halves the
/// suite rests on -- a token issued here authenticates `/rest/api/content`
/// (200), and once revoked it is a 401 there.
const CONFLUENCE_TOKENS: TokenStatuses = TokenStatuses {
    created: 201,
    revoked: 204,
};

/// What this Confluence answers a **second create of a page whose title is
/// already taken in its space** -- the refusal `knobas_app::protocol`'s
/// duplicate-title ruling rests on (#289).
///
/// A number rather than `>= 400`, on the same rule as [`TokenStatuses`]: a
/// status is a **measured fact about a server** and every assertion over one
/// cites where it was measured. A range obeys neither half, and it is not a
/// harmless looseness here -- the refusals it also accepts are the ones that
/// would mean this test learned nothing at all (see
/// [`a_second_page_with_one_title_in_one_space_is_refused`], which spells out
/// what each other status would be saying).
///
/// Measured by #289's live run and again by #355's, against
/// `atlassian/confluence:9.2.21` -- the image `testenv/pin-images.sh` pins and
/// the product version [`CONFLUENCE_TOKENS`] cites.
const DUPLICATE_TITLE_REFUSED: u16 = 400;

/// A personal access token of this suite's own, revoked when the guard drops.
///
/// The credential-health criterion is about a **PAT**, and it is about one that
/// *worked* and stopped working -- so the suite makes a real one rather than
/// asserting over a token that was never valid. It also certifies, against the
/// product, the claim each adapter's `http` module makes from the
/// documentation: a Data Center personal access token is a Bearer token.
///
/// `/rest/pat/latest/tokens` is outside either adapter's own endpoint set (it
/// is why `ConnectionInfo::secret_expires_at` is always `None`), which is
/// exactly why it is reached here with raw requests and not through the
/// adapter.
///
/// **One type for both products**, reached through [`Env::pat`] and
/// [`Wiki::pat`]: the call, the guard and the checked revoke are the same three
/// requests at Jira and at Confluence, and the only things that differ are the
/// account they are made as and the two statuses above -- both of which are
/// values, so a second copy of the type would carry no second fact.
struct Pat {
    url: String,
    user: String,
    password: String,
    statuses: TokenStatuses,
    /// The path segment it becomes: see [`token_id`].
    id: String,
    raw: String,
}

impl Pat {
    async fn issue(url: &str, user: &str, password: &str, statuses: TokenStatuses) -> Pat {
        let name = format!("{LITTER_LABEL}-{}", std::process::id());
        let (status, body) = api(
            &client(),
            url,
            user,
            password,
            reqwest::Method::POST,
            "rest/pat/latest/tokens",
            Some(json!({ "name": name, "expirationDuration": 1 })),
        )
        .await;
        assert_eq!(
            status, statuses.created,
            "creating a personal access token at {url}: {status} {body}"
        );
        Pat {
            url: url.to_owned(),
            user: user.to_owned(),
            password: password.to_owned(),
            statuses,
            id: token_id(&body),
            raw: body["rawToken"]
                .as_str()
                .unwrap_or_else(|| {
                    panic!("the raw token is answered exactly once, at creation: {body}")
                })
                .to_owned(),
        }
    }
}

impl Drop for Pat {
    fn drop(&mut self) {
        let (url, user, password, statuses, id) = (
            self.url.clone(),
            self.user.clone(),
            self.password.clone(),
            self.statuses,
            self.id.clone(),
        );
        let what = format!("the personal access token at {url}");
        undo(&what, move || async move {
            let http = client();
            let (status, body) = api(
                &http,
                &url,
                &user,
                &password,
                reqwest::Method::DELETE,
                &format!("rest/pat/latest/tokens/{id}"),
                None,
            )
            .await;
            if status != statuses.revoked && status != 404 {
                return Err(format!("DELETE token {id} -> {status}: {body}"));
            }
            let (status, body) = api(
                &http,
                &url,
                &user,
                &password,
                reqwest::Method::GET,
                "rest/pat/latest/tokens",
                None,
            )
            .await;
            if status != 200 {
                return Err(format!(
                    "listing tokens after the delete -> {status}: {body}"
                ));
            }
            if token_ids(&body, |_| true).contains(&id) {
                return Err(format!("token {id} is still listed after its delete"));
            }
            Ok(())
        });
    }
}

/// The ids of the tokens in a `rest/pat/latest/tokens` listing whose name the
/// predicate accepts.
fn token_ids(body: &serde_json::Value, accept: impl Fn(&str) -> bool) -> Vec<String> {
    token_rows(body)
        .iter()
        .filter(|t| t["name"].as_str().is_some_and(&accept))
        .map(token_id)
        .collect()
}

/// The records in a token listing -- and a **panic** for a body that is not
/// one.
///
/// Both products answer a bare array, measured: Jira's by #276, Confluence's by
/// #317. So there is one shape here and no envelope to unwrap, and this is a
/// function rather than an `as_array()` at each of the three call sites for the
/// sake of the panic. Reading an unrecognised body as *no tokens* is the
/// failure worth spending one on: [`Pat`]'s `Drop` would then report a revoke
/// it never checked, and the header of this file promises a guard that checks
/// rather than assumes. [`undo`] catches the panic and reports it, so a `Drop`
/// that hits this says so rather than aborting.
fn token_rows(body: &serde_json::Value) -> &Vec<serde_json::Value> {
    body.as_array().unwrap_or_else(|| {
        panic!(
            "a token listing is an array; this is not one, and reading it as no tokens would \
             make every revoke below vacuous: {body}"
        )
    })
}

/// One token record's id, as the path segment it becomes.
///
/// Both products answer a JSON **number** -- Jira's read that way since #276,
/// Confluence's measured by #317 -- and every use of an id here is either a URL
/// path segment or a comparison between two of those, so it is stringified in
/// this one place and there is one spelling downstream.
///
/// A record whose id is neither panics rather than being skipped, for
/// [`token_rows`]'s reason: a silently dropped record is a revoke reported
/// without being checked.
fn token_id(token: &serde_json::Value) -> String {
    token["id"]
        .as_i64()
        .map(|id| id.to_string())
        .unwrap_or_else(|| panic!("a token id is a JSON number on both products: {token}"))
}

/// Everything the write tests put into Jira, taken back out when the guard
/// drops -- passing or panicking alike, and each undo **checked** rather than
/// assumed.
#[derive(Default)]
struct Litter {
    /// `(issue key, comment id)`.
    comment: Option<(String, String)>,
    /// `(issue key, the status it was in before)`.
    moved: Option<(String, String)>,
    /// The key of the ticket the create filed.
    created: Option<String>,
    /// `(issue key, the account it was assigned to before)` -- what the digest
    /// suite borrows a ticket with.
    ///
    /// `None` in the second half is a ticket that was **unassigned**, which is
    /// a state the restore has to be able to put back: `{"name": null}` is how
    /// Jira spells it, and restoring it as the empty string would leave a
    /// ticket assigned to an account that does not exist.
    assigned: Option<(String, Option<String>)>,
    /// `(issue key, worklog id)` -- what `log_work` put on the ticket.
    ///
    /// A worklog carries no label, so a **killed** run's worklog is the one
    /// thing here that `Env::clear_leftovers` cannot find: it is not an issue
    /// and it is not a token. What puts it back is the adapter live suite's
    /// `Seeded::clear_leftovers`, which restores PAY-231's worklogs from
    /// `seed-state.json` rather than from a marker -- the same division the
    /// module docs record for a comment and a status.
    worklog: Option<(String, String)>,
}

impl Drop for Litter {
    fn drop(&mut self) {
        let (comment, moved, created, worklog, assigned) = (
            self.comment.take(),
            self.moved.take(),
            self.created.take(),
            self.worklog.take(),
            self.assigned.take(),
        );
        let env = env();
        undo("what the write queue sent", move || async move {
            let http = client();
            let call = |method: reqwest::Method, path: String, body: Option<serde_json::Value>| {
                let http = http.clone();
                let (url, user, password) =
                    (env.url.clone(), env.user.clone(), env.password.clone());
                async move { api(&http, &url, &user, &password, method, &path, body).await }
            };
            let mut failures: Vec<String> = Vec::new();

            if let Some((key, id)) = comment {
                let (status, body) = call(
                    reqwest::Method::DELETE,
                    format!("rest/api/2/issue/{key}/comment/{id}"),
                    None,
                )
                .await;
                if status != 204 && status != 404 {
                    failures.push(format!("DELETE comment {id} on {key} -> {status}: {body}"));
                }
            }

            // The status is put back through the workflow, which is the only
            // way a status changes: this workflow offers every status from
            // every status, so the move back always exists.
            if let Some((key, was)) = moved {
                let (status, body) = call(
                    reqwest::Method::GET,
                    format!("rest/api/2/issue/{key}/transitions"),
                    None,
                )
                .await;
                let id = body["transitions"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .find(|t| t["to"]["name"].as_str() == Some(was.as_str()))
                    .and_then(|t| t["id"].as_str().map(str::to_owned));
                match id {
                    None => failures.push(format!(
                        "the workflow no longer offers {key} a way back to {was:?} \
                         ({status}): {body}"
                    )),
                    Some(id) => {
                        let (status, body) = call(
                            reqwest::Method::POST,
                            format!("rest/api/2/issue/{key}/transitions"),
                            Some(json!({ "transition": { "id": id } })),
                        )
                        .await;
                        if status != 204 {
                            failures
                                .push(format!("moving {key} back to {was:?} -> {status}: {body}"));
                        }
                    }
                }
            }

            // Deleted with `adjustEstimate=leave`, deliberately: the POST that
            // made it left the estimate at Jira's default of `auto`, which took
            // the logged time *off* the remaining estimate. The delete's own
            // default would put it back -- and `auto` on a delete means
            // "increase", so a seeded estimate would come back changed by the
            // rounding rather than restored. `leave` takes the worklog away and
            // touches nothing else, which is what a guard is for.
            if let Some((key, id)) = worklog {
                let (status, body) = call(
                    reqwest::Method::DELETE,
                    format!("rest/api/2/issue/{key}/worklog/{id}?adjustEstimate=leave"),
                    None,
                )
                .await;
                if status != 204 && status != 404 {
                    failures.push(format!("DELETE worklog {id} on {key} -> {status}: {body}"));
                }
                let (status, after) = call(
                    reqwest::Method::GET,
                    format!("rest/api/2/issue/{key}/worklog"),
                    None,
                )
                .await;
                if status != 200 {
                    failures.push(format!(
                        "reading {key}'s worklogs back -> {status}: {after}"
                    ));
                } else if after["worklogs"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .any(|w| w["id"].as_str() == Some(id.as_str()))
                {
                    failures.push(format!("worklog {id} is still on {key} after its delete"));
                }
            }

            // Put the assignee back, including putting *nobody* back: an
            // unassigned ticket is a state the seed can be in, and `name:
            // null` is how Jira is told to restore it.
            if let Some((key, was)) = assigned {
                let (status, body) = call(
                    reqwest::Method::PUT,
                    format!("rest/api/2/issue/{key}/assignee"),
                    Some(json!({ "name": was })),
                )
                .await;
                if status != 204 {
                    failures.push(format!(
                        "restoring {key}'s assignee to {was:?} -> {status}: {body}"
                    ));
                }
            }

            if let Some(key) = created {
                let (status, body) = call(
                    reqwest::Method::DELETE,
                    format!("rest/api/2/issue/{key}"),
                    None,
                )
                .await;
                if status != 204 && status != 404 {
                    failures.push(format!("DELETE issue {key} -> {status}: {body}"));
                }
                let (status, _) = call(
                    reqwest::Method::GET,
                    format!("rest/api/2/issue/{key}?fields=key"),
                    None,
                )
                .await;
                if status != 404 {
                    failures.push(format!("{key} still answers {status} after its delete"));
                }
            }

            if failures.is_empty() {
                Ok(())
            } else {
                Err(failures.join("; "))
            }
        });
    }
}

/// Run one cleanup on a thread with a runtime of its own -- `Drop` cannot
/// await, and a `reqwest::Client` driven from a second runtime hangs rather
/// than failing (the Gitea suite measured that) -- and report what it could
/// not undo.
///
/// Panicking while already unwinding aborts the process; the test is already
/// red in that case and the report only has to be visible.
fn undo<F, Fut>(what: &str, cleanup: F)
where
    F: FnOnce() -> Fut + Send + 'static,
    Fut: std::future::Future<Output = Result<(), String>>,
{
    let joined = std::thread::spawn(move || {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("a runtime for the cleanup")
            .block_on(cleanup())
    })
    .join();
    let failure = match joined {
        Ok(Ok(())) => return,
        Ok(Err(e)) => e,
        Err(_) => "the cleanup thread panicked (its own message is on stderr)".to_owned(),
    };
    let report = format!(
        "the live suite did not take back {what}, so the server is no longer in the plain-seed \
         state -- the next run's leftover clearing removes what carries {LITTER_LABEL:?}: {failure}"
    );
    if std::thread::panicking() {
        eprintln!("live suite cleanup: {report}");
    } else {
        panic!("{report}");
    }
}

// -- the app, wired the way the app wires it --------------------------------

/// The `source:health` events the shell would render, kept so the assertion
/// can be on the **event** and not only on the stored column. The two are
/// different claims: `set_health` reports a change once, and a sources view
/// that never heard would show a stale monogram until something else redrew it.
#[derive(Default)]
struct Events {
    health: Mutex<Vec<knobas_sync::CredentialHealth>>,
}

impl SyncEvents for Events {
    fn sync_state(&self, _status: knobas_sync::SourceSyncStatus) {}
    fn source_health(&self, health: knobas_sync::CredentialHealth) {
        self.health
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(health);
    }
    fn activity_new(&self, _row: knobas_core::activity::ActivityRow) {}
}

impl Events {
    fn states(&self) -> Vec<AuthState> {
        self.health
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .map(|h| h.state)
            .collect()
    }

    fn last(&self) -> knobas_sync::CredentialHealth {
        self.health
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .last()
            .cloned()
            .expect("at least one health event")
    }
}

/// A `SourcesState` over a database of this test's own, with the Jira source
/// configured and its credential in the (in-memory) keychain.
async fn app(name: &str, env: &Env, auth: AuthMethod, secret: &str) -> (SourcesState, Arc<Events>) {
    let connector = knobas_db::test_util::scratch_database(name).await;
    let pool = connector
        .pool(4)
        .await
        .expect("a pool onto the scratch database");
    let secrets = Arc::new(MemoryStore::new());
    secrets
        .put(&knobas_secrets::KeychainAccount::source(JIRA), &Secret::just(auth, secret))
        .expect("the Jira credential is stored");

    knobas_sync::config::insert(
        &pool,
        &knobas_sync::config::InsertConfig {
            id: JIRA.to_owned(),
            adapter_kind: "jira".to_owned(),
            display_name: "Tidewater Jira (seeded)".to_owned(),
            base_url: env.url.clone(),
            auth_kind: knobas_sync::config::AuthKind::Method(auth),
            // The username is half of a Basic pair and, for a PAT source, what
            // `@me` filters match against; the Add-source dialog fills it in
            // from `test_connection`.
            config: json!({ "username": env.user }),
            sync_interval_secs: 86_400,
            enabled: true,
        },
    )
    .await
    .expect("the source row is written");

    let events = Arc::new(Events::default());
    let scheduler = Scheduler::start(SchedulerDeps {
        pool: pool.clone(),
        connections: Arc::new(Connections(connector)),
        registry: Arc::new(Registry::builtin()),
        secrets: secrets.clone(),
        events: events.clone(),
        timing: knobas_sync::scheduler::SchedulerTiming::default(),
    })
    .await
    .expect("a scheduler over the scratch database");

    (
        SourcesState {
            pool,
            scheduler,
            secrets,
            registry: Arc::new(Registry::builtin()),
        },
        events,
    )
}

/// Sync the source **from no position at all**, and wait for the run to end.
///
/// `knobas_sync::backfill`'s own words: an incremental run re-fetches what
/// changed *upstream*, and it decides that from the watermark and the records
/// the previous run recorded. A fixture that changes something through the REST
/// API and then wants the mirror to agree is asking for exactly the job that
/// entry point exists to do.
///
/// **The Jira half of this fixture no longer needs it, and that is the point of
/// issue #345.** It used to: a comment through the write queue and a
/// reassignment in Jira landed in the same second, `/search` reports `updated`
/// to the second, and the cursor's `(key, updated)` pair therefore recognised
/// the reassignment as something already delivered and dropped it for ever.
/// Reaching for `backfill` got the fixture green and hid a real bug behind it.
/// The cursor now recognises a *record* by fingerprint, so an ordinary `sync`
/// delivers it, and the digest test below asks for one -- which is what keeps
/// that fixed.
///
/// **The Confluence half still needs it** (#289), for a different reason and
/// one no cursor change touches: that adapter reads CQL for *both* its runs, so
/// a page created seconds ago is missing from an incremental and from a
/// backfill alike until the CQL index catches up. It takes the source for that
/// reason.
async fn backfill(state: &SourcesState, source: &str) {
    let (sink, wait) = Ending::for_run();
    state
        .scheduler
        .trigger(source, SyncTrigger::Backfill, Some(sink))
        .await
        .expect("the backfill starts");
    wait.await.expect("the run reports its ending");
}

/// The keys the mirror holds live for this source -- the thing a wrongly
/// reported empty sync would erase.
async fn mirrored(pool: &sqlx::PgPool) -> Vec<String> {
    sqlx::query_scalar::<_, String>(
        "select entity_id from sync.live_item where source_id = $1 order by entity_id",
    )
    .bind(JIRA)
    .fetch_all(pool)
    .await
    .expect("the mirror is readable")
}

/// Queue one write through the app's own submit path -- the same call the
/// *Comment* button makes -- and answer the row as it settled.
async fn write(
    state: &SourcesState,
    op: serde_json::Value,
) -> knobas_core::write_queue::QueuedWrite {
    let queued = knobas_app::sources::write_queue::submit(state, op)
        .await
        .expect("the write is queued");
    // The row came back **as queued**, before the attempt, which says nothing
    // about the outcome. Read it again for what actually happened.
    knobas_core::write_queue::get(&state.pool, queued.id)
        .await
        .expect("the queue row is readable")
        .expect("the row this call just wrote")
}

// -- the criteria -----------------------------------------------------------

/// **A personal access token that worked and stopped working**, all the way
/// to what the sources view renders -- and the mirror still standing
/// afterwards.
///
/// The sequence is the one that actually happens to people: a source is added
/// with a PAT, syncs, and the token is later revoked. What makes it worth a
/// live test rather than a fake is what this Jira does with the revoked one:
/// an unresolvable bearer token is **not** a failed login. It never reaches
/// Seraph, and the request proceeds anonymously -- so
/// `GET /rest/api/2/search` answers **200 with `total: 0`**, an answer
/// indistinguishable from a Jira whose issues have all been deleted.
///
/// The `ticket` kind claims `full_sync_exhaustive: true`. A run that reported
/// that empty page as a completed full sync would therefore have the engine
/// tombstone every ticket knobas holds -- silently, and on nothing worse than
/// an expired token. It does not, because the run asks `/serverInfo` first for
/// the server's UTC offset and that call is a 401. The last assertion here is
/// the one that would fail if that ever stopped being true.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs testenv's seeded Jira: `just atlassian-live`"]
async fn a_revoked_pat_reaches_the_credential_health_surface_and_the_mirror_survives() {
    let env = env();
    env.clear_leftovers().await;
    let pat = env.pat().await;

    let (state, events) = app("atlassian_live_health", &env, AuthMethod::Pat, &pat.raw).await;

    // 1. The token works, which is what makes revoking it mean anything -- and
    //    is the product's own answer to `http::credential`'s claim that a Jira
    //    DC personal access token is a Bearer token.
    sync(&state, JIRA).await;
    let synced = mirrored(&state.pool).await;
    assert!(
        synced.len() >= 7,
        "the seeded corpus is mirrored under a personal access token: {synced:?}"
    );
    let healthy = knobas_sync::config::get(&state.pool, JIRA)
        .await
        .expect("the source row")
        .expect("the source this test configured");
    assert_eq!(healthy.health.state, AuthState::Ok, "{:?}", healthy.health);
    assert_eq!(
        events.states(),
        vec![AuthState::Ok],
        "the sources view is told once that the credential works"
    );

    // The stamp the *good* run left, kept so the refusal's own can be compared
    // against it. `set_health` writes `auth_checked_at = now()` on every
    // verdict, so by this point the column is already non-null and
    // `is_some()` on it below would pass whether or not the refused run ever
    // reached it.
    let checked_when_healthy = healthy.health.checked_at;

    // 2. The token is revoked -- here by swapping what the keychain holds,
    //    which is the same thing from the adapter's side and leaves the real
    //    token for the guard to clean up.
    //
    //    Bound rather than inlined because it is the secret **in play** for
    //    the run below, and so the one the §14 assertion has to name.
    let refused_secret = format!("revoked-{}", std::process::id());
    state
        .secrets
        .put(
            &knobas_secrets::KeychainAccount::source(JIRA),
            &Secret::just(AuthMethod::Pat, refused_secret.clone()),
        )
        .expect("the replacement credential is stored");

    sync(&state, JIRA).await;

    // 3. The credential-health path, at both ends of it: the stored column the
    //    sources view polls, and the event it re-renders on.
    let refused = knobas_sync::config::get(&state.pool, JIRA)
        .await
        .expect("the source row")
        .expect("the source this test configured");
    assert_eq!(
        refused.health.state,
        AuthState::Unauthorized,
        "a credential the server refuses is `unauthorized`, which is what puts *Re-enter* on the \
         row (interfaces §3): {:?}",
        refused.health
    );
    assert!(
        refused.health.checked_at > checked_when_healthy,
        "the refusal stamps `auth_checked_at` itself -- the column the sources view reads as \
         *when this was last asked*. Compared against the good run's stamp and not merely for \
         non-null, because the good run already filled it in: {:?} vs {checked_when_healthy:?}",
        refused.health
    );
    let event = events.last();
    assert_eq!(event.state, AuthState::Unauthorized);
    assert_eq!(event.source_id, JIRA);
    let detail = event.detail.clone().unwrap_or_default();
    assert!(
        detail.contains("unauthorized"),
        "the detail line names the fault the row is in: {detail:?}"
    );
    // The **status** behind it is on `SourceError::status` (ADR-0004) and is
    // deliberately not on this line -- `SourceError::Unauthorized`'s own
    // `Display` is the bare word, because 401 and 403 are one state to the
    // person being asked to re-enter a credential. Asserted as it is rather
    // than wished otherwise: putting the status here is an IPC-surface change
    // (§10.8) and no criterion asks for one.
    //
    // **`refused_secret` first, and it is the one that does the work.** The
    // bearer this run presented is the replacement, not `pat.raw` -- a detail
    // line that grew the server's answer, or the request that drew it, would
    // carry *that* string, and naming only `pat.raw` here would pass under an
    // implementation that echoed the presented credential verbatim. `pat.raw`
    // is asserted too because it is still this source's secret of record at
    // the keychain the run before.
    assert!(
        !detail.contains(&refused_secret) && !detail.contains(&pat.raw),
        "spec §14: a health detail is never a place a secret can reach: {detail:?}"
    );
    println!("SEEDED credential health after the revoke: {:?}", event);

    // 4. **And the mirror is untouched.** This is the assertion the anonymous
    //    empty search is dangerous for: a `cursor: None` run reported `Ok` with
    //    no items authorises the sweep to tombstone the lot.
    assert_eq!(
        mirrored(&state.pool).await,
        synced,
        "a refused sync must leave the mirror exactly as it was -- an anonymous \
         /rest/api/2/search answers 200 with total 0 on this server, and reporting that as a \
         completed full sync would tombstone every ticket knobas holds"
    );

    state.scheduler.shutdown().await;
}

/// **The three write ops M2 ratified, through the write queue and back out of
/// Jira** -- comment, a transition the workflow offers, one it does not, and a
/// create.
///
/// One test rather than four: they share one source, one mirror and one
/// cleanup guard, and splitting them would have each pass over a state the
/// others established. Every assertion is against **Jira's own answer** or
/// against the queue row, never against knobas' mirror of either.
///
/// The refusal is the interesting one. `tests/mockd.rs` asserts that the
/// adapter refuses an unreachable status *by name*; what the queue does with
/// that refusal -- `refused`, terminal, never retried, with the adapter's
/// sentence on the row for the pending-writes panel to show -- is this file's,
/// and it is the difference between an edit a person can act on and one that
/// disappears.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs testenv's seeded Jira: `just atlassian-live`"]
async fn the_three_write_ops_go_through_the_queue_and_come_back_from_jira() {
    use knobas_core::write_queue::WriteState;

    let env = env();
    env.clear_leftovers().await;
    let mut litter = Litter::default();
    // Under a **personal access token**, not the seed's password: a PAT is the
    // credential a real deployment configures and the one M3.1's `LogWork`
    // will write worklogs with, and nothing else in the repo has ever sent
    // Jira a write over one. The read direction under user + password is what
    // the adapter's own live suite certifies, and the seed script itself
    // writes over Basic, so neither scheme is left unwitnessed.
    let pat = env.pat().await;
    let (state, _events) = app("atlassian_live_writes", &env, AuthMethod::Pat, &pat.raw).await;

    // The mirror has to hold the tickets first: the queue snapshots its target
    // when a write is queued and re-reads it before sending, which is how a
    // write over a ticket that moved is held rather than sent.
    sync(&state, JIRA).await;
    let held = mirrored(&state.pool).await;
    for key in [COMMENTED, TRANSITIONED] {
        assert!(
            held.contains(&format!("{JIRA}:{key}")),
            "{key} is not in the mirror: {held:?}"
        );
    }

    // -- 1. Comment ---------------------------------------------------------
    let body = format!(
        "{LITTER_LABEL}: knobas wrote this through the write queue (pid {})",
        std::process::id()
    );
    let before = env.issue(COMMENTED, "comment").await["fields"]["comment"]["total"]
        .as_i64()
        .expect("a comment total");
    let row = write(
        &state,
        json!({ "Comment": { "entity": format!("{JIRA}:{COMMENTED}"), "body": body } }),
    )
    .await;
    assert_eq!(row.state, WriteState::Sent, "{:?}", row.detail);

    // Read back, and **owned before anything is asserted**: from here on the
    // comment exists at Jira, so a failing assertion below must still leave the
    // guard something to delete.
    let comments = env.issue(COMMENTED, "comment").await["fields"]["comment"].clone();
    let posted = comments["comments"]
        .as_array()
        .and_then(|c| c.last())
        .cloned()
        .expect("the comment just added");
    if let Some(id) = posted["id"].as_str() {
        litter.comment = Some((COMMENTED.to_owned(), id.to_owned()));
    }
    assert_eq!(
        comments["total"].as_i64(),
        Some(before + 1),
        "the comment is on the ticket at Jira: {comments}"
    );
    assert_eq!(posted["body"], body);
    assert_eq!(
        posted["author"]["name"], env.user,
        "story 17: the source attributes the write to the credential's own account"
    );

    // -- 2. A transition the workflow offers --------------------------------
    let was = env.status_at_jira(TRANSITIONED).await;
    let to = env
        .statuses
        .iter()
        .find(|s| *s != &was)
        .expect("the seeded workflow has more than one status")
        .clone();
    let row = write(
        &state,
        json!({ "Transition": { "entity": format!("{JIRA}:{TRANSITIONED}"), "status": to } }),
    )
    .await;
    // Recorded before the assertion: the guard has to put the status back even
    // if the queue reports something other than `sent`.
    litter.moved = Some((TRANSITIONED.to_owned(), was.clone()));
    assert_eq!(row.state, WriteState::Sent, "{:?}", row.detail);
    assert_eq!(
        env.status_at_jira(TRANSITIONED).await,
        to,
        "the board still lies about what is being worked on"
    );

    // -- ...and the mirror caught up with it, before anything else is queued --
    //
    // **Issue #325**, whose mechanism is not what the ticket guessed and is
    // worth writing down where the wait is.
    //
    // A queued write is compared against its target twice: `target_snapshot`
    // is `knobas_core::write_queue::project`'s reading of `sync.live_item`
    // when the row is inserted, and `knobas_sync::write_queue::attempt`
    // projects it again immediately before sending -- "hold detection comes
    // first, so a write over a moved target never reaches the network at all".
    // If the two differ it calls `knobas_core::write_queue::hold`, whose
    // statement sets `state = 'held', wait_reason = null, held_snapshot = $2`
    // and **writes no `detail`** -- unlike `refuse`, which sets `detail = $2`.
    // So `Held` with `detail: None` is the hold path and can be nothing else,
    // which is exactly what #297's run saw here.
    //
    // What moves the mirror in the middle of a test that syncs once, at the
    // top? The write queue's own follow-up read.
    // `knobas_app::sources::write_queue::flush` fires an incremental sync as
    // soon as a write lands (`refresh`, story 15), and `Scheduler::trigger`
    // **spawns** that run -- so the transition above returned with a sync of
    // this source still in flight. It re-reads `PAY-240`, whose `updated`
    // moved with the transition, and commits a `sync.item` row carrying the
    // new status. Land that commit between the `queue` and the `flush_source`
    // inside the next `submit` -- milliseconds, but a real window -- and the
    // refusal below is held instead of refused, with no detail, and nothing
    // in the output says why.
    //
    // Jira's search index is upstream of *when* that happens rather than the
    // cause: it decides which run first sees the transition (`Env::indexed`
    // records the same asynchrony for the raw-REST direction). Waiting on the
    // index alone would not settle this, because a stale mirror is harmless
    // -- both projections read it and agree. What has to be true before the
    // next write is queued is that the mirror has **finished** moving, so the
    // wait is on the mirrored status itself, to [`INDEX_BUDGET`], the budget
    // `knobas-source-jira`'s own live suite uses for this index.
    //
    // `sync` here is not an extra run in the ordinary case: `trigger` attaches
    // to the run `refresh` already started and waits for its ending.
    //
    // **What this compares, and what it does not.** `project`'s fallback arm
    // -- the one `"transition"` takes -- compares `title`, `text`,
    // `item_updated_at` *and the whole payload*, and the poll below reads one
    // field of one of those. That is enough here and only here: a run writes
    // the mirrored row in a single upsert, so the status arriving is the whole
    // row arriving. It is a witness that this ticket's re-mirror has happened,
    // not a general proof that no projected field can still move.
    //
    // **The negative control**, for anyone re-running the mutation check: point
    // the poll at `was` -- the status the mirror will never hold again -- and
    // the bounded `assert!` below is what dies, after `INDEX_BUDGET`. A wait
    // that was not really reading the mirror would sail past it.
    let transitioned = format!("{JIRA}:{TRANSITIONED}");
    let deadline = std::time::Instant::now() + INDEX_BUDGET;
    for round in 1.. {
        sync(&state, JIRA).await;
        // `fields.status.name` is where a Jira status lives, and the mirrored
        // `payload` is the record verbatim -- the reading
        // `knobas_core::write_queue::project` records for why `"transition"`
        // compares the whole record instead of the status.
        let in_mirror: Option<String> = sqlx::query_scalar(
            "select payload->'fields'->'status'->>'name' from sync.live_item
              where entity_id = $1",
        )
        .bind(&transitioned)
        .fetch_one(&state.pool)
        .await
        .expect(
            "the transitioned ticket left the mirror while this test was waiting for it -- \
             a sync that tombstoned it, not a stale index",
        );
        if in_mirror.as_deref() == Some(to.as_str()) {
            // Printed either way: the run's own output is the only place a
            // reader can see whether this wait was a formality on the day or
            // the thing that made the refusal below deterministic.
            println!("SEEDED mirror holds {TRANSITIONED} as {to:?} after {round} sync(s)");
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "{TRANSITIONED} is {to:?} at Jira (asserted above) but the mirror still holds it \
             as {in_mirror:?} after {INDEX_BUDGET:?}. Until the mirror agrees, the next write's \
             queue-time snapshot and its flush-time re-read can disagree, and the refusal \
             below comes back Held with no detail instead (#325)."
        );
        tokio::time::sleep(Duration::from_secs(1)).await;
    }

    // -- 3. One it does not -------------------------------------------------
    assert!(
        !env.statuses.iter().any(|s| s == UNREACHABLE_STATUS),
        "this workflow does have {UNREACHABLE_STATUS:?} after all ({:?}), so the refusal below \
         would be testing nothing -- pick a status it has not got",
        env.statuses
    );
    let row = write(
        &state,
        json!({
            "Transition": {
                "entity": format!("{JIRA}:{TRANSITIONED}"),
                "status": UNREACHABLE_STATUS
            }
        }),
    )
    .await;
    assert_eq!(
        row.state,
        WriteState::Refused,
        "a status the workflow does not have is refused, not retried for ever. The row came \
         back {:?} with detail {:?} and held_snapshot {:?}. `Held` here is never Jira's \
         answer -- it is the mirror moving between this write's snapshot and its flush, which \
         the wait above exists to rule out (#325); `Pending` is a fault the queue thinks will \
         pass, and its wait_reason says which.",
        row.state,
        row.detail,
        row.held_snapshot
    );
    let detail = row.detail.clone().unwrap_or_default();
    assert!(
        detail.contains(UNREACHABLE_STATUS),
        "the refusal names the status that was asked for: {detail:?}"
    );
    assert!(
        detail.contains(&to),
        "...and what the workflow does offer instead, which is what makes it actionable: \
         {detail:?}"
    );
    assert_eq!(
        env.status_at_jira(TRANSITIONED).await,
        to,
        "a refused transition must not have moved anything"
    );
    println!("SEEDED refused transition: {detail}");

    // -- 4. Create ----------------------------------------------------------
    //
    // The target is the **project** (`jira:PAY`), a container knobas does not
    // mirror -- which is what the queue's hold detection is built for: an
    // unmirrored container is `live: false` at queue time and at flush time
    // alike, so a create never holds.
    let title = format!(
        "{LITTER_LABEL}: filed by the knobas live suite (pid {})",
        std::process::id()
    );
    let scope = format!("project = {CREATE_PROJECT}");
    let before: std::collections::BTreeSet<String> = env.jql(&scope).await.into_iter().collect();
    let row = write(
        &state,
        json!({
            "CreateTicket": {
                "entity": format!("{JIRA}:{CREATE_PROJECT}"),
                "title": title,
                "body": "the batch job times out and the payouts are lost",
                "ticket_type": CREATE_TYPE
            }
        }),
    )
    .await;
    assert_eq!(row.state, WriteState::Sent, "{:?}", row.detail);

    // `Source::write` answers `()` -- widening it is a frozen-SPI change and no
    // criterion asks for one -- so the new key is found the way a person would
    // find it, and then **confirmed by a read of the issue itself**.
    //
    // Both halves are needed. JQL is an index read and Jira's index lags its
    // own writes, so the key may not be there yet; and the index also lags
    // *deletes*, so a key it hands back may be a ticket an earlier run's
    // cleanup already removed. Taking the newest key on trust would then read
    // back a 404. `GET /rest/api/2/issue/{key}` goes to the database, so a
    // ghost fails the summary check and the poll goes round again.
    let deadline = std::time::Instant::now() + INDEX_BUDGET;
    let created = 'found: loop {
        for key in env.jql(&scope).await {
            if before.contains(&key) {
                continue;
            }
            let (status, body) = env
                .api(
                    reqwest::Method::GET,
                    &format!("rest/api/2/issue/{key}?fields=summary"),
                    None,
                )
                .await;
            if status == 200 && body["fields"]["summary"] == title.as_str() {
                break 'found key;
            }
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the created ticket did not reach Jira's search index within {INDEX_BUDGET:?}"
        );
        tokio::time::sleep(Duration::from_secs(1)).await;
    };
    litter.created = Some(created.clone());
    // Labelled as soon as it exists, so a run that is *killed* after this line
    // still leaves something the next run's leftover clearing can find.
    let (status, labelled) = env
        .api(
            reqwest::Method::PUT,
            &format!("rest/api/2/issue/{created}"),
            Some(json!({ "update": { "labels": [{ "add": LITTER_LABEL }] } })),
        )
        .await;
    assert_eq!(status, 204, "labelling the created ticket: {labelled}");

    let filed = env
        .issue(&created, "summary,description,issuetype,project,reporter")
        .await;
    assert_eq!(filed["fields"]["summary"], title);
    assert_eq!(
        filed["fields"]["description"], "the batch job times out and the payouts are lost",
        "a create that dropped the description would be reported as a success"
    );
    assert_eq!(filed["fields"]["issuetype"]["name"], CREATE_TYPE);
    assert_eq!(filed["fields"]["project"]["key"], CREATE_PROJECT);
    assert_eq!(
        filed["fields"]["reporter"]["name"], env.user,
        "story 17: attributed to me"
    );
    println!("SEEDED created ticket: {created} ({title})");

    state.scheduler.shutdown().await;
    drop(litter);
}

/// **What the workflow offers, from each of its four states** (issue #498,
/// spec #491's stream 3).
///
/// The read the status select is offered, against the real thing. A mock can
/// only say the adapter reads `to.name` off the response it was handed; what it
/// cannot say is what a **Jira workflow** answers, and this suite's own header
/// records why that matters here more than usual: mockd's fixture workflow has
/// shape -- one move out of *To Do*, two out of *In Progress* -- and the seeded
/// project's has none, offering all four of its statuses from every one of
/// them, the issue's own included. An adapter certified only against mockd
/// would be certified against the wrong shape (ADR-0013).
///
/// **The ground truth is `seed-state.json`'s `jira.statuses`**, which the seed
/// read off `GET /rest/api/2/project/PAY/statuses` -- a different endpoint from
/// the one under test. So this is the workflow's own answer checked against the
/// project's own answer, not the read checked against itself.
///
/// **Four tickets, one per state, and the states are read off Jira rather than
/// assumed.** The fixture puts PAY-240 in *To Do*, PAY-231 *In Progress*,
/// PAY-228 *In Review* and PAY-219 *Done*; this reads each one's status from
/// the server and asserts the four are the four, because a run that found two
/// of them in the same column would otherwise quietly certify three states and
/// report four. That is also the leftover check: a killed run of the write
/// test above leaves PAY-240 moved, and `live_jira_seeded.rs`'s
/// `Seeded::clear_leftovers` is what puts it back.
///
/// **What this now witnesses that it could not before** (issue #522). Until the
/// seed grew a second project, this Jira's whole status list
/// (`GET /rest/api/2/status`) was exactly the four this workflow reaches, so
/// "the workflow's reply" and "every status the project has" were the same set
/// and the reads below could not tell them apart; the assertion here was an
/// equality, written to go red the day the list grew. `NARROW` -- the seed's
/// process-management project -- is what grew it, and the equality is now the
/// **proper superset** below: this instance has statuses PAY's workflow never
/// reaches, and every read below still answers PAY's four and none of them. So
/// the leaving-out is witnessed here, live, at the instance level.
///
/// What is *not* witnessed here is a workflow that narrows **within its own
/// project**, because PAY's still reaches all four of its statuses from every
/// one of them, and that is the seeded template's own shape rather than
/// something to work around.
/// [`the_reachable_transitions_read_answers_a_proper_subset_where_the_workflow_narrows`]
/// is where that lives, on `NARROW`, together with the refusal of a status
/// that exists in the project and is not reachable from where the ticket
/// stands.
///
/// **This test writes nothing.** Every call it makes is a `GET`, including the
/// refusal at the end -- an unreachable status never reaches the `POST`.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs testenv's seeded Jira: `just atlassian-live`"]
async fn the_reachable_transitions_read_answers_the_seeded_workflow_from_every_state() {
    use knobas_app::sources::crud;
    use knobas_source::{Source, SourceError, instance::SourceInstance};
    use knobas_sync::scheduler::AdapterRegistry;

    let env = env();
    let expected: std::collections::BTreeSet<String> = env.statuses.iter().cloned().collect();
    assert_eq!(
        expected.len(),
        4,
        "this workflow is the seeded template's four statuses; {:?} is not that, and every \
         assertion below is written against it",
        env.statuses
    );

    // **User and password, not a PAT**: every call here is a read, the adapter's
    // own live suite certifies this scheme, and it means this test mints and
    // revokes nothing. The wrong credential below is a *bearer token* for the
    // reason `live_jira_seeded.rs` spells out at length -- a wrong password
    // earns the seed's admin a Jira CAPTCHA lockout, and a token Jira cannot
    // resolve never reaches Seraph at all.
    let (state, _events) = app(
        "atlassian_live_reachable",
        &env,
        AuthMethod::UserPassword,
        &env.password,
    )
    .await;

    // **The premise the reads below narrow against, measured rather than
    // assumed.** `GET /rest/api/2/status`, a third endpoint listing every
    // status this Jira has at all, is what says the answers below are a
    // *subset* of what exists rather than a copy of it. This was an equality
    // until #522 -- the instance's whole list was exactly PAY's four -- and
    // what grew it is the seed's `NARROW` project, whose process-management
    // workflow brings `Open`, `Under Review`, `Approved`, `Cancelled` and
    // `Rejected` onto the instance. Every one of those is a status a read that
    // answered "every status the corpus has been seen to use" could leak into
    // PAY's select, and none of them appears in any answer below.
    //
    // The two halves are asserted separately because they fail for different
    // reasons: a missing PAY status means the seed or the template changed, an
    // instance with nothing else on it means `NARROW` is gone and the reads
    // below have quietly stopped witnessing anything.
    let (status, body) = env
        .api(reqwest::Method::GET, "rest/api/2/status", None)
        .await;
    assert_eq!(status, 200, "GET the instance's statuses: {body}");
    let on_the_instance: std::collections::BTreeSet<String> = body
        .as_array()
        .expect("a status array")
        .iter()
        .filter_map(|s| s["name"].as_str().map(str::to_owned))
        .collect();
    let narrowing: std::collections::BTreeSet<String> =
        env.narrowing.statuses.iter().cloned().collect();
    let never_reached: std::collections::BTreeSet<String> =
        on_the_instance.difference(&expected).cloned().collect();
    assert_eq!(
        on_the_instance,
        expected.union(&narrowing).cloned().collect(),
        "this Jira's statuses are this workflow's four ({expected:?}) and `{}`'s ({narrowing:?}) \
         and nothing else. Both halves matter: without the second the reads below cannot tell \
         the workflow's reply from every status the project has -- so if `{}` is gone, run \
         `./seed-atlassian-content.sh`. And an instance that grew a status neither project \
         explains is one this suite has stopped describing, which is what this equality is here \
         to say out loud (it replaced #498's, which said the instance had only the four)",
        env.narrowing.key,
        env.narrowing.key
    );
    assert!(
        !never_reached.is_empty(),
        "the narrowing project has to put statuses on this instance that this workflow never \
         reaches, or the reads below witness nothing: the instance has {on_the_instance:?} and \
         the workflow offers {expected:?}"
    );
    println!(
        "SEEDED this instance has {} statuses, {} of which this workflow never reaches: \
         {never_reached:?}",
        on_the_instance.len(),
        never_reached.len()
    );

    let mut seen: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for key in ["PAY-240", COMMENTED, "PAY-228", "PAY-219"] {
        let standing = env.status_at_jira(key).await;
        assert!(
            expected.contains(&standing),
            "{key} is in {standing:?}, which is not one of this project's statuses {:?}",
            env.statuses
        );
        assert!(
            seen.insert(standing.clone()),
            "two of the four tickets are in {standing:?}, so this run witnesses fewer than the \
             four states it claims -- a killed run of the write test above leaves PAY-240 \
             moved, and `live_jira_seeded.rs`'s Seeded::clear_leftovers puts it back"
        );

        let reachable = crud::reachable_transitions(
            &state.pool,
            &state.secrets,
            state.registry.as_ref(),
            &format!("{JIRA}:{key}"),
        )
        .await
        .unwrap_or_else(|error| panic!("the read must answer for {key}: {error}"));
        let answered: std::collections::BTreeSet<String> = reachable.iter().cloned().collect();
        println!("SEEDED {key} stands in {standing:?} and reaches {reachable:?}");
        assert_eq!(
            answered,
            expected,
            "from {standing:?} this workflow reaches every one of its statuses ({:?}) and none of \
             the {} the instance has besides ({never_reached:?}); the read answered {reachable:?}",
            env.statuses,
            never_reached.len()
        );
    }
    assert_eq!(
        seen, expected,
        "the four tickets have to stand in the four states for this to be a read from each of \
         them"
    );

    // -- the wrong token ----------------------------------------------------
    //
    // A bearer token this Jira cannot resolve. The read **errors**, and that is
    // the whole claim: the shell's fallback is keyed on a rejection, so a read
    // that answered an empty list here would put an empty select on screen and
    // call it the workflow's answer.
    let (refused_state, _refused_events) = app(
        "atlassian_live_reachable_refused",
        &env,
        AuthMethod::Pat,
        &format!("revoked-{}", std::process::id()),
    )
    .await;
    let error = crud::reachable_transitions(
        &refused_state.pool,
        &refused_state.secrets,
        refused_state.registry.as_ref(),
        &format!("{JIRA}:{COMMENTED}"),
    )
    .await
    .expect_err("a token Jira cannot resolve must not read a workflow");
    // **Which** error, not merely that there was one. `unauthorized` is what
    // puts *Re-enter* in front of the reader (ADR-0004), and it is not the
    // obvious answer on this server: the same unresolvable token on
    // `GET /rest/api/2/search` proceeds *anonymously* and answers 200 with
    // `total: 0`, which is the hazard
    // `a_revoked_pat_reaches_the_credential_health_surface_and_the_mirror_survives`
    // exists for. `/transitions` is a clean 401, and a regression to
    // `not_found` or `invalid` would pass a bare `expect_err`.
    assert!(
        matches!(
            &error,
            knobas_app::sources::SourcesError::Source(SourceError::Unauthorized { .. })
        ),
        "a token this Jira cannot resolve is an unauthorized read, got {error:?}"
    );
    println!("SEEDED the read under a token Jira cannot resolve: {error}");

    // -- and the write side is unchanged ------------------------------------
    //
    // The point of the whole ticket: this read narrows what is *offered*, and
    // it replaces nothing. The adapter still resolves the status it is handed
    // against the source's own answer at write time and still refuses by name
    // with what the workflow does offer. Asserted at the adapter rather than
    // through the queue because the queue's half is already this file's
    // `the_three_write_ops_go_through_the_queue_and_come_back_from_jira`, and
    // because a refusal is the one write that reaches no `POST`.
    assert!(
        !env.statuses.iter().any(|s| s == UNREACHABLE_STATUS),
        "this workflow does have {UNREACHABLE_STATUS:?} after all ({:?}), so the refusal below \
         would be testing nothing",
        env.statuses
    );
    let source: Box<dyn Source> = Registry::builtin()
        .build(SourceInstance {
            id: JIRA.to_owned(),
            kind: "jira".to_owned(),
            display_name: "Tidewater Jira (seeded)".to_owned(),
            base_url: env.url.clone(),
            auth: Some(AuthMethod::UserPassword),
            secret: Some(env.password.clone()),
            account: None,
            config: json!({ "username": env.user }),
        })
        .unwrap_or_else(|error| panic!("the adapter must build against the seeded URL: {error:?}"));
    let standing = env.status_at_jira(COMMENTED).await;
    let refused = source
        .write(knobas_source::WriteOp::Transition {
            entity: format!("{JIRA}:{COMMENTED}"),
            status: UNREACHABLE_STATUS.to_owned(),
        })
        .await;
    let Err(SourceError::Protocol { message, .. }) = &refused else {
        panic!("a status this workflow does not have must be refused, got {refused:?}");
    };
    assert!(
        message.contains(UNREACHABLE_STATUS),
        "the refusal names the status that was asked for: {message}"
    );
    assert!(
        env.statuses.iter().any(|s| message.contains(s)),
        "...and carries what the workflow does offer, which is what makes it actionable: \
         {message}"
    );
    assert_eq!(
        env.status_at_jira(COMMENTED).await,
        standing,
        "a refused transition must not have moved anything"
    );
    println!("SEEDED the write-side refusal, unchanged: {message}");

    state.scheduler.shutdown().await;
    refused_state.scheduler.shutdown().await;
}

/// **A workflow that narrows: the read answers a proper subset, and the write
/// side refuses a status the project has** (issue #522, the deputy's ruling of
/// 2026-09-08 on #498).
///
/// The direction
/// [`the_reachable_transitions_read_answers_the_seeded_workflow_from_every_state`]
/// cannot see from inside PAY. Both Tidewater projects run Jira's *Simplified*
/// workflow, which reaches all four of its statuses from every one of them, so
/// inside PAY "what the workflow offers" and "what the project has" are the
/// same list and a read that had never looked at a workflow would answer
/// identically. `NARROW` is the seed's second project, from Jira Core's
/// process-management template, and it does what a real Jira does all day:
/// from `Open` it offers exactly one of its seven statuses.
///
/// ADR-0013 is why this is a live test and not a mockd one. mockd's fixture
/// workflow already has a narrowing shape, and a mock certifies nothing: what
/// it cannot say is that a **Jira workflow** answers this way, or that the
/// adapter's `to.name` reading survives a template whose transition names are
/// `Start Progress` and `Ready For Review` rather than the statuses they land
/// on. Every row of the table on [`NARROW_FIRST_STATE`] was walked on the real
/// server; this test stands a ticket in the first two of them, so those two are
/// the rows a changed template would be caught in. The other five are a
/// measurement recorded there, asserted by nothing.
///
/// **Three claims, and they fail for different reasons.**
///
/// 1. From `Open` the read answers exactly [`NARROW_FROM_FIRST`], and that is
///    a *proper* subset of the statuses this project's workflow has for the
///    type the ticket is filed as -- the denominator read off
///    `GET /rest/api/2/project/NARROW/statuses` by the seed, a different
///    endpoint from the `/transitions` one under test.
/// 2. The write side refuses [`NARROW_UNREACHABLE_FROM_FIRST`] **by name**, and
///    that status is asserted to be one the project has. This is the refusal
///    the read exists to stop a person ever meeting, and it is the one
///    [`UNREACHABLE_STATUS`] cannot witness: `"Blocked"` exists nowhere here,
///    so refusing it needs no workflow.
/// 3. The answer *moves with the ticket*: standing the same ticket in
///    `In Progress` changes it to [`NARROW_FROM_SECOND`], a different, larger,
///    still proper subset. A read that answered the project's statuses, or the
///    corpus's, would answer the same thing twice.
///
/// **The ticket is this test's own, and it is deleted.** `NARROW` holds no
/// issues between runs, deliberately: the adapter suite that runs before this
/// one syncs the whole instance and asserts the mirror is exactly
/// `seed-state.json`'s `jira.issues`, and its `Seeded::clear_leftovers` deletes
/// every issue on the instance the seed did not create -- which is also what
/// clears this ticket after a run that was *killed* rather than failed, since
/// only an unwinding process reaches a `Drop`. The ticket also carries
/// [`LITTER_LABEL`] from the moment it exists, so this suite's own
/// [`Env::clear_leftovers`] finds it too and the file clears after itself
/// rather than relying on the recipe's order.
///
/// Every assertion here is about `NARROW`, and what this test writes *there*
/// is one ticket: filed, labelled, moved once and deleted. It is *not* true
/// that it touches nothing else, and neither of the two things that do is an
/// assertion. [`Env::clear_leftovers`] below is a **write** and is
/// instance-wide rather than scoped to `NARROW`: it deletes every issue on
/// this Jira carrying [`LITTER_LABEL`], which is the same sweep the two write
/// tests above run and is instance-wide for the same reason -- a killed run's
/// litter is not scoped either. And [`app`] starts a real scheduler over a
/// scratch database, whose first full sync reads every project on the
/// instance because this source is unscoped; those reads land in a database
/// this test throws away.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs testenv's seeded Jira: `just atlassian-live`"]
async fn the_reachable_transitions_read_answers_a_proper_subset_where_the_workflow_narrows() {
    use knobas_app::sources::crud;
    use knobas_source::{Source, instance::SourceInstance};
    use knobas_sync::scheduler::AdapterRegistry;

    let env = env();
    let project: std::collections::BTreeSet<String> =
        env.narrowing.statuses.iter().cloned().collect();
    let from_first: std::collections::BTreeSet<String> =
        NARROW_FROM_FIRST.iter().map(|s| (*s).to_owned()).collect();
    let from_second: std::collections::BTreeSet<String> =
        NARROW_FROM_SECOND.iter().map(|s| (*s).to_owned()).collect();

    // The premise, against the seed's record rather than against the endpoint
    // under test: this project has statuses its first state cannot reach, and
    // the one the refusal below asks for is one of them. Without this a
    // template that had quietly become all-to-all would leave every assertion
    // below true and none of them about narrowing.
    for (what, wanted) in [
        ("from the first state", &from_first),
        ("from the second", &from_second),
    ] {
        assert!(
            wanted.is_subset(&project) && wanted.len() < project.len(),
            "what this workflow offers {what} ({wanted:?}) has to be a proper subset of the \
             {} statuses `{}` has ({project:?}) -- otherwise this test witnesses no narrowing. \
             The statuses come from seed-state.json's jira.narrowing; re-run \
             `./seed-atlassian-content.sh`",
            project.len(),
            env.narrowing.key
        );
    }
    assert!(
        project.contains(NARROW_UNREACHABLE_FROM_FIRST)
            && !from_first.contains(NARROW_UNREACHABLE_FROM_FIRST),
        "{NARROW_UNREACHABLE_FROM_FIRST:?} has to be a status `{}` HAS ({project:?}) and does \
         NOT offer from {NARROW_FIRST_STATE:?} ({from_first:?}) -- a status the project lacks is \
         the refusal `{UNREACHABLE_STATUS}` already witnesses, and a reachable one is no refusal \
         at all",
        env.narrowing.key
    );

    // A run that was *killed* between the create and the `Drop` below left its
    // ticket standing; this is what takes it away, by the label the create
    // puts on. Read-only in the ordinary case, and instance-wide rather than
    // scoped to `NARROW`, which is what makes it the same sweep the two write
    // tests above run.
    env.clear_leftovers().await;

    let (state, _events) = app(
        "atlassian_live_narrowing",
        &env,
        AuthMethod::UserPassword,
        &env.password,
    )
    .await;
    let mut litter = Litter::default();

    // -- a ticket of this suite's own, in the workflow's first state ---------
    //
    // Filed rather than seeded: see the header. The summary names the suite so
    // that a person looking at the server can tell whose it is, and it is
    // labelled as soon as it exists so that [`Env::clear_leftovers`] above can
    // find it after a run that never reached a `Drop`.
    let (status, created) = env
        .api(
            reqwest::Method::POST,
            "rest/api/2/issue",
            Some(json!({
                "fields": {
                    "project": { "key": env.narrowing.key },
                    "summary": "knobas live suite: the reachable-transition read, narrowed",
                    "issuetype": { "name": env.narrowing.issue_type },
                }
            })),
        )
        .await;
    assert_eq!(
        status, 201,
        "filing a ticket in {}: {created}",
        env.narrowing.key
    );
    let key = created["key"]
        .as_str()
        .unwrap_or_else(|| panic!("a created issue has a key: {created}"))
        .to_owned();
    litter.created = Some(key.clone());
    let entity = format!("{JIRA}:{key}");
    let (status, labelled) = env
        .api(
            reqwest::Method::PUT,
            &format!("rest/api/2/issue/{key}"),
            Some(json!({ "update": { "labels": [{ "add": LITTER_LABEL }] } })),
        )
        .await;
    assert_eq!(status, 204, "labelling {key}: {labelled}");

    let standing = env.status_at_jira(&key).await;
    assert_eq!(
        standing, NARROW_FIRST_STATE,
        "a ticket filed into `{}` starts in this workflow's first state; {key} started in \
         {standing:?}",
        env.narrowing.key
    );

    // -- 1. the read narrows ------------------------------------------------
    let read = |entity: String| {
        let state = &state;
        async move {
            crud::reachable_transitions(
                &state.pool,
                &state.secrets,
                state.registry.as_ref(),
                &entity,
            )
            .await
            .unwrap_or_else(|error| panic!("the read must answer for {entity}: {error}"))
        }
    };
    let reachable = read(entity.clone()).await;
    let answered: std::collections::BTreeSet<String> = reachable.iter().cloned().collect();
    let left_out: std::collections::BTreeSet<&String> = project.difference(&answered).collect();
    assert_eq!(
        answered, from_first,
        "{key} stands in {standing:?}, from which this workflow offers {from_first:?}; the read \
         answered {reachable:?}"
    );
    assert!(
        !left_out.is_empty(),
        "the read has to leave something out, or it is not narrowing: it answered {answered:?} \
         and `{}` has {project:?}",
        env.narrowing.key
    );
    println!(
        "SEEDED {key} stands in {standing:?} and reaches {reachable:?} -- {} of the {} statuses \
         `{}` has; left out: {left_out:?}",
        answered.len(),
        project.len(),
        env.narrowing.key
    );

    // -- 2. the write side refuses a status the project has -----------------
    //
    // At the adapter, for the reason the PAY refusal above is: a refusal is the
    // one write that reaches no `POST`, and the queue's half of the story is
    // already `the_three_write_ops_go_through_the_queue_and_come_back_from_jira`.
    let source: Box<dyn Source> = Registry::builtin()
        .build(SourceInstance {
            id: JIRA.to_owned(),
            kind: "jira".to_owned(),
            display_name: "Tidewater Jira (seeded)".to_owned(),
            base_url: env.url.clone(),
            auth: Some(AuthMethod::UserPassword),
            secret: Some(env.password.clone()),
            account: None,
            config: json!({ "username": env.user }),
        })
        .unwrap_or_else(|error| panic!("the adapter must build against the seeded URL: {error:?}"));
    let refused = source
        .write(knobas_source::WriteOp::Transition {
            entity: entity.clone(),
            status: NARROW_UNREACHABLE_FROM_FIRST.to_owned(),
        })
        .await;
    let Err(knobas_source::SourceError::Protocol { message, .. }) = &refused else {
        panic!(
            "{NARROW_UNREACHABLE_FROM_FIRST:?} is a status `{}` has and {key} cannot reach from \
             {standing:?}, so the write must be refused; got {refused:?}",
            env.narrowing.key
        );
    };
    assert!(
        message.contains(NARROW_UNREACHABLE_FROM_FIRST),
        "the refusal names the status that was asked for: {message}"
    );
    assert!(
        from_first.iter().all(|s| message.contains(s)),
        "...and carries what the workflow does offer from here ({from_first:?}), which is what \
         makes it actionable: {message}"
    );
    assert_eq!(
        env.status_at_jira(&key).await,
        standing,
        "a refused transition must not have moved anything"
    );
    println!("SEEDED the refusal of a status this project has: {message}");

    // -- 3. the answer moves with the ticket --------------------------------
    //
    // Moved through Jira's own endpoint rather than through the queue: this is
    // the setup for the second read, not the thing under test, and the write
    // path's own live witness is the PAY test above.
    let (status, offered) = env
        .api(
            reqwest::Method::GET,
            &format!("rest/api/2/issue/{key}/transitions"),
            None,
        )
        .await;
    assert_eq!(status, 200, "transitions of {key}: {offered}");
    let id = offered["transitions"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|t| t["to"]["name"].as_str() == Some(NARROW_SECOND_STATE))
        .and_then(|t| t["id"].as_str().map(str::to_owned))
        .unwrap_or_else(|| panic!("{key} has no transition to {NARROW_SECOND_STATE:?}: {offered}"));
    let (status, moved) = env
        .api(
            reqwest::Method::POST,
            &format!("rest/api/2/issue/{key}/transitions"),
            Some(json!({ "transition": { "id": id } })),
        )
        .await;
    assert_eq!(
        status, 204,
        "moving {key} to {NARROW_SECOND_STATE:?}: {moved}"
    );
    let standing = env.status_at_jira(&key).await;
    assert_eq!(standing, NARROW_SECOND_STATE, "{key} did not move");

    let reachable = read(entity).await;
    let answered: std::collections::BTreeSet<String> = reachable.iter().cloned().collect();
    assert_eq!(
        answered, from_second,
        "{key} now stands in {standing:?}, from which this workflow offers {from_second:?}; the \
         read answered {reachable:?}"
    );
    assert_ne!(
        from_second, from_first,
        "the two states have to offer different sets, or standing the ticket somewhere else \
         witnessed nothing"
    );
    println!("SEEDED {key} now stands in {standing:?} and reaches {reachable:?}");

    state.scheduler.shutdown().await;
    drop(litter);
}

/// **A day's blocks logged to PAY-231, at Jira and back through the mirror**
/// (issue #280, M3.1).
///
/// The claim M3.1 makes is not "a POST returns 201". It is that a person can
/// stop a timer and have the afternoon end up on the ticket, that knobas'
/// copy can still name what it sent afterwards, and that the next sync shows
/// the same worklog coming back. Each of those is a different piece of
/// machinery and the middle one is unrecoverable if it is wrong: Jira names a
/// worklog exactly once, in the answer to the POST, and the local copy is
/// stamped from that answer inside the same statement that settles the write.
/// If the settle dropped it there is nothing to re-read it from.
///
/// Four assertions, in the order the failure would matter:
///
/// 1. **the write settled `sent`** through the queue, like every other write;
/// 2. **the worklog is on PAY-231 at Jira**, with the seconds, the comment and
///    the start knobas asked for -- read back over REST as the seed's admin,
///    so it is the server's account and not knobas';
/// 3. **the copy carries Jira's id**, and it is the id of the row that was
///    just read back;
/// 4. **the next sync's mirrored payload carries it**, which is what makes a
///    worklog visible to everything downstream of the mirror.
///
/// The blocks are **two with a gap between them**, because that is the fixture
/// that can tell "the time worked" from "the window it sat in": 90 minutes and
/// 60 minutes inside a four-and-a-half-hour span. A draft that logged the span
/// would put 4h30m on somebody's timesheet.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs testenv's seeded Jira: `just atlassian-live`"]
async fn a_days_work_is_logged_to_pay_231_and_comes_back_in_the_mirror() {
    use knobas_core::write_queue::WriteState;

    let env = env();
    env.clear_leftovers().await;
    let mut litter = Litter::default();
    // A personal access token, for the reason the write test above gives: it
    // is the credential a real deployment configures, and M3.1's worklog is
    // the write it was chosen for.
    let pat = env.pat().await;
    let (state, _events) = app("atlassian_live_worklog", &env, AuthMethod::Pat, &pat.raw).await;

    sync(&state, JIRA).await;
    let ticket = format!("{JIRA}:{COMMENTED}");
    assert!(
        mirrored(&state.pool).await.contains(&ticket),
        "the queue snapshots its target at queue time, so {COMMENTED} has to be in \
         the mirror before a write against it can go"
    );

    // A day whose whole 09:00--13:30 window is in the **past**, wherever in the
    // day this suite happens to run: before 14:00 UTC that is yesterday. A
    // worklog dated in the future is not what this is testing, and a run at
    // 08:00 would otherwise file one.
    let now = chrono::Utc::now();
    let day = if now.time() < chrono::NaiveTime::from_hms_opt(14, 0, 0).expect("14:00") {
        now.date_naive().pred_opt().expect("yesterday exists")
    } else {
        now.date_naive()
    };
    let at = |hour: u32, minute: u32| {
        day.and_hms_opt(hour, minute, 0)
            .expect("a time of day")
            .and_utc()
    };
    for (from, to) in [(at(9, 0), at(10, 30)), (at(12, 30), at(13, 30))] {
        sqlx::query(
            "insert into knobas.block (started_at, ended_at, entity_id, kind)
             values ($1, $2, $3, 'manual')",
        )
        .bind(from)
        .bind(to)
        .bind(&ticket)
        .execute(&state.pool)
        .await
        .expect("a block is written");
    }

    let draft =
        knobas_app::time::worklog::draft(&state.pool, state.registry.as_ref(), &ticket, day, 0)
            .await
            .expect("the draft is readable")
            .expect("a Jira ticket with unlogged blocks has a draft");
    assert_eq!(
        draft.seconds,
        150 * 60,
        "two and a half hours were worked inside a four-and-a-half-hour window"
    );
    assert_eq!(draft.started_at, at(9, 0));

    let comment = format!(
        "{LITTER_LABEL}: knobas logged this through the write queue (pid {})",
        std::process::id()
    );
    let before = env.issue(COMMENTED, "worklog").await["fields"]["worklog"]["total"]
        .as_i64()
        .unwrap_or(0);

    let logged = knobas_app::time::worklog::log(
        &state,
        &ticket,
        day,
        0,
        draft.started_at,
        draft.seconds,
        &comment,
    )
    .await
    .expect("the day is logged");

    // Owned before anything is asserted: from here the worklog exists at Jira,
    // so a failing assertion below must still leave the guard something to
    // delete.
    if let Some(id) = logged.remote_id.clone() {
        litter.worklog = Some((COMMENTED.to_owned(), id));
    }

    // 1. Through the queue, settled.
    let write_id = logged
        .write_queue_id
        .expect("a logged worklog names the write that carries it");
    let row = knobas_core::write_queue::get(&state.pool, write_id)
        .await
        .expect("the queue row is readable")
        .expect("the row `log` queued");
    assert_eq!(row.state, WriteState::Sent, "{:?}", row.detail);
    assert_eq!(row.op, "log_work");

    // 2. On the ticket at Jira, in Jira's own account of it.
    let worklogs = env.issue(COMMENTED, "worklog").await["fields"]["worklog"].clone();
    assert_eq!(
        worklogs["total"].as_i64(),
        Some(before + 1),
        "the worklog is on the ticket at Jira: {worklogs}"
    );
    let at_jira = worklogs["worklogs"]
        .as_array()
        .and_then(|all| {
            all.iter()
                .find(|w| w["id"].as_str() == logged.remote_id.as_deref())
        })
        .cloned()
        .unwrap_or_else(|| panic!("the worklog knobas named is not on the ticket: {worklogs}"));
    assert_eq!(at_jira["timeSpentSeconds"].as_i64(), Some(150 * 60));
    assert_eq!(at_jira["comment"], comment.as_str());
    assert_eq!(
        at_jira["author"]["name"], env.user,
        "story 17: the source attributes the write to the credential's own account"
    );
    // `started` is the field with a *format* rather than a value, and Jira
    // refuses every spelling but `yyyy-MM-dd'T'HH:mm:ss.SSSZ`. Compared as an
    // instant, because Jira echoes it back in the instance's own offset.
    let started_back = at_jira["started"]
        .as_str()
        .and_then(|raw| chrono::DateTime::parse_from_str(raw, "%Y-%m-%dT%H:%M:%S%.3f%z").ok())
        .unwrap_or_else(|| panic!("Jira's own `started` did not parse: {at_jira}"));
    assert_eq!(
        started_back.with_timezone(&chrono::Utc),
        at(9, 0),
        "the worklog is filed at the moment the work began, not at the moment it \
         was logged"
    );

    // 3. ...and the copy names it.
    //
    //     Cleared by #347's sweep rather than rewritten, and the reason is
    //     worth writing down: this is a restatement, not a witness. `None`
    //     here dies at the `unwrap_or_else(|| panic!(...))` above -- the
    //     lookup that found `at_jira` matched on `logged.remote_id`, and a
    //     `None` matches no worklog Jira answered with a string id. The line
    //     stays because the claim is worth saying out loud where the reader
    //     is; what carries it is the panic above.
    assert!(
        logged.remote_id.is_some(),
        "the settle is the only moment Jira's worklog id exists, and the copy \
         has nothing to point at without it"
    );
    println!(
        "SEEDED worklog {} on {COMMENTED}: {}s",
        logged.remote_id.clone().unwrap_or_default(),
        logged.seconds
    );

    // 4. And the next sync brings it back into the mirror's payload, which is
    //    what everything downstream of the mirror reads (§4.1: the record is
    //    verbatim).
    sync(&state, JIRA).await;
    let payload: serde_json::Value =
        sqlx::query_scalar("select payload from sync.live_item where entity_id = $1")
            .bind(&ticket)
            .fetch_one(&state.pool)
            .await
            .expect("the mirrored ticket is readable");
    let mirrored_ids: Vec<String> = payload["fields"]["worklog"]["worklogs"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|w| w["id"].as_str().map(str::to_owned))
        .collect();
    assert!(
        mirrored_ids.contains(&logged.remote_id.clone().unwrap_or_default()),
        "the worklog knobas wrote is not in the next sync's mirrored payload: \
         {mirrored_ids:?}"
    );

    state.scheduler.shutdown().await;
    drop(litter);
}

/// **A day's real work, listed under *yesterday* with its refs** (issue #288,
/// M3.3).
///
/// The digest is a **join** over three producers that reach the database by
/// three unrelated routes, and this is the only place all three are real at
/// once:
///
/// 1. **`knobas.worklog`** -- the local copy of an afternoon logged to Jira
///    through the write queue;
/// 2. **the activity stream** -- the `queued` line the queue writes when a
///    person comments, carrying the op the digest reports as the verb;
/// 3. **the mirror** -- `sync.live_item`, an item the sources say is *mine*,
///    which on Jira means §4.1's `author`, which the adapter maps from the
///    **assignee** (`map.rs`). So this test borrows a seeded ticket by
///    assigning it to the suite's own account, and gives it back.
///
/// Every one of the three can be green in `tests/standup_ipc.rs` while this
/// join is broken, and the third is the one no scratch fixture can settle: it
/// rests on what the *adapter* puts in `author`, against a real server, for
/// the account the source is really configured as.
///
/// **Asked for tomorrow, so today is *yesterday*.** The rule under test is
/// "the newest day before the given date with any of my activity", and the
/// only day this suite can put real work on is the day it runs. Asking for
/// tomorrow's digest makes today the newest earlier day, which is the same
/// question a Monday morning asks of Friday.
///
/// **And one thing that must not be on it**: PAY-231 is the seed's, assigned
/// to `mara`, and the suite is somebody else. Story 63 -- *the digest
/// describes me only* -- is asserted here against a real corpus rather than
/// against a fixture, which is the one place a mirror read that forgot whose
/// day this is would show up.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs testenv's seeded Jira: `just atlassian-live`"]
async fn a_seeded_days_work_is_what_the_digest_lists_under_yesterday() {
    use knobas_core::write_queue::WriteState;

    let env = env();
    env.clear_leftovers().await;
    let mut litter = Litter::default();
    let pat = env.pat().await;
    let (state, _events) = app("atlassian_live_digest", &env, AuthMethod::Pat, &pat.raw).await;

    sync(&state, JIRA).await;
    let borrowed = format!("{JIRA}:{TRANSITIONED}");
    let seeded = format!("{JIRA}:{COMMENTED}");
    let held = mirrored(&state.pool).await;
    for id in [&borrowed, &seeded] {
        assert!(held.contains(id), "{id} is not in the mirror: {held:?}");
    }

    // -- the day, and the work on it ----------------------------------------
    //
    // The window is bounded below by this morning's own midnight, so the
    // worklog is never dated before the day it is being logged on, and above
    // by `now`, so it is never dated in the future -- a run at 00:30 and a run
    // at 23:30 both file an hour that is inside today and already past.
    let now = chrono::Utc::now();
    let day = now.date_naive();
    let midnight = day.and_hms_opt(0, 0, 0).expect("midnight").and_utc();
    let ended = now - Duration::from_secs(60);
    let started = std::cmp::max(ended - Duration::from_secs(3600), midnight);
    assert!(
        started < ended,
        "this run is inside the first minute of the day; there is no past hour to log"
    );

    // 1. An afternoon on PAY-240, through the write queue.
    sqlx::query(
        "insert into knobas.block (started_at, ended_at, entity_id, kind)
         values ($1, $2, $3, 'manual')",
    )
    .bind(started)
    .bind(ended)
    .bind(&borrowed)
    .execute(&state.pool)
    .await
    .expect("a block is written");

    let draft =
        knobas_app::time::worklog::draft(&state.pool, state.registry.as_ref(), &borrowed, day, 0)
            .await
            .expect("the draft is readable")
            .expect("a Jira ticket with an unlogged block has a draft");
    let note = format!(
        "{LITTER_LABEL}: the digest suite logged this (pid {})",
        std::process::id()
    );
    let logged = knobas_app::time::worklog::log(
        &state,
        &borrowed,
        day,
        0,
        draft.started_at,
        draft.seconds,
        &note,
    )
    .await
    .expect("the day is logged");
    if let Some(id) = logged.remote_id.clone() {
        litter.worklog = Some((TRANSITIONED.to_owned(), id));
    }

    // 2. A comment on the same ticket, through the same queue.
    let body = format!(
        "{LITTER_LABEL}: the digest suite commented (pid {})",
        std::process::id()
    );
    let row = write(
        &state,
        json!({ "Comment": { "entity": borrowed, "body": body } }),
    )
    .await;
    let comments = env.issue(TRANSITIONED, "comment").await["fields"]["comment"].clone();
    if let Some(id) = comments["comments"]
        .as_array()
        .and_then(|all| all.last())
        .and_then(|posted| posted["id"].as_str())
    {
        litter.comment = Some((TRANSITIONED.to_owned(), id.to_owned()));
    }
    assert_eq!(row.state, WriteState::Sent, "{:?}", row.detail);

    // 3. The ticket becomes *mine* at the source, which is what the mirror
    //    half reads. Owned before the assertion, so a failure below still
    //    gives it back.
    let was = env.issue(TRANSITIONED, "assignee").await["fields"]["assignee"]["name"]
        .as_str()
        .map(str::to_owned);
    let (status, answered) = env
        .api(
            reqwest::Method::PUT,
            &format!("rest/api/2/issue/{TRANSITIONED}/assignee"),
            Some(json!({ "name": env.user })),
        )
        .await;
    litter.assigned = Some((TRANSITIONED.to_owned(), was.clone()));
    assert_eq!(
        status, 204,
        "assigning {TRANSITIONED} to the suite: {answered}"
    );
    // Jira's search is a Lucene index updated asynchronously, and the adapter
    // enumerates through it -- so a sync fired the instant the PUT answers 204
    // mirrors the assignee as it was, and the digest's mirror half then
    // correctly reports a ticket that is still somebody else's. Wait for the
    // index to agree, *then* sync.
    env.indexed(
        &format!("key = {TRANSITIONED} AND assignee = \"{}\"", env.user),
        TRANSITIONED,
    )
    .await;
    // -- and now wait for the *mirror* to have caught up ---------------------
    //
    // The precondition the digest's mirror half needs is "`sync.live_item`
    // holds this ticket as the suite's", and an **ordinary sync** delivers it.
    //
    // It did not use to, and this is where that showed. The comment above ran
    // through the write queue, which fires a sync as soon as the write lands
    // (`knobas_app::sources::write_queue`'s `refresh`), and the assignment
    // below it landed inside the same second -- 88 ms after, measured. Jira's
    // `/search` reports `updated` to the second, so to a run those were one
    // version of the ticket, and the cursor's `(key, updated)` pair recognised
    // the reassignment as something it had already delivered and dropped it.
    // For ever: `updated` never moves again on its own. This fixture reached
    // for `backfill` to get round it, which hid a real bug behind a fixture.
    // Issue #345 fixed it in the cursor -- `seen` recognises a **record**, by
    // fingerprint, not a timestamp -- so a plain sync is enough again, and
    // asking for one here is what keeps that fixed. (*Fingerprint*, never
    // *digest*: in this file `digest` is the standup's three lists.)
    //
    // Still a convergence loop rather than a single sync, and for the reason it
    // always had: Jira's search index is asynchronous (#325), so the run that
    // first sees the write is not necessarily the next one. It fails loudly
    // with what the mirror actually held.
    let mut attributed_to = None;
    for attempt in 0..5 {
        sync(&state, JIRA).await;
        attributed_to = sqlx::query_scalar::<_, Option<String>>(
            "select author from sync.live_item where entity_id = $1",
        )
        .bind(&borrowed)
        .fetch_one(&state.pool)
        .await
        .expect("the borrowed ticket is mirrored");
        if attributed_to.as_deref() == Some(env.user.as_str()) {
            if attempt > 0 {
                println!(
                    "SEEDED mirror caught up with the assignment on sync {}",
                    attempt + 1
                );
            }
            break;
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    assert_eq!(
        attributed_to.as_deref(),
        Some(env.user.as_str()),
        "five ordinary syncs and the mirror still does not hold {TRANSITIONED} as the suite's, \
         so the digest cannot attribute it. The write itself landed (its 204 is asserted \
         above) and the search index agreed (the wait above returned), so what did not happen \
         is the delivery: the assignment shares a second with the comment's write, which is \
         the record #345's cursor fingerprint exists to tell apart."
    );

    // -- and what the digest makes of it ------------------------------------
    let digest = knobas_app::commands::entity::standup_digest_inner(
        &state.pool,
        state.registry.as_ref(),
        chrono::Utc::now(),
        day_window(day.succ_opt().expect("tomorrow exists")),
        &[day_window(day)],
    )
    .await
    .expect("the digest reads");

    assert_eq!(
        digest.yesterday_day,
        Some(day),
        "today is the newest day before tomorrow with any of this account's work"
    );
    let listed: Vec<(Option<&str>, &str, &str)> = digest
        .yesterday
        .iter()
        .map(|line| {
            (
                line.entity_id.as_deref(),
                line.source.as_str(),
                line.verb.as_str(),
            )
        })
        .collect();
    for verb in ["log_work", "comment", "attributed"] {
        assert!(
            listed.contains(&(Some(borrowed.as_str()), JIRA, verb)),
            "no {verb} line for {borrowed} under yesterday: {listed:?}"
        );
    }
    for line in &digest.yesterday {
        assert!(
            line.entity_id.is_some(),
            "every line on this list has an item to open: {line:?}"
        );
        assert!(
            !line.reason.trim().is_empty(),
            "a line whose provenance cannot be shown is not shippable: {line:?}"
        );
    }
    // Story 63, and **what it does not witness**, named by #347's sweep. A
    // digest line is reached by two filters at once -- whose the item is, and
    // whether the day is in the window -- and this negative only fails when
    // *both* would have let {COMMENTED} through. PAY-231 is `mara`'s, which is
    // the filter under test; it is also a ticket this suite's other tests
    // write to, so whether its `updated` falls inside today's window is the
    // seed's business and not this test's. A mirror read that forgot whose day
    // this is is therefore caught here only on a run where PAY-231 moved
    // today. Closing that would mean this test writing to a ticket it exists
    // to see excluded, which would leave the exclusion resting on the write it
    // just made rather than on the fixture.
    assert!(
        !listed.iter().any(|(id, _, _)| *id == Some(seeded.as_str())),
        "{COMMENTED} is assigned to somebody else and its work is theirs: {listed:?}"
    );
    println!(
        "SEEDED digest for {day}: {} lines under yesterday",
        digest.yesterday.len()
    );

    state.scheduler.shutdown().await;
    drop(litter);
}

/// **The connection note reaches `test_source`, on a draft and on a saved
/// source (#326).** The draft is the Add-source dialog's call; the draft
/// naming a saved source with no typed secret is a row's *Test*. Both answer
/// with the Epic Link clause -- the one thing this Jira says that nothing
/// else on the report does -- and the id in it is asserted by **containment**
/// and never by value: it is minted per instance run, and `customfield_10101`
/// and `customfield_10109` have both been measured from one seed script.
///
/// Which arm: [`app`] configures the source with the username alone and no
/// `epic_link_field`, so both answers are the *found but not configured* arm
/// -- the case the Add-source dialog closes for a source being *created* and
/// the one a saved row can still be in, which is what the row's *Test* is
/// for. The *configured* arm -- the one every source created through the
/// dialog ends up in, because the dialog fills the field from `discovered` --
/// is [`test_source_answers_the_configured_arm_once_the_saved_source_names_the_field`]'s
/// (#381), on the same kind of row once its config names the field; at the
/// adapter it was already `tests/field_discovery.rs`'s and the adapter's own
/// live suite's. So two of the three arms reach `test_source` in a test here;
/// the no-field arm has no real product to witness it, and is the adapter's
/// unit test's alone.
///
/// That the call **writes nothing** is `tests/sources_crud.rs`'s claim over
/// the mock; it is not re-asserted on the row here, because the scheduler
/// [`app`] starts may run the source on its own clock and write a verdict of
/// its own.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs testenv's seeded Jira: `just atlassian-live`"]
async fn test_source_carries_the_epic_link_note_for_a_draft_and_for_a_saved_source() {
    use knobas_app::sources::{SecretInput, SourceDraft, crud};

    let env = env();
    let pat = env.pat().await;
    let (state, _events) = app("atlassian_live_note", &env, AuthMethod::Pat, &pat.raw).await;

    // 1. A draft, as the Add-source dialog sends one: nothing saved, the
    //    typed secret in memory for the length of the call.
    let draft = crud::test(
        &state.pool,
        &state.secrets,
        state.registry.as_ref(),
        SourceDraft {
            source_id: None,
            adapter_kind: "jira".to_owned(),
            base_url: env.url.clone(),
            auth_kind: AuthMethod::Pat,
            config: json!({ "username": env.user }),
            secret: Some(SecretInput::of(pat.raw.clone())),
        },
    )
    .await
    .expect("test_source on a draft against the seeded Jira");
    assert!(draft.ok, "{draft:?}");
    let note = draft.detail.clone().unwrap_or_default();
    assert!(
        note.contains("Epic Link customfield_"),
        "the note names this instance's Epic Link field: {draft:?}"
    );
    assert!(
        note.contains("found but not configured"),
        "no id is configured, so the note says membership is not mirrored: {draft:?}"
    );
    // The note is the clause alone: no `Server 10.3.24 ·` in front of it,
    // because `server_version` already carries the version.
    assert!(
        !note.contains('\u{b7}') && !note.starts_with("Server"),
        "the note carries nothing the report already says: {note:?}"
    );
    let found = draft
        .discovered
        .get("epic_link_field")
        .expect("the same call discovered the id the note names");
    assert!(
        note.contains(found.as_str()),
        "the note and the discovered id agree: {note:?} vs {found:?}"
    );
    println!("SEEDED connection note (draft): {note}");

    // 2. A draft naming the **saved** source with no typed secret: the row's
    //    *Test*. The backend tests the stored row against the stored PAT and
    //    answers the same note -- same instance, same arm.
    let saved = crud::test(
        &state.pool,
        &state.secrets,
        state.registry.as_ref(),
        SourceDraft {
            source_id: Some(JIRA.to_owned()),
            adapter_kind: "jira".to_owned(),
            base_url: env.url.clone(),
            auth_kind: AuthMethod::Pat,
            config: json!({}),
            secret: None,
        },
    )
    .await
    .expect("test_source on the saved source with no typed secret");
    assert!(saved.ok, "{saved:?}");
    assert!(
        saved
            .detail
            .as_deref()
            .is_some_and(|d| d.contains("Epic Link customfield_")),
        "the saved source's test carries the note too: {saved:?}"
    );
    assert_eq!(
        saved.detail, draft.detail,
        "one instance, one field, one note, whichever way it was asked"
    );
    println!("SEEDED connection note (saved): {:?}", saved.detail);

    state.scheduler.shutdown().await;
    drop(pat);
}

/// **The configured arm reaches `test_source` (#381).** The arm above is the
/// one a row can *still* be in; this is the one every source created through
/// the Add-source dialog *ends up* in, because the dialog fills
/// `epic_link_field` from `discovered` -- and until this test it was witnessed
/// at the adapter only, never through the app's `ConnectionReport`.
///
/// The id has to come from **this run's** discovery, not from a constant: it
/// is minted per instance run -- `customfield_10101` and `customfield_10109`
/// have both been measured from one seed script, and on one seed
/// `customfield_10102` was *Epic Status* (`knobas-source-jira/src/discover.rs`)
/// -- so a typed one would read the wrong field rather than fail. So: a
/// draft's test reads `discovered["epic_link_field"]`, the saved
/// row is edited to name it -- the whole config, the way the edit form sends
/// one, since `config::patch` replaces the column -- and the row's *Test*
/// answers `Epic Link customfield_…` with **no** *found but not configured*
/// clause. The absence is the load-bearing assertion: containment of the
/// prefix alone is true of both arms. The id's value is never asserted.
///
/// Which arms reach `test_source` in a test, after this one: the configured
/// arm (here) and the found-but-not-configured arm (above). The no-field arm
/// still does not: no real product here lacks the field.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs testenv's seeded Jira: `just atlassian-live`"]
async fn test_source_answers_the_configured_arm_once_the_saved_source_names_the_field() {
    use knobas_app::sources::{SecretInput, SourceDraft, SourcePatch, crud};

    let env = env();
    let pat = env.pat().await;
    let (state, _events) = app(
        "atlassian_live_configured_note",
        &env,
        AuthMethod::Pat,
        &pat.raw,
    )
    .await;

    // 1. Discover the id from this run, the way the Add-source dialog does.
    let draft = crud::test(
        &state.pool,
        &state.secrets,
        state.registry.as_ref(),
        SourceDraft {
            source_id: None,
            adapter_kind: "jira".to_owned(),
            base_url: env.url.clone(),
            auth_kind: AuthMethod::Pat,
            config: json!({ "username": env.user }),
            secret: Some(SecretInput::of(pat.raw.clone())),
        },
    )
    .await
    .expect("test_source on a draft against the seeded Jira");
    assert!(draft.ok, "{draft:?}");
    let found = draft
        .discovered
        .get("epic_link_field")
        .cloned()
        .expect("the draft's test discovered this instance's Epic Link field");
    assert!(
        found.starts_with("customfield_"),
        "a bare custom field id, as the dialog would store it: {found:?}"
    );

    // 2. Name it on the saved row. The whole config, as the edit form sends
    //    it: `username` rides along because the column is replaced, not merged.
    crud::update(
        &state.pool,
        state.registry.as_ref(),
        JIRA,
        SourcePatch {
            config: Some(json!({ "username": env.user, "epic_link_field": found })),
            ..SourcePatch::default()
        },
    )
    .await
    .expect("the saved source now names its Epic Link field");

    // 3. The row's *Test*: a draft naming the saved source, no typed secret.
    let saved = crud::test(
        &state.pool,
        &state.secrets,
        state.registry.as_ref(),
        SourceDraft {
            source_id: Some(JIRA.to_owned()),
            adapter_kind: "jira".to_owned(),
            base_url: env.url.clone(),
            auth_kind: AuthMethod::Pat,
            config: json!({}),
            secret: None,
        },
    )
    .await
    .expect("test_source on the configured saved source");
    assert!(saved.ok, "{saved:?}");
    let note = saved.detail.clone().unwrap_or_default();
    assert!(
        note.contains("Epic Link customfield_"),
        "the configured note names the field: {saved:?}"
    );
    assert!(
        !note.contains("found but not configured"),
        "the field is configured, so the note must not say it is not: {note:?}"
    );
    assert!(
        note.contains(found.as_str()),
        "the note names the id the row was configured with: {note:?} vs {found:?}"
    );
    println!("SEEDED connection note (configured): {note}");

    state.scheduler.shutdown().await;
    drop(pat);
}

// -- the Confluence half: a mention in the inbox (#287) ----------------------

/// The Confluence source id, and the `EntityRef` namespace every mirrored page
/// is in.
const CONFLUENCE: &str = "confluence";

/// Where the seeded Confluence is, and the fixture ids the seed recorded.
///
/// A second environment struct rather than a field on [`Env`]: the Jira tests
/// above must keep failing with *Jira* advice on a run where only Jira is up,
/// and a merged struct would make every one of them need Confluence too.
struct Wiki {
    url: String,
    user: String,
    password: String,
    http: reqwest::Client,
    /// The space key the seed created.
    space: String,
    /// The seeded page a mentioning comment is posted on, by content id.
    page: String,
    /// The page's title, for the messages.
    title: String,
    /// The seeded *Standup protocols* page, by content id -- what #289
    /// publishes under.
    standup_parent: String,
    /// The space's home page, by content id: the outermost ancestor of every
    /// seeded page, and so the first segment of any launcher path (#388).
    home_page_id: String,
    /// The content id the seed put [`Wiki::page`] **under**, as
    /// `seed-state.json` records it. The seed nests that page a level deeper
    /// than its siblings (#396), so this is the *innermost* ancestor and the
    /// second segment of its launcher path -- which is what makes the path a
    /// join of two titles rather than a single one.
    parent_page_id: String,
}

fn wiki() -> Wiki {
    let need = |key: &str| {
        std::env::var(key)
            .ok()
            .map(|v| v.trim().to_owned())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| {
                panic!(
                    "{key} is not set -- this test needs testenv's seeded Confluence. From the \
                     repo root: `just atlassian-live`; or, inside a licence window already open, \
                     `eval \"$(cd testenv && ./seed --env)\"`"
                )
            })
    };
    let state = std::env::var("KNOBAS_CONFLUENCE_SEED_STATE")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testenv/seed-state.json")
        });
    let raw = std::fs::read_to_string(&state).unwrap_or_else(|e| {
        panic!(
            "{}: {e} -- run `./seed-atlassian-content.sh`",
            state.display()
        )
    });
    let whole: serde_json::Value = serde_json::from_str(&raw).expect("seed-state.json is JSON");
    let confluence = &whole["confluence"];
    // The `sepa-design` page: the one fixture page that already has a
    // discussion, so a comment posted on it is one more in a thread rather
    // than the first thing anybody ever said.
    let page = confluence["pages"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|p| p["fixture_id"] == json!("sepa-design"))
        .unwrap_or_else(|| {
            panic!(
                "{}: no confluence.pages entry for `sepa-design` -- run \
                 `./seed-atlassian-content.sh`",
                state.display()
            )
        });
    // The fixture's "parent of daily protocol pages" (`fixtures/tidewater/
    // work.json`), which the seed creates empty precisely so the standup
    // protocol has somewhere to land.
    let standup = confluence["pages"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|p| p["fixture_id"] == json!("standup-protocols"))
        .unwrap_or_else(|| {
            panic!(
                "{}: no confluence.pages entry for `standup-protocols` -- run \
                 `./seed-atlassian-content.sh`",
                state.display()
            )
        });
    Wiki {
        standup_parent: standup["id"].as_str().expect("a page id").to_owned(),
        parent_page_id: page["parent_id"]
            .as_str()
            .unwrap_or_else(|| {
                panic!(
                    "{}: the `sepa-design` entry records no `parent_id` -- re-run \
                     `./seed-atlassian-content.sh`, which writes where it put each page",
                    state.display()
                )
            })
            .to_owned(),
        home_page_id: confluence["home_page_id"]
            .as_str()
            .expect("confluence.home_page_id")
            .to_owned(),
        url: need("KNOBAS_CONFLUENCE_URL")
            .trim_end_matches('/')
            .to_owned(),
        user: need("KNOBAS_CONFLUENCE_USER"),
        password: need("KNOBAS_CONFLUENCE_PASSWORD"),
        http: client(),
        space: confluence["space"]
            .as_str()
            .expect("confluence.space")
            .to_owned(),
        page: page["id"].as_str().expect("a page id").to_owned(),
        title: page["title"].as_str().expect("a page title").to_owned(),
    }
}

impl Wiki {
    async fn api(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<serde_json::Value>,
    ) -> (u16, serde_json::Value) {
        api(
            &self.http,
            &self.url,
            &self.user,
            &self.password,
            method,
            path,
            body,
        )
        .await
    }

    /// `userKey` -- the stable id a Confluence mention is written with, and
    /// the one thing that makes the comment below a *mention* rather than
    /// prose. Asked of the server rather than composed: only Confluence knows
    /// it.
    async fn my_user_key(&self) -> String {
        let (status, body) = self
            .api(reqwest::Method::GET, "rest/api/user/current", None)
            .await;
        assert_eq!(status, 200, "GET /rest/api/user/current: {body}");
        assert_eq!(
            body["username"].as_str(),
            Some(self.user.as_str()),
            "the credential is the seed's admin: {body}"
        );
        body["userKey"]
            .as_str()
            .unwrap_or_else(|| {
                panic!(
                    "this Confluence reports no userKey for {}: {body}",
                    self.user
                )
            })
            .to_owned()
    }

    /// One page's record, with whatever `expand` asks for.
    async fn content(&self, id: &str, expand: &str) -> serde_json::Value {
        let (status, body) = self
            .api(
                reqwest::Method::GET,
                &format!("rest/api/content/{id}?expand={expand}"),
                None,
            )
            .await;
        assert_eq!(status, 200, "GET content {id}: {body}");
        body
    }

    /// The page's `version.when` -- what CQL's `lastmodified` matches, and the
    /// value the claim "a comment does not move its page's timestamp" is
    /// about.
    async fn page_modified(&self) -> String {
        self.content(&self.page, "version").await["version"]["when"]
            .as_str()
            .unwrap_or_default()
            .to_owned()
    }

    /// Delete every comment a killed run left behind -- the ones no `Drop`
    /// ever reached, recognised by the marker in their body. Read-only in the
    /// ordinary case.
    async fn clear_leftovers(&self) {
        let (status, body) = self
            .api(
                reqwest::Method::GET,
                &format!(
                    "rest/api/content/{}/child/comment?expand=body.storage&limit=100",
                    self.page
                ),
                None,
            )
            .await;
        assert_eq!(status, 200, "listing comments of {}: {body}", self.page);
        for id in body["results"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|c| {
                c["body"]["storage"]["value"]
                    .as_str()
                    .is_some_and(|v| v.contains(LITTER_LABEL))
            })
            .filter_map(|c| c["id"].as_str().map(str::to_owned))
        {
            let (status, body) = self
                .api(
                    reqwest::Method::DELETE,
                    &format!("rest/api/content/{id}"),
                    None,
                )
                .await;
            assert!(
                status == 204 || status == 200,
                "deleting the leftover comment {id}: {status} {body}"
            );
            println!("live suite: deleted leftover Confluence comment {id}");
        }
    }

    /// Revoke every personal access token a **killed** run left behind,
    /// recognised by the name it was created under. Read-only in the ordinary
    /// case.
    ///
    /// Its own method rather than a clause of [`Wiki::clear_leftovers`]: that
    /// one runs before the mention test, which needs no token at all, and a
    /// Confluence that answered this path with anything but `200` would turn
    /// #287's criterion red over a fixture it never touches.
    async fn clear_leftover_tokens(&self) {
        let (status, body) = self
            .api(reqwest::Method::GET, "rest/pat/latest/tokens", None)
            .await;
        assert_eq!(
            status, 200,
            "listing Confluence personal access tokens: {body}"
        );
        for id in token_ids(&body, |name| name.starts_with(LITTER_LABEL)) {
            let (status, body) = self
                .api(
                    reqwest::Method::DELETE,
                    &format!("rest/pat/latest/tokens/{id}"),
                    None,
                )
                .await;
            assert_eq!(
                status, CONFLUENCE_TOKENS.revoked,
                "revoking the leftover Confluence token {id}: {body}"
            );
            println!("live suite: revoked leftover Confluence personal access token {id}");
        }
    }

    /// A personal access token at this Confluence, revoked when the guard
    /// drops.
    async fn pat(&self) -> Pat {
        Pat::issue(&self.url, &self.user, &self.password, CONFLUENCE_TOKENS).await
    }
}

/// The comment this test posts, taken back out when the guard drops.
///
/// **The fixture cannot carry it.** `testenv/seed-atlassian-content.sh` builds
/// every comment body through one `jq` definition that escapes `<`, `>` and
/// `&`, so a seeded comment is one `<p>` of text and *cannot* hold the
/// `<ac:link><ri:user/></ac:link>` markup a mention is. The seed is therefore
/// left alone and the mention is created here, over REST, with the real
/// `userKey` the server just reported.
struct Mention {
    url: String,
    user: String,
    password: String,
    id: String,
}

impl Mention {
    /// Post a comment on the seeded page that mentions the admin account.
    async fn post(wiki: &Wiki, user_key: &str) -> Mention {
        let storage = format!(
            "<p><ac:link><ri:user ri:userkey=\"{user_key}\" /></ac:link> can you confirm the \
             manual-review SLA? [{LITTER_LABEL}]</p>"
        );
        let (status, body) = wiki
            .api(
                reqwest::Method::POST,
                "rest/api/content",
                Some(json!({
                    "type": "comment",
                    "container": { "id": wiki.page, "type": "page" },
                    "body": { "storage": { "value": storage, "representation": "storage" } },
                })),
            )
            .await;
        assert_eq!(
            status, 200,
            "posting a mentioning comment on {}: {body}",
            wiki.page
        );
        let id = body["id"].as_str().expect("a comment id").to_owned();
        // What the server *stored*, which is not necessarily what was sent: a
        // Confluence may normalise a user link, and the whole feature rests on
        // which spelling comes back.
        let stored = wiki.content(&id, "body.storage").await["body"]["storage"]["value"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        println!(
            "SEEDED mentioning comment {id} on page {}: {stored}",
            wiki.page
        );
        Mention {
            url: wiki.url.clone(),
            user: wiki.user.clone(),
            password: wiki.password.clone(),
            id,
        }
    }
}

impl Drop for Mention {
    fn drop(&mut self) {
        let (url, user, password, id) = (
            self.url.clone(),
            self.user.clone(),
            self.password.clone(),
            self.id.clone(),
        );
        undo("the mentioning comment", move || async move {
            let http = client();
            let (status, body) = api(
                &http,
                &url,
                &user,
                &password,
                reqwest::Method::DELETE,
                &format!("rest/api/content/{id}"),
                None,
            )
            .await;
            if status != 204 && status != 200 && status != 404 {
                return Err(format!("DELETE comment {id} -> {status}: {body}"));
            }
            let (status, _) = api(
                &http,
                &url,
                &user,
                &password,
                reqwest::Method::GET,
                &format!("rest/api/content/{id}"),
                None,
            )
            .await;
            // A deleted comment is trashed rather than purged, and Confluence
            // answers 404 for one whose status is `trashed` on this path.
            if status != 404 {
                return Err(format!(
                    "comment {id} still answers {status} after its delete"
                ));
            }
            Ok(())
        });
    }
}

/// A `SourcesState` with the seeded Confluence configured and nothing else,
/// authenticating the way the caller says.
///
/// The auth method is a parameter because the two Confluence criteria need
/// different ones and neither may have the other's. The mention test signs in
/// as the seed admin with **user + password**, which is also the identity the
/// inbox matches a mention against. The credential-health test must be able to
/// present a credential the server *refuses*, and on a real Confluence the
/// only safe way to do that is a **bearer token**: a wrong password is a failed
/// login, and a few of those lock the account out for the correct password too
/// (#276 measured it on Jira). `AuthKind` is not patchable after the insert --
/// `config::PatchConfig` deliberately has no field for it, because a re-auth
/// goes through `set_source_secret` -- so the method is chosen here, once.
async fn wiki_app(
    name: &str,
    wiki: &Wiki,
    auth: AuthMethod,
    secret: &str,
) -> (SourcesState, Arc<Events>) {
    let connector = knobas_db::test_util::scratch_database(name).await;
    let pool = connector
        .pool(4)
        .await
        .expect("a pool onto the scratch database");
    let secrets = Arc::new(MemoryStore::new());
    secrets
        .put(&knobas_secrets::KeychainAccount::source(CONFLUENCE), &Secret::just(auth, secret))
        .expect("the Confluence credential is stored");

    knobas_sync::config::insert(
        &pool,
        &knobas_sync::config::InsertConfig {
            id: CONFLUENCE.to_owned(),
            adapter_kind: "confluence".to_owned(),
            display_name: "Tidewater Confluence (seeded)".to_owned(),
            base_url: wiki.url.clone(),
            auth_kind: knobas_sync::config::AuthKind::Method(auth),
            // The username is half of the Basic pair **and** the identity the
            // inbox matches a mention against -- one field doing the two jobs
            // #82 gave it. It stays filled in under a personal access token
            // for the second of those jobs.
            config: json!({ "username": wiki.user, "spaces": [wiki.space] }),
            sync_interval_secs: 86_400,
            enabled: true,
        },
    )
    .await
    .expect("the source row is written");

    let events = Arc::new(Events::default());
    let scheduler = Scheduler::start(SchedulerDeps {
        pool: pool.clone(),
        connections: Arc::new(Connections(connector)),
        registry: Arc::new(Registry::builtin()),
        secrets: secrets.clone(),
        events: events.clone(),
        timing: knobas_sync::scheduler::SchedulerTiming::default(),
    })
    .await
    .expect("a scheduler over the scratch database");

    (
        SourcesState {
            pool,
            scheduler,
            secrets,
            registry: Arc::new(Registry::builtin()),
        },
        events,
    )
}

/// **M3.2's exit criterion: a comment that mentions the admin account becomes
/// an inbox item** (#287), through the real Confluence, the real sync engine
/// and the real inbox derivation.
///
/// The sequence is the one the feature exists for, and the order is the whole
/// witness:
///
/// 1. A full sync, which mirrors the seeded space and leaves a cursor.
/// 2. A comment mentioning the admin is posted on a seeded page -- written
///    with the `userKey` the server itself reports, because that is what a
///    mention *is* in the storage format and what the fixture cannot hold.
/// 3. An incremental sync. The page's own `lastmodified` is asserted either
///    way and printed: on this product a comment does not move it, so the page
///    walk cannot reach the page and the mention query is the only path to it.
/// 4. The item is in the inbox as a **mention**, keyed on the page.
/// 5. Its actions are **empty** -- the adapter declares no write op yet
///    (#286) -- and *snooze* records an activity line naming the page, which
///    is the criterion's "each records its activity line" for the two answers
///    the write queue knows nothing about.
///
/// CQL reads an index the write path updates asynchronously, so step 3 polls
/// to [`INDEX_BUDGET`] rather than sleeping a guessed amount.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs testenv's seeded Confluence: `just atlassian-live`"]
async fn a_comment_that_mentions_me_becomes_an_inbox_mention() {
    let wiki = wiki();
    wiki.clear_leftovers().await;
    let (state, _events) = wiki_app(
        "atlassian_live_mention",
        &wiki,
        AuthMethod::UserPassword,
        &wiki.password,
    )
    .await;

    // 1. The seeded corpus, and a cursor.
    sync(&state, CONFLUENCE).await;
    let mirrored = confluence_pages(&state.pool).await;
    assert!(
        mirrored
            .iter()
            .any(|id| id == &format!("confluence:{}", wiki.page)),
        "the seeded page is mirrored before anything is written: {mirrored:?}"
    );
    let before = wiki.page_modified().await;
    let items = inbox_mentions(&state).await;
    assert!(
        items.is_empty(),
        "the seeded fixture mentions nobody -- its one comment names `@Mara` in prose, and the \
         admin is not Mara: {items:?}"
    );

    // 2. The mention.
    let user_key = wiki.my_user_key().await;
    let mention = Mention::post(&wiki, &user_key).await;

    // 3. The incremental run, polled until the index has it.
    let after = wiki.page_modified().await;
    println!(
        "SEEDED page {} lastmodified: {before} before the comment, {after} after -- {}",
        wiki.page,
        if before == after {
            "unchanged, so the page walk cannot reach it and the mention query is the only path"
        } else {
            "MOVED, so the page walk reaches it too and this run witnesses the weaker path"
        }
    );
    let deadline = std::time::Instant::now() + INDEX_BUDGET;
    let found = loop {
        sync(&state, CONFLUENCE).await;
        let items = inbox_mentions(&state).await;
        if let Some(item) = items.into_iter().next() {
            break item;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "no inbox mention after {INDEX_BUDGET:?} of syncing -- Confluence's CQL index never \
             answered `mention = currentUser()` for comment {}",
            mention.id
        );
    };

    // 4. The item, keyed on the page the comment is on.
    assert_eq!(
        found.item.entity_id.as_deref(),
        Some(format!("confluence:{}", wiki.page).as_str()),
        "the entity is the page, not the comment: {:?}",
        found.item
    );
    assert_eq!(found.item.kind.as_deref(), Some("page"));
    assert_eq!(found.item.title, wiki.title);
    assert_eq!(found.item.source_id, CONFLUENCE);
    assert!(
        found.item.web_url.is_some(),
        "*Open in browser* is the action knobas' own: {:?}",
        found.item
    );
    println!(
        "SEEDED inbox mention: {} ({})",
        found.item.key, found.item.title
    );

    // 5. What it offers, and what an answer records.
    //
    // **`Comment`, and it appeared without this test being told to expect it**
    // -- which is the claim #287 wrote this clause to be able to make. That
    // ticket asserted `is_empty()` because the Confluence adapter declared no
    // write op; #286 declares `comment` and nothing in the inbox changed. The
    // assertion is the same shape it always was: whatever
    // `knobas_app::inbox::offer` keeps of what the **descriptor** declares,
    // measured against the descriptor itself rather than against a list typed
    // here, so the day the adapter declares a fourth op this still reads true.
    let declared = state
        .registry
        .descriptors()
        .into_iter()
        .find(|d| d.adapter_kind == "confluence")
        .expect("the Confluence adapter is compiled in")
        .write_ops;
    let offered: Vec<String> = found.actions.clone();
    assert_eq!(
        offered,
        declared
            .iter()
            .filter(|op| op.as_str() == "comment")
            .cloned()
            .collect::<Vec<String>>(),
        "a mention offers exactly the mention ops its source declares: {:?}",
        found.actions
    );
    println!("SEEDED mention offers: {offered:?} (declared {declared:?})");
    let line = knobas_app::commands::entity::snooze_inbox_item_inner(
        &state.pool,
        state.registry.as_ref(),
        chrono::Utc::now(),
        &found.item.key,
        chrono::Utc::now() + chrono::Duration::days(2),
    )
    .await
    .expect("the mention is on the stream");
    assert_eq!(line.verb, "snoozed");
    assert_eq!(line.actor, "user");
    assert_eq!(
        line.entity_id.as_deref(),
        Some(format!("confluence:{}", wiki.page).as_str())
    );
    println!("SEEDED activity line: {} {:?}", line.verb, line.entity_id);

    state.scheduler.shutdown().await;
    drop(mention);
}

/// **M3.2's other exit criterion: the launcher finds "SEPA payout retry
/// design" with its ancestor path** (#388), through the real Confluence, the
/// real sync engine and the real search statement.
///
/// Every link of this chain had a witness of its own -- the adapter's live
/// suite reads the seeded page's `ancestors` off the wire, `knobas-core`
/// joins a bound payload, `knobas-search`'s `tests/ancestor_path.rs` searches
/// a fixture page -- and none of them was the sentence the criterion makes:
/// that a search over *this* mirror of *this* server answers with the path
/// under the row. So:
///
/// 1. A full sync mirrors the seeded space.
/// 2. The criterion's own words are searched, through the command the IPC
///    calls, and the hit is the seeded page.
/// 3. Its `path` is **exactly** the ancestor titles Confluence itself reports
///    for the page, joined -- read back from the server rather than from the
///    mirror, so the expected value is not a copy of what the read is being
///    asked to produce. Equality rather than containment (#388's brief allows
///    it "unless the seed pins it", and the seed does): a substring check
///    would pass a path that had appended the page's own title, and it would
///    say nothing about order or about the join.
///
/// The **join** is witnessed here too, since #396: the seed nests this page
/// under *Payments architecture overview* rather than directly under the
/// space home, so the path is two segments and `string_agg` really aggregates
/// -- the separator and the outermost-first ordering are read off a real
/// server rather than off a fixture. The equality below did not have to move
/// for that (#388 wrote it against whatever the server reports); what moved is
/// the check on *which* ancestors those are, which now names both ends of the
/// tree the seed built. `knobas-search`'s `tests/ancestor_path.rs` and
/// `knobas-core`'s still pin the separator literal itself.
///
/// Nothing is written, and the search is the plain launcher query with no
/// filters -- the "under 100 ms" clause of story 46 is not measured here.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs testenv's seeded Confluence: `just atlassian-live`"]
async fn the_launcher_finds_the_seeded_page_with_its_ancestor_path() {
    let wiki = wiki();
    let (state, _events) = wiki_app(
        "atlassian_live_search_path",
        &wiki,
        AuthMethod::UserPassword,
        &wiki.password,
    )
    .await;

    // 1. The seeded corpus.
    sync(&state, CONFLUENCE).await;
    let mirrored = confluence_pages(&state.pool).await;
    assert!(
        mirrored
            .iter()
            .any(|id| id == &format!("confluence:{}", wiki.page)),
        "the seeded page is mirrored: {mirrored:?}"
    );

    // 2. The criterion's words, through the command the launcher calls.
    const CRITERION: &str = "SEPA payout retry design";
    assert_eq!(
        wiki.title, CRITERION,
        "the seed's `sepa-design` page is the one the roadmap names"
    );
    let response = knobas_app::commands::search::search_inner(
        &state.pool,
        knobas_search::SearchQuery {
            raw: CRITERION.to_owned(),
            limit: 30,
            filters: knobas_search::SearchFilters::default(),
        },
    )
    .await
    .expect("the search runs");
    let hit = response
        .groups
        .iter()
        .flat_map(|g| g.hits.iter())
        .find(|h| h.row.entity_id == format!("confluence:{}", wiki.page))
        .unwrap_or_else(|| {
            panic!(
                "{CRITERION:?} did not find the seeded page confluence:{} among {} hits: {:?}",
                wiki.page,
                response.total,
                response
                    .groups
                    .iter()
                    .flat_map(|g| g.hits.iter().map(|h| &h.row.entity_id))
                    .collect::<Vec<_>>()
            )
        });
    assert_eq!(hit.row.kind, "page");
    assert_eq!(hit.row.title, CRITERION);

    // 3. The path, against the server's own answer for the page.
    let record = wiki.content(&wiki.page, "ancestors").await;
    let ancestors = record["ancestors"]
        .as_array()
        .unwrap_or_else(|| panic!("no ancestors array on the server's record: {record}"))
        .clone();
    let titles: Vec<String> = ancestors
        .iter()
        .map(|a| {
            a["title"]
                .as_str()
                .unwrap_or_else(|| panic!("an ancestor without a title: {a}"))
                .to_owned()
        })
        .collect();
    // Both ends of the tree the seed built, so the equality below is against
    // the seeded ancestors and not against whatever the page has drifted to.
    // Two ids rather than one: the outermost is the space home every page
    // hangs off, and the innermost is the page the seed nests *this* one under
    // -- and since the seed makes those two different pages, asserting them is
    // asserting that the path being compared has a join in it at all (#396).
    assert_eq!(
        ancestors.first().and_then(|a| a["id"].as_str()),
        Some(wiki.home_page_id.as_str()),
        "the outermost ancestor is the space home the seeded tree hangs off"
    );
    assert_eq!(
        ancestors.last().and_then(|a| a["id"].as_str()),
        Some(wiki.parent_page_id.as_str()),
        "the innermost ancestor is the page `seed-state.json` says the seed put this one under"
    );
    assert_ne!(
        wiki.parent_page_id, wiki.home_page_id,
        "the seed nests this page below the space home; flat, the path is one segment and this \
         test witnesses no join at all"
    );
    let path = hit.row.path.as_deref().unwrap_or_else(|| {
        panic!(
            "the hit carries no path, and Confluence reports ancestors {titles:?}: {:?}",
            hit.row
        )
    });
    // The whole path and nothing else: outermost first, on the one separator.
    // The separator is `knobas-core`'s constant here rather than a literal --
    // the literal is pinned once, by `knobas-search`'s fixture test, and what
    // this run adds is that the mirror's answer is the server's ancestors and
    // no more.
    assert_eq!(
        path,
        titles.join(knobas_core::payload::ANCESTOR_SEPARATOR),
        "the path is exactly the ancestors Confluence reports, joined: {titles:?}"
    );
    println!(
        "SEEDED launcher hit for {CRITERION:?}: {} with path {path:?} in {} segment(s) \
         (Confluence reports ancestors {titles:?}, home page {}, parent {})",
        hit.row.entity_id,
        titles.len(),
        wiki.home_page_id,
        wiki.parent_page_id
    );

    state.scheduler.shutdown().await;
}

/// **A page the source says is mine is on the digest, under the day it moved**
/// (issue #288, the Confluence half of criterion 4).
///
/// The Jira test above witnesses the two producers a *write* makes -- the
/// worklog copy and the activity stream. This is the third, the **mirror**,
/// and it is the one that cannot be settled against a scratch fixture: it
/// rests on what the *adapter* puts in §4.1's `author` for a real page, and
/// `knobas-source-confluence`'s answer is the version's author falling back to
/// the creator (`map.rs`). The seeded pages were written by this account, so
/// the digest for the day after the day one of them last moved has to carry
/// it, with its content id as the ref.
///
/// **Nothing is written here, and that stays deliberate.** The claim is about
/// what the adapter puts in `author` for a page nobody touched, and a write
/// would only put knobas' own account in the way of it. The other half of
/// #288's criterion 4 -- *a page edited* -- is
/// [`a_page_edited_through_knobas_is_on_the_digest_under_yesterday`], which
/// became expressible when #286 landed `UpdatePage` and carries the
/// body-restore path ([`Edited`]) this one is written to avoid needing.
///
/// It survives that sibling, which runs first and edits this very page: the
/// day is taken from the page's **own mirrored timestamp** rather than assumed
/// to be today, so the assertion holds whether the page last moved when the
/// environment was seeded or a minute ago.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs testenv's seeded Confluence: `just atlassian-live`"]
async fn a_page_the_source_says_is_mine_is_on_the_digest_for_the_day_it_moved() {
    let wiki = wiki();
    // User + password, which is what this test had before `wiki_app` took the
    // method as a parameter (#317): the digest matches "mine" against
    // `config.username`, and that is half of the Basic pair here.
    let (state, _events) = wiki_app(
        "atlassian_live_digest_page",
        &wiki,
        AuthMethod::UserPassword,
        &wiki.password,
    )
    .await;
    sync(&state, CONFLUENCE).await;

    // The page, and the day the mirror says it last moved -- read back rather
    // than assumed, so this is a statement about the adapter's own `author`
    // and `item_updated_at` and not about when the suite happens to run.
    let page = format!("{CONFLUENCE}:{}", wiki.page);
    let moved: chrono::DateTime<chrono::Utc> = sqlx::query_scalar(
        "select coalesce(item_updated_at, synced_at) from sync.live_item
          where entity_id = $1 and author = $2",
    )
    .bind(&page)
    .bind(&wiki.user)
    .fetch_optional(&state.pool)
    .await
    .expect("the mirror is readable")
    .unwrap_or_else(|| {
        panic!(
            "{} is not mirrored with {} as its §4.1 author -- the seed writes the \
             pages as this account, and the digest's mirror half is what that \
             normalization feeds",
            wiki.title, wiki.user
        )
    });

    let day = moved.date_naive();
    let digest = knobas_app::commands::entity::standup_digest_inner(
        &state.pool,
        state.registry.as_ref(),
        // A clock outside the day being asked about, so no running timer of
        // this scratch database's own can join the list. There is none, and
        // saying so costs one argument.
        day_window(day.succ_opt().expect("tomorrow exists")).to,
        day_window(day.succ_opt().expect("tomorrow exists")),
        &[day_window(day)],
    )
    .await
    .expect("the digest reads");

    assert_eq!(digest.yesterday_day, Some(day));
    let line = digest
        .yesterday
        .iter()
        .find(|line| line.entity_id.as_deref() == Some(page.as_str()))
        .unwrap_or_else(|| {
            panic!(
                "{} is not on the digest for {day}, which is the day the mirror \
                 says it moved: {:?}",
                wiki.title, digest.yesterday
            )
        });
    assert_eq!(line.source, CONFLUENCE);
    assert_eq!(
        line.verb, "attributed",
        "the mirror half says the source attributes the page to the reader, \
         never that the reader wrote it"
    );
    assert!(
        line.reason.contains(CONFLUENCE),
        "the reason names the source it came from: {:?}",
        line.reason
    );
    println!("SEEDED digest page line: {} ({})", line.title, line.reason);

    state.scheduler.shutdown().await;
}

/// The seeded page this suite edits, put back to its seeded body when the
/// guard drops.
///
/// **Why a body and not a version number.** Confluence has no undo: every
/// content `PUT` is the next version, so the restore is one more version on
/// top of the edit rather than a removal of it. What has to be preserved is
/// therefore the *bytes* -- the storage format the seed wrote -- and they are
/// read before the edit and sent back after it, with the title and content
/// type the `PUT` also replaces (`knobas-source-confluence`'s `write` module
/// records why all three travel together).
///
/// **And why it verifies.** A restore that answered `200` and left something
/// else behind is the failure this guard exists for: it changes what every
/// later suite reads off a shared fixture, silently. So the record is read
/// back and compared -- the body, and the title and content type the `PUT`
/// replaced alongside it, since a check over one of the three would pass a
/// restore that damaged the other two -- and [`undo`] turns any mismatch into
/// a panic, the same contract [`Litter`] and [`Protocol`] have.
///
/// Unlike a comment or a created issue, an edit carries no marker a later run
/// could sweep: a killed run leaves the page edited. `just atlassian-live`
/// tears the pair down and `seed-atlassian-content.sh` rebuilds it, so the
/// cost of that is a re-seed rather than lost data.
///
/// **What a successful restore still leaves behind**, said here rather than
/// discovered later. Confluence has no undo, so the page comes out of this
/// suite at `version.number + 2` with its `version.when` moved to the run's
/// own clock. The bytes are the seed's again; the *history* is not. Nothing in
/// the suite reads either today --
/// [`a_page_the_source_says_is_mine_is_on_the_digest_for_the_day_it_moved`]
/// takes its day from whatever the mirror says rather than from the seed's
/// date, which is exactly why it survives this -- but a future test that
/// assumed the seed's timestamp would not, and this is the note that says so.
/// The CQL index needs no symmetric wait the way [`Protocol`]'s delete does:
/// this page was in the index before the suite ran and is in it after, and an
/// edit changes what a later read *says* about it rather than whether it is
/// there.
struct Edited {
    url: String,
    user: String,
    password: String,
    id: String,
    title: String,
    content_type: String,
    /// The storage format the page had before this suite touched it.
    body: String,
}

impl Drop for Edited {
    fn drop(&mut self) {
        let (url, user, password, id, title, content_type, body) = (
            self.url.clone(),
            self.user.clone(),
            self.password.clone(),
            self.id.clone(),
            self.title.clone(),
            self.content_type.clone(),
            self.body.clone(),
        );
        // The `what` carries the recovery, because [`undo`]'s standing
        // sentence -- "the next run's leftover clearing removes what carries
        // the marker" -- is not true of an edit: nothing sweeps one.
        undo(
            "the page it edited (an edit carries no marker, so nothing sweeps it -- \
             re-seed with `testenv/seed-atlassian-content.sh`)",
            move || async move {
                let http = client();
                let call = |method: reqwest::Method,
                            path: String,
                            payload: Option<serde_json::Value>| {
                    let http = http.clone();
                    let (url, user, password) = (url.clone(), user.clone(), password.clone());
                    async move { api(&http, &url, &user, &password, method, &path, payload).await }
                };

                // The version it is at *now*: the edit bumped it, and Confluence
                // accepts only the next number.
                let (status, current) = call(
                    reqwest::Method::GET,
                    format!("rest/api/content/{id}?expand=version"),
                    None,
                )
                .await;
                if status != 200 {
                    return Err(format!(
                        "reading {id}'s version back -> {status}: {current}"
                    ));
                }
                let Some(number) = current["version"]["number"].as_i64() else {
                    return Err(format!(
                        "content {id} answered no version.number: {current}"
                    ));
                };

                let (status, answered) = call(
                    reqwest::Method::PUT,
                    format!("rest/api/content/{id}"),
                    Some(json!({
                        "id": id,
                        "type": content_type,
                        "title": title,
                        "version": { "number": number + 1 },
                        "body": { "storage": { "value": body, "representation": "storage" } },
                    })),
                )
                .await;
                if status != 200 {
                    return Err(format!("PUT restoring page {id} -> {status}: {answered}"));
                }

                // Verified, not assumed -- and all three of what the `PUT`
                // replaced, not only the body. The record travels together, so
                // a restore that put the bytes back under a changed title, or
                // wrote a blog post back as a page, is the same silent damage
                // to a shared fixture that checking the body at all exists to
                // catch.
                let (status, after) = call(
                    reqwest::Method::GET,
                    format!("rest/api/content/{id}?expand=body.storage"),
                    None,
                )
                .await;
                if status != 200 {
                    return Err(format!("reading {id}'s body back -> {status}: {after}"));
                }
                let restored = after["body"]["storage"]["value"]
                    .as_str()
                    .unwrap_or_default();
                if restored != body {
                    return Err(format!(
                        "page {id} did not come back to its seeded body: it now holds \
                         {restored:?}, and the seed wrote {body:?}"
                    ));
                }
                if after["title"].as_str() != Some(title.as_str()) {
                    return Err(format!(
                        "page {id} came back under the title {:?}, and it was {title:?}",
                        after["title"]
                    ));
                }
                if after["type"].as_str() != Some(content_type.as_str()) {
                    return Err(format!(
                        "page {id} came back as a {:?}, and it was a {content_type:?}",
                        after["type"]
                    ));
                }
                Ok(())
            },
        );
    }
}

/// **A page edited *through knobas* is on the digest under yesterday**
/// (issue #342, the half of #288's criterion 4 that merged partial).
///
/// [`a_page_the_source_says_is_mine_is_on_the_digest_for_the_day_it_moved`]
/// witnesses the **mirror** producer against a real Confluence and writes
/// nothing, which is the right shape for the claim it makes. It is not the
/// claim #288's criterion 4 spells out, though: *"after the seeded day (a
/// worklog logged, a comment posted, **a page edited**), the digest lists them
/// under yesterday with their refs"* -- and an edit knobas did not make is not
/// a page edited through knobas. That line is the **activity** producer with
/// `op = update_page`, the same producer
/// [`a_seeded_days_work_is_what_the_digest_lists_under_yesterday`] asserts for
/// `comment` and `log_work` on the Jira side, and it became expressible when
/// #286 landed `UpdatePage`.
///
/// So: a real edit, through the write queue, against the real product, and
/// then the digest. Every step is a fact no scratch fixture settles --
/// `knobas_app::standup`'s `WRITTEN` reads `a.detail->>'op'` off the `queued`
/// activity line the queue writes, and what puts a value there is
/// `WriteOp::identifier` travelling through `knobas_sync::write_queue`'s
/// `announce` against a source that actually took the write.
///
/// **Asked for tomorrow, so today is *yesterday*** -- the same device the Jira
/// digest test uses, and for the same reason: the only day this suite can put
/// real work on is the day it runs.
///
/// The edit itself is read back **from Confluence**, not from the mirror: a
/// mirror read would only prove knobas agrees with itself.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs testenv's seeded Confluence: `just atlassian-live`"]
async fn a_page_edited_through_knobas_is_on_the_digest_under_yesterday() {
    use knobas_core::write_queue::WriteState;

    let wiki = wiki();
    // User + password, as the seeded admin: the digest matches "mine" against
    // `config.username`, which is half of the Basic pair, and the edit has to
    // be attributed to an account the seed's pages already belong to.
    let (state, _events) = wiki_app(
        "atlassian_live_digest_edit",
        &wiki,
        AuthMethod::UserPassword,
        &wiki.password,
    )
    .await;
    sync(&state, CONFLUENCE).await;

    // The mirror has to hold the page first: the queue snapshots its target
    // when a write is queued and re-reads it before sending, so an unmirrored
    // page would hold rather than send.
    let page = format!("{CONFLUENCE}:{}", wiki.page);
    let held = confluence_pages(&state.pool).await;
    assert!(
        held.contains(&page),
        "{page} is not in the mirror: {held:?}"
    );

    // The record as it stands, before anything is written: the version the
    // edit is made **against**, and the bytes the guard has to put back.
    //
    // Read from the product rather than out of the mirrored payload, which is
    // where the app reads it: the sync above is the only thing between the two
    // and this is the same `GET` the adapter's own `update_page` makes, so the
    // number is the server's and not a projection of it.
    let before = wiki.content(&wiki.page, "version,body.storage").await;
    let base_version = before["version"]["number"]
        .as_i64()
        .unwrap_or_else(|| panic!("{} answered no version.number: {before}", wiki.title));
    let original = before["body"]["storage"]["value"]
        .as_str()
        .unwrap_or_else(|| panic!("{} answered no stored body: {before}", wiki.title))
        .to_owned();
    // Refused rather than guessed. `knobas-source-confluence`'s `update_page`
    // falls back to `"page"` because it is writing an edit somebody asked for
    // and a guess beats a lost edit; a *restore* has no such excuse, and a
    // wrong `type` here would write a blog post back as a page.
    let content_type = before["type"]
        .as_str()
        .unwrap_or_else(|| panic!("{} answered no content type: {before}", wiki.title))
        .to_owned();
    // The title the **server** holds, not the one `seed-state.json` recorded:
    // the content `PUT` replaces the record, so what goes back has to be what
    // is there now, and a fixture file is a statement about what was seeded.
    let live_title = before["title"]
        .as_str()
        .unwrap_or_else(|| panic!("{} answered no title: {before}", wiki.title))
        .to_owned();

    // Owned **before** the write, not after: from the moment the PUT lands the
    // page is changed, so a failing assertion below must still leave the guard
    // something to put back.
    let _guard = Edited {
        url: wiki.url.clone(),
        user: wiki.user.clone(),
        password: wiki.password.clone(),
        id: wiki.page.clone(),
        title: live_title,
        content_type,
        body: original.clone(),
    };

    // Storage format, appended to what the seed wrote -- `UpdatePage` replaces
    // the **whole** body, so the edit is the old body plus a paragraph and not
    // the paragraph alone.
    let edited = format!(
        "{original}<p>{LITTER_LABEL}: knobas edited this through the write queue (pid {})</p>",
        std::process::id()
    );
    let row = write(
        &state,
        json!({
            "UpdatePage": {
                "entity": page,
                "base_version": base_version,
                "body": edited
            }
        }),
    )
    .await;
    assert_eq!(row.state, WriteState::Sent, "{:?}", row.detail);

    // At Confluence, read back from Confluence.
    let after = wiki.content(&wiki.page, "version,body.storage").await;
    assert_eq!(
        after["body"]["storage"]["value"].as_str(),
        Some(edited.as_str()),
        "the edit knobas queued is the page's stored body: {after}"
    );
    assert_eq!(
        after["version"]["number"].as_i64(),
        Some(base_version + 1),
        "...as the version after the one it was made against, which is the check that stops \
         a second writer being overwritten: {after}"
    );

    // -- and what the digest makes of it ------------------------------------
    let day = chrono::Utc::now().date_naive();
    let digest = knobas_app::commands::entity::standup_digest_inner(
        &state.pool,
        state.registry.as_ref(),
        // The real clock, as
        // `a_seeded_days_work_is_what_the_digest_lists_under_yesterday` passes
        // it: `now` is what a *running timer* would be measured against, and
        // this scratch database has none. The sibling above pins a clock
        // outside the day it asks about because its day may be long past; the
        // day here is today, so the two devices are the same statement.
        chrono::Utc::now(),
        day_window(day.succ_opt().expect("tomorrow exists")),
        &[day_window(day)],
    )
    .await
    .expect("the digest reads");

    assert_eq!(
        digest.yesterday_day,
        Some(day),
        "today is the newest day before tomorrow with any of this account's work"
    );
    let listed: Vec<(Option<&str>, &str, &str)> = digest
        .yesterday
        .iter()
        .map(|line| {
            (
                line.entity_id.as_deref(),
                line.source.as_str(),
                line.verb.as_str(),
            )
        })
        .collect();
    assert!(
        listed.contains(&(Some(page.as_str()), CONFLUENCE, "update_page")),
        "no update_page line for {} under yesterday, so a page edited through knobas is not \
         on the digest: {listed:?}",
        wiki.title
    );
    let line = digest
        .yesterday
        .iter()
        .find(|line| line.entity_id.as_deref() == Some(page.as_str()) && line.verb == "update_page")
        .expect("the line the assertion above found");
    assert!(
        !line.reason.trim().is_empty(),
        "a line whose provenance cannot be shown is not shippable: {line:?}"
    );
    assert!(
        line.reason.contains(CONFLUENCE),
        "the reason names the source the write went to: {:?}",
        line.reason
    );
    println!(
        "SEEDED digest edit line: {} -- {} (version {} -> {})",
        line.title,
        line.reason,
        base_version,
        base_version + 1
    );

    state.scheduler.shutdown().await;
}

/// The page ids this source holds live.
async fn confluence_pages(pool: &sqlx::PgPool) -> Vec<String> {
    sqlx::query_scalar::<_, String>(
        "select entity_id from sync.live_item where source_id = $1 order by entity_id",
    )
    .bind(CONFLUENCE)
    .fetch_all(pool)
    .await
    .expect("the mirror is readable")
}

/// The inbox's mentions, from the real derivation over the real identity.
async fn inbox_mentions(state: &SourcesState) -> Vec<knobas_app::inbox::InboxEntry> {
    knobas_app::commands::entity::inbox_items_inner(
        &state.pool,
        state.registry.as_ref(),
        chrono::Utc::now(),
        knobas_core::inbox::Shelf::Stream,
    )
    .await
    .expect("the inbox derivation reads")
    .into_iter()
    .filter(|e| e.item.category == knobas_core::inbox::Category::Mention)
    .collect()
}

// -- the Confluence half: credential health through the app (#317) -----------

/// **A Confluence personal access token that worked and stopped working**, all
/// the way to what the sources view renders -- and the mirror still standing
/// afterwards.
///
/// # What this witnesses that the adapter's own live suite cannot
///
/// `knobas-source-confluence/tests/live_confluence_seeded.rs`'s
/// `a_rejected_credential_is_refused_and_the_seed_account_still_works` already
/// holds the wire end: a bearer token this Confluence cannot resolve draws a
/// clean **401** on `test_connection` *and* on the content search, so
/// `Source::sync` answers `Err(SourceError::Unauthorized)` and emits nothing
/// (#284). That is a claim about one `Source` object, and it is the whole of
/// what a crate with no database and no event bus can say.
///
/// Three claims live downstream of it and none of them is asserted anywhere
/// else:
///
/// * **`knobas.source_config.auth_state` becomes `unauthorized`**, with
///   `auth_checked_at` stamped. That column is what puts *Re-enter* on the row
///   (interfaces §3) and what stops the scheduler backing off over a fault no
///   retry can fix (P7). Turning a `SourceError` into it is
///   `knobas-sync`'s work, three crates from the adapter.
/// * **The `source:health` event reaches the shell.** The stored column and
///   the event are different claims: a sources view that never heard would
///   show a stale monogram until something else redrew it.
/// * **The mirror is exactly as the good run left it**, cursor included.
///
/// # How this differs from the Jira half
///
/// [`a_revoked_pat_reaches_the_credential_health_surface_and_the_mirror_survives`]
/// carries a hazard that does not arise here. On Jira an unresolvable bearer
/// token never reaches Seraph and the request proceeds *anonymously*, so
/// `/rest/api/2/search` answers `200` with `total: 0` -- an answer a run could
/// report as a completed full sync, which for an `full_sync_exhaustive` kind
/// is a licence to tombstone the lot (#276). Confluence closes that at the
/// wire: #284 measured the same bad bearer as a `401` on the content search
/// too, so there is no plausible `Ok`-with-nothing to mistake.
///
/// So this test does not re-witness that hazard, and saying otherwise would be
/// a lie about what it measures. The `page` kind *is* `full_sync_exhaustive`
/// all the same, and the refused run here is an **incremental** one (the good
/// run before it stored a cursor), so what the last two assertions hold shut
/// is narrower and worth naming exactly: a refused run commits **nothing** --
/// not a mirror row, not a tombstone, not a cursor. The engine's failure path,
/// measured against a real refusal rather than a fake one.
///
/// **Never a wrong password.** The refusal is a bearer token and nothing else;
/// a few failed password logins lock the seed admin out for the *correct*
/// password too, which is why `AuthKind` is `Pat` from the insert onwards and
/// [`wiki_app`] takes the method as a parameter.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs testenv's seeded Confluence: `just atlassian-live`"]
async fn a_revoked_confluence_pat_reaches_the_credential_health_surface_and_the_mirror_survives() {
    let wiki = wiki();
    wiki.clear_leftover_tokens().await;
    let pat = wiki.pat().await;

    let (state, events) = wiki_app(
        "atlassian_live_wiki_health",
        &wiki,
        AuthMethod::Pat,
        &pat.raw,
    )
    .await;

    // 1. The token works, which is what makes revoking it mean anything -- and
    //    is the product's own answer to `http::credential`'s claim that a
    //    Confluence DC personal access token is a Bearer token. Nothing else
    //    in the repo presents this server a *good* one.
    sync(&state, CONFLUENCE).await;
    let synced = confluence_pages(&state.pool).await;
    assert!(
        synced.len() >= 5,
        "`fixtures/tidewater/work.json` names five pages, and `seed-atlassian-content.sh` creates \
         each of them under the space home page it reads off the space, or under one of its \
         siblings there -- so a walk of the space in fact answers six. The bound is the fixture's \
         five and not the six, because the sixth is the seed's own scaffolding and this \
         assertion is about the corpus arriving under a personal access token; the exact set is \
         pinned at step 4 instead: {synced:?}"
    );
    assert!(
        synced
            .iter()
            .any(|id| id == &format!("confluence:{}", wiki.page)),
        "the seeded page the mention test uses is among them: {synced:?}"
    );
    let healthy = knobas_sync::config::get(&state.pool, CONFLUENCE)
        .await
        .expect("the source row")
        .expect("the source this test configured");
    assert_eq!(healthy.health.state, AuthState::Ok, "{:?}", healthy.health);
    assert_eq!(
        events.states(),
        vec![AuthState::Ok],
        "the sources view is told once that the credential works"
    );
    let cursor = healthy.cursor.clone();
    assert!(
        cursor.is_some(),
        "a completed sync stores a cursor, which is what makes the next run incremental and the \
         last assertion here a claim about a cursor that exists: {healthy:?}"
    );

    // The stamp the *good* run left, for the comparison at step 3: every
    // verdict writes `auth_checked_at = now()`, so the column is already
    // non-null here and a bare `is_some()` below would pass whether or not the
    // refused run ever reached it.
    let checked_when_healthy = healthy.health.checked_at;

    // 2. The token stops working -- here by swapping what the keychain holds
    //    for a string this Confluence never issued, which is the shape
    //    `live_confluence_seeded.rs`'s own `bad_token()` uses, and which leaves
    //    the real token for the guard to revoke. What it is *not* is a revoke
    //    at the server: what the next run measures is a bearer the server
    //    cannot **resolve** -- the 401 #284 recorded -- and not a token whose
    //    row Confluence has deleted.
    //
    //    Bound rather than inlined because it is the secret **in play** for
    //    the refused run, and so the one the §14 assertion has to name.
    let refused_secret = format!("revoked-{}", std::process::id());
    state
        .secrets
        .put(
            &knobas_secrets::KeychainAccount::source(CONFLUENCE),
            &Secret::just(AuthMethod::Pat, refused_secret.clone()),
        )
        .expect("the replacement credential is stored");

    sync(&state, CONFLUENCE).await;

    // 3. The credential-health path, at both ends of it: the stored column the
    //    sources view polls, and the event it re-renders on.
    let refused = knobas_sync::config::get(&state.pool, CONFLUENCE)
        .await
        .expect("the source row")
        .expect("the source this test configured");
    assert_eq!(
        refused.health.state,
        AuthState::Unauthorized,
        "a credential the server refuses is `unauthorized`, which is what puts *Re-enter* on the \
         row (interfaces §3): {:?}",
        refused.health
    );
    assert!(
        refused.health.checked_at > checked_when_healthy,
        "the refusal stamps `auth_checked_at` itself -- the column the sources view reads as \
         *when this was last asked*. Compared against the good run's stamp and not merely for \
         non-null, because the good run already filled it in: {:?} vs {checked_when_healthy:?}",
        refused.health
    );
    let event = events.last();
    assert_eq!(event.state, AuthState::Unauthorized);
    assert_eq!(event.source_id, CONFLUENCE);
    let detail = event.detail.clone().unwrap_or_default();
    assert!(
        detail.contains("unauthorized"),
        "the detail line names the fault the row is in: {detail:?}"
    );
    // **`refused_secret` first, and it is the one that does the work**, for
    // the reason the Jira half above gives at length: the bearer this run
    // presented is the replacement, so an implementation that echoed the
    // presented credential into the detail would be caught by that name and
    // not by `pat.raw`, which this source stopped using a run ago.
    assert!(
        !detail.contains(&refused_secret) && !detail.contains(&pat.raw),
        "spec §14: a health detail is never a place a secret can reach: {detail:?}"
    );
    println!("SEEDED Confluence credential health after the revoke: {event:?}");

    // 4. **And the mirror is untouched**, cursor included: a refused run
    //    commits nothing at all.
    assert_eq!(
        confluence_pages(&state.pool).await,
        synced,
        "a refused sync must leave the mirror exactly as it was"
    );
    assert_eq!(
        refused.cursor, cursor,
        "a refused sync must not move the cursor either -- a cleared one would make the next run \
         a full, sweeping one over a source whose `page` kind declares `full_sync_exhaustive`"
    );

    state.scheduler.shutdown().await;
}

// -- the standup protocol, published into the real Confluence (#289) ---------

/// The date this suite's protocol is for.
///
/// Fixed and far in the past, for two reasons: the page's title *is* the date,
/// so a leftover from a killed run is recognisable by name without a marker in
/// its body, and a date nobody's real standup will ever be about cannot
/// collide with a page a person made.
const PROTOCOL_DAY: &str = "2009-02-13";

/// The page the publish makes, taken back out when the guard drops.
///
/// A guard rather than a `clear_leftovers` sweep alone, for the reason every
/// other guard in this file exists: the seeded server is a shared fixture and
/// a suite that leaves a page behind has changed what the next suite reads.
struct Protocol {
    url: String,
    user: String,
    password: String,
    id: String,
}

impl Drop for Protocol {
    fn drop(&mut self) {
        let (url, user, password, id) = (
            self.url.clone(),
            self.user.clone(),
            self.password.clone(),
            self.id.clone(),
        );
        undo("the published protocol page", move || async move {
            let http = client();
            let (status, body) = api(
                &http,
                &url,
                &user,
                &password,
                reqwest::Method::DELETE,
                &format!("rest/api/content/{id}"),
                None,
            )
            .await;
            if status != 204 && status != 200 && status != 404 {
                return Err(format!("DELETE page {id} -> {status}: {body}"));
            }
            // Trashed rather than purged, and this path answers 404 for a
            // trashed page -- the reading `Mention`'s guard records.
            let (status, _) = api(
                &http,
                &url,
                &user,
                &password,
                reqwest::Method::GET,
                &format!("rest/api/content/{id}"),
                None,
            )
            .await;
            if status != 404 {
                return Err(format!("page {id} still answers {status} after its delete"));
            }
            // **And wait for the CQL index to give it up**, which is a
            // separate fact from the page being gone.
            //
            // Measured, not assumed: this run deleted the page, verified the
            // 404, and `live_confluence_seeded`'s
            // `a_full_sync_mirrors_every_seeded_page_of_the_space` -- which
            // asserts the space holds nothing the seed does not know about --
            // failed seconds later with the deleted page still in its results.
            // Confluence writes that index asynchronously in *both*
            // directions, and this suite is the one that pushed the page into
            // it (the publish test polls until it appears). So the wait is
            // symmetric: a suite that put a page in a shared index takes it
            // back out of the index, not merely out of the API, before the
            // next suite reads it.
            //
            // A one-second tick rather than a busy loop, because there is no
            // sync run here to make time pass -- and a bound rather than a
            // guessed sleep, so the failure names what did not happen.
            let deadline = std::time::Instant::now() + INDEX_BUDGET;
            loop {
                let (status, listing) = api(
                    &http,
                    &url,
                    &user,
                    &password,
                    reqwest::Method::GET,
                    // The adapter's own query, which is what the next suite
                    // asks: `type = page`, percent-encoded.
                    "rest/api/content/search?cql=type%20%3D%20page&limit=200",
                    None,
                )
                .await;
                if status != 200 {
                    return Err(format!(
                        "CQL search after deleting {id} -> {status}: {listing}"
                    ));
                }
                let indexed = listing["results"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|page| page["id"].as_str())
                    .any(|found| found == id);
                if !indexed {
                    return Ok(());
                }
                if std::time::Instant::now() >= deadline {
                    return Err(format!(
                        "page {id} is deleted and answers 404, but Confluence's CQL index still \
                         lists it after {INDEX_BUDGET:?} -- the next suite's full sync would read \
                         it as a page the seed knows nothing about"
                    ));
                }
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        });
    }
}

impl Wiki {
    /// Delete every child of *Standup protocols* a killed run left behind.
    ///
    /// Recognised by **title**, which for a protocol page is the date and
    /// nothing else. Read-only in the ordinary case: the seed leaves that page
    /// childless.
    async fn clear_protocol_leftovers(&self) {
        let (status, body) = self
            .api(
                reqwest::Method::GET,
                &format!(
                    "rest/api/content/{}/child/page?limit=100",
                    self.standup_parent
                ),
                None,
            )
            .await;
        assert_eq!(
            status, 200,
            "listing children of {}: {body}",
            self.standup_parent
        );
        for id in body["results"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|page| page["title"] == json!(PROTOCOL_DAY))
            .filter_map(|page| page["id"].as_str().map(str::to_owned))
        {
            let (status, body) = self
                .api(
                    reqwest::Method::DELETE,
                    &format!("rest/api/content/{id}"),
                    None,
                )
                .await;
            assert!(
                status == 204 || status == 200,
                "deleting the leftover protocol page {id}: {status} {body}"
            );
            println!("live suite: deleted leftover protocol page {id}");
        }
    }
}

/// **M3.3's exit criterion for the protocol: it is published to the real
/// Confluence under *Standup protocols*, reads back with its body, and the
/// note and the page are linked** (#289, spec #272 stories 64-67).
///
/// The sequence, and every step of it is a fact the offline batteries cannot
/// establish:
///
/// 1. A full sync, so the *Standup protocols* page is in the mirror -- which
///    is what makes it pickable as a parent and what `space_of` reads the
///    space key off.
/// 2. Get-or-create the protocol, and type into it.
/// 3. *Publish*, with the target the dialog would have produced. The write
///    goes through the queue, the real adapter and the real REST API.
/// 4. The publication settled **sent**, carrying the id Confluence gave the
///    page.
/// 5. A backfill, then the mirror asserted directly, then the ordinary read --
///    the one the standup view makes on open -- and *then* the link, from both
///    ends. The order is the point: an incremental run reads CQL, an index
///    Confluence writes asynchronously, so the page a publish just made is
///    routinely not in the mirror when the publish returns. That is the
///    product working -- `reconcile` runs on every read for this reason, and
///    the panel says so on screen -- and asserting the mirror before anything
///    derived from it is what makes a failure name the step that did not
///    happen.
/// 6. The page is read back **from Confluence, not from the mirror**: its
///    title is the date, its parent is *Standup protocols*, and its stored
///    body carries the markup the note's markdown became. A mirror read would
///    only prove knobas agrees with itself.
///
/// The duplicate-title ruling has a test of its own below.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs testenv's seeded Confluence: `just atlassian-live`"]
async fn a_protocol_is_published_under_standup_protocols_and_reads_back() {
    let wiki = wiki();
    wiki.clear_protocol_leftovers().await;
    // User + password, the way the mention test signs in: this criterion is
    // about publishing as the seeded admin, and #317's bearer-token variant
    // exists for the credential-health path alone.
    let (state, _events) = wiki_app(
        "atlassian_live_protocol",
        &wiki,
        AuthMethod::UserPassword,
        &wiki.password,
    )
    .await;
    let day: chrono::NaiveDate = PROTOCOL_DAY.parse().expect("the date parses");

    // 1. The seeded corpus, so the parent page has an address.
    sync(&state, CONFLUENCE).await;
    let parent = format!("{CONFLUENCE}:{}", wiki.standup_parent);

    // 2. The protocol, and what was said at the standup.
    let opened = knobas_app::commands::entity::standup_protocol_inner(&state.pool, day)
        .await
        .expect("the protocol opens");
    assert!(
        opened.publication.is_none(),
        "a fresh date has not been published"
    );
    knobas_app::commands::entity::save_note_inner(
        &state.pool,
        &opened.note_id,
        &knobas_app::protocol::title_of(day),
        "## Attendees\n\n- Mara\n- Jonas\n\n## Action items\n\n- [ ] Ask Ines about the retry\n",
    )
    .await
    .expect("the note saves");

    // 3. Publish, with the answer the first publish's dialog would have given.
    let published = knobas_app::commands::entity::publish_standup_protocol_inner(
        &state,
        day,
        Some(knobas_app::protocol::PublishTarget {
            source_id: CONFLUENCE.to_owned(),
            parent: parent.clone(),
        }),
    )
    .await
    .expect("the protocol publishes");

    // 4. What the queue says.
    let publication = published.publication.expect("there is a publication");
    assert_eq!(
        publication.state,
        knobas_core::write_queue::WriteState::Sent,
        "the write reached the real Confluence: {:?}",
        publication.detail
    );
    let page_entity = publication
        .page_entity_id
        .clone()
        .expect("Confluence named the page it made");
    let content_id = page_entity
        .split_once(':')
        .expect("an entity id")
        .1
        .to_owned();
    // From here on the page exists on a shared server, so the guard is armed
    // before anything else can fail.
    let _guard = Protocol {
        url: wiki.url.clone(),
        user: wiki.user.clone(),
        password: wiki.password.clone(),
        id: content_id.clone(),
    };
    // **Not linked yet, and that is the product working.** `submit` re-reads the
    // source when a write lands, but a Confluence incremental run reads CQL --
    // an index written *asynchronously* -- so the page it just made is
    // routinely not there yet. `knobas.link`'s endpoints are `knobas.entity`
    // rows, so no link can be drawn until a run has seen the page, and
    // `protocol::reconcile` is written to run on every read for exactly this
    // reason. The panel says so on screen; `a_page_the_mirror_has_not_seen_yet_
    // is_named_but_not_linked` pins the state offline.
    //
    // So: **backfill, and poll**. A backfill walks from no position at all --
    // the reasoning `backfill` records for Jira's assignee one product over
    // (#345) -- but it is not enough on its own here, and two live runs proved
    // it: this adapter reads CQL for *both* its runs, and CQL answers from an
    // index Confluence writes after the create has already returned an id. So
    // the run that matters is not the next one but the first one after the
    // index catches up, and this polls to `INDEX_BUDGET` the way
    // `a_comment_that_mentions_me_becomes_an_inbox_mention` polls against the
    // same index -- never a sleep somebody guessed at.
    let deadline = std::time::Instant::now() + INDEX_BUDGET;
    loop {
        backfill(&state, CONFLUENCE).await;
        // The **mirror's own state**, asked directly and before anything
        // derived from it, so a failure names the step that did not happen
        // rather than the one that could not have.
        let mirrored: Option<String> =
            sqlx::query_scalar("select entity_id from sync.live_item where entity_id = $1")
                .bind(&page_entity)
                .fetch_optional(&state.pool)
                .await
                .expect("the mirror reads");
        if mirrored.is_some() {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the published page {page_entity} never reached the mirror in {INDEX_BUDGET:?} of \
             backfilling -- Confluence's CQL index never listed the page its own create had \
             already answered for, so nothing downstream could have linked it"
        );
    }

    // Now the ordinary read -- the one the standup view makes on open -- draws
    // the link, from the id the settle wrote onto the queue row.
    let reopened = knobas_app::commands::entity::standup_protocol_inner(&state.pool, day)
        .await
        .expect("the protocol reopens");
    let publication = reopened
        .publication
        .expect("the publication is still there");
    assert_eq!(
        publication.page_entity_id.as_deref(),
        Some(page_entity.as_str()),
        "the id came off the queue row, which is where the settle wrote it"
    );
    assert!(publication.linked, "the note and the page are linked");

    let linked: Vec<(String, String, String)> = sqlx::query_as(
        "select from_id, to_id, relation from knobas.confirmed_link
          where from_id = $1 or to_id = $1",
    )
    .bind(&opened.note_id)
    .fetch_all(&state.pool)
    .await
    .expect("the note's links read");
    assert_eq!(
        linked,
        [(
            opened.note_id.clone(),
            page_entity.clone(),
            knobas_app::protocol::PUBLISHED_RELATION.to_owned()
        )],
        "one link, and both details read this row"
    );
    // The other end, as the *page's* detail asks it.
    let from_the_page: Vec<String> = sqlx::query_scalar(
        "select from_id from knobas.confirmed_link where to_id = $1 or from_id = $1",
    )
    .bind(&page_entity)
    .fetch_all(&state.pool)
    .await
    .expect("the page's links read");
    assert_eq!(from_the_page, std::slice::from_ref(&opened.note_id));

    // 6. The page, read back from Confluence itself.
    let (status, body) = wiki
        .api(
            reqwest::Method::GET,
            &format!("rest/api/content/{content_id}?expand=body.storage,ancestors,space"),
            None,
        )
        .await;
    assert_eq!(status, 200, "reading the published page back: {body}");
    assert_eq!(
        body["title"],
        json!(PROTOCOL_DAY),
        "the page is titled with the date"
    );
    assert_eq!(body["space"]["key"], json!(wiki.space));
    let ancestors: Vec<&str> = body["ancestors"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|a| a["id"].as_str())
        .collect();
    assert!(
        ancestors.contains(&wiki.standup_parent.as_str()),
        "the page sits under Standup protocols; its ancestors are {ancestors:?}"
    );
    let stored = body["body"]["storage"]["value"]
        .as_str()
        .expect("a storage body");
    for expected in [
        "<h2>Attendees</h2>",
        "<li>Mara</li>",
        "Ask Ines about the retry",
    ] {
        assert!(
            stored.contains(expected),
            "the published body is missing {expected:?}: {stored}"
        );
    }
    println!(
        "live suite: published protocol page {content_id} under {}",
        wiki.standup_parent
    );
}

/// **The duplicate-title ruling's second layer, against the real product**
/// (#289).
///
/// knobas refuses to *ask* twice -- `protocol::publish` answers with the
/// publication already on the queue, and `publishing_a_date_twice_queues_one_page`
/// is that half's witness. What that cannot cover is the ask knobas does not
/// know it made: delivery is at-least-once (ADR-0012), a `POST` whose response
/// was lost is a page that exists with knobas none the wiser, and the queue
/// re-sends. Unlike `UpdatePage` there is no version for the server to check.
///
/// The claim the ruling rests on is therefore a claim about **Confluence**:
/// a page title is unique within its space, so the redelivery comes back a
/// refusal rather than a second page. Atlassian publishes no machine-readable
/// specification for this product (ADR-0013), so the only way to know it is to
/// ask the product -- which is what this does, by sending the *same* create
/// twice over REST, exactly as a re-sent write would.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs testenv's seeded Confluence: `just atlassian-live`"]
async fn a_second_page_with_one_title_in_one_space_is_refused() {
    let wiki = wiki();
    wiki.clear_protocol_leftovers().await;

    let create = json!({
        "type": "page",
        "title": PROTOCOL_DAY,
        "space": { "key": wiki.space },
        "ancestors": [{ "id": wiki.standup_parent }],
        "body": { "storage": { "value": "<p>first</p>", "representation": "storage" } },
    });

    let (status, body) = wiki
        .api(
            reqwest::Method::POST,
            "rest/api/content",
            Some(create.clone()),
        )
        .await;
    assert_eq!(status, 200, "the first create: {body}");
    let id = body["id"].as_str().expect("an id").to_owned();
    let _guard = Protocol {
        url: wiki.url.clone(),
        user: wiki.user.clone(),
        password: wiki.password.clone(),
        id: id.clone(),
    };

    // The same request again -- what an at-least-once redelivery is.
    let (status, body) = wiki
        .api(reqwest::Method::POST, "rest/api/content", Some(create))
        .await;
    assert_eq!(
        status, DUPLICATE_TITLE_REFUSED,
        "a second page with one title in one space must be refused with \
         {DUPLICATE_TITLE_REFUSED}, and this Confluence answered {status}: {body}\n\
         \n\
         What another answer would mean, since only one of them is about titles:\n\
         * **2xx** -- the product made the page. `knobas_app::protocol`'s duplicate ruling \
           has lost its backstop: delivery is at-least-once (ADR-0012), so a re-sent create \
           now leaves two pages. Re-decide the ruling; do not relax this number.\n\
         * **401 or 403** -- nothing was learned about titles. The timebomb licence lapsed, \
           or the seed admin's credential did, and the request never reached the duplicate \
           check. Re-seed and run again.\n\
         * **another 4xx** -- Confluence refused this request for a reason of its own. The \
           create above is byte-for-byte the one it accepted a moment ago, so read its answer \
           before changing the number here.\n\
         * **5xx** -- the server, not the rule."
    );
    println!("live suite: the duplicate create was refused with {status}");

    // And there is still exactly one page by that title under the parent.
    let (status, listing) = wiki
        .api(
            reqwest::Method::GET,
            &format!(
                "rest/api/content/{}/child/page?limit=100",
                wiki.standup_parent
            ),
            None,
        )
        .await;
    assert_eq!(status, 200, "listing the parent's children: {listing}");
    let named: Vec<&str> = listing["results"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|page| page["title"] == json!(PROTOCOL_DAY))
        .filter_map(|page| page["id"].as_str())
        .collect();
    assert_eq!(named, [id.as_str()], "one page, not two");
}
