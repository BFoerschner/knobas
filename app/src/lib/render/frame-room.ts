/**
 * A renderer's page in its sandboxed frame, as a person scrolls and zooms it (2026-11-17, Björn: "When I have an
 * application render an html for code folders I can't scroll it even though it's longer than the page").
 *
 * The frame scrolls inside itself: with no script in it, the page's height can't be read, so it can't be sized to its
 * content. {@link fitRoom} gives it the room from where it starts to the bottom of the pane it is drawn in, so the
 * header above it (*Run again*, *Stop*) and both of its scrollbars are in the window at rest, and a wheel runs on into
 * the pane only once the page is at its end.
 *
 * A wheel over the frame goes to the frame's document, which the app can't hear, so ⌘ or Ctrl and the wheel never
 * reached the window's zoom there. {@link zoomThrough} takes the pointer off the frame while ⌘ or Ctrl is held, so the
 * wheel lands on the app and zooms it, as it does over the rest of the window.
 */

/** The least room a frame takes, where the pane is short or the frame starts low: the pane scrolls then. */
export const ROOM_FLOOR = 360;
/** What is left between the frame's box and the pane's bottom edge. */
export const ROOM_GAP = 12;

/** The nearest box that scrolls up and down, `null` for the page itself. */
function paneOf(node: HTMLElement): HTMLElement | null {
  for (let at = node.parentElement; at !== null; at = at.parentElement) {
    const y = getComputedStyle(at).overflowY;
    if (y === "auto" || y === "scroll") return at;
  }
  return null;
}

/**
 * The room from `node`'s top, with its pane scrolled to the top, to the pane's bottom edge less {@link ROOM_GAP}; at
 * least {@link ROOM_FLOOR}. Where the pane stands now does not count, so a return to a scrolled page gives the same.
 */
export function roomIn(node: HTMLElement, pane: HTMLElement | null): number {
  const top = node.getBoundingClientRect().top;
  const [paneTop, paneHeight, scrolled] =
    pane === null ? [0, document.documentElement.clientHeight, window.scrollY] : [pane.getBoundingClientRect().top, pane.clientHeight, pane.scrollTop];
  return Math.max(ROOM_FLOOR, Math.floor(paneTop + paneHeight - (top + scrolled) - ROOM_GAP));
}

/**
 * An attachment: sets `--tf-room` on `node`, its room in the pane ({@link roomIn}), before its first frame, and again
 * when the pane's size changes or the nearest `watch` (a selector for what holds the lines above it) grows or shrinks,
 * so a banner that comes above the frame shortens it rather than pushing its bottom out of the window.
 */
export function fitRoom(watch: string | null = null) {
  return (node: HTMLElement) => {
    const pane = paneOf(node);
    let set = "";
    const fit = () => {
      const room = `${roomIn(node, pane)}px`;
      if (room === set) return;
      set = room;
      node.style.setProperty("--tf-room", room);
    };
    fit();
    if (typeof ResizeObserver === "undefined") return;
    const seen = new ResizeObserver(fit);
    seen.observe(pane ?? document.documentElement);
    const held = watch === null ? null : node.closest(watch);
    if (held !== null) seen.observe(held);
    return () => seen.disconnect();
  };
}

/** The class that takes the pointer off a frame while ⌘ or Ctrl is held. */
export const ZOOM_THROUGH = "tf-zoomthrough";

/**
 * An attachment on a frame: while ⌘ or Ctrl is held, the frame takes no pointer (`ZOOM_THROUGH`), so ⌘ or Ctrl and
 * the wheel over it zoom the window. Read from every key and wheel the app hears; a blur lets go. Where focus is in the
 * page (after a press in it, so its keys scroll it), the app hears no key and no pointer over the frame (a frame of
 * another origin tells its parent nothing, measured in Chrome): a press anywhere outside the page gives the keys and
 * the zoom back to the app.
 */
export function zoomThrough(frame: HTMLElement) {
  let on = false;
  const hold = (now: boolean) => {
    if (now === on) return;
    on = now;
    frame.classList.toggle(ZOOM_THROUGH, now);
  };
  const read = (event: KeyboardEvent | MouseEvent) => hold(event.metaKey || event.ctrlKey);
  const off = () => hold(false);
  window.addEventListener("keydown", read, true);
  window.addEventListener("keyup", read, true);
  window.addEventListener("wheel", read, { capture: true, passive: true });
  window.addEventListener("blur", off);
  return () => {
    window.removeEventListener("keydown", read, true);
    window.removeEventListener("keyup", read, true);
    window.removeEventListener("wheel", read, true);
    window.removeEventListener("blur", off);
    frame.classList.remove(ZOOM_THROUGH);
  };
}
