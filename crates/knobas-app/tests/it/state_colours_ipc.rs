//! *State colours* (#2465, spec #2463): a reader's tone override for one
//! state word of one kind reaches every surface that draws the state, with
//! no resync, and never the stored parent declaration.
//!
//! Each test has a database of its own: an override is per kind across the
//! whole installation, so two tests sharing one would colour each other's
//! rows.

use chrono::Utc;
use knobas_app::commands::entity::list_entities_inner;
use knobas_app::link_sets::{self, LinkSetDraft, LinkSetMember, LinkSetRule};
use knobas_app::sources::Registry;
use knobas_app::sources::state_colours::{self, KindStateColours, ToneOrigin};
use knobas_core::entity_list::{EntityFilter, EntityOrder};
use knobas_core::inbox::{Category, Shelf};
use knobas_core::payload::StateChip;
use knobas_core::state_colours::ToneChoice;
use knobas_core::tone::Tone;
use knobas_search::{SearchFilters, SearchQuery};
use sqlx::PgPool;

const ME: &str = "mara.lindqvist";

/// A database of this test's own, with a TeamCity, a Gitea, a mock and a
/// Jira configured and the declarations stored as bring-up stores them.
async fn pool(label: &str) -> PgPool {
    let pool = knobas_db::test_util::scratch_database(label)
        .await
        .pool(4)
        .await
        .expect("a pool onto this test's own database");
    for (id, kind) in [
        ("tc", "teamcity"),
        ("git", "gitea"),
        ("mock", "mock"),
        ("jira", "jira"),
    ] {
        sqlx::query(
            "insert into knobas.source_config (id, kind, display_name, base_url, auth_kind, config)
             values ($1, $2, $1, 'http://x.example', 'Pat', '{}')",
        )
        .bind(id)
        .bind(kind)
        .execute(&pool)
        .await
        .unwrap();
    }
    knobas_app::commands::search::searcher(&pool, &Registry::for_process())
        .await
        .unwrap();
    pool
}

/// One live mirror item, authored by [`ME`], and its entity row.
async fn mirrored(
    pool: &PgPool,
    source: &str,
    kind: &str,
    key: &str,
    title: &str,
    payload: serde_json::Value,
) -> String {
    let id = format!("{source}:{key}");
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1,$2,$3)")
        .bind(&id)
        .bind(kind)
        .bind(title)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into sync.item
           (entity_id, source_id, kind, title, body_text, author, payload, item_updated_at)
         values ($1,$2,$3,$4,'',$5,$6, now())",
    )
    .bind(&id)
    .bind(source)
    .bind(kind)
    .bind(title)
    .bind(ME)
    .bind(payload)
    .execute(pool)
    .await
    .unwrap();
    id
}

/// A TeamCity build of `config` in `state` with `status`.
async fn build(pool: &PgPool, config: &str, n: u32, state: &str, status: &str) -> String {
    let key = format!("{config}-{n}");
    mirrored(
        pool,
        "tc",
        "build",
        &key,
        &format!("{config} {n}"),
        serde_json::json!({
            "id": key, "buildTypeId": config, "number": n.to_string(),
            "state": state, "status": status,
            "buildType": { "id": config, "name": config },
        }),
    )
    .await
}

fn filter(sources: &[&str], kinds: &[&str]) -> EntityFilter {
    EntityFilter {
        sources: sources.iter().map(|s| (*s).to_owned()).collect(),
        kinds: kinds.iter().map(|s| (*s).to_owned()).collect(),
        updated_within_days: None,
        context: None,
        project: None,
        order: EntityOrder::UpdatedDesc,
        include_deleted: false,
        reach: None,
        role: None,
    }
}

/// The chip a room row of `id` draws, through the command's own route.
async fn room_chip(pool: &PgPool, source: &str, kind: &str, id: &str) -> Option<StateChip> {
    let registry = Registry::for_process();
    let declarations = knobas_app::sources::paths::declared_paths(pool, &registry)
        .await
        .unwrap();
    let page = list_entities_inner(
        pool,
        &registry,
        &filter(&[source], &[kind]),
        100,
        0,
        &declarations,
    )
    .await
    .unwrap();
    page.rows
        .into_iter()
        .find(|row| row.entity_id == id)
        .unwrap_or_else(|| panic!("{id} is not in the room"))
        .state
}

