/**
 * **The focus keeper** (design system, The press rule): a press never drops the reader's place on the body.
 *
 * The shell installs one keeper over the page and the Connections drawer. It notes what holds focus there, and when that goes away or is
 * disabled and focus has nowhere else to be, it hands focus on. While the pressed row is still there: the same words in
 * that row, else the row itself, never a neighbour and never what happens to stand at the same place, because a second
 * `Enter` must repeat the press or do nothing (F-646, F-447). Once the row has left: the same words in the row that took
 * its place (or the one before, at the end of a list), else that row itself. `Tab` goes on into the row.
 * A control with no row: the same words beside it, else what held it (F-340), never what follows it. With neither left:
 * the same words on the page, else the page's heading.
 *
 * A move made by key with no place to return to focuses the page's heading (F-052), so the next `Tab` starts on the
 * page and not back at the top bar. So does a dialog or the launcher closed by key with nothing to give focus back to.
 *
 * It stays out of the way of what knows better: an open dialog owns focus and gives it back itself, and a return to a
 * place is `return-place.ts`'s. A page that puts focus somewhere itself, on the same press, has got there first, because
 * the keeper only acts a frame later, and only when focus is free.
 *
 * A pressed button doesn't take focus in every webview, so a press counts as being on that control. A dialog it opened
 * then has nothing to give focus back to, and the keeper does that once the dialog has gone (`shared-radio-enter`).
 * Such a webview gives focus to the nearest box that takes it, so a box that holds what fell counts as nothing here.
 *
 * **The box you pressed in holds the keys** (design system, Keys) is {@link installKeysHere}, below.
 */
import { placeRestoring } from "./return-place";

/** What counts as a row, for finding the one that took a row's place. */
const ROW = "tr, li, [data-row], [data-entity], .tf-rowhost, .tf-ib, .tf-r, .tf-lk";
/** Where the keeper looks after focus: the page, and the drawer that floats over it. */
const KEPT = ".tf-main, .tf-drawer";
const CONTROL = 'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), summary, [tabindex]:not([tabindex="-1"])';
/** How long a move by key waits for its page to draw a heading, in ms. */
const WAIT_FOR_HEADING = 1500;
/** What a control with no row can hand focus back to stops here: the page has its heading for that, and a pane's scroller would draw a clipped ring (F-649). */
const PAGE = ".tf-main, .tf-kept, .tf-scroller, body";
/** What holds the keys after a press in its text or a wheel over it: a page's inner scroller, and the asset pane. */
const BOX = ".tf-scroller, .pane";
/** What a page moves by, with no modifier but `Shift`. */
const READING_KEYS = new Set(["PageDown", "PageUp", " ", "Home", "End", "ArrowDown", "ArrowUp", "ArrowLeft", "ArrowRight"]);
/** What turns a page, which a row in a reading rail hands on: the arrows stay the rail's, to walk its rows. */
const PAGE_KEYS = new Set(["PageDown", "PageUp", " "]);
/** A rail of links beside what they open, as Docs' tree and its search hits are: a press there opens a page to read. */
const READING_RAIL = "[data-reading-rail] a[href]";

const later = (run: () => void): number => (typeof requestAnimationFrame === "function" ? requestAnimationFrame(run) : (setTimeout(run, 16) as unknown as number));
const stop = (handle: number): void => {
  if (typeof cancelAnimationFrame === "function") cancelAnimationFrame(handle);
  else clearTimeout(handle);
};

/** What the control says: its label, else its words. */
function saidBy(el: Element): string {
  return (el.getAttribute("aria-label") ?? el.textContent).replace(/\s+/g, " ").trim();
}

function isDisabled(el: Element): boolean {
  return el.matches(":disabled");
}

function controlsIn(root: Element): HTMLElement[] {
  return [...root.querySelectorAll<HTMLElement>(CONTROL)].filter((el) => el.closest("[hidden], details:not([open]) > :not(summary)") === null);
}

/** What identifies a row across a redraw: what it says it is, else where its first link goes. */
function rowKey(row: Element): string | null {
  if (row instanceof HTMLElement) {
    const said = row.dataset["row"] ?? row.dataset["entity"];
    if (said !== undefined) return said;
  }
  return row.querySelector("a[href]")?.getAttribute("href") ?? null;
}

