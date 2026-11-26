/**
 * A JAR in a comparison (2026-11-22, Fable's ruling), in the one diff component as its callers mount it: a changed JAR
 * opens *Rendered*, its sides listed by the archive read (never the byte read), the head line and only the entries that
 * differ; a row opens to its line diff, a changed class to its decompiled source or its outline. Where the caller reads
 * no archives it stays "A binary file".
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import type { JavaRuntime } from "../../ipc/app";
import type { ArchiveIndex, ClassOutline, DiffFile } from "../../ipc/entity";
import type { ArchivePorts } from "../../render/jar-ports";
import { plainPort } from "../../render/port";
import { classMode } from "./class-mode.svelte";
import { createDiffLayout } from "./diff-layout.svelte";
import TfDiffView from "./TfDiffView.svelte";

const JAR = (path: string, change: DiffFile["change"] = "binary"): DiffFile => ({ path, old_path: null, change, additions: 0, deletions: 0, hunks: [], truncated: false, threads: [] });

/** An archive of `entries`, each `[name, text]`, its CRC the text's own stand-in. */
function jar(handle: string, entries: [string, string][], version: string): ArchiveIndex {
  const sorted = [...entries].sort(([a], [b]) => (a < b ? -1 : 1));
  const crc = (s: string) => [...s].reduce((h, c) => (Math.imul(h, 31) + c.charCodeAt(0)) >>> 0, 7);
  return {
    handle,
    kind: "jar",
    size: 1,
    total: 0,
    packed: 0,
    names: sorted.map(([n]) => n).join("\n"),
    sizes: sorted.map(([, t]) => t.length),
    packed_sizes: sorted.map(([, t]) => t.length),
    crcs: sorted.map(([, t]) => crc(t)),
    flags: sorted.map(() => 0),
    manifest: [],
    maven: [{ group: "t", artifact: "ledger-core", version, name: null, description: null, packaging: "jar", path: "x" }],
    release: "Java 17",
    releases: [],
    libs: [],
    prefix: 0,
    warnings: [],
  };
}

const BEFORE: [string, string][] = [
  ["META-INF/MANIFEST.MF", "Manifest-Version: 1.0\n"],
  ["app.properties", "rate=1\nlimit=5\n"],
  ["com/t/Ledger.class", "ledger v1"],
  ["com/t/Same.class", "same"],
  ["old.txt", "gone"],
];
const AFTER: [string, string][] = [
  ["META-INF/MANIFEST.MF", "Manifest-Version: 1.0\n"],
  ["app.properties", "rate=2\nlimit=5\n"],
  ["com/t/Ledger.class", "ledger v2"],
  ["com/t/Same.class", "same"],
  ["new.txt", "here"],
];
const SIDES: Record<string, [string, string][]> = { a: BEFORE, b: AFTER };

function outline(name: string, method: string): ClassOutline {
  return {
    major: 61, minor: 0, release: "Java 17", kind: "class", modifiers: "public", name, type_params: "", extends: null, implements: [], permits: [], annotations: [], components: [],
    fields: [],
    methods: [{ annotations: [], modifiers: "public", type_params: "", ty: "void", name: method, params: "", throws: [], value: null, constant: false, deprecated: false }],
    synthetic: 0, source_file: null, deprecated: false, module: [],
  } as ClassOutline;
}

function fakes(java: JavaRuntime) {
  const asked: string[] = [];
  const archives: Omit<ArchivePorts, "open"> = {
    nested: () => Promise.reject(new Error("none")),
    entry: (handle, entry) => (asked.push(`entry ${handle} ${entry}`), Promise.resolve(new TextEncoder().encode(SIDES[handle]!.find(([n]) => n === entry)![1]))),
    cls: (handle, entry) => (asked.push(`cls ${handle} ${entry}`), Promise.resolve(outline("com.t.Ledger", handle === "a" ? "post" : "postAll"))),
    decompile: (handle, entry) => (asked.push(`decompile ${handle} ${entry}`), Promise.resolve(`class Ledger {\n  void ${handle === "a" ? "post" : "postAll"}() {}\n}\n`)),
    download: () => Promise.resolve(""),
    java: () => Promise.resolve(java),
  };
  return { archives, asked };
}
const JAVA: JavaRuntime = { java: { path: "/j", version: "21", release: 21 } as never, decompiler: "Vineflower 1.12.0", needs: 17, can_decompile: true, reason: null, set_path: null };

