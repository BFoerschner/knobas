//! Suggestions: the links knobas proposes, and what happens when you answer.
//!
//! A **suggestion is a link row, not a second table** (#41). The ratified link
//! vocabulary already closes `origin` over five spellings and one link table is
//! a standing rule -- a parallel suggestions table would be a second graph that
//! can disagree with the first. What tells a suggestion from a link is
//! therefore its *state*: [`LinkRow::confirmed_at`](crate::link::LinkRow::confirmed_at) is `None` on a proposal and
//! `Some` on a link that is in the graph.
//!
//! ## The two reads cannot blur
//!
//! Migration `0007` cuts the table into `knobas.confirmed_link` and
//! `knobas.proposed_link` -- two views whose predicates are each other's
//! negation over the same rows. [`crate::link::entries_of`] reads the first and
//! [`proposals`] reads the second, so no edit to either can make the links
//! panel show a guess or the tray show a link. That is not a tidiness
//! preference: a links panel showing an unconfirmed guess is a correctness bug,
//! and a `where` clause spelled out in two readers is a clause one of them can
//! forget.
//!
//! ## Dismissal is the withdrawal memory, not a second mechanism
//!
//! Links v1 ratified that unlinking is remembered so "a later import or
//! suggestion pass cannot silently resurrect a link I removed" (#40, story 12),
//! and the memory is the tombstone: [`crate::link::unlink`] keeps the row and
//! sets `deleted_at`. [`dismiss`] does exactly the same thing to a proposal, so
//! **a dismissed suggestion and an unlinked link are one fact** as far as
//! detection is concerned -- and detection's suppression, which reads the pair
//! in both directions with no filter at all, cannot tell them apart even in
//! principle.
//!
//! ## Detection
//!
//! [`detect`] is a pass over the mirror -- no SPI change, no descriptor change,
//! no adapter capability. Each [`Rule`] is separate, named, independently
//! runnable ([`detect_rule`]) and carries the reason string it produces, which
//! is *stored*: a suggestion whose reason cannot be shown is not shippable, and
//! a reason re-rendered from a rule id at display time is one the next surface
//! can be missing.
//!
//! Every rule's statement is built from [`driver_head!`] and [`driver_tail!`],
//! so the suppression, the self-link guard and the undirected de-duplication
//! are written **once**. Mutating the `not exists` in the tail breaks every
//! idempotence and suppression test at the same time, which is the point.
//!
//! Nothing here ever writes a confirmed link, and nothing here is ever written
//! to a source.

use crate::CoreError;
use crate::link::Origin;

mod tray;
pub use tray::*;

crate::closed_vocabulary! {
    /// What *kind* of evidence a rule is -- the axis a user calibrates trust
    /// on.
    ///
    /// #41: "exact-key detection and similarity detection are different classes
    /// and must be distinguishable in the data, because a user's trust in them
    /// differs". Stored as lowercase text in `knobas.link.rule_class`, whose
    /// `link_rule_class_chk` (migration `0007`) allows exactly these spellings.
    ///
    /// The *rule* name beside it is deliberately unconstrained: rules are
    /// expected to grow and a new one must not cost a migration. The class is
    /// the closed axis because it is the one a surface branches on.
    #[derive(ts_rs::TS)]
    #[ts(export_to = "entity.ts")]
    pub enum RuleClass {
        /// The two ends name each other: an issue key found in text that
        /// belongs to the other end. Nothing is inferred.
        ExactKey => "exact_key",
        /// The two ends read alike. Speculative by construction, and labelled
        /// so.
        Similarity => "similarity",
        /// The source system already states the relation. Evidence, but still
        /// only a proposal: M2's rule is that nothing enters the graph
        /// unconfirmed.
        SourceRelation => "source_relation",
    }
}

/// Who a proposal belongs to, in `created_by`.
///
/// Not `"user"` -- the user did not draw it -- and not `"sync:<source_id>"`,
/// because detection is a pass over the mirror rather than something a source
/// did. Accepting does not rewrite it: "knobas proposed this and I said yes" is
/// the truth, and it is what `origin = suggested` already means.
pub const DETECTOR: &str = "knobas";

/// The head of every detection statement.
///
/// `confirmed_at` is written as an explicit `null` and not left to the column
/// default: **detection never creates a confirmed link** (#41), and saying so
/// in the statement is what makes that a property of the one place proposals
/// are written rather than of everyone remembering.
macro_rules! driver_head {
    () => {
        "insert into knobas.link
                (from_id, to_id, relation, origin, created_by,
                 confirmed_at, rule, rule_class, reason)
         select distinct on (least(c.from_id, c.to_id), greatest(c.from_id, c.to_id), c.relation)
                c.from_id, c.to_id, c.relation, $1, $2,
                null, $3, $4, c.reason
           from ("
    };
}

/// The tail of every detection statement: the suppression, and the two guards.
///
/// **The `not exists` is the whole of "dismissing persists".** It reads
/// `knobas.link` -- the table, not either view -- in both directions and with
/// no filter whatsoever, so one pair of clauses covers four separate promises:
///
/// * a pair that is already linked produces no suggestion (#41 story 17),
/// * running detection twice proposes each suggestion once (idempotence),
/// * a dismissed suggestion is never proposed again (story 7, 8),
/// * a link the user unlinked is never proposed back (story 9).
///
/// Undirected on purpose, and it was undirected *first*. The unique index used
/// to be directed while this was not (#70), because the *fact* "these two are
/// connected" is not a directed one and a detector that re-proposed `B -> A`
/// after the user removed `A -> B` would be the silent resurrection the
/// withdrawal memory exists to prevent. Migration `0011` made the index agree;
/// this clause did not change.
///
/// Two `not exists`, one per direction, each on the whole key -- not one with
/// the directions `or`ed (#770). The `or` left the planner one equality it
/// could hash on, `relation`, and with `knobas.link` analyzed while it held
/// few rows it did: a hash anti-join on `relation` alone, every candidate
/// compared with every link of its relation (6.5 million comparisons for 302
/// proposals in one batch on the probe). Split, either clause is a probe of
/// `link_pair_idx` or a hash on all three columns, whichever the statistics
/// favour, and both are linear.
///
/// `distinct on` in the head plus this `order by` de-duplicates *within* one
/// pass: `not exists` reads the snapshot the statement started from, so two
/// candidate rows describing one undirected pair would both survive it.
///
/// **What actually keeps the promise is this `on conflict do nothing`, and the
/// `distinct on` is belt to its braces.** This paragraph used to end "the index
/// refuses the second row, and refusing it is a failed statement, not a skipped
/// candidate", and that is wrong: the clause carries no conflict target, so the
/// unique violation `0011`'s index raises on the second row is swallowed rather
/// than raised, and the row `order by` puts first lands either way. Measured in
/// #451, and corrected here rather than left standing (ADR-0011, *verify, then
/// correct or file*): deleting the `distinct on` leaves every test in
/// `tests/it/suggestions.rs` green. It stays because it says in the statement
/// which of two duplicate candidates is meant to land, instead of leaving that
/// to a constraint firing behind it.
macro_rules! driver_tail {
    () => {
        ") as c(from_id, to_id, relation, reason)
          where c.from_id <> c.to_id
            and not exists (
                  select 1
                    from knobas.link l
                   where l.from_id = c.from_id and l.to_id = c.to_id
                     and l.relation = c.relation)
            and not exists (
                  select 1
                    from knobas.link l
                   where l.from_id = c.to_id and l.to_id = c.from_id
                     and l.relation = c.relation)
          order by least(c.from_id, c.to_id), greatest(c.from_id, c.to_id),
                   c.relation, c.from_id, c.reason
         on conflict do nothing"
    };
}

/// Wrap one rule's `select from_id, to_id, relation, reason` in the driver.
///
/// A macro rather than a runtime `format!` so every statement is a
/// `&'static str`: nothing in this crate builds SQL at run time, and the
/// suppression is compiled into all seven rather than appended by a call each of
/// them has to remember to make.
macro_rules! detection {
    ($($candidates:tt)*) => {
        concat!(driver_head!(), $($candidates)*, driver_tail!())
    };
}

/// The issue key an exact-key rule looks for, as a PostgreSQL regular
/// expression.
///
/// `\m` and `\M` are word boundaries, and they are what stops `PAY-231` from
/// matching inside `PAY-2311`: without them a rule fires for a superset of what
/// it means, which is the failure mode a negative control exists to catch.
/// Anchoring is all this pattern does -- what makes a match *real* is the join
/// against a ticket entity that actually carries that key, so an accidental
/// `UTF-8` costs nothing.
macro_rules! issue_key {
    () => {
        "'\\m[A-Z][A-Z0-9]+-[0-9]+\\M'"
    };
}

/// One named detection rule.
///
/// Separate, named and independently testable (#41). `sql` is the whole
/// statement, already carrying the driver, and takes four binds: the origin,
/// `created_by`, the rule name and the rule class -- in that order. A
/// similarity rule takes a fifth, the batch of item ids it
/// compares.
pub struct Rule {
    /// Stored in `knobas.link.rule`. Stable: it is what a test names and what
    /// a later reader uses to find out where a proposal came from.
    pub name: &'static str,
    /// Which class of evidence this rule is.
    pub class: RuleClass,
    /// The origin the resulting *link* carries once accepted.
    ///
    /// Provenance, not state. A relation Jira already states genuinely came
    /// from the source, so [`Origin::Source`] is the honest label for it; the
    /// fact that nobody has confirmed it yet is `confirmed_at`, which is a
    /// different axis. The ratified reading of `Origin::Suggested` -- "proposed
    /// by knobas **and confirmed by the user**" -- holds because a proposal is
    /// not in the graph: every `suggested` row a reader can see through
    /// `knobas.confirmed_link` was confirmed.
    pub origin: Origin,
    /// The complete statement. Private so a caller cannot run half of one.
    sql: &'static str,
    /// What the statement reads on each pass.
    scope: Scope,
}

/// That the item `$alias` is live, as `sync.live_item` means it -- the entity
/// not withdrawn, the source not turned off -- for a statement that reads
/// `sync.item` itself (#770).
///
/// Two correlated subqueries in the `where`, #768's shape, which the planner
/// cannot turn into joins and so cannot drive a plan from. Through the view's
/// joins, on tables never analyzed, `knobas.entity` is estimated at 84 rows
/// for `deleted_at is null` (22,000 on the probe), and the planner drove every
/// exact-key rule from it -- see [`ticket_keyed!`].
macro_rules! live {
    ($alias:literal) => {
        $crate::live_by_key!($alias)
    };
}

/// The namespaces a live ticket has, for [`ticket_keyed!`]: one per ticket
/// source, read once per statement.
///
/// An entity id is `<namespace>:<key>` split on the **first** colon, so the
/// namespace is `split_part(id, ':', 1)` -- it holds no colon -- and the key
/// after it may hold more.
macro_rules! ticket_namespaces {
    () => {
        "ticket_ns as materialized (
             select distinct split_part(id, ':', 1) as ns
               from knobas.entity
              where kind = 'ticket' and deleted_at is null
         )"
    };
}

