import { describe, expect, test } from "vitest";

import { childElements, elType, keyOfPlace, labelOfKey, leftoversOf, loadSet, placeAtLine, placeOf, shape, type ElementDecl } from "./xsd";
import { family, familyFiles, loadAll } from "./xsd-fixture";
import { ns } from "./xml-names";

const count = (set: ReturnType<typeof family>, kind?: string) => [...set.comps.values()].filter((c) => kind === undefined || c.kind === kind).length;

describe("the four fixtures open with the counts the design read (section 2)", () => {
  test("openTRANS 2.1 with BMEcat, xmldsig and xmlmime: 4 files, 848 declarations", () => {
    const set = family("opentrans-2.1");
    expect(set.docs.map((d) => d.path)).toEqual(["opentrans_2_1.xsd", "bmecat_2005.xsd", "xmlmime.xsd", "xmldsig-core-schema.xsd"]);
    expect(set.docs.every((d) => d.bad === null && !d.reading)).toBe(true);
    expect(count(set)).toBe(848);
    expect(count(set, "el")).toBe(747);
    expect(count(set, "ct") + count(set, "st")).toBe(99);
    expect(count(set, "at")).toBe(2);
    expect(set.docs[1]!.refs).toEqual([]);
    expect(set.docs[0]!.refs.map((r) => r.how)).toEqual(["beside", "beside", "beside"]);
  });

  test("openTRANS has one root and ten leftovers; BMEcat alone one root and three (section 7)", () => {
    const ot = family("opentrans-2.1");
    expect(ot.roots.map((k) => labelOfKey(ot, k))).toEqual(["OPENTRANS"]);
    expect(leftoversOf(ot)).toHaveLength(10);
    const bme = family("bmecat-2005fd");
    expect(bme.roots.map((k) => labelOfKey(bme, k))).toEqual(["BMECAT"]);
    expect(leftoversOf(bme)).toHaveLength(3);
  });

  test("UBL's invoice reaches 14 files, CII's 54", () => {
    expect(family("ubl-2.1").docs).toHaveLength(14);
    const cii = family("cii-d16b");
    expect(cii.docs).toHaveLength(54);
    expect(cii.docs.every((d) => d.bad === null)).toBe(true);
  });
});

describe("what an element holds, merged and in order (section 10)", () => {
  test("ORDER holds its header, its list of items and its summary, each once, and its attributes first", () => {
    const set = family("opentrans-2.1");
    const order = set.comps.get(keyOfPlace(set, "el/ORDER")!) as ElementDecl;
    const s = shape(set, elType(set, order).def)!;
    expect(s.attrs.map((a) => ("at" in a ? a.at.name : "?"))).toEqual(["version", "type"]);
    expect(childElements(set, s.content).map((c) => [labelOfKey(set, `el ${c.el.ref}`), c.min, c.max])).toEqual([
      ["ORDER_HEADER", 1, 1],
      ["ORDER_ITEM_LIST", 1, 1],
      ["ORDER_SUMMARY", 1, 1],
    ]);
  });

  test("a type's chain reads down to its built-in, and a name from BMEcat carries its prefix", () => {
    const set = family("opentrans-2.1");
    const unit = keyOfPlace(set, "el/bmecat:ORDER_UNIT")!;
    expect(labelOfKey(set, unit)).toBe("bmecat:ORDER_UNIT");
    const t = elType(set, set.comps.get(unit) as ElementDecl);
    expect(labelOfKey(set, t.key!)).toBe("bmecat:dtPUNIT");
    const s = shape(set, t.def)!;
    expect(s.simple!.facets.enumeration!.length).toBeGreaterThan(1000);
  });

  test("an extension of an extension answers its bases' rows before its own (UBL)", () => {
    const set = family("ubl-2.1");
    const invoice = set.comps.get(keyOfPlace(set, "el/Invoice")!) as ElementDecl;
    const kids = childElements(set, shape(set, elType(set, invoice).def)!.content);
    expect(kids[0]!.el.ref).toContain("UBLExtensions");
    expect(kids.length).toBeGreaterThan(50);
  });
});

