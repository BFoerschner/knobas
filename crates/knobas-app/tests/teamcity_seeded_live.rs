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
//! # The `under:` witness (issue #644)
//!
//! The second test is the SQL `under:` path on this server's records: each
//! fixture configuration, by its id and its name, through the app's own
//! search. Read-only too, and for the same reason it lives here and not in the
//! adapter's crate: the search is an app-crate read.
//!
//! # A build leads to its code (issue #683)
//!
//! `a_builds_change_and_its_revision_open_the_mirrored_gitea_commit` is the one
//! test here that needs **both** products: it configures the seeded Gitea
//! beside TeamCity in the same app, syncs both, and opens the build view the
//! detail panel opens. The change witness's second build lists a change, and
//! fixture build 1187 lists none; each row named by repository and sha must
//! carry the mirrored commit's id. `just teamcity-live-seeded` already starts
//! and seeds Gitea (the VCS roots clone it) and exports `KNOBAS_GITEA_URL` and
//! `KNOBAS_GITEA_TOKEN`, so it needs no recipe of its own. Read-only like the
//! rest.
//!
//! # Running it
//!
//! `#[ignore]`d, so `just check` runs none of it. The recipe is
//! `just teamcity-live-seeded`, which runs the adapter suite and then this one:
//!
//! ```text
//! cd testenv
//! docker compose --env-file images.env --profile real-teamcity up -d teamcity teamcity-agent
//! ./seed && ./seed --teamcity        # Gitea first: the VCS roots point at it
//! cd .. && just teamcity-live-seeded
//! ```
//!
//! **One environment, one owner at a time** -- `testenv/README.md`.

mod live_digest;
mod live_under;

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use knobas_app::sources::{Registry, SourcesState};
use knobas_live_cleanup::Cleanup;
use knobas_secrets::{MemoryStore, Secret, SecretStore};
use knobas_source::AuthMethod;
use knobas_sync::scheduler::{RefreshEnd, RefreshEnded, Scheduler, SchedulerDeps, SyncEvents};
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
                     (`docker compose --env-file images.env --profile real-teamcity up -d teamcity teamcity-agent`, \
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
    seeded_builds()
        .into_iter()
        .find(|b| b.number == FIXTURE_BUILD)
        .map(|b| b.real_id)
        .unwrap_or_else(|| panic!("the seed recorded no real id for fixture build {FIXTURE_BUILD}"))
}

/// One build the seed ran, as `seed-state.json` records it.
struct SeededBuild {
    /// The fixture's `num`.
    number: i64,
    /// The configuration it ran in, the fixture's `cfg`.
    config: String,
    /// The id the server assigned.
    real_id: i64,
}

/// Every build the seed recorded.
fn seeded_builds() -> Vec<SeededBuild> {
    let whole = live_under::seed_state("KNOBAS_TEAMCITY_SEED_STATE");
    whole["teamcity"]["builds"]
        .as_array()
        .unwrap_or_else(|| {
            panic!("seed-state.json: no `teamcity.builds` -- run `./seed --teamcity`")
        })
        .iter()
        .map(|b| SeededBuild {
            number: b["fixture_number"].as_i64().expect("a fixture number"),
            config: b["build_type"].as_str().expect("a build type").to_owned(),
            real_id: b["real_id"].as_i64().expect("a real id"),
        })
        .collect()
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
    app_with(env, account, None, None).await
}

/// The Gitea source a cross-product test adds beside TeamCity (#683): the
/// seeded Gitea, as `./seed --env` names it, over the two repositories the
/// seeded VCS roots build.
struct GiteaEnv {
    url: String,
    token: String,
}

/// The configured Gitea source's id and entity namespace.
const GITEA: &str = "gitea";

fn gitea_env() -> GiteaEnv {
    let need = |key: &str| {
        std::env::var(key)
            .ok()
            .map(|v| v.trim().to_owned())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| {
                panic!("{key} is not set -- `just teamcity-live-seeded` seeds Gitea and exports it")
            })
    };
    GiteaEnv {
        url: need("KNOBAS_GITEA_URL").trim_end_matches('/').to_owned(),
        token: need("KNOBAS_GITEA_TOKEN"),
    }
}

