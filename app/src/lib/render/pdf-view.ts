/** The PDF viewer's zoom (#2260): steps for − and +, and *Fit width*. */

/** The zoom steps, smallest first; 1 is the page at its printed size. */
export const ZOOMS = [0.25, 0.5, 0.67, 0.8, 1, 1.25, 1.5, 2, 3, 4] as const;

/** The pane's padding each side of the page, in CSS pixels (`.tf-pdf-pages` in `tf/css/code.css`). */
export const GUTTER = 18;

const MIN = ZOOMS[0];
const MAX = ZOOMS[ZOOMS.length - 1]!;

/** The next step from `scale` up (`1`) or down (`-1`); the end step when there is none further. */
export function zoomStep(scale: number, way: 1 | -1): number {
  if (way === 1) return ZOOMS.find((step) => step > scale + 1e-6) ?? MAX;
  return [...ZOOMS].reverse().find((step) => step < scale - 1e-6) ?? MIN;
}

/** The zoom at which a page `pageWidth` wide fills a pane `paneWidth` wide; 1 while the pane has no width yet. */
export function fitWidth(pageWidth: number, paneWidth: number): number {
  if (paneWidth <= 0 || pageWidth <= 0) return 1;
  return Math.min(MAX, Math.max(MIN, (paneWidth - 2 * GUTTER) / pageWidth));
}
