/** Search and the launcher — `crates/knobas-app/src/commands/search.rs`. */
import { invoke } from "@tauri-apps/api/core";

import type { EntityRow, StateChip } from "./entity";
import type { CredentialHealth } from "./sources";

/** `knobas_search::SearchFilters`. */
export interface SearchFilters {
  sources: string[];
  kinds: string[];
  updated_within_days: number | null;
  mine: boolean;
  /**
   * The people the query named — `@jonas`, `author:jonas`, `owner:`, `by:`.
   *
   * `mine` is a separate dimension and never lands here: the backend resolves
   * it to the configured usernames on its own, and echoing those would put
   * names the user never typed into the chip row.
   */
  authors: string[];
}

/** `knobas_search::SearchQuery` — the raw box text; the backend parses it. */
export interface SearchQuery {
  raw: string;
  limit: number;
  filters: SearchFilters;
}

/** The launcher's prefixes: `>` `#` `@` `/` `t ` `note:` `list:` `asset:` `?`. */
export type Prefix =
  | "action"
  | "ticket"
  | "person"
  | "source"
  | "time"
  | "note"
  | "list"
  | "asset"
  | "help";

/** What the backend made of the raw text, echoed so the UI renders its chips. */
export interface ParsedQuery {
  text: string;
  prefix: Prefix | null;
  filters: SearchFilters;
  unknown_tokens: string[];
}

/**
 * A run of excerpt text and whether it is part of the match.
 *
 * XSS-UNSAFE. `text` is *source* text: an excerpt of whatever a person typed
 * into a ticket, `<script>` included. Render it as text (`{segment.text}`),
 * never with `{@html}`. The highlight is the `hit` flag — no markup crosses
 * the bridge.
 */
export interface Segment {
  text: string;
  hit: boolean;
}

/** `knobas_search::EntityRow`, inlined into every hit by `#[serde(flatten)]`. */
export interface SearchHit {
  entity_id: string;
  kind: string;
  source_id: string;
  /** RFC 3339 timestamp, or null if the source never dated the item. */
  updated_at: string | null;
  /** RFC 3339 timestamp — the row's provenance ("synced 4 min ago"). */
  synced_at: string;
  title: string;
  /**
   * Where the row sits **inside its source**, as one line — a Confluence
   * page's ancestor path, `Engineering › Payments` (#284) — and, since #596,
   * the parent chain any kind declares (ADR-0018): `owner/repo` for a Gitea
   * branch, pull request or commit.
   *
   * `null` for every row whose kind declares no parent and whose record
   * carries no readable `ancestors`: the ADR-0007 **miss**, so a row shows no
   * path rather than a wrong one and no caller has to ask what kind it holds.
   * Joined by one SQL expression, `knobas_core::path_line_read!`, so the
   * launcher row and the detail panel cannot disagree about what a path is.
   */
  path: string | null;
  /** The item's state chip, or `null` — `EntityRow.state` in `entity.ts` (#666). */
  state: StateChip | null;
  rank: number;
  snippet: Segment[];
}

/** Results of one entity kind, with the display metadata to render them. */
export interface ResultGroup {
  kind: string;
  label: string;
  plural: string;
  monogram: string;
  /** Matches of this kind before `limit` was applied. */
  total: number;
  hits: SearchHit[];
}

/**
 * A filter dimension search reports coverage for — `knobas_search::FilterDimension`.
 *
 * `author` since #141, and `under` since #595: a source that has synced
 * nothing naming a parent cannot answer `under:<name>` (ADR-0018; measured on
 * the corpus since #642). The union is
 * what made the second dimension an addition rather than a reshape.
 */
export type FilterDimension = "author" | "under";

/**
 * What one source could do with a filter — `knobas_search::FilterAnswer`.
 *
 * The distinction issue #141 exists for: an empty `author:` result means
 * *nobody by that name* for `answered` and *this source cannot be asked* for
 * `no_values`, and nothing on the wire said which until this field.
 *
 * - `answered` — the source's corpus carries authors, so an empty result is a
 *   real answer.
 * - `no_values` — it put rows in this query's corpus and not one of them names
 *   a person.
 *
 * A source that contributed no rows at all has no member here: it is left out
 * of `FilterCoverage.sources` entirely, because the filter is not why it is
 * missing from the results.
 */
export type FilterAnswer = "answered" | "no_values";

