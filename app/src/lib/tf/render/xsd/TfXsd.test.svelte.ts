/**
 * The schema explorer (the XSD design, sections 5 to 13) on this thread's engine, on openTRANS's fixture: the shell
 * drawn before the files it names are read, the overview, the rail by tree, element and type, a page with *how often*,
 * rows opened in place, a name followed into BMEcat and Back, picks that replace and links that push, the filter
 * across files, a file not found here, a place an address names and one the set lacks.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

const kept = vi.hoisted(() => ({ pointed: [] as unknown[], downloaded: new Map<string, string>() }));
vi.mock("../../../ipc/api", async (original) => ({
  ...(await original<typeof import("../../../ipc/api")>()),
  xsdLocations: async () => structuredClone(kept),
  xsdSetLocations: async (list: { pointed: unknown[] }) => ((kept.pointed = structuredClone(list.pointed)), structuredClone(kept)),
  xsdFetch: async (address: string) => ({ url: address, sha256: "d".repeat(64), size: 1, at: "2026-11-24" }),
  xsdDownloaded: async (sha: string) => kept.downloaded.get(sha) ?? null,
}));

import TfXsd from "./TfXsd.svelte";
import { localXsdEngine } from "../../../render/xsd-port";
import { familyFiles } from "../../../render/xsd-fixture";
import { ns } from "../../../render/xml-names";
import { XS } from "../../../render/xsd-xml";

let target: HTMLElement;
let view: ReturnType<typeof mount> | null = null;

beforeEach(() => {
  kept.pointed = [];
  kept.downloaded.clear();
  target = document.createElement("div");
  document.body.appendChild(target);
  Element.prototype.scrollIntoView = function () {};
});
afterEach(() => {
  if (view) unmount(view);
  view = null;
  target.remove();
});

const OT = familyFiles("opentrans-2.1", "schemas/opentrans/");
const ENTRY = "schemas/opentrans/opentrans_2_1.xsd";

function draw(files: Map<string, string> = OT, extra: Record<string, unknown> = {}) {
  const reads: string[] = [];
  const read = vi.fn((p: string) => {
    reads.push(p);
    return Promise.resolve(files.get(p) ?? null);
  });
  const paths = vi.fn(() => Promise.resolve([...files.keys()]));
  const onplace = vi.fn();
  view = mount(TfXsd, { target, props: { text: files.get(ENTRY)!, path: ENTRY, read, paths, engine: localXsdEngine(), onplace, ...extra } });
  flushSync();
  return { read, reads, paths, onplace };
}

/** Waits until the explorer has drawn (the first mount loads the engine's modules). */
async function drawn() {
  for (let i = 0; i < 200 && target.querySelector(".tf-oas-page") === null; i += 1) await new Promise((resolve) => setTimeout(resolve, 10));
  await settle();
}

async function settle(rounds = 12) {
  for (let round = 0; round < rounds; round += 1) {
    for (let i = 0; i < 10; i += 1) await Promise.resolve();
    await new Promise((resolve) => setTimeout(resolve, 0));
    flushSync();
  }
}

const text = (el: Element | null | undefined) => el?.textContent?.replace(/\s+/g, " ").trim() ?? "";
const rail = () => [...target.querySelectorAll<HTMLButtonElement>(".tf-oas-list .tf-oas-row")];
const railText = () => rail().map((b) => text(b));
const page = () => target.querySelector(".tf-oas-page")!;
const button = (label: string, within: ParentNode = target) => [...within.querySelectorAll<HTMLButtonElement>("button")].find((b) => text(b) === label);

test("the overview: what a document starts with, the four files and how each was found", async () => {
  draw();
  await drawn();
  expect(text(page().querySelector(".tf-oas-title"))).toBe("opentrans_2_1.xsd");
  expect(text(page())).toContain("283 elements and 13 types in this file; with the 3 files it reaches, 848 declarations.");
  expect(text(page())).toContain("A document starts with");
  expect(text(page().querySelector(".tf-xsd-roots"))).toContain("OPENTRANS");
  const files = [...page().querySelectorAll(".tf-xsd-frow")].map((r) => text(r));
  expect(files).toHaveLength(4);
  expect(files[1]).toContain("bmecat_2005.xsd");
  expect(files[1]).toContain("imported, beside it");
  expect(text(page())).toContain("These files hold no documentation.");
});

