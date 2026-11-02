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

import { keptValue, windowStorage, type KeptStorage } from "../kept-value";
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

export function createDiffLayout(storage: () => KeptStorage | null): DiffLayoutChoice {
  const kept = keptValue(storage, LAYOUT_KEY);
  let chosen = $state<DiffLayout>(kept.read() === "split" ? "split" : "unified");
  return {
    get chosen() {
      return chosen;
    },
    set(layout) {
      chosen = layout;
      kept.write(layout);
    },
    reset() {
      chosen = "unified";
      kept.clear();
    },
  };
}

/** The app's one choice, over this viewer's `localStorage` where there is one. */
export const diffLayout: DiffLayoutChoice = createDiffLayout(windowStorage);

/** Whether the window is too narrow for split; never where there is no `matchMedia` (jsdom). */
export function windowNarrow(): { readonly current: boolean } {
  if (typeof window === "undefined" || typeof window.matchMedia !== "function") return { current: false };
  return new MediaQuery(NARROW, false);
}