/// The app over the seeded TeamCity, with Gitea beside it when given, and,
/// given `events`, the app's own scheduler
/// (`knobas_app::sources::start_scheduler`) telling them: there a landed
/// write is followed as in the app, by its refresh and then a run (#2099,
/// ADR-0031).
async fn app_with(
    env: &Env,
    account: &str,
    gitea: Option<&GiteaEnv>,
    events: Option<&dyn Fn(sqlx::PgPool) -> Arc<dyn SyncEvents>>,
) -> SourcesState {
    let connector = knobas_db::test_util::scratch_database(if events.is_some() {
        "teamcity_live_refresh"
    } else {
        "teamcity_live_digest"
    })
    .await;
    let pool = connector
        .pool(4)
        .await
        .expect("a pool onto the scratch database");
    // Bring-up's order (#627): the declarations before the first sync.
    live_under::declare(&pool).await;

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

    if let Some(gitea) = gitea {
        secrets
            .put(
                &knobas_secrets::KeychainAccount::source(GITEA),
                &Secret::just(AuthMethod::Pat, gitea.token.clone()),
            )
            .expect("the Gitea token is stored");
        knobas_sync::config::insert(
            &pool,
            &knobas_sync::config::InsertConfig {
                id: GITEA.to_owned(),
                adapter_kind: "gitea".to_owned(),
                display_name: "Tidewater Git (seeded)".to_owned(),
                base_url: gitea.url.clone(),
                auth_kind: knobas_sync::config::AuthKind::Method(AuthMethod::Pat),
                // The address the seeded VCS roots clone from, inside the
                // compose network: what lets a change named by it open the
                // commit this source mirrors under its own URL (#683).
                config: json!({
                    "repos": ["tidewater/payout-service", "tidewater/ledger-api"],
                    "clone_urls": ["http://gitea:3000"]
                }),
                sync_interval_secs: 86_400,
                enabled: true,
            },
        )
        .await
        .expect("the Gitea source row is written");
    }

    let deps = |events: Arc<dyn SyncEvents>| SchedulerDeps {
        pool: pool.clone(),
        connections: Arc::new(Connections(connector.clone())),
        registry: Arc::new(Registry::for_process()),
        secrets: secrets.clone(),
        events,
        timing: knobas_sync::scheduler::SchedulerTiming::default(),
        quit: knobas_sync::scheduler::Quit::default(),
    };
    let scheduler = match events {
        None => Scheduler::start(deps(Arc::new(Quiet))).await,
        Some(events) => {
            knobas_app::sources::start_scheduler(
                deps(events(pool.clone())),
                knobas_sync::background::Background::new(),
            )
            .await
        }
    }
    .expect("a scheduler over the scratch database");

    SourcesState {
        pool,
        scheduler,
        secrets,
        registry: Arc::new(Registry::for_process()),
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

/// **`under:` a configuration, by its id and by its name, finds it and its
/// builds through the search the app runs** (#644).
///
/// The configurations are the fixture's: `work.json`'s build `cfg`s, each named
/// by `seed-teamcity-builds.sh`'s rule -- the id is the `cfg` byte for byte, the
/// name is what follows its first `_`, with `_` as a space. A name with a space
/// is typed quoted, as the query box writes it (#601).
///
/// Which builds sit in a configuration is read off `buildTypeId`, the field the
/// server sends at the build's root -- not the `buildType` object the
/// descriptor declares the parent at -- so a declaration that missed would
/// leave this set full and the search's empty. The set is every mirrored build
/// of that configuration, plus the configuration itself, which answers to its
/// own name and id as a parent. Each fixture build the seed recorded in
/// `seed-state.json` is checked to be in its configuration's set, so an empty
/// mirror cannot pass. Read-only, like the digest test above.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs testenv's seeded TeamCity: `just teamcity-live-seeded`"]
async fn under_each_seeded_configuration_finds_it_and_its_builds_by_id_and_name() {
    let env = env();
    let account = env.account().await;
    let state = app(&env, &account).await;
    sync(&state, TEAMCITY).await;

    let fixture = live_under::fixture();
    let mut configurations: Vec<String> = live_under::rows(&fixture, "builds")
        .iter()
        .map(|b| b["cfg"].as_str().expect("a build's cfg").to_owned())
        .collect();
    configurations.sort();
    configurations.dedup();
    assert_eq!(
        configurations.len(),
        3,
        "the fixture's three configurations"
    );

    let builds: Vec<(String, String)> = sqlx::query_as(
        "select entity_id, payload->>'buildTypeId'
           from sync.live_item where source_id = $1 and kind = 'build'",
    )
    .bind(TEAMCITY)
    .fetch_all(&state.pool)
    .await
    .expect("the mirror is readable");
    let seeded = seeded_builds();

    for id in &configurations {
        let name = id
            .split_once('_')
            .map(|(_, rest)| rest.replace('_', " "))
            .unwrap_or_else(|| panic!("{id} has a project prefix"));
        let of_config: BTreeSet<String> = builds
            .iter()
            .filter(|(_, config)| config == id)
            .map(|(entity, _)| entity.clone())
            .collect();
        for seeded in seeded.iter().filter(|b| &b.config == id) {
            let (number, build) = (
                seeded.number,
                format!("{TEAMCITY}:build:{}", seeded.real_id),
            );
            assert!(
                of_config.contains(&build),
                "fixture build {number} ({build}) of {id} is mirrored: {of_config:?}"
            );
        }
        let mut expected = BTreeMap::from([(
            "build_config".to_owned(),
            BTreeSet::from([format!("{TEAMCITY}:buildType:{id}")]),
        )]);
        if !of_config.is_empty() {
            expected.insert("build".to_owned(), of_config);
        }

        live_under::each_exactly(&state.pool, TEAMCITY, &[id.clone(), name], &expected).await;
    }
    state.scheduler.shutdown().await;
}

/// Puts one seeded parameter back to its recorded value when dropped,
/// passing or panicking alike: the fixture is shared with
/// `live_teamcity_seeded.rs`, whose view test asserts the seed's values.
struct ParameterBack {
    url: String,
    token: String,
    config: String,
    name: &'static str,
    value: String,
}

impl Drop for ParameterBack {
    fn drop(&mut self) {
        let url = format!(
            "{}/app/rest/buildTypes/id:{}/parameters/{}/value",
            self.url, self.config, self.name
        );
        let (token, value) = (self.token.clone(), self.value.clone());
        Cleanup::new(
            format!(
                "{} of {} was not put back to {:?}; re-run `./seed --teamcity`",
                self.name, self.config, self.value
            ),
            "it may still hold the value the test wrote",
        )
        .run(move || async move {
            let status = reqwest::Client::builder()
                .timeout(REQUEST_BUDGET)
                .build()
                .expect("a bounded client")
                .put(&url)
                .bearer_auth(&token)
                .header("Accept", "text/plain")
                .header("Content-Type", "text/plain")
                .body(value)
                .send()
                .await
                .map_err(|e| e.to_string())?
                .status();
            if status.is_success() {
                Ok(())
            } else {
                Err(format!("PUT answered {status}"))
            }
        });
    }
}

/// **A seeded parameter changed through the write queue reads back changed in
/// the next view** (spec #660 story 19, issue #668).
///
/// Every step is the shipped path, as `kuma_write_live.rs` has it for a
/// pause: `write_queue::submit` is what the *Parameters* tab's Enter calls --
/// it decodes the op, finds the source, checks the instance declares
/// `set_parameter`, writes the queue row with its whole-record snapshot,
/// flushes it -- and the view is the app's own read, `crud::view`, which is
/// what `entity_view` answers the detail with. What this adds to the adapter
/// suite's three cases is the queue itself: the row settles `sent`.
///
/// The configuration is synced first, so the write's target is in the mirror
/// and its hold snapshot is of a real record, as in the app.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs testenv's seeded TeamCity: `just teamcity-live-seeded`"]
async fn a_seeded_parameter_changed_through_the_write_queue_reads_back_in_the_next_view() {
    use knobas_app::sources::{crud, write_queue};
    use knobas_core::write_queue::WriteState;
    use knobas_source::view::Block;

    const NAME: &str = "env.SEPA_RETRIES";
    let env = env();
    let account = env.account().await;
    let config =
        live_under::seed_state("KNOBAS_TEAMCITY_SEED_STATE")["teamcity"]["settings"]["build_type"]
            .as_str()
            .unwrap_or_else(|| {
                panic!("seed-state.json has no teamcity.settings -- re-run `./seed --teamcity`")
            })
            .to_owned();
    let entity = format!("{TEAMCITY}:buildType:{config}");

    // The restore point, read from the server before anything is written.
    let before = env
        .get(&format!(
            "app/rest/buildTypes/id:{config}/parameters/{NAME}?fields=name,value"
        ))
        .await["value"]
        .as_str()
        .expect("the seeded parameter has a value")
        .to_owned();
    let back = ParameterBack {
        url: env.url.clone(),
        token: env.token.clone(),
        config: config.clone(),
        name: NAME,
        value: before.clone(),
    };

    let state = app(&env, &account).await;
    sync(&state, TEAMCITY).await;
    let value_in_view = async || -> String {
        let view = crud::view(&state.pool, &state.secrets, &state.registry, &entity, None)
            .await
            .unwrap_or_else(|e| panic!("the view of {entity} reads: {e:?}"))
            .expect("TeamCity draws a build configuration");
        let section = view
            .sections
            .iter()
            .find(|s| s.id == "parameters")
            .unwrap_or_else(|| panic!("no parameters: {view:#?}"));
        let [Block::Settings { values, .. }] = section.blocks.as_slice() else {
            panic!("one settings block: {section:#?}");
        };
        values
            .iter()
            .find(|s| s.name == NAME)
            .unwrap_or_else(|| panic!("no {NAME}: {values:#?}"))
            .value
            .clone()
    };
    let base = value_in_view().await;
    assert_eq!(base, before);

    // Edited against the value the view drew, as the shell sends it.
    let after = "knobas-668 through the queue";
    let queued = write_queue::submit(
        &state,
        json!({ "SetParameter": { "entity": entity, "name": NAME, "value": after, "base": base } }),
    )
    .await
    .expect("the queue takes a parameter set for a source that declares it");
    assert_eq!(queued.op, "set_parameter");
    assert_eq!(queued.entity_id, entity);
    // Settled, not merely queued: `submit` flushes before it returns.
    let settled = knobas_core::write_queue::get(&state.pool, queued.id)
        .await
        .expect("the queue row is readable")
        .expect("the queue row exists");
    assert_eq!(
        settled.state,
        WriteState::Sent,
        "the write did not leave the queue: {:?}",
        settled.detail
    );
    assert_eq!(
        value_in_view().await,
        after,
        "the next view reads the new value"
    );
    println!(
        "SEEDED {entity}: {NAME} {before:?} -> {after:?} through the write queue (row {} sent)",
        queued.id
    );

    drop(back);
    assert_eq!(
        value_in_view().await,
        before,
        "the parameter is back at its recorded value"
    );
    println!("SEEDED {entity}: {NAME} restored to {before:?} and read back so");
}

/// The mirrored Gitea commit whose page is `<gitea>/tidewater/<repo>/commit/<sha>`,
/// read by the web URL as Gitea wrote it -- not through the resolver under test.
async fn mirrored_commit(state: &SourcesState, gitea: &GiteaEnv, repo: &str, sha: &str) -> String {
    let page = format!("{}/tidewater/{repo}/commit/{sha}", gitea.url);
    sqlx::query_scalar(
        "select entity_id from sync.live_item
          where source_id = $1 and kind = 'commit' and web_url = $2",
    )
    .bind(GITEA)
    .bind(&page)
    .fetch_optional(&state.pool)
    .await
    .expect("the mirror is readable")
    .unwrap_or_else(|| panic!("the Gitea sync mirrored no commit at {page}"))
}

/// The rows of one entity-list block of a section, by the block's title.
fn rows_of<'v>(
    view: &'v knobas_source::view::SourceView,
    section: &str,
    title: Option<&str>,
) -> &'v [knobas_source::view::ViewRow] {
    use knobas_source::view::Block;
    let found = view
        .sections
        .iter()
        .find(|s| s.id == section)
        .unwrap_or_else(|| panic!("no section {section}: {view:#?}"));
    found
        .blocks
        .iter()
        .find_map(|b| match b {
            Block::Entities { title: t, rows } if t.as_deref() == title => Some(rows.as_slice()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no {title:?} rows in {section}: {found:#?}"))
}

/// **A build's change and its revision open the mirrored Gitea commit**
/// (#683, spec #660 story 23), with Gitea and TeamCity synced in one app.
///
/// The seed's change witness is a configuration built twice on a branch that
/// moved between, so its second build lists the head commit as a change; the
/// failed fixture build 1187 is its configuration's first and lists none, and
/// its *Revision* row is what still leads to its code. Both rows are drawn by
/// the app's own read, `crud::view`, which resolves the adapter's commit hint
/// against this mirror. The seeded VCS roots clone `http://gitea:3000`, the
/// compose network's name, while every mirrored commit's page carries Gitea's
/// ROOT_URL, `127.0.0.1:3000`; the Gitea source's `clone_urls` says the two
/// are one server, as a real deployment whose CI clones over an internal
/// hostname would.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs testenv's seeded TeamCity and Gitea: `just teamcity-live-seeded`"]
async fn a_builds_change_and_its_revision_open_the_mirrored_gitea_commit() {
    use knobas_app::sources::crud;

    let env = env();
    let gitea = gitea_env();
    let account = env.account().await;
    let state = app_with(&env, &account, Some(&gitea), None).await;
    sync(&state, TEAMCITY).await;
    sync(&state, GITEA).await;

    let view_of = async |id: i64| {
        let entity = format!("{TEAMCITY}:build:{id}");
        crud::view(&state.pool, &state.secrets, &state.registry, &entity, None)
            .await
            .unwrap_or_else(|e| panic!("the view of {entity} reads: {e:?}"))
            .expect("TeamCity draws a build")
    };

    // The change witness: one change, and the revision it ran on, both the
    // head the seed recorded.
    let changes = &live_under::seed_state("KNOBAS_TEAMCITY_SEED_STATE")["teamcity"]["changes"];
    let (after, repo, sha) = (
        changes["after_id"].as_i64().unwrap_or_else(|| {
            panic!("seed-state.json has no teamcity.changes -- re-run `./seed --teamcity`")
        }),
        changes["repo"].as_str().expect("a repo").to_owned(),
        changes["sha"].as_str().expect("a sha").to_owned(),
    );
    let commit = mirrored_commit(&state, &gitea, &repo, &sha).await;
    let view = view_of(after).await;
    let listed = rows_of(&view, "changes", None);
    assert_eq!(
        listed
            .iter()
            .map(|r| r.entity_id.as_deref())
            .collect::<Vec<_>>(),
        [Some(commit.as_str())],
        "build {after}'s one change opens the mirrored commit: {listed:#?}"
    );
    let built = rows_of(&view, "overview", Some("Revision"));
    assert_eq!(
        built
            .iter()
            .map(|r| r.entity_id.as_deref())
            .collect::<Vec<_>>(),
        [Some(commit.as_str())],
        "build {after}'s revision opens the same commit: {built:#?}"
    );
    println!(
        "SEEDED CHANGE ROW (build {after}): {}",
        serde_json::to_string(&listed[0]).expect("json")
    );
    println!(
        "SEEDED REVISION ROW (build {after}): {}",
        serde_json::to_string(&built[0]).expect("json")
    );

    // 1187: no change -- a first build -- and still a way to its code.
    let failed = seeded_build_id();
    let record = env
        .get(&format!(
            "app/rest/builds/id:{failed}?fields=revisions(revision(version))"
        ))
        .await;
    let built_sha = record["revisions"]["revision"][0]["version"]
        .as_str()
        .unwrap_or_else(|| panic!("build {failed} names no revision: {record}"))
        .to_owned();
    let commit = mirrored_commit(&state, &gitea, "payout-service", &built_sha).await;
    let view = view_of(failed).await;
    assert!(
        rows_of(&view, "changes", None).is_empty(),
        "fixture build {FIXTURE_BUILD} is its configuration's first and lists no change"
    );
    let built = rows_of(&view, "overview", Some("Revision"));
    assert_eq!(
        built
            .iter()
            .map(|r| r.entity_id.as_deref())
            .collect::<Vec<_>>(),
        [Some(commit.as_str())],
        "fixture build {FIXTURE_BUILD}'s revision opens the mirrored commit: {built:#?}"
    );
    println!(
        "SEEDED REVISION ROW (build {failed}, no changes): {}",
        serde_json::to_string(&built[0]).expect("json")
    );
    let wire = serde_json::to_string(&view).expect("json");
    assert!(
        !wire.contains("\"commit\""),
        "the hint never reaches the shell: {wire}"
    );

    state.scheduler.shutdown().await;
}

// -- a landed write's build, refreshed before the run after it (#2099) -------

/// What the mirror held the moment a refresh ended: the payload of the item
/// refreshed, and how many runs of its source had started since the write it
/// followed settled.
struct Snapshot {
    ended: RefreshEnded,
    payload: Option<serde_json::Value>,
    runs_since_the_write: i64,
}

/// Hears `item:refreshed` and reads the mirror **inside** the call. It is
/// told before the run after the write is asked for
/// (`sources::write_queue::after_landing`), so what it reads is what the
/// refresh wrote, not what that run did; the run count checks it rather than
/// trusting the order.
#[derive(Clone)]
struct Snapshots {
    pool: sqlx::PgPool,
    taken: Arc<std::sync::Mutex<Vec<Snapshot>>>,
}

impl SyncEvents for Snapshots {
    fn sync_state(&self, _status: knobas_sync::SourceSyncStatus) {}
    fn source_health(&self, _health: knobas_sync::CredentialHealth) {}
    fn activity_new(&self, _row: knobas_core::activity::ActivityRow) {}
    fn refresh_ended(&self, ended: &RefreshEnded) {
        let pool = self.pool.clone();
        let id = ended.entity_id.clone();
        let write = ended.write_id;
        let (payload, runs_since_the_write) = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async move {
                let payload: Option<serde_json::Value> =
                    sqlx::query_scalar("select payload from sync.live_item where entity_id = $1")
                        .bind(&id)
                        .fetch_optional(&pool)
                        .await
                        .expect("the mirror is readable");
                let runs: i64 = sqlx::query_scalar(
                    "select count(*) from knobas.sync_run r, knobas.write_queue w
                      where w.id = $1 and r.source_id = w.source_id
                        and r.started_at > w.settled_at",
                )
                .bind(write)
                .fetch_one(&pool)
                .await
                .expect("the runs are readable");
                (payload, runs)
            })
        });
        self.taken.lock().unwrap().push(Snapshot {
            ended: ended.clone(),
            payload,
            runs_since_the_write,
        });
    }
}

impl Snapshots {
    /// The snapshot of write `id`'s refresh, waited for: the refresh follows
    /// the send in the background.
    async fn of(&self, id: i64) -> Snapshot {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
        loop {
            {
                let mut taken = self.taken.lock().unwrap();
                if let Some(at) = taken.iter().position(|s| s.ended.write_id == Some(id)) {
                    return taken.remove(at);
                }
            }
            assert!(
                std::time::Instant::now() < deadline,
                "write {id} landed but no refresh of it ended within a minute"
            );
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        }
    }
}

/// A bounded client for the guards' own requests.
fn bounded() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(REQUEST_BUDGET)
        .build()
        .expect("a bounded client")
}

