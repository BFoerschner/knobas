//! One sync run.
//!
//! A **full sync** (`cursor: None`) lists the build configurations in scope
//! and, for each, the newest `builds_per_config` finished builds. An
//! **incremental** run skips the listing and asks once, globally, for
//! `state:finished,sinceBuild:(id:N)` -- build ids are server-wide, so one
//! query covers every configuration -- then narrows to the scope client-side.
//! **Both** then poll `state:(queued:true,running:true)` unconditionally, in
//! one query, because a running build mutates in place and never gets a new id
//! for a watermark to find it by.
//!
//! Configurations are emitted for every configuration in scope on a full sync,
//! and on an incremental run only for the configurations whose builds moved --
//! from the same `/buildTypes` listing either way, so a configuration's
//! payload never alternates between a rich and a lean shape. If nothing was
//! emitted, the cursor handed in comes back byte-identical (contract battery
//! clause 2).

use std::collections::{BTreeMap, BTreeSet};

use knobas_source::{Cursor, Sink, SourceError, SyncItem};

use crate::client::{Rec, Rest};
use crate::config::TeamCityConfig;
use crate::rest::{Build, BuildType, Locator, StateFilter};
use crate::{cursor, map};

/// How many builds one query asks for. Interfaces §4.1 pins TeamCity's page
/// size at 100.
const PAGE: u32 = 100;

/// The most finished builds one incremental run will carry.
///
/// `/app/rest/builds` has no offset dimension this adapter uses, so the only
/// way past a full page is to ask again with a bigger `count`; see [`since`].
/// Beyond this the run **fails** rather than returning what it has: an
/// `Ok(partial)` here would move the watermark past builds nobody ever
/// emitted, and they would be unreachable for good.
const MAX_FINISHED_PER_RUN: u32 = 1_000;

