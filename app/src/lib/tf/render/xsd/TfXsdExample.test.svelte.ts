/**
 * The example (the XSD design, sections 19 to 23) in the explorer, on this thread's engine: under 1,480 px the page's
 * switch shows it in the reference's place; its controls make it again; Check, Copy, Another and `A`; a choice picked
 * on its own line of the reference.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

const saved = vi.hoisted(() => [] as { name: string; text: string }[]);
vi.mock("../../../ipc/api", async (original) => ({
  ...(await original<typeof import("../../../ipc/api")>()),
  xsdLocations: async () => ({ pointed: [] }),
  xsdSaveExample: async (name: string, text: string) => (saved.push({ name, text }), `/home/me/Downloads/${name}`),
}));

import TfXsd from "./TfXsd.svelte";
import { localXsdEngine } from "../../../render/xsd-port";
import { familyFiles } from "../../../render/xsd-fixture";
import type { RenderPort } from "../../../render/port";
import { XS } from "../../../render/xsd-xml";

let target: HTMLElement;
let view: ReturnType<typeof mount> | null = null;
const quiet: RenderPort = { highlight: () => ({ focus: () => {}, cancel: () => {} }) };

beforeEach(() => {
  saved.length = 0;
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
/** A schema of the test's own with a choice whose branches both end. */
const CHOICE = `<xs:schema xmlns:xs="${XS}" targetNamespace="urn:c" xmlns="urn:c" elementFormDefault="qualified">
  <xs:element name="Pay"><xs:complexType><xs:choice><xs:element name="Card" type="xs:string"/><xs:element name="Invoice" type="xs:string"/></xs:choice></xs:complexType></xs:element>
</xs:schema>`;

function draw(files: Map<string, string>, entry: string, at: string) {
  view = mount(TfXsd, { target, props: { text: files.get(entry)!, path: entry, read: (p: string) => Promise.resolve(files.get(p) ?? null), paths: () => Promise.resolve([...files.keys()]), engine: localXsdEngine(), render: quiet, at } });
  flushSync();
}
async function until(what: () => boolean, rounds = 400) {
  for (let i = 0; i < rounds && !what(); i += 1) {
    await new Promise((resolve) => setTimeout(resolve, 10));
    flushSync();
  }
  flushSync();
}
const text = (el: Element | null | undefined) => el?.textContent?.replace(/\s+/g, " ").trim() ?? "";
const button = (label: string, within: ParentNode = target) => [...within.querySelectorAll<HTMLButtonElement>("button")].find((b) => text(b) === label);
const ex = () => target.querySelector(".tf-xsd-ex");
const count = () => text(target.querySelector(".tf-xsd-excount"));
const doc = () => [...target.querySelectorAll(".tf-xsd-xml .tf-l")].map((l) => l.textContent ?? "");

async function opened(files = OT, entry = ENTRY, at = "el/ORDER") {
  draw(files, entry, at);
  await until(() => button("Example") !== undefined);
  button("Example")!.click();
  flushSync();
  await until(() => /\d lines/.test(count()));
}

test("under 1,480 px the page's switch shows the example in the reference's place, labelled, with its controls", async () => {
  await opened();
  expect(text(ex()?.querySelector(".tf-xsd-exhead"))).toBe("Example Made from the schema");
  expect(target.querySelector(".tf-oas-page .tf-xsd-ex")).not.toBeNull();
  const group = (label: string) => [...target.querySelectorAll(`.tf-xsd-ctl [aria-label="${label}"] button`)].map((b) => text(b));
  expect(group("Parts")).toEqual(["Required only", "Everything"]);
  expect(group("Repeats")).toEqual(["1", "2", "3"]);
  expect([...target.querySelectorAll(".tf-xsd-ctl select option")].map((o) => text(o))).toEqual(["Deutsch", "English"]);
  // Its second line says what made it.
  await until(() => doc().length > 2);
  expect(doc()[1]).toContain("<!-- Made by knobas from opentrans_2_1.xsd (ORDER), seed 4711, required parts only. The values are invented. -->");
  button("Reference")!.click();
  flushSync();
  expect(target.querySelector(".tf-oas-page .tf-xsd-ex")).toBeNull();
});

test("each control makes it again: Everything is longer, a repeat longer still, the same settings the same text", async () => {
  await opened();
  const required = Number(count().replace(/\D/g, ""));
  button("Everything")!.click();
  await until(() => Number(count().replace(/\D/g, "")) > required);
  const all = Number(count().replace(/\D/g, ""));
  expect(all).toBeGreaterThan(required);
  button("3")!.click();
  await until(() => Number(count().replace(/\D/g, "")) > all);
  expect(Number(count().replace(/\D/g, ""))).toBeGreaterThan(all);
  button("1")!.click();
  await until(() => Number(count().replace(/\D/g, "")) === all);
  expect(Number(count().replace(/\D/g, ""))).toBe(all);
});

test("Check: libxml2 says Valid; Copy copies the document with its label; Save writes it under the element and the seed", async () => {
  const wrote: string[] = [];
  Object.defineProperty(navigator, "clipboard", { value: { writeText: async (t: string) => void wrote.push(t) }, configurable: true });
  await opened();
  button("Check")!.click();
  await until(() => /Valid|departure/.test(text(target.querySelector(".tf-xsd-verdict"))));
  expect(text(target.querySelector(".tf-xsd-verdict"))).toMatch(/^Valid against the schema, says libxml2\./);
  button("Copy")!.click();
  await until(() => wrote.length > 0);
  expect(wrote[0]!.split("\n").slice(0, 2)).toEqual(['<?xml version="1.0" encoding="UTF-8"?>', "<!-- Made by knobas from opentrans_2_1.xsd (ORDER), seed 4711, required parts only. The values are invented. -->"]);
  button("Save to Downloads")!.click();
  await until(() => saved.length > 0);
  expect(saved[0]!.name).toBe("ORDER-example-4711.xml");
  expect(saved[0]!.text).toBe(wrote[0]);
});

test("Another and A draw a new seed, never the one shown; the seed's field says it", async () => {
  await opened();
  const seed = () => (target.querySelector<HTMLInputElement>(".tf-xsd-seed")?.value ?? "");
  const random = vi.spyOn(Math, "random").mockReturnValueOnce((4711 - 1) / 99_999).mockReturnValueOnce(0.5);
  button("Another")!.click();
  flushSync();
  expect(seed()).not.toBe("4711");
  const after = seed();
  random.mockReturnValueOnce(0.25);
  target.querySelector(".tf-xsd")!.dispatchEvent(new KeyboardEvent("keydown", { key: "a", bubbles: true }));
  flushSync();
  expect(seed()).not.toBe(after);
  random.mockRestore();
});

test("a choice picked on its own line of the reference makes the example take that branch", async () => {
  const files = new Map([["c.xsd", CHOICE]]);
  await opened(files, "c.xsd", "el/Pay");
  await until(() => doc().some((l) => l.includes("<Card>")));
  // The reference shows the choice's line with what the example takes.
  button("Reference")!.click();
  await until(() => target.querySelector(".tf-xsd-takes select") !== null);
  const select = target.querySelector<HTMLSelectElement>(".tf-xsd-takes select")!;
  expect([...select.options].map((o) => o.textContent)).toEqual(["Card", "Invoice"]);
  select.value = "1";
  select.dispatchEvent(new Event("change", { bubbles: true }));
  flushSync();
  button("Example")!.click();
  await until(() => doc().some((l) => l.includes("<Invoice>")));
  expect(doc().some((l) => l.includes("<Card>"))).toBe(false);
});
