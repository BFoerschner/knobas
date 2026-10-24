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
 */
import type { Component } from "svelte";

import type { Timer } from "../shell/timer.svelte";
import type { JobPorts } from "./job/ports";
import type { Layers } from "./layers.svelte";
import type { Theme } from "./theme.svelte";
import type { TfRoute } from "./route";

/** What the shell hands every page. */
export interface TfPageContext {
  route: TfRoute;
  /** Whether the database answers yet: a page's reads wait for it. */
  ready: boolean;
  /** The job the reader was last in, as its key. */
  job: string | null;
  /** The address that job was opened at (#1525): its key, or its entity id when two sources hold the key. */
  jobAt: string | null;
  /** The ticket key the timer runs on, or `null`. */
  timedTicket: string | null;
  /** The Job tab's job (the last opened, else the timer's) with its entity id, or `null`. */
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
  props: (ctx: TfPageContext & { route: Matched }) => Props;
}): TfPageEntry {
  return page as unknown as TfPageEntry;
}
