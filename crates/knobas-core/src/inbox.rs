//! The inbox: the single actionable stream, derived from the mirror (#45).
//!
//! `CONTEXT.md` defines it as *"the single actionable stream -- mentions,
//! review requests, failed builds, assignments, credential expiry -- with
//! actions and snooze"*. This module is the derivation and the one thing it
//! stores.
//!
//! # The decision everything else follows from
//!
//! **The inbox is derived from the mirror, not synced into.** An
//! [`InboxItem`] is computed from `sync.live_item`, `knobas.confirmed_link`
//! and `knobas.source_config` every time the stream is read. There is no
//! adapter that fetches "inbox items", no table of them, and no fifth thing to
//! keep consistent -- a materialised inbox would be a second corpus that can
//! disagree with the first, which is the failure this feature exists to end
//! rather than to add to.
//!
//! Three promises fall out of that for free, and are worth naming because a
//! later "optimisation" that materialises the stream would quietly break all
//! three:
//!
//! * **An item leaves when the thing it was about is resolved at the source**
//!   (#45, story 17). Nobody deletes it; the rule stops firing on the next
//!   sync.
//! * **The inbox stays usable when one source is broken** (story 24). The
//!   derivation reads the mirror, not the sources, so a rejected credential
//!   leaves the other four categories exactly where they were and leaves that
//!   source's own last-known items in the stream.
//! * **Snoozing is durable** (story 20) without the stream being durable: the
//!   only row is the user's answer, and it is keyed so that the item derived
//!   again after a sync finds it.
//!
//! # What is stored, and what makes its key stable
//!
//! One table, `knobas.inbox_state` (migration `0009`), holding *snoozed until*
//! and *done*. Neither is derivable from anything a source says. Its key is
//! `'<category>:<subject>'` and both halves are stable across syncs:
//!
//! * the **category** is one of five words fixed in [`Category`] -- not the
//!   name of the rule that produced the item. Rules are expected to grow (a
//!   second mention spelling, a second way a build is "on my work"), and
//!   keying on a rule name would forget a snooze the day a category gained a
//!   second detector;
//! * the **subject** is `knobas.entity.id` for the four mirror-derived
//!   categories -- the durable identity in this database, which survives
//!   re-sync, tombstoning and a source being deleted and re-added -- and
//!   `source_config.id` for credential expiry, which interfaces §4.1 makes
//!   immutable.
//!
//! # The rules
//!
//! One rule per category, each stated, each independently runnable
//! ([`rule`] + [`items_from`]) and each with its own test *and a negative
//! control*: a rule that fires for everything is as broken as one that never
//! fires, and only the negative case catches the first.
//!
//! Four of the five read a payload path, and that is a deliberate and narrow
//! coupling rather than an oversight. Interfaces §4.1 normalizes `title`,
//! `body_text`, `author` and `updated_at` and nothing else; a review request,
//! a build's status and an assignee live only in the verbatim `payload`.
//! ADR-0007 ratified the pattern -- miss, one named statement, a pinned
//! failure direction -- and #277 finished it for two of the four: the review
//! request's reviewers and the assignment's assignee are read at the paths
//! **the source declares** ([`crate::payload`]), so a source that spells them
//! otherwise is one descriptor entry rather than one more arm here. A source
//! that declares neither produces no items of those two categories, which is
//! the same absence a source shaped differently produced before.
//!
//! The other two are unchanged and still spelled here: the mention rule reads
//! `body_text`, which §4.1 normalizes, and the failed-build rule reads a
//! build's *outcome* and its configuration -- a different fact from any of the
//! declared fields, and one no reader outside this rule wants. They are a
//! later ticket's to expire, if ever.
//!
//! Every such read is written to **miss** rather than to guess when the shape
//! is absent: a payload that does not carry the path contributes nothing, so a
//! source whose records are shaped differently simply produces no items of
//! that category instead of producing wrong ones.
//!
//! # Actions
//!
//! An item's actions are `knobas_source::WriteOp` identifiers
//! ([`Category::candidate_ops`]), dispatched through the write queue. **The
//! inbox introduces no write path of its own**, which is why this module names
//! ops as strings and knows nothing else about them -- `knobas-source` depends
//! on this crate, not the other way round, exactly as
//! [`crate::write_queue::PROJECTED_OPS`] does. Which of the candidates a
//! *particular* source can actually perform is decided where the descriptors
//! are, in `knobas_app::inbox`: an action the source does not declare is never
//! offered.

