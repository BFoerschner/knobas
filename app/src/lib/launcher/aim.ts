/**
 * Whether the pointer really moved (F-628). A row taking the selection on hover must not take it because the row
 * appeared under a still pointer: type `PAY-2`, press Enter, and the row the answer drew under the pointer opened.
 * `mouseenter` fires for that, and WebKit's move after a layout reports a `mousemove` too, so a move counts only when
 * the pointer's place on the screen changed. The first move ever seen has nothing to compare, and only sets the place.
 */
let x: number | null = null;
let y: number | null = null;
let moved = false;

function note(event: PointerEvent): void {
  moved = x !== null && (event.screenX !== x || event.screenY !== y);
  x = event.screenX;
  y = event.screenY;
}

/** Watch the pointer for as long as the launcher is mounted; the capture runs before a row's own handler. Returns the stop. */
export function watchPointer(): () => void {
  window.addEventListener("pointermove", note, { capture: true, passive: true });
  return () => {
    window.removeEventListener("pointermove", note, { capture: true });
    x = null;
    y = null;
    moved = false;
  };
}

/** A row's `onpointermove`: `take` runs only for a move that went somewhere. */
export function aimed(take: () => void): () => void {
  return () => {
    if (moved) take();
  };
}
