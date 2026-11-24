import { describe, expect, test } from "vitest";

import { answerXsd, type Checked, type Opened, type XsdCall, type XsdSessions } from "./xsd-answer";
import { fixture } from "./doc-fixture";
import { KEEP_SETS, evict, keptOf, type Kept, type KeepAt, type KeptSets } from "./xsd-kept";
import { XsdPort, localXsdEngine, type XsdReads } from "./xsd-port";
import { bundledFiles, familyFiles } from "./xsd-fixture";
import { plain } from "./xsd-findings";
import { entryFor, rootSays } from "../tf/render/doc/doc-check.svelte";
import { ns } from "./xml-names";
import { XS } from "./xsd-xml";
import { sniffNamespace } from "./xsd-sniff";

const b = bundledFiles();
const BUNDLED = { byNs: b.byNs, texts: b.texts };
const OT = ns("www.opentrans.org/XMLSchema/2.1");
const PIN: KeepAt = { scope: '["gitea:t/ledger-api","gitea:t/ledger-api",null]', at: '["c1",null,null]' };
/** openTRANS where the orders name it, as `ledger-api` holds it. */
const FILES = familyFiles("opentrans-2.1", "schemas/opentrans/");
const ORDER_4711 = fixture("order-4711.xml");
const ORDER_4710 = fixture("order-4710.xml");

type Open = Extract<XsdCall, { kind: "open" }>;

/** A worker's sessions, and every path the port was asked to read, in order. */
function worker(at: ReadonlyMap<string, string> = FILES) {
  const sessions: XsdSessions = new Map();
  /** The files as the pin reads them: changed by a test to stand for another commit. */
  const repo = { files: at };
  const read: string[] = [];
  let next = 0;
  /** Opens a session on `open` and answers what it needs, round after round, as the port does: the open's own answer. */
  const open = (call: Omit<Open, "kind">, away: (path: string) => string | null = () => null, elsewhere: (ns: string) => unknown[] = () => []) => {
    const id = ++next;
    const first = answerXsd(sessions, id, { kind: "open", ...call }, BUNDLED) as Opened;
    let a = first;
    const asked = new Set<string>();
    for (let i = 0; i < 10 && a.ok && a.need.length > 0; i += 1) {
      const ask = a.need.filter((p) => !asked.has(p));
      if (ask.length === 0) break;
      for (const p of ask) asked.add(p);
      const paths = ask.filter((p) => !p.startsWith("?"));
      read.push(...ask);
      const ns = ask.find((p) => p.startsWith("?elsewhere:"))?.slice("?elsewhere:".length);
      const index = ask.includes("?index") ? { declares: [...repo.files].map(([p, t]) => [p, sniffNamespace(t) ?? null] as [string, string | null]), catalogs: [], total: repo.files.size, read: repo.files.size } : undefined;
      a = answerXsd(sessions, id, {
        kind: "addFiles",
        files: paths.map((p) => ({ path: p, text: p.startsWith("@") ? away(p) : (repo.files.get(p) ?? null) })),
        ...(index !== undefined ? { index, paths: [...repo.files.keys()] } : {}),
        ...(ns !== undefined ? { elsewhere: { namespace: ns, list: elsewhere(ns) as never } } : {}),
      }, BUNDLED) as Opened;
    }
    return {
      id,
      first,
      check: (xml: string) => answerXsd(sessions, id, { kind: "check", xml }, BUNDLED) as Promise<Checked>,
      close: () => answerXsd(sessions, id, { kind: "close" }, BUNDLED),
    };
  };
  return { sessions, read, open, repo, kept: () => keptOf(sessions) };
}

/** The order's check as the XML view opens it: its schema as an entry beside it, lean, pinned to `keep`. */
const orderOpen = (xml: string, path: string, keep: KeepAt | null = PIN, pointed: Open["pointed"] = []): Omit<Open, "kind"> => {
  const e = entryFor(path, rootSays(xml)!)!;
  return { entry: e.path, text: e.text, paths: null, pointed, home: OT, lean: true, ...(keep !== null ? { keep } : {}) };
};
const verdict = (c: Checked) => (c.state === "checked" ? c.findings.map((f) => plain(f.words.text)) : c.state);

