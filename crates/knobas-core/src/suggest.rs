//! Suggestions: the links knobas proposes, and what happens when you answer.
//!
//! A **suggestion is a link row, not a second table** (#41). The ratified link
//! vocabulary already closes `origin` over five spellings and one link table is
//! a standing rule -- a parallel suggestions table would be a second graph that
//! can disagree with the first. What tells a suggestion from a link is
//! therefore its *state*: [`LinkRow::confirmed_at`] is `None` on a proposal and
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

use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::CoreError;
use crate::link::{LinkEnd, LinkRow, Origin, link_columns};

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

impl std::fmt::Display for RuleClass {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A `rule_class` column value that is not one of the known classes.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("unknown suggestion rule class {0:?}")]
pub struct UnknownRuleClass(pub String);

impl std::str::FromStr for RuleClass {
    type Err = UnknownRuleClass;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        RuleClass::ALL
            .iter()
            .copied()
            .find(|class| class.as_str() == s)
            .ok_or_else(|| UnknownRuleClass(s.to_owned()))
    }
}

// Plain `text`, like `Origin`: the codec borrows `str`'s rather than declaring
// a PostgreSQL enum type that does not exist.
impl sqlx::Type<sqlx::Postgres> for RuleClass {
    fn type_info() -> sqlx::postgres::PgTypeInfo {
        <str as sqlx::Type<sqlx::Postgres>>::type_info()
    }

    fn compatible(ty: &sqlx::postgres::PgTypeInfo) -> bool {
        <&str as sqlx::Type<sqlx::Postgres>>::compatible(ty)
    }
}

