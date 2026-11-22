import { expect, test } from "vitest";

import type { Held, PwRow } from "../../ipc/passwords";
import { CUT, chainOf, commentFolder, expiry, holding, marks, model, panels, pathOf, sourceNode, sourceWord, treeRows } from "./model";

const offers = { one_time_codes: true, expiry: true, usage_comments: true, audit_log: true, history: null, attachments: false, access_requests: false, marks_protected_fields: false, writes: [] };

function source(id: string, name: string) {
  return { id, name, account: "mara", by_hand: true, synced_at: null, down_at: null, never_synced: false, offers, skipped_root: null, adapter_name: "Pleasant" };
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
  expect(m.nodes.get("pw:db")!.own.map((e) => e.title).slice(0, 2)).toEqual(["db user 1", "db user 2"]);
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
  expect(marks("db user 1", "USER")).toEqual([
    { text: "db ", hit: false },
    { text: "user", hit: true },
    { text: " 1", hit: false },
  ]);
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
