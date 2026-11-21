/**
 * Which highlighter colours code (the highlighter re-ruling, 2026-11-21, section 1): Pierre Highlights, the default,
 * TanStack Highlight or Shiki. Kept in the webview's `localStorage` (`knobas.highlighter`), absent for Pierre, and read
 * as this module loads (`main.ts`), so the first file opened asks the chosen one. This computer's, like the fonts and
 * the themes: a backup does not carry it.
 *
 * A choice recolours what is open in place: the port answers every job it holds again (`rehighlight`). An engine whose
 * import failed this session is `failed` here; *Try again* clears that and imports it again.
 */
import { SvelteSet } from "svelte/reactivity";

import { DEFAULT_ENGINE, engineOf, type EngineId } from "../render/engines";
import { renderPort, useEngineChoice, type RenderPort } from "../render/port";
import { keptValue, windowStorage, type KeptStorage } from "./kept-value";

export const HIGHLIGHTER_KEY = "knobas.highlighter";

export interface HighlighterChoice {
  readonly chosen: EngineId;
  /** The engines that could not be loaded this session. */
  readonly failed: ReadonlySet<EngineId>;
  choose(engine: EngineId): void;
  /** Clears an engine's failure and loads it again, colouring what is open. */
  retry(engine: EngineId): void;
  /** Heard from the worker: an engine could not be loaded. */
  heardFailed(engine: EngineId): void;
  /** Tests only: Pierre, nothing kept, nothing failed. */
  reset(): void;
}

export function createHighlighter(storage: () => KeptStorage | null, port: () => RenderPort | null): HighlighterChoice {
  const kept = keptValue(storage, HIGHLIGHTER_KEY);
  let chosen = $state<EngineId>(engineOf(kept.read()));
  const failed = new SvelteSet<EngineId>();
  return {
    get chosen() {
      return chosen;
    },
    failed,
    choose(engine) {
      if (engine === chosen) return;
      chosen = engine;
      if (engine === DEFAULT_ENGINE) kept.clear();
      else kept.write(engine);
      port()?.rehighlight?.();
    },
    retry(engine) {
      failed.delete(engine);
      port()?.warm?.(engine);
      port()?.rehighlight?.();
    },
    heardFailed(engine) {
      failed.add(engine);
    },
    reset() {
      chosen = DEFAULT_ENGINE;
      kept.clear();
      failed.clear();
    },
  };
}

/** The window's choice; the app's port reads it for every job. */
export const highlighter: HighlighterChoice = createHighlighter(windowStorage, () => (typeof Worker === "undefined" ? null : renderPort()));
useEngineChoice({ engine: () => highlighter.chosen, failed: (engine) => highlighter.heardFailed(engine) });

// The worker and the chosen engine start while the app is idle after boot, as the code face loads (`shell/faces.ts`):
// the first file of a session pays neither. Nothing else is warmed.
if (typeof window !== "undefined" && typeof Worker !== "undefined") {
  const idle = (run: () => void) => ("requestIdleCallback" in window ? window.requestIdleCallback(run, { timeout: 2000 }) : setTimeout(run, 500));
  idle(() => renderPort().warm?.());
}
