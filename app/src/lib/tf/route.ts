/**
 * The ticket-first shell's addresses (#809): prototype-6's hash routes, read
 * into the place the reader is standing.
 *
 * Pure. The shell reads `location.hash` and hands it here; nothing in this
 * module touches the window, so every rung of the `Esc` ladder and every nav
 * highlight can be tested as a string.
 *
 * Prototype-6 has more first segments than nav entries: a pull request lives
 * under Code, a run under Builds, a page under Docs. {@link SECTION_OF} is that
 * mapping, copied from the prototype's `renderNav`, so the nav lights the place
 * a deep address belongs to. The asset tree is *Assets*, a section with its own entry after Monitors
 * (Björn, 2026-11-19: `docs/decisions/2026-11-19-assets-has-a-nav-entry-again.md`).
 */
import { fileHash, readRepoRoute, repoHash } from "./repo/repo";

/** A place in the main navigation, or Settings, which has its cog instead. */
export type Section =
  | "tickets"
  | "job"
  | "inbox"
  | "code"
  | "builds"
  | "docs"
  | "secrets"
  | "monitors"
  | "assets"
  | "sets"
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
  secrets: "Secrets",
  monitors: "Monitors",
  assets: "Assets",
  sets: "Link sets",
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
  assets: "Assets",
  asset: "Asset",
  route: "Route",
  branch: "Branch",
  commit: "Commit",
  diff: "Diff",
};

/**
 * The main navigation, left to right (R-1.1). Settings is the cog, not a tab.
 * Job has no entry (spec #2396): the job is a ticket's page, under Tickets.
 * Assets, the asset tree, follows Monitors, whose monitors watch what it holds (2026-11-19).
 */
