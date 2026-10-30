/** User command renderers in the registry, and what their output becomes before a frame draws it (#2262). */
import { afterEach, expect, test } from "vitest";

import type { UserRenderer } from "../ipc/app";
import { commandIdOf, frameDocument, installCommands, savedCommand } from "./command";
import { renderersFor, renderersForFence } from "./registry";

const PLANTUML: UserRenderer = {
  id: "plantuml",
  name: "PlantUML",
  command: "plantuml -tsvg -pipe",
  patterns: ["*.puml"],
  fences: ["plantuml"],
  input: "text",
  output: "svg",
  timeout_secs: 10,
  diff: false,
};

afterEach(() => installCommands([]));

const names = (list: { name: string }[]) => list.map((r) => r.name);

test("a saved renderer accepts its file patterns and fence languages, ahead of Source", () => {
  installCommands([PLANTUML]);

  expect(names(renderersFor({ path: "docs/flow.puml", binary: false }))).toEqual(["PlantUML", "Source"]);
  expect(names(renderersForFence("PlantUML"))).toEqual(["PlantUML", "Source"]);
  expect(names(renderersForFence("rust"))).toEqual(["Source"]);
  const [first] = renderersForFence("plantuml");
  expect(commandIdOf(first!)).toBe("plantuml");
  expect(savedCommand(first!)?.command).toBe("plantuml -tsvg -pipe");
  expect(commandIdOf(renderersForFence("rust")[0]!)).toBeNull();
});

test("a saved renderer draws diffs only where it declares so (#2279)", () => {
  installCommands([PLANTUML, { ...PLANTUML, id: "d2", name: "D2 CLI", patterns: ["*.d2"], fences: [], diff: true }]);

  expect(renderersFor({ path: "docs/flow.puml", binary: false })[0]?.diff).toBe(false);
  expect(renderersFor({ path: "docs/flow.d2", binary: false })[0]?.diff).toBe(true);
});

test("installing the list again replaces the last one, so a removed renderer is gone", () => {
  installCommands([PLANTUML]);
  installCommands([{ ...PLANTUML, id: "d2", name: "D2 CLI", patterns: ["*.d2"], fences: ["d2"] }]);

  expect(names(renderersFor({ path: "docs/flow.puml", binary: false }))).toEqual(["Source"]);
  expect(names(renderersFor({ path: "docs/flow.d2", binary: false }))).toEqual(["D2 CLI", "Source"]);
  installCommands([]);
  expect(names(renderersForFence("d2"))).toEqual(["Source"]);
});

test("HTML output loses its scripts and handlers before it reaches the frame", () => {
  const doc = frameDocument(
    "html",
    `<p>hello</p><script>parent.document.title = "owned"</script><img src="x" onerror="alert(1)"><a href="javascript:alert(2)">x</a>`,
  );

  expect(doc).toContain("<p>hello</p>");
  expect(doc).not.toMatch(/<script/i);
  expect(doc).not.toMatch(/onerror/i);
  expect(doc).not.toMatch(/javascript:/i);
});

test("SVG output is sanitized and drawn as an image, which runs nothing", () => {
  const doc = frameDocument("svg", `<svg><script>alert(1)</script><rect width="4" height="4" onload="alert(2)"/></svg>`);

  const src = /<img[^>]* src="data:image\/svg\+xml;base64,([^"]+)"/.exec(doc)?.[1];
  expect(src).toBeDefined();
  const svg = new TextDecoder().decode(Uint8Array.from(atob(src!), (c) => c.charCodeAt(0)));
  expect(svg).toContain("<rect");
  expect(svg).not.toMatch(/<script|onload/i);
});