test("the shell draws from the opened file alone: the files it names say reading… until their reads land", async () => {
  let release: () => void = () => {};
  const gate = new Promise<void>((r) => (release = r));
  const read = (p: string) => gate.then(() => OT.get(p) ?? null);
  view = mount(TfXsd, { target, props: { text: OT.get(ENTRY)!, path: ENTRY, read, paths: () => Promise.resolve([...OT.keys()]), engine: localXsdEngine() } });
  await settle(4);
  expect(page().querySelectorAll(".tf-xsd-frow")).toHaveLength(4);
  expect(text(page().querySelectorAll(".tf-xsd-frow")[1]!)).toContain("reading…");
  release();
  await settle();
  expect(text(page().querySelectorAll(".tf-xsd-frow")[1]!)).not.toContain("reading…");
});

test("the tree: OPENTRANS open one level, its documents with how often; a fold opens", async () => {
  draw();
  await settle();
  expect(railText().slice(0, 4)).toEqual(["Overview", "OPENTRANS root", "RFQ 0..1", "QUOTATION 0..1"]);
  const order = rail().find((b) => text(b).startsWith("ORDER "))!;
  order.querySelector<SVGElement>(".tf-oas-caret")!.dispatchEvent(new MouseEvent("click", { bubbles: true }));
  await settle();
  expect(railText()).toContain("ORDER_HEADER 1");
  // How often is loud where the element must stand, quiet where it may be left out.
  const header = rail().find((b) => text(b) === "ORDER_HEADER 1")!;
  expect(header.querySelector(".tf-xsd-occ")!.classList.contains("tf-xsd-may")).toBe(false);
  expect(rail().find((b) => text(b).startsWith("RFQ"))!.querySelector(".tf-xsd-occ")!.classList.contains("tf-xsd-may")).toBe(true);
});

test("a pick in the rail replaces the page: ORDER's attributes first, then what it holds, with how often", async () => {
  const { onplace } = draw();
  await settle();
  rail().find((b) => text(b).startsWith("ORDER "))!.click();
  await settle();
  expect(text(page().querySelector(".tf-xsd-h"))).toBe("ORDER Element");
  expect(onplace).toHaveBeenLastCalledWith("el/ORDER", "replace", "ORDER");
  const names = [...page().querySelectorAll(".tf-oas-fname")].map((el) => text(el));
  expect(names.slice(0, 5)).toEqual(["@version", "@type", "ORDER_HEADER", "ORDER_ITEM_LIST", "ORDER_SUMMARY"]);
  expect(text(page())).toContain("Used by 1");
});

test("a row opens in place; a name followed into BMEcat keeps the rail and says where it lives; Back returns", async () => {
  const { onplace } = draw(OT, { at: "el/ORDER" });
  await settle();
  const header = [...page().querySelectorAll<HTMLElement>(".tf-oas-fr.tf-oas-can")].find((r) => text(r.querySelector(".tf-oas-fname")) === "ORDER_HEADER")!;
  header.click();
  await settle();
  expect(header.getAttribute("aria-expanded")).toBe("true");
  const inner = [...header.parentElement!.querySelectorAll(":scope > .tf-oas-in .tf-oas-fname")].map((el) => text(el));
  expect(inner).toContain("CONTROL_INFO");
  // A name from BMEcat, deeper down: open ORDER_INFO and follow its first bmecat: name.
  const info = [...header.parentElement!.querySelectorAll<HTMLElement>(".tf-oas-fr.tf-oas-can")].find((r) => text(r.querySelector(".tf-oas-fname")) === "ORDER_INFO")!;
  info.click();
  await settle();
  const foreign = [...page().querySelectorAll<HTMLButtonElement>("button.tf-oas-ref")].find((b) => text(b).startsWith("bmecat:"))!;
  const label = text(foreign);
  foreign.click();
  await settle();
  expect(text(page().querySelector(".tf-xsd-hname"))).toBe(label);
  expect(text(page().querySelector(".tf-oas-meta"))).toContain("in bmecat_2005.xsd, which this file reaches");
  expect(onplace).toHaveBeenLastCalledWith(expect.stringMatching(new RegExp(`^(el|type)/${label}$`)), "push", label);
  expect(target.querySelector(".tf-oas-rail")).not.toBeNull();
  button("Back to ORDER")!.click();
  await settle();
  expect(text(page().querySelector(".tf-xsd-hname"))).toBe("ORDER");
});

