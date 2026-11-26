/**
 * An answer's body in the API explorer (#2533) with the app's reads mocked: a JSON answer in the compact JSON view with
 * its jq query and no rail, an XML answer in the compact XML view with XPath, HTML only ever as its text, *Render as*,
 * *Pipeline* run over the kept answer in the backend (never over text the page sends), and a failed step said.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { BodyView } from "./oas-answer";

const bodies = vi.hoisted(() => new Map<number, string>());
const reads = vi.hoisted(() => [] as [number, boolean][]);
const piped = vi.hoisted(() => [] as { id: number; name: string; steps: unknown[] }[]);
const jq = vi.hoisted(() => ({ opened: [] as unknown[], asked: [] as [number, string][] }));
vi.mock("../../ipc/api", async (original) => ({
  ...(await original<typeof import("../../ipc/api")>()),
  apiAnswerBody: async (id: number, pretty: boolean) => {
    const body = bodies.get(id);
    if (body === undefined) throw { code: "not_found", message: "The answer is no longer kept." };
    reads.push([id, pretty]);
    // As Rust does: JSON indented where asked and it parses, else the bytes as they came.
    let out = body;
    if (pretty) {
      try {
        out = JSON.stringify(JSON.parse(body), null, 2);
      } catch {
        out = body;
      }
    }
    return new TextEncoder().encode(out);
  },
  apiAnswerPiped: async (id: number, name: string, steps: { step: string; filter?: string }[]) => {
    piped.push({ id, name, steps: structuredClone(steps) });
    if (steps.some((s) => s.step === "gunzip")) return { outcome: "failed", failure: { index: 0, name: "gunzip", why: "error", error: "invalid gzip header", input: { size: 9, head: '{"a": 1}', binary: false } } };
    const filter = steps.find((s) => s.step === "jq")?.filter ?? ".";
    return { outcome: "done", name: name.replace(/\.gz$/, ""), output: { kind: "text", text: filter === ".items" ? '[\n  {\n    "id": 7\n  }\n]\n' : '{"made": true}\n' } };
  },
  jsonQueryOpen: async (source: unknown) => (jq.opened.push(source), 41),
  jsonQuery: async (id: number, filter: string) => (jq.asked.push([id, filter]), { state: "answered", values: ["7"], count: 1, paths: [["items", 0, "id"]], ms: 1 }),
  jsonQueryClose: async () => {},
}));

const { default: TfOasBody } = await import("./TfOasBody.svelte");

let target: HTMLElement;
let view: ReturnType<typeof mount> | null = null;
beforeEach(() => {
  target = document.createElement("div");
  document.body.appendChild(target);
  Element.prototype.scrollIntoView = function () {};
  bodies.clear();
  reads.length = 0;
  piped.length = 0;
  jq.opened.length = 0;
  jq.asked.length = 0;
});
afterEach(() => {
  if (view) unmount(view);
  view = null;
  target.remove();
});

async function settle() {
  for (let round = 0; round < 12; round += 1) {
    for (let i = 0; i < 10; i += 1) await Promise.resolve();
    await new Promise((resolve) => setTimeout(resolve, 0));
    flushSync();
  }
}
const text = (el: Element | null | undefined) => el?.textContent?.replace(/\s+/g, " ").trim() ?? "";
const button = (label: string) => [...target.querySelectorAll<HTMLButtonElement>("button")].find((b) => text(b) === label);
const pressed = () => [...target.querySelectorAll<HTMLButtonElement>('[aria-label="Render as"] button')].filter((b) => b.getAttribute("aria-pressed") === "true").map((b) => text(b));

function draw(id: number, contentType: string, body: string, extra: Record<string, unknown> = {}) {
  bodies.set(id, body);
  const shared = $state<BodyView>({ steps: null, as: null });
  view = mount(TfOasBody, { target, props: { answer: { id, status: 200, content_type: contentType, bytes: body.length }, view: shared, ...extra } });
  return shared;
}

test("a JSON answer is the compact JSON view: folding, Levels and the jq query, no rail", async () => {
  draw(1, "application/json", '{"items":[{"id":7,"name":"Rex"}]}');
  // The view's code loads with the first answer drawn (`doc-module.ts`): under a loaded suite that is later than a few rounds.
  await vi.waitFor(() => expect(target.querySelector(".tf-xv-compactview .tf-xv-lines")).not.toBeNull(), { timeout: 10_000, interval: 20 });
  await settle();
  expect(pressed()).toEqual(["JSON view"]);
  const doc = target.querySelector(".tf-xv-compactview")!;
  expect(doc).not.toBeNull();
  // No rail and no side column: the answer's column is narrow.
  expect(target.querySelector(".tf-xv-rail")).toBeNull();
  expect(target.querySelector(".tf-xv-sidecol")).toBeNull();
  expect(text(target.querySelector(".tf-xv-lang"))).toBe("jq");
  expect(target.querySelector('[aria-label="Levels shown"]')).not.toBeNull();
  // The one-line answer is read indented, so it folds: a caret on its first line.
  expect(reads).toEqual([[1, true]]);
  expect(target.querySelectorAll(".tf-xv-lines .tf-l").length).toBeGreaterThan(5);
  // The outline lands as paint, after the lines.
  await vi.waitFor(() => expect(target.querySelector('.tf-xv-caret[data-xv-caret="1"]')).not.toBeNull(), { timeout: 10_000, interval: 20 });
  // jq asks a session over the text drawn; its hit is said.
  const field = target.querySelector<HTMLInputElement>(".tf-xv-qfield input")!;
  field.value = ".items[0].id";
  field.dispatchEvent(new Event("input", { bubbles: true }));
  await settle();
  expect(jq.opened).toEqual([{ text: JSON.stringify({ items: [{ id: 7, name: "Rex" }] }, null, 2) }]);
  expect(jq.asked).toEqual([[41, ".items[0].id"]]);
  expect(text(target.querySelector(".tf-xv-qsay"))).toContain("1 value");
});

test("an XML answer is the compact XML view with its XPath query", async () => {
  draw(2, "application/vnd.pets+xml; charset=utf-8", '<?xml version="1.0"?><pets><pet id="7"/></pets>');
  await settle();
  expect(pressed()).toEqual(["XML view"]);
  expect(target.querySelector(".tf-xv-compactview")).not.toBeNull();
  expect(target.querySelector(".tf-xv-rail")).toBeNull();
  expect(text(target.querySelector(".tf-xv-lang"))).toBe("XPath");
});

test("an HTML answer is only ever its text: no frame, no script element, the views refused", async () => {
  const page = '<!doctype html><html><body><script>window.ran = true</script><img src=x onerror="window.ran=true"></body></html>';
  draw(3, "text/html; charset=utf-8", page);
  await settle();
  expect(pressed()).toEqual(["Text"]);
  expect(target.querySelector("pre.tf-oas-ansbody")!.textContent).toBe(page);
  expect(target.querySelector("script, iframe, img")).toBeNull();
  expect((window as { ran?: boolean }).ran).toBeUndefined();
  expect(button("JSON view")!.disabled).toBe(true);
  expect(button("XML view")!.disabled).toBe(true);
});

test("Render as picks the view; the text as written is Text", async () => {
  const shared = draw(4, "text/plain", '{"a": 1}');
  await settle();
  // A text/plain body that is JSON is drawn by the JSON view.
  expect(pressed()).toEqual(["JSON view"]);
  button("Text")!.click();
  await settle();
  expect(shared.as).toBe("text");
  // The text as it came: not indented.
  expect(target.querySelector("pre.tf-oas-ansbody")!.textContent).toBe('{"a": 1}');
  expect(reads.at(-1)).toEqual([4, false]);
  expect(target.querySelector(".tf-xv-compactview")).toBeNull();
});

test("Pipeline runs its steps over the kept answer in the backend, by its id and name; a failed step is said", async () => {
  const shared = draw(5, "application/json", '{"items":[{"id":7}]}');
  await settle();
  button("Pipeline")!.click();
  await settle();
  expect(target.querySelector(".tf-pipe")).not.toBeNull();
  // An empty bar runs nothing: the body as it came.
  expect(piped).toEqual([]);
  shared.steps = [{ step: "jq", filter: ".items" }];
  await settle();
  // The page names the answer and the steps, never its text.
  expect(piped).toEqual([{ id: 5, name: "answer-200.json", steps: [{ step: "jq", filter: ".items" }] }]);
  expect(text(target.querySelector(".tf-xv-compactview"))).toContain('"id": 7');
  shared.steps = [{ step: "gunzip" }];
  await settle();
  expect(text(target.querySelector(".tf-stepfail"))).toContain("Step 1, gunzip, failed.");
  expect(text(target.querySelector(".tf-stepfail"))).toContain("invalid gzip header");
});

test("a body not drawn as it is (bytes, a compressed type) waits for a pipeline, and draws what it makes", async () => {
  const shared = draw(6, "application/gzip", "\u001f\u008b...");
  await settle();
  expect(text(target)).toContain("not drawn here. A pipeline can make text of it.");
  shared.steps = [{ step: "base64_decode" }, { step: "jq", filter: "." }];
  await settle();
  expect(piped.at(-1)).toEqual({ id: 6, name: "answer-200.gz", steps: [{ step: "base64_decode" }, { step: "jq", filter: "." }] });
  expect(text(target.querySelector(".tf-xv-compactview"))).toContain('"made": true');
});

test("Expand hands the answer up; expanded, the whole view with its rail and Back to the explorer", async () => {
  const expanded: number[] = [];
  draw(7, "application/json", '{"a": [1, 2]}', { onexpand: () => expanded.push(7) });
  await settle();
  button("Expand")!.click();
  expect(expanded).toEqual([7]);
  if (view) unmount(view);
  const collapsed: number[] = [];
  draw(7, "application/json", '{"a": [1, 2]}', { full: true, oncollapse: () => collapsed.push(7) });
  await settle();
  expect(target.querySelector(".tf-xv-compactview")).toBeNull();
  expect(target.querySelector(".tf-xv-rail")).not.toBeNull();
  expect(button("Expand")).toBeUndefined();
  button("Back to the explorer")!.click();
  expect(collapsed).toEqual([7]);
});
