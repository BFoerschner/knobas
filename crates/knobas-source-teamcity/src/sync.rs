//! One sync run.
//!
//! Every run **opens** with one unconditional `state:(queued:true,running:true)`
//! poll, because a running build mutates in place and never gets a new id for a
//! watermark to find it by. It opens with it rather than closing with it so
//! that a build which is running when the run starts and finished when it ends
//! lands in *both* sets rather than in neither; see [`execute`] step 1.
//!
//! Then a **full sync** (`cursor: None`) lists the build configurations in
//! scope and, for each, the newest `builds_per_config` finished builds. An
//! **incremental** run skips the listing and asks once, globally, for
//! `state:finished,sinceBuild:(id:N)` -- build ids are server-wide, so one
//! query covers every configuration -- then narrows to the scope client-side.
//!
//! Neither query is allowed to come back truncated. Both widen `count:` until
//! the page is short and **fail** at a ceiling rather than compute a watermark
//! from a set the run knows is incomplete; see [`all_of`].
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

/// The most builds one `/app/rest/builds` query will carry.
///
/// `/app/rest/builds` has no offset dimension this adapter uses, so the only
/// way past a full page is to ask again with a bigger `count`; see [`all_of`].
/// Beyond this the run **fails** rather than returning what it has: an
/// `Ok(partial)` here would move the watermark past builds nobody ever
/// emitted, and they would be unreachable for good.
///
/// It governs **both** queries. The finished side is the obvious one, but the
/// in-flight poll is the one with teeth: it is the only input to the watermark
/// clamp, so a page silently truncated there is the same permanent hole with
/// none of the visibility. See [`in_flight`].
const MAX_BUILDS_PER_QUERY: u32 = 1_000;

