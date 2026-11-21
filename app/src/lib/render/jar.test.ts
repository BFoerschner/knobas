/** The JAR view's model (ux19): the tree from the listing, its rows, the filter, the dependency list, the head. */
import { expect, test } from "vitest";

import type { ArchiveIndex, ClassOutline } from "../ipc/entity";
import { bytes, chain, declaration, filteredRows, head, kindOf, libRows, listing, memberLine, supers, terms, textOf, topClass, treeRows, type Nested, type Row } from "./jar";

function index(names: string[], over: Partial<ArchiveIndex> = {}): ArchiveIndex {
  const sorted = [...names].sort();
  return {
    handle: "a1",
    kind: "jar",
    size: 1000,
    total: sorted.length * 10,
    packed: sorted.length * 5,
    names: sorted.join("\n"),
    sizes: sorted.map(() => 10),
    packed_sizes: sorted.map(() => 5),
    flags: sorted.map(() => 0),
    manifest: [],
    maven: [],
    release: null,
    releases: [],
    libs: [],
    prefix: 0,
    warnings: [],
    ...over,
  };
}

const NONE = { dirs: new Set<string>(), files: new Set<string>() };
const label = (r: Row) => ("label" in r ? r.label : r.kind === "lib" ? r.name : r.kind);

test("the tree holds every entry under its folders, sizes and counts summed, folders before files", () => {
  const l = listing(index(["META-INF/MANIFEST.MF", "com/x/A.class", "com/x/B.class", "com/x/y/C.class", "app.yml"]));
  expect(l.tree.count).toBe(5);
  expect(l.tree.size).toBe(50);
  expect(l.tree.dirs.map((d) => d.name)).toEqual(["META-INF", "com"]);
  expect(l.tree.files.map((i) => l.names[i])).toEqual(["app.yml"]);
  const com = l.tree.dirs[1]!;
  expect(com.count).toBe(3);
  expect(com.dirs[0]!.files.map((i) => l.names[i])).toEqual(["com/x/A.class", "com/x/B.class"]);
  // Folders sort by name even where a file sorts between their first entries.
  const l2 = listing(index(["b/x", "a-b", "a/y"]));
  expect(l2.tree.dirs.map((d) => d.name)).toEqual(["a", "b"]);
});

test("a folder holding only one folder is one row with it", () => {
  const l = listing(index(["com/example/demo/App.class", "com/example/demo/web/W.class"]));
  const { label: text, end } = chain(l.tree.dirs[0]!);
  expect(text).toBe("com/example/demo");
  expect(end.path).toBe("com/example/demo/");
  const rows = treeRows(l, "", 0, NONE, new Map());
  expect(rows.map(label)).toEqual(["com/example/demo"]);
  const open = { dirs: new Set(["|com/example/demo/"]), files: new Set<string>() };
  const opened = treeRows(l, "", 0, open, new Map());
  expect(opened.map(label)).toEqual(["com/example/demo", "web", "App.class"]);
  expect(opened.map((r) => r.depth)).toEqual([0, 1, 1]);
});

test("an entry opened has its detail row below it; a nested JAR opened draws its own tree one level in", () => {
  const l = listing(index(["BOOT-INF/lib/x-1.0.jar", "a.yml"]));
  const inner = listing(index(["org/x/X.class"]));
  const nested = new Map<string, Nested>([["BOOT-INF/lib/x-1.0.jar", { ok: true, listing: inner }]]);
  const open = { dirs: new Set(["|BOOT-INF/lib/", "BOOT-INF/lib/x-1.0.jar|org/x/"]), files: new Set(["|a.yml", "|BOOT-INF/lib/x-1.0.jar"]) };
  // `BOOT-INF/lib` is one row: `BOOT-INF` holds only it.
  const rows = treeRows(l, "", 0, { ...open, dirs: new Set([...open.dirs]) }, nested);
  expect(rows.map((r) => [r.kind, label(r), r.depth])).toEqual([
    ["dir", "BOOT-INF/lib", 0],
    ["file", "x-1.0.jar", 1],
    ["dir", "org/x", 2],
    ["file", "X.class", 3],
    ["file", "a.yml", 0],
    ["detail", "detail", 0],
  ]);
  // Keys are unique across kinds and scopes.
  expect(new Set(rows.map((r) => r.k)).size).toBe(rows.length);
  // A nested archive that could not be read says why in its place.
  const failed = new Map<string, Nested>([["BOOT-INF/lib/x-1.0.jar", { ok: false, error: "not a zip archive" }]]);
  const withNote = treeRows(l, "", 0, open, failed);
  expect(withNote.find((r) => r.kind === "note")).toMatchObject({ text: "not a zip archive", depth: 2 });
});

test("the filter lists every entry whose path holds every word, the open nested archives' too", () => {
  const l = listing(index(["com/x/PayoutService.class", "com/x/PayoutMapper.class", "com/y/Other.class", "BOOT-INF/lib/p-1.jar"]));
  const inner = listing(index(["org/p/PayoutHelper.class"]));
  const nested = new Map<string, Nested>([["BOOT-INF/lib/p-1.jar", { ok: true, listing: inner }]]);
  const { rows, hits } = filteredRows(l, terms("payout SERVICE"), NONE, nested);
  expect(rows.map(label)).toEqual(["com/x/PayoutService.class"]);
  expect(hits).toBe(1);
  const all = filteredRows(l, terms("payout"), NONE, nested);
  expect(all.rows.map(label)).toEqual(["com/x/PayoutMapper.class", "com/x/PayoutService.class", "BOOT-INF/lib/p-1.jar!org/p/PayoutHelper.class"]);
  expect(all.hits).toBe(2);
  expect(terms("  ")).toEqual([]);
});

