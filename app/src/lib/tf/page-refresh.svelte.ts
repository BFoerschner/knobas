/**
 * What ⌘R presses on the page in front of the reader (#2156): the page's own
 * *Refresh* or *Sync now*, whichever it draws.
 *
 * The buttons register here while they are drawn: a detail's
 * (`TfDetailRefresh.svelte`, *Refresh* or its *Sync now* fallback) and a role
 * view's header *Sync now* (`TfSyncNow.svelte`). A press runs the newest
 * detail's, else the newest role view's, through the function the button's own
 * click runs, so ⌘R is the click and never a second implementation. A button
 * drawn disabled answers the press with nothing, as a click on it would. The
 * shell's keyboard (`TfShell.svelte`, `refreshKey` in `shell/keys.ts`) is the
 * only caller, so no page names the key.
 */

import { refreshKey } from "../shell/keys";

/** A drawn button ⌘R may press. */
export interface PagePress {
  /** A detail's button wins over the role view's header. */
  kind: "detail" | "role";
  /** What a click on the button runs; nothing while it is disabled. */
  press: () => void;
}

export interface PageRefresh {
  /** Register a drawn button; the answer takes it off. */
  add(press: PagePress): () => void;
  /** Press the page's button: the newest detail's, else the newest role view's; nothing when none is drawn. */
  press(): void;
  /** Tests only: forget every button without pressing it. */
  reset(): void;
}

export function createPageRefresh(): PageRefresh {
  let drawn: PagePress[] = [];
  return {
    add(press) {
      drawn = [...drawn, press];
      return () => {
        drawn = drawn.filter((candidate) => candidate !== press);
      };
    },
    press() {
      const newest = [...drawn].reverse();
      (newest.find((candidate) => candidate.kind === "detail") ?? newest[0])?.press();
    },
    reset() {
      drawn = [];
    },
  };
}

/** The window's one registry. */
export const pageRefresh = createPageRefresh();

/**
 * The shell's ⌘R (#2156). The webview's own reload is swallowed on every page,
 * with or without a button: a reload loses the page's state. The page's button
 * is pressed unless an overlay (the launcher, a dialog) owns the keyboard, or
 * the key is held down. Inside a text field too: a field has no ⌘R of its own.
 * The answer says whether the key was ⌘R.
 */
export function onRefreshKey(
  event: KeyboardEvent,
  overlayOpen: boolean,
  registry: PageRefresh = pageRefresh,
): boolean {
  if (!refreshKey(event)) return false;
  event.preventDefault();
  if (!overlayOpen && !event.repeat) registry.press();
  return true;
}