test("Elements lists a fold a file, the opened file's open; Types lists types with their kind", async () => {
  draw();
  await settle();
  button("Elements")!.click();
  await settle();
  expect(railText()[1]).toBe("opentrans_2_1.xsd 283");
  button("Types")!.click();
  await settle();
  expect(railText()[1]).toBe("opentrans_2_1.xsd 13");
  expect(railText().some((r) => r.endsWith("simple"))).toBe(true);
  button("Elements")!.click();
  await settle();
  // The opened file's fold shut, the other files' folds stand under it.
  rail()[1]!.click();
  await settle();
  expect(railText()).toContain("bmecat_2005.xsd 440");
});

test("words typed draw one flat list across files; clearing them gives the folds back", async () => {
  // No backend here: the order answers every row whose text holds every word, as it does when the ranker is away.
  draw();
  await settle();
  const before = railText();
  const box = target.querySelector<HTMLInputElement>(".tf-oas-railhead input")!;
  box.value = "price amount";
  box.dispatchEvent(new Event("input", { bubbles: true }));
  await settle();
  expect(railText()).toContain("bmecat:PRICE_AMOUNT element");
  expect(railText().every((r) => /price/i.test(r) && /amount/i.test(r))).toBe(true);
  expect(text(target.querySelector(".tf-oas-count"))).toMatch(/of 848$/);
  box.value = "";
  box.dispatchEvent(new Event("input", { bubbles: true }));
  await settle();
  expect(railText()).toEqual(before);
});

test("a type with 157 values shows 48 and the rest behind Show the other", async () => {
  draw(OT, { at: "type/bmecat:dtCURRENCIES" });
  await settle();
  expect(text(page())).toContain("One of 157");
  expect(page().querySelectorAll(".tf-xsd-vals code")).toHaveLength(48);
  button("Show the other 109")!.click();
  await settle();
  expect(page().querySelectorAll(".tf-xsd-vals code")).toHaveLength(157);
});

test("BME's own BMEcat beside openTRANS: the reference is not found here, with its reason and Point to a file…", async () => {
  const files = new Map(OT);
  files.set("schemas/opentrans/bmecat_2005.xsd", familyFiles("bmecat-2005fd").get("bmecat_2005.xsd")!);
  const onpoint = vi.fn();
  draw(files, { onpoint });
  await settle();
  const lost = page().querySelector(".tf-xsd-nf")!;
  expect(text(lost)).toContain(`bmecat_2005.xsd is there, but it declares ${ns("www.bmecat.org/bmecat/2005fd")}.`);
  button("Point to a file…", lost)!.click();
  expect(onpoint).toHaveBeenCalledWith(ns("www.bmecat.org/bmecat/2005"), "bmecat_2005.xsd");
  expect(text(page())).toContain("not found here are drawn as written and marked");
});

test("a place the set does not have opens the overview and says so; no read of the repository's paths when all is beside", async () => {
  const { paths } = draw(OT, { at: "el/NO_SUCH" });
  await settle();
  expect(text(page())).toContain("This link named el/NO_SUCH, which this version of the file does not have.");
  expect(paths).not.toHaveBeenCalled();
});

