import { describe, expect, test } from "vitest";

import { defaultPattern, makeDefaultPattern, openWith, withDefault, type RendererDefault } from "./defaults";
import { IMAGE, MARKDOWN, SOURCE, type Renderer } from "./registry";

const PUML: Renderer = {
  id: "command:plantuml",
  name: "PlantUML",
  patterns: ["*.puml"],
  fences: [],
  input: "text",
  output: "svg",
  diff: false,
};

const BUILT_IN = [IMAGE, MARKDOWN, SOURCE];
const text = (path: string) => ({ path, binary: false });
const ids = (list: readonly Renderer[]) => list.map((r) => r.id);

describe("openWith", () => {
  test("with no mapping the file opens with the first renderer that accepts it", () => {
    expect(openWith(text("README.md"), BUILT_IN, [])).toEqual({ offered: [MARKDOWN, SOURCE], renderer: MARKDOWN, pattern: null });
  });

  test("a .md file mapped to Source opens as source, and Markdown is still offered", () => {
    const got = openWith(text("docs/README.md"), BUILT_IN, [{ pattern: "*.md", renderer: "source" }]);
    expect(got.renderer).toBe(SOURCE);
    expect(got.pattern).toBe("*.md");
    expect(ids(got.offered)).toEqual(["markdown", "source"]);
  });

  test("a file matched by two patterns uses the more specific one, whatever their order", () => {
    const both: RendererDefault[] = [
      { pattern: "*.md", renderer: "source" },
      { pattern: "docs/*.md", renderer: "markdown" },
    ];
    expect(openWith(text("docs/plan.md"), BUILT_IN, both).renderer).toBe(MARKDOWN);
    expect(openWith(text("docs/plan.md"), BUILT_IN, [...both].reverse()).renderer).toBe(MARKDOWN);
    expect(openWith(text("README.md"), BUILT_IN, both).renderer).toBe(SOURCE);
    const named: RendererDefault[] = [
      { pattern: "*.md", renderer: "markdown" },
      { pattern: "CHANGELOG.md", renderer: "source" },
    ];
    expect(openWith(text("CHANGELOG.md"), BUILT_IN, named).pattern).toBe("CHANGELOG.md");
  });

  test("patterns match without regard to case", () => {
    expect(openWith(text("NOTES.MD"), BUILT_IN, [{ pattern: "*.md", renderer: "source" }]).renderer).toBe(SOURCE);
  });

  test("a renderer that is deleted falls back to the built-in default for its patterns", () => {
    const mapped: RendererDefault[] = [{ pattern: "*.md", renderer: PUML.id }];
    expect(openWith(text("README.md"), [IMAGE, MARKDOWN, PUML, SOURCE], mapped).renderer).toBe(PUML);
    // PlantUML deleted: the registry no longer holds it.
    expect(openWith(text("README.md"), BUILT_IN, mapped)).toEqual({ offered: [MARKDOWN, SOURCE], renderer: MARKDOWN, pattern: null });
  });

  test("a mapping whose renderer is gone gives way to the next pattern that still names one", () => {
    const mapped: RendererDefault[] = [
      { pattern: "docs/*.md", renderer: PUML.id },
      { pattern: "*.md", renderer: "source" },
    ];
    expect(openWith(text("docs/a.md"), BUILT_IN, mapped)).toMatchObject({ renderer: SOURCE, pattern: "*.md" });
  });

  test("a mapping lets its renderer take files its own patterns do not name", () => {
    const got = openWith(text("notes/plan.txt"), BUILT_IN, [{ pattern: "*.txt", renderer: "markdown" }]);
    expect(got.renderer).toBe(MARKDOWN);
    expect(ids(got.offered)).toEqual(["markdown", "source"]);
  });

  test("a binary file is never opened by a renderer that reads text, whatever the mapping says", () => {
    const got = openWith({ path: "logo.png", binary: true }, BUILT_IN, [{ pattern: "*.png", renderer: "source" }]);
    expect(got).toEqual({ offered: [IMAGE], renderer: IMAGE, pattern: null });
  });

  test("nothing accepts the file: nothing opens it", () => {
    expect(openWith({ path: "blob.bin", binary: true }, BUILT_IN, [])).toEqual({ offered: [], renderer: null, pattern: null });
  });
});

describe("defaultPattern", () => {
  test("is the file's extension, lowercase, or its whole name without one", () => {
    expect(defaultPattern("docs/Architecture.MMD")).toBe("*.mmd");
    expect(defaultPattern("archive.json.gz")).toBe("*.gz");
    expect(defaultPattern("justfile")).toBe("justfile");
    expect(defaultPattern(".gitignore")).toBe(".gitignore");
  });
});

describe("makeDefaultPattern", () => {
  test("is the pattern that chose the default, so a narrower row is changed rather than shadowed", () => {
    const list: RendererDefault[] = [
      { pattern: "*.md", renderer: "markdown" },
      { pattern: "docs/*.md", renderer: "source" },
    ];
    expect(makeDefaultPattern(openWith(text("docs/plan.md"), BUILT_IN, list), "docs/plan.md")).toBe("docs/*.md");
  });

  test("is the file's own extension when a broader pattern chose the default, or none did", () => {
    const everything: RendererDefault[] = [{ pattern: "*", renderer: "source" }];
    expect(makeDefaultPattern(openWith(text("README.md"), BUILT_IN, everything), "README.md")).toBe("*.md");
    expect(makeDefaultPattern(openWith(text("README.md"), BUILT_IN, []), "README.md")).toBe("*.md");
  });
});

describe("withDefault", () => {
  test("replaces the renderer of a pattern already mapped, in place", () => {
    const list: RendererDefault[] = [
      { pattern: "*.md", renderer: "markdown" },
      { pattern: "*.mmd", renderer: "source" },
    ];
    expect(withDefault(list, "*.MD", "source")).toEqual([
      { pattern: "*.md", renderer: "source" },
      { pattern: "*.mmd", renderer: "source" },
    ]);
  });

  test("adds a pattern not mapped yet at the end", () => {
    expect(withDefault([{ pattern: "*.md", renderer: "markdown" }], "*.mmd", "mermaid")).toEqual([
      { pattern: "*.md", renderer: "markdown" },
      { pattern: "*.mmd", renderer: "mermaid" },
    ]);
  });
});
