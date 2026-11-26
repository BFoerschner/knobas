/**
 * The JAR view (ux19) over the JAR fixture's archive reads (`render/jar-fixture.ts`, the shapes `knobas_jar` answers):
 * the head, the tree, an entry opened (a class's outline, a resource's text, an image), *Decompile* by what Java is
 * found, the fat JAR's dependency list and a library opened in place, the filter, an archive that does not read, a
 * handle the backend let go, and 10,000 entries drawn as a window.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import TfJar, { type JarPorts } from "./TfJar.svelte";
import { jarAnswers } from "../../render/jar-fixture";
import type { ArchiveIndex, ClassOutline } from "../../ipc/entity";
import type { JavaRuntime } from "../../ipc/app";

let target: HTMLElement;
let view: ReturnType<typeof mount> | null = null;

beforeEach(() => {
  target = document.createElement("div");
  document.body.appendChild(target);
  URL.createObjectURL ??= () => "blob:fake";
  URL.revokeObjectURL ??= () => {};
});

afterEach(() => {
  if (view) unmount(view);
  view = null;
  target.remove();
});

const PLAIN = "build/libs/ledger-core-2.3.1.jar";
const FAT = "build/libs/payout-service-0.0.1-SNAPSHOT.jar";

function portsFor(path: string, java = "", over: Partial<JarPorts> = {}): JarPorts & { calls: string[] } {
  const h = jarAnswers(java) as unknown as Record<string, (args: Record<string, unknown>) => unknown>;
  const calls: string[] = [];
  const call = <T>(name: string, args: Record<string, unknown>): Promise<T> => {
    calls.push(`${name} ${Object.values(args).join(" ")}`);
    return Promise.resolve(h[name]!(args) as T);
  };
  return {
    calls,
    open: () => call<ArchiveIndex>("archive_open", { path }),
    nested: (handle, entry) => call<ArchiveIndex>("archive_nested", { handle, entry }),
    entry: (handle, entry) => call<Uint8Array>("archive_entry", { handle, entry }),
    cls: (handle, entry) => call<ClassOutline>("archive_class", { handle, entry }),
    decompile: (handle, entry) => call<string>("archive_decompile", { handle, entry }),
    download: (handle, entry) => call<string>("archive_download", { handle, entry }),
    java: () => call<JavaRuntime>("java_runtime", {}),
    ...over,
  };
}

function draw(path: string, ports: JarPorts) {
  view = mount(TfJar, { target, props: { path, ports } });
  flushSync();
}

async function settle() {
  for (let round = 0; round < 6; round += 1) {
    for (let i = 0; i < 10; i += 1) await Promise.resolve();
    await new Promise((resolve) => setTimeout(resolve, 0));
    flushSync();
  }
}

const text = (el: Element | null | undefined) => el?.textContent?.replace(/\s+/g, " ").trim() ?? "";
const rows = () => [...target.querySelectorAll<HTMLButtonElement>(".tf-jar-rbtn")];
const row = (start: string) => rows().find((b) => text(b).startsWith(start));
const button = (label: string) => [...target.querySelectorAll<HTMLButtonElement>("button")].find((b) => text(b) === label);

function type(value: string) {
  const box = target.querySelector<HTMLInputElement>(".tf-jar-band input")!;
  box.value = value;
  box.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
}

test("a library JAR opens on its name, version, coordinates and facts, and its tree", async () => {
  draw(PLAIN, portsFor(PLAIN));
  await settle();
  expect(text(target.querySelector(".tf-jar-name"))).toBe("ledger-core");
  expect(text(target.querySelector(".tf-jar-ver"))).toBe("2.3.1");
  expect(text(target.querySelector(".tf-jar-sub"))).toBe("JAR com.tidewater:ledger-core");
  const facts = [...target.querySelectorAll(".tf-jar-facts dt")].map(text);
  expect(facts).toEqual(["Compiled for", "Module", "Built by", "Built with JDK", "Created by", "Description", "Size"]);
  expect(text(target.querySelector(".tf-jar-facts dd:last-of-type"))).toContain("11 entries");
  expect(text(target.querySelector(".tf-jar-count"))).toBe("11 entries");
  // No Dependencies: it holds no JARs.
  expect(button("Dependencies")).toBeUndefined();
  expect(rows().map((b) => text(b.querySelector(".tf-jar-dname, .tf-jar-fname")))).toEqual([
    "META-INF/",
    "com/tidewater/ledger/",
    "db/migration/",
    "ledger-defaults.properties",
    "logo.png",
  ]);
  expect(text(target.querySelector("summary"))).toBe("Manifest 7 fields");
});

test("a folder opens to its entries; a class opens in place to its outline, read once", async () => {
  const ports = portsFor(PLAIN);
  draw(PLAIN, ports);
  await settle();
  row("com/tidewater/ledger/")!.click();
  flushSync();
  expect(row("Ledger.class")).toBeDefined();
  row("Ledger.class")!.click();
  flushSync();
  // Nothing opens before its read is in: the entry draws whole, once.
  expect(row("Ledger.class")!.getAttribute("aria-expanded")).toBe("false");
  expect(target.querySelector(".tf-jar-detail")).toBeNull();
  await settle();
  expect(row("Ledger.class")!.getAttribute("aria-expanded")).toBe("true");
  const detail = target.querySelector(".tf-jar-detail")!;
  expect(text(detail.querySelector(".tf-jar-dline"))).toBe("public class Ledger");
  expect(text(detail.querySelector(".tf-jar-super"))).toBe("implements java.io.Serializable");
  expect([...detail.querySelectorAll(".tf-jar-glabel")].map(text)).toEqual(["Fields 2", "Constructors 1", "Methods 2"]);
  expect(text(detail)).toContain("private static final long serialVersionUID = 1L");
  expect(text(detail)).toContain("One member the compiler made is not listed.");
  row("Ledger.class")!.click();
  flushSync();
  expect(target.querySelector(".tf-jar-detail")).toBeNull();
  row("Ledger.class")!.click();
  await settle();
  expect(ports.calls.filter((c) => c.startsWith("archive_class"))).toHaveLength(1);
});

test("with a Java the decompiler runs on, Source decompiles the class; it says it is not the original", async () => {
  const ports = portsFor(PLAIN);
  draw(PLAIN, ports);
  await settle();
  row("com/tidewater/ledger/")!.click();
  flushSync();
  row("Ledger$Entry.class")!.click();
  await settle();
  button("Source")!.click();
  await settle();
  expect(ports.calls).toContain("archive_decompile a1 com/tidewater/ledger/Ledger$Entry.class");
  const note = text(target.querySelector(".tf-jar-detail .tf-jar-note"));
  expect(note).toContain("Ledger.class, which holds this class, decompiled by Vineflower 1.12.0");
  expect(note).toContain("It is not the original source");
  expect(text(target.querySelector(".tf-jar-code"))).toContain("public class Ledger implements java.io.Serializable");
  button("Outline")!.click();
  flushSync();
  expect(target.querySelector(".tf-jar-dline")).not.toBeNull();
});

test("with no Java there is no Decompile; with one too old it is there, disabled, its reason beside it", async () => {
  draw(PLAIN, portsFor(PLAIN, "nojava"));
  await settle();
  row("com/tidewater/ledger/")!.click();
  flushSync();
  row("Posting.class")!.click();
  await settle();
  expect(button("Source")).toBeUndefined();
  expect(button("Decompile")).toBeUndefined();
  unmount(view!);
  view = null;
  draw(PLAIN, portsFor(PLAIN, "oldjava"));
  await settle();
  row("com/tidewater/ledger/")!.click();
  flushSync();
  row("Posting.class")!.click();
  await settle();
  expect(button("Decompile")!.disabled).toBe(true);
  expect(text(target.querySelector(".tf-jar-why"))).toBe("Java 11 found; Decompile needs Java 17");
});

test("a resource opens with the file viewer's drawing: text as lines, an image in its frame", async () => {
  draw(PLAIN, portsFor(PLAIN));
  await settle();
  row("ledger-defaults.properties")!.click();
  await settle();
  const lines = [...target.querySelectorAll(".tf-jar-code .tf-l")].map(text);
  expect(lines).toEqual(["1ledger.currency=EUR", "2ledger.rounding=HALF_EVEN"]);
  row("logo.png")!.click();
  await settle();
  expect(target.querySelector(".tf-jar-img img")?.getAttribute("alt")).toBe("logo.png");
});

test("a fat JAR lists its dependencies with their versions, and one opens to its tree in place", async () => {
  const ports = portsFor(FAT);
  draw(FAT, ports);
  await settle();
  expect(text(target.querySelector(".tf-jar-sub"))).toBe("Spring Boot JAR com.tidewater:payout-service");
  const facts = [...target.querySelectorAll(".tf-jar-facts dt")].map(text);
  expect(facts.slice(0, 3)).toEqual(["Compiled for", "Application class", "Launcher"]);
  button("Dependencies")!.click();
  flushSync();
  expect(text(target.querySelector(".tf-jar-count"))).toBe("150 JARs");
  const first = rows()[0]!;
  expect(text(first.querySelector(".tf-jar-fname"))).toBe("spring-boot");
  expect(text(first.querySelector(".tf-jar-vers"))).toBe("3.2.0");
  first.click();
  await settle();
  expect(ports.calls).toContain("archive_nested a1 BOOT-INF/lib/spring-boot-3.2.0.jar");
  expect(first.getAttribute("aria-expanded")).toBe("true");
  // Its tree one level in.
  expect(rows().slice(1, 3).map((b) => text(b.querySelector(".tf-jar-dname")))).toEqual(["META-INF/", "org/springboot/"]);
  type("jackson");
  expect(text(target.querySelector(".tf-jar-count"))).toBe("20 of 150");
});

test("10,000 entries draw a window of rows; the filter counts every match", async () => {
  draw(FAT, portsFor(FAT));
  await settle();
  expect(text(target.querySelector(".tf-jar-count"))).toBe("10,000 entries");
  type("payout");
  expect(text(target.querySelector(".tf-jar-count"))).toMatch(/^\d{1,3}(,\d{3})* of 10,000$/);
  const drawn = rows().length;
  expect(drawn).toBeGreaterThan(0);
  expect(drawn).toBeLessThan(300);
  type("payoutevent5.class");
  expect(rows().map((b) => text(b.querySelector(".tf-jar-fname")))).toEqual(["BOOT-INF/classes/com/tidewater/payout/domain/events/PayoutEvent5.class"]);
  expect(text(target.querySelector(".tf-jar-count"))).toBe("1 of 10,000");
});

test("an archive that does not read says so, with why", async () => {
  draw(PLAIN, portsFor(PLAIN, "", { open: () => Promise.reject({ code: "invalid", message: "not a zip archive (no end of central directory)", source_id: null }) }));
  await settle();
  expect(text(target.querySelector(".tf-jar-bad"))).toBe("ledger-core-2.3.1.jar could not be read as an archive. not a zip archive (no end of central directory)");
});

test("an archive the backend let go is opened again once, and the read goes on", async () => {
  let gone = true;
  const base = portsFor(PLAIN);
  const ports = portsFor(PLAIN, "", {
    cls: (handle, entry) => {
      if (gone) {
        gone = false;
        return Promise.reject({ code: "not_found", message: `archive ${handle} is no longer open; open it again`, source_id: null });
      }
      return base.cls(handle, entry);
    },
  });
  const open = vi.spyOn(ports, "open");
  draw(PLAIN, ports);
  await settle();
  row("com/tidewater/ledger/")!.click();
  flushSync();
  row("rules/")!.click();
  flushSync();
  row("Rounding.class")!.click();
  await settle();
  // Once to draw, once again when the class's read found the handle gone.
  expect(open).toHaveBeenCalledTimes(2);
  expect(text(target.querySelector(".tf-jar-dline"))).toBe("public class Rounding");
});

test("Save to Downloads saves the archive, and says where", async () => {
  const ports = portsFor(PLAIN);
  draw(PLAIN, ports);
  await settle();
  button("Save to Downloads")!.click();
  await settle();
  expect(ports.calls).toContain("archive_download a1 ");
  expect(text(target.querySelector(".tf-jar-saved"))).toBe("Saved to /Users/mara/Downloads/ledger-core-2.3.1.jar");
});

test("Alt+click on a JAR folder recursively expands and collapses all nested subdirectories", async () => {
  draw(PLAIN, portsFor(PLAIN));
  await settle();
  const pkg = row("com/tidewater/ledger/")!;
  pkg.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true, altKey: true }));
  flushSync();
  expect(pkg.getAttribute("aria-expanded")).toBe("true");
  expect(row("rules/")!.getAttribute("aria-expanded")).toBe("true");
  expect(row("Rounding.class")).toBeDefined();

  // Alt+click again collapses the folder and its nested directories.
  pkg.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true, altKey: true }));
  flushSync();
  expect(pkg.getAttribute("aria-expanded")).toBe("false");
  expect(row("rules/")).toBeUndefined();
  pkg.click();
  flushSync();
  expect(row("rules/")!.getAttribute("aria-expanded")).toBe("false");
});

test("Alt+click collapsing a JAR folder also collapses open directories inside nested archives under it", async () => {
  const outer: ArchiveIndex = {
    handle: "outer",
    kind: "war",
    size: 100,
    total: 50,
    packed: 40,
    names: "WEB-INF/lib/core.jar",
    sizes: [50],
    packed_sizes: [40],
    crcs: [1],
    flags: [0],
    manifest: [],
    maven: [],
    release: null,
    releases: [],
    libs: [],
    prefix: 0,
    warnings: [],
  };
  const inner: ArchiveIndex = {
    handle: "inner",
    kind: "jar",
    size: 50,
    total: 30,
    packed: 30,
    names: "META-INF/MANIFEST.MF\ncom/app/sub/A.class",
    sizes: [10, 20],
    packed_sizes: [10, 20],
    crcs: [2, 3],
    flags: [0, 0],
    manifest: [],
    maven: [],
    release: null,
    releases: [],
    libs: [],
    prefix: 0,
    warnings: [],
  };
  const ports = portsFor("app.war", "", {
    open: () => Promise.resolve(outer),
    nested: () => Promise.resolve(inner),
  });
  draw("app.war", ports);
  await settle();
  row("WEB-INF/lib/")!.click();
  flushSync();
  row("core.jar")!.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true, altKey: true }));
  await settle();
  expect(row("com/app/sub/")!.getAttribute("aria-expanded")).toBe("true");

  // Alt+click WEB-INF/lib/ collapses the folder, the nested archive, and the nested archive's open directories.
  row("WEB-INF/lib/")!.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true, altKey: true }));
  flushSync();
  expect(row("WEB-INF/lib/")!.getAttribute("aria-expanded")).toBe("false");
  row("WEB-INF/lib/")!.click();
  flushSync();
  expect(row("core.jar")!.getAttribute("aria-expanded")).toBe("false");
  row("core.jar")!.click();
  await settle();
  expect(row("com/app/sub/")!.getAttribute("aria-expanded")).toBe("false");
});
