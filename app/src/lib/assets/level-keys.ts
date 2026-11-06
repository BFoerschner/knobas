/**
 * The keys of a picker's level (`MoveDialog`, `RouteDialog`): `Tab` stops on the level once, on the row its `tabindex`
 * names, and `↑`, `↓`, Home and End walk the rest. Twelve rows of tab stops stood between a dialog's fields and its footer.
 */
const ROW = ".opt:not(:disabled)";

export function levelKeys(event: KeyboardEvent): void {
  if (!["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key)) return;
  const rows = [...(event.currentTarget as HTMLElement).querySelectorAll<HTMLElement>(ROW)];
  if (rows.length === 0) return;
  const at = rows.indexOf(document.activeElement as HTMLElement);
  const to =
    event.key === "Home" ? 0 : event.key === "End" ? rows.length - 1 : Math.min(rows.length - 1, Math.max(0, at + (event.key === "ArrowDown" ? 1 : -1)));
  event.preventDefault();
  event.stopPropagation();
  rows[to]?.focus();
}
