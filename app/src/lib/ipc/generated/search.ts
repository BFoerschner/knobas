// Generated from the Rust types by ts-rs; do not edit. Regenerate with `cargo test -p knobas-app --lib ipc_mirror` (crates/knobas-app/src/ipc_mirror.rs).
import type { EntityRow, StateChip } from "./entity";
import type { CredentialHealth } from "./sources";

/**
 * [`CARD_FIELDS`]: what a card can show (R-3.16).
 */
export type CardField = "type" | "signals" | "comments" | "assignee" | "epic" | "labels";

/**
 * One value a ticket key can take (#811).
 */
export type FieldSuggestion = { 
/**
 * The key as the grammar spells it, without the colon: `status`.
 */
key: string, 
/**
 * What the box completes the key with: `In Review`, `PAY`, `@me`.
 */
value: string, 
/**
 * What is drawn beside it: a project's name, `you` for `@me`, or empty.
 */
detail: string, };

/**
 * Whether a source's corpus can answer a filter at all.
 *
 * **These two are the whole of issue #141**: collapsing them is the defect
 * this type exists to remove. An `author:` query that comes back empty is
 * *honest* for an [`Answered`](FilterAnswer::Answered) source and
 * *unanswerable* for a [`NoValues`](FilterAnswer::NoValues) one, and nothing
 * on the wire said which until now.
 *
 * There is deliberately **no third variant** for a source that contributed no
 * rows to the query at all. Such a source is left out of
 * [`FilterCoverage::sources`] instead: whatever it is missing from the
 * results, the filter is not the reason, and a verdict on it would explain the
 * wrong absence.
 *
 * - `answered`: the source's corpus carries values this dimension matches
 *   on, so an empty result for it means nobody matched -- for `author:`,
 *   *nobody by that name*.
 * - `no_values`: the source put rows in this query's corpus and **not one of
 *   them** carries such a value -- *this source cannot be asked*. Measured on
 *   the corpus, not declared about the source: this is why issue #141 was
 *   ruled onto the response rather than onto the descriptor.
 *
 * Documented here and not on each variant for [`FilterDimension`]'s reason.
 */
export type FilterAnswer = "answered" | "no_values";

/**
 * What each source in a query's scope could do with **one** filter dimension.
 *
 * One entry per *reported* dimension, and a dimension is reported only when
 * the query actually filtered on it. Issue #141 scopes the behaviour to
 * [`FilterDimension::Author`]; the list shape is what makes a second dimension
 * an addition rather than a reshape.
 */
export type FilterCoverage = { dimension: FilterDimension, 
/**
 * Every source that put rows in this query's corpus, **ordered by id** --
 * the vocabulary's own order, which is the order the sources list shows.
 *
 * A configured source the query's `source:` or kind scope left with
 * nothing is **not** here, and that is not an omission: it contributed no
 * corpus, so this dimension is not why it is absent from the results.
 * Neither is a **disabled** source — the report's scope is the
 * vocabulary's, which reads enabled sources only. Since migration
 * `0012`, `sync.live_item` reads enabled sources only too, so a disabled
 * source's rows match nothing, and this report does not verdict it.
 */
sources: Array<SourceAnswer>, };

/**
 * A filter dimension whose coverage is reported.
 *
 * Deliberately an enum rather than a bare string: the wire vocabulary is
 * closed, so a UI that branches on it cannot be handed a word nobody defined.
 * Two variants since #595; a further dimension appends one (#141's entry in
 * §10.8).
 *
 * - `author` (since #141): `@jonas`, `author:jonas`, an author chip, or
 *   `@me` -- everything that ends up in the one `author = any(...)`
 *   predicate.
 * - `under` (ADR-0018, #595): `under:<name>`. Answered by a source some live
 *   item of which names a parent (measured on the corpus since #642); a
 *   source whose corpus names none -- an Uptime Kuma, whose metrics have no
 *   grouping, or a classic Jira with no Epic Link field configured -- has
 *   nothing the filter can match, and an empty `under:` there is not an
 *   empty repo. The second dimension #141's list shape was built for: an
 *   entry and a variant, nothing existing moved.
 *
 * The variants are documented here and not on each variant because the
 * generated TypeScript (`app/src/lib/ipc/generated/search.ts`) carries a
 * type's and a field's doc but not a variant's (#1325).
 */
export type FilterDimension = "author" | "under";

