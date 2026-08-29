//! Stream C's live certification: the adapter against a **real** TeamCity.
//!
//! Everything else in this crate runs against `knobas-mockd` and an in-crate
//! fake. Both serve what they were written to serve, in the order they were
//! written to serve it, so both agree with the adapter by construction. This
//! suite asks the server that decides. **If the two disagree, the fake is
//! wrong**, and the fix goes there (issue #35 task 8's standing rule, now
//! applied to a second adapter).
//!
//! The argument for the file is that the first hour of real-server contact
//! found a permanent sync deadlock (#91): the ceiling read row 0 of a page as
//! the newest build in existence, and a current TeamCity does not answer that
//! page in id order. Nothing docker-free could have caught it.
//!
//! `#[ignore]`d, so `just check` runs none of it:
//!
//! ```text
//! cp .env.example .env        # the default URL already works
//! just teamcity-live
//! ```
//!
//! # How this differs from `live_gitea.rs`
//!
//! **There is no container to seed, and the server is not ours.** Gitea's
//! suite owns a pinned container it seeded itself, so it can assert seeded
//! titles and can open a real pull request through the API. This one points at
//! JetBrains' public instance, whose corpus changes between one request and
//! the next.
//!
//! Two rules follow, and both are load-bearing:
//!
//! * **Read-only. Every request here is a GET.** Nothing in this file may
//!   create, cancel, trigger or tag anything, whatever a future assertion
//!   would be worth.
//! * **Every assertion is by *form*.** No fixed id, no count, no title, no
//!   ordering that the server does not promise. Where the only honest thing to
//!   do is record what the server answered, the test prints the finding and
//!   asserts the shape around it -- the pattern `live_gitea.rs`'s ETag probe
//!   established.
//!
//! # Why the contract battery is not run here
//!
//! It is, against `knobas-mockd` (`tests/mockd.rs`). Clause 2 -- an
//! incremental run after no changes emits nothing and hands back the same
//! cursor -- is not a property any public build server can be held to: builds
//! finish between two runs, and a running build in scope is re-emitted by
//! design. Running it here would produce a flake whose failures mean nothing,
//! which is worse than not running it.

use knobas_source::contract::VecSink;
use knobas_source::instance::SourceInstance;
use knobas_source::{AuthMethod, Source, SourceError, SyncItem};

/// Read from the environment rather than hardcoded, so the suite can be
/// pointed at a private TeamCity without a code change; `.env.example` carries
/// the public default.
struct Live {
    url: String,
    secret: String,
    http: reqwest::Client,
}

/// `None` when the suite has nothing to talk to.
///
/// The **URL alone** decides. A token is optional because the instance
/// `.env.example` documents needs none: `/guestAuth` is a path prefix that
/// authenticates the request, it ignores an `Authorization` header entirely,
/// and `knobas-http` keeps a path prefix because it builds URLs by
/// concatenation. The adapter refuses a *missing* secret outright, so an empty
/// token becomes a placeholder that guestAuth discards -- and on a server that
/// does want a credential the run fails as `Unauthorized`, which is the honest
/// outcome there rather than a silent skip.
fn live() -> Option<Live> {
    let url = std::env::var("KNOBAS_TEAMCITY_URL").ok()?;
    let url = url.trim().trim_end_matches('/');
    if url.is_empty() {
        return None;
    }
    let secret = std::env::var("KNOBAS_TEAMCITY_TOKEN")
        .unwrap_or_default()
        .trim()
        .to_owned();
    Some(Live {
        url: url.to_owned(),
        // Never empty: `http::credential` maps a missing secret to
        // `Unauthorized` before a request is ever sent.
        secret: if secret.is_empty() {
            "guest-no-token-needed".to_owned()
        } else {
            secret
        },
        // Not `Client::new()`. `knobas-http` calls a bare client "a client
        // with no rate limiter, no retry budget, no `Retry-After`", and the
        // budget being spent here is somebody else's. The recipe runs this
        // suite serially for the same reason; a timeout keeps a wedged
        // request from holding that serial queue open, and the user agent
        // means the server's operators can see who is asking.
        http: reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .user_agent(concat!(
                "knobas-live-certification/",
                env!("CARGO_PKG_VERSION")
            ))
            .build()
            .expect("a reqwest client with a timeout"),
    })
}

/// Skip, loudly and by name, rather than fail: absent credentials are not a
/// defect in the adapter (this issue's own acceptance criterion).
macro_rules! live_or_skip {
    () => {
        match live() {
            Some(live) => live,
            None => {
                println!(
                    "SKIP: KNOBAS_TEAMCITY_URL is not set, so there is no server to certify \
                     against. `cp .env.example .env` -- its default value is a public, \
                     guest-readable TeamCity and needs no token -- then `just teamcity-live`."
                );
                return;
            }
        }
    };
}

impl Live {
    /// One raw `GET`, as the adapter sends them: JSON demanded explicitly
    /// (without it a real TeamCity answers XML) and an `Authorization` header
    /// that a `/guestAuth` URL ignores.
    ///
    /// Raw rather than through the adapter wherever the fact under test does
    /// not survive mapping -- an HTTP status, a page's order, a field the
    /// mapping deliberately drops.
    async fn get(&self, path_and_query: &str) -> (u16, serde_json::Value) {
        let response = self
            .http
            .get(format!("{}/{path_and_query}", self.url))
            .header("Accept", "application/json")
            .header("Authorization", format!("Bearer {}", self.secret))
            .send()
            .await
            .unwrap_or_else(|e| panic!("GET {path_and_query}: {e}"));
        let status = response.status().as_u16();
        let body = response.text().await.unwrap_or_default();
        let json = serde_json::from_str(&body).unwrap_or(serde_json::Value::String(body));
        (status, json)
    }

