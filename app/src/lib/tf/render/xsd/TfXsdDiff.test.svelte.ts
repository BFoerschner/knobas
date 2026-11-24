/**
 * A schema in a comparison (S7): the counts' line, a row a declaration in the file's order with which way it went,
 * a row opened to its facts and both sides' declarations, `n` stepping through, a reformatted schema the same, a side
 * that is no schema, a file only on one side, and a set's other file read through the diff's reader.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import TfXsdDiff from "./TfXsdDiff.svelte";
import { localXsdEngine } from "../../../render/xsd-port";
import { XS } from "../../../render/xsd-xml";

let target: HTMLElement;
let view: ReturnType<typeof mount> | null = null;
let said = "";

beforeEach(() => {
  target = document.createElement("div");
  document.body.appendChild(target);
  Element.prototype.scrollIntoView = function () {};
  said = "";
});
afterEach(() => {
  if (view) unmount(view);
  view = null;
  target.remove();
});

const schema = (body: string, extra = "") => `<xs:schema xmlns:xs="${XS}" targetNamespace="urn:t" xmlns="urn:t" elementFormDefault="qualified"${extra}>${body}</xs:schema>`;
const BEFORE = schema(`<xs:simpleType name="Id"><xs:restriction base="xs:string"><xs:maxLength value="20"/></xs:restriction></xs:simpleType>
<xs:complexType name="Party"><xs:sequence><xs:element name="ID" type="Id"/></xs:sequence></xs:complexType>
<xs:element name="Order"><xs:complexType><xs:sequence><xs:element name="Buyer" type="Party"/><xs:element name="Note" type="xs:string" minOccurs="0"/></xs:sequence></xs:complexType></xs:element>
<xs:element name="Gone" type="xs:string"/>`);
const AFTER = schema(`<xs:simpleType name="Id"><xs:restriction base="xs:string"><xs:maxLength value="10"/></xs:restriction></xs:simpleType>
<xs:complexType name="Party"><xs:sequence><xs:element name="ID" type="Id"/></xs:sequence></xs:complexType>
<xs:element name="Order"><xs:complexType><xs:sequence><xs:element name="Buyer" type="Party"/><xs:element name="Note" type="xs:string"/></xs:sequence></xs:complexType></xs:element>
<xs:element name="New" type="xs:string" minOccurs="0"/>`);

function draw(before: string | null, after: string | null, read: (side: "old" | "new", path: string) => Promise<string | null> = () => Promise.resolve(null), path = "s.xsd") {
  view = mount(TfXsdDiff, { target, props: { before, after, path, read, engine: localXsdEngine(), onsay: (w: string) => (said = w) } });
  flushSync();
}
async function until(what: () => boolean, rounds = 300) {
  for (let i = 0; i < rounds && !what(); i += 1) {
    await new Promise((resolve) => setTimeout(resolve, 10));
    flushSync();
  }
  flushSync();
}
const text = (el: Element | null | undefined) => el?.textContent?.replace(/\s+/g, " ").trim() ?? "";
const rows = () => [...target.querySelectorAll<HTMLElement>(".tf-xsd-crow")];

test("the counts' line and a row a declaration, in the file's order, then the ones changed through", async () => {
  draw(BEFORE, AFTER);
  expect(text(target.querySelector(".tf-oas-wait"))).toBe("Comparing the two sides…");
  await until(() => rows().length > 0);
  expect(text(target.querySelector(".tf-xsd-csum"))).toBe("2 declarations changed, 1 added, 1 removed. 2 stricter, 0 looser, 0 different. 1 more changed through a type it names. 1 reads the same, however it is written.");
  expect(said).toBe("2 declarations changed, 1 added, 1 removed");
  expect(rows().map((r) => `${r.dataset["mark"]} ${text(r.querySelector(".tf-xsd-cname"))} | ${text(r.querySelector(".tf-xsd-dirs"))}`)).toEqual([
    "changed Id | stricter",
    "changed Order | stricter",
    "added New | ",
    "removed Gone | ",
    "through Party Id | ",
  ]);
  expect(text(rows()[0]!.querySelector(".tf-xsd-csay"))).toBe("longest: 20 → 10");
});

test("a row opened says each fact and draws the declaration on both sides; n steps on and opens the next", async () => {
  draw(BEFORE, AFTER);
  await until(() => rows().length > 0);
  rows()[1]!.querySelector("button")!.click();
  flushSync();
  expect([...rows()[1]!.querySelectorAll(".tf-xsd-cfacts li")].map((li) => text(li))).toEqual(["Note optional, now it must stand stricter"]);
  await until(() => rows()[1]!.querySelectorAll(".tf-xsd-hname").length === 2);
  expect([...rows()[1]!.querySelectorAll(".tf-xsd-cwhich")].map((w) => text(w))).toEqual(["Before", "After"]);
  // A declaration gone: the side after says so.
  const stage = view as unknown as { nextChange: (step: 1 | -1, entering?: boolean) => boolean };
  expect(stage.nextChange(1, true)).toBe(true);
  expect(stage.nextChange(1)).toBe(true);
  expect(stage.nextChange(1)).toBe(true);
  flushSync();
  expect(rows()[2]!.classList.contains("tf-xsd-cur")).toBe(true);
  await until(() => text(rows()[2]!.querySelector(".tf-xsd-cpair")).includes("Not declared before."));
  expect(text(rows()[2]!.querySelector(".tf-xsd-cpair"))).toContain("Not declared before.");
  for (let i = 0; i < 3; i += 1) stage.nextChange(1);
  expect(stage.nextChange(1)).toBe(false);
});

test("a reformatted schema is the same; a side that is no schema says so; a file only after lists everything added", async () => {
  const pretty = AFTER.replace(/></g, ">\n  <").replace(`xmlns="urn:t"`, `xmlns="urn:t"  `);
  draw(AFTER, pretty);
  await until(() => text(target.querySelector(".tf-xsd-csum")) !== "");
  expect(text(target.querySelector(".tf-xsd-csum"))).toBe("The same schema on both sides. 4 declarations read the same, however they are written.");
  expect(said).toBe("The same schema on both sides.");
  unmount(view!);
  draw("<notes/>", AFTER);
  await until(() => rows().length > 0);
  expect(text(target.querySelector(".tf-xsd-cnot"))).toBe("Not an XML Schema on the side before.");
  expect(said).toBe("4 declarations, all added");
  unmount(view!);
  draw("<a/>", "<b/>");
  expect(text(target.querySelector(".tf-xsd-cmp"))).toBe("Neither side is an XML Schema: Lines has the change.");
});

test("a set's other file is read through the diff's reader, at each side, and its declarations are names, not rows", async () => {
  const types = (len: string) => schema(`<xs:simpleType name="Code"><xs:restriction base="xs:string"><xs:maxLength value="${len}"/></xs:restriction></xs:simpleType>`);
  const main = schema(`<xs:include schemaLocation="types.xsd"/><xs:element name="Item" type="Code"/>`);
  const asked: string[] = [];
  draw(main, main, (side, p) => (asked.push(`${side} ${p}`), Promise.resolve(p === "types.xsd" ? types(side === "old" ? "5" : "9") : null)));
  await until(() => rows().length > 0);
  expect(asked.sort()).toEqual(["new types.xsd", "old types.xsd"]);
  // The compared file's own declarations are compared: the included Code is no row of its own, and Item, which names
  // it, changed through it.
  expect(rows().map((r) => `${r.dataset["mark"]} ${text(r.querySelector(".tf-xsd-cname"))}`)).toEqual(["through Item Code"]);
});

test("a side that is no schema is never sent to the worker", async () => {
  const local = localXsdEngine();
  const opened: string[] = [];
  const engine = (session: number, call: { kind: string; entry?: string; text?: string }) => {
    if (call.kind === "open") opened.push(call.text ?? "");
    return local(session, call as never);
  };
  view = mount(TfXsdDiff, { target, props: { before: "<notes/>", after: AFTER, path: "s.xsd", read: () => Promise.resolve(null), engine } });
  flushSync();
  await until(() => rows().length > 0);
  expect(opened).toEqual([AFTER]);
});
