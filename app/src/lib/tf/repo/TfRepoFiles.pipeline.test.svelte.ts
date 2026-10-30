/**
 * The Files tab's pipeline editor (#2264, spec #2234 step 5; `mockups/code-area/pipeline.html`), mounted over a
 * stand-in source and a stand-in `piped` port that records every pipeline it is asked to run and answers as the
 * test says. What is under test is the viewer's half: the bar sends the steps in their order, a changed filter
 * runs again, the output is drawn under the name the backend gives it, and a failed step is shown with its input.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { PipeRun, PipeStep, SourceView } from "../../ipc/entity";
import TfRepoFiles from "./TfRepoFiles.svelte";
import type { HighlightHandlers, HighlightRequest, RenderPort } from "../../render/port";
import { createRendererDefaults } from "../../render/defaults.svelte";

const REPO = "gitea:tidewater/payout-service";
const PATH = "fixtures/events.json.gz";

function fileView(section: string): SourceView {
  const blocks =
    section === "files"
      ? [{ type: "tree", trail: [], entries: [{ name: "fixtures", kind: "dir", size: null, section: "files:fixtures" }], truncated: false, url: null }]
      : section === "files:fixtures"
        ? [{ type: "tree", trail: [], entries: [{ name: "events.json.gz", kind: "file", size: 48_213, section: `file:${PATH}` }], truncated: false, url: null }]
        : [{ type: "file", trail: [], path: PATH, text: "", language: null, size: 48_213, truncated: false, binary: true, url: null, sha: "b10b", commit: "c0mmit" }];
  return { sections: [{ id: section, title: "Files", count: null, deferred: false, blocks }], actions: [] } as unknown as SourceView;
}

/** A `piped` port that records each ask and answers with `respond`. */
function pipedPort(respond: (steps: PipeStep[]) => PipeRun) {
  const asked: { path: string; commit: string | null; steps: PipeStep[] }[] = [];
  const piped = async (_entity: string, path: string, commit: string | null, steps: PipeStep[]) => {
    asked.push({ path, commit, steps });
    return respond(steps);
  };
  return { asked, piped };
}

/** A highlighter that records what it was asked to colour and answers nothing. */
function recordingRender() {
  const requests: HighlightRequest[] = [];
  const port: RenderPort = {
    highlight(request: HighlightRequest, _handlers: HighlightHandlers) {
      requests.push(request);
      return { focus() {}, cancel() {} };
    },
  };
  return { requests, port };
}

const done = (text: string, name = "fixtures/events.json"): PipeRun => ({ outcome: "done", name, output: { kind: "text", text } });

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
  vi.useRealTimers();
});

function draw(piped: ReturnType<typeof pipedPort>["piped"], render?: RenderPort) {
  const ports = {
    entityView: async (_at: string, section: string | null) => fileView(section ?? ""),
    piped,
    submitWrite: async () => {
      throw { code: "invalid", message: "no write in this suite" };
    },
  };
  view = mount(TfRepoFiles, {
    target,
    props: {
      ports,
      repoId: REPO,
      at: REPO,
      branch: "main",
      path: PATH,
      changed: new Set<string>(),
      defaults: createRendererDefaults({ read: async () => [], save: async (list) => list }),
      ...(render ? { render } : {}),
    },
  });
  flushSync();
}

async function settle() {
  for (let round = 0; round < 4; round += 1) {
    for (let i = 0; i < 10; i += 1) await Promise.resolve();
    await new Promise((resolve) => setTimeout(resolve, 0));
    flushSync();
  }
}

const text = (el: Element | null) => el?.textContent?.replace(/\s+/g, " ").trim() ?? "";
const button = (label: string) =>
  [...target.querySelectorAll<HTMLButtonElement>("button")].find((b) => text(b) === label || b.getAttribute("aria-label") === label) ?? null;

async function addStep(label: string) {
  button("+ Add step")!.click();
  flushSync();
  const row = [...target.querySelectorAll<HTMLButtonElement>(".tf-pmenu [role=menuitem]")].find((b) => text(b).startsWith(label));
  expect(row, label).toBeDefined();
  row!.click();
  await settle();
}

const drawnLines = () => [...target.querySelectorAll(".tf-code .tf-l")].map((l) => text(l.lastElementChild));

