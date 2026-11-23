/** The value viewer (the XML view's design, 2026-11-23, sections 15 to 19). */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import TfValueViewer from "./TfValueViewer.svelte";
import type { PipeRun, PipeStep } from "../../ipc/entity";
import { guess } from "../../render/value-guess";

let target: HTMLElement;
let view: ReturnType<typeof mount> | null = null;
beforeEach(() => {
  target = document.createElement("div");
  document.body.appendChild(target);
});
afterEach(() => {
  if (view) unmount(view);
  view = null;
  target.remove();
});

const value = { from: 10, to: 40, read: "xml_text" as const, name: "MIME_DATA", field: "ORDER/MIME_EMBEDDED/MIME_DATA", length: 30, fileName: "terms.pdf", line: 12 };
const quiet = { highlight: () => ({ focus: () => {}, cancel: () => {} }) };
const text = (s: string): PipeRun => ({ outcome: "done", name: "value", output: { kind: "text", text: s } });
async function settle() {
  for (let i = 0; i < 6; i += 1) {
    await new Promise((resolve) => setTimeout(resolve, 0));
    flushSync();
  }
}
const words = (el: Element | null | undefined) => el?.textContent?.replace(/\s+/g, " ").trim() ?? "";

function draw(extra: Record<string, unknown>) {
  const run = vi.fn((steps: PipeStep[]) => Promise.resolve(text(`ran ${steps.map((s) => s.step).join(" ")}`)));
  const save = vi.fn(() => Promise.resolve("/Downloads/terms.pdf"));
  const keep = vi.fn(() => Promise.resolve([]));
  view = mount(TfValueViewer, { target: document.body, props: { value, path: "orders/a.xml", pipelines: [], run, save, keep, render: quiet, onclose: () => {}, ...extra } });
  flushSync();
  return { run, save, keep };
}

test("a chip's guess opens on its chain: the value taken out of the file, then the guessed steps, run once", async () => {
  const g = guess("JVBERi0xLjQKJcOkw7zDtsOfCjIgMCBvYmoKPDwvTGVuZ3RoIDMgMCBSL0Zp", { base64: true })!;
  const { run } = draw({ guess: g });
  await settle();
  expect(run).toHaveBeenCalledTimes(1);
  expect(run.mock.calls[0]![0]).toEqual([{ step: "take", from: 10, to: 40, read: "xml_text" }, { step: "base64_decode" }]);
  expect(words(document.querySelector(".tf-xv-flow"))).toContain("30 characters →");
  expect(words(document.querySelector(".tf-pipe .tf-acts"))).toContain("knobas's guess");
});

test("with no guess the value is drawn as text: value → Text", async () => {
  const { run } = draw({});
  await settle();
  expect(run.mock.calls[0]![0]).toEqual([{ step: "take", from: 10, to: 40, read: "xml_text" }]);
  expect(document.querySelector<HTMLSelectElement>(".tf-prenderer select")!.value).toBe("text");
  expect(words(document.querySelector(".tf-vstage .tf-code"))).toContain("ran take");
});

test("a step removed runs the chain again; a step that fails is red and its failure stands under the bar", async () => {
  const g = guess("JVBERi0xLjQKJcOkw7zDtsOfCjIgMCBvYmoKPDwvTGVuZ3RoIDMgMCBSL0Zp", { base64: true })!;
  const { run } = draw({ guess: g });
  await settle();
  run.mockImplementationOnce(() => Promise.resolve({ outcome: "failed", failure: { index: 1, name: "base64 decode", why: "error", error: "not base64", input: { size: 3, head: "?!?", binary: false } } }));
  document.querySelector<HTMLButtonElement>('.tf-pstep button[aria-label="Remove base64 decode"]')!.click();
  await settle();
  expect(run).toHaveBeenCalledTimes(2);
  expect(run.mock.calls[1]![0]).toEqual([{ step: "take", from: 10, to: 40, read: "xml_text" }]);
});

test("a failed step is said, with what it said and the head of its input", async () => {
  const run = vi.fn(() => Promise.resolve<PipeRun>({ outcome: "failed", failure: { index: 1, name: "gunzip", why: "error", error: "invalid gzip header", input: { size: 3, head: "abc", binary: false } } }));
  view = mount(TfValueViewer, { target: document.body, props: { value, path: "a.xml", pipelines: [], run, save: vi.fn(), keep: vi.fn(), render: quiet, onclose: () => {}, first: { steps: [{ step: "gunzip" }], end: null } } });
  await settle();
  expect(words(document.querySelector(".tf-stepfail"))).toContain("invalid gzip header");
  expect(document.querySelector(".tf-pstep.tf-pbad")).not.toBeNull();
});

test("Keep for this field keeps the chain as a named pipeline for the field's last two names in the file's ending", async () => {
  const g = guess("JVBERi0xLjQKJcOkw7zDtsOfCjIgMCBvYmoKPDwvTGVuZ3RoIDMgMCBSL0Zp", { base64: true })!;
  const { keep } = draw({ guess: g });
  await settle();
  [...document.querySelectorAll<HTMLButtonElement>("button")].find((b) => words(b) === "Keep for this field…")!.click();
  flushSync();
  expect(document.querySelector<HTMLInputElement>('input[aria-label="For the field"]')!.value).toBe("MIME_EMBEDDED/MIME_DATA");
  expect(document.querySelector<HTMLInputElement>('input[aria-label="In the files"]')!.value).toBe("*.xml");
  document.querySelector<HTMLFormElement>(".tf-xv-keep")!.requestSubmit();
  await settle();
  expect(keep).toHaveBeenCalledTimes(1);
  const list = (keep.mock.calls[0] as unknown as [{ name: string; steps: PipeStep[]; fields?: { field: string; files: string }[] }[]])[0];
  expect(list[0]!.steps).toEqual([{ step: "base64_decode" }]);
  expect(list[0]!.fields).toEqual([{ files: "*.xml", field: "MIME_EMBEDDED/MIME_DATA", end: "text" }]);
});

test("Download saves what the chain gave under the name the document gives beside the value", async () => {
  const { save } = draw({});
  await settle();
  [...document.querySelectorAll<HTMLButtonElement>("button")].find((b) => words(b) === "Download")!.click();
  await settle();
  expect(save).toHaveBeenCalledWith([{ step: "take", from: 10, to: 40, read: "xml_text" }], "terms.pdf");
});