fn tone_of(chip: Option<StateChip>) -> Option<Tone> {
    chip.and_then(|chip| chip.tone)
}

/// The tone a launcher search for `raw` draws `id` in.
async fn search_tone(pool: &PgPool, raw: &str, id: &str) -> Option<Tone> {
    let response = knobas_app::commands::search::search_inner(
        pool,
        &Registry::for_process(),
        SearchQuery {
            titles: false,
            raw: raw.to_owned(),
            limit: 20,
            filters: SearchFilters::default(),
        },
    )
    .await
    .unwrap();
    let hit = response
        .groups
        .iter()
        .flat_map(|group| &group.hits)
        .find(|hit| hit.row.entity_id == id)
        .unwrap_or_else(|| panic!("the search for {raw} did not find {id}"));
    tone_of(hit.row.state.clone())
}

/// The ids of the failed builds in [`ME`]'s inbox.
async fn failed_builds(pool: &PgPool) -> Vec<String> {
    knobas_app::inbox::stream(
        pool,
        &Registry::for_process(),
        &[ME.to_owned()],
        Utc::now(),
        Shelf::Stream,
    )
    .await
    .unwrap()
    .into_iter()
    .filter(|entry| entry.item.category == Category::FailedBuild)
    .filter_map(|entry| entry.item.entity_id)
    .collect()
}

/// The dot a link set's pipeline member draws: its newest run's state.
async fn set_dot(pool: &PgPool, pipeline: &str, on: &str) -> Option<Tone> {
    let saved = link_sets::save(
        pool,
        &Registry::for_process(),
        LinkSetDraft {
            id: None,
            name: format!("Set {}", uuid::Uuid::new_v4().simple()),
            description: String::new(),
            members: vec![LinkSetMember {
                kind: "pipeline".to_owned(),
                entity_id: pipeline.to_owned(),
            }],
            patterns: Vec::new(),
            rules: vec![LinkSetRule {
                kind: "manual".to_owned(),
                value: on.to_owned(),
            }],
        },
    )
    .await
    .unwrap();
    tone_of(saved.set.members[0].state.clone())
}

async fn set(pool: &PgPool, kind: &str, word: &str, choice: ToneChoice) {
    state_colours::set(pool, kind, word, choice).await.unwrap();
}

/// A TeamCity `FAILURE` moved to `warn` is `warn` on a search hit (the
/// pipeline's, which draws its newest run's state: runs are not in the
/// launcher), a room row and a set dot, and the build leaves the inbox's failed builds: the
/// tone is the fact, not only its colour.
#[tokio::test]
async fn a_failure_set_to_warn_is_warn_everywhere_and_no_longer_failed() {
    let pool = pool("state_colours_failure_warn").await;
    let config = "Payouts_Build";
    let pipeline = mirrored(
        &pool,
        "tc",
        "build_config",
        config,
        "zipipe Payouts Build",
        serde_json::json!({
            "id": config, "name": config, "projectId": "Payouts", "projectName": "Payouts",
            "projectPath": [{ "name": "Root" }, { "name": "Payouts" }],
        }),
    )
    .await;
    let red = build(&pool, config, 1, "finished", "FAILURE").await;
    let ticket = mirrored(
        &pool,
        "jira",
        "ticket",
        "PAY-1",
        "PAY-1",
        serde_json::json!({ "key": "PAY-1", "fields": { "project": { "key": "PAY" } } }),
    )
    .await;

    let before = (
        search_tone(&pool, "zipipe", &pipeline).await,
        tone_of(room_chip(&pool, "tc", "build", &red).await),
        set_dot(&pool, &pipeline, &ticket).await,
        failed_builds(&pool).await,
    );
    assert_eq!(
        before,
        (
            Some(Tone::Err),
            Some(Tone::Err),
            Some(Tone::Err),
            vec![red.clone()]
        ),
        "the adapter's tone, before any override"
    );

    set(&pool, "build", "FAILURE", ToneChoice::Warn).await;

    let after = (
        search_tone(&pool, "zipipe", &pipeline).await,
        tone_of(room_chip(&pool, "tc", "build", &red).await),
        set_dot(&pool, &pipeline, &ticket).await,
        failed_builds(&pool).await,
    );
    assert_eq!(
        after,
        (
            Some(Tone::Warn),
            Some(Tone::Warn),
            Some(Tone::Warn),
            Vec::<String>::new()
        ),
        "the override, on every surface, and the build is no longer failed"
    );
}

