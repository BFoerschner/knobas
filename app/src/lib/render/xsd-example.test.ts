import { describe, expect, test } from "vitest";

import { check } from "./xsd-check";
import { makeExample, SEED, type ExampleOptions } from "./xsd-example";
import { bundledFiles, family, familyFiles, loadAll, ENTRY } from "./xsd-fixture";
import { XS } from "./xsd-xml";
import { compKey } from "./xsd";

/** A schema of the tests' own, for what the families lack (section 19's cases). */
const OWN = `<xs:schema xmlns:xs="${XS}" targetNamespace="urn:t" xmlns="urn:t" elementFormDefault="unqualified">
  <xs:element name="Root">
    <xs:complexType>
      <xs:sequence>
        <xs:element name="Never" type="xs:string" minOccurs="0" maxOccurs="0"/>
        <xs:element name="Local" type="xs:string"/>
        <xs:choice>
          <xs:element ref="Loop"/>
          <xs:element name="Ends" type="xs:string"/>
        </xs:choice>
        <xs:any namespace="##other" processContents="lax" minOccurs="0"/>
        <xs:element ref="Head"/>
        <xs:element name="Fixed" type="xs:string" fixed="F"/>
        <xs:element name="Def" type="xs:string" default="D" minOccurs="0"/>
      </xs:sequence>
      <xs:attribute name="a" type="xs:string" fixed="x"/>
      <xs:attribute name="b" type="xs:string" default="y"/>
    </xs:complexType>
  </xs:element>
  <xs:element name="Loop"><xs:complexType><xs:sequence><xs:element ref="Loop"/></xs:sequence></xs:complexType></xs:element>
  <xs:element name="Head" abstract="true" type="xs:string"/>
  <xs:element name="Member" substitutionGroup="Head" type="xs:string"/>
  <xs:complexType name="Base"><xs:sequence><xs:element name="B" type="xs:string"/></xs:sequence></xs:complexType>
  <xs:element name="Ext"><xs:complexType><xs:complexContent><xs:extension base="Base"><xs:sequence><xs:element name="E" type="xs:int"/></xs:sequence></xs:extension></xs:complexContent></xs:complexType></xs:element>
  <xs:element name="Strict"><xs:complexType><xs:sequence><xs:any namespace="##other" processContents="strict"/></xs:sequence></xs:complexType></xs:element>
  <xs:element name="Opt"><xs:complexType><xs:sequence><xs:element name="Maybe" type="xs:string" minOccurs="0"/></xs:sequence></xs:complexType></xs:element>
</xs:schema>`;

const opts = (over: Partial<ExampleOptions> = {}): ExampleOptions => ({ seed: SEED, optional: "required", repeats: 1, lang: "en", picks: {}, ...over });
const own = () => loadAll(new Map([["t.xsd", OWN]]), "t.xsd").set;
const key = (name: string, ns = "urn:t") => compKey("el", ns, name);
const body = (xml: string) => xml.split("\n").slice(2).join("\n");

async function valid(files: Map<string, string>, set: ReturnType<typeof own>, xml: string) {
  const b = bundledFiles();
  return check(set, (p) => files.get(p) ?? b.texts.get(p) ?? null, xml);
}

