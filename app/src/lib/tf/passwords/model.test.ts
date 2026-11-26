import { expect, test, vi } from "vitest";

import type { Held, PwRow } from "../../ipc/passwords";
import { CUT, chainOf, commentFolder, expiry, holding, homesOf, entryMarks, indexBuild, model, ordered, panels, pathOf, scopedEntries, searchIndex, sourceNode, sourceWord, treeRows } from "./model";

const offers = { one_time_codes: true, expiry: true, usage_comments: true, audit_log: true, history: null, attachments: false, access_requests: false, marks_protected_fields: false, writes: [] };

function source(id: string, name: string) {
  return { id, name, account: "mara", by_hand: true, synced_at: null, down_at: null, never_synced: false, offers, skipped_root: null, adapter_name: "Pleasant", web: { entry: false, folder: false } };
}

function row(id: string, folder: string | null, title: string, user: string | null = null, src = 0): PwRow {
  return [id, src, folder, title, user, null, null, 0, null, []];
}

/** Payments ▸ Prod ▸ Databases (comment-required at Prod), Platform; eight entries in Databases. */
function held(): Held {
  return {
    sources: [source("pw", "Team passwords")],
    folders: [
      { id: "pw:pay", source: 0, parent: null, name: "Payments", comment_read: false, can_add: true },
      { id: "pw:prod", source: 0, parent: "pw:pay", name: "Prod", comment_read: true, can_add: true },
      { id: "pw:db", source: 0, parent: "pw:prod", name: "Databases", comment_read: false, can_add: true },
      { id: "pw:plat", source: 0, parent: null, name: "Platform", comment_read: false, can_add: true },
    ],
    entries: [
      row("pw:root1", null, "loose entry"),
      row("pw:p1", "pw:pay", "payments admin", "pay_admin"),
      ...Array.from({ length: 8 }, (_, i) => row(`pw:d${i}`, "pw:db", `db user ${8 - i}`, `user_${i}`)),
      row("pw:pl1", "pw:plat", "Grafana", "grafana"),
    ],
    recent: ["pw:pl1", "pw:gone"],
  };
}

test("the tree nests folders under their parents, sorted, with totals that count everything under them", () => {
  const m = model(held());
  expect(m.root.kids.map((n) => n.name)).toEqual(["Payments", "Platform"]);
  const pay = m.nodes.get("pw:pay")!;
  expect(pay.total).toBe(9);
  expect(m.root.total).toBe(11);
  expect(m.nodes.get("pw:db")!.depth).toBe(2);
  // Entries sort by title in a folder, case-insensitively.
  expect(ordered(m, m.nodes.get("pw:db")!).map((e) => e.title).slice(0, 2)).toEqual(["db user 1", "db user 2"]);
  expect(pathOf(m, m.entries.get("pw:d0")!)).toEqual(["Payments", "Prod", "Databases"]);
  expect(chainOf(m, m.root)).toEqual([]);
});

test("a folder lists its own entries, then a panel per folder below; the top lists the recent ones first", () => {
  const m = model(held());
  const top = panels(m, m.root, "");
  expect(top.map((p) => p.kind)).toEqual(["recent", "folder", "folder", "folder", "folder"]);
  // A recent id the read no longer has is left out.
  expect(top[0]!.rows.map((e) => e.id)).toEqual(["pw:pl1"]);
  expect(top.slice(1).map((p) => p.node!.name)).toEqual(["All entries", "Payments", "Databases", "Platform"]);
  const pay = panels(m, m.nodes.get("pw:pay")!, "");
  expect(pay.map((p) => [p.node!.name, p.rows.length])).toEqual([
    ["Payments", 1],
    ["Databases", 8],
  ]);
  expect(CUT).toBe(5);
});

test("the filter needs every word somewhere in title, username, address or path, any case", () => {
  const m = model(held());
  expect(panels(m, m.root, "DB prod")[0]!.rows).toHaveLength(8);
  expect(panels(m, m.root, "grafana platform")[0]!.rows.map((e) => e.id)).toEqual(["pw:pl1"]);
  expect(panels(m, m.root, "user_3")[0]!.rows.map((e) => e.id)).toEqual(["pw:d3"]);
  expect(panels(m, m.root, "nothing here")[0]!.rows).toEqual([]);
});

