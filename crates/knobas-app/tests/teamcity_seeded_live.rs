//! **The standup digest over the seeded, self-hosted TeamCity** (issue #389).
//!
//! M3.3's exit criterion is *"the digest is drawn from a day of real activity
//! across the seeded Gitea, TeamCity, Jira and Confluence"*. Three of those
//! four now have a live witness: Jira and Confluence in
//! `tests/atlassian_live.rs`, Gitea at the end of `tests/start_work_live.rs`.
//! This file is TeamCity's, and it exists as a file of its own for a reason
//! the brief names: `knobas-source-teamcity/tests/live_teamcity_seeded.rs`
//! certifies the adapter against the same server, thoroughly, but it is in the
//! **adapter's** crate and cannot reach `knobas_app::commands::entity`. The
//! digest is an app-crate read, so the assertion has to be made from here.
//!
//! **`_seeded_` is in the name because `just teamcity-live` does not run this
//! file.** Only `just teamcity-live-seeded` does, against the TeamCity
//! `./seed --teamcity` filled; the public JetBrains instance the sibling
//! recipe reads has no build of ours on it and could not satisfy a word of
//! this. Same distinction, same spelling, as the adapter crate's
//! `live_teamcity.rs` / `live_teamcity_seeded.rs` pair.
//!
//! **What "across" does and does not mean here.** Four sources now have a
//! live digest witness, but they are four suites over four
//! `scratch_database`s, each reading the day *its own* server acted on --
//! there is no single digest read that spans all four, and there cannot be one
//! while the Atlassian pair lives behind a three-hour timebomb licence and its
//! own recipe (`testenv/README.md`, "Jira and Confluence, end to end"). The
//! criterion is met route by route. Whether that is what the exit sentence
//! asks for is Björn's call at the gate, not this file's.
//!
//! # What it asserts, and what it does not
//!
//! One thing: a build the seed really ran, on the day the server really ran it
//! on, is a line on the digest, attributed to the account knobas is configured
//! as. Everything about the *adapter* -- the locators, the branch dimension,
//! the cursor, the payload fields -- belongs to `live_teamcity_seeded.rs` and
//! is not restated here.
//!
//! The build is **not** one this suite triggers. Issue #320's finding stands:
//! the seeded server will not run a build queued from a fresh cold start
//! without an agent picking it up, and `live_teamcity_seeded.rs` is the suite
//! that owns queueing and its cleanup. So the subject is a build that is
//! already there and that the mirror attributes to the reader -- which is what
//! the seed leaves, because *"a build queued through this token is triggered by
//! `knobas`, not by the fixture's `mara` or by a VCS trigger"*
//! (`testenv/seed-teamcity-builds.sh`).
//!
//! **Read-only.** Nothing is created at TeamCity, nothing is deleted, and
//! there is no litter guard because there is no litter. It may therefore run
//! after `live_teamcity_seeded.rs` in the same recipe without disturbing that
//! suite's exact-set assertions or its leftover clearing.
//!
//! # Running it
//!
//! `#[ignore]`d, so `just check` runs none of it. The recipe is
//! `just teamcity-live-seeded`, which runs the adapter suite and then this one:
//!
//! ```text
//! cd testenv
//! docker compose --profile real-teamcity up -d teamcity teamcity-agent
//! ./seed && ./seed --teamcity        # Gitea first: the VCS roots point at it
//! cd .. && just teamcity-live-seeded
//! ```
//!
//! **One environment, one owner at a time** -- `testenv/README.md`.

mod live_digest;

use std::path::PathBuf;
use std::sync::Arc;

use knobas_app::sources::{Registry, SourcesState};
use knobas_secrets::{MemoryStore, Secret, SecretStore};
use knobas_source::AuthMethod;
use knobas_sync::scheduler::{Scheduler, SchedulerDeps};
use live_digest::{Connections, Quiet, day_window, on_digest, sync};
use serde_json::json;

/// The configured source id, which is also the entity namespace.
const TEAMCITY: &str = "teamcity";

/// The fixture build this suite reads, by the number the fixture gives it.
///
/// `1187` -- `Payout_IntegrationTests`, the failed build on
/// `feature/PAY-231-sepa-retry`. Named rather than "whatever the newest build
/// is": a suite that took the first row it found would stay green against a
/// mirror holding one row of somebody else's, which is exactly the reading
/// `live_teamcity_seeded.rs` refuses for the adapter and this file refuses for
/// the digest. Its **real** id is the server's to assign and is looked up in
/// `testenv/seed-state.json`.
const FIXTURE_BUILD: i64 = 1187;

