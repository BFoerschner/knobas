/**
 * **The Snooze choices float under the row's *Snooze…*** (F-154): the hand barely moves, and no row shifts. Not a `TfMenu`,
 * whose `Tab` closes it and whose `↓` is its own, because this holds a date field that needs both. It still acts as one:
 * `↑` and `↓` walk the buttons, a click elsewhere or a move to another address closes it, and `Tab` past either end
 * closes it and goes back to its opener. `Esc` and focus coming back are `inlineEditor`'s.
 */
import { untrack } from "svelte";
import type { Attachment } from "svelte/attachments";

import { inFloatHost, placeFloat } from "../floats";

/** Attach before `inlineEditor`, which moves focus in: an element that moves to the float host loses focus. */
export function snoozeFloat(opener: () => HTMLElement | null, close: () => void): Attachment<HTMLElement> {
  return (node) =>
    untrack(() => {
      const unhost = inFloatHost(node);
      const spot = () => {
        const from = opener();
        if (from === null) return;
        const box = from.getBoundingClientRect();
        // Its right edge under the button's, where there is room: stacked, the actions start at the left, and so does it.
        const align = box.right - node.offsetWidth < 8 ? "start" : "end";
        const at = placeFloat(box, { width: node.offsetWidth, height: node.offsetHeight }, { width: window.innerWidth, height: window.innerHeight }, align);
        node.style.left = `${at.left}px`;
        node.style.top = `${at.top}px`;
      };
      spot();
      const away = (event: MouseEvent) => {
        if (event.target instanceof Node && (node.contains(event.target) || opener()?.contains(event.target))) return;
        close();
      };
      const onkey = (event: KeyboardEvent) => {
        const buttons = [...node.querySelectorAll("button")];
        if (event.key === "ArrowDown" || event.key === "ArrowUp") {
          // A date field's own arrows change its date.
          const on = buttons.indexOf(document.activeElement as HTMLButtonElement);
          if (on === -1) return;
          event.preventDefault();
          event.stopPropagation();
          buttons[(on + (event.key === "ArrowDown" ? 1 : buttons.length - 1)) % buttons.length]?.focus();
        } else if (event.key === "Tab") {
          const controls = [...node.querySelectorAll<HTMLElement>("button, input")];
          if (document.activeElement !== (event.shiftKey ? controls[0] : controls.at(-1))) return;
          event.preventDefault();
          close();
        }
      };
      window.addEventListener("resize", spot);
      window.addEventListener("scroll", spot, true);
      window.addEventListener("mousedown", away, true);
      window.addEventListener("hashchange", close);
      node.addEventListener("keydown", onkey);
      return () => {
        window.removeEventListener("resize", spot);
        window.removeEventListener("scroll", spot, true);
        window.removeEventListener("mousedown", away, true);
        window.removeEventListener("hashchange", close);
        node.removeEventListener("keydown", onkey);
        unhost();
      };
    });
}

/** Where a pick was made, kept under the pointer: the float's own box, set through the CSSOM, as the app's CSP refuses `style:`. */
export function spentAt(box: { left: number; top: number; width: number; height: number }): Attachment<HTMLElement> {
  return (node) => {
    const unhost = inFloatHost(node);
    node.style.left = `${box.left}px`;
    node.style.top = `${box.top}px`;
    node.style.width = `${box.width}px`;
    node.style.height = `${box.height}px`;
    return unhost;
  };
}