use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::PgPool;

use crate::CoreError;

/// The five kinds of demand the inbox distinguishes.
///
/// Serialized as its stored spelling, which is also the first half of every
/// [`InboxItem::key`]. **Not a database vocabulary**: no column holds a
/// category, because no table holds an item -- `knobas.inbox_state` stores the
/// composed key as opaque text and nothing constrains it. So this list has no
/// CHECK to be pinned against and is deliberately not declared with
/// [`crate::closed_vocabulary!`], whose whole purpose is that pinning.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    /// Somebody is waiting on your review of a pull request.
    ReviewRequest,
    /// Somebody wrote your name at you.
    Mention,
    /// A build on your work went red.
    FailedBuild,
    /// Work arrived: a ticket somebody else pointed at you.
    NewAssignment,
    /// A credential knobas holds stops working soon.
    CredentialExpiry,
}

impl Category {
    /// Every category, generated from the same list as the rules below.
    pub const ALL: &'static [Category] = &[
        Category::ReviewRequest,
        Category::Mention,
        Category::FailedBuild,
        Category::NewAssignment,
        Category::CredentialExpiry,
    ];

    /// The spelling that opens an [`InboxItem::key`].
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Category::ReviewRequest => "review_request",
            Category::Mention => "mention",
            Category::FailedBuild => "failed_build",
            Category::NewAssignment => "new_assignment",
            Category::CredentialExpiry => "credential_expiry",
        }
    }

    /// The write ops an item of this category is asking for, best first.
    ///
    /// **Candidates, not an offer.** These are `WriteOp` identifiers; whether
    /// a given source can perform one is a property of that source's
    /// descriptor, and filtering them is `knobas_app::inbox`'s job, where the
    /// registry is. An action the source does not declare is never shown --
    /// the interface must not present a button that will fail (#45, story 23).
    ///
    /// Two categories are deliberately empty, and both absences are decisions:
    ///
    /// * **`NewAssignment`.** What a new assignment asks for is *transition*,
    ///   and `WriteOp::Transition` needs the status the user picked, in the
    ///   source's own spelling -- which is per-workflow and per-project, so
    ///   there is no one-click form of it. Offering a transition that opens a
    ///   picker is not "the common response takes one action" (story 9), and
    ///   the start-work flow (#44) is where choosing a status lives. Open,
    ///   snooze and done are the honest actions here.
    /// * **`CredentialExpiry`.** Nothing at a source can fix it: re-entering a
    ///   credential is a knobas-local act on the sources view, and there is no
    ///   write op for it. Snooze -- *after the credential expires* is one of
    ///   the presets -- and done are the actions.
    #[must_use]
    pub fn candidate_ops(self) -> &'static [&'static str] {
        match self {
            Category::ReviewRequest => &["approve", "comment"],
            Category::Mention => &["comment"],
            Category::FailedBuild => &["rerun_build"],
            Category::NewAssignment | Category::CredentialExpiry => &[],
        }
    }
}

impl std::fmt::Display for Category {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A `category` spelling that names no known category.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("unknown inbox category {0:?}")]
pub struct UnknownCategory(pub String);

impl std::str::FromStr for Category {
    type Err = UnknownCategory;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Category::ALL
            .iter()
            .copied()
            .find(|category| category.as_str() == value)
            .ok_or_else(|| UnknownCategory(value.to_owned()))
    }
}

/// One line of the stream.
///
/// Everything a reader needs to judge an item without opening it (#45, story
/// 7): which source it came from, what it is about, and why it is here.
#[derive(Clone, Debug, Serialize, sqlx::FromRow)]
pub struct InboxItem {
    /// `'<category>:<subject>'` -- what snooze and done are keyed on, and what
    /// the interface hands back when the user answers.
    pub key: String,
    /// Which demand this is. Decoded from the statement's text column so the
    /// wire form is the enum rather than a bare string.
    #[sqlx(try_from = "String")]
    pub category: Category,
    /// The source that owes this item -- a `source_config.id`, which is also
    /// the entity namespace.
    pub source_id: String,
    /// The entity to open. `None` only for credential expiry, whose subject is
    /// a source and not an entity: the inbox is a way in, not a dead end
    /// (story 8), and the one item with no entity behind it says so by
    /// carrying none rather than by pointing somewhere wrong.
    pub entity_id: Option<String>,
    /// The entity's kind, for the monogram. `None` with `entity_id`.
    pub kind: Option<String>,
    /// What the item is about, in the source's own words.
    pub title: String,
    /// Why this is in the stream, as a sentence -- produced by the rule, not
    /// re-rendered from the category at display time. The same discipline
    /// `suggest::SuggestionEntry::reason` gets: a demand whose reason cannot
    /// be shown is not shippable.
    pub reason: String,
    /// When the item last moved. The stream's ordering, and what *done* is
    /// measured against.
    pub occurred_at: DateTime<Utc>,
    /// Where a human reads this in the source's own UI, when the mirror knows.
    pub web_url: Option<String>,
    /// When this item comes back, for the rows on the snoozed shelf. `None` on
    /// the stream itself.
    pub snoozed_until: Option<DateTime<Utc>>,
}

