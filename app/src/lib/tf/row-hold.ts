/**
 * **A row leaving under a pointer holds its place** (design system, F-138, F-064, F-159). After a mouse press removes a row, the
 * page draws it at its height, in `--muted` with nothing to press, and this says when to let it go: when the pointer leaves what
 * `held` finds, the window, or a wheel or a key takes the reader on. A second press where the pointer rests lands on nothing.
 *
 * The Inbox and Day pass their list, so someone working down it sees nothing move until they leave it: several rows hold, each at
 * its index in what was drawn ({@link withHolds}), and they close together. Monitors and the drawer pass the row, because walking
 * those is rare and *Undo* is on the toast.
 *
 * A press by key has `detail` 0 and closes at once, because focus moves on with it: {@link heldByPointer} tells them apart.
 */
import type { Attachment } from "svelte/attachments";

/** Whether this press came from a pointer, which is resting where the row was. */
export function heldByPointer(event: MouseEvent | undefined): boolean {
  return event !== undefined && event.detail > 0;
}

/** Listens until `release` should run, for the held row or list `held` finds. Returns what takes the listeners off. */
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

/**
 * `list` with each hold back in its place. `at` is its index in what was drawn, so it counts the holds above it, which is why
 * they go in rising order. A row a failed answer brings back is left out of `holds`, and takes its own place in `list`.
 */
export function withHolds<T>(list: readonly T[], holds: readonly { at: number; entry: T }[]): readonly T[] {
  if (holds.length === 0) return list;
  const drawn = [...list];
  for (const hold of [...holds].sort((a, b) => a.at - b.at)) drawn.splice(hold.at, 0, hold.entry);
  return drawn;
}

/** A held row keeps its height through the CSSOM, as `--tf-row-held`: the app's CSP refuses `style:` in markup. */
export function heldHeight(height: number): Attachment<HTMLElement> {
  return (node) => {
    node.style.setProperty("--tf-row-held", `${height}px`);
  };
}
