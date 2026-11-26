/**
 * Which hand moved last, on `<html data-input>`: `pointer` after a press, `keys` after a key that isn't a bare modifier.
 *
 * Chromium (WebView2) draws a select's focus ring after a mouse press, which it never does for a button, so the branch box
 * rang blue when Björn picked a branch with the mouse ("the blue outline is kinda weird when i select it somehow
 * sometimes", 2026-11-21). CSS can't tell the two apart, so this flag does, and only a select reads it
 * (`tokens.css`, design system, Motion and focus). A field still rings under a press: it holds the caret.
 *
 * Capture listeners on `window`, so a handler that stops a press or a key can't hide it. The attribute is written only
 * when it changes, so a run of keys or presses costs one write.
 */
const MODIFIERS = new Set(["Shift", "Control", "Alt", "Meta", "CapsLock", "Fn", "FnLock", "Hyper", "Super", "OS", "AltGraph"]);

export type InputHand = "pointer" | "keys";

/** The hand a key event counts for, or `null` when it is a bare modifier and changes nothing. */
export function handOfKey(key: string): InputHand | null {
  return MODIFIERS.has(key) ? null : "keys";
}

export function installInputModality(root: HTMLElement = document.documentElement): () => void {
  const set = (hand: InputHand | null): void => {
    if (hand !== null && root.dataset.input !== hand) root.dataset.input = hand;
  };
  const onPointer = (): void => set("pointer");
  const onKey = (event: KeyboardEvent): void => set(handOfKey(event.key));
  window.addEventListener("pointerdown", onPointer, true);
  window.addEventListener("keydown", onKey, true);
  return () => {
    window.removeEventListener("pointerdown", onPointer, true);
    window.removeEventListener("keydown", onKey, true);
    delete root.dataset.input;
  };
}
