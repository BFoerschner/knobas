/**
 * The one file-drawing dispatch (#2581): which leaf draws a file, for the Code page and *Try it on* alike. Every
 * renderer the registry knows is walked, so a renderer added to the registry without a route here fails this file.
 */
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join, relative } from "node:path";

import { describe, expect, test } from "vitest";

import { allRenderers, type Renderer } from "../../render/registry";
import { FILE_ROUTES, routeOf, type FileRoute } from "./file-draw";

const text = { binary: false };
const binary = { binary: true };
const byId = (id: string): Renderer => {
  const found = allRenderers().find((r) => r.id === id);
  if (found === undefined) throw new Error(`no renderer ${id}`);
  return found;
};
const command: Renderer = { id: "command:wc", name: "Count", patterns: ["*"], fences: [], input: "text", output: "html", diff: false };

describe("routeOf", () => {
  // A text file with no bytes read, in a repository that reads archives: each registered renderer's leaf.
  const expected: Record<string, FileRoute> = {
    image: "svg",
    markdown: "markdown",
    mermaid: "diagram",
    d2: "diagram",
    graphviz: "diagram",
    pdf: "lines",
    openapi: "openapi",
    jar: "jar",
    xsd: "xsd",
    xml: "doc",
    json: "doc",
    source: "lines",
    html: "html",
  };

  test("every registered renderer has a route, and it is the same inside a repository and outside one", () => {
    for (const r of allRenderers()) {
      expect(expected[r.id], `a route for ${r.id}`).toBeDefined();
      expect(routeOf(r, text, false, "read"), r.id).toBe(expected[r.id]);
      if (r.id !== "jar") expect(routeOf(r, text, false, "outside"), r.id).toBe(expected[r.id]);
    }
  });

  test("a command renderer draws its command's output; no renderer draws the lines", () => {
    expect(routeOf(command, text, false, "outside")).toBe("command");
    expect(routeOf(null, text, false, "outside")).toBe("lines");
  });

  test("a PDF and a binary image draw from the bytes read, and only where the bytes are the file as read", () => {
    expect(routeOf(byId("pdf"), binary, true, "read")).toBe("pdf");
    expect(routeOf(byId("pdf"), text, true, "outside")).toBe("pdf");
    expect(routeOf(byId("image"), binary, true, "read")).toBe("image");
    expect(routeOf(byId("image"), binary, false, "read")).toBe("binary");
    expect(routeOf(byId("image"), text, true, "read")).toBe("svg");
  });

  test("a JAR opens where the repository reads archives, falls to binary where it does not, and says so outside one", () => {
    expect(routeOf(byId("jar"), binary, false, "read")).toBe("jar");
    expect(routeOf(byId("jar"), binary, false, "unread")).toBe("binary");
    expect(routeOf(byId("jar"), binary, false, "outside")).toBe("jar-elsewhere");
  });

  test("nothing drawn, or binary bytes no byte renderer takes, is a binary file not drawn", () => {
    expect(routeOf(byId("markdown"), binary, false, "read")).toBe("binary");
    expect(routeOf(byId("markdown"), null, false, "read")).toBe("binary");
    expect(routeOf(null, null, false, "outside")).toBe("binary");
  });
});

/**
 * The guard against a second copy (#2581): a renderer dispatch lives in `TfFileDraw.svelte` alone, every route has its
 * branch there, and the Code page and *Try it on* draw through it.
 */
describe("one dispatch", () => {
  const ROOT = join(process.cwd(), "src");
  const svelteFiles = (dir: string): string[] =>
    readdirSync(dir).flatMap((name) => {
      const full = join(dir, name);
      if (statSync(full).isDirectory()) return svelteFiles(full);
      return name.endsWith(".svelte") ? [full] : [];
    });
  const source = (rel: string) => readFileSync(join(ROOT, rel), "utf8");

  test("TfFileDraw has a branch for every route, and no route it does not know", () => {
    const draw = source("lib/tf/render/TfFileDraw.svelte");
    const branches = [...draw.matchAll(/route === "([a-z-]+)"/g)].map((m) => m[1]);
    expect(new Set(branches)).toEqual(new Set(FILE_ROUTES));
  });

  test("no other component carries a file view's dispatch", () => {
    // The issue's own search (`drawAs === "mermaid"`), and the shape any copy has whatever it names its variable: the diagram
    // leaf and the Source leaf it falls through to, in one component. A rendered comparison (`TfDiffRendered`) has
    // diagrams and no Source lines, and is a dispatch of its own.
    const copies = svelteFiles(ROOT)
      .filter((f) => !f.endsWith("TfFileDraw.svelte"))
      .filter((f) => {
        const s = readFileSync(f, "utf8");
        return /=== "mermaid"/.test(s) || (/<TfDiagram\b/.test(s) && /<TfLines\b/.test(s));
      })
      .map((f) => relative(ROOT, f));
    expect(copies).toEqual([]);
  });

  test("the Code page and Try it on both draw through TfFileDraw", () => {
    for (const rel of ["lib/tf/repo/TfRepoFiles.svelte", "lib/tf/render/TfTryDraw.svelte"]) {
      const s = source(rel);
      expect(s, rel).toMatch(/<TfFileDraw\b/);
      for (const leaf of ["<TfPdf", "<TfDiagram", "<TfHtmlPage", "<TfCommandOutput", "<TfOpenApi", "<TfXsd", "<TfDocView", "<TfJar"]) expect(s.includes(leaf), `${rel} mounts ${leaf}`).toBe(false);
    }
  });
});