let target: HTMLElement;
let views: ReturnType<typeof mount>[] = [];
beforeEach(() => {
  target = document.createElement("div");
  document.body.appendChild(target);
  // The app's one choice, as a fresh start finds it: Source.
  classMode.reset();
});
afterEach(() => {
  views.forEach((view) => unmount(view));
  views = [];
  target.remove();
});

function draw(files: DiffFile[], opts: { archive: boolean; java?: JavaRuntime }) {
  const listed: string[] = [];
  const read: string[] = [];
  const { archives, asked } = fakes(opts.java ?? JAVA);
  const context = {
    text: () => Promise.resolve(null),
    bytes: (f: DiffFile) => (read.push(f.path), Promise.resolve(new Uint8Array())),
    ...(opts.archive
      ? {
          archive: (f: DiffFile, side: "old" | "new") => {
            listed.push(`${side}:${f.path}`);
            // A JAR git lists as binary says not whether it was added: its side before is simply not there.
            if (f.path.startsWith("added/") && side === "old") return Promise.resolve(null);
            return Promise.resolve(side === "old" ? jar("a", BEFORE, "2.3.0") : jar("b", AFTER, "2.3.1"));
          },
        }
      : {}),
  };
  views.push(mount(TfDiffView, { target, props: { files, context, archives, render: plainPort, layout: createDiffLayout(() => null), defaults: { list: [] } } as never }));
  flushSync();
  return { listed, read, asked };
}

const settle = async () => {
  for (let n = 0; n < 40; n += 1) await Promise.resolve();
  flushSync();
};
const card = () => target.querySelector(".tf-diff")!;
const rowTexts = () => [...card().querySelectorAll(".tf-jdiff-row")].map((r) => `${r.querySelector(".tf-bd")!.textContent} ${r.querySelector(".tf-jar-fname")!.textContent}`);
const rowOf = (path: string) => [...card().querySelectorAll<HTMLButtonElement>(".tf-jdiff-row")].find((r) => r.querySelector(".tf-jar-fname")!.textContent === path)!;
/** Each drawn line as `old new text`, its numbers and its text in their own cells. */
const hunkText = () =>
  [...card().querySelectorAll(".tf-jar-detail .tf-pdfd-hunk .tf-ln")].map((l) => [...l.children].map((c) => c.textContent!.trim()).join(" ").replace(/\s+/g, " ").trim());

test("a changed JAR opens Rendered: its sides listed by the archive read, the head line and only what differs", async () => {
  const { listed, read } = draw([JAR("build/libs/ledger-core.jar")], { archive: true });
  await settle();
  expect([...card().querySelectorAll(".tf-fh .tf-seg button")].map((b) => `${b.textContent}${b.getAttribute("aria-pressed") === "true" ? "*" : ""}`)).toEqual(["Bytes", "Rendered*"]);
  expect(listed.sort()).toEqual(["new:build/libs/ledger-core.jar", "old:build/libs/ledger-core.jar"]);
  expect(read, "never through the byte read").toEqual([]);
  expect(card().querySelector(".tf-jdiff-head")?.textContent).toBe("ledger-core 2.3.0 → 2.3.1 · JAR · Java 17");
  expect(rowTexts()).toEqual(["M app.properties", "M com/t/Ledger.class", "A new.txt", "D old.txt"]);
  expect(card().querySelector(".tf-rfoot-say")?.textContent).toBe("2 changed, 1 added, 1 removed; 2 the same");
  expect(card().textContent).not.toContain("A binary file");
});

test("where the caller reads no archives a JAR stays a binary file, with no Rendered", async () => {
  draw([JAR("build/libs/ledger-core.jar")], { archive: false });
  await settle();
  expect(card().querySelector(".tf-jdiff")).toBeNull();
  expect(card().textContent).toContain("A binary file: no lines to draw.");
});

test("a changed text entry opens to its line diff; an added one to its text", async () => {
  const { asked } = draw([JAR("ledger-core.jar")], { archive: true });
  await settle();
  rowOf("app.properties").click();
  await settle();
  expect(asked).toEqual(["entry a app.properties", "entry b app.properties"]);
  expect(hunkText()).toEqual(["1 − rate=1", "1 + rate=2", "2 2 limit=5"]);
  expect(rowOf("app.properties").getAttribute("aria-expanded")).toBe("true");
});

