/**
 * Which hand moved last (`<html data-input>`) and whether the transient keyboard focus ring is drawn
 * (`<html data-focus-ring>`):
 *
 * - `data-input`: `"pointer"` after a press, `"keys"` after a key that isn't a bare modifier.
 * - `data-focus-ring`: `"on"` (solid `--focus` ring) for `FOCUS_HOLD_MS` (~1.2 s) when focus lands via keyboard or
 *   programmatic navigation (or when `Tab` is pressed), then `"fade"` for `FOCUS_FADE_MS` (~0.3 s) while CSS fades the
 *   ring out smoothly, then removed (~1.5 s total). Under `prefers-reduced-motion: reduce`, it clears cleanly right after
 *   the hold without entering `"fade"`.
 * - Pointer presses clear any active ring immediately and keep `focusin` silent so mouse clicks never draw a blue ring.
 * - The window coming back (Alt+Tab, #2528) gives the focused element its `focusin` again, in the task of the window's
 *   own `focus`: that is no focus move, and draws no ring.
 *
 * Capture listeners on `window`, so a handler that stops an event can't hide it. Attributes are written only when they
 * change, so a run of keys or presses costs one write.
 */
const MODIFIERS = new Set(["Shift", "Control", "Alt", "Meta", "CapsLock", "Fn", "FnLock", "Hyper", "Super", "OS", "AltGraph"]);
const NAV_KEYS = new Set(["ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight", "Home", "End", "PageUp", "PageDown"]);
const COMPOSITE_ROLES = new Set(["listbox", "tree", "grid", "menu", "menubar", "tablist", "radiogroup"]);

export const FOCUS_HOLD_MS = 1200;
export const FOCUS_FADE_MS = 300;

export type InputHand = "pointer" | "keys";
export type FocusRingState = "on" | "fade";

/** The hand a key event counts for, or `null` when it is a bare modifier and changes nothing. */
export function handOfKey(key: string): InputHand | null {
  return MODIFIERS.has(key) ? null : "keys";
}

function prefersReducedMotion(): boolean {
  return typeof window.matchMedia === "function" && window.matchMedia("(prefers-reduced-motion: reduce)").matches;
}

/** Whether `el` is a non-editable composite widget (e.g. a listbox rail) that moves an internal cursor without firing `focusin`. */
function isCompositeFocus(el: Element | null): boolean {
  if (el === null) return false;
  if (el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement) return false;
  if (el instanceof HTMLElement && el.isContentEditable) return false;
  return COMPOSITE_ROLES.has(el.getAttribute("role") ?? "") || el.hasAttribute("aria-activedescendant");
}

export function installInputModality(root: HTMLElement = document.documentElement): () => void {
  let hand: InputHand | null = null;
  let holdTimer: ReturnType<typeof setTimeout> | null = null;
  let fadeTimer: ReturnType<typeof setTimeout> | null = null;
  /** The window's `focus` came in this task: a `focusin` now is the focus it had, given back. */
  let returning = false;
  let returnTimer: ReturnType<typeof setTimeout> | null = null;

  const setHand = (next: InputHand | null): void => {
    if (next === null) return;
    hand = next;
    if (root.dataset.input !== next) root.dataset.input = next;
  };

  const setRing = (state: FocusRingState | null): void => {
    if (state === null) {
      if ("focusRing" in root.dataset) delete root.dataset.focusRing;
    } else if (root.dataset.focusRing !== state) {
      root.dataset.focusRing = state;
    }
  };

  const clearTimers = (): void => {
    if (holdTimer !== null) {
      clearTimeout(holdTimer);
      holdTimer = null;
    }
    if (fadeTimer !== null) {
      clearTimeout(fadeTimer);
      fadeTimer = null;
    }
  };

  const hideRing = (): void => {
    clearTimers();
    setRing(null);
  };

  const showRing = (): void => {
    clearTimers();
    setRing("on");
    holdTimer = setTimeout(() => {
      holdTimer = null;
      if (prefersReducedMotion()) {
        setRing(null);
        return;
      }
      setRing("fade");
      fadeTimer = setTimeout(() => {
        fadeTimer = null;
        setRing(null);
      }, FOCUS_FADE_MS);
    }, FOCUS_HOLD_MS);
  };

  const onPointer = (): void => {
    setHand("pointer");
    hideRing();
  };

  const onKey = (event: KeyboardEvent): void => {
    const next = handOfKey(event.key);
    if (next === null) return;
    setHand(next);
    if (event.key === "Tab" || (NAV_KEYS.has(event.key) && isCompositeFocus(document.activeElement))) showRing();
  };

  const onWindowFocus = (event: FocusEvent): void => {
    // The window's own `focus`, not an element's passing through the capture phase.
    if (event.target instanceof Node) return;
    returning = true;
    if (returnTimer !== null) clearTimeout(returnTimer);
    returnTimer = setTimeout(() => {
      returnTimer = null;
      returning = false;
    }, 0);
  };

  const onFocusIn = (event: FocusEvent): void => {
    if (hand === "pointer" || returning) return;
    const target = event.target;
    if (target === null || target === window || target === document || target === document.body || target === document.documentElement) return;
    showRing();
  };

  const onFocusOut = (event: FocusEvent): void => {
    if (event.relatedTarget === null) hideRing();
  };

  window.addEventListener("pointerdown", onPointer, true);
  window.addEventListener("keydown", onKey, true);
  window.addEventListener("focusin", onFocusIn, true);
  window.addEventListener("focusout", onFocusOut, true);
  window.addEventListener("focus", onWindowFocus, true);
  return () => {
    clearTimers();
    if (returnTimer !== null) clearTimeout(returnTimer);
    window.removeEventListener("pointerdown", onPointer, true);
    window.removeEventListener("keydown", onKey, true);
    window.removeEventListener("focusin", onFocusIn, true);
    window.removeEventListener("focusout", onFocusOut, true);
    window.removeEventListener("focus", onWindowFocus, true);
    delete root.dataset.input;
    delete root.dataset.focusRing;
  };
}
