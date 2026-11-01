/**
 * The ticket page card's width (spec #2396, #2402) through the app's own
 * mount over the whole fake IPC: a six-pixel separator on the card's right
 * edge drags the card's column between 240px and 720px, the width is browser
 * state read when the shell mounts, a double click fits the card to the width
 * at which nothing on it wraps, and below 900px the handle is not drawn.
 *
 * jsdom lays nothing out, so the fit's measure is stood in for: the card's
 * clone, laid out at its content's width with every line kept whole, answers
 * the width the test gives it. That nothing wraps at that width is the
 * screenshot's to show. Under `dev/` because only the harness may import the
 * harness.
 */
import { mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import App from "../../../App.svelte";
import { chosenJob } from "../../tf/job/chosen.svelte";
import { SIDE_KEY } from "../../tf/job/card-width.svelte";
import { demoHandlers, installFakeTauri } from "./fake-tauri";
import { until } from "../test-wait";

let target: HTMLDivElement;
let app: Record<string, unknown> | undefined;
let storage: Storage;

/** A storage for the width, which this runner's jsdom lacks (`test-no-storage.ts`), kept across a remount as a restart keeps it. */
function memoryStorage(): Storage {
  const held = new Map<string, string>();
  return {
    get length() {
      return held.size;
    },
    clear: () => held.clear(),
    getItem: (key) => held.get(key) ?? null,
    key: (at) => [...held.keys()][at] ?? null,
    removeItem: (key) => void held.delete(key),
    setItem: (key, value) => void held.set(key, value),
  };
}

const realMatchMedia = window.matchMedia;

/** The window at `width`: what `(max-width: 900px)` answers. */
function windowWidth(width: number) {
  window.matchMedia = ((query: string) => ({
    matches: query === "(max-width: 900px)" ? width <= 900 : false,
    media: query,
    onchange: null,
    addListener: () => {},
    removeListener: () => {},
    addEventListener: () => {},
    removeEventListener: () => {},
    dispatchEvent: () => false,
  })) as typeof window.matchMedia;
}

beforeEach(() => {
  storage = memoryStorage();
  Object.defineProperty(window, "localStorage", { value: storage, configurable: true, writable: true });
  installFakeTauri(demoHandlers());
  windowWidth(1440);
  target = document.createElement("div");
  document.body.append(target);
});

afterEach(() => {
  if (app) unmount(app);
  app = undefined;
  target.remove();
  history.replaceState(null, "", location.pathname + location.search);
  chosenJob.reset();
  window.matchMedia = realMatchMedia;
  vi.restoreAllMocks();
  Object.defineProperty(window, "localStorage", { value: undefined, configurable: true, writable: true });
});

async function openPage(): Promise<HTMLElement> {
  location.hash = "#/ticket/mock%3APAY-231";
  app = mount(App, { target, props: {} });
  const card = () => target.querySelector<HTMLElement>(".tf-kept:not([hidden]) article.tf-ticketcard");
  await until(() => card()?.querySelector(".tf-jobkey")?.textContent === "PAY-231", "the page never drew PAY-231");
  return card()!;
}

/** The card's column as the page lays it out: `--tk-side` on the two columns. */
function side(): string {
  return target.querySelector<HTMLElement>(".tf-kept:not([hidden]) .tf-ws")!.style.getPropertyValue("--tk-side");
}

function handle(card: HTMLElement): HTMLElement | null {
  return card.querySelector<HTMLElement>("[role='separator']");
}

/** A drag from where the handle sits to `x`, the card's left edge at 0 as jsdom lays it. */
function drag(on: HTMLElement, x: number) {
  on.dispatchEvent(new MouseEvent("pointerdown", { button: 0, clientX: 320, bubbles: true, cancelable: true }));
  window.dispatchEvent(new MouseEvent("pointermove", { clientX: x, bubbles: true }));
  window.dispatchEvent(new MouseEvent("pointerup", { clientX: x, bubbles: true }));
}

test("the handle is a separator on the card's edge, named for what it does, the column at its default width", async () => {
  const card = await openPage();
  const edge = handle(card)!;
  expect(edge).not.toBeNull();
  expect(edge.getAttribute("aria-orientation")).toBe("vertical");
  expect(edge.getAttribute("aria-label")).toBe("Card width. Drag to resize, double-click to fit.");
  expect(edge.getAttribute("aria-valuemin")).toBe("240");
  expect(edge.getAttribute("aria-valuemax")).toBe("720");
  expect(edge.getAttribute("aria-valuenow")).toBe("320");
  expect(side()).toBe("320px");
});

test("dragging the handle sets the column's width, and a width past either bound is clamped", async () => {
  const card = await openPage();
  drag(handle(card)!, 412);
  await until(() => side() === "412px", `the drag left the column at ${side()}`);
  expect(handle(card)!.getAttribute("aria-valuenow")).toBe("412");
  drag(handle(card)!, 1200);
  await until(() => side() === "720px", `past the cap the column is ${side()}`);
  drag(handle(card)!, 90);
  await until(() => side() === "240px", `under the floor the column is ${side()}`);
});

test("a move after the pointer is up drags nothing", async () => {
  const card = await openPage();
  drag(handle(card)!, 500);
  await until(() => side() === "500px", "the drag never landed");
  window.dispatchEvent(new MouseEvent("pointermove", { clientX: 300, bubbles: true }));
  await new Promise((resolve) => setTimeout(resolve, 20));
  expect(side()).toBe("500px");
});

test("the width survives a remount, as a restart reads it", async () => {
  const card = await openPage();
  drag(handle(card)!, 455);
  await until(() => side() === "455px", "the drag never landed");
  expect(storage.getItem(SIDE_KEY)).toBe("455");
  unmount(app!);
  app = undefined;
  const again = await openPage();
  expect(side()).toBe("455px");
  expect(handle(again)!.getAttribute("aria-valuenow")).toBe("455");
});

test("a kept width outside the bounds, or no number, is read clamped or as the default", async () => {
  storage.setItem(SIDE_KEY, "9000");
  await openPage();
  expect(side()).toBe("720px");
  unmount(app!);
  app = undefined;
  storage.setItem(SIDE_KEY, "wide");
  await openPage();
  expect(side()).toBe("320px");
});

/**
 * Stands in for layout: the card's clone answers `natural` for its width
 * once it is laid out at its content's width with every line kept whole, and
 * any other element answers nothing.
 */
function layout(natural: number) {
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
    const whole =
      this.style.width === "max-content" &&
      this.matches("article.tf-ticketcard") &&
      [...this.querySelectorAll<HTMLElement>("*")].every((el) => el.style.whiteSpace === "nowrap");
    const width = whole ? natural : 0;
    return { x: 0, y: 0, left: 0, top: 0, right: width, bottom: 0, width, height: 0, toJSON: () => ({}) } as DOMRect;
  });
}