    /// `GET /app/rest/builds?locator=…&fields=…`, unwrapped to its elements.
    async fn builds(&self, locator: &str, fields: &str) -> Vec<serde_json::Value> {
        let (status, body) = self
            .get(&format!(
                "app/rest/builds?locator={locator}&fields=count,build({fields})"
            ))
            .await;
        assert_eq!(status, 200, "locator {locator:?} answered {body}");
        assert!(
            body.get("count")
                .and_then(serde_json::Value::as_u64)
                .is_some(),
            "every collection carries the count of the page it is: {body}"
        );
        body.get("build")
            .and_then(|b| b.as_array().cloned())
            .unwrap_or_default()
    }

    fn source(&self, config: serde_json::Value) -> Box<dyn Source> {
        self.source_at(&self.url, config)
    }

    /// The adapter against a base URL that is not necessarily the configured
    /// one -- the un-prefixed root, for the 401 certification below.
    fn source_at(&self, base_url: &str, config: serde_json::Value) -> Box<dyn Source> {
        match knobas_source_teamcity::build(SourceInstance {
            id: "teamcity".to_owned(),
            kind: knobas_source_teamcity::ADAPTER_KIND.to_owned(),
            display_name: "the live instance".to_owned(),
            base_url: base_url.to_owned(),
            auth: Some(AuthMethod::Pat),
            secret: Some(self.secret.clone()),
            config,
        }) {
            Ok(s) => s,
            // `Box<dyn Source>` is not `Debug`, so `expect` is unavailable.
            Err(e) => panic!("the adapter must build against the live URL: {e:?}"),
        }
    }

    /// The adapter, scoped to one configuration.
    ///
    /// A full sync issues one request per configuration in scope, and this
    /// server has thousands of them. Scoping is what makes an end-to-end run
    /// affordable — for the suite and, more to the point, for a server nobody
    /// here is paying for.
    fn scoped_to(&self, build_type_id: &str) -> Box<dyn Source> {
        self.source(serde_json::json!({ "build_type_ids": [build_type_id] }))
    }
}

async fn full(source: &dyn Source) -> (Vec<SyncItem>, String) {
    let mut sink = VecSink(Vec::new());
    let cursor = source
        .sync(None, &mut sink)
        .await
        .expect("a full sync against the live server");
    (sink.0, cursor)
}

fn of_kind<'a>(items: &'a [SyncItem], kind: &str) -> Vec<&'a SyncItem> {
    items.iter().filter(|i| i.kind == kind).collect()
}

/// Every test here scopes itself to a configuration it *discovered*, so this
/// walk appears once per test; `id_of` is the same idea for the other half of
/// the pair.
fn build_type_of(build: &serde_json::Value) -> String {
    build
        .get("buildTypeId")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_else(|| panic!("a build names its configuration: {build}"))
        .to_owned()
}

fn id_of(build: &serde_json::Value) -> i64 {
    build
        .get("id")
        .and_then(serde_json::Value::as_i64)
        .unwrap_or_else(|| panic!("a build with no numeric id: {build}"))
}

/// The two endpoints *Test connection* is built on, and the two `fields=`
/// selectors nothing else sends.
///
/// Through the adapter rather than raw, because the selectors are the
/// adapter's own and a transcription of them here could drift from the ones it
/// really sends — which is exactly the class of disagreement this suite exists
/// to close. A wrong name is a 400 on a real TeamCity, so reaching a version
/// string at all certifies `SERVER_FIELDS`; `USER_FIELDS` is certified when
/// the account comes back and is deliberately not required, because the
/// adapter treats `/app/rest/users/current` as the nicer half of the report.
#[tokio::test]
#[ignore = "needs a live TeamCity: `just teamcity-live`"]
async fn test_connection_names_the_server_and_needs_no_token_on_guestauth() {
    let live = live_or_skip!();
    let info = live
        .source(serde_json::json!({}))
        .test_connection()
        .await
        .expect("the live server answers /app/rest/server");
    let version = info
        .server_version
        .as_deref()
        .expect("a TeamCity names its version");
    assert!(!version.trim().is_empty());
    assert_eq!(
        info.secret_expires_at, None,
        "TeamCity publishes no token expiry over REST, so the health strip shows no countdown"
    );
    println!("LIVE server: version={version:?} detail={:?}", info.detail);
    println!("LIVE account: {:?}", info.account);
}