pub(crate) async fn execute(
    source_id: &str,
    cfg: &TeamCityConfig,
    rest: &dyn Rest,
    cursor_in: Option<Cursor>,
    sink: &mut (dyn Sink + Send),
) -> Result<Cursor, SourceError> {
    let previous = cursor_in.as_deref().and_then(cursor::parse);
    let mut configs: BTreeMap<String, Rec<BuildType>> = BTreeMap::new();

    // 1. The finished builds this run is responsible for.
    let finished = match previous {
        None => {
            // Full sync: the whole scope, newest `builds_per_config` each. The
            // listing doubles as the source of the `build_config` items.
            //
            // This is a *window*, not the corpus, which is why the descriptor
            // declares `full_sync_exhaustive: false`: the engine must not
            // tombstone the builds this deliberately did not re-list.
            for t in scope(rest, cfg).await? {
                configs.insert(t.rec.id.clone(), t);
            }
            let ids: Vec<String> = configs.keys().cloned().collect();
            let mut out = Vec::new();
            for id in ids {
                out.extend(
                    rest.builds(&Locator {
                        build_type_id: Some(id),
                        state: Some(StateFilter::Finished),
                        count: cfg.builds_per_config,
                        ..Locator::default()
                    })
                    .await?,
                );
            }
            out
        }
        // Build ids are server-wide and monotonic, so one query covers every
        // configuration; the scope is applied client-side below.
        Some(state) => since(rest, state.since_build_id).await?,
    };

    // 2. Queued and running builds, unconditionally: a build mutates in place
    //    while it runs and never gets a new id, so no watermark can find it.
    //    One query, not two -- `state` is a single locator dimension and
    //    `state:(queued:true,running:true)` is its combined spelling.
    //    Repeating the dimension is rejected, and knobas-mockd records the
    //    wrong spelling as a violation.
    let in_flight = rest
        .builds(&Locator {
            state: Some(StateFilter::InFlight),
            count: PAGE,
            ..Locator::default()
        })
        .await?;

    // 3. Scope, watermarks, items.
    // Paired with the build id so the run can order them numerically: the
    // key's *string* order puts `build:1188` before `build:412`.
    let mut builds: Vec<(i64, SyncItem)> = Vec::new();
    let mut touched: BTreeSet<String> = BTreeSet::new();
    let mut max_finished: Option<i64> = None;
    let mut min_unfinished: Option<i64> = None;
    for b in finished.iter().chain(in_flight.iter()) {
        let in_scope = in_scope_build(cfg, &b.rec);
        if StateFilter::Finished.matches(b.rec.state.as_deref()) {
            // Counted whether or not it is in scope. A finished build outside
            // the scope will never be emitted, so it never needs to be
            // reachable again -- and *not* counting it would pin a scoped
            // source's watermark below every foreign build that finishes,
            // growing the incremental query without bound.
            max_finished = Some(max_finished.map_or(b.rec.id, |m: i64| m.max(b.rec.id)));
        } else if in_scope {
            // Queued, running, or -- if the server sent no state at all --
            // unclassifiable. Unknown counts as in flight: clamping the
            // watermark costs a re-fetch, advancing past a build that turns
            // out to be running loses it for good.
            //
            // Only in-scope builds clamp, for the mirror image of the reason
            // above: holding the watermark back for a build that will never be
            // emitted buys nothing and costs every later run.
            min_unfinished = Some(min_unfinished.map_or(b.rec.id, |m: i64| m.min(b.rec.id)));
        }
        if !in_scope {
            continue;
        }
        if let Some(id) = type_id_of(&b.rec) {
            touched.insert(id.to_owned());
        }
        builds.push((b.rec.id, map::build_item(source_id, &b.raw, &b.rec)));
    }
    // Deterministic and oldest-first: the two queries above arrive in whatever
    // order the server chose, and a sink that writes an activity line per item
    // should see a build's history in the order it happened.
    builds.sort_by_key(|(id, _)| *id);

    // 4. The configurations of the builds that moved. Fetched from the same
    //    listing a full sync uses, so a configuration's payload never
    //    alternates between a rich and a lean shape -- and only when something
    //    moved, so an idle poll stays silent.
    if previous.is_some() && !touched.is_empty() {
        for t in scope(rest, cfg).await? {
            configs.insert(t.rec.id.clone(), t);
        }
    }
    let mut pushed = 0usize;
    for (id, t) in &configs {
        if previous.is_none() || touched.contains(id) {
            sink.item(map::build_config_item(source_id, &t.raw, &t.rec))
                .await?;
            pushed += 1;
        }
    }
    for (_, item) in builds {
        // Not the adapter's failure to swallow: a sink that rejected an item
        // wants the run abandoned (interfaces §4.1, battery clause 6).
        sink.item(item).await?;
        pushed += 1;
    }

    if pushed == 0 {
        // Battery clause 2: a run that emitted nothing hands back the cursor
        // it was given, byte for byte. The engine reads "same cursor, no
        // items" as "nothing happened" and writes no activity line.
        return Ok(cursor_in.unwrap_or_else(|| cursor::render(cursor::new(0))));
    }
    let before = previous.map_or(0, |p| p.since_build_id);
    Ok(cursor::render(cursor::new(cursor::advance(
        before,
        max_finished,
        min_unfinished,
    ))))
}

