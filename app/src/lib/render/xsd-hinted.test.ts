/**
 * The XML view's first open (2026-11-25, Fable's ruling): the paths a set read last time read in the first round
 * (`xsd-hint.ts`), and the document crossed once with its open, its verdict answered by the round that leaves the set
 * whole. A hint only fetches bytes sooner: any hint answers as no hint does.
 */
import { describe, expect, test } from "vitest";

import { answerXsd, type Checked, type Opened, type SetIndex, type XsdCall, type XsdSessions } from "./xsd-answer";
import { fixture } from "./doc-fixture";
import { keptOf, type KeepAt } from "./xsd-kept";
import { XsdPort, type XsdEngine, type XsdReads } from "./xsd-port";
import { bundledFiles, familyFiles } from "./xsd-fixture";
import { plain } from "./xsd-findings";
import { entryFor, rootSays } from "../tf/render/doc/doc-check.svelte";
import { ns } from "./xml-names";

const b = bundledFiles();
const BUNDLED = { byNs: b.byNs, texts: b.texts };
const OT = ns("www.opentrans.org/XMLSchema/2.1");
const PIN: KeepAt = { scope: '["gitea:t/ledger-api","gitea:t/ledger-api",null]', at: '["c1",null,null]' };
const FILES = familyFiles("opentrans-2.1", "schemas/opentrans/");
const ORDER = fixture("order-4711.xml");
const ENTRY = entryFor("orders/order-4711.xml", rootSays(ORDER)!)!;
type File = { path: string; text: string | null };

/** What an index says of the set, without its timings. */
const facts = (i: SetIndex) => JSON.stringify({ ...i, ms: null });
const verdict = (c: Checked | undefined) => (c === undefined ? undefined : c.state === "checked" ? c.findings.map((f) => plain(f.words.text)) : c.state);

/** A worker opened on the order's set and walked as the port walks it, `hint` going aside in the first round. */
function walk(hint: readonly File[] | null, xml?: string) {
  const sessions: XsdSessions = new Map();
  const call = (c: XsdCall) => answerXsd(sessions, 1, c, BUNDLED) as Opened | Promise<Opened>;
  const read: string[] = [];
  return (async () => {
    let a = await call({ kind: "open", entry: ENTRY.path, text: ENTRY.text, paths: null, pointed: [], home: OT, lean: true, keep: PIN, ...(xml !== undefined ? { xml } : {}) });
    const first = a;
    let rounds = 0;
    const asked = new Set<string>();
    for (; a.ok && a.need.length > 0 && rounds < 10; rounds += 1) {
      const want = a.need.filter((p) => !p.startsWith("?") && !asked.has(p));
      for (const p of a.need) asked.add(p);
      read.push(...want);
      const files = want.map((p) => ({ path: p, text: FILES.get(p) ?? null }));
      const aside = rounds === 0 && hint !== null ? hint.filter((f) => !want.includes(f.path)) : [];
      a = await call({ kind: "addFiles", files, ...(aside.length > 0 ? { hinted: aside } : {}) });
    }
    return { first, last: a, rounds, read, kept: [...keptOf(sessions).keys()], check: () => answerXsd(sessions, 1, { kind: "check", xml: ORDER }, BUNDLED) as Promise<Checked> };
  })();
}

describe("a hint fetches bytes sooner, and changes nothing else", () => {
  test("the set read last time, hinted, is walked in one round: the same index, kept key and verdict as no hint", async () => {
    const none = await walk(null);
    expect(none.rounds).toBeGreaterThan(1);
    const hint = none.read.map((p) => ({ path: p, text: FILES.get(p) ?? null }));
    const right = await walk(hint);
    expect(right.rounds).toBe(1);
    expect(right.last.ok && right.last.need).toEqual([]);
    expect(right.last.ok && facts(right.last.index)).toBe(none.last.ok && facts(none.last.index));
    expect(right.kept).toEqual(none.kept);
    expect(right.kept).toHaveLength(1);
    expect(verdict(await right.check())).toEqual(verdict(await none.check()));
    expect(verdict(await right.check())).toHaveLength(5);
  });

  test("a file the walk never asks for stays out of the set; a hint too wide, garbage or short answers as no hint", async () => {
    const none = await walk(null);
    const wanted = none.read.map((p) => ({ path: p, text: FILES.get(p) ?? null }));
    const hints: Record<string, File[]> = {
      wide: [...wanted, { path: "schemas/opentrans/other.xsd", text: '<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema" targetNamespace="urn:other"><xs:element name="X"/></xs:schema>' }, { path: "@repo/x/y.xsd", text: "<y/>" }],
      garbage: [{ path: "nowhere.xsd", text: null }, { path: "schemas/opentrans/broken.xsd", text: "<not a schema" }],
      short: wanted.slice(0, 1),
      empty: [],
    };
    for (const [name, hint] of Object.entries(hints)) {
      const w = await walk(hint);
      expect(w.last.ok && facts(w.last.index), name).toBe(none.last.ok && facts(none.last.index));
      expect(w.kept, name).toEqual(none.kept);
      expect(verdict(await w.check()), name).toEqual(verdict(await none.check()));
    }
  });

  test("hints alone leave a set served kept whole as it was: still held, not read again", async () => {
    const sessions: XsdSessions = new Map();
    const call = (id: number, c: XsdCall) => answerXsd(sessions, id, c, BUNDLED) as Opened | Promise<Opened>;
    const open = { kind: "open" as const, entry: ENTRY.path, text: ENTRY.text, paths: null, pointed: [], home: OT, lean: true, keep: PIN };
    let a = await call(1, open);
    for (let i = 0; a.ok && a.need.length > 0 && i < 10; i += 1) a = await call(1, { kind: "addFiles", files: a.need.map((p) => ({ path: p, text: FILES.get(p) ?? null })) });
    const served = await call(2, open);
    expect(served.ok && served.need).toEqual([]);
    expect(sessions.get(2)!.kept).toBeDefined();
    const after = await call(2, { kind: "addFiles", files: [], hinted: [{ path: "schemas/opentrans/other.xsd", text: "<x/>" }] });
    expect(after.ok && facts(after.index)).toBe(served.ok && facts(served.index));
    expect(sessions.get(2)!.kept, "still holding the kept set").toBeDefined();
  });

  test("hints alone leave a set already whole as it was", async () => {
    const sessions: XsdSessions = new Map();
    // A set with nothing to read: the schema is its own entry.
    const own = '<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema" targetNamespace="urn:a"><xs:element name="a"/></xs:schema>';
    const open = answerXsd(sessions, 1, { kind: "open", entry: "a.xsd", text: own, paths: null, pointed: [], lean: true, keep: PIN }, BUNDLED) as Opened;
    expect(open.ok && open.need).toEqual([]);
    const after = answerXsd(sessions, 1, { kind: "addFiles", files: [], hinted: [{ path: "b.xsd", text: own }] }, BUNDLED) as Opened;
    expect(after.ok && facts(after.index)).toBe(open.ok && facts(open.index));
    expect(sessions.get(1)!.texts.has("b.xsd")).toBe(false);
  });
});