test("the rail follows the page: an element named by the address opens the tree down to it, the shortest way", async () => {
  draw(OT, { at: "el/ORDER_HEADER" });
  await settle(20);
  const current = rail().find((b) => b.getAttribute("aria-current") === "true");
  expect(text(current)).toBe("ORDER_HEADER 1");
  // OPENTRANS › ORDER › ORDER_HEADER: the fold above it opened.
  expect(railText()).toContain("ORDER 0..1");
  expect(rail().find((b) => text(b) === "ORDER 0..1")!.getAttribute("aria-expanded")).toBe("true");
});

test("Point to a file…: the picker lists the repository's schemas; one that declares another namespace is dim and not pickable", async () => {
  const files = new Map(OT);
  files.set("schemas/opentrans/bmecat_2005.xsd", familyFiles("bmecat-2005fd").get("bmecat_2005.xsd")!);
  draw(files);
  await settle();
  button("Point to a file…", page().querySelector(".tf-xsd-nf")!)!.click();
  await settle();
  const rows = [...document.querySelectorAll<HTMLButtonElement>(".tf-xsd-pointrow")];
  const named = (r: HTMLButtonElement) => text(r.querySelector(".tf-xsd-pointpath"));
  // Started on the location's name: BME's own file, which declares another namespace.
  expect(rows.map(named)).toEqual(["schemas/opentrans/bmecat_2005.xsd"]);
  expect(rows[0]!.disabled).toBe(true);
  expect(text(rows[0]!)).toContain(`declares ${ns("www.bmecat.org/bmecat/2005fd")}`);
});

/** openTRANS without its BMEcat: the import is answered by another synced repository, or two. */
const BME = "schemas/opentrans/bmecat_2005.xsd";
const BMECAT = ns("www.bmecat.org/bmecat/2005");
function withoutBmecat() {
  const files = new Map(OT);
  const bme = files.get(BME)!;
  files.delete(BME);
  return { files, bme };
}
const catalog = { repo: "gitea:tidewater/catalog-tools", name: "tidewater/catalog-tools", branch: "main", path: "lib/bmecat_2005.xsd" };
const mirror = { repo: "gitea:acme/schemas", name: "acme/schemas", branch: "trunk", path: "bmecat/bmecat_2005.xsd" };

test("another synced repository answers an import by itself where it is the only one: the row says which, and at what branch", async () => {
  const { files, bme } = withoutBmecat();
  const readOther = vi.fn((repo: string, path: string) => Promise.resolve(repo === catalog.repo && path === catalog.path ? bme : null));
  const elsewhere = vi.fn(() => Promise.resolve([catalog]));
  draw(files, { readOther, elsewhere });
  await drawn();
  await settle(20);
  expect(elsewhere).toHaveBeenCalledWith(BMECAT);
  const row = [...page().querySelectorAll(".tf-xsd-frow")].find((r) => text(r).includes("bmecat_2005.xsd"))!;
  expect(text(row)).toContain("in tidewater/catalog-tools, at main");
  expect(readOther).toHaveBeenCalledWith(catalog.repo, catalog.path);
  expect(page().querySelector(".tf-xsd-nf")).toBeNull();
});

test("two repositories declare it: the row asks once, with both; the pick is kept as a pointed file and read", async () => {
  const { files, bme } = withoutBmecat();
  const readOther = vi.fn(() => Promise.resolve(bme));
  draw(files, { readOther, elsewhere: () => Promise.resolve([catalog, mirror]) });
  await drawn();
  await settle(20);
  const lost = page().querySelector(".tf-xsd-nf")!;
  expect(text(lost)).toContain("Two repositories declare it.");
  // Nothing was read from either before the person picks.
  expect(readOther).not.toHaveBeenCalled();
  const offered = [...lost.querySelectorAll<HTMLButtonElement>(".tf-xsd-two button")].map((b) => text(b));
  expect(offered).toEqual(["Use tidewater/catalog-tools: lib/bmecat_2005.xsd", "Use acme/schemas: bmecat/bmecat_2005.xsd"]);
  button("Use acme/schemas: bmecat/bmecat_2005.xsd", lost)!.click();
  await settle(30);
  expect(kept.pointed).toEqual([{ namespace: BMECAT, location: "bmecat_2005.xsd", to: { repo: mirror.repo, path: mirror.path, name: "acme/schemas" } }]);
  expect(readOther).toHaveBeenCalledWith(mirror.repo, mirror.path);
  expect(page().querySelector(".tf-xsd-nf")).toBeNull();
  const row = [...page().querySelectorAll(".tf-xsd-frow")].find((r) => text(r).includes("bmecat_2005.xsd"))!;
  expect(text(row)).toContain("you pointed to it");
});

