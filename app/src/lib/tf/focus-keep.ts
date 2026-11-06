/**
 * **The focus keeper** (design system, The press rule): a press never drops the reader's place on the body.
 *
 * The shell installs one keeper over the page and the Connections drawer. It notes what holds focus there, and when that goes away or is
 * disabled and focus has nowhere else to be, it hands focus on: the same control in that row, else the same control in
 * the row that took its place (or the one before, at the end of a list), else that row itself, because a second `Enter`
 * must repeat the press or do nothing, and never open whatever that row happens to lead with. `Tab` goes on into the row.
 * With no such row: the same words on the page, else the page's heading.
 *
 * A move made by key with no place to return to focuses the page's heading (F-052), so the next `Tab` starts on the
 * page and not back at the top bar.
 *
 * It stays out of the way of what knows better: an open dialog owns focus and gives it back itself, and a return to a
 * place is `return-place.ts`'s. A page that puts focus somewhere itself, on the same press, has got there first, because
 * the keeper only acts a frame later, and only when focus is free.
 *
 * A pressed button doesn't take focus in every webview, so a press counts as being on that control. A dialog it opened
 * then has nothing to give focus back to, and the keeper does that once the dialog has gone (`shared-radio-enter`).
 */
import { placeRestoring } from "./return-place";

/** What counts as a row, for finding the one that took a row's place. */
const ROW = "tr, li, [data-row], [data-entity], .tf-rowhost, .tf-ib, .tf-r, .tf-lk";
/** Where the keeper looks after focus: the page, and the drawer that floats over it. */
const KEPT = ".tf-main, .tf-drawer";
const CONTROL = 'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), summary, [tabindex]:not([tabindex="-1"])';
/** How long a move by key waits for its page to draw a heading, in ms. */
const WAIT_FOR_HEADING = 1500;

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
  /** The control's place among the row's. */
  nth: number;
  list: Element | null;
  /** The row's place in its list. */
  at: number;
  key: string | null;
  parent: Element | null;
  /** The drawer it was in: what is left of it takes focus before the page behind it does. */
  drawer: Element | null;
}

function note(control: HTMLElement): Held {
  const row = control.closest(ROW);
  const list = row?.parentElement ?? null;
  return {
    control,
    said: saidBy(control),
    row,
    nth: row === null ? 0 : controlsIn(row).indexOf(control),
    list,
    at: row !== null && list !== null ? [...list.children].indexOf(row) : -1,
    key: row === null ? null : rowKey(row),
    parent: control.parentElement,
    drawer: control.closest(".tf-drawer"),
  };
}

/** A row that holds something to focus, at or nearest before `at` in `list`. */
function rowAt(list: Element, at: number): Element | null {
  const rows = [...list.children].filter((child) => child.matches(ROW) && controlsIn(child).length > 0);
  if (rows.length === 0) return null;
  return [...list.children].slice(at).find((child) => rows.includes(child)) ?? rows.at(-1)!;
}

/** In `row`, the control saying `said`; `nth` when the row is the one it was in and it says something else now; else the row. */
function inRow(row: Element, held: Held, sameRow: boolean): HTMLElement | null {
  const controls = controlsIn(row);
  const same = controls.find((el) => saidBy(el) === held.said);
  if (same !== undefined) return same;
  if (sameRow && controls.length > 0) return controls[Math.min(Math.max(held.nth, 0), controls.length - 1)]!;
  if (controls.length === 0 || !(row instanceof HTMLElement)) return null;
  row.tabIndex = -1;
  return row;
}

/** The page shown in `.tf-main`. */
function shownPage(): Element | null {
  return document.querySelector(".tf-main > .tf-kept:not([hidden])") ?? document.querySelector(".tf-main");
}

/** The page's heading, ready to take focus. It isn't a stop on the way through the page, so a `Tab` doesn't land on it. */
function headingOf(page: Element | null): HTMLElement | null {
  const heading = page?.querySelector<HTMLElement>("h1") ?? null;
  if (heading !== null && !heading.hasAttribute("tabindex")) heading.setAttribute("tabindex", "-1");
  return heading;
}

/** Where focus goes now `held` has gone. */
function nextFor(held: Held): HTMLElement | null {
  if (held.row !== null) {
    const list = held.list?.isConnected ? held.list : null;
    if (held.row.isConnected) {
      const own = inRow(held.row, held, true);
      if (own !== null) return own;
    }
    if (list !== null) {
      // Redrawn: the row that says it is the same one.
      const again = held.key === null ? undefined : [...list.children].find((child) => child.matches(ROW) && rowKey(child) === held.key);
      if (again !== undefined) {
        const own = inRow(again, held, true);
        if (own !== null) return own;
      }
      const took = rowAt(list, held.at);
      if (took !== null) {
        const own = inRow(took, held, false);
        if (own !== null) return own;
      }
    }
  } else if (held.parent?.isConnected) {
    const near = controlsIn(held.parent);
    const same = near.find((el) => saidBy(el) === held.said);
    if (same !== undefined) return same;
    // Disabled where it stands: what follows it, else what is before it.
    if (held.control.isConnected) {
      const after = near.find((el) => held.control.compareDocumentPosition(el) & Node.DOCUMENT_POSITION_FOLLOWING);
      if (after !== undefined) return after;
      if (near.length > 0) return near.at(-1)!;
    }
  }
  if (held.drawer?.isConnected) return controlsIn(held.drawer)[0] ?? null;
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
  return at.matches(".tf-main") || at.closest("[hidden]") !== null;
}

const dialogUp = (): boolean => document.querySelector(".tf-modal") !== null;

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
    if (held === null || up || placeRestoring() || !focusFree(held.control)) return;
    const gone = fell(held);
    if (!gone && !closed) return;
    const next = gone ? nextFor(held) : held.control;
    held = null;
    next?.focus({ preventScroll: true });
  };
  const soon = () => {
    if (held !== null && checking === null) checking = later(check);
  };

  const onfocusin = (event: Event) => {
    const target = event.target;
    // A dialog drawn inside a page gives focus back itself. What stays noted is what opened it, in case that goes with the answer.
    if (!(target instanceof HTMLElement) || target.closest(KEPT) === null || target.closest(".tf-modal") !== null) return;
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
      if (!focusFree()) return;
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
