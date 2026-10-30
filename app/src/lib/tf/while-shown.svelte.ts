/**
 * A hidden page pauses (#2249, spec #2234 step 3): a page that keep-alive
 * holds but does not show does not work its sources. Each of these runs its
 * work only while `shown()` answers true and runs it once when the page is
 * shown again, so what it would have read while hidden is read on the return.
 *
 * `shown` is the page's `visible` flag, or `pageVisible()` inside a page.
 * Call them while the component initialises: each is an effect.
 */
import { untrack } from "svelte";

/** Run `run` (untracked) each time `shown()` turns from false to true; never on the first. */
export function onShownAgain(shown: () => boolean, run: () => void): void {
  let was = untrack(shown);
  $effect(() => {
    const now = shown();
    if (now && !was) untrack(run);
    was = now;
  });
}

/** Run `run` when the window gets focus while shown, and once on being shown again. */
export function onFocusWhileShown(shown: () => boolean, run: () => void): void {
  $effect(() => {
    if (!shown()) return;
    const again = () => untrack(run);
    window.addEventListener("focus", again);
    return () => window.removeEventListener("focus", again);
  });
  onShownAgain(shown, run);
}

/** Run `run` every `ms` while shown, and once on being shown again. */
export function everyWhileShown(shown: () => boolean, ms: number, run: () => void): void {
  $effect(() => {
    if (!shown()) return;
    const tick = setInterval(() => untrack(run), ms);
    return () => clearInterval(tick);
  });
  onShownAgain(shown, run);
}

/**
 * Keep the scroll of what `anchor()` holds across a hide (#2248). The shell's
 * one scroller (`.tf-main`) is shared by every page and tab, and an element
 * inside a `display: none` subtree loses its own scroll, so each is noted as
 * the page or tab is hidden (before the DOM changes) and set back once it is
 * shown again (after it is drawn).
 */
export function keepScroll(anchor: () => HTMLElement | undefined, shown: () => boolean): void {
  let saved: [Element, number, number][] | null = null;
  $effect.pre(() => {
    if (!shown()) return;
    // Runs as `shown` turns false, before the hide is drawn.
    return () => {
      const at = untrack(anchor);
      if (at === undefined) return;
      const scrolled: [Element, number, number][] = [];
      const main = at.closest(".tf-main");
      if (main !== null) scrolled.push([main, main.scrollTop, main.scrollLeft]);
      for (const el of at.querySelectorAll("*")) {
        if (el.scrollTop !== 0 || el.scrollLeft !== 0) scrolled.push([el, el.scrollTop, el.scrollLeft]);
      }
      saved = scrolled;
    };
  });
  $effect(() => {
    if (!shown() || saved === null) return;
    for (const [el, top, left] of saved) {
      el.scrollTop = top;
      el.scrollLeft = left;
    }
    saved = null;
  });
}
