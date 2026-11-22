/**
 * The comb's one ticker (ADR-0056): thirty ticks that count a hold down. Every comb on the page and the status bar's
 * registers its node here; one interval of 250 ms runs only while something counts and sets `--n` (the ticks still
 * standing) and the seconds text on the registered nodes directly, so a tick re-renders no component and lays out
 * nothing. Below six ticks the comb takes the warn tone (`tf-pw-low`). A count of 60 seconds keeps thirty ticks, two
 * seconds each.
 */
import { secondsLabel } from "../../time/format";
import { HOLD_SECS } from "./hold";

/** One counted node. */
interface Counted {
  /** The comb, if drawn. */
  comb: HTMLElement | null;
  /** The text that says the seconds left, if drawn. */
  left: HTMLElement | null;
  /** When the count ends, in `Date.now()` milliseconds. */
  until: number;
  /** The length of a full count, in seconds: 30 for a hold, a code's period for a code. */
  span: number;
  /** How the seconds read: `24s`. */
  words: (secs: number) => string;
  /** Told once when the count ends. */
  ended?: (() => void) | undefined;
}

const counted = new Set<Counted>();
let timer: ReturnType<typeof setInterval> | null = null;

/** The ticks still standing out of thirty for `secs` left of a `span`-second count. */
export function ticks(secs: number, span: number = HOLD_SECS): number {
  if (secs <= 0) return 0;
  return Math.min(30, Math.ceil((secs * 30) / span));
}

/** `24s`: a length in the app's one spelling (`secondsLabel`). */
export const seconds = (secs: number): string => secondsLabel(secs);

function paint(c: Counted, now: number): boolean {
  const secs = Math.max(0, Math.ceil((c.until - now) / 1000));
  const n = ticks(secs, c.span);
  if (c.comb) {
    c.comb.style.setProperty("--n", String(n));
    c.comb.classList.toggle("tf-pw-low", n > 0 && n < 6);
  }
  if (c.left) c.left.textContent = c.words(secs);
  return secs > 0;
}

function tick(): void {
  const now = Date.now();
  for (const c of [...counted]) {
    if (!paint(c, now)) {
      counted.delete(c);
      c.ended?.();
    }
  }
  if (counted.size === 0 && timer !== null) {
    clearInterval(timer);
    timer = null;
  }
}

/**
 * Count `comb` and `left` down to `until`. Answers the way to stop counting them (the node left the page). Painted at
 * once, so the first frame is right.
 */
export function count(c: Counted): () => void {
  counted.add(c);
  paint(c, Date.now());
  if (timer === null) timer = setInterval(tick, 250);
  return () => {
    counted.delete(c);
    if (counted.size === 0 && timer !== null) {
      clearInterval(timer);
      timer = null;
    }
  };
}

/** How many nodes count now (tests). */
export const counting = (): number => counted.size;
