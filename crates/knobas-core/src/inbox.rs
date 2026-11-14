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
    ///
    /// **It leads with the subject or stays short (F-115).** Where the reason
    /// names the subject, the subject is its first word (*knobas-jira is down*,
    /// *Payout_IntegrationTests #1187 failed*); what would bury it goes in
    /// [`detail`](Self::detail).
    pub reason: String,
    /// What the row says after the subject, on its second line: an alert's
    /// asset path above the asset, a failed build's own status text. Reachable
    /// without leading the row (F-115). Raw text; absent on the wire where a
    /// rule has none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub detail: Option<String>,
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

/// [`crate::live_by_exclusion!`] for a rule of the stream: the same two
/// exclusion sets, the withdrawn entities read from `gone`, which
/// [`shelved!`] reads once for every rule, where each rule scanned
/// `knobas.entity` for them itself.
macro_rules! live_once {
    ($alias:literal) => {
        concat!(
            $alias,
            ".entity_id not in (select g.id from gone g)
         and ",
            $alias,
            ".source_id not in (select c.id from knobas.source_config c where not c.enabled)"
        )
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
///
/// A review request is open while the pull request's declared state chip
/// reads `open` ([`crate::resolved_state!`] over `state_paths`, the
/// declared state resolved once a statement, #2364), the word #2358's
/// start-work reads compare to. A state no map names, or a kind that declares
/// none, counts as open: the request stays rather than vanishing on a word
/// knobas cannot read.
///
/// **The pull requests that ask me are found through the names the database
/// stores** (`sync.item_reviewer`, migration `0080`; the query audit of
/// 2026-11-12). The rule read the declared reviewers of every live pull
/// request out of its payload and kept the ones naming me: 1,710 payloads
/// opened for 5 requests, 46 of the inbox's 89 ms. It now has two halves,
/// and a source is in exactly one:
///
/// * a source whose declared `reviewers` a stored declaration of the kind
///   gives: the pull requests storing one of my names under those same
///   candidates, a probe of the table's key per name. The stored names are
///   [`crate::declared_list!`]'s over those candidates, written as the item
///   is, so they are the walk's answer;
/// * any other source that declares reviewers (declarations the database
///   never stored: a test's, an adapter not yet written down): the walk, as
///   before, over that source's pull requests alone.
///
/// The first half reaches a pull request by its id and by nothing else: its
/// kind is tested inside a `coalesce`, which no index answers, because on
/// analyzed tables the planner otherwise read every pull request by kind
/// once per stored name (`knobas-app`'s `tests/it/read_plans.rs`).
///
/// A pull request that lists me twice was two rows and is one, which
/// [`shelved!`]'s `distinct on` made one item either way. `knobas-core`'s
/// `tests/it/inbox.rs` holds the two halves to the walk over every pull
/// request, [`REVIEW_REQUEST_WALKED`].
///
/// **The state is read for a pull request that asks me, and no other.**
/// Written beside the reviewer test it named the pull request alone, so the
/// planner ran it on every pull request before it looked at one reviewer:
/// 1,710 states for 144 requests. Both halves are fenced (`offset 0`), so it
/// runs on their rows.
macro_rules! review_request {
    () => {
        concat!(
            "(with state_paths as materialized (",
            $crate::state_paths!("$4"),
            ")
             select 'review_request'                     as category,
                    i.entity_id                          as subject,
                    i.source_id                          as source_id,
                    i.entity_id                          as entity_id,
                    i.kind                               as kind,
                    i.title                              as title,
                    format('%s asked for your review',
                           coalesce(nullif(i.author, ''), 'somebody')) as reason,
                    coalesce(i.item_updated_at, i.synced_at)           as occurred_at,
                    i.web_url                            as web_url,
                    null::text                           as detail
               from (select i.entity_id, i.source_id, i.kind, i.title, i.author,
                            i.item_updated_at, i.synced_at, i.web_url,
                            i.payload #> '{}'::text[] as payload
                       from (select distinct rv.entity_id, rv.paths
                               from sync.item_reviewer rv
                              where rv.reviewer = any($1)
                             offset 0) named
                       join sync.item i on i.entity_id = named.entity_id
                      where coalesce(i.kind = 'pr', false)
                        and named.paths = $4::jsonb #> array[i.source_id, i.kind, 'reviewers']
                     union all
                     select i.entity_id, i.source_id, i.kind, i.title, i.author,
                            i.item_updated_at, i.synced_at, i.web_url,
                            i.payload #> '{}'::text[] as payload
                       from sync.item i
                      where i.kind = 'pr'
                        and i.source_id = any (array(",
            reviewers_not_stored!(),
            "))
                        and exists (select 1 from ",
            $crate::declared_list!("$4", "reviewers"),
            " as r where r.value = any($1))
                     offset 0) i
              where ",
            live_once!("i"),
            "
                and coalesce(",
            $crate::resolved_state!(),
            "->>'text', 'open') = 'open')"
        )
    };
}

/// The sources whose declared `reviewers` of a pull request no stored
/// declaration of the kind gives, so `sync.item_reviewer` holds no row under
/// them and [`review_request!`] walks their payloads. The test is the one
/// `sync.resolve_item_reviewers` (migration `0080`) writes by: it stores
/// under every non-empty `reviewers` a stored declaration gives the item's
/// kind. A source that declares none has nobody to ask, so it is in neither
/// half. The stored candidates are read once, as an array, and the sources
/// are an array too: the walk is one pass over those sources' pull requests,
/// and no pass at all where there is none.
macro_rules! reviewers_not_stored {
    () => {
        "select s.key
           from jsonb_each(case when jsonb_typeof($4::jsonb) = 'object'
                                then $4::jsonb else '{}'::jsonb end) as s(key, kinds)
          where jsonb_typeof(s.kinds #> '{pr,reviewers}') = 'array'
            and jsonb_array_length(s.kinds #> '{pr,reviewers}') > 0
            and s.kinds #> '{pr,reviewers}' <> all (array(
                  select kp.p->'reviewers'
                    from sync.parent_declaration spd
                   cross join lateral jsonb_array_elements(
                           case when jsonb_typeof(spd.payload_paths) = 'array'
                                then spd.payload_paths else '[]'::jsonb end) as kp(p)
                   where kp.p->>'kind' = 'pr'
                     and jsonb_typeof(kp.p->'reviewers') = 'array'
                     and jsonb_array_length(kp.p->'reviewers') > 0))"
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
/// ever named you. [`WINDOW_DAYS`] on the mention's own date
/// ([`mentioned_at!`]) is the stated limit, and *done* is what clears an
/// older one -- and lets it come back when somebody names you again, not
/// whenever the item moves.
///
/// **The rule reads the items that write the name and moved inside the
/// window** (the query audit of 2026-11-12), found through two stored facts
/// and no body:
///
/// * `sync.item_mention` (migration `0079`) holds, for each `@` and `[~` of a
///   prose item's body, the run of name characters after it. An item that
///   writes a name `N` at somebody stores `N`'s own leading run of name
///   characters, which is `N` for a name made of them, so the items storing
///   that text are a superset of the rule's matches (the migration says why)
///   and the expressions below decide on them alone.
/// * The item's own date is inside the window. The window is on the
///   mention's date, and a mention is dated by a comment, by the item's
///   `created` or by the item itself, none of them later than the item's own
///   date: both sources whose records carry a thread (Jira, the mock) move an
///   item's `updated` with every comment.
///
/// Before, the names were looked for through the bodies' trigram index
/// (#1170, below), which answers with every body holding a name's letters:
/// 4,465 bodies for a username that is also a word, each fetched and matched
/// again, and every match then had its thread read to date it, 115 ms of
/// every stream on a mirror of 39,770 items. **Which way the date test fails,
/// stated** (ADR-0007): a comment dated after its item's own date, which no
/// adapter produces, is not looked for until the item next moves. Pinned by
/// `a_mention_is_looked_for_in_the_items_that_moved_inside_the_window`; the
/// stored names by `a_mention_is_found_through_the_names_its_body_stores`.
///
/// **The names were looked for through an index** (#1170). Read item by item
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
/// Pinned by `a_name_with_pattern_characters_is_matched_literally`. Both
/// tests stay, on the items the stored names and the window kept. Neither is
/// what finds them any more, so both are written over the body joined to an
/// empty text the names' subquery carries (`named.pad`): no index answers
/// that, so the planner is not offered the trigram index's estimate of a
/// handful of rows for what turns out to be thousands, and it names both
/// relations, so it is tested after their join and never on a body the
/// names did not lead to.
///
/// **When the comment was written is read last**, on the items every test
/// above kept. That join is fenced (`offset 0`), because `mentioned_at!`
/// walks an item's comments and the planner, offered its test beside the
/// others, ran it on each of the 1,227 items of the window before it joined
/// them to the 1,163 storing the name: 50 of the rule's 55 ms for 59
/// mentions.
macro_rules! mention {
    () => {
        concat!(
            r"select 'mention', i.entity_id, i.source_id, i.entity_id, i.kind, i.title,
                     'you are mentioned here',
                     m.at, i.web_url, null::text
                from (select i.entity_id, i.source_id, i.kind, i.title, i.web_url,
                             i.payload #> '{}'::text[] as payload,
                             i.item_updated_at, i.synced_at
                        from (select distinct mt.entity_id, ''::text as pad
                                from sync.item_mention mt
                               where mt.token = any (array(
                                       select substring(u from '^[A-Za-z0-9_.-]*')
                                         from unnest($1::text[]) as u))
                              offset 0) named
                        join sync.item i on i.entity_id = named.entity_id
                       where ",
            live_once!("i"),
            r"
                         and i.kind in ('ticket','pr','page','note')
                         and (i.item_updated_at >= $2 - make_interval(days => ",
            window_days!(),
            r")
                              or (i.item_updated_at is null
                                  and i.synced_at >= $2 - make_interval(days => ",
            window_days!(),
            r")))
                         and cardinality($1::text[]) > 0
                         and (i.body_text || named.pad) ~ replace(",
            mention_of!("array_to_string($1::text[], chr(1))"),
            r", chr(1), '|')
                         and (i.body_text || named.pad) ~ any (array(select ",
            mention_of!("u"),
            r" from unnest($1::text[]) as u))
                      offset 0) i
               cross join lateral ",
            mentioned_at!(),
            r" m
               where m.at >= $2 - make_interval(days => ",
            window_days!(),
            r")"
        )
    };
}

/// **When I was mentioned**, as a one-row relation `(at)` for the row `i`:
/// the moment a mention entered the item, not the moment the item last moved.
///
/// Before this, a mention was dated by the item's `updated`, so any move of a
/// ticket that had ever named me -- a worklog, a transition, a field edit, a
/// comment naming somebody else -- raised "you are mentioned here" afresh,
/// past *done* and inside the window, and opening it showed nothing new.
///
/// Where the record carries a thread, in either spelling the conversation
/// reads (`knobas_app::conversation`'s `COMMENTS`: Jira's
/// `fields.comment.comments[]` with `body` and `created`, the mock adapter's
/// `comments[]` with `text` and `when`):
///
/// 1. the newest dated comment that names me, by the rule's own per-name
///    expression;
/// 2. else the item's declared `created` (Jira's `fields.created`): the
///    mention is in the title or the description, which has no date of its
///    own. A mention edited into an old ticket's description is missed --
///    ADR-0007's direction, the one the trailing-`.` exclusion also takes --
///    rather than raised on every move of the ticket;
/// 3. else the item's own date, as before.
///
/// An item with no thread in the payload (Gitea, whose comments are folded
/// into `body_text` only; Confluence, whose comment walk re-emits the page)
/// keeps the item's date: nothing finer is on the record. A stamp that is not
/// a timestamp is no stamp (`pg_input_is_valid`), so one bad record cannot
/// fail the stream.
macro_rules! mentioned_at {
    () => {
        concat!(
            r"(select case when t.comments is null
                          then coalesce(i.item_updated_at, i.synced_at)
                          else coalesce(
                            (select max(coalesce(", stamp_of!("e.entry->>'created'"), ", ", stamp_of!("e.entry->>'when'"), r"))
                               from jsonb_array_elements(t.comments) as e(entry)
                              where coalesce(e.entry->>'body', e.entry->>'text') ~ any (array(select ",
            mention_of!("u"),
            r" from unnest($1::text[]) as u))),
                            ",
            stamp_of!($crate::declared_string!("$4", "created")),
            r",
                            coalesce(i.item_updated_at, i.synced_at))
                     end as at
                from (select coalesce(
                        case when jsonb_typeof(i.payload #> '{fields,comment,comments}') = 'array'
                             then i.payload #> '{fields,comment,comments}' end,
                        case when jsonb_typeof(i.payload -> 'comments') = 'array'
                             then i.payload -> 'comments' end) as comments) t)"
        )
    };
}

/// A text as a `timestamptz`, or null where it is not one.
macro_rules! stamp_of {
    ($text:expr) => {
        concat!(
            "(case when pg_input_is_valid(",
            $text,
            ", 'timestamptz') then (",
            $text,
            ")::timestamptz end)"
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
/// **What "failed" means**: the build's declared state chip
/// ([`crate::resolved_state!`] over `state_paths`, #2364) is in the `err`
/// tone, and a green one is in the `ok` tone, the reading the room signals'
/// `failing-build` makes (`err_builds!`). For TeamCity that is `FAILURE` and
/// `SUCCESS` on a finished build: its declaration maps `running` and `queued`
/// first, so a build in flight draws an `info` chip and has no verdict yet,
/// and a canceled one is `UNKNOWN`, `muted`, not a failure (`0002`-era
/// TeamCity work ratified `finished canceled` as its own reading, #105). A
/// kind that declares no state raises nothing, the ADR-0007 miss.
///
/// **The configuration** is the build's declared `pipeline` (TeamCity's
/// `buildTypeId`), resolved once a source as the CTE `pipeline_paths`. The
/// one payload key read here at a literal path is `statusText`, the item's
/// `detail`: no [`KindPaths`](crate::payload::KindPaths) role names it, and a
/// record without it draws `<title> failed` alone, a shorter line and never a
/// wrong one.
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
/// The newer build must itself be **finished**, its chip `ok`: the mirror holds
/// queued and running builds too, and a running build's `status` is TeamCity's
/// interim verdict -- a re-run that is green *so far* has not resolved anything, and
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
///
/// **Each build's state is read once** (the query audit of 2026-11-12).
/// `scored` names a build's tone three times above it, to keep the verdicts,
/// to find the greens and to keep the reds, and the planner, folding the
/// subquery into its reader, resolved the state once per mention: three
/// times a build, 44 ms for 2,531. The `offset 0` keeps `scored` a subquery
/// of its own.
///
/// **No build's payload is opened to rule it** (the stored-reads pass of
/// 2026-11-12). The rule read every live build's state and pipeline out of
/// its payload to find each configuration's latest green: 19 of the
/// stream's 40 ms on a mirror of 2,620 builds. The database already keeps
/// both, written with the build:
///
/// * the tone is the stored row's (`sync.item_row`, migration `0081`), read
///   under the key of the bound declarations ([`crate::row_join!`]), and the
///   payload's for a build with no row under it ([`crate::row_or!`]);
/// * the configuration is the stored pipeline (`sync.item_pipeline`,
///   migration `0062`), for a source whose adapter's stored declaration
///   gives a build the candidates the bound one does and frames it a run,
///   which is when the database keeps a row for every build that names one
///   (`pipeline_stored`); the payload's for any other source.
///
/// The status text is read for the builds the rule keeps. [`FAILED_BUILD_WALKED`]
/// is the rule as it read before, which `tests/it/inbox.rs` holds this one
/// to.
macro_rules! failed_build {
    () => {
        concat!(
            "(with state_paths as materialized (",
            $crate::state_paths!("$4"),
            "),
             pipeline_paths as materialized (
               select i.source_id, c.segs, c.ord
                 from (select d.source_id, 'build'::text as kind
                         from jsonb_object_keys(case when jsonb_typeof($4::jsonb) = 'object'
                                                     then $4::jsonb else '{}'::jsonb end)
                              as d(source_id)) i
                cross join lateral ",
            $crate::declared_candidates!("$4", "pipeline"),
            " c),
             ",
            $crate::row_keys!("$4"),
            ",
             pipeline_stored as materialized (
               select sc.id as source_id, a.adapter_kind
                 from knobas.source_config sc
                 join (select * from sync.pipeline_adapters() offset 0) a
                   on a.adapter_kind = sc.kind
                where 'build' = any(a.reached)
                  and a.kinds #> '{build,pipeline}'
                      = ($4::jsonb #> array[sc.id, 'build', 'pipeline']))
         select 'failed_build', i.entity_id, i.source_id, i.entity_id, i.kind, i.title,
                format('%s failed', i.title),
                i.at, i.web_url,
                (select nullif(x.payload->>'statusText', '') from sync.item x
                  where x.entity_id = i.entity_id)
           from (select scored.*,
                        max(scored.at) filter (where scored.tone = 'ok')
                          over (partition by scored.source_id collate \"C\", scored.config collate \"C\")
                          as latest_green
                   from (select i.entity_id, i.source_id, i.kind, i.title, i.author, i.web_url,
                                coalesce(i.item_updated_at, i.synced_at) as at,
                                ",
            $crate::row_or!("rw", "tone", $crate::resolved_state!(), "->>'tone'"),
            " as tone,
                                case when ps.source_id is null then ",
            $crate::first_string_of!(
                "(select p.segs, p.ord from pipeline_paths p where p.source_id = i.source_id)"
            ),
            " else ip.pkey end as config
                           from sync.item i",
            $crate::row_join!("rw", "i"),
            "
                           left join pipeline_stored ps on ps.source_id = i.source_id
                           left join sync.item_pipeline ip
                             on ip.entity_id = i.entity_id and ip.adapter_kind = ps.adapter_kind
                          where ",
            live_once!("i"),
            "
                            and i.kind = 'build'
                         offset 0) scored
                  where scored.tone in ('err', 'ok')) i
          where i.tone = 'err'
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
            and (i.config is null or i.latest_green is null or i.latest_green <= i.at))"
        )
    };
}

/// [`failed_build!`] as it read before it took the stored rows: every live
/// build's state, pipeline and status text out of its payload.
macro_rules! failed_build_walked {
    () => {
        concat!(
            "(with state_paths as materialized (",
            $crate::state_paths!("$4"),
            "),
             pipeline_paths as materialized (
               select i.source_id, c.segs, c.ord
                 from (select d.source_id, 'build'::text as kind
                         from jsonb_object_keys(case when jsonb_typeof($4::jsonb) = 'object'
                                                     then $4::jsonb else '{}'::jsonb end)
                              as d(source_id)) i
                cross join lateral ",
            $crate::declared_candidates!("$4", "pipeline"),
            " c)
         select 'failed_build', i.entity_id, i.source_id, i.entity_id, i.kind, i.title,
                format('%s failed', i.title),
                i.at, i.web_url, nullif(i.status_text, '')
           from (select scored.*,
                        max(scored.at) filter (where scored.tone = 'ok')
                          over (partition by scored.source_id collate \"C\", scored.config collate \"C\")
                          as latest_green
                   from (select i.entity_id, i.source_id, i.kind, i.title, i.author, i.web_url,
                                coalesce(i.item_updated_at, i.synced_at) as at,
                                ",
            $crate::resolved_state!(),
            "->>'tone' as tone,
                                i.payload->>'statusText' as status_text,
                                ",
            $crate::first_string_of!(
                "(select p.segs, p.ord from pipeline_paths p where p.source_id = i.source_id)"
            ),
            " as config
                           from sync.item i
                          where ",
            $crate::live_by_exclusion!("i"),
            "
                            and i.kind = 'build'
                         offset 0) scored
                  where scored.tone in ('err', 'ok')) i
          where i.tone = 'err'
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
            and (i.config is null or i.latest_green is null or i.latest_green <= i.at))"
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
///
/// **The assignee is the stored one where the database holds it** (the
/// stored-reads pass of 2026-11-12). The rule decompressed the payload of
/// every ticket of the window to read it, 477 of them and 5 of the stream's
/// 29 ms on a mirror of 23,000 tickets, where `sync.item_field` (migration
/// `0063`) has kept each ticket's declared assignee since #2184: the first
/// candidate's non-blank string, which is the value this rule's two tests
/// come to. For a source whose candidates the stored declarations hold
/// ([`crate::stored_under!`], `assignee_stored`) a ticket is mine when its
/// stored row under those candidates names me, one index probe a ticket;
/// any other source's tickets are walked as above. One row a ticket either
/// way after [`shelved!`]. [`NEW_ASSIGNMENT_WALKED`] is the rule as it read
/// before, which `tests/it/inbox.rs` holds this one to.
///
/// **The window is spelled so an index answers it** (the query audit of
/// 2026-11-12): the source's date inside it, or no source date and knobas'
/// own inside it, which is `coalesce(item_updated_at, synced_at)` inside it
/// and nothing else. Written as the `coalesce`, no index held the
/// expression and the rule scanned every item for the fortnight's tickets,
/// 26 ms on 39,770. `item_kind_updated_idx` reads both halves. The tickets
/// of the window are a subquery of their own (`offset 0`): joined flat to
/// `assignee_paths`, the planner started from the paths and walked every
/// ticket of each source to find the window's.
macro_rules! new_assignment {
    () => {
        concat!(
            "(with assignee_paths as materialized (",
            $crate::declared_ticket_paths!("$4", "assignee"),
            "),
             assignee_stored as materialized (
               select i.source_id,
                      $4::jsonb #> array[i.source_id, i.kind, 'assignee'] as paths
                 from (select d.source_id, 'ticket'::text as kind
                         from jsonb_object_keys(case when jsonb_typeof($4::jsonb) = 'object'
                                                     then $4::jsonb else '{}'::jsonb end)
                              as d(source_id)) i
                where ",
            $crate::stored_under!(
                "assignee",
                "($4::jsonb #> array[i.source_id, i.kind, 'assignee'])"
            ),
            ")
             select 'new_assignment', w.entity_id, w.source_id, w.entity_id, w.kind, w.title,
                    'assigned to you',
                    coalesce(w.item_updated_at, w.synced_at), w.web_url, null::text
               from (select i.entity_id, i.source_id, i.kind, i.title, i.web_url,
                            i.item_updated_at, i.synced_at
                       from sync.item i
                      where ",
            live_once!("i"),
            "
                        and i.kind = 'ticket'
                        and coalesce(i.author, '') <> all($1)
                        and (i.item_updated_at >= $2 - make_interval(days => ",
            window_days!(),
            ")
                             or (i.item_updated_at is null
                                 and i.synced_at >= $2 - make_interval(days => ",
            window_days!(),
            ")))
                     offset 0) w
               left join assignee_stored st on st.source_id = w.source_id
              where case when st.source_id is not null
                         then exists (select 1 from sync.item_field f
                                       where f.entity_id = w.entity_id
                                         and f.field = 'assignee'
                                         and f.paths = st.paths
                                         and f.value = any($1))
                         else exists (select 1
                                        from sync.item i
                                        join assignee_paths p on p.source_id = i.source_id
                                       where i.entity_id = w.entity_id
                                         and btrim(i.payload #>> p.segs) = any($1)
                                         and ",
            $crate::first_string_of!(
                "(select a.segs, a.ord from assignee_paths a where a.source_id = p.source_id)"
            ),
            " = any($1)) end)"
        )
    };
}

/// [`new_assignment!`] as it read before it took the stored assignee: every
/// ticket of the window's payload, decompressed, and its candidates walked.
macro_rules! new_assignment_walked {
    () => {
        concat!(
            "(with assignee_paths as materialized (",
            $crate::declared_ticket_paths!("$4", "assignee"),
            ")
             select 'new_assignment', i.entity_id, i.source_id, i.entity_id, i.kind, i.title,
                    'assigned to you',
                    coalesce(i.item_updated_at, i.synced_at), i.web_url, null::text
               from assignee_paths p
               join (select i.entity_id, i.source_id, i.kind, i.title, i.web_url,
                            i.payload #> '{}'::text[] as payload,
                            i.item_updated_at, i.synced_at
                       from sync.item i
                      where ",
            $crate::live_by_exclusion!("i"),
            "
                        and i.kind = 'ticket'
                        and coalesce(i.author, '') <> all($1)
                        and (i.item_updated_at >= $2 - make_interval(days => ",
            window_days!(),
            ")
                             or (i.item_updated_at is null
                                 and i.synced_at >= $2 - make_interval(days => ",
            window_days!(),
            ")))
                     offset 0) i on i.source_id = p.source_id
              where btrim(i.payload #>> p.segs) = any($1)
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
                    null::text, null::text
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
                null::text, null::text
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
/// **Which alerts.** Open (`closed_at is null`), **un-acked**, and of a
/// monitor nobody **hid** (`knobas.monitor_hidden`, `0066`): a hidden monitor
/// raises no inbox item and so no desktop notification, while its alert still
/// opens and closes for every estate surface. Ack is
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
/// the source it came from, for the same reason. `reason` names the asset and
/// says which trouble it is in, which is what makes the row readable without
/// opening it (story 7), and a monitor's `warn` is the *slow* of its chips;
/// `detail` is the path above the asset, the row's second line (F-115).
/// `entity_id` is the **asset**, not the
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
                    format('%s is %s', watched.name,
                           case a.state when 'warn' then 'slow' else a.state end),
                    a.opened_at, null::text, nullif(watched.path_text, '')
               from knobas.monitor_alert a
               join knobas.entity e on e.id = a.entity_id
               cross join lateral (
                     select ast.id, ast.name, ast.path_text
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
                and a.acked_at is null
                and not exists (select 1 from knobas.monitor_hidden h
                                 where h.entity_id = a.entity_id)"
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
/// two readers have to remember. `$3` null is **both shelves from one run**
/// ([`shelves`]): every row either read would return, a snoozed one carrying
/// its date and a stream one none, which is how the reader tells them apart.
/// The same expression decides, so the two halves are the two reads.
///
/// **Done.** Hidden while `done_at >= occurred_at`, so an item whose subject
/// has moved since it was answered comes back. See migration `0009` for why
/// that is a timestamp and not a boolean.
///
/// **A payload is opened once.** The review request, mention and new
/// assignment rules each fence the items they kept and hand on
/// `payload #> '{}'`, the record expanded, where the column itself is a
/// compressed value every later read expands again (`0076`'s fence): a
/// pull request's state reads it four times, and 182 of them took 12.6 ms
/// of the stream and take 2. The failed build rule keeps the column: a
/// build's record is under a kilobyte and stored as it is.
///
/// **The withdrawn entities are read once** (the stored-reads pass of
/// 2026-11-12). Each of the four rules that read the mirror excluded them
/// by a scan of `knobas.entity` of its own ([`crate::live_by_exclusion!`]):
/// four scans of 40,000 rows to find the same 291, 1.5 ms each of a stream
/// that takes 20. `gone` holds them, and each rule hashes it
/// ([`live_once!`]).
///
/// **De-duplication.** `distinct on (category, subject)` because a rule with a
/// lateral join can produce a row per match -- a pull request that lists an
/// account twice is one review request -- and because a category that later
/// grows a second rule must still produce one item per subject, or its key
/// would name two things.
macro_rules! shelved {
    ($($candidates:tt)*) => {
        concat!(
            "with gone (id) as materialized (
               select e.id from knobas.entity e where e.deleted_at is not null
             ),
             candidate (category, subject, source_id, entity_id, kind,
                             title, reason, occurred_at, web_url, detail) as (\n",
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
                      occurred_at, web_url, detail,
                      case when coalesce($3::boolean, snoozed_until > $2)
                           then snoozed_until end as snoozed_until
                 from answered
                where (done_at is null or done_at < occurred_at)
                  and ($3::boolean is null
                       or $3 = (snoozed_until is not null and snoozed_until > $2))
                order by category, subject, occurred_at desc
             )
             select key, category, source_id, entity_id, kind, title, reason,
                    occurred_at, web_url, detail, snoozed_until
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

/// [`review_request!`] as it read before `sync.item_reviewer`: every live
/// pull request's declared reviewers, out of its payload. Kept for
/// `tests/it/inbox.rs`, which holds the rule to it.
#[doc(hidden)]
pub const REVIEW_REQUEST_WALKED: &str = shelved!(concat!(
    "(with state_paths as materialized (",
    crate::state_paths!("$4"),
    ")
     select 'review_request', i.entity_id, i.source_id, i.entity_id, i.kind, i.title,
            format('%s asked for your review', coalesce(nullif(i.author, ''), 'somebody')),
            coalesce(i.item_updated_at, i.synced_at), i.web_url, null::text
       from sync.item i
       cross join lateral ",
    crate::declared_list!("$4", "reviewers"),
    " as r
      where ",
    crate::live_by_exclusion!("i"),
    "
        and i.kind = 'pr'
        and case when r.value = any($1)
                 then coalesce(",
    crate::resolved_state!(),
    "->>'text', 'open') = 'open'
                 else false end)"
));

/// [`failed_build!`] as it read before it took the stored rows
/// (`sync.item_row`, `sync.item_pipeline`): every live build's state and
/// pipeline, out of its payload. Kept for `tests/it/inbox.rs`, which holds
/// the rule to it.
#[doc(hidden)]
pub const FAILED_BUILD_WALKED: &str = shelved!(failed_build_walked!());

/// [`new_assignment!`] as it read before it took the stored assignee
/// (`sync.item_field`): every ticket of the window's payload. Kept for
/// `tests/it/inbox.rs`, which holds the rule to it.
#[doc(hidden)]
pub const NEW_ASSIGNMENT_WALKED: &str = shelved!(new_assignment_walked!());

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

/// The stream and the snoozed shelf, read together.
#[derive(Debug, Clone, Default)]
pub struct Shelves {
    /// What needs you now, newest first: [`items`] of [`Shelf::Stream`].
    pub stream: Vec<InboxItem>,
    /// What you deferred, newest first: [`items`] of [`Shelf::Snoozed`].
    pub snoozed: Vec<InboxItem>,
}

/// Both shelves from **one run** of the derivation (the query audit of
/// 2026-11-12). The window asked for the stream, the snoozed shelf and the
/// count as three reads, and the stream and its count each ran every rule:
/// 330 ms twice on a mirror of 39,770 items, on two of the pool's five
/// connections, at every sync's end. The count is the stream's length.
///
/// The statement is [`items`]' with no shelf named (`shelved!`, `$3` null),
/// so each half is what that read answers: `the_two_shelves_from_one_run_
/// are_the_two_reads` in `tests/it/inbox.rs` holds them together. Planned for
/// its values, as [`items`] is.
///
/// # Errors
///
/// [`CoreError::Db`] if the query fails.
pub async fn shelves(
    pool: &PgPool,
    identity: &[String],
    now: DateTime<Utc>,
    declarations: &crate::payload::Declarations,
) -> Result<Shelves, CoreError> {
    let rows = sqlx::query_as::<_, InboxItem>(ALL_RULES)
        .persistent(false)
        .bind(identity)
        .bind(now)
        .bind(None::<bool>)
        .bind(declarations.as_param())
        .fetch_all(pool)
        .await?;
    let (snoozed, stream) = rows
        .into_iter()
        .partition(|item| item.snoozed_until.is_some());
    Ok(Shelves { stream, snoozed })
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

/// Forget an answer: no snooze and no *done*, so the item is what its rule says.
///
/// Deletes the row rather than nulling its columns: a row that decides nothing
/// may not exist (`inbox_state_decision_chk`, migration `0009`). #446's ack is
/// [`complete_with`]'s caller, and taking an ack back is this one's, in that
/// write's transaction for the same reason.
///
/// # Errors
///
/// [`CoreError::Db`] if the write fails.
pub async fn reopen_with<'e, E>(executor: E, key: &str) -> Result<(), CoreError>
where
    E: sqlx::PgExecutor<'e>,
{
    sqlx::query("delete from knobas.inbox_state where item_key = $1")
        .bind(key)
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
