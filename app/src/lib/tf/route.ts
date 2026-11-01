/**
 * The ticket-first shell's addresses (#809): prototype-6's hash routes, read
 * into the place the reader is standing.
 *
 * Pure. The shell reads `location.hash` and hands it here; nothing in this
 * module touches the window, so every rung of the `Esc` ladder and every nav
 * highlight can be tested as a string.
 *
 * Prototype-6 has more first segments than nav entries: a pull request lives
 * under Code, a run under Builds, a page under Docs, an asset under
 * Dependencies. {@link SECTION_OF} is that mapping, copied from the
 * prototype's `renderNav`, so the nav lights the place a deep address belongs
 * to. `#/map` and `#/assets` are the retired global map and asset page (D13);
 * they still belong to Dependencies, which took their places.
 */
import { repoHash } from "./repo/repo";

/** A place in the main navigation, or Settings, which has its cog instead. */
export type Section =
  | "tickets"
  | "job"
  | "inbox"
  | "code"
  | "builds"
  | "docs"
  | "deps"
  | "monitors"
  | "sets"
  | "review"
  | "time"
  | "settings";

/** What each section is called, in prototype-6's words. */
export const SECTION_NAME: Record<Section, string> = {
  tickets: "Tickets",
  job: "Job",
  inbox: "Inbox",
  code: "Code",
  builds: "Builds",
  docs: "Docs",
  deps: "Dependencies",
  monitors: "Monitors",
  sets: "Link sets",
  review: "Proposed links",
  time: "Time",
  settings: "Settings",
};

/**
 * What a deeper address under a section is called (spec #2227): the one map
 * the breadcrumb (`TfCrumbs.svelte`) and `Esc`'s *Back to <name>*
 * (`escape.ts`) both read, so the two cannot name a place differently. A
 * run's and a pull request's word goes before a bare number (*Run #4811*); a
 * repository's page is *Repository* until it has read its title.
 */
export const HEAD_NAME: Readonly<Record<string, string>> = {
  build: "Run",
  pr: "Pull request",
  repo: "Repository",
  assets: "Asset tree",
  asset: "Asset",
  route: "Route",
  map: "Map",
  branch: "Branch",
  commit: "Commit",
  diff: "Diff",
};

/**
 * The main navigation, left to right (R-1.1). Settings is the cog, not a tab.
 * Job has no entry (spec #2396): the job is a ticket's page, under Tickets.
 */
export const NAV: readonly Exclude<Section, "job">[] = [
  "tickets",
  "inbox",
  "code",
  "builds",
  "docs",
  "deps",
  "monitors",
  "sets",
  "review",
  "time",
];

/** Which section a first path segment belongs to. */
const SECTION_OF: Record<string, Section> = {
  tickets: "tickets",
  job: "job",
  inbox: "inbox",
  code: "code",
  repo: "code",
  pr: "code",
  // A branch's and a commit's addresses (#816's rows open them), which send
  // the reader to the repository page that draws them (#1440).
  branch: "code",
  commit: "code",
  // The diff between any two commits of a repository (#2271).
  diff: "code",
  builds: "builds",
  build: "builds",
  docs: "docs",
  page: "docs",
  deps: "deps",
  assets: "deps",
  asset: "deps",
  map: "deps",
  // A route's address (#432) opens the asset tree at the asset exposing it,
  // which is Dependencies' *Asset tree* (#871).
  route: "deps",
  monitors: "monitors",
  sets: "sets",
  review: "review",
  time: "time",
  settings: "settings",
};

/**
 * Where the reader is.
 *
 * - `tickets` is the list of tickets, and its saved views.
 * - `job` is home (`home.ts`, spec #2227): the empty address lands on a
 *   bare `#/job`, which draws the chosen job's page (`job/chosen.svelte.ts`)
 *   under that address. `#/job/<key or id>[/<tab>]` is the old way in, kept
 *   for links, bookmarks, the timer chip and *Start work* (spec #2396): it
 *   chooses the ticket as the job and lands on its `ticket` address.
 * - `ticket` is one ticket's page, `#/ticket/<entity id>[/<tab>]` (spec
 *   #2396), under Tickets in the nav, the way a run opens under Builds. It
 *   draws the job page, in its reading mood unless the ticket is the chosen
 *   job; opening it chooses no job. A bare `#/ticket` with no id is Tickets.
 * - `full` is every other section: a full view, its first segment kept as
 *   `head` and whatever follows it as `rest` for the ticket that builds it.
 * - `item` is one item's page, `#/item/<entity id>` (#1201): the source's
 *   block view of an item whose kind no surface draws, such as a custom
 *   role's (ADR-0029). It lights no nav entry: a custom role has none.
 * - `note` is one note's read-only page, `#/note/<note id>` (#1504), which
 *   sends the reader to Settings › Notes and capture while notes hide (D10).
 *   It lights no nav entry either: notes have none.
 * - `unknown` is an address this shell has no place for. The shell first asks
 *   `address.ts` whether it is a rooms-UI address (`#/ctx/all`, #1085) and
 *   replaces it; otherwise saying so beats drawing Tickets under
 *   an address that names something else.
 */
