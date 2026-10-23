import type { SourceFacts } from "./job";

/**
 * A `list_sources` row as the wire sends it, for the tests' port stand-ins: the
 * fields a case names, and every other one at its idle value. `SourceFacts`
 * requires them all (#1884), so a stand-in cannot pass a row `{ id }` alone and
 * hide a reader that falls back on a field the wire always sends.
 */
export function sourceFacts(id: string, over: Partial<Omit<SourceFacts, "id">> = {}): SourceFacts {
  return { id, display_name: id, config: {}, roles: [], auth_kind: null, ...over };
}
