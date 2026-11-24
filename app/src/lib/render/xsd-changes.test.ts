/**
 * A schema compared by what it declares (the XSD design, section 27; S7): each row of the design's table, formatting
 * that is no change, an order changed, a declaration changed through another, a rename that is one removed and one
 * added, and a side that holds no schema.
 */
import { expect, test } from "vitest";
import { loadSet, type SchemaSet } from "./xsd";
import { changes } from "./xsd-changes";
import { changesOf } from "./xsd-answer";
import { familyFiles } from "./xsd-fixture";
import { XS } from "./xsd-xml";

const F = "s.xsd";
const schema = (body: string) => `<xs:schema xmlns:xs="${XS}" targetNamespace="urn:t" xmlns="urn:t" elementFormDefault="qualified">${body}</xs:schema>`;
const setOf = (text: string, path = F): SchemaSet => loadSet({ entry: path, texts: new Map([[path, text]]), paths: null, pointed: [], index: null, elsewhere: null, bundled: new Map() });
/** The facts that changed in the one changed declaration, as `place: text dir`. */
function facts(before: string, after: string): string[] {
  const ch = changes(setOf(schema(before)), setOf(schema(after)), F, F);
  return ch.changed.flatMap((c) => c.facts.map((f) => `${f.place}: ${f.text.replace(/[\u0001\u0002]/g, "")} ${f.dir ?? "-"}`));
}
const seq = (inner: string) => `<xs:element name="Order"><xs:complexType><xs:sequence>${inner}</xs:sequence></xs:complexType></xs:element>`;
const A = `<xs:element name="A" type="xs:string"/>`;

test("a required element added, an optional one made required, minOccurs up, maxOccurs down: stricter", () => {
  expect(facts(seq(A), seq(`${A}<xs:element name="B" type="xs:string"/>`))).toEqual(["B: added, and it must stand (1) stricter"]);
  expect(facts(seq(`<xs:element name="A" type="xs:string" minOccurs="0"/>`), seq(A))).toEqual(["A: optional, now it must stand stricter"]);
  expect(facts(seq(`<xs:element name="A" type="xs:string" maxOccurs="9"/>`), seq(`<xs:element name="A" type="xs:string" minOccurs="2" maxOccurs="9"/>`))).toEqual(["A: 1..9 → 2..9 stricter"]);
  expect(facts(seq(`<xs:element name="A" type="xs:string" maxOccurs="unbounded"/>`), seq(`<xs:element name="A" type="xs:string" maxOccurs="5"/>`))).toEqual(["A: 1..∞ → 1..5 stricter"]);
});

test("an element or a required attribute removed, a value removed, a pattern added, a length narrowed, fixed added: stricter", () => {
  expect(facts(seq(`${A}<xs:element name="B" type="xs:string" minOccurs="0"/>`), seq(A))).toEqual(["B: removed stricter"]);
  const st = (inner: string) => `<xs:simpleType name="Code"><xs:restriction base="xs:string">${inner}</xs:restriction></xs:simpleType>`;
  expect(facts(st(`<xs:enumeration value="a"/><xs:enumeration value="b"/>`), st(`<xs:enumeration value="a"/>`))).toEqual(["allowed values: removed b stricter"]);
  expect(facts(st(""), st(`<xs:pattern value="[A-Z]+"/>`))).toEqual(["pattern: now held to [A-Z]+ stricter"]);
  expect(facts(st(`<xs:maxLength value="50"/>`), st(`<xs:maxLength value="30"/>`))).toEqual(["longest: 50 → 30 stricter"]);
  expect(facts(st(`<xs:minLength value="1"/>`), st(`<xs:minLength value="3"/>`))).toEqual(["shortest: 1 → 3 stricter"]);
  const at = (use: string) => `<xs:element name="P"><xs:complexType><xs:attribute name="id" type="xs:string" ${use}/></xs:complexType></xs:element>`;
  expect(facts(at(`use="optional"`), at(`use="required"`))).toEqual(["@id: optional → required stricter"]);
  expect(facts(seq(A), seq(`<xs:element name="A" type="xs:string" fixed="x"/>`))).toEqual(["A fixed: now fixed at x stricter"]);
});

test("an optional part added, minOccurs down, maxOccurs up, a value added, a range widened, a pattern removed: looser", () => {
  expect(facts(seq(A), seq(`${A}<xs:element name="B" type="xs:string" minOccurs="0"/>`))).toEqual(["B: added (0..1) looser"]);
  expect(facts(seq(`<xs:element name="A" type="xs:string" minOccurs="2" maxOccurs="9"/>`), seq(`<xs:element name="A" type="xs:string" maxOccurs="9"/>`))).toEqual(["A: 2..9 → 1..9 looser"]);
  expect(facts(seq(`<xs:element name="A" type="xs:string" maxOccurs="5"/>`), seq(`<xs:element name="A" type="xs:string" maxOccurs="unbounded"/>`))).toEqual(["A: 1..5 → 1..∞ looser"]);
  const st = (inner: string) => `<xs:simpleType name="N"><xs:restriction base="xs:decimal">${inner}</xs:restriction></xs:simpleType>`;
  expect(facts(st(`<xs:enumeration value="1"/>`), st(`<xs:enumeration value="1"/><xs:enumeration value="2"/>`))).toEqual(["allowed values: added 2 looser"]);
  expect(facts(st(`<xs:maxInclusive value="10"/>`), st(`<xs:maxInclusive value="99"/>`))).toEqual(["at most: 10 → 99 looser"]);
  expect(facts(st(`<xs:pattern value="[0-9]+"/>`), st(""))).toEqual(["pattern: no longer held to it looser"]);
});

