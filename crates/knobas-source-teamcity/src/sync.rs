//! One sync run.
//!
//! Every run **opens** by witnessing as much of the build id space as two
//! queries can see -- one page of builds in any state, then one unconditional
//! `state:(queued:true,running:true)` poll. The highest id across those two
//! pages is the run's [`ceiling`], which the watermark may not pass, and that
//! is what keeps a build queued *during* the run from being skipped. The
//! in-flight poll is unconditional because a running build mutates in place
//! and never gets a new id for a watermark to find it by, and it opens the run
//! rather than closing it so that a build which is running when the run starts
//! and finished when it ends lands in *both* sets rather than in neither; see
//! [`execute`] steps 1 to 3.
//!
//! Then a **full sync** (`cursor: None`) lists the build configurations in
//! scope and, for each, the newest `builds_per_config` finished builds. An
//! **incremental** run skips the listing and asks once, globally, for
//! `state:finished,sinceBuild:(id:N)` -- build ids are server-wide, so one
//! query covers every configuration -- then narrows to the scope client-side.
//!
//! Those two are the run's **item-producing** queries, and both carry
//! `canceled:any,failedToStart:any` (issue #105) and `branch:default:any`
//! (issue #266). TeamCity's default filter hides canceled, failed-to-start
//! and personal builds, and narrows a branched configuration to its default
//! branch, even when `state:` is set -- and a build that never enters the
//! mirror can never be corrected later: the mirror shows in-flight builds,
//! has no deletion channel, and `sinceBuild` is exclusive, so a build shown
//! as running and then canceled would say `running` for ever, and a
//! feature-branch build was never shown at all. The three named dimensions
//! re-open exactly their own facets; personal builds stay out, which
//! `defaultFilter:false` would not have allowed. See [`since`].
//!
//! Neither query is allowed to come back truncated. Both widen `count:` until
//! the server itself says there is no page after the one it served, and
//! **fail** at a ceiling rather than compute a watermark from a set the run
//! knows is incomplete; see [`all_of`] and [`last_page`]. A page merely
//! *shorter* than the `count:` a request named ends nothing on its own: that
//! reading is true only of a server that served exactly what it was asked for
//! (issue #114).
//!
//! Configurations are emitted for every configuration in scope on a full sync,
//! and on an incremental run only for the configurations whose builds moved --
//! from the same `/buildTypes` listing either way, so a configuration's
//! payload never alternates between a rich and a lean shape. If nothing was
//! emitted, the cursor handed in comes back byte-identical (contract battery
//! clause 2).

use std::collections::{BTreeMap, BTreeSet};

use knobas_source::{Cursor, Sink, SourceError, SyncItem};

use crate::client::{Page, Rec, Rest};
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

/// How many builds the opening probe asks for.
///
/// A whole page. The ceiling is the **maximum** id the page carries and not
/// the id printed in row 0, because `/app/rest/builds` does not answer in id
/// order (issue #91), so every extra row is more of the id space witnessed and
/// none of it is an ordering assumption. Two rows were enough only while row 0
/// was believed to be the newest build in existence.
///
/// Deliberately one page, and deliberately not widened by [`all_of`]: this is
/// evidence, not a corpus. An id the probe misses costs a re-fetch on a later
/// run, never a build -- see [`ceiling`].
const CEILING_PROBE: u32 = PAGE;

pub(crate) async fn execute(
    source_id: &str,
    cfg: &TeamCityConfig,
    rest: &dyn Rest,
    cursor_in: Option<Cursor>,
    sink: &mut (dyn Sink + Send),
) -> Result<Cursor, SourceError> {
    let previous = cursor_in.as_deref().and_then(cursor::parse);
    let before = previous.map_or(0, |p| p.since_build_id);
    let mut configs: BTreeMap<String, Rec<BuildType>> = BTreeMap::new();

    // 1. The opening probe: one page of builds, whatever their state.
    //
    //    Taken before anything else, because a ceiling only means anything if
    //    it is read at the *start* of the run. Steps 2 and 4 below can only
    //    protect builds they can see, and neither can see a build queued after
    //    they ran. Ids are assigned at queue time and are monotonic, so every
    //    such build has an id above everything that existed now, and one clamp
    //    covers all of them. See [`ceiling`] and [`cursor::advance`].
    let probe = probe(rest).await?;

    // 2. Queued and running builds, unconditionally and **first**.
    //
    //    Unconditionally, because a build mutates in place while it runs and
    //    never gets a new id, so no watermark can find it.
    //
    //    First, because step 4 is not instantaneous. A full sync issues one
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
    //    The two loss classes are disjoint, which is why this order is a fix
    //    and the ceiling is a second one: this order saves builds *in flight
    //    at run start*, and the ceiling saves builds *queued after* it.
    let in_flight = in_flight(rest).await?;

    // 3. The ceiling, from the two pages the run has now witnessed -- and the
    //    one check that can still refuse the whole run.
    //
    //    Both before step 4, which is the expensive one: a full sync issues a
    //    request per configuration, and a run that is going to refuse should
    //    not spend that first.
    let ceiling = ceiling(&probe, &in_flight);
    refuse_a_replaced_server(rest, ceiling, before).await?;

    // 4. The finished builds this run is responsible for.
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
                // The one query in the run that is a **window** rather than
                // a set: `builds_per_config` is the newest N this full sync
                // wants, so a page filled to it is the answer and a
                // `nextHref` past it is the older history the descriptor
                // already declares non-exhaustive. Nothing here reads
                // `Page::more` for that reason -- see
                // `descriptor_template`.
                out.extend(
                    rest.builds(&Locator {
                        build_type_id: Some(id),
                        state: Some(StateFilter::Finished),
                        // One of the run's two **item-producing** locators, and
                        // both carry the two dimensions: a build the mirror
                        // never stores is a build no later run can heal. See
                        // [`since`].
                        //
                        // The cost here is *eviction*, not the overflow
                        // `all_of` refuses -- this query is never walked. A
                        // canceled or failed-to-start build now occupies a
                        // slot in a fixed newest-N window, so the window
                        // reaches correspondingly less far back in ordinary
                        // builds, and a configuration with many cancellations
                        // heals fewer stale `running` rows per full sync.
                        // `builds_per_config` is the lever.
                        canceled_any: true,
                        failed_to_start_any: true,
                        // And the branch facet, the third one the same
                        // filter closes -- the fixture's failed build on
                        // `feature/PAY-231-sepa-retry` came back from
                        // neither item-producing query on a real server
                        // until this was measured (issue #266). See
                        // [`since`].
                        branch_any: true,
                        count: cfg.builds_per_config,
                        ..Locator::default()
                    })
                    .await?
                    .items,
                );
            }
            out
        }
        // Build ids are server-wide and monotonic, so one query covers every
        // configuration; the scope is applied client-side below.
        Some(state) => since(rest, state.since_build_id).await?,
    };

    // 5. Scope, watermarks, items.
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

    // 6. The configurations of the builds that moved. Fetched from the same
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
    Ok(cursor::render(cursor::new(cursor::advance(
        before,
        cursor::Seen {
            max_finished,
            min_unfinished,
            ceiling,
        },
    ))))
}

/// Whether a page just answered is the whole of its query.
///
/// **Two signals, and both have to agree.** The obvious spelling --
/// `page.items.len() < asked` -- reads a page shorter than the `count:` the
/// request named as the end of the query, and that is sound only if the server
/// served exactly what it was asked for. `count:` is a request: a TeamCity can
/// be configured with its own ceiling on how many entries a REST list returns,
/// and a loaded one can answer partially. Under either, the short-page rule
/// hands a truncated set back as the complete answer, the watermark is computed
/// from it, and every record after the cut is one `sinceBuild` will never offer
/// again. That is the failure the module docs above are about, and it is the
/// reading issue #81 removed from the Gitea adapter's four walks; #114 is the
/// same reading, one crate over.
///
/// So the *positive* signal is TeamCity's own: [`Page::more`] is `nextHref`,
/// which the server computes from the page it actually produced rather than
/// from the `count:` it was asked for, and which is therefore present exactly
/// when it truncated -- whether it truncated at our number or at its own. That
/// moves the judgement off an assumption this adapter makes about the server
/// and onto a statement the server makes about itself.
///
/// **The short page is kept as a second condition, deliberately, and it is
/// free.** TeamCity reports a next page whenever the page came back *filled to
/// the number of rows it served* -- `count:41` and `count:42` over exactly 42
/// matches both answered a `nextHref`, `count:43` answered none -- so on a
/// server that serves what it was asked for the two conditions say the same
/// thing and the second costs nothing. They diverge in exactly one place, which
/// is the one this is about: a page short of `count:` but filled to the
/// server's own ceiling reports more while being short.
///
/// What the second condition buys is the other direction. `more` is a positive
/// claim that another page exists; its *absence* is not a positive claim of
/// exhaustion -- a reverse proxy, a TeamCity too old to send `nextHref`, or a
/// `fields=` that stopped asking for it all produce the same absence over a
/// server with plenty left. Ending on `!more` alone would truncate every walk
/// at its first page under any of those, which is a worse regression than the
/// one being removed here and one no fixture written against a well-behaved
/// fake would show. Requiring the page to be short as well means the walk stops
/// only when two independent things agree, the one outcome that must never
/// happen -- stopping on a page the server filled -- is unreachable, and against
/// a server that omits `nextHref` this degrades to exactly the old rule rather
/// than to something new
/// (`a_server_that_never_reports_a_next_page_is_still_walked_to_the_end`).
/// "Filled" there means filled to the `count:` **this request asked for**; the
/// residual below is the other sense of the word, and the two do not
/// contradict each other.
///
/// **What is left, stated rather than guarded.** A server that both serves
/// fewer rows than it was asked for *and* omits `nextHref` while doing it is
/// indistinguishable from an exhausted query, from the client, on this
/// endpoint: `/app/rest/builds` publishes no total, and the only other walk
/// available is the offset one this adapter refuses (see [`Page::more`]). No
/// such server is known -- JetBrains' public instance answers `nextHref` on
/// every truncated page, measured 2026-08-29 -- and the residual is recorded
/// here because that is where the next reader of this file will look.
fn last_page(page: &Page<Build>, asked: u32) -> bool {
    !page.more && page.items.len() < asked as usize
}

