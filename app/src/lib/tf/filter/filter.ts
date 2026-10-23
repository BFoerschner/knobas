/**
 * The filter bar's rules (#810): one set for every list view of the
 * ticket-first shell (requirements §2, R-2.1 to R-2.7).
 *
 * Pure. `FilterBar.svelte` draws a {@link FilterConfig} over a
 * {@link FilterState}; a view reads the same state back through these
 * functions, in one of two modes:
 *
 * - **in the browser**, for a list the view already holds whole (a job's
 *   section, a page tree): {@link matchesAll} keeps a row when every word
 *   typed appears in it;
 * - **on the server**, for a list that can run past a few hundred rows
 *   (tickets, pages, commits; R-2.7): the config carries the view's
 *   {@link FilterConfig.base | base query}, and {@link toQuery} turns the base,
 *   the chips, the dropdowns and the words into one search-grammar text for
 *   the view's read. The browser filters nothing then.
 *
 * Either way {@link marks} marks the words in what is drawn (R-2.3), and
 * {@link countLine} says how many rows are left (R-2.1).
 */

/** One value a chip or a dropdown offers: what is sent, and what is shown. */
export interface FilterOption {
  value: string;
  label: string;
}

/**
 * A field with few values, drawn as a row of toggles (R-2.1): several may be
 * on, and they mean any of them. Server-side, `key` is the grammar key each
 * value is sent under (`issuetype:Bug`).
 */
export interface FilterChips {
  key: string;
  /** The group's accessible name: *Type*, *Priority*. */
  label: string;
  options: FilterOption[];
}

/**
 * A field with many values, drawn as a select whose first option is *Any*
 * (R-2.1). Server-side, `key` is the grammar key the value is sent under.
 */
export interface FilterDropdown {
  key: string;
  /** The words before the select: *Assignee*, *Epic*. */
  label: string;
  options: FilterOption[];
}

/**
 * One choice of the scope toggle, *This job / Mine / All* (R-2.1, R-2.5).
 * Offered only where the view gives each a meaning; `query` is what a
 * server-side view adds for it (`assignee:@me`), empty for *All*.
 */
export interface FilterScope {
  id: string;
  label: string;
  query: string;
  /** A code value drawn after the label in mono: *This job* names its key (prototype-6's `jobk`). */
  tag?: string;
}

/**
 * What one view's filter bar offers. The one parameter shape every list view
 * passes (#810's first criterion): later views fill it, none forks the bar.
 */
export interface FilterConfig {
  /** The view this bar filters. State is kept per id for the session (R-2.6). */
  id: string;
  /** Names what the field filters: "Filter by key, summary, epic or person". */
  placeholder: string;
  /** What the count counts, plural: "tickets". */
  noun: string;
  /** The same in the singular, for a count of one: "ticket". */
  one?: string;
  /** The scope toggle's choices, in order; none draws no toggle. */
  scopes?: FilterScope[];
  chips?: FilterChips[];
  dropdowns?: FilterDropdown[];
  /**
   * The view's base query, in the search grammar, for a view filtered on the
   * server (R-2.7); `""` for a view whose base is everything. Absent for a
   * view that filters what it holds in the browser.
   */
  base?: string;
  /**
   * Whether the bar draws the `/` hint and answers the key. Default on; a page
   * that holds two bars (Builds) turns it off on the inner ones, since `/` is
   * the page filter's key (R-2.4, R-14) and goes to the first (#1453).
   */
  hint?: boolean;
}

/** Where one view's filter stands. */
export interface FilterState {
  /** What is typed. */
  text: string;
  /** The scope chosen, a {@link FilterScope.id}, or `null` for the first. */
  scope: string | null;
  /** Per chip group, the values switched on. */
  chips: Record<string, string[]>;
  /** Per dropdown, the value chosen; absent is *Any*. */
  dropdowns: Record<string, string>;
}

