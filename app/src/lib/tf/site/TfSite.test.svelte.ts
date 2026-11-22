/**
 * A site's page (ADR-0050, decisions §7, §9, §10, §14, §16): when the native view is on screen, what the rectangle
 * says, the insets sent once per change, the bar's question and the first Esc's cap.
 */
import { flushSync, mount, tick, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import type { SiteInsets, SiteSetting, SiteStatus } from "../../ipc/site";
import { createLayers } from "../layers.svelte";
import { createSiteStore, type SiteStore } from "./site.svelte";
import TfSite, { type SitePagePorts } from "./TfSite.svelte";

let target: HTMLElement;
let view: ReturnType<typeof mount> | undefined;
let calls: string[];
let store: SiteStore;
let insets: SiteInsets;

const SETTING: SiteSetting = {
  source: "wiki",
  open_inside: true,
  base: "https://wiki.example",
  own: "https://wiki.example",
  allowed: [],
  suggested: [],
  kept_since: null,
  offered: true,
  persists: true,
  sign_in_user: null,
  own_sign_in: null,
  confined: false,
};

function status(over: Partial<SiteStatus> = {}): SiteStatus {
  return {
    source: "wiki",
    phase: "live",
    url: "https://wiki.example/display/PAY/Retry",
    title: "SEPA payout retry design",
    own: true,
    loading: false,
    can_back: true,
    can_forward: false,
    shown: false,
    keyboard: "site",
    asks_origin: null,
    failure: null,
    esc_again_ms: 500,
    fill: false,
    ...over,
  };
}

const ports: SitePagePorts = {
  show: (source, at, focus, esc) => void calls.push(`show ${source} ${at.top} focus=${focus} esc=${esc}`),
  hide: (source) => void calls.push(`hide ${source}`),
  open: (source) => void calls.push(`open ${source}`),
  browse: (url) => void calls.push(`browse ${url}`),
  answer: (source, origin, allow) => {
    calls.push(`answer ${source} ${origin} ${allow}`);
    return Promise.resolve(allow ? null : "https://idp.example/sso");
  },
  measure: () => insets,
};

let visits: { answer: (url: string) => undefined | null | { entity_id: string; kind: string }; see: (source: string, url: string) => Promise<{ entity_id: string; kind: string } | null>; leave: (source: string) => Promise<void> };
let named: Record<string, string>;
let seen: string[];

beforeEach(() => {
  named = {};
  seen = [];
  visits = {
    answer: () => undefined,
    see: (source, url) => {
      seen.push(`see ${source} ${url}`);
      const id = named[url];
      return Promise.resolve(id === undefined ? null : { entity_id: id, kind: "page" });
    },
    leave: (source) => {
      seen.push(`leave ${source}`);
      return Promise.resolve();
    },
  };
  target = document.createElement("div");
  document.body.append(target);
  calls = [];
  store = createSiteStore();
  store.hold([SETTING]);
  store.nameAll([{ id: "wiki", display_name: "Tidewater Confluence" }]);
  insets = { top: 93, right: 0, bottom: 28, left: 0 };
});

afterEach(() => {
  if (view) unmount(view);
  view = undefined;
  target.remove();
});

function draw(over: { visible?: boolean; back?: { hash: string; name: string; opened: boolean } | null } = {}) {
  const layers = createLayers();
  const props = $state({
    source: "wiki",
    visible: over.visible ?? true,
    back: over.back === undefined ? { hash: "#/ticket/jira%3APAY-231", name: "PAY-231", opened: true } : over.back,
    layers,
    ports,
    store,
    visits,
  });
  view = mount(TfSite, { target, props });
  flushSync();
  return { props, layers };
}

test("a live site is shown once at its insets, with the keyboard, and Esc's target", () => {
  store.update(status());
  draw();
  expect(calls).toEqual(["show wiki 93 focus=true esc=true"]);
});

test("a layer over it hides the view once and draws the veil; its close shows it once, the keyboard back", () => {
  store.update(status());
  const { layers } = draw();
  calls = [];
  const off = layers.add({ kind: "dialog", close: () => {} });
  flushSync();
  expect(calls).toEqual(["hide wiki"]);
  expect(target.querySelector(".tf-veil")?.getAttribute("data-state")).toBe("covered");
  expect(target.querySelector(".tf-veil h2")?.textContent).toBe("SEPA payout retry design");
  expect(target.textContent).toContain("Still open, as you left it.");
  off();
  flushSync();
  expect(calls).toEqual(["hide wiki", "show wiki 93 focus=true esc=true"]);
});

test("two layers are one hide and one show; a menu counts as a layer as a dialog does", () => {
  store.update(status());
  const { layers } = draw();
  calls = [];
  const menu = layers.add({ kind: "dialog", close: () => {} });
  const panel = layers.add({ kind: "panel", close: () => {} });
  flushSync();
  menu();
  flushSync();
  panel();
  flushSync();
  expect(calls).toEqual(["hide wiki", "show wiki 93 focus=true esc=true"]);
});

test("a page keep-alive hides calls hide, and a layer closing then shows nothing", () => {
  store.update(status());
  const { props, layers } = draw();
  calls = [];
  props.visible = false;
  flushSync();
  expect(calls).toEqual(["hide wiki"]);
  const off = layers.add({ kind: "dialog", close: () => {} });
  flushSync();
  off();
  flushSync();
  expect(calls).toEqual(["hide wiki"]);
});

test("a page mounted again shows its site and opens nothing", () => {
  store.update(status());
  draw();
  if (view) unmount(view);
  calls = [];
  draw();
  expect(calls).toEqual(["show wiki 93 focus=true esc=true"]);
  expect(calls.some((call) => call.startsWith("open"))).toBe(false);
});

test("with no site, the rectangle says it is not open and offers to open it", () => {
  draw();
  expect(calls).toEqual([]);
  expect(target.querySelector(".tf-veil h2")?.textContent).toBe("Tidewater Confluence is not open");
  expect(target.textContent).toContain("You will sign in on its own page, once.");
  target.querySelector<HTMLButtonElement>(".tf-veil .tf-primary")!.click();
  expect(calls).toEqual(["open wiki"]);
});

test("the question about another address names the host, and Continue here allows it", async () => {
  store.update(status({ phase: "asks", asks_origin: "https://idp.example", url: null }));
  draw();
  expect(target.querySelector(".tf-veil h2")?.textContent).toBe("Tidewater Confluence wants to continue at another address");
  expect(target.querySelector(".tf-veil-big")?.textContent).toBe("idp.example");
  const [go, browser] = target.querySelectorAll<HTMLButtonElement>(".tf-veil-acts button");
  expect(document.activeElement).toBe(go);
  go!.click();
  await Promise.resolve();
  expect(calls).toContain("answer wiki https://idp.example true");
  browser!.click();
  await Promise.resolve();
  await Promise.resolve();
  expect(calls).toContain("browse https://idp.example/sso");
});

test("a failure says what happened, and nothing is drawn as HTML", () => {
  store.update(status({ phase: "failed", url: null, failure: { kind: "certificate" }, title: "<b>x</b>" }));
  draw();
  expect(target.querySelector(".tf-veil")?.textContent).toContain("wiki.example's certificate is not trusted on this computer.");
  expect(target.querySelector(".tf-veil b")).toBeNull();
});

test("the bar names the site, the address in parts, and an address that is not the site's own", () => {
  store.update(status({ url: "http://sso.example/login?x=1", own: false }));
  draw();
  const bar = target.querySelector(".tf-sitebar")!;
  expect(bar.querySelector(".tf-site-name")?.textContent).toBe("Tidewater Confluence");
  expect(bar.querySelector(".tf-site-plain")?.textContent).toBe("http://");
  expect(bar.querySelector(".tf-site-host")?.textContent).toBe("sso.example");
  expect(bar.querySelector(".tf-site-path")?.textContent).toBe("/login?x=1");
  expect(bar.querySelector(".tf-chip.tf-warn")?.textContent).toBe("not Tidewater Confluence's own address");
});

test("Back to with two caps where Esc has somewhere to go; the first cap down while armed; none without an opener", () => {
  store.update(status());
  draw();
  const caps = () => [...target.querySelectorAll(".tf-sitebar .tf-esc kbd")].map((kbd) => kbd.className.includes("tf-kbd-down"));
  expect(target.querySelector(".tf-sitebar .tf-esc-t")?.textContent).toBe("Back to PAY-231");
  expect(caps()).toEqual([false, false]);
  store.arm("wiki", 500);
  flushSync();
  expect(caps()).toEqual([true, false]);
  expect(target.querySelector(".tf-sitebar .tf-sr")?.textContent).toBe("Esc again goes back to PAY-231");
  store.disarm("wiki");
  flushSync();
  expect(caps()).toEqual([false, false]);
  if (view) unmount(view);
  draw({ back: null });
  expect(target.querySelector(".tf-sitebar .tf-esc")).toBeNull();
});

test("the bar asks in its row, the first answer keeps the page and takes focus, Esc is that answer", async () => {
  store.update(status());
  draw();
  store.ask("wiki", { kind: "close" });
  flushSync();
  const bar = target.querySelector(".tf-sitebar.tf-asks")!;
  expect(bar.querySelector(".tf-site-says")?.textContent).toBe("This page has changes it has not saved.");
  const buttons = [...bar.querySelectorAll<HTMLButtonElement>("button")];
  expect(buttons.map((b) => b.textContent)).toEqual(["Keep it open", "Close anyway"]);
  expect(buttons[1]!.classList.contains("tf-danger")).toBe(true);
  expect(buttons[1]!.classList.contains("tf-primary")).toBe(false);
  await tick();
  await tick();
  expect(document.activeElement).toBe(buttons[0]);
  buttons[0]!.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
  flushSync();
  expect(store.asking.has("wiki")).toBe(false);
  expect(target.querySelector(".tf-sitebar.tf-asks")).toBeNull();
});

test("another address asks with three answers, the browser in the middle", () => {
  store.update(status());
  draw();
  store.ask("wiki", { kind: "address", url: "https://wiki.example/x", name: "Runbook: payout stuck" });
  flushSync();
  const bar = target.querySelector(".tf-sitebar.tf-asks")!;
  expect(bar.querySelector(".tf-site-says")?.textContent).toBe("This page has changes it has not saved. Open Runbook: payout stuck here anyway?");
  expect([...bar.querySelectorAll("button")].map((b) => b.textContent)).toEqual(["Keep this page", "Open in browser", "Open here anyway"]);
});

test("the insets go again only when they change", () => {
  let fire: (() => void) | null = null;
  const real = globalThis.ResizeObserver;
  globalThis.ResizeObserver = class {
    constructor(callback: () => void) {
      fire = callback;
    }
    observe() {}
    unobserve() {}
    disconnect() {}
  } as unknown as typeof ResizeObserver;
  try {
    store.update(status());
    draw();
    calls = [];
    // The rectangle resized with the window: its distances from the edges did not change, nothing is sent.
    fire!();
    expect(calls).toEqual([]);
    // The top bar wrapped: once.
    insets = { top: 120, right: 0, bottom: 28, left: 0 };
    fire!();
    fire!();
    expect(calls).toEqual(["show wiki 120 focus=false esc=true"]);
  } finally {
    globalThis.ResizeObserver = real;
  }
});

test("every kind of layer hides the site: a panel and a drawer as a dialog does", () => {
  for (const kind of ["panel", "drawer"] as const) {
    store.update(status());
    const { layers } = draw();
    calls = [];
    const off = layers.add({ kind, close: () => {} });
    flushSync();
    expect(calls, kind).toEqual(["hide wiki"]);
    off();
    flushSync();
    if (view) unmount(view);
    view = undefined;
  }
});

test("Open in knobas appears for an address the mirror names, and goes to its page", async () => {
  named["https://wiki.example/display/PAY/Retry"] = "confluence:page:88213";
  store.update(status());
  draw();
  expect(target.querySelector(".tf-sitebar a.tf-btn")).toBeNull();
  await Promise.resolve();
  await Promise.resolve();
  flushSync();
  const open = target.querySelector<HTMLAnchorElement>(".tf-sitebar a.tf-btn");
  expect(open?.textContent).toBe("Open in knobas");
  expect(open?.getAttribute("href")).toContain("confluence%3Apage%3A88213");
  expect(seen).toEqual(["see wiki https://wiki.example/display/PAY/Retry"]);
});

test("leaving the site's page refreshes what it stood on; showing it again does not", () => {
  store.update(status());
  const { props } = draw();
  expect(seen.filter((one) => one.startsWith("leave"))).toEqual([]);
  props.visible = false;
  flushSync();
  expect(seen.filter((one) => one.startsWith("leave"))).toEqual(["leave wiki"]);
  props.visible = true;
  flushSync();
  expect(seen.filter((one) => one.startsWith("leave"))).toEqual(["leave wiki"]);
});