/// Every build matching `locator`, widening `count:` until the server says
/// there is nothing after the page it just served. The locator's own `count` is
/// ignored -- this owns that dimension.
///
/// A page the server did not end ([`last_page`]) means it had more to give and
/// the page boundary hid them, so asking again with a bigger `count` is the
/// only way to see them. Widening re-reads from the top rather than paging by
/// offset, which is what makes it safe under both orderings: real TeamCity
/// answers newest-first and `knobas-mockd` answers ascending, and an offset
/// walked over a list that grows at the front skips rows. `count` is an
/// independent locator dimension, so widening adds no grammar the mock contract
/// does not already allow.
///
/// # Errors
///
/// [`SourceError::Protocol`], with the caller's message, once the server is
/// still reporting more at [`MAX_BUILDS_PER_QUERY`]. **This must never be
/// `Ok`.** A truncated page silently returned is how a build becomes
/// permanently unreachable: on the finished side the watermark advances past
/// builds this run never emitted, and on the in-flight side it advances past
/// builds that have not finished yet. Either way `sinceBuild` never offers them
/// again, and a full sync is a window rather than the corpus (see
/// [`descriptor_template`](crate::descriptor_template)), so not even a cursor
/// reset recovers them. A failed run, by contrast, leaves the cursor where it
/// is and the scheduler retries.
///
/// The final probe deliberately asks for one *more* than the ceiling, so
/// "exactly [`MAX_BUILDS_PER_QUERY`] builds" is a success rather than a
/// spurious failure at the boundary.
///
/// **What a capping server costs, and why refusing is the whole of the
/// remedy.** Widening is the only lever this query has. Against an instance
/// whose own per-request ceiling sits below the number of builds one run needs
/// to see, every widening comes back at that ceiling with more still reported,
/// and the run ends here -- for as long as the query stays that big. That is a
/// source which cannot sync until it is narrowed (`build_type_ids`,
/// `project_ids`, a shorter interval) or until the ceiling is raised, and it is
/// deliberately preferred to the alternative: mirroring the ceiling's worth of
/// builds, advancing the watermark past the rest, and reporting success. The
/// failure names the ceiling ([`capped_short`]) so nobody spends the outage
/// narrowing a source that was never too big.
///
/// **The boundary that costs, stated because it is new.** A query whose matches
/// number *exactly* the server's ceiling is refused although it fitted: the
/// page comes back filled to that ceiling, a real TeamCity reports a further
/// page off a filled one whether or not anything follows, and the two signals
/// then agree on "not the end" over a set the run had whole. That query synced
/// correctly before this rule, so it is a regression -- in the safe direction,
/// at one match count per capping server, and against the silent truncation
/// this whole change removes. The `+ 1` on `ceiling` below is what keeps
/// [`MAX_BUILDS_PER_QUERY`] itself off that boundary, and it cannot be aimed at
/// a ceiling the client does not know. Held by
/// `a_query_that_exactly_fills_a_server_s_ceiling_is_refused_though_it_fitted`,
/// so the next reader meets it as a decision rather than as a bug.
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
        if last_page(&page, count) {
            return Ok(page.items);
        }
        if count >= ceiling {
            // The caller's message says the query is bigger than one run can
            // carry, and against a capping server that is not what happened:
            // it would send the user narrowing a source that already fits.
            // `capped_short` replaces it rather than joining it.
            return Err(SourceError::protocol(
                capped_short(page.items.len(), count).unwrap_or_else(overflowed),
            ));
        }
        count = count.saturating_mul(2).min(ceiling);
    }
}

/// The refusal for a server that stopped the page itself, in place of the
/// caller's "this query is too big".
///
/// `None` when the last request came back filled: then the query really is
/// bigger than one run can carry, and the caller's own message -- sync more
/// often, narrow the configurations -- is the true one.
///
/// `Some` when it came back **short and the server still reported more**. That
/// is a per-request ceiling on the instance rather than a large query, and the
/// caller's advice is then actively misleading: a source of two hundred builds
/// behind a fifty-row ceiling is not one narrowing helps, and a message naming
/// [`MAX_BUILDS_PER_QUERY`] would send the user looking for a thousand builds
/// that are not there. Naming the two numbers the server itself produced is
/// what lets the two cases be told apart without reading this file.
///
/// One case the wording overstates: at [`all_of`]'s boundary -- a query with
/// exactly the ceiling's worth of matches -- the run *did* see everything, so
/// "below what this run has to see in one query" is then a claim about the page
/// rather than about the corpus. The remedy it names still works, and naming
/// the ceiling still beats the caller's "narrow this source", which is wrong in
/// both cases.
fn capped_short(served: usize, asked: u32) -> Option<String> {
    (served < asked as usize).then(|| {
        format!(
            "teamcity: this server answered a request for {asked} builds with {served} of them \
             and still reported a further page, so it puts its own ceiling on how many entries \
             one REST list returns -- below what this run has to see in one query. Widening \
             `count:` is the only lever this query has and it is spent. Carrying on would mean \
             mirroring {served} builds and advancing the watermark past every one after them, \
             which `sinceBuild` could never offer again. Raise the server's per-request limit, \
             or narrow this source with build_type_ids/project_ids, or sync more often, so that \
             one query needs fewer than {served} builds."
        )
    })
}

/// One page of builds, in whatever state and whatever order the server likes:
/// the run's opening witness of the build id space.
///
/// One request, deliberately un-widened -- see [`CEILING_PROBE`]. Read by
/// [`ceiling`], which takes the page's **maximum** id and reads nothing
/// positional, so this asks for no order and depends on none.
///
/// **`defaultFilter:false` is load-bearing.** TeamCity's default filter hides
/// everything that is not a finished, non-personal, non-canceled build, so
/// without it this page witnesses only the newest *finished* ordinary builds
/// and the ceiling comes back lower than the evidence allows -- which costs
/// re-fetches on every later run. It is not a hypothetical margin: on
/// JetBrains' public instance the highest id in the id space belonged to
/// `JetBrainsPublicProjects_Compose_AllPersonalBuild`, a *personal* build
/// configuration, which is exactly the class the default filter removes.
///
/// No `state:` dimension: `state:` names a set of builds to fetch, and the
/// question here is one number about *every* build whatever its state.
///
/// The page's own `nextHref` is dropped rather than acted on, and this is the
/// one place in the run where that is right: there is always more of the id
/// space than one page of it, and this asks for evidence rather than for a
/// set. A server that serves fewer rows than [`CEILING_PROBE`] asked for makes
/// the ceiling lower, which [`ceiling`] already records as the safe direction
/// -- a re-fetch on a later run, never a build.
async fn probe(rest: &dyn Rest) -> Result<Vec<Rec<Build>>, SourceError> {
    Ok(rest
        .builds(&Locator {
            default_filter: Some(false),
            count: CEILING_PROBE,
            ..Locator::default()
        })
        .await?
        .items)
}

/// The highest build id this run **witnessed** while opening, or `None` when
/// both opening pages were empty.
///
/// # What the ceiling actually has to be
///
/// Weaker than "the newest build on the server", which is what this used to
/// try to be and could not. It only has to be an id known to have existed by
/// the time the in-flight poll completed. Every build that did not exist then
/// has a higher id, because ids are handed out at queue time and are
/// monotonic; so a watermark held at or below such an id cannot pass one of
/// them, and one number covers all of them at once.
///
/// That is exactly what the two opening pages supply. Every unfinished build
/// alive at that instant is in the in-flight page -- [`in_flight`] widens and
/// **fails** at [`MAX_BUILDS_PER_QUERY`] rather than truncating, which is what
/// makes its maximum a witness and not a guess -- and every id in either page
/// is an id the server itself reported.
///
/// # Never the finished pages
///
/// The pages step 4 fetches are right there and taking their maximum looks
/// like a simplification. It is the bug this ceiling exists to prevent, so it
/// is written down rather than left to be rediscovered.
///
/// Build Q is queued after the in-flight poll and is still running when the
/// finished query answers. Build R is queued after Q and finishes inside the
/// same run. R is in the finished pages, so their maximum is at least R, which
/// is above Q. A ceiling of R clamps nothing, the watermark passes Q, and
/// `sinceBuild` never offers Q again once it finishes -- and a full sync is a
/// window rather than the corpus, so not even a cursor reset recovers it. That
/// is the silent, watermark-advancing loss this crate refuses everywhere else,
/// reintroduced by the thing meant to close it. [`execute`] step 2 states the
/// division of labour the other way round: the poll order saves builds *in
/// flight at run start*, and the ceiling saves builds *queued after* it.
///
/// # Why the maximum and not row 0
///
/// `/app/rest/builds` does not answer in id order. Measured read-only against
/// JetBrains' public instance (2026.2 EAP, build 238763) on 2026-08-28, all in
/// the same minute: `defaultFilter:false,count:200` answered a page whose row
/// 0 was `6518363` and whose maximum was `6520204`, and
/// `defaultFilter:false,count:20` answered `6518363, 6518362, 6466333,
/// 6466438, 6471105, ...` -- two descending rows and then a long ascending
/// run. Nothing in `testenv/specs/teamcity.json` ever promised an order, and
/// asking for one is refused outright: `order:(id:desc)` comes back
/// `Locator dimension [order] is unknown`.
///
/// A maximum is the same number under every ordering, so there is no ordering
/// assumption left here to guard -- which is why the two-row guard that used
/// to stand in this file is gone. It read rows 0 and 1 of exactly the page
/// above, saw them descend, and certified the assumption it existed to
/// falsify.
///
/// # Under-estimating is the safe direction
///
/// Both inputs are ids the server reported, so this cannot over-estimate. It
/// can under-estimate -- a page is not the id space -- and that costs a
/// re-fetch on a later run, never a build: a watermark held too low re-reads
/// builds already mirrored, and upserts are idempotent.
///
/// **What remains.** A server that answered every probe with the same hundred
/// ancient builds while keeping its queue empty would pin the ceiling near the
/// bottom of the id space, and past [`MAX_BUILDS_PER_QUERY`] finished builds
/// [`since`] would then refuse every run. It is recorded rather than guarded,
/// because the guard that used to stand here did not detect that case either
/// and did assert something false about a real server. No case of *it* is
/// known -- and that sentence is deliberately narrower than the one it
/// replaces, which said the same of a page that is newest-first and still
/// wrong about the newest build. That case is issue #91, and it was the
/// default behaviour of a current TeamCity.
fn ceiling(probe: &[Rec<Build>], in_flight: &[Rec<Build>]) -> Option<i64> {
    probe.iter().chain(in_flight).map(|b| b.rec.id).max()
}

/// Refuse a server that cannot be the one this source's watermark came from.
///
/// The watermark is a build id this source has already synced. Ids are
/// monotonic and never reused, and TeamCity's cleanup removes the *oldest*
/// builds, so on the server that issued it that build is still there. A source
/// repointed at another instance, or one restored from a state older than the
/// watermark, is the case where it is not -- and carrying on from a position
/// that describes another server's id space fills the mirror with the wrong
/// builds while looking like an ordinary incremental run. That is the quiet,
/// expensive variant: it arrives years in, on a source everyone trusts.
///
/// # The evidence is a fetch, not a comparison
///
/// Until issue #91 this refused whenever the opening probe's row 0 came back
/// below the watermark. On a server whose pages are unordered that happens on
/// an ordinary run -- the live instance answered one page with a maximum of
/// `6520204` and another, the same minute, with a row 0 of `6518363` -- so a
/// healthy source refused every run for ever, blaming a replacement that had
/// not happened. The remedy the message named, resetting the cursor, only
/// restarted the loop: the next run read the same row 0 and re-pinned the
/// ceiling in the same place.
///
/// So the comparison is only a **trigger** now, and the answer comes from the
/// server: when nothing the run witnessed reaches the watermark, ask about
/// that one build. `GET /app/rest/builds/id:{id}` is in interfaces §4.2's
/// endpoint list, and it answers 404 exactly when the build is not there.
///
/// * **Found** -- the watermark stands and the run carries on. The opening
///   pages simply did not happen to show it, which is the ordinary case on a
///   server whose pages are not ordered. The ceiling is then below the
///   watermark, which [`cursor::advance`]'s floor already handles: it clamps
///   nothing and drags nothing back.
/// * **Not found** -- the replacement, refused. And the remedy works now:
///   after a cursor reset the ceiling comes from ids this run witnessed rather
///   than from a stale row 0, so the loop cannot restart.
///
/// A watermark of 0 is nothing to check -- no id is below it, and a source
/// that has never synced has no position to contradict.
///
/// # Errors
///
/// [`SourceError::Protocol`] when the watermark's build is absent. Refusing is
/// the same trade [`all_of`] makes: a failed run leaves the cursor where it is
/// and the scheduler retries, where a run computed from a position the server
/// disowns writes items nothing can later untangle.
async fn refuse_a_replaced_server(
    rest: &dyn Rest,
    ceiling: Option<i64>,
    watermark: i64,
) -> Result<(), SourceError> {
    if watermark == 0 || ceiling.is_some_and(|witnessed| witnessed >= watermark) {
        return Ok(());
    }
    if rest.build_exists(watermark).await? {
        return Ok(());
    }
    Err(SourceError::protocol(format!(
        "teamcity: this source's watermark is build {watermark}, and \
         `/app/rest/builds/id:{watermark}` answers 404 -- that build is not on this server. \
         Build ids are monotonic and are never reused, and TeamCity's cleanup removes the \
         oldest builds, so a build this source has already synced cannot be missing from the \
         server it came from: either this source now points at a different instance, or one was \
         restored from a state older than the watermark. Refusing rather than syncing on from a \
         position that describes another server's id space; reset the source's cursor if the \
         server really was replaced."
    )))
}

