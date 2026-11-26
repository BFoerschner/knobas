/**
 * The explorers' pane (ux31, section 4; shared by the API explorer, the XML view and the schema explorer): how many
 * columns the file view's width holds, and how tall the pane is. Pure, so the thresholds and the height have tests of
 * their own.
 */

/** From this width of the file view a request column stands beside the reference. */
export const THREE_FROM = 1480;
/** From this width the answer has a column of its own. */
export const FOUR_FROM = 2500;
/** The pane is never shorter than this. */
export const PANE_FLOOR = 380;
/** The room left under the pane, above the status bar. */
export const PANE_GAP = 14;

/** The columns a file view `width` px wide holds: rail and page; then the request; then the answer. */
export function columnsFor(width: number): 2 | 3 | 4 {
  return width >= FOUR_FROM ? 4 : width >= THREE_FROM ? 3 : 2;
}

/**
 * The columns a width read or observed holds, or `null` where the file view has no width: a page kept hidden while
 * another is shown is not laid out and reports 0 px. Its columns stand as they were, so on its return they are drawn
 * in the first frame; taken as two, the side column came back a frame late and moved the XML view's *Levels* 798 px
 * at 2,560 px (`moves`, 2026-11-24).
 */
export function columnsSeen(width: number): 2 | 3 | 4 | null {
  return width > 0 ? columnsFor(width) : null;
}

/**
 * The pane's height: the window's, less the box's own top with the page's scroller at 0, less the status bar and
 * {@link PANE_GAP}; at least {@link PANE_FLOOR}. Read at mount and on `resize`, never on scroll, so at scroll 0
 * every column is whole on screen, and a page scrolled down leaves the freed room unused.
 */
export function paneHeight(innerHeight: number, top: number, status: number): number {
  return Math.max(PANE_FLOOR, Math.round(innerHeight - top - status - PANE_GAP));
}
