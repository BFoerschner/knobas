/**
 * The window's zoom (2026-11-16, Björn: "make it possible to also zoom in the content with like strg&+ or mousewheel
 * and strg"; `docs/decisions/2026-11-16-zoom-is-the-window-s.md`).
 *
 * One level for the whole window, set through the webview's own zoom (WebView2's `ZoomFactor`, WKWebView's
 * `pageZoom`), so the page lays out as at a display scale of the display's times the level and every length stays in
 * one coordinate space. ⌘+ / ⌘= and ⌘− step it, ⌘0 puts it back to 100 % (Ctrl off the Mac, either on either, as the
 * shell's other keys), and so do ⌘ or Ctrl with the wheel, a trackpad's pinch, and on the Mac WebKit's gesture events.
 * A step lands on the webview once a frame at most however fast the wheel turns, and while one is on its way the next
 * waits for it (Fable's ruling, the ux18 pass, 2026-11-17): each level lays the whole page out again, and three wheel
 * notches 25 ms apart at CPU 6x on Builds were three layouts of 215 to 335 ms each, where the newest level alone is
 * what the reader asked for.
 *
 * Kept in `localStorage` (`knobas.zoom`), as the code face is: the level suits a display, and is this machine's.
 * `main.ts` sets it before the shell mounts, so the first frame is at the kept level.
 */
import { keptValue, windowStorage, type KeptStorage } from "./kept-value";

export const ZOOM_KEY = "knobas.zoom";
/** The steps, Chromium's own between the bounds: below 80 % the smallest type is under 9 px, above 200 % the window's
 * narrowest is 550 px wide. */
export const LEVELS = [0.8, 0.9, 1, 1.1, 1.25, 1.5, 1.75, 2] as const;
/** A wheel's pixels to one step: a mouse's notch is 100 (or 3 lines), a trackpad's pinch a few a frame. */
export const WHEEL_STEP = 50;
/** How long a trackpad's small deltas add up before they start again from nothing. */
const WHEEL_IDLE_MS = 400;
/** A pinch's scale, from where the last step was taken, to one more step. */
const PINCH_STEP = 1.12;
/** The longest the first draw waits for the webview to take the kept level. */
const BOOT_WAIT_MS = 250;
/** The longest a level on its way holds the next one, where the webview's resize is never heard. */
export const SETTLE_WAIT_MS = 600;

/** The level as a person reads it: `110 %`. */
export function percent(level: number): string {
  return `${Math.round(level * 100)} %`;
}

/** The step nearest `level`; 100 % for anything that is not a number. */
export function nearestLevel(level: number): number {
  if (!Number.isFinite(level) || level <= 0) return 1;
  let best: number = LEVELS[0];
  for (const at of LEVELS) if (Math.abs(at - level) < Math.abs(best - level)) best = at;
  return best;
}

/** `steps` steps from `level` (in when positive), held at the ends. */
export function stepped(level: number, steps: number): number {
  const at = LEVELS.indexOf(nearestLevel(level) as (typeof LEVELS)[number]);
  return LEVELS[Math.max(0, Math.min(LEVELS.length - 1, at + steps))]!;
}

type ZoomPress = Pick<KeyboardEvent, "key" | "code" | "metaKey" | "ctrlKey" | "altKey" | "defaultPrevented">;

/**
 * Whether a key press is a zoom key: `1` in, `-1` out, `0` back to 100 %, `null` neither. Read by the key, so a
 * German board's own + and − keys answer; by the keypad's code too. With ⌥ it is not: ⌥ with ⌘ types on a Mac
 * layout, and Ctrl+Alt is AltGr off the Mac. A key a control already answered is that control's.
 */
export function zoomKey(event: ZoomPress): 1 | -1 | 0 | null {
  if (!(event.metaKey || event.ctrlKey) || event.altKey || event.defaultPrevented) return null;
  if (event.key === "+" || event.key === "=" || event.code === "NumpadAdd") return 1;
  if (event.key === "-" || event.key === "_" || event.key === "−" || event.code === "NumpadSubtract") return -1;
  if (event.key === "0" || event.code === "Numpad0") return 0;
  return null;
}

/** What the level is set on: the webview, or a test's stand-in. A failure leaves the level as it was drawn. */
export type ApplyZoom = (level: number) => Promise<void> | void;