/**
 * The values the query box suggests after `kind:`, `source:` and `under:`
 * (#601, spec #591 stories 27-30).
 *
 * **Only values that exist right now**, so a picked suggestion never leads
 * to an empty result or an unknown token: the kinds the corpus has live rows
 * of, the enabled sources that have a live row, and the parents the
 * mirror and the estate declare. The kinds and sources are read once per
 * launcher opening and narrowed by the box; the parents are asked again per
 * keystroke of an `under:` value, by prefix (#761) and alone (#768).
 * Free-text keys (`author:`, `updated:`) get no suggestions and have no list
 * here.
 */
export type FilterSuggestions = { 
/**
 * Every kind with at least one row in the launcher's corpora, by id.
 */
kinds: Array<KindSuggestion>, 
/**
 * Every enabled source with at least one live row, ordered by id -- the
 * sources `source:` resolves that lead somewhere (#614).
 */
sources: Array<SourceSuggestion>, 
/**
 * The newest [`crate::PARENT_SUGGESTIONS`] names a parent answers to that
 * start with the prefix asked for, ignoring case, each with the parent's
 * kind, newest first (#761). A parent with two names (a Gitea repo's
 * `owner/name` and `name`) is listed once per name that has the prefix.
 */
parents: Array<ParentSuggestion>, 
/**
 * The values the ticket keys can take (#811): for each of `project:`,
 * `status:`, `issuetype:`, `priority:`, `assignee:` and `label:` the
 * most used [`crate::FIELD_SUGGESTIONS`] values the live mirror holds,
 * in the source's spelling; `assignee:`'s `@me` first; `sprint:`'s
 * `current` while a ticket is in an active sprint; `assigned:`'s
 * `today` and `week`. Read with the kinds and the sources, once per
 * opening, and empty from a parents-only read.
 */
fields: Array<FieldSuggestion>, };

/**
 * One kind `kind:` can name.
 */
export type KindSuggestion = { 
/**
 * The kind id, which is what the box completes `kind:` with.
 */
kind: string, 
/**
 * The catalog's label for it, which is what a result group says.
 */
label: string, };

/**
 * What an empty launcher box shows (interfaces §2.4).
 *
 * **The one DTO in this stream that composes two streams' data**, which is
 * why it lives in the command module rather than in `knobas-search`:
 * `smart_lists` and `recent` are the search crate's [`LauncherBoard`],
 * `sources` is stream F's [`CredentialHealth`], and `pending_writes` counts
 * stream G's write queue. `knobas-search` takes a `PgPool` and deliberately
 * depends on none of those crates.
 *
 * [`LauncherBoard`]: knobas_search::LauncherBoard
 * [`CredentialHealth`]: knobas_sync::CredentialHealth
 */
export type LauncherHome = { smart_lists: Array<SmartListSummary>, 
/**
 * `knobas_search::EntityRow`, a *different Rust struct* from
 * `knobas_core::entity_list::EntityRow` with the same eight fields on
 * the wire. The shell has one type for both, the room's, which is the
 * truth about the JSON; `the_two_entity_rows_are_one_wire_shape` fails
 * if the two ever stop agreeing.
 */
recent: Array<EntityRow>, 
/**
 * Stream F's credential health, read straight from `knobas.source_config`
 * -- **not** a second copy of the type (interfaces §2.2, open question
 * **E-Q5**, resolved: `knobas_sync::CredentialHealth` has merged, so the
 * planned field-identical stand-in is not needed).
 */
sources: Array<CredentialHealth>, 
/**
 * What the launcher's footer counts in *"mirror · N pending writes"*:
 * the writes knobas still owes a source and will send on its own.
 *
 * [`QueueCounts::pending`] alone, never the sum of the three open states.
 * A pending write asks the user for patience; a held or refused one asks
 * for a *decision*, and the status bar's badge is where those are put
 * (`QueueCounts`: "3 waiting" may never absorb a held write). Folding
 * them in would let this number fall to zero with nothing sent.
 *
 * [`QueueCounts::pending`]: knobas_core::write_queue::QueueCounts::pending
 */
pending_writes: number, 
/**
 * Whether *Show notes and quick capture* is on (#1036, R-13a.6): the one
 * read of `capture.notes_shown` the board's own searcher already made.
 * The launcher drops its `note:` syntax row and Help entry while this is
 * `false`, because typing the prefix then finds nothing.
 */
notes_shown: boolean, };

/**
 * One part of the source's query knobas left out, as the source spells it,
 * and why.
 */
