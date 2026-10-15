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
import type { TfRoute } from "./route";

/** What the shell hands every page. */
export interface TfPageContext {
  route: TfRoute;
  /** Whether the database answers yet: a page's reads wait for it. */
  ready: boolean;
  /** The job the reader was last in. */
  job: string | null;
  /** The ticket key the timer runs on, or `null`. */
  timedTicket: string | null;
  /** Switch back to the rooms UI. */
  onrooms: () => void;
  timer: Pick<Timer, "current" | "elapsed">;
  layers: Layers;
  /** The timer verb, which stops a running timer into the worklog draft. */
  ontimer: (() => void) | undefined;
  /** Start the timer on a ticket (#816's *Start timer*). */
  onstarttimer: (entityId: string, title: string) => void;
  /** The job page's reads. */
  jobPorts: JobPorts;
}

/** One registered page, its types erased once {@link definePage} checked them. */
export interface TfPageEntry {
  /** Whether this page draws `route`. */
  matches: (route: TfRoute) => boolean;
  component: Component<Record<string, unknown>>;
  /** The component's props, read off the shell's context for a matched route. */
  props: (ctx: TfPageContext) => Record<string, unknown>;
}

/**
 * A page whose props are checked against its component's. `matches` narrows
 * the route, so `props` reads the fields of the route it matched.
 */
export function definePage<Props extends object, Matched extends TfRoute>(page: {
  matches: (route: TfRoute) => route is Matched;
  component: Component<Props>;
  props: (ctx: TfPageContext & { route: Matched }) => Props;
}): TfPageEntry {
  return page as unknown as TfPageEntry;
}