/// The live ticket whose key is `k.key`, under any namespace: a lateral
/// joined after the keys a rule found, probing `knobas.entity`'s primary key
/// with `id = any(...)`: `<namespace>:<key>` for each namespace in
/// [`ticket_namespaces!`].
///
/// A key found in text names no namespace, so any Jira's `PAY-231` matches.
/// Until #770 the rules joined the key to `substr(id, position(':' in id) +
/// 1)` over every live ticket, and every rule's item through
/// `sync.live_item`, and the planner was free to drive that from either
/// side. On tables never analyzed it estimated one ticket (15,000 on the
/// probe) and 84 live entities (22,000), and drove from the tickets: every
/// ticket against every entity, which ran past the pass's two-minute
/// statement limit in the first rule, so the pass ended there and proposed
/// nothing at all. Here the lookup is a lateral the `offset 0` keeps from
/// being flattened into a join -- flattened, the planner drove it from the
/// tickets again, the `= any` a filter on every ticket-item pair -- and its
/// `= any(array)` on the primary key is an index probe per key found. There is
/// no ticket side to drive from: the cost is one walk of the rule's kind and
/// a probe per key, statistics or none (`tests/it/suggest_plans.rs`).
macro_rules! ticket_keyed {
    () => {
        "cross join lateral (
               select t.id
                 from knobas.entity t
                where t.id = any(array(select n.ns || ':' || k.key from ticket_ns n))
                  and t.kind = 'ticket' and t.deleted_at is null
               offset 0) t"
    };
}

