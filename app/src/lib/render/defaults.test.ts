import { describe, expect, test } from "vitest";

import { defaultPattern, defaultPipeline, makeDefaultPattern, openWith, withDefault, withPipeline, type RendererDefault } from "./defaults";
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
    const got = openWith(text("docs/README.md"), BUILT_IN, [{ pattern: "*.md", renderer: "source", pipeline: null }]);
    expect(got.renderer).toBe(SOURCE);
    expect(got.pattern).toBe("*.md");
    expect(ids(got.offered)).toEqual(["markdown", "source"]);
  });

  test("a file matched by two patterns uses the more specific one, whatever their order", () => {
    const both: RendererDefault[] = [
      { pattern: "*.md", renderer: "source", pipeline: null },
      { pattern: "docs/*.md", renderer: "markdown", pipeline: null },
    ];
    expect(openWith(text("docs/plan.md"), BUILT_IN, both).renderer).toBe(MARKDOWN);
    expect(openWith(text("docs/plan.md"), BUILT_IN, [...both].reverse()).renderer).toBe(MARKDOWN);
    expect(openWith(text("README.md"), BUILT_IN, both).renderer).toBe(SOURCE);
    const named: RendererDefault[] = [
      { pattern: "*.md", renderer: "markdown", pipeline: null },
      { pattern: "CHANGELOG.md", renderer: "source", pipeline: null },
    ];
    expect(openWith(text("CHANGELOG.md"), BUILT_IN, named).pattern).toBe("CHANGELOG.md");
  });

  test("patterns match without regard to case", () => {
    expect(openWith(text("NOTES.MD"), BUILT_IN, [{ pattern: "*.md", renderer: "source", pipeline: null }]).renderer).toBe(SOURCE);
  });

  test("a renderer that is deleted falls back to the built-in default for its patterns", () => {
    const mapped: RendererDefault[] = [{ pattern: "*.md", renderer: PUML.id, pipeline: null }];
    expect(openWith(text("README.md"), [IMAGE, MARKDOWN, PUML, SOURCE], mapped).renderer).toBe(PUML);
    // PlantUML deleted: the registry no longer holds it.
    expect(openWith(text("README.md"), BUILT_IN, mapped)).toEqual({ offered: [MARKDOWN, SOURCE], renderer: MARKDOWN, pattern: null });
  });

  test("a mapping whose renderer is gone gives way to the next pattern that still names one", () => {
    const mapped: RendererDefault[] = [
      { pattern: "docs/*.md", renderer: PUML.id, pipeline: null },
      { pattern: "*.md", renderer: "source", pipeline: null },
    ];
    expect(openWith(text("docs/a.md"), BUILT_IN, mapped)).toMatchObject({ renderer: SOURCE, pattern: "*.md" });
  });

  test("a mapping lets its renderer take files its own patterns do not name", () => {
    const got = openWith(text("notes/plan.txt"), BUILT_IN, [{ pattern: "*.txt", renderer: "markdown", pipeline: null }]);
    expect(got.renderer).toBe(MARKDOWN);
    expect(ids(got.offered)).toEqual(["markdown", "source"]);
  });

  test("a binary file is never opened by a renderer that reads text, whatever the mapping says", () => {
    const got = openWith({ path: "logo.png", binary: true }, BUILT_IN, [{ pattern: "*.png", renderer: "source", pipeline: null }]);
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
      { pattern: "*.md", renderer: "markdown", pipeline: null },
      { pattern: "docs/*.md", renderer: "source", pipeline: null },
    ];
    expect(makeDefaultPattern(openWith(text("docs/plan.md"), BUILT_IN, list), "docs/plan.md")).toBe("docs/*.md");
  });

  test("is the file's own extension when a broader pattern chose the default, or none did", () => {
    const everything: RendererDefault[] = [{ pattern: "*", renderer: "source", pipeline: null }];
    expect(makeDefaultPattern(openWith(text("README.md"), BUILT_IN, everything), "README.md")).toBe("*.md");
    expect(makeDefaultPattern(openWith(text("README.md"), BUILT_IN, []), "README.md")).toBe("*.md");
  });
});

describe("withDefault", () => {
  test("replaces the renderer of a pattern already mapped, in place", () => {
    const list: RendererDefault[] = [
      { pattern: "*.md", renderer: "markdown", pipeline: null },
      { pattern: "*.mmd", renderer: "source", pipeline: null },
    ];
    expect(withDefault(list, "*.MD", "source")).toEqual([
      { pattern: "*.md", renderer: "source", pipeline: null },
      { pattern: "*.mmd", renderer: "source", pipeline: null },
    ]);
  });

  test("adds a pattern not mapped yet at the end", () => {
    expect(withDefault([{ pattern: "*.md", renderer: "markdown", pipeline: null }], "*.mmd", "mermaid")).toEqual([
      { pattern: "*.md", renderer: "markdown", pipeline: null },
      { pattern: "*.mmd", renderer: "mermaid", pipeline: null },
    ]);
  });
});

describe("defaultPipeline (#2265)", () => {
  const gz = { id: "gz-json", name: "Gzipped JSON", steps: [{ step: "gunzip" as const }, { step: "jq" as const, filter: "." }] };
  const b64 = { id: "b64", name: "Base64", steps: [{ step: "base64_decode" as const }] };
  const row = (pattern: string, pipeline: string | null, renderer = ""): RendererDefault => ({ pattern, renderer, pipeline });

  test("a pipeline saved as the default for *.json.gz applies to every such file, by its id", () => {
    const list = [row("*.json.gz", "gz-json")];
    for (const path of ["events.json.gz", "a/b/c.JSON.GZ", "x.json.gz"]) {
      expect(defaultPipeline(path, list, [gz, b64])).toEqual({ pipeline: gz, pattern: "*.json.gz" });
    }
    expect(defaultPipeline("events.gz", list, [gz, b64])).toBeNull();
    expect(defaultPipeline("events.json", list, [gz, b64])).toBeNull();
  });

  test("the most specific pattern naming a pipeline decides; a row naming none, or a deleted one, is passed over", () => {
    const list = [row("*.gz", "b64"), row("events.json.gz", null, "source"), row("*.json.gz", "gone"), row("logs/*.json.gz", "gz-json")];
    expect(defaultPipeline("logs/events.json.gz", list, [gz, b64])?.pipeline).toBe(gz);
    expect(defaultPipeline("events.json.gz", list, [gz, b64])).toEqual({ pipeline: b64, pattern: "*.gz" });
  });

  test("withPipeline sets a pattern's pipeline and keeps its renderer; withDefault keeps its pipeline", () => {
    const list = [row("*.md", null, "source")];
    expect(withPipeline(list, "*.MD", "b64")).toEqual([row("*.md", "b64", "source")]);
    expect(withPipeline(list, "*.json.gz", "gz-json")).toEqual([row("*.md", null, "source"), row("*.json.gz", "gz-json")]);
    expect(withDefault([row("*.json.gz", "gz-json")], "*.json.gz", "markdown")).toEqual([row("*.json.gz", "gz-json", "markdown")]);
    expect(withDefault([], "*.md", "markdown")).toEqual([row("*.md", null, "markdown")]);
  });

  test("a row that only sets a pipeline does not pick the file's renderer", () => {
    expect(openWith(text("events.json.gz"), BUILT_IN, [row("*.json.gz", "gz-json")]).pattern).toBeNull();
  });
});
