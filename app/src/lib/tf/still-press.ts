/**
 * **A press that closes a dialog holds for a moment** (design system, A press keeps your place). The dialog goes, and what
 * it covered is now under the pointer, and focus is back on what opened it. So a second press, a double click or a second
 * `Enter`, would shut the drawer behind, walk to a row or open the dialog again. Until the pointer really moves, or for
 * {@link HOLD_MS}, a person's press lands on nothing. Only a press from the person: a script's `click()` isn't trusted and
 * goes through. Any key but `Enter` or `Space` ends the hold, as it ends `press-guard.ts`'s wait, because `Tab` then `Enter`
 * was aimed. The guard's wait runs from the press, and this one from the close, which can come long after a slow write.
 * `TfDialog` does this for every dialog a press closes; a page calls it only for what isn't a dialog closing: an inline
 * editor's save, a toast's *Undo*.
 */
export const HOLD_MS = 700;

const PRESSES = ["pointerdown", "mousedown", "mouseup", "click"] as const;
const ALONE = new Set(["Shift", "Control", "Alt", "Meta", "CapsLock", "Fn"]);

export function holdStillPress(): void {
  const stop = (event: Event) => {
    if (!event.isTrusted) return;
    event.preventDefault();
    event.stopPropagation();
  };
  // A page redrawn under a still pointer sends a move of nothing.
  const moved = (event: PointerEvent) => {
    if (event.movementX !== 0 || event.movementY !== 0) release();
  };
  const aimed = (event: KeyboardEvent) => {
    if (event.key !== "Enter" && event.key !== " " && !ALONE.has(event.key)) release();
  };
  const release = () => {
    clearTimeout(timer);
    window.removeEventListener("pointermove", moved, true);
    window.removeEventListener("keydown", aimed, true);
    for (const press of PRESSES) window.removeEventListener(press, stop, true);
  };
  const timer = setTimeout(release, HOLD_MS);
  window.addEventListener("pointermove", moved, true);
  window.addEventListener("keydown", aimed, true);
  for (const press of PRESSES) window.addEventListener(press, stop, true);
}
