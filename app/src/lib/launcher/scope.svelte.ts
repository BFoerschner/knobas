/**
 * *Titles* or *Everything* (spec #2419 story 50, #2430): whether the launcher
 * finds only what a row shows, its title or key, or searches bodies too. One
 * choice for the app, kept across closing the launcher and across a restart.
 *
 * The reader's, in the webview's `localStorage` (`knobas.launcher.scope`), as
 * the diff layout (`tf/diff/diff-layout.svelte.ts`) is: a toggle drawn at the
 * right of the box, not a Settings entry, so it needs no command and no
 * `knobas.setting` row. Storage missing, blocked or holding rubbish costs the
 * memory, never the toggle: the choice is Titles then.
 *
 * The launcher sends it as the query's `titles` flag and the engine narrows;
 * the frontend never filters an answer.
 */
import { keptValue, windowStorage, type KeptStorage } from "../tf/kept-value";

export const SCOPE_KEY = "knobas.launcher.scope";

export interface ScopeChoice {
  /** `true` for Titles, `false` for Everything. */
  readonly titles: boolean;
  toggle(): void;
  /** Tests only: back to Titles, nothing stored. */
  reset(): void;
}

export function createScope(storage: () => KeptStorage | null): ScopeChoice {
  const kept = keptValue(storage, SCOPE_KEY);
  let titles = $state(kept.read() !== "everything");
  return {
    get titles() {
      return titles;
    },
    toggle() {
      titles = !titles;
      kept.write(titles ? "titles" : "everything");
    },
    reset() {
      titles = true;
      kept.clear();
    },
  };
}

/** The app's one choice, over this viewer's `localStorage` where there is one. */
export const launcherScope: ScopeChoice = createScope(windowStorage);