/// An override is per kind, not per adapter: Gitea's `closed` set to `err`
/// colours a mock pull request too.
#[tokio::test]
async fn an_override_on_a_kind_reaches_every_adapter_of_it() {
    let pool = pool("state_colours_per_kind").await;
    let repo = serde_json::json!({ "full_name": "tidewater/payouts", "name": "payouts" });
    let gitea = mirrored(
        &pool,
        "git",
        "pr",
        "tidewater/payouts#1",
        "One",
        serde_json::json!({ "state": "closed", "merged": false, "base": { "repo": repo } }),
    )
    .await;
    let mock = mirrored(
        &pool,
        "mock",
        "pr",
        "payouts#2",
        "Two",
        serde_json::json!({ "num": 2, "repo": "payouts", "state": "closed" }),
    )
    .await;
    let tones = async |pool: &PgPool| {
        (
            tone_of(room_chip(pool, "git", "pr", &gitea).await),
            tone_of(room_chip(pool, "mock", "pr", &mock).await),
        )
    };
    assert_eq!(tones(&pool).await, (Some(Tone::Muted), Some(Tone::Muted)));

    set(&pool, "pr", "closed", ToneChoice::Err).await;

    assert_eq!(tones(&pool).await, (Some(Tone::Err), Some(Tone::Err)));
}

/// *No colour* on `running` stops the walk: a running build draws `running`
/// with no tone, not its interim `status`.
#[tokio::test]
async fn no_colour_on_running_draws_the_word_and_no_tone() {
    let pool = pool("state_colours_no_colour").await;
    let running = build(&pool, "Ledger_Build", 1, "running", "SUCCESS").await;

    set(&pool, "build", "running", ToneChoice::None).await;

    assert_eq!(
        room_chip(&pool, "tc", "build", &running).await,
        Some(StateChip {
            text: "running".to_owned(),
            tone: None
        })
    );
}

/// A Jira status no adapter names, once the mirror has seen it, is offered
/// and can be coloured: it is added to the open candidate.
#[tokio::test]
async fn an_undeclared_seen_status_can_be_coloured() {
    let pool = pool("state_colours_seen_status").await;
    let blocked = mirrored(
        &pool,
        "jira",
        "ticket",
        "PAY-7",
        "PAY-7",
        serde_json::json!({ "key": "PAY-7", "fields": { "status": { "name": "Blocked" } } }),
    )
    .await;
    assert_eq!(room_chip(&pool, "jira", "ticket", &blocked).await, None);

    set(&pool, "ticket", "Blocked", ToneChoice::Err).await;

    assert_eq!(
        room_chip(&pool, "jira", "ticket", &blocked).await,
        Some(StateChip {
            text: "Blocked".to_owned(),
            tone: Some(Tone::Err)
        })
    );
}

/// Clearing a word restores the adapter's tone for that word alone;
/// clearing the kind restores all of them.
#[tokio::test]
async fn clearing_a_word_then_the_kind_restores_the_adapters_tones() {
    let pool = pool("state_colours_clear").await;
    let red = build(&pool, "Fees_Build", 1, "finished", "FAILURE").await;
    let running = build(&pool, "Fees_Build", 2, "running", "SUCCESS").await;
    let tones = async |pool: &PgPool| {
        (
            tone_of(room_chip(pool, "tc", "build", &red).await),
            tone_of(room_chip(pool, "tc", "build", &running).await),
        )
    };
    set(&pool, "build", "FAILURE", ToneChoice::Warn).await;
    set(&pool, "build", "running", ToneChoice::Muted).await;
    assert_eq!(tones(&pool).await, (Some(Tone::Warn), Some(Tone::Muted)));

    let cleared = state_colours::clear(&pool, "build", Some("FAILURE"))
        .await
        .unwrap();
    assert!(
        cleared.is_some(),
        "a clear that forgot a word writes its line"
    );
    assert_eq!(tones(&pool).await, (Some(Tone::Err), Some(Tone::Muted)));

    state_colours::clear(&pool, "build", None).await.unwrap();
    assert_eq!(tones(&pool).await, (Some(Tone::Err), Some(Tone::Info)));

    assert!(
        state_colours::clear(&pool, "build", None)
            .await
            .unwrap()
            .is_none(),
        "a clear that forgot nothing is no change and writes no line"
    );
}