test("the app's ranking is held to the folder's scope and keeps its order, best first (#2535)", () => {
  const scoped = held();
  scoped.entries.push(["pw:shared", 0, "pw:plat", "shared admin", null, null, null, 0, null, ["pw:db"]]);
  const m = model(scoped);
  const e = (id: string) => m.entries.get(id)!;
  // As the app ranked every entry: the ranking's order, whatever the titles' order.
  const ranked = [e("pw:pl1"), e("pw:shared"), e("pw:p1"), e("pw:d3")];
  expect(panels(m, m.root, "x", ranked)[0]!.rows.map((r) => r.id)).toEqual(["pw:pl1", "pw:shared", "pw:p1", "pw:d3"]);
  // Payments holds its own entry and, through Prod ▸ Databases, the one also listed there and the database's own.
  expect(panels(m, m.nodes.get("pw:pay")!, "x", ranked)[0]!.rows.map((r) => r.id)).toEqual(["pw:shared", "pw:p1", "pw:d3"]);
  expect(panels(m, m.nodes.get("pw:plat")!, "x", ranked)[0]!.rows.map((r) => r.id)).toEqual(["pw:pl1", "pw:shared"]);
  expect(panels(m, m.nodes.get("pw:plat")!, "x", ranked)[0]!.count).toBe(2);
  // What the app holds at a folder: the entries of its scope, by title; every entry at All entries.
  expect(scopedEntries(m, m.nodes.get("pw:plat")!).map((r) => r.id)).toEqual(["pw:pl1", "pw:shared"]);
  expect(scopedEntries(m, m.root)).toHaveLength(m.entries.size);
});

test("an entry's text is its title, username, address and path, a line each, and its marks fall on the cells drawn (#2535)", () => {
  const withAddress = held();
  withAddress.entries.push(["pw:web", 0, "pw:db", "Ledger", "ops", "https://ledger.example/", null, 0, null, []]);
  const m = model(withAddress);
  const web = m.entries.get("pw:web")!;
  expect(searchIndex(m).textOf(web)).toBe("Ledger\nops\nhttps://ledger.example/\nPayments Prod Databases");
  // "Le" of the title, "op" of the username, "led" of the address past its scheme.
  expect(entryMarks(web, [0, 2, 7, 9, 19, 22])).toEqual({ title: [0, 2], user: [0, 2], address: [0, 3] });
});

test("the filter matches only inside the folder on show and the folders under it; at the top, everything (#2531)", () => {
  const scoped = held();
  // Grafana is also listed in Databases: a match in a folder it is also in counts there.
  scoped.entries.push(["pw:shared", 0, "pw:plat", "shared admin", null, null, null, 0, null, ["pw:db"]]);
  const m = model(scoped);
  const ids = (node: string, q: string) => panels(m, m.nodes.get(node)!, q)[0]!.rows.map((e) => e.id).sort();
  // "admin" is in Payments (payments admin) and Platform (shared admin).
  expect(panels(m, m.root, "admin")[0]!.rows.map((e) => e.id).sort()).toEqual(["pw:p1", "pw:shared"]);
  expect(ids("pw:plat", "admin")).toEqual(["pw:shared"]);
  // Payments holds its own entry and, through Prod ▸ Databases, the one also listed there.
  expect(ids("pw:pay", "admin")).toEqual(["pw:p1", "pw:shared"]);
  expect(ids("pw:prod", "admin")).toEqual(["pw:shared"]);
  expect(ids("pw:db", "db user")).toHaveLength(8);
  expect(ids("pw:plat", "db user")).toEqual([]);
  // A path word still matches inside the scope: "prod" names Databases' chain.
  expect(ids("pw:pay", "prod")).toHaveLength(8);
});

