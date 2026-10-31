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
 * - an address set through {@link goWithoutOpener}: the ⌘K launcher, the
 *   capture window, a notification's click (a deep link), an `Esc` step;
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
 */
import { parseTfHash, type TfRoute } from "./route";

/** What an entry's history state says about how it was opened. */
export interface Opened {
  /** The address it was opened from, and what the breadcrumb calls that page, or `null`. */
  opener: { hash: string; name: string } | null;
  /** The address of the entry before it when it was made, so `Esc` can step back instead of pushing. */
  prev: string | null;
}

/** The key the stamp sits under in `history.state`, beside whatever else is there. */
const KEY = "knobasOpened";

/** The next address change the shell is told about, when it is not an in-app link's. */
let told: { hash: string; how: "unopened" | "replace" } | null = null;

/** Go to `hash` as a new entry with no opener: the launcher, a deep link, an `Esc` step. */
export function goWithoutOpener(hash: string): void {
  location.hash = hash;
  told = { hash: location.hash, how: "unopened" };
}

/** Put `hash` in place of the address the reader is on: the same entry, its opener kept. */
export function replaceAddress(hash: string): void {
  location.replace(hash);
  told = { hash: location.hash, how: "replace" };
}

/** How the address change to `hash` was made, if it was told; forgotten once asked. */
export function takeTold(hash: string): "unopened" | "replace" | null {
  const was = told;
  told = null;
  return was !== null && was.hash === hash ? was.how : null;
}

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
 * picked on Builds, a saved view on Tickets) are one page. A commit under its
 * repository is a page of its own, as `parentOf` has it.
 */
export function pageOf(route: TfRoute): string {
  switch (route.view) {
    case "tickets":
      return "tickets";
    case "job":
      return `job/${route.key ?? ""}`;
    case "item":
    case "note":
      return `${route.view}/${route.id}`;
    case "unknown":
      return route.hash;
    case "full": {
      if (route.head === route.section) return route.head;
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
