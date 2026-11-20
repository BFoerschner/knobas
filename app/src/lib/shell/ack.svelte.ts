/**
 * An opening's acknowledgement (ux21, 2026-11-19; Björn: "the toast in the bottom when opening something is really big
 * and a little irritating honestly. maybe something different to show it accepted the click?").
 *
 * When knobas hands something to another program (*Open in…* an application, the Diff tool, the browser), a small chip
 * says so where the press happened, beside the pointer or above the control a key pressed, for about a second: *Opening
 * retry.rs in VS Code*. It names the program and at most the item's last name, each cut in the middle when long, never a
 * path or a command line. It floats over the page, so nothing moves for it, and nothing can be pressed on it. A failure
 * takes it away at once and says what went wrong in a toast, as before.
 *
 * Where the press happened is read from the window, not handed down: {@link notePress} is the shell's capture listener
 * for `pointerdown` and `keydown`, so a store several calls under a click (`apps.run`) still knows where it was.
 */

/** How long a chip stands after the press, at least. */
export const ACK_MS = 1200;
/** How long it stands after a late answer, so a slow program's chip doesn't blink out the moment it answers. */
export const ACK_AFTER_MS = 400;
/** How long it fades before it goes (none under reduced motion, where the fade is off). */
export const ACK_FADE_MS = 160;
/** A chip whose answer never comes goes anyway; the failure, if it comes, is still a toast. */
export const ACK_GIVE_UP_MS = 15000;
/** A press older than this is not where an acknowledgement belongs: it stands where a toast would. */
const PRESS_FRESH_MS = 4000;
/** How far from the pointer a chip stands, and from a control. */
const POINTER_GAP = 12;
const CONTROL_GAP = 6;
/** Under this much room above, it stands below. */
const ROOM_ABOVE = 40;
/** The chip's widest (`toasts.css`, `.tf-ack`): past the window's right edge it hangs to the left of its point. */
export const ACK_MAX_W = 360;

/** Where a press landed: a point for a pointer, a box for a key. */
export interface Press {
  at: number;
  x: number;
  y: number;
  /** The control a key pressed; `null` for a pointer's press, at `x`, `y`. */
  box: { top: number; bottom: number; left: number; right: number } | null;
}

/** Where a chip stands, in CSS pixels from the window's edges. Only the sides it is pinned to are set; `centre` for none. */
export interface AckPlace {
  centre?: boolean;
  left?: number;
  right?: number;
  top?: number;
  bottom?: number;
}

export interface AckItem {
  id: number;
  text: string;
  place: AckPlace;
  leaving: boolean;
}

/** The one chip on screen, or none: a second opening takes the first one's place. */
export const acks = $state<{ item: AckItem | null }>({ item: null });

let nextId = 0;
let last: Press | null = null;
let timers: ReturnType<typeof setTimeout>[] = [];

function clearTimers() {
  for (const timer of timers) clearTimeout(timer);
  timers = [];
}

/** The shell's capture listener: a pointer's press is a point, `Enter` or `Space` the focused control's box. */
export function notePress(event: PointerEvent | KeyboardEvent, now = Date.now()): void {
  if (event.type === "pointerdown") {
    const pointer = event as PointerEvent;
    last = { at: now, x: pointer.clientX, y: pointer.clientY, box: null };
    return;
  }
  const key = (event as KeyboardEvent).key;
  if (key !== "Enter" && key !== " ") return;
  const focused = document.activeElement;
  if (!(focused instanceof HTMLElement) || focused === document.body) return;
  const box = focused.getBoundingClientRect();
  if (box.width === 0 && box.height === 0) return;
  last = { at: now, x: box.left, y: box.top, box: { top: box.top, bottom: box.bottom, left: box.left, right: box.right } };
}

/**
 * Where a chip stands for `press` in a window `width` by `height`: above and to the right of the pointer, or above the
 * control a key pressed, below either when there is no room above, and hung to the left of its point when the right
 * edge is near. With no fresh press, where a toast stands. Computed before it draws, so it never moves once drawn.
 */
export function placeFor(press: Press | null, width: number, height: number, now = Date.now(), band = false): AckPlace {
  if (band) return bandPlace(press, width, now);
  if (press === null || now - press.at > PRESS_FRESH_MS) return { centre: true, bottom: 44 };
  const { box } = press;
  let across: AckPlace;
  if (box === null) across = press.x + POINTER_GAP + ACK_MAX_W <= width ? { left: press.x + POINTER_GAP } : { right: Math.max(8, width - press.x + POINTER_GAP) };
  else across = box.left + ACK_MAX_W <= width ? { left: Math.max(8, box.left) } : { right: Math.max(8, width - box.right) };
  const top = box === null ? press.y : box.top;
  const bottom = box === null ? press.y : box.bottom;
  const gap = box === null ? POINTER_GAP : CONTROL_GAP;
  return top >= ROOM_ABOVE ? { ...across, bottom: height - top + gap } : { ...across, top: bottom + gap };
}

