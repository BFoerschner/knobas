/**
 * *Try it on*'s drawing (#2562): the person's example drawn as the Code page draws a file, the same renderer chosen the
 * same way, a pipeline run the same way, and every state on one stage that holds its room.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import type { NamedPipeline, RendererDefault, TryOn } from "../../ipc/app";
import type { PipeRun, PipeStep } from "../../ipc/entity";
import TfTryDraw, { type TryDrawPorts } from "./TfTryDraw.svelte";

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

const quiet = { highlight: () => ({ focus: () => {}, cancel: () => {} }) };
async function settle() {
  for (let i = 0; i < 6; i += 1) {
    await new Promise((resolve) => setTimeout(resolve, 0));
    flushSync();
  }
}

function draw(on: TryOn, extra: { renderer?: string; pipeline?: string; defaults?: RendererDefault[]; pipelines?: NamedPipeline[]; ports?: Partial<TryDrawPorts> } = {}) {
  const asked = { bytes: [] as string[], piped: [] as [TryOn, PipeStep[]][] };
  const ports: TryDrawPorts = {
    bytes: async (path) => (asked.bytes.push(path), new TextEncoder().encode("# Chosen\n")),
    piped: async (at, steps) => (asked.piped.push([at, steps]), { outcome: "done", name: "order.json", output: { kind: "text", text: '{"a":1}' } }),
    commands: { run: async () => ({ outcome: "drawn", output: "html", body: "<p>x</p>" }) },
    frame: async () => "t",
    ...extra.ports,
  };
  view = mount(TfTryDraw, {
    target,
    props: { on, renderer: extra.renderer ?? "auto", pipeline: extra.pipeline ?? "none", defaults: extra.defaults ?? [], pipelines: extra.pipelines ?? [], ports, render: quiet },
  });
  flushSync();
  return asked;
}

const stage = () => target.querySelector(".tf-try-stage");
const shownAs = () => target.querySelector(".tf-try-view")?.getAttribute("data-shows");

test("pasted Markdown opens as the Code page opens it, and Drawn by picks another that takes it", async () => {
  draw({ kind: "text", name: "notes.md", text: "# Hello\n\nA **bold** word.\n" });
  await settle();
  expect(shownAs()).toBe("markdown");
  expect(stage()?.querySelector(".tf-rendered strong")?.textContent).toBe("bold");
  expect(target.querySelector(".tf-try-view > header .tf-p")?.textContent).toBe("notes.md");
  unmount(view!);
  draw({ kind: "text", name: "notes.md", text: "# Hello\n" }, { renderer: "source" });
  await settle();
  expect(shownAs()).toBe("source");
});

test("a default renderer for the name is the one it opens with, as on the Code page", async () => {
  draw({ kind: "text", name: "notes.md", text: "# Hello\n" }, { defaults: [{ pattern: "*.md", renderer: "source", pipeline: null }] });
  await settle();
  expect(shownAs()).toBe("source");
});

test("a renderer that does not take the example says so, and the Code page's choice draws it", async () => {
  draw({ kind: "text", name: "notes.txt", text: "plain" }, { renderer: "markdown" });
  await settle();
  expect(shownAs()).toBe("source");
  expect(stage()?.querySelector("[role=status]")?.textContent).toBe("Markdown does not take notes.txt: it is drawn as the Code page would draw it.");
});

test("a chosen file is read once, on the press that drew it, by its path", async () => {
  const asked = draw({ kind: "path", path: "/Users/mara/examples/chosen.md" });
  await settle();
  expect(asked.bytes).toEqual(["/Users/mara/examples/chosen.md"]);
  expect(shownAs()).toBe("markdown");
  expect(target.querySelector(".tf-try-view > header .tf-p")?.textContent).toBe("chosen.md");
});

test("reading, a refusal and the drawing sit on one stage that holds its room", async () => {
  let refuse!: (cause: unknown) => void;
  draw({ kind: "path", path: "/Users/mara/huge.bin" }, { ports: { bytes: () => new Promise<Uint8Array>((_, reject) => (refuse = reject)) } });
  const first = stage();
  expect(first?.querySelector("[role=status]")?.textContent).toBe("Reading huge.bin…");
  refuse({ code: "invalid", message: "huge.bin is 61 MB, past the 50 MB knobas hands a renderer" });
  await settle();
  expect(stage()).toBe(first);
  expect(first?.querySelector("[role=alert]")?.textContent).toBe("Could not read huge.bin: huge.bin is 61 MB, past the 50 MB knobas hands a renderer");
});

test("a pipeline the panel names runs on the example, and its output is drawn by its own name", async () => {
  const steps: PipeStep[] = [{ step: "base64_decode" }];
  const on: TryOn = { kind: "text", name: "order.json.b64", text: "eyJhIjoxfQ==" };
  const asked = draw(on, { pipeline: "b64", pipelines: [{ id: "b64", name: "Base64 JSON", steps }] });
  await settle();
  expect(asked.piped).toEqual([[on, steps]]);
  expect(target.querySelector(".tf-try-view > header .tf-p")?.textContent).toBe("order.json");
  expect(shownAs()).toBe("json");
});

test("the default pipeline for the name runs where the panel says as the Code page does", async () => {
  const steps: PipeStep[] = [{ step: "gunzip" }];
  const asked = draw(
    { kind: "text", name: "events.json.gz", text: "x" },
    { pipeline: "auto", defaults: [{ pattern: "*.json.gz", renderer: "", pipeline: "gz" }], pipelines: [{ id: "gz", name: "Gunzip", steps }] },
  );
  await settle();
  expect(asked.piped.map(([, s]) => s)).toEqual([steps]);
});

test("a failed step says which, what it said and its input", async () => {
  const failed: PipeRun = { outcome: "failed", failure: { index: 0, name: "Gunzip", error: "not gzip", input: { size: 1, head: "x" } } } as PipeRun;
  draw({ kind: "text", name: "a.gz", text: "x" }, { pipeline: "gz", pipelines: [{ id: "gz", name: "Gunzip", steps: [{ step: "gunzip" }] }], ports: { piped: async () => failed } });
  await settle();
  expect(stage()?.querySelector("[role=alert] b")?.textContent).toBe("Step 1, Gunzip, failed.");
});

test("bytes that are no text are not drawn as text", async () => {
  draw({ kind: "path", path: "/Users/mara/blob.dat" }, { ports: { bytes: async () => Uint8Array.from([0, 1, 2, 3]) } });
  await settle();
  expect(stage()?.textContent).toContain("A binary file, not drawn.");
});
