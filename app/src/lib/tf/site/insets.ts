/**
 * Where a site's view sits (decisions §14): the distance of its rectangle from the window's four edges, in the
 * window's logical pixels. The page measures the rectangle (CSS pixels) and multiplies by the window's zoom, since the
 * webview's own zoom makes a CSS pixel `level` logical pixels; each edge is rounded on its own. Rust keeps the insets
 * and sets the view's bounds from the window's size minus them, again on every resize, with no trip through the page,
 * so the page sends them only when they change.
 */
import type { SiteInsets } from "../../ipc/site";
import { zoom } from "../zoom.svelte";

/** The site bar's height, which stands between the main area's top and the rectangle. */
export const SITE_BAR_PX = 40;

/** The insets of `rect` in a window `width` by `height` (CSS pixels), at zoom `level`. */
export function insetsOf(rect: Pick<DOMRect, "top" | "left" | "right" | "bottom">, width: number, height: number, level: number): SiteInsets {
  const at = (value: number) => Math.max(0, Math.round(value * level));
  return { top: at(rect.top), left: at(rect.left), right: at(width - rect.right), bottom: at(height - rect.bottom) };
}

/** Whether two insets are the same. */
export function sameInsets(a: SiteInsets | null, b: SiteInsets): boolean {
  return a !== null && a.top === b.top && a.left === b.left && a.right === b.right && a.bottom === b.bottom;
}

/** The insets of `el` now. */
export function measure(el: Element, level: number = zoom.level): SiteInsets {
  return insetsOf(el.getBoundingClientRect(), window.innerWidth, window.innerHeight, level);
}

/**
 * The rectangle a site page will draw, before it is drawn: the shell's main area less the bar. What `openWeb` sends
 * with `site_open` in the press's own task, so making the view starts at once; the page sends the measured insets
 * when it shows the view.
 */
export function siteInsetsGuess(level: number = zoom.level): SiteInsets {
  const main = document.querySelector(".tf-main");
  if (main === null) return { top: 93, left: 0, right: 0, bottom: 28 };
  const rect = main.getBoundingClientRect();
  return insetsOf({ top: rect.top + SITE_BAR_PX, left: rect.left, right: rect.right, bottom: rect.bottom }, window.innerWidth, window.innerHeight, level);
}