describe("places", () => {
  test("a place with a prefix round trips, and a name the set does not have answers nothing", () => {
    const set = family("opentrans-2.1");
    for (const place of ["el/ORDER", "el/bmecat:PRODUCT_DETAILS", "type/bmecat:dtSTRING", "type/typeOPENTRANS_version"]) {
      const key = keyOfPlace(set, place);
      expect(key, place).not.toBeNull();
      expect(placeOf(set, key!)).toBe(place);
    }
    expect(keyOfPlace(set, "el/NO_SUCH_ELEMENT")).toBeNull();
    expect(keyOfPlace(set, "el/nope:ORDER")).toBeNull();
  });

  test("a line inside a declaration answers that declaration", () => {
    const set = family("opentrans-2.1");
    const order = set.comps.get(keyOfPlace(set, "el/ORDER")!)!;
    expect(placeAtLine(set, "opentrans_2_1.xsd", order.line + 2)).toBe("el/ORDER");
  });
});

describe("used by (section 11)", () => {
  test("dtSTRING is named by many, ORDER is held by OPENTRANS", () => {
    const set = family("opentrans-2.1");
    expect((set.uses.get(keyOfPlace(set, "el/ORDER")!) ?? []).map((u) => labelOfKey(set, u.by))).toEqual(["OPENTRANS"]);
    expect((set.uses.get(keyOfPlace(set, "type/bmecat:dtSTRING")!) ?? []).length).toBeGreaterThan(100);
  });
});

describe("loading in rounds", () => {
  test("the entry alone answers its three files as needs, and nothing of the index", () => {
    const files = familyFiles("opentrans-2.1");
    const { rounds, indexed } = loadAll(files, "opentrans_2_1.xsd");
    expect(indexed).toBe(false);
    expect(rounds).toBe(2);
  });

  test("BME's own bmecat_2005.xsd beside openTRANS is passed over and named (section 2, item 2)", () => {
    const files = familyFiles("opentrans-2.1");
    files.set("bmecat_2005.xsd", familyFiles("bmecat-2005fd").get("bmecat_2005.xsd")!);
    const { set, indexed } = loadAll(files, "opentrans_2_1.xsd");
    expect(indexed).toBe(true);
    const ref = set.docs[0]!.refs[0]!;
    expect(ref.to).toBeNull();
    expect(ref.why).toBe(`bmecat_2005.xsd is there, but it declares ${ns("www.bmecat.org/bmecat/2005fd")}.`);
    expect(set.missing.size).toBeGreaterThan(100);
  });

  test("a file not found here leaves the rest of the set readable", () => {
    const files = familyFiles("opentrans-2.1");
    files.delete("xmlmime.xsd");
    const { set } = loadAll(files, "opentrans_2_1.xsd");
    expect(set.docs[0]!.refs[1]!.why).toBe("No file of that name is here.");
    expect(count(set, "el")).toBeGreaterThan(700);
  });

  test("xmldsig comes with knobas where the repository has none", () => {
    const files = familyFiles("opentrans-2.1");
    files.delete("xmldsig-core-schema.xsd");
    const { set } = loadAll(files, "opentrans_2_1.xsd");
    const ref = set.docs[0]!.refs[2]!;
    expect(ref.how).toBe("bundled");
    expect(ref.to).toBe("@knobas/xmldsig-core-schema.xsd");
  });
});

