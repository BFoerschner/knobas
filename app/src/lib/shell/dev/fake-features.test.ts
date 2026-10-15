/**
 * The fixture's feature modules (#908): each one under `fake/` exports its
 * `handlers`, and no two of them answer the same command.
 */
import { expect, test } from "vitest";

import { demoHandlers } from "./fake-tauri";

const MODULES = import.meta.glob<{ handlers?: unknown }>("./fake/*.ts", { eager: true });

test("every module under fake/ is a feature with handlers", () => {
  // The glob found the modules at all, so the loop below is not vacuous.
  expect(Object.keys(MODULES)).toContain("./fake/corpus.ts");
  for (const [path, module] of Object.entries(MODULES)) {
    expect(typeof module.handlers, `${path} exports no handlers`).toBe("function");
  }
});

test("no two features answer one command, and none answers nothing", () => {
  const params = new URLSearchParams();
  const counts = Object.values(MODULES).map(
    (module) => Object.keys((module.handlers as (p: URLSearchParams) => object)(params)).length,
  );
  expect(counts).not.toContain(0);
  const counted = counts.reduce((sum, count) => sum + count, 0);
  expect(Object.keys(demoHandlers(params))).toHaveLength(counted);
});

// -- the TF2-TF6 fixture fill (#921) ----------------------------------------

type Calls = Record<string, (args: Record<string, unknown>) => unknown>;

test("the inbox: snooze moves a row to the other shelf, done removes it, the count follows the stream", () => {
  // One handler table, so state survives between calls.
  const h = demoHandlers() as Calls;
  const stream = h["inbox_items"]!({ shelf: "stream" }) as { item: { key: string } }[];
  expect(h["inbox_count"]!({})).toBe(stream.length);
  const [first, second] = stream;
  h["snooze_inbox_item"]!({ itemKey: first!.item.key, until: "2026-09-01T07:00:00Z" });
  h["complete_inbox_item"]!({ itemKey: second!.item.key });
  expect(h["inbox_count"]!({})).toBe(stream.length - 2);
  const later = h["inbox_items"]!({ shelf: "snoozed" }) as { item: { key: string; snoozed_until: string } }[];
  expect(later[0]!.item.key).toBe(first!.item.key);
  expect(() => h["complete_inbox_item"]!({ itemKey: "nope" })).toThrow();
});

test("the week and Log all: the label and passive blocks are never planned, and logging empties the plan", () => {
  const h = demoHandlers() as Calls;
  const today = new Date();
  const from = new Date(today.getFullYear(), today.getMonth(), today.getDate());
  const to = new Date(from.getTime() + 86_400_000);
  const day = `${from.getFullYear()}-${String(from.getMonth() + 1).padStart(2, "0")}-${String(from.getDate()).padStart(2, "0")}`;
  const dayBefore = new Date(from.getFullYear(), from.getMonth(), from.getDate() - 1);
  const dayBeforeName = `${dayBefore.getFullYear()}-${String(dayBefore.getMonth() + 1).padStart(2, "0")}-${String(dayBefore.getDate()).padStart(2, "0")}`;
  const days = [
    { day: dayBeforeName, from: dayBefore.toISOString(), to: from.toISOString() },
    { day, from: from.toISOString(), to: to.toISOString() },
  ];
  const plan = h["log_all_preview"]!({ days }) as { day: string; entity_id: string; seconds: number }[];
  // Yesterday's fixed 1h 30m row (prototype-6's Thu 21), then today's PAY-231.
  expect(plan.map((row) => [row.day, row.entity_id, row.seconds])).toEqual([
    [dayBeforeName, "mock:PAY-231", 90 * 60],
    [day, "mock:PAY-231", 130 * 60],
  ]);
  const logged = h["log_all"]!({ days }) as unknown[];
  expect(logged).toHaveLength(2);
  expect(h["log_all_preview"]!({ days })).toEqual([]);
  const week = h["week_timesheet"]!({ days: [days[0], days[0], days[0], days[0], days[0]] }) as { rows: unknown[] };
  expect(week.rows).toHaveLength(5);
});

test("Log all's dialog path: a draft comment per row, and log_work per row empties the plan (#828)", () => {
  const h = demoHandlers() as Calls;
  const key = (at: Date) =>
    `${at.getFullYear()}-${String(at.getMonth() + 1).padStart(2, "0")}-${String(at.getDate()).padStart(2, "0")}`;
  const from = new Date();
  from.setHours(0, 0, 0, 0);
  const before = new Date(from.getFullYear(), from.getMonth(), from.getDate() - 1);
  const to = new Date(from.getFullYear(), from.getMonth(), from.getDate() + 1);
  const days = [
    { day: key(before), from: before.toISOString(), to: from.toISOString() },
    { day: key(from), from: from.toISOString(), to: to.toISOString() },
  ];
  const plan = h["log_all_preview"]!({ days }) as { day: string; entity_id: string; started_at: string; seconds: number }[];
  const draft = h["worklog_draft"]!({ entityId: "mock:PAY-231", day: key(before), offsetMinutes: 0 }) as { comment: string };
  expect(draft.comment).toBe("Retry classification for TEMP_UNAVAILABLE; review on PR #142.");
  for (const row of plan) {
    h["log_work"]!({ entityId: row.entity_id, day: row.day, offsetMinutes: 0, startedAt: row.started_at, seconds: row.seconds, comment: "" });
  }
  expect(h["log_all_preview"]!({ days })).toEqual([]);
});

