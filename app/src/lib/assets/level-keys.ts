/**
 * The keys of a picker's level (`MoveDialog`, `RouteDialog`): `Tab` stops on the level once, on the row its `tabindex`
 * names, and `↑`, `↓`, Home and End walk the rest. Twelve rows of tab stops stood between a dialog's fields and its footer.
 * `→` and `←` walk the tree as the columns behind the dialog do (F-537): into the focused row, or the first when the list
 * itself holds focus, and up a level. A row says who it is with `data-row`.
 */
const ROW = ".opt:not(:disabled)";

export interface LevelWalk {
  into: (id: string) => void;
  up: () => void;
}

export function levelKeys(event: KeyboardEvent, walk: LevelWalk): void {
  if (!["ArrowDown", "ArrowUp", "ArrowLeft", "ArrowRight", "Home", "End"].includes(event.key)) return;
  if (event.key === "ArrowLeft") {
    event.preventDefault();
    event.stopPropagation();
    walk.up();
    return;
  }
  const rows = [...(event.currentTarget as HTMLElement).querySelectorAll<HTMLElement>(ROW)];
  if (rows.length === 0) return;
  const at = rows.indexOf(document.activeElement as HTMLElement);
  event.preventDefault();
  event.stopPropagation();
  if (event.key === "ArrowRight") {
    const id = (rows[at] ?? rows[0])?.dataset.row;
    if (id !== undefined) walk.into(id);
    return;
  }
  const to =
    event.key === "Home" ? 0 : event.key === "End" ? rows.length - 1 : Math.min(rows.length - 1, Math.max(0, at + (event.key === "ArrowDown" ? 1 : -1)));
  rows[to]?.focus();
}
