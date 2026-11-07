/**
 * The page tree's keys (F-444): the tree is one `Tab` stop, and the arrows walk the rows shown. A folder's row is its
 * title's link, so its chevron's `summary` is no stop of its own and a mouse still folds it.
 *
 * The stop follows focus while the tree has it. Once focus has left, it is the open page's row, so coming back lands
 * where the reader is reading, else the row last walked to, else the first. It is read off what is drawn, because a
 * fold can close under it by mouse and rows come and go with a read.
 */

/** A leaf's link, and a folder's title. */
const ROWS = "a.tf-dl-leaf, summary > a.tf-sl";

/** The fold a folder's row opens, or `null` for a leaf. */
function foldOf(row: HTMLElement): HTMLDetailsElement | null {
  return row.parentElement?.tagName === "SUMMARY" ? (row.closest("details") as HTMLDetailsElement | null) : null;
}

/** The fold holding `row`, in `root`: for a folder's row, the one around its own. */
function holderOf(row: HTMLElement, root: HTMLElement): HTMLDetailsElement | null {
  const from = foldOf(row) ?? row;
  const holder = from.parentElement?.closest<HTMLDetailsElement>("details") ?? null;
  return holder !== null && root.contains(holder) ? holder : null;
}

/** Every row a reader can see: those whose folds up to `root` are all open, in the order drawn. */
function shownRows(root: HTMLElement): HTMLElement[] {
  return [...root.querySelectorAll<HTMLElement>(ROWS)].filter((row) => {
    for (let fold = holderOf(row, root); fold !== null; fold = holderOf(fold.querySelector<HTMLElement>(":scope > summary > a") ?? fold, root)) {
      if (!fold.open) return false;
    }
    return true;
  });
}

/** Attach to the element around the top of the tree. */
export function treeKeys(root: HTMLElement): () => void {
  let stop: HTMLElement | null = null;

  function sync(): void {
    const rows = shownRows(root);
    const focused = rows.find((row) => row === document.activeElement);
    const current = rows.find((row) => row.getAttribute("aria-current") === "true");
    stop = focused ?? current ?? (stop !== null && rows.includes(stop) ? stop : (rows[0] ?? null));
    for (const row of root.querySelectorAll<HTMLElement>(ROWS)) {
      row.tabIndex = row === stop ? 0 : -1;
      const fold = foldOf(row);
      if (fold !== null) row.setAttribute("aria-expanded", String(fold.open));
    }
    for (const summary of root.querySelectorAll<HTMLElement>("summary")) summary.tabIndex = -1;
  }

  function onkeydown(event: KeyboardEvent): void {
    if (event.altKey || event.ctrlKey || event.metaKey || event.shiftKey || !(event.target instanceof Element)) return;
    const row = event.target.closest<HTMLElement>("summary")?.querySelector<HTMLElement>(":scope > a") ?? event.target.closest<HTMLElement>(ROWS);
    if (!row) return;
    const rows = shownRows(root);
    const at = rows.indexOf(row);
    if (at < 0) return;
    const fold = foldOf(row);
    let next: HTMLElement | undefined;
    switch (event.key) {
      case "ArrowDown":
        next = rows[at + 1];
        break;
      case "ArrowUp":
        next = rows[at - 1];
        break;
      case "Home":
        next = rows[0];
        break;
      case "End":
        next = rows.at(-1);
        break;
      case "ArrowRight":
        if (fold !== null && !fold.open) fold.open = true;
        else if (fold !== null && rows[at + 1] !== undefined && fold.contains(rows[at + 1]!)) next = rows[at + 1];
        break;
      case "ArrowLeft":
        if (fold?.open) fold.open = false;
        else next = holderOf(row, root)?.querySelector<HTMLElement>(":scope > summary > a") ?? undefined;
        break;
      default:
        return;
    }
    event.preventDefault();
    next?.focus();
  }

  const seen = new MutationObserver(sync);
  seen.observe(root, { subtree: true, childList: true, attributes: true, attributeFilter: ["open", "aria-current"] });
  root.addEventListener("keydown", onkeydown);
  root.addEventListener("focusin", sync);
  root.addEventListener("focusout", sync);
  sync();
  return () => {
    seen.disconnect();
    root.removeEventListener("keydown", onkeydown);
    root.removeEventListener("focusin", sync);
    root.removeEventListener("focusout", sync);
  };
}