/** The top bar's height: while a site is shown, the one band of knobas's own pixels a chip can stand in (ADR-0050). */
export const TOP_BAND = 53;
/** A chip's height, near enough to keep it inside the band. */
const ACK_H = 26;

/**
 * Where a chip stands while a site's native view covers the page (decisions §7): above its control, clamped to the
 * top bar's band, never below, since below is the site's rectangle, which nothing knobas draws can stand over.
 */
export function bandPlace(press: Press | null, width: number, now = Date.now()): AckPlace {
  if (press === null || now - press.at > PRESS_FRESH_MS) return { centre: true, top: Math.round((TOP_BAND - ACK_H) / 2) };
  const { box } = press;
  const left = box === null ? press.x : box.left;
  const right = box === null ? press.x : box.right;
  const across: AckPlace = left + ACK_MAX_W <= width ? { left: Math.max(8, left) } : { right: Math.max(8, width - right) };
  const top = box === null ? press.y : box.top;
  return { ...across, top: Math.max(4, Math.min(top - CONTROL_GAP - ACK_H, TOP_BAND - ACK_H - 2)) };
}

/** Whether a site's view covers the page now, or is about to: chips stand in the top bar's band. Set by the shell. */
export const ackBand = { site: false };

/** `text` at most `max` characters, its middle given up for `…`: the end says which one it is, so it keeps more of it. */
export function cutMiddle(text: string, max: number): string {
  if (text.length <= max) return text;
  const head = Math.floor((max - 1) * 0.4);
  return `${text.slice(0, head)}…${text.slice(text.length - (max - 1 - head))}`;
}

/** The last name of a path inside a checkout or a folder: `src/sepa/retry.rs` is `retry.rs`. */
export function lastName(path: string): string {
  const trimmed = path.replace(/[\\/]+$/, "");
  return trimmed.slice(Math.max(trimmed.lastIndexOf("/"), trimmed.lastIndexOf("\\")) + 1);
}

/** The chip's words: *Opening retry.rs in VS Code*, *Opening in VS Code*, each name cut in the middle when long. */
export function openingWords(app: string, item: string | null = null): string {
  const name = cutMiddle(app.trim(), 32);
  return item === null || item.trim() === "" ? `Opening in ${name}` : `Opening ${cutMiddle(item.trim(), 28)} in ${name}`;
}

/** An acknowledgement on screen: its opening answered, or refused. */
export interface Ack {
  /** It opened: the chip stands out its time, then goes. */
  settle(): void;
  /** It didn't: the chip goes at once, and the caller says why in a toast. */
  fail(): void;
}

function leave(id: number, after: number) {
  const fade = () => {
    if (acks.item?.id !== id) return;
    acks.item.leaving = true;
    timers.push(setTimeout(() => acks.item?.id === id && (acks.item = null), ACK_FADE_MS));
  };
  if (after <= 0) fade();
  else timers.push(setTimeout(fade, after));
}

/**
 * Say an opening was taken, where the press was. Call it in the press, before the program is asked, so the chip is the
 * press's answer; then `settle` or `fail` it when the program answers.
 */
export function acknowledge(text: string, now = Date.now(), band = ackBand.site): Ack {
  clearTimers();
  const id = ++nextId;
  const width = typeof window === "undefined" ? 1440 : window.innerWidth;
  const height = typeof window === "undefined" ? 900 : window.innerHeight;
  acks.item = { id, text, place: placeFor(last, width, height, now, band), leaving: false };
  const shown = now;
  let answered = false;
  timers.push(setTimeout(() => !answered && leave(id, 0), ACK_GIVE_UP_MS));
  return {
    settle() {
      if (answered) return;
      answered = true;
      const left = shown + ACK_MS - Date.now();
      leave(id, left > 0 ? left : ACK_AFTER_MS);
    },
    fail() {
      if (answered) return;
      answered = true;
      if (acks.item?.id === id) acks.item = null;
    },
  };
}

/** Run `open`, acknowledged: the chip at once, gone on a refusal, which `open`'s caller still sees. */
export async function acknowledged<T>(text: string, open: () => Promise<T>, band = ackBand.site): Promise<T> {
  const ack = acknowledge(text, Date.now(), band);
  try {
    const value = await open();
    ack.settle();
    return value;
  } catch (cause) {
    ack.fail();
    throw cause;
  }
}

/** No chip, no press, no timers: a test's start, and the shell's chip host going. */
export function resetAcks(): void {
  clearTimers();
  acks.item = null;
  last = null;
}