/// Every enabled agent disabled while the guard lives, so the builds the
/// writes queue stay on the queue, and re-enabled from `Drop` passing or
/// panicking alike -- `live_teamcity_seeded.rs`'s `AgentsParked`, which this
/// crate cannot reach. Told each agent the moment it disabled it.
struct Parked {
    url: String,
    token: String,
    disabled: Vec<String>,
}

impl Parked {
    async fn park(&mut self, env: &Env) {
        let body = env
            .get("app/rest/agents?locator=authorized:any&fields=agent(id,enabled)")
            .await;
        let agents = body["agent"].as_array().cloned().unwrap_or_default();
        assert!(!agents.is_empty(), "the server lists an agent: {body}");
        for agent in agents {
            if agent["enabled"] == json!(false) {
                continue;
            }
            let id = agent["id"].to_string().trim_matches('"').to_owned();
            self.disabled.push(id.clone());
            let status = bounded()
                .put(format!("{}/app/rest/agents/id:{id}/enabled", self.url))
                .header("Content-Type", "text/plain")
                .header("Accept", "text/plain")
                .bearer_auth(&self.token)
                .body("false")
                .send()
                .await
                .unwrap_or_else(|e| panic!("disable agent {id}: {e}"))
                .status();
            assert!(status.is_success(), "disable agent {id} -> {status}");
        }
    }
}

