/**
 * Where a Settings list's focus goes when one of its pages hands back to it (2026-11-17, Björn: "After adding a new
 * application for open in it opens another page instead of going back to the checkouts and open…"): an application's
 * page to *Checkouts and Open in*, a folder renderer's page to *Renderers*. *Save* lands on the entry saved, a new one
 * included, *Remove* on the entry that took its place (or *Add*), *Cancel* on the entry it was opened on.
 *
 * The page notes it as it leaves for the list; the list takes it once drawn and focuses its `[data-land]` of that key,
 * scrolled into view. The shell's return (`return-place.ts`) puts the scroll back and focuses what was pressed, which a
 * new entry or a removed one isn't, so the list focuses after the return's first look. Taken once: a later visit to the
 * list lands nowhere.
 */

export type LandingList = "checkouts" | "folder-renderers";

/** What `data-land` names for the list's *Add* link. */
export const LAND_ADD = "add";

let waiting: { list: LandingList; key: string } | null = null;

/** The page's end: the list `list` is to focus `key`'s entry when it is next drawn. */
export function landOn(list: LandingList, key: string): void {
  waiting = { list, key };
}

/** The list's end: the key it is to focus, once. */
export function takeLanding(list: LandingList): string | null {
  if (waiting === null || waiting.list !== list) return null;
  const key = waiting.key;
  waiting = null;
  return key;
}

/** What a list's entry carries as `data-land`, so lists drawn side by side never take each other's landing. */
export function landKey(list: LandingList, key: string): string {
  return `${list}:${key}`;
}

/**
 * Focus `key`'s `[data-land]` in the document, scrolled into view, and once more after the shell's return has had its first
 * look (it focuses what was pressed while focus is free). `false` if none is drawn.
 */
export function land(list: LandingList, key: string): boolean {
  const name = landKey(list, key);
  const target = [...document.querySelectorAll<HTMLElement>("[data-land]")].find((el) => el.dataset["land"] === name);
  if (target === undefined) return false;
  const go = () => {
    target.focus({ preventScroll: true });
    target.scrollIntoView?.({ block: "nearest" });
  };
  go();
  if (typeof requestAnimationFrame === "function") {
    requestAnimationFrame(() =>
      requestAnimationFrame(() => {
        const at = document.activeElement;
        // Only if the return took it, or nothing holds it: a reader who moved on keeps their focus.
        const taken = at === null || at === document.body || (at instanceof HTMLElement && at.dataset["land"] !== undefined && at.dataset["land"].startsWith(`${list}:`));
        if (target.isConnected && at !== target && taken) go();
      }),
    );
  }
  return true;
}
