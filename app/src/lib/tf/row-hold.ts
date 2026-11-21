/**
 * **A row leaving under a pointer holds its place** (design system, F-138, F-064, F-159). After a mouse press removes a row, the
 * page draws it at its height, in `--muted` with nothing to press, and this says when to let it go: when the pointer leaves what
 * `held` finds, the window, or a wheel or a key takes the reader on. A second press where the pointer rests lands on nothing.
 *
 * The Inbox and Day pass their list, so someone working down it sees nothing move until they leave it: several rows hold, each at
 * its index in what was drawn ({@link withHolds}), and they close together. *Undo* is outside the list, on the toast, so a list's
 * hold also outlasts the pointer leaving while a toast offers it, and a press elsewhere closes the rows only once it has landed
 * (F-163). Its box runs to the window's right edge, since the gutter beside a row's actions is no way out of it (F-164). Monitors
 * and the drawer pass the row, because walking those is rare.
 *
 * A press by key has `detail` 0 and closes at once, because focus moves on with it: {@link heldByPointer} tells them apart.
 */
import type { Attachment } from "svelte/attachments";

import { toasts } from "../shell/toasts.svelte";

/** Whether this press came from a pointer, which is resting where the row was. */
export function heldByPointer(event: MouseEvent | undefined): boolean {
  return event !== undefined && event.detail > 0;
}

/** How often a list's hold asks again whether the toast has gone, in ms, with the pointer out of the list. */
const TOAST_LOOK_MS = 200;

const undoOnToast = () => toasts.items.some((toast) => toast.action?.label === "Undo");

/**
 * Listens until `release` should run, for the held row or list `held` finds. Returns what takes the listeners off. `list` is a
 * list's hold (see above): a row's lets go as the pointer leaves or presses elsewhere.
 */
export function holdUnderPointer(held: () => Pick<Element, "getBoundingClientRect"> | null, release: () => void, list = false): () => void {
  let out = false;
  let pressedOut = false;
  let landing: ReturnType<typeof setTimeout> | undefined;
  const outside = (event: PointerEvent) => {
    const box = held()?.getBoundingClientRect();
    return box === undefined || event.clientX < box.left || (!list && event.clientX >= box.right) || event.clientY < box.top || event.clientY >= box.bottom;
  };
  const askToast = () => {
    if (out && !(list && undoOnToast())) release();
  };
  const moved = (event: PointerEvent) => {
    out = outside(event);
    askToast();
  };
  const down = (event: PointerEvent) => {
    if (!list) return moved(event);
    pressedOut = out = outside(event);
  };
  // A timeout, so the click that follows has landed before the rows close. A press that dragged off to nothing has no click and closes too.
  const up = () => {
    if (pressedOut) landing = setTimeout(release);
    pressedOut = false;
  };
  const left = () => {
    out = true;
    askToast();
  };
  const looking = list ? setInterval(askToast, TOAST_LOOK_MS) : undefined;
  window.addEventListener("pointermove", moved);
  window.addEventListener("pointerdown", down);
  window.addEventListener("pointerup", up);
  window.addEventListener("wheel", release, { passive: true });
  window.addEventListener("keydown", release, true);
  document.documentElement.addEventListener("pointerleave", left);
  return () => {
    clearInterval(looking);
    clearTimeout(landing);
    window.removeEventListener("pointermove", moved);
    window.removeEventListener("pointerdown", down);
    window.removeEventListener("pointerup", up);
    window.removeEventListener("wheel", release);
    window.removeEventListener("keydown", release, true);
    document.documentElement.removeEventListener("pointerleave", left);
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