/// Every finished build newer than `since_build_id` -- **canceled and
/// failed-to-start ones included** (issue #105), **on every branch** (issue
/// #266).
///
/// # The branch facet, measured on a server we own
///
/// TeamCity's default filter has a third facet the public-instance suite
/// could not see: in a branched configuration it answers only the **default
/// branch**, and it does so on every locator whose state set includes
/// `finished`. Measured on the seeded TeamCity 2026.1.3 in `testenv` on
/// 2026-09-02: this query and the per-configuration one in [`execute`] step
/// 4 both answered `count: 0` over a server holding the fixture's failed
/// build 1187 on `feature/PAY-231-sepa-retry`, and `branch:default:any`
/// answered it. The vendored swagger says the same in prose. So until #266
/// no feature-branch build ever entered the mirror, for the same permanent
/// reason as the canceled ones below -- and the fixture's own story, a
/// failed integration-test run on a feature branch, could not be told from
/// a real TeamCity at all. The in-flight poll is **not** widened: a locator
/// restricted to `queued`/`running` was measured to answer every branch
/// unasked, so [`in_flight`] carries none of the three dimensions, as before.
///
/// # Why the two dimensions are here and not left to the server's default
///
/// TeamCity's default filter hides canceled, failed-to-start and personal
/// builds, and goes on hiding them when `state:` is set. This query and the
/// per-configuration one in [`execute`] step 4 are the only two that produce
/// items, so with the default in force **no canceled build could ever enter
/// the mirror**: `sinceBuild` is exclusive and the watermark advances past it,
/// so one canceled while the watermark moved over it was missing permanently
/// rather than late.
///
/// Excluding them is not the tidy alternative it looks like, and this is the
/// argument the decision rests on: [`execute`] step 5 emits in-flight builds
/// too, and [`cursor::advance`] clamps the watermark under them, so a running
/// build is mirrored **before** anyone knows how it ends. Cancel it and the
/// finished queries can no longer return it, the in-flight poll stops
/// returning it, and `map::build_item` records that M1 has no deletion
/// channel -- so that row says `running` for ever. The same happens to a
/// queued build that fails to start. TeamCity's own UI un-hides a canceled
/// build with one click; a mirror that never stored it cannot.
///
/// **`canceled:any,failedToStart:any` and not `defaultFilter:false`.** The two
/// named dimensions re-open exactly their own facets. `defaultFilter:false`
/// also opens the personal facet -- and every facet nobody has enumerated --
/// which would mirror other people's experiments and force a client-side
/// re-filter off a widened `BUILD_FIELDS`. Measured read-only against
/// JetBrains' public instance (2026.2 EAP) on 2026-08-29 over one window:
/// `state:finished,canceled:any,count:100` answered one canceled build, no
/// failed-to-start build and no personal build, while
/// `state:finished,defaultFilter:false,count:100` answered the same canceled
/// build plus a failed-to-start one.
///
/// # What this does not heal
///
/// **Written here because this is where the next reader of the cursor will
/// look.** `sinceBuild` is exclusive, so builds canceled *before* this landed,
/// whose ids sit under the watermark, stay absent -- and pre-existing stale
/// `running` rows heal only when a full sync's per-configuration window
/// reaches them. A deliberate full sync after this lands is the healing move
/// (CONTEXT.md calls it a backfill); anything older than
/// `builds_per_config` back is gone. That is the permanence issue #105
/// measured, now bounded instead of ongoing.
async fn since(rest: &dyn Rest, since_build_id: i64) -> Result<Vec<Rec<Build>>, SourceError> {
    all_of(
        rest,
        &Locator {
            state: Some(StateFilter::Finished),
            since_build_id: Some(since_build_id),
            canceled_any: true,
            failed_to_start_any: true,
            branch_any: true,
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

/// The build configurations in scope.
///
/// # Errors
///
/// [`SourceError::Protocol`] when the server reports a page after the one it
/// answered with. This walk sends no `count:` at all -- interfaces §4.2 lists
/// the endpoint as `GET /app/rest/buildTypes?fields=...` and nothing else --
/// and a TeamCity serves the whole listing for that: 4,253 configurations in
/// one answer with no `nextHref`, measured read-only against JetBrains' public
/// instance on 2026-08-29.
///
/// An instance that pages it anyway would silently narrow the scope of every
/// full sync, and the narrowing is invisible from here: the run would emit
/// fewer `build_config` items and fetch builds for fewer configurations while
/// reporting success, which is the same silent-truncation class as issue #114
/// arriving through the one walk that never had a page size to be wrong about.
/// Refusing is what makes it visible; the alternative -- adding `count:` and
/// widening as [`all_of`] does -- is a locator on an endpoint the contract
/// lists without one, and is the deliberate change to make when a server that
/// pages this is actually met.
async fn scope(rest: &dyn Rest, cfg: &TeamCityConfig) -> Result<Vec<Rec<BuildType>>, SourceError> {
    let page = rest.build_types().await?;
    if page.more {
        return Err(SourceError::protocol(format!(
            "teamcity: `/app/rest/buildTypes` answered {} build configurations and reported a \
             further page. This adapter asks that endpoint for the whole listing and has no \
             second request to make for the rest, so carrying on would sync a silently narrowed \
             scope -- fewer configurations, and only the builds belonging to them -- while \
             reporting a complete run. Refusing instead; this needs a paged build-configuration \
             walk, which no TeamCity has yet been seen to require.",
            page.items.len()
        )));
    }
    Ok(page
        .items
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
        order: PageOrder,
        /// The most rows this server will put on a page, whatever `count:`
        /// asked for -- see [`FakeRest::capped`]. `None` honours `count:`
        /// exactly, which is what every other constructor here does.
        cap: Option<usize>,
        /// Does `/app/rest/buildTypes` report a page after the one it serves?
        /// No TeamCity has been seen to; `scope` refuses one that does.
        paged_build_types: bool,
        /// A server that never sends `nextHref` at all -- see
        /// [`FakeRest::silent`].
        silent: bool,
        calls: Mutex<Vec<String>>,
    }

    /// The order a page comes back in.
    ///
    /// Three, not two, and the third one is the whole reason issue #91 went
    /// unnoticed for a milestone. A fake that can only sort a page can never
    /// serve one whose **row 0 is not its maximum**, so every test written
    /// against it agreed with the code that row 0 was the newest build. A
    /// real TeamCity 2026.2 answers `defaultFilter:false,count:20` with
    /// `6518363, 6518362, 6466333, 6466438, 6471105, ...` -- two newest-first
    /// rows and then a long ascending run -- and [`PageOrder::AsGiven`] is
    /// what lets a fixture say that.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum PageOrder {
        /// As both real TeamCity and `knobas-mockd` answer when the page is
        /// ordered at all.
        NewestFirst,
        /// The opposite. Kept because the run may not depend on either order.
        Ascending,
        /// Fixture order, untouched: whatever the fixture lists, in that
        /// sequence. The only one that can express an unordered page.
        AsGiven,
    }

    impl FakeRest {
        fn new(build_types: Vec<serde_json::Value>, builds: Vec<serde_json::Value>) -> Self {
            Self {
                build_types,
                builds,
                order: PageOrder::NewestFirst,
                cap: None,
                paged_build_types: false,
                silent: false,
                calls: Mutex::new(Vec::new()),
            }
        }

        /// A server whose build-configuration listing is paged: it answers
        /// with a `nextHref`, and this adapter has no second request to make
        /// for the rest of it.
        fn paged_build_types(mut self) -> Self {
            self.paged_build_types = true;
            self
        }

        /// A server that **ignores the `count:` this run asked for** and puts
        /// at most `cap` rows on a page.
        ///
        /// Every other constructor here serves exactly what it was asked for,
        /// so on them a page shorter than `count:` can only mean the query ran
        /// out of matches. That is the one reading issue #114 is about, and a
        /// fake that cannot express the other one cannot show the bug -- which
        /// is why this was never caught. `count:` is a request, not a
        /// guarantee: a TeamCity may be configured with a lower ceiling on how
        /// many entries a REST list returns, and a loaded one may answer a
        /// partial page.
        ///
        /// It reports the truncation the way a real TeamCity does, through
        /// `nextHref`: a page filled to the number of rows the server served
        /// reports a further page, whether or not one is there. Measured on
        /// JetBrains' public instance 2026-08-29, over a query with exactly 42
        /// matches -- `count:41` and `count:42` both answered a `nextHref`,
        /// `count:43` answered none. A capped page is filled to the cap, so it
        /// says there is more while being short, which is the one combination
        /// no other constructor here can produce and the one [`all_of`] has to
        /// read correctly; see [`Page::more`].
        fn capped(mut self, cap: usize) -> Self {
            assert!(
                0 < cap && cap < PAGE as usize,
                "a cap only says anything below the page size the run asks for"
            );
            self.cap = Some(cap);
            self
        }

        /// A server that **never sends `nextHref`**, whatever it truncated:
        /// a reverse proxy that strips it, a TeamCity too old to serve it, or
        /// a `fields=` that stopped asking for it.
        ///
        /// The one that says whether [`last_page`]'s second condition earns
        /// its keep. Ending a walk on `!more` alone would read every one of
        /// this server's pages as the end of its query -- including the first,
        /// full one -- which is a worse truncation than the short-page rule
        /// #114 removed. Against this server the walk falls back to exactly
        /// that older rule and still reaches the end.
        fn silent(mut self) -> Self {
            self.silent = true;
            self
        }

        fn ascending(mut self) -> Self {
            self.order = PageOrder::Ascending;
            self
        }

        /// Pages come back in fixture order, however unordered that is.
        fn fixture_order(mut self) -> Self {
            self.order = PageOrder::AsGiven;
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
        async fn build_types(&self) -> Result<Page<BuildType>, SourceError> {
            self.calls
                .lock()
                .expect("not poisoned")
                .push("buildTypes".to_owned());
            // The whole listing, and no `nextHref`: that is what a real
            // TeamCity answers `/app/rest/buildTypes?fields=...` with, 4,253
            // configurations deep. `FakeRest::paged_build_types` is the other
            // one, and `scope` refuses it.
            Ok(Page {
                items: self
                    .build_types
                    .iter()
                    .map(|raw| Rec {
                        raw: raw.clone(),
                        rec: serde_json::from_value(raw.clone()).expect("fixture parses"),
                    })
                    .collect(),
                more: self.paged_build_types,
            })
        }
        async fn builds(&self, locator: &Locator) -> Result<Page<Build>, SourceError> {
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
                // Which **states** the query asks for.
                .filter(|r| match (locator.state, locator.default_filter) {
                    // `state:` names the states wanted.
                    (Some(state), _) => state.matches(r.rec.state.as_deref()),
                    // No `state:`, default filter off: every build there is.
                    (None, Some(false)) => true,
                    // No `state:` and no `defaultFilter:false` takes TeamCity's
                    // default filter, whose state half is "finished". Without
                    // the dimension the opening probe would witness only the
                    // newest ordinary finished builds, so a fake that ignored
                    // this could not witness the difference.
                    (None, _) => StateFilter::Finished.matches(r.rec.state.as_deref()),
                })
                // ...and which **facets** of them, which is a separate
                // question from the state and the one issue #105 turns on.
                //
                // TeamCity's default filter hides three classes -- canceled,
                // failed-to-start and personal -- and it goes on hiding them
                // when `state:` is set, which is why every item-producing
                // query missed the first two. `defaultFilter:false` opens all
                // three at once; `canceled:any` and `failedToStart:any` open
                // exactly their own and leave the personal facet closed.
                // Measured read-only on JetBrains' public instance
                // (2026-08-29): `state:finished,canceled:any` answered one
                // canceled build and no failed-to-start or personal one, while
                // `state:finished,defaultFilter:false` answered the canceled
                // build plus a failed-to-start one.
                //
                // A fake that could not tell the two apart would let a
                // `defaultFilter:false` on an item-producing locator pass --
                // the widening this ruling deliberately did not make, because
                // it mirrors other people's personal builds.
                .filter(|r| {
                    if locator.default_filter == Some(false) {
                        return true;
                    }
                    (locator.canceled_any || !canceled(&r.rec))
                        && (locator.failed_to_start_any || !flagged(&r.raw, "failedToStart"))
                        && !flagged(&r.raw, "personal")
                })
                // ...and the **branch** facet, the third one, measured on a
                // TeamCity we own rather than on JetBrains' (issue #266).
                // The default filter narrows a branched configuration to its
                // default branch, and it does so on every locator whose
                // state set includes `finished` -- `state:finished`,
                // `state:any`, no `state:` at all -- while a locator
                // restricted to `queued`/`running` answers every branch
                // without being asked. `branch:default:any` re-opens the
                // facet on its own, and `defaultFilter:false` (returned
                // above) opens it with the rest. A record that says nothing
                // about its branch is an unbranched configuration's, which
                // the facet does not touch.
                .filter(|r| {
                    locator.branch_any
                        || locator.default_filter == Some(false)
                        || locator.state == Some(StateFilter::InFlight)
                        || r.raw.get("defaultBranch").and_then(serde_json::Value::as_bool)
                            != Some(false)
                })
                .filter(|r| {
                    locator
                        .build_type_id
                        .as_deref()
                        .is_none_or(|id| r.rec.build_type_id.as_deref() == Some(id))
                })
                .filter(|r| locator.since_build_id.is_none_or(|since| r.rec.id > since))
                .collect();
            match self.order {
                PageOrder::NewestFirst => out.sort_by_key(|r| std::cmp::Reverse(r.rec.id)),
                PageOrder::Ascending => out.sort_by_key(|r| r.rec.id),
                // Deliberately no sort: the fixture's own sequence is the
                // page, which is the only way to serve one whose row 0 is
                // not its maximum.
                PageOrder::AsGiven => {}
            }
            // What the server puts on the page, and what it then says about
            // the rest. A real TeamCity reports a next page whenever the page
            // came back **filled to the number of rows it served** -- measured
            // over a query with exactly 42 matches, `count:41` and `count:42`
            // both answered a `nextHref` and `count:43` answered none -- so it
            // is `serves` and not `asked` that the comparison is against, and
            // a capped page reports the remainder even though it is short.
            // That is the whole difference issue #114 turns on.
            let asked = locator.count as usize;
            let serves = self.cap.map_or(asked, |cap| cap.min(asked));
            let filled = serves > 0 && out.len() >= serves;
            out.truncate(serves);
            Ok(Page {
                items: out,
                more: filled && !self.silent,
            })
        }
        /// The by-id endpoint answers from the corpus, not from a page:
        /// `/app/rest/builds/id:{id}` takes no `count` and no ordering, which
        /// is the whole reason `refuse_a_replaced_server` asks it rather than
        /// reading a page. A fixture can therefore hide a build from every
        /// page -- by listing it past `count` in [`PageOrder::AsGiven`] order
        /// -- and still have the server admit it exists, which is exactly
        /// what the live instance does.
        async fn build_exists(&self, id: i64) -> Result<bool, SourceError> {
            self.calls
                .lock()
                .expect("not poisoned")
                .push(format!("builds/id:{id}"));
            Ok(self
                .builds
                .iter()
                .any(|b| b.get("id").and_then(serde_json::Value::as_i64) == Some(id)))
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

    /// TeamCity reports a canceled build as finished with `status: "UNKNOWN"`
    /// and `statusText: "Canceled"` -- 20 of 20 sampled on the live instance
    /// -- and its default filter hides it. That is the class `canceled:any`
    /// puts back on the two item-producing queries (issue #105).
    fn canceled(b: &Build) -> bool {
        b.status.as_deref() == Some("UNKNOWN")
    }

    /// A boolean the *record* carries and `struct Build` does not parse.
    ///
    /// `failedToStart` and `personal` are real fields on a TeamCity build and
    /// nothing in `map` reads either, so `BUILD_FIELDS` does not ask for them
    /// (`the_selectors_ask_for_nothing_no_reader_looks_at`). The server still
    /// filters on them, so the fake reads them off the raw record -- which is
    /// where a server keeps facts the client never parses.
    fn flagged(raw: &serde_json::Value, name: &str) -> bool {
        raw.get(name).and_then(serde_json::Value::as_bool) == Some(true)
    }

    fn canceled_build(id: i64, type_id: &str, project: &str) -> serde_json::Value {
        let mut b = build(id, type_id, project, "finished");
        b["status"] = serde_json::json!("UNKNOWN");
        b["statusText"] = serde_json::json!("Canceled");
        b
    }

    /// A build that never ran: TeamCity finishes it with `status: "FAILURE"`
    /// and its own `statusText`, and marks it `failedToStart`. The default
    /// filter hides it exactly as it hides a canceled one, and
    /// `failedToStart:any` is what puts it back.
    fn failed_to_start_build(id: i64, type_id: &str, project: &str) -> serde_json::Value {
        let mut b = build(id, type_id, project, "finished");
        b["status"] = serde_json::json!("FAILURE");
        b["statusText"] = serde_json::json!("Failed to start: no agent could run this build");
        b["failedToStart"] = serde_json::json!(true);
        b
    }

    /// A build on a branch that is not its configuration's default. The
    /// class the default filter hides on every finished-state locator, and
    /// the one the fixture's own failed build belongs to: 1187 ran on
    /// `feature/PAY-231-sepa-retry`. `branch:default:any` is what puts it
    /// back on the two item-producing queries (issue #266).
    fn feature_branch_build(id: i64, type_id: &str, project: &str, state: &str) -> serde_json::Value {
        let mut b = build(id, type_id, project, state);
        b["branchName"] = serde_json::json!("feature/PAY-231-sepa-retry");
        b["defaultBranch"] = serde_json::json!(false);
        b
    }

    /// Somebody else's experiment. The third class the default filter hides,
    /// and the one this adapter deliberately leaves hidden: only
    /// `defaultFilter:false` -- which no item-producing query sends -- would
    /// bring it back.
    fn personal_build(id: i64, type_id: &str, project: &str, state: &str) -> serde_json::Value {
        let mut b = build(id, type_id, project, state);
        b["personal"] = serde_json::json!(true);
        b
    }

    /// The parsed-plus-raw pairs a page is made of, for the tests that call
    /// [`ceiling`] directly rather than through a run.
    fn recs(builds: &[serde_json::Value]) -> Vec<Rec<Build>> {
        builds
            .iter()
            .map(|raw| Rec {
                raw: raw.clone(),
                rec: serde_json::from_value(raw.clone()).expect("fixture parses"),
            })
            .collect()
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
        // The whole request budget of a full sync, in order: the ceiling,
        // then ONE in-flight poll -- no repeated `state` dimension, and
        // nothing outside the locator grammar -- and only then the
        // per-configuration queries, which can take ~40 s to walk. See
        // `a_build_that_finishes_mid_run_is_not_lost_between_the_two_queries`
        // and `a_build_queued_after_the_opening_poll_is_not_skipped` for what
        // the order buys; this pins the requests themselves.
        assert_eq!(
            rest.calls(),
            [
                "defaultFilter:false,count:100",
                "state:(queued:true,running:true),count:100",
                "buildTypes",
                concat!(
                    "buildType:(id:Ledger_Deploy_Staging),state:finished,",
                    "canceled:any,failedToStart:any,branch:default:any,count:100"
                ),
                concat!(
                    "buildType:(id:Payout_Build),state:finished,",
                    "canceled:any,failedToStart:any,branch:default:any,count:100"
                ),
                concat!(
                    "buildType:(id:Payout_IntegrationTests),state:finished,",
                    "canceled:any,failedToStart:any,branch:default:any,count:100"
                ),
            ]
        );
        // The **in-flight poll carries none of the dimensions**, and that is
        // the half of the decision that keeps personal builds out: `state:`
        // names queued and running, and the rest of TeamCity's default filter
        // -- the personal facet included -- still applies to it. A run that
        // widened this query too would start mirroring other people's
        // experiments the moment one was running. The branch facet is the
        // measured exception: a queued/running-only locator already answers
        // every branch (issue #266), so there is nothing for `branch:` to
        // re-open there.
        let poll = rest
            .calls()
            .into_iter()
            .find(|c| c.contains("queued:true"))
            .expect("the in-flight poll");
        for dimension in ["canceled:", "failedToStart:", "branch:", "defaultFilter:"] {
            assert!(
                !poll.contains(dimension),
                "the in-flight poll must not carry {dimension}: {poll}"
            );
        }
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
        // Three requests and no `/buildTypes`: an idle poll costs the
        // ceiling, the in-flight query and the finished one, in that order.
        // (The full sync above made four: those three, with the listing and
        // one per-configuration query in place of the last.)
        assert_eq!(
            rest.calls()[4..],
            [
                "defaultFilter:false,count:100",
                "state:(queued:true,running:true),count:100",
                "state:finished,sinceBuild:(id:412),canceled:any,failedToStart:any,branch:default:any,count:100",
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
            "400 is foreign and finished: the run saw it and will never want it again, so \
             leaving the watermark at 300 would re-fetch it on every poll for good"
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
            matches!(&err, SourceError::Protocol { message: m, .. } if m.contains("1000")),
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
                "state:finished,sinceBuild:(id:0),canceled:any,failedToStart:any,branch:default:any,count:100",
                "state:finished,sinceBuild:(id:0),canceled:any,failedToStart:any,branch:default:any,count:200",
                "state:finished,sinceBuild:(id:0),canceled:any,failedToStart:any,branch:default:any,count:400",
                "state:finished,sinceBuild:(id:0),canceled:any,failedToStart:any,branch:default:any,count:800",
                "state:finished,sinceBuild:(id:0),canceled:any,failedToStart:any,branch:default:any,count:1001",
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

    /// One incremental run from build 0 against a server that serves at most
    /// `cap` rows per request, `total` finished builds deep.
    async fn capped_run(
        cap: usize,
        total: i64,
    ) -> (Result<Vec<SyncItem>, SourceError>, Vec<String>) {
        let many: Vec<serde_json::Value> = (1..=total)
            .map(|n| build(n, "Payout_Build", "Payout", "finished"))
            .collect();
        let rest = FakeRest::new(vec![build_type("Payout_Build", "Payout")], many).capped(cap);
        let mut sink = VecSink(Vec::new());
        let outcome = execute(
            "teamcity",
            &TeamCityConfig::default(),
            &rest,
            Some(r#"{"v":1,"since_build_id":0}"#.to_owned()),
            &mut sink,
        )
        .await
        .map(|_| sink.0);
        let widened = rest
            .calls()
            .into_iter()
            .filter(|c| c.contains("sinceBuild"))
            .collect();
        (outcome, widened)
    }

    /// Issue #114: a page shorter than the `count:` this run asked for is
    /// **not** proof the query ran out of matches.
    ///
    /// `count:` is a request. A TeamCity may be configured to serve fewer
    /// entries per REST list than were asked for, and a loaded one may answer
    /// a partial page; under either, reading a short page as the end returns a
    /// truncated set as the whole answer, `max_finished` is computed from it,
    /// and the watermark advances past every build the run never emitted --
    /// which `sinceBuild` can then never offer again. That is the silent,
    /// watermark-advancing loss ADR-0003 and this whole module exist to
    /// refuse, and it is the same reading #81 removed from Gitea's four walks
    /// one crate over.
    ///
    /// Deliberately one test over the two halves of the decision, with an
    /// uncapped control, so the assertions are about the cap and not about the
    /// fixture:
    ///
    /// * **The cap bites.** 250 builds behind a 50-row ceiling cannot be seen
    ///   in one query at all, so the run **fails** rather than mirroring 50 of
    ///   them and moving the watermark to 250. Widening is the only lever this
    ///   query has -- `/app/rest/builds` grows at the front, so an offset walk
    ///   over it skips rows -- and against a server that will not widen there
    ///   is nothing left but to refuse. That cost is stated in [`all_of`] and
    ///   named in the message.
    /// * **The cap does not bite.** 30 builds behind the same ceiling are all
    ///   the query has, the server says so, and the run mirrors every one of
    ///   them. Without this half, "make every capped server fail" would pass.
    #[tokio::test]
    async fn a_short_page_from_a_capping_server_is_not_read_as_the_end_of_the_query() {
        let (truncated, widened) = capped_run(50, 250).await;
        let error = truncated
            .map(|items| format!("Ok({} items)", items.len()))
            .expect_err(
                "50 of 250 builds returned as the whole answer would advance the watermark to \
                 250 and lose the other 200 for good",
            );
        assert!(
            matches!(&error, SourceError::Protocol { message: m, .. }
                if m.contains("answered a request for 1001 builds with 50 of them")
                    && m.contains("still reported a further page")),
            "the refusal has to name the server's own ceiling with the two numbers the server \
             produced: {error:?}"
        );
        assert!(
            matches!(&error, SourceError::Protocol { message: m, .. }
                if !m.contains(&format!("more than {MAX_BUILDS_PER_QUERY}"))),
            "and must NOT claim the query was too big -- there are 250 builds here, not more \
             than 1000, and that advice would send the user narrowing a source that fits: \
             {error:?}"
        );
        assert_eq!(
            widened,
            [
                "state:finished,sinceBuild:(id:0),canceled:any,failedToStart:any,branch:default:any,count:100",
                "state:finished,sinceBuild:(id:0),canceled:any,failedToStart:any,branch:default:any,count:200",
                "state:finished,sinceBuild:(id:0),canceled:any,failedToStart:any,branch:default:any,count:400",
                "state:finished,sinceBuild:(id:0),canceled:any,failedToStart:any,branch:default:any,count:800",
                "state:finished,sinceBuild:(id:0),canceled:any,failedToStart:any,branch:default:any,count:1001",
            ],
            "it widened rather than accepting the first short page"
        );

        // The same ceiling, over a query it can actually carry.
        let (walked, widened) = capped_run(50, 30).await;
        let items = walked.expect("30 builds fit inside a 50-row ceiling");
        assert_eq!(
            items.iter().filter(|i| i.kind == "build").count(),
            30,
            "every build, and no extra request spent proving it"
        );
        assert_eq!(
            widened,
            ["state:finished,sinceBuild:(id:0),canceled:any,failedToStart:any,branch:default:any,count:100"],
            "one request: the server answered short and said there was nothing after it"
        );
    }

    /// The boundary the fix costs, pinned as a decision rather than met later
    /// as a bug: a query with **exactly** the server's ceiling's worth of
    /// matches is refused although it fitted.
    ///
    /// The page comes back filled to the cap, a real TeamCity reports a further
    /// page off a filled one whether or not anything follows it, and
    /// [`last_page`]'s two signals then agree on "not the end" over a set the
    /// run had whole. That query synced correctly before issue #114, so this is
    /// a regression -- in the safe direction, at one match count per capping
    /// server, and against the silent truncation the change exists to remove.
    /// [`all_of`]'s `+ 1` probe is what keeps [`MAX_BUILDS_PER_QUERY`] itself
    /// off this boundary; it cannot be aimed at a ceiling the client does not
    /// know.
    #[tokio::test]
    async fn a_query_that_exactly_fills_a_server_s_ceiling_is_refused_though_it_fitted() {
        let (outcome, _) = capped_run(50, 50).await;
        let error = outcome
            .map(|items| format!("Ok({} items)", items.len()))
            .expect_err("50 builds behind a 50-row ceiling fill the page, so the walk widens");
        assert!(
            matches!(&error, SourceError::Protocol { message: m, .. }
                if m.contains("answered a request for 1001 builds with 50 of them")),
            "{error:?}"
        );

        // One build below it is the ordinary case and still walks: this is a
        // boundary, not "every capped server fails".
        let (outcome, widened) = capped_run(50, 49).await;
        let items = outcome.expect("49 builds behind a 50-row ceiling do not fill the page");
        assert_eq!(items.iter().filter(|i| i.kind == "build").count(), 49);
        assert_eq!(
            widened,
            ["state:finished,sinceBuild:(id:0),canceled:any,failedToStart:any,branch:default:any,count:100"]
        );
    }

    /// [`last_page`]'s second condition, and the reason it is there: a walk
    /// must not end on `!more` alone.
    ///
    /// `nextHref` is a *positive* claim that another page exists. Its absence
    /// is not the opposite claim -- a reverse proxy that strips it, a TeamCity
    /// too old to serve it, or a `fields=` that stopped asking for it all
    /// produce the same absence over a server with plenty left. A walk that
    /// ended on `!more` alone would stop at the first page against every one
    /// of those, which is a *worse* truncation than the short-page reading
    /// #114 removed, and no fixture written against a well-behaved fake would
    /// have shown it.
    ///
    /// So this server truncates its pages and says nothing about it, and the
    /// walk still reaches all 250 builds: the fix degrades to the rule it
    /// replaced rather than to something new.
    #[tokio::test]
    async fn a_server_that_never_reports_a_next_page_is_still_walked_to_the_end() {
        let many: Vec<serde_json::Value> = (1..=250)
            .map(|n| build(n, "Payout_Build", "Payout", "finished"))
            .collect();
        let rest = FakeRest::new(vec![build_type("Payout_Build", "Payout")], many).silent();
        let (items, cursor) = run(
            &rest,
            &TeamCityConfig::default(),
            Some(r#"{"v":1,"since_build_id":0}"#.to_owned()),
        )
        .await;
        assert_eq!(
            items.iter().filter(|i| i.kind == "build").count(),
            250,
            "every build: a page of 100 out of 250 is full, and full is not the end whatever \
             the server did or did not say about a next page"
        );
        assert_eq!(cursor, r#"{"v":1,"since_build_id":250}"#);
    }

    /// The one walk with no `count:` to widen: `/app/rest/buildTypes` is asked
    /// for the whole listing, so a server that pages it has nothing this
    /// adapter can ask next.
    ///
    /// Refusing is the visible outcome. Carrying on would sync a silently
    /// narrowed scope -- fewer `build_config` items, and only the builds
    /// belonging to the configurations that fitted on the page -- while
    /// reporting a complete run, which is issue #114's failure class arriving
    /// through the one walk that never had a page size to be wrong about.
    #[tokio::test]
    async fn a_paged_build_configuration_listing_is_refused_rather_than_narrowing_the_scope() {
        let rest = tidewater().paged_build_types();
        let mut sink = VecSink(Vec::new());
        let error = execute(
            "teamcity",
            &TeamCityConfig::default(),
            &rest,
            None,
            &mut sink,
        )
        .await
        .expect_err("a partial build-configuration listing is not a scope");
        assert!(
            matches!(&error, SourceError::Protocol { message: m, .. }
                if m.contains("buildTypes") && m.contains("further page")),
            "{error:?}"
        );
        assert!(
            sink.0.is_empty(),
            "nothing is emitted from a run that cannot complete"
        );

        // The control: the same fixture from a server that serves the listing
        // whole -- which is what a real TeamCity does, 4,253 configurations in
        // one answer -- syncs.
        let (items, _) = run(&tidewater(), &TeamCityConfig::default(), None).await;
        assert!(items.iter().any(|i| i.kind == "build_config"));
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
            matches!(&err, SourceError::Protocol { message: m, .. } if m.contains("queued or running")),
            "{err:?}"
        );
        assert!(
            sink.0.is_empty(),
            "nothing is emitted from a run that cannot complete"
        );
    }

    /// A [`Rest`] whose builds change **once**, between the run's two build
    /// queries: it serves `before` at t0 and `after` from t1 on. The race in
    /// the middle of a run, made deterministic.
    ///
    /// The window is not theoretical: a full sync issues one request per
    /// configuration between the two, and at the default 5 req/s two hundred
    /// configurations is ~40 s.
    ///
    /// **The ceiling probe reads the clock without advancing it.** It is part
    /// of opening the run, not one of the two queries the race sits between --
    /// and counting it would silently move t1 one request earlier, so that
    /// every test built on this would stop discriminating the order of the two
    /// queries and quietly start passing under either. That is not
    /// hypothetical: adding the probe to the front of the run did exactly
    /// that, and [`is_ceiling_probe`] is the repair.
    ///
    /// Nothing about that repair is self-evident from a green suite, so
    /// [`elapsed`](MidRun::elapsed) is exposed and
    /// `the_mid_run_clock_advances_on_exactly_the_two_queries_the_race_sits_between`
    /// pins it.
    struct MidRun {
        build_types: Vec<serde_json::Value>,
        before: Vec<serde_json::Value>,
        after: Vec<serde_json::Value>,
        elapsed: Mutex<usize>,
    }

    /// Is this locator the run's opening ceiling probe?
    ///
    /// The probe is the run's only query with no `state:` at all, so that
    /// shape identifies it without naming `defaultFilter`. Two reasons not to
    /// name it. It would exempt any future query that also turns the default
    /// filter off -- the same class of accident this function exists to undo
    /// -- and, more immediately, it would make `defaultFilter` un-mutatable:
    /// flipping it in [`probe`] would silently move this clock too, and the
    /// kill for `the_ceiling_counts_a_build_the_default_filter_would_hide`
    /// could no longer be attributed to the thing under test.
    ///
    /// `count` is matched too, and follows [`CEILING_PROBE`] rather than a
    /// literal, so widening the probe cannot silently un-identify it. It no
    /// longer discriminates on its own -- the probe and the in-flight poll
    /// both ask for [`PAGE`] now -- which is why `state` carries the check.
    fn is_ceiling_probe(l: &Locator) -> bool {
        l.count == CEILING_PROBE
            && l.state.is_none()
            && l.since_build_id.is_none()
            && l.build_type_id.is_none()
    }

    impl MidRun {
        fn new(
            build_types: Vec<serde_json::Value>,
            before: Vec<serde_json::Value>,
            after: Vec<serde_json::Value>,
        ) -> Self {
            Self {
                build_types,
                before,
                after,
                elapsed: Mutex::new(0),
            }
        }

        /// The server as it stands at one of the two instants this fake has.
        fn snapshot(&self, builds: &[serde_json::Value]) -> FakeRest {
            FakeRest::new(self.build_types.clone(), builds.to_vec())
        }

        /// How many clock-advancing queries the run made. Exactly two on a
        /// correct run: the in-flight poll and the finished query.
        fn elapsed(&self) -> usize {
            *self.elapsed.lock().expect("not poisoned")
        }
    }

    #[async_trait::async_trait]
    impl Rest for MidRun {
        async fn server(&self) -> Result<crate::rest::Server, SourceError> {
            self.snapshot(&self.before).server().await
        }
        async fn current_user(&self) -> Result<crate::rest::CurrentUser, SourceError> {
            self.snapshot(&self.before).current_user().await
        }
        async fn build_types(&self) -> Result<Page<BuildType>, SourceError> {
            self.snapshot(&self.before).build_types().await
        }
        async fn builds(&self, locator: &Locator) -> Result<Page<Build>, SourceError> {
            let at_t0 = {
                let mut elapsed = self.elapsed.lock().expect("not poisoned");
                if !is_ceiling_probe(locator) {
                    *elapsed += 1;
                }
                *elapsed <= 1
            };
            let builds = if at_t0 { &self.before } else { &self.after };
            self.snapshot(builds).builds(locator).await
        }
        async fn build_exists(&self, id: i64) -> Result<bool, SourceError> {
            self.snapshot(&self.before).build_exists(id).await
        }
    }

    /// The clock [`MidRun`] keeps is what makes every mid-run test able to
    /// fail, and nothing else checks it.
    ///
    /// Two clock-advancing queries, no more and no fewer: the in-flight poll
    /// is t0 and the finished query is t1, and the ceiling probe is neither.
    /// A third would mean the probe started counting, which moves t1 one
    /// request earlier and leaves
    /// `a_build_that_finishes_mid_run_is_not_lost_between_the_two_queries`
    /// passing under **either** query order -- a vacuous test, produced by a
    /// change that never touched it.
    ///
    /// An incremental run, because a full sync's per-configuration queries are
    /// clock-advancing too and would drown the number this is about.
    #[tokio::test]
    async fn the_mid_run_clock_advances_on_exactly_the_two_queries_the_race_sits_between() {
        let rest = MidRun::new(
            vec![build_type("Payout_Build", "Payout")],
            vec![build(500, "Payout_Build", "Payout", "running")],
            vec![build(500, "Payout_Build", "Payout", "finished")],
        );
        let mut sink = VecSink(Vec::new());
        execute(
            "teamcity",
            &TeamCityConfig::default(),
            &rest,
            Some(r#"{"v":1,"since_build_id":400}"#.to_owned()),
            &mut sink,
        )
        .await
        .expect("run");
        assert_eq!(
            rest.elapsed(),
            2,
            "the in-flight poll and the finished query advance the clock; the ceiling probe must \
             not, or t1 moves one request earlier and the mid-run tests stop \
             discriminating the query order"
        );
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
        let rest = MidRun::new(
            vec![build_type("Payout_Build", "Payout")],
            vec![
                build(500, "Payout_Build", "Payout", "running"),
                build(600, "Payout_Build", "Payout", "finished"),
            ],
            vec![
                build(500, "Payout_Build", "Payout", "finished"),
                build(600, "Payout_Build", "Payout", "finished"),
            ],
        );
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

    /// Issue #91, measured against JetBrains' public TeamCity (2026.2 EAP,
    /// build 238763) and reproduced here: **row 0 of the probe page is not
    /// its maximum**, and a ceiling read from row 0 wedges the source
    /// permanently on its second run.
    ///
    /// The live numbers, all read-only `GET /app/rest/builds` against the
    /// same server in the same minute:
    ///
    /// * `defaultFilter:false,count:2` answered `6518363, 6518362` --
    ///   descending, so the ordering guard this crate used to carry passed.
    /// * `defaultFilter:false,count:200` answered a page whose **row 0 was
    ///   6518363 and whose maximum was 6520204**.
    /// * `defaultFilter:false,count:20` answered `6518363, 6518362, 6466333,
    ///   6466438, 6471105, ...` -- two newest-first rows and then an
    ///   ascending run. The page is not ordered at all.
    ///
    /// So row 0 is neither the maximum nor stable between runs, and reading
    /// it as "the newest build in existence" produced the failure this test
    /// pins: one run sets the watermark from a high row 0, the next run's row
    /// 0 comes back lower, and the run refuses as though the server had been
    /// replaced. The remedy that refusal named -- reset the cursor -- only
    /// restarted the loop.
    ///
    /// Both fixtures below are [`PageOrder::AsGiven`], which is the point:
    /// this could not be written at all until the fake stopped sorting every
    /// page it served, which is why a milestone of green tests never saw it.
    #[tokio::test]
    async fn a_page_whose_row_0_is_not_its_max_does_not_deadlock_the_source() {
        let types = vec![build_type("Payout_Build", "Payout")];
        let rest = FakeRest::new(
            types.clone(),
            vec![
                build(6_518_363, "Payout_Build", "Payout", "finished"),
                build(6_518_362, "Payout_Build", "Payout", "finished"),
                build(6_466_333, "Payout_Build", "Payout", "finished"),
                build(6_520_204, "Payout_Build", "Payout", "finished"),
            ],
        )
        .fixture_order();
        let cfg = TeamCityConfig::default();
        let (_, first) = run(&rest, &cfg, None).await;
        assert_eq!(
            first, r#"{"v":1,"since_build_id":6520204}"#,
            "the ceiling is the highest id the page witnessed, not the id that happened to be \
             printed first"
        );

        // The same server one moment later: a newer build, and a page whose
        // row 0 has dropped below both the previous watermark and the new
        // build. Reading row 0 as the newest build in existence refuses here.
        let rest = FakeRest::new(
            types,
            vec![
                build(6_466_438, "Payout_Build", "Payout", "finished"),
                build(6_520_300, "Payout_Build", "Payout", "finished"),
                build(6_518_363, "Payout_Build", "Payout", "finished"),
                build(6_518_362, "Payout_Build", "Payout", "finished"),
                build(6_466_333, "Payout_Build", "Payout", "finished"),
                build(6_520_204, "Payout_Build", "Payout", "finished"),
            ],
        )
        .fixture_order();
        let (items, second) = run(&rest, &cfg, Some(first)).await;
        assert_eq!(
            keys(&items),
            ["buildType:Payout_Build", "build:6520300"],
            "the run carries on and emits the new build rather than refusing"
        );
        assert_eq!(second, r#"{"v":1,"since_build_id":6520300}"#);
    }

    /// The ordering guard is deleted, and this is the case it used to refuse.
    ///
    /// It read rows 0 and 1 of the opening page and failed the run if they
    /// ascended, because the ceiling was row 0 and row 0 had to be the newest
    /// build. The live instance then showed the guard passing on a page that
    /// was not ordered at all -- `6518363, 6518362, 6466333, 6466438,
    /// 6471105, ...`, two descending rows and then an ascending run -- so it
    /// certified exactly the assumption it existed to falsify, and there was
    /// nothing left for it to protect once the ceiling became the page's
    /// maximum (issue #91).
    ///
    /// What replaces it is this: the same fixture in the opposite order syncs
    /// to the same items and the same watermark. A maximum is the same number
    /// under every ordering, which is the property the guard was standing in
    /// for.
    #[tokio::test]
    async fn an_oldest_first_server_syncs_rather_than_being_refused() {
        let cfg = TeamCityConfig::default();
        let (newest_first, expected_cursor) = run(&tidewater(), &cfg, None).await;
        assert_eq!(expected_cursor, r#"{"v":1,"since_build_id":1187}"#);

        let rest = tidewater().ascending();
        let (items, cursor) = run(&rest, &cfg, None).await;
        assert_eq!(
            keys(&items),
            keys(&newest_first),
            "the same builds, a page in the other order"
        );
        assert_eq!(cursor, expected_cursor, "...and the same watermark");
        assert!(
            !rest.calls().iter().any(|c| c.starts_with("builds/id:")),
            "an ordinary run asks nothing by id: {:?}",
            rest.calls()
        );
    }

    /// The ceiling is the **maximum** over **both** opening pages, asserted
    /// on the function rather than through a run.
    ///
    /// Both halves are load-bearing and neither is visible from a cursor
    /// value alone. Reading row 0 is the defect issue #91 records, using the
    /// live page's own numbers. Dropping the in-flight page would leave the
    /// ceiling at whatever the single un-widened probe page happened to
    /// carry, where the in-flight poll is the one page that is complete or an
    /// error ([`in_flight`]) and so the one whose maximum is a witness rather
    /// than a sample.
    #[test]
    fn the_ceiling_is_the_maximum_over_both_opening_pages() {
        let probe = recs(&[
            build(6_518_363, "Payout_Build", "Payout", "finished"),
            build(6_520_204, "Payout_Build", "Payout", "finished"),
            build(6_466_333, "Payout_Build", "Payout", "finished"),
        ]);
        let in_flight = recs(&[build(6_520_300, "Payout_Build", "Payout", "running")]);
        assert_eq!(
            ceiling(&probe, &[]),
            Some(6_520_204),
            "the maximum of the page, not the id printed in row 0"
        );
        assert_eq!(
            ceiling(&probe, &in_flight),
            Some(6_520_300),
            "a build queued after the in-flight poll has an id above everything in either page, \
             so both pages are witnesses"
        );
        assert_eq!(
            ceiling(&[], &in_flight),
            Some(6_520_300),
            "an empty probe page is not the end of the evidence"
        );
        assert_eq!(
            ceiling(&[], &[]),
            None,
            "a server with no builds at all names no ceiling"
        );
    }

    /// A server with a single build: the opening page has one row, and that
    /// row's id is the ceiling.
    ///
    /// Worth its own test because a one-row page is the degenerate input to
    /// every "maximum of the page" claim above, and because it is the shape
    /// the deleted ordering guard used to have to special-case.
    #[tokio::test]
    async fn a_server_with_a_single_build_syncs() {
        let rest = FakeRest::new(
            vec![build_type("Payout_Build", "Payout")],
            vec![build(412, "Payout_Build", "Payout", "finished")],
        );
        let (items, cursor) = run(&rest, &TeamCityConfig::default(), None).await;
        assert_eq!(keys(&items), ["buildType:Payout_Build", "build:412"]);
        assert_eq!(cursor, r#"{"v":1,"since_build_id":412}"#);
    }

    /// A server whose opening pages cannot reach the top of its own id space.
    ///
    /// A hundred old builds listed first and one recent build past the end of
    /// the page, so `defaultFilter:false,count:100` comes back with a maximum
    /// of 100 while build 5 000 is sitting on the server. That is not a
    /// contrivance: `/app/rest/builds` is unordered and one page is not the
    /// id space, which is precisely why "nothing this run witnessed reaches
    /// the watermark" is a trigger to ask the server rather than a verdict
    /// about it.
    fn a_page_that_cannot_reach_the_top(top: Option<i64>) -> FakeRest {
        let mut builds: Vec<serde_json::Value> = (1..=100)
            .map(|n| build(n, "Payout_Build", "Payout", "finished"))
            .collect();
        if let Some(id) = top {
            builds.push(build(id, "Payout_Build", "Payout", "finished"));
        }
        FakeRest::new(vec![build_type("Payout_Build", "Payout")], builds).fixture_order()
    }

    /// The replaced server, still caught -- on the server's own answer about
    /// the one build the question is about.
    ///
    /// The watermark is a build this source has already synced. Ids are
    /// monotonic and never reused and TeamCity's cleanup removes the oldest
    /// builds, so on the server that issued it that build is still there.
    /// Build 5 000 is *not* there, and `/app/rest/builds/id:5000` says so:
    /// the source was repointed at another instance, or one was restored from
    /// a state older than the watermark. The run fails before spending the
    /// per-configuration budget, and the cursor stays where it is.
    #[tokio::test]
    async fn a_watermark_whose_build_is_gone_is_refused_as_a_replaced_server() {
        let rest = a_page_that_cannot_reach_the_top(None);
        let mut sink = VecSink(Vec::new());
        let err = execute(
            "teamcity",
            &TeamCityConfig::default(),
            &rest,
            Some(r#"{"v":1,"since_build_id":5000}"#.to_owned()),
            &mut sink,
        )
        .await
        .expect_err("a server missing a build this source synced must not be trusted");
        assert!(
            matches!(&err, SourceError::Protocol { message: m, .. }
                if m.contains("watermark is build 5000")
                    && m.contains("`/app/rest/builds/id:5000` answers 404")
                    && m.contains("points at a different instance")
                    && m.contains("reset the source's cursor")),
            "the message must name the build it asked about, the answer it got and the remedy: \
             {err:?}"
        );
        assert!(sink.0.is_empty(), "nothing is emitted from a refused run");
        assert_eq!(
            rest.calls(),
            [
                "defaultFilter:false,count:100",
                "state:(queued:true,running:true),count:100",
                "builds/id:5000",
            ],
            "it asks the server about the watermark and refuses there, before the \
             per-configuration walk"
        );
    }

    /// ...and the case that is *not* a replaced server, which is what makes
    /// the refusal above a check rather than a blanket failure.
    ///
    /// The same hundred-build page, the same watermark, the same "nothing I
    /// witnessed reaches it" trigger -- and build 5 000 is on the server, so
    /// `/app/rest/builds/id:5000` finds it and the run carries on. This is
    /// the ordinary case on an unordered server, and it is the one the
    /// refusal this replaces got wrong on every run: issue #91's source
    /// refused for ever, and resetting the cursor only restarted the loop.
    ///
    /// The watermark stands rather than being dragged down to the ceiling:
    /// [`cursor::advance`] never regresses, so a ceiling below the watermark
    /// clamps nothing.
    #[tokio::test]
    async fn a_watermark_the_pages_did_not_show_is_confirmed_by_id_and_the_run_proceeds() {
        let rest = a_page_that_cannot_reach_the_top(Some(5_000));
        let cursor_in = r#"{"v":1,"since_build_id":5000}"#.to_owned();
        let (items, cursor) = run(&rest, &TeamCityConfig::default(), Some(cursor_in.clone())).await;
        assert!(items.is_empty(), "nothing has finished since build 5000");
        assert_eq!(
            cursor, cursor_in,
            "the watermark stands: the pages did not show build 5000, the server did"
        );
        assert!(
            rest.calls().contains(&"builds/id:5000".to_owned()),
            "the run asked, rather than inferring a replacement from a page: {:?}",
            rest.calls()
        );
    }

    /// An ordinary run never asks by id.
    ///
    /// The by-id fetch is one request per run in the case it fires, and it
    /// fires only when nothing the run witnessed reaches the watermark. A
    /// version of this check that asked every run would be a request per
    /// source per poll, for ever, to confirm something that is almost always
    /// visible in the pages already fetched.
    #[tokio::test]
    async fn a_run_whose_pages_reach_the_watermark_asks_nothing_by_id() {
        let rest = tidewater();
        let cfg = TeamCityConfig::default();
        let (_, first) = run(&rest, &cfg, None).await;
        let (_, _) = run(&rest, &cfg, Some(first)).await;
        assert!(
            !rest.calls().iter().any(|c| c.starts_with("builds/id:")),
            "{:?}",
            rest.calls()
        );
    }

    /// The loss class the opening in-flight poll **traded** rather than
    /// closed, and the ceiling closes.
    ///
    /// Build 1100 is queued after the poll, so the poll cannot have seen it,
    /// and it is still running when the finished query goes out, so that query
    /// cannot see it either. It is in neither set: nothing emits it and
    /// nothing clamps below it. Build 1200 was queued later still and finished
    /// inside the same run, which is what pushes `max_finished` to 1200 -- and
    /// a watermark at 1200 puts 1100 permanently out of `sinceBuild`'s reach,
    /// with a full sync being a window rather than the corpus, so not even a
    /// cursor reset recovers it.
    ///
    /// The ceiling is the highest build id in existence when the run started,
    /// 1000 here. Everything queued afterwards has an id above it, so a
    /// watermark that may not pass it cannot skip any of them. The cost is
    /// re-fetching 1200 next run, and upserts are idempotent.
    ///
    /// 1200 is what makes this fixture able to fail: without a build that both
    /// appeared and finished inside the run, `max_finished` would never rise
    /// above the ceiling and a run with no ceiling at all would land on the
    /// same number.
    #[tokio::test]
    async fn a_build_queued_after_the_opening_poll_is_not_skipped() {
        let rest = MidRun::new(
            vec![build_type("Payout_Build", "Payout")],
            vec![build(1000, "Payout_Build", "Payout", "finished")],
            vec![
                build(1000, "Payout_Build", "Payout", "finished"),
                build(1100, "Payout_Build", "Payout", "running"),
                build(1200, "Payout_Build", "Payout", "finished"),
            ],
        );
        let mut sink = VecSink(Vec::new());
        let cursor = execute(
            "teamcity",
            &TeamCityConfig::default(),
            &rest,
            Some(r#"{"v":1,"since_build_id":900}"#.to_owned()),
            &mut sink,
        )
        .await
        .expect("run");
        assert_eq!(
            cursor, r#"{"v":1,"since_build_id":1000}"#,
            "1000 was the newest build in existence when the run opened; 1100 was queued after \
             the poll and is still running, so the watermark may not pass 1000 or `sinceBuild` \
             will never offer 1100 again"
        );
    }

    /// The ceiling counts builds that are **in flight** at run start, not only
    /// finished ones.
    ///
    /// Build 1100 belongs to a foreign configuration and is running when the
    /// run opens, so it does not clamp: a scoped source must not be held below
    /// a build it will never emit ([`cursor::advance`]'s asymmetry). It
    /// finishes during the run and turns up in the global finished query, and
    /// a foreign *finished* build is exactly the case the watermark must
    /// advance past, or the incremental query re-offers it on every poll for
    /// good. The ceiling is what lets it: 1100 existed when the run opened, so
    /// the watermark is free to reach it.
    ///
    /// Both opening pages witness 1100 here -- the probe because
    /// `defaultFilter:false` does not hide a running build, the in-flight poll
    /// because that is its whole subject. What `defaultFilter:false` buys
    /// beyond the poll is
    /// `the_ceiling_counts_a_build_the_default_filter_would_hide`.
    #[tokio::test]
    async fn the_ceiling_counts_the_builds_in_flight_at_run_start() {
        let types = vec![
            build_type("Payout_Build", "Payout"),
            build_type("Ledger_Deploy_Staging", "Ledger"),
        ];
        let rest = MidRun::new(
            types,
            vec![
                build(1000, "Payout_Build", "Payout", "finished"),
                build(1100, "Ledger_Deploy_Staging", "Ledger", "running"),
            ],
            vec![
                build(1000, "Payout_Build", "Payout", "finished"),
                build(1100, "Ledger_Deploy_Staging", "Ledger", "finished"),
            ],
        );
        let cfg = TeamCityConfig::from_json(&serde_json::json!({ "project_ids": ["Payout"] }))
            .expect("config");
        let mut sink = VecSink(Vec::new());
        let cursor = execute(
            "teamcity",
            &cfg,
            &rest,
            Some(r#"{"v":1,"since_build_id":900}"#.to_owned()),
            &mut sink,
        )
        .await
        .expect("run");
        assert_eq!(
            keys(&sink.0),
            ["buildType:Payout_Build", "build:1000"],
            "1100 is foreign and is not emitted"
        );
        assert_eq!(
            cursor, r#"{"v":1,"since_build_id":1100}"#,
            "1100 existed when the run opened, so the ceiling is 1100 and the foreign build it \
             saw finish is free to move the watermark"
        );
    }

    /// What `defaultFilter:false` on the opening probe buys that the
    /// in-flight poll does not, and the whole reason the dimension is there.
    ///
    /// Build 1100 is **canceled**: TeamCity reports one as finished with
    /// `status: "UNKNOWN"` and `statusText: "Canceled"` -- 20 of 20 sampled
    /// on the live instance -- and its default filter hides it. It is not in
    /// flight, so the in-flight poll never sees it either. A probe without
    /// the override therefore witnesses 1000, the ceiling clamps the
    /// watermark there, and every later run re-reads build 1100.
    ///
    /// **Issue #105 sharpened this rather than retiring it.** The
    /// item-producing queries now send `canceled:any`, so build 1100 really is
    /// emitted, and a ceiling of 1000 would hold the watermark below a build
    /// the run just mirrored -- re-reading it on every run for ever instead of
    /// merely losing a margin. The probe keeps `defaultFilter:false` and not
    /// the two named dimensions because it asks a *stateless* question about
    /// the id space: personal builds count toward the ceiling although they
    /// are never mirrored, and the highest id on JetBrains' public instance
    /// belonged to `JetBrainsPublicProjects_Compose_AllPersonalBuild` -- a
    /// personal build configuration.
    #[tokio::test]
    async fn the_ceiling_counts_a_build_the_default_filter_would_hide() {
        let rest = FakeRest::new(
            vec![build_type("Payout_Build", "Payout")],
            vec![
                build(1_000, "Payout_Build", "Payout", "finished"),
                canceled_build(1_100, "Payout_Build", "Payout"),
            ],
        );
        let (_, cursor) = run(&rest, &TeamCityConfig::default(), None).await;
        assert_eq!(
            cursor, r#"{"v":1,"since_build_id":1100}"#,
            "1100 is canceled and the default filter hides it; a probe that took the default \
             would witness 1000 and pin the watermark there"
        );
    }

    /// **The argument the whole of issue #105 rests on**: a build the mirror
    /// has already shown as running, then canceled on the server, must not say
    /// "running" for ever.
    ///
    /// This is the reason "just exclude canceled builds" is not the clean
    /// option it looks like. [`execute`] step 5 emits every observed in-scope
    /// build, in-flight ones included, and [`cursor::advance`] holds the
    /// watermark under them -- so a running build is mirrored *before* anyone
    /// knows how it ends. Cancel it, and with the two item-producing queries
    /// taking TeamCity's default filter the finished ones can never return it,
    /// the in-flight poll stops returning it, and `map::build_item` records
    /// that "M1 has no deletion channel: the adapter never sees the removal".
    /// The row stays `running` until somebody notices by hand. TeamCity's own
    /// UI un-hides a canceled build with one click; a mirror that never stored
    /// it cannot.
    ///
    /// Two runs over the same server at two moments, which is the only way to
    /// witness it: one fixture with the build running, one with it canceled.
    #[tokio::test]
    async fn a_build_canceled_after_it_was_mirrored_as_running_does_not_stay_running() {
        let types = vec![build_type("Payout_Build", "Payout")];
        let while_running = FakeRest::new(
            types.clone(),
            vec![
                build(400, "Payout_Build", "Payout", "finished"),
                build(500, "Payout_Build", "Payout", "running"),
            ],
        );
        let cfg = TeamCityConfig::default();
        let (first, cursor) = run(&while_running, &cfg, None).await;
        let running = first
            .iter()
            .find(|i| i.entity.key == "build:500")
            .expect("the running build is mirrored before anyone knows how it ends");
        assert!(running.body_text.contains("running"), "{running:?}");
        assert_eq!(
            cursor, r#"{"v":1,"since_build_id":400}"#,
            "the watermark is clamped under the running build"
        );

        // Somebody cancels it.
        let after_cancel = FakeRest::new(
            types,
            vec![
                build(400, "Payout_Build", "Payout", "finished"),
                canceled_build(500, "Payout_Build", "Payout"),
            ],
        );
        let (second, cursor) = run(&after_cancel, &cfg, Some(cursor)).await;
        let healed = second
            .iter()
            .find(|i| i.entity.key == "build:500")
            .unwrap_or_else(|| {
                panic!(
                    "the canceled build must be re-emitted so the mirror stops saying \"running\"; \
                     this run emitted {:?}",
                    keys(&second)
                )
            });
        assert!(
            healed.body_text.contains("finished canceled"),
            "and it must say what actually happened to it: {:?}",
            healed.body_text
        );
        assert!(
            !healed.body_text.contains("running"),
            "the stale state is gone, not merely joined: {:?}",
            healed.body_text
        );
        assert_eq!(
            cursor, r#"{"v":1,"since_build_id":500}"#,
            "and nothing holds the watermark back any more"
        );
    }

    /// The same permanence for the other class the default filter hides: a
    /// queued build the in-flight poll mirrored can terminate as
    /// **failed-to-start**, and that row would say "queued" for ever.
    ///
    /// Ruling canceled builds in while leaving failed-to-start out would leave
    /// exactly the same stale row behind, which is why the two dimensions
    /// travel together.
    #[tokio::test]
    async fn a_build_that_failed_to_start_after_it_was_mirrored_as_queued_does_not_stay_queued() {
        let types = vec![build_type("Payout_Build", "Payout")];
        let cfg = TeamCityConfig::default();
        let (first, cursor) = run(
            &FakeRest::new(
                types.clone(),
                vec![
                    build(400, "Payout_Build", "Payout", "finished"),
                    build(500, "Payout_Build", "Payout", "queued"),
                ],
            ),
            &cfg,
            None,
        )
        .await;
        assert!(
            first
                .iter()
                .find(|i| i.entity.key == "build:500")
                .expect("the queued build is mirrored")
                .body_text
                .contains("queued")
        );

        let (second, _) = run(
            &FakeRest::new(
                types,
                vec![
                    build(400, "Payout_Build", "Payout", "finished"),
                    failed_to_start_build(500, "Payout_Build", "Payout"),
                ],
            ),
            &cfg,
            Some(cursor),
        )
        .await;
        let healed = second
            .iter()
            .find(|i| i.entity.key == "build:500")
            .unwrap_or_else(|| {
                panic!(
                    "a build that failed to start must be re-emitted, or its row says \"queued\" \
                     for ever; this run emitted {:?}",
                    keys(&second)
                )
            });
        assert!(
            healed.body_text.contains("finished FAILURE"),
            "a failed-to-start build is a FAILURE and needs no new wording: {:?}",
            healed.body_text
        );
        assert!(
            healed.body_text.contains("Failed to start"),
            "TeamCity's own statusText is what says which kind of failure: {:?}",
            healed.body_text
        );
    }

    /// A full sync mirrors both hidden classes too, not only an incremental
    /// run: the per-configuration query is the other item-producing locator,
    /// and it carries the same two dimensions.
    ///
    /// Without them a first-ever sync of a configuration silently indexes less
    /// than the configuration holds, which is ADR-0003's class -- and
    /// `sinceBuild` is exclusive, so the watermark then moves past the builds
    /// it skipped and no later run can offer them.
    #[tokio::test]
    async fn a_full_sync_mirrors_the_canceled_and_failed_to_start_builds_in_a_configuration() {
        let rest = FakeRest::new(
            vec![build_type("Payout_Build", "Payout")],
            vec![
                build(400, "Payout_Build", "Payout", "finished"),
                canceled_build(401, "Payout_Build", "Payout"),
                failed_to_start_build(402, "Payout_Build", "Payout"),
            ],
        );
        let (items, cursor) = run(&rest, &TeamCityConfig::default(), None).await;
        assert_eq!(
            keys(&items),
            [
                "buildType:Payout_Build",
                "build:400",
                "build:401",
                "build:402"
            ],
            "every finished build in the configuration, whichever way it ended"
        );
        assert_eq!(cursor, r#"{"v":1,"since_build_id":402}"#);
        assert!(
            items
                .iter()
                .any(|i| i.body_text.contains("finished canceled")),
            "{items:?}"
        );
    }

    /// **The fixture's own failed build was never mirrored on a real server**
    /// (issue #266): 1187 ran on `feature/PAY-231-sepa-retry`, and the
    /// per-configuration query took TeamCity's default filter, which narrows
    /// a branched configuration to its default branch. Measured on the seeded
    /// TeamCity 2026.1.3 in `testenv` on 2026-09-02, the adapter's own locator
    /// answered `count: 0` for that configuration, and `branch:default:any`
    /// answered the build. Every fake here served it regardless, which is how
    /// the gap survived every docker-free suite.
    ///
    /// The other direction is pinned too: the fake hides the build from the
    /// locator without the dimension, so this test is red against an adapter
    /// that does not send it -- and the wire string is asserted, because a
    /// fake that opened the facet for free would let the dimension disappear
    /// again in silence.
    #[tokio::test]
    async fn a_full_sync_mirrors_a_feature_branch_build_in_a_configuration() {
        let rest = FakeRest::new(
            vec![build_type("Payout_IntegrationTests", "Payout")],
            vec![
                build(400, "Payout_IntegrationTests", "Payout", "finished"),
                feature_branch_build(1187, "Payout_IntegrationTests", "Payout", "finished"),
            ],
        );
        let (items, cursor) = run(&rest, &TeamCityConfig::default(), None).await;
        assert_eq!(
            keys(&items),
            ["buildType:Payout_IntegrationTests", "build:400", "build:1187"],
            "a finished build on a feature branch is mirrored like one on the default branch"
        );
        assert_eq!(cursor, r#"{"v":1,"since_build_id":1187}"#);
        assert!(
            rest.calls().iter().any(|c| c.starts_with("buildType:(id:Payout_IntegrationTests)")
                && c.contains("branch:default:any")),
            "the per-configuration query carries branch:default:any: {:?}",
            rest.calls()
        );
    }

    /// The same facet on the **incremental** query, which is the one that
    /// would have lost the build for good: `sinceBuild` is exclusive, so a
    /// feature-branch build that finished while the watermark moved over it
    /// was missing permanently rather than late.
    #[tokio::test]
    async fn an_incremental_run_mirrors_a_feature_branch_build_that_finished() {
        let cfg = TeamCityConfig::default();
        let before = FakeRest::new(
            vec![build_type("Payout_IntegrationTests", "Payout")],
            vec![build(400, "Payout_IntegrationTests", "Payout", "finished")],
        );
        let (_, cursor) = run(&before, &cfg, None).await;
        assert_eq!(cursor, r#"{"v":1,"since_build_id":400}"#);

        let after = FakeRest::new(
            vec![build_type("Payout_IntegrationTests", "Payout")],
            vec![
                build(400, "Payout_IntegrationTests", "Payout", "finished"),
                feature_branch_build(401, "Payout_IntegrationTests", "Payout", "finished"),
            ],
        );
        let (items, moved) = run(&after, &cfg, Some(cursor)).await;
        assert_eq!(
            keys(&items),
            ["buildType:Payout_IntegrationTests", "build:401"],
            "the feature-branch build that finished since the last run"
        );
        assert_eq!(moved, r#"{"v":1,"since_build_id":401}"#);
        assert!(
            after
                .calls()
                .iter()
                .any(|c| c.contains("sinceBuild:(id:400)") && c.contains("branch:default:any")),
            "the incremental query carries branch:default:any: {:?}",
            after.calls()
        );
    }

    /// The in-flight poll needs no branch dimension, and sends none: measured
    /// on the seeded server, `state:(queued:true,running:true)` answered a
    /// queued and then a running build on a non-default branch without being
    /// asked. The fake reproduces that, so this pins both halves -- the
    /// running feature-branch build is mirrored and clamps the watermark, and
    /// the poll's wire string is unchanged.
    #[tokio::test]
    async fn a_running_feature_branch_build_is_seen_by_the_in_flight_poll_unasked() {
        let rest = FakeRest::new(
            vec![build_type("Payout_Build", "Payout")],
            vec![
                build(400, "Payout_Build", "Payout", "finished"),
                feature_branch_build(1188, "Payout_Build", "Payout", "running"),
            ],
        );
        let (items, cursor) = run(&rest, &TeamCityConfig::default(), None).await;
        assert_eq!(keys(&items), ["buildType:Payout_Build", "build:400", "build:1188"]);
        assert_eq!(
            cursor, r#"{"v":1,"since_build_id":400}"#,
            "the running build holds the watermark below itself"
        );
        let poll = rest
            .calls()
            .into_iter()
            .find(|c| c.contains("queued:true"))
            .expect("the in-flight poll");
        assert!(!poll.contains("branch:"), "{poll}");
    }

    /// **Personal builds stay out**, and this is what says the widening was
    /// `canceled:any,failedToStart:any` rather than `defaultFilter:false`.
    ///
    /// The two named dimensions re-open exactly their own facets;
    /// `defaultFilter:false` disables the personal facet as well -- and every
    /// facet nobody has enumerated. Their absence from the mirror is
    /// consistent, in *every* state: the personal facet applies to the
    /// in-flight poll too, so no personal build is ever mirrored and no row of
    /// one can go stale. Nobody has decided that a work cockpit should mirror
    /// other people's experiments.
    ///
    /// The opening probe still *witnesses* them, which is deliberate and is
    /// the reason it alone keeps `defaultFilter:false`: on JetBrains' public
    /// instance the highest id in the whole id space belonged to a personal
    /// build configuration, so a ceiling blind to them is a ceiling too low.
    /// Witnessing is not mirroring -- the probe's page is read for one number
    /// and never mapped to an item, and
    /// [`the_ceiling_counts_a_build_the_default_filter_would_hide`] is where
    /// the ceiling half is asserted.
    #[tokio::test]
    async fn a_personal_build_is_witnessed_by_the_ceiling_and_never_mirrored() {
        let rest = FakeRest::new(
            vec![build_type("Payout_Build", "Payout")],
            vec![
                build(400, "Payout_Build", "Payout", "finished"),
                personal_build(700, "Payout_Build", "Payout", "finished"),
                personal_build(701, "Payout_Build", "Payout", "running"),
            ],
        );
        let (items, cursor) = run(&rest, &TeamCityConfig::default(), None).await;
        assert_eq!(
            keys(&items),
            ["buildType:Payout_Build", "build:400"],
            "neither personal build reaches the mirror, finished or running"
        );
        assert_eq!(
            cursor, r#"{"v":1,"since_build_id":400}"#,
            "and nothing about a build that is not in the mirror moves the watermark: 700 \
             finished, and the run neither emitted it nor counted it"
        );
    }

    /// The emission order is the run's own, asserted against the property
    /// rather than against another run.
    ///
    /// The server here answers newest-first (3, 2, 1) and the run must still
    /// emit oldest-first. A fixture whose ids arrive already ascending could
    /// not tell the two apart.
    ///
    /// It is asserted against the property rather than against a second run
    /// in the other order, which is a *different* claim and has its own test:
    /// `an_oldest_first_server_syncs_rather_than_being_refused` runs the same
    /// fixture both ways and compares them. That pairing was impossible while
    /// the ceiling refused an oldest-first server outright; the ordering guard
    /// is gone (issue #91), so it is available again and lives there.
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