test("the dependency list is the archive's nested JARs by name and version, filtered by either", () => {
  const ix = index(["BOOT-INF/lib/a-1.jar", "BOOT-INF/lib/b-2.0.jar"], {
    libs: [
      { entry: "BOOT-INF/lib/a-1.jar", name: "a", version: "1", size: 10 },
      { entry: "BOOT-INF/lib/b-2.0.jar", name: "b", version: "2.0", size: 20 },
    ],
  });
  expect(libRows(ix, [], NONE, new Map()).map((r) => [label(r), r.kind === "lib" ? r.version : null])).toEqual([["a", "1"], ["b", "2.0"]]);
  expect(libRows(ix, terms("2.0"), NONE, new Map()).map(label)).toEqual(["b"]);
});

test("the head names the artifact, its version and coordinates, and the manifest's facts", () => {
  const ix = index(["a"], {
    kind: "boot-jar",
    release: "Java 17",
    releases: [21],
    manifest: [
      ["Manifest-Version", "1.0"],
      ["Main-Class", "org.springframework.boot.loader.launch.JarLauncher"],
      ["Start-Class", "com.x.App"],
      ["Built-By", "mara"],
      ["Build-Jdk-Spec", "17"],
      ["Class-Path", "a.jar b.jar"],
      ["X-Custom", "yes"],
    ],
    maven: [{ group: "com.x", artifact: "demo", version: "1.2.3", name: null, description: null, packaging: null, path: "p/" }],
  });
  const h = head(ix, "demo-1.2.3.jar");
  expect([h.title, h.version, h.coords, h.kind]).toEqual(["demo", "1.2.3", "com.x:demo", "Spring Boot JAR"]);
  expect(h.facts.map((f) => [f.label, f.value])).toEqual([
    ["Compiled for", "Java 17, and Java 21 where it runs on it"],
    ["Application class", "com.x.App"],
    ["Launcher", "org.springframework.boot.loader.launch.JarLauncher"],
    ["Built by", "mara"],
    ["Built with JDK", "17"],
  ]);
  expect(h.classPath).toEqual(["a.jar", "b.jar"]);
  expect(h.rest).toEqual([["X-Custom", "yes"]]);
  // No Maven files: the name and version come from the file's name.
  const bare = head(index(["a"]), "guava-32.1.3-jre.jar");
  expect([bare.title, bare.version, bare.coords]).toEqual(["guava", "32.1.3-jre", null]);
});

test("entries open by what their names say; text is text only when it decodes", () => {
  expect(["A.class", "x.JAR", "a.png", "b.pdf", "c.md", "d.yml"].map(kindOf)).toEqual(["class", "archive", "image", "pdf", "markdown", "file"]);
  expect(textOf(new TextEncoder().encode("key: value\n"))).toBe("key: value\n");
  expect(textOf(Uint8Array.of(0xca, 0xfe, 0, 1))).toBeNull();
  expect(textOf(Uint8Array.of(0xff, 0xfe, 0x41))).toBeNull();
  expect([bytes(12), bytes(1_536), bytes(61_204_118), bytes(2_147_483_648)]).toEqual(["12 B", "1.5 KB", "58.4 MB", "2.00 GB"]);
  expect(topClass("a/b/Foo$Bar$1.class")).toBe("a/b/Foo.class");
  expect(topClass("Foo.class")).toBe("Foo.class");
  expect(topClass("a/$Odd.class")).toBe("a/$Odd.class");
});

test("a class's declaration and members read as source writes them", () => {
  const o: ClassOutline = {
    major: 61, minor: 0, release: "Java 17", kind: "record", modifiers: "public", name: "com.x.Shelf$Book", type_params: "",
    extends: null, implements: ["Comparable<Book>"], permits: [], annotations: [], components: ["String title", "int pages"],
    fields: [], methods: [], synthetic: 0, source_file: null, deprecated: false, module: [],
  };
  expect(declaration(o)).toBe("public record Shelf.Book(String title, int pages)");
  expect(supers(o)).toEqual([{ word: "implements", names: ["Comparable<Book>"] }]);
  expect(supers({ ...o, kind: "interface", implements: ["A"] })[0]!.word).toBe("extends");
  const base = { annotations: [], type_params: "", throws: [], value: null, deprecated: false, constant: false };
  expect(memberLine({ ...base, modifiers: "public static final", ty: "int", name: "MAX", params: null, value: "42" })).toBe("public static final int MAX = 42");
  expect(memberLine({ ...base, modifiers: "public", type_params: "<T>", ty: "java.util.List<T>", name: "find", params: "String q", throws: ["java.io.IOException"] })).toBe(
    "public <T> java.util.List<T> find(String q) throws java.io.IOException",
  );
  expect(memberLine({ ...base, modifiers: "", ty: "", name: "RED", params: null, constant: true })).toBe("RED");
});

test("10,000 entries list and draw their first rows quickly", () => {
  const names = Array.from({ length: 10_000 }, (_, i) => `BOOT-INF/classes/com/t/p${i % 40}/C${i}.class`);
  const ix = index(names);
  const started = performance.now();
  const l = listing(ix);
  const rows = treeRows(l, "", 0, { dirs: new Set(["|BOOT-INF/classes/com/t/"]), files: new Set() }, new Map());
  const took = performance.now() - started;
  expect(l.tree.count).toBe(10_000);
  expect(rows).toHaveLength(41);
  // jsdom on a loaded machine; the browser is a few milliseconds.
  expect(took).toBeLessThan(400);
});
