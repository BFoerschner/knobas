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
 * 7. an opener is known         → go to the opener;
 * 8. a structural parent exists → go to it;
 * 9. otherwise                  → nothing.
 *
 * A top page is a section's own page, the address its nav entry or the
 * Settings cog links to (`isTopPage` in `route.ts`): `Esc` never jumps from
 * one section to another, even when something opened it. The opener is the
 * page the current history entry was opened from by an in-app link or button
 * (`opener.ts`, #2229): a run opened from the Job page returns there, one
 * opened from another run returns to that run. An entry reached by `⌘K`, a
 * deep link, startup or an `Esc` step has none, and a tab switch keeps the
 * page's own. The shell steps back in history when the opener is the entry
 * before, so `⌘]` goes forward to the detail again; otherwise it goes there.
 *
 * With no opener, the structural parent is `parentOf`'s: a run's is
 * Builds, a pull request's or a repository's Code, a commit's its
 * repository's page, a doc page's Docs, an asset's or a route's
 * Dependencies, a ticket page's Tickets, and anything else inside a section that section's top page.
 * An item's page and a note's page have no parent, so rung 9 leaves them
 * where they are. `Esc` never reads "the job you were last in" (#2228); a
 * key that sometimes does nothing is what makes the others safe to press.
 *
 * Rungs 5 to 9 are {@link escTarget}, which the breadcrumb's *Back to <name>*
 * reads too, so the hint and the key cannot disagree.
 *
 * Pure: the shell describes what is open and acts on the answer. Every dialog
 * registers on the layer stack (`TfDialog`, and the rooms UI's `Modal` the
 * asset tree still draws, #1527), so rung 1 sees it even when focus is
 * outside it. `Modal` also answers an `Esc` pressed inside it and stops the
 * event, so that press never reaches the shell.
 */
import { segmentKey } from "./job/job";
import { isTopPage, jobHash, parentOf, parseTfHash, SECTION_NAME, type TfRoute } from "./route";

/** What `Esc` can see when it is pressed. */
export interface EscState {
  /** Open dialogs and drawers, oldest first. */
  layers: readonly { kind: "dialog" | "drawer" }[];
  /** The focused text field, or `null` when focus is not in one. */
  field: { filter: boolean; text: string } | null;
  route: TfRoute;
  /** The page the current history entry was opened from, and its name, or `null` (`opener.ts`). */
  opener: EscTarget | null;
}

/** What one press does. */
export type EscStep =
  | { do: "close"; index: number }
  | { do: "clear-filter" }
  | { do: "blur" }
  /** `opened`: the target is the entry's opener, which the shell may reach by stepping back. */
  | { do: "go"; hash: string; opened: boolean }
  | { do: "nothing" };

/** Where `Esc` goes once nothing is open, and what the breadcrumb calls it. */
export interface EscTarget {
  hash: string;
  name: string;
}

/** An {@link EscTarget}, and whether it is the entry's opener (rung 7) rather than a job's first tab or a structural parent. */
export type EscTargetOf = EscTarget & { opened: boolean };

export function escStep(state: EscState): EscStep {
  for (const kind of ["dialog", "drawer"] as const) {
    const index = state.layers.map((layer) => layer.kind).lastIndexOf(kind);
    if (index !== -1) return { do: "close", index };
  }
  if (state.field) {
    if (state.field.filter && state.field.text !== "") return { do: "clear-filter" };
    return { do: "blur" };
  }
  const target = escTarget(state.route, state.opener);
  return target === null ? { do: "nothing" } : { do: "go", hash: target.hash, opened: target.opened };
}

/**
 * Rungs 5 to 9: where `Esc` goes from `route` once nothing is open, or `null`
 * where it does nothing. `opener` is the current entry's (`opener.ts`).
 */
export function escTarget(route: TfRoute, opener: EscTarget | null): EscTargetOf | null {
  if (route.view === "job" && route.key !== null && route.tab !== null) {
    return { hash: jobHash(route.key), name: segmentKey(route.key), opened: false };
  }
  if (isTopPage(route)) return null;
  if (opener !== null) return { ...opener, opened: true };
  const parent = parentOf(route);
  return parent === null ? null : { hash: parent, name: placeName(parseTfHash(parent)), opened: false };
}

/** What a run's or a pull request's own breadcrumb name is prefixed with when it is a bare number (`#4811`). */
const NUMBERED: Record<string, string> = { build: "Run", pr: "Pull request" };

/**
 * What the breadcrumb calls an opener (#2229): a section's name for a
 * section's own page, the Job page and Tickets (*Back to Job*); otherwise the
 * name the page's own breadcrumb gave itself when it was left (`own`), a bare
 * number under its kind (*Back to Run #4811*), else the section's name.
 */
export function openerName(route: TfRoute, own: string | null): string {
  if (route.view === "full" && route.head !== route.section && own !== null && own !== "") {
    const kind = NUMBERED[route.head];
    return kind !== undefined && own.startsWith("#") ? `${kind} ${own}` : own;
  }
  if (route.view === "item" || route.view === "note") return own || (route.view === "item" ? "Item" : "Note");
  // A ticket page (spec #2227) is named by its ticket's key, as its breadcrumb names it.
  if (route.view === "ticket") return own || segmentKey(route.id);
  return placeName(route) || "the last page";
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
