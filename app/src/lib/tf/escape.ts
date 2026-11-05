/**
 * The ticket-first shell's `Esc` (#809, spec #2227): one layer per press, in
 * this order. `Esc` goes up one level, never sideways.
 *
 * 1. a dialog is open           → close the topmost dialog;
 * 2. a drawer is open           → close it;
 * 3. a filter field has text    → clear it, focus stays;
 * 4. a text field has focus     → blur it;
 * 5. a top page                 → nothing;
 * 6. an opener is known         → go to the opener;
 * 7. a structural parent exists → go to it;
 * 8. otherwise                  → nothing.
 *
 * The ticket page has no tabs (#2399), so it has no rung of its own: an old
 * `/map` or `/conversation` address is the page, and `Esc` leaves it.
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
 * With no opener, the structural parent is the crumb above the page: what the
 * page says (`parentPage`: a pull request's repository, a run's pipeline by its
 * name, an asset's holder), else `parentOf`'s from the address. A section's own
 * tabs and a pipeline picked on Builds have no parent: they are the section's
 * page, so `Esc` never goes sideways to a sibling.
 * An item's page and a note's page have no parent, so rung 8 leaves them
 * where they are. `Esc` never reads "the job you were last in" (#2228); a
 * key that sometimes does nothing is what makes the others safe to press.
 *
 * Rungs 5 to 8 are {@link escTarget}, which the breadcrumb's *Back to <name>*
 * reads too, so the hint and the key cannot disagree. No page answers `Esc` for
 * itself on those rungs: the asset tree's `←` walks up its columns, and its `Esc`
 * follows the bar like any page's, whatever has focus.
 *
 * Pure: the shell describes what is open and acts on the answer. Every dialog
 * registers on the layer stack (`TfDialog`, the one dialog shell, #1527), so
 * rung 1 sees it even when focus is outside it. `TfDialog` also answers an
 * `Esc` pressed inside it and stops the event, so that press never reaches the
 * shell.
 */
import { segmentKey } from "./job/job";
import { HEAD_NAME, isTopPage, parentOf, parseTfHash, SECTION_NAME, type TfRoute } from "./route";

/** What `Esc` can see when it is pressed. */
export interface EscState {
  /** Open dialogs and drawers, oldest first. */
  layers: readonly { kind: "dialog" | "drawer" }[];
  /** The focused text field, or `null` when focus is not in one. */
  field: { filter: boolean; text: string } | null;
  route: TfRoute;
  /** The page the current history entry was opened from, and its name, or `null` (`opener.ts`). */
  opener: EscTarget | null;
  /** A ticket card's row opened this section's page, narrowed to that ticket (spec #2401). */
  fromTicketRow?: boolean;
  /** The place the page says it sits under (`page-name.svelte.ts`), where the address cannot say it. */
  parent?: EscTarget | null;
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

/** An {@link EscTarget}, and whether it is the entry's opener (rung 6) rather than a structural parent. */
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
  const target = escTarget(state.route, state.opener, state.fromTicketRow, state.parent);
  return target === null ? { do: "nothing" } : { do: "go", hash: target.hash, opened: target.opened };
}

/**
 * Rungs 5 to 8: where `Esc` goes from `route` once nothing is open, or `null`
 * where it does nothing. `opener` is the current entry's (`opener.ts`).
 * A section's top page does nothing, unless a ticket card's row opened it
 * narrowed to that ticket (`fromTicketRow`, spec #2401): then it goes back.
 * Settings and the asset tree are the others: places with no nav entry, visited
 * from somewhere and returned from, Settings on every one of its sections alike,
 * and the tree on its bare root as on an asset in it.
 */
export function escTarget(route: TfRoute, opener: EscTarget | null, fromTicketRow = false, parent: EscTarget | null = null): EscTargetOf | null {
  const visited = route.view === "full" && (route.section === "settings" || route.section === "assets");
  if (isTopPage(route) && !(opener !== null && (fromTicketRow || visited))) return null;
  if (opener !== null) return { ...opener, opened: true };
  if (parent !== null) return { ...parent, opened: false };
  const up = parentOf(route);
  return up === null ? null : { hash: up, name: placeName(parseTfHash(up)), opened: false };
}

/**
 * What the breadcrumb calls an opener (#2229): the name the page gave itself
 * (`own`, `page-name.svelte.ts`) when it was left, so a section's deeper address
 * is called what `Esc` lands on (*Back to Payout_Lint*, *Back to Engineering*,
 * *Back to Payout stack*), a bare number under its kind's {@link HEAD_NAME}
 * (*Back to Run #4811*); the key a ticket page or home's job page named itself
 * (*Back to PAY-231*), else *Job* for home; else what {@link HEAD_NAME} calls the
 * address, else the section's name.
 */
export function openerName(route: TfRoute, own: string | null): string {
  if (route.view === "full" && own !== null && own !== "") {
    const kind = HEAD_NAME[route.head];
    return kind !== undefined && own.startsWith("#") ? `${kind} ${own}` : own;
  }
  if (route.view === "item" || route.view === "note") return own || (route.view === "item" ? "Item" : "Note");
  // A ticket page (spec #2227) is named by its ticket's key, as its breadcrumb names it.
  if (route.view === "ticket") return own || segmentKey(route.id);
  // Home draws the chosen job's page (spec #2396), which names itself by its key.
  if (route.view === "job") return own || SECTION_NAME.job;
  return placeName(route) || "the last page";
}

/**
 * What the breadcrumb calls a place `Esc` goes to: a section's name for its
 * own pages, else what {@link HEAD_NAME} calls a deeper address (a
 * repository's page is *Repository*: its title only its own read knows, and
 * `TfRepo` names it once it has), else its section's name.
 */
function placeName(route: TfRoute): string {
  if (route.view === "tickets") return SECTION_NAME.tickets;
  if (route.view === "job") return SECTION_NAME.job;
  if (route.view !== "full") return "";
  if (route.head === "settings" && route.rest[0] === "sources") return "Sources";
  return (route.head !== route.section ? HEAD_NAME[route.head] : undefined) ?? SECTION_NAME[route.section];
}
