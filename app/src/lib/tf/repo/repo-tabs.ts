/**
 * The repository page's tabs kept for the visit (#2248): each tab visited
 * inside one repository stays mounted, hidden while another is shown, and
 * shows again as it was left. This module is the bookkeeping, pure: which
 * tab bodies the page holds, the address each was left at, and when each
 * reads again.
 *
 * - A **slot** is one tab body, one component: Files (and its edit), the
 *   Commits list, a commit, Branches, the compare page, Pull requests, Local
 *   changes. A slot re-shown at another address of its own (another file,
 *   another commit) is the same component given that address.
 * - Another repository starts afresh. Another branch keeps only the slot
 *   shown: the hidden ones read at the old branch and would read again on
 *   their return anyway, so they are dropped rather than kept reading.
 * - `reads` is each slot's re-read count, handed to it as its `refreshed`
 *   prop (`refreshed.svelte.ts`): it moves for the slot shown when the page
 *   refreshes or is shown again, and for a slot shown again at the address
 *   it was left at. Shown at another address, the slot reads that address
 *   itself, so the count stays.
 */
import { litTab, readRepoRoute, repoHash, type RepoRoute, type RepoTabId } from "./repo";

export const SLOTS = ["files", "commits", "commit", "branches", "compare", "prs", "local"] as const;
export type TabSlot = (typeof SLOTS)[number];

/** The slot an address of the repository page draws. */
export function slotOf(route: RepoRoute): TabSlot {
  if (route.tab === "edit") return "files";
  if (route.tab === "commits" && route.commit !== null) return "commit";
  return route.tab;
}

export interface KeptTabs {
  /** The repository the slots belong to. */
  repoId: string | null;
  /** The entity the tabs read at (the branch picked). */
  at: string | null;
  /** Each slot held, with what follows `repo` in the address it was last shown at. */
  places: Partial<Record<TabSlot, readonly string[]>>;
  /** The slots held, the one shown last. */
  order: readonly TabSlot[];
  /** Each slot's re-read count; never reset inside one repository, so a count never moves back. */
  reads: Partial<Record<TabSlot, number>>;
}

export const NO_TABS: KeptTabs = { repoId: null, at: null, places: {}, order: [], reads: {} };

const same = (a: readonly string[] | undefined, b: readonly string[]) =>
  a !== undefined && a.length === b.length && a.every((part, i) => part === b[i]);

/** The tabs once the page is shown at `rest`, reading at `at`. */
export function visit(held: KeptTabs, rest: readonly string[], at: string | null): KeptTabs {
  const route = readRepoRoute(rest);
  const slot = slotOf(route);
  if (route.repoId !== held.repoId) {
    return { repoId: route.repoId, at, places: { [slot]: rest }, order: [slot], reads: { [slot]: 0 } };
  }
  const shown = held.order[held.order.length - 1];
  const branched = at !== held.at;
  const places = branched ? {} : { ...held.places };
  const order = branched ? [] : held.order.filter((one) => one !== slot);
  const back = !branched && shown !== slot && same(held.places[slot], rest);
  const reads = { ...held.reads, [slot]: (held.reads[slot] ?? 0) + (back ? 1 : 0) };
  places[slot] = rest;
  return { repoId: held.repoId, at, places, order: [...order, slot], reads };
}

/** The shown slot reads again: the page's refresh, or the page shown again. */
export function readShown(held: KeptTabs): KeptTabs {
  const shown = held.order[held.order.length - 1];
  if (shown === undefined) return held;
  return { ...held, reads: { ...held.reads, [shown]: (held.reads[shown] ?? 0) + 1 } };
}

/** The address a tab of the row links: where it was last shown, or the tab itself. */
export function tabHref(held: KeptTabs, repoId: string, tab: RepoTabId): string {
  for (let i = held.order.length - 1; i >= 0; i -= 1) {
    const rest = held.places[held.order[i]!];
    if (rest === undefined || litTab(readRepoRoute(rest).tab) !== tab) continue;
    if (rest.length < 2) break;
    return `#/repo/${rest.map(encodeURIComponent).join("/")}`;
  }
  return repoHash(repoId, tab);
}
