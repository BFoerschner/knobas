/**
 * The sticky table head sits under the filter bar, however many rows the bar wrapped to: `--tf-fbar-h` on the page's
 * scroller is the bar's height. Set on the scroller, a custom property is inherited by every card and row it holds, so
 * each write restyles all of them. It is written only when the height changes from what is in force, which on a first
 * open is the stylesheet's fallback: the bar on one line wrote its own 47 px, and the frame that paints the list's first
 * rows restyled the whole list again in the observer's callback (the ux30 end gate, Windows: 37 ms of a sixth-speed
 * main thread in WebView2, on the frame the rows wait for).
 */
export const FBAR_FALLBACK_PX = 47;

export function fbarHeight(wrap: HTMLElement) {
  if (typeof ResizeObserver === "undefined") return;
  let inForce = FBAR_FALLBACK_PX;
  const seen = new ResizeObserver(() => {
    const bar = wrap.querySelector<HTMLElement>(":scope .tf-sbody > .tf-fbar");
    // A page held hidden lays nothing out: its bar measures 0 and says nothing of the height it will have.
    const px = bar?.offsetHeight ?? 0;
    if (px === 0 || px === inForce) return;
    inForce = px;
    wrap.style.setProperty("--tf-fbar-h", `${px}px`);
  });
  seen.observe(wrap);
  return () => seen.disconnect();
}
