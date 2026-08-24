/** Search and the launcher — `crates/knobas-app/src/commands/search.rs`. */
import { invoke } from "@tauri-apps/api/core";

/** One full-text search result — `knobas_db::search::SearchHit`. */
export interface SearchHit {
  /** `"<namespace>:<key>"`, e.g. `"mock:PAY-231"`. */
  entity_id: string;
  kind: string;
  source_id: string;
  title: string;
  /**
   * XSS-UNSAFE. Plain text, but *source* text: an excerpt of whatever a person
   * typed into a ticket, `<script>` included. Render it as text (`{snippet}`),
   * never with `{@html}`. It carries no markup of its own — the match is not
   * marked up, because highlighting via a string that must be escaped anyway
   * can only produce literal tags.
   */
  snippet: string;
  rank: number;
  /** RFC 3339 timestamp. */
  synced_at: string;
}

/**
 * The best `limit` matches for `q`, ranked. `q` is user text in the
 * `websearch_to_tsquery` dialect — quoted phrases, `or`, leading `-`.
 */
export function search(q: string, limit: number): Promise<SearchHit[]> {
  return invoke<SearchHit[]>("search", { q, limit });
}