describe("a check's set kept whole (the schema session cache)", () => {
  test("the order opened again is answered whole by its open: nothing is read, and the verdict is the same", async () => {
    const w = worker();
    const one = w.open(orderOpen(ORDER_4711, "orders/order-4711.xml"));
    const before = await one.check(ORDER_4711);
    expect(before.state === "checked" && before.total).toBe(5);
    expect(w.read.length).toBeGreaterThan(0);
    one.close();
    w.read.length = 0;
    const again = w.open(orderOpen(ORDER_4711, "orders/order-4711.xml"));
    expect(again.first.ok && again.first.need).toEqual([]);
    expect(w.read).toEqual([]);
    expect(verdict(await again.check(ORDER_4711))).toEqual(verdict(before));
  });

  test("another order on the same schema, in the same folder, takes the set the first kept", async () => {
    const w = worker();
    w.open(orderOpen(ORDER_4711, "orders/order-4711.xml")).close();
    w.read.length = 0;
    const other = w.open(orderOpen(ORDER_4710, "orders/order-4710.xml"));
    expect(other.first.ok && other.first.need).toEqual([]);
    expect(w.read).toEqual([]);
    expect(verdict(await other.check(ORDER_4710))).toEqual([]);
  });

  test("the port opens a kept set without reading a file", async () => {
    const engine = localXsdEngine();
    const reads: string[] = [];
    const e = entryFor("orders/order-4711.xml", rootSays(ORDER_4711)!)!;
    const xsdReads: XsdReads = {
      paths: async () => [...FILES.keys()],
      read: async (p) => {
        reads.push(p);
        return FILES.get(p) ?? null;
      },
      index: async () => ({ declares: [], catalogs: [], total: 0, read: 0 }),
      pointed: [],
      keep: PIN,
    };
    const first = new XsdPort(engine);
    await first.open(e.path, e.text, xsdReads, OT, true);
    await first.whole;
    expect(reads.length).toBeGreaterThan(0);
    first.close();
    reads.length = 0;
    const second = new XsdPort(engine);
    const answer = await second.open(e.path, e.text, xsdReads, OT, true);
    await second.whole;
    expect(answer.ok && answer.need).toEqual([]);
    expect(reads).toEqual([]);
    expect(verdict(await second.check(ORDER_4711))).toHaveLength(5);
    // Without a pin nothing is kept: the reads move.
    const loose = new XsdPort(engine);
    await loose.open(e.path, e.text, { ...xsdReads, keep: null }, OT, true);
    await loose.whole;
    expect(reads.length).toBeGreaterThan(0);
  });

  test("any part of the key that differs misses, and the set is read", () => {
    const base = orderOpen(ORDER_4711, "orders/order-4711.xml");
    const pointed = [{ namespace: OT, location: "../schemas/opentrans/opentrans_2_1.xsd", to: { repo: "gitea:t/ledger-api", path: "schemas/opentrans/opentrans_2_1.xsd" } }];
    const variants: [string, Omit<Open, "kind">][] = [
      ["the repository", { ...base, keep: { ...PIN, scope: '["gitea:t/other",null]' } }],
      ["the commit", { ...base, keep: { ...PIN, at: '["c2",null,null]' } }],
      ["the working tree's stamp", { ...base, keep: { ...PIN, at: '["c1","c1","stamp-2"]' } }],
      ["the entry's folder", { ...base, entry: "elsewhere/.knobas-document-schema.xsd" }],
      ["the entry's text (another prefix)", { ...base, text: base.text.replace("<xs:import", '<xs:import xmlns:x="urn:x"') }],
      ["the namespace written bare", { ...base, home: "urn:other" }],
      ["lean or not", { ...base, lean: false }],
      ["a file pointed to", { ...base, pointed }],
      ["a download (its SHA-256)", { ...base, pointed: [{ namespace: OT, location: "x.xsd", to: { url: "https://schemas.example/x.xsd", sha256: "a".repeat(64), at: "2026-11-24" } }] as never }],
    ];
    for (const [what, call] of variants) {
      const w = worker();
      w.open(base).close();
      w.read.length = 0;
      w.open(call);
      expect(w.read.length, what).toBeGreaterThan(0);
    }
    // The bundled schemas changed: another version of them.
    const w = worker();
    w.open(base).close();
    w.read.length = 0;
    const sessions = w.sessions;
    const changed = { byNs: BUNDLED.byNs, texts: new Map([...BUNDLED.texts].map(([p, t]) => [p, `${t} `])) };
    const a = answerXsd(sessions, 99, { kind: "open", ...base }, changed) as Opened;
    expect(a.ok && a.need.length).toBeGreaterThan(0);
  });

  test("a set is kept only where the reads are pinned, only whole, and every file read", () => {
    const loose = worker();
    loose.open(orderOpen(ORDER_4711, "orders/order-4711.xml", null));
    expect(loose.kept().size).toBe(0);
    // xmlmime.xsd could not be read (a read that failed may answer next time): nothing is kept, though nothing is
    // left to read once the repository's index has answered.
    const gaps = new Map(FILES);
    gaps.delete("schemas/opentrans/xmlmime.xsd");
    const w = worker(gaps);
    const s = w.open(orderOpen(ORDER_4711, "orders/order-4711.xml"));
    expect(s.first.ok).toBe(true);
    expect(w.read).toContain("?index");
    const session = w.sessions.get(s.id)!;
    expect(session.set!.need).toEqual([]);
    expect([...session.texts.values()]).toContain(null);
    expect(w.kept().size).toBe(0);
  });

  test("eviction drops the least recently used set, never one a session holds", () => {
    const w = worker();
    const at = (n: number): KeepAt => ({ scope: `["r${n}"]`, at: '["c1"]' });
    const held = w.open(orderOpen(ORDER_4711, "orders/order-4711.xml", at(0)));
    for (let n = 1; n <= KEEP_SETS + 2; n += 1) w.open(orderOpen(ORDER_4711, "orders/order-4711.xml", at(n))).close();
    const scopes = [...w.kept().values()].map((k) => k.scope);
    expect(scopes).toHaveLength(KEEP_SETS);
    expect(scopes).toContain(at(0).scope);
    // The least recent dropped first: r1, r2 and r3 went, the newest stand.
    expect(scopes).toEqual([at(0).scope, at(4).scope, at(5).scope, at(6).scope].slice(0, KEEP_SETS));
    // Let go of, it is the least recently used and goes with the next set kept.
    held.close();
    w.open(orderOpen(ORDER_4711, "orders/order-4711.xml", at(7))).close();
    expect([...w.kept().values()].map((k) => k.scope)).not.toContain(at(0).scope);
  });

  test("a set taken again is the most recent, and is held while its session reads it", () => {
    const w = worker();
    const at = (n: number): KeepAt => ({ scope: `["r${n}"]`, at: '["c1"]' });
    for (let n = 0; n < KEEP_SETS; n += 1) w.open(orderOpen(ORDER_4711, "orders/order-4711.xml", at(n))).close();
    // r0 taken again: the most recent now, so r1 goes when another set is kept.
    w.open(orderOpen(ORDER_4711, "orders/order-4711.xml", at(0))).close();
    w.open(orderOpen(ORDER_4711, "orders/order-4711.xml", at(KEEP_SETS))).close();
    const scopes = () => [...w.kept().values()].map((k) => k.scope);
    expect(scopes()).toContain(at(0).scope);
    expect(scopes()).not.toContain(at(1).scope);
    // Taken again and held: it stays while more sets are kept than the bound, however long ago it was taken.
    const held = w.open(orderOpen(ORDER_4711, "orders/order-4711.xml", at(2)));
    expect(held.first.ok && held.first.need).toEqual([]);
    for (let n = 10; n < 10 + KEEP_SETS + 1; n += 1) w.open(orderOpen(ORDER_4711, "orders/order-4711.xml", at(n))).close();
    expect(scopes()).toContain(at(2).scope);
    // Let go of, it is the least recent, and goes with the next set kept.
    held.close();
    w.open(orderOpen(ORDER_4711, "orders/order-4711.xml", at(20))).close();
    expect(scopes()).not.toContain(at(2).scope);
  });

  test("a set a session holds stays kept past the bounds, and the session still reads it", async () => {
    const w = worker();
    const at = (n: number): KeepAt => ({ scope: `["r${n}"]`, at: '["c1"]' });
    const holding = Array.from({ length: KEEP_SETS + 2 }, (_, n) => w.open(orderOpen(ORDER_4711, "orders/order-4711.xml", at(n))));
    expect(w.kept().size).toBe(KEEP_SETS + 2);
    for (const h of holding) expect(verdict(await h.check(ORDER_4711))).toHaveLength(5);
    for (const h of holding) h.close();
    expect(w.kept().size).toBe(KEEP_SETS);
  });

  test("a new commit misses, reads the changed file, and drops the set kept for the commit before", async () => {
    const w = worker();
    const c1 = w.open(orderOpen(ORDER_4711, "orders/order-4711.xml"));
    const before = verdict(await c1.check(ORDER_4711));
    c1.close();
    // At c2 the order's header takes no ORDER_INFO: the check must say so, not answer from c1's set.
    const main = "schemas/opentrans/opentrans_2_1.xsd";
    w.repo.files = new Map([...FILES, [main, FILES.get(main)!.replace('<xsd:element ref="ORDER_INFO"/>', '<xsd:element ref="ORDER_INFO" minOccurs="0" maxOccurs="0"/>')]]);
    w.read.length = 0;
    const c2 = w.open(orderOpen(ORDER_4711, "orders/order-4711.xml", { ...PIN, at: '["c2",null,null]' }));
    expect(w.read).toContain(main);
    expect([...w.kept().values()].map((k) => k.at)).toEqual(['["c2",null,null]']);
    expect(verdict(await c2.check(ORDER_4711))).not.toEqual(before);
  });

  test("another repository's file that moved under the pin is asked again, and the set is read again, not served stale", async () => {
    // The order's schema is pointed to another repository's copy, read at its branch's tip.
    const away = "@repo/gitea%3At%2Fschemas/opentrans/opentrans_2_1.xsd";
    const pointed = [{ namespace: OT, location: "../schemas/opentrans/opentrans_2_1.xsd", to: { repo: "gitea:t/schemas", path: "opentrans/opentrans_2_1.xsd", name: "t/schemas" } }];
    const files = new Map([...FILES].map(([p, t]) => [p.replace("schemas/opentrans/", "@repo/gitea%3At%2Fschemas/opentrans/"), t]));
    let tip = files;
    const w = worker();
    const awayRead = (p: string) => tip.get(p) ?? null;
    const one = w.open(orderOpen(ORDER_4711, "orders/order-4711.xml", PIN, pointed), awayRead);
    const before = verdict(await one.check(ORDER_4711));
    expect(before).toHaveLength(5);
    one.close();
    expect([...w.kept().values()][0]?.verify).toContain(away);
    // Unmoved: asked again, alike, served whole.
    w.read.length = 0;
    const same = w.open(orderOpen(ORDER_4711, "orders/order-4711.xml", PIN, pointed), awayRead);
    expect(same.first.ok && same.first.need).toEqual([...w.kept().values()][0]!.verify);
    expect(w.read.filter((p) => !p.startsWith("@repo/"))).toEqual([]);
    expect(verdict(await same.check(ORDER_4711))).toHaveLength(5);
    same.close();
    // Moved: the order's type takes no ORDER_INFO now.
    tip = new Map(files);
    tip.set(away, files.get(away)!.replace('<xsd:element ref="ORDER_INFO"/>', '<xsd:element ref="ORDER_INFO" minOccurs="0" maxOccurs="0"/>'));
    w.read.length = 0;
    const moved = w.open(orderOpen(ORDER_4711, "orders/order-4711.xml", PIN, pointed), awayRead);
    expect(w.read).toContain(away);
    const after = verdict(await moved.check(ORDER_4711));
    expect(after).not.toEqual(before);
    moved.close();
    // The stale set went, and the set read again is the one kept: served whole, once its file answers as it does now.
    expect([...w.kept().values()].map((k) => k.base.texts.get(away))).toEqual([tip.get(away)]);
    w.read.length = 0;
    const next = w.open(orderOpen(ORDER_4711, "orders/order-4711.xml", PIN, pointed), awayRead);
    expect(next.first.ok && next.first.need).toEqual([...w.kept().values()][0]!.verify);
    expect(verdict(await next.check(ORDER_4711))).toEqual(after);
  });

  test("a session that reads its set again lets go of the kept one, which stays as it was", async () => {
    const w = worker();
    const one = w.open(orderOpen(ORDER_4711, "orders/order-4711.xml"));
    const kept = [...w.kept().values()][0]!;
    const texts = kept.base.texts;
    const size = texts.size;
    answerXsd(w.sessions, one.id, { kind: "addFiles", files: [{ path: "schemas/opentrans/extra.xsd", text: "<x/>" }] }, BUNDLED);
    expect(kept.refs).toBe(0);
    expect(kept.base.texts.size).toBe(size);
    answerXsd(w.sessions, one.id, { kind: "pointed", pointed: [] }, BUNDLED);
    const again = w.open(orderOpen(ORDER_4711, "orders/order-4711.xml"));
    expect(again.first.ok && again.first.need).toEqual([]);
    expect(verdict(await again.check(ORDER_4711))).toHaveLength(5);
  });

  test("forget drops every kept set; a session holding one still reads it", async () => {
    const w = worker();
    const held = w.open(orderOpen(ORDER_4711, "orders/order-4711.xml"));
    answerXsd(w.sessions, 0, { kind: "forget" }, BUNDLED);
    expect(w.kept().size).toBe(0);
    expect(verdict(await held.check(ORDER_4711))).toHaveLength(5);
    held.close();
    w.read.length = 0;
    w.open(orderOpen(ORDER_4711, "orders/order-4711.xml"));
    expect(w.read.length).toBeGreaterThan(0);
  });
});

