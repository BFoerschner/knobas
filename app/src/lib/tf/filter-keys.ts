/**
 * **`Esc` in a box that narrows or finds** (design system, the key table), one rule for all of them:
 *
 * - a filter with text clears, and focus stays in the box;
 * - an empty filter falls through to the next thing `Esc` does (`escape.ts`), so one press leaves it;
 * - a find bar, given `close`, closes on the first press with its words in it, and returns focus.
 *
 * `{@attach filterKeys()}` on the input is all a filter needs: it clears through the input's own `input` event, so
 * `bind:value` and `oninput` hear it as they would hear typing. It marks the box `data-filter`, which is how the shell's
 * ladder knows an empty one falls through. It answers inside a dialog too, where the shell's ladder never hears a key.
 *
 * It uses `svelte/events`, so the input's own `onkeydown` runs first and can take the key.
 */
import type { Attachment } from "svelte/attachments";
import { on } from "svelte/events";

export interface FilterKeysOptions {
  /** A find bar: closes on `Esc`. Say where focus returns in `close`, or let the keeper (`focus-keep.ts`) find it. */
  close?: () => void;
}

/** What a key does in `box`: `true` when `Esc` was answered here. */
function answer(box: HTMLInputElement, options: FilterKeysOptions): boolean {
  if (options.close !== undefined) {
    options.close();
    return true;
  }
  if (box.value === "") return false;
  box.value = "";
  box.dispatchEvent(new Event("input", { bubbles: true }));
  return true;
}

export function filterKeys(options: FilterKeysOptions = {}): Attachment<HTMLInputElement> {
  return (box) => {
    box.setAttribute("data-filter", "");
    return on(box, "keydown", (event: KeyboardEvent) => {
      if (event.key !== "Escape" || event.defaultPrevented || event.isComposing) return;
      if (!answer(box, options)) return;
      event.preventDefault();
      event.stopPropagation();
    });
  };
}