test("the picker lists another repository's schemas that declare the namespace, after this one's, with their branch", async () => {
  const files = new Map(OT);
  files.set(BME, familyFiles("bmecat-2005fd").get("bmecat_2005.xsd")!);
  draw(files, { elsewhere: () => Promise.resolve([catalog, mirror]) });
  await settle(20);
  button("Point to a file…", page().querySelector(".tf-xsd-nf")!)!.click();
  await settle();
  const rows = [...document.querySelectorAll<HTMLButtonElement>(".tf-xsd-pointrow")];
  expect(rows.map((r) => text(r.querySelector(".tf-xsd-pointpath")))).toEqual([
    "tidewater/catalog-tools, at main: lib/bmecat_2005.xsd",
    "acme/schemas, at trunk: bmecat/bmecat_2005.xsd",
    BME,
  ]);
  expect(rows.map((r) => r.disabled)).toEqual([false, false, true]);
});

const seenLines = () => [...page().querySelectorAll(".tf-xsd-seen li")].map((li) => text(li));

test("Seen while reading: a namespace two files here declare says which was taken, the nearest", async () => {
  const { files, bme } = withoutBmecat();
  files.set("vendor/bme/bmecat_2005.xsd", bme);
  files.set("vendor/old/bme/bmecat_2005.xsd", bme);
  draw(files);
  await drawn();
  await settle(20);
  expect(seenLines()).toContain(`2 files here declare ${BMECAT}; knobas took the nearest, bmecat_2005.xsd.`);
});

test("Seen while reading: a repository with more schemas than the index reads says how many it looked at", async () => {
  const { files, bme } = withoutBmecat();
  // 320 schemas beside openTRANS's three and BMEcat: the index reads 300 of them, the nearest folders first.
  for (let i = 0; i < 320; i += 1) files.set(`schemas/opentrans/z/${i}.xsd`, `<xs:schema xmlns:xs="${XS}" targetNamespace="urn:n${i}"/>`);
  files.set("schemas/opentrans/b/bmecat_2005.xsd", bme);
  draw(files);
  await drawn();
  await settle(30);
  expect([...files.keys()].filter((p) => p.endsWith(".xsd"))).toHaveLength(324);
  expect(seenLines()).toContain("knobas looked at 300 of 324 .xsd files here.");
});

test("an import at an address is offered, never fetched; Download it keeps the file and the set reads it from there", async () => {
  const { files, bme } = withoutBmecat();
  const address = ns("schemas.example/bmecat/bmecat_2005.xsd");
  files.set(ENTRY, files.get(ENTRY)!.replace(/schemaLocation="bmecat_2005\.xsd"/, `schemaLocation="${address}"`));
  kept.downloaded.set("d".repeat(64), bme);
  draw(files);
  await drawn();
  await settle(20);
  const lost = page().querySelector(".tf-xsd-nf")!;
  expect(text(lost)).toContain("It is an address. knobas fetches none by itself.");
  button("Download it", lost)!.click();
  await settle(30);
  expect(kept.pointed).toEqual([{ namespace: BMECAT, location: address, to: { url: address, sha256: "d".repeat(64), at: "2026-11-24" } }]);
  expect(page().querySelector(".tf-xsd-nf")).toBeNull();
  const row = [...page().querySelectorAll(".tf-xsd-frow")].find((r) => text(r).includes("bmecat_2005.xsd"))!;
  expect(text(row)).toContain("downloaded");
});