export interface Zoom {
  /** The level, 1 for 100 %. */
  readonly level: number;
  /** Bumped on every step asked for, one that changes nothing at an end too: what the indicator shows itself on. */
  readonly asked: number;
  /** Set a level (the nearest step). */
  set(level: number): void;
  /** `steps` steps in (positive) or out. */
  step(steps: number): void;
  /** Back to 100 %. */
  reset(): void;
  /**
   * A wheel with ⌘ or Ctrl (a pinch is one): the steps it makes, adding a trackpad's small deltas up to a step. Wheel
   * up is in.
   */
  wheelSteps(event: Pick<WheelEvent, "deltaY" | "deltaMode" | "timeStamp">): number;
  /** A WebKit gesture's scale since it started: the steps it makes since the last. */
  pinchSteps(scale: number, start: boolean): number;
  /** The kept level on the webview; awaited before the shell mounts. */
  boot(): Promise<void>;
  /** Tests only: 100 %, nothing kept. */
  forget(): void;
}

export interface ZoomPorts {
  storage: () => KeptStorage | null;
  apply: ApplyZoom;
  /** Runs `draw` before the next frame; the window's `requestAnimationFrame` when omitted. */
  frame?: (draw: () => void) => void;
  /**
   * Called as a level is sent to the webview; calls `done` once the page is laid out at it: its resize heard and a
   * frame drawn after it, or {@link SETTLE_WAIT_MS} at most, whether or not the webview's call has returned. The
   * window's when omitted.
   */
  settled?: (done: () => void) => void;
}

export function createZoom({ storage, apply, frame = nextFrame, settled = heardAndDrawn }: ZoomPorts): Zoom {
  const kept = keptValue(storage, ZOOM_KEY);
  const read = Number.parseFloat(kept.read() ?? "");
  let level = $state(Number.isNaN(read) ? 1 : nearestLevel(read));
  let asked = $state(0);
  /** The level on the webview, and whether a frame is asked to put the newest there. */
  // svelte-ignore state_referenced_locally
  // A snapshot on purpose: what the webview has trails the level asked for.
  let applied = level;
  let waiting = false;
  /** A level is on its way to the webview: the next one waits for it to be laid out. */
  let flying = false;
  let wheel = { sum: 0, at: -Infinity };
  let pinchFrom = 1;

  function land() {
    waiting = false;
    if (flying || applied === level) return;
    const to = level;
    applied = to;
    flying = true;
    let over = false;
    const done = () => {
      if (over) return;
      over = true;
      flying = false;
      // The steps that came meanwhile: the newest, once.
      if (applied !== level) ask();
    };
    // Listening from the moment the level is sent: WebView2 lays the page out, and its resize is heard, before the
    // call that set it returns (measured on Björn's PC: a listener added after it waited out the whole bound).
    settled(done);
    void Promise.resolve()
      .then(() => apply(to))
      // Refused: nothing is on its way, and the next step tries again.
      .catch(done);
  }

  function ask() {
    if (waiting || flying) return;
    waiting = true;
    frame(land);
  }

  function set(next: number) {
    asked++;
    const to = nearestLevel(next);
    if (to === level) return;
    level = to;
    if (to === 1) kept.clear();
    else kept.write(String(to));
    ask();
  }

  return {
    get level() {
      return level;
    },
    get asked() {
      return asked;
    },
    set,
    step(steps) {
      if (steps !== 0) set(stepped(level, steps));
    },
    reset() {
      set(1);
    },
    wheelSteps(event) {
      const px = event.deltaMode === 1 ? event.deltaY * 40 : event.deltaMode === 2 ? event.deltaY * 800 : event.deltaY;
      if (event.timeStamp - wheel.at > WHEEL_IDLE_MS || Math.sign(px) !== Math.sign(wheel.sum)) wheel.sum = 0;
      wheel.at = event.timeStamp;
      // A notch is a step however far it says it goes: a fast wheel's 300 px is still one turn of it.
      if (Math.abs(px) >= WHEEL_STEP) {
        wheel.sum = 0;
        return px < 0 ? 1 : -1;
      }
      wheel.sum += px;
      if (Math.abs(wheel.sum) < WHEEL_STEP) return 0;
      const steps = wheel.sum < 0 ? 1 : -1;
      wheel.sum = 0;
      return steps;
    },
    pinchSteps(scale, start) {
      if (start) pinchFrom = 1;
      if (!(scale > 0)) return 0;
      let steps = 0;
      while (scale / pinchFrom >= PINCH_STEP) {
        pinchFrom *= PINCH_STEP;
        steps++;
      }
      while (pinchFrom / scale >= PINCH_STEP) {
        pinchFrom /= PINCH_STEP;
        steps--;
      }
      return steps;
    },
    async boot() {
      if (level === 1) return;
      applied = level;
      try {
        // Never longer than a beat: a webview that does not answer draws at 100 %, not nothing.
        await Promise.race([apply(level), new Promise((done) => setTimeout(done, BOOT_WAIT_MS))]);
      } catch {
        // Drawn at 100 % then: the setting still says what was kept, and the next step sets it.
      }
    },
    forget() {
      level = 1;
      applied = 1;
      asked = 0;
      waiting = false;
      flying = false;
      wheel = { sum: 0, at: -Infinity };
      pinchFrom = 1;
      kept.clear();
    },
  };
}

