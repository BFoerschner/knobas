/**
 * The fixture's feature modules (#908): each one under `fake/` exports its
 * `handlers`, and no two of them answer the same command.
 */
import { expect, test, vi } from "vitest";

import type { AdHocBlock, DayBlock, OpenStretch } from "../../ipc/time";
import type { EntityDetail, EntityPage, SourceView } from "../../ipc/entity";
import { repoOfRow, underRepoHash } from "../../tf/repo/repo";
import { dayRows, offerRuns } from "../../tf/time/day-view";
import { demoHandlers } from "./fake-tauri";
import { fakeRoleScope } from "./fake/corpus";
import { FIXTURE_SOURCES } from "./fake/sources";

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

test("the inbox's failed sign-ins are the fixture's sources whose sign-in fails", () => {
  const h = demoHandlers() as Calls;
  const stream = h["inbox_items"]!({ shelf: "stream" }) as { item: { key: string; reason: string; occurred_at: string } }[];
  const signIns = stream.filter((entry) => entry.item.key.startsWith("failed_sign_in:"));
  expect(signIns.map((entry) => [entry.item.key, entry.item.reason])).toEqual([
    ["failed_sign_in:teamcity", "the Tidewater TeamCity sign-in was rejected"],
    ["failed_sign_in:kuma", "no credential is stored for Uptime Kuma"],
  ]);
  const times = stream.map((entry) => entry.item.occurred_at);
  expect(times, "still newest first").toEqual(times.slice().sort().reverse());
});

test("the week and Log all's plan: the label and passive blocks are never planned", () => {
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
  const week = h["week_timesheet"]!({ days: [days[0], days[0], days[0], days[0], days[0]] }) as { rows: unknown[] };
  expect(week.rows).toHaveLength(5);
});

