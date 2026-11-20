/**
 * *Rendered* in a diff's band (ux26, `docs/decisions/2026-11-20-a-comparison-is-drawn-rendered-and-may-end-at-the-
 * working-tree.md` ruling 4): one toggle for every diff in the app, kept across a restart. On, every file a renderer can
 * draw on both sides opens rendered, and (with folder renderers in a comparison) every changed folder a folder renderer
 * takes shows what it made. It starts no program: only *Run* runs.
 *
 * The viewer's own, in the webview's `localStorage` (`knobas.diff-rendered`, `"1"` or `"0"`), as the layout is
 * (`diff-layout.svelte.ts`): a toggle drawn in the diff's own toolbar, not a Settings entry. Off until set; storage
 * missing, blocked or holding rubbish costs the memory, never the toggle.
 */
import { keptValue, windowStorage, type KeptStorage } from "../kept-value";

export const RENDERED_KEY = "knobas.diff-rendered";

export interface DiffRenderedChoice {
  readonly on: boolean;
  set(on: boolean): void;
  /** Tests only: back to off, nothing stored. */
  reset(): void;
}

export function createDiffRendered(storage: () => KeptStorage | null): DiffRenderedChoice {
  const kept = keptValue(storage, RENDERED_KEY);
  let on = $state(kept.read() === "1");
  return {
    get on() {
      return on;
    },
    set(next) {
      on = next;
      kept.write(next ? "1" : "0");
    },
    reset() {
      on = false;
      kept.clear();
    },
  };
}

/** The app's one choice, over this viewer's `localStorage` where there is one. */
export const diffRendered: DiffRenderedChoice = createDiffRendered(windowStorage);