/// An issue key in a branch name -- the most common connection in this work.
const BRANCH_NAME_KEY: &str = detection!(concat!(
    "with ",
    ticket_namespaces!(),
    " select b.entity_id, t.id, 'related',
             format('the branch name contains %s', k.key)
        from sync.item b
        cross join lateral (
               select distinct m[1] as key
                 from regexp_matches(b.title, ",
    issue_key!(),
    ", 'g') m) k
        ",
    ticket_keyed!(),
    "
       where b.kind = 'branch' and ",
    live!("b")
));

/// An issue key in a commit message.
///
/// Title *and* body, because an adapter puts the subject in one and the rest of
/// the message in the other, and a key written in the trailer is as much a
/// statement of connection as one in the subject.
const COMMIT_MESSAGE_KEY: &str = detection!(concat!(
    "with ",
    ticket_namespaces!(),
    " select i.entity_id, t.id, 'related',
             format('the commit message mentions %s', k.key)
        from sync.item i
        cross join lateral (
               select distinct m[1] as key
                 from regexp_matches(i.title || ' ' || i.body_text, ",
    issue_key!(),
    ", 'g') m) k
        ",
    ticket_keyed!(),
    "
       where i.kind = 'commit' and ",
    live!("i")
));

/// An issue key in a build's parameters.
///
/// "Parameters" is the mirrored build record: the branch it ran on, its status
/// text, what triggered it. That is what an adapter puts in `body_text` (see
/// `knobas_source_teamcity::map::build_item`), and reading the blob rather than
/// a vendor-shaped `payload` path is what keeps this rule true for the next
/// build source as well.
const BUILD_PARAMETER_KEY: &str = detection!(concat!(
    "with ",
    ticket_namespaces!(),
    " select i.entity_id, t.id, 'related',
             format('this build''s parameters name %s', k.key)
        from sync.item i
        cross join lateral (
               select distinct m[1] as key
                 from regexp_matches(i.title || ' ' || i.body_text, ",
    issue_key!(),
    ", 'g') m) k
        ",
    ticket_keyed!(),
    "
       where i.kind = 'build' and ",
    live!("i")
));

/// An issue key in a page's text -- documentation connecting to what it
/// documents.
const PAGE_TEXT_KEY: &str = detection!(concat!(
    "with ",
    ticket_namespaces!(),
    " select i.entity_id, t.id, 'related',
             format('this page mentions %s', k.key)
        from sync.item i
        cross join lateral (
               select distinct m[1] as key
                 from regexp_matches(i.title || ' ' || i.body_text, ",
    issue_key!(),
    ", 'g') m) k
        ",
    ticket_keyed!(),
    "
       where i.kind = 'page' and ",
    live!("i")
));

/// A relation the source system already states.
///
/// Jira reports these on the issue as `fields.issuelinks`, each naming a type
/// and exactly one of `outwardIssue` / `inwardIssue`. The **outward phrase is
/// always the relation** and the direction is what changes: `A` with an
/// `outwardIssue` `B` under "blocks" is `A -> B`, and `A` with an `inwardIssue`
/// `B` under the same type is `B -> A`. Storing the inward phrase instead would
/// give one relationship two spellings and two panel groups.
///
/// A source-shaped read outside an adapter (ADR-0007): the `jsonb_typeof`
/// guard below is the miss -- an absent or differently-shaped `issuelinks`
/// contributes nothing rather than guessing.
///
/// The other end is addressed in the **same source** (`source_id || ':' ||
/// key`): two Jiras are two namespaces, and matching by bare key across them
/// would join `jira:PAY-1` to `jira-eu:PAY-1`.
///
/// It is still only a proposal. A relation Jira states is evidence, but M2's
/// rule is that nothing enters the graph unconfirmed.
const SOURCE_RECORDED_RELATION: &str = detection!(concat!(
    "select case when side.outward then i.entity_id else other.id end,
            case when side.outward then other.id else i.entity_id end,
            lower(side.relation),
            format('%s already records this link (%s)', i.source_id, side.label)
       from sync.item i
       cross join lateral jsonb_array_elements(i.payload->'fields'->'issuelinks') as l
       cross join lateral (
              select coalesce(l->'outwardIssue'->>'key', l->'inwardIssue'->>'key') as other_key,
                     (l->'outwardIssue') is not null                               as outward,
                     nullif(trim(coalesce(l->'type'->>'outward', l->'type'->>'name', '')), '')
                                                                                   as relation,
                     coalesce(l->'type'->>'name', 'link')                          as label) side
       cross join lateral (
              select o.id
                from knobas.entity o
               where o.id = i.source_id || ':' || side.other_key
                 and o.deleted_at is null) other
      where jsonb_typeof(i.payload->'fields'->'issuelinks') = 'array'
        and side.other_key is not null
        and side.relation is not null
        and ",
    live!("i")
));

/// The host an address names, lowercased, or `null` if it names none.
///
/// One POSIX regular expression, and every piece of it is load-bearing:
///
/// * `(?:[A-Za-z][A-Za-z0-9+.-]*://)?` -- an optional scheme, because a route's
///   `url` "carries a scheme" (`0018`) but a monitor's may be a bare
///   `host:port` and an asset's hostname property is a bare host. Optional
///   rather than required, so all three go through one reader.
/// * `(?:[^@/]*@)?` -- userinfo, dropped. `https://kuma:secret@host/` is a
///   monitor pointed at `host`, and a credential is never a hostname.
/// * `([^:/?#]+)` -- the host, stopping at the **port**, the path, the query
///   and the fragment. The port is what makes `http://gitea:3000` match an
///   asset whose hostname is `gitea`, which is the whole of criterion 1.
///
/// Lowercased because hostnames are case-insensitive; `btrim` because a
/// hostname property is typed by hand into a form and ` gitea ` is the same
/// host as `gitea`; `nullif(..., '')` because a blank property and an absent
/// one are the same fact and neither may join to the other -- which is also
/// the whole of "an address with no host joins to nothing", since SQL equality
/// on `null` is `null`.
///
/// An IPv6 literal in brackets comes back as `[2001` and therefore matches
/// nothing rather than matching wrongly -- the failure direction ADR-0007 asks
/// of a read like this one, and no address in the estate this milestone
/// describes is written that way.
macro_rules! host_of {
    ($address:expr) => {
        concat!(
            "nullif(lower(btrim(substring(",
            $address,
            " from '^(?:[A-Za-z][A-Za-z0-9+.-]*://)?(?:[^@/]*@)?([^:/?#]+)'))), '')"
        )
    };
}

/// A monitor watching a host an asset states -- the estate's own exact-key
/// rule.
///
/// Spec #427: the monitor is attached to the asset by a `monitored-by` link,
/// and this is the pass that offers to draw it. **The asset is the subject**:
/// `monitored-by` reads *this asset is monitored by that check*, which is why
/// the candidate's `from_id` is the asset and its `to_id` the monitor
/// (`app/src/lib/detail/relations.ts` gives the sentence and its inverse).
///
/// # What a monitor watches: the URL where there is one, else the hostname
///
/// One definition, shared with `knobas_app::assets::reading_of`, which draws
/// the Monitors roster's *target* column from the same pair of keys and whose
/// doc points back here. Kuma gives an HTTP check a `url` and no `hostname`
/// and a `ping` a `hostname` and no `url`, so the two keys are one fact under
/// two spellings and a rule that read only the first could not see a ping at
/// all -- which is what #479 was: three of the eight monitors
/// `testenv/monitors.json` then seeded were pings carrying the Hetzner
/// servers' addresses in `hostname`, and the three real servers were out of
/// reach. (The servers were decommissioned in #634; the one ping left watches
/// the notebook's address.)
///
/// `coalesce` and **not** a union: a monitor carrying both is matched on the
/// URL only, the way `reading_of` prefers the URL. A blank `url`, or one that
/// is not a string, is *not one* -- the sibling's reading, where `text_at`
/// misses on a blank as well as on an absent key -- and the hostname beside it
/// is then what the monitor watches.
///
/// The `coalesce` is over the **address**, and `host_of!` is applied to what it
/// picks. That ordering is the whole of "the two agree": a monitor that has a
/// URL is matched on that URL *even when the URL turns out to name no host*,
/// rather than quietly falling through to a hostname the sibling would never
/// have shown. Coalescing the two *hosts* instead would put the divergence
/// exactly where nobody would look for it.
///
/// # How an asset states an address: two properties and a route
///
/// Unioned rather than written as three rules, because they are one fact --
/// *knobas knows this asset by that name*:
///
/// * the **hostname property**,
/// * the **ip property** (both `knobas_core::asset::TYPES`: a hypervisor's and
///   a VM's), and
/// * the host of a **route**, credited to
///   `coalesce(target_id, asset_id)` -- *the asset the route lands on, or the
///   one that exposes it when it lands on nothing knobas knows*.
///
/// Reading `ip` is what makes a ping check reachable at all. Kuma pings a bare
/// address, and an asset arm that read only `hostname` would match one only
/// where somebody had typed an address into a field labelled *Hostname*.
///
/// # A property is stored **tagged**, and this reads through the tag
///
/// `knobas.asset.properties` is a bag of `knobas_app::assets`' `PropertyValue`,
/// which is `#[serde(tag = "kind")]`: a hostname in a real database is
/// `{"kind":"text","value":"gitea"}` and never a bare `"gitea"`. So both
/// property arms read `->'hostname'->>'value'` and guard on that `value` being
/// a string. Three of `PropertyValue`'s four kinds are string-valued, so the
/// guard admits a `url` or a `date` as well as a `text` -- `host_of!` reads a
/// scheme off the first and finds nothing to match in the second. What it keeps
/// out is the fourth: `8080` typed as a `number` is not a host.
///
/// **This is a correction, not a decision.** #478 read `properties->>'hostname'`
/// against a battery whose fixtures wrote an untagged bag, so the arm passed
/// every test in `tests/it/suggestions.rs` and could not fire against a single row
/// any import or editor had written. What found it is the estate witness #479
/// added (`knobas-app`'s
/// `the_monitor_host_rule_proposes_nothing_over_the_real_estate`), which runs
/// the real import before it runs the rule; the battery's helpers now write
/// what the store writes.
///
/// # One pair, one suggestion, and which reason survives
///
/// A pair two arms find must still produce one suggestion, and the driver
/// guarantees that twice over: `distinct on` collapses the candidates, and `on
/// conflict do nothing` would swallow the duplicate anyway. Measured, because
/// the second half is easy to forget -- deleting the `distinct on` leaves
/// every assertion in this rule's battery passing. The reason the survivor
/// carries follows the driver's `order by`, whose last key is `c.reason`: the
/// route wording wins ("the host of ..." sorts before "which is ..."), and
/// between the two properties `hostname` wins over `ip`.
///
/// That `coalesce` is the one place this rule reads #451's sentence -- "the
/// host of a route the asset exposes" -- as naming *which routes are in play*
/// rather than which end of one gets the suggestion, and the estate the
/// milestone is developed against is why. `testenv/estate.json`'s only route
/// whose host a seeded monitor watches is `route:compose-gitea`,
/// `http://gitea:3000/`, exposed by `asset:orbstack-docker` and targeting
/// `asset:knobas-gitea` (until #634, `route:tunnel-gitea-reverse` from a
/// Hetzner server, in the same shape); the monitor is `gitea`,
/// `http://gitea:3000/api/healthz`. The thing that answers at that host is
/// Gitea. Crediting the exposing end would make the rule's *only* route
/// firing on the real estate a proposal naming the wrong asset -- and the
/// right one is already a link, drawn from that asset's `monitors: ["gitea"]`
/// by #439's import, so the driver's suppression drops it and the tray is left
/// holding exactly the wrong half. `0018` is where the fallback comes from: a
/// route with no target is "an endpoint that lands on nothing knobas knows",
/// and then the asset that exposes it is the best answer there is.
///
/// # The port is not compared
///
/// On either side: `host_of!` stops at it, which is what criterion 1 asks for
/// on the monitor's side and what makes a host serving several ports one asset
/// rather than several. So a monitor on `http://host:3000/` matches a route at
/// `http://host:8080/`, and an asset exposing many routes on one host is
/// proposed once -- the driver's `distinct on` keeps one row per pair, and the
/// reason then names whichever of those routes sorts first. The monitor's own
/// `port` key is not read either, for the same reason: there is nothing on the
/// asset's side to compare it against.
///
/// The cost is **a host several assets share**, and it is the known weakness
/// of this rule rather than an oversight: eight routes in
/// `testenv/estate.json` carry `127.0.0.1` and land on six different
/// assets, so a Kuma running on the notebook rather than in a container would
/// watch `http://127.0.0.1:8111/` and be proposed to all six. Nothing fires
/// on it today -- the seeded Kuma is in a container and reaches the notebook as
/// `host.docker.internal` -- and narrowing it was **rejected in triage**
/// (#479): the hostname and ip arms have no port to compare, so comparing
/// ports on the route arm alone would make one rule fail two ways. A proposal
/// is dismissible and a dismissal is remembered, which is what makes the weak
/// side of this trade survivable. The argument and the trigger that would
/// reopen it are `.out-of-scope/monitor-host-rule-shared-host-narrowing.md`.
///
/// # Telling the three arms apart
///
/// `route_name` carries the route's name **and** says the candidate came from
/// the route arm; `property` names which of the two properties stated the
/// address and is null on that arm. Sound rather than clever: `0018` declares
/// `name text not null` with `route_name_chk check (btrim(name) <> '')`, so a
/// route arm's `route_name` is never null and a property arm's always is. A
/// migration that relaxed either would make the `case` below tell the wrong
/// story, which is why the constraint is named here.
///
/// # ADR-0007
///
/// A **payload read outside an adapter**, and it takes that discipline in
/// full, as `SOURCE_RECORDED_RELATION` does: one named statement, a
/// `jsonb_typeof` guard on each of the four keys it reads -- the monitor's
/// `url` and its `hostname`, the asset's `hostname` and its `ip` -- and a
/// failure direction of *absence*. A monitor with neither key, with either of
/// them not a string, or with one naming no host contributes no candidate
/// rather than a guessed one, and so does an asset whose property carries a
/// `value` that is not a string. Neither bag is a typed column: `payload` is
/// what a source sent and `properties` is what knobas wrote, and the database
/// vouches for the shape of neither.
const MONITOR_URL_HOST: &str = detection!(concat!(
    "with watched as (
         select m.entity_id, ",
    host_of!(
        "coalesce(
                    nullif(btrim(case when jsonb_typeof(m.payload->'url') = 'string'
                                      then m.payload->>'url' end), ''),
                    nullif(btrim(case when jsonb_typeof(m.payload->'hostname') = 'string'
                                      then m.payload->>'hostname' end), ''))"
    ),
    " as host
           from sync.item m
          where m.kind = 'monitor' and ",
    live!("m"),
    "
     ),
     stated as (
         select a.id as asset_id, ",
    host_of!("a.properties->'hostname'->>'value'"),
    " as host, null::text as route_name, 'hostname'::text as property
           from knobas.asset a
          where jsonb_typeof(a.properties->'hostname'->'value') = 'string'
          union all
         select a.id, ",
    host_of!("a.properties->'ip'->>'value'"),
    ", null::text, 'ip'::text
           from knobas.asset a
          where jsonb_typeof(a.properties->'ip'->'value') = 'string'
          union all
         select coalesce(r.target_id, r.asset_id), ",
    host_of!("r.url"),
    ", r.name, null::text
           from knobas.route r
     )
     select s.asset_id, w.entity_id, 'monitored-by',
            case when s.route_name is null
                 then format('this monitor watches %s, which is the asset''s %s',
                             w.host, s.property)
                 else format('this monitor watches %s, the host of the route %s',
                             w.host, s.route_name)
            end
       from watched w
       join stated s on s.host = w.host"
));

/// How many distinct shared lexemes make two documents similar, as the literal
/// [`RANK_SIMILAR`] is compiled with.
///
/// A macro because the statement is a compile-time `concat!` and `concat!`
/// takes literals. [`SIMILARITY_FLOOR`] is the same number for a reader to name,
/// and a test holds the two together.
macro_rules! similarity_floor {
    () => {
        "5"
    };
}

/// How many distinct shared lexemes make two documents similar.
///
/// A judgement, not a measurement: low enough that two tickets about one
/// incident find each other, high enough that two tickets which both mention a
/// team and a quarter do not. English stop words are gone before this is
/// counted, so these are content stems.
pub const SIMILARITY_FLOOR: usize = 5;

/// The kinds the similarity rule reads: prose.
macro_rules! prose_kinds {
    () => {
        "('ticket','page','pr')"
    };
}

/// An item's title as the OR query the similarity rule matches other
/// documents with: every lexeme of the title, quoted, joined by `|`. `null`
/// for a title with no lexemes, and `null` matches nothing.
///
/// One spelling for the two places it is built -- the batch a pass compares
/// and the row [`MARK_SEEN`] stores -- because a stored query that differed
/// from the one a pass builds would judge a pair differently depending on
/// which of its ends changed last.
///
/// A [`common_lexemes!`] word is not in it (#776): a title of nothing but common
/// words finds nothing.
macro_rules! title_query {
    ($title:expr) => {
        concat!(
            "to_tsquery('english', (select string_agg(quote_literal(w), ' | ')
                                       from unnest(tsvector_to_array(to_tsvector('english', ",
            $title,
            "))) w
                                      where w <> all(",
            common_lexemes!(),
            ")))"
        )
    };
}

/// Every lexeme an item's [`title_query!`] can match on, for the GIN index
/// that finds the settled anchors whose query a changed item might satisfy.
///
/// Each title lexeme is put through `to_tsvector` again because that is what
/// `to_tsquery` does to it: a lexeme the parser splits further (`foo-bar`,
/// a URL) becomes a phrase of the parts, and every part is a lexeme the query
/// can match on. The index only *finds* candidates; `@@` against the stored
/// query still decides, so this set has to contain the query's lexemes and
/// may contain more. The oracle test's corpus carries hyphenated words,
/// versions, paths and host names to hold it to that.
macro_rules! title_lexemes {
    ($title:expr) => {
        concat!(
            "array(select distinct l
                     from unnest(tsvector_to_array(to_tsvector('english', ",
            $title,
            "))) w,
                          unnest(tsvector_to_array(to_tsvector('english', w))) l
                    where w <> all(",
            common_lexemes!(),
            "))"
        )
    };
}

/// [`COMMON_FROM_ITEMS`] as the literal [`FLIP_COMMON`] is compiled with.
macro_rules! common_from_items {
    () => {
        "200"
    };
}

/// The words too common to count (#776), as one array: every lexeme
/// `sync.similar_text_lexeme` (`0034`) marks common, which [`FLIP_COMMON`]
/// decides from how many live prose items hold it. Uncorrelated, so a
/// statement reads it once.
macro_rules! common_lexemes {
    () => {
        "array(select c.lexeme from sync.similar_text_lexeme c where c.common)"
    };
}

/// An item's lexemes without positions and without the [`common_lexemes!`] words:
/// what two documents' shared lexemes are counted over.
macro_rules! rare_lexemes {
    ($fts:expr) => {
        concat!("ts_delete(strip(", $fts, "), ", common_lexemes!(), ")")
    };
}

/// What `sync.similar_text_seen.digest` holds for an item (`0031`): the two
/// things the rule reads of it, the title its query is built from and the
/// `fts` its lexemes are counted in.
macro_rules! digest_of {
    ($item:expr) => {
        concat!("md5(", $item, ".title || E'\\n' || ", $item, ".fts::text)")
    };
}

/// How many counterparts an item's ranking keeps, as the literal the
/// statements are compiled with. [`SIMILAR_TEXT_CAP`] is the same number for a
/// reader to name, and a test holds the two together.
macro_rules! similar_text_cap {
    () => {
        "10"
    };
}

/// How many `similar_text` counterparts an item ranks (#771): its ten most
/// similar, by distinct shared lexemes, ties to the newest. A pair is proposed
/// when **either** end ranks the other, so an item can be proposed with more
/// than ten -- the ones that rank it -- but never because of a counterpart that
/// neither end ranks. No setting: the ruling of #771.
pub const SIMILAR_TEXT_CAP: usize = 10;

/// What a counterpart is ranked by when its distinct shared lexemes tie: the
/// item's own time, newest first, the way the project census (`0032`) reads an
/// item's time.
macro_rules! item_at {
    ($item:literal) => {
        concat!(
            "coalesce(",
            $item,
            ".item_updated_at, ",
            $item,
            ".synced_at)"
        )
    };
}

/// The distinct lexemes two documents share, as an array.
macro_rules! shared_lexemes {
    ($a:expr, $b:expr) => {
        concat!(
            "array(select x from unnest(tsvector_to_array(",
            $a,
            ")) x intersect select y from unnest(tsvector_to_array(",
            $b,
            ")) y)"
        )
    };
}

/// Rank one batch of changed items, and every settled item they meet, by
/// their ten most similar counterparts (#771): the judging half of
/// `similar_text`. [`SIMILAR_TEXT`] then proposes what the rankings hold.
///
/// Built on the **existing** full-text corpus (`sync.item.fts`, `0001`): the
/// anchor's title becomes an OR query, and the pair is then judged on how many
/// distinct lexemes the two *documents* share. Three deliberate narrowings,
/// each the difference between a rule and noise:
///
/// * **Prose kinds only.** A branch and its repository share their whole name
///   and are not "similar" in any sense a reader cares about.
/// * **The smaller id is the anchor** whose title is the query, so each
///   unordered pair is judged once.
/// * **A floor on shared lexemes.** English stop words are already gone by the
///   time `to_tsvector` is done, so [`SIMILARITY_FLOOR`] distinct shared stems
///   is a real overlap rather than two documents both being about work.
/// * **Common words count for nothing** (#776). A lexeme in more than 5% of
///   the live prose items ([`FLIP_COMMON`]) is a template's word, not a
///   topic's: it is not in the title query and not among the shared lexemes.
///   The title query and lexemes a seen row stores leave them out too, and an
///   item holding a word that crossed the line is compared again.
///
/// # Incremental (#745)
///
/// `$1` is a batch of items that changed since a pass last compared them; the
/// statement judges every pair with at least one end in it against the
/// **settled** corpus, the live prose items `sync.similar_text_seen` (`0031`)
/// says were compared with the text they have now. Three arms cover every such
/// pair once: batch with batch, a batch anchor with a settled item, a settled
/// anchor with a batch item. The second finds its candidates through
/// `similar_text_seen_fts_idx` (`0034`), the anchor's query probing the
/// settled items' `fts` -- not `item_fts_idx`, whose matches in the whole
/// mirror (unsettled, not prose) cost a third of the pass on the probe; the
/// third through
/// `similar_text_seen_lex_idx`, the changed item's lexemes probing the settled
/// anchors' title lexemes, with `@@` against the stored query deciding. Each
/// probe sits in a `lateral ... offset 0`, which keeps the planner from
/// flattening it into a join that tests every batch item against every settled
/// one.
///
/// **The anchor order is not an index condition** (#771). Written plainly,
/// `x.entity_id < o.entity_id` was one the planner preferred to the lexemes: a
/// walk of `similar_text_seen_pkey` over every settled item with a smaller id
/// -- half the table per batch item -- with the lexemes and the query only
/// filters on it. Wrapped in `is true` it is a filter and nothing else, so the
/// only index condition left in the probe is the one the GIN index answers.
/// The pass runs this statement with seq scans off, because a GIN index cannot
/// estimate an array it only sees per row, and on the plan with the best
/// estimate the whole of `similar_text_seen` is read once per batch item.
///
/// **A settled counterpart costs no visit to `sync.item`** (#771): its
/// lexemes (`strip(fts)` without the common words) and its time are on its
/// seen row, as they were when it was settled (`0033`), and two documents'
/// shared lexemes are counted as
/// `length(a) + length(b) - length(a || b)`, the tsvector union doing the
/// work. On the probe's 42-word corpus every batch item is paired with every
/// settled one, and the count is most of the pass. Whether a settled
/// counterpart is still the live text its seen row vouches for (a seen row
/// is only "was settled") is checked once per counterpart that came out
/// similar (`verified`), not once per candidate. That check is also what keeps
/// an item that left the corpus since the pass began, and so still has its
/// seen row until the next pass prunes it, out of a batch's rankings.
///
/// # Ranked (#771)
///
/// Every similar pair is an edge both ways, and each item's ranking
/// (`sync.similar_text_top`, `0033`) keeps its [`SIMILAR_TEXT_CAP`] best: a
/// batch item's from its edges alone (its old ranking was cleared with
/// [`CLEAR_RANKINGS`]), a settled item's from its stored ranking and its new
/// edges, the ones pushed past the tenth place deleted. Only the edges that
/// can place are sorted: `cut` counts each item's edges by shared lexemes and
/// keeps those at or above the tenth's count, ties included, so a batch item
/// paired with all 17,000 settled items sorts a handful rather than all of
/// them. The stored ranking is exact because every pair is judged once, when
/// the later of its ends is compared, and anything that would make a stored
/// place wrong -- a counterpart's text changing, a counterpart leaving the
/// corpus -- has the item ranked again from the whole corpus first
/// ([`UNSETTLE_RANKINGS`]).
///
/// Ranked on text alone: a counterpart the user already linked, or dismissed,
/// keeps its place, so dismissing a proposal does not bring the eleventh in.
const RANK_SIMILAR: &str = concat!(
    "with batch as materialized (
         select p.entity_id, p.fts, ",
    rare_lexemes!("p.fts"),
    " as lexemes, ",
    title_query!("p.title"),
    " as q, ",
    item_at!("p"),
    " as at, ",
    digest_of!("p"),
    " as digest
           from sync.item p
          where p.entity_id = any($1) and p.kind in ",
    prose_kinds!(),
    " and ",
    live!("p"),
    "
     ),
     judged as (
         select a.entity_id as from_id, o.entity_id as to_id,
                a.lexemes as a_lexemes, o.lexemes as o_lexemes,
                a.at as from_at, o.at as to_at, null::text as settled_id
           from batch a join batch o on o.entity_id > a.entity_id and o.fts @@ a.q
         union all
         select a.entity_id, i.entity_id, a.lexemes, i.lexemes, a.at, i.at, i.entity_id
           from batch a
           cross join lateral (
                  select i.entity_id, i.lexemes, i.at
                    from sync.similar_text_seen i
                   where i.fts @@ a.q
                     and (i.entity_id > a.entity_id) is true
                     and i.entity_id <> all($1)
                  offset 0) i
         union all
         select x.entity_id, o.entity_id, x.lexemes, o.lexemes, x.at, o.at, x.entity_id
           from batch o
           cross join lateral (
                  select x.entity_id, x.lexemes, x.at
                    from sync.similar_text_seen x
                   where x.title_lex && tsvector_to_array(o.fts)
                     and (x.entity_id < o.entity_id) is true
                     and o.fts @@ x.title_q
                     and x.entity_id <> all($1)
                  offset 0) x
     ),
     alike as materialized (
         select from_id, to_id, from_at, to_at, settled_id, shared
           from (select from_id, to_id, from_at, to_at, settled_id,
                        length(a_lexemes) + length(o_lexemes) - length(a_lexemes || o_lexemes)
                          as shared
                   from judged) j
          where shared >= ",
    similarity_floor!(),
    "
     ),
     verified as materialized (
         select d.settled_id
           from (select distinct settled_id from alike where settled_id is not null offset 0) d
          where exists (
                  select 1
                    from sync.similar_text_seen s
                    join sync.item p on p.entity_id = s.entity_id
                   where s.entity_id = d.settled_id and s.digest = ",
    digest_of!("p"),
    " and p.kind in ",
    prose_kinds!(),
    " and ",
    live!("p"),
    ")
     ),
     edge as materialized (
         select from_id as entity_id, to_id as other_id, shared, to_at as other_at
           from alike
          where settled_id is null or (settled_id in (select settled_id from verified)) is true
         union all
         select to_id, from_id, shared, from_at
           from alike
          where settled_id is null or (settled_id in (select settled_id from verified)) is true
     ),
     pool as materialized (
         select entity_id, other_id, shared, other_at, true as fresh
           from edge
         union all
         select t.entity_id, t.other_id, t.shared, t.other_at, false
           from (select distinct entity_id from edge) e
           join sync.similar_text_top t on t.entity_id = e.entity_id
     ),
     tally as (
         select entity_id, shared, count(*) as n from pool group by entity_id, shared
     ),
     cut as (
         select entity_id, min(shared) as least_shared
           from (select entity_id, shared, n,
                        sum(n) over (partition by entity_id order by shared desc) as through
                   from tally) t
          where through - n < ",
    similar_text_cap!(),
    "
          group by entity_id
     ),
     kept as materialized (
         select entity_id, other_id, shared, other_at, fresh
           from (select p.entity_id, p.other_id, p.shared, p.other_at, p.fresh,
                        row_number() over (partition by p.entity_id
                                           order by p.shared desc, p.other_at desc nulls last,
                                                    p.other_id) as place
                   from pool p
                   join cut c on c.entity_id = p.entity_id and p.shared >= c.least_shared) r
          where place <= ",
    similar_text_cap!(),
    "
     ),
     pushed_out as (
         delete from sync.similar_text_top t
          where t.entity_id in (select entity_id from edge)
            and not exists (
                  select 1 from kept k
                   where k.entity_id = t.entity_id and k.other_id = t.other_id)
     )
     insert into sync.similar_text_top (entity_id, other_id, shared, other_at, other_digest)
     select k.entity_id, k.other_id, k.shared, k.other_at,
            coalesce((select b.digest from batch b where b.entity_id = k.other_id),
                     (select s.digest from sync.similar_text_seen s
                       where s.entity_id = k.other_id))
       from kept k
      where k.fresh"
);

/// Text that reads alike -- the connection nobody wrote a key for.
///
/// What the rankings [`RANK_SIMILAR`] just wrote hold for the batch `$5`: every
/// pair one of whose ends ranks the other, with a batch item at either end.
/// Every pair a batch can make proposable has a batch item at one end, since a
/// ranking only ever gains a counterpart that was just compared.
///
/// The reason names the overlap itself, because "these look related" is exactly
/// the generic sentence #41's story 3 rejects. The smaller id is `from_id`, as
/// the anchor was before #771.
const SIMILAR_TEXT: &str = detection!(concat!(
    "select least(t.entity_id, t.other_id), greatest(t.entity_id, t.other_id), 'related',
            format('both mention %s',
                   (select string_agg(s, ', ' order by s)
                      from (select unnest(",
    shared_lexemes!(
        "(select a.lexemes from sync.similar_text_seen a where a.entity_id = t.entity_id)",
        "(select b.lexemes from sync.similar_text_seen b where b.entity_id = t.other_id)"
    ),
    ") as s order by 1 limit 4) top))
       from (select r.entity_id, r.other_id
               from sync.similar_text_top r where r.entity_id = any($5)
             union all
             select r.entity_id, r.other_id
               from sync.similar_text_top r where r.other_id = any($5)) t"
));

/// Forget the rankings a batch is about to replace: its items' own, and every
/// place they hold in another item's (#771). [`RANK_SIMILAR`] then ranks the
/// batch and puts each of its items back wherever it still places, against the
/// text it has now.
const CLEAR_RANKINGS: &str = "delete from sync.similar_text_top t
      where t.entity_id = any($1) or t.other_id = any($1)";

/// Forget the rankings of items settled without being compared (#755's
/// skipped item): [`CLEAR_RANKINGS`]' first half. The places they hold in
/// other items' rankings stay, measured against text they still have.
const FORGET_OWN_RANKINGS: &str = "delete from sync.similar_text_top where entity_id = any($1)";

/// Unsettle the items whose ranking can no longer be trusted, so the pass
/// compares them again, returning them (#771).
///
/// A ranking keeps ten places and not the eleventh, so a counterpart whose
/// text changed, or that left the corpus, leaves a place nobody can refill
/// from the stored rows: the item is compared with the whole corpus again.
/// `$1` is what changed. A counterpart in it whose stored digest still matches
/// its text -- unsettled, but not rewritten, after a pass gave up part-way --
/// is still the counterpart it was, and costs nothing. A counterpart with no
/// seen row that is not a live prose item has left the corpus.
const UNSETTLE_RANKINGS: &str = concat!(
    "delete from sync.similar_text_seen s
      where s.entity_id <> all($1)
        and s.entity_id in (
              select t.entity_id
                from sync.similar_text_top t
               where t.other_id = any($1)
                 and t.other_digest is distinct from (
                       select ",
    digest_of!("p"),
    " from sync.item p where p.entity_id = t.other_id)
              union
              select t.entity_id
                from sync.similar_text_top t
               where not exists (
                       select 1 from sync.similar_text_seen o where o.entity_id = t.other_id)
                 and not exists (
                       select 1
                         from sync.item p
                        where p.entity_id = t.other_id and p.kind in ",
    prose_kinds!(),
    " and ",
    live!("p"),
    "))
      returning s.entity_id"
);

/// Retire the unanswered `similar_text` proposals that neither end ranks any
/// more (#771), among the items a batch touched: its own, and those whose
/// ranking it entered -- the only rankings that lost a place. Deleted, not
/// tombstoned: a tombstone is a dismissal (the suppression reads it as one),
/// and a pair that ranks again later is proposed again.
///
/// Only between two items compared with the text they have now. An end still
/// waiting to be compared is ranked when it is, and its pairs are decided then.
/// Accepted and dismissed rows are never touched.
const RETIRE_SIMILAR: &str = concat!(
    "with touched as materialized (
         select unnest($1::text[]) as entity_id
         union
         select t.entity_id from sync.similar_text_top t where t.other_id = any($1)
     ),
     proposed as (
         select l.id, l.from_id, l.to_id
           from touched x
           join knobas.link l on l.from_id = x.entity_id
          where l.rule = 'similar_text' and l.confirmed_at is null and l.deleted_at is null
         union
         select l.id, l.from_id, l.to_id
           from touched x
           join knobas.link l on l.to_id = x.entity_id
          where l.rule = 'similar_text' and l.confirmed_at is null and l.deleted_at is null
     )
     delete from knobas.link d
      using proposed p
      where d.id = p.id
        and not exists (
              select 1 from sync.similar_text_top t
               where t.entity_id = p.from_id and t.other_id = p.to_id)
        and not exists (
              select 1 from sync.similar_text_top t
               where t.entity_id = p.to_id and t.other_id = p.from_id)
        and exists (
              select 1 from sync.similar_text_seen s join sync.item i on i.entity_id = s.entity_id
               where s.entity_id = p.from_id and s.digest = ",
    digest_of!("i"),
    " and ",
    live!("i"),
    ")
        and exists (
              select 1 from sync.similar_text_seen s join sync.item i on i.entity_id = s.entity_id
               where s.entity_id = p.to_id and s.digest = ",
    digest_of!("i"),
    " and ",
    live!("i"),
    ")"
);

/// The live prose items [`RANK_SIMILAR`] has not compared with the text they
/// have now: never compared, or changed since. Ordered, so a pass that gives
/// up part-way has settled a prefix and the next one starts after it.
const CHANGED_PROSE: &str = concat!(
    "select p.entity_id
       from sync.item p
      where p.kind in ",
    prose_kinds!(),
    " and ",
    live!("p"),
    "
        and not exists (
              select 1
                from sync.similar_text_seen s
               where s.entity_id = p.entity_id and s.digest = ",
    digest_of!("p"),
    ")
      order by p.entity_id"
);

/// Forget the items that left the live prose corpus -- tombstoned, their
/// source disabled, their kind no longer prose -- so one that comes back is
/// compared again rather than trusted on verdicts taken while it was away.
///
/// Once a pass, before the batches (#776). It read every seen row and cost 13
/// of the realistic probe's 75 s when every batch ran it; an item that leaves
/// mid-pass is kept out of the rankings by `verified` in [`RANK_SIMILAR`] and
/// out of [`RETIRE_SIMILAR`] by its liveness test, and is pruned by the next
/// pass.
const PRUNE_SEEN: &str = concat!(
    "delete from sync.similar_text_seen s
      where not exists (
              select 1
                from sync.item p
               where p.entity_id = s.entity_id and p.kind in ",
    prose_kinds!(),
    " and ",
    live!("p"),
    ")"
);

/// Take off the lexeme counts (`0034`) every counted item that left the live
/// prose corpus or whose lexemes changed (#776). [`COUNT_ARRIVED`] counts the
/// changed ones again, with the lexemes they have now. Each statement keeps
/// `similar_text_lexeme.items` the sum over the counted rows, so the counts are
/// exact without ever being recounted.
const COUNT_LEFT: &str = concat!(
    "with gone as (
         delete from sync.similar_text_counted c
          where c.lexemes is distinct from (
                  select strip(p.fts)
                    from sync.item p
                   where p.entity_id = c.entity_id and p.kind in ",
    prose_kinds!(),
    " and ",
    live!("p"),
    ")
         returning c.lexemes
     ),
     n as (
         select l.lexeme, count(*)::int as n
           from gone, unnest(gone.lexemes) l
          group by l.lexeme
     )
     update sync.similar_text_lexeme x
        set items = x.items - n.n
       from n
      where x.lexeme = n.lexeme"
);

/// Count every live prose item that is not counted yet (#776): new, back, or
/// changed and taken off by [`COUNT_LEFT`].
const COUNT_ARRIVED: &str = concat!(
    "with came as (
         insert into sync.similar_text_counted (entity_id, lexemes)
         select p.entity_id, strip(p.fts)
           from sync.item p
          where p.kind in ",
    prose_kinds!(),
    " and ",
    live!("p"),
    "
            and (p.entity_id in (select c.entity_id from sync.similar_text_counted c)) is not true
         on conflict (entity_id) do nothing
         returning lexemes
     ),
     n as (
         select l.lexeme, count(*)::int as n
           from came, unnest(came.lexemes) l
          group by l.lexeme
     )
     insert into sync.similar_text_lexeme (lexeme, items)
     select lexeme, n from n
     on conflict (lexeme) do update
        set items = sync.similar_text_lexeme.items + excluded.items"
);

/// How many live prose items make the 5% line (#776): under this many,
/// nothing is common, and a small install judges every word as before.
pub const COMMON_FROM_ITEMS: usize = 200;

/// Flip each lexeme whose count crossed the line since the corpus was last
/// ranked, and unsettle every item that holds a flipped one (#776).
///
/// A lexeme is **common** when more than 5% of the live prose items hold it,
/// once there are [`COMMON_FROM_ITEMS`] of them. A flip changes the shared
/// lexemes of exactly the pairs whose two ends both hold the word, and the
/// title query of exactly the items whose title does, so those items -- and
/// no others -- are compared again: their seen rows go, and their rankings
/// with them. Everything else stays ranked against the words it was ranked
/// with, which are the words it would be ranked with now.
const FLIP_COMMON: &str = concat!(
    "with census as materialized (select count(*) as n from sync.similar_text_counted),
     flipped as materialized (
         update sync.similar_text_lexeme x
            set common = not x.common
          where x.common <> ((select n from census) >= ",
    common_from_items!(),
    " and x.items * 20 > (select n from census))
         returning x.lexeme
     )
     delete from sync.similar_text_seen s
      where s.entity_id = any(array(
              select c.entity_id
                from sync.similar_text_counted c
               where (select count(*) from flipped) > 0
                 and tsvector_to_array(c.lexemes) && array(select lexeme from flipped)))"
);

/// Mark a batch settled: its items were compared with the text they have in
/// this snapshot.
const MARK_SEEN: &str = concat!(
    "insert into sync.similar_text_seen (entity_id, digest, title_q, title_lex, lexemes, at, fts)
     select p.entity_id, ",
    digest_of!("p"),
    ", ",
    title_query!("p.title"),
    ", ",
    title_lexemes!("p.title"),
    ", ",
    rare_lexemes!("p.fts"),
    ", ",
    item_at!("p"),
    ", p.fts
       from sync.item p
      where p.entity_id = any($1) and p.kind in ",
    prose_kinds!(),
    " and ",
    live!("p"),
    "
     on conflict (entity_id) do update
        set digest = excluded.digest, title_q = excluded.title_q,
            title_lex = excluded.title_lex, lexemes = excluded.lexemes, at = excluded.at,
            fts = excluded.fts"
);

/// How many changed items one [`RANK_SIMILAR`] statement compares.
///
/// Each batch is its own statement and its own transaction, so a pass that
/// times out keeps what its finished batches settled and the next pass goes
/// on from there rather than from nothing. Measured on the #745 probe's
/// corpus (a 108-word vocabulary, so every title matches nearly every
/// document -- the worst case): a batch of 100 against 5,000 settled items
/// judges about half a million pairs, ~15 s on a loaded machine, well inside
/// the pass's statement timeout.
pub const SIMILAR_TEXT_BATCH: usize = 100;

/// See [`Pacing::stall`].
#[doc(hidden)]
pub type Stall = Box<dyn Fn(&[String]) -> bool + Send + Sync>;

/// How big the next `similar_text` batch is, kept across passes (#755).
///
/// A batch that runs past the statement timeout is halved and the pass gives
/// up; the next pass retries with the smaller batch. A single item that still
/// times out is settled without proposals and logged, so the rule advances
/// past it. A pass that completes doubles the size back, up to its ceiling.
/// No setting: the ruling of #755.
pub struct Pacing {
    batch: std::sync::atomic::AtomicUsize,
    ceiling: usize,
    /// Test hook: batches this answers `true` for time out as if the server
    /// had cancelled them.
    #[doc(hidden)]
    pub stall: Option<Stall>,
}

impl Pacing {
    /// Batches of up to `ceiling`, starting there.
    #[must_use]
    pub fn new(ceiling: usize) -> Self {
        let ceiling = ceiling.max(1);
        Self {
            batch: std::sync::atomic::AtomicUsize::new(ceiling),
            ceiling,
            stall: None,
        }
    }

    /// The size the next batch is cut to.
    #[must_use]
    pub fn batch(&self) -> usize {
        self.batch.load(std::sync::atomic::Ordering::Relaxed)
    }

    fn shrink_to(&self, size: usize) {
        self.batch
            .store(size.max(1), std::sync::atomic::Ordering::Relaxed);
    }

    fn recover(&self) {
        let grown = self.batch().saturating_mul(2).min(self.ceiling);
        self.batch
            .store(grown, std::sync::atomic::Ordering::Relaxed);
    }
}

impl std::fmt::Debug for Pacing {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Pacing")
            .field("batch", &self.batch())
            .field("ceiling", &self.ceiling)
            .finish_non_exhaustive()
    }
}

impl Default for Pacing {
    fn default() -> Self {
        Self::new(SIMILAR_TEXT_BATCH)
    }
}

/// What a rule reads on each pass.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Scope {
    /// The whole mirror, every pass, in one statement taking the four driver
    /// binds. The exact-key and source-relation rules: a join on a key is
    /// cheap at any size the mirror reaches.
    Mirror,
    /// The prose items that changed since the last pass, in batches
    /// ([`SIMILAR_TEXT`]'s fifth bind, ranked by [`RANK_SIMILAR`] first), each
    /// marked settled as it is compared.
    ChangedProse,
}

/// Every rule, in the order [`detect`] runs them.
///
/// Order is not arbitrary: the suppression sees rows an earlier rule wrote in
/// the same pass, so the **most specific evidence wins the pair**. An exact key
/// beats a shared vocabulary, and the reason the user reads is the better one.
pub const RULES: &[Rule] = &[
    Rule {
        name: "branch_name_key",
        class: RuleClass::ExactKey,
        origin: Origin::Suggested,
        sql: BRANCH_NAME_KEY,
        scope: Scope::Mirror,
    },
    Rule {
        name: "commit_message_key",
        class: RuleClass::ExactKey,
        origin: Origin::Suggested,
        sql: COMMIT_MESSAGE_KEY,
        scope: Scope::Mirror,
    },
    Rule {
        name: "build_parameter_key",
        class: RuleClass::ExactKey,
        origin: Origin::Suggested,
        sql: BUILD_PARAMETER_KEY,
        scope: Scope::Mirror,
    },
    Rule {
        name: "page_text_key",
        class: RuleClass::ExactKey,
        origin: Origin::Suggested,
        sql: PAGE_TEXT_KEY,
        scope: Scope::Mirror,
    },
    Rule {
        name: "monitor_url_host",
        class: RuleClass::ExactKey,
        origin: Origin::Suggested,
        sql: MONITOR_URL_HOST,
        scope: Scope::Mirror,
    },
    Rule {
        name: "source_recorded_relation",
        class: RuleClass::SourceRelation,
        origin: Origin::Source,
        sql: SOURCE_RECORDED_RELATION,
        scope: Scope::Mirror,
    },
    Rule {
        name: "similar_text",
        class: RuleClass::Similarity,
        origin: Origin::Suggested,
        sql: SIMILAR_TEXT,
        scope: Scope::ChangedProse,
    },
];

/// What a [`PlannedStatement`] binds, in order.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Binds {
    /// A whole-mirror rule's four: origin, `created_by`, rule, rule class.
    Rule,
    /// `similar_text`'s four and the batch of item ids.
    RuleAndBatch,
    /// The batch of item ids alone ([`MARK_SEEN`]).
    Batch,
    /// Nothing.
    Nothing,
}

/// One statement the pass runs, for the plan-shape tests of #770.
#[doc(hidden)]
#[derive(Clone, Copy, Debug)]
pub struct PlannedStatement {
    pub name: &'static str,
    pub sql: &'static str,
    pub binds: Binds,
    /// The rule class a rule's statement binds; empty for the others.
    pub class: &'static str,
    /// Planned with `enable_seqscan` off, as the pass runs it
    /// ([`RANK_SIMILAR`]).
    pub no_seq_scan: bool,
}

/// Every statement a pass runs: each rule's, and the ones that keep
/// `sync.similar_text_seen` and the rankings (`sync.similar_text_top`).
/// `tests/it/suggest_plans.rs` holds each one's plan to a shape that does not
/// depend on the planner's statistics (#770).
#[doc(hidden)]
#[must_use]
pub fn planned_statements() -> Vec<PlannedStatement> {
    let mut statements: Vec<PlannedStatement> = RULES
        .iter()
        .map(|rule| PlannedStatement {
            name: rule.name,
            sql: rule.sql,
            binds: match rule.scope {
                Scope::Mirror => Binds::Rule,
                Scope::ChangedProse => Binds::RuleAndBatch,
            },
            class: rule.class.as_str(),
            no_seq_scan: false,
        })
        .collect();
    statements.extend([
        PlannedStatement {
            name: "changed_prose",
            sql: CHANGED_PROSE,
            binds: Binds::Nothing,
            class: "",
            no_seq_scan: false,
        },
        PlannedStatement {
            name: "count_left",
            sql: COUNT_LEFT,
            binds: Binds::Nothing,
            class: "",
            no_seq_scan: false,
        },
        PlannedStatement {
            name: "count_arrived",
            sql: COUNT_ARRIVED,
            binds: Binds::Nothing,
            class: "",
            no_seq_scan: false,
        },
        PlannedStatement {
            name: "flip_common",
            sql: FLIP_COMMON,
            binds: Binds::Nothing,
            class: "",
            no_seq_scan: false,
        },
        PlannedStatement {
            name: "prune_seen",
            sql: PRUNE_SEEN,
            binds: Binds::Nothing,
            class: "",
            no_seq_scan: false,
        },
        PlannedStatement {
            name: "mark_seen",
            sql: MARK_SEEN,
            binds: Binds::Batch,
            class: "",
            no_seq_scan: false,
        },
        PlannedStatement {
            name: "unsettle_rankings",
            sql: UNSETTLE_RANKINGS,
            binds: Binds::Batch,
            class: "",
            no_seq_scan: false,
        },
        PlannedStatement {
            name: "clear_rankings",
            sql: CLEAR_RANKINGS,
            binds: Binds::Batch,
            class: "",
            no_seq_scan: false,
        },
        PlannedStatement {
            name: "rank_similar",
            sql: RANK_SIMILAR,
            binds: Binds::Batch,
            class: "",
            no_seq_scan: true,
        },
        PlannedStatement {
            name: "retire_similar",
            sql: RETIRE_SIMILAR,
            binds: Binds::Batch,
            class: "",
            no_seq_scan: false,
        },
    ]);
    statements
}

/// The rule called `name`, for a caller that wants one on its own.
#[must_use]
pub fn rule(name: &str) -> Option<&'static Rule> {
    RULES.iter().find(|rule| rule.name == name)
}

/// Run one rule, returning how many proposals it wrote.
///
/// Independently runnable so each rule has a test of its own that no other
/// rule's output can satisfy -- which is what makes a negative control mean
/// anything.
///
/// Takes anything a connection can be had from -- a pool, or a connection the
/// caller opened. The app's pass runs on a connection of its own (#745), with
/// a `statement_timeout` it sets there, so a pass never holds a connection the
/// UI is waiting for.
///
/// # Errors
///
/// [`CoreError::Db`] if a statement fails, a timeout included. A
/// similarity rule keeps the batches it finished before it.
pub async fn detect_rule<'c, A>(conn: A, rule: &Rule) -> Result<u64, CoreError>
where
    A: sqlx::Acquire<'c, Database = sqlx::Postgres>,
{
    detect_rule_batched(conn, rule, SIMILAR_TEXT_BATCH).await
}

/// [`detect_rule`] with the batch size of a similarity rule
/// spelled out, for the tests that hold batching to the same answer as one
/// batch. A whole-mirror rule ignores it.
///
/// # Errors
///
/// As [`detect_rule`].
#[doc(hidden)]
pub async fn detect_rule_batched<'c, A>(
    conn: A,
    rule: &Rule,
    batch: usize,
) -> Result<u64, CoreError>
where
    A: sqlx::Acquire<'c, Database = sqlx::Postgres>,
{
    let mut conn = conn.acquire().await?;
    run_rule(&mut conn, rule, &Pacing::new(batch)).await
}

/// [`detect_rule`] with the [`Pacing`] a caller keeps across passes, for the
/// tests of #755.
///
/// # Errors
///
/// As [`detect_rule`].
#[doc(hidden)]
pub async fn detect_rule_paced(
    conn: &mut sqlx::PgConnection,
    rule: &Rule,
    pacing: &Pacing,
) -> Result<u64, CoreError> {
    run_rule(conn, rule, pacing).await
}

/// One rule on one connection. Concrete rather than generic over
/// [`sqlx::Acquire`], so a caller can hold the future across a
/// `tokio::spawn` -- the generic one is not `Send` for every lifetime.
async fn run_rule(
    conn: &mut sqlx::PgConnection,
    rule: &Rule,
    pacing: &Pacing,
) -> Result<u64, CoreError> {
    match rule.scope {
        Scope::Mirror => {
            let written = sqlx::query(rule.sql)
                .bind(rule.origin.as_str())
                .bind(DETECTOR)
                .bind(rule.name)
                .bind(rule.class.as_str())
                .execute(&mut *conn)
                .await?;
            Ok(written.rows_affected())
        }
        Scope::ChangedProse => {
            // The words too common to count, from the corpus as it stands;
            // an item holding a word that crossed the line is unsettled.
            count_lexemes(conn).await?;
            // Forget what left the corpus first, so a ranking that held a
            // place for it is found below.
            sqlx::query(PRUNE_SEEN).execute(&mut *conn).await?;
            let mut changed: Vec<String> = sqlx::query_scalar(CHANGED_PROSE)
                .fetch_all(&mut *conn)
                .await?;
            let unsettled: Vec<String> = sqlx::query_scalar(UNSETTLE_RANKINGS)
                .bind(&changed)
                .fetch_all(&mut *conn)
                .await?;
            if !unsettled.is_empty() {
                changed.extend(unsettled);
                changed.sort_unstable();
                changed.dedup();
            }
            let mut written = 0;
            let mut rest = changed.as_slice();
            while !rest.is_empty() {
                let n = pacing.batch().min(rest.len());
                let ids = &rest[..n];
                match compare_batch(conn, rule, ids, pacing).await {
                    Ok(batch_written) => written += batch_written,
                    Err(error) if is_timeout(&error) && n > 1 => {
                        pacing.shrink_to(n / 2);
                        tracing::warn!(
                            batch = n,
                            next = n / 2,
                            "a similar_text batch ran past the statement limit; \
                             the next pass retries it in smaller batches"
                        );
                        return Err(error.into());
                    }
                    Err(error) if is_timeout(&error) => {
                        tracing::warn!(
                            entity_id = %ids[0],
                            "similar_text skips an item whose comparison runs past the \
                             statement limit; it gets no similar-text proposals until its \
                             text changes"
                        );
                        settle_unranked(conn, ids).await?;
                    }
                    Err(error) => return Err(error.into()),
                }
                rest = &rest[n..];
            }
            pacing.recover();
            Ok(written)
        }
    }
}

/// Bring the lexeme counts up to date with the live prose corpus and flip the
/// words that crossed the 5% line (#776), as one transaction, so the common
/// words and the seen rows ranked with them never disagree. A pass over a
/// corpus whose counts did not move flips nothing and skips the flip.
async fn count_lexemes(conn: &mut sqlx::PgConnection) -> Result<(), sqlx::Error> {
    use sqlx::Connection as _;
    let mut tx = conn.begin().await?;
    let left = sqlx::query(COUNT_LEFT).execute(&mut *tx).await?;
    let arrived = sqlx::query(COUNT_ARRIVED).execute(&mut *tx).await?;
    if left.rows_affected() + arrived.rows_affected() > 0 {
        sqlx::query(FLIP_COMMON).execute(&mut *tx).await?;
    }
    tx.commit().await
}

/// `query_canceled`: what the server answers when `statement_timeout` fires.
fn is_timeout(error: &sqlx::Error) -> bool {
    error
        .as_database_error()
        .and_then(sqlx::error::DatabaseError::code)
        .is_some_and(|code| code == "57014")
}

/// Compare one batch of changed items and mark it settled, as one snapshot.
///
/// `repeatable read`, so the corpus the batch is judged against, the text the
/// batch is judged with and the digests [`MARK_SEEN`] records are the same
/// text: a sync that rewrites an item mid-batch leaves it unsettled for the
/// next pass rather than marked with text nobody compared.
///
/// The price of the snapshot is a serialization failure when a concurrent
/// transaction commits a row this one would conflict with -- the user drawing
/// the very pair, or a second knobas process's pass. The batch is retried a
/// few times on that and nothing else.
async fn compare_batch(
    conn: &mut sqlx::PgConnection,
    rule: &Rule,
    ids: &[String],
    pacing: &Pacing,
) -> Result<u64, sqlx::Error> {
    const ATTEMPTS: usize = 3;
    let mut attempt = 1;
    loop {
        let outcome = if pacing.stall.as_ref().is_some_and(|stall| stall(ids)) {
            stalled(conn).await
        } else {
            compare_batch_once(conn, rule, ids).await
        };
        match outcome {
            Err(error)
                if attempt < ATTEMPTS
                    && error
                        .as_database_error()
                        .and_then(sqlx::error::DatabaseError::code)
                        .is_some_and(|code| code == "40001") =>
            {
                attempt += 1;
            }
            outcome => return outcome,
        }
    }
}

/// The server cancelling a statement, for [`Pacing::stall`].
async fn stalled(conn: &mut sqlx::PgConnection) -> Result<u64, sqlx::Error> {
    use sqlx::Connection as _;
    let mut tx = conn.begin().await?;
    sqlx::query("set local statement_timeout = '20ms'")
        .execute(&mut *tx)
        .await?;
    sqlx::query("select pg_sleep(5)").execute(&mut *tx).await?;
    Ok(0)
}

async fn compare_batch_once(
    conn: &mut sqlx::PgConnection,
    rule: &Rule,
    ids: &[String],
) -> Result<u64, sqlx::Error> {
    use sqlx::Connection as _;
    let mut tx = conn.begin().await?;
    sqlx::query("set transaction isolation level repeatable read")
        .execute(&mut *tx)
        .await?;
    sqlx::query(CLEAR_RANKINGS)
        .bind(ids)
        .execute(&mut *tx)
        .await?;
    // Settled before it is ranked: a ranking row references its item's seen
    // row. `RANK_SIMILAR` reads the settled corpus without the batch.
    sqlx::query(MARK_SEEN).bind(ids).execute(&mut *tx).await?;
    // Seq scans off for this one statement: the third arm's probe of
    // `similar_text_seen_lex_idx` is costed as if it read most of the index
    // (a GIN index cannot estimate a per-row array), so with the settled
    // corpus a few thousand rows the planner scans the whole table once per
    // batch item instead (#771). Nothing in the statement needs a seq scan.
    sqlx::query("set local enable_seqscan = off")
        .execute(&mut *tx)
        .await?;
    sqlx::query(RANK_SIMILAR)
        .persistent(false)
        .bind(ids)
        .execute(&mut *tx)
        .await?;
    sqlx::query("set local enable_seqscan = on")
        .execute(&mut *tx)
        .await?;
    // Not a prepared statement: planned afresh for every batch, with the
    // batch's own ids and the settled corpus as it stands. A cached generic
    // plan, which PostgreSQL switches to after five executions, is costed for
    // a batch it cannot see and against `sync.similar_text_seen` statistics
    // taken while it was still empty; measured on the perf corpus it grew 4 s
    // per 100 settled items, where the batch's own plan stays under 2 s at
    // 4,000 (the #745 perf reading took 20 minutes before this).
    let written = sqlx::query(rule.sql)
        .persistent(false)
        .bind(rule.origin.as_str())
        .bind(DETECTOR)
        .bind(rule.name)
        .bind(rule.class.as_str())
        .bind(ids)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    sqlx::query(RETIRE_SIMILAR)
        .persistent(false)
        .bind(ids)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(written)
}

/// Settle `ids` without comparing them: the item a pass gives up on (#755).
/// Its old ranking goes with it, so nothing proposes from text it no longer
/// has.
async fn settle_unranked(conn: &mut sqlx::PgConnection, ids: &[String]) -> Result<(), sqlx::Error> {
    use sqlx::Connection as _;
    let mut tx = conn.begin().await?;
    sqlx::query(FORGET_OWN_RANKINGS)
        .bind(ids)
        .execute(&mut *tx)
        .await?;
    sqlx::query(MARK_SEEN).bind(ids).execute(&mut *tx).await?;
    tx.commit().await
}

/// Run every rule, returning how many proposals the pass wrote.
///
/// Idempotent: a second pass over an unchanged mirror writes nothing, because
/// every candidate it produces is already suppressed by a row it wrote the
/// first time -- and since #745 the similarity rule does not even look again:
/// it compares only the prose that changed since it last compared it. A pass
/// after a sync writes only what the new items justify, and resurrects
/// nothing the user dismissed.
///
/// Detection never creates a confirmed link and never writes to a source.
///
/// # Errors
///
/// [`CoreError::Db`] if a statement fails.
pub async fn detect<'c, A>(conn: A) -> Result<u64, CoreError>
where
    A: sqlx::Acquire<'c, Database = sqlx::Postgres>,
{
    let mut conn = conn.acquire().await?;
    detect_on(&mut conn).await
}

/// [`detect`] on a connection the caller holds -- the app's pass, which opens
/// one of its own and sets its `statement_timeout` there (#745).
///
/// # Errors
///
/// As [`detect`].
pub async fn detect_on(conn: &mut sqlx::PgConnection) -> Result<u64, CoreError> {
    detect_paced(conn, &Pacing::default()).await
}

/// [`detect_on`] with the batch size `pacing` keeps across passes (#755).
///
/// # Errors
///
/// As [`detect`].
pub async fn detect_paced(
    conn: &mut sqlx::PgConnection,
    pacing: &Pacing,
) -> Result<u64, CoreError> {
    let mut written = 0;
    for rule in RULES {
        written += run_rule(conn, rule, pacing).await?;
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The relation `monitor_url_host` proposes is the one the estate reads
    /// (#446).
    ///
    /// The fourth place that word is written and the second one in SQL, so it
    /// gets the pin the inbox's own alert rule gets: a proposal confirmed into
    /// a link nothing joins on would be a monitor visibly attached to an asset
    /// that never colours it, never reaches the inbox and never takes a
    /// recovery line -- with nothing failing anywhere.
    #[test]
    fn the_relation_this_rule_proposes_is_the_one_the_estate_reads() {
        assert!(
            MONITOR_URL_HOST.contains(&format!("'{}'", crate::link::MONITORED_BY)),
            "monitor_url_host proposes a relation nothing else in knobas reads"
        );
    }

    #[test]
    fn rule_class_roundtrips_through_its_column_value() {
        for class in RuleClass::ALL {
            assert_eq!(class.as_str().parse(), Ok(*class));
        }
        assert!("confident".parse::<RuleClass>().is_err());
    }

    /// The enum and `link_rule_class_chk` are one list written in two places,
    /// and neither may grow without the other -- exactly the pin
    /// `knobas_core::link` puts on `Origin` and migration `0003`.
    ///
    /// It reads `0007` because `0007` is where the constraint is, and applied
    /// migrations are never edited: widening the vocabulary means a later
    /// migration that drops and re-adds it, and rewriting this test to read
    /// *that* file is part of doing so.
    #[test]
    fn the_rule_classes_are_exactly_what_the_migration_allows() {
        let migration = include_str!("../../knobas-db/migrations/0007_suggestions.sql");
        let line = migration
            .lines()
            .find(|line| line.contains("check (rule_class in ("))
            .expect("link_rule_class_chk is missing from 0007");

        for class in RuleClass::ALL {
            assert!(
                line.contains(&format!("'{}'", class.as_str())),
                "{class:?} is a variant the constraint does not allow: {line}"
            );
        }
        assert_eq!(
            line.matches('\'').count() / 2,
            RuleClass::ALL.len(),
            "the constraint and the enum list different numbers of classes: {line}"
        );
    }

    #[test]
    fn rule_class_serializes_as_its_column_value() {
        assert_eq!(
            serde_json::to_string(&RuleClass::Similarity).unwrap(),
            "\"similarity\""
        );
    }

    /// Rule names are the stored `rule` value, so two rules sharing one would
    /// make a proposal's provenance unreadable -- and `rule()` would hand back
    /// whichever came first.
    #[test]
    fn every_rule_has_its_own_name_and_can_be_found_by_it() {
        let names: std::collections::BTreeSet<&str> = RULES.iter().map(|r| r.name).collect();
        assert_eq!(names.len(), RULES.len(), "two rules share a name");
        for rule in RULES {
            assert_eq!(super::rule(rule.name).map(|r| r.name), Some(rule.name));
        }
        assert!(super::rule("no_such_rule").is_none());
    }

    /// Every rule carries the driver, and no rule carries its own copy of it.
    ///
    /// This is the assertion behind "the suppression is written once": a rule
    /// added later that hand-rolled its own `insert` would be one the
    /// idempotence tests do not cover, and it would look perfectly correct
    /// beside the others.
    #[test]
    fn every_rule_statement_is_built_from_the_one_driver() {
        for rule in RULES {
            assert!(
                rule.sql.starts_with(driver_head!()),
                "{} does not open with the driver",
                rule.name
            );
            assert!(
                rule.sql.ends_with(driver_tail!()),
                "{} does not close with the driver",
                rule.name
            );
            // The driver's suppression is two `not exists`, one per direction
            // (#770); a rule adds none of its own.
            assert_eq!(
                rule.sql.matches("not exists").count(),
                driver_tail!().matches("not exists").count(),
                "{} carries a second suppression",
                rule.name
            );
            assert!(
                rule.sql.contains("null, $3, $4"),
                "{} writes something other than an unconfirmed row",
                rule.name
            );
        }
    }

    /// The floor a reader can name and the floor the statement is compiled with
    /// are one number.
    #[test]
    fn the_similarity_floor_in_the_statement_is_the_one_that_is_documented() {
        assert_eq!(
            similarity_floor!().parse::<usize>().unwrap(),
            SIMILARITY_FLOOR
        );
        assert!(
            RANK_SIMILAR.contains(concat!("where shared >= ", similarity_floor!())),
            "the similarity rule no longer reads the floor it declares"
        );
    }

    /// The cap a reader can name and the cap the ranking is compiled with are
    /// one number, and the ranking keeps and drops at it.
    #[test]
    fn the_common_word_floor_in_the_statement_is_the_one_that_is_documented() {
        assert_eq!(
            common_from_items!().parse::<usize>().unwrap(),
            COMMON_FROM_ITEMS
        );
        assert!(FLIP_COMMON.contains(concat!("(select n from census) >= ", common_from_items!())));
    }

    #[test]
    fn the_similar_text_cap_in_the_statement_is_the_one_that_is_documented() {
        assert_eq!(
            similar_text_cap!().parse::<usize>().unwrap(),
            SIMILAR_TEXT_CAP
        );
        assert!(RANK_SIMILAR.contains(concat!("where through - n < ", similar_text_cap!())));
        assert!(RANK_SIMILAR.contains(concat!("where place <= ", similar_text_cap!())));
    }

    /// Both classes #41 names are actually reachable, and so is the third.
    ///
    /// A rule table that had quietly lost its similarity rule would still pass
    /// every other test in this module.
    #[test]
    fn every_rule_class_has_at_least_one_rule() {
        for class in RuleClass::ALL {
            assert!(
                RULES.iter().any(|rule| rule.class == *class),
                "no rule produces {class:?}"
            );
        }
    }
}