/** Where focus was, and what finds its like again once it has gone. */
interface Held {
  control: HTMLElement;
  said: string;
  row: Element | null;
  list: Element | null;
  /** The row's place in its list. */
  at: number;
  key: string | null;
  /** What holds it, nearest first, up to the drawer it is in or the page. */
  above: HTMLElement[];
  /** The drawer it was in: what is left of it takes focus before the page behind it does. */
  drawer: HTMLElement | null;
}

function holders(control: HTMLElement): HTMLElement[] {
  const above: HTMLElement[] = [];
  for (let at = control.parentElement; at !== null && !at.matches(PAGE); at = at.parentElement) {
    above.push(at);
    if (at.matches(".tf-drawer")) break;
  }
  return above;
}

function note(control: HTMLElement): Held {
  const row = control.closest(ROW);
  const list = row?.parentElement ?? null;
  return {
    control,
    said: saidBy(control),
    row,
    list,
    at: row !== null && list !== null ? [...list.children].indexOf(row) : -1,
    key: row === null ? null : rowKey(row),
    above: holders(control),
    drawer: control.closest<HTMLElement>(".tf-drawer"),
  };
}

/** A row that holds something to focus, at or nearest before `at` in `list`. */
function rowAt(list: Element, at: number): Element | null {
  const rows = [...list.children].filter((child) => child.matches(ROW) && controlsIn(child).length > 0);
  if (rows.length === 0) return null;
  return [...list.children].slice(at).find((child) => rows.includes(child)) ?? rows.at(-1)!;
}

/** `el`, ready to take focus without becoming a stop on the way through the page. */
function ready(el: HTMLElement): HTMLElement {
  if (!el.hasAttribute("tabindex")) el.tabIndex = -1;
  return el;
}

/** In `within`, the control saying what `held` said; else `within` itself. */
function sameWordsIn(within: Element, held: Held): HTMLElement | null {
  const same = controlsIn(within).find((el) => saidBy(el) === held.said);
  if (same !== undefined) return same;
  return within instanceof HTMLElement ? ready(within) : null;
}

/** The page shown in `.tf-main`. */
function shownPage(): Element | null {
  return document.querySelector(".tf-main > .tf-kept:not([hidden])") ?? document.querySelector(".tf-main");
}

/** The page's heading, ready to take focus. It isn't a stop on the way through the page, so a `Tab` doesn't land on it. */
function headingOf(page: Element | null): HTMLElement | null {
  const heading = page?.querySelector<HTMLElement>("h1") ?? null;
  return heading === null ? null : ready(heading);
}

/** The shown page's heading, ready to take focus: where a place with nothing nearer ends. */
export function pageHeading(): HTMLElement | null {
  return headingOf(shownPage());
}

/** Where focus goes now `held` has gone. */
function nextFor(held: Held): HTMLElement | null {
  if (held.row !== null) {
    if (held.row.isConnected) return sameWordsIn(held.row, held);
    const list = held.list?.isConnected ? held.list : null;
    if (list !== null) {
      // Redrawn: the row that says it is the same one.
      const again = held.key === null ? undefined : [...list.children].find((child) => child.matches(ROW) && rowKey(child) === held.key);
      if (again !== undefined) return sameWordsIn(again, held);
      const took = rowAt(list, held.at);
      if (took !== null) return sameWordsIn(took, held);
    }
  } else {
    // What held it, or the nearest that is left when the whole of it went (*Add 47m*'s group).
    const near = held.above.find((el) => el.isConnected && el.closest("[hidden]") === null);
    if (near !== undefined) return sameWordsIn(near, held);
  }
  if (held.drawer?.isConnected) return sameWordsIn(held.drawer, held);
  const page = shownPage();
  // The same words, where only one control says them: a block the page drew anew.
  if (page !== null && held.said !== "") {
    const alike = controlsIn(page).filter((el) => saidBy(el) === held.said);
    if (alike.length === 1) return alike[0]!;
  }
  return headingOf(page);
}

/** Whether focus is nowhere a reader put it. */
function focusFree(held: HTMLElement | null = null): boolean {
  const at = document.activeElement;
  if (at === null || at === document.body || at === document.documentElement || at === held) return true;
  return at.matches(".tf-main, .tf-scroller") || at.closest("[hidden]") !== null;
}

const dialogUp = (): boolean => document.querySelector(".tf-modal") !== null;