export type LeftOut = { 
/**
 * The part, as the source's query spells it.
 */
clause: string, 
/**
 * Why knobas cannot test it, in the reader's words.
 */
reason: string, };

/**
 * [`LIST_COLUMNS`]: the list's columns (R-3.16); `key` is always one.
 */
export type ListColumn = "key" | "summary" | "status" | "priority" | "signals" | "assignee" | "updated" | "epic" | "labels";

/**
 * The list's header sort (R-3.6).
 */
export type ListSort = { column: ViewSortColumn, direction: "ascending" | "descending", };

/**
 * One name `under:` can scope by (ADR-0018).
 */
export type ParentSuggestion = { 
/**
 * The name the parent answers to, as its source spells it.
 */
name: string, 
/**
 * The parent's kind, so `payout-service · repo` and a same-named page can
 * be told apart. For a parent outside the mirror it is the kind its
 * children declare, which is why `ParentPath` carries one (#594).
 */
kind: string, };

/**
 * What the backend made of the raw text -- echoed back so the UI renders the
 * chips it inferred rather than guessing at the same grammar twice.
 */
export type ParsedQuery = { 
/**
 * The search terms with the grammar stripped out.
 */
text: string, prefix: Prefix | null, filters: SearchFilters, 
/**
 * `key:value` pairs the parser did not recognise, so the UI can say so
 * instead of silently ignoring them.
 */
unknown_tokens: Array<string>, };

/**
 * The launcher's prefixes (§4): `>` `#` `@` `/` `t ` `note:` `list:` `asset:` `?`.
 */
export type Prefix = "action" | "ticket" | "person" | "source" | "time" | "note" | "list" | "asset" | "help";

/**
 * One condition: a key, *is* or *is not*, and values any of which it means.
 */
export type QueryCondition = { 
/**
 * The key it tests.
 */
key: QueryKey, 
/**
 * *Is not*: `-key:value` in the grammar.
 */
negated: boolean, 
/**
 * The values, any of which it means, as the grammar reads them.
 */
values: Array<string>, };

/**
 * A key of the search grammar a condition tests, as the grammar spells it
 * (`under:PAY-12`, `label:on-call`), #1208.
 *
 * - `project`: `project:PAY`.
 * - `status`: `status:Done`.
 * - `issuetype`: `issuetype:Bug`.
 * - `priority`: `priority:High`.
 * - `assignee`: `assignee:me` or an account.
 * - `under`: the tickets under an epic or parent.
 * - `label`: `label:on-call`.
 * - `sprint`: `current` is the active sprint.
 * - `has`: a knobas signal (`has:review`).
 * - `updated`: a window, `7d` or `2w`.
 * - `assigned`: assigned to me within a window.
 *
 * The keys are documented here and not on each variant because the
 * generated TypeScript (`app/src/lib/ipc/generated/search.ts`) carries a
 * type's and a field's doc but not a variant's (#1325).
 */
export type QueryKey = "project" | "status" | "issuetype" | "priority" | "assignee" | "under" | "label" | "sprint" | "has" | "updated" | "assigned";

/**
 * A saved view's query as knobas can test it, read into the search grammar
 * by its own source (#1208): the conditions, in the query's order, how many
 * of the query's parts they keep, and the parts left out.
 */
export type QueryTranslation = { 
/**
 * What a copy tests, in the query's order.
 */
conditions: Array<QueryCondition>, 
/**
 * How many of the query's parts the conditions keep: a part, not a
 * condition, since two parts may read into one condition.
 */
kept: number, 
/**
 * The parts it leaves out, in the query's order.
 */
left_out: Array<LeftOut>, };

/**
 * Results of one entity kind, with the display metadata the launcher renders
 * from (§3a: never a hardcoded kind list).
 */
export type ResultGroup = { kind: string, label: string, plural: string, monogram: string, 
/**
 * Matches of this kind, before `limit` was applied.
 */
total: number, hits: Array<SearchHit>, };

/**
 * What a ticket system says about one of its saved views (#877, #1273,
 * `sync.saved_view`): enough for the views pane and *View settings* to say
 * where it comes from.
 */
