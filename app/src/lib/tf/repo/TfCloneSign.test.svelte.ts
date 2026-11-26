/**
 * The clone's sign (the clone-path pass, 2026-11-16): *Local* or *Remote* with how the clone stands, its path in a
 * bubble that opens on hover and on focus, pinned by a press, with *Copy path* and *Open in* the default application.
 * The path is the sign's description and one run of text, open or closed, for a screen reader and the desktop witness.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { CheckoutSettings, CheckoutView, OpenAt } from "../../ipc/entity";
import { drawsReading } from "../../shell/test-witness";
import { toasts } from "../../shell/toasts.svelte";
import { createLayers, type Layers } from "../layers.svelte";
import { createApps, type Apps } from "../openin/apps.svelte";
import TfCloneSign from "./TfCloneSign.svelte";

const REPO = "gitea:tidewater/payout-service";
const LONG = String.raw`C:\Users\someone\source\repos\very\deep\nested\folders with spaces\the-repository-name-that-is-long`;

function view(path: string | null): CheckoutView {
  return {
    repo_entity_id: REPO,
    repo_url: null,
    path,
    found_by: path === null ? "nothing" : "scan",
    clones_root: null,
    clone_command: null,
    tree: path === null ? null : { status: "read", branch: "main", head: null, upstream: null, ahead: 0, behind: 2, changed: 3, untracked: 0, conflicted: 0 },
  };
}

function settings(defaultApplication: string | null): CheckoutSettings {
  return {
    clones_root: null,
    git: { status: "found", path: "/usr/bin/git" },
    default_application: defaultApplication,
    applications: [{ id: "vscode", name: "VS Code", command: "code {path}", file_command: null }],
  };
}

let target: HTMLElement;
let view_: ReturnType<typeof mount> | null = null;
let apps: Apps;
let layers: Layers;
let opened: [string, OpenAt][];
let copied: string[];
let copy: (text: string) => Promise<void>;

function draw(checkout: CheckoutView | undefined, defaultApplication: string | null = "vscode") {
  apps = createApps({
    checkoutSettings: () => Promise.resolve(settings(defaultApplication)),
    setCheckoutApplications: () => Promise.reject(new Error("not here")),
    openCheckout: (entityId, at) => {
      opened.push([entityId, at]);
      return Promise.resolve(["code", "x"]);
    },
  });
  view_ = mount(TfCloneSign, { target, props: { entityId: REPO, checkout, source: "Tidewater Gitea", apps, layers, copy } });
  flushSync();
}

async function settle() {
  for (let i = 0; i < 10; i += 1) await Promise.resolve();
  flushSync();
}

const face = () => target.querySelector<HTMLButtonElement>(".tf-signbtn")!;
const bubble = () => target.querySelector<HTMLElement>(".tf-signbubble")!;
const shown = () => !bubble().classList.contains("tf-sr");
const text = (el: Element | null) => el?.textContent?.replace(/\s+/g, " ").trim() ?? "";
const button = (words: string) => [...target.querySelectorAll<HTMLButtonElement>(".tf-signbubble button")].find((b) => text(b) === words) ?? null;

beforeEach(() => {
  target = document.createElement("div");
  document.body.append(target);
  layers = createLayers();
  opened = [];
  copied = [];
  copy = (value) => {
    copied.push(value);
    return Promise.resolve();
  };
  toasts.items = [];
});

afterEach(() => {
  vi.useRealTimers();
  if (view_) unmount(view_);
  view_ = null;
  target.remove();
});

test("nothing is drawn while the checkout has not answered", () => {
  draw(undefined);
  expect(target.querySelector(".tf-clonesign")).toBeNull();
});

test("a clone reads Local and how it stands, and its path is the sign's description, one run, while the bubble is closed", () => {
  draw(view(LONG));
  expect(text(face())).toBe("Local, main, 2 behind, 3 changed");
  expect(face().getAttribute("aria-expanded")).toBe("false");
  expect(shown()).toBe(false);
  const described = document.getElementById(face().getAttribute("aria-describedby")!);
  expect(text(described)).toBe(LONG);
  // The desktop witness waits for the path as a reading of its own (open-in-editor.sh).
  expect(drawsReading(target, LONG)).toBe(true);
  // No action is in the Tab order until the bubble shows.
  expect(target.querySelectorAll(".tf-signbubble button")).toHaveLength(0);
});

test("no clone reads Remote, and its bubble names the source it is read from, with nothing to press", () => {
  draw(view(null));
  expect(text(face())).toBe("Remote, not cloned");
  expect(face().classList.contains("tf-remote")).toBe(true);
  face().focus();
  flushSync();
  expect(shown()).toBe(true);
  expect(text(bubble())).toBe("Read from Tidewater Gitea: not cloned on this machine.");
  expect(target.querySelectorAll(".tf-signbubble button")).toHaveLength(0);
});

test("focus opens the bubble at once with the whole path, its folder in ink, and Tab goes on into its actions", async () => {
  draw(view(LONG));
  await settle();
  face().focus();
  flushSync();
  expect(shown()).toBe(true);
  expect(face().getAttribute("aria-expanded")).toBe("true");
  expect(text(target.querySelector(".tf-signbubble-drawn"))).toBe(LONG);
  expect(text(target.querySelector(".tf-signbubble-drawn b"))).toBe("the-repository-name-that-is-long");
  expect(text(target.querySelector(".tf-signbubble-state"))).toBe("main, 2 behind, 3 changed");
  // The path stays one run when open, too.
  expect(drawsReading(target, LONG)).toBe(true);
  // Focus moving into the bubble keeps it open; leaving the sign closes it.
  button("Copy path")!.focus();
  flushSync();
  expect(shown()).toBe(true);
  const after = document.createElement("button");
  document.body.append(after);
  after.focus();
  flushSync();
  expect(shown()).toBe(false);
  after.remove();
});

test("hover opens it after a rest, keeps it while the pointer crosses into it, and closes it once the pointer has gone", () => {
  vi.useFakeTimers();
  draw(view(LONG));
  const wrap = target.querySelector(".tf-clonesign")!;
  wrap.dispatchEvent(new PointerEvent("pointerenter"));
  vi.advanceTimersByTime(100);
  flushSync();
  expect(shown()).toBe(false);
  vi.advanceTimersByTime(60);
  flushSync();
  expect(shown()).toBe(true);
  // Out of the sign and back in (over the gap to the bubble) inside the grace: still open.
  wrap.dispatchEvent(new PointerEvent("pointerleave"));
  vi.advanceTimersByTime(150);
  wrap.dispatchEvent(new PointerEvent("pointerenter"));
  vi.advanceTimersByTime(300);
  flushSync();
  expect(shown()).toBe(true);
  wrap.dispatchEvent(new PointerEvent("pointerleave"));
  vi.advanceTimersByTime(210);
  flushSync();
  expect(shown()).toBe(false);
});

test("a press pins it open past the pointer, a second press closes it, and Esc closes it with focus left on the sign", () => {
  vi.useFakeTimers();
  draw(view(LONG));
  const wrap = target.querySelector(".tf-clonesign")!;
  face().click();
  flushSync();
  expect(shown()).toBe(true);
  wrap.dispatchEvent(new PointerEvent("pointerleave"));
  vi.advanceTimersByTime(500);
  flushSync();
  expect(shown()).toBe(true);
  face().click();
  flushSync();
  expect(shown()).toBe(false);

  face().focus();
  flushSync();
  expect(shown()).toBe(true);
  // The shell's Esc closes the topmost layer: the bubble registers as a panel while it shows.
  expect(layers.open).toHaveLength(1);
  button("Copy path")!.focus();
  layers.open.at(-1)!.close();
  flushSync();
  expect(shown()).toBe(false);
  expect(document.activeElement).toBe(face());
  expect(layers.open).toHaveLength(0);
});

test("a press elsewhere and a move to another address close a pinned bubble", () => {
  draw(view(LONG));
  face().click();
  flushSync();
  expect(shown()).toBe(true);
  document.body.dispatchEvent(new MouseEvent("mousedown", { bubbles: true }));
  flushSync();
  expect(shown()).toBe(false);
  face().click();
  flushSync();
  window.dispatchEvent(new HashChangeEvent("hashchange"));
  flushSync();
  expect(shown()).toBe(false);
});

test("Copy path puts the whole path on the clipboard and says Copied on the button for a moment", async () => {
  vi.useFakeTimers();
  draw(view(LONG));
  face().click();
  flushSync();
  button("Copy path")!.click();
  await settle();
  expect(copied).toEqual([LONG]);
  expect(button("Copied")).not.toBeNull();
  vi.advanceTimersByTime(2000);
  flushSync();
  expect(button("Copy path")).not.toBeNull();
});

test("a copy the clipboard refuses says so in a toast", async () => {
  copy = () => Promise.reject(new Error("the clipboard is not available"));
  draw(view(LONG));
  face().click();
  flushSync();
  button("Copy path")!.click();
  await settle();
  expect(toasts.items.map((toast) => toast.text)).toEqual(["Could not copy the path: the clipboard is not available"]);
  expect(button("Copy path")).not.toBeNull();
});

test("Open in runs the default application on the clone, as the Open in… menu's first item, and closes the bubble", async () => {
  draw(view(LONG));
  await settle();
  face().click();
  flushSync();
  button("Open in VS Code")!.click();
  await settle();
  expect(opened).toEqual([[REPO, { application: "vscode", file: null, line: null }]]);
  expect(shown()).toBe(false);
});

test("with no default application there is only Copy path", async () => {
  draw(view(LONG), null);
  await settle();
  face().click();
  flushSync();
  expect([...target.querySelectorAll(".tf-signbubble button")].map(text)).toEqual(["Copy path"]);
});