/// How long any one direct request to TeamCity may take.
///
/// `reqwest` carries no default timeout; the same ten seconds
/// `start_work_live.rs` bounds its own direct calls with, for the same reason.
const REQUEST_BUDGET: std::time::Duration = std::time::Duration::from_secs(10);

struct Env {
    url: String,
    token: String,
}

/// Panics with the commands to run rather than skipping: an unset variable
/// that skipped would be counted a pass by libtest, which is issue #351 and
/// the reason `just teamcity-live-seeded` gates on the variables at all.
fn env() -> Env {
    let need = |key: &str| {
        std::env::var(key)
            .ok()
            .map(|v| v.trim().to_owned())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| {
                panic!(
                    "{key} is not set -- start testenv's TeamCity and seed it first \
                     (`docker compose --profile real-teamcity up -d teamcity teamcity-agent`, \
                     `./seed`, `./seed --teamcity`), then run `just teamcity-live-seeded`"
                )
            })
    };
    Env {
        url: need("KNOBAS_TEAMCITY_URL").trim_end_matches('/').to_owned(),
        token: need("KNOBAS_TEAMCITY_TOKEN"),
    }
}

impl Env {
    async fn get(&self, path_and_query: &str) -> serde_json::Value {
        let response = reqwest::Client::builder()
            .timeout(REQUEST_BUDGET)
            .build()
            .expect("a bounded client")
            .get(format!("{}/{path_and_query}", self.url))
            .header("Accept", "application/json")
            .bearer_auth(&self.token)
            .send()
            .await
            .unwrap_or_else(|e| panic!("GET {path_and_query}: {e}"));
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        assert!(
            status.is_success(),
            "GET {path_and_query} answered {status}: {body}"
        );
        serde_json::from_str(&body)
            .unwrap_or_else(|e| panic!("GET {path_and_query}: {e} in {body}"))
    }

    /// The TeamCity account this token belongs to, as the server names it.
    ///
    /// `GET /app/rest/users/current`, which is the call the adapter's own
    /// `test_connection` makes to fill `TeamCityConfig::username` in at add
    /// time -- *"the TeamCity account this credential belongs to ... what
    /// `@me`-style filters match `sync.item.author` against"*. Asked of the
    /// **server** rather than read out of the mirror row this test is about,
    /// which would take the identity under test from the field under test.
    async fn account(&self) -> String {
        let me = self
            .get("app/rest/users/current?fields=username,name")
            .await;
        me["username"]
            .as_str()
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| panic!("/app/rest/users/current named no username: {me}"))
            .to_owned()
    }
}

/// The real build id the seed recorded for [`FIXTURE_BUILD`].
///
/// A real server assigns ids and the seed cannot dictate them, so
/// `seed-state.json` is the bridge -- the same file and the same reading
/// `live_teamcity_seeded.rs` uses, and `KNOBAS_TEAMCITY_SEED_STATE` overrides
/// its location there and here alike.
fn seeded_build_id() -> i64 {
    let path = std::env::var("KNOBAS_TEAMCITY_SEED_STATE")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testenv/seed-state.json")
        });
    let raw = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "{}: {e} -- `./seed --teamcity` writes the number-to-id map this suite reads",
            path.display()
        )
    });
    let whole: serde_json::Value = serde_json::from_str(&raw).expect("seed-state.json is JSON");
    let builds = whole["teamcity"]["builds"].as_array().unwrap_or_else(|| {
        panic!(
            "{}: no `teamcity.builds` -- run `./seed --teamcity`",
            path.display()
        )
    });
    builds
        .iter()
        .find(|b| b["fixture_number"] == json!(FIXTURE_BUILD))
        .and_then(|b| b["real_id"].as_i64())
        .unwrap_or_else(|| {
            panic!(
                "{}: the seed recorded no real id for fixture build {FIXTURE_BUILD}",
                path.display()
            )
        })
}

// -- the app, wired the way the app wires it --------------------------------