test("with two sources a source's top scopes the filter to that source, entries outside its folders included (#2531)", () => {
  const two = held();
  two.sources.push(source("org", "Tidewater org"));
  two.folders.push({ id: "org:f", source: 1, parent: null, name: "Shared", comment_read: false, can_add: false });
  two.entries.push(row("org:e", "org:f", "shared admin", null, 1), row("org:loose", null, "loose admin", null, 1));
  const m = model(two);
  const top = m.nodes.get(sourceNode("org"))!;
  expect(panels(m, top, "admin")[0]!.rows.map((e) => e.id).sort()).toEqual(["org:e", "org:loose"]);
  expect(panels(m, m.root, "admin")[0]!.rows.map((e) => e.id).sort()).toEqual(["org:e", "org:loose", "pw:p1"]);
});

test("the rule of a comment-required folder covers the folders under it", () => {
  const m = model(held());
  expect(commentFolder(m.nodes.get("pw:db")!)?.id).toBe("pw:prod");
  expect(commentFolder(m.nodes.get("pw:pay")!)).toBeNull();
});

test("the tree's rows are the open folds' children; the folds holding an entry are known", () => {
  const m = model(held());
  expect(treeRows(m, new Set()).map((r) => r.node.name)).toEqual(["All entries", "Payments", "Platform"]);
  expect(treeRows(m, new Set(["pw:pay", "pw:prod"])).map((r) => r.node.name)).toEqual(["All entries", "Payments", "Prod", "Databases", "Platform"]);
  expect([...holding(m, m.entries.get("pw:d0")!)].sort()).toEqual(["", "pw:db", "pw:pay", "pw:prod"]);
});

test("with two sources each has a top named after it, and the source's name is its word", () => {
  const two = held();
  two.sources.push(source("org", "Tidewater org"));
  two.folders.push({ id: "org:f", source: 1, parent: null, name: "Shared", comment_read: false, can_add: false });
  two.entries.push(row("org:e", "org:f", "shared thing", null, 1));
  const m = model(two);
  expect(m.root.kids.map((n) => n.name)).toEqual(["Team passwords", "Tidewater org"]);
  expect(m.nodes.get(sourceNode("org"))!.kids.map((n) => n.name)).toEqual(["Shared"]);
  expect(pathOf(m, m.entries.get("org:e")!)).toEqual(["Tidewater org", "Shared"]);
  expect(sourceWord(two, 1)).toBe("Tidewater org");
  expect(sourceWord(held(), 0)).toBe("the password manager");
});

test("an expiry turns warn within two weeks and err within three days", () => {
  const now = new Date(2026, 10, 22);
  expect(expiry("2026-11-25", now)).toMatchObject({ days: 3, tone: "err", text: "Expires in 3 days" });
  expect(expiry("2026-12-01", now)).toMatchObject({ tone: "warn" });
  expect(expiry("2027-03-01", now)).toMatchObject({ tone: "" });
  expect(expiry("2026-11-21", now)).toMatchObject({ text: "Expired" });
  expect(expiry(null, now)).toBeNull();
});

