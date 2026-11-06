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
 *
 * **⌘F** goes to the shown page's box, with its words selected (`installFindKey`). `Esc` in a box ⌘F went to, once it is empty,
 * gives focus back to what held it instead of falling through, because ⌘F then `Esc` is changing your mind, not leaving the page.
 */
import type { Attachment } from "svelte/attachments";
import { on } from "svelte/events";

import { escHolder } from "./escape";
import { layers } from "./layers.svelte";

export interface FilterKeysOptions {
  /** A find bar: closes on `Esc`. Say where focus returns in `close`, or let the keeper (`focus-keep.ts`) find it. */
  close?: () => void;
  /** A box beside the page rather than for it: *Find a view*, *Go to file*. ⌘F takes one only where the page has no other. */
  aside?: boolean;
}

/** A box ⌘F went to, and what held focus then. A blur forgets it, so a box you tab into or press later falls through as usual. */
const cameFrom = new WeakMap<HTMLInputElement, Element | null>();

/** What a key does in `box`: `true` when `Esc` was answered here. */
function answer(box: HTMLInputElement, options: FilterKeysOptions): boolean {
  if (options.close !== undefined) {
    options.close();
    return true;
  }
  if (box.value === "") {
    if (!cameFrom.has(box)) return false;
    const from = cameFrom.get(box);
    (from instanceof HTMLElement && from.isConnected ? from : escHolder(box))?.focus({ preventScroll: true });
    return true;
  }
  box.value = "";
  box.dispatchEvent(new Event("input", { bubbles: true }));
  return true;
}

export function filterKeys(options: FilterKeysOptions = {}): Attachment<HTMLInputElement> {
  return (box) => {
    box.setAttribute("data-filter", options.aside === true ? "aside" : "");
    const off = on(box, "keydown", (event: KeyboardEvent) => {
      if (event.key !== "Escape" || event.defaultPrevented || event.isComposing) return;
      if (!answer(box, options)) return;
      event.preventDefault();
      event.stopPropagation();
    });
    const left = on(box, "blur", () => cameFrom.delete(box));
    return () => {
      off();
      left();
    };
  };
}

/** The box ⌘F goes to: the first one drawn that is the page's own, else the first drawn. A page a keep-alive holds hidden draws none. */
function shownBox(): HTMLInputElement | null {
  const boxes = [...document.querySelectorAll<HTMLInputElement>("input[data-filter]")].filter((box) => box.getClientRects().length > 0);
  return boxes.find((box) => box.dataset["filter"] !== "aside") ?? boxes[0] ?? null;
}

/**
 * ⌘F, one meaning on every page (design system, the key table): focus in the shown page's find or filter box, with its words
 * selected, so pressed twice it only selects again. A dialog or a drawer owns the key, as it does the rest of the keyboard. With
 * no box, `nobox` answers, and it does nothing where the page has no search to offer.
 *
 * A Docs page's own listener opens its find bar. Whichever hears the key first, the answer is the same: the bar's box is the page's only box, and an
 * event either one took is left alone.
 */
export function installFindKey(nobox: () => void = () => {}): () => void {
  return on(window, "keydown", (event: KeyboardEvent) => {
    if (!(event.metaKey || event.ctrlKey) || event.altKey || event.shiftKey || event.key.toLowerCase() !== "f" || event.defaultPrevented) return;
    if (layers.open.some((layer) => layer.kind !== "panel")) return;
    const box = shownBox();
    if (box === null) {
      nobox();
      return;
    }
    event.preventDefault();
    const from = document.activeElement;
    if (from !== box) {
      box.focus();
      cameFrom.set(box, from === document.body ? null : from);
    }
    box.select();
  });
}
