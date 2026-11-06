/**
 * **Opener tracking** (#2229, spec #2227): the address each history entry was
 * opened from, so `Esc` returns to the page that opened a detail, the way
 * Linear, Mail and Finder do.
 *
 * The shell stamps every entry it sees with an {@link Opened} in the entry's
 * history state, so the opener survives `⌘[` / `⌘]` and stays with its own
 * entry. A new entry (one with no stamp yet) reached by an in-app link or
 * button from another page gets that page as its opener. These get none:
 *
 * - an address set through {@link goWithoutOpener}: the capture window, a
 *   notification's click (a deep link), a nav entry;
 * - the window's first address (startup), and a rooms address the shell
 *   replaces (the shell stamps those itself).
 *
 * A switch between one page's own tabs ({@link samePage}: a pull request's
 * Files, a run's Tests, a job's sub-tab) is not an opening: the new entry
 * keeps the page's opener. An address replaced in place through
 * {@link replaceAddress} (a bare run's address once its pipeline is known) is
 * the same entry and keeps its stamp. A page that replaces its address must
 * use it: a bare `location.replace` would read as a new entry opened from the
 * address it replaced.
 *
 * **The opener is a chain, not one link.** A stamp holds what its opener's own
 * stamp said ({@link Opener}'s `from`). An `Esc` step that can't step back in
 * history (a tab switch came between, so the opener is not the entry before)
 * goes through {@link goBackTo}, and the new entry takes that stamp, so the same
 * trip ends with the same *Back to <name>* whichever tabs it went through.
 *
 * **A card row's narrowing goes with the entries** ({@link Trip}), so stepping to
 * an entry by `Esc`, *Back to* or `⌘[` and `⌘]` narrows the section as that entry
 * was, and a nav entry or the launcher ends it. Three presses stamp explicitly,
 * because the address alone can't say what they mean: {@link goAcross}, a jump to
 * another place on one page; {@link goVisiting}, a top page opened from
 * somewhere to come back from; and {@link goLaunched}, the launcher's jump, which
 * is both of those and ends the narrowing.
 */
import type { Trip } from "./job/scoped.svelte";
import { parseTfHash, type TfRoute } from "./route";

export type { Trip };

/** How an entry came to be, without the entry before it: what a page's next entry can inherit. */
export interface Came {
  /** The page it was opened from, or `null`. */
  opener: Opener | null;
  /** The card row's narrowing this entry is part of, or `null`. */
  trip: Trip | null;
  /** A top page opened to be returned from, so its `Esc` follows the opener ({@link goVisiting}). */
  visit: boolean;
}

/** What an entry's history state says about how it was opened. */
export interface Opened extends Came {
  /** The address of the entry before it when it was made, so `Esc` can step back instead of pushing. */
  prev: string | null;
}

/** A page something was opened from: its address, what the breadcrumb calls it, and how it came to be. */
export interface Opener {
  hash: string;
  name: string;
  from: Came;
}

/** An entry opened by nothing, as the window's first address is. */
export const NOT_OPENED: Came = { opener: null, trip: null, visit: false };

/** How long a chain a stamp keeps. History state is copied on every write, and nobody presses `Esc` further than this. */
const CHAIN = 24;

function trimmed(opener: Opener | null, left: number): Opener | null {
  if (opener === null || left === 0) return null;
  return { ...opener, from: { ...opener.from, opener: trimmed(opener.from.opener, left - 1) } };
}

/** The page at `hash`, called `name`, as the opener of what it opens next. */
export function openerOf(hash: string, name: string, came: Came): Opener {
  return { hash, name, from: { opener: trimmed(came.opener, CHAIN - 1), trip: came.trip, visit: came.visit } };
}

/** The key the stamp sits under in `history.state`, beside whatever else is there. */
const KEY = "knobasOpened";

/** How an address change was made, when it is not a plain in-app link's. */
export type Told = "unopened" | "replace" | "back" | "across" | "visit" | "launched";

/** The next address change the shell is told about, and what to call the place it left where the page's own name is not it. */
let told: { hash: string; how: Told; left?: string | undefined } | null = null;
let toldLeft: string | undefined;

function goTold(hash: string, how: Told, left?: string): void {
  location.hash = hash;
  told = { hash: location.hash, how, left };
}

/** Go to `hash` as a new entry with no opener: a deep link, a nav entry, an `Esc` step to a parent. */
export function goWithoutOpener(hash: string): void {
  goTold(hash, "unopened");
}