export type SavedViewSource = { 
/**
 * `board` or `filter`.
 */
kind: "board" | "filter", 
/**
 * The ticket system it comes from, as its group names it (`Jira`, #2032):
 * the kind's declared `system_name`, else the adapter's name, by
 * `knobas_sync::saved_views::declared`; the source's display name when its
 * adapter is not compiled in or declares no saved views. What the shell's
 * words about the view name: *Open in Jira*, *Jira decides which tickets*.
 */
system: string, 
/**
 * `scrum`, `kanban` or `simple` for a board; `None` for a filter.
 */
board_type: string | null, 
/**
 * The saved view's query in the source's own language, as the source
 * holds it (Jira's JQL); empty when the source sends none. Read from
 * `sync.saved_view.source_query`, the column the adapter's payload fills.
 */
source_query: string, 
/**
 * A Kanban board's sub-filter, or `None`.
 */
sub_query: string | null, 
/**
 * A Scrum board's active sprint, `{"id", "name", "end_date"}`, or `None`.
 */
sprint: { id: number, name: string, end_date: string | null, } | null, 
/**
 * When the last sync read it (RFC 3339).
 */
last_read: string, 
/**
 * Where the source shows it, or `None`.
 */
web_url: string | null, 
/**
 * *Hide from the list* (#878, R-3.25): it keeps syncing, and the views
 * pane draws it as one line at the bottom with *Show*.
 */
hidden: boolean, 
/**
 * Turned up after its source's first sync and not opened yet (#878,
 * R-3.20).
 */
new: boolean, 
/**
 * The tickets the source put on it that knobas does not have, per project the
 * source does not sync, most first (#879, R-3.23): the Tickets warn line
 * and *Boards and filters*' "and 9 in WEB, not synced" (#878, R-13a.2).
 * A missing ticket of a project the source syncs is a sync still to come.
 */
unsynced: Array<UnsyncedTickets>, };

/**
 * The filters a query is narrowed by, whether typed inline or clicked.
 */
export type SearchFilters = { sources: Array<string>, kinds: Array<string>, updated_within_days: number | null, mine: boolean, 
/**
 * The people the query **named** -- `@jonas`, `author:jonas`, `owner:`,
 * `by:`, or an author chip (ruling **E-Q1**).
 *
 * A username as the source spells it, matched exactly, because that is
 * what `sync.item.author` holds and what `mine` already matches against.
 *
 * **What a source can answer depends on the source** (issue #106): a
 * TeamCity item carries an author only when a person pressed Run, which
 * on a real server is effectively never -- 100 of 100 of the newest
 * finished builds on JetBrains' public instance named no user. So an
 * author query narrowed to a corpus of builds legitimately comes back
 * empty, and that is the data being sparse rather than this filter being
 * broken. Ruled and left as it is on 2026-08-29; making the emptiness
 * visible in the response is a change to this frozen schema and is
 * Björn's call, not one taken here.
 *
 * [`Self::mine`] is a *separate* dimension and its resolved identity
 * never appears here: the echo is what the launcher redraws its chips
 * from, and putting usernames the user never typed into that row would
 * have the chips claim a filter nobody wrote.
 *
 * The one field here carrying `#[serde(default)]`, because it is the one
 * field that was **added** to a frozen struct (§10.8): a caller written
 * against the four-field shape sends no `authors`, and refusing its query
 * outright is a worse answer than reading the absence as "named nobody".
 * The generated TypeScript declares it required, so the launcher always
 * sends it -- the default is the wire being permissive, not the frontend
 * being allowed to forget.
 */
authors: Array<string>, };

/**
 * One result: a row, its rank, and the excerpt with the match marked.
 */
export type SearchHit = { rank: number, snippet: Array<Segment>, entity_id: string, kind: string, source_id: string, title: string, 
/**
 * When the source says the item changed, an RFC 3339 timestamp; `None`
 * if the source never dated the item.
 */
updated_at: string | null, 
/**
 * When knobas last saw it, an RFC 3339 timestamp -- the per-row
 * provenance §4 requires ("synced 4 min ago").
 */
synced_at: string, 
/**
 * Where this row sits **inside its source**, as one line -- a Confluence
 * page's ancestor path, `Engineering \u{203a} Payments` (#284, ratified
 * as a contract §10.8 exception under that ticket's criterion 5).
 *
 * Since #596 it is the **parent chain** the row's kind declares
 * (ADR-0018) -- `owner/repo` for a Gitea branch, pull request or commit
 * -- drawn by one SQL expression, `knobas_core::path_line_read!`, so the
 * launcher row and the detail panel cannot disagree about what a path
 * is. A page's chain is
 * its `ancestors`, which Confluence declares (#599, and #646 retired the
 * undeclared ancestor read). `None` for every row whose kind declares no
 * parent or whose declared parent misses: the ADR-0007 **miss**, so a
 * launcher row shows no path rather than a wrong one, and nothing
 * downstream has to ask what kind it is holding.
 *
 * `#[serde(default)]`, so a stored or piped row written before this
 * existed still decodes -- as a row that sits nowhere, which is the same
 * thing a miss says.
 */
path: string | null, 
/**
 * The item's state chip (#666) -- a pull request's `open` / `merged` /
 * `closed`, a build's `running` or outcome -- read by
 * `knobas_core::declared_state!` beside the path line. `None` for a kind
 * that declares no state, a value the declaration does not name, and
 * every row of a corpus knobas owns: the ADR-0007 **miss**.
 * `#[serde(default)]` for `path`'s reason.
 */
state: StateChip | null, };

