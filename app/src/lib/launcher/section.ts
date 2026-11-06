/**
 * The filter a section's own search used to apply (#2425, spec #2419): what
 * the launcher's box holds when `/`, ⌘/ or the top bar's search button opens
 * it on that section. ⌘K opens it empty wherever it is pressed.
 *
 * Each value is the grammar's own text (`#` and the kind letters, #2420),
 * with the space after it, so typing goes after the filter. A section whose
 * pages no kind letter names opens the box empty.
 */
import { SECTION_NAME, type Section, type TfRoute, sectionOf } from "../tf/route";
import type { SyntaxEntry } from "./syntax";

const FILTER: Partial<Record<Section, string>> = {
  tickets: "#t ",
  inbox: "#i ",
  code: "#rp ",
  builds: "#b ",
  docs: "#d ",
  monitors: "#m ",
};

/** The section filter for the page at `route`, or `""` where none applies. */
export function sectionFilter(route: TfRoute): string {
  const section = sectionOf(route);
  return section === null ? "" : (FILTER[section] ?? "");
}

/**
 * What the box's chip calls a section's filter: the section's own name, so the
 * grammar is not what the reader sees (*Code*, not `#rp `). `null` for text no
 * section opens with, which stays in the box as typed.
 */
export function seedName(filter: string): string | null {
  const found = (Object.entries(FILTER) as [Section, string][]).find(([, text]) => text === filter);
  return found ? SECTION_NAME[found[0]] : null;
}

/** What the chip in front of the box calls a prefix the backend read: `>` is *Go to*, as its rows' heading says. */
const MODE_NAME: Readonly<Record<string, string>> = {
  action: "Go to",
  person: "People",
  source: "Source",
  time: "Time",
  note: "Notes",
  list: "Smart list",
  asset: "Assets",
  help: "Help",
};

/** The chip's words for a prefix; one this build has no word for keeps its own, with a capital. */
export function modeName(prefix: string): string {
  return MODE_NAME[prefix] ?? prefix.charAt(0).toUpperCase() + prefix.slice(1);
}

/**
 * The kinds a section's filter names, read from the `#` rows of `table`, the
 * letters the parser pins: the chip already says them, so the bar under the
 * box does not say them again.
 */
export function seedKinds(filter: string, table: readonly SyntaxEntry[]): string[] {
  const letters = filter.trim().slice(1);
  return table.flatMap((row) => (row.kind !== undefined && letters.includes(row.token.slice(1)) ? [row.kind] : []));
}
