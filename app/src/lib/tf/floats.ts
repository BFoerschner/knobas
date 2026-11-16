/**
 * What floats draws in one host at the shell's root (design system, *Layers*), never inside a page, so no page's
 * `overflow`, sticky band or stacking context can reach it.
 *
 * `inFloatHost` is an attachment: `{@attach inFloatHost}` moves the element into the host, which `TfShell` draws
 * inside `.tf`, so the tokens and Svelte's delegated events still hold. A component drawn with no shell (a test, the
 * specimen) keeps it where it stands. The element has to be its component's one root node, because Svelte removes a
 * run of root nodes by walking their siblings. Svelte also leaves a nested block's nodes to whatever holds them, and
 * the host doesn't hold them for it: when a whole page goes, what floated from it goes here.
 */
export function inFloatHost(node: HTMLElement): () => void {
  document.querySelector(".tf-floats")?.append(node);
  return () => node.remove();
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

/**
 * Where a float standing at a point goes (a right-click's menu, 2026-11-16): its top left corner at the point, or its
 * right edge there where the window has no room on the right, and over the point where there is more room above. Kept
 * inside the window by `EDGE`; taller than the room, it scrolls. In CSS pixels, so a scale or a zoom changes nothing.
 */
export function placeAt(point: { x: number; y: number }, size: Size, view: Size): FloatSpot {
  const below = view.height - point.y - EDGE;
  const above = point.y - EDGE;
  const under = size.height <= below || below >= above;
  const room = Math.max(0, under ? below : above);
  const top = under ? point.y : point.y - Math.min(size.height, room);
  const wanted = point.x + size.width <= view.width - EDGE ? point.x : point.x - size.width;
  const placed = Math.max(EDGE, top);
  return { left: Math.max(EDGE, Math.min(wanted, view.width - EDGE - size.width)), top: placed, maxHeight: under ? Math.min(room, view.height - EDGE - placed) : room };
}

/**
 * Where a submenu goes: beside its item, overlapping its menu by the menu's padding, on the right or on the left where
 * the right has no room, its top level with the item's and pushed up to stay inside the window.
 */
export function placeBeside(item: Box, size: Size, view: Size): FloatSpot {
  const PAD = 4;
  const right = item.right - PAD;
  const left = right + size.width <= view.width - EDGE ? right : item.left + PAD - size.width;
  const room = Math.max(0, view.height - 2 * EDGE);
  const top = Math.max(EDGE, Math.min(item.top - PAD, view.height - EDGE - Math.min(size.height, room)));
  return { left: Math.max(EDGE, Math.min(left, view.width - EDGE - size.width)), top, maxHeight: room };
}
