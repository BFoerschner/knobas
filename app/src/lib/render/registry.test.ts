/** The renderer registry (#2254): what each renderer declares, and which accept a file or a fence. */
import { expect, test } from "vitest";

import { imageUrl, register, renderersFor, renderersForFence, type Renderer } from "./registry";

const names = (list: Renderer[]) => list.map((r) => r.name);

test("each built-in declares the spec's six fields", () => {
  const [image, source] = renderersFor({ path: "logo.svg", binary: false });
  expect(image).toEqual({
    id: "image",
    name: "Image",
    patterns: expect.arrayContaining(["*.png", "*.svg"]),
    fences: [],
    input: "bytes",
    output: "image",
    diff: false,
  });
  expect(source).toEqual({ id: "source", name: "Source", patterns: ["*"], fences: ["*"], input: "text", output: "lines", diff: true });
});

test("a text file is Source's; an image is Image's first; a binary that is no image has none", () => {
  expect(names(renderersFor({ path: "src/main.rs", binary: false }))).toEqual(["Source"]);
  expect(names(renderersFor({ path: "docs/Logo.SVG", binary: false }))).toEqual(["Image", "Source"]);
  expect(names(renderersFor({ path: "assets/shot.png", binary: true }))).toEqual(["Image"]);
  expect(names(renderersFor({ path: "bin/tool", binary: true }))).toEqual([]);
});

test("a Markdown file is Markdown's first, then Source's; a fence is never Markdown's (#2255)", () => {
  expect(names(renderersFor({ path: "README.md", binary: false }))).toEqual(["Markdown", "Source"]);
  expect(names(renderersFor({ path: "docs/Guide.MARKDOWN", binary: false }))).toEqual(["Markdown", "Source"]);
  expect(names(renderersFor({ path: "README", binary: false }))).toEqual(["Source"]);
  expect(names(renderersForFence("markdown"))).toEqual(["Source"]);
});

test("a renderer registered later goes ahead of Source, by file pattern and by fence", () => {
  const undo = register({ id: "mermaid", name: "Mermaid", patterns: ["*.mmd", "docs/*.mermaid"], fences: ["mermaid"], input: "text", output: "svg", diff: false });
  try {
    expect(names(renderersFor({ path: "docs/architecture.mmd", binary: false }))).toEqual(["Mermaid", "Source"]);
    expect(names(renderersFor({ path: "docs/a.mermaid", binary: false }))).toEqual(["Mermaid", "Source"]);
    expect(names(renderersFor({ path: "other/a.mermaid", binary: false }))).toEqual(["Source"]);
    expect(names(renderersForFence("Mermaid"))).toEqual(["Mermaid", "Source"]);
    expect(names(renderersForFence("rust"))).toEqual(["Source"]);
  } finally {
    undo();
  }
  expect(names(renderersFor({ path: "docs/architecture.mmd", binary: false }))).toEqual(["Source"]);
});

test("an image's bytes become a data URL of its type", () => {
  expect(imageUrl(new TextEncoder().encode("<svg/>"), "a.svg")).toBe(`data:image/svg+xml;base64,${btoa("<svg/>")}`);
  expect(imageUrl(new Uint8Array([0x89, 0x50]), "a.png")).toBe("data:image/png;base64,iVA=");
});