test("a changed class diffs its decompiled source; Outline in the caption diffs its outline instead", async () => {
  const { asked } = draw([JAR("ledger-core.jar")], { archive: true });
  await settle();
  const seg = [...card().querySelectorAll<HTMLButtonElement>(".tf-rcap .tf-seg button")];
  expect(seg.map((b) => `${b.textContent}${b.getAttribute("aria-pressed") === "true" ? "*" : ""}`)).toEqual(["Outline", "Source*"]);
  expect(asked, "nothing is decompiled before a press").toEqual([]);
  rowOf("com/t/Ledger.class").click();
  await settle();
  expect(asked).toEqual(["decompile a com/t/Ledger.class", "decompile b com/t/Ledger.class"]);
  expect(card().querySelector(".tf-jar-detail .tf-jar-dnote")?.textContent).toBe("Decompiled by Vineflower 1.12.0 on both sides: not the original source.");
  expect(hunkText()).toContain("2 − void post() {}");
  seg[0]!.click();
  await settle();
  expect(asked.slice(2)).toEqual(["cls a com/t/Ledger.class", "cls b com/t/Ledger.class"]);
  expect(hunkText()).toEqual(["1 1 public class Ledger", "2 − public void post()", "2 + public void postAll()"]);
});

test("with no Java there is no Source: a changed class opens to its outline", async () => {
  const { asked } = draw([JAR("ledger-core.jar")], { archive: true, java: { ...JAVA, java: null, can_decompile: false } });
  await settle();
  expect(card().querySelector(".tf-rcap .tf-seg")).toBeNull();
  rowOf("com/t/Ledger.class").click();
  await settle();
  expect(asked.filter((a) => a.startsWith("decompile"))).toEqual([]);
  expect(hunkText()).toEqual(["1 1 public class Ledger", "2 − public void post()", "2 + public void postAll()"]);
});

test("an added JAR lists every entry added", async () => {
  draw([JAR("added/ledger-core.jar")], { archive: true });
  await settle();
  expect(rowTexts()).toEqual(AFTER.map(([n]) => `A ${n}`).sort((a, b) => (a < b ? -1 : 1)));
  expect(card().querySelector(".tf-rfoot-say")?.textContent).toBe("5 entries, all added");
});

test("Alt+click on a nested archive in JAR diff recursively expands and collapses nested archives", async () => {
  const nestedCalls: string[] = [];
  const earOld = jar("ear-old", [["app.war", "war-v1"]], "1.0.0");
  const earNew = jar("ear-new", [["app.war", "war-v2"]], "1.0.1");
  const warOld = jar("war-old", [["lib/core.jar", "core-v1"]], "1.0.0");
  const warNew = jar("war-new", [["lib/core.jar", "core-v2"]], "1.0.1");
  const coreOld = jar("core-old", [["a.txt", "before"]], "1.0.0");
  const coreNew = jar("core-new", [["a.txt", "after"]], "1.0.1");
  const archives: Omit<ArchivePorts, "open"> = {
    nested: (handle, entry) => {
      nestedCalls.push(`${handle}:${entry}`);
      if (handle === "ear-old") return Promise.resolve(warOld);
      if (handle === "ear-new") return Promise.resolve(warNew);
      if (handle === "war-old") return Promise.resolve(coreOld);
      if (handle === "war-new") return Promise.resolve(coreNew);
      return Promise.reject(new Error("unexpected"));
    },
    entry: () => Promise.resolve(new Uint8Array()),
    cls: () => Promise.resolve(outline("A", "m")),
    decompile: () => Promise.resolve(""),
    download: () => Promise.resolve(""),
    java: () => Promise.resolve(JAVA),
  };
  const context = {
    text: () => Promise.resolve(null),
    bytes: () => Promise.resolve(new Uint8Array()),
    archive: (_f: DiffFile, side: "old" | "new") => Promise.resolve(side === "old" ? earOld : earNew),
  };
  views.push(mount(TfDiffView, { target, props: { files: [JAR("app.ear")], context, archives, render: plainPort, layout: createDiffLayout(() => null), defaults: { list: [] } } as never }));
  flushSync();
  await settle();

  rowOf("app.war").dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true, altKey: true }));
  await settle();
  expect(rowTexts()).toEqual(["M app.war", "M lib/core.jar", "M a.txt"]);
  expect(nestedCalls).toHaveLength(4);

  // Alt+click again collapses app.war and lib/core.jar together.
  rowOf("app.war").dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true, altKey: true }));
  flushSync();
  expect(rowTexts()).toEqual(["M app.war"]);
  rowOf("app.war").click();
  await settle();
  expect(rowTexts()).toEqual(["M app.war", "M lib/core.jar"]);
});
