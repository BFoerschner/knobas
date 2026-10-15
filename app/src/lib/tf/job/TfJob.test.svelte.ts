/**
 * The job (#816): `#/job/<key>` resolved, promoted once, the yellow card and
 * the Code section, mounted with stand-ins for every read and write. What the
 * backend does with each call is `crates/knobas-app/tests/it/job_ipc.rs`'s.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import type {
  CheckoutCommand,
  CheckoutView,
  ContextRow,
  EntityDetail,
  EntityFilter,
  EntityRow,
  StateChip,
} from "../../ipc/entity";
import type { WriteOpPayload } from "../../ipc/sources";
import type { RunningTimer, Week } from "../../ipc/time";
import { toasts } from "../../shell/toasts.svelte";
import { createLayers, type Layers } from "../layers.svelte";
import type { JobPorts } from "./ports";
import TfJob from "./TfJob.svelte";

let target: HTMLElement;
let job: ReturnType<typeof mount> | null = null;

const calls = {
  read: [] as string[],
  promote: [] as string[],
  list: [] as EntityFilter[],
  writes: [] as WriteOpPayload[],
  checkout: [] as string[],
  open: [] as [string, string][],
  stop: 0,
  start: [] as [string, string][],
  visits: [] as string[],
  unlinks: [] as string[],
  links: [] as [string, string, string | undefined][],
};

beforeEach(() => {
  for (const list of [calls.read, calls.promote, calls.list, calls.writes, calls.checkout, calls.open, calls.start, calls.visits, calls.unlinks, calls.links]) {
    list.length = 0;
  }
  calls.stop = 0;
  toasts.items.length = 0;
  target = document.createElement("div");
  document.body.appendChild(target);
});

afterEach(() => {
  if (job) unmount(job);
  job = null;
  target.remove();
});

async function settle() {
  for (let turn = 0; turn < 8; turn += 1) await new Promise((resolve) => setTimeout(resolve, 0));
  flushSync();
}

const ID = "jira:PAY-231";

function ticket(over: Partial<EntityDetail> = {}): EntityDetail {
  return {
    row: {
      entity_id: ID,
      kind: "ticket",
      source_id: "jira",
      title: "Retry failed SEPA payouts",
      updated_at: "2026-10-15T09:00:00Z",
      synced_at: "2026-10-15T09:00:00Z",
      path: null,
      state: null,
    },
    source: { id: "jira", display_name: "Jira", adapter_kind: "jira", enabled: true },
    kind_info: null,
    body_text: "Payouts to two SEPA banks fail with a 409 on retry.",
    author: null,
    payload: {},
    web_url: null,
    deleted_at: null,
    links: [],
    activity: [],
    builds: [],
    status: "In Progress",
    issue_type: "Story",
    priority: "High",
    estimate_seconds: 3 * 3600,
    ...over,
  };
}

function code(
  id: string,
  kind: string,
  title: string,
  updated: string,
  state: StateChip | null = null,
  path: string | null = "tidewater/payout-service",
): EntityRow {
  return { entity_id: id, kind, source_id: "gitea", title, updated_at: updated, synced_at: updated, path, state };
}

const PR_OPEN = code("gitea:tidewater/payout-service#142", "pr", "Idempotent retry window", "2026-10-15T10:00:00Z", { text: "open", tone: "info" });
const BRANCH = code("gitea:tidewater/payout-service@refs/heads/feature/PAY-231", "branch", "feature/PAY-231-sepa-retry", "2026-10-15T11:00:00Z");
const REPO = code("gitea:tidewater/payout-service", "repo", "tidewater/payout-service", "2026-10-14T08:00:00Z", null, null);
const PR_MERGED = code("gitea:tidewater/psp-client#9", "pr", "Classify TEMP_UNAVAILABLE", "2026-10-15T12:00:00Z", { text: "merged", tone: "ok" }, "tidewater/psp-client");
const COMMIT_OLD = code("gitea:tidewater/payout-service@c90d11", "commit", "jitter in backoff", "2026-10-10T08:00:00Z");
const FAILING = code("gitea:tidewater/payout-service#140", "pr", "Old attempt", "2026-10-01T08:00:00Z", { text: "conflict", tone: "err" });

const CHECKOUT: CheckoutView = {
  repo_entity_id: REPO.entity_id,
  repo_url: null,
  path: "~/code/payout-service",
  found_by: "scan",
  clones_root: "~/code",
  clone_command: null,
  tree: {
    status: "read",
    branch: "feature/PAY-231-sepa-retry",
    head: "c90d11",
    upstream: null,
    ahead: 0,
    behind: 0,
    changed: 1,
    untracked: 0,
    conflicted: 0,
  },
};

const APPS: CheckoutCommand[] = [
  { action: "vscode", label: "Open in VS Code", template: "open -a Code {path}", is_default: true },
  { action: "jetbrains", label: "Open in JetBrains", template: null, is_default: true },
];

function week(): Week {
  const cell = (logged: number, unlogged: number) => ({
    tracked_seconds: logged + unlogged,
    offered_seconds: 0,
    logged_seconds: logged,
    held_seconds: 0,
    unlogged_seconds: unlogged,
  });
  return {
    days: ["2026-10-12", "2026-10-13", "2026-10-14", "2026-10-15", "2026-10-16", "2026-10-17", "2026-10-18"],
    rows: [
      {
        target: { kind: "entity", entity_id: ID },
        title: "Retry failed SEPA payouts",
        cells: [cell(3 * 3600, 0), cell(4.5 * 3600, 0), cell(0, 0), cell(0, 4 * 3600 + 14 * 60), cell(0, 0), cell(0, 0), cell(0, 0)],
      },
    ],
    past_horizon: [false, false, false, false, false, false, false],
  };
}

function ports(over: Partial<JobPorts> & { rows?: EntityRow[]; reachable?: string[] | "fails" } = {}): JobPorts {
  const rows = over.rows ?? [PR_OPEN, BRANCH, REPO];
  return {
    listSources: () => Promise.resolve([{ id: "gitea" }, { id: "jira" }]),
    getEntity: (id) => {
      calls.read.push(id);
      return id === ID ? Promise.resolve(ticket()) : Promise.reject({ code: "not_found", message: `${id} is not in the mirror`, source_id: null });
    },
    promoteContext: (id) => {
      calls.promote.push(id);
      return Promise.resolve({ id: "ctx:1", kind: "ticket", title: "t", anchor_id: id, created_at: "", archived_at: null } satisfies ContextRow);
    },
    listEntities: (filter) => {
      calls.list.push(filter);
      return Promise.resolve({ rows, total: rows.length, tally: null, groups: null });
    },
    reachableTransitions: () =>
      over.reachable === "fails" ? Promise.reject({ code: "unreachable", message: "down", source_id: "jira" }) : Promise.resolve(over.reachable ?? ["In Review", "To Do"]),
    submitWrite: (payload) => {
      calls.writes.push(payload);
      return Promise.resolve({} as never);
    },
    entityCheckout: (id) => {
      calls.checkout.push(id);
      return Promise.resolve(CHECKOUT);
    },
    checkoutCommands: () => Promise.resolve(APPS),
    openCheckout: (id, action) => {
      calls.open.push([id, action]);
      return Promise.resolve();
    },
    weekTimesheet: () => Promise.resolve(week()),
    visitEntity: (id) => {
      calls.visits.push(id);
      return Promise.resolve(null);
    },
    entityThread: () => Promise.resolve({ comments: [], changes: [], me: ["mara"] }),
    createLink: (from, to, relation) => {
      calls.links.push([from, to, relation]);
      return Promise.resolve({} as never);
    },
    unlink: (id) => {
      calls.unlinks.push(id);
      return Promise.resolve();
    },
    roomSuggestions: () => Promise.resolve({ rows: [], total: 0, more: [] }),
    search: () => Promise.reject(new Error("not in this test")),
    launcherHome: () => Promise.reject(new Error("not in this test")),
    ...over,
  };
}

function open(
  segment = "PAY-231",
  over: { ports?: JobPorts; timer?: { current: RunningTimer | null; elapsed: string | null }; layers?: Layers; tab?: string | null } = {},
) {
  job = mount(TfJob, {
    target,
    props: {
      segment,
      tab: over.tab ?? null,
      timer: over.timer ?? { current: null, elapsed: null },
      layers: over.layers ?? createLayers(),
      ontimer: () => (calls.stop += 1),
      onstarttimer: (id: string, title: string) => calls.start.push([id, title]),
      ports: over.ports ?? ports(),
      today: () => new Date(2026, 9, 15, 14, 0),
    },
  });
  flushSync();
}

function text(): string {
  return target.textContent?.replace(/\s+/g, " ") ?? "";
}

function codeRows(): HTMLAnchorElement[] {
  return [...target.querySelectorAll<HTMLAnchorElement>("[aria-labelledby='tf-sec-code'] .tf-rows a")];
}

test("a key opens its ticket and promotes it once, and the card shows key, summary, type, priority and estimate", async () => {
  open();
  await settle();

  expect(calls.read).toContain(ID);
  expect(calls.promote).toEqual([ID]);
  const card = target.querySelector("[aria-label='Job card']")!;
  expect(card.querySelector(".tf-jobkey")?.textContent).toBe("PAY-231");
  expect(card.querySelector("h1")?.textContent).toBe("Retry failed SEPA payouts");
  expect(card.textContent).toContain("Story, High, est. 3h");
  expect(card.textContent).toContain("Payouts to two SEPA banks fail with a 409 on retry.");
  expect(target.querySelector<HTMLAnchorElement>(".tf-back")?.getAttribute("href")).toBe("#/tickets");

  // A move re-reads nothing and promotes nothing again.
  const select = target.querySelector<HTMLSelectElement>("select[aria-label='Status']")!;
  select.value = "In Review";
  select.dispatchEvent(new Event("change", { bubbles: true }));
  await settle();
  expect(calls.promote).toEqual([ID]);
});

test("the Code section reads the job's context for code kinds only", async () => {
  open();
  await settle();
  expect(calls.list).toHaveLength(1);
  expect(calls.list[0]!.context).toBe("ctx:1");
  expect([...calls.list[0]!.kinds].sort()).toEqual(["branch", "commit", "pr", "repo"]);
});

test("a key nothing answers to says so and offers the way back", async () => {
  open("NOPE-1");
  await settle();
  expect(calls.promote).toEqual([]);
  expect(text()).toContain("No ticket NOPE-1 in the mirror.");
  expect(target.querySelector(".tf-btn")?.getAttribute("href")).toBe("#/tickets");
});

test("a read that fails for another reason than a miss says it failed, not that the ticket is missing", async () => {
  open("PAY-231", {
    ports: ports({ getEntity: () => Promise.reject({ code: "not_ready", message: "the database is starting", source_id: null }) }),
  });
  await settle();
  expect(text()).toContain("Could not open PAY-231: the database is starting");
  expect(text()).not.toContain("in the mirror");
  expect(calls.promote).toEqual([]);
});

test("code rows with no repository name still roll up to a count", async () => {
  open("PAY-231", { ports: ports({ rows: [code("gitea:x@1", "commit", "fix", "2026-10-15T10:00:00Z", null, null)] }) });
  await settle();
  expect(target.querySelector(".tf-roll")?.textContent?.trim()).toBe("1 connected");
});

test("the status select offers where the ticket stands and the reachable transitions only, and a pick queues it", async () => {
  open();
  await settle();
  const select = target.querySelector<HTMLSelectElement>("select[aria-label='Status']")!;
  expect([...select.options].map((option) => option.value)).toEqual(["In Progress", "In Review", "To Do"]);

  select.value = "In Review";
  select.dispatchEvent(new Event("change", { bubbles: true }));
  await settle();
  expect(calls.writes).toEqual([{ Transition: { entity: ID, status: "In Review" } }]);
  expect(select.value).toBe("In Progress");
  expect(toasts.items.at(-1)?.text).toBe("Queued: PAY-231 to In Review. Sends with the next sync.");
  expect(toasts.items.at(-1)?.action?.label).toBe("Show queue");
});

test("a workflow that cannot be read offers nothing to move to", async () => {
  open("PAY-231", { ports: ports({ reachable: "fails" }) });
  await settle();
  const select = target.querySelector<HTMLSelectElement>("select[aria-label='Status']")!;
  expect([...select.options].map((option) => option.value)).toEqual(["In Progress"]);
  expect(select.disabled).toBe(true);
});

test("the clock offers Start timer, and Stop and log while the timer runs on this ticket", async () => {
  open();
  await settle();
  const start = [...target.querySelectorAll("button")].find((button) => button.textContent === "Start timer")!;
  start.click();
  expect(calls.start).toEqual([[ID, "Retry failed SEPA payouts"]]);
  unmount(job!);

  const current: RunningTimer = {
    target: { kind: "entity", entity_id: ID },
    started_at: new Date(2026, 9, 15, 13, 58).toISOString(),
    last_heartbeat: new Date().toISOString(),
  };
  open("PAY-231", { timer: { current, elapsed: "12:04" } });
  await settle();
  expect(text()).toContain("12:04");
  expect(text()).toContain("since 13:58");
  const stop = [...target.querySelectorAll("button")].find((button) => button.textContent === "Stop and log")!;
  stop.click();
  expect(calls.stop).toBe(1);
});

test("a timer on another ticket leaves this card on Start timer", async () => {
  const current = { target: { kind: "entity", entity_id: "jira:PAY-228" }, started_at: new Date().toISOString() } as RunningTimer;
  open("PAY-231", { timer: { current, elapsed: "1:00" } });
  await settle();
  expect([...target.querySelectorAll("button")].some((button) => button.textContent === "Start timer")).toBe(true);
  expect(text()).not.toContain("Stop and log");
});

test("a confirmed blocks link to this ticket draws the blocked chip", async () => {
  const blocked = ticket({
    links: [
      {
        link: { id: "l1", from_id: "jira:OPS-77", to_id: ID, relation: "blocks", origin: "source", note: null, created_by: "sync", created_at: "" } as never,
        other: { entity_id: "jira:OPS-77", kind: "ticket", title: "Runbook", deleted_at: null },
      },
      {
        link: { id: "l2", from_id: ID, to_id: "jira:PAY-240", relation: "blocks", origin: "source", note: null, created_by: "sync", created_at: "" } as never,
        other: { entity_id: "jira:PAY-240", kind: "ticket", title: "Dashboard", deleted_at: null },
      },
    ],
  });
  open("PAY-231", { ports: ports({ getEntity: (id) => (id === ID ? Promise.resolve(blocked) : Promise.reject({ code: "not_found" })) }) });
  await settle();
  const chips = [...target.querySelectorAll(".tf-jobcard .tf-chip")].map((chip) => chip.textContent);
  expect(chips).toEqual(["Blocked by OPS-77"]);
});

test("Time this week says what is logged and what is left to log, Monday to Friday", async () => {
  open();
  await settle();
  const section = [...target.querySelectorAll(".tf-jobcard section")].find((s) => s.textContent?.includes("Time this week"))!;
  expect(section.textContent).toContain("7h 30m logged, 4h 14m to log");
  expect(section.querySelectorAll("svg g")).toHaveLength(5);
  expect(section.querySelectorAll("svg g.tf-today")).toHaveLength(1);
  expect(section.querySelector("a")?.getAttribute("href")).toBe("#/time/week");
});

test("the Code section rolls up repos and open PRs, sorts what asks for you first, and opens full views", async () => {
  open("PAY-231", { ports: ports({ rows: [PR_OPEN, BRANCH, REPO, PR_MERGED] }) });
  await settle();
  const section = target.querySelector("[aria-labelledby='tf-sec-code']")!;
  expect(section.querySelector(".tf-roll")?.textContent?.replace(/\s+/g, " ").trim()).toBe(
    "2 repos: tidewater/payout-service, tidewater/psp-client 1 open PR",
  );
  const rows = codeRows();
  // The open PR asks for you (info), then newest first among the settled.
  expect(rows.map((row) => row.getAttribute("href"))).toEqual([
    `#/pr/${encodeURIComponent(PR_OPEN.entity_id)}`,
    `#/pr/${encodeURIComponent(PR_MERGED.entity_id)}`,
    `#/branch/${encodeURIComponent(BRANCH.entity_id)}`,
    `#/repo/${encodeURIComponent(REPO.entity_id)}`,
  ]);
  expect(section.querySelector("a.tf-open")?.getAttribute("href")).toBe("#/code");
});

test("past four rows the section shows four, with a filter, Show all N and All in Code view", async () => {
  open("PAY-231", { ports: ports({ rows: [COMMIT_OLD, PR_OPEN, BRANCH, REPO, PR_MERGED, FAILING] }) });
  await settle();
  let rows = codeRows();
  expect(rows).toHaveLength(4);
  expect(rows[0]!.getAttribute("href")).toBe(`#/pr/${encodeURIComponent(FAILING.entity_id)}`);

  const showAll = [...target.querySelectorAll("button")].find((button) => button.textContent === "Show all 6")!;
  expect(showAll).toBeTruthy();
  expect([...target.querySelectorAll("a")].some((a) => a.textContent === "All in Code view")).toBe(true);
  showAll.click();
  flushSync();
  expect(codeRows()).toHaveLength(6);

  const filter = target.querySelector<HTMLInputElement>("input[aria-label='Filter Code']")!;
  expect(filter.placeholder).toBe("Filter 6 code");
  expect(filter.hasAttribute("data-filter")).toBe(true);
  filter.value = "psp";
  filter.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
  rows = codeRows();
  expect(rows.map((row) => row.getAttribute("href"))).toEqual([`#/pr/${encodeURIComponent(PR_MERGED.entity_id)}`]);
  expect(text()).toContain("1 match");
});

test("the checkout line names the clone, its branch and changes, and Open in… runs a configured application", async () => {
  const layers = createLayers();
  open("PAY-231", { layers });
  await settle();
  // Branches are asked first: a branch answers with its repository's clone.
  expect(calls.checkout[0]).toBe(BRANCH.entity_id);
  const line = target.querySelector(".tf-checkout")!;
  expect(line.textContent?.replace(/\s+/g, " ")).toContain("Local ~/code/payout-service on feature/PAY-231-sepa-retry");
  expect(line.textContent).toContain("1 changed");

  line.querySelector<HTMLButtonElement>("button[aria-haspopup='menu']")!.click();
  flushSync();
  const items = [...line.querySelectorAll<HTMLButtonElement>("[role='menuitem']")];
  expect(items.map((item) => item.textContent)).toEqual(["Open in VS Code"]);
  expect(layers.open).toHaveLength(1);
  items[0]!.click();
  await settle();
  expect(calls.open).toEqual([[BRANCH.entity_id, "vscode"]]);
  expect(layers.open).toHaveLength(0);
});

test("a job with no code says nothing is connected and asks for no checkout", async () => {
  open("PAY-231", { ports: ports({ rows: [] }) });
  await settle();
  expect(text()).toContain("Nothing connected.");
  expect(calls.checkout).toEqual([]);
  expect(target.querySelector(".tf-checkout")).toBeNull();
});

test("Map is a tab that names itself until it is built", async () => {
  open("PAY-231", { tab: "map" });
  await settle();
  const tabs = [...target.querySelectorAll("[role='tab']")];
  expect(tabs.map((tab) => [tab.textContent?.trim(), tab.getAttribute("aria-selected")])).toEqual([
    ["Work", "false"],
    ["Map", "true"],
    ["Conversation", "false"],
  ]);
  expect(text()).toContain("Map is not built in the new shell yet.");
  expect(target.querySelector("[aria-labelledby='tf-sec-code']")).toBeNull();
});