test("a type or a base replaced, a sequence reordered, both ways at once: different; documentation and a default have no word", () => {
  expect(facts(seq(A), seq(`<xs:element name="A" type="xs:int"/>`))).toEqual(["A type: xs:string → xs:int different"]);
  expect(facts(seq(`${A}<xs:element name="B" type="xs:string"/>`), seq(`<xs:element name="B" type="xs:string"/>${A}`))).toEqual([": the order changed: B moved different"]);
  expect(facts(seq(`<xs:element name="A" type="xs:string" maxOccurs="5"/>`), seq(`<xs:element name="A" type="xs:string" minOccurs="0" maxOccurs="3"/>`))).toEqual(["A: 1..5 → 0..3 different"]);
  const doc = (w: string) => `<xs:element name="W" type="xs:string"><xs:annotation><xs:documentation>${w}</xs:documentation></xs:annotation></xs:element>`;
  expect(facts(doc("one"), doc("two"))).toEqual(["words: words changed -"]);
  expect(facts(seq(`<xs:element name="A" type="xs:string" default="x"/>`), seq(`<xs:element name="A" type="xs:string" default="y"/>`))).toEqual(["A default: x → y -"]);
});

test("formatting is no change: white space, attribute order, a prefix spelled differently, an enumeration's order", () => {
  const before = `<xs:schema xmlns:xs="${XS}" targetNamespace="urn:t" xmlns="urn:t"><xs:simpleType name="C"><xs:restriction base="xs:string"><xs:enumeration value="a"/><xs:enumeration value="b"/></xs:restriction></xs:simpleType><xs:element name="E" type="C" minOccurs="0"/></xs:schema>`;
  const after = `<schema xmlns="${XS}" xmlns:t="urn:t" targetNamespace="urn:t">
    <simpleType   name="C">
      <restriction base="string"><enumeration value="b"/>
        <enumeration value="a"/></restriction>
    </simpleType>
    <element type="t:C" name="E"/>
  </schema>`;
  const ch = changes(setOf(before), setOf(after), F, F);
  expect(ch.changed).toEqual([]);
  expect(ch.same).toBe(2);
});

test("a declaration that names one that changed is changed through it, one step; a rename is one removed and one added", () => {
  const t = (len: string) => `<xs:simpleType name="Id"><xs:restriction base="xs:string"><xs:maxLength value="${len}"/></xs:restriction></xs:simpleType><xs:complexType name="Party"><xs:sequence><xs:element name="ID" type="Id"/></xs:sequence></xs:complexType><xs:element name="Buyer" type="Party"/>`;
  const ch = changes(setOf(schema(t("20"))), setOf(schema(t("10"))), F, F);
  expect(ch.changed.map((c) => c.key)).toEqual(["ty {urn:t}Id"]);
  expect([...ch.through]).toEqual([["ty {urn:t}Party", "ty {urn:t}Id"]]);
  const renamed = changes(setOf(schema(`<xs:element name="Old" type="xs:string"/>`)), setOf(schema(`<xs:element name="New" type="xs:string"/>`)), F, F);
  expect([renamed.added, renamed.removed]).toEqual([["el {urn:t}New"], ["el {urn:t}Old"]]);
});

test("the answer: rows in the file's order, then those changed through; counts; a side with no schema lists everything", () => {
  const a = setOf(schema(`<xs:element name="A" type="xs:string"/><xs:element name="B" type="xs:string" nillable="true"/>`));
  const b = setOf(schema(`<xs:element name="A" type="xs:int"/><xs:element name="C" type="xs:string"/>`));
  const answer = changesOf(a, b, F, F);
  expect(answer.rows.map((r) => `${r.mark} ${r.label} ${r.place}`)).toEqual(["changed A el/A", "added C el/C", "removed B el/B"]);
  expect(answer.rows[0]!.say).toBe("type: xs:string → xs:int");
  expect(answer.rows[0]!.facts[0]!.text).toEqual([{ text: "xs:string", code: true }, { text: " → " }, { text: "xs:int", code: true }]);
  expect([answer.changed, answer.added, answer.removed, answer.counts.different]).toEqual([1, 1, 1, 1]);
  const gone = changesOf(a, null, F, F);
  expect(gone.rows.map((r) => r.mark)).toEqual(["removed", "removed"]);
  expect(gone.sides).toEqual({ old: true, new: false });
});

test("openTRANS against itself reads the same: every declaration, however it is written", () => {
  const files = familyFiles("opentrans-2.1", "");
  const load = () => loadSet({ entry: "opentrans_2_1.xsd", texts: new Map([...files].map(([p, t]) => [p, t])), paths: null, pointed: [], index: null, elsewhere: null, bundled: new Map() });
  const ch = changes(load(), load(), "opentrans_2_1.xsd", "opentrans_2_1.xsd");
  expect([ch.changed.length, ch.added.length, ch.removed.length]).toEqual([0, 0, 0]);
  expect(ch.same).toBeGreaterThan(200);
});

test("a type changed in another file of the set marks the declarations that name it through it", () => {
  const types = (len: string) => schema(`<xs:simpleType name="Code"><xs:restriction base="xs:string"><xs:maxLength value="${len}"/></xs:restriction></xs:simpleType>`);
  const main = schema(`<xs:include schemaLocation="types.xsd"/><xs:element name="Item" type="Code"/><xs:element name="Other" type="xs:string"/>`);
  const both = (len: string) => loadSet({ entry: F, texts: new Map([[F, main], ["types.xsd", types(len)]]), paths: null, pointed: [], index: null, elsewhere: null, bundled: new Map() });
  const ch = changes(both("5"), both("9"), F, F);
  expect(ch.changed).toEqual([]);
  expect([...ch.through]).toEqual([["el {urn:t}Item", "ty {urn:t}Code"]]);
  expect(changes(both("5"), both("5"), F, F).through.size).toBe(0);
});
