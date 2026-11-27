/**
 * A tree row that opens something and holds more (Björn, 2026-11-21: "when I click on a tree element, I want it to always
 * expand … click on it again, it should toggle"; `docs/decisions/2026-11-21-a-click-on-a-docs-row-opens-its-fold.md`).
 * Shared by every tree drawn as `<details>` whose `<summary>` holds the row's link, `summary > a`: the Docs tree
 * (`docs/TfDocTree.svelte`), and any other that wants the same rule.
 *
 * - A click anywhere on the row right of the chevron opens the link's place and opens the fold. A first click never closes it.
 * - A click on the row of the place already open toggles the fold.
 * - The chevron, left of the link, folds alone and opens nothing.
 * - `Enter` on the link (a click with no count), a modified click and a middle click keep the link's own way.
 *
 * The fold's state is set in the press (`set`, then `open`), not left to the browser's `toggle`, which comes a frame
 * later, so a fold that draws its rows only while open draws them in the frame its chevron turns.
 */

import { isAltClick } from "./tree-alt";

/** What the caller keeps for a fold: `set(open)` stores it before the fold's `open` changes. */
export type FoldSet = (open: boolean) => void;

/**
 * Call from the row's `<summary>` click. `here`: whether the link's place is the one open now.
 * When `onAlt` is passed, `Alt+click` toggles the fold recursively instead of opening the link.
 */
export function foldPress(
  event: MouseEvent & { currentTarget: HTMLElement },
  here: boolean,
  set: FoldSet,
  onAlt?: FoldSet,
): void {
  const summary = event.currentTarget;
  const fold = summary.parentElement;
  if (onAlt !== undefined && isAltClick(event)) {
    if (!(fold instanceof HTMLDetailsElement)) return;
    event.preventDefault();
    const now = !fold.open;
    onAlt(now);
    fold.open = now;
    return;
  }
  if (event.detail === 0 || event.button !== 0 || event.ctrlKey || event.metaKey || event.shiftKey || event.altKey) return;
  const link = summary.querySelector<HTMLAnchorElement>(":scope > a");
  if (!(fold instanceof HTMLDetailsElement) || link === null) return;
  const onLink = event.target instanceof Node && link.contains(event.target);
  const onChevron = !onLink && event.clientX < link.getBoundingClientRect().left;
  const now = onChevron || here ? !fold.open : true;
  // A link inside a summary neither folds it nor needs stopping. The chevron and the row's free end are the summary's,
  // whose own fold would land its rows a frame after the chevron turns: folded here.
  if (!onLink) event.preventDefault();
  if (now !== fold.open) {
    set(now);
    fold.open = now;
  }
  // The row's free end goes where its link goes, through the link, so the shell sees a plain press of it.
  if (!onLink && !onChevron && !here) link.click();
}

/**
 * Call from the row's `<summary>` keydown, when the tree's own keys set `open` on → and ← (`docs/tree-keys.ts`, which
 * listens below the app's root, so it has run): the caller's state follows in the same press.
 */
export function foldKeyed(event: KeyboardEvent & { currentTarget: HTMLElement }, set: FoldSet): void {
  if (event.key !== "ArrowRight" && event.key !== "ArrowLeft") return;
  const fold = event.currentTarget.parentElement;
  if (fold instanceof HTMLDetailsElement) set(fold.open);
}
