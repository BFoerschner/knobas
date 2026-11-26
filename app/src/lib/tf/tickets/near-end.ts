/**
 * A view that shows every ticket reads a column's, a list's or the lanes' next page as the reader nears its end
 * (ux18, Fable's ruling): the mark under the last card drawn is watched, and `reach` runs while it stands within two
 * screens below its scroller's view, which is where a page of 200 lands before the eye gets there. Nothing is read
 * for a column scrolled away sideways.
 *
 * `held` is how many the column holds: read here, so a page that lands attaches the mark again, and a mark a page
 * left in reach asks for the next at once.
 */
export function nearEnd(reach: () => void, held: () => number) {
  return (node: HTMLElement) => {
    void held();
    if (typeof IntersectionObserver === "undefined") return;
    let seen: IntersectionObserver | null = null;
    // Its scroller is read the frame after it drew: read while the page mounts, each mark's computed style forced the
    // page's whole style to be worked out in the middle of the mount.
    const frame = requestAnimationFrame(() => {
      seen = new IntersectionObserver((entries) => {
        if (entries.some((entry) => entry.isIntersecting)) reach();
      }, { root: scrollerOf(node), rootMargin: "0px 0px 200% 0px" });
      seen.observe(node);
    });
    return () => {
      cancelAnimationFrame(frame);
      seen?.disconnect();
    };
  };
}

/** The nearest box above `node` that scrolls up and down, or `null` for the window. */
function scrollerOf(node: HTMLElement): HTMLElement | null {
  for (let at = node.parentElement; at !== null; at = at.parentElement) {
    const { overflowY } = getComputedStyle(at);
    if (overflowY === "auto" || overflowY === "scroll") return at;
  }
  return null;
}