/** The shown page's box for reading keys when no press or wheel named one: its inner scroller, the one with somewhere to go first, else the page scroller. */
function readingBox(): HTMLElement | null {
  const inner = [...(shownPage()?.querySelectorAll<HTMLElement>(".tf-scroller") ?? [])];
  return inner.find((el) => el.scrollHeight > el.clientHeight) ?? inner[0] ?? document.querySelector<HTMLElement>(".tf-main");
}

/**
 * **The box you pressed in holds the keys** (design system, Keys). A press in a box's text focuses it by itself, since it
 * takes focus (F-564, F-463). This covers what that leaves: a wheel over a box, a page opened by a tile or an address, a
 * nav press, whose focus stays on the entry, and a webview that gave the press no focus. The first reading key with focus
 * on nothing moves focus to the box last pressed in or wheeled over, else to the shown page's, before the key acts.
 *
 * Asked at the key and not at the wheel or the opening, so a page that puts focus somewhere itself is never raced, and a
 * selection isn't touched by a wheel. It stops at a control or a field, which keep their keys, at an open dialog or drawer,
 * and at a press or a wheel outside a box, so a wheel over a tree's column leaves `↓` to the walk.
 */
export function installKeysHere(): () => void {
  let aimed: HTMLElement | null = null;
  const aim = (event: Event) => {
    aimed = event.target instanceof Element ? event.target.closest<HTMLElement>(BOX) : null;
  };
  const onkey = (event: KeyboardEvent) => {
    const at = document.activeElement;
    // `Tab` starts where it starts from the page scroller, or it would throw a scrolled column back to its first control.
    if (event.key === "Tab" && at?.matches(".tf-scroller")) document.querySelector<HTMLElement>(".tf-main")?.focus({ preventScroll: true });
    if (!READING_KEYS.has(event.key) || event.metaKey || event.ctrlKey || event.altKey || event.defaultPrevented) return;
    // A row of a reading rail keeps `↓` for its walk and hands the page keys on (Round 17: PageDown after opening a Docs page from its tree moved nothing).
    const railRow = at?.matches(READING_RAIL) === true && PAGE_KEYS.has(event.key);
    const nothing = at === null || at === document.body || railRow || at.matches('.tf-main, .tf-scroller, .tf-top a, h1[tabindex="-1"]') || at.closest("[hidden]") !== null;
    if (!nothing || document.querySelector(".tf-modal, .tf-drawer") !== null) return;
    const box = !railRow && aimed?.isConnected && aimed.closest("[hidden]") === null ? aimed : readingBox();
    if (box !== null && !box.contains(at)) box.focus({ preventScroll: true });
  };
  window.addEventListener("pointerdown", aim, true);
  window.addEventListener("wheel", aim, { capture: true, passive: true });
  window.addEventListener("keydown", onkey, true);
  return () => {
    window.removeEventListener("pointerdown", aim, true);
    window.removeEventListener("wheel", aim, true);
    window.removeEventListener("keydown", onkey, true);
  };
}

