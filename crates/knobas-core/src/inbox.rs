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
//! [`InboxItem`] is computed from the live mirror (`sync.item` and its
//! liveness, `crate::live_by_exclusion!` since #778), `knobas.confirmed_link`
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
//!   leaves the other categories exactly where they were and leaves that
//!   source's own last-known items in the stream -- and, since #1571, is an
//!   item of its own (`failed_sign_in`).
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
//! * the **category** is one of seven words fixed in [`Category`] -- not the
//!   name of the rule that produced the item. Rules are expected to grow (a
//!   second mention spelling, a second way a build is "on my work"), and
//!   keying on a rule name would forget a snooze the day a category gained a
//!   second detector;
//! * the **subject** is `knobas.entity.id` for the four mirror-derived
//!   categories and for the alert (the *monitor's* entity, which outlives the
//!   mirror row) -- the durable identity in this database, which survives
//!   re-sync, tombstoning and a source being deleted and re-added -- and
//!   `source_config.id` for credential expiry and a failed sign-in, which
//!   interfaces §4.1 makes immutable.
//!
//! # The rules
//!
//! One rule per category, each stated, each independently runnable
//! ([`rule`] + [`items_from`]) and each with its own test *and a negative
//! control*: a rule that fires for everything is as broken as one that never
//! fires, and only the negative case catches the first.
//!
//! Four of the seven read a payload path, and that is a deliberate and narrow
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
//! ops as strings and knows nothing else about them: this crate cannot see
//! `WriteOp`, which is why [`crate::write_queue::project`] is handed a
//! shape rather than an op. Which of the candidates a
//! *particular* source can actually perform is decided where the descriptors
//! are, in `knobas_app::inbox`: an action the source does not declare is never
//! offered.

use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::PgPool;

use crate::CoreError;

crate::closed_vocabulary! {
    /// The seven kinds of demand the inbox distinguishes.
    ///
    /// Serialized as its stored spelling, which is also the first half of every
    /// [`InboxItem::key`]. **No CHECK pins it**: no column holds a category,
    /// because no table holds an item -- `knobas.inbox_state` stores the composed
    /// key as opaque text and nothing constrains it. It is declared with
    /// [`crate::closed_vocabulary!`] all the same (#1989), for the rest of what the
    /// macro gives: `ALL` and `as_str` from the variant list, so a category cannot
    /// be missing from `ALL`, and the codec the stream's statement decodes its
    /// computed `category` column through.
    ///
    /// - `review_request`: Somebody is waiting on your review of a pull request.
    /// - `mention`: Somebody wrote your name at you.
    /// - `failed_build`: A build on your work went red.
    /// - `new_assignment`: Work arrived: a ticket somebody else pointed at you.
    /// - `credential_expiry`: A credential knobas holds stops working soon.
    /// - `failed_sign_in`: A source refuses knobas' sign-in, or knobas holds no
    ///   credential for it, so it takes nothing until somebody acts.
    /// - `alert`: A monitor watching something in the estate is down or slow, and a
    ///   context holds the asset it watches.
    ///
    /// The shell's union is generated from this enum, so a category added here
    /// makes a total `switch` over it fail `svelte-check` rather than fall through
    /// to a label nobody notices.
    #[derive(ts_rs::TS)]
    #[ts(export_to = "entity.ts", rename = "InboxCategory")]
    pub enum Category {
        ReviewRequest => "review_request",
        Mention => "mention",
        FailedBuild => "failed_build",
        NewAssignment => "new_assignment",
        CredentialExpiry => "credential_expiry",
        FailedSignIn => "failed_sign_in",
        Alert => "alert",
    }
}