/// Interfaces §4.2's key forms and `SyncItem` shapes, against records the real
/// server issued — the whole of what the fake asserts, re-asserted here.
///
/// Every assertion is a form. The configuration is *discovered* from a recent
/// build rather than named, because a named one would be a fixed id in a
/// corpus that changes, and because a configuration that is deleted upstream
/// would otherwise fail this suite for a reason that has nothing to do with
/// the adapter.
#[tokio::test]
#[ignore = "needs a live TeamCity: `just teamcity-live`"]
async fn a_scoped_full_sync_lands_under_the_documented_key_forms() {
    let live = live_or_skip!();
    let recent = live
        .builds("state:finished,count:1", "id,buildTypeId")
        .await;
    let build_type = build_type_of(recent.first().expect("the server has a finished build"));
    println!("LIVE scope: buildType {build_type}");

    let (items, cursor) = full(&*live.scoped_to(&build_type)).await;

    let configs = of_kind(&items, "build_config");
    assert_eq!(
        configs
            .iter()
            .map(|i| i.entity.key.as_str())
            .collect::<Vec<_>>(),
        vec![format!("buildType:{build_type}").as_str()],
        "the configuration key is buildType:<id>, and the scope holds"
    );
    // The configuration carries the same shape the builds do; `tests/mockd.rs`
    // asserts all of it against the fake, so all of it is re-asserted here.
    for item in &configs {
        assert!(
            !item.title.trim().is_empty(),
            "every configuration has a title"
        );
        assert!(
            item.body_text.contains(&item.title),
            "the indexed blob opens with the title: {:?}",
            item.body_text
        );
        assert!(
            item.web_url.is_some(),
            "every configuration is openable in the browser (P5): {:?}",
            item.title
        );
        assert!(
            item.payload.get("id").is_some(),
            "the payload is the record verbatim (spec §3a), not a re-serialisation"
        );
        assert!(!item.deleted, "M1 has no deletion channel for TeamCity");
    }

    let builds = of_kind(&items, "build");
    assert!(
        !builds.is_empty(),
        "a configuration with a finished build must mirror at least one"
    );
    for item in &builds {
        let id = item
            .entity
            .key
            .strip_prefix("build:")
            .expect("build keys are build:<id>");
        assert!(
            id.parse::<i64>().is_ok(),
            "the build key carries the numeric id, not the build number: {:?}",
            item.entity.key
        );
        assert!(!item.title.trim().is_empty(), "every build has a title");
        assert!(
            item.body_text.contains(&item.title),
            "the indexed blob opens with the title: {:?}",
            item.body_text
        );
        assert!(
            item.web_url.is_some(),
            "every build is openable in the browser (P5): {:?}",
            item.title
        );
        assert!(
            item.updated_at.is_some(),
            "TeamCity stamps yyyyMMdd'T'HHmmssZ, not RFC 3339; a None here means `parse_ts` \
             stopped understanding what the server sends: {:?}",
            item.payload.get("finishDate")
        );
        assert!(
            item.payload.get("id").is_some(),
            "the payload is the record verbatim (spec §3a), not a re-serialisation"
        );
        assert!(!item.deleted, "M1 has no deletion channel for TeamCity");
    }

    let state: serde_json::Value = serde_json::from_str(&cursor).expect("the cursor is JSON");
    assert_eq!(state.get("v").and_then(serde_json::Value::as_u64), Some(1));
    let watermark = state
        .get("since_build_id")
        .and_then(serde_json::Value::as_i64)
        .expect("the cursor carries a build id");
    assert!(watermark > 0, "a run that emitted builds names a position");
    println!("LIVE cursor: {cursor}");
}

