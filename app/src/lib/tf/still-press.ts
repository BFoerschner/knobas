/**
 * **A press that closes a dialog holds for a moment** (design system, A press keeps your place). The dialog goes, and what
 * it covered is now under the pointer, and focus is back on what opened it. So a second press, a double click or a second
 * `Enter`, would shut the drawer behind, walk to a row or open the dialog again. Until the pointer really moves, or for
 * {@link HOLD_MS}, a person's press lands on nothing. Only a press from the person: a script's `click()` isn't trusted and
 * goes through, and so do the arrows, `Tab` and `Esc`.
 */
export const HOLD_MS = 700;

const PRESSES = ["pointerdown", "mousedown", "mouseup", "click"] as const;

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
  const release = () => {
    clearTimeout(timer);
    window.removeEventListener("pointermove", moved, true);
    for (const press of PRESSES) window.removeEventListener(press, stop, true);
  };
  const timer = setTimeout(release, HOLD_MS);
  window.addEventListener("pointermove", moved, true);
  for (const press of PRESSES) window.addEventListener(press, stop, true);
}
