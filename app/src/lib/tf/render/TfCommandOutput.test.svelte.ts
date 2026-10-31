/**
 * A command renderer's run drawn (#2262): the output in a sandboxed frame,
 * sanitized; a failure as the source and what the command said.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import type { RenderContent, RenderRun } from "../../ipc/app";
import type { CommandPort } from "../../render/command";
import type { Renderer } from "../../render/registry";
import TfCommandOutput from "./TfCommandOutput.svelte";

const CAT: Renderer = { id: "command:cat", name: "Cat", patterns: ["*.html"], fences: ["cat"], input: "text", output: "html", diff: false };

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

/** A port answering `run`, with what it was asked. */
function answering(run: RenderRun) {
  const asked: [string, RenderContent][] = [];
  const port: CommandPort = {
    run: (id, content) => {
      asked.push([id, content]);
      return Promise.resolve(run);
    },
  };
  return { asked, port };
}

async function draw(text: string, run: RenderRun, renderer = CAT) {
  const { asked, port } = answering(run);
  app = mount(TfCommandOutput, { target, props: { renderer, text, commands: port } });
  await Promise.resolve();
  await Promise.resolve();
  flushSync();
  return asked;
}

test("a cat renderer's HTML is drawn in a sandboxed frame, and its script is gone", async () => {
  const html = `<h1>Title</h1><script>parent.document.title = "owned"</script>`;
  const asked = await draw(html, { outcome: "drawn", output: "html", body: html });

  expect(asked).toEqual([["cat", { kind: "text", text: html }]]);
  const frame = target.querySelector("iframe")!;
  expect(frame.getAttribute("sandbox")).toBe("");
  const doc = frame.getAttribute("srcdoc")!;
  expect(doc).toContain("<h1>Title</h1>");
  expect(doc).not.toMatch(/<script/i);
});

test("a renderer past its timeout shows the source and says it timed out", async () => {
  await draw("@startuml\nA -> B\n@enduml", { outcome: "timed_out", seconds: 1, stderr: "" });

  expect(target.querySelector("iframe")).toBeNull();
  expect(target.querySelector(".tf-stepfail")?.textContent).toContain("Cat could not draw this");
  expect(target.querySelector(".tf-stepfail")?.textContent).toContain("longer than its timeout of 1 s");
  expect([...target.querySelectorAll(".tf-code .tf-l")].map((l) => l.textContent)).toEqual(["1@startuml", "2A -> B", "3@enduml"]);
});

test("a renderer whose command fails shows its stderr above the source", async () => {
  await draw("@startuml", { outcome: "failed", status: 3, stderr: "Syntax error on line 1\n" });

  const fail = target.querySelector(".tf-stepfail")!;
  expect(fail.textContent).toContain("exited with status 3");
  expect(fail.querySelector("pre")?.textContent).toBe("Syntax error on line 1\n");
  expect(target.querySelector(".tf-code")?.textContent).toContain("@startuml");
});

/** `URL.createObjectURL` recorded for the test's length: the blobs made, the URLs let go. */
async function withBlobs(run: (made: Blob[], revoked: string[]) => Promise<void>) {
  const made: Blob[] = [];
  const revoked: string[] = [];
  const create = URL.createObjectURL;
  const revoke = URL.revokeObjectURL;
  URL.createObjectURL = (blob: Blob) => (made.push(blob), `blob:fake/${made.length}`);
  URL.revokeObjectURL = (url: string) => void revoked.push(url);
  try {
    await run(made, revoked);
  } finally {
    URL.createObjectURL = create;
    URL.revokeObjectURL = revoke;
  }
}

test("an image renderer's output is an image from a blob of its bytes, no frame", async () => {
  await withBlobs(async (made) => {
    await draw("x", { outcome: "drawn", output: "image", body: "data:image/png;base64,iVBORw0KGgo=" }, { ...CAT, output: "image" });
    await Promise.resolve();
    flushSync();

    expect(target.querySelector("iframe")).toBeNull();
    expect(target.querySelector("img")?.getAttribute("src")).toBe("blob:fake/1");
    expect(made[0]!.type).toBe("image/png");
    expect([...new Uint8Array(await made[0]!.arrayBuffer())]).toEqual([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);
  });
});

test("an image renderer's SVG is sanitized before it is drawn (Standards 1, spec story 44)", async () => {
  const svg = `<svg onload="parent.x=1"><script>parent.x=2</script><rect width="4" height="4"/></svg>`;
  await withBlobs(async (made) => {
    await draw("x", { outcome: "drawn", output: "image", body: `data:image/svg+xml;base64,${btoa(svg)}` }, { ...CAT, output: "image" });
    await Promise.resolve();
    flushSync();

    expect(target.querySelector("img")?.getAttribute("src")).toBe("blob:fake/1");
    expect(made[0]!.type).toBe("image/svg+xml");
    const drawn = await made[0]!.text();
    expect(drawn).toContain("<rect");
    expect(drawn).not.toMatch(/script|onload/i);
  });
});

test("a bytes renderer gets the text's UTF-8 bytes", async () => {
  const asked = await draw("é", { outcome: "drawn", output: "html", body: "" }, { ...CAT, input: "bytes" });

  expect(asked[0]![1]).toEqual({ kind: "bytes", base64: "w6k=" });
});

test("in the app a renderer's HTML frame is served from knobas-render, not a srcdoc (#2304)", async () => {
  const html = `<p style="color: red">red</p>`;
  const { port } = answering({ outcome: "drawn", output: "html", body: html });
  const bridge = { isTauri: () => true, convertFileSrc: (path: string, protocol: string) => `${protocol}://localhost/${path}` };
  app = mount(TfCommandOutput, { target, props: { renderer: CAT, text: html, commands: port, bridge } });
  await Promise.resolve();
  await Promise.resolve();
  flushSync();

  const frame = target.querySelector("iframe")!;
  expect(frame.getAttribute("sandbox")).toBe("");
  expect(frame.getAttribute("src")?.startsWith("knobas-render://localhost/")).toBe(true);
  expect(frame.hasAttribute("srcdoc")).toBe(false);
});
