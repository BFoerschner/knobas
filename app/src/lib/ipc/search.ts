/** Search and the launcher — `crates/knobas-app/src/commands/search.rs`. */
import { invoke } from "@tauri-apps/api/core";

/** `knobas_search::SearchFilters`. */
export interface SearchFilters {
  sources: string[];
  kinds: string[];
  updated_within_days: number | null;
  mine: boolean;
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

export interface SearchResponse {
  interpreted: ParsedQuery;
  groups: ResultGroup[];
  total: number;
  took_ms: number;
}

/** No filters — what the box sends until the chips exist. */
export function noFilters(): SearchFilters {
  return { sources: [], kinds: [], updated_within_days: null, mine: false };
}

/** Answer one launcher query. */
export function search(query: SearchQuery): Promise<SearchResponse> {
  return invoke<SearchResponse>("search", { query });
}