impl Drop for Parked {
    fn drop(&mut self) {
        if self.disabled.is_empty() {
            return;
        }
        let (url, token, ids) = (self.url.clone(), self.token.clone(), self.disabled.clone());
        Cleanup::new(
            "the live suite did not re-enable the agents it disabled, so no later build will \
             run (enable them under Agents in TeamCity, or re-seed)",
            format!("agents {ids:?} may still be disabled"),
        )
        .run(move || async move {
            for id in ids {
                let status = bounded()
                    .put(format!("{url}/app/rest/agents/id:{id}/enabled"))
                    .header("Content-Type", "text/plain")
                    .header("Accept", "text/plain")
                    .bearer_auth(&token)
                    .body("true")
                    .send()
                    .await
                    .map_err(|e| format!("enable agent {id}: {e}"))?
                    .status();
                if !status.is_success() {
                    return Err(format!("enable agent {id} -> {status}"));
                }
            }
            Ok(())
        });
    }
}

/// The builds the writes queued, canceled and deleted from `Drop` passing or
/// panicking alike, and checked gone. What a killed run leaves is the
/// adapter suite's leftover clearing's, which removes every build the seed
/// did not record.
struct QueuedBuilds {
    url: String,
    token: String,
    ids: Arc<std::sync::Mutex<Vec<i64>>>,
}