describe("the bounds: a few sets and their bytes, the least recently used first, never one held", () => {
  const store = (...sets: [string, number, number][]): KeptSets =>
    new Map(sets.map(([key, bytes, refs]) => [key, { key, scope: key, at: "c", base: {} as never, refs, bytes, verify: [] } satisfies Kept]));
  test("over the count, the least recent go first", () => {
    const s = store(["a", 1, 0], ["b", 1, 0], ["c", 1, 0]);
    evict(s, 2, 100);
    expect([...s.keys()]).toEqual(["b", "c"]);
  });
  test("over the bytes, the least recent go until they fit", () => {
    const s = store(["a", 40, 0], ["b", 40, 0], ["c", 40, 0]);
    evict(s, 10, 90);
    expect([...s.keys()]).toEqual(["b", "c"]);
    evict(s, 10, 40);
    expect([...s.keys()]).toEqual(["c"]);
  });
  test("a set a session holds stays, however far over; the next least recent goes in its place", () => {
    const s = store(["a", 80, 1], ["b", 10, 0], ["c", 10, 0]);
    evict(s, 2, 90);
    expect([...s.keys()]).toEqual(["a", "c"]);
    evict(s, 1, 10);
    expect([...s.keys()]).toEqual(["a"]);
  });
  test("within the bounds nothing goes", () => {
    const s = store(["a", 30, 0], ["b", 30, 0]);
    evict(s, 2, 60);
    expect([...s.keys()]).toEqual(["a", "b"]);
  });
});

describe("libxml2's compiled schema is kept by the set's files, never by their lengths", () => {
  test("a file changed to the same length is compiled again", async () => {
    const xs = `xmlns:xs="${XS}"`;
    const schema = (n: number) => `<xs:schema ${xs}><xs:element name="a"><xs:simpleType><xs:restriction base="xs:string"><xs:maxLength value="${n}"/></xs:restriction></xs:simpleType></xs:element></xs:schema>`;
    expect(schema(5).length).toBe(schema(3).length);
    const sessions: XsdSessions = new Map();
    const run = async (id: number, n: number) => {
      answerXsd(sessions, id, { kind: "open", entry: "a.xsd", text: schema(n), paths: null, pointed: [] }, BUNDLED);
      return (await answerXsd(sessions, id, { kind: "check", xml: "<a>abcd</a>" }, BUNDLED)) as Checked;
    };
    expect((await run(1, 5)).state === "checked" && (await run(1, 5))).toMatchObject({ total: 0 });
    const after = await run(2, 3);
    expect(after.state === "checked" && after.total).toBe(1);
  });
});
