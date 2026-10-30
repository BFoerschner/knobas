/**
 * Split or unified (#2266, spec #2234 story 59): one choice for every diff in
 * the app, kept across a restart, and a window too narrow for split showing
 * unified whatever was chosen (story 60).
 *
 * The choice is this viewer's, in the webview's `localStorage`
 * (`knobas.diff.layout`), as the shell switch (`knobas.shell`) and the
 * pending line comments are: a toggle drawn in the diff's own toolbar, not a
 * Settings entry, so it is not a `knobas.setting` row (*Pages kept open*,
 * #2247, is one) and needs no command. The webview keeps `localStorage`
 * across a restart; a backup does not carry it. Storage missing, blocked or
 * holding rubbish costs the memory, never the toggle: the choice is unified
 * then.
 */
import { MediaQuery } from "svelte/reactivity";

import type { DiffLayout } from "./diff";

export const LAYOUT_KEY = "knobas.diff.layout";
/** Below this window width split shows unified: two code columns beside the file list need about this much. */
export const NARROW = "(max-width: 1100px)";

export interface DiffLayoutChoice {
  readonly chosen: DiffLayout;
  set(layout: DiffLayout): void;
  /** Tests only: back to unified, nothing stored. */
  reset(): void;
}

type Storage = Pick<globalThis.Storage, "getItem" | "setItem" | "removeItem">;

export function createDiffLayout(storage: () => Storage | null): DiffLayoutChoice {
  const read = (): DiffLayout => {
    try {
      return storage()?.getItem(LAYOUT_KEY) === "split" ? "split" : "unified";
    } catch {
      return "unified";
    }
  };
  let chosen = $state<DiffLayout>(read());
  return {
    get chosen() {
      return chosen;
    },
    set(layout) {
      chosen = layout;
      try {
        storage()?.setItem(LAYOUT_KEY, layout);
      } catch {
        // Kept for this run only.
      }
    },
    reset() {
      chosen = "unified";
      try {
        storage()?.removeItem(LAYOUT_KEY);
      } catch {
        // Nothing stored to clear.
      }
    },
  };
}

/** The app's one choice, over this viewer's `localStorage` where there is one. */
export const diffLayout: DiffLayoutChoice = createDiffLayout(() =>
  typeof window === "undefined" ? null : window.localStorage,
);

/** Whether the window is too narrow for split; never where there is no `matchMedia` (jsdom). */
export function windowNarrow(): { readonly current: boolean } {
  if (typeof window === "undefined" || typeof window.matchMedia !== "function") return { current: false };
  return new MediaQuery(NARROW, false);
}