test("the write queue holds the Week tab's refused worklog on PAY-228 (#828)", () => {
  const h = demoHandlers() as Calls;
  const rows = h["pending_writes"]!({}) as { op: string; entity_id: string; state: string }[];
  expect(rows.filter((row) => row.op === "log_work")).toEqual([
    expect.objectContaining({ entity_id: "mock:PAY-228", state: "refused" }),
  ]);
});

test("a route can be created, edited and deleted, and a missing exposer is refused", () => {
  const h = demoHandlers() as Calls;
  const exposer = (h["asset_tree"]!({}) as { id: string }[])[0]!.id;
  const made = h["create_route"]!({ assetId: exposer, name: "docs", url: "http://127.0.0.1:1/x" }) as { id: string };
  const edited = h["edit_route"]!({ routeId: made.id, edits: [{ field: "name", value: "docs2" }] }) as { name: string };
  expect(edited.name).toBe("docs2");
  h["delete_route"]!({ routeId: made.id });
  expect(() => h["get_route"]!({ routeId: made.id })).toThrow();
  expect(() => h["create_route"]!({ assetId: "asset:nope", name: "x", url: "y" })).toThrow();
});

test("a link made shows on both ends and unlink takes it off both", () => {
  const h = demoHandlers() as Calls;
  const made = h["create_link"]!({ fromId: "mock:PAY-228", toId: "mock:OPS-77", relation: "blocks", note: null }) as { id: string };
  const links = (id: string) => (h["get_entity"]!({ entityId: id }) as { links: { link: { id: string } }[] }).links;
  expect(links("mock:PAY-228").map((entry) => entry.link.id)).toContain(made.id);
  expect(links("mock:OPS-77").map((entry) => entry.link.id)).toContain(made.id);
  h["unlink"]!({ linkId: made.id });
  expect(links("mock:OPS-77").map((entry) => entry.link.id)).not.toContain(made.id);
});

test("a publish reads pending and then sent, and an action-item ticket likewise", () => {
  const h = demoHandlers() as Calls;
  const published = h["publish_standup_protocol"]!({ day: "2026-08-22" }) as { publication: { state: string } };
  expect(published.publication.state).toBe("pending");
  expect((h["standup_protocol"]!({ day: "2026-08-22" }) as typeof published).publication.state).toBe("sent");
  const ticket = h["create_action_item_ticket"]!({}) as { write_id: number; ticket_entity_id: string | null };
  expect(ticket.ticket_entity_id).toBeNull();
  expect((h["action_item_ticket"]!({ noteId: "n", writeId: ticket.write_id }) as typeof ticket).ticket_entity_id).toBeNull();
  expect((h["action_item_ticket"]!({ noteId: "n", writeId: ticket.write_id }) as typeof ticket).ticket_entity_id).not.toBeNull();
  const digest = h["standup_digest"]!({ today: { day: "2026-08-22" }, earlier: [{ day: "2026-08-21" }] }) as {
    yesterday_day: string;
    yesterday: unknown[];
  };
  expect(digest.yesterday_day).toBe("2026-08-21");
  expect(digest.yesterday).toHaveLength(3);
});

test("settings preferences keep what they are told, and a clone answers cloning then stopped", () => {
  const h = demoHandlers() as Calls;
  expect(h["set_notification_kinds"]!({ kinds: ["mention", "bogus"] })).toEqual(["mention"]);
  expect(h["set_passive_attribution"]!({ enabled: false })).toBe(false);
  expect(h["passive_attribution"]!({})).toBe(false);
  expect(() => h["set_monitoring_settings"]!({ settings: { sample_retention_days: 0, response_time_warn_ms: 5 } })).toThrow();
  expect((h["backup_status"]!({}) as { archives: unknown[] }).archives.length).toBeGreaterThan(1);
  const repo = "mock:tidewater/ledger";
  const first = h["clone_repo"]!({ entityId: repo }) as { status: string };
  expect(["cloning", "adopted"]).toContain(first.status);
  if (first.status === "cloning") {
    expect((h["clone_repo"]!({ entityId: repo, stop: true }) as { status: string }).status).toBe("stopped");
  }
});
