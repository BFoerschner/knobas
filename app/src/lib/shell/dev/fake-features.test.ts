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

// -- Open in… (#844) ----------------------------------------------------------

test("the applications are macOS's four, a saved list replaces them, and opening answers the argv", () => {
  const h = demoHandlers() as Calls;
  type Settings = { default_application: string | null; applications: { id: string; name: string }[] };
  const first = h["checkout_settings"]!({}) as Settings;
  expect(first.applications.map((app) => app.name)).toEqual(["VS Code", "Terminal", "Fork", "IntelliJ IDEA"]);
  expect(first.default_application).toBe("vscode");

  const argv = h["open_checkout"]!({ entityId: "mock:tidewater/payout-service", application: null, file: "src/lib.rs", line: 3 });
  expect(argv).toEqual(["open", "-a", "Visual Studio Code", "/Users/mara/src/payout-service/src/lib.rs"]);

  expect(() =>
    h["set_checkout_applications"]!({ applications: [{ id: null, name: "X", command: "x {branch}", file_command: null }], defaultApplication: null }),
  ).toThrow();
  const saved = h["set_checkout_applications"]!({
    applications: [...first.applications, { id: null, name: "My Git Viewer", command: "mgv {path}", file_command: null }],
    defaultApplication: "my-git-viewer",
  }) as Settings;
  expect(saved.applications.at(-1)?.id).toBe("my-git-viewer");
  expect(saved.default_application).toBe("my-git-viewer");
  // Put the shipped list back for the tests after this one: the fixture is module state.
  h["set_checkout_applications"]!({ applications: first.applications, defaultApplication: "vscode" });
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