describe("files with no namespace, cycles and redefinitions (section 18)", () => {
  const xs = `xmlns:xs="${ns("www.w3.org/2001/XMLSchema")}"`;
  test("a chameleon include takes its includer's namespace, its unprefixed names with it", () => {
    const files = new Map([
      ["main.xsd", `<xs:schema ${xs} xmlns="urn:a" targetNamespace="urn:a"><xs:include schemaLocation="c.xsd"/><xs:element name="root" type="T"/></xs:schema>`],
      ["c.xsd", `<xs:schema ${xs}><xs:complexType name="T"><xs:sequence><xs:element name="x" type="xs:string"/></xs:sequence></xs:complexType></xs:schema>`],
    ]);
    const { set } = loadAll(files, "main.xsd");
    expect(set.comps.has("ty {urn:a}T")).toBe(true);
    expect(set.missing.size).toBe(0);
  });
  test("A imports B imports A: each file read once", () => {
    const files = new Map([
      ["a.xsd", `<xs:schema ${xs} targetNamespace="urn:a"><xs:import namespace="urn:b" schemaLocation="b.xsd"/><xs:element name="a"/></xs:schema>`],
      ["b.xsd", `<xs:schema ${xs} targetNamespace="urn:b"><xs:import namespace="urn:a" schemaLocation="a.xsd"/><xs:element name="b"/></xs:schema>`],
    ]);
    const { set } = loadAll(files, "a.xsd");
    expect(set.docs.map((d) => d.path)).toEqual(["a.xsd", "b.xsd"]);
  });
  test("a redefined type keeps the original it replaced", () => {
    const files = new Map([
      ["main.xsd", `<xs:schema ${xs} targetNamespace="urn:a" xmlns="urn:a"><xs:redefine schemaLocation="base.xsd"><xs:simpleType name="Code"><xs:restriction base="Code"><xs:maxLength value="3"/></xs:restriction></xs:simpleType></xs:redefine></xs:schema>`],
      ["base.xsd", `<xs:schema ${xs} targetNamespace="urn:a"><xs:simpleType name="Code"><xs:restriction base="xs:string"/></xs:simpleType></xs:schema>`],
    ]);
    const { set } = loadAll(files, "main.xsd");
    const code = set.comps.get("ty {urn:a}Code")!;
    expect(code.over).toBe("redefine");
    expect(code.original?.file).toBe("base.xsd");
  });
});

test("an extension of an extension answers three types' rows in order (section 10)", () => {
  const xs = `xmlns:xs="${ns("www.w3.org/2001/XMLSchema")}"`;
  const files = new Map([
    [
      "m.xsd",
      `<xs:schema ${xs} targetNamespace="urn:a" xmlns="urn:a" elementFormDefault="qualified">
  <xs:complexType name="A"><xs:sequence><xs:element name="a" type="xs:string"/></xs:sequence><xs:attribute name="ida"/></xs:complexType>
  <xs:complexType name="B"><xs:complexContent><xs:extension base="A"><xs:sequence><xs:element name="b" type="xs:string"/></xs:sequence><xs:attribute name="idb"/></xs:extension></xs:complexContent></xs:complexType>
  <xs:complexType name="C"><xs:complexContent><xs:extension base="B"><xs:sequence><xs:element name="c" type="xs:string"/></xs:sequence></xs:extension></xs:complexContent></xs:complexType>
  <xs:complexType name="R"><xs:complexContent><xs:restriction base="A"><xs:sequence><xs:element name="a" type="xs:string"/></xs:sequence><xs:attribute name="ida" use="prohibited"/></xs:restriction></xs:complexContent></xs:complexType>
</xs:schema>`,
    ],
  ]);
  const { set } = loadAll(files, "m.xsd");
  const c = shape(set, set.comps.get("ty {urn:a}C") as never)!;
  expect(childElements(set, c.content).map((k) => k.el.name)).toEqual(["a", "b", "c"]);
  expect(c.attrs.map((a) => ("at" in a ? a.at.name : ""))).toEqual(["ida", "idb"]);
  expect(c.chain.map((s) => s.name)).toEqual(["B", "A"]);
  const r = shape(set, set.comps.get("ty {urn:a}R") as never)!;
  expect(childElements(set, r.content).map((k) => k.el.name)).toEqual(["a"]);
  expect(r.attrs).toEqual([]);
});

test("a file still being read stands under the namespace its import asks for", () => {
  const files = familyFiles("opentrans-2.1");
  const set = loadSet({ entry: "opentrans_2_1.xsd", texts: new Map([["opentrans_2_1.xsd", files.get("opentrans_2_1.xsd")!]]), paths: null, pointed: [], index: null, elsewhere: null, bundled: new Map() });
  const reading = set.docs.filter((d) => d.reading);
  expect(reading.map((d) => d.tns)).toEqual([ns("www.bmecat.org/bmecat/2005"), ns("www.w3.org/2005/05/xmlmime"), ns("www.w3.org/2000/09/xmldsig#")]);
});
