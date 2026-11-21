/**
 * The window's device scale: the display's scale times the window's zoom (`tf/zoom.svelte.ts`), as
 * `devicePixelRatio` tells it, and who hears when it changes (`docs/decisions/2026-11-16-zoom-is-the-window-s.md`).
 *
 * A zoom step and a move to a screen of another scale are the same event here. What keeps lengths on the device's
 * grid hears it: each windowed list re-reads its drawn rows and writes its room again (`windowed.svelte.ts`), and a
 * PDF's pages are drawn again at the new scale (`tf/render/TfPdf.svelte`, through `ratio`). Heard through a media
 * query on the scale now, asked again on each resize too (a zoom resizes the window's CSS viewport), so an engine that
 * tells only one of the two is heard.
 */

import { createSubscriber } from "svelte/reactivity";

/** `devicePixelRatio`, 1 where there is none (jsdom). */
export function currentScale(): number {
  return typeof devicePixelRatio === "number" && devicePixelRatio > 0 ? devicePixelRatio : 1;
}

const listeners = new Set<() => void>();
let ratio = $state(currentScale());
let query: MediaQueryList | null = null;
let armed = false;

function arm() {
  if (typeof window === "undefined") return;
  if (!armed) {
    armed = true;
    window.addEventListener("resize", check, { passive: true });
  }
  query?.removeEventListener?.("change", check);
  query = typeof window.matchMedia === "function" ? window.matchMedia(`(resolution: ${currentScale()}dppx)`) : null;
  query?.addEventListener?.("change", check);
}

/** Reads the scale again and, when it changed, tells everyone who listens. Tests call it after setting the ratio. */
export function checkScale(): void {
  check();
}

function check() {
  const now = currentScale();
  if (now === ratio) return;
  ratio = now;
  arm();
  for (const heard of [...listeners]) heard();
}

/** While an effect reads `deviceScale.ratio`, it is one of those who listen, so the window is heard for it. */
const subscribe = createSubscriber(() => onScaleChange(() => {}));

/** The scale now, read reactively: an effect that reads it runs again when it changes. */
export const deviceScale = {
  get ratio(): number {
    subscribe();
    return ratio;
  },
};

/** Stops hearing the window: nobody listens any more. */
function disarm() {
  query?.removeEventListener?.("change", check);
  query = null;
  if (armed && typeof window !== "undefined") window.removeEventListener("resize", check);
  armed = false;
}

/**
 * Calls `heard` after each change of the scale, until the returned function is called. The window is heard only while
 * someone listens; the scale is read afresh when the first one comes.
 */
export function onScaleChange(heard: () => void): () => void {
  if (listeners.size === 0) {
    ratio = currentScale();
    arm();
  }
  listeners.add(heard);
  return () => {
    listeners.delete(heard);
    if (listeners.size === 0) disarm();
  };
}

/** Tests only: the scale read afresh, and nobody listening. */
export function resetDeviceScale(): void {
  listeners.clear();
  ratio = currentScale();
  disarm();
}