describe("the document crosses once, and the round that leaves the set whole answers its verdict", () => {
  test("no round before the last answers a verdict; the last answers the one a check of its own gives", async () => {
    const w = await walk(null, ORDER);
    expect(w.first.ok && w.first.checked).toBeUndefined();
    expect(w.last.ok && verdict(w.last.checked)).toEqual(verdict(await w.check()));
    expect(w.last.ok && w.last.checked?.state).toBe("checked");
    // Without the document, no verdict comes back.
    const bare = await walk(null);
    expect(bare.last.ok && bare.last.checked).toBeUndefined();
  });

  test("a set kept whole answers each document its own verdict, never the one it was kept with", async () => {
    const sessions: XsdSessions = new Map();
    const call = (id: number, c: XsdCall) => answerXsd(sessions, id, c, BUNDLED) as Opened | Promise<Opened>;
    const other = fixture("order-4710.xml");
    const open = { kind: "open" as const, entry: ENTRY.path, text: ENTRY.text, paths: null, pointed: [], home: OT, lean: true, keep: PIN, xml: ORDER };
    let a = await call(1, open);
    for (let i = 0; a.ok && a.need.length > 0 && i < 10; i += 1) a = await call(1, { kind: "addFiles", files: a.need.map((p) => ({ path: p, text: FILES.get(p) ?? null })) });
    expect(a.ok && verdict(a.checked)).toHaveLength(5);
    // The other order, on the set the first kept: its own verdict, valid.
    const second = await call(2, { ...open, xml: other });
    expect(second.ok && second.need).toEqual([]);
    expect(second.ok && verdict(second.checked)).toEqual([]);
    // Opened without a document, nothing is checked: not the first order again.
    const { xml: _, ...without } = open;
    const bare = await call(3, without);
    expect(bare.ok && bare.checked).toBeUndefined();
    expect(keptOf(sessions).get([...keptOf(sessions).keys()][0]!)!.base.xml).toBeUndefined();
  });

  test("a set kept whole answers the verdict with its open", async () => {
    const sessions: XsdSessions = new Map();
    const call = (id: number, c: XsdCall) => answerXsd(sessions, id, c, BUNDLED) as Opened | Promise<Opened>;
    const open = { kind: "open" as const, entry: ENTRY.path, text: ENTRY.text, paths: null, pointed: [], home: OT, lean: true, keep: PIN };
    let a = await call(1, open);
    for (let i = 0; a.ok && a.need.length > 0 && i < 10; i += 1) a = await call(1, { kind: "addFiles", files: a.need.map((p) => ({ path: p, text: FILES.get(p) ?? null })) });
    const again = await call(2, { ...open, xml: ORDER });
    expect(again.ok && again.need).toEqual([]);
    expect(again.ok && verdict(again.checked)).toHaveLength(5);
  });

  test("the port crosses the document with its open and asks no check of its own; the hint's files are read once, in one round", async () => {
    const sessions: XsdSessions = new Map();
    const calls: string[] = [];
    const engine: XsdEngine = async (id, c) => (calls.push(c.kind), answerXsd(sessions, id, c, BUNDLED));
    const reads: string[] = [];
    const none = await walk(null);
    const xsdReads: XsdReads = { paths: async () => [...FILES.keys()], read: async (p) => (reads.push(p), FILES.get(p) ?? null), index: async () => ({ declares: [], catalogs: [], total: 0, read: 0 }), pointed: [], keep: null };
    const port = new XsdPort(engine);
    await port.open(ENTRY.path, ENTRY.text, xsdReads, OT, true, null, { xml: ORDER, hint: [...none.read, "nowhere.xsd"] });
    await port.whole;
    expect(calls).toEqual(["open", "addFiles"]);
    expect(reads.sort()).toEqual([...none.read, "nowhere.xsd"].sort());
    expect(verdict(port.checked ?? undefined)).toHaveLength(5);
    // The hint's reads wait for the outline, as the set's own do.
    reads.length = 0;
    let land!: (go: boolean) => void;
    const landed = new Promise<boolean>((r) => (land = r));
    const late = new XsdPort(engine);
    await late.open(ENTRY.path, ENTRY.text, xsdReads, OT, true, landed, { hint: none.read });
    await new Promise((r) => setTimeout(r, 0));
    expect(reads).toEqual([]);
    land(true);
    await late.whole;
    expect(reads.length).toBe(none.read.length);
  });
});
