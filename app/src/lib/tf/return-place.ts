/**
 * **The place a reader left** (design system, the return contract): back restores
 * the list's scroll and focuses the row that was opened.
 *
 * The shell takes a {@link Place} of the shown page as an address is left, before
 * that page hides, and {@link restorePlace} sets it back on a return: an `Esc` or
 * *Back to* step, or a step through history. Keep-alive's own `keepScroll` holds a
 * page across a hide, but a place is more than that: a pipeline's runs and a run,
 * the link sets and a set, or a space and a page are one kept page whose scroller
 * stays while what is in it changes.
 *
 * A scroller is told by its element while that is still there, else by its classes
 * and its place among its like. What it holds may redraw after the return (a page
 * reads again when it is shown), so the scroll and the focus are held until they
 * stay or until the reader moves, whichever is first.
 */

/** One scroller's offset, and how to find it again if its element was redrawn. */
interface Scroll {
  el: Element;
  classes: string;
  nth: number;
  top: number;
  left: number;
}

export interface Place {
  scrolls: Scroll[];
  /** What the reader pressed to leave, if a press did. */
  pressed: Element | null;
  /** The address it opened: what finds the row again if its element was redrawn. */
  to: string;
  /** What the pressed control said, and which of its like it was: what finds a control with no address of its own. */
  said: { text: string; nth: number } | null;
}

/** How long a return keeps setting the scroll while the page redraws, in ms. */
const HOLD_FOR = 1500;
/** How long the scroll must stay put before the return is over, in frames. */
const STAYED = 12;
/** A return lasts at least this long, in ms: a page shown again reads again, and may draw its rows anew. */
const AT_LEAST = 500;

const FOCUSABLE = "a[href], button:not([disabled]), [tabindex]";
const classesOf = (el: Element): string => [...el.classList].map((name) => `.${name}`).join("");

/** The page's scrollers: its page scroller and its inner ones even at 0, and anything else that has moved. */
function scrollersOf(main: Element, page: Element): Element[] {
  const inner = [...page.querySelectorAll(".tf-scroller")];
  const moved = [...page.querySelectorAll("*")].filter((el) => (el.scrollTop !== 0 || el.scrollLeft !== 0) && !inner.includes(el));
  return [main, ...inner, ...moved];
}

/** The place in `page`, scrolled by `main` and what is inside it, on the way to the address `to`. */
export function capturePlace(main: Element, page: Element, pressed: Element | null, to: string): Place {
  const scrolls = scrollersOf(main, page).map((el) => {
    const classes = classesOf(el);
    const like = classes === "" || el === main ? [] : [...page.querySelectorAll(classes)];
    return { el, classes, nth: Math.max(0, like.indexOf(el)), top: el.scrollTop, left: el.scrollLeft };
  });
  const held = pressed !== null && page.contains(pressed) ? pressed : null;
  return { scrolls, pressed: held, to, said: saidBy(held, page) };
}

const textOf = (el: Element): string => el.textContent.replace(/\s+/g, " ").trim();

/** What the control around `pressed` says, among those in `page` saying the same. */
function saidBy(pressed: Element | null, page: Element): Place["said"] {
  const near = pressed?.closest(FOCUSABLE) ?? null;
  if (near === null) return null;
  const text = textOf(near);
  return text === "" ? null : { text, nth: Math.max(0, [...page.querySelectorAll(FOCUSABLE)].filter((el) => textOf(el) === text).indexOf(near)) };
}

/** The scroller as it is now: the same element, or the one that took its place. */
function scrollerNow(scroll: Scroll, page: Element): Element | null {
  if (scroll.el.isConnected) return scroll.el;
  return scroll.classes === "" ? null : (page.querySelectorAll(scroll.classes)[scroll.nth] ?? null);
}

/** Sets each scroller back; whether all of them are where they were left. */
function setScrolls(place: Place, page: Element): boolean {
  let all = true;
  for (const scroll of place.scrolls) {
    const el = scrollerNow(scroll, page);
    if (el === null) {
      all = false;
      continue;
    }
    if (Math.abs(el.scrollTop - scroll.top) > 1) el.scrollTop = scroll.top;
    if (Math.abs(el.scrollLeft - scroll.left) > 1) el.scrollLeft = scroll.left;
    if (Math.abs(el.scrollTop - scroll.top) > 1 || Math.abs(el.scrollLeft - scroll.left) > 1) all = false;
  }
  return all;
}

const ROW = "tr, li, [data-entity]";

/** The ticket an address names, which a board's card or a list's row carries as `data-entity`. */
function ticketOf(hash: string): string | null {
  const found = /^#\/(?:ticket|job)\/([^/]+)/.exec(hash)?.[1];
  if (found === undefined) return null;
  try {
    return decodeURIComponent(found);
  } catch {
    return found;
  }
}

/** The row a press opened from, what in it takes focus: the row's link to `to`, else what was pressed, else its first link. */
function inRow(pressed: Element, to: string): HTMLElement | null {
  const near = pressed.closest<HTMLElement>(FOCUSABLE);
  if (near?.getAttribute("href") === to) return near;
  const row = pressed.closest(ROW);
  if (row !== null) {
    const links = [...row.querySelectorAll<HTMLElement>("a[href]")];
    const own = links.find((link) => link.getAttribute("href") === to);
    if (own !== undefined) return own;
  }
  if (near !== null) return near;
  return row?.querySelector<HTMLElement>(FOCUSABLE) ?? null;
}