impl TryFrom<String> for Category {
    type Error = UnknownCategory;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

/// How long before a credential expires knobas starts asking about it, and how
/// far back the two categories with no natural resolution look.
///
/// **Fourteen days, and the same fourteen for both**, which is a judgement
/// rather than a measurement: it is `mine-stale`'s fortnight
/// (`knobas_search::lists`), it is long enough that a PAT expiry is actionable
/// before the week it dies in, and it is short enough that switching the inbox
/// on for the first time does not produce a year of history.
///
/// Spelled into the statements below as a literal because they are compile-time
/// `concat!`s; [`WINDOW_DAYS`] is the same number for a reader to name, and
/// `the_window_constant_matches_the_statements` holds the two together.
macro_rules! window_days {
    () => {
        "14"
    };
}

/// How many days of window the two unresolving categories get, and how early a
/// credential expiry is raised. See [`window_days!`].
pub const WINDOW_DAYS: i64 = 14;

/// The columns every rule produces, in order, so the union type-checks.
///
/// `occurred_at` is `coalesce(item_updated_at, synced_at)` for every
/// mirror-derived rule: interfaces §4.1 lets `updated_at` be null where the
/// source never dated the record, and an item with no date could not be
/// ordered, could not be marked done and would sort to one end of the stream
/// forever. knobas' own clock is the honest fallback and is always known.
macro_rules! review_request {
    () => {
        concat!(
            "select 'review_request'                     as category,
                    i.entity_id                          as subject,
                    i.source_id                          as source_id,
                    i.entity_id                          as entity_id,
                    i.kind                               as kind,
                    i.title                              as title,
                    format('%s asked for your review',
                           coalesce(nullif(i.author, ''), 'somebody')) as reason,
                    coalesce(i.item_updated_at, i.synced_at)           as occurred_at,
                    i.web_url                            as web_url
               from sync.live_item i
               cross join lateral ",
            $crate::declared_list!("$4", "reviewers"),
            " as r
              where i.kind = 'pr'
                and r.value = any($1)
                and coalesce(i.payload->>'state', 'open') = 'open'"
        )
    };
}

/// Somebody wrote your name at you.
///
/// Read out of `body_text` and not out of a payload, which makes this the one
/// rule that is true for **every** source by construction: interfaces §4.1
/// makes every adapter build `body_text` from the item's title, description
/// and comment texts, so a mention in a reply is in there whatever the record
/// looks like.
///
/// What counts as a mention, stated: the identity's username preceded by `@`
/// or by `[~` -- the two spellings the sources knobas syncs actually use
/// (Gitea and Confluence write `@name`, Jira Data Center writes `[~name]`) --
/// and **not** followed by another name character, `.` or `-`. The trailing
/// exclusion is what stops an account called `mara` matching every `@mara.
/// lindqvist`, which is a rule firing for somebody else's mentions and is the
/// specific failure the negative control pins. It costs one narrow false
/// negative -- a mention at the very end of a sentence, `@mara.` -- and that
/// is the direction to be wrong in.
///
/// Matching is **case-sensitive**, the same construction author matching has
/// (#82): the identity is a username as the source spells it.
///
/// Prose kinds only, and a window. A mention lives in a blob of text that has
/// no state saying it was answered, so nothing at the source ever resolves it;
/// without the window, switching the inbox on would produce every ticket that
/// ever named you. [`WINDOW_DAYS`] on the item's own timestamp is the stated
/// limit, and *done* is what clears an older one -- and lets it come back when
/// somebody says something new.
macro_rules! mention {
    () => {
        concat!(
            r"select 'mention', i.entity_id, i.source_id, i.entity_id, i.kind, i.title,
                     'you are mentioned here',
                     coalesce(i.item_updated_at, i.synced_at), i.web_url
                from sync.live_item i
               where i.kind in ('ticket','pr','page','note')
                 and coalesce(i.item_updated_at, i.synced_at)
                       >= $2 - make_interval(days => ",
            window_days!(),
            r")
                 and exists (
                       select 1
                         from unnest($1::text[]) as u
                        where i.body_text ~ ('(\[~|@)'
                              || regexp_replace(u, '([\\^$.|?*+()\[\]{}])', '\\\1', 'g')
                              || '($|[^A-Za-z0-9_.-])'))"
        )
    };
}

/// A build on your work went red.
///
/// Three statements, each of which is the difference between a rule and noise.
///
/// **What "failed" means**: the mirrored record says its status is `FAILURE`
/// and it is finished. A running build has no verdict yet and a canceled one
/// is not a failure (`0002`-era TeamCity work ratified `finished canceled` as
/// its own reading, #105).
///
/// **What "on my work" means**, and it is two arms because a red build reaches
/// you in two ways: either **you triggered it** -- `author` is the person who
/// started a build, by interfaces §4.1 and the TeamCity authorship ruling
/// (#106) -- or it is **confirmed-linked to something you authored**, which is
/// how a CI-triggered build on your own pull request finds you. The second arm
/// reads `knobas.confirmed_link` and not `knobas.link`: a *proposed* link is a
/// guess nobody has confirmed, and an inbox built on guesses is one the reader
/// stops trusting (#41, #161).
///
/// **What resolves it**: a newer build of the same configuration that
/// succeeded. A build is immutable once finished and a re-run is a new build
/// with a new id, so nothing at the source ever edits this record into a
/// resolved one -- without this clause a red build would sit in the inbox for
/// ever and story 17 would be false for the one category that most needs it.
/// The newer build must itself be **finished**: the mirror holds queued and
/// running builds too, and a running build's `status` is TeamCity's interim
/// verdict -- a re-run that is green *so far* has not resolved anything, and
/// hiding the red build while it runs would un-hide it minutes later if the
/// re-run fails, which is a stream that flickers rather than resolves.
macro_rules! failed_build {
    () => {
        "select 'failed_build', i.entity_id, i.source_id, i.entity_id, i.kind, i.title,
                format('this build failed%s',
                       coalesce(': ' || nullif(i.payload->>'statusText', ''), '')),
                coalesce(i.item_updated_at, i.synced_at), i.web_url
           from sync.live_item i
          where i.kind = 'build'
            and i.payload->>'status' = 'FAILURE'
            and coalesce(i.payload->>'state', 'finished') = 'finished'
            and (   i.author = any($1)
                 or exists (
                       select 1
                         from knobas.confirmed_link l
                         join sync.live_item mine
                           on mine.entity_id = case when l.from_id = i.entity_id
                                                    then l.to_id else l.from_id end
                        where (l.from_id = i.entity_id or l.to_id = i.entity_id)
                          and mine.author = any($1)))
            and not exists (
                  select 1
                    from sync.live_item newer
                   where newer.kind = 'build'
                     and newer.source_id = i.source_id
                     and newer.payload->>'status' = 'SUCCESS'
                     and coalesce(newer.payload->>'state', 'finished') = 'finished'
                     and coalesce(newer.payload->>'buildTypeId',
                                  newer.payload->'buildType'->>'id')
                       = coalesce(i.payload->>'buildTypeId',
                                  i.payload->'buildType'->>'id')
                     and coalesce(i.payload->>'buildTypeId',
                                  i.payload->'buildType'->>'id') is not null
                     and coalesce(newer.item_updated_at, newer.synced_at)
                       > coalesce(i.item_updated_at, i.synced_at))"
    };
}

/// Work arrived.
///
/// **Which assignment is "new"**, stated: a ticket whose mirrored record names
/// one of your accounts as its assignee, that **you did not raise yourself**
/// -- assigning your own ticket to yourself is not news -- and that the source
/// dated within [`WINDOW_DAYS`]. The window is the same admission the mention
/// rule makes: nothing in the record says "this assignment has been
/// acknowledged", so the corpus alone cannot tell a fresh assignment from a
/// two-year-old one, and *done* is what carries the acknowledgement afterwards.
///
/// The assignee is read at the path the **source declares** (#277), which for
/// Jira Data Center is `fields.assignee.name` with `fields.assignee.key` as
/// its second candidate -- the username, and the same identity on instances
/// that still key on it. A source that declares no assignee, or a record that
/// does not carry the declared one, contributes nothing: the miss direction,
/// unchanged, and pinned by
/// `an_assignee_the_declaration_does_not_reach_contributes_nothing` in
/// `knobas-core/tests/inbox.rs`.
macro_rules! new_assignment {
    () => {
        concat!(
            "select 'new_assignment', i.entity_id, i.source_id, i.entity_id, i.kind, i.title,
                    'assigned to you',
                    coalesce(i.item_updated_at, i.synced_at), i.web_url
               from sync.live_item i
              where i.kind = 'ticket'
                and ",
            $crate::declared_string!("$4", "assignee"),
            " = any($1)
                and coalesce(i.author, '') <> all($1)
                and coalesce(i.item_updated_at, i.synced_at)
                      >= $2 - make_interval(days => ",
            window_days!(),
            ")"
        )
    };
}

/// A credential knobas holds stops working soon.
///
/// The one category that is not derived from the mirror at all: it is
/// `knobas.source_config`'s own `secret_expires_at`, which is exactly the
/// point of the story -- *"my token expires on Friday is nowhere at all until
/// it stops working"*. Enabled sources only; a source the user switched off
/// owes them nothing.
///
/// **`occurred_at` is the moment it entered the window**, not the expiry date.
/// The expiry is in the future, and an item dated in the future would sit at
/// the top of a newest-first stream for ever and could never be marked done
/// (done compares against `occurred_at`). Dated at
/// `secret_expires_at - `[`WINDOW_DAYS`], it enters the stream where a
/// fortnight-old item belongs and moves only if somebody changes the expiry --
/// which is precisely when a *done* on it should be undone.
macro_rules! credential_expiry {
    () => {
        concat!(
            "select 'credential_expiry', s.id, s.id, null::text, null::text,
                    s.display_name,
                    format('the %s credential expires %s', s.display_name,
                           to_char(s.secret_expires_at, 'on FMDay DD Mon YYYY')),
                    s.secret_expires_at - make_interval(days => ",
            window_days!(),
            "),
                    null::text
               from knobas.source_config s
              where s.enabled
                and s.secret_expires_at is not null
                and s.secret_expires_at <= $2 + make_interval(days => ",
            window_days!(),
            ")"
        )
    };
}

/// Wrap a set of candidate rows in the shelving, the de-duplication and the
/// ordering every read shares.
///
/// **The three binds are the same for every read**: `$1` the identity
/// usernames, `$2` the clock, `$3` which shelf. That is what lets the count
/// below be the *same statement* rather than a second predicate that can
/// disagree with the stream -- the failure `knobas_search::lists` calls the
/// worst one available, a number on screen that no test comparing the list
/// against itself can see.
///
/// **Shelving.** `$3 = false` is the stream, `$3 = true` the snoozed shelf,
/// and they are complementary by construction: one boolean compared against
/// one expression, so an item is on exactly one of them and *the count
/// excludes snoozed items* (stories 15, 19) is one clause rather than a rule
/// two readers have to remember.
///
/// **Done.** Hidden while `done_at >= occurred_at`, so an item whose subject
/// has moved since it was answered comes back. See migration `0009` for why
/// that is a timestamp and not a boolean.
///
/// **De-duplication.** `distinct on (category, subject)` because a rule with a
/// lateral join can produce a row per match -- a pull request that lists an
/// account twice is one review request -- and because a category that later
/// grows a second rule must still produce one item per subject, or its key
/// would name two things.
macro_rules! shelved {
    ($($candidates:tt)*) => {
        concat!(
            "with candidate (category, subject, source_id, entity_id, kind,
                             title, reason, occurred_at, web_url) as (\n",
            $($candidates)*,
            "\n),
             answered as (
               select c.*, c.category || ':' || c.subject as key,
                      s.snoozed_until, s.done_at
                 from candidate c
                 left join knobas.inbox_state s
                        on s.item_key = c.category || ':' || c.subject
             ),
             shelf as (
               select distinct on (category, subject)
                      key, category, source_id, entity_id, kind, title, reason,
                      occurred_at, web_url,
                      case when $3 then snoozed_until end as snoozed_until
                 from answered
                where (done_at is null or done_at < occurred_at)
                  and $3 = (snoozed_until is not null and snoozed_until > $2)
                order by category, subject, occurred_at desc
             )
             select key, category, source_id, entity_id, kind, title, reason,
                    occurred_at, web_url, snoozed_until
               from shelf
              order by occurred_at desc, key"
        )
    };
}

/// Every rule, unioned. A macro rather than a `const` because the count below
/// is a `concat!` of this same text, and `concat!` takes literals only -- which
/// is exactly the property that makes the count unable to drift from the
/// stream.
macro_rules! all_rules {
    () => {
        shelved!(concat!(
            review_request!(),
            "\nunion all\n",
            mention!(),
            "\nunion all\n",
            failed_build!(),
            "\nunion all\n",
            new_assignment!(),
            "\nunion all\n",
            credential_expiry!(),
        ))
    };
}

/// The whole stream: every rule, unioned, shelved.
const ALL_RULES: &str = all_rules!();

/// How many items are on the stream.
///
/// **The same statement**, counted -- not a second predicate. A count computed
/// from a different `where` than the rows it claims to count is a wrong number
/// that no test comparing the inbox against itself can catch, so there is no
/// second `where` to get wrong.
const COUNT_ALL: &str = concat!("select count(*) as n from (", all_rules!(), ") counted");

/// One named detection rule.
///
/// Separate, named and independently runnable, the shape `suggest::Rule` has
/// and for the same reason: a rule nobody can run on its own is a rule nobody
/// can write a negative control for.
pub struct Rule {
    /// Which demand it detects. Also the rule's name: there is exactly one
    /// rule per category today, and the *category* is what an item's key
    /// carries, so naming rules separately would invite keying on the name.
    pub category: Category,
    /// The complete statement, already shelved. Private so a caller cannot run
    /// half of one.
    sql: &'static str,
}

/// Every rule, one per category.
pub const RULES: &[Rule] = &[
    Rule {
        category: Category::ReviewRequest,
        sql: shelved!(review_request!()),
    },
    Rule {
        category: Category::Mention,
        sql: shelved!(mention!()),
    },
    Rule {
        category: Category::FailedBuild,
        sql: shelved!(failed_build!()),
    },
    Rule {
        category: Category::NewAssignment,
        sql: shelved!(new_assignment!()),
    },
    Rule {
        category: Category::CredentialExpiry,
        sql: shelved!(credential_expiry!()),
    },
];

/// The rule for a category.
///
/// `None` never happens today -- [`RULES`] has one rule per variant, and
/// `every_category_has_a_rule` keeps that true -- but the signature stays
/// honest about being a lookup rather than promising a totality the type
/// system is not enforcing.
#[must_use]
pub fn rule(category: Category) -> Option<&'static Rule> {
    RULES.iter().find(|rule| rule.category == category)
}

/// Which shelf a read wants.
///
/// Serialized snake_case because it crosses the IPC boundary as a command
/// argument: the inbox view and the snoozed panel are one read with one
/// predicate, and asking for the shelf by name is what keeps them from
/// becoming two statements that can disagree about what "snoozed" means.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Shelf {
    /// What needs you now. This is the inbox.
    Stream,
    /// What you deferred, and when it comes back.
    Snoozed,
}

impl Shelf {
    fn snoozed(self) -> bool {
        matches!(self, Shelf::Snoozed)
    }
}

/// The stream, newest first.
///
/// `identity` is the usernames the sources were configured with -- the same
/// identity `@me` resolves to (`knobas_search::vocab`), never a second
/// mechanism. With none, every identity-bearing rule matches nothing, which is
/// the honest answer: knobas cannot tell which items are yours.
///
/// `now` is passed in rather than read from the clock, because a snooze that
/// cannot be tested is a snooze that does not work.
///
/// # Errors
///
/// [`CoreError::Db`] if the query fails.
pub async fn items(
    pool: &PgPool,
    identity: &[String],
    now: DateTime<Utc>,
    shelf: Shelf,
    declarations: &crate::payload::Declarations,
) -> Result<Vec<InboxItem>, CoreError> {
    Ok(sqlx::query_as::<_, InboxItem>(ALL_RULES)
        .bind(identity)
        .bind(now)
        .bind(shelf.snoozed())
        .bind(declarations.as_param())
        .fetch_all(pool)
        .await?)
}

/// One rule's items, for a test that wants to know what *that* rule sees.
///
/// # Errors
///
/// [`CoreError::Db`] if the query fails.
pub async fn items_from(
    pool: &PgPool,
    rule: &Rule,
    identity: &[String],
    now: DateTime<Utc>,
    shelf: Shelf,
    declarations: &crate::payload::Declarations,
) -> Result<Vec<InboxItem>, CoreError> {
    Ok(sqlx::query_as::<_, InboxItem>(rule.sql)
        .bind(identity)
        .bind(now)
        .bind(shelf.snoozed())
        .bind(declarations.as_param())
        .fetch_all(pool)
        .await?)
}

/// How many items need you now -- the number the top strip shows.
///
/// Snoozed items are not in it, because the number means "needs me now"
/// (story 19), and neither are items marked done.
///
/// # Errors
///
/// [`CoreError::Db`] if the query fails.
pub async fn count(
    pool: &PgPool,
    identity: &[String],
    now: DateTime<Utc>,
    declarations: &crate::payload::Declarations,
) -> Result<i64, CoreError> {
    let (n,): (i64,) = sqlx::query_as(COUNT_ALL)
        .bind(identity)
        .bind(now)
        .bind(Shelf::Stream.snoozed())
        .bind(declarations.as_param())
        .fetch_one(pool)
        .await?;
    Ok(n)
}

/// Come back on this date.
///
/// Idempotent per key: snoozing an already-snoozed item moves its date rather
/// than adding a row. Clears `done_at`, because the two are answers to the
/// same question and a row that was both would be hidden by whichever check
/// ran first.
///
/// # Errors
///
/// [`CoreError::Db`] if the write fails.
pub async fn snooze(pool: &PgPool, key: &str, until: DateTime<Utc>) -> Result<(), CoreError> {
    sqlx::query(
        "insert into knobas.inbox_state (item_key, snoozed_until)
              values ($1, $2)
         on conflict (item_key) do update
                set snoozed_until = excluded.snoozed_until,
                    done_at       = null,
                    updated_at    = now()",
    )
    .bind(key)
    .bind(until)
    .execute(pool)
    .await?;
    Ok(())
}

/// I handled this.
///
/// `at` is the clock, passed in for the reason [`items`]'s is. It is stored
/// and compared against the item's `occurred_at`, so the item stays gone until
/// its subject moves again -- see migration `0009`.
///
/// # Errors
///
/// [`CoreError::Db`] if the write fails.
pub async fn complete(pool: &PgPool, key: &str, at: DateTime<Utc>) -> Result<(), CoreError> {
    sqlx::query(
        "insert into knobas.inbox_state (item_key, done_at)
              values ($1, $2)
         on conflict (item_key) do update
                set done_at       = excluded.done_at,
                    snoozed_until = null,
                    updated_at    = now()",
    )
    .bind(key)
    .bind(at)
    .execute(pool)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A category with no rule is a demand nothing detects, and it would fail
    /// silently: the union simply would not contain it and the stream would be
    /// short by a whole class of item with nothing red anywhere.
    #[test]
    fn every_category_has_a_rule() {
        for category in Category::ALL {
            assert!(rule(*category).is_some(), "{category} has no rule in RULES");
        }
        assert_eq!(RULES.len(), Category::ALL.len());
    }

    /// The window is written twice -- once as a literal the statements are
    /// compiled with, once as a constant a reader can name -- so they are held
    /// together here.
    #[test]
    fn the_window_constant_matches_the_statements() {
        assert_eq!(window_days!(), WINDOW_DAYS.to_string());
        assert!(
            ALL_RULES.contains(concat!("days => ", window_days!())),
            "the compiled statement must carry the same window"
        );
    }

    /// The count is the stream's own statement, counted. If it ever became a
    /// second `select` with its own `where`, the two could disagree about
    /// snoozing and nothing comparing the inbox against itself would notice.
    #[test]
    fn the_count_is_the_stream_statement_counted() {
        assert!(COUNT_ALL.contains(ALL_RULES));
    }

    /// Round-trips, because the wire form and the key's first half are the
    /// same spelling and a decoder that could not read what the encoder wrote
    /// would be an item nobody can snooze.
    #[test]
    fn every_category_parses_back_from_its_spelling() {
        for category in Category::ALL {
            assert_eq!(category.as_str().parse::<Category>().unwrap(), *category);
        }
        assert!("mentions".parse::<Category>().is_err());
    }
}
