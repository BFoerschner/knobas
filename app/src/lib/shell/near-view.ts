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
    const seen = new IntersectionObserver(
      (entries) => {
        near = entries.some((entry) => entry.isIntersecting);
        if (near) step();
      },
      { root: scrollerOf(mark), rootMargin: `${margin}px 0px` },
    );
    seen.observe(mark);
    return () => {
      seen.disconnect();
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
