/**
 * The keys of a dialog's filter over the list under it (*Add…*, *Apply a link set…*): `↓` goes from the filter
 * into the list and `↑` from the list's top comes back, so the two read as one control. In the list the arrows walk
 * it. A radio's own arrows pick as they walk; these only stop them wrapping past the ends.
 */
const ROWS = "input:not(:disabled)";

function rowsOf(list: HTMLElement | null): HTMLInputElement[] {
  return list === null ? [] : [...list.querySelectorAll<HTMLInputElement>(ROWS)];
}

/** `↓` in the filter: to what is picked, else the first row. A radio is picked as focus lands, as its own arrows do. */
export function intoList(event: KeyboardEvent, list: HTMLElement | null): void {
  if (event.key !== "ArrowDown") return;
  const rows = rowsOf(list);
  const to = rows.find((row) => row.checked) ?? rows[0];
  if (to === undefined) return;
  event.preventDefault();
  to.focus();
  if (to.type === "radio" && !to.checked) to.click();
}

/** The arrows on a row: walk the ticks, stop a radio wrapping, and `↑` from the top goes back to the filter. */
export function withinList(event: KeyboardEvent, list: HTMLElement | null, filter: HTMLElement | null): void {
  if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return;
  const rows = rowsOf(list);
  const at = rows.indexOf(event.target as HTMLInputElement);
  if (at === -1) return;
  const down = event.key === "ArrowDown";
  const next = rows[at + (down ? 1 : -1)];
  if (next === undefined) {
    event.preventDefault();
    if (!down) filter?.focus();
    return;
  }
  // A radio's own arrow moves it and picks it. A tick has no arrows of its own.
  if (next.type === "radio") return;
  event.preventDefault();
  next.focus();
}
