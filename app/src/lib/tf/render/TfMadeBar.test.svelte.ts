/** The made bar (ADR-0046): each state's words and colour on one row, the panel of what was left out, Stop. */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import type { FolderRenderView } from "../../ipc/app";
import TfMadeBar from "./TfMadeBar.svelte";

let target: HTMLElement;
let app: ReturnType<typeof mount> | null = null;
beforeEach(() => {
  target = document.createElement("div");
  document.body.append(target);
});
afterEach(() => {
  if (app) unmount(app);
  app = null;
  target.remove();
});

const NOW = new Date("2026-11-20T10:00:00Z");
const DRAWN = { token: "t", kind: { kind: "folder", files: 14, bytes: 2_400_000, left_out: 2 }, size: 2_400_000, commit: "abc1234ffff", from: "clone", took_ms: 3200, drawn_at: "2026-11-20T08:00:00Z" } as const;
const view = (over: Partial<FolderRenderView> = {}): FolderRenderView => ({
  renderer: "site",
  folder: "apps/order-flow",
  drawn: null,
  current: false,
  running: null,
  failure: null,
  from: "clone",
  commit: "def5678ffff",
  ...over,
});
const MADE = {
  files: 14,
  bytes: 2_400_000,
  leftOut: [
    { path: "latest", why: "link" },
    { path: ".git", why: "git" },
  ] as const,
  leftOutTotal: 2,
};

let pressed = 0;
let real = 0;
function draw(props: Record<string, unknown>) {
  pressed = 0;
  real = 0;
  app = mount(TfMadeBar, {
    target,
    props: { folder: "apps/order-flow", renderer: "site", name: "Low-code preview", onpress: () => (pressed += 1), onreal: () => (real += 1), now: () => NOW, view: null, ...props },
  });
  flushSync();
}
const status = () => target.querySelector(".tf-madebar [role='status']")!.textContent!;
const bar = () => target.querySelector(".tf-madebar")!;
const tone = () => (bar().classList.contains("tf-err") ? "err" : bar().classList.contains("tf-warn") ? "warn" : "info");

test("each state says itself in the bar's one row, in its colour", () => {
  const cases: [Record<string, unknown>, string, string][] = [
    [{ view: null }, "Asking Low-code preview…", "info"],
    [{ view: view({ running: { state: "queued" } }) }, "Low-code preview waits for another folder renderer to finish.", "info"],
    [{ view: view({ running: { state: "copying", done: 120, total: 2000 } }) }, "Copying 120 of 2,000 files from your clone…", "info"],
    [{ view: view({ running: { state: "running", elapsed_ms: 12_000 } }), elapsed: 12_000 }, "Running Low-code preview… 12s", "info"],
    [{ view: view({ running: { state: "keeping", done: 120, total: 2000 } }) }, "Keeping 120 of 2,000 files…", "info"],
    [{ view: view({ drawn: DRAWN, current: true }), made: MADE }, "Made by Low-code preview at abc1234, 14 files, 2.3 MB, 3.2s", "info"],
    [{ view: view({ drawn: DRAWN }), made: MADE }, "Made at abc1234 2 h ago. The folder is at def5678 now.", "warn"],
    // 2026-11-24: the folder is where it was, the tool changed since (its stamp is in the key).
    [{ view: view({ drawn: DRAWN, commit: "abc1234ffff" }), made: MADE }, "Made at abc1234 2 h ago, before Low-code preview changed.", "warn"],
    [{ view: view({ drawn: DRAWN, failure: { why: "failed", status: 2, stderr: "no project" } }), made: MADE }, "The last run failed. This is what it made at abc1234.", "err"],
    [{ view: view({ failure: { why: "failed", status: 2, stderr: "no project" } }) }, "Low-code preview could not make this folder.", "err"],
    [{ view: view({ failure: { why: "stopped" } }) }, "Low-code preview was stopped.", "info"],
    [{ view: view() }, "Low-code preview has not made this folder yet.", "info"],
  ];
  for (const [props, words, colour] of cases) {
    draw(props);
    expect(status()).toBe(words);
    expect(tone(), words).toBe(colour);
    // One row: it never wraps, and the buttons are there in every state.
    expect(target.querySelectorAll(".tf-madebar").length).toBe(1);
    expect([...target.querySelectorAll(".tf-madebar button")].map((b) => b.textContent).filter((t) => t === "Show the real files")).toHaveLength(1);
    unmount(app!);
    app = null;
  }
});

test("the checkout's changes are said in the made line's own slot", () => {
  draw({ view: view({ drawn: DRAWN, current: true }), made: MADE, changedInside: true });
  expect(status()).toBe("Made by Low-code preview at abc1234, 14 files, 2.3 MB, 3.2s. Your checkout's changes here are not in it.");
});

test("Stop while keeping stops; Run again runs; Show the real files says so", () => {
  draw({ view: view({ running: { state: "keeping", done: 1, total: 2 } }) });
  const run = target.querySelector<HTMLButtonElement>(".tf-madebar-run")!;
  expect(run.textContent).toBe("Stop");
  run.click();
  expect(pressed).toBe(1);
  [...target.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent === "Show the real files")!.click();
  expect(real).toBe(1);
});

test("N left out opens the list with each reason, and What it said the tool's stderr", () => {
  draw({ view: view({ drawn: DRAWN, current: true }), made: MADE });
  const left = [...target.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent === "2 left out")!;
  left.click();
  flushSync();
  const rows = [...target.querySelectorAll(".tf-madebar-left li")].map((li) => li.textContent);
  expect(rows).toEqual(["latesta link, not followed", ".gita .git folder"]);
  unmount(app!);
  draw({ view: view({ failure: { why: "failed", status: 2, stderr: "no project file" } }) });
  [...target.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent === "What it said")!.click();
  flushSync();
  expect(target.querySelector(".tf-madebar-more pre")?.textContent).toBe("no project file");
});
