/**
 * The words the Tickets filter's typed text asks for, as the server marks them:
 * `knobas_search::query::needles` over the text left once the grammar's keys
 * are taken out (#1903). `mini_board` carries no needles on the wire, so the
 * browser reads the text the way `query::tokenize`, `apply_token` and
 * `needles` do; a change to one is a change to the other.
 */

/** The keys the grammar reads (`FieldKey::ALL`, `space`, and `apply_key_value`'s), negated or not. */
const KEYS = new Set([
  "project", "space", "status", "issuetype", "priority", "assignee", "label", "sprint", "assigned", "has",
  "source", "src", "in", "kind", "type", "updated", "is", "author", "owner", "by", "under", "reach", "name", "env", "health",
]);

/** Split on whitespace, a double-quoted run kept together with its quotes (`query::tokenize`). */
function tokenize(input: string): string[] {
  const tokens: string[] = [];
  let current = "";
  let inQuotes = false;
  for (const ch of input) {
    if (ch === '"') {
      inQuotes = !inQuotes;
      current += ch;
    } else if (/\s/.test(ch) && !inQuotes) {
      if (current !== "") tokens.push(current);
      current = "";
    } else current += ch;
  }
  if (current !== "") tokens.push(current);
  return tokens;
}

/** `key:value`'s lowercased key, when the token is one. */
function keyOf(token: string): string | null {
  const at = token.indexOf(":");
  return at > 0 ? token.slice(0, at).toLowerCase() : null;
}

/** Whether the grammar takes the token as a key, a marker or a refused negation, not a search word. */
function isGrammar(token: string): boolean {
  if (token.startsWith("/") || token.startsWith("@")) return true;
  const bare = token.startsWith("-") ? token.slice(1) : token;
  // `-@x`, `-/x`, `-#x` are refused whole (`negates_what_cannot_be_negated`).
  if (token.startsWith("-") && /^[@/#]./u.test(bare)) return true;
  const key = keyOf(bare);
  return key !== null && KEYS.has(key);
}

/** `token` as a needle: edge punctuation and quotes off, lowercased; `null` when nothing is left. */
function needleOf(token: string): string | null {
  const needle = token.replace(/^[^\p{L}\p{N}]+|[^\p{L}\p{N}]+$/gu, "").toLowerCase();
  return needle === "" ? null : needle;
}

/** One alternative of the word filter: a row holds every word of `holds` and none of `lacks`. */
export interface WordAlternative {
  holds: string[];
  lacks: string[];
}

/** The text's search words, keys taken out, as `query::parse` leaves them (`#key` is its rest). */
function terms(text: string): string[] {
  const out: string[] = [];
  for (const token of tokenize(text)) {
    if (token.startsWith("#")) {
      if (token.length > 1) out.push(token.slice(1));
    } else if (!isGrammar(token)) out.push(token);
  }
  return out;
}

/** `token` as a needle onto `into`, unless nothing is left of it or it is there already. */
function push(into: string[], token: string): void {
  const needle = needleOf(token);
  if (needle !== null && !into.includes(needle)) into.push(needle);
}

/** The word filter's alternatives, as `query::word_alternatives` reads them (#1889): `or` splits, `-` leaves out. */
export function wordAlternatives(text: string): WordAlternative[] {
  const out: WordAlternative[] = [];
  let current: WordAlternative = { holds: [], lacks: [] };
  for (const token of terms(text)) {
    if (token.toLowerCase() === "or") {
      out.push(current);
      current = { holds: [], lacks: [] };
    } else if (token.startsWith("-")) push(current.lacks, token.slice(1));
    else push(current.holds, token);
  }
  out.push(current);
  return out.filter((alternative) => alternative.holds.length + alternative.lacks.length > 0);
}

/** The words and phrases typed, lowercased, once each, in order; `-word`, `or` and keys left out. */
export function needles(text: string): string[] {
  const out: string[] = [];
  for (const token of terms(text)) {
    if (!token.startsWith("-") && token.toLowerCase() !== "or") push(out, token);
  }
  return out;
}