test("today's fake day holds one run of offered rows to PAY-231, so ?fake-ipc draws it (#1108)", () => {
  const h = demoHandlers() as Calls;
  const now = new Date();
  const from = new Date(now.getFullYear(), now.getMonth(), now.getDate());
  const to = new Date(from.getTime() + 86_400_000);
  const day = `${from.getFullYear()}-${String(from.getMonth() + 1).padStart(2, "0")}-${String(from.getDate()).padStart(2, "0")}`;
  const read = h["day_blocks"]!({ from: from.toISOString(), to: to.toISOString() }) as {
    blocks: DayBlock[];
    open: OpenStretch[];
  };
  const offers: Record<number, AdHocBlock | null> = {};
  for (const entry of read.blocks) offers[entry.block.id] = h["ad_hoc_block"]!({ blockId: entry.block.id }) as AdHocBlock | null;
  const rows = dayRows({ day, blocks: read.blocks, offers, open: read.open, running: null, now });
  const runs = offerRuns(rows);
  expect(runs.map((run) => [run.entityId, run.rows.length, run.ms / 60_000])).toEqual([["mock:PAY-231", 3, 47]]);
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

// Pinned on both sides of midnight: the clock must not decide this test (#1155).
test.each([
  ["00:05", [0, 5]],
  ["00:20", [0, 20]],
  ["12:00", [12, 0]],
  ["23:55", [23, 55]],
] as const)("with fake-timer at %s, PAY-231's draft counts the running stretch up to now, and a stop leaves it a block (#896)", (_label, [hour, minute]) => {
  vi.useFakeTimers();
  try {
    vi.setSystemTime(new Date(2026, 9, 17, hour, minute, 0));
    const h = demoHandlers(new URLSearchParams("fake-timer")) as Calls;
    const timer = h["current_timer"]!({}) as { started_at: string };
    const at = new Date(timer.started_at);
    const day = `${at.getFullYear()}-${String(at.getMonth() + 1).padStart(2, "0")}-${String(at.getDate()).padStart(2, "0")}`;
    const ask = () =>
      h["worklog_draft"]!({ entityId: "mock:PAY-231", day, offsetMinutes: 0 }) as {
        running_since: string | null;
        seconds: number;
        block_ids: number[];
      };
    const open = ask();
    expect(open.running_since).toBe(timer.started_at);
    expect(open.seconds).toBeGreaterThanOrEqual(34 * 60);
    h["stop_timer"]!({});
    const stopped = ask();
    expect(stopped.running_since).toBeNull();
    expect(stopped.block_ids.length).toBe(open.block_ids.length + 1);
  } finally {
    vi.useRealTimers();
  }
});

test("at ten past midnight the timer started yesterday, and its draft still counts the running stretch (#1031)", () => {
  vi.useFakeTimers();
  try {
    vi.setSystemTime(new Date(2026, 9, 17, 0, 10, 0));
    const h = demoHandlers(new URLSearchParams("fake-timer")) as Calls;
    const timer = h["current_timer"]!({}) as { started_at: string };
    expect(timer.started_at < new Date(2026, 9, 17).toISOString()).toBe(true);
    const draft = h["worklog_draft"]!({ entityId: "mock:PAY-231", day: "2026-10-16", offsetMinutes: 0 }) as {
      running_since: string | null;
      seconds: number;
    };
    expect(draft.running_since).toBe(timer.started_at);
    expect(draft.seconds).toBeGreaterThanOrEqual(34 * 60);
  } finally {
    vi.useRealTimers();
  }
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

// -- Open in… (#844) ----------------------------------------------------------

test("the applications are macOS's four, a saved list replaces them, and opening answers the argv", () => {
  const h = demoHandlers() as Calls;
  type Settings = { default_application: string | null; applications: { id: string; name: string }[] };
  const first = h["checkout_settings"]!({}) as Settings;
  expect(first.applications.map((app) => app.name)).toEqual(["VS Code", "Terminal", "Fork", "IntelliJ IDEA"]);
  expect(first.default_application).toBe("vscode");

  const argv = h["open_checkout"]!({ entityId: "mock:tidewater/payout-service", application: null, file: "src/lib.rs", line: 3 });
  expect(argv).toEqual(["open", "-a", "Visual Studio Code", "/Users/mara/src/payout-service/src/lib.rs"]);
  // A command the webview names is not run: the program is a saved one (#1643, ADR-0016).
  const named = h["open_checkout"]!({ entityId: "mock:tidewater/payout-service", application: "terminal", command: "/bin/sh {path}" });
  expect(named).toEqual(["open", "-a", "Terminal", "/Users/mara/src/payout-service"]);

  expect(() =>
    h["set_checkout_applications"]!({ applications: [{ id: null, name: "X", command: "x {branch}", file_command: null }], defaultApplication: null }),
  ).toThrow();
  const saved = h["set_checkout_applications"]!({
    applications: [...first.applications, { id: null, name: "My Git Viewer", command: "mgv {path}", file_command: null }],
    defaultApplication: "my-git-viewer",
  }) as Settings;
  expect(saved.applications.at(-1)?.id).toBe("my-git-viewer");
  expect(saved.default_application).toBe("my-git-viewer");
});

// -- the job's reach and Start work (#817, #818) ------------------------------

test("job_reach labels a sub-task's parent links behind the switch, and list_entities scopes by the same table", () => {
  const h = demoHandlers() as Calls;
  const own = h["job_reach"]!({ entityId: "mock:PAY-231" }) as { entity_id: string; kind: string; origin: { kind: string } }[];
  expect(own.some((row) => row.kind === "build")).toBe(true);
  // Since #866 PAY-231's link sets (Payout stack by its epic, Payments staging by hand) join it.
  expect(own.every((row) => row.origin.kind === "own" || row.origin.kind === "link_set")).toBe(true);

  const sub = h["job_reach"]!({ entityId: "mock:PAY-232" }) as { entity_id: string; origin: { kind: string; ticket?: string } }[];
  expect(sub.length).toBeGreaterThan(0);
  const inherited = sub.filter((row) => row.origin.kind !== "link_set");
  expect(inherited.length).toBeGreaterThan(0);
  expect(inherited.every((row) => row.origin.kind === "parent" && row.origin.ticket === "mock:PAY-231")).toBe(true);

  const page = h["list_entities"]!({
    filter: { sources: [], kinds: ["build"], order: "latest_per_parent", context: null, reach: "mock:PAY-231", include_deleted: false },
    limit: 50,
    offset: 0,
  }) as { rows: { entity_id: string; state: { tone: string } | null }[] };
  const builds = own.filter((row) => row.kind === "build" && row.origin.kind === "own").map((row) => row.entity_id).sort();
  expect(page.rows.map((row) => row.entity_id).sort()).toEqual(builds);
  expect(page.rows.some((row) => row.state?.tone === "err")).toBe(true);

  expect(h["set_parent_links"]!({ enabled: false })).toBe(false);
  const off = h["job_reach"]!({ entityId: "mock:PAY-232" }) as { origin: { kind: string } }[];
  expect(off.every((row) => row.origin.kind === "link_set")).toBe(true);
  expect(h["set_parent_links"]!({ enabled: true })).toBe(true);
});

// -- a role, answered by the mirror (#1294) ----------------------------------

test("list_entities asked for a role reads its fillers' rows of its kinds, and a miss takes the source off", () => {
  const h = demoHandlers() as Calls;
  const read = (role: string, kinds: string[] = []) =>
    (
      h["list_entities"]!({
        filter: { sources: [], kinds, order: "updated_desc", context: null, project: null, include_deleted: false, role },
        limit: 500,
        offset: 0,
      }) as { rows: { source_id: string; kind: string }[]; total: number }
    ).rows.map((row) => `${row.source_id}/${row.kind}`);
  const pairs = (rows: string[]) => [...new Set(rows)].sort();

  // CI is TeamCity's, and its kinds are the frames its adapter declares: the
  // corpus holds its runs (its pipelines are `builds.ts`'s own read).
  expect(pairs(read("ci"))).toEqual(["teamcity/build"]);
  // CI's scope is its run kinds: TeamCity's `build_config`, declared on the
  // pipeline frame, is the pipelines read's, not the entity list's (#1344).
  expect(fakeRoleScope("ci")).toEqual({ sources: ["teamcity"], kinds: ["build"] });
  // The code host is the mock's, of the code host's kinds only.
  const code = pairs(read("code-host"));
  expect(code.length).toBeGreaterThan(0);
  expect(code.every((pair) => /^mock\/(repo|branch|pr|commit)$/.test(pair))).toBe(true);
  // Named kinds intersect the role's.
  expect(pairs(read("code-host", ["pr", "ticket"]))).toEqual(["mock/pr"]);
  // Jira holds three of the mock's keys too (#1476).
  expect(pairs(read("ticket-system"))).toEqual(["jira/ticket", "mock/ticket"]);
  // An unknown role is an empty page.
  expect(read("nobody:fills-this")).toEqual([]);

  // A recorded miss of the role takes the source off that surface alone.
  const mock = FIXTURE_SOURCES.find((source) => source.id === "mock") as { role_misses: { role: string }[] };
  mock.role_misses = [{ role: "code-host" }];
  try {
    expect(read("code-host")).toEqual([]);
    expect(read("ticket-system")).toContain("mock/ticket");
  } finally {
    mock.role_misses = [];
  }
});

test("the start-work flow proposes in the named repository, then runs to queued writes and a done link", () => {
  const h = demoHandlers() as Calls;
  expect(h["start_work_flow"]!({ entityId: "mock:PAY-236", repoId: null })).toEqual([]);
  const proposed = h["start_work_flow"]!({ entityId: "mock:PAY-236", repoId: "mock:tidewater/payout-service" }) as {
    step: string;
    outcome: string;
    payload: { CreateBranch?: { name: string; entity: string } };
  }[];
  expect(proposed.map((step) => step.outcome)).toEqual(["pending", "pending", "pending", "pending"]);
  expect(proposed[0]!.payload.CreateBranch).toMatchObject({
    entity: "mock:tidewater/payout-service",
    name: "feature/PAY-236-payout-csv-export-for-finance",
  });
  const ran = h["start_work_run"]!({ entityId: "mock:PAY-236" }) as { outcome: string }[];
  expect(ran.map((step) => step.outcome)).toEqual(["queued", "queued", "succeeded", "queued"]);
});

// -- Builds (#848) ----------------------------------------------------------

test("builds: 59 pipelines by name, each with at most twelve runs, and pipeline_runs answers each one", () => {
  const h = demoHandlers() as Calls;
  const list = h["pipelines"]!({}) as { entity_id: string; name: string; recent: { state: { tone: string } }[] }[];
  expect(list).toHaveLength(59);
  const names = list.map((p) => p.name);
  expect([...names].sort((a, b) => a.toLowerCase().localeCompare(b.toLowerCase()))).toEqual(names);
  expect(list.every((p) => p.recent.length > 0 && p.recent.length <= 12)).toBe(true);
  const tests = list.find((p) => p.name === "Payout_IntegrationTests")!;
  expect(tests.recent[0]!.state.tone).toBe("err");
  const runs = h["pipeline_runs"]!({ pipelineId: tests.entity_id, limit: null }) as { number: string }[];
  expect(runs[0]!.number).toBe("1187");
  expect(runs.length).toBeGreaterThan(12);
  expect(() => h["pipeline_runs"]!({ pipelineId: "teamcity:buildType:Nope" })).toThrow();
});

/** Runs `read` with `role` recorded as missed on fixture source `id`, then takes the miss back. */
function withMiss<T>(id: string, role: string, read: () => T): T {
  const source = FIXTURE_SOURCES.find((entry) => entry.id === id) as { role_misses: { role: string }[] };
  source.role_misses = [{ role }];
  try {
    return read();
  } finally {
    source.role_misses = [];
  }
}

test("docs: the tree and the search read the wiki's fillers alone, so a recorded wiki miss empties both (#1296)", () => {
  const h = demoHandlers() as Calls;
  type Tree = { spaces: { source_id: string }[]; last_read: string | null; for_you: unknown[] };
  const tree = h["docs_tree"]!({}) as Tree;
  expect(new Set(tree.spaces.map((space) => space.source_id))).toEqual(new Set(["confluence"]));
  expect(tree.last_read).not.toBeNull();
  const hits = () => (h["docs_search"]!({ query: "payout", space: null }) as { hits: unknown[] }).hits;
  expect(hits().length).toBeGreaterThan(0);
  withMiss("confluence", "wiki", () => {
    expect(h["docs_tree"]!({})).toEqual({ spaces: [], last_read: null, for_you: [] });
    expect(hits()).toEqual([]);
  });
  expect((h["docs_tree"]!({}) as Tree).spaces).toHaveLength(tree.spaces.length);
});

test("monitors: the roster reads monitoring's fillers alone, paused one included, and a recorded miss takes Kuma's off (#1297)", () => {
  const h = demoHandlers() as Calls;
  type Row = { source_id: string; tombstoned: boolean };
  const roster = h["monitor_roster"]!({}) as Row[];
  expect(new Set(roster.map((row) => row.source_id))).toEqual(new Set(["kuma"]));
  expect(roster.some((row) => row.tombstoned)).toBe(true);
  withMiss("kuma", "monitoring", () => expect(h["monitor_roster"]!({})).toEqual([]));
});

test("code: every read Code makes for the code host role answers code host fillers alone, never Jira's or Kuma's (#1298)", () => {
  const h = demoHandlers() as Calls;
  for (const [kinds, order] of [[["repo"], "updated_desc"], [["pr"], "attention"], [["pr"], "updated_desc"], [["branch"], "updated_desc"]] as const) {
    const filter = { sources: [], kinds, updated_within_days: null, order, context: null, project: null, include_deleted: false, reach: null, role: "code-host" };
    const rows = (h["list_entities"]!({ filter, limit: 1000, offset: 0 }) as { rows: { source_id: string; kind: string }[] }).rows;
    expect(rows.every((row) => ["mock", "gitea"].includes(row.source_id) && kinds.includes(row.kind as never))).toBe(true);
    if (kinds[0] !== "branch") expect(rows.length).toBeGreaterThan(0);
  }
});

test("builds: pipelines reads CI's fillers alone, so a recorded CI miss takes the source's pipelines off (#1295)", () => {
  const h = demoHandlers() as Calls;
  const sources = () => [...new Set((h["pipelines"]!({}) as { source_id: string }[]).map((p) => p.source_id))];
  expect(sources()).toEqual(["teamcity"]);
  const teamcity = FIXTURE_SOURCES.find((source) => source.id === "teamcity") as { role_misses: { role: string }[] };
  teamcity.role_misses = [{ role: "ci" }];
  try {
    expect(h["pipelines"]!({})).toEqual([]);
  } finally {
    teamcity.role_misses = [];
  }
  expect(h["pipelines"]!({})).toHaveLength(59);
});

test("builds: every pipeline carries its project path, prototype-6's three are pinned, and a pin is kept (#849)", () => {
  const h = demoHandlers() as Calls;
  type Row = { entity_id: string; name: string; path: string[]; pinned: boolean };
  const list = () => h["pipelines"]!({}) as Row[];
  const deploy = list().find((p) => p.name === "Payout_Deploy_Prod")!;
  expect(deploy.path).toEqual(["Payments", "Payouts", "Deploy"]);
  expect(list().filter((p) => p.pinned).map((p) => p.name)).toEqual(["Payout_Build", "Payout_Deploy_Staging", "Payout_IntegrationTests"]);
  expect(h["set_pipeline_pinned"]!({ pipelineId: deploy.entity_id, pinned: true })).toBe(true);
  expect(list().find((p) => p.name === "Payout_Deploy_Prod")!.pinned).toBe(true);
  expect(h["set_pipeline_pinned"]!({ pipelineId: deploy.entity_id, pinned: false })).toBe(false);
});

test("builds: job_pipelines answers PAY-231's pipelines with their reasons, and none for a ticket with none (#850)", () => {
  const h = demoHandlers() as Calls;
  const rows = h["job_pipelines"]!({ entityId: "jira:PAY-231" }) as { pipeline_id: string; reasons: { kind: string }[] }[];
  const tests = rows.find((r) => r.pipeline_id === "teamcity:buildType:Payout_IntegrationTests")!;
  expect(tests.reasons.map((r) => r.kind)).toEqual(["branch", "pr_check"]);
  const psp = rows.find((r) => r.pipeline_id === "teamcity:buildType:PspClient_Build")!;
  expect(psp.reasons.map((r) => r.kind)).toEqual(["pr_check"]);
  expect(rows.some((r) => r.pipeline_id === "teamcity:buildType:Dbt_Test")).toBe(false);
  expect(h["job_pipelines"]!({ entityId: "jira:NOPE-1" })).toEqual([]);
});

test("code: code_facts answers a repository's team and language, and an author and whether it is mara's (#966)", () => {
  const h = demoHandlers() as Calls;
  const facts = h["code_facts"]!({
    entityIds: [
      "mock:tidewater/payout-service",
      "mock:ledger#139",
      "mock:fx-rates#88",
      "mock:tidewater/payout-service@refs/heads/feature/csv-export",
      "mock:nope",
    ],
  }) as { entity_id: string; author: string | null; mine: boolean; language: string | null; teams: string[] }[];
  const of = (id: string) => facts.find((f) => f.entity_id === id);
  expect(of("mock:tidewater/payout-service")).toMatchObject({ language: "Rust", teams: ["Payments"] });
  expect(of("mock:ledger#139")).toMatchObject({ author: "mara", mine: true });
  expect(of("mock:fx-rates#88")).toMatchObject({ author: "priya", mine: false });
  expect(of("mock:tidewater/payout-service@refs/heads/feature/csv-export")).toMatchObject({ author: "jonas", mine: false });
  expect(of("mock:nope")).toBeUndefined();
});

test("builds: a chained pipeline names its chain, and run_chain gives each step its run on the branch (#857)", () => {
  const h = demoHandlers() as Calls;
  const docker = "teamcity:buildType:Payout_Docker";
  const settings = h["pipeline_settings"]!({ pipelineId: docker }) as { chain: { name: string }[] };
  expect(settings.chain.map((s) => s.name)).toEqual(["Payout_Build", "Payout_Docker", "Payout_Deploy_Staging", "Payout_Deploy_Prod"]);
  const lone = h["pipeline_settings"]!({ pipelineId: "teamcity:buildType:Payout_IntegrationTests" }) as { chain: unknown[] };
  expect(lone.chain).toEqual([]);
  const run = (h["pipeline_runs"]!({ pipelineId: docker, limit: null }) as { entity_id: string; branch: string }[])[0]!;
  const chain = h["run_chain"]!({ runId: run.entity_id }) as { name: string; run: { entity_id: string; branch: string } | null }[];
  expect(chain.map((s) => s.name)).toEqual(["Payout_Build", "Payout_Docker", "Payout_Deploy_Staging", "Payout_Deploy_Prod"]);
  expect(chain[1]!.run!.entity_id).toBe(run.entity_id);
  for (const step of chain) if (step.run !== null) expect(step.run.branch).toBe(run.branch);
  expect(h["run_chain"]!({ runId: "teamcity:build:Nope-1" })).toEqual([]);
});

test("builds: older_pipeline_runs pages back from any run until the history ends; pipeline_settings answers every pipeline (#851)", () => {
  const h = demoHandlers() as Calls;
  const id = "teamcity:buildType:Payout_IntegrationTests";
  const mirrored = h["pipeline_runs"]!({ pipelineId: id, limit: null }) as { entity_id: string; number: string }[];
  const first = h["older_pipeline_runs"]!({ pipelineId: id, before: mirrored.at(-1)!.entity_id, limit: 50 }) as { entity_id: string; number: string }[];
  expect(first).toHaveLength(50);
  expect(Number(first[0]!.number)).toBeLessThan(Number(mirrored.at(-1)!.number));
  const next = h["older_pipeline_runs"]!({ pipelineId: id, before: first.at(-1)!.entity_id, limit: 50 }) as { entity_id: string; number: string }[];
  expect(Number(next[0]!.number)).toBeLessThan(Number(first.at(-1)!.number));
  let page = next as { entity_id: string }[];
  for (let turn = 0; turn < 10 && page.length > 0; turn += 1) {
    page = h["older_pipeline_runs"]!({ pipelineId: id, before: page.at(-1)!.entity_id, limit: 50 }) as { entity_id: string }[];
  }
  expect(page).toEqual([]);
  const settings = h["pipeline_settings"]!({ pipelineId: id }) as { repositories: { entity_id: string }[]; parameters: { name: string }[] };
  expect(settings.repositories[0]!.entity_id).toBe("mock:tidewater/payout-service");
  expect(settings.parameters.map((p) => p.name)).toEqual(["env", "test.filter", "log.level"]);
  expect(() => h["pipeline_settings"]!({ pipelineId: "teamcity:buildType:Nope" })).toThrow();
});

// -- link sets (#866) ---------------------------------------------------------

test("link sets: the job's reach carries a set's members under its name, a save changes it and a hide takes one off", () => {
  const h = demoHandlers() as Calls;
  const sets = h["link_sets"]!({}) as { id: string; name: string; tickets: number }[];
  expect(sets.map((set) => set.name)).toContain("Payments staging");
  const reach = () => h["job_reach"]!({ entityId: "mock:PAY-240" }) as { entity_id: string; origin: { kind: string; name?: string } }[];
  const staged = reach().find((row) => row.entity_id === "asset:knobas-jira");
  expect(staged?.origin).toEqual({ kind: "link_set", set: "staging", name: "Payments staging" });
  const detail = h["link_set"]!({ id: "staging" }) as { members: { kind: string; entity_id: string }[]; rules: unknown[] };
  h["save_link_set"]!({ draft: { id: "staging", name: "Payments staging", description: "", members: detail.members.slice(1), rules: detail.rules } });
  expect(reach().some((row) => row.entity_id === "asset:knobas-jira")).toBe(false);
  h["hide_link_set_member"]!({ ticketId: "mock:PAY-240", memberId: "kuma:3", hidden: true });
  expect(reach().some((row) => row.entity_id === "kuma:3")).toBe(false);
  expect(reach().some((row) => row.entity_id === "asset:knobas-jira-db")).toBe(true);
});

test("link sets: a monitor row draws its type and its dot, a repository its team (#1020)", () => {
  const h = demoHandlers() as Calls;
  type Row = { entity_id: string; line: string | null; state: { text: string; tone: string } | null };
  const rows = (h["link_set"]!({ id: "payout-stack" }) as { members: Row[] }).members;
  const of = (id: string) => rows.find((row) => row.entity_id === id);
  const roster = h["monitor_roster"]!({}) as { entity_id: string; monitor_type: string }[];
  const kind = roster.find((row) => row.entity_id === "kuma:5")?.monitor_type;
  expect(kind).toBeTruthy();
  expect(of("kuma:5")).toMatchObject({ line: kind, state: { text: "up", tone: "ok" } });
  expect(of("mock:tidewater/payout-service")).toMatchObject({ line: "Payments", state: null });
});

test("builds: a fixture run's log answers the steps bar its steps' durations, the running one without (#1003)", () => {
  const h = demoHandlers() as Calls;
  const steps = (runId: string) => (h["run_log"]!({ runId, from: null, count: 1 }) as { steps: { name: string; seconds: number | null }[] }).steps;
  expect(steps("teamcity:build:Payout_IntegrationTests-1187").map((s) => s.seconds)).toEqual([12, 20, 130, 82, 8]);
  expect(steps("teamcity:build:Payout_Build-1188").map((s) => [s.name, s.seconds])).toEqual([
    ["Checkout", 14],
    ["cargo build --release", 98],
    ["cargo test", null],
  ]);
});

test("builds: pipeline_settings names Payout_Build's five steps, of which 1188's log heads the first three (#1016)", () => {
  const h = demoHandlers() as Calls;
  const named = (config: string) => (h["pipeline_settings"]!({ pipelineId: `teamcity:buildType:${config}` }) as { steps: string[] }).steps;
  const headed = (runId: string) => (h["run_log"]!({ runId, from: null, count: 1 }) as { steps: { name: string }[] }).steps.map((s) => s.name);
  expect(named("Payout_Build")).toEqual(["Checkout", "cargo build --release", "cargo test", "Build image", "Publish artifacts"]);
  expect(named("Payout_Build").slice(0, 3)).toEqual(headed("teamcity:build:Payout_Build-1188"));
  expect(named("Payout_IntegrationTests")).toEqual(headed("teamcity:build:Payout_IntegrationTests-1187"));
});

test("builds: a fixture run answers its log, tests, a test's history, artifacts, a preview and downloads (#854, #855, #856)", () => {
  const h = demoHandlers() as Calls;
  const runId = "teamcity:build:Payout_IntegrationTests-1187";
  const log = h["run_log"]!({ runId, from: null, count: null }) as { lines: { step: number | null; tone: string | null }[]; steps: { name: string; tone: string | null }[]; total: number };
  expect(log.steps.map((s) => s.name)).toEqual(["Checkout", "Start PSP sandbox stub", "cargo build --tests", "cargo test --test sepa_retry", "Publish test report"]);
  expect(log.steps[3]!.tone).toBe("err");
  expect(log.lines).toHaveLength(log.total);
  expect((h["run_log"]!({ runId, from: null, count: 5 }) as { lines: unknown[] }).lines).toHaveLength(5);
  const tests = h["run_tests"]!({ runId }) as { tests: { id: string; name: string; status: string }[] };
  expect(tests.tests).toHaveLength(42);
  expect(tests.tests[0]).toMatchObject({ name: "sepa::retry::gives_up_after_max_attempts", status: "failed" });
  const history = h["test_history"]!({ runId, testId: tests.tests[0]!.id }) as { status: string }[];
  expect(history.filter((o) => o.status === "failed")).toHaveLength(2);
  const root = h["run_artifacts"]!({ runId, path: null }) as { path: string; folder: boolean }[];
  expect(root.map((e) => e.path)).toEqual(["test-report", "logs", "coverage"]);
  expect((h["run_artifacts"]!({ runId, path: "logs" }) as { path: string }[]).map((e) => e.path)).toEqual(["logs/psp-sandbox.log", "logs/integration.log"]);
  expect((h["artifact_preview"]!({ runId, path: "test-report/junit.xml" }) as { text: string }).text).toContain("<testsuites");
  expect(h["download_run_file"]!({ runId, target: { kind: "folder", path: "logs" } })).toBe("/Users/mara/Downloads/logs.zip");
  expect(() => h["run_tests"]!({ runId: "teamcity:build:Nope-1" })).toThrow();
});

test("link sets on the job (#867): each set that applies with its first matching rule, hides marked, the parent's own links", () => {
  const h = demoHandlers() as Calls;
  type Read = {
    sets: { id: string; via: { kind: string; value: string }; members: { entity_id: string; hidden: boolean }[] }[];
    parent: { ticket: string; links: { entity_id: string }[] } | null;
  };
  const read = (entityId: string) => h["job_link_sets"]!({ entityId }) as Read;
  const on231 = read("mock:PAY-231");
  expect(on231.sets.map((set) => [set.id, set.via.kind, set.via.value])).toContainEqual(["staging", "manual", "mock:PAY-231"]);
  expect(on231.parent).toBeNull();
  h["hide_link_set_member"]!({ ticketId: "mock:PAY-231", memberId: "kuma:3", hidden: true });
  const staging = read("mock:PAY-231").sets.find((set) => set.id === "staging")!;
  expect(staging.members.find((m) => m.entity_id === "kuma:3")?.hidden).toBe(true);
  h["hide_link_set_member"]!({ ticketId: "mock:PAY-231", memberId: "kuma:3", hidden: false });
  const sub = read("mock:PAY-232");
  expect(sub.parent?.ticket).toBe("mock:PAY-231");
  expect(sub.parent?.links.length).toBeGreaterThan(0);
  expect(read("mock:NOPE-1")).toEqual({ sets: [], parent: null });
});

test("boot: ?fake-first-run holds app_status on the wizard until complete_first_run, and without it there is no wizard (#1396)", () => {
  type Status = { first_run: boolean; source_count: number };
  const h = demoHandlers(new URLSearchParams("fake-first-run")) as Calls;
  expect(h["app_status"]!({}) as Status).toMatchObject({ first_run: true, source_count: 0 });
  expect(h["app_status"]!({}) as Status).toMatchObject({ first_run: true, source_count: 0 });
  h["complete_first_run"]!({});
  expect(h["app_status"]!({}) as Status).toMatchObject({ first_run: false, source_count: 1 });
  expect((demoHandlers() as Calls)["app_status"]!({}) as Status).toMatchObject({ first_run: false, source_count: 1 });
});

test("boot: ?fake-secret-store=memory answers credentials for this session only, and without it the keychain (#1585)", () => {
  type Status = { secrets_in_memory: boolean };
  expect((demoHandlers(new URLSearchParams("fake-secret-store=memory")) as Calls)["app_status"]!({}) as Status).toMatchObject({
    secrets_in_memory: true,
  });
  expect((demoHandlers() as Calls)["app_status"]!({}) as Status).toMatchObject({ secrets_in_memory: false });
});

// -- cross-references between modules ---------------------------------------

/** Every run the Builds fixture answers, across its pipelines. */
function fixtureRunIds(h: Calls): Set<string> {
  const pipelines = h["pipelines"]!({}) as { entity_id: string }[];
  return new Set(pipelines.flatMap((p) => (h["pipeline_runs"]!({ pipelineId: p.entity_id }) as { entity_id: string }[]).map((r) => r.entity_id)));
}

test("every inbox row opens an item the fixture holds: its page through docs_page, its build among the pipelines' runs", () => {
  const h = demoHandlers() as Calls;
  const items = (["stream", "snoozed"] as const).flatMap(
    (shelf) => (h["inbox_items"]!({ shelf }) as { item: { entity_id: string | null; kind: string | null } }[]).map((entry) => entry.item),
  );
  const pages = items.filter((item) => item.kind === "page");
  const builds = items.filter((item) => item.kind === "build");
  expect(pages.length).toBeGreaterThan(0);
  expect(builds.length).toBeGreaterThan(0);
  for (const page of pages) expect(() => h["docs_page"]!({ entityId: page.entity_id }), `${page.entity_id} opens nothing`).not.toThrow();
  const runs = fixtureRunIds(h);
  for (const build of builds) expect(runs, `${build.entity_id} is no run of the Builds fixture`).toContain(build.entity_id);
});

test("everything PAY-231's job reaches opens in the fixture: its pages through docs_page, its builds among the pipelines' runs", () => {
  const h = demoHandlers() as Calls;
  const reach = JSON.stringify(h["job_reach"]!({ entityId: "mock:PAY-231" }));
  const rows = [...reach.matchAll(/"entity_id":"([^"]+)","kind":"(page|build)"/g)].map((m) => ({ id: m[1]!, kind: m[2]! }));
  expect(rows.some((row) => row.kind === "page")).toBe(true);
  expect(rows.some((row) => row.kind === "build")).toBe(true);
  const runs = fixtureRunIds(h);
  for (const row of rows) {
    if (row.kind === "page") expect(() => h["docs_page"]!({ entityId: row.id }), `${row.id} opens nothing`).not.toThrow();
    else expect(runs, `${row.id} is no run of the Builds fixture`).toContain(row.id);
  }
});

test("job PAY-231's branch and commit declare payout-service as their parent, so their addresses land under it (#1440)", () => {
  const h = demoHandlers() as Calls;
  const land = (kind: "branch" | "commit", entityId: string) => {
    const { row } = h["get_entity"]!({ entityId }) as EntityDetail;
    const filter = { sources: [row.source_id], kinds: ["repo"], updated_within_days: null, order: "updated_desc", context: null, project: null, include_deleted: false, reach: null };
    const repos = (h["list_entities"]!({ filter, limit: 1000, offset: 0 }) as EntityPage).rows;
    const repo = repoOfRow(row, repos);
    return repo === null ? null : underRepoHash(kind, row.entity_id, repo.entity_id);
  };
  expect(land("branch", "mock:tidewater/payout-service@refs/heads/feat/PAY-231-idempotent-retry")).toBe(
    "#/repo/mock%3Atidewater%2Fpayout-service/branches",
  );
  expect(land("commit", "mock:9f2c1ab")).toBe("#/repo/mock%3Atidewater%2Fpayout-service/commits/mock%3A9f2c1ab");
});

test("PR #88 of fx-rates is read by a token without read:issue: Conversation opens on Gitea's line, the review under it, every write refused (#2065)", () => {
  const h = demoHandlers() as Calls;
  const view = h["entity_view"]!({ entityId: "mock:fx-rates#88", section: null }) as SourceView;
  const conversation = view.sections.find((section) => section.id === "conversation")!;
  expect(conversation.blocks[0]).toEqual({
    type: "entities",
    title: null,
    rows: [
      {
        entity_id: null,
        title: "This token cannot read this pull request's comments: it lacks the read:issue scope",
        meta: null,
        tone: "muted",
        chip: null,
        url: null,
      },
    ],
  });
  expect(conversation.blocks[1]).toMatchObject({ type: "facts", facts: [{ label: "jonas" }, { label: "Review", value: "approved", tone: "ok" }] });
  expect(conversation.count).toBe(1);
  // Nothing such a token may write: Gitea draws each write disabled, saying why (#2043).
  expect(view.actions.length).toBeGreaterThan(0);
  for (const action of view.actions) expect(action.disabled, action.label).toMatch(/^This token cannot write: it lacks the write:(issue|repository) scope$/);
  // The other pull requests' conversations hold no such line.
  const other = h["entity_view"]!({ entityId: "mock:booking-api#121", section: "conversation" }) as SourceView;
  expect(other.sections[0]!.blocks).toEqual([]);
});

test("refresh_entity reads a refreshing adapter's item back as refreshed, and refuses an id the mirror does not hold (#2107)", () => {
  const h = demoHandlers() as Calls;
  const detailOf = (entityId: string) => h["get_entity"]!({ entityId }) as EntityDetail;
  expect(detailOf("jira:PAY-231").refreshes).toBe(true);
  expect(h["refresh_entity"]!({ entityId: "jira:PAY-231" })).toEqual({ outcome: "refreshed" });
  expect(() => h["refresh_entity"]!({ entityId: "jira:NOPE-1" })).toThrow();
});

test("a fixture build and its configuration answer get_entity; TeamCity refreshes the build, not the configuration (#2137)", () => {
  const h = demoHandlers() as Calls;
  const run = h["get_entity"]!({ entityId: "teamcity:build:Payout_IntegrationTests-1186" }) as EntityDetail;
  expect(run.row.kind).toBe("build");
  expect(run.source.id).toBe("teamcity");
  expect(run.refreshes).toBe(true);
  expect(h["refresh_entity"]!({ entityId: run.row.entity_id })).toEqual({ outcome: "refreshed" });
  const config = h["get_entity"]!({ entityId: "teamcity:buildType:Payout_IntegrationTests" }) as EntityDetail;
  expect(config.row.kind).toBe("build_config");
  expect(h["refresh_entity"]!({ entityId: config.row.entity_id })).toEqual({ outcome: "not_offered" });
  expect(() => h["get_entity"]!({ entityId: "teamcity:buildType:Nope" })).toThrow();
});