pub(crate) async fn execute(
    source_id: &str,
    cfg: &TeamCityConfig,
    rest: &dyn Rest,
    cursor_in: Option<Cursor>,
    sink: &mut (dyn Sink + Send),
) -> Result<Cursor, SourceError> {
    let previous = cursor_in.as_deref().and_then(cursor::parse);
    let mut configs: BTreeMap<String, Rec<BuildType>> = BTreeMap::new();

    // 1. Queued and running builds, unconditionally and **first**.
    //
    //    Unconditionally, because a build mutates in place while it runs and
    //    never gets a new id, so no watermark can find it.
    //
    //    First, because step 2 is not instantaneous. A full sync issues one
    //    request per configuration, and at the default 5 req/s two hundred
    //    configurations put ~40 s between the two queries. A build that is
    //    running when this poll executes and has finished by the time the
    //    finished query runs is, in this order, in **both** sets: emitted from
    //    the finished one, and the watermark may pass it because it was
    //    emitted. In the other order it would be in **neither** -- excluded by
    //    `state:finished` at the start and by `state:(queued:true,running:true)`
    //    at the end -- with nothing holding the watermark below it and nothing
    //    emitting it. `sinceBuild` would then never offer it again.
    //
    //    **What this order does not close.** A build *queued* after this poll
    //    can still be missed: it is not in this set (it did not exist yet), and
    //    if it is still running when its configuration is queried it is not in
    //    the finished set either. It is lost only if some *later*-queued build
    //    finished early enough to land in the finished set, since that is what
    //    pushes `max_finished` above it. The complete fix is a ceiling -- the
    //    highest build id in existence at this instant, which the watermark may
    //    never pass -- and that needs a portable "newest build" query: `count:1`
    //    returns the newest against real TeamCity and the *oldest* against
    //    `knobas-mockd` (its deviation 12), so the contract has no one spelling
    //    for it. Recorded rather than silently left; the window is one run's own
    //    finished phase, and it is strictly narrower than the one this ordering
    //    removes.
    let in_flight = in_flight(rest).await?;

    // 2. The finished builds this run is responsible for.
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

    // 3. Scope, watermarks, items.
    //
    //    Keyed by build id, because the two queries above can legitimately
    //    return the *same* build -- one that was running when (1) polled and
    //    had finished by (2). The finished observation is inserted last so it
    //    wins: it is the fresher state, and a build known to have finished
    //    must advance the watermark rather than clamp it. Without the map that
    //    build would be emitted twice in one run.
    //
    //    The map also fixes the emission order, which the queries do not:
    //    `BTreeMap<i64, _>` iterates ascending by build id, so the run emits
    //    oldest-first whether the server answered newest-first (both real
    //    TeamCity and `knobas-mockd` do) or ascending. Keying on the *id*
    //    rather than on the entity key is what makes that numeric: the key's
    //    string order puts `build:1188` before `build:412`.
    let mut observed: BTreeMap<i64, &Rec<Build>> = BTreeMap::new();
    for b in in_flight.iter().chain(finished.iter()) {
        observed.insert(b.rec.id, b);
    }

    let mut builds: Vec<SyncItem> = Vec::new();
    let mut touched: BTreeSet<String> = BTreeSet::new();
    let mut max_finished: Option<i64> = None;
    let mut min_unfinished: Option<i64> = None;
    for b in observed.values() {
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
        builds.push(map::build_item(source_id, &b.raw, &b.rec));
    }

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
    for item in builds {
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

/// Every build matching `locator`, widening `count:` until the server stops
/// filling the page. The locator's own `count` is ignored -- this owns that
/// dimension.
///
/// A *full* page means the server had more to give and the page boundary hid
/// them, so asking again with a bigger `count` is the only way to see them.
/// Widening re-reads from the top rather than paging by offset, which is what
/// makes it safe under both orderings: real TeamCity answers newest-first and
/// `knobas-mockd` answers ascending, and an offset walked over a list that
/// grows at the front skips rows. `count` is an independent locator dimension,
/// so widening adds no grammar the mock contract does not already allow.
///
/// # Errors
///
/// [`SourceError::Protocol`], with the caller's message, once the page is
/// still full at [`MAX_BUILDS_PER_QUERY`]. **This must never be `Ok`.** A
/// truncated page silently returned is how a build becomes permanently
/// unreachable: on the finished side the watermark advances past builds this
/// run never emitted, and on the in-flight side it advances past builds that
/// have not finished yet. Either way `sinceBuild` never offers them again, and
/// a full sync is a window rather than the corpus (see
/// [`descriptor_template`](crate::descriptor_template)), so not even a cursor
/// reset recovers them. A failed run, by contrast, leaves the cursor where it
/// is and the scheduler retries.
///
/// The final probe deliberately asks for one *more* than the ceiling, so
/// "exactly [`MAX_BUILDS_PER_QUERY`] builds" is a success rather than a
/// spurious failure at the boundary.
async fn all_of(
    rest: &dyn Rest,
    locator: &Locator,
    overflowed: impl FnOnce() -> String + Send,
) -> Result<Vec<Rec<Build>>, SourceError> {
    let ceiling = MAX_BUILDS_PER_QUERY + 1;
    let mut count = PAGE;
    loop {
        let page = rest
            .builds(&Locator {
                count,
                ..locator.clone()
            })
            .await?;
        if page.len() < count as usize {
            return Ok(page);
        }
        if count >= ceiling {
            return Err(SourceError::Protocol(overflowed()));
        }
        count = count.saturating_mul(2).min(ceiling);
    }
}

/// Every finished build newer than `since_build_id`.
async fn since(rest: &dyn Rest, since_build_id: i64) -> Result<Vec<Rec<Build>>, SourceError> {
    all_of(
        rest,
        &Locator {
            state: Some(StateFilter::Finished),
            since_build_id: Some(since_build_id),
            ..Locator::default()
        },
        || {
            format!(
                "teamcity: more than {MAX_BUILDS_PER_QUERY} builds have finished since build \
                 {since_build_id}; this run would have had to drop the oldest of them, which \
                 `sinceBuild` could never offer again. Sync more often, or narrow the source's \
                 build configurations."
            )
        },
    )
    .await
}

/// Every queued and running build on the server.
///
/// One query, not two -- `state` is a single locator dimension and
/// `state:(queued:true,running:true)` is its combined spelling. Repeating the
/// dimension is rejected, and `knobas-mockd` records the wrong spelling as a
/// violation.
///
/// This poll is **global**: `/app/rest/builds` has no project dimension in the
/// contract, so scoping happens client-side. That makes it the whole server's
/// in-flight population and the *only* input to the watermark clamp, which is
/// why a truncated page here is not a cosmetic loss. Real TeamCity answers
/// newest-first, so a full page drops the **oldest** in-flight builds --
/// exactly the ones the clamp exists to protect. `min_unfinished` would come
/// back `None`, or too high, the watermark would advance past them, and
/// `sinceBuild` would never offer them again once they finished. A mass
/// trigger on a monorepo puts more than [`PAGE`] builds in the queue routinely.
///
/// So the same rule as [`since`], for the same reason: widen, and **fail** at
/// the ceiling rather than compute a watermark from a set the run knows is
/// incomplete.
async fn in_flight(rest: &dyn Rest) -> Result<Vec<Rec<Build>>, SourceError> {
    all_of(
        rest,
        &Locator {
            state: Some(StateFilter::InFlight),
            ..Locator::default()
        },
        || {
            format!(
                "teamcity: more than {MAX_BUILDS_PER_QUERY} builds are queued or running. This \
                 run cannot see the whole in-flight population, and a watermark computed from \
                 part of it would advance past builds that are still running -- `sinceBuild` \
                 could never offer them again once they finished. Retry once the queue drains."
            )
        },
    )
    .await
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
        /// Newest first, as both real TeamCity and `knobas-mockd` answer.
        /// The ascending variant is kept because the run may not depend on
        /// either order: it is the only thing left that can witness a
        /// dependency on one, now that both servers agree.
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
        // ONE in-flight poll -- no repeated `state` dimension, and nothing
        // outside the locator grammar -- and it comes **first**, before the
        // per-configuration queries that can take ~40 s to walk. See
        // `a_build_that_finishes_mid_run_is_not_lost_between_the_two_queries`
        // for what the order buys; this pins the requests themselves.
        assert_eq!(
            rest.calls(),
            [
                "state:(queued:true,running:true),count:100",
                "buildTypes",
                "buildType:(id:Ledger_Deploy_Staging),state:finished,count:100",
                "buildType:(id:Payout_Build),state:finished,count:100",
                "buildType:(id:Payout_IntegrationTests),state:finished,count:100",
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
        // Two requests and no `/buildTypes`: an idle poll costs the in-flight
        // query plus the finished one, in that order. (The full sync above made
        // three: the in-flight poll, the listing, one per-configuration query.)
        assert_eq!(
            rest.calls()[3..],
            [
                "state:(queued:true,running:true),count:100",
                "state:finished,sinceBuild:(id:412),count:100",
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

    /// The in-flight poll is the only input to the watermark clamp, so a page
    /// it silently truncated is a permanently unreachable build.
    ///
    /// The server here answers newest-first, as real TeamCity does, so a page
    /// capped at [`PAGE`] drops the **oldest** in-flight builds -- exactly the
    /// ones the clamp protects. Without widening, `min_unfinished` would be 51
    /// and the watermark would settle at 50, above 50 builds that have not
    /// finished; `sinceBuild:(id:50)` would never offer them again. With it,
    /// the run sees all 150 and clamps to 0.
    ///
    /// The fixture needs more than one page of in-flight builds to witness
    /// this at all: with 100 or fewer, a truncating poll and a complete one
    /// return the same set and the assertion cannot fail.
    #[tokio::test]
    async fn the_in_flight_poll_widens_past_a_full_page_before_clamping() {
        let mut builds: Vec<serde_json::Value> = (1..=150)
            .map(|n| build(n, "Payout_Build", "Payout", "running"))
            .collect();
        builds.push(build(2_000, "Payout_Build", "Payout", "finished"));
        let rest = FakeRest::new(vec![build_type("Payout_Build", "Payout")], builds);

        let (_, cursor) = run(&rest, &TeamCityConfig::default(), None).await;
        assert_eq!(
            cursor, r#"{"v":1,"since_build_id":0}"#,
            "build 1 is still running, so the watermark must sit below it -- a poll truncated at \
             100 would see only ids 51..150 and settle at 50"
        );
        let polls: Vec<String> = rest
            .calls()
            .into_iter()
            .filter(|c| c.starts_with("state:(queued"))
            .collect();
        assert_eq!(
            polls,
            [
                "state:(queued:true,running:true),count:100",
                "state:(queued:true,running:true),count:200",
            ],
            "it widened rather than accepting the first full page"
        );
    }

    /// ...and past the ceiling it fails, exactly as the finished side does. A
    /// run that cannot see the whole in-flight population cannot compute a safe
    /// watermark, and `Ok` with a partial one is the silent hole.
    #[tokio::test]
    async fn an_unreadably_large_queue_is_an_error_not_a_truncated_clamp() {
        let many: Vec<serde_json::Value> = (1..=1_500)
            .map(|n| build(n, "Payout_Build", "Payout", "queued"))
            .collect();
        let rest = FakeRest::new(vec![build_type("Payout_Build", "Payout")], many);
        let mut sink = VecSink(Vec::new());
        let err = execute(
            "teamcity",
            &TeamCityConfig::default(),
            &rest,
            None,
            &mut sink,
        )
        .await
        .expect_err("a run that cannot see the whole queue must fail");
        assert!(
            matches!(&err, SourceError::Protocol(m) if m.contains("queued or running")),
            "{err:?}"
        );
        assert!(
            sink.0.is_empty(),
            "nothing is emitted from a run that cannot complete"
        );
    }

    /// A [`Rest`] whose build 500 is running on the first query and finished on
    /// every later one -- the race in the middle of a run, made deterministic.
    ///
    /// The window is not theoretical: a full sync issues one request per
    /// configuration before it would reach a trailing in-flight poll, and at
    /// the default 5 req/s two hundred configurations is ~40 s.
    struct FinishesMidRun {
        inner: FakeRest,
        seen: Mutex<usize>,
    }

    #[async_trait::async_trait]
    impl Rest for FinishesMidRun {
        async fn server(&self) -> Result<crate::rest::Server, SourceError> {
            self.inner.server().await
        }
        async fn current_user(&self) -> Result<crate::rest::CurrentUser, SourceError> {
            self.inner.current_user().await
        }
        async fn build_types(&self) -> Result<Vec<Rec<BuildType>>, SourceError> {
            self.inner.build_types().await
        }
        async fn builds(&self, locator: &Locator) -> Result<Vec<Rec<Build>>, SourceError> {
            let first = {
                let mut seen = self.seen.lock().expect("not poisoned");
                *seen += 1;
                *seen == 1
            };
            let state = if first { "running" } else { "finished" };
            let rest = FakeRest::new(
                vec![build_type("Payout_Build", "Payout")],
                vec![
                    build(500, "Payout_Build", "Payout", state),
                    build(600, "Payout_Build", "Payout", "finished"),
                ],
            );
            rest.builds(locator).await
        }
    }

    /// Finding: a build that is running when the run starts and finished when
    /// it ends must land in one of the two sets, never neither.
    ///
    /// With the in-flight poll **first** it lands in both: the poll clamps
    /// below it, then the finished query emits it and the watermark may pass
    /// it because it was emitted. With the finished query first it is in
    /// neither -- `state:finished` excludes it at the start,
    /// `state:(queued:true,running:true)` excludes it at the end -- nothing
    /// holds the watermark below it, and `sinceBuild:(id:600)` never offers it
    /// again. This test fails in that order, which is the point of it.
    #[tokio::test]
    async fn a_build_that_finishes_mid_run_is_not_lost_between_the_two_queries() {
        let rest = FinishesMidRun {
            inner: tidewater(),
            seen: Mutex::new(0),
        };
        let mut sink = VecSink(Vec::new());
        let cursor = execute(
            "teamcity",
            &TeamCityConfig::default(),
            &rest,
            Some(r#"{"v":1,"since_build_id":400}"#.to_owned()),
            &mut sink,
        )
        .await
        .expect("run");
        let keys = keys(&sink.0);
        assert!(
            keys.contains(&"build:500".to_owned()),
            "the build that finished mid-run must still be emitted; got {keys:?}"
        );
        // ...and exactly once, though both queries returned it.
        assert_eq!(
            keys.iter().filter(|k| *k == "build:500").count(),
            1,
            "the two queries both saw build 500; it is one item, not two: {keys:?}"
        );
        assert_eq!(
            cursor, r#"{"v":1,"since_build_id":600}"#,
            "500 was emitted, so the watermark is free to pass it"
        );
    }

    /// The run must not depend on the server's ordering. Both real TeamCity
    /// and `knobas-mockd` answer newest-first, so the ascending run here is
    /// the only thing that can catch a dependency on the direction.
    ///
    /// This pins **determinism**, not direction: both runs pass through the
    /// same ordering, so they move together under any change to it. Direction
    /// is pinned by
    /// `builds_are_emitted_oldest_first_whatever_the_server_sent` instead.
    #[tokio::test]
    async fn the_run_is_the_same_whichever_order_the_server_answers_in() {
        let cfg = TeamCityConfig::default();
        let (newest_first, cursor_a) = run(&tidewater(), &cfg, None).await;
        let (ascending, cursor_b) = run(&tidewater().ascending(), &cfg, None).await;
        assert_eq!(keys(&newest_first), keys(&ascending));
        assert_eq!(cursor_a, cursor_b);
    }

    /// The emission order is the run's own, asserted against the property
    /// rather than against another run.
    ///
    /// The server here answers newest-first (3, 2, 1) and the run must still
    /// emit oldest-first. A fixture whose ids arrive already ascending could
    /// not tell the two apart.
    #[tokio::test]
    async fn builds_are_emitted_oldest_first_whatever_the_server_sent() {
        let rest = FakeRest::new(
            vec![build_type("Payout_Build", "Payout")],
            vec![
                build(3, "Payout_Build", "Payout", "finished"),
                build(1, "Payout_Build", "Payout", "finished"),
                build(2, "Payout_Build", "Payout", "finished"),
            ],
        );
        let (items, _) = run(&rest, &TeamCityConfig::default(), None).await;
        assert_eq!(
            keys(&items),
            ["buildType:Payout_Build", "build:1", "build:2", "build:3"],
            "oldest-first, and numerically: the key's string order would put build:10 before \
             build:2"
        );
    }
}