impl Drop for QueuedBuilds {
    fn drop(&mut self) {
        let ids = std::mem::take(&mut *self.ids.lock().unwrap());
        if ids.is_empty() {
            return;
        }
        let (url, token) = (self.url.clone(), self.token.clone());
        Cleanup::new(
            "the live suite did not remove the builds its writes queued, so the server is no \
             longer in the plain-seed state",
            format!(
                "builds {ids:?} may still be standing; the adapter suite's leftover clearing \
                 removes them"
            ),
        )
        .run(move || async move {
            let http = bounded();
            for id in ids {
                let at = format!("{url}/app/rest/builds/id:{id}");
                // Cancels a queued build as well as a running one, which then
                // finishes canceled and can be deleted.
                let canceled = http
                    .post(&at)
                    .header("Accept", "application/json")
                    .bearer_auth(&token)
                    .json(&json!({ "comment": "canceled by the knobas app's TeamCity live suite", "readdIntoQueue": false }))
                    .send()
                    .await
                    .map_err(|e| format!("cancel build {id}: {e}"))?;
                if !canceled.status().is_success() && canceled.status() != 404 {
                    return Err(format!("cancel build {id} -> {}", canceled.status()));
                }
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
                loop {
                    let state = http
                        .get(format!("{at}?fields=state"))
                        .header("Accept", "application/json")
                        .bearer_auth(&token)
                        .send()
                        .await
                        .map_err(|e| format!("GET build {id}: {e}"))?;
                    if state.status() == 404 {
                        break;
                    }
                    let body: serde_json::Value = state.json().await.unwrap_or_default();
                    if body["state"] == "finished" {
                        let deleted = http
                            .delete(&at)
                            .bearer_auth(&token)
                            .send()
                            .await
                            .map_err(|e| format!("DELETE build {id}: {e}"))?
                            .status();
                        if !deleted.is_success() && deleted != 404 {
                            return Err(format!("DELETE build {id} -> {deleted}"));
                        }
                    }
                    if std::time::Instant::now() > deadline {
                        return Err(format!("build {id} is still there after its delete: {body}"));
                    }
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                }
            }
            Ok(())
        });
    }
}