/// A `SourcesState` over a database of this test's own, with the TeamCity
/// source configured and its token in the (in-memory) keychain.
///
/// `account` goes into `username`, which is what
/// `knobas_search::Vocabulary::load` collects into the identity behind `@me` --
/// and that identity is what the digest's mirror half matches
/// `sync.live_item.author` against. Without it the app knows of nobody and the
/// digest is empty for everyone.
async fn app(env: &Env, account: &str) -> SourcesState {
    let connector = knobas_db::test_util::scratch_database("teamcity_live_digest").await;
    let pool = connector
        .pool(4)
        .await
        .expect("a pool onto the scratch database");

    let secrets = Arc::new(MemoryStore::new());
    secrets
        .put(
            &knobas_secrets::KeychainAccount::source(TEAMCITY),
            &Secret::just(AuthMethod::Pat, env.token.clone()),
        )
        .expect("the TeamCity token is stored");

    knobas_sync::config::insert(
        &pool,
        &knobas_sync::config::InsertConfig {
            id: TEAMCITY.to_owned(),
            adapter_kind: "teamcity".to_owned(),
            display_name: "Tidewater CI (seeded)".to_owned(),
            base_url: env.url.clone(),
            auth_kind: knobas_sync::config::AuthKind::Method(AuthMethod::Pat),
            config: json!({ "username": account }),
            sync_interval_secs: 86_400,
            enabled: true,
        },
    )
    .await
    .expect("the source row is written");

    let scheduler = Scheduler::start(SchedulerDeps {
        pool: pool.clone(),
        connections: Arc::new(Connections(connector)),
        registry: Arc::new(Registry::builtin()),
        secrets: secrets.clone(),
        events: Arc::new(Quiet),
        timing: knobas_sync::scheduler::SchedulerTiming::default(),
    })
    .await
    .expect("a scheduler over the scratch database");

    SourcesState {
        pool,
        scheduler,
        secrets,
        registry: Arc::new(Registry::builtin()),
    }
}

/// **A seeded build the mirror attributes to the reader is on the digest for
/// the day it ran** (issue #389, M3.3's exit criterion).
///
/// The mirror row is asserted first, and separately, because the digest
/// reaches a line by two filters at once -- whose the item is, and whether the
/// day is in the window. An empty list on its own could not tell "TeamCity
/// named nobody" from "the day is wrong", and the first of those is the
/// finding this ticket exists to make either way: `map.rs` fills `author` from
/// `triggered.user.username` and *"`None` is a build no person started -- a
/// VCS, schedule or dependency trigger -- which is effectively all of them on
/// a real server"*. It is not all of them **here**, because the seed queued
/// these through its own token.
///
/// The digest is read for the day the *mirror* dates the build on rather than
/// for today: the seed ran when it ran, and a fixture that assumed "today"
/// would be witnessing the calendar.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs testenv's seeded TeamCity: `just teamcity-live-seeded`"]
async fn a_seeded_build_the_mirror_attributes_to_the_reader_is_on_that_days_digest() {
    let env = env();
    let account = env.account().await;
    let state = app(&env, &account).await;
    sync(&state, TEAMCITY).await;

    let build = format!("{TEAMCITY}:build:{}", seeded_build_id());
    let (attributed_to, at): (Option<String>, chrono::DateTime<chrono::Utc>) = sqlx::query_as(
        "select author, coalesce(item_updated_at, synced_at)
           from sync.live_item where entity_id = $1",
    )
    .bind(&build)
    .fetch_one(&state.pool)
    .await
    .unwrap_or_else(|e| {
        panic!("fixture build {FIXTURE_BUILD} ({build}) is not in the mirror after a sync: {e}")
    });
    assert_eq!(
        attributed_to.as_deref(),
        Some(account.as_str()),
        "the mirror attributes {build} to {attributed_to:?} and the source is configured as \
         {account:?}, so the digest's mirror half -- `where i.author = any($1)`, matched \
         case-sensitively against the configured usernames -- cannot reach it. TeamCity names \
         a person only in `triggered.user.username`, which is what the adapter maps"
    );

    let day = at.date_naive();
    let digest = knobas_app::commands::entity::standup_digest_inner(
        &state.pool,
        state.registry.as_ref(),
        // A clock **outside** the day being asked about, so no running timer of
        // this scratch database's own can join the list -- the precaution
        // `tests/atlassian_live.rs` states in as many words. The window is
        // half-open, so its own `to` is already outside it. There is no timer
        // here and nothing in this file starts one; the argument costs one
        // expression and the alternative is a list whose length depends on
        // something the fixture never wrote.
        day_window(day).to,
        day_window(day),
        &[],
    )
    .await
    .expect("the digest reads");
    on_digest(
        &digest.today,
        &format!("fixture build {FIXTURE_BUILD}"),
        day,
        &build,
        TEAMCITY,
        "build",
        "attributed",
    );
    for line in &digest.today {
        assert!(
            !line.reason.trim().is_empty(),
            "a line whose provenance cannot be shown is not shippable: {line:?}"
        );
    }
    println!(
        "SEEDED digest for {day}: {} lines under today, including {build}",
        digest.today.len()
    );

    state.scheduler.shutdown().await;
}
