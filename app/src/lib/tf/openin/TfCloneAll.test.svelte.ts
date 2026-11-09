/**
 * *Clone all…* (the smoothness pass, PR #2479): said first in a dialog, one
 * run however often it is pressed, by pointer or key, progress on the button,
 * one summary toast at the end with every failure and its reason, and a page
 * left and come back to that draws where the run is.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import type { CloneAllPlan, CloneAllState } from "../../ipc/entity";
import { toasts } from "../../shell/toasts.svelte";
import { createLayers } from "../layers.svelte";
import TfCloneAll from "./TfCloneAll.svelte";
import { createCloneAll, summary, type CloneAllPorts } from "./clone-all.svelte";

const PLAN: CloneAllPlan = {
  root: "~/code",
  names: ["ledger-api", "ops-runbooks"],
  sources: [{ source_id: "gitea", name: "Tidewater Git", repositories: 2 }],
  already: 1,
  skipped: [{ entity_id: "x:o/r", name: "o/r", reason: "no address to clone from" }],
};

const state = (over: Partial<CloneAllState> = {}): CloneAllState => ({ run: 1, total: 2, cloned: 0, adopted: 0, failed: [], current: [], finished: false, ...over });

let target: HTMLElement;
let view: ReturnType<typeof mount> | null = null;
let calls: { plan: number; start: number };
let hear: ((event: { payload: unknown }) => void) | null;

function ports(now: CloneAllState | null = null): CloneAllPorts {
  return {
    plan: async () => {
      calls.plan += 1;
      return PLAN;
    },
    start: async () => {
      calls.start += 1;
      return state({ current: ["ops-runbooks"] });
    },
    state: async () => now,
    listen: async (_event, handler) => {
      hear = handler;
      return () => (hear = null);
    },
  };
}

beforeEach(() => {
  calls = { plan: 0, start: 0 };
  hear = null;
  toasts.items.splice(0);
  target = document.createElement("div");
  document.body.appendChild(target);
});

afterEach(() => {
  if (view) unmount(view);
  view = null;
  target.remove();
});

async function settle() {
  for (let round = 0; round < 3; round += 1) {
    for (let i = 0; i < 10; i += 1) await Promise.resolve();
    await new Promise((resolve) => setTimeout(resolve, 0));
    flushSync();
  }
}

const text = (el: Element | null) => el?.textContent?.replace(/\s+/g, " ").trim() ?? "";
const button = () => target.querySelector<HTMLButtonElement>("button.tf-btn")!;
const dialog = () => document.querySelector(".tf-dialog");
const primary = () => document.querySelector<HTMLButtonElement>(".tf-dialog .tf-primary")!;

function drawn(store = createCloneAll(ports())) {
  view = mount(TfCloneAll, { target, props: { store, layers: createLayers() } });
  flushSync();
  return store;
}

test("a press says first what it would clone, from where, and what it leaves", async () => {
  drawn();
  button().click();
  await settle();
  expect(text(dialog()?.querySelector(".tf-dtitle, h2") ?? dialog())).toContain("Clone 2 repositories?");
  const said = text(dialog());
  expect(said).toContain("Into ~/code, from Tidewater Git (2)");
  expect(said).toContain("ledger-api, ops-runbooks");
  expect(said).toContain("1 repository already on this machine is left as it is.");
  expect(said).toContain("Not cloned: o/r (no address to clone from).");
  expect(primary().textContent).toBe("Clone 2 repositories");
});

test("pressed twice by pointer, then twice by key, one plan is read and one run starts", async () => {
  drawn();
  button().click();
  button().click();
  await settle();
  expect(calls.plan).toBe(1);
  primary().click();
  primary().click();
  await settle();
  expect(calls.start).toBe(1);
  expect(text(button())).toBe("Cloning 1 of 2…");

  // Enter twice on the button while the run goes reads nothing and starts nothing.
  button().focus();
  button().click();
  button().click();
  await settle();
  expect(calls.plan).toBe(1);
  expect(dialog()).toBeNull();
});

test("the run's end is one toast, with every failure and its reason", async () => {
  drawn();
  button().click();
  await settle();
  primary().click();
  await settle();
  hear!({ payload: state({ cloned: 1, current: [] }) });
  hear!({
    payload: state({ cloned: 1, failed: [{ entity_id: "g:t/ops-runbooks", name: "ops-runbooks", reason: "Authentication failed" }], finished: true }),
  });
  // The same end told again is not a second toast.
  hear!({
    payload: state({ cloned: 1, failed: [{ entity_id: "g:t/ops-runbooks", name: "ops-runbooks", reason: "Authentication failed" }], finished: true }),
  });
  await settle();
  const ends = toasts.items.filter((t) => t.text.startsWith("Cloned"));
  expect(ends.map((t) => t.text)).toEqual(["Cloned 1 of 2 repositories into ~/code. 1 failed (ops-runbooks: Authentication failed)."]);
  expect(ends[0]!.tone).toBe("err");
  expect(text(button())).toBe("Clone all…");
});

test("a page left and come back to draws the run where it is, and hears it to its end", async () => {
  const store = createCloneAll(ports(state({ cloned: 1, current: ["ops-runbooks"] })));
  view = mount(TfCloneAll, { target, props: { store, layers: createLayers() } });
  await settle();
  expect(text(button())).toBe("Cloning 2 of 2…");
  expect(button().title).toBe("Cloning ops-runbooks");
  unmount(view);
  view = mount(TfCloneAll, { target, props: { store, layers: createLayers() } });
  await settle();
  expect(text(button())).toBe("Cloning 2 of 2…");
  hear!({ payload: state({ cloned: 2, finished: true }) });
  await settle();
  expect(toasts.items.map((t) => t.text)).toContain("Cloned 2 repositories.");
});

test("a run that ended before the window watched it is not summed up again", async () => {
  drawn(createCloneAll(ports(state({ cloned: 2, finished: true }))));
  await settle();
  expect(toasts.items).toEqual([]);
  expect(hear).toBeNull();
});

test("the summary says what was already there and when there was nothing to do", () => {
  expect(summary(state({ total: 3, cloned: 2, adopted: 1, finished: true }), "~/code").text).toBe(
    "Cloned 2 repositories into ~/code; 1 was already there.",
  );
  expect(summary(state({ total: 0, finished: true }), null).text).toBe("Nothing to clone: every repository knobas knows is on this machine.");
});