/// Issue #91's deadlock, live: a watermark at the top of what a probe page
/// showed must not make the next run refuse.
///
/// The cursor handed in is the **maximum id of a page the server just
/// answered** — a position a healthy run can genuinely reach, and the one the
/// fix computes. Against the adapter as it stood before #91, the next run
/// re-probes, reads *row 0* of a page that is not ordered, finds it below that
/// watermark and refuses with "the newest build this server reports is …",
/// permanently: resetting the cursor only restarts the loop. On the live
/// instance the gap was real and routine — one page's row 0 was 6518363 while
/// the same page's maximum was 6520204.
///
/// The run is allowed to reach the server's evidence either way: if a later
/// page happens not to show a build that new, the adapter asks
/// `/app/rest/builds/id:{watermark}`, is told the build exists, and carries
/// on. Refusing is reserved for a 404. So the assertion is simply that the run
/// **completes**.
///
/// **This test needs the #91 fix.** Run against the adapter without it, it
/// fails — which is the finding, not a flake. Fable's ruling of 2026-08-29
/// anticipated it: neither PR blocks the other, and whichever lands second
/// re-verifies against the first.
#[tokio::test]
#[ignore = "needs a live TeamCity: `just teamcity-live`"]
async fn a_watermark_at_the_top_of_a_page_does_not_wedge_the_next_run() {
    let live = live_or_skip!();
    let page = live
        .builds("defaultFilter:false,count:100", "id,buildTypeId")
        .await;
    let watermark = page.iter().map(id_of).max().expect("the server has builds");
    let build_type = build_type_of(page.first().expect("the server has builds"));
    println!("LIVE wedge check: resuming from the page maximum {watermark}");

    let source = live.scoped_to(&build_type);
    let mut sink = VecSink(Vec::new());
    let cursor = source
        .sync(
            Some(format!(r#"{{"v":1,"since_build_id":{watermark}}}"#)),
            &mut sink,
        )
        .await
        .unwrap_or_else(|e| {
            panic!(
                "the run refused a watermark this server's own page maximum produced. That is \
                 issue #91: the ceiling read row 0 of an unordered page as the newest build in \
                 existence, so the source became unsyncable after its second run. {e:?}"
            )
        });
    let next = serde_json::from_str::<serde_json::Value>(&cursor)
        .ok()
        .and_then(|v| v["since_build_id"].as_i64())
        .expect("a cursor this adapter wrote");
    // `cursor::advance` ends in `next.max(previous)` and a zero-item run hands
    // `cursor_in` back byte-for-byte, so this cannot fail against the adapter
    // as it stands. It is here as the statement of the invariant a future edit
    // to `cursor.rs` would have to break -- the *live* subject of this test is
    // the `unwrap_or_else` above, which is where #91 landed.
    assert!(
        next >= watermark,
        "the watermark must never regress: {watermark} -> {next}"
    );
    println!(
        "LIVE wedge check: {watermark} -> {next}, {} items",
        sink.0.len()
    );
}

/// The `state` vocabulary the run classifies on, and the `status` vocabulary
/// the mapping renders.
///
/// `StateFilter::matches` re-answers the server's own question locally — the
/// in-flight poll is global and its results are re-classified client-side — so
/// a fourth state name would silently drop builds out of both buckets.
#[tokio::test]
#[ignore = "needs a live TeamCity: `just teamcity-live`"]
async fn the_state_vocabulary_is_queued_running_finished_and_nothing_else() {
    let live = live_or_skip!();
    let page = live
        .builds("defaultFilter:false,count:100", "id,state,status")
        .await;
    assert!(!page.is_empty(), "the server has builds");
    let mut states = std::collections::BTreeMap::<String, usize>::new();
    let mut statuses = std::collections::BTreeMap::<String, usize>::new();
    for b in &page {
        let state = b
            .get("state")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("<absent>");
        assert!(
            matches!(state, "queued" | "running" | "finished"),
            "build {} is in state {state:?}, which this adapter classifies as neither finished \
             nor in flight — it would be emitted by nothing and clamp nothing",
            id_of(b)
        );
        *states.entry(state.to_owned()).or_default() += 1;
        *statuses
            .entry(
                b.get("status")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("<absent>")
                    .to_owned(),
            )
            .or_default() += 1;
    }
    // A queued build has no `status` at all, which `map::build_item` renders
    // as the bare state. Recorded rather than required: it is a fact about
    // this corpus today.
    println!("LIVE states:   {states:?}");
    println!("LIVE statuses: {statuses:?}");
}

/// Issue #91: `/app/rest/builds` does not answer in id order, and the ceiling
/// is therefore the page's **maximum** rather than its row 0.
///
/// What can be *asserted* here is narrow, deliberately. "The page is
/// unordered" is not an invariant — a quiet server may well answer one that
/// happens to descend — so requiring it would be a test that fails for the
/// wrong reason. What the ceiling actually depends on is asserted: every row
/// carries a positive integer id, so a maximum exists. The ordering itself is
/// *printed*, which is the honest form for a fact about a corpus that changes.
///
/// The one hard assertion is a tripwire. The obvious fix for #91 — ask the
/// server to sort — was not available: TeamCity 2026.2 answers
/// `order:(id:desc)` with `Locator dimension [order] is unknown`. If that ever
/// changes, this fails and somebody re-reads `sync::ceiling`.
#[tokio::test]
#[ignore = "needs a live TeamCity: `just teamcity-live`"]
async fn the_opening_page_carries_ids_but_promises_no_order() {
    let live = live_or_skip!();
    let page = live.builds("defaultFilter:false,count:100", "id").await;
    assert!(!page.is_empty(), "the server has builds");
    let ids: Vec<i64> = page.iter().map(id_of).collect();
    assert!(
        ids.iter().all(|id| *id > 0),
        "build ids are positive: {ids:?}"
    );
    let max = *ids.iter().max().expect("non-empty");
    let descending = ids.windows(2).all(|w| w[0] > w[1]);
    println!(
        "LIVE probe: rows={} row0={} max={} row0_is_max={} descending={}",
        ids.len(),
        ids[0],
        max,
        ids[0] == max,
        descending
    );
    if ids[0] != max {
        println!(
            "LIVE probe: row 0 is NOT the page maximum — issue #91's defect, still present. \
             The ceiling reads {max}, not {}.",
            ids[0]
        );
    }

    let (status, body) = live
        .get("app/rest/builds?locator=defaultFilter:false,count:3,order:(id:desc)&fields=count")
        .await;
    // `400`, not merely "not 200": a 429 or a 502 is a transient failure and
    // must not read as "the dimension is still unknown".
    assert_eq!(
        status, 400,
        "an unknown locator dimension is a 400. A 200 means this TeamCity now accepts \
         `order:(id:desc)` on /app/rest/builds -- the fix issue #91 could not use, so the \
         ceiling's maximum-of-the-page reading could be revisited. Any other status is a \
         transient failure and this run proved nothing: {body}"
    );
    println!("LIVE order:(id:desc) -> HTTP {status}");
}

/// `GET /app/rest/builds/id:{id}` is the evidence the replaced-server refusal
/// rests on, so both of its answers are certified here.
///
/// A 404 is read as "this build is not on this server", which is the one thing
/// that makes a source refuse to sync on. Reading a *different* status that
/// way would accuse a healthy server of having been replaced, so the positive
/// case matters as much as the negative one.
#[tokio::test]
#[ignore = "needs a live TeamCity: `just teamcity-live`"]
async fn a_build_answers_by_id_and_an_id_no_build_has_is_a_404() {
    let live = live_or_skip!();
    let page = live.builds("defaultFilter:false,count:1", "id").await;
    let real = id_of(page.first().expect("the server has a build"));

    let (status, body) = live
        .get(&format!("app/rest/builds/id:{real}?fields=id"))
        .await;
    assert_eq!(status, 200, "a build the server just listed: {body}");
    assert_eq!(
        body.get("id").and_then(serde_json::Value::as_i64),
        Some(real),
        "the by-id route answers about the build asked for: {body}"
    );

    // Above every id this server has issued, and monotonic ids mean it always
    // will be — the same reasoning the refusal itself rests on.
    let absent = real.saturating_mul(1000);
    let (status, body) = live
        .get(&format!("app/rest/builds/id:{absent}?fields=id"))
        .await;
    assert_eq!(
        status, 404,
        "an id no build has must be a 404 and nothing else; the adapter reads any other status \
         as a failure rather than as absence: {body}"
    );
    println!("LIVE by-id: {real} -> 200, {absent} -> 404");
}

/// Finding 2 from the first real-server contact: **real CI builds name
/// nobody.**
///
/// `triggered.user.username` is the only place TeamCity names a person, and a
/// VCS- or schedule-triggered build has none. `SyncItem::author` is therefore
/// empty for most builds on a real server, which is what #39's `author:` and
/// `@` search tokens rest on for this source — and no fixture shows the case
/// at scale.
///
/// Asserted from both ends: the record really omits the field, and the adapter
/// maps that to `None` rather than inventing a triggerer (interfaces §4.1
/// forbids inventing one).
#[tokio::test]
#[ignore = "needs a live TeamCity: `just teamcity-live`"]
async fn most_builds_name_nobody_and_the_adapter_leaves_the_author_empty() {
    let live = live_or_skip!();
    // The adapter's own locator, `defaultFilter` left off exactly as
    // `sync::execute` leaves it off. Sampling from a `defaultFilter:false`
    // page instead would pick builds the default filter hides -- a canceled
    // one, say (see the cancellation test below) -- and the scoped run
    // underneath could then legitimately come back with nothing.
    let page = live
        .builds(
            "state:finished,count:100",
            "id,buildTypeId,triggered(user(username))",
        )
        .await;
    assert!(!page.is_empty(), "the server has finished builds");
    let named = |b: &serde_json::Value| {
        b.get("triggered")
            .and_then(|t| t.get("user"))
            .and_then(|u| u.get("username"))
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
    };
    let anonymous: Vec<&serde_json::Value> = page.iter().filter(|b| named(b).is_none()).collect();
    println!(
        "LIVE authorship: {}/{} of the newest finished builds name no user",
        anonymous.len(),
        page.len()
    );
    let Some(sample) = anonymous.first().copied() else {
        // Recorded, not asserted. That every build in one window names a user
        // would be a surprising server rather than a broken adapter, and the
        // window is not ours to arrange -- the same rule the cancellation
        // test below follows.
        println!(
            "LIVE authorship: every build in this window names a user, so there is no \
             anonymous build to carry through the adapter. That is not what this finding was \
             measured on; re-run, and re-read the finding if it holds."
        );
        return;
    };
    let build_type = build_type_of(sample);

    let (items, _) = full(&*live.scoped_to(&build_type)).await;
    let builds = of_kind(&items, "build");
    assert!(!builds.is_empty());
    assert!(
        builds.iter().any(|i| i.author.is_none()),
        "a build TeamCity attributes to nobody must reach the mirror with no author, not with \
         an invented one: {:?}",
        builds
            .iter()
            .map(|i| (&i.title, &i.author))
            .collect::<Vec<_>>()
    );
}

/// Finding 3, and a correction to it: a canceled build is `status: "UNKNOWN"`
/// with `statusText: "Canceled"` — and **the adapter never mirrors one.**
///
/// The issue that asked for this suite recorded that `defaultFilter:false`
/// "deliberately *includes* canceled builds, so this path runs in production
/// and is untested". Measured here, that is not what happens.
/// `defaultFilter:false` is on the opening probe alone, whose page is read for
/// one number and never mapped. The two queries that *do* produce items — the
/// per-configuration full-sync query and the incremental
/// `state:finished,sinceBuild:` — send no `defaultFilter`, and TeamCity's
/// default filter hides canceled builds from both.
///
/// So the `UNKNOWN` render path is not merely untested, it is unreachable, and
/// canceled builds are silently absent from the mirror for good: `sinceBuild`
/// is exclusive and the watermark advances past them. Whether that is right is
/// a product question and not this suite's to answer; certifying it is.
///
/// The comparison is made with the adapter's **own** per-configuration
/// locator, on the canceled build's own configuration, so the two pages cover
/// the same builds and the difference is the dimension under test.
#[tokio::test]
#[ignore = "needs a live TeamCity: `just teamcity-live`"]
async fn a_canceled_build_is_status_unknown_and_the_adapter_s_locator_hides_it() {
    let live = live_or_skip!();
    let page = live
        .builds(
            "state:finished,defaultFilter:false,count:100",
            "id,buildTypeId,status,statusText",
        )
        .await;
    let status_of = |b: &serde_json::Value| {
        b.get("status")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("<absent>")
            .to_owned()
    };
    let canceled: Vec<&serde_json::Value> =
        page.iter().filter(|b| status_of(b) == "UNKNOWN").collect();
    println!(
        "LIVE cancellations: {}/{} of the newest finished builds are status UNKNOWN",
        canceled.len(),
        page.len()
    );
    let Some(sample) = canceled.into_iter().max_by_key(|b| id_of(b)) else {
        // Recorded, not asserted: a quiet hour with no cancellation in the
        // window is a fact about the corpus, and the corpus is not ours.
        println!(
            "LIVE cancellations: none in this window, so the comparison below has nothing to \
             run on. Re-run later; this is the one assertion here that depends on what the \
             server happens to be doing."
        );
        return;
    };
    let id = id_of(sample);
    let build_type = build_type_of(sample);
    assert!(
        sample
            .get("statusText")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|t| t.to_ascii_lowercase().contains("cancel")),
        "an UNKNOWN status on a finished build is a cancellation, and `statusText` is where \
         TeamCity says so: {sample}"
    );

    let widened: Vec<i64> = live
        .builds(
            &format!("buildType:(id:{build_type}),state:finished,defaultFilter:false,count:100"),
            "id",
        )
        .await
        .iter()
        .map(id_of)
        .collect();
    if !widened.contains(&id) {
        // Recorded, not asserted, for the same reason as the empty window
        // above: the corpus moved between two requests, which is a fact about
        // somebody else's server and not something anyone here can act on.
        println!(
            "LIVE cancellations: build {id} left its own configuration's newest 100 finished \
             builds between two requests, so the comparison below would compare two different \
             pages. Re-run."
        );
        return;
    }

    // The locator `sync::execute`'s full sync actually sends.
    let adapters_own: Vec<i64> = live
        .builds(
            &format!("buildType:(id:{build_type}),state:finished,count:100"),
            "id",
        )
        .await
        .iter()
        .map(id_of)
        .collect();
    assert!(
        !adapters_own.contains(&id),
        "TeamCity's default filter no longer hides canceled builds: build {id} came back from \
         the adapter's own per-configuration locator. That is a *fix* to the gap this test \
         records, not a failure — the UNKNOWN render path is now reachable, and \
         `map::build_item`'s \"finished UNKNOWN\" string finally needs a decision."
    );
    println!(
        "LIVE cancellations: build {id} in {build_type} is served with defaultFilter:false and \
         hidden from the adapter's own locator — canceled builds never reach the mirror"
    );
}

/// The `/guestAuth` prefix is the whole credential story here, so the other
/// half of it is certified too: **without** that prefix the same server
/// refuses, and the adapter calls that refusal `Unauthorized`.
///
/// `tests/mockd.rs`'s `a_401_is_unauthorized_from_both_entry_points` encodes
/// this against the fake; this is the same shape asked of the server that
/// decides. It matters beyond the prefix: `Unauthorized` is what puts *Re-enter
/// password* on screen, and a 401 misread as a protocol failure would report a
/// dead credential as a broken server.
///
/// Only runs when there is a prefix to strip. Pointed at a TeamCity that is
/// not guest-readable there is no un-prefixed variant to compare against, and
/// inventing one would be a test about this file's idea of the URL rather than
/// about the server.
#[tokio::test]
#[ignore = "needs a live TeamCity: `just teamcity-live`"]
async fn the_bare_path_refuses_where_guestauth_admits_and_the_adapter_calls_it_unauthorized() {
    let live = live_or_skip!();
    let Some(bare) = live.url.strip_suffix("/guestAuth") else {
        println!(
            "LIVE auth: {} carries no /guestAuth prefix, so there is no un-prefixed variant of \
             it to refuse. Nothing to certify here.",
            live.url
        );
        return;
    };

    let (status, body) = live.get("app/rest/server?fields=version").await;
    assert_eq!(
        status, 200,
        "the guestAuth prefix admits without a token: {body}"
    );

    let unprefixed = live
        .http
        .get(format!("{bare}/app/rest/server?fields=version"))
        .header("Accept", "application/json")
        .header("Authorization", format!("Bearer {}", live.secret))
        .send()
        .await
        .expect("the un-prefixed root answers");
    assert_eq!(
        unprefixed.status().as_u16(),
        401,
        "the same server, the same header, without /guestAuth: the prefix is what authenticates \
         the request, and if the bare path now admits too then `.env.example`'s note that the \
         prefix is load-bearing is stale"
    );

    let err = live
        .source_at(bare, serde_json::json!({}))
        .test_connection()
        .await
        .expect_err("a refused credential is not a connection");
    assert!(
        matches!(err, SourceError::Unauthorized { .. }),
        "a 401 is Unauthorized and nothing else -- a Protocol failure here would report a dead \
         credential as a broken server: {err:?}"
    );
    assert_eq!(
        err.status(),
        Some(401),
        "the status rides along, so the health strip can tell 401 from 403 (ADR-0004): {err:?}"
    );
    println!("LIVE auth: /guestAuth -> 200, bare -> 401 -> {err:?}");
}

/// **The error shape, certified end to end** -- this test used to record the
/// disagreement and now closes it (issue #113).
///
/// `http::error_message` required a body whose first line starts with `Error
/// has occurred during request processing`, on a doc comment calling that "the
/// shape a real TeamCity serves". A real TeamCity 2026.2 serves that shape to
/// nobody: with `Accept: application/json` -- which `knobas-http` sets on every
/// request and cannot be talked out of -- errors come back as
/// `{"errors":[{"message": …, "statusText": …}]}`, with the sentence in a field
/// rather than on a line. So the function returned `None` on every error this
/// adapter would ever see and the user was shown a JSON blob.
///
/// Two things are asserted, and the second is the one that could not be
/// asserted anywhere else: the server sends the envelope, **and
/// `error_message` reads it**, checked by pushing the real body through
/// `knobas_http::status_error` -- the same function `HttpClient::send` calls,
/// with the same hook the client is built with. A fake could show the second
/// half alone; only this can show it over a body the server actually wrote.
///
/// Asserted by form otherwise: the envelope's presence and a non-empty
/// message, on both statuses the adapter reads (400 and 404). The messages
/// themselves are printed.
#[tokio::test]
#[ignore = "needs a live TeamCity: `just teamcity-live`"]
async fn a_rest_error_is_a_json_envelope_the_adapter_can_read() {
    let live = live_or_skip!();
    let newest = id_of(
        live.builds("defaultFilter:false,count:1", "id")
            .await
            .first()
            .expect("the server has a build"),
    );
    let probes = [
        (
            400,
            "app/rest/builds?locator=defaultFilter:false,order:(id:desc)&fields=count".to_owned(),
            "an unknown locator dimension",
        ),
        (
            404,
            format!(
                "app/rest/builds/id:{}?fields=id",
                newest.saturating_mul(1000)
            ),
            "an id no build has",
        ),
    ];
    for (want, path, what) in probes {
        let (status, body) = live.get(&path).await;
        assert_eq!(status, want, "{what} is a {want}: {body}");
        let message = body
            .get("errors")
            .and_then(serde_json::Value::as_array)
            .and_then(|errors| errors.first())
            .and_then(|error| error.get("message"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or_else(|| {
                panic!(
                    "{what} did not answer with TeamCity's JSON error envelope. If this is now \
                     the plaintext body `http::error_message` parses, that function finally \
                     works against a real server and this test records a *fix*: {body}"
                )
            });
        assert!(
            !message.trim().is_empty(),
            "the envelope carries the sentence, not just a status: {body}"
        );
        // The plaintext shape `error_message` used to *require*, absent --
        // which is why requiring it made the function useless here.
        let plaintext = body.as_str().unwrap_or_default();
        assert!(
            !plaintext.starts_with("Error has occurred during request processing"),
            "the body is both a JSON envelope and the plaintext shape, which cannot be: {body}"
        );

        // The three keys beside it are the ones mockd transcribes, so a server
        // that stopped sending them would be a fake drifting from a server
        // again. `message` is the only one read, and it is read *because* the
        // other two are noise: `additionalMessage` is the same sentence behind
        // a fully-qualified Java class name.
        for key in ["additionalMessage", "statusText"] {
            assert!(
                body["errors"][0].get(key).is_some(),
                "the envelope carries {key}, which `knobas-mockd`'s `tc_error` transcribes: \
                 {body}"
            );
        }
        assert!(
            !message.contains("jetbrains.buildServer") && !message.contains("javax.ws.rs"),
            "the class name lives in additionalMessage, not in the sentence -- which is why \
             `message` is the field read: {body}"
        );
        println!("LIVE error {status} ({what}): errors[0].message = {message:?}");
    }
    println!(
        "LIVE error: `http::error_message` reads all of these. It used to want a first line \
         reading `Error has occurred during request processing`, which this server sends to \
         nobody -- a JSON-accepting client gets the envelope, any other Accept gets a 406 whose \
         body is also the envelope, and no Accept at all gets XML."
    );

    // **The end-to-end half.** Everything above reads the body with the test's
    // own eyes; this reads it with the adapter's. A base URL nested one level
    // inside `/app/rest` leaves every path this adapter builds under the REST
    // resource -- so `test_connection`'s `app/rest/server` becomes
    // `/app/rest/server/app/rest/server`, which the server answers 404 with its
    // real error envelope. Still a GET, still read-only, and it is the only
    // route from a *configuration* to a live REST error: every locator this
    // adapter sends is one it built itself out of a listing the server gave it.
    //
    // Before #113 the message here was the whole envelope printed at the user.
    let nested = live.source_at(
        &format!("{}/app/rest/server", live.url),
        serde_json::json!({}),
    );
    let error = nested
        .test_connection()
        .await
        .expect_err("a path one level inside the REST resource is a 404");
    let SourceError::Protocol { message, status } = &error else {
        panic!("a 404 is a protocol fault, not a credential one: {error:?}");
    };
    assert_eq!(*status, Some(404), "ADR-0004: the status rides along");
    for envelope_key in [
        "errors",
        "additionalMessage",
        "statusText",
        "stackTrace",
        "{",
    ] {
        assert!(
            !message.contains(envelope_key),
            "the user is shown the server's sentence, not the envelope around it -- {envelope_key:?} \
             is in {message:?}"
        );
    }
    assert!(
        message.starts_with("HTTP 404 Not Found: ") && message.len() > "HTTP 404 Not Found: ".len(),
        "and there is a sentence after the status `knobas-http` prefixes: {message:?}"
    );
    println!("LIVE error rendered through the adapter: {message:?}");
}

/// Issue #114: `nextHref` is the server's own statement that a page is not the
/// whole answer, and the one `sync::last_page` ends a walk on.
///
/// The adapter used to end a walk on `page.len() < count`, which is true only
/// of a server that serves exactly the `count:` it was asked for. Nothing in
/// `/app/rest/builds` promises that, and the field that does answer the
/// question had never been asked for. This is the certification that it
/// answers it -- and, more importantly, **that it has to be asked for at all**,
/// which is the half no fake can teach because every fake here serves whatever
/// field it is asked and mockd's projector drops the rest.
///
/// By form, as everything in this file is: the corpus behind the query changes
/// between requests, so nothing below names an id or a count. What is pinned is
/// which of the three answers carries the field.
#[tokio::test]
#[ignore = "needs a live TeamCity: `just teamcity-live`"]
async fn a_page_says_for_itself_whether_the_collection_ran_out() {
    let live = live_or_skip!();

    // A selector that does not name `nextHref` never receives one, however
    // much the server has left. That is why `BUILD_FIELDS` and
    // `BUILD_TYPE_FIELDS` name it, and why dropping it from either would put
    // every walk back on the page-length assumption in silence.
    let (status, unasked) = live
        .get("app/rest/builds?locator=defaultFilter:false,count:2&fields=count,build(id)")
        .await;
    assert_eq!(status, 200, "{unasked}");
    assert_eq!(
        unasked.get("nextHref"),
        None,
        "a `fields=` that does not ask for nextHref does not get one, whatever is left to \
         serve -- so the field is not something an adapter can rely on arriving by default: \
         {unasked}"
    );

    let asked = async |locator: &str| -> serde_json::Value {
        let (status, body) = live
            .get(&format!(
                "app/rest/builds?locator={locator}&fields=count,nextHref,build(id)"
            ))
            .await;
        assert_eq!(status, 200, "locator {locator:?} answered {body}");
        body
    };

    // A page the server filled: it reports a further one. Two rows out of the
    // whole build history is a page nothing could exhaust.
    let filled = asked("defaultFilter:false,count:2").await;
    assert_eq!(
        filled["build"].as_array().map(Vec::len),
        Some(2),
        "{filled}"
    );
    assert!(
        filled["nextHref"]
            .as_str()
            .is_some_and(|h| h.contains("start:")),
        "a filled page has to say there is more, and where it resumes: {filled}"
    );

    // The other end: a query the server could not fill reports nothing after
    // it.
    //
    // `id:` is the locator that makes that deterministic on a server whose
    // corpus moves. It matches at most one build, so `count:100` over it is a
    // page the server cannot fill however busy it is -- and non-empty, which
    // an empty page would not be.
    //
    // **Not `sinceBuild:` past the id this run witnessed**, which is what
    // stood here and is the #91 trap wearing a new hat: the opening page is
    // unordered, so its maximum is an id known to *exist*, never the newest on
    // the server. Measured on this instance 2026-08-29, that query came back
    // with a full 100 rows and a `nextHref` -- more than a hundred builds had
    // finished above the witnessed id -- so the assertion was about JetBrains'
    // build throughput rather than about `nextHref`.
    let one = id_of(
        live.builds("defaultFilter:false,count:1", "id")
            .await
            .first()
            .expect("the server has builds"),
    );
    let exhausted = asked(&format!("id:{one},count:100")).await;
    let rows = exhausted["build"].as_array().map_or(0, Vec::len);
    assert_eq!(
        rows, 1,
        "`id:` names one build, so a page of 100 over it is one the server cannot fill: \
         {exhausted}"
    );
    assert!(
        exhausted["nextHref"].is_null() || exhausted.get("nextHref").is_none(),
        "a page the server could not fill is the end of its collection, and must not claim a \
         successor -- ending a walk there is exactly what `sync::last_page` does: {exhausted}"
    );
    println!(
        "LIVE nextHref: filled page -> {:?}; unfilled page ({rows} rows) -> {:?}; \
         unasked -> absent",
        filled["nextHref"].as_str().map(|_| "present"),
        exhausted.get("nextHref"),
    );

    // The build-configuration listing, which this adapter asks for **whole**:
    // no `count:`, and `sync::scope` refuses an answer that reports a further
    // page. This is the certification that a real TeamCity does not page it.
    let (status, types) = live
        .get("app/rest/buildTypes?fields=count,nextHref,buildType(id)")
        .await;
    assert_eq!(status, 200, "{types}");
    let listed = types["buildType"].as_array().map_or(0, Vec::len);
    assert!(listed > 100, "this server has thousands of them: {listed}");
    assert_eq!(
        types["count"].as_u64(),
        Some(listed as u64),
        "the envelope counts the page it is: {types}"
    );
    assert!(
        types["nextHref"].is_null() || types.get("nextHref").is_none(),
        "`/app/rest/buildTypes` with no locator answers the whole listing -- {listed} of them \
         here -- which is the assumption `sync::scope` is built on and refuses to sync \
         without: {types}"
    );
    println!("LIVE buildTypes: {listed} configurations in one answer, no nextHref");
}
