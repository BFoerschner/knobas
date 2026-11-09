/**
 * A list read again lands without moving what the reader is reading (the
 * third smoothness pass of PR #2479): a row the new read adds above the
 * reader's place would push every row under it down. The first row still
 * on screen is noted before the read lands and the scroller is moved by as
 * much as that row moved, so it stays where it was. A reader at the top of
 * the page stays at the top and sees the new row come in.
 *
 * A row is an element marked `data-place` inside `within`. Rows keyed by
 * what they show keep their element across the read, so focus inside one
 * stays too; this keeps only the scroll.
 */
import { tick } from "svelte";

/** Run `apply` (the read landing), keeping the first row on screen under `within` where it was. */
export async function landInPlace(within: Element | null | undefined, apply: () => void): Promise<void> {
  const scroller = within?.closest<HTMLElement>(".tf-main, .tf-scroller") ?? null;
  if (within === null || within === undefined || scroller === null || scroller.scrollTop === 0) {
    apply();
    return;
  }
  const top = scroller.getBoundingClientRect().top;
  let mark: Element | null = null;
  let was = 0;
  for (const row of within.querySelectorAll("[data-place]")) {
    if (row.getClientRects().length === 0) continue;
    const box = row.getBoundingClientRect();
    if (box.bottom > top) {
      mark = row;
      was = box.top;
      break;
    }
  }
  apply();
  if (mark === null) return;
  await tick();
  if (!mark.isConnected) return;
  const moved = mark.getBoundingClientRect().top - was;
  if (moved !== 0) scroller.scrollTop += moved;
}