test("a double click fits the card to the width at which nothing on it wraps, and the clone is gone", async () => {
  const card = await openPage();
  layout(431.2);
  handle(card)!.dispatchEvent(new MouseEvent("dblclick", { bubbles: true }));
  await until(() => side() === "433px", `the fit left the column at ${side()}`);
  expect(storage.getItem(SIDE_KEY)).toBe("433");
  expect(target.querySelectorAll("article.tf-ticketcard").length, "the measuring clone is removed").toBe(1);
});

test("a fit is capped at 720px", async () => {
  const card = await openPage();
  layout(980);
  handle(card)!.dispatchEvent(new MouseEvent("dblclick", { bubbles: true }));
  await until(() => side() === "720px", `the fit left the column at ${side()}`);
});

test("from the keyboard: the arrows step the width, Enter fits", async () => {
  const card = await openPage();
  const edge = handle(card)!;
  expect(edge.tabIndex).toBe(0);
  edge.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowRight", bubbles: true, cancelable: true }));
  await until(() => side() === "336px", `ArrowRight left ${side()}`);
  edge.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowLeft", bubbles: true, cancelable: true }));
  await until(() => side() === "320px", `ArrowLeft left ${side()}`);
  layout(500);
  edge.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true }));
  await until(() => side() === "501px", `Enter left ${side()}`);
});

test("at 860px wide the handle is not drawn", async () => {
  windowWidth(860);
  const card = await openPage();
  expect(handle(card)).toBeNull();
});
