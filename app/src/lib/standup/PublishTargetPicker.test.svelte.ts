/**
 * What the publish-target picker asks `refused_writes` as the reader types
 * (#2063): each page once for the picker's life, the hits once the search
 * has stood still, the chosen parent at once.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { PublishTarget, WriteAsk } from "../ipc/entity";
import type { search as realSearch } from "../ipc/search";
import { SETTLE_MS } from "../shell/refused-writes";
import PublishTargetPicker from "./PublishTargetPicker.svelte";

/** The hits each query finds, by the text typed. */
const PAGES: Record<string, string[]> = {
  s: ["c:1", "c:2", "c:3"],
  st: ["c:2", "c:3", "c:4"],
  sta: ["c:3", "c:4"],
  stan: ["c:4", "c:5"],
  standup: ["c:5"],
};

const search = (async (query: { raw: string }) => ({
  groups: [
    {
      hits: (PAGES[query.raw] ?? []).map((entity_id) => ({ entity_id, kind: "page", source_id: "c", title: `Page ${entity_id}`, path: null })),
    },
  ],
})) as unknown as typeof realSearch;

let target: HTMLElement;
let view: ReturnType<typeof mount> | null = null;

beforeEach(() => {
  vi.useFakeTimers();
  target = document.createElement("div");
  document.body.appendChild(target);
});

afterEach(() => {
  if (view) unmount(view);
  view = null;
  target.remove();
  vi.useRealTimers();
});

function render(refuse: (ask: WriteAsk) => string | null, value: PublishTarget | null = null) {
  const sent: string[][] = [];
  const announced: (PublishTarget | null)[] = [];
  view = mount(PublishTargetPicker, {
    target,
    props: {
      sources: [{ id: "c", name: "Confluence" }],
      value,
      search,
      refusedWrites: async (asks: WriteAsk[]) => {
        sent.push(asks.map((ask) => ask.entity));
        return asks.flatMap((ask) => {
          const reason = refuse(ask);
          return reason === null ? [] : [{ ...ask, reason }];
        });
      },
      onchange: (next: PublishTarget | null) => announced.push(next),
    },
  });
  flushSync();
  return { sent, announced };
}

async function turns() {
  for (let turn = 0; turn < 10; turn += 1) await Promise.resolve();
  flushSync();
}

/** Type `text` into an empty box one key after another, each search landing before the next key. */
async function type(text: string) {
  const box = target.querySelector<HTMLInputElement>('input[aria-label="Parent page"]')!;
  for (let end = 1; end <= text.length; end += 1) {
    box.value = text.slice(0, end);
    box.dispatchEvent(new Event("input", { bubbles: true }));
    await turns();
  }
}

const hit = (entity: string) =>
  [...target.querySelectorAll<HTMLButtonElement>(".hits button")].find((b) => b.textContent?.includes(`Page ${entity}`))!;

test("typing asks nothing until the search stands still, then asks about the hits drawn (#2063)", async () => {
  const { sent } = render(() => null);
  await type("stan");
  expect(sent, "nothing is asked while the reader types").toEqual([]);

  await vi.advanceTimersByTimeAsync(SETTLE_MS);
  expect(sent).toEqual([["c:4", "c:5"]]);
});

test("a page already answered is not asked about again for the picker's life (#2063)", async () => {
  const { sent } = render(() => null);
  await type("stan");
  await vi.advanceTimersByTimeAsync(SETTLE_MS);
  await type("standup");
  await vi.advanceTimersByTimeAsync(SETTLE_MS);
  // Back to a query whose hits were answered, and one hit that was not.
  const box = target.querySelector<HTMLInputElement>('input[aria-label="Parent page"]')!;
  box.value = "sta";
  box.dispatchEvent(new Event("input", { bubbles: true }));
  await turns();
  await vi.advanceTimersByTimeAsync(SETTLE_MS);

  expect(sent).toEqual([["c:4", "c:5"], ["c:3"]]);
});

test("a hit picked before its answer lands is asked about at once, and a refusal announces no target, saying why (#2063)", async () => {
  const reason = "This Confluence user cannot add pages to Archive: it lacks the Add page permission";
  const { sent, announced } = render((ask) => (ask.entity === "c:5" ? reason : null));
  await type("standup");
  hit("c:5").click();
  flushSync();
  await turns();

  expect(sent).toEqual([["c:5"]]);
  expect(announced.at(-1)).toBeNull();
  expect(target.querySelector('[role="alert"]')?.textContent).toBe(reason);
  expect(hit("c:5").getAttribute("aria-disabled")).toBe("true");

  await vi.advanceTimersByTimeAsync(SETTLE_MS);
  expect(sent, "the settled ask finds the hit answered").toEqual([["c:5"]]);
});

test("a refused hit, once answered, cannot be picked (#2053, #2063)", async () => {
  const reason = "This Confluence user cannot add pages to Archive: it lacks the Add page permission";
  const { announced } = render((ask) => (ask.entity === "c:4" ? reason : null));
  await type("stan");
  await vi.advanceTimersByTimeAsync(SETTLE_MS);
  hit("c:4").click();
  flushSync();
  expect(hit("c:4").classList.contains("picked"), "a refused page is not picked").toBe(false);
  expect(announced.filter((next) => next !== null)).toEqual([]);

  hit("c:5").click();
  flushSync();
  expect(announced.at(-1)).toEqual({ source_id: "c", parent: "c:5" });
});

test("the stored parent is asked about at once (#2063)", async () => {
  const { sent } = render(() => null, { source_id: "c", parent: "c:9" });
  await turns();
  expect(sent).toEqual([["c:9"]]);
});

test("an ask that fails is no verdict, and is asked again once the hits change (#2063)", async () => {
  let down = true;
  const sent: string[][] = [];
  view = mount(PublishTargetPicker, {
    target,
    props: {
      sources: [{ id: "c", name: "Confluence" }],
      search,
      refusedWrites: async (asks: WriteAsk[]) => {
        sent.push(asks.map((ask) => ask.entity));
        if (down) throw new Error("not ready");
        return [];
      },
      onchange: () => {},
    },
  });
  flushSync();
  await type("sta");
  await vi.advanceTimersByTimeAsync(SETTLE_MS);
  expect(hit("c:3").getAttribute("aria-disabled")).toBeNull();

  down = false;
  await type("stan");
  await vi.advanceTimersByTimeAsync(SETTLE_MS);
  expect(sent).toEqual([
    ["c:3", "c:4"],
    ["c:4", "c:5"],
  ]);
});

test("an answer that lands after the picker is gone announces nothing (#2063)", async () => {
  let answer: (refused: never[]) => void = () => {};
  const announced: unknown[] = [];
  view = mount(PublishTargetPicker, {
    target,
    props: {
      sources: [{ id: "c", name: "Confluence" }],
      value: { source_id: "c", parent: "c:9" },
      search,
      refusedWrites: () => new Promise((resolve) => (answer = resolve)),
      onchange: (next: unknown) => announced.push(next),
    },
  });
  flushSync();
  await turns();
  unmount(view);
  view = null;
  answer([]);
  await turns();
  expect(announced).toEqual([]);
});
