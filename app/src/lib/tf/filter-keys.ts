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
 * **⌘F** is one ladder (`installFindKey`), and this is the one place that decides it. `Esc` in a box ⌘F went to, once it is empty,
 * gives focus back to what held it instead of falling through, because ⌘F then `Esc` is changing your mind, not leaving the page.
 */
import type { Attachment } from "svelte/attachments";
import { on } from "svelte/events";

import { escHolder } from "./escape";
import { layers } from "./layers.svelte";

export interface FilterKeysOptions {
  /** A find bar: closes on `Esc`. Say where focus returns in `close`, or let the keeper (`focus-keep.ts`) find it. */
  close?: () => void;
  /** A box beside the page rather than for it: *Find a view*, *Go to file*, *Filter changed files*. ⌘F takes one only where the page has no other. */
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

/** A page's find bar, which ⌘F opens ahead of any box. */
export interface FindBar {
  /** Whether this bar's page is the one shown, with something to find in. */
  here: () => boolean;
  open: () => void;
}

const bars = new Set<FindBar>();

/** Put a find bar on the ladder, until the call it returns. */
export function addFindBar(bar: FindBar): () => void {
  bars.add(bar);
  return () => bars.delete(bar);
}

/**
 * ⌘F, one ladder on every page (design system, the key table), so no page leaves it dead:
 *
 * 1. a dialog or a drawer owns the key, as it does the rest of the keyboard;
 * 2. the shown page's find bar (`addFindBar`);
 * 3. its find or filter box, with focus in it and its words selected, so pressed twice it only selects again;
 * 4. `launcher`: the launcher on the section's filter, empty where the section has none.
 */
export function installFindKey(launcher: () => void = () => {}): () => void {
  return on(window, "keydown", (event: KeyboardEvent) => {
    if (!(event.metaKey || event.ctrlKey) || event.altKey || event.shiftKey || event.key.toLowerCase() !== "f" || event.defaultPrevented) return;
    if (layers.open.some((layer) => layer.kind !== "panel")) return;
    event.preventDefault();
    const bar = [...bars].find((one) => one.here());
    if (bar !== undefined) {
      bar.open();
      return;
    }
    const box = shownBox();
    if (box === null) {
      launcher();
      return;
    }
    const from = document.activeElement;
    if (from !== box) {
      box.focus();
      cameFrom.set(box, from === document.body ? null : from);
    }
    box.select();
  });
}