/// Every finished build newer than `since_build_id`, widening the window until
/// the server stops filling it.
///
/// A *full* page means the server had more to give and the page boundary hid
/// them, so asking again with a bigger `count` is the only way to see them.
/// Widening re-reads from the top rather than paging by offset, which is what
/// makes it safe under both orderings: real TeamCity answers newest-first and
/// `knobas-mockd` answers ascending, and an offset walked over a list that
/// grows at the front skips rows.
///
/// # Errors
///
/// [`SourceError::Protocol`] once more than [`MAX_FINISHED_PER_RUN`] builds
/// have finished since the watermark. **This must never be `Ok`.** Returning a
/// truncated page would advance the watermark past builds this run never
/// emitted, and `sinceBuild` would never offer them again -- a silent hole in
/// the mirror. A failed run, by contrast, leaves the cursor where it is and
/// the scheduler retries.
///
/// The final probe deliberately asks for one *more* than the ceiling, so
/// "exactly [`MAX_FINISHED_PER_RUN`] builds finished" is a success rather than
/// a spurious failure at the boundary.
async fn since(rest: &dyn Rest, since_build_id: i64) -> Result<Vec<Rec<Build>>, SourceError> {
    let ceiling = MAX_FINISHED_PER_RUN + 1;
    let mut count = PAGE;
    loop {
        let page = rest
            .builds(&Locator {
                state: Some(StateFilter::Finished),
                since_build_id: Some(since_build_id),
                count,
                ..Locator::default()
            })
            .await?;
        if page.len() < count as usize {
            return Ok(page);
        }
        if count >= ceiling {
            return Err(SourceError::Protocol(format!(
                "teamcity: more than {MAX_FINISHED_PER_RUN} builds have finished since build \
                 {since_build_id}; this run would have had to drop the oldest of them, which \
                 `sinceBuild` could never offer again. Sync more often, or narrow the source's \
                 build configurations."
            )));
        }
        count = count.saturating_mul(2).min(ceiling);
    }
}

async fn scope(rest: &dyn Rest, cfg: &TeamCityConfig) -> Result<Vec<Rec<BuildType>>, SourceError> {
    Ok(rest
        .build_types()
        .await?
        .into_iter()
        .filter(|t| in_scope_type(cfg, &t.rec))
        .collect())
}

fn type_id_of(b: &Build) -> Option<&str> {
    b.build_type_id
        .as_deref()
        .or_else(|| b.build_type.as_ref().map(|t| t.id.as_str()))
}

fn in_scope_type(cfg: &TeamCityConfig, t: &BuildType) -> bool {
    (cfg.build_type_ids.is_empty() || cfg.build_type_ids.iter().any(|id| id == &t.id))
        && (cfg.project_ids.is_empty()
            || t.project_id
                .as_deref()
                .is_some_and(|p| cfg.project_ids.iter().any(|x| x == p)))
}