impl Category {
    /// The write ops an item of this category is asking for, best first.
    ///
    /// **Candidates, not an offer.** These are `WriteOp` identifiers; whether
    /// a given source can perform one is a property of that source's
    /// descriptor, and filtering them is `knobas_app::inbox`'s job, where the
    /// registry is. An action the source does not declare is never shown --
    /// the interface must not present a button that will fail (#45, story 23).
    ///
    /// Four categories are deliberately empty, and every absence is a decision:
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
    /// * **`FailedSignIn`.** The same act for the same reason: *Re-enter* is on
    ///   the source's page, and the source that refuses the sign-in is the one
    ///   thing that cannot be asked to fix it.
    /// * **`Alert`.** Three, and none of them is a `WriteOp`. *Ack* is
    ///   knobas-local because **Uptime Kuma has no ack** (`CONTEXT.md`,
    ///   **Alert**) -- it is `knobas_app::assets::ack_alert`, an estate write
    ///   and not a source one. *Open* lands in the Tree at the affected asset
    ///   (story 61). What would actually fix a down monitor is nowhere near a
    ///   source API, and Kuma's own write ops -- create, pause, resume a
    ///   monitor -- are not answers to "this thing is down".
    #[must_use]
    pub fn candidate_ops(self) -> &'static [&'static str] {
        match self {
            Category::ReviewRequest => &["approve", "comment"],
            Category::Mention => &["comment"],
            Category::FailedBuild => &["rerun_build"],
            Category::NewAssignment
            | Category::CredentialExpiry
            | Category::FailedSignIn
            | Category::Alert => &[],
        }
    }
}

/// One line of the stream.
///
/// Everything a reader needs to judge an item without opening it (#45, story
/// 7): which source it came from, what it is about, and why it is here.
///
/// **Derived, not stored.** There is no inbox table: every field here is
/// computed from the mirror when the stream is read, which is why an item
/// leaves on its own when the thing it was about is resolved at the source.
#[derive(Clone, Debug, Serialize, sqlx::FromRow, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct InboxItem {
    /// `'<category>:<subject>'` -- what snooze and done are keyed on, and what
    /// the interface hands back when the user answers.
    ///
    /// Stable across syncs: the category is a fixed word and the subject is an
    /// entity id (or a source id, for a credential expiry or a failed
    /// sign-in). The shell never
    /// composes one: it hands back the string it was given, to
    /// `snooze_inbox_item` and `complete_inbox_item`.
    pub key: String,
    /// Which demand this is. Decoded from the statement's text column so the
    /// wire form is the enum rather than a bare string.
    pub category: Category,
    /// The source that owes this item -- a `source_config.id`, which is also
    /// the entity namespace.
    pub source_id: String,
    /// The entity to open. `None` only for credential expiry and a failed
    /// sign-in, whose subject is a source and not an entity: the inbox is a
    /// way in, not a dead end (story 8), and an item with no entity behind it
    /// says so by carrying none rather than by pointing somewhere wrong.
    ///
    /// An item with no entity has no *Open*, and that is honest rather than
    /// incomplete.
    pub entity_id: Option<String>,
    /// The entity's kind, for the monogram. `None` with `entity_id`.
    pub kind: Option<String>,
    /// What the item is about, in the source's own words.
    ///
    /// Raw source text. Render it as text, never as markup (gotcha 7).
    pub title: String,
    /// Why this is in the stream, as a sentence -- produced by the rule, not
    /// re-rendered from the category at display time. The same discipline
    /// `suggest::SuggestionEntry::reason` gets: a demand whose reason cannot
    /// be shown is not shippable.
    ///
    /// Also raw text.
    pub reason: String,
    /// When the item last moved. The stream's ordering, and what *done* is
    /// measured against.
    ///
    /// RFC 3339 on the wire.
    pub occurred_at: DateTime<Utc>,
    /// Where a human reads this in the source's own UI, when the mirror knows.
    pub web_url: Option<String>,
    /// When this item comes back, for the rows on the snoozed shelf. `None` on
    /// the stream itself.
    ///
    /// RFC 3339 on the wire.
    pub snoozed_until: Option<DateTime<Utc>>,
}