/** One source's verdict — `knobas_search::SourceAnswer`. */
export interface SourceAnswer {
  source_id: string;
  /** The name the sources list shows — `CredentialHealth` carries none. */
  display_name: string;
  answer: FilterAnswer;
}

/** What every scoped source could do with one filter — `knobas_search::FilterCoverage`. */
export interface FilterCoverage {
  dimension: FilterDimension;
  /**
   * Every source that put rows in this query's corpus, ordered by id.
   *
   * A configured source the query's `source:` or kind scope left with nothing
   * is not here — it contributed no corpus, so this dimension is not why it is
   * absent from the results. Neither is a disabled source: the report's scope
   * is the vocabulary's (enabled sources only), so a source the user turned
   * off is never verdicted even where its still-mirrored rows match a search.
   */
  sources: SourceAnswer[];
}

export interface SearchResponse {
  interpreted: ParsedQuery;
  groups: ResultGroup[];
  total: number;
  took_ms: number;
  /**
   * Which sources could answer the filters this query narrowed by (#141).
   *
   * Empty is the ordinary case and means *there was nobody to report on* — no
   * reportable dimension was filtered on, or no source put rows in this query's
   * corpus. A dimension that was filtered on lists every contributing source,
   * whatever it answered, so "measured and they all answered" is legible.
   */
  coverage: FilterCoverage[];
}

/** No filters — what the box sends until the chips exist. */
export function noFilters(): SearchFilters {
  return { sources: [], kinds: [], updated_within_days: null, mine: false, authors: [] };
}

/** Answer one launcher query. */
export function search(query: SearchQuery): Promise<SearchResponse> {
  return invoke<SearchResponse>("search", { query });
}

/**
 * One smart list — `knobas_search::SmartListSummary`.
 *
 * Built-in and saved lists are **one shape** (#506, story 57): both are opened
 * by `list:<id>`, both carry a count and a change badge, and `saved` is the
 * only thing that tells them apart.
 */
export interface SmartListSummary {
  /** Stable id; also what `list:<id>` in the box names. */
  id: string;
  label: string;
  /** How many items are in the list right now. */
  count: number;
  /** Something in it is newer than the last time it was opened. */
  changed: boolean;
  /**
   * The list's blurb, the reason it is empty, or — for a saved list — the
   * query it stands for.
   */
  description: string;
  /**
   * Somebody saved this list, so it can be renamed and deleted.
   *
   * A capability rather than a provenance note: it is what puts the two
   * controls on the row, and the built-ins have neither.
   */
  saved: boolean;
  /**
   * The saved query is one today's grammar cannot run (#506, story 60).
   *
   * `count` is `0` and `changed` `false` on such a row because nothing was
   * asked — not because the list is empty — and `description` says which rule
   * refused it. Always `false` for a built-in. The row does not open.
   */
  needs_attention: boolean;
}

/**
 * What an empty launcher box shows — `knobas_app::commands::search::LauncherHome`.
 *
 * The one DTO here that is a composition: the lists and the recent rows come
 * from the search engine, `sources` is `CredentialHealth` (`./sources`), and
 * `pending_writes` is the write queue's depth.
 */
export interface LauncherHome {
  smart_lists: SmartListSummary[];
  /**
   * `knobas_search::EntityRow`, which is a *different Rust struct* from
   * `knobas_app::commands::entity::EntityRow` with the same six fields on the
   * wire. One TypeScript type for both is the truth about the JSON, and a Rust
   * test in `commands/search.rs` fails if the two ever stop agreeing.
   */
  recent: EntityRow[];
  sources: CredentialHealth[];
  /**
   * The footer's *"N pending writes"* — `QueueCounts.pending` (`./sources`)
   * and nothing else. Held and refused writes are decisions the user owes,
   * and the status bar's badge is where they are asked for; counting them
   * here would make the two surfaces disagree about the same queue in the
   * other direction.
   */
  pending_writes: number;
  /**
   * *Show notes and quick capture* (#1036, R-13a.6). While `false` the
   * launcher offers no `note:` syntax row and no Help entry for it.
   */
  notes_shown: boolean;
}

/** The launcher board: the lists, the newest items, and source health. */
export function launcherHome(): Promise<LauncherHome> {
  return invoke<LauncherHome>("launcher_home");
}

/** Every smart list — built-in and saved — with its count and change badge. */
export function smartLists(): Promise<SmartListSummary[]> {
  return invoke<SmartListSummary[]>("smart_lists");
}

