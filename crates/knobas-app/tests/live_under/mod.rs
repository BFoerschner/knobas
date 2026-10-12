//! **`under:` through the search the app runs, on records a real server sent**
//! (issue #644).
//!
//! Each adapter's own live suite holds its records against the descriptor's
//! declaration with `knobas_core::payload`'s resolvers -- the Rust half of the
//! rule. The SQL half (the `sync.item_parent` trigger, the stored declarations,
//! the read-time `source_config` join, the parser and the predicate) ran only
//! on payloads a test author typed. These helpers are the wire between them:
//! the searcher is built the way the app builds it,
//! [`knobas_app::commands::search::searcher`], which writes
//! `sync.parent_declaration` before it answers, and the query goes through
//! `Searcher::search` as the launcher sends it.
//!
//! A directory rather than a test target, for `live_digest`'s reason: cargo
//! does not pick it up on its own, and each suite that wants it says
//! `mod live_under;`.
//!
//! **The expected sets are not here.** Each suite takes them from the fixture
//! (`fixtures/tidewater/work.json`, `testenv/seed-state.json`) or from what
//! its server sent outside the declared parent path. Nothing in this module
//! resolves a parent, so nothing here can agree with a wrong declaration.

#![allow(dead_code)]

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use knobas_search::types::{FilterAnswer, FilterDimension};

/// What the query box writes for a picked name: quoted when it has a space
/// (the deputy's ruling of 2026-10-12 on #597).
pub fn under(name: &str) -> String {
    if name.chars().any(char::is_whitespace) {
        format!("under:\"{name}\"")
    } else {
        format!("under:{name}")
    }
}

/// Store the adapters' declarations the way bring-up does, **before** the
/// scheduler starts and the first sync writes an item (#627).
///
/// The order is the app's, and it decides which half of the trigger work a
/// witness runs: declared first, each synced row's parents are resolved by the
/// `sync.item` insert trigger as the sync writer's upsert lands; declared
/// after, by the declaration trigger recomputing every row at once. The app
/// takes the first path on every sync but its very first launch's, so that is
/// the one these suites hold. Every builder that wants it calls this between
/// the scratch database and `Scheduler::start` -- a scheduler that is running
/// may already be syncing.
pub async fn declare(pool: &sqlx::PgPool) {
    knobas_app::commands::search::searcher(pool)
        .await
        .expect("the adapters' declarations are stored");
}

/// What one `under:` query found: per kind, how many rows matched and the ids
/// the page shows.
///
/// Per kind because the page is: the search caps each kind's group at
/// `knobas_search`'s `PER_GROUP` (ten rows, so one flood of commits cannot own
/// the launcher) and reports the kind's true `total` beside it. A repository
/// with nineteen commits therefore shows ten of them, and the other nine are a
/// count. [`exactly`] holds both halves to the expected set.
#[derive(Debug)]
pub struct Found {
    pub kinds: BTreeMap<String, Group>,
}

#[derive(Debug)]
pub struct Group {
    pub total: usize,
    pub ids: BTreeSet<String>,
}

impl Found {
    /// Every id the page shows, of every kind.
    pub fn ids(&self) -> BTreeSet<String> {
        self.kinds.values().flat_map(|g| g.ids.iter().cloned()).collect()
    }
}

/// Run `raw` through `Searcher::search`, built the app's way, after the two
/// checks every `under:` witness owes:
///
/// * the token is **not reported unknown** -- a value the parser dropped would
///   leave a plain search, and a plain search can still find the parent by its
///   title;
/// * the `under:` coverage lists `source` as **answered** -- the verdict the
///   launcher prints when the result is empty, measured on the same corpus.
pub async fn search(pool: &sqlx::PgPool, source: &str, raw: &str) -> Found {
    let response = knobas_app::commands::search::searcher(pool)
        .await
        .expect("the searcher is built the app's way")
        .search(knobas_search::SearchQuery {
            raw: raw.to_owned(),
            limit: 200,
            filters: knobas_search::SearchFilters::default(),
        })
        .await
        .unwrap_or_else(|e| panic!("{raw}: the search runs: {e}"));
    assert!(
        response.interpreted.unknown_tokens.is_empty(),
        "{raw} names a parent and is not unknown: {:?}",
        response.interpreted.unknown_tokens
    );
    let coverage = response
        .coverage
        .iter()
        .find(|c| c.dimension == FilterDimension::Under)
        .unwrap_or_else(|| {
            panic!(
                "{raw} filters by parent, so its coverage is reported: {:?}",
                response.coverage
            )
        });
    let answer = coverage
        .sources
        .iter()
        .find(|s| s.source_id == source)
        .unwrap_or_else(|| panic!("{raw}: {source} is in the under: coverage: {coverage:?}"));
    assert_eq!(
        answer.answer,
        FilterAnswer::Answered,
        "{raw}: {source} names parents on its synced records, so it answers under:"
    );
    Found {
        kinds: response
            .groups
            .iter()
            .map(|g| {
                (
                    g.kind.clone(),
                    Group {
                        total: usize::try_from(g.total).expect("a count"),
                        ids: g.hits.iter().map(|h| h.row.entity_id.clone()).collect(),
                    },
                )
            })
            .collect(),
    }
}

/// `found` is exactly `expected` (kind -> ids): the same kinds, each with the
/// expected count, and every row the page shows one of the expected. Where a
/// kind's whole group fits on the page that is set equality; where the cap cut
/// it, it is the count plus the rows shown -- which is as far as the launcher's
/// own page can be read.
pub fn exactly(raw: &str, found: &Found, expected: &BTreeMap<String, BTreeSet<String>>) {
    assert_eq!(
        found.kinds.keys().collect::<Vec<_>>(),
        expected.keys().collect::<Vec<_>>(),
        "{raw} finds exactly these kinds: {found:?}"
    );
    for (kind, want) in expected {
        let got = &found.kinds[kind];
        assert_eq!(
            got.total,
            want.len(),
            "{raw} matches {} {kind}(s), and the expected set has {}: shown {:?}, expected {want:?}",
            got.total,
            want.len(),
            got.ids
        );
        if got.ids.len() == got.total {
            assert_eq!(&got.ids, want, "{raw}: every {kind} it finds");
        } else {
            let foreign: Vec<&String> = got.ids.difference(want).collect();
            assert!(
                foreign.is_empty(),
                "{raw} shows {kind}(s) outside the expected set: {foreign:?}"
            );
        }
    }
}

/// `fixtures/tidewater/work.json`, the corpus every seed is derived from.
pub fn fixture() -> serde_json::Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/tidewater/work.json");
    let raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&raw).expect("work.json is JSON")
}

/// `testenv/seed-state.json`: the ids the real servers assigned to the
/// fixture's records, which no seed can dictate.
pub fn seed_state() -> serde_json::Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testenv/seed-state.json");
    let raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{}: {e} -- the live recipe's seed writes it", path.display()));
    serde_json::from_str(&raw).expect("seed-state.json is JSON")
}

/// One array of [`fixture`] (or of [`seed_state`]), by name.
pub fn rows(fixture: &serde_json::Value, of: &str) -> Vec<serde_json::Value> {
    fixture[of]
        .as_array()
        .unwrap_or_else(|| panic!("no `{of}` array"))
        .clone()
}
