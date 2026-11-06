/**
 * **A row leaving under a pointer holds its place** (design system, F-138, F-064). After a mouse press removes a row, the page
 * draws it at its height, in `--muted` with nothing to press, and this says when to let it go: when the pointer leaves it, the
 * window, or a wheel or a key takes the reader on. The list closes up then, so a second press where the pointer rests lands on nothing.
 *
 * A press by key has `detail` 0 and closes at once, because focus moves on with it: {@link heldByPointer} tells them apart.
 */
import type { Attachment } from "svelte/attachments";

/** Whether this press came from a pointer, which is resting where the row was. */
export function heldByPointer(event: MouseEvent | undefined): boolean {
  return event !== undefined && event.detail > 0;
}

/** Listens until `release` should run, for the held row `held` finds. Returns what takes the listeners off. */
export function holdUnderPointer(held: () => Element | null, release: () => void): () => void {
  const moved = (event: PointerEvent) => {
    const box = held()?.getBoundingClientRect();
    if (box === undefined || event.clientX < box.left || event.clientX >= box.right || event.clientY < box.top || event.clientY >= box.bottom) release();
  };
  window.addEventListener("pointermove", moved);
  window.addEventListener("pointerdown", moved);
  window.addEventListener("wheel", release, { passive: true });
  window.addEventListener("keydown", release, true);
  document.documentElement.addEventListener("pointerleave", release);
  return () => {
    window.removeEventListener("pointermove", moved);
    window.removeEventListener("pointerdown", moved);
    window.removeEventListener("wheel", release);
    window.removeEventListener("keydown", release, true);
    document.documentElement.removeEventListener("pointerleave", release);
  };
}

/** A held row keeps its height through the CSSOM, as `--tf-row-held`: the app's CSP refuses `style:` in markup. */
export function heldHeight(height: number): Attachment<HTMLElement> {
  return (node) => {
    node.style.setProperty("--tf-row-held", `${height}px`);
  };
}