/**
 * What the launcher asks for: the raw box text, plus whatever the UI already
 * knows (chips the user clicked).
 */
export type SearchQuery = { 
/**
 * Exactly what is in the box, prefixes and all. **The backend parses it**
 * -- see the module docs.
 */
raw: string, limit: number, filters: SearchFilters, };

/**
 * One answered query.
 */
export type SearchResponse = { interpreted: ParsedQuery, groups: Array<ResultGroup>, 
/**
 * Matches across every kind, before `limit` was applied.
 */
total: number, took_ms: number, 
/**
 * Which sources could answer the filters this query narrowed by, for the
 * dimensions where "could not" is a thing that happens (issue #141).
 *
 * **Empty means there was nobody to report on**: no reportable dimension
 * was filtered on -- the ordinary case, and why an ordinary keystroke pays
 * nothing for this -- or no source put rows in this query's corpus, which
 * is a scope that matched nothing rather than a filter that could not be
 * answered. A dimension that *was* filtered on is here with **every**
 * contributing source listed, whatever each answered, so a reader can tell
 * "measured, and they all answered" from "not measured". A list pruned to
 * the failures could not.
 *
 * `#[serde(default)]` for the same reason [`SearchFilters::authors`]
 * carries it: this field was **added** to a frozen struct (§10.8), and a
 * peer that sends no `coverage` means "nothing to report" rather than a
 * response worth refusing. The generated TypeScript declares it required,
 * and the backend always sends it.
 */
coverage: Array<FilterCoverage>, };

/**
 * A run of excerpt text, and whether it is part of the match.
 *
 * **XSS-unsafe: `text` is raw source text** -- whatever a person typed into
 * a ticket, `<script>` included. Render it as text (`{segment.text}`), never
 * with `{@html}`; the highlighting is the `hit` flag, and no markup crosses
 * the bridge (roadmap §4 gotcha 7).
 */
export type Segment = { text: string, hit: boolean, };

/**
 * One entry of the launcher board's smart-list rail (interfaces §2.4).
 *
 * Built-in and saved lists are **one shape** (#506, story 57): both are
 * opened by `list:<id>`, both carry a count and a change badge, and
 * [`Self::saved`] is the only thing that tells them apart.
 */
export type SmartListSummary = { 
/**
 * Stable id; also what `list:<id>` in the box names.
 */
id: string, label: string, 
/**
 * How many items are in the list right now.
 */
count: number, 
/**
 * Something in this list is newer than the last time it was opened.
 */
changed: boolean, 
/**
 * Display text: the list's blurb, the reason it is empty, or -- for a
 * saved list -- the query it stands for.
 */
description: string, 
/**
 * Whether somebody saved this list, rather than knobas shipping it
 * (#506).
 *
 * The one thing the rail needs to tell the two apart, and it is a
 * *capability* rather than a provenance note: a saved list can be renamed
 * and deleted and a built-in cannot, so the row draws those two controls
 * exactly when this is `true`. Both kinds are opened the same way, by
 * `list:<id>`, which is why the id is not what says which one this is.
 */
saved: boolean, 
/**
 * The saved query cannot be run by today's grammar (#506, story 60).
 *
 * Always `false` for a built-in: a built-in's predicate is compiled in, so
 * there is no version of it the launcher could fail to understand.
 * [`Self::count`] is `0` and [`Self::changed`] `false` on such a row --
 * not because the list is empty, but because nothing was asked -- and
 * [`Self::description`] carries [`crate::saved::NEEDS_ATTENTION`] and the
 * reason, the rule that refused it. The row does not open.
 */
needs_attention: boolean, };

/**
 * One source, and what it could do with the dimension it is listed under.
 */
