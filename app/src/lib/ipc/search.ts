/** Search and the launcher — `crates/knobas-app/src/commands/search.rs`. */
import { invoke } from "@tauri-apps/api/core";

import type { EntityRow } from "./entity";
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
 * One member today (issue #141 scopes the behaviour to authorship). The union
 * is what makes a second dimension an addition rather than a reshape.
 */
export type FilterDimension = "author";

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

/** One built-in smart list — `knobas_search::SmartListSummary`. */
export interface SmartListSummary {
  /** Stable id; also what `list:<id>` in the box names. */
  id: string;
  label: string;
  /** How many items are in the list right now. */
  count: number;
  /** Something in it is newer than the last time it was opened. */
  changed: boolean;
  /** The list's blurb, or the reason it is empty. */
  description: string;
}

/**
 * What an empty launcher box shows — `knobas_app::commands::search::LauncherHome`.
 *
 * The one DTO here that is a composition: the lists and the recent rows come
 * from the search engine, `sources` is `CredentialHealth` (`./sources`), and
 * `pending_writes` counts an offline write queue that is M2 — it is 0 in M1,
 * by rule rather than by omission.
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
  pending_writes: number;
}

/** The launcher board: the lists, the newest items, and source health. */
export function launcherHome(): Promise<LauncherHome> {
  return invoke<LauncherHome>("launcher_home");
}

/** Every built-in smart list, with its count and its change badge. */
export function smartLists(): Promise<SmartListSummary[]> {
  return invoke<SmartListSummary[]>("smart_lists");
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
