/**
 * How an image changed on both sides is drawn in a rendered diff's stage: side by side, swiped or onion-skinned
 * (`docs/decisions/2026-11-20-a-comparison-is-drawn-rendered-and-may-end-at-the-working-tree.md`, ruling 3). One
 * choice app-wide for the visit, so the second image opens the way the first was left; not kept across a restart.
 */
import type { ImageMode } from "./rendered";

export interface ImageModeChoice {
  readonly current: ImageMode;
  set(mode: ImageMode): void;
}

export function createImageMode(start: ImageMode = "sides"): ImageModeChoice {
  let current = $state<ImageMode>(start);
  return {
    get current() {
      return current;
    },
    set(mode) {
      current = mode;
    },
  };
}

/** The app's one choice. */
export const imageMode: ImageModeChoice = createImageMode();
