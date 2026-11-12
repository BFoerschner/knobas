/**
 * Search and the launcher — `crates/knobas-app/src/commands/search.rs`.
 *
 * The types are generated from the Rust (`./generated/search.ts`, contract
 * §2.6 as amended by #1325, switched by #1329): a field is explained in its
 * Rust doc comment and nowhere else. The `invoke` functions and
 * {@link noFilters} stay hand-written.
 */
import { invoke } from "./invoke";
import type { EntityRow } from "./generated/entity";
import type {
  FavouriteTarget,
  FilterSuggestions,
  Keys,
  LauncherHome,
  QueryTranslation,
  SearchFilters,
  SearchQuery,
  SearchResponse,
  SmartListSummary,
  TicketView,
  TicketViewDraft,
  TicketViewRow,
} from "./generated/search";

export type {
  ActionKey,
  CardField,
  FavouriteTarget,
  FieldSuggestion,
  FilterAnswer,
  FilterCoverage,
  FilterDimension,
  FilterSuggestions,
  Keys,
  KindSuggestion,
  LauncherHome,
  LeftOut,
  ListColumn,
  ListKey,
  ListSort,
  ParentSuggestion,
  ParsedQuery,
  Prefix,
  QueryCondition,
  QueryKey,
  QueryTranslation,
  ResultGroup,
  SavedViewSource,
  SearchFilters,
  Excerpt,
  SearchHit,
  SearchQuery,
  SearchResponse,
  Segment,
  SmartListSummary,
  SourceAnswer,
  SourceSuggestion,
  TicketView,
  TicketViewDraft,
  TicketViewRow,
  UnsyncedTickets,
  ViewColumn,
  ViewLanes,
  ViewSortColumn,
} from "./generated/search";

/** No filters — what the box sends until the chips exist. */
export function noFilters(): SearchFilters {
  return { sources: [], kinds: [], updated_within_days: null, mine: false, authors: [] };
}

/** Answer one launcher query. */
export function search(query: SearchQuery): Promise<SearchResponse> {
  return invoke<SearchResponse>("search", { query });
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
 * Every favourite (#2428), in the order they were favourited: the block the
 * empty box draws above the recent items, which `launcher_home` also carries.
 * A saved list's row is its `#l` hit's: kind and source `list`, its text as
 * `path`, the refusal as `state`.
 */
export function favourites(): Promise<EntityRow[]> {
  return invoke<EntityRow[]>("favourites");
}

/** *Favourite* an entity or a saved list, last in the block (#2428). */
export function addFavourite(target: FavouriteTarget): Promise<void> {
  return invoke<void>("add_favourite", { target });
}

/** *Unfavourite* an entity or a saved list (#2428); not a favourite is no error. */
export function removeFavourite(target: FavouriteTarget): Promise<void> {
  return invoke<void>("remove_favourite", { target });
}

/** Every key bound (#2429): the saved lists' chords, each with its list's name and text, and the actions' chords per kind. */
export function keys(): Promise<Keys> {
  return invoke<Keys>("keys");
}

/** Give a saved list a chord that opens it from anywhere, or take it away with `null` (#2429). */
export function bindListKey(listId: string, chord: string | null): Promise<void> {
  return invoke<void>("bind_list_key", { listId, chord });
}

/** Give the Tab sheet's action `actionId` a chord on `kind`, or take it away with `null` (#2429). */
export function bindActionKey(kind: string, actionId: string, chord: string | null): Promise<void> {
  return invoke<void>("bind_action_key", { kind, actionId, chord });
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

/** *Hide from the list* (`true`) or *Show* (`false`) a saved view, such as a Jira view (#878, #1245). */
export function hideSavedView(id: string, hidden: boolean): Promise<void> {
  return invoke<void>("hide_saved_view", { id, hidden });
}

/** A view was opened: a saved view, such as a Jira view, stops saying *new* (#878, #1245). */
export function openSavedView(id: string): Promise<void> {
  return invoke<void>("open_saved_view", { id });
}

/**
 * *Make an editable copy* (#1208): the saved view `id`'s query, read by its
 * source's adapter, live. `null` for a source that translates no query.
 */
export function translateTicketView(id: string): Promise<QueryTranslation | null> {
  return invoke<QueryTranslation | null>("translate_ticket_view", { id });
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