/**
 * Save the box's current query as a smart list (#506, story 56).
 *
 * `query` is the **raw box text**: the prefix, the chips and the terms are all
 * in it, and the backend is what parses them (ruling P2). Answers with the row
 * as the rail draws it.
 */
export function createSmartList(label: string, query: string): Promise<SmartListSummary> {
  return invoke<SmartListSummary>("create_smart_list", { label, query });
}

/**
 * Give a saved smart list a different name.
 *
 * The id does not move: it is what `list:<id>` names and what the change
 * badge's seen-stamp is keyed on.
 */
export function renameSmartList(id: string, label: string): Promise<void> {
  return invoke<void>("rename_smart_list", { id, label });
}

/** Forget a saved smart list. */
export function deleteSmartList(id: string): Promise<void> {
  return invoke<void>("delete_smart_list", { id });
}

/**
 * The rows of one smart list, shaped exactly like a search — so the launcher
 * renders a list with the component it renders results with.
 *
 * Opening a list is also what clears its badge.
 */
export function smartListItems(id: string, limit: number): Promise<SearchResponse> {
  return invoke<SearchResponse>("smart_list_items", { id, limit });
}

/**
 * The values the query box suggests after `kind:`, `source:` and `under:` —
 * `knobas_search::FilterSuggestions` (#601).
 *
 * Only values that exist right now, so a picked suggestion never leads to an
 * empty result or an unknown token. Read once per launcher opening; the box
 * narrows the lists itself as the reader types; the `under:` parents are asked
 * again per keystroke, alone (#768). Free-text keys (`author:`,
 * `updated:`) have no list and get no suggestions.
 */
export interface FilterSuggestions {
  /** Every kind with a row in the launcher's corpora. */
  kinds: KindSuggestion[];
  /** Every enabled source with at least one live row, ordered by id. */
  sources: SourceSuggestion[];
  /**
   * The newest twenty names a parent answers to that start with the `under`
   * prefix asked for, ignoring case, newest first, with their kind; a repo is
   * listed under `owner/name` and `name` (#761).
   */
  parents: ParentSuggestion[];
  /**
   * The values the ticket keys take (#811): per key the most used fifty the
   * live mirror holds, in the source's spelling; `@me` first for `assignee`;
   * `current` for `sprint` while a ticket is in an active sprint; `today` and
   * `week` for `assigned`. Empty from a `parentsOnly` read.
   */
  fields: FieldSuggestion[];
}

/** One value a ticket key can take — `knobas_search::FieldSuggestion` (#811). */
export interface FieldSuggestion {
  /** The key as the grammar spells it, without the colon: `status`. */
  key: string;
  /** What the key is completed with: `In Review`, `PAY`, `@me`. */
  value: string;
  /** Drawn beside it: a project's name, `you` for `@me`, or empty. */
  detail: string;
}

/** One kind `kind:` can name — `knobas_search::KindSuggestion`. */
export interface KindSuggestion {
  /** The kind id, which is what `kind:` is completed with. */
  kind: string;
  /** What a result group of this kind is labelled. */
  label: string;
}

/** One source `source:` can name — `knobas_search::SourceSuggestion`. */
export interface SourceSuggestion {
  /** The instance id, which is what `source:` is completed with. */
  id: string;
  display_name: string;
}

/** One name `under:` can scope by — `knobas_search::ParentSuggestion` (ADR-0018). */
export interface ParentSuggestion {
  /** The name the parent answers to, as its source spells it. */
  name: string;
  /** The parent's kind, drawn beside the name: `payout-service · repo`. */
  kind: string;
}

/**
 * The query box's suggestion lists (#601), the parents for the `under:` value
 * being typed (#761). `parentsOnly` leaves `kinds` and `sources` empty and
 * unread (#768): the box asks for them once per opening, with its first read.
 */
export function filterSuggestions(under = "", parentsOnly = false): Promise<FilterSuggestions> {
  return invoke<FilterSuggestions>("filter_suggestions", { under, parentsOnly });
}

// ---- ticket views (#812) -----------------------------------------------------

/** What a ticket board's lanes split by (R-3.5) — `ticket_views::LANES`. */
export type ViewLanes = "none" | "epic" | "assignee" | "priority" | "issuetype" | "label";
/** What a card can show (R-3.16) — `ticket_views::CARD_FIELDS`. */
export type CardField = "type" | "signals" | "comments" | "assignee" | "epic" | "labels";
/** The list's columns (R-3.16); `key` is always one — `ticket_views::LIST_COLUMNS`. */
export type ListColumn = "key" | "summary" | "status" | "priority" | "signals" | "assignee" | "updated" | "epic" | "labels";
/** What a list header sorts by (R-3.6) — `ticket_views::SORT_COLUMNS`. */
export type ViewSortColumn = "key" | "summary" | "status" | "priority" | "assignee" | "updated";