/** A filter that keeps everything. */
export function emptyFilter(): FilterState {
  return { text: "", scope: null, chips: {}, dropdowns: {} };
}

/** The words typed, lowercased, without the spaces between them. */
export function words(text: string): string[] {
  return text.toLowerCase().split(/\s+/).filter(Boolean);
}

/** Whether every word typed appears in `haystack`, in any order, ignoring case (R-2.3). */
export function matchesAll(haystack: string, text: string): boolean {
  const lower = haystack.toLowerCase();
  return words(text).every((word) => lower.includes(word));
}

/** A run of drawn text, and whether a typed word matched it. */
export interface Mark {
  text: string;
  hit: boolean;
}

/**
 * `text` cut into runs, the ones a typed word matched marked (R-2.3).
 *
 * Each word of two letters or more is marked wherever it occurs, ignoring
 * case; overlapping matches are one mark. A one-letter word marks nothing,
 * as in prototype-6: one letter would mark half of every row.
 */
export function marks(text: string, typed: string): Mark[] {
  return markNeedles(text, words(typed));
}

/** {@link marks} for needles already read (lowercased); a needle may hold spaces, as a phrase does. */
export function markNeedles(text: string, needles: readonly string[]): Mark[] {
  const hit = new Array<boolean>(text.length).fill(false);
  const lower = text.toLowerCase();
  for (const word of needles) {
    if (word.length < 2) continue;
    for (let at = lower.indexOf(word); at !== -1; at = lower.indexOf(word, at + 1)) {
      hit.fill(true, at, at + word.length);
    }
  }
  const runs: Mark[] = [];
  for (let at = 0; at < text.length; at += 1) {
    const last = runs[runs.length - 1];
    if (last && last.hit === hit[at]) last.text += text[at];
    else runs.push({ text: text[at]!, hit: hit[at]! });
  }
  return runs.length === 0 ? [{ text, hit: false }] : runs;
}

/** The result count (R-2.1): "8 of 75 tickets", or "75 tickets" when nothing filters. */
export function countLine(shown: number, total: number, noun: string, one?: string): string {
  const word = total === 1 && one !== undefined ? one : noun;
  return shown === total ? `${total} ${word}` : `${shown} of ${total} ${word}`;
}

/** Whether anything narrows: words, a chip, or a dropdown off *Any*. The scope is the view's own. */
export function isFiltering(state: FilterState): boolean {
  return (
    state.text.trim() !== "" ||
    Object.values(state.chips).some((values) => values.length > 0) ||
    Object.values(state.dropdowns).some((value) => value !== "")
  );
}

/** One `key:value` of the grammar, quoted when the value holds a space (`status:"In Review"`). */
export function grammarToken(key: string, value: string): string {
  const clean = value.replaceAll('"', "");
  return /\s/.test(clean) ? `${key}:"${clean}"` : `${key}:${clean}`;
}

/**
 * The search-grammar text a server-side view reads with (R-2.7): its base
 * query, the scope's, each chip value and dropdown value as its key, and the
 * words typed, in that order. Values of one chip group mean any of them,
 * which is the grammar's rule for a repeated key.
 */
export function toQuery(config: FilterConfig, state: FilterState): string {
  const parts: string[] = [];
  if (config.base) parts.push(config.base);
  const scopes = config.scopes ?? [];
  const scope = scopes.find((choice) => choice.id === state.scope) ?? scopes[0];
  if (scope?.query) parts.push(scope.query);
  for (const group of config.chips ?? []) {
    for (const value of state.chips[group.key] ?? []) parts.push(grammarToken(group.key, value));
  }
  for (const dropdown of config.dropdowns ?? []) {
    const value = state.dropdowns[dropdown.key];
    if (value) parts.push(grammarToken(dropdown.key, value));
  }
  const typed = state.text.trim();
  if (typed) parts.push(typed.replace(/\s+/g, " "));
  return parts.join(" ");
}
