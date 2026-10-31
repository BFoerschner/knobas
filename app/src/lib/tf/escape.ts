/**
 * The ticket-first shell's `Esc` (#809, spec #2227): one layer per press, in
 * this order. `Esc` goes up one level, never sideways.
 *
 * 1. a dialog is open           → close the topmost dialog;
 * 2. a drawer is open           → close it;
 * 3. a filter field has text    → clear it, focus stays;
 * 4. a text field has focus     → blur it;
 * 5. a job's sub-tab is open    → the job's first tab;
 * 6. a top page                 → nothing;
 * 7. a structural parent exists → go to it;
 * 8. otherwise                  → nothing.
 *
 * A top page is a section's own page, the address its nav entry or the
 * Settings cog links to (`isTopPage` in `route.ts`): `Esc` never jumps from
 * one section to another. The structural parent is `parentOf`'s: a run's is
 * Builds, a pull request's or a repository's Code, a commit's its
 * repository's page, a doc page's Docs, an asset's or a route's
 * Dependencies, and anything else inside a section that section's top page.
 * An item's page and a note's page have no parent, so rung 8 leaves them
 * where they are. `Esc` never reads "the job you were last in" (#2228); a
 * key that sometimes does nothing is what makes the others safe to press.
 *
 * Rungs 5 to 8 are {@link escTarget}, which the breadcrumb's *Back to <name>*
 * reads too, so the hint and the key cannot disagree.
 *
 * Pure: the shell describes what is open and acts on the answer. Every dialog
 * registers on the layer stack (`TfDialog`, and the rooms UI's `Modal` the
 * asset tree still draws, #1527), so rung 1 sees it even when focus is
 * outside it. `Modal` also answers an `Esc` pressed inside it and stops the
 * event, so that press never reaches the shell.
 */
import { isEntityId, keyOf } from "./job/job";
import { isTopPage, jobHash, parentOf, parseTfHash, SECTION_NAME, type TfRoute } from "./route";

/** What `Esc` can see when it is pressed. */
export interface EscState {
  /** Open dialogs and drawers, oldest first. */
  layers: readonly { kind: "dialog" | "drawer" }[];
  /** The focused text field, or `null` when focus is not in one. */
  field: { filter: boolean; text: string } | null;
  route: TfRoute;
}

/** What one press does. */
export type EscStep =
  | { do: "close"; index: number }
  | { do: "clear-filter" }
  | { do: "blur" }
  | { do: "go"; hash: string }
  | { do: "nothing" };

/** Where `Esc` goes once nothing is open, and what the breadcrumb calls it. */
export interface EscTarget {
  hash: string;
  name: string;
}

export function escStep(state: EscState): EscStep {
  for (const kind of ["dialog", "drawer"] as const) {
    const index = state.layers.map((layer) => layer.kind).lastIndexOf(kind);
    if (index !== -1) return { do: "close", index };
  }
  if (state.field) {
    if (state.field.filter && state.field.text !== "") return { do: "clear-filter" };
    return { do: "blur" };
  }
  const target = escTarget(state.route);
  return target === null ? { do: "nothing" } : { do: "go", hash: target.hash };
}

/** Rungs 5 to 8: where `Esc` goes from `route` once nothing is open, or `null` where it does nothing. */
export function escTarget(route: TfRoute): EscTarget | null {
  if (route.view === "job" && route.key !== null && route.tab !== null) {
    return { hash: jobHash(route.key), name: isEntityId(route.key) ? keyOf(route.key) : route.key };
  }
  if (isTopPage(route)) return null;
  const parent = parentOf(route);
  return parent === null ? null : { hash: parent, name: placeName(parseTfHash(parent)) };
}

/**
 * What the breadcrumb calls a place `Esc` goes to: a section's name, or
 * *Repository* for a repository's page, whose title only its own read knows
 * (`TfRepo` names it once it has).
 */
function placeName(route: TfRoute): string {
  if (route.view === "full" && route.head === "repo") return "Repository";
  if (route.view === "tickets") return SECTION_NAME.tickets;
  if (route.view === "job") return SECTION_NAME.job;
  return route.view === "full" ? SECTION_NAME[route.section] : "";
}
