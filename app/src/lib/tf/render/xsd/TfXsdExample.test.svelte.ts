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
import { forgetHeld, shown } from "./example-held.svelte";
import { layers } from "../../layers.svelte";
import { localXsdEngine } from "../../../render/xsd-port";
import { familyFiles } from "../../../render/xsd-fixture";
import type { RenderPort } from "../../../render/port";
import { XS } from "../../../render/xsd-xml";

let target: HTMLElement;
let view: ReturnType<typeof mount> | null = null;
const quiet: RenderPort = { highlight: () => ({ focus: () => {}, cancel: () => {} }) };

beforeEach(() => {
  saved.length = 0;
  // An example's settings are held for its schema until knobas quits: each test starts from the first ones.
  forgetHeld();
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

test("a tick on an optional row puts it in a Required only example; its count says how often; untick takes it out", async () => {
  const schema = `<xs:schema xmlns:xs="${XS}" targetNamespace="urn:t" xmlns="urn:t" elementFormDefault="qualified">
  <xs:element name="Note"><xs:complexType><xs:sequence><xs:element name="Must" type="xs:string"/><xs:element name="Maybe" type="xs:string" minOccurs="0" maxOccurs="3"/></xs:sequence></xs:complexType></xs:element>
</xs:schema>`;
  await opened(new Map([["t.xsd", schema]]), "t.xsd", "el/Note");
  const maybes = () => doc().filter((l) => l.includes("<Maybe>")).length;
  expect(maybes()).toBe(0);
  button("Reference")!.click();
  await until(() => target.querySelector(".tf-xsd-tick input") !== null);
  // Only the optional row carries a tick; Must stands anyway.
  expect([...target.querySelectorAll(".tf-xsd-tick input")].map((i) => i.getAttribute("aria-label"))).toEqual(["Put Maybe in the example"]);
  const tick = target.querySelector<HTMLInputElement>(".tf-xsd-tick input")!;
  tick.checked = true;
  tick.dispatchEvent(new Event("change", { bubbles: true }));
  flushSync();
  const times = target.querySelector<HTMLSelectElement>(".tf-xsd-tick select")!;
  expect(times.disabled).toBe(false);
  button("Example")!.click();
  await until(() => maybes() === 1);
  expect(maybes()).toBe(1);
  button("Reference")!.click();
  flushSync();
  const again = target.querySelector<HTMLSelectElement>(".tf-xsd-tick select")!;
  again.value = "2";
  again.dispatchEvent(new Event("change", { bubbles: true }));
  flushSync();
  button("Example")!.click();
  await until(() => maybes() === 2);
  expect(maybes()).toBe(2);
  button("Reference")!.click();
  flushSync();
  const off = target.querySelector<HTMLInputElement>(".tf-xsd-tick input")!;
  off.checked = false;
  off.dispatchEvent(new Event("change", { bubbles: true }));
  flushSync();
  button("Example")!.click();
  // The example made again: Note's own two lines and Must, as before the tick.
  await until(() => count() === "5 lines");
  expect(maybes()).toBe(0);
});

// ---- The example as a miniature XML view, and opened in the XML view (E1) ----


test("the example is drawn by the XML view's document: Levels and folds, no rail; the controls hold their place when it is made again", async () => {
  await opened();
  expect(ex()?.querySelector(".tf-xsd-xml .tf-xv-inline")).not.toBeNull();
  expect(ex()?.querySelector(".tf-xv-rail")).toBeNull();
  await until(() => (ex()?.querySelectorAll(".tf-xv-caret").length ?? 0) > 0);
  expect(ex()!.querySelectorAll(".tf-xv-step button").length).toBe(3);
  const acts = target.querySelector(".tf-xsd-acts")!;
  const before = [...acts.children].map((c) => text(c));
  button("Everything")!.click();
  flushSync();
  // While it is made again, the box and the buttons stay; only the count says it.
  expect([...acts.children].slice(0, 4).map((c) => text(c))).toEqual(before.slice(0, 4));
  expect(ex()?.querySelector(".tf-xsd-xml .tf-xv-inline")).not.toBeNull();
});

test("Check marks the example's departures at their lines' ends in the miniature view", async () => {
  await opened();
  // Valid examples have no marks.
  button("Check")!.click();
  await until(() => /Valid/.test(text(target.querySelector(".tf-xsd-verdict"))));
  expect(target.querySelectorAll(".tf-xsd-xml .tf-xv-bad").length).toBe(0);
});

function drawAt(files: Map<string, string>, entry: string, place: { at: string }, onplace: (p: string, how: string, label: string) => void, onback = vi.fn()) {
  view = mount(TfXsd, {
    target,
    props: {
      text: files.get(entry)!,
      path: entry,
      repo: "r",
      read: (p: string) => Promise.resolve(files.get(p) ?? null),
      paths: () => Promise.resolve([...files.keys()]),
      engine: localXsdEngine(),
      render: quiet,
      get at() {
        return place.at;
      },
      onplace,
      onback,
    },
  });
  flushSync();
}

test("Open in XML view pushes the example's place; there the full XML view stands over the hidden explorer, checked against this schema", async () => {
  const place = $state({ at: "el/ORDER" });
  const asked: [string, string, string][] = [];
  const onback = vi.fn();
  drawAt(OT, ENTRY, place, (p, how, label) => asked.push([p, how, label]), onback);
  await until(() => button("Example") !== undefined);
  button("Example")!.click();
  await until(() => /\d lines/.test(count()));
  button("Open in XML view")!.click();
  expect(asked.at(-1)).toEqual(["example/ORDER", "push", "ORDER-example-4711.xml"]);
  place.at = "example/ORDER";
  await until(() => target.querySelector(".tf-xsd-opened .tf-xv-rail") !== null);
  const under = target.querySelector<HTMLElement>(".tf-xsd.tf-xsd-under")!;
  expect(under.hasAttribute("inert") || (under as HTMLElement & { inert?: boolean }).inert === true).toBe(true);
  await until(() => /Valid|departure/.test(text(target.querySelector(".tf-xsd-opened .tf-xv-verdict"))));
  expect(text(target.querySelector(".tf-xsd-opened .tf-xv-head"))).toContain("Against opentrans_2_1.xsd, found where the example was made.");
  expect(shown.head?.name).toBe("ORDER-example-4711.xml");
  expect(shown.head?.meta).toMatch(/^Example made by knobas, seed 4711, required parts only\. [\d,]+ lines\. Not saved\.$/);
  // One verdict: the column's says the same, with no press.
  expect(text(target.querySelector(".tf-xsd-verdict"))).toMatch(/^Valid/);
  // A does nothing while the view shows.
  const seed = () => target.querySelector<HTMLInputElement>(".tf-xsd-seed")?.value ?? "";
  target.querySelector(".tf-xsd-opened .tf-l")!.dispatchEvent(new KeyboardEvent("keydown", { key: "a", bubbles: true }));
  flushSync();
  expect(seed()).toBe("4711");
  // Esc leaves it by Back, since a press here opened it.
  const panel = layers.open.at(-1);
  expect(panel?.kind).toBe("panel");
  panel!.close();
  expect(onback).toHaveBeenCalledTimes(1);
  place.at = "el/ORDER";
  await until(() => target.querySelector(".tf-xsd-opened") === null);
  expect(target.querySelector(".tf-xsd-under")).toBeNull();
  expect(shown.head).toBeNull();
});

test("an address straight to an example makes it with the settings held for the schema; Esc there goes to the element", async () => {
  const place = $state({ at: "el/ORDER" });
  drawAt(OT, ENTRY, place, () => {});
  await until(() => button("Example") !== undefined);
  button("Example")!.click();
  await until(() => /\d lines/.test(count()));
  const field = target.querySelector<HTMLInputElement>(".tf-xsd-seed")!;
  field.value = "99";
  field.dispatchEvent(new Event("change", { bubbles: true }));
  flushSync();
  unmount(view!);
  view = null;
  const asked: [string, string, string][] = [];
  const cold = $state({ at: "example/ORDER" });
  drawAt(OT, ENTRY, cold, (p, how, label) => asked.push([p, how, label]));
  await until(() => target.querySelector(".tf-xsd-opened .tf-l") !== null);
  expect(shown.head?.name).toBe("ORDER-example-99.xml");
  layers.open.at(-1)!.close();
  expect(asked.at(-1)).toEqual(["el/ORDER", "replace", "ORDER"]);
});
