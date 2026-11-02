/**
 * The filter a section's own search used to apply (#2425, spec #2419): what
 * the launcher's box holds when `/`, ⌘/ or the top bar's search button opens
 * it on that section. ⌘K opens it empty wherever it is pressed.
 *
 * Each value is the grammar's own text (`#` and the kind letters, #2420),
 * with the space after it, so typing goes after the filter. A section whose
 * pages no kind letter names opens the box empty.
 */
import { type Section, type TfRoute, sectionOf } from "../tf/route";

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