test("a gzipped file has no pipeline until one is added; then gunzip → jq runs in that order and draws its output", async () => {
  const { asked, piped } = pipedPort((steps) => done(steps.length === 1 ? '{"a":1}' : "{\n  \"a\": 1\n}\n"));
  const { requests, port } = recordingRender();
  draw(piped, port);
  await settle();
  expect(text(target.querySelector(".tf-empty"))).toBe("A binary file, not drawn.");
  expect(target.querySelector(".tf-pipe")).toBeNull();
  expect(asked).toEqual([]);

  button("+ Pipeline")!.click();
  await settle();
  expect(text(target.querySelector(".tf-psteps"))).toContain("bytes→");
  await addStep("gzip · zstd · bzip2 · xz decompress");
  await addStep("jq");
  expect(asked.map((a) => a.steps)).toEqual([[{ step: "gunzip" }], [{ step: "gunzip" }, { step: "jq", filter: "." }]]);
  expect(asked[1]).toMatchObject({ path: PATH, commit: "c0mmit" });
  expect(drawnLines()).toEqual(["{", '"a": 1', "}"]);
  // The output meets the renderer under its own name: highlighted as `events.json`, the bar's last box says so.
  expect(requests.at(-1)?.path).toBe("fixtures/events.json");
  expect(text(target.querySelector(".tf-prenderer"))).toBe("Source · events.json");
});

test("a changed jq filter runs again once typing rests, and at once on Enter", async () => {
  const { asked, piped } = pipedPort((steps) => done(JSON.stringify(steps)));
  draw(piped);
  await settle();
  button("+ Pipeline")!.click();
  await settle();
  await addStep("gzip");
  await addStep("jq");
  const filter = target.querySelector<HTMLInputElement>("input[aria-label='jq filter']")!;
  vi.useFakeTimers();
  filter.value = ".events[0]";
  filter.dispatchEvent(new Event("input", { bubbles: true }));
  expect(asked).toHaveLength(2);
  vi.advanceTimersByTime(400);
  vi.useRealTimers();
  await settle();
  expect(asked.at(-1)?.steps).toEqual([{ step: "gunzip" }, { step: "jq", filter: ".events[0]" }]);
  filter.value = ".events[1]";
  filter.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
  await settle();
  expect(asked.at(-1)?.steps).toEqual([{ step: "gunzip" }, { step: "jq", filter: ".events[1]" }]);
});

test("a failed step is named, marked, and shown with what it said and the input it was given", async () => {
  const { piped } = pipedPort(() => ({
    outcome: "failed",
    failure: {
      index: 1,
      name: "jq",
      why: "error",
      error: "the filter does not parse: expected term at column 6",
      input: { size: 217_088, head: '{ "events": [', binary: false },
    },
  }));
  draw(piped);
  await settle();
  button("+ Pipeline")!.click();
  await settle();
  await addStep("gzip");
  await addStep("jq");
  const failure = target.querySelector(".tf-stepfail");
  expect(text(failure?.querySelector(":scope > b") ?? null)).toBe("Step 2, jq, failed.");
  expect(text(failure)).toContain("the filter does not parse: expected term at column 6");
  expect(text(failure)).toContain("Its input, the output of step 1 (gunzip), 212.0 KB");
  expect(text(failure)).toContain('{ "events": [');
  const bad = target.querySelector(".tf-pstep.tf-pbad");
  expect(bad?.getAttribute("aria-label")).toBe("Step 2, jq");
  // The file is drawn as read meanwhile.
  expect(text(target.querySelector(".tf-empty"))).toBe("A binary file, not drawn.");
});

test("a step is moved with its grip's arrow keys and removed with its ×; Reset drops the pipeline", async () => {
  const { asked, piped } = pipedPort(() => done("x"));
  draw(piped);
  await settle();
  button("+ Pipeline")!.click();
  await settle();
  await addStep("gzip");
  await addStep("Hex dump");
  button("Move hex dump")!.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowLeft", bubbles: true }));
  await settle();
  expect(asked.at(-1)?.steps).toEqual([{ step: "hex_dump" }, { step: "gunzip" }]);
  button("Remove hex dump")!.click();
  await settle();
  expect(asked.at(-1)?.steps).toEqual([{ step: "gunzip" }]);
  button("Reset")!.click();
  await settle();
  expect(target.querySelector(".tf-pipe")).toBeNull();
  expect(button("+ Pipeline")).not.toBeNull();
});
