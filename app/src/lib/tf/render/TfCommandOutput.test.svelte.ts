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

test("an image renderer's data URL is an image, no frame", async () => {
  await draw("x", { outcome: "drawn", output: "image", body: "data:image/png;base64,iVBORw0KGgo=" }, { ...CAT, output: "image" });

  expect(target.querySelector("iframe")).toBeNull();
  expect(target.querySelector("img")?.getAttribute("src")).toBe("data:image/png;base64,iVBORw0KGgo=");
});

test("a bytes renderer gets the text's UTF-8 bytes", async () => {
  const asked = await draw("é", { outcome: "drawn", output: "html", body: "" }, { ...CAT, input: "bytes" });

  expect(asked[0]![1]).toEqual({ kind: "bytes", base64: "w6k=" });
});