/** Keeps focus on the page through presses and moves. Returns what takes it off. */
export function installFocusKeep(): () => void {
  let held: Held | null = null;
  let checking: number | null = null;
  let waiting: number | null = null;
  let byKey = false;
  /** Whether a dialog was up when last looked: its going is a reason to look again. */
  let wasUp = false;

  const fell = (place: Held): boolean => !place.control.isConnected || isDisabled(place.control);

  const check = () => {
    checking = null;
    const up = dialogUp();
    const closed = wasUp && !up;
    wasUp = up;
    if (up || placeRestoring()) return;
    if (held === null) {
      if (closed && byKey && focusFree()) headingOf(shownPage())?.focus({ preventScroll: true });
      return;
    }
    if (!focusFree(held.control) && !held.above.includes(document.activeElement as HTMLElement)) return;
    const gone = fell(held);
    if (!gone && !closed) return;
    const next = gone ? nextFor(held) : held.control;
    held = null;
    next?.focus({ preventScroll: true });
    // What can't take focus (nothing drawn for it) still doesn't drop the reader's place on the body.
    if (next !== null && document.activeElement !== next) headingOf(shownPage())?.focus({ preventScroll: true });
  };
  const soon = () => {
    if (checking === null) checking = later(check);
  };

  const onfocusin = (event: Event) => {
    const target = event.target;
    // A dialog drawn inside a page gives focus back itself. What stays noted is what opened it, in case that goes with the answer.
    if (!(target instanceof HTMLElement) || target.closest(KEPT) === null || target.closest(".tf-modal") !== null) return;
    // A box is no place of the reader's to hand on from: what it holds is the page's.
    if (target.matches(BOX)) {
      // A find bar closing gave focus back to the box ⌘F was pressed in, so what goes with the bar is no place to hand on from either.
      if (held?.control.closest(".tf-findbar")) held = null;
      return;
    }
    held = note(target);
  };
  // Focus went because the reader moved it: a click on nothing, or `Esc` out of a field. Asked once the DOM work of this
  // task is over, since a control being removed blurs before it goes.
  const onfocusout = (event: Event) => {
    const target = event.target;
    if (!(target instanceof HTMLElement) || held?.control !== target || (event as FocusEvent).relatedTarget !== null) return;
    queueMicrotask(() => {
      if (held?.control === target && !fell(held)) held = null;
    });
  };
  /** What a row's neighbours are is read as the press starts: by the time focus falls they may have gone. */
  const refresh = () => {
    if (held?.control.isConnected) held = note(held.control);
  };
  /** A press on a control is a place, even where the webview gave it no focus. */
  const onclick = (event: Event) => {
    const control = event.target instanceof Element ? event.target.closest<HTMLElement>(CONTROL) : null;
    if (control !== null && control.closest(KEPT) !== null && control.closest(".tf-modal") === null) held = note(control);
  };
  const onkey = (event: KeyboardEvent) => {
    if (event.key === "Shift" || event.key === "Meta" || event.key === "Control" || event.key === "Alt") return;
    byKey = true;
    refresh();
    endWait();
  };
  const onpointer = (event: Event) => {
    byKey = false;
    endWait();
    // A press on nothing leaves no place, whether or not anything had focus to lose.
    const target = event.target;
    if (target instanceof Element && target.closest(KEPT) !== null && target.closest(".tf-modal") === null && target.closest(CONTROL) === null) held = null;
    else refresh();
  };

  const endWait = () => {
    if (waiting !== null) stop(waiting);
    waiting = null;
  };

  /** Heading, once the new page draws one, if nothing took focus by then. */
  const toHeading = () => {
    let started = performance.now();
    const step = () => {
      waiting = null;
      // A launcher closing as it moves: it gives focus back first. One that stays is the page asking something, and the wait ends.
      if (dialogUp()) {
        if (performance.now() - started < WAIT_FOR_HEADING) waiting = later(step);
        return;
      }
      // The nav link it was pressed on is no place to read from: `Tab` goes on into the page (F-052). Nor is the pane a move by key left behind: at `#/assets` it holds nothing, and `↓` would be dead.
      const at = document.activeElement;
      if (!focusFree() && at?.closest(".tf-nav") == null && !at?.matches(".pane")) return;
      if (placeRestoring()) {
        started = performance.now();
        waiting = later(step);
        return;
      }
      const heading = headingOf(shownPage());
      if (heading !== null) heading.focus({ preventScroll: true });
      else if (performance.now() - started < WAIT_FOR_HEADING) waiting = later(step);
      else document.querySelector<HTMLElement>(".tf-main")?.focus({ preventScroll: true });
    };
    waiting = later(step);
  };

  const onhash = () => {
    // What had focus belongs to the page it was on. The way back to it is `return-place.ts`'s.
    held = null;
    endWait();
    if (byKey) toHeading();
  };

  const watcher = new MutationObserver(soon);
  watcher.observe(document.body, { childList: true, subtree: true, attributes: true, attributeFilter: ["disabled"] });
  document.addEventListener("focusin", onfocusin);
  document.addEventListener("focusout", onfocusout);
  window.addEventListener("keydown", onkey, true);
  window.addEventListener("pointerdown", onpointer, true);
  window.addEventListener("click", onclick, true);
  window.addEventListener("hashchange", onhash);
  return () => {
    watcher.disconnect();
    document.removeEventListener("focusin", onfocusin);
    document.removeEventListener("focusout", onfocusout);
    window.removeEventListener("keydown", onkey, true);
    window.removeEventListener("pointerdown", onpointer, true);
    window.removeEventListener("click", onclick, true);
    window.removeEventListener("hashchange", onhash);
    if (checking !== null) stop(checking);
    endWait();
    held = null;
  };
}