export const NAV: readonly Exclude<Section, "job">[] = [
  "tickets",
  "inbox",
  "code",
  "builds",
  "docs",
  "secrets",
  "monitors",
  "assets",
  "sets",
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
  // A secrets source's entries (ADR-0056, #2515): `#/secrets[/in/<folder>|/source/<id>|/entry/<entry>]`.
  // Legacy `#/passwords/...` addresses parse as `unknown` and redirect via `address.ts`' `legacyAddress`.
  secrets: "secrets",
  monitors: "monitors",
  // The asset tree (#871). A route's address (#432) opens it at the asset

  // exposing the route.
  assets: "assets",
  asset: "assets",
  route: "assets",
  sets: "sets",
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
  /** A source's own website inside knobas (ADR-0050): `#/site/<source id>`. No nav entry, no parent. */
  | { view: "site"; source: string }
  | { view: "unknown"; hash: string };

/** Read an address. Total: every string is some route. */
export function parseTfHash(hash: string): TfRoute {
  const path = hash.replace(/^#\/?/, "");
  const segments = path === "" ? [] : path.split("/").map(decode);
  const head = segments[0] ?? "";
  if (head === "") return { view: "tickets", rest: [] };
  if (head === "item" && segments[1]) return { view: "item", id: segments[1] };
  if (head === "note" && segments[1]) return { view: "note", id: segments[1] };
  if (head === "site" && segments[1]) return { view: "site", source: segments[1] };
  // A ticket's own page (spec #2227); with no id it is Tickets, so a cut-off address still lands.
  if (head === "ticket") {
    return segments[1] ? { view: "ticket", id: segments[1], tab: segments[2] || null } : { view: "tickets", rest: [] };
  }
  // The rooms UI's *Monitors* tab beside *Tree* (#448), whose roster went with
  // #1087: Monitors lives at `#/monitors` now (R-11.3), so this is an old
  // address, which `address.ts`' `legacyAddress` sends there (#1459).
  if (head === "assets" && segments[1] === "monitors") return { view: "unknown", hash };
  // The tree with nothing selected, under any old tab name (`#/assets/tree`): `shell/router.ts` reads them as one.
  if (head === "assets") return { view: "full", section: "assets", head, rest: [] };
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
 * The ticket page's one meaningful `/<tab>` (the page has no tabs, #2399):
 * `#/ticket/<id>/mention` draws the same page, brought to the newest comment
 * that mentions the reader. What an inbox mention opens.
 */
export const MENTION_TAB = "mention";

/**
 * A ticket's page (spec #2396) by its entity id, on `tab` when one is named.
 * Going there chooses nothing; {@link jobHash} is the address that chooses.
 */
export function ticketHash(entityId: string, tab: string | null = null): string {
  return `#/ticket/${encodeURIComponent(entityId)}${tab === null ? "" : `/${tab}`}`;
}

/** The word in Monitors' address that opens it at one monitor: {@link monitorHash}. */
export const MONITOR_AT = "at";

/**
 * Monitors opened at one monitor, its row marked: what a row or link that means
 * that monitor opens (an inbox alert, a set's member). The page's own, like a tab.
 */
export function monitorHash(entityId: string): string {
  return `#/monitors/${MONITOR_AT}/${encodeURIComponent(entityId)}`;
}

/**
 * The section whose nav entry should come back to `route`, or `null`: a saved
 * view on Tickets, the tabs of Code, Monitors and Time, a pipeline on Builds,
 * so the address says what the page shows, a search on Docs, and a section of
 * Settings, which the cog comes back to. A deeper address (a set, a Docs space,
 * a marked monitor, a source's page) is a place, and the entry is the way to the section's own page.
 */
export function sectionTabOf(route: TfRoute): Section | null {
  if (route.view === "tickets") return "tickets";
  if (route.view !== "full" || route.head !== route.section) return null;
  if (route.section === "code" || route.section === "time" || route.section === "builds") return route.section;
  if (route.section === "docs") return route.rest[0] === undefined || route.rest[0] === "search" ? "docs" : null;
  // A source's page, an application's (`#/settings/checkouts/add`, PR #2479), a folder renderer's and *Add a source*'s
  // (`#/settings/sources/add`, ux31) are places, not the section's tab.
  if (route.section === "settings") return route.rest[0] === "source" || (route.rest[0] === "checkouts" && route.rest.length > 1) || (route.rest[0] === "renderers" && route.rest[1] === "folder") || (route.rest[0] === "sources" && route.rest[1] === "add") ? null : "settings";
  return route.section === "monitors" && route.rest[0] !== MONITOR_AT ? "monitors" : null;
}

/**
 * The run, pull request or repository `route` stands on, as its address with no tab, or `null`: the items a section
 * remembers when one is left by a nav press, so its row can bring it back. A ticket and a Docs page have no such place:
 * a ticket has no tab to lose, and Docs has *Continue reading*.
 */
export function itemOf(route: TfRoute): string | null {
  if (route.view !== "full") return null;
  const [id, run] = route.rest;
  if (id === undefined) return null;
  if (route.head === "build") return run === undefined ? null : `#/build/${encodeURIComponent(id)}/${encodeURIComponent(run)}`;
  if (route.head === "pr" || route.head === "repo") return `#/${route.head}/${encodeURIComponent(id)}`;
  return null;
}

/** Whether `to` is the bare address of the item the address `left` stands on: the one link that reopens it. A link that names a tab is not. */
export function reopens(left: string, to: string): boolean {
  const route = parseTfHash(to);
  const item = itemOf(route);
  return item !== null && item === itemOf(parseTfHash(left)) && route.view === "full" && route.rest.length === (route.head === "build" ? 2 : 1);
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
 * Where a page sits in its own section (spec #2227, story 29; #2559), as an address, or `null` where `Esc` goes
 * nowhere. `Esc` is not Back (Björn, 2026-11-27, #2559): it goes one level up this chain, whatever opened the page.
 * Each section's chain, the one place it is written down (the page's own `parentPage` only names a step the address
 * cannot, never another):
 *
 * - **Tickets**: a ticket's page, any tab of it → Tickets. Tickets with a saved view and home (`#/job`) → nowhere.
 * - **Inbox, Time**: their tabs (Time's day, week and standup, a week picked in it) are the section's page → nowhere.
 * - **Code**: a file in a repository (and its line, place, folder render and generated folder) → the repository
 *   (`#/repo/<id>`, its Files tab); a file's edit → the file; a commit → the repository's Commits tab; a repository on
 *   any tab, a pick in it included (Compare's branches, Builds' branch and pipeline) → Code (its Repositories); a pull
 *   request → its repository (the page says which, `parentPage`), else Code; a diff → its repository (the page says);
 *   a bare branch or commit address, until its page names the repository → Code. Code's own tabs → nowhere.
 * - **Builds**: a run → its pipeline; a pipeline, any tab of it, and a folder → All pipelines (`#/builds/recent`, not
 *   the bare `#/builds`, which draws the last place stood on). All pipelines and a bare Builds → nowhere.
 * - **Docs**: a page → its space (the page says, `parentPage`), else Docs; a space, a search → Docs.
 * - **Secrets**: a folder, a source, an entry → Secrets.
 * - **Monitors**: a marked monitor → Monitors. Its tabs → nowhere.
 * - **Assets**: an asset → its holder (the page says), else Assets; a route → Assets.
 * - **Link sets**: a set → Link sets.
 * - **Settings**: a source's page and *Add a source* → Sources; an application's page → Checkouts; a command's page →
 *   Commands; a folder renderer's page → Renderers; a child section (Appearance › Fonts) → its parent section. A
 *   section at the top of its rail is the Settings page's own → nowhere.
 * - An item's page, a note's page, a site and an unknown address have no section → nowhere (a site has its own rung,
 *   `escape.ts`).
 */
export function parentOf(route: TfRoute): string | null {
  if (route.view === "ticket") return sectionHash("tickets");
  if (route.view !== "full" || isTopPage(route)) return null;
  if (route.head === "repo") return repoParent(route.rest);
  // A run sits under its pipeline. A bare run's address names none yet.
  if (route.head === "build" && route.rest[0] && route.rest[1]) return `#/builds/${encodeURIComponent(route.rest[0])}`;
  // A pipeline picked on Builds, any tab of it, and a folder sit under Builds' own page, All pipelines (#2559). Not the
  // bare `#/builds`: that draws the last place stood on, the page just left (#2570, Ruling 12). All pipelines is the top.
  if (route.head === "builds") return route.rest.length === 1 && route.rest[0] === "recent" ? null : BUILDS_ALL;
  // A monitor's mark sits on Monitors' own page: leaving it is the page with no mark.
  if (route.head === "monitors" && route.rest[0] === MONITOR_AT) return sectionHash("monitors");
  // A section's own tabs are the section's page: its parent would be a sibling.
  if (TABBED_HEADS.has(route.head)) return null;
  if (route.head === "settings") return settingsUp(route.rest);
  return sectionHash(route.section);
}

/** A repository's address's parent (#2559): a place inside a tab → the tab with nothing open, a tab → Code. */
function repoParent(rest: readonly string[]): string {
  const at = readRepoRoute(rest);
  if (at.repoId === null) return sectionHash("code");
  if (at.commit !== null) return repoHash(at.repoId, "commits");
  if (at.tab === "edit" && at.path !== null) return fileHash(at.repoId, at.path);
  if (at.tab === "files" && (at.path !== null || at.render !== undefined || at.rendered !== undefined)) return repoHash(at.repoId);
  return sectionHash("code");
}

/** A Settings address's parent: a page under a section → that section, a child section → its parent, else nowhere. */
function settingsUp(rest: readonly string[]): string | null {
  const [head, arg] = rest;
  if (head === "source" || (head === "sources" && arg === "add")) return "#/settings/sources";
  if (arg === undefined) return null;
  if (head === "renderers" && arg === "folder") return "#/settings/renderers";
  if (head === "checkouts" || head === "commands") return `#/settings/${head}`;
  // A child section (the subtrees ruling, 2026-11-21): `#/settings/appearance/fonts` sits under Appearance.
  return head === undefined ? null : `#/settings/${encodeURIComponent(head)}`;
}

/** All pipelines, Builds' own page (`RECENT_HASH` in `builds/recent.ts`, which imports this module). */
const BUILDS_ALL = "#/builds/recent";

/** The sections whose deeper addresses are their own page's tabs or a pick in its rail. */
const TABBED_HEADS: ReadonlySet<string> = new Set(["code", "time", "monitors", "inbox"]);

function decode(segment: string): string {
  try {
    return decodeURIComponent(segment);
  } catch {
    return segment;
  }
}
