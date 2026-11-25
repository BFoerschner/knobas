/**
 * The XML view's component, held once loaded: it loads with the first document opened, never at start-up; after that a
 * document opened is drawn in the press's own task, as the API explorer's is (`oas-module.ts`).
 */
import { quietAndIdle } from "../../ahead";

type DocView = typeof import("./TfDocView.svelte");

let held: DocView | null = null;

/** The view's module: a promise the first time, the module itself once it is in. */
export function docViewModule(): DocView | Promise<DocView> {
  return held ?? import("./TfDocView.svelte").then((loaded) => (held = loaded));
}

const ahead = new Set<string>();

/**
 * `load`, once a session for `key`, ahead of the press that needs it: once the reader has been still and the window is
 * idle (`quietAndIdle`). A Docs page whose block offers *XML view*, *JSON view* or *Schema* loads that view's code so
 * (D1's checks, 2026-11-25): pressed first in a session, *XML view* reached its carets in 283 ms at CPU 6x at
 * 2560 x 1440 at 1.25, against 162 once the code was in. A load that fails is tried again by the next page that asks.
 * Never under jsdom, which draws nothing.
 */
export function loadAhead(key: string, load: () => unknown, later: () => Promise<void> = quietAndIdle): void {
  if (ahead.has(key)) return;
  if (later === quietAndIdle && typeof navigator !== "undefined" && /jsdom/i.test(navigator.userAgent)) return;
  ahead.add(key);
  void later()
    .then(load)
    .catch(() => ahead.delete(key));
}
