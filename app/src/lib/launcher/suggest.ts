/**
 * Value suggestions for `kind:`, `source:` and `under:` (#601, spec #591
 * stories 27-30), and for the ticket keys `project:`, `status:`,
 * `issuetype:`, `priority:`, `assignee:`, `label:`, `sprint:` and
 * `assigned:` (#811), and `has:` (#815).
 *
 * The values are the backend's (`filter_suggestions`: only values that exist
 * right now); what is here is which token of the box is being typed, which
 * of those values it can still become, and how picking one rewrites the box.
 *
 * **The keys are the grammar's spellings** (`crates/knobas-search/src/query.rs`,
 * `apply_key_value`): `type:` is `kind:`, `src:` and `in:` are `source:`.
 * Free-text keys -- `author:`, `updated:` -- are absent on purpose: they take
 * any value, and a list of the ones that happen to exist would read as the
 * only ones that work.
 */
import type { FilterSuggestions } from "../ipc";

/** The ticket keys (#811), each drawing from `fields` by its own name. */
const FIELD_KEYS = [
  "project",
  "status",
  "issuetype",
  "priority",
  "assignee",
  "label",
  "sprint",
  "assigned",
  "has",
] as const;
type FieldKey = (typeof FIELD_KEYS)[number];

/** Which list a key draws from. */
type SuggestKey = "kind" | "source" | "under" | FieldKey;

const KEYS: Record<string, SuggestKey> = {
  kind: "kind",
  type: "kind",
  source: "source",
  src: "source",
  in: "source",
  under: "under",
  ...Object.fromEntries(FIELD_KEYS.map((key) => [key, key])),
};

/** How many values the list shows at most. */
export const SUGGESTION_LIMIT = 8;

/** The `key:value` token at the end of the box, still being typed. */
export interface ActiveToken {
  key: SuggestKey;
  /** Where the token starts in the box. */
  start: number;
  /** The key as it was typed, `type` or `Kind` included. */
  typedKey: string;
  /** The value typed so far, without an opening quote. */
  partial: string;
}

/** One value the box offers. */
export interface Suggestion {
  /** What the token is completed with. */
  value: string;
  /** What is drawn beside it: a kind's label, a source's name, a parent's kind. */
  detail: string;
}

/**
 * The token at the end of `raw`, if it is a suggestible key still being
 * typed. A box that ends in whitespace outside quotes has finished its last
 * token, so nothing is being typed.
 *
 * Tokenized as the grammar tokenizes (`query.rs`, `tokenize`): whitespace
 * splits, and a double-quoted run stays together.
 */
export function activeToken(raw: string): ActiveToken | null {
  let start = 0;
  let inQuotes = false;
  for (let i = 0; i < raw.length; i++) {
    const ch = raw[i]!;
    if (ch === '"') inQuotes = !inQuotes;
    else if (/\s/.test(ch) && !inQuotes) start = i + 1;
  }
  const token = raw.slice(start);
  const colon = token.indexOf(":");
  if (colon <= 0) return null;
  const typedKey = token.slice(0, colon);
  // A negated ticket key (`-status:`) is offered the same values.
  const key = KEYS[typedKey.replace(/^-/, "").toLowerCase()];
  if (!key) return null;
  const value = token.slice(colon + 1);
  return {
    key,
    start,
    typedKey,
    partial: value.startsWith('"') ? value.slice(1) : value,
  };
}

/** Every value `key` can be completed with, before narrowing. */
function candidates(key: SuggestKey, lists: FilterSuggestions): Suggestion[] {
  switch (key) {
    case "kind":
      return lists.kinds.map((k) => ({ value: k.kind, detail: k.label }));
    case "source":
      return lists.sources.map((s) => ({
        value: s.id,
        detail: s.display_name,
      }));
    case "under":
      return lists.parents.map((p) => ({ value: p.name, detail: p.kind }));
    default:
      return (lists.fields ?? [])
        .filter((f) => f.key === key)
        .map((f) => ({ value: f.value, detail: f.detail }));
  }
}

/**
 * The values the token being typed can still become: those containing what
 * was typed, ignoring case, the ones that start with it first.
 *
 * A value typed out in full closes the list, even when longer values still
 * contain it: the open list owns `Enter`, so `under:vm-1` with a `vm-10`
 * beside it would otherwise complete to `vm-10` on the key that runs the row.
 */
export function suggest(token: ActiveToken, lists: FilterSuggestions): Suggestion[] {
  const typed = token.partial.toLowerCase();
  const values = candidates(token.key, lists);
  if (values.some((s) => s.value.toLowerCase() === typed)) return [];
  const all = values.filter((s) => s.value.toLowerCase().includes(typed));
  const starts = all.filter((s) => s.value.toLowerCase().startsWith(typed));
  const rest = all.filter((s) => !s.value.toLowerCase().startsWith(typed));
  return [...starts, ...rest].slice(0, SUGGESTION_LIMIT);
}

/**
 * The box with the token being typed completed to `value`, and a space after
 * it so the next word starts a new token. A value with whitespace in it is
 * quoted, which is how `under:` reads a page title with spaces.
 */
export function complete(raw: string, token: ActiveToken, value: string): string {
  const written = /\s/.test(value) ? `"${value}"` : value;
  return `${raw.slice(0, token.start)}${token.typedKey}:${written} `;
}