/** One ticket board column — `ticket_views::ViewColumn`. */
export interface ViewColumn {
  name: string;
  statuses: string[];
  /** *At most*, or `null`. */
  limit: number | null;
}

/** The list's header sort — `ticket_views::ListSort`. */
export interface ListSort {
  column: ViewSortColumn;
  direction: "ascending" | "descending";
}

/** One ticket view as stored — `ticket_views::TicketView`. */
export interface TicketView {
  /** A slug: `#/tickets/<id>` opens it. */
  id: string;
  name: string;
  group: string;
  position: number;
  /** The conditions as search text, exactly as saved; empty is every ticket. */
  query: string;
  layout: "board" | "list";
  /** Empty is one column per status. */
  columns: ViewColumn[];
  lanes: ViewLanes;
  card_fields: CardField[];
  list_columns: ListColumn[];
  /** `null` is newest first. */
  list_sort: ListSort | null;
  /** The Jira board or filter a Jira view shows (#877); `null` on your own. */
  jira_ref: string | null;
}

/** A row of the views pane — `ticket_views::TicketViewRow`. */
export interface TicketViewRow extends TicketView {
  /** How many tickets the text admits; `null` when the grammar could not read it. */
  count: number | null;
  /** The part of the text the grammar could not read, or `null`. */
  unreadable: string | null;
  /** Where a Jira view comes from (#877), or `null` on a view somebody made. */
  jira: JiraSource | null;
}

/** What Jira says about a Jira view — `ticket_views::JiraSource` (#877). */
export interface JiraSource {
  kind: "board" | "filter";
  /** `scrum`, `kanban` or `simple` for a board; `null` for a filter. */
  board_type: string | null;
  /** The saved filter's JQL, as Jira holds it. */
  jql: string;
  /** A Kanban board's sub-filter, or `null`. */
  sub_query: string | null;
  /** A Scrum board's active sprint, or `null`. */
  sprint: { id: number; name: string; end_date: string | null } | null;
  /** RFC 3339: when the last sync read it. */
  last_read: string;
  /** Where Jira shows it, or `null`. */
  web_url: string | null;
  /** *Hide from the list* (#878, R-3.25): it keeps syncing. */
  hidden: boolean;
  /** Turned up after its source's first sync and not opened yet (#878, R-3.20). */
  new: boolean;
  /** Tickets on it knobas does not have, per project the source does not sync, most first (#879, R-3.23). */
  unsynced: UnsyncedTickets[];
}

/** Tickets on a Jira view in one project the source does not sync — `ticket_views::Unsynced` (#879). */
export interface UnsyncedTickets {
  project: string;
  count: number;
}

/** *Hide from the list* (`true`) or *Show* (`false`) a Jira view (#878). */
export function hideJiraView(id: string, hidden: boolean): Promise<void> {
  return invoke<void>("hide_jira_view", { id, hidden });
}

/** A view was opened: a Jira view stops saying *new* (#878). */
export function openJiraView(id: string): Promise<void> {
  return invoke<void>("open_jira_view", { id });
}

/** What `save_ticket_view` takes — `ticket_views::TicketViewDraft`. */
export interface TicketViewDraft {
  /** `null` makes a new view; an id saves over it, or puts a deleted one back. */
  id: string | null;
  name: string;
  group: string;
  /** `null` keeps a saved view's place and puts a new one last. */
  position: number | null;
  query: string;
  layout: "board" | "list";
  columns: ViewColumn[];
  lanes: ViewLanes;
  card_fields: CardField[];
  list_columns: ListColumn[];
  list_sort: ListSort | null;
  jira_ref: string | null;
}

/** Every ticket view in list order, each with its count (#812). */
export function ticketViews(): Promise<TicketViewRow[]> {
  return invoke<TicketViewRow[]>("ticket_views");
}

/** Save a ticket view, new or over the one the draft names; answers it as stored. */
export function saveTicketView(view: TicketViewDraft): Promise<TicketView> {
  return invoke<TicketView>("save_ticket_view", { view });
}

/** Delete a ticket view; *Undo* saves it back. */
export function deleteTicketView(id: string): Promise<void> {
  return invoke<void>("delete_ticket_view", { id });
}