impl<'r> sqlx::Decode<'r, sqlx::Postgres> for RuleClass {
    fn decode(value: sqlx::postgres::PgValueRef<'r>) -> Result<Self, sqlx::error::BoxDynError> {
        let text = <&str as sqlx::Decode<sqlx::Postgres>>::decode(value)?;
        Ok(text.parse()?)
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
/// no filter whatsoever, so one clause covers four separate promises:
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
/// `tests/suggestions.rs` green. It stays because it says in the statement
/// which of two duplicate candidates is meant to land, instead of leaving that
/// to a constraint firing behind it.
macro_rules! driver_tail {
    () => {
        ") as c(from_id, to_id, relation, reason)
          where c.from_id <> c.to_id
            and not exists (
                  select 1
                    from knobas.link l
                   where l.relation = c.relation
                     and (   (l.from_id = c.from_id and l.to_id = c.to_id)
                          or (l.from_id = c.to_id   and l.to_id = c.from_id)))
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

/// Every ticket in the mirror, with the bare key an issue reference spells.
///
/// An entity id is `<namespace>:<key>` split on the **first** colon, which is
/// why this is `substr(id, position(':' in id) + 1)` and not `split_part`: a
/// key may itself contain colons.
macro_rules! tickets {
    () => {
        "ticket as (
             select id, substr(id, position(':' in id) + 1) as key
               from knobas.entity
              where kind = 'ticket' and deleted_at is null
         )"
    };
}

/// One named detection rule.
///
/// Separate, named and independently testable (#41). `sql` is the whole
/// statement, already carrying the driver, and takes four binds: the origin,
/// `created_by`, the rule name and the rule class -- in that order.
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
}

/// An issue key in a branch name -- the most common connection in this work.
const BRANCH_NAME_KEY: &str = detection!(concat!(
    "with ",
    tickets!(),
    " select b.entity_id, t.id, 'related',
             format('the branch name contains %s', k.key)
        from sync.live_item b
        cross join lateral (
               select distinct m[1] as key
                 from regexp_matches(b.title, ",
    issue_key!(),
    ", 'g') m) k
        join ticket t on t.key = k.key
       where b.kind = 'branch'"
));

/// An issue key in a commit message.
///
/// Title *and* body, because an adapter puts the subject in one and the rest of
/// the message in the other, and a key written in the trailer is as much a
/// statement of connection as one in the subject.
const COMMIT_MESSAGE_KEY: &str = detection!(concat!(
    "with ",
    tickets!(),
    " select i.entity_id, t.id, 'related',
             format('the commit message mentions %s', k.key)
        from sync.live_item i
        cross join lateral (
               select distinct m[1] as key
                 from regexp_matches(i.title || ' ' || i.body_text, ",
    issue_key!(),
    ", 'g') m) k
        join ticket t on t.key = k.key
       where i.kind = 'commit'"
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
    tickets!(),
    " select i.entity_id, t.id, 'related',
             format('this build''s parameters name %s', k.key)
        from sync.live_item i
        cross join lateral (
               select distinct m[1] as key
                 from regexp_matches(i.title || ' ' || i.body_text, ",
    issue_key!(),
    ", 'g') m) k
        join ticket t on t.key = k.key
       where i.kind = 'build'"
));

/// An issue key in a page's text -- documentation connecting to what it
/// documents.
const PAGE_TEXT_KEY: &str = detection!(concat!(
    "with ",
    tickets!(),
    " select i.entity_id, t.id, 'related',
             format('this page mentions %s', k.key)
        from sync.live_item i
        cross join lateral (
               select distinct m[1] as key
                 from regexp_matches(i.title || ' ' || i.body_text, ",
    issue_key!(),
    ", 'g') m) k
        join ticket t on t.key = k.key
       where i.kind = 'page'"
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
const SOURCE_RECORDED_RELATION: &str = detection!(
    "select case when side.outward then i.entity_id else other.id end,
            case when side.outward then other.id else i.entity_id end,
            lower(side.relation),
            format('%s already records this link (%s)', i.source_id, side.label)
       from sync.live_item i
       cross join lateral jsonb_array_elements(i.payload->'fields'->'issuelinks') as l
       cross join lateral (
              select coalesce(l->'outwardIssue'->>'key', l->'inwardIssue'->>'key') as other_key,
                     (l->'outwardIssue') is not null                               as outward,
                     nullif(trim(coalesce(l->'type'->>'outward', l->'type'->>'name', '')), '')
                                                                                   as relation,
                     coalesce(l->'type'->>'name', 'link')                          as label) side
       join knobas.entity other
         on other.deleted_at is null
        and other.id = i.source_id || ':' || side.other_key
      where jsonb_typeof(i.payload->'fields'->'issuelinks') = 'array'
        and side.other_key is not null
        and side.relation is not null"
);

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
/// Two ways an asset states a host, unioned rather than written as two rules,
/// because they are one fact -- *knobas knows this asset by that name*:
///
/// * the **hostname property** (`knobas_core::asset::TYPES`: a hypervisor's
///   and a VM's), and
/// * the host of a **route**, credited to
///   `coalesce(target_id, asset_id)` -- *the asset the route lands on, or the
///   one that exposes it when it lands on nothing knobas knows*.
///
/// A pair both arms find must still produce one suggestion, and the driver
/// guarantees that twice over: `distinct on` collapses the candidates, and `on
/// conflict do nothing` would swallow the duplicate anyway. Measured, because
/// the second half is easy to forget -- deleting the `distinct on` leaves
/// every assertion in this rule's battery passing. The reason the survivor
/// carries follows the driver's `order by`, whose last key is `c.reason`: where
/// both arms reach one pair the route wording wins, "the host of ..." sorting
/// before "which is ...".
///
/// That `coalesce` is the one place this rule reads #451's sentence -- "the
/// host of a route the asset exposes" -- as naming *which routes are in play*
/// rather than which end of one gets the suggestion, and the estate the
/// milestone is developed against is why. `testenv/hetzner/estate.json`'s only
/// route whose host a seeded monitor watches is `route:tunnel-gitea-reverse`,
/// `http://gitea:3000/`, exposed by `asset:hetzner-teamcity` and targeting
/// `asset:knobas-gitea`; the monitor is `gitea`,
/// `http://gitea:3000/api/healthz`. The thing that answers at that host is
/// Gitea. Crediting the exposing end would make the rule's *only* firing on
/// the real estate a proposal naming the wrong asset -- and the right one is
/// already a link, drawn from that asset's `monitors: ["gitea"]` by #439's
/// import, so the driver's suppression drops it and the tray is left holding
/// exactly the wrong half. `0018` is where the fallback comes from: a route
/// with no target is "an endpoint that lands on nothing knobas knows", and
/// then the asset that exposes it is the best answer there is.
///
/// **The port is not compared**, on either side: `host_of!` stops at it, which
/// is what criterion 1 asks for on the monitor's side and what makes a host
/// serving several ports one asset rather than several. So a monitor on
/// `http://host:3000/` matches a route at `http://host:8080/`, and an asset
/// exposing many routes on one host is proposed once -- the driver's
/// `distinct on` keeps one row per pair, and the reason then names whichever
/// of those routes sorts first.
///
/// The cost is **a host several assets share**, and it is the known weakness
/// of this rule rather than an oversight: eight routes in
/// `testenv/hetzner/estate.json` carry `127.0.0.1` and land on seven different
/// assets, so a Kuma running on the notebook rather than in a container would
/// watch `http://127.0.0.1:8111/` and be proposed to all seven. Nothing fires
/// on it today -- the seeded Kuma is in a container and reaches the notebook as
/// `host.docker.internal` -- and narrowing it is its own decision: the hostname
/// arm has no port to compare, so comparing ports on the route arm alone would
/// make one rule fail two ways. A proposal is dismissible and a dismissal is
/// remembered, which is what makes the weak side of this trade survivable.
///
/// `route_name` carries the route's name **and** tells the two arms apart, and
/// that is sound rather than clever: `0018` declares `name text not null` with
/// `route_name_chk check (btrim(name) <> '')`, so a route arm's `route_name` is
/// never null and the hostname arm's always is. A migration that relaxed either
/// would make the `case` below tell the wrong story, which is why the constraint
/// is named here.
///
/// A **payload read outside an adapter** (ADR-0007), and it takes that
/// discipline in full, as `SOURCE_RECORDED_RELATION` does: one named
/// statement, the `jsonb_typeof` guard, and a failure direction of *absence* --
/// a monitor with no `url`, a `url` that is not a string, or one naming no host
/// contributes no candidate rather than a guessed one. The same guard sits on
/// the asset's `hostname`, which is a jsonb bag knobas writes but does not
/// type.
///
/// The monitor's own `hostname` field -- what Kuma reports for a ping or a port
/// check -- is **not** read here. #451's sentence is "a mirrored monitor's URL
/// host", and widening it to every address a monitor carries is a decision with
/// its own negative controls to write. It is not free, and it disagrees with a
/// sibling: `knobas_app::assets::reading_of` draws the roster's target column
/// from "the URL where there is one, else the hostname", because "Kuma gives an
/// HTTP monitor a URL and no hostname and a ping a hostname and no URL, so the
/// three keys are one fact under three spellings". Three of the eight monitors
/// `testenv/monitors.json` seeds are `ping` checks carrying the Hetzner
/// servers' IPs in `hostname`, and this rule cannot see them.
const MONITOR_URL_HOST: &str = detection!(concat!(
    "with watched as (
         select m.entity_id, ",
    host_of!("m.payload->>'url'"),
    " as host
           from sync.live_item m
          where m.kind = 'monitor'
            and jsonb_typeof(m.payload->'url') = 'string'
     ),
     stated as (
         select a.id as asset_id, ",
    host_of!("a.properties->>'hostname'"),
    " as host, null::text as route_name
           from knobas.asset a
          where jsonb_typeof(a.properties->'hostname') = 'string'
          union all
         select coalesce(r.target_id, r.asset_id), ",
    host_of!("r.url"),
    ", r.name
           from knobas.route r
     )
     select s.asset_id, w.entity_id, 'monitored-by',
            case when s.route_name is null
                 then format('this monitor watches %s, which is the asset''s hostname', w.host)
                 else format('this monitor watches %s, the host of the route %s',
                             w.host, s.route_name)
            end
       from watched w
       join stated s on s.host = w.host"
));

/// How many distinct shared lexemes make two documents similar, as the literal
/// [`SIMILAR_TEXT`] is compiled with.
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

/// Text that reads alike -- the connection nobody wrote a key for.
///
/// Built on the **existing** full-text corpus (`sync.item.fts`, `0001`) rather
/// than on a new index: the anchor's title becomes an OR query so the GIN index
/// does the candidate selection, and the pair is then judged on how many
/// distinct lexemes the two *documents* share.
///
/// Three deliberate narrowings, each of which is the difference between a rule
/// and noise:
///
/// * **Prose kinds only.** A branch and its repository share their whole name
///   and are not "similar" in any sense a reader cares about.
/// * **`o.entity_id > a.entity_id`.** Each unordered pair is considered once,
///   so the rule cannot propose `A/B` and `B/A` in one pass.
/// * **A floor on shared lexemes.** English stop words are already gone by the
///   time `to_tsvector` is done, so [`SIMILARITY_FLOOR`] distinct shared stems
///   is a real overlap rather than two documents both being about work.
///
/// The reason names the overlap itself, because "these look related" is exactly
/// the generic sentence #41's story 3 rejects.
const SIMILAR_TEXT: &str = detection!(concat!(
    "with prose as (
         select entity_id, title, fts from sync.live_item where kind in ('ticket','page','pr')
     ),
     anchor as (
         select entity_id, fts,
                (select string_agg(quote_literal(w), ' | ')
                   from unnest(tsvector_to_array(to_tsvector('english', title))) w) as q
           from prose
     ),
     pair as (
         select a.entity_id as from_id, o.entity_id as to_id,
                array(select x from unnest(tsvector_to_array(a.fts)) x
                       intersect
                      select y from unnest(tsvector_to_array(o.fts)) y) as shared
           from anchor a
           join prose o
             on o.entity_id > a.entity_id
            and o.fts @@ to_tsquery('english', a.q)
          where a.q is not null
     )
     select from_id, to_id, 'related',
            format('both mention %s',
                   (select string_agg(s, ', ' order by s)
                      from (select unnest(shared) as s order by 1 limit 4) top))
       from pair
      where cardinality(shared) >= ",
    similarity_floor!()
));

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
    },
    Rule {
        name: "commit_message_key",
        class: RuleClass::ExactKey,
        origin: Origin::Suggested,
        sql: COMMIT_MESSAGE_KEY,
    },
    Rule {
        name: "build_parameter_key",
        class: RuleClass::ExactKey,
        origin: Origin::Suggested,
        sql: BUILD_PARAMETER_KEY,
    },
    Rule {
        name: "page_text_key",
        class: RuleClass::ExactKey,
        origin: Origin::Suggested,
        sql: PAGE_TEXT_KEY,
    },
    Rule {
        name: "monitor_url_host",
        class: RuleClass::ExactKey,
        origin: Origin::Suggested,
        sql: MONITOR_URL_HOST,
    },
    Rule {
        name: "source_recorded_relation",
        class: RuleClass::SourceRelation,
        origin: Origin::Source,
        sql: SOURCE_RECORDED_RELATION,
    },
    Rule {
        name: "similar_text",
        class: RuleClass::Similarity,
        origin: Origin::Suggested,
        sql: SIMILAR_TEXT,
    },
];

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
/// # Errors
///
/// [`CoreError::Db`] if the statement fails.
pub async fn detect_rule(pool: &PgPool, rule: &Rule) -> Result<u64, CoreError> {
    let written = sqlx::query(rule.sql)
        .bind(rule.origin.as_str())
        .bind(DETECTOR)
        .bind(rule.name)
        .bind(rule.class.as_str())
        .execute(pool)
        .await?;
    Ok(written.rows_affected())
}

/// Run every rule, returning how many proposals the pass wrote.
///
/// Idempotent: a second pass over an unchanged mirror writes nothing, because
/// every candidate it produces is already suppressed by a row it wrote the
/// first time. A pass after a sync writes only what the new items justify, and
/// resurrects nothing the user dismissed.
///
/// Detection never creates a confirmed link and never writes to a source.
///
/// # Errors
///
/// [`CoreError::Db`] if a statement fails.
pub async fn detect(pool: &PgPool) -> Result<u64, CoreError> {
    let mut written = 0;
    for rule in RULES {
        written += detect_rule(pool, rule).await?;
    }
    Ok(written)
}

/// One proposal, as the tray draws it: the row, and **both** its ends.
///
/// Both, unlike [`crate::link::LinkEntry`], which resolves the end the reader is
/// not on. The tray is not read from either end -- it is read from a room -- so
/// there is no "here" for it to leave out, and #41's story 20 wants both ends
/// openable before deciding.
#[derive(Clone, Debug, Serialize)]
pub struct SuggestionEntry {
    pub link: LinkRow,
    pub from: LinkEnd,
    pub to: LinkEnd,
}

/// The row shape [`PROPOSALS`] returns, before the two ends are lifted out.
///
/// Two [`LinkEnd`]s cannot both be `#[sqlx(flatten)]` -- they would collide on
/// every column name -- so the statement aliases them apart and this struct
/// names the aliases.
#[derive(sqlx::FromRow)]
struct ProposalRow {
    #[sqlx(flatten)]
    link: LinkRow,
    from_entity_id: String,
    from_kind: String,
    from_title: String,
    from_deleted_at: Option<chrono::DateTime<chrono::Utc>>,
    to_entity_id: String,
    to_kind: String,
    to_title: String,
    to_deleted_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// The tray's read: proposals, newest first, optionally scoped to a room.
///
/// `knobas.proposed_link` and nothing else, which is what makes "the tray shows
/// proposals only" structural rather than remembered.
///
/// The ends join `knobas.entity` and not `sync.live_item`, for the reason
/// `entries_of` does: an entity a source withdrew still has to resolve, so the
/// tray marks it rather than dropping the row. `sync.item` is joined **left**
/// and only to learn which source an end came from, because knobas' own
/// entities (notes, contexts) have no mirror row at all.
const PROPOSALS: &str = concat!(
    "select ",
    link_columns!("l."),
    ", f.id as from_entity_id, f.kind as from_kind,
       f.title as from_title, f.deleted_at as from_deleted_at,
       t.id as to_entity_id, t.kind as to_kind,
       t.title as to_title, t.deleted_at as to_deleted_at
       from knobas.proposed_link l
       join knobas.entity f on f.id = l.from_id
       join knobas.entity t on t.id = l.to_id
       left join sync.item fi on fi.entity_id = l.from_id
       left join sync.item ti on ti.entity_id = l.to_id
      where ($1::text[] is null
         or fi.source_id = any($1) or ti.source_id = any($1))
        and ($2::text[] is null
         or l.from_id = any($2) or l.to_id = any($2))
      order by l.created_at desc, l.id desc
      limit $3"
);

/// The proposals a room holds, newest first.
///
/// `sources` is the room's membership, the same convention
/// `EntityFilter::sources` uses: **empty means every source**, bound as SQL
/// `NULL`, because `= any('{}')` matches nothing and a room whose filter said
/// "no sources in particular" would come back empty. A proposal belongs to a
/// room if *either* end does -- a suggestion connecting this room to another is
/// exactly the one worth surfacing here.
///
/// `members` is the second scope, and the one #47 promised this signature: a
/// stored context's room passes its membership (ADR-0008's rule, computed by
/// `crate::context::member_ids` plus the context's own entity), and only
/// proposals touching it come back. `None` is unscoped -- the derived rooms --
/// while `Some(&[])` is a context with no members, whose tray is honestly
/// empty. Membership is computed from **confirmed** links only, so scoping
/// proposals by it cannot become circular.
///
/// The tray holds no state: this is the whole of it.
///
/// # Errors
///
/// [`CoreError::Db`] if the query fails.
pub async fn proposals(
    pool: &PgPool,
    sources: &[String],
    members: Option<&[String]>,
    limit: i64,
) -> Result<Vec<SuggestionEntry>, CoreError> {
    let scope = (!sources.is_empty()).then(|| sources.to_vec());
    let rows = sqlx::query_as::<_, ProposalRow>(PROPOSALS)
        .bind(scope)
        .bind(members.map(<[String]>::to_vec))
        .bind(limit)
        .fetch_all(pool)
        .await?;
    Ok(rows
        .into_iter()
        .map(|row| SuggestionEntry {
            link: row.link,
            from: LinkEnd {
                entity_id: row.from_entity_id,
                kind: row.from_kind,
                title: row.from_title,
                deleted_at: row.from_deleted_at,
            },
            to: LinkEnd {
                entity_id: row.to_entity_id,
                kind: row.to_kind,
                title: row.to_title,
                deleted_at: row.to_deleted_at,
            },
        })
        .collect())
}

/// How many proposals a room is holding.
///
/// Its own statement rather than `proposals(..).len()`: the tray shows a count
/// before it shows rows (#41 story 19), and a count that had to fetch every row
/// to be produced would be capped by whatever `limit` the caller happened to
/// pass.
///
/// # Errors
///
/// [`CoreError::Db`] if the query fails.
pub async fn proposal_count(
    pool: &PgPool,
    sources: &[String],
    members: Option<&[String]>,
) -> Result<i64, CoreError> {
    let scope = (!sources.is_empty()).then(|| sources.to_vec());
    let (count,): (i64,) = sqlx::query_as(
        "select count(*)
           from knobas.proposed_link l
           left join sync.item fi on fi.entity_id = l.from_id
           left join sync.item ti on ti.entity_id = l.to_id
          where ($1::text[] is null
             or fi.source_id = any($1) or ti.source_id = any($1))
            and ($2::text[] is null
             or l.from_id = any($2) or l.to_id = any($2))",
    )
    .bind(scope)
    .bind(members.map(<[String]>::to_vec))
    .fetch_one(pool)
    .await?;
    Ok(count)
}

/// Accept a proposal: it becomes an ordinary link.
///
/// The row does not move and nothing about it is rewritten but `confirmed_at`,
/// which is what "there are not two kinds of link to reason about afterwards"
/// (#41 story 5) means in practice -- the same id, the same ends, the same
/// relation, and from the panel's side the same query that returns every other
/// link. Its `rule` and `reason` stay: they are provenance, and a link that can
/// still say why knobas thought so is strictly more useful than one that
/// cannot.
///
/// `Some(row)` is "this call accepted that proposal"; `None` is "there was
/// nothing left to accept" -- it was already accepted, or it was dismissed. The
/// distinction is the caller's, which writes one activity line per *mutation*.
///
/// # Errors
///
/// [`CoreError::LinkNotFound`] if no link carries `id` at all; [`CoreError::Db`]
/// if the statement fails.
pub async fn accept(pool: &PgPool, id: Uuid) -> Result<Option<LinkRow>, CoreError> {
    let updated = sqlx::query_as::<_, LinkRow>(concat!(
        "update knobas.link set confirmed_at = now()
          where id = $1 and deleted_at is null and confirmed_at is null
         returning ",
        link_columns!("")
    ))
    .bind(id)
    .fetch_optional(pool)
    .await?;
    settle(pool, id, updated).await
}

/// What [`resolve_edge`] found standing in the way of a hand-drawn link.
#[derive(Debug)]
pub enum Edge {
    /// A proposal for *exactly* this triple was confirmed. It is the link now,
    /// and the caller writes nothing.
    Promoted(LinkRow),
    /// A proposal for the **reversed** triple was withdrawn to make room. The
    /// caller writes the link the user actually drew, in the same transaction.
    Superseded(LinkRow),
    /// Nothing to answer: no live proposal for this pair and relation, either
    /// way round. Whatever refuses the write now is a real link.
    Open,
}

/// Answer the live proposal for a pair and relation, if there is one, so that a
/// hand-drawn link can be written where one stands.
///
/// The unique index spans proposals and links alike -- one active edge per pair
/// per relation, whatever its state -- so without this a user drawing a link
/// knobas had already proposed is told "already linked" about a pair whose links
/// panel is empty.
///
/// ## Two answers, because the two cases are different acts
///
/// **Same direction: the user is pressing *Accept*.** Drawing by hand the link
/// knobas proposed is that gesture spelled another way, so the proposal is
/// confirmed and keeps its detector provenance -- `rule`, `rule_class` and the
/// `reason` it was proposed with all still describe it truthfully.
///
/// **Reversed: the user is contradicting the proposal's direction**, and the
/// user wins. The proposal is *withdrawn* rather than confirmed, because
/// confirming it would store the opposite of what the user drew (`A blocks B`
/// landing as `B blocks A`, which story 7's inverse labels would then faithfully
/// render back at them as "A is blocked by B"), and reorienting it in place
/// would leave a row whose stored `reason` describes ends it no longer has.
/// Withdrawal is a tombstone -- the same mechanism as [`dismiss`], and therefore
/// the same fact to detection's undirected suppression, so the proposal is not
/// proposed straight back.
///
/// Before #70 the promotion was direction-exact while the index was directed,
/// which cancelled out into a third outcome nobody chose: the reversed
/// hand-drawn link *succeeded*, and left a stale proposal sitting in the tray
/// beside a confirmed link for the same pair.
///
/// ## Why it takes an executor
///
/// [`Edge::Superseded`] is only half a mutation -- the caller's write is the
/// other half -- so the two have to be able to share one transaction. A
/// tombstone committed without the link that replaced it would have thrown away
/// a proposal for nothing.
///
/// # Errors
///
/// [`CoreError::Db`] if either statement fails.
pub async fn resolve_edge(
    conn: &mut sqlx::PgConnection,
    from: &crate::entity::EntityRef,
    to: &crate::entity::EntityRef,
    relation: &str,
) -> Result<Edge, CoreError> {
    let promoted = sqlx::query_as::<_, LinkRow>(concat!(
        "update knobas.link set confirmed_at = now()
          where from_id = $1 and to_id = $2 and relation = $3
            and deleted_at is null and confirmed_at is null
         returning ",
        link_columns!("")
    ))
    .bind(from.to_string())
    .bind(to.to_string())
    .bind(relation)
    .fetch_optional(&mut *conn)
    .await?;
    if let Some(row) = promoted {
        return Ok(Edge::Promoted(row));
    }

    // The ends swapped, and nothing else: a proposal is what this withdraws, so
    // `confirmed_at is null` is the clause that keeps a *link* out of reach. A
    // confirmed link in the way is the caller's conflict to report.
    let superseded = sqlx::query_as::<_, LinkRow>(concat!(
        "update knobas.link set deleted_at = now()
          where from_id = $2 and to_id = $1 and relation = $3
            and deleted_at is null and confirmed_at is null
         returning ",
        link_columns!("")
    ))
    .bind(from.to_string())
    .bind(to.to_string())
    .bind(relation)
    .fetch_optional(conn)
    .await?;
    Ok(match superseded {
        Some(row) => Edge::Superseded(row),
        None => Edge::Open,
    })
}

/// Dismiss a proposal by tombstoning it -- the row stays, `deleted_at` is set.
///
/// **The same mechanism as [`crate::link::unlink`], deliberately.** A dismissed
/// suggestion and an unlinked link are one fact to the detector, and the
/// tombstone is that fact: [`detect`]'s suppression reads the table with no
/// filter, so neither can come back. Building a second store for dismissals
/// would be a second thing to keep in step with the first.
///
/// `Some(row)` is "this call dismissed that proposal"; `None` is "there was
/// nothing left to dismiss" -- already dismissed, or already accepted, in which
/// case it is a link and the panel's *Unlink* is what withdraws it. A confirmed
/// link is therefore never tombstoned through this door.
///
/// # Errors
///
/// [`CoreError::LinkNotFound`] if no link carries `id` at all; [`CoreError::Db`]
/// if the statement fails.
pub async fn dismiss(pool: &PgPool, id: Uuid) -> Result<Option<LinkRow>, CoreError> {
    let updated = sqlx::query_as::<_, LinkRow>(concat!(
        "update knobas.link set deleted_at = now()
          where id = $1 and deleted_at is null and confirmed_at is null
         returning ",
        link_columns!("")
    ))
    .bind(id)
    .fetch_optional(pool)
    .await?;
    settle(pool, id, updated).await
}

/// Tell "nothing to do" from "no such link", the way `unlink` does.
///
/// An update that matched nothing is either a proposal that has already been
/// answered -- fine, and idempotent -- or an id nothing carries, which the
/// caller wants to hear about rather than silently succeed on.
async fn settle(
    pool: &PgPool,
    id: Uuid,
    updated: Option<LinkRow>,
) -> Result<Option<LinkRow>, CoreError> {
    if updated.is_some() {
        return Ok(updated);
    }
    let existing: Option<(Uuid,)> = sqlx::query_as("select id from knobas.link where id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await?;
    existing.map(|_| None).ok_or(CoreError::LinkNotFound(id))
}

#[cfg(test)]
mod tests {
    use super::*;

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
            assert_eq!(
                rule.sql.matches("not exists").count(),
                1,
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
            SIMILAR_TEXT.contains(concat!("cardinality(shared) >= ", similarity_floor!())),
            "the similarity rule no longer reads the floor it declares"
        );
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
