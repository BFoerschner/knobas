/**
 * A grouped folder's bar in a comparison (ux26, the decision list's section 9): each state's words and tone in one row
 * whose node never changes; *Render with* only where two renderers take the folder; *Run* primary only while nothing is
 * kept.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { FolderRenderer } from "../../ipc/app";
import { FolderCompare, laneOf, type FolderCompareAt } from "../../render/folder-compare.svelte";
import { fakeCompareBackend } from "../../render/folder-compare.test-support";
import TfDiffFolder from "./TfDiffFolder.svelte";

const SHA = "4f2a91c0d2b9e8a7f6c5d4b3a2918f7e6d5c4b3a";
const WT = "~working-tree";
const SITE: FolderRenderer = { id: "site", name: "Low-code docs", command: "lowcode {path} --out {outdir}", holds: [], folders: ["flows"], output: "folder", timeout_secs: 30 };
const DIAGRAM: FolderRenderer = { ...SITE, id: "diagram", name: "Flow diagram" };

let target: HTMLElement;
let stops: (() => void)[] = [];
beforeEach(() => {
  target = document.createElement("div");
  document.body.appendChild(target);
});
afterEach(() => {
  vi.useRealTimers();
  for (const stop of stops) stop();
  stops = [];
  target.remove();
});
const settle = async () => {
  for (let n = 0; n < 12; n += 1) await Promise.resolve();
  flushSync();
};

function draw(kept: (fake: ReturnType<typeof fakeCompareBackend>) => void = () => {}, renderers: FolderRenderer[] = [SITE], lists = false) {
  const fake = fakeCompareBackend();
  kept(fake);
  const at: FolderCompareAt = { entityId: "repo", renderer: SITE, folder: "flows/card-capture", before: SHA, after: WT };
  let compare!: FolderCompare;
  let view!: ReturnType<typeof mount>;
  const stop = $effect.root(() => {
    compare = new FolderCompare(() => at, fake.ports);
  });
  view = mount(TfDiffFolder, {
    target,
    props: { folder: at.folder, renderer: SITE, renderers, compare, lists, ends: { old: "main at 4f2a91c", new: "your working tree" }, onreal: () => {} },
  });
  stops.push(() => unmount(view), stop);
  flushSync();
  return { fake, compare };
}
const bar = () => target.querySelector<HTMLElement>(".tf-madebar")!;
const status = () => bar().querySelector("[role=status]")!.textContent;
const run = () => bar().querySelector<HTMLButtonElement>(".tf-madebar-run")!;

test("nothing kept: Not rendered yet, Run is primary, and the note box stands", async () => {
  draw();
  await settle();
  expect(status()).toBe("Not rendered yet for both sides.");
  expect(run().textContent).toBe("Run");
  expect(run().classList.contains("tf-primary")).toBe(true);
  expect(target.querySelector(".tf-dfnote")!.textContent).toBe("Its generated files show here when both sides are made.");
  expect(bar().querySelector("select"), "one renderer: no Render with").toBeNull();
});

test("one side kept: the other side is named, and Run is no longer primary", async () => {
  draw((fake) => fake.keep(laneOf(SHA), "a", true, "flows/card-capture"));
  await settle();
  expect(status()).toBe("Not rendered yet for your working tree.");
  expect(run().classList.contains("tf-primary")).toBe(false);
  expect(run().textContent).toBe("Run again");
});

test("the words change through running and made, the bar's node and box do not", async () => {
  const { fake, compare } = draw((fake) => fake.keep(laneOf(SHA), "a", true, "flows/card-capture"));
  await settle();
  const node = bar();
  compare.press();
  await settle();
  fake.progress(laneOf(WT), { state: "copying", done: 120, total: 412 }, "flows/card-capture");
  await settle();
  expect(status()).toBe("Before: kept. After: copying 120 of 412 files from your clone…");
  expect(run().textContent).toBe("Stop");
  expect(bar().getAttribute("aria-busy")).toBe("true");
  fake.answers.set("a\nb", { files: [], same: 13, eol_only: 3, before_files: 13, after_files: 13 });
  fake.land(laneOf(WT), "b", "flows/card-capture");
  await settle();
  expect(bar()).toBe(node);
  expect(status()).toBe("Low-code docs made the same 13 files on both sides. 3 more differ only in line endings.");
  expect(target.querySelector(".tf-dfnote")!.textContent).toBe("Its generated files are the same on both sides.");
});

test("made from an earlier state of the working tree turns the bar warn", async () => {
  draw((fake) => {
    fake.keep(laneOf(SHA), "a", true, "flows/card-capture");
    fake.keep(laneOf(WT), "old", false, "flows/card-capture");
  });
  await settle();
  expect(status()).toBe("Made from an earlier state of your working tree.");
  expect(bar().classList.contains("tf-warn")).toBe(true);
  expect(run().textContent).toBe("Run again");
});

test("a failure that answers a press of Run here shows what it said under the bar", async () => {
  const { fake } = draw();
  await settle();
  run().click();
  await settle();
  fake.fail(laneOf(WT), { why: "failed", status: 2, stderr: "lowcode: no project file\n" }, "flows/card-capture");
  fake.land(laneOf(SHA), "a", "flows/card-capture");
  await settle();
  expect(bar().classList.contains("tf-err")).toBe(true);
  expect(status()).toBe("After: the command exited with status 2.");
  expect(target.querySelector(".tf-dffail pre")!.textContent).toBe("lowcode: no project file\n");
});

test("what a failed side said stays under the bar while it runs again, until the next answer replaces it", async () => {
  vi.useFakeTimers();
  const { fake } = draw();
  await settle();
  run().click();
  await settle();
  fake.land(laneOf(SHA), "a", "flows/card-capture");
  fake.fail(laneOf(WT), { why: "failed", status: 2, stderr: "lowcode: no project file\n" }, "flows/card-capture");
  await settle();
  const block = target.querySelector(".tf-dffail");
  expect(block).not.toBeNull();
  vi.advanceTimersByTime(2100);
  run().click();
  await settle();
  expect(run().textContent).toBe("Stop");
  expect(target.querySelector(".tf-dffail"), "held while the side runs again").toBe(block);
  fake.land(laneOf(WT), "b", "flows/card-capture");
  await settle();
  expect(target.querySelector(".tf-dffail")).toBeNull();
  vi.useRealTimers();
});

test("Render with is offered where two renderers take the folder", async () => {
  draw(() => {}, [SITE, DIAGRAM]);
  await settle();
  const select = bar().querySelector("select")!;
  expect([...select.options].map((o) => o.textContent)).toEqual(["Low-code docs", "Flow diagram"]);
  expect(select.value).toBe("site");
});