describe("an example is a document made from an element, valid first (section 19)", () => {
  test("the content model decides: maxOccurs 0 is nothing, a choice takes the branch that ends, fixed and default stand", async () => {
    const set = own();
    const ex = makeExample(set, key("Root"), opts())!;
    const doc = body(ex.xml);
    expect(doc).not.toContain("Never");
    // The choice's first branch (Loop holds itself without end) never ends: the second is taken.
    expect(doc).toContain("<Ends>");
    expect(doc).not.toContain("Loop");
    expect(ex.choices).toEqual([{ id: "t.xsd:7", names: ["Loop", "Ends"], picked: 1 }]);
    // An abstract element is stood in for by its member.
    expect(doc).toContain("Member>");
    expect(doc).not.toContain("<Head");
    expect(doc).toContain("<Fixed>F</Fixed>");
    // Required parts only: an optional default and the optional attributes stay out; the lax wildcard too.
    expect(doc).not.toContain("<Def");
    expect(doc).not.toContain("Anything");
    // An unqualified local element stands in no namespace: the root takes a prefix and no default is declared.
    expect(doc).toMatch(/^<(\w+):Root xmlns:\1="urn:t">/);
    expect(doc).toContain("  <Local>");
    expect(await valid(new Map([["t.xsd", OWN]]), set, ex.xml)).toMatchObject({ state: "checked", findings: [] });
  });

  test("Everything: optional attributes with their fixed and default values, the default element, a lax wildcard's made-up element", async () => {
    const set = own();
    const ex = makeExample(set, key("Root"), opts({ optional: "all" }))!;
    const doc = body(ex.xml);
    expect(doc).toContain(' a="x" b="y"');
    expect(doc).toMatch(/<Def>[^<]+<\/Def>/);
    expect(doc).toContain(">any element may stand here</");
    expect(ex.notes).toContain("One place takes any element (xs:any): the example puts a made-up one there.");
    expect(await valid(new Map([["t.xsd", OWN]]), set, ex.xml)).toMatchObject({ state: "checked", findings: [] });
  });

  test("a picked branch is taken; an extension holds its base's part first; an element that would be empty takes its first optional part", async () => {
    const set = own();
    const picked = makeExample(set, key("Root"), opts({ picks: { "t.xsd:7": 0 } }))!;
    // The branch that never ends, picked: the second Loop is left out where it may be, and kept where it must stand.
    expect(picked.choices[0]!.picked).toBe(0);
    expect(body(picked.xml)).toContain("Loop");
    const ext = makeExample(set, key("Ext"), opts())!;
    expect(body(ext.xml).indexOf("<B>")).toBeLessThan(body(ext.xml).indexOf("<E>"));
    expect(await valid(new Map([["t.xsd", OWN]]), set, ext.xml)).toMatchObject({ state: "checked", findings: [] });
    expect(body(makeExample(set, key("Opt"), opts())!.xml)).toContain("<Maybe>");
  });

  test("what cannot be made is left out upward: an optional element named but not found here, and the notes say so", () => {
    // Its own schema: libxml2 would not compile a set that names an element it lacks.
    const gone = `<xs:schema xmlns:xs="${XS}" targetNamespace="urn:g" xmlns="urn:g" elementFormDefault="qualified">
      <xs:element name="Root"><xs:complexType><xs:sequence>
        <xs:element name="Here" type="xs:string"/>
        <xs:element ref="Gone" minOccurs="0"/>
      </xs:sequence></xs:complexType></xs:element>
    </xs:schema>`;
    const set = loadAll(new Map([["g.xsd", gone]]), "g.xsd").set;
    const ex = makeExample(set, compKey("el", "urn:g", "Root"), opts({ optional: "all" }))!;
    expect(ex.xml).toContain("<Here>");
    expect(ex.xml).not.toContain("Gone");
    expect(ex.notes).toContain("One name was not found here, so the example leaves it out: Gone.");
  });

  test("a strict wildcard nothing fits: kept with a comment where it must stand, and the notes say so", () => {
    const ex = makeExample(own(), key("Strict"), opts())!;
    expect(ex.xml).toContain("<!-- any declared element may stand here; nothing in this schema set fits -->");
    expect(ex.notes).toContain("One place needs a declared element of another schema (xs:any, strict), which this set does not hold.");
  });

  test("labelled: the second line says what made it, its seed and parts; the file is named by the element and the seed", () => {
    const ex = makeExample(family("opentrans-2.1"), key("ORDER", "http:/" + "/www.opentrans.org/XMLSchema/2.1"), opts())!;
    expect(ex.xml.split("\n").slice(0, 2)).toEqual(['<?xml version="1.0" encoding="UTF-8"?>', "<!-- Made by knobas from opentrans_2_1.xsd (ORDER), seed 4711, required parts only. The values are invented. -->"]);
    expect(ex.fileName).toBe("ORDER-example-4711.xml");
    expect(ex.lines).toHaveLength(ex.xml.trimEnd().split("\n").length);
  });

  test("the same element, settings and seed make the same document; another seed another", () => {
    const set = family("opentrans-2.1");
    const order = key("ORDER", "http:/" + "/www.opentrans.org/XMLSchema/2.1");
    const a = makeExample(set, order, opts({ optional: "all", repeats: 2 }))!.xml;
    expect(makeExample(family("opentrans-2.1"), order, opts({ optional: "all", repeats: 2 }))!.xml).toBe(a);
    expect(makeExample(set, order, opts({ optional: "all", repeats: 2, seed: 4712 }))!.xml).not.toBe(a);
  });

  test("values read their names: a line's number counts up, a currency is EUR, a GTIN's check digit holds, a reference names a party held", () => {
    const set = family("opentrans-2.1");
    const xml = makeExample(set, key("ORDER", "http:/" + "/www.opentrans.org/XMLSchema/2.1"), opts({ optional: "all", repeats: 3 }))!.xml;
    // An item's own line number, not a reference's to another order's line.
    const ids = [...xml.matchAll(/<ORDER_ITEM>\s*<LINE_ITEM_ID>([^<]+)<\/LINE_ITEM_ID>/g)].map((m) => m[1]);
    expect(ids.slice(0, 3)).toEqual(["1", "2", "3"]);
    expect(xml).toMatch(/<bmecat:CURRENCY>EUR<\/bmecat:CURRENCY>|<CURRENCY>EUR<\/CURRENCY>/);
    const gtin = /type="gtin">(\d{13})</.exec(xml)?.[1];
    if (gtin !== undefined) {
      const d = [...gtin].map(Number);
      expect((10 - (d.slice(0, 12).reduce((s, x, i) => s + x * (i % 2 === 0 ? 1 : 3), 0) % 10)) % 10).toBe(d[12]);
    }
    const parties = new Set([...xml.matchAll(/<bmecat:PARTY_ID[^>]*>([^<]+)</g)].map((m) => m[1]));
    for (const m of xml.matchAll(/<[A-Z_]*IDREF[^>]*>([^<]+)</g)) expect(parties.has(m[1])).toBe(true);
  });
});

test("the families' entries open", () => {
  expect(Object.keys(ENTRY)).toHaveLength(4);
  expect(familyFiles("opentrans-2.1").size).toBe(4);
});