export type TfRoute =
  | { view: "tickets"; rest: string[] }
  | { view: "job"; key: string | null; tab: string | null }
  | { view: "ticket"; id: string; tab: string | null }
  | {
      view: "full";
      section: Exclude<Section, "tickets" | "job">;
      /**
       * The address's first segment, which several sections have more than
       * one of: `builds` is the Builds list and `build` a run (#848).
       */
      head: string;
      rest: string[];
    }
  | { view: "item"; id: string }
  | { view: "note"; id: string }
  | { view: "unknown"; hash: string };

/** Read an address. Total: every string is some route. */
export function parseTfHash(hash: string): TfRoute {
  const path = hash.replace(/^#\/?/, "");
  const segments = path === "" ? [] : path.split("/").map(decode);
  const head = segments[0] ?? "";
  if (head === "") return { view: "tickets", rest: [] };
  if (head === "item" && segments[1]) return { view: "item", id: segments[1] };
  if (head === "note" && segments[1]) return { view: "note", id: segments[1] };
  // A ticket's own page (spec #2227); with no id it is Tickets, so a cut-off address still lands.
  if (head === "ticket") {
    return segments[1] ? { view: "ticket", id: segments[1], tab: segments[2] || null } : { view: "tickets", rest: [] };
  }
  // The rooms UI's *Monitors* tab beside *Tree* (#448), whose roster went with
  // #1087: Monitors lives at `#/monitors` now (R-11.3), so this is an old
  // address, which `address.ts`' `legacyAddress` sends there (#1459).
  if (head === "assets" && segments[1] === "monitors") return { view: "unknown", hash };
  const section = SECTION_OF[head];
  if (section === undefined) return { view: "unknown", hash };
  if (section === "tickets") return { view: "tickets", rest: segments.slice(1) };
  if (section === "job") {
    return { view: "job", key: segments[1] || null, tab: segments[2] || null };
  }
  return { view: "full", section, head, rest: segments.slice(1) };
}

/** The section a route lights in the nav, or `null` for an unknown address. */
export function sectionOf(route: TfRoute): Section | null {
  // Every ticket page lights Tickets, the job's too (spec #2396): Job has no entry.
  if (route.view === "tickets" || route.view === "ticket" || route.view === "job") return "tickets";
  if (route.view === "full") return route.section;
  return null;
}

/**
 * The address of a section's own page: `#/inbox`, `#/settings`. Job's needs a
 * ticket, so it has {@link jobHash} instead.
 */
export function sectionHash(section: Exclude<Section, "job">): string {
  return `#/${section}`;
}

/**
 * The address of a job, or of the bare Job page when no ticket is known.
 * `segment` is the ticket's entity id, which names its source (#1977), or the
 * segment a reader opened the job at, handed back as it was.
 */
export function jobHash(segment: string | null): string {
  return segment === null ? "#/job" : `#/job/${encodeURIComponent(segment)}`;
}

/**
 * A ticket's page (spec #2396) by its entity id, on `tab` when one is named.
 * Going there chooses nothing; {@link jobHash} is the address that chooses.
 */
export function ticketHash(entityId: string, tab: string | null = null): string {
  return `#/ticket/${encodeURIComponent(entityId)}${tab === null ? "" : `/${tab}`}`;
}

/**
 * Whether `route` is a section's top page (spec #2227): the address its nav
 * entry or the Settings cog links to. Tickets with a saved view open
 * (`#/tickets/<view>`) and a job on its first tab (`#/job/<key>`, and the
 * bare `#/job`) count, so `Esc` there does nothing.
 */
export function isTopPage(route: TfRoute): boolean {
  if (route.view === "tickets") return true;
  if (route.view === "job") return route.tab === null;
  if (route.view === "full") return route.head === route.section && route.rest.length === 0;
  return false;
}

/**
 * Where a detail page sits under (spec #2227, story 29), as an address, or
 * `null` for a top page, an item's page, a note's page and an unknown
 * address, which have none. Listed first, the pages whose parent is not their
 * section's top page; everything else inside a section goes to that top page.
 *
 * - a run (`#/build/…`) → Builds;
 * - a pull request or a repository, any tab of it → Code;
 * - a commit, which opens under its repository's Commits tab
 *   (`#/repo/<id>/commits/<commit>`) → that repository's page;
 * - a doc page → Docs; an asset or a route → Dependencies;
 * - a ticket page, any tab of it → Tickets.
 *
 * A branch opens on its repository's Branches tab, so its parent is the
 * repository's (Code). `#/branch/<id>` and `#/commit/<id>` themselves name no
 * repository until their page has read the item and replaced the address
 * (`code-row/`), so until then their parent is Code too.
 *
 * A ticket page's tab is not here: it is the `Esc` ladder's own rung.
 */
export function parentOf(route: TfRoute): string | null {
  if (route.view === "ticket") return sectionHash("tickets");
  if (route.view !== "full" || isTopPage(route)) return null;
  if (route.head === "repo" && route.rest[0] && route.rest[1] === "commits" && route.rest[2]) {
    return repoHash(route.rest[0]);
  }
  return sectionHash(route.section);
}

function decode(segment: string): string {
  try {
    return decodeURIComponent(segment);
  } catch {
    return segment;
  }
}