/** `Esc`'s step to the opener when it is not the entry before: a new entry, stamped as the opener's own was. */
export function goBackTo(hash: string): void {
  goTold(hash, "back");
}

/**
 * A jump to another place on this page, which is an opening though the address
 * is the same page's: a link from one asset to another across the tree, as a walk
 * through its columns is not. `Esc` comes back to where the reader was reading.
 * `left` names that place when the page's name would not say it: a press from a repository's
 * Branches to its Compare is *Back to Branches*, not the repository they are both on.
 */
export function goAcross(hash: string, left?: string): void {
  goTold(hash, "across", left);
}

/**
 * Open a section's top page to come back from. A top page's `Esc` does nothing,
 * since a nav entry brought the reader there. Pressed from a ticket's dialog
 * (*Manage link sets*), it says where it came from, as a card's row does.
 */
export function goVisiting(hash: string): void {
  goTold(hash, "visit");
}

/**
 * The launcher's jump (F-047, overruling #2229's "the launcher gets none"): the page it was pressed over is the opener,
 * as for any other press, and a top page counts as visited, so `Esc` and the bar come back from `>settings` and `asset`
 * too. A card row's narrowing still ends.
 */
export function goLaunched(hash: string): void {
  goTold(hash, "launched");
}

/** Put `hash` in place of the address the reader is on: the same entry, its opener kept. */
export function replaceAddress(hash: string): void {
  location.replace(hash);
  told = { hash: location.hash, how: "replace" };
}

/** How the address change to `hash` was made, if it was told, and the name given for the place left; forgotten once asked. */
export function takeTold(hash: string): Told | null {
  const was = told;
  told = null;
  toldLeft = was !== null && was.hash === hash ? was.left : undefined;
  return was !== null && was.hash === hash ? was.how : null;
}
/** The name {@link goAcross} gave the place it left, for the change {@link takeTold} just answered about. */
export const leftName = (): string | undefined => toldLeft;

/** The stamp in a history state, or `null` when the entry has none yet. */
export function stampOf(state: unknown): Opened | null {
  if (typeof state !== "object" || state === null || !(KEY in state)) return null;
  return (state as Record<string, Opened>)[KEY] ?? null;
}

/** `state` with `opened` stamped in it, keeping what else it holds. */
export function stamped(state: unknown, opened: Opened): object {
  return { ...(typeof state === "object" && state !== null ? state : {}), [KEY]: opened };
}

/**
 * Which page an address is, ignoring the page's own tabs: a job's sub-tab, a
 * pull request's or a run's tab, a repository's tabs and files, a doc page's
 * section, and a section's own deeper addresses (`#/code/repos`, a pipeline
 * picked on Builds, a saved view on Tickets) are one page, and so are the asset
 * tree and an asset in it. A commit under its repository is a page of its own,
 * as `parentOf` has it, and so are a link set and a Docs space under their
 * section: each names itself, so *Back to* can name it.
 */
export function pageOf(route: TfRoute): string {
  switch (route.view) {
    case "tickets":
      return "tickets";
    case "job":
      return `job/${route.key ?? ""}`;
    case "item":
    case "note":
    case "ticket":
      return `${route.view}/${route.id}`;
    case "unknown":
      return route.hash;
    case "full": {
      if (route.head === "sets" && route.rest[0]) return `sets/${route.rest[0]}`;
      if (route.head === "docs" && route.rest[0] === "space") return `docs/space/${route.rest[1] ?? ""}`;
      if (route.head === route.section && route.head !== "assets") return route.head;
      // The tree and an asset in it are one page: a walk through its columns keeps how the tree was opened.
      if (route.head === "assets" || route.head === "asset") return "assets";
      if (route.head === "repo" && route.rest[1] === "commits" && route.rest[2]) {
        return ["repo", route.rest[0], "commits", route.rest[2]].join("/");
      }
      // A run is its pipeline and its id; a diff its repository and both ends.
      const ids = route.head === "build" ? 2 : route.head === "diff" ? 3 : 1;
      return [route.head, ...route.rest.slice(0, ids)].join("/");
    }
  }
}

/** Whether two addresses are one page, so moving between them is a tab switch, not an opening. */
export function samePage(a: string, b: string): boolean {
  return pageOf(parseTfHash(a)) === pageOf(parseTfHash(b));
}