test("the model sorts no entries and lower-cases nothing: a folder sorts when its rows are read, the filter on its first use", () => {
  // 600 entries in four folders of 150. At 20,000 entries the sorts of every folder and of all entries, and the
  // filter's text, were most of the first open, which draws a dozen rows (the ux30 end gate, 2026-11-23).
  const big = held();
  big.entries = Array.from({ length: 600 }, (_, i) => row(`pw:x${i}`, ["pw:pay", "pw:prod", "pw:db", "pw:plat"][i % 4]!, `Title ${(i * 7919) % 600}`, `User${i}`));
  const sorted: number[] = [];
  const sort = Array.prototype.sort;
  const sortSpy = vi.spyOn(Array.prototype, "sort").mockImplementation(function (this: unknown[], compare) {
    sorted.push(this.length);
    return sort.call(this, compare);
  });
  const lower = vi.spyOn(String.prototype, "toLowerCase");
  try {
    const m = model(big);
    expect(lower).not.toHaveBeenCalled();
    expect(m.entries.made).toBe(0);
    expect(m.root.total).toBe(600);
    const top = panels(m, m.root, "");
    // The tree's folders sort; no list of entries does.
    expect(Math.max(...sorted)).toBeLessThanOrEqual(2);
    expect(top.map((p) => p.count)).toEqual([150, 150, 150, 150]);
    // A panel's rows, read, sort that folder alone, by title.
    const db = top.find((p) => p.node?.id === "pw:db")!;
    const titles = db.rows.map((e) => e.title);
    expect(sorted.filter((n) => n > 2)).toEqual([150]);
    expect(m.entries.made).toBe(150);
    expect(m.entries.get("pw:x7")?.title).toBe(`Title ${(7 * 7919) % 600}`);
    expect(m.entries.get("pw:nothing")).toBeUndefined();
    // The filter builds what it reads on its first keystroke.
    const hits = panels(m, m.root, "user1")[0]!.rows.map((e) => e.title);
    // In runs of 512 (`IndexBuild`'s merge sort), all 600 of them.
    expect(sorted.filter((n) => n > 2)).toEqual([150, 512, 88]);
    sortSpy.mockRestore();
    const byTitle = (a: string, b: string) => a.localeCompare(b, undefined, { sensitivity: "base" });
    expect(titles).toEqual([...titles].sort(byTitle));
    expect(hits).toHaveLength(111);
    expect(hits).toEqual([...hits].sort(byTitle));
  } finally {
    sortSpy.mockRestore();
    lower.mockRestore();
  }
});

test("an entry's homes are the nodes the model put it in: its folder, the ones it is also in, else the top", () => {
  const two = held();
  two.sources.push(source("org", "Tidewater org"));
  two.entries.push(row("org:loose", null, "loose org thing", null, 1));
  two.entries.push(["pw:both", 0, "pw:db", "in two", null, null, null, 0, null, ["pw:plat"]]);
  const m = model(two);
  expect(homesOf(m, m.entries.get("pw:both")!).map((n) => n.id)).toEqual(["pw:db", "pw:plat"]);
  expect(homesOf(m, m.entries.get("org:loose")!).map((n) => n.id)).toEqual([sourceNode("org")]);
  for (const e of m.entries.values()) for (const home of homesOf(m, e)) expect(ordered(m, home)).toContain(e);
});

test("the filter's index builds in slices that each stop when told, and comes out as one build at once", () => {
  // 3,001 entries in titles out of order, some the same but for case: the merge sort's runs of 512 and its last
  // uneven pair. Built at once it was one task of 140 ms at 4x at 20,000 entries (the ux30 end gate, 2026-11-23).
  const big = held();
  big.entries = Array.from({ length: 3001 }, (_, i) => row(`pw:y${i}`, i % 3 ? "pw:db" : null, `${i % 2 ? "Key" : "key"} ${(i * 7919) % 1500}`, `u${i}`));
  const byTitle = (a: { title: string; id: string }, b: { title: string; id: string }) =>
    new Intl.Collator(undefined, { sensitivity: "base" }).compare(a.title, b.title) || (a.id < b.id ? -1 : a.id > b.id ? 1 : 0);
  const sliced = indexBuild(model(big));
  let steps = 0;
  while (!sliced.step(() => false)) steps++;
  expect(steps).toBeGreaterThan(20);
  const whole = searchIndex(model(big));
  expect(sliced.index!.all.map((e) => e.id)).toEqual(whole.all.map((e) => e.id));
  expect(sliced.index!.hay).toEqual(whole.hay);
  expect(whole.all.map((e) => e.id)).toEqual([...whole.all].sort(byTitle).map((e) => e.id));
  expect(whole.hay[0]).toContain("\nu");
  // A keystroke before the slices are done finishes the build where it stands.
  const half = model(big);
  for (let i = 0; i < 10; i++) indexBuild(half).step(() => false);
  expect(indexBuild(half).index).toBeNull();
  expect(searchIndex(half).all.map((e) => e.id)).toEqual(whole.all.map((e) => e.id));
  expect(panels(half, half.root, "key 7")[0]!.count).toBe(whole.hay.filter((h) => h.toLowerCase().includes("key") && h.includes("7")).length);
});
