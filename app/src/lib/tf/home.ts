import type { TfRoute } from "./route";

/**
 * Where the app opens (#881, spec #2227; 2026-11-20,
 * `docs/decisions/2026-11-20-knobas-opens-on-all-tickets.md`): home. While a
 * job is chosen (`job/chosen.svelte.ts`) home is the bare `#/job`, which draws
 * that job's ticket page (spec #2396) and keeps its address, under Tickets in
 * the nav; `Esc` does nothing there. With none chosen home is *All tickets*,
 * `#/tickets`: the page a person picks a job from, drawn in the first frame
 * rather than a page that only points at it (Björn: "It looks kind of broken
 * when it starts in the 'no job has been selected'"). An address the window
 * was opened on is kept.
 */
export const JOB_HOME = "#/job";
export const TICKETS_HOME = "#/tickets";

/** Home for this chosen job (an entity id), or for none. */
export function homeOf(chosen: string | null): string {
  return chosen === null ? TICKETS_HOME : JOB_HOME;
}

/**
 * Replace a blank address with home for `chosen`, leaving nothing behind it
 * in history. Run before the shell mounts, so its first frame is home's.
 */
export function homeIfBlank(place: { hash: string; replace: (url: string) => void }, chosen: string | null): void {
  if (place.hash === "" || place.hash === "#" || place.hash === "#/") place.replace(homeOf(chosen));
}

/**
 * Where a bare `#/job` goes when no job is chosen: *All tickets*, in its
 * place (the address typed or kept in history, or a chosen job knobas no
 * longer holds, which the ticket page forgets). `null` where the address
 * stays.
 */
export function homeInstead(route: TfRoute, chosen: string | null): string | null {
  return chosen === null && route.view === "job" && route.key === null ? TICKETS_HOME : null;
}
