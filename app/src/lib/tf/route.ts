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

/** The main navigation, left to right (R-1.1). Settings is the cog, not a tab. */
export const NAV: readonly Section[] = [
  "tickets",
  "job",
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
 * - `tickets` is home: the empty address lands here, as in the prototype.
 * - `job` carries the ticket key and the job's tab (`#/job/PAY-231/conv`),
 *   either of which may be absent.
 * - `full` is every other section: a full view, its first segment kept as
 *   `head` and whatever follows it as `rest` for the ticket that builds it.
 * - `item` is one item's page, `#/item/<entity id>` (#1201): the source's
 *   block view of an item whose kind no surface draws, such as a custom
 *   role's (ADR-0029). It lights no nav entry: a custom role has none.
 * - `note` is one note's read-only page, `#/note/<note id>` (#1504), which
 *   sends the reader to Settings › Notes and capture while notes hide (D10).
 *   It lights no nav entry either: notes have none.
 * - `unknown` is an address this shell has no place for. The shell first asks
 *   `address.ts` whether it is a rooms-UI address (`#/ticket/jira:PAY-231`,
 *   #1085) and replaces it; otherwise saying so beats drawing Tickets under
 *   an address that names something else.
 */
export type TfRoute =
  | { view: "tickets"; rest: string[] }
  | { view: "job"; key: string | null; tab: string | null }
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
  if (route.view === "tickets") return "tickets";
  if (route.view === "job") return "job";
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

function decode(segment: string): string {
  try {
    return decodeURIComponent(segment);
  } catch {
    return segment;
  }
}