fn words(kinds: &[KindStateColours], kind: &str) -> Vec<String> {
    kinds
        .iter()
        .find(|k| k.kind == kind)
        .unwrap_or_else(|| panic!("no {kind} section in {kinds:?}"))
        .words
        .iter()
        .map(|w| w.word.clone())
        .collect()
}

/// The read offers Jira's seen statuses after its declared ones,
/// alphabetically, and TeamCity's declared words, never the pass-through
/// `finished`; and it says where each tone comes from.
#[tokio::test]
async fn the_read_offers_declared_then_seen_words_and_never_finished() {
    let pool = pool("state_colours_read").await;
    for (key, status) in [
        ("PAY-1", "To Do"),
        ("PAY-2", "Blocked"),
        ("PAY-3", "Done"),
        ("PAY-4", " Blocked "),
    ] {
        mirrored(
            &pool,
            "jira",
            "ticket",
            key,
            key,
            serde_json::json!({ "key": key, "fields": { "status": { "name": status } } }),
        )
        .await;
    }
    build(&pool, "Ledger_Build", 1, "finished", "SUCCESS").await;
    set(&pool, "build", "FAILURE", ToneChoice::Warn).await;

    let kinds = state_colours::read(&pool, &Registry::for_process())
        .await
        .unwrap();

    assert_eq!(
        kinds.iter().map(|k| k.kind.as_str()).collect::<Vec<_>>(),
        ["build", "monitor", "pr", "ticket"],
        "every kind a configured source declares a state for"
    );
    assert_eq!(
        words(&kinds, "ticket"),
        [
            "Done",
            "Resolved",
            "In Progress",
            "In Review",
            "Closed",
            "Blocked",
            "To Do"
        ]
    );
    assert_eq!(
        words(&kinds, "build"),
        [
            "running", "failed", "success", "queued", "SUCCESS", "FAILURE", "UNKNOWN"
        ],
        "declared words only, the mock's then TeamCity's: no build candidate is open, so \
         TeamCity's `finished` is never offered"
    );
    let build = &kinds.iter().find(|k| k.kind == "build").unwrap().words;
    let failure = build.iter().find(|w| w.word == "FAILURE").unwrap();
    assert_eq!(
        (
            failure.declared_tone,
            failure.yours,
            failure.tone,
            failure.origin
        ),
        (
            Some(Tone::Err),
            Some(ToneChoice::Warn),
            Some(Tone::Warn),
            ToneOrigin::Yours
        )
    );
    let success = build.iter().find(|w| w.word == "SUCCESS").unwrap();
    assert_eq!(
        (success.yours, success.tone, success.origin),
        (None, Some(Tone::Ok), ToneOrigin::Adapter)
    );
    let ticket = &kinds.iter().find(|k| k.kind == "ticket").unwrap().words;
    let blocked = ticket.iter().find(|w| w.word == "Blocked").unwrap();
    assert_eq!(
        (
            blocked.declared,
            blocked.category_tone,
            blocked.tone,
            blocked.origin
        ),
        (false, None, None, ToneOrigin::Unset),
        "a status with no category on record falls back to nothing"
    );
}

/// A seen status no name maps reports the tone Jira's status category gives
/// its tickets (#2468), with origin *category*; a to-do status has none and
/// stays unset, and the reader's override still wins. The category keys
/// themselves are no words: the chip never says them.
#[tokio::test]
async fn the_read_reports_a_seen_statuss_category_tone() {
    let pool = pool("state_colours_category").await;
    for (key, status, category) in [
        ("PAY-1", "Ready for QA", "indeterminate"),
        ("PAY-2", "Shipped", "done"),
        ("PAY-3", "Backlog", "new"),
        ("PAY-4", "Parked", "indeterminate"),
        ("PAY-5", "Closed", "done"),
    ] {
        mirrored(
            &pool,
            "jira",
            "ticket",
            key,
            key,
            serde_json::json!({ "key": key, "fields": { "status": {
                "name": status, "statusCategory": { "key": category, "name": "x" },
            } } }),
        )
        .await;
    }
    set(&pool, "ticket", "Parked", ToneChoice::Err).await;

    let kinds = state_colours::read(&pool, &Registry::for_process())
        .await
        .unwrap();
    assert_eq!(
        words(&kinds, "ticket"),
        [
            "Done",
            "Resolved",
            "In Progress",
            "In Review",
            "Closed",
            "Backlog",
            "Parked",
            "Ready for QA",
            "Shipped",
        ]
    );
    let ticket = &kinds.iter().find(|k| k.kind == "ticket").unwrap().words;
    let drawn = |word: &str| {
        let w = ticket.iter().find(|w| w.word == word).unwrap();
        (w.category_tone, w.tone, w.origin)
    };
    assert_eq!(
        drawn("Ready for QA"),
        (Some(Tone::Info), Some(Tone::Info), ToneOrigin::Category)
    );
    assert_eq!(
        drawn("Shipped"),
        (Some(Tone::Ok), Some(Tone::Ok), ToneOrigin::Category)
    );
    assert_eq!(drawn("Backlog"), (None, None, ToneOrigin::Unset));
    assert_eq!(
        drawn("Parked"),
        (Some(Tone::Info), Some(Tone::Err), ToneOrigin::Yours)
    );
    assert_eq!(
        drawn("Closed"),
        (None, Some(Tone::Muted), ToneOrigin::Adapter),
        "an exact name is the adapter's, whatever its category"
    );
}