/** The window's resize after a level was set, then the frame after it; {@link SETTLE_WAIT_MS} at most. */
function heardAndDrawn(done: () => void) {
  if (typeof window === "undefined") return done();
  let over = false;
  const finish = () => {
    if (over) return;
    over = true;
    window.removeEventListener("resize", finish);
    clearTimeout(timer);
    nextFrame(done);
  };
  window.addEventListener("resize", finish);
  const timer = setTimeout(finish, SETTLE_WAIT_MS);
}

function nextFrame(draw: () => void) {
  if (typeof requestAnimationFrame === "function") requestAnimationFrame(draw);
  else setTimeout(draw, 0);
}

/** The webview's own zoom; nothing outside Tauri (a test, the probes' headless Chrome). */
async function webviewZoom(level: number): Promise<void> {
  const { isTauri } = await import("@tauri-apps/api/core");
  if (!isTauri()) return;
  const { getCurrentWebview } = await import("@tauri-apps/api/webview");
  await getCurrentWebview().setZoom(level);
}

/** The window's zoom. */
export const zoom: Zoom = createZoom({ storage: windowStorage, apply: webviewZoom });

/**
 * ⌘ or Ctrl with the wheel, a pinch, and WebKit's gesture events, on the window, until the returned function is called.
 *
 * A wheel listener that may cancel holds each scroll's start for the main thread, so this one listens passively, and
 * a cancelling one is added only while ⌘ or Ctrl is down (and on the first wheel that carries one, should the key's
 * press have gone to another window): it goes when the key comes up, at the first wheel with neither, and when the
 * window loses focus. A scroll with no modifier never meets it. The wheel over something with a zoom of its own (a
 * PDF, `data-own-zoom`) steps that, through a `knobas-zoom` event whose detail is the steps.
 */
export function installZoomInput(zoom: Zoom, target: Window = window): () => void {
  let holding = false;
  const route = (event: Event, steps: number) => {
    if (steps === 0) return;
    const own = event.target instanceof Element ? event.target.closest("[data-own-zoom]") : null;
    if (own !== null) own.dispatchEvent(new CustomEvent("knobas-zoom", { detail: steps }));
    else zoom.step(steps);
  };
  const cancelling = (event: WheelEvent) => {
    if (!(event.ctrlKey || event.metaKey)) return release();
    if (event.defaultPrevented) return;
    event.preventDefault();
    route(event, zoom.wheelSteps(event));
  };
  const hold = () => {
    if (holding) return;
    holding = true;
    target.addEventListener("wheel", cancelling, { capture: true, passive: false });
  };
  const release = () => {
    if (!holding) return;
    holding = false;
    target.removeEventListener("wheel", cancelling, { capture: true });
  };
  // The cancelling listener, while it is on, answers first (it was added for the press) and cancels.
  const passive = (event: WheelEvent) => {
    if (!(event.ctrlKey || event.metaKey) || event.defaultPrevented || holding) return;
    route(event, zoom.wheelSteps(event));
    hold();
  };
  const down = (event: KeyboardEvent) => {
    if (event.key === "Control" || event.key === "Meta") hold();
  };
  const up = (event: KeyboardEvent) => {
    if ((event.key === "Control" || event.key === "Meta") && !event.ctrlKey && !event.metaKey) release();
  };
  // WebKit's trackpad pinch (the Mac): a gesture's scale since it started.
  const gesture = (event: Event) => {
    const scale = (event as Event & { scale?: number }).scale;
    if (typeof scale !== "number") return;
    event.preventDefault();
    route(event, zoom.pinchSteps(scale, event.type === "gesturestart"));
  };
  target.addEventListener("wheel", passive, { capture: true, passive: true });
  target.addEventListener("keydown", down, { capture: true, passive: true });
  target.addEventListener("keyup", up, { capture: true, passive: true });
  target.addEventListener("blur", release);
  target.addEventListener("gesturestart", gesture);
  target.addEventListener("gesturechange", gesture);
  return () => {
    release();
    target.removeEventListener("wheel", passive, { capture: true });
    target.removeEventListener("keydown", down, { capture: true });
    target.removeEventListener("keyup", up, { capture: true });
    target.removeEventListener("blur", release);
    target.removeEventListener("gesturestart", gesture);
    target.removeEventListener("gesturechange", gesture);
  };
}
