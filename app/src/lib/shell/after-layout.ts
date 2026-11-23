/**
 * Runs `then` once the next frame has laid `node` out: in that frame, after its layout and before its paint (a
 * ResizeObserver's first word on a box), so what `then` reads is laid out already and what it changes is painted in that
 * same frame. Read in the task that mounts a page instead, the same reads lay the whole new page out there, and every
 * answer the page asked for waits behind it (the ux30 end gate, Windows: Builds' rail kept its pipeline in view on a
 * run's first open, 135 ms of a sixth-speed main thread before the run's reads could land).
 *
 * A box with no size (a page held hidden) is never laid out, so `then` waits until it is shown. Where there is no
 * ResizeObserver (jsdom), `then` runs once the current microtasks are done. Returns what cancels it.
 */
export function afterLayout(node: Element, then: () => void): () => void {
  let done = false;
  if (typeof ResizeObserver === "undefined") {
    queueMicrotask(() => {
      if (!done) then();
      done = true;
    });
    return () => (done = true);
  }
  const seen = new ResizeObserver(() => {
    seen.disconnect();
    if (done) return;
    done = true;
    then();
  });
  seen.observe(node);
  return () => {
    done = true;
    seen.disconnect();
  };
}