/// The same scope, applied to a build.
///
/// The global in-flight poll and the global incremental query both return
/// builds from outside the scope -- project filtering has no locator dimension
/// in the contract, so it happens here, off the nested `buildType(projectId)`
/// the `fields=` selector asks for.
fn in_scope_build(cfg: &TeamCityConfig, b: &Build) -> bool {
    let type_id = type_id_of(b);
    let project = b.build_type.as_ref().and_then(|t| t.project_id.as_deref());
    (cfg.build_type_ids.is_empty()
        || type_id.is_some_and(|id| cfg.build_type_ids.iter().any(|x| x == id)))
        && (cfg.project_ids.is_empty()
            || project.is_some_and(|p| cfg.project_ids.iter().any(|x| x == p)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use knobas_source::contract::VecSink;
    use std::sync::Mutex;

    /// A [`Rest`] that answers locators the way TeamCity does: `sinceBuild` is
    /// exclusive, `count` truncates, `buildType` and `state` filter. Small
    /// enough to read, honest enough that the run's logic is actually
    /// exercised.
    struct FakeRest {
        build_types: Vec<serde_json::Value>,
        builds: Vec<serde_json::Value>,
        /// Newest first, as real TeamCity answers. `knobas-mockd` answers
        /// ascending (its deviation 12), which is why the run may not depend
        /// on either -- `tests/mockd.rs` is the other half of this pair.
        newest_first: bool,
        calls: Mutex<Vec<String>>,
    }

    impl FakeRest {
        fn new(build_types: Vec<serde_json::Value>, builds: Vec<serde_json::Value>) -> Self {
            Self {
                build_types,
                builds,
                newest_first: true,
                calls: Mutex::new(Vec::new()),
            }
        }

        fn ascending(mut self) -> Self {
            self.newest_first = false;
            self
        }

        fn calls(&self) -> Vec<String> {
            self.calls.lock().expect("not poisoned").clone()
        }
    }

    #[async_trait::async_trait]
    impl Rest for FakeRest {
        async fn server(&self) -> Result<crate::rest::Server, SourceError> {
            Ok(crate::rest::Server {
                version: Some("2025.07.2".into()),
                build_number: None,
            })
        }
        async fn current_user(&self) -> Result<crate::rest::CurrentUser, SourceError> {
            Ok(crate::rest::CurrentUser {
                username: Some("mara".into()),
                name: None,
            })
        }
        async fn build_types(&self) -> Result<Vec<Rec<BuildType>>, SourceError> {
            self.calls
                .lock()
                .expect("not poisoned")
                .push("buildTypes".to_owned());
            Ok(self
                .build_types
                .iter()
                .map(|raw| Rec {
                    raw: raw.clone(),
                    rec: serde_json::from_value(raw.clone()).expect("fixture parses"),
                })
                .collect())
        }
        async fn builds(&self, locator: &Locator) -> Result<Vec<Rec<Build>>, SourceError> {
            self.calls
                .lock()
                .expect("not poisoned")
                .push(locator.render());
            let mut out: Vec<Rec<Build>> = self
                .builds
                .iter()
                .map(|raw| Rec::<Build> {
                    raw: raw.clone(),
                    rec: serde_json::from_value(raw.clone()).expect("fixture parses"),
                })
                .filter(|r| {
                    locator
                        .state
                        .is_none_or(|s| s.matches(r.rec.state.as_deref()))
                })
                .filter(|r| {
                    locator
                        .build_type_id
                        .as_deref()
                        .is_none_or(|id| r.rec.build_type_id.as_deref() == Some(id))
                })
                .filter(|r| locator.since_build_id.is_none_or(|since| r.rec.id > since))
                .collect();
            if self.newest_first {
                out.sort_by_key(|r| std::cmp::Reverse(r.rec.id));
            } else {
                out.sort_by_key(|r| r.rec.id);
            }
            out.truncate(locator.count as usize);
            Ok(out)
        }
    }

    fn build_type(id: &str, project: &str) -> serde_json::Value {
        serde_json::json!({ "id": id, "name": id, "projectId": project, "projectName": project })
    }

    fn build(id: i64, type_id: &str, project: &str, state: &str) -> serde_json::Value {
        serde_json::json!({
            "id": id, "number": id.to_string(), "buildTypeId": type_id, "state": state,
            "status": "SUCCESS",
            "finishDate": if state == "finished" { Some("20260822T101018+0000") } else { None },
            "startDate": "20260822T100600+0000",
            "buildType": { "id": type_id, "name": type_id, "projectId": project, "projectName": project }
        })
    }

    fn tidewater() -> FakeRest {
        FakeRest::new(
            vec![
                build_type("Payout_Build", "Payout"),
                build_type("Payout_IntegrationTests", "Payout"),
                build_type("Ledger_Deploy_Staging", "Ledger"),
            ],
            vec![
                build(1188, "Payout_Build", "Payout", "running"),
                build(1187, "Payout_IntegrationTests", "Payout", "finished"),
                build(412, "Ledger_Deploy_Staging", "Ledger", "finished"),
            ],
        )
    }

    async fn run(
        rest: &FakeRest,
        cfg: &TeamCityConfig,
        cursor: Option<String>,
    ) -> (Vec<SyncItem>, String) {
        let mut sink = VecSink(Vec::new());
        let next = execute("teamcity", cfg, rest, cursor, &mut sink)
            .await
            .expect("run");
        (sink.0, next)
    }

    fn keys(items: &[SyncItem]) -> Vec<String> {
        items.iter().map(|i| i.entity.key.clone()).collect()
    }

    #[tokio::test]
    async fn a_full_sync_emits_every_configuration_and_its_builds() {
        let rest = tidewater();
        let (items, cursor) = run(&rest, &TeamCityConfig::default(), None).await;
        let keys = keys(&items);
        assert!(keys.contains(&"buildType:Payout_Build".to_owned()));
        assert!(keys.contains(&"buildType:Ledger_Deploy_Staging".to_owned()));
        assert!(
            keys.contains(&"build:1188".to_owned()),
            "the running build is part of the picture"
        );
        assert!(keys.contains(&"build:1187".to_owned()));
        assert!(keys.contains(&"build:412".to_owned()));
        // 1188 is still running: the watermark must stay below it, or 1188
        // would be unreachable by `sinceBuild` once it finishes.
        assert_eq!(cursor, r#"{"v":1,"since_build_id":1187}"#);
        // Per-configuration full-sync queries, then ONE in-flight poll -- no
        // repeated `state` dimension, and nothing outside the locator grammar.
        assert_eq!(
            rest.calls(),
            [
                "buildTypes",
                "buildType:(id:Ledger_Deploy_Staging),state:finished,count:100",
                "buildType:(id:Payout_Build),state:finished,count:100",
                "buildType:(id:Payout_IntegrationTests),state:finished,count:100",
                "state:(queued:true,running:true),count:100",
            ]
        );
    }

    /// Contract battery clause 2, at the level where it is decided.
    #[tokio::test]
    async fn an_idle_incremental_run_emits_nothing_and_returns_the_same_cursor() {
        let rest = FakeRest::new(
            vec![build_type("Ledger_Deploy_Staging", "Ledger")],
            vec![build(412, "Ledger_Deploy_Staging", "Ledger", "finished")],
        );
        let cfg = TeamCityConfig::default();
        let (_, first) = run(&rest, &cfg, None).await;
        assert_eq!(first, r#"{"v":1,"since_build_id":412}"#);
        let (items, second) = run(&rest, &cfg, Some(first.clone())).await;
        assert!(items.is_empty());
        assert_eq!(second, first, "byte-identical, not merely equivalent");
        // Two requests and no `/buildTypes`: an idle poll costs the finished
        // query plus the one in-flight query. (The full sync above made three:
        // the listing, one per-configuration query, one in-flight poll.)
        assert_eq!(
            rest.calls()[3..],
            [
                "state:finished,sinceBuild:(id:412),count:100",
                "state:(queued:true,running:true),count:100"
            ]
        );
    }

    /// The exit criterion: a running build is re-polled every run, and that
    /// alone never moves the cursor.
    #[tokio::test]
    async fn a_running_build_is_re_emitted_without_moving_the_cursor() {
        let rest = tidewater();
        let cfg = TeamCityConfig::default();
        let (_, first) = run(&rest, &cfg, None).await;
        let (items, second) = run(&rest, &cfg, Some(first.clone())).await;
        assert_eq!(
            keys(&items),
            ["buildType:Payout_Build", "build:1188"],
            "the running build, and the configuration it belongs to"
        );
        assert_eq!(second, first);
    }

    /// ...and the other half: the cursor moves exactly when a finished build
    /// was emitted.
    #[tokio::test]
    async fn a_newly_finished_build_is_emitted_and_advances_the_watermark() {
        let rest = FakeRest::new(
            vec![build_type("Ledger_Deploy_Staging", "Ledger")],
            vec![build(412, "Ledger_Deploy_Staging", "Ledger", "finished")],
        );
        let cfg = TeamCityConfig::default();
        let (_, first) = run(&rest, &cfg, None).await;

        let rest = FakeRest::new(
            vec![build_type("Ledger_Deploy_Staging", "Ledger")],
            vec![
                build(412, "Ledger_Deploy_Staging", "Ledger", "finished"),
                build(500, "Ledger_Deploy_Staging", "Ledger", "finished"),
            ],
        );
        let (items, second) = run(&rest, &cfg, Some(first)).await;
        assert_eq!(
            keys(&items),
            ["buildType:Ledger_Deploy_Staging", "build:500"],
            "exactly the new build, and its configuration"
        );
        assert_eq!(second, r#"{"v":1,"since_build_id":500}"#);
    }

    /// The reason the watermark is clamped: ids are handed out when a build is
    /// queued, so a build that finishes late has an id below builds that
    /// finished before it.
    #[tokio::test]
    async fn a_build_that_finishes_late_is_still_picked_up() {
        let rest = FakeRest::new(
            vec![build_type("Payout_Build", "Payout")],
            vec![
                build(1150, "Payout_Build", "Payout", "running"),
                build(1200, "Payout_Build", "Payout", "finished"),
            ],
        );
        let cfg = TeamCityConfig::default();
        let (_, first) = run(&rest, &cfg, None).await;
        assert_eq!(first, r#"{"v":1,"since_build_id":1149}"#);

        let rest = FakeRest::new(
            vec![build_type("Payout_Build", "Payout")],
            vec![
                build(1150, "Payout_Build", "Payout", "finished"),
                build(1200, "Payout_Build", "Payout", "finished"),
            ],
        );
        let (items, second) = run(&rest, &cfg, Some(first)).await;
        assert!(
            keys(&items).contains(&"build:1150".to_owned()),
            "the late finisher"
        );
        assert_eq!(second, r#"{"v":1,"since_build_id":1200}"#);
    }

    #[tokio::test]
    async fn an_unreadable_cursor_falls_back_to_a_full_sync() {
        let rest = tidewater();
        let (items, _) = run(
            &rest,
            &TeamCityConfig::default(),
            Some("tidewater-v1".to_owned()),
        )
        .await;
        assert!(
            items
                .iter()
                .any(|i| i.entity.key == "buildType:Ledger_Deploy_Staging")
        );
        assert!(rest.calls().contains(&"buildTypes".to_owned()));
    }

    #[tokio::test]
    async fn the_scope_narrows_by_configuration_and_by_project() {
        let by_type = TeamCityConfig::from_json(&serde_json::json!({
            "build_type_ids": ["Ledger_Deploy_Staging"]
        }))
        .expect("config");
        let (items, _) = run(&tidewater(), &by_type, None).await;
        assert_eq!(
            keys(&items),
            ["buildType:Ledger_Deploy_Staging", "build:412"]
        );

        let by_project =
            TeamCityConfig::from_json(&serde_json::json!({ "project_ids": ["Payout"] }))
                .expect("config");
        let (items, _) = run(&tidewater(), &by_project, None).await;
        assert!(
            items.iter().all(|i| !i.entity.key.contains("Ledger")),
            "Ledger is out of scope"
        );
        assert!(
            items.iter().any(|i| i.entity.key == "build:1188"),
            "the in-flight poll is global, so its result is filtered client-side"
        );
    }

    /// A scoped source must not be held back by a build it will never emit.
    ///
    /// The watermark counts **every** finished build the run saw, in scope or
    /// not: a foreign build is knowingly skipped, so re-offering it forever
    /// only grows the incremental query. The clamp is the mirror image -- it
    /// counts only in-scope in-flight builds, because a foreign running build
    /// needs no protection.
    #[tokio::test]
    async fn foreign_builds_advance_the_watermark_but_do_not_clamp_it() {
        let rest = FakeRest::new(
            vec![
                build_type("Payout_Build", "Payout"),
                build_type("Ledger_Deploy_Staging", "Ledger"),
            ],
            vec![
                // Out of scope and still running, at the lowest id: clamping
                // for it would pin the watermark at 99 forever.
                build(100, "Ledger_Deploy_Staging", "Ledger", "running"),
                build(200, "Payout_Build", "Payout", "finished"),
            ],
        );
        let cfg = TeamCityConfig::from_json(&serde_json::json!({ "project_ids": ["Payout"] }))
            .expect("config");
        let (items, first) = run(&rest, &cfg, None).await;
        assert_eq!(keys(&items), ["buildType:Payout_Build", "build:200"]);
        assert_eq!(
            first, r#"{"v":1,"since_build_id":200}"#,
            "a foreign running build must not clamp the watermark"
        );

        // ...and a foreign build that *finishes* still moves it along, so the
        // incremental query does not accumulate builds it always discards.
        // The **newest** finished build is the foreign one, deliberately: if
        // an in-scope build were newest, a watermark that ignored foreign
        // builds would land on the same number and this could not fail.
        let rest = FakeRest::new(
            vec![
                build_type("Payout_Build", "Payout"),
                build_type("Ledger_Deploy_Staging", "Ledger"),
            ],
            vec![
                build(100, "Ledger_Deploy_Staging", "Ledger", "finished"),
                build(200, "Payout_Build", "Payout", "finished"),
                build(300, "Payout_Build", "Payout", "finished"),
                build(400, "Ledger_Deploy_Staging", "Ledger", "finished"),
            ],
        );
        let (items, second) = run(&rest, &cfg, Some(first)).await;
        assert_eq!(keys(&items), ["buildType:Payout_Build", "build:300"]);
        assert_eq!(
            second, r#"{"v":1,"since_build_id":400}"#,
            "400 is foreign and finished: the run saw it and will never want it again, so              leaving the watermark at 300 would re-fetch it on every poll for good"
        );
    }

    /// An in-scope build still running does clamp -- the case the rule above
    /// must not have broken.
    #[tokio::test]
    async fn an_in_scope_running_build_still_clamps_the_watermark() {
        let rest = FakeRest::new(
            vec![build_type("Payout_Build", "Payout")],
            vec![
                build(100, "Payout_Build", "Payout", "running"),
                build(200, "Payout_Build", "Payout", "finished"),
            ],
        );
        let cfg = TeamCityConfig::from_json(&serde_json::json!({ "project_ids": ["Payout"] }))
            .expect("config");
        let (_, cursor) = run(&rest, &cfg, None).await;
        assert_eq!(cursor, r#"{"v":1,"since_build_id":99}"#);
    }

    /// A configuration two builds share is emitted once, not twice.
    #[tokio::test]
    async fn a_configuration_is_emitted_once_per_run() {
        let rest = FakeRest::new(
            vec![build_type("Payout_Build", "Payout")],
            vec![
                build(1, "Payout_Build", "Payout", "finished"),
                build(2, "Payout_Build", "Payout", "finished"),
            ],
        );
        let (items, _) = run(&rest, &TeamCityConfig::default(), None).await;
        assert_eq!(items.iter().filter(|i| i.kind == "build_config").count(), 1);
    }

    /// A sink that rejects one kind of item and not the other.
    ///
    /// Configurations are pushed before builds, so a sink that fails on
    /// *everything* only ever exercises the first `?` in the run -- and the
    /// build loop's `?` could be deleted with the suite still green. This one
    /// can be pointed at either loop.
    struct RejectsKind(&'static str);

    #[async_trait::async_trait]
    impl knobas_source::Sink for RejectsKind {
        async fn item(&mut self, item: SyncItem) -> Result<(), SourceError> {
            if item.kind == self.0 {
                return Err(SourceError::Sink(format!("no room for a {}", self.0)));
            }
            Ok(())
        }
    }

    /// Interfaces §4.1 and battery clause 6: a sink failure abandons the run
    /// -- from **either** loop, which is two `?`s and therefore two tests in
    /// one.
    #[tokio::test]
    async fn a_sink_failure_aborts_the_run() {
        for kind in ["build_config", "build"] {
            let outcome = execute(
                "teamcity",
                &TeamCityConfig::default(),
                &tidewater(),
                None,
                &mut RejectsKind(kind),
            )
            .await;
            assert!(
                matches!(outcome, Err(SourceError::Sink(_))),
                "a rejected {kind} must abandon the run as SourceError::Sink, got {outcome:?}"
            );
        }
    }

    /// The page cap **fails**; it must never return `Ok` with a short answer.
    ///
    /// An `Ok(partial)` would advance the watermark past the builds the run
    /// dropped, and `sinceBuild` could never offer them again.
    #[tokio::test]
    async fn overflowing_one_run_is_an_error_not_a_silent_truncation() {
        let many: Vec<serde_json::Value> = (1..=1_500)
            .map(|n| build(n, "Payout_Build", "Payout", "finished"))
            .collect();
        let rest = FakeRest::new(vec![build_type("Payout_Build", "Payout")], many);
        let mut sink = VecSink(Vec::new());
        let err = execute(
            "teamcity",
            &TeamCityConfig::default(),
            &rest,
            Some(r#"{"v":1,"since_build_id":0}"#.to_owned()),
            &mut sink,
        )
        .await
        .expect_err("a run that cannot carry them all must fail");
        assert!(
            matches!(&err, SourceError::Protocol(m) if m.contains("1000")),
            "{err:?}"
        );
        assert!(
            sink.0.is_empty(),
            "nothing is emitted from a run that cannot complete"
        );
        // It widened rather than giving up on the first full page.
        let widened: Vec<String> = rest
            .calls()
            .into_iter()
            .filter(|c| c.contains("sinceBuild"))
            .collect();
        assert_eq!(
            widened,
            [
                "state:finished,sinceBuild:(id:0),count:100",
                "state:finished,sinceBuild:(id:0),count:200",
                "state:finished,sinceBuild:(id:0),count:400",
                "state:finished,sinceBuild:(id:0),count:800",
                "state:finished,sinceBuild:(id:0),count:1001",
            ]
        );
    }

    /// Exactly at the ceiling is a success: the last probe asks for one more
    /// than the ceiling, so a full page at 1 000 is distinguishable from a
    /// truncated one.
    #[tokio::test]
    async fn exactly_the_ceiling_is_not_an_overflow() {
        let many: Vec<serde_json::Value> = (1..=1_000)
            .map(|n| build(n, "Payout_Build", "Payout", "finished"))
            .collect();
        let rest = FakeRest::new(vec![build_type("Payout_Build", "Payout")], many);
        let (items, cursor) = run(
            &rest,
            &TeamCityConfig::default(),
            Some(r#"{"v":1,"since_build_id":0}"#.to_owned()),
        )
        .await;
        assert_eq!(items.len(), 1_001, "1000 builds and their configuration");
        assert_eq!(cursor, r#"{"v":1,"since_build_id":1000}"#);
    }

    /// The widening walks past a full page rather than stopping at it, which
    /// is what a fixture of exactly one page cannot witness.
    #[tokio::test]
    async fn a_full_page_is_widened_until_the_server_stops_filling_it() {
        let many: Vec<serde_json::Value> = (1..=250)
            .map(|n| build(n, "Payout_Build", "Payout", "finished"))
            .collect();
        let rest = FakeRest::new(vec![build_type("Payout_Build", "Payout")], many);
        let (items, cursor) = run(
            &rest,
            &TeamCityConfig::default(),
            Some(r#"{"v":1,"since_build_id":0}"#.to_owned()),
        )
        .await;
        assert_eq!(
            items.iter().filter(|i| i.kind == "build").count(),
            250,
            "every build, not the first page of them"
        );
        assert_eq!(cursor, r#"{"v":1,"since_build_id":250}"#);
    }

    /// The run must not depend on the server's ordering: real TeamCity answers
    /// newest-first, `knobas-mockd` answers ascending (its deviation 12).
    #[tokio::test]
    async fn the_run_is_the_same_whichever_order_the_server_answers_in() {
        let cfg = TeamCityConfig::default();
        let (newest_first, cursor_a) = run(&tidewater(), &cfg, None).await;
        let (ascending, cursor_b) = run(&tidewater().ascending(), &cfg, None).await;
        assert_eq!(keys(&newest_first), keys(&ascending));
        assert_eq!(cursor_a, cursor_b);
    }
}
