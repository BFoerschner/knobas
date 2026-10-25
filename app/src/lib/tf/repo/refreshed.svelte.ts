/**
 * A repository tab's half of the page's *Refresh* (#2135): the page counts its
 * refreshes into a `refreshed` prop, and each tab reads its own sections again
 * when the count moves on, over what it drew (shell conventions, #2135). The
 * page's *Refreshing…* holds until the open tab's reads have answered too, as
 * the job page's does (#2134).
 */
import { getContext, setContext, tick, untrack } from "svelte";

const KEY = Symbol("repo-refreshed");

/** The page's side: the tabs' re-reads of the refresh under way. */
export interface TabRereads {
  /** Settles once every re-read a tab started for the refresh just counted has answered. */
  settled(): Promise<void>;
}

/** Called by the page while it initialises; its tabs hand it their re-reads. */
export function provideTabRereads(): TabRereads {
  let held: Promise<unknown>[] = [];
  setContext(KEY, (read: Promise<unknown>) => held.push(read));
  return {
    async settled() {
      // The tabs' watchers run on the flush that follows the count's bump.
      await tick();
      const out = held;
      held = [];
      await Promise.allSettled(out);
    },
  };
}

/**
 * Run `reread` each time `count` moves on, never for the count the tab mounted
 * with. Call it while the component initialises; `reread` runs untracked, and
 * the page's *Refreshing…* waits for what it returns.
 */
export function onRefreshed(count: () => number, reread: () => Promise<unknown>): void {
  const hold = getContext<((read: Promise<unknown>) => void) | undefined>(KEY);
  let seen: number | null = null;
  $effect(() => {
    const n = count();
    if (seen !== null && n !== seen) hold?.(untrack(reread));
    seen = n;
  });
}
