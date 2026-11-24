import { describe, expect, test } from "vitest";

import { answerXsd, type Checked, type XsdSessions } from "./xsd-answer";
import { fixture } from "./doc-fixture";
import { bundledFiles, familyFiles } from "./xsd-fixture";
import { plain } from "./xsd-findings";
import { ns } from "./xml-names";

const ENTRY = "schemas/opentrans/opentrans_2_1.xsd";

/** A session on the set, read whole, as the worker holds it. */
function session(files: Map<string, string>, entry = ENTRY): { ask: (xml: string) => Promise<Checked> } {
  const b = bundledFiles();
  const bundled = { byNs: b.byNs, texts: b.texts };
  const sessions: XsdSessions = new Map();
  let a = answerXsd(sessions, 1, { kind: "open", entry, text: files.get(entry)!, paths: null, pointed: [] }, bundled) as { need: string[] };
  for (let i = 0; i < 10 && a.need.length > 0; i += 1) {
    const index = a.need.includes("?index") ? { declares: [...files.keys()].map((p) => [p, null] as [string, null]), catalogs: [], total: 0, read: 0 } : undefined;
    // No other repository declares anything here.
    const away = a.need.find((p) => p.startsWith("?elsewhere:"));
    const elsewhere = away === undefined ? undefined : { namespace: away.slice("?elsewhere:".length), list: [] };
    a = answerXsd(sessions, 1, { kind: "addFiles", files: a.need.filter((p) => !p.startsWith("?")).map((p) => ({ path: p, text: files.get(p) ?? null })), paths: [...files.keys()], ...(index ? { index } : {}), ...(elsewhere ? { elsewhere } : {}) }, bundled) as { need: string[] };
  }
  return { ask: (xml) => answerXsd(sessions, 1, { kind: "check", xml }, bundled) as Promise<Checked> };
}

describe("libxml2 judges a document against the set, from memory (section 24)", () => {
  test("order-4710 is valid; order-4711's five departures are found, in knobas's words, each with its path", async () => {
    const s = session(familyFiles("opentrans-2.1", "schemas/opentrans/"));
    const good = await s.ask(fixture("order-4710.xml"));
    expect(good.state).toBe("checked");
    if (good.state === "checked") expect(good.findings).toEqual([]);
    const bad = await s.ask(fixture("order-4711.xml"));
    expect(bad.state).toBe("checked");
    if (bad.state !== "checked") return;
    expect(bad.total).toBe(5);
    expect(bad.findings.map((f) => plain(f.words.text))).toEqual([
      "DELIVERY_DATE stands where ORDER_DATE is expected.",
      "bmecat:SUPPLIER_PID is 37 characters long; it may be 32 at most.",
      "zwölf is not a bmecat:dtNUMBER, which is what QUANTITY holds.",
      "Stück is not one of the 1,095 values bmecat:ORDER_UNIT may hold.",
      "DISCOUNT may not stand here. Expected: ORDER_DATE, ORDER_DESCR, CUSTOMER_IDREF.",
    ]);
    expect(bad.findings.every((f) => f.xpath !== null)).toBe(true);
  });

  test("it asks for nothing but the files of the set", async () => {
    const s = session(familyFiles("opentrans-2.1", "schemas/opentrans/"));
    const answer = await s.ask(fixture("order-4710.xml"));
    if (answer.state !== "checked") throw new Error(answer.why);
    expect(answer.asked.length).toBeGreaterThan(0);
    expect(answer.asked.every((u) => u.startsWith("mem:///set/schemas/opentrans/"))).toBe(true);
  });

  test("a set with a file not found here is not checked, and says which", async () => {
    const files = familyFiles("opentrans-2.1", "schemas/opentrans/");
    files.delete("schemas/opentrans/xmlmime.xsd");
    const answer = await session(files).ask(fixture("order-4710.xml"));
    expect(answer).toEqual({ state: "not-checked", why: "not whole", detail: ["The schema needs xmlmime.xsd, which is not found here."] });
  });

  test("an import by address nothing answers is not checked; libxml2 asks the provider and nothing is fetched", async () => {
    const xs = `xmlns:xs="${ns("www.w3.org/2001/XMLSchema")}"`;
    const files = new Map([["a.xsd", `<xs:schema ${xs} targetNamespace="urn:a"><xs:import namespace="urn:far" schemaLocation="https://far.example/far.xsd"/><xs:element name="a" type="xs:string"/></xs:schema>`]]);
    const answer = await session(files, "a.xsd").ask(`<a xmlns="urn:a">x</a>`);
    expect(answer).toEqual({ state: "not-checked", why: "not whole", detail: ["The schema needs far.xsd, which is not found here."] });
  });

  test("XML Schema 1.1 is not checked", async () => {
    const xs = `xmlns:xs="${ns("www.w3.org/2001/XMLSchema")}"`;
    const files = new Map([["a.xsd", `<xs:schema ${xs} xmlns:vc="${ns("www.w3.org/2007/XMLSchema-versioning")}" vc:minVersion="1.1"><xs:element name="a" type="xs:string"/></xs:schema>`]]);
    const answer = await session(files, "a.xsd").ask(`<a>x</a>`);
    expect(answer).toMatchObject({ state: "not-checked", why: "XML Schema 1.1" });
  });
});
