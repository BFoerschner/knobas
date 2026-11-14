/**
 * Drawing a long thing as it comes into view (the extremes pass of PR #2479, 2026-11-13): an attachment for a mark
 * after what is drawn so far, which asks for more, a task at a time, while the mark is within `margin` pixels of what
 * its scroller shows. A 5 MB Confluence page drawn a piece a task whatever the reader did was still building its body
 * 30 s later, every piece a style and layout pass over all before it; drawn as it nears the view, it draws what is
 * read.
 *
 * `grow` draws one more piece and says whether there is more after it. Where there is no `IntersectionObserver` (the
 * unit tests' DOM) it grows a task at a time, as before.
 */
export function whileNear(grow: () => boolean, margin = 1600) {
  return (mark: HTMLElement) => {
    let timer: ReturnType<typeof setTimeout> | undefined;
    let near = typeof IntersectionObserver === "undefined";
    const step = () => {
      clearTimeout(timer);
      timer = setTimeout(() => {
        if (near && grow()) step();
      }, 0);
    };
    if (typeof IntersectionObserver === "undefined") {
      step();
      return () => clearTimeout(timer);
    }
    let seen: IntersectionObserver | null = null;
    let gone = false;
    const watch = () => {
      if (gone || seen !== null) return;
      seen = new IntersectionObserver(
        (entries) => {
          near = entries.some((entry) => entry.isIntersecting);
          if (near) step();
        },
        { root: scrollerOf(mark), rootMargin: `${margin}px 0px` },
      );
      seen.observe(mark);
    };
    // Which box scrolls the mark is read off its ancestors' computed overflow, which inside a size container lays the
    // page out. Attached as a page is built, that read laid out the page half built, and the frame laid it out again
    // (the render-cost pass, 2026-11-14: 56 to 74 ms of a quarter-speed main thread on a long Docs page's open). So it
    // is read once the mark's first layout is done, where a resize observer is called, when it costs nothing; the
    // intersection observer made there reports in the same frame.
    const laidOut = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(() => (laidOut?.disconnect(), watch()));
    if (laidOut === null) watch();
    else laidOut.observe(mark);
    return () => {
      gone = true;
      laidOut?.disconnect();
      seen?.disconnect();
      clearTimeout(timer);
    };
  };
}

/** The nearest box that scrolls `el` up and down, or `null` (the viewport). */
function scrollerOf(el: HTMLElement): Element | null {
  for (let at = el.parentElement; at !== null; at = at.parentElement) {
    const y = getComputedStyle(at).overflowY;
    if (y === "auto" || y === "scroll") return at;
  }
  return null;
}
