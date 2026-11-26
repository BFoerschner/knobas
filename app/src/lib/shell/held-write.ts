/**
 * A run's answer that lands in one frame (`docs/decisions/2026-11-20-a-runs-answer-lands-in-one-frame.md`): the rows it
 * brings are put in one write, and the scroll is set so that a row that stays keeps its place on screen. Lifted out of
 * the Files tab's graft (`TfRepoFiles`' `swapIn`, ADR-0046's re-run) so a comparison's folder group (ux26, the decision
 * list's section 9) lands the same way: one way to hold the view, not two.
 *
 * The caller says which rows may hold it (`rows`, in order, each named by `idOf`), which of those still on screen does
 * (`choose`: the first outside what the answer changes, else one inside it the answer keeps, else the answer's own head),
 * how to find it again once the write is drawn (`same`, which may draw it first), and how to scroll (`scrollBy`).
 */
import { flushSync } from "svelte";

export interface HeldPlace {
  /** The rows that may hold the place, in order. */
  rows(): HTMLElement[];
  /** Where the view starts on screen: a row that ends above it is out of view. */
  top(): number;
  /** Which of the rows on screen holds the place; none, nothing is held. */
  choose(drawn: HTMLElement[]): HTMLElement | undefined;
  /** What names a row, the same before and after the write. */
  idOf(row: HTMLElement): string;
  /** The row named `id` once the write is drawn. */
  same(id: string): HTMLElement | undefined;
  /** Scrolls the view by `px`, `row` the one held. */
  scrollBy(row: HTMLElement, px: number): void;
}

/** Makes `write` in one frame, holding the place `how` says. */
export function heldWrite(write: () => void, how: HeldPlace): void {
  const top = how.top();
  const drawn = how.rows().filter((row) => row.getBoundingClientRect().bottom > top);
  const pick = how.choose(drawn);
  const anchor = pick === undefined ? null : { id: how.idOf(pick), top: pick.getBoundingClientRect().top };
  write();
  flushSync();
  if (anchor === null) return;
  const row = how.same(anchor.id);
  if (row === undefined) return;
  const off = row.getBoundingClientRect().top - anchor.top;
  if (Math.abs(off) >= 0.5) how.scrollBy(row, off);
}