export type SourceAnswer = { 
/**
 * The configured instance id, which is also `SearchHit.source_id`.
 */
source_id: string, 
/**
 * The name the sources list shows. Carried rather than looked up: the
 * launcher has `CredentialHealth` per source and that DTO has no name in
 * it, so a UI that had to say *Buildserver* would otherwise print an id.
 */
display_name: string, answer: FilterAnswer, };

/**
 * One source `source:` can name.
 */
export type SourceSuggestion = { 
/**
 * The instance id, which is what the box completes `source:` with: an
 * exact id resolves to that instance and nothing else.
 */
id: string, 
/**
 * The name the sources list shows.
 */
display_name: string, };

/**
 * One ticket view as stored.
 */
export type TicketView = { 
/**
 * A slug: `#/tickets/<id>` opens the view.
 */
id: string, name: string, 
/**
 * *Mine*, *Team*, *Epics*, *Projects* or a group somebody typed.
 */
group: string, 
/**
 * Where the view sits in the list; lower first.
 */
position: number, 
/**
 * The conditions as search text, exactly as saved; empty is every ticket.
 */
query: string, 
/**
 * `board` or `list`.
 */
layout: "board" | "list", 
/**
 * The board's columns; empty is one column per status.
 */
columns: Array<ViewColumn>, lanes: ViewLanes, card_fields: Array<CardField>, list_columns: Array<ListColumn>, 
/**
 * `None` is newest first.
 */
list_sort: ListSort | null, 
/**
 * The saved view (a board or filter, `sync.saved_view`'s entity id) a
 * view from a ticket system shows (#877; named for the role by #1245);
 * `None` on a view somebody made.
 */
saved_view: string | null, };

/**
 * What `save_ticket_view` takes: a view, with the id and position left to
 * the save when it is a new one.
 */
export type TicketViewDraft = { 
/**
 * `None` makes a new view with an id from its name; an id saves over that
 * view, or puts a deleted one back (*Undo*).
 */
id: string | null, name: string, group: string, 
/**
 * `None` keeps a saved view where it is and puts a new one last.
 */
position: number | null, query: string, layout: "board" | "list", columns: Array<ViewColumn>, lanes: ViewLanes, card_fields: Array<CardField>, list_columns: Array<ListColumn>, list_sort: ListSort | null, saved_view: string | null, };

/**
 * One row of the views pane: the view, and what its text reads as today.
 */
export type TicketViewRow = { 
/**
 * How many tickets the text admits; `None` when the grammar could not
 * read it.
 */
count: number | null, 
/**
 * The part of the text the grammar could not read, or `None`.
 */
unreadable: string | null, 
/**
 * What the source says about the saved view a view shows (#877, #1273),
 * or `None` on a view somebody made.
 */
saved_view_source: SavedViewSource | null, 
/**
 * A slug: `#/tickets/<id>` opens the view.
 */
id: string, name: string, 
/**
 * *Mine*, *Team*, *Epics*, *Projects* or a group somebody typed.
 */
group: string, 
/**
 * Where the view sits in the list; lower first.
 */
position: number, 
/**
 * The conditions as search text, exactly as saved; empty is every ticket.
 */
query: string, 
/**
 * `board` or `list`.
 */
layout: "board" | "list", 
/**
 * The board's columns; empty is one column per status.
 */
columns: Array<ViewColumn>, lanes: ViewLanes, card_fields: Array<CardField>, list_columns: Array<ListColumn>, 
/**
 * `None` is newest first.
 */
list_sort: ListSort | null, 
/**
 * The saved view (a board or filter, `sync.saved_view`'s entity id) a
 * view from a ticket system shows (#877; named for the role by #1245);
 * `None` on a view somebody made.
 */
saved_view: string | null, };

/**
 * How many tickets on a saved view are in one project the source does not
 * sync (#879, R-3.23).
 */
export type UnsyncedTickets = { 
/**
 * The project key, read off the ticket key (`WEB-5` → `WEB`).
 */
project: string, count: number, };

/**
 * One ticket board column: its name, the statuses on it, and an optional
 * *At most* (R-3.15).
 */
export type ViewColumn = { name: string, statuses: Array<string>, 
/**
 * *At most*, or `None`.
 */
limit: number | null, };

/**
 * [`LANES`]: what a ticket board's lanes split by (R-3.5).
 */
export type ViewLanes = "none" | "epic" | "assignee" | "priority" | "issuetype" | "label";

/**
 * [`SORT_COLUMNS`]: what a list header sorts by (R-3.6).
 */
export type ViewSortColumn = "key" | "summary" | "status" | "priority" | "assignee" | "updated";