/// The build key a queueing write's settle stamped (`build:<id>`, #1949).
async fn remote_id(pool: &sqlx::PgPool, id: i64) -> String {
    sqlx::query_scalar::<_, Option<String>>(
        "select remote_id from knobas.write_queue where id = $1",
    )
    .bind(id)
    .fetch_one(pool)
    .await
    .expect("the queue row")
    .unwrap_or_else(|| panic!("write {id} carries no remote id"))
}

/// **A trigger and a re-run, each through the write queue, are in the mirror
/// as the build TeamCity queued, by the key the receipt names, before the
/// run after the write starts** (#2099, ADR-0031): the refresh reads the
/// queued build by id, so it needs no run to have listed it.
///
/// The agents are parked, so each build is still on the queue when the
/// refresh reads it and when the test compares it with the server's record;
/// both builds are canceled and deleted again, and the agents re-enabled.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs testenv's seeded TeamCity: `just teamcity-live-seeded`"]
async fn a_landed_trigger_and_rerun_are_mirrored_as_their_queued_build_before_the_run_after_them() {
    use knobas_app::sources::write_queue;
    use knobas_core::write_queue::WriteState;

    let env = env();
    let account = env.account().await;
    let rerun_of = seeded_build_id();
    let rerun_config = seeded_builds()
        .into_iter()
        .find(|b| b.real_id == rerun_of)
        .expect("the fixture build")
        .config;
    let trigger_config = seeded_builds()
        .into_iter()
        .map(|b| b.config)
        .find(|c| *c != rerun_config)
        .expect("a second seeded configuration");

    let mut parked = Parked {
        url: env.url.clone(),
        token: env.token.clone(),
        disabled: Vec::new(),
    };
    let queued = QueuedBuilds {
        url: env.url.clone(),
        token: env.token.clone(),
        ids: Arc::default(),
    };

    let taken = Arc::new(std::sync::Mutex::new(Vec::new()));
    let heard = std::sync::Mutex::new(None);
    let state = app_with(
        &env,
        &account,
        None,
        Some(&|pool: sqlx::PgPool| -> Arc<dyn SyncEvents> {
            let snapshots = Snapshots {
                pool,
                taken: Arc::clone(&taken),
            };
            *heard.lock().unwrap() = Some(snapshots.clone());
            Arc::new(snapshots)
        }),
    )
    .await;
    let snapshots = heard.lock().unwrap().take().expect("the events were built");
    sync(&state, TEAMCITY).await;
    parked.park(&env).await;

    for (what, op, config) in [
        (
            "a trigger",
            json!({ "TriggerBuild": { "entity": format!("{TEAMCITY}:buildType:{trigger_config}") } }),
            &trigger_config,
        ),
        (
            "a re-run",
            json!({ "RerunBuild": { "entity": format!("{TEAMCITY}:build:{rerun_of}") } }),
            &rerun_config,
        ),
    ] {
        let sent = write_queue::submit(&state, op)
            .await
            .unwrap_or_else(|e| panic!("the queue takes {what}: {e:?}"));
        let row = knobas_core::write_queue::get(&state.pool, sent.id)
            .await
            .expect("the queue row is readable")
            .expect("the queue row exists");
        assert_eq!(row.state, WriteState::Sent, "{what}: {:?}", row.detail);
        let key = remote_id(&state.pool, sent.id).await;
        let id: i64 = key
            .strip_prefix("build:")
            .and_then(|n| n.parse().ok())
            .unwrap_or_else(|| panic!("{what}'s receipt names a build: {key:?}"));
        queued.ids.lock().unwrap().push(id);

        let shot = snapshots.of(sent.id).await;
        assert_eq!(
            shot.ended.entity_id,
            format!("{TEAMCITY}:{key}"),
            "{what} is refreshed by the build its receipt names"
        );
        assert_eq!(shot.ended.ended, RefreshEnd::Refreshed, "{what}");
        assert_eq!(
            shot.runs_since_the_write, 0,
            "{what}: a run started before the refresh ended"
        );
        let payload = shot
            .payload
            .unwrap_or_else(|| panic!("{what}'s build is mirrored"));
        let theirs = env
            .get(&format!(
                "app/rest/builds/id:{id}?fields=id,state,buildTypeId,branchName,queuedDate"
            ))
            .await;
        println!("SEEDED {what}: mirrored {payload}; TeamCity holds {theirs}");
        assert_eq!(payload["id"], json!(id), "{what}");
        assert_eq!(payload["state"], "queued", "{what}: {payload}");
        assert_eq!(payload["state"], theirs["state"], "{what}");
        assert_eq!(payload["buildTypeId"], json!(config), "{what}");
        assert_eq!(payload["branchName"], theirs["branchName"], "{what}");
        assert_eq!(payload["queuedDate"], theirs["queuedDate"], "{what}");
    }

    state.scheduler.shutdown().await;
    drop(queued);
    drop(parked);
}
