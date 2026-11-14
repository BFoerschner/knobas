/**
 * **Row room** (design system, Motion): a row's actions (`.tf-rowacts`) stand over the end of the row while it is
 * pointed at, and a value (`.tf-rowval`) hides only if the group really covers it. A value with room beside the group
 * stays, so pointing at a row costs nothing it didn't have to (F-248, F-342).
 *
 * The shell installs one delegated handler. On each `pointerover` in a row with a group, and on keyboard focus into a
 * group, it measures the row's values against the group and marks the covered ones `.tf-covered`; `chrome.css` hides a
 * marked value while the group shows. A partly covered value hides whole, so no word is cut. It measures the words, not
 * the box: a column's `min-width` holds room the group may stand in without touching any of them.
 *
 * While a row is held, its group and the row are watched: a group that changes its words (*Pause* to *Resume*) or a
 * window that resizes measures again without the pointer moving.
 */

/** What a group's row is, as `chrome.css` reads it. */
const ROW = "tr, li, .tf-rowhost, .tf-ib, .tf-r";
/** The audit's own tolerance: a pixel's touch is neither a cut word nor a reason to hide one. */
const TOUCH = 1;

/** Where a value's words are. A `Range` isn't there in every test's DOM, and there the box is the answer. */
function wordsOf(value: Element): DOMRect {
  const range = document.createRange();
  range.selectNodeContents(value);
  return typeof range.getBoundingClientRect === "function" ? range.getBoundingClientRect() : value.getBoundingClientRect();
}

function overlaps(a: DOMRect, b: DOMRect): boolean {
  return a.left < b.right - TOUCH && a.right > b.left + TOUCH && a.top < b.bottom - TOUCH && a.bottom > b.top + TOUCH;
}

/** Mark the values in `row` that one of its groups covers, and unmark the rest. */
export function measureRow(row: Element): void {
  const groups = [...row.querySelectorAll(".tf-rowacts")].map((group) => group.getBoundingClientRect());
  for (const value of row.querySelectorAll(".tf-rowval")) {
    const words = wordsOf(value);
    const covered = words.width > 0 && groups.some((group) => group.width > 0 && overlaps(words, group));
    value.classList.toggle("tf-covered", covered);
  }
}

/** Measures rows as they are pointed at or tabbed into. Returns what takes the handlers off. */
export function installRowRoom(root: Document | HTMLElement = document): () => void {
  let held: Element | null = null;
  const remeasure = () => {
    if (held?.isConnected) measureRow(held);
  };
  const watcher = typeof ResizeObserver === "function" ? new ResizeObserver(remeasure) : null;

  const hold = (row: Element) => {
    measureRow(row);
    if (row === held) return;
    held = row;
    watcher?.disconnect();
    watcher?.observe(row);
    for (const group of row.querySelectorAll(".tf-rowacts")) watcher?.observe(group);
  };

  const onover = (event: Event) => {
    if (!(event.target instanceof Element)) return;
    const row = event.target.closest(ROW);
    if (row?.querySelector(".tf-rowacts")) hold(row);
  };
  // Focus on the row's own link reveals nothing, so only focus in a group is worth a measure.
  const onfocus = (event: Event) => {
    if (!(event.target instanceof Element) || event.target.closest(".tf-rowacts") === null) return;
    const row = event.target.closest(ROW);
    if (row !== null) hold(row);
  };

  root.addEventListener("pointerover", onover);
  root.addEventListener("focusin", onfocus);
  return () => {
    root.removeEventListener("pointerover", onover);
    root.removeEventListener("focusin", onfocus);
    watcher?.disconnect();
    held = null;
  };
}