/** What takes focus in `page` on the way back, or `null` while nothing on it does. */
function focusTarget(place: Place, page: Element): HTMLElement | null {
  const ticket = ticketOf(place.to);
  if (ticket !== null) {
    const card = [...page.querySelectorAll<HTMLElement>("[data-entity]")].find((el) => el.dataset["entity"] === ticket);
    if (card !== undefined) return card;
  }
  const pressed = place.pressed !== null && place.pressed.isConnected && !reworded(place) ? inRow(place.pressed, place.to) : null;
  if (pressed !== null) return pressed;
  // Redrawn: the same words among the links to where it opened, else any link there, else the same words anywhere.
  // Not one in a closed fold, which can't take focus: Docs' rail holds a link to every page, ahead of the row that opened it (F-438).
  const links = [...page.querySelectorAll<HTMLElement>("a[href]")].filter((link) => link.getAttribute("href") === place.to && link.closest("details:not([open])") === null);
  const said = place.said;
  if (said === null) return links[0] ?? null;
  const own = links.find((link) => textOf(link) === said.text) ?? links[0];
  if (own !== undefined) return own;
  const alike = [...page.querySelectorAll<HTMLElement>(FOCUSABLE)].filter((el) => textOf(el) === said.text);
  return alike[said.nth] ?? alike[0] ?? null;
}

/** Whether the pressed element now says something else: a list reused it for another row. */
function reworded(place: Place): boolean {
  const near = place.pressed?.closest(FOCUSABLE) ?? null;
  return near !== null && place.said !== null && textOf(near) !== place.said.text;
}

/**
 * Whether focus is free to take: on nothing, on the page scroller a press in empty space left it on, on the heading the focus
 * keeper puts it on while a return has nothing to focus yet, or left behind on a page now hidden. A dialog or a field the reader went to is not.
 */
function focusIsFree(main: Element): boolean {
  const at = document.activeElement;
  return at === null || at === document.body || at === main || at.matches('h1[tabindex="-1"]') || at.closest("[hidden]") !== null;
}

const later = (run: () => void): void => {
  if (typeof requestAnimationFrame === "function") requestAnimationFrame(run);
  else setTimeout(run, 16);
};

/** What ends a return's hold: the reader moving, by any means. */
const READER_MOVES = ["wheel", "pointerdown", "keydown", "touchstart"] as const;

/** How many frames a return gets to find what it focuses before the focus keeper may use the heading. */
const FIRST_LOOK = 2;

/** Returns still holding focus for what they will put it on. */
let restoring = 0;

/**
 * Whether a return is still putting a place back: focus is its to set for now (`focus-keep.ts`). One with nothing to focus
 * lets go after a first look, so a launcher jump's `Esc` lands at once. The scroll keeps being held, and a row drawn late still takes focus from the heading.
 */
export function placeRestoring(): boolean {
  return restoring > 0;
}

/**
 * Put `place` back on `page`: the scroll now and on each frame until it stays, the
 * focus once its row is there, and again if a redraw takes that row away. Returns
 * what ends it early, as a new address does.
 */
export function restorePlace(place: Place, page: Element): () => void {
  const main = place.scrolls[0]?.el ?? page;
  let over = false;
  let stayed = 0;
  let focused: HTMLElement | null = null;
  let looks = 0;
  let holdsFocus = true;
  const letGo = () => {
    if (holdsFocus) restoring -= 1;
    holdsFocus = false;
  };
  const started = performance.now();
  const end = () => {
    if (over) return;
    over = true;
    letGo();
    for (const type of READER_MOVES) window.removeEventListener(type, end, true);
  };
  restoring += 1;
  for (const type of READER_MOVES) window.addEventListener(type, end, true);
  const step = () => {
    if (over) return;
    if (!page.isConnected || performance.now() - started > HOLD_FOR) return end();
    const set = setScrolls(place, page);
    stayed = set ? stayed + 1 : 0;
    // Tried on each frame while focus is free: a row just shown may not take it yet, and a redraw may take it away.
    // A pane may reuse what took it for something else once its read lands, which is not what was pressed.
    if (focusIsFree(main) || (focused !== null && document.activeElement === focused && place.said !== null && textOf(focused) !== place.said.text)) {
      focused = focusTarget(place, page);
      focused?.focus({ preventScroll: true });
    }
    looks += 1;
    if (focused === null && looks > FIRST_LOOK) letGo();
    const lasted = performance.now() - started;
    if (stayed >= STAYED && lasted > AT_LEAST && focused !== null && document.activeElement === focused) return end();
    if (stayed >= STAYED && lasted > HOLD_FOR / 2) return end();
    later(step);
  };
  step();
  return end;
}

/** A place opened anew in a kept page: its scrollers back at the top. */
export function resetScrolls(main: Element, page: Element): void {
  for (const el of [main, ...page.querySelectorAll(".tf-scroller")]) {
    el.scrollTop = 0;
    el.scrollLeft = 0;
  }
}
