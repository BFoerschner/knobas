/** The renderer registry (#2254): what each renderer declares, and which accept a file or a fence. */
import { expect, test } from "vitest";

import { register, renderersFor, renderersForFence, type Renderer } from "./registry";
import { ns } from "./xml-names";

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
    diff: true,
  });
  expect(source).toEqual({ id: "source", name: "Source", patterns: ["*"], fences: ["*"], input: "text", output: "lines", diff: true });
});

test("Markdown, Mermaid, D2, Graphviz and Image draw a diff before and after (#2279)", () => {
  const ids = (path: string) => renderersFor({ path, binary: false }).filter((r) => r.diff).map((r) => r.id);
  expect(ids("README.md")).toEqual(["markdown", "source"]);
  expect(ids("docs/flow.mmd")).toEqual(["mermaid", "source"]);
  expect(ids("docs/flow.d2")).toEqual(["d2", "source"]);
  expect(ids("docs/flow.gv")).toEqual(["graphviz", "source"]);
  expect(renderersFor({ path: "logo.png", binary: true }).map((r) => r.diff)).toEqual([true]);
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

test("Mermaid is built in: a .mmd or .mermaid file and a mermaid fence are its first, then Source's (#2258)", () => {
  expect(names(renderersFor({ path: "docs/architecture.mmd", binary: false }))).toEqual(["Mermaid", "Source"]);
  expect(names(renderersFor({ path: "flow.MERMAID", binary: false }))).toEqual(["Mermaid", "Source"]);
  expect(names(renderersForFence("Mermaid"))).toEqual(["Mermaid", "Source"]);
  expect(names(renderersFor({ path: "flow.mmd", binary: true }))).toEqual([]);
});

test("PDF is built in: a .pdf, read as bytes, is PDF's alone, and it draws diffs (#2260, #2282)", () => {
  expect(names(renderersFor({ path: "docs/spec.pdf", binary: true }))).toEqual(["PDF"]);
  expect(names(renderersFor({ path: "Design.PDF", binary: true }))).toEqual(["PDF"]);
  expect(renderersFor({ path: "a.pdf", binary: true })[0]).toMatchObject({ id: "pdf", input: "bytes", fences: [], diff: true });
  expect(names(renderersForFence("pdf"))).toEqual(["Source"]);
});

test("D2 and Graphviz are built in: .d2, .dot and .gv files and d2, dot and graphviz fences are theirs first (#2259)", () => {
  expect(names(renderersFor({ path: "docs/flow.d2", binary: false }))).toEqual(["D2", "Source"]);
  expect(names(renderersFor({ path: "deps.DOT", binary: false }))).toEqual(["Graphviz", "Source"]);
  expect(names(renderersFor({ path: "graphs/deps.gv", binary: false }))).toEqual(["Graphviz", "Source"]);
  expect(names(renderersForFence("d2"))).toEqual(["D2", "Source"]);
  expect(names(renderersForFence("dot"))).toEqual(["Graphviz", "Source"]);
  expect(names(renderersForFence("GraphViz"))).toEqual(["Graphviz", "Source"]);
  expect(renderersForFence("d2")[0]).toMatchObject({ id: "d2", input: "text", output: "svg", diff: true });
  expect(renderersForFence("dot")[0]).toMatchObject({ id: "graphviz", input: "text", output: "svg", diff: true });
});

test("a renderer registered later goes ahead of Source, by file pattern and by fence", () => {
  const undo = register({ id: "boxes", name: "Boxes", patterns: ["*.box", "docs/*.boxes"], fences: ["boxes"], input: "text", output: "svg", diff: false });
  try {
    expect(names(renderersFor({ path: "docs/architecture.box", binary: false }))).toEqual(["Boxes", "Source"]);
    expect(names(renderersFor({ path: "docs/a.boxes", binary: false }))).toEqual(["Boxes", "Source"]);
    expect(names(renderersFor({ path: "other/a.boxes", binary: false }))).toEqual(["Source"]);
    expect(names(renderersForFence("Boxes"))).toEqual(["Boxes", "Source"]);
    expect(names(renderersForFence("rust"))).toEqual(["Source"]);
  } finally {
    undo();
  }
  expect(names(renderersFor({ path: "docs/architecture.box", binary: false }))).toEqual(["Source"]);
});

test("a repository's own .html opens as Source and offers Page; Page comes after Source (ADR-0046)", () => {
  const offered = renderersFor({ path: "site/index.html", binary: false });
  expect(offered[0]?.id).toBe("source");
  expect(offered.map((r) => r.name)).toEqual(["Source", "Page"]);
  expect(renderersFor({ path: "a.HTM", binary: false }).map((r) => r.id)).toEqual(["source", "html"]);
  // A renderer registered later still goes before Source, and so before Page.
  const off = register({ id: "x", name: "X", patterns: ["*.html"], fences: [], input: "text", output: "html", diff: false });
  expect(renderersFor({ path: "a.html", binary: false }).map((r) => r.id)).toEqual(["x", "source", "html"]);
  off();
});

test("an XML file opens in the XML view, Source one pick away; an SVG stays an image with the XML view next", () => {
  const ids = (path: string, text: string) => renderersFor({ path, binary: false, text }).map((r) => r.id);
  expect(ids("orders/order-4711.xml", '<?xml version="1.0"?>\n<ORDER/>')).toEqual(["xml", "source"]);
  expect(ids("build/App.csproj", "<Project Sdk='x'/>")).toEqual(["xml", "source"]);
  expect(ids("logo.svg", "<svg xmlns='urn:svg'/>")).toEqual(["image", "xml", "source"]);
  expect(ids("notes.txt", "<?xml version='1.0'?><a/>")).toEqual(["xml", "source"]);
  // The path reaches the sniff: an XML ending and a start tag, not a start tag alone.
  expect(ids("App.svelte", "<script>let a;</script>")).toEqual(["source"]);
  expect(ids("app.config", '{ "a": 1 }')).toEqual(["source"]);
  expect(ids("data.xml", "")).toEqual(["source"]);
});

const XS_DECL = `xmlns:xs="${ns("www.w3.org/2001/XMLSchema")}"`;

test("a schema opens as Schema, with the XML view and Source next; a .xsd that is no schema opens as XML", () => {
  const ids = (path: string, text: string) => renderersFor({ path, binary: false, text }).map((r) => r.id);
  expect(ids("schemas/order.xsd", `<?xml version="1.0"?>\n<xs:schema ${XS_DECL}/>`)).toEqual(["xsd", "xml", "source"]);
  expect(ids("schemas/odd.xsd", '<?xml version="1.0"?>\n<html/>')).toEqual(["xml", "source"]);
  // A schema at another name is the XML view's: the pattern is the cheap guard, the sniff the true one.
  expect(ids("schemas/order.xml", `<xs:schema ${XS_DECL}/>`)).toEqual(["xml", "source"]);
});
