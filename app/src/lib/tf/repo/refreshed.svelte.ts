/**
 * A repository tab's half of the page's *Refresh* (#2135): the page counts its
 * refreshes into a `refreshed` prop, and each tab reads its own sections again
 * when the count moves on, over what it drew (shell conventions, #2135).
 */
import { untrack } from "svelte";

/**
 * Run `reread` each time `count` moves on, never for the count the tab mounted
 * with. Call it while the component initialises; `reread` runs untracked.
 */
export function onRefreshed(count: () => number, reread: () => void): void {
  let seen: number | null = null;
  $effect(() => {
    const n = count();
    if (seen !== null && n !== seen) untrack(reread);
    seen = n;
  });
}
