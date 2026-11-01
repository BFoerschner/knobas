/**
 * **The new shell's pages, registered per feature** (#908).
 *
 * Every screen of the ticket-first shell lives in its own folder and says
 * which addresses it draws in that folder's `page.ts`: a {@link definePage}
 * call naming the component, the routes it matches and the props it takes
 * from the shell. `TfPage.svelte` draws the first page that matches and owns
 * only the shell's own fallbacks (a bare `#/job`, a section not built yet, an
 * unknown address). So a new screen adds a folder and edits no shared file.
 *
 * What a page can read is {@link TfPageContext}. A screen that needs a value
 * the shell does not hand out yet adds it there and to the `ctx` that
 * `TfShell.svelte` builds, which is the one shared edit left.
 *
 * **Keep-alive** (#2247): the host does not destroy the page you leave. It
 * keeps it mounted and hidden, up to *Pages kept open* of them (Settings ›
 * Performance, 0 to 15, 5 until set; 0 is the shell before keep-alive), and
 * drops the least recently used past that (`kept-pages.svelte.ts`). Going back
 * to a kept page shows the same instance, with what it drew still there.
 *
 * - **One page per address key**: the entry plus {@link TfPageEntry.address}.
 *   Two addresses with the same key are one page drawn again with new props
 *   (the job's tabs); two keys are two pages, even of one component (two
 *   repositories). A page without `address` is one page wherever it is.
 * - **`visible`** on the context is `true` for the shown page and `false` for a
 *   hidden one. A hidden page keeps the props of the address it was left at.
 *   A page that reads `visible` pauses its focus re-reads, polling and timers
 *   while hidden and re-reads when shown (#2249); a page that ignores it still
 *   works. A component deep inside a page asks `pageVisible()` instead.
 * - **Shown again**, a role view's `syncRevision` moves, so it re-reads as it
 *   does after *Sync now*; the shell's ⌘R and a page's own keys reach only
 *   the shown page.
 */
import type { Component, Snippet } from "svelte";

import type { Timer } from "../shell/timer.svelte";
import type { JobPorts } from "./job/ports";
import type { Layers } from "./layers.svelte";
import type { RoleSyncs } from "./role-sync.svelte";
import type { Theme } from "./theme.svelte";
import type { EscTarget } from "./escape";
import type { TfRoute } from "./route";

/** What the shell hands every page. */
export interface TfPageContext {
  route: TfRoute;
  /**
   * Whether this page is the one shown (#2247): `false` while keep-alive holds
   * it hidden. Set by the page host, not the shell.
   */
  visible: boolean;
  /** Whether the database answers yet: a page's reads wait for it. */
  ready: boolean;
  /**
   * The chosen job (spec #2227), as its key; on a page of the section a
   * ticket card's sections row opened, that ticket (#2401, set per page by
   * `TfKeptPage`).
   */
  job: string | null;
  /** That job's entity id, which its ticket is read by (#1525: a key two sources hold reads the one chosen). */
  jobAt: string | null;
  /**
   * Where `Esc` goes once nothing is open, and what the breadcrumb's *Back to
   * <name>* calls it (`escTarget` in `escape.ts`, #2228); `null` where `Esc`
   * does nothing, which draws no hint.
   */
  back: EscTarget | null;
  /** The ticket key the timer runs on, or `null`. */
  timedTicket: string | null;
  /** The Job tab's job (the chosen job, else the timer's ticket) with its entity id, or `null`. */
  navJob: { key: string; entityId: string } | null;
  timer: Pick<Timer, "current" | "elapsed">;
  layers: Layers;
  /** The colour theme the top bar toggles and Settings › Appearance sets. */
  theme: Theme;
  /** The timer verb, which stops a running timer into the worklog draft. */
  ontimer: (() => void) | undefined;
  /** Start the timer on a ticket (#816's *Start timer*). */
  onstarttimer: (entityId: string, title: string) => void;
  /** The job page's reads. */
  jobPorts: JobPorts;
  /** Open the worklog draft for a ticket and day (the Time view's *Log…*). */
  onlog: ((entityId: string, day: string) => void) | undefined;
  /** Bumped when something outside the Time view changed a day's blocks. */
  timeRevision: number;
  ontimechanged: (() => void) | undefined;
  /** Each role's *Sync now* (#2106), which the role frame draws into a role view's header. */
  roleSyncs: RoleSyncs;
}

/**
 * What the role frame hands a page that names a {@link TfPageEntry.role}
 * (#2106), beside the props it reads off the context: its header draws `sync`
 * where its own actions go, and `syncRevision` moves when the runs it
 * started have all ended, so a view not re-read on a run's end by itself
 * reads again.
 */
export interface RoleFrameProps {
  sync?: Snippet;
  syncRevision?: number;
}

/** One registered page, its types erased once {@link definePage} checked them. */
export interface TfPageEntry {
  /** Whether this page draws `route`. */
  matches: (route: TfRoute) => boolean;
  component: Component<Record<string, unknown>>;
  /**
   * Whether the shell draws its breadcrumb (R-1.6) above the page, as for a
   * full view. A page that draws its own, like Settings, leaves it unset.
   */
  crumbs?: boolean;
  /**
   * The role the page is a view of (#2106), as `knobas_source::role` spells
   * it. The frame then draws *Sync now* for it into the page's header, and
   * hands the page {@link RoleFrameProps}.
   */
  role?: string;
  /**
   * What tells two pages of this entry apart (#2247): routes with the same
   * answer are one page instance, which keep-alive keeps and shows again. Left
   * out, every route the entry matches is one page.
   */
  address?: (route: TfRoute) => string;
  /**
   * The component's props, read off the shell's context for a matched route.
   * Always the same keys: `TfPage` reads them once per page and hands each
   * as its own signal (#1987), so a key left out on one call is never seen.
   */
  props: (ctx: TfPageContext) => Record<string, unknown>;
}

/**
 * A page whose props are checked against its component's. `matches` narrows
 * the route, so `props` reads the fields of the route it matched.
 */
export function definePage<Props extends object, Matched extends TfRoute>(page: {
  matches: (route: TfRoute) => route is Matched;
  component: Component<Props>;
  crumbs?: boolean;
  role?: string;
  address?: (route: Matched) => string;
  props: (ctx: TfPageContext & { route: Matched }) => Props;
}): TfPageEntry {
  return page as unknown as TfPageEntry;
}