/// A word's category tone is the most common outcome among its items, no
/// tone counted as one (#2468): with two Jiras, a status most of whose
/// tickets are to do stays unset, though the other Jira files one under in
/// progress; one most of whose tickets are in progress takes `info`.
#[tokio::test]
async fn a_seen_statuss_category_tone_counts_its_items_that_draw_none() {
    let pool = pool("state_colours_category_mode").await;
    sqlx::query(
        "insert into knobas.source_config (id, kind, display_name, base_url, auth_kind, config)
         values ('jira2', 'jira', 'jira2', 'http://y.example', 'Pat', '{}')",
    )
    .execute(&pool)
    .await
    .unwrap();
    for (source, key, status, category) in [
        ("jira", "A-1", "Review", "new"),
        ("jira", "A-2", "Review", "new"),
        ("jira2", "B-1", "Review", "indeterminate"),
        ("jira", "A-3", "Testing", "indeterminate"),
        ("jira", "A-4", "Testing", "indeterminate"),
        ("jira2", "B-2", "Testing", "new"),
    ] {
        mirrored(
            &pool,
            source,
            "ticket",
            key,
            key,
            serde_json::json!({ "key": key, "fields": { "status": {
                "name": status, "statusCategory": { "key": category, "name": "x" },
            } } }),
        )
        .await;
    }
    let kinds = state_colours::read(&pool, &Registry::for_process())
        .await
        .unwrap();
    let ticket = &kinds.iter().find(|k| k.kind == "ticket").unwrap().words;
    let drawn = |word: &str| {
        let w = ticket.iter().find(|w| w.word == word).unwrap();
        (w.category_tone, w.origin)
    };
    assert_eq!(drawn("Review"), (None, ToneOrigin::Unset));
    assert_eq!(drawn("Testing"), (Some(Tone::Info), ToneOrigin::Category));
}

/// A set never touches `sync.parent_declaration`: the stored copy stays the
/// adapters' own, byte for byte, through the searcher's next write of it.
#[tokio::test]
async fn a_set_leaves_the_stored_parent_declaration_byte_identical() {
    let pool = pool("state_colours_parent_declaration").await;
    let stored = async |pool: &PgPool| -> Vec<(String, String)> {
        sqlx::query_as(
            "select adapter_kind, payload_paths::text from sync.parent_declaration order by 1",
        )
        .fetch_all(pool)
        .await
        .unwrap()
    };
    let before = stored(&pool).await;
    assert!(!before.is_empty());

    set(&pool, "build", "FAILURE", ToneChoice::Warn).await;
    set(&pool, "ticket", "Blocked", ToneChoice::Err).await;
    knobas_app::commands::search::searcher(&pool, &Registry::for_process())
        .await
        .unwrap();
    search_tone_any(&pool).await;

    assert_eq!(stored(&pool).await, before);
}

/// A search, which builds the searcher's own declarations and stores the
/// adapters' copy.
async fn search_tone_any(pool: &PgPool) {
    knobas_app::commands::search::search_inner(
        pool,
        &Registry::for_process(),
        SearchQuery {
            titles: false,
            raw: "anything".to_owned(),
            limit: 5,
            filters: SearchFilters::default(),
        },
    )
    .await
    .unwrap();
}
