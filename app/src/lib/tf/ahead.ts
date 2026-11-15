/**
 * The sections opened ahead of their press (the ux13-overs pass, 2026-11-15): once the window is ready and the reader
 * has been still a moment, the shell opens each section's page hidden, one at a time, as keep-alive holds a page the
 * reader left (`kept-pages.svelte.ts`), so the first press of its nav entry shows a page already built.
 *
 * A first open cost about twice a later one: at CPU 4x on the saved scale database Builds drew in 231 ms first and in
 * 107 when built again later, Tickets' board 191 and 94, Monitors 253 and 62, nearly all of it the engine running the
 * page's code and laying out its styles for the first time. Held, the press is a return: 45 to 75 ms.
 *
 * Each page goes into a free room of *Pages kept open* behind every page held, so it never drops a page the reader
 * left, and *Pages kept open* at 0 opens none. Each waits for a second with no key, press, pointer move, wheel or scroll and for the
 * window to be idle, so its build never lands in the reader's input. The page is hidden from its first frame: it
 * reads what it draws (the answer cache warmed most of it), pauses what it does while hidden (#2249) and takes no keys.
 */
import { holdAhead } from "./kept-pages.svelte";
import type { Section } from "./route";

/** The sections opened ahead, most used first: each a page whose first open was over its line or near it. */
export const AHEAD: readonly Exclude<Section, "job" | "assets" | "settings" | "time" | "sets">[] = ["tickets", "builds", "code", "monitors", "inbox", "docs"];

/** How long the reader must have been still before a page is opened, ms. */
export const AHEAD_QUIET_MS = 1000;

let lastInput = 0;
const heard = () => (lastInput = performance.now());

function idle(): Promise<void> {
  return new Promise((resolve) => {
    if (typeof requestIdleCallback === "function") requestIdleCallback(() => resolve(), { timeout: 2000 });
    else setTimeout(resolve, 50);
  });
}

async function quietAndIdle(): Promise<void> {
  for (;;) {
    const since = performance.now() - lastInput;
    if (since < AHEAD_QUIET_MS) await new Promise((resolve) => setTimeout(resolve, AHEAD_QUIET_MS - since));
    await idle();
    if (performance.now() - lastInput >= AHEAD_QUIET_MS) return;
  }
}

/** Two frames, so the page before has drawn and its reads have gone out before the next is built. */
function twoFrames(): Promise<void> {
  return new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(() => resolve())));
}

let started = false;

/**
 * Opens {@link AHEAD}'s sections hidden, one at a time, each once the window is `ready()` and `wait` has passed (the
 * reader still and the window idle); never the section `current()` is in, and none under jsdom unless a test hands its
 * own `wait`. Once a session. Returns a stop for the shell's teardown.
 */
export function openAhead(
  ready: () => boolean,
  current: () => Section | null,
  /** Where the section's nav entry goes now: the page held is the one its press shows (Fable's ruling). */
  hrefOf: (section: (typeof AHEAD)[number]) => string = (section) => `#/${section}`,
  wait: () => Promise<void> = quietAndIdle,
  sections: readonly (typeof AHEAD)[number][] = AHEAD,
): () => void {
  if (started || typeof document === "undefined") return () => {};
  if (wait === quietAndIdle && /jsdom/i.test(navigator.userAgent)) return () => {};
  started = true;
  let stopped = false;
  // A pointer on its way to a nav entry is the moment before a press (Fable's ruling): it counts as input too.
  const events = ["keydown", "pointerdown", "pointermove", "wheel", "scroll", "touchstart"] as const;
  for (const event of events) addEventListener(event, heard, { capture: true, passive: true });
  heard();
  void (async () => {
    for (const section of sections) {
      await wait();
      while (!stopped && !ready()) {
        await new Promise((resolve) => setTimeout(resolve, 250));
        await wait();
      }
      if (stopped) return;
      if (current() === section) continue;
      holdAhead(hrefOf(section));
      if (wait === quietAndIdle) await twoFrames();
    }
  })().finally(() => {
    for (const event of events) removeEventListener(event, heard, { capture: true });
  });
  return () => {
    stopped = true;
  };
}

/** Tests only: a new session. */
export function forgetAhead(): void {
  started = false;
}
