/**
 * What floats draws in one host at the shell's root (design system, *Layers*), never inside a page, so no page's
 * `overflow`, sticky band or stacking context can reach it.
 *
 * `inFloatHost` is an attachment: `{@attach inFloatHost}` moves the element into the host, which `TfShell` draws
 * inside `.tf`, so the tokens and Svelte's delegated events still hold. A component drawn with no shell (a test, the
 * specimen) keeps it where it stands. The element has to be its component's one root node, because Svelte removes a
 * run of root nodes by walking their siblings.
 */
export function inFloatHost(node: HTMLElement): void {
  document.querySelector(".tf-floats")?.append(node);
}

interface Box {
  top: number;
  bottom: number;
  left: number;
  right: number;
}

interface Size {
  width: number;
  height: number;
}

export interface FloatSpot {
  left: number;
  top: number;
  /** The room on the side it opened to: taller than this, it scrolls. */
  maxHeight: number;
}

/** What it leaves between itself and the window's edge. */
const EDGE = 8;
/** What it leaves between itself and its opener. */
const GAP = 4;

/**
 * Where a float goes: under its opener, or above it when it fits only there or there is more room there, with an edge
 * of the opener (`align`) as its own and pushed back inside the window. `size` is its whole height, however much of it
 * is showing.
 */
export function placeFloat(opener: Box, size: Size, view: Size, align: "start" | "end"): FloatSpot {
  const below = view.height - opener.bottom - GAP - EDGE;
  const above = opener.top - GAP - EDGE;
  const under = size.height <= below || below >= above;
  const room = Math.max(0, under ? below : above);
  const top = under ? opener.bottom + GAP : opener.top - GAP - Math.min(size.height, room);
  const wanted = align === "start" ? opener.left : opener.right - size.width;
  return { left: Math.max(EDGE, Math.min(wanted, view.width - EDGE - size.width)), top, maxHeight: room };
}