/// How long before a credential expires knobas starts asking about it, and how
/// far back the two categories with no natural resolution look.
///
/// **Fourteen days, and the same fourteen for both**, which is a judgement
/// rather than a measurement: it is `mine-stale`'s fortnight
/// (`knobas_search::lists`), it is long enough that a credential's expiry is
/// actionable before the week it dies in, and it is short enough that switching
/// the inbox on for the first time does not produce a year of history.
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
               from sync.item i
               cross join lateral ",
            $crate::declared_list!("$4", "reviewers"),
            " as r
              where ",
            $crate::live_by_exclusion!("i"),
            "
                and i.kind = 'pr'
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
///
/// **The names are looked for through an index** (#1170). Read item by item
/// (#785's `strpos` ahead of the expression, per name in an `exists`), the
/// rule read all 18,500 of the probe's prose bodies: 12 ms of the stream for
/// every reader. Now the first test is **one expression for every name at
/// once**, an alternation built from `$1` with no subquery, so a plan made
/// for the bound names sees it as a constant: `item_body_trgm_idx` (migration
/// `0049`, trigrams of the prose kinds' bodies) answers it and the planner
/// costs it by the names' own trigrams. The rule then reads only the bodies
/// holding them, none for a name nobody wrote. The names are joined on
/// `chr(1)` and escaped together, which can only widen the alternation (a
/// name holding a `chr(1)` becomes its two halves), so the second test --
/// each name's own expression, #785's without its `strpos` -- decides. Both
/// are spelled by `mention_of!`. `cardinality`
/// keeps an empty `$1` empty: the alternation of no names matches any `@`.
/// Pinned by `a_name_with_pattern_characters_is_matched_literally` and, for
/// the index in both statistics states, `knobas-app`'s `read_plans.rs`. The
/// index keeps a short pending list (`0057`, #1864): at the default 4 MB a
/// load could leave enough of it unmerged that the planner, charging a
/// random read per pending page, priced the index above a scan of every body.
macro_rules! mention {
    () => {
        concat!(
            r"select 'mention', i.entity_id, i.source_id, i.entity_id, i.kind, i.title,
                     'you are mentioned here',
                     coalesce(i.item_updated_at, i.synced_at), i.web_url
                from sync.item i
               where ",
            $crate::live_by_exclusion!("i"),
            r"
                 and i.kind in ('ticket','pr','page','note')
                 and coalesce(i.item_updated_at, i.synced_at)
                       >= $2 - make_interval(days => ",
            window_days!(),
            r")
                 and cardinality($1::text[]) > 0
                 and i.body_text ~ replace(",
            mention_of!("array_to_string($1::text[], chr(1))"),
            r", chr(1), '|')
                 and i.body_text ~ any (array(select ",
            mention_of!("u"),
            r" from unnest($1::text[]) as u))"
        )
    };
}

/// The expression a mention of `$names` matches: `@` or `[~`, the text of
/// `$names` with every expression character escaped, then the end or a
/// character no name continues with. One spelling for both of
/// `mention!`'s tests, so the alternation cannot drift narrower than the
/// per-name expression that decides.
macro_rules! mention_of {
    ($names:literal) => {
        concat!(
            r"('(\[~|@)(' || regexp_replace(",
            $names,
            r", '([\\^$.|?*+()\[\]{}])', '\\\1', 'g') || ')($|[^A-Za-z0-9_.-])')"
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
///
/// **Read once per statement, not once per build** (#785). Both lookups were
/// correlated subqueries, and neither had an index to probe: the newer green
/// build walked every build of the kind per red one (nothing indexes the
/// configuration), and the confirmed link was a scan of `knobas.link` per
/// build wherever the planner judged the table small. With every build the
/// reader's, the rule read 185 ms on the probe's 5,000 builds in either
/// statistics state. Now:
///
/// * the newest finished green of each configuration is a window over the
///   builds, partitioned by source and configuration, so a red build is
///   resolved when that time is later than its own -- the same question as
///   "a newer green exists", answered in one sort with no join to plan. The
///   partition keys sort under `"C"`: only equality matters to a partition;
/// * the confirmed links are two `exists` probes per red build, one per
///   direction, keyed on `link_from_idx` and `link_to_idx`: a link from the
///   build to my work, and one from my work to the build. They were
///   uncorrelated `in` sets until #800, hashed once, but building a set meant
///   reading every confirmed link and probing its item: 12,000 links and
///   about 100 ms on the probe for a reader who owns none of them, the
///   all-but-everyone case. A red build has a few links, and only the red
///   builds still unresolved reach the probe (7 ms in either statistics
///   state).
macro_rules! failed_build {
    () => {
        concat!(
            "select 'failed_build', i.entity_id, i.source_id, i.entity_id, i.kind, i.title,
                format('this build failed%s', coalesce(': ' || nullif(i.status_text, ''), '')),
                i.at, i.web_url
           from (select scored.*,
                        max(scored.at) filter (where scored.status = 'SUCCESS' and scored.finished)
                          over (partition by scored.source_id collate \"C\", scored.config collate \"C\")
                          as latest_green
                   from (select b.entity_id, b.source_id, b.kind, b.title, b.author, b.web_url,
                                coalesce(b.item_updated_at, b.synced_at) as at,
                                b.payload->>'status' as status,
                                coalesce(b.payload->>'state', 'finished') = 'finished' as finished,
                                b.payload->>'statusText' as status_text,
                                coalesce(b.payload->>'buildTypeId',
                                         b.payload->'buildType'->>'id') as config
                           from sync.item b
                          where ",
            $crate::live_by_exclusion!("b"),
            "
                            and b.kind = 'build'
                            and b.payload->>'status' in ('FAILURE', 'SUCCESS')) scored) i
          where i.status = 'FAILURE'
            and i.finished
            and (   i.author = any($1)
                 or exists (
                       select 1
                         from knobas.confirmed_link l
                         join sync.item mine on mine.entity_id = l.to_id
                        where l.from_id = i.entity_id
                          and mine.author = any($1)
                          and ",
            $crate::live_by_key!("mine"),
            ")
                 or exists (
                       select 1
                         from knobas.confirmed_link l
                         join sync.item mine on mine.entity_id = l.from_id
                        where l.to_id = i.entity_id
                          and mine.author = any($1)
                          and ",
            $crate::live_by_key!("mine"),
            "))
            and (i.config is null or i.latest_green is null or i.latest_green <= i.at)"
        )
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
/// `knobas-core/tests/it/inbox.rs`.
///
/// **The declaration is walked once a source, not once a ticket** (#1164).
/// Through [`crate::declared_string!`] every ticket in the window walked the
/// whole declaration, about 4 µs a ticket: 58 ms of the stream on the probe's
/// 15,000 tickets for a reader who raised none of them, the realistic reader.
/// Now the assignee candidates are the CTE `assignee_paths`, one row per
/// source and candidate ([`crate::declared_ticket_paths!`], the mini board's
/// own), and each ticket is joined to its own source's rows and asked one
/// cheap question per candidate first: does the value there, trimmed, name
/// one of my accounts? Only a ticket that passes reads
/// [`crate::first_string_of!`], the rule itself, so a later candidate naming
/// me still loses to an earlier one naming somebody else. Two things keep
/// that order: the rule is correlated on `p.source_id`, the joined path's
/// source, so it is a join filter after the hash join rather than a scan
/// filter on every ticket; and a ticket whose candidates name me twice is two
/// rows here and one item after `shelved!`'s `distinct on`. The arm is
/// parenthesized because it carries its own `with`. Pinned by
/// `the_first_declared_assignee_decides_and_only_for_its_own_source`.
macro_rules! new_assignment {
    () => {
        concat!(
            "(with assignee_paths as materialized (",
            $crate::declared_ticket_paths!("$4", "assignee"),
            ")
             select 'new_assignment', i.entity_id, i.source_id, i.entity_id, i.kind, i.title,
                    'assigned to you',
                    coalesce(i.item_updated_at, i.synced_at), i.web_url
               from assignee_paths p
               join sync.item i on i.source_id = p.source_id
              where ",
            $crate::live_by_exclusion!("i"),
            "
                and i.kind = 'ticket'
                and coalesce(i.author, '') <> all($1)
                and coalesce(i.item_updated_at, i.synced_at)
                      >= $2 - make_interval(days => ",
            window_days!(),
            ")
                and btrim(i.payload #>> p.segs) = any($1)
                and ",
            $crate::first_string_of!(
                "(select a.segs, a.ord from assignee_paths a where a.source_id = p.source_id)"
            ),
            " = any($1))"
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

/// A source refuses knobas' sign-in, or knobas holds no credential for it
/// (#1571).
///
/// The second category read off `knobas.source_config` rather than the
/// mirror, and for the same reason as [`credential_expiry!`]: the mirror of a
/// source that refuses the sign-in simply stops moving, which says nothing.
///
/// **Which states.** `unauthorized` and `missing_secret` -- the two the
/// scheduler skips because no retry can fix them, and the two the shell's
/// `signInFails` reads as *the sign-in fails until the reader acts*.
/// `unreachable` is retried on its own and `unknown` vouches for nothing, so
/// neither is an item. Enabled sources only, as for an expiry.
///
/// **Once per episode.** The subject is the source, so the key is the same for
/// every failure of it; `occurred_at` is `auth_failing_since`, the first
/// failing verdict of the current run of them, which migration `0055`'s
/// trigger keeps. A retry rejected again (or a rejection turning into a
/// missing secret) is the same episode: the row does not move and a *done* on
/// it holds. A verdict that is not a failure nulls the column and ends the
/// episode; the rule stops firing and the item leaves by construction. The
/// next failure dates a new episode past the old *done*, so it comes back.
/// The `coalesce` onto `auth_checked_at` and then the clock is for a failing
/// row with no start recorded, which a data-only restore of an archive taken
/// before `0055` can leave until the restore's own verdict fires the trigger;
/// a null `occurred_at` would fail the whole stream's decode.
///
/// **What it says.** Which of the two it is, because they are different
/// prompts on the source's page (*Re-enter* after a rejection, *no token
/// stored*): the reason names the source, so the row is readable without
/// opening it.
macro_rules! failed_sign_in {
    () => {
        "select 'failed_sign_in', s.id, s.id, null::text, null::text,
                s.display_name,
                case s.auth_state
                  when 'missing_secret' then format('no credential is stored for %s', s.display_name)
                  else format('the %s sign-in was rejected', s.display_name)
                end,
                coalesce(s.auth_failing_since, s.auth_checked_at, $2),
                null::text
           from knobas.source_config s
          where s.enabled
            and s.auth_state in ('unauthorized', 'missing_secret')"
    };
}

/// A monitor watching something somebody is working on is in trouble.
///
/// The sixth category (#446), and the one that is **not about the reader's
/// accounts at all**: an alert is routed by *what it is about* rather than by
/// whose name is on it, so this rule reads neither `$1` nor a declared path.
/// `CONTEXT.md`, **Alert**: *"it reaches the inbox only when some context holds
/// the affected asset, directly or through an ancestor"* -- spec #427 story 59,
/// with story 60 as its negative control ("a fresh install with no contexts
/// shows alerts in the Assets view and the top strip and never in the inbox").
///
/// Four statements, each of which is the difference between this and noise.
///
/// **Which alerts.** Open (`closed_at is null`) and **un-acked**. Ack is
/// knobas-local and is *seen*, not *fixed* (story 62), so an acked alert leaves
/// the inbox and stays in the Assets view; recovery closes the row and takes
/// the item with it **by construction** (story 64) rather than by anybody
/// deleting an item. Both are the derived-inbox promise the module header makes
/// -- nobody removes an item, the rule stops firing.
///
/// **Which asset.** The one the monitor watches through a *confirmed*
/// `monitored-by` link (`CONTEXT.md`, **Monitor**), and, where a monitor
/// watches several, exactly one of them: a `lateral … limit 1` rather than a
/// join, because a monitor on a product can honestly watch the VM and the
/// container on it, and a plain join would draw the alert once per asset and
/// leave `distinct on (category, subject)` to pick between rows an arbitrary
/// order produced. Ordered by path and name so the one it picks is the same one
/// on every read.
///
/// **Which contexts.** [`crate::held_by_any_context!`] -- ADR-0008's membership
/// walk seeded from every context at once, so *"directly or through an
/// ancestor"* is that statement's `held` layer and not a second reading of it
/// (#434's third criterion). The lateral is what turns it into the routing
/// rule: an asset no context holds contributes no row, so the alert is simply
/// not a candidate.
///
/// **What it says.** `title` is the monitor's name, out of `knobas.entity` and
/// **not** `sync.live_item` -- a monitor paused in Kuma keeps its open alert
/// (#444) and would otherwise fall out of the inbox while still counting in the
/// top strip. `source_id` is the namespace half of the monitor's id, which is
/// the source it came from, for the same reason. `reason` names the asset by
/// its whole path and says which trouble it is in, which is what makes the row
/// readable without opening it (story 7). `entity_id` is the **asset**, not the
/// monitor: story 61 is *"opening an alert from the inbox lands in the Tree at
/// the affected asset with the monitor in the pane"*, and an item that pointed
/// at the monitor would land in a room detail instead. `web_url` is null --
/// `OpenAlert` carries no link into Kuma either, and for the reason #444
/// records: no surface opens one.
macro_rules! alert {
    () => {
        concat!(
            "select 'alert', a.entity_id, split_part(a.entity_id, ':', 1), watched.id,
                    'asset', e.title,
                    format('%s is %s', watched.full_path, a.state),
                    a.opened_at, null::text
               from knobas.monitor_alert a
               join knobas.entity e on e.id = a.entity_id
               cross join lateral (
                     select ast.id,
                            case when nullif(ast.path_text, '') is null then ast.name
                                 else ast.path_text || ' / ' || ast.name end as full_path
                       from knobas.confirmed_link l
                       join knobas.asset ast
                         on ast.id = case when l.from_id = a.entity_id
                                          then l.to_id else l.from_id end
                      where (l.from_id = a.entity_id or l.to_id = a.entity_id)
                        and l.relation = 'monitored-by'
                        and ast.id in ",
            $crate::held_by_any_context!(),
            "
                      order by ast.path_text asc, ast.name asc, ast.id asc
                      limit 1
                   ) watched
              where a.closed_at is null
                and a.acked_at is null"
        )
    };
}

/// Wrap a set of candidate rows in the shelving, the de-duplication and the
/// ordering every read shares.
///
/// **The four binds are the same for every read**: `$1` the identity
/// usernames, `$2` the clock, `$3` which shelf, and `$4` what every configured
/// source declares about its own payloads ([`crate::payload::Declarations`],
/// bound as one jsonb since #277 -- a rule added here binds it whether or not
/// it resolves a declared path, because the union has to type-check). That is
/// what lets the count
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
            "\nunion all\n",
            failed_sign_in!(),
            "\nunion all\n",
            alert!(),
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

/// Each rule's statement, shelved, named by its category, for `knobas-app`'s
/// plan test (`tests/it/read_plans.rs`, #778). Each binds what [`items`]
/// binds; the stream and its count are these unioned.
#[doc(hidden)]
#[must_use]
pub fn planned_statements() -> Vec<(Category, &'static str)> {
    RULES.iter().map(|rule| (rule.category, rule.sql)).collect()
}

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
    Rule {
        category: Category::FailedSignIn,
        sql: shelved!(failed_sign_in!()),
    },
    Rule {
        category: Category::Alert,
        sql: shelved!(alert!()),
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
///
/// - `stream`: What needs you now. This is the inbox.
/// - `snoozed`: What you deferred, and when it comes back.
///
/// One derivation with one predicate, asked for one side or the other, so the
/// two shelves can never overlap or leave a gap.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, serde::Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
#[ts(export_to = "entity.ts", rename = "InboxShelf")]
pub enum Shelf {
    Stream,
    Snoozed,
}

impl Shelf {
    fn snoozed(self) -> bool {
        matches!(self, Shelf::Snoozed)
    }
}

/// The stream, newest first.
///
/// **Planned for its values on every read** (`.persistent(false)`, #785), as
/// the board's and the tiles' statements are. The stream and the snoozed shelf
/// are one statement told apart by `$3`, and kept prepared, PostgreSQL ran its
/// generic plan from the sixth execution on: the shelf read 70 ms on the
/// probe's 22,000 items never analyzed, against 12 for the stream planned for
/// its values. [`count`] is the same statement and gets the same treatment.
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
        .persistent(false)
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
        .persistent(false)
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
    complete_with(pool, key, at).await
}

/// [`complete`], against an executor the caller chooses.
///
/// `activity::record_with`'s shape and its reason: an answer that is one part
/// of a larger write belongs in that write's transaction. #446's ack is the
/// caller -- it sets an alert's `acked_at`, completes this item and writes the
/// asset's history line as one act, and an ack that recorded two of the three
/// would leave an item on the stream for an alert nobody will be told about
/// again.
///
/// # Errors
///
/// [`CoreError::Db`] if the write fails.
pub async fn complete_with<'e, E>(
    executor: E,
    key: &str,
    at: DateTime<Utc>,
) -> Result<(), CoreError>
where
    E: sqlx::PgExecutor<'e>,
{
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
    .execute(executor)
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

    /// The alert rule reads the relation the estate actually draws.
    ///
    /// The rule is a compile-time `concat!`, so the relation is a literal in
    /// it and cannot be the constant itself. Three modules have to agree on
    /// that word -- this rule, `knobas_sync::alerts`' recovery line and
    /// `knobas_app::assets`, which draws the link -- and a fourth spelling
    /// would be a monitor attached to an asset that reaches no inbox, with
    /// nothing failing anywhere.
    #[test]
    fn the_alert_rule_reads_the_relation_the_estate_draws() {
        assert!(
            ALL_RULES.contains(&format!("l.relation = '{}'", crate::link::MONITORED_BY)),
            "the alert rule does not join on knobas_core::link::MONITORED_BY"
        );
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
