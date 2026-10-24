/**
 * The job (#816): `#/job/<key>` resolved, promoted once, the yellow card and
 * the Code section, mounted with stand-ins for every read and write. What the
 * backend does with each call is `crates/knobas-app/tests/it/job_ipc.rs`'s.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import type { AssetDetail } from "../../ipc/assets";
import type {
  CheckoutSettings,
  OpenAt,
  CheckoutView,
  ContextRow,
  EntityDetail,
  EntityFilter,
  EntityRow,
  Reached,
  StartWorkStep,
  StateChip,
} from "../../ipc/entity";
import type { CredentialHealth, SourceSyncStatus, SourceDescriptor, WriteOpPayload } from "../../ipc/sources";
import type { RunningTimer, Week } from "../../ipc/time";
import { health } from "../../shell/health.svelte";
import type { Hear, WriteFollower } from "../../shell/follow-write";
import { toasts } from "../../shell/toasts.svelte";
import { page as codeRowPage } from "../code-row/page";
import { writeQueuePanel } from "../../shell/write-queue.svelte";
import { forgetQueued } from "../queued.svelte";
import { forgetFilters, sessionFilter } from "../filter/filter-state.svelte";
import { createLayers, type Layers } from "../layers.svelte";
import { createApps } from "../openin/apps.svelte";
import { pageFor } from "../pages";
import { parseTfHash } from "../route";
import { aroundLines } from "../tickets/tickets";
import type { JobPorts } from "./ports";
import { createKindRegistry } from "../../shell/kind-registry.svelte";
import type { FrameKinds } from "./job";
import { reachTally } from "./sections";
import TfJob from "./TfJob.svelte";
import { sourceFacts } from "./source-facts.fixture";

let target: HTMLElement;
let job: ReturnType<typeof mount> | null = null;

const calls = {
  read: [] as string[],
  promote: [] as string[],
  list: [] as EntityFilter[],
  writes: [] as WriteOpPayload[],
  checkout: [] as string[],
  open: [] as [string, OpenAt][],
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

/** What a page queued is held for the session (#1424), and each test is one. */
afterEach(() => {
  forgetQueued();
  health.replace([]);
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

const APPS: CheckoutSettings = {
  clones_root: "~/code",
  git: { status: "found", path: "/usr/bin/git" },
  default_application: "vscode",
  applications: [
    { id: "jetbrains", name: "IntelliJ IDEA", command: "idea {path}", file_command: null },
    { id: "vscode", name: "VS Code", command: "code {path}", file_command: null },
  ],
};

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

function ports(
  over: Partial<JobPorts> & { rows?: EntityRow[]; others?: EntityRow[]; reachable?: string[] | "fails" } = {},
): JobPorts {
  const rows = over.rows ?? [PR_OPEN, BRANCH, REPO];
  const all = [...rows, ...(over.others ?? [])];
  return {
    listSources: () => Promise.resolve([sourceFacts("gitea"), sourceFacts("jira")]),
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
      // The mirror answers a role (#1295): asked for CI with no kinds, it
      // reads the run kinds CI's fillers declare, which here are these two.
      const roleKinds = filter.role === "ci" ? ["build", "workflow_run"] : null;
      const kept = all.filter(
        (row) => (filter.kinds.length === 0 || filter.kinds.includes(row.kind)) && (roleKinds === null || roleKinds.includes(row.kind)),
      );
      return Promise.resolve({ rows: kept, total: kept.length, tally: null, groups: null });
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
    jobReach: () => Promise.resolve([]),
    jobLinkSets: () => Promise.resolve({ sets: [], parent: null }),
    hideLinkSetMember: (_ticket, _member, hidden) => Promise.resolve(hidden),
    getAsset: (id) => Promise.reject({ code: "not_found", message: `no asset ${id}`, source_id: null }),
    credentialHealth: () => Promise.resolve([]),
    syncStatus: () => Promise.resolve([]),
    startWork: {
      flow: () => Promise.resolve([]),
      run: () => Promise.resolve([]),
      retry: () => Promise.resolve([]),
      skip: () => Promise.resolve([]),
      amend: () => Promise.resolve([]),
    },
    ...over,
  };
}

function open(
  segment = "PAY-231",
  over: {
    ports?: JobPorts;
    timer?: { current: RunningTimer | null; elapsed: string | null };
    layers?: Layers;
    tab?: string | null;
    registry?: FrameKinds;
    follower?: WriteFollower;
  } = {},
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
      ...(over.registry ? { registry: over.registry } : {}),
      ...(over.follower ? { follower: over.follower } : {}),
      apps: createApps({
        checkoutSettings: () => Promise.resolve(APPS),
        setCheckoutApplications: () => Promise.reject(new Error("not in this suite")),
        openCheckout: (id, at) => {
          calls.open.push([id, at]);
          return Promise.resolve(["code", "~/code/payout-service"]);
        },
      }),
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

  // A move re-reads the ticket (the reader's tests hold what else it reads) and promotes nothing again.
  calls.read.length = 0;
  const select = target.querySelector<HTMLSelectElement>("select[aria-label='Status']")!;
  select.value = "In Review";
  select.dispatchEvent(new Event("change", { bubbles: true }));
  await settle();
  expect(calls.read).toEqual([ID]);
  expect(calls.promote).toEqual([ID]);
});

test("every section reads through what the ticket reaches, each for its own kinds", async () => {
  open();
  await settle();
  // The ticket's context is promoted first, and the reach starts from it.
  expect(calls.promote).toEqual([ID]);
  expect(calls.list.every((filter) => filter.reach === ID && filter.context === null)).toBe(true);
  // Each asks for its role (#1295, #1296, #1297, #1298); Builds names no
  // kinds, and the mirror reads the role's.
  const asked = calls.list.map((filter) => `${filter.role}:${[...filter.kinds].sort().join(",")}`);
  expect(asked.sort()).toEqual(["ci:", "code-host:branch,commit,pr,repo", "monitoring:monitor", "wiki:page"]);
  expect(calls.list.find((filter) => filter.order === "latest_per_parent")).toMatchObject({ role: "ci", kinds: [] });
});

/** Ports where Gitea holds PAY-231 too (#1476), listed before Jira. */
function twoHolders(): JobPorts {
  const gitea = ticket({
    row: { ...ticket().row, entity_id: "gitea:PAY-231", source_id: "gitea" },
    source: { id: "gitea", display_name: "Tidewater Gitea", adapter_kind: "gitea", enabled: true },
  });
  return ports({
    getEntity: (id) => {
      calls.read.push(id);
      if (id === ID) return Promise.resolve(ticket());
      if (id === "gitea:PAY-231") return Promise.resolve(gitea);
      return Promise.reject({ code: "not_found", message: `${id} is not in the mirror`, source_id: null });
    },
  });
}

const sourceLine = () => target.querySelector("[aria-label='Job card'] .tf-jobsrc");

test("a key two sources hold: the card names its source and links the other's, and the source's address opens it (#1476)", async () => {
  open("PAY-231", { ports: twoHolders() });
  await settle();
  expect(calls.promote).toEqual(["gitea:PAY-231"]);
  expect(sourceLine()?.textContent?.replace(/\s+/g, " ").trim()).toBe("In Tidewater Gitea · also in Jira");
  expect(sourceLine()?.querySelector("a")?.getAttribute("href")).toBe("#/job/jira%3APAY-231");

  unmount(job!);
  calls.promote.length = 0;
  open("jira:PAY-231", { ports: twoHolders() });
  await settle();
  expect(calls.promote).toEqual([ID]);
  expect(target.querySelector(".tf-jobkey")?.textContent).toBe("PAY-231");
  expect(sourceLine()?.textContent?.replace(/\s+/g, " ").trim()).toBe("In Jira · also in Tidewater Gitea");
});

test("a key one source holds names no source on the card", async () => {
  open();
  await settle();
  expect(target.querySelector(".tf-jobkey")?.textContent).toBe("PAY-231");
  expect(sourceLine()).toBeNull();
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
  expect(toasts.items.at(-1)?.text).toBe("Queued: PAY-231 to In Review.");
  expect(toasts.items.at(-1)?.action?.label).toBe("Show queue");
});

test("a move while the ticket's source fails its sign-in says it waits there", async () => {
  health.patch({ source_id: "jira", state: "unauthorized", checked_at: "2026-10-21T09:00:00Z", detail: null, secret_expires_at: null });
  open();
  await settle();
  const select = target.querySelector<HTMLSelectElement>("select[aria-label='Status']")!;
  select.value = "In Review";
  select.dispatchEvent(new Event("change", { bubbles: true }));
  await settle();
  expect(toasts.items.at(-1)?.text).toBe("Queued: PAY-231 to In Review. Jira sign-in is failing, so it waits there.");
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
  // The branch's address is the repository page's redirect to its Branches tab (#1440).
  expect(pageFor(parseTfHash(rows[2]!.getAttribute("href")!))).toBe(codeRowPage);
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

test("the checkout line names the clone, its branch and changes, and Open in… runs the default application", async () => {
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
  // The default first, marked, with its command filled in for this checkout.
  expect(items.map((item) => item.textContent?.replace(/\s+/g, " ").trim())).toEqual([
    "VS Code (default)code ~/code/payout-service",
    "IntelliJ IDEAidea ~/code/payout-service",
    "Add an application…",
    "Manage applications…",
  ]);
  expect(layers.open).toHaveLength(1);
  items[0]!.click();
  await settle();
  expect(calls.open).toEqual([[BRANCH.entity_id, { application: "vscode", file: null, line: null }]]);
  expect(layers.open).toHaveLength(0);
  expect(toasts.items.map((toast) => toast.text)).toContain("Started VS Code: code ~/code/payout-service");
});

test("a job with no code says nothing is connected and asks for no checkout", async () => {
  open("PAY-231", { ports: ports({ rows: [] }) });
  await settle();
  expect(text()).toContain("Nothing connected.");
  expect(calls.checkout).toEqual([]);
  expect(target.querySelector(".tf-checkout")).toBeNull();
});

test("the Map tab hides the card and draws Dependencies in place of the sections (#874)", async () => {
  open("PAY-231", { tab: "map" });
  await settle();
  const tabs = [...target.querySelectorAll("[role='tab']")];
  expect(tabs.map((tab) => [tab.textContent?.trim(), tab.getAttribute("aria-selected")])).toEqual([
    ["Work", "false"],
    ["Map", "true"],
    ["Conversation", "false"],
  ]);
  expect(target.querySelector(".tf-ws")!.classList.contains("tf-mapmode")).toBe(true);
  expect(target.querySelector(".tf-wbody .tf-dp.tf-dp-job")).not.toBeNull();
  expect(target.querySelector(".tf-setsbar")).toBeNull();
  expect(target.querySelector("[aria-labelledby='tf-sec-code']")).toBeNull();
});

// ---- #817: the other four sections ----

const PARENT = "jira:PAY-200";

function build(id: string, title: string, tone: StateChip["tone"], text: string, source = "teamcity"): EntityRow {
  return { entity_id: id, kind: "build", source_id: source, title, updated_at: "2026-10-15T10:00:00Z", synced_at: "", path: "Payments › Payouts", state: { text, tone } };
}
const B_FAIL = build("teamcity:it#1187", "Payout_IntegrationTests #1187", "err", "failed");
const B_RUN = build("teamcity:build#1188", "Payout_Build #1188", "info", "running");
const B_OK = build("teamcity:lint#904", "Payout_Lint #904", "ok", "success");
const PAGE = { ...code("confluence:ENG-1", "page", "SEPA payout retry design", "2026-10-15T10:40:00Z", { text: "mentions you", tone: "warn" } as StateChip, "ENG › Payments"), source_id: "confluence" };
const MONITOR = { ...code("kuma:flows-stage", "monitor", "flows-stage", "2026-10-15T12:40:00Z", { text: "down", tone: "err" } as StateChip, null), source_id: "kuma" };
const MON_OK = { ...code("kuma:payouts-stage", "monitor", "payouts-stage /health", "2026-10-15T12:00:00Z", { text: "up", tone: "ok" } as StateChip, null), source_id: "kuma" };

function own(entity_id: string, kind: string): Reached {
  return { entity_id, kind, step: entity_id, origin: { kind: "own" } };
}

function section(id: string): Element {
  return target.querySelector(`[aria-labelledby='tf-sec-${id}']`)!;
}

/** The addresses a section's rows open, in drawn order. */
function hrefsOf(id: string): string[] {
  return [...section(id).querySelectorAll(".tf-rows a")].map((row) => row.getAttribute("href") ?? "");
}

/** A rollup's words, one part per drawn span (the gap between them is CSS). */
function roll(id: string): string {
  const parts = [...section(id).querySelectorAll(".tf-roll > span")];
  return parts.map((part) => part.textContent?.replace(/\s+/g, " ").trim()).join(" ");
}

test("Builds rolls up configurations by state, sorts the failing first, and labels a row from the parent", async () => {
  open("PAY-231", {
    ports: ports({
      others: [B_OK, B_RUN, B_FAIL],
      jobReach: () =>
        Promise.resolve([
          own(B_FAIL.entity_id, "build"),
          own(B_RUN.entity_id, "build"),
          { entity_id: B_OK.entity_id, kind: "build", step: PARENT, origin: { kind: "parent", ticket: PARENT } },
        ]),
    }),
  });
  await settle();
  expect(roll("builds")).toBe("3 configurations: 1 failing 1 running 1 passing");
  const rows = [...section("builds").querySelectorAll(".tf-rows a")];
  expect(rows.map((row) => row.getAttribute("href"))).toEqual([
    `#/build/${encodeURIComponent(B_FAIL.entity_id)}`,
    `#/build/${encodeURIComponent(B_RUN.entity_id)}`,
    `#/build/${encodeURIComponent(B_OK.entity_id)}`,
  ]);
  expect(rows[2]!.querySelector(".tf-via")?.textContent).toBe("Parent PAY-200");
  expect(rows[0]!.querySelector(".tf-via")).toBeNull();
  expect(section("builds").querySelector("a.tf-open")?.textContent?.trim()).toBe("Open Builds view");
});

test("Builds warns when a source that holds builds cannot sign in, with the fix", async () => {
  const failing: CredentialHealth = { source_id: "teamcity", state: "unauthorized", checked_at: null, detail: null, secret_expires_at: null };
  const run = {
    source_id: "teamcity",
    running: false,
    run_id: 9,
    started_at: null,
    last_finished_at: new Date(2026, 9, 15, 13, 22).toISOString(),
    last_outcome: "unauthorized",
    next_run_at: null,
    backoff_until: null,
    last_changed: false,
  } as SourceSyncStatus;
  open("PAY-231", {
    ports: ports({
      others: [B_FAIL],
      listSources: () => Promise.resolve([sourceFacts("teamcity", { display_name: "TeamCity" }), sourceFacts("jira")]),
      credentialHealth: () => Promise.resolve([failing]),
      syncStatus: () => Promise.resolve([run]),
    }),
  });
  await settle();
  const stale = section("builds").querySelector(".tf-stale")!;
  expect(stale.textContent?.replace(/\s+/g, " ").trim()).toBe("TeamCity data is 38 min old: sign-in failing. Re-enter password");
  expect(stale.querySelector("a")?.getAttribute("href")).toBe("#/settings/source/teamcity");
});

test("a source that signs in draws no warning", async () => {
  open("PAY-231", {
    ports: ports({
      others: [B_OK],
      listSources: () => Promise.resolve([sourceFacts("teamcity", { display_name: "TeamCity" }), sourceFacts("jira")]),
      credentialHealth: () => Promise.resolve([{ source_id: "teamcity", state: "ok", checked_at: null, detail: null, secret_expires_at: null }]),
    }),
  });
  await settle();
  expect(section("builds").querySelector(".tf-stale")).toBeNull();
});

test("Assets reads each reached asset and rolls up environments and problems", async () => {
  const detail = (id: string, name: string, type_label: string, health: "up" | "down", env: "prod" | "stage", on: string | null) =>
    ({
      asset: { id, name, type_label, health } as AssetDetail["asset"],
      effective_environment: { value: env, source_id: id, source_name: name },
      held_by: on ? [{ name: on } as AssetDetail["asset"]] : [],
    }) as AssetDetail;
  const assets: Record<string, AssetDetail> = {
    "asset:c": detail("asset:c", "payout-service", "Container", "down", "stage", "vm-pay-stage-01"),
    "asset:vm": detail("asset:vm", "vm-pay-stage-01", "VM", "up", "stage", null),
  };
  open("PAY-231", {
    ports: ports({
      jobReach: () => Promise.resolve([own("asset:vm", "asset"), own("asset:c", "asset"), own(PR_OPEN.entity_id, "pr")]),
      getAsset: (id) => Promise.resolve(assets[id]!),
    }),
  });
  await settle();
  expect(roll("assets")).toBe("2 assets in stage 1 with a problem");
  const rows = [...section("assets").querySelectorAll(".tf-rows a")];
  expect(rows.map((row) => row.getAttribute("href"))).toEqual([`#/asset/${encodeURIComponent("asset:c")}`, `#/asset/${encodeURIComponent("asset:vm")}`]);
  expect(rows[0]!.textContent).toContain("Container, stage, on vm-pay-stage-01");
  expect(section("assets").querySelector("a.tf-open")?.getAttribute("href")).toBe("#/job/PAY-231/map");
});

test("Monitors and Pages roll up in prototype-6's words", async () => {
  open("PAY-231", { ports: ports({ others: [MON_OK, MONITOR, PAGE] }) });
  await settle();
  expect(roll("monitors")).toBe("2 watched: 1 down 1 fine");
  expect(section("monitors").querySelector(".tf-rows a")?.getAttribute("href")).toBe("#/monitors");
  expect(roll("pages")).toBe("1 page in ENG 1 ask something of you");
  expect(section("pages").querySelector("a.tf-open")?.textContent?.trim()).toBe("Open Docs view");
});

// #1030: the Tickets preview's *Around this ticket* counts from the same reads.
test("Around this ticket counts the rows the job's sections draw, a parent's build among them", async () => {
  const healthy = (id: string) =>
    ({ asset: { id, name: id, type_label: "VM", health: "up" }, effective_environment: null, held_by: [] }) as unknown as AssetDetail;
  const p = ports({
    others: [B_OK, B_RUN, B_FAIL, MON_OK, MONITOR, PAGE],
    jobReach: () =>
      Promise.resolve([
        own(PR_OPEN.entity_id, "pr"),
        own(B_FAIL.entity_id, "build"),
        { entity_id: B_OK.entity_id, kind: "build", step: PARENT, origin: { kind: "parent", ticket: PARENT } },
        own("asset:vm", "asset"),
        own("asset:db", "asset"),
      ]),
    getAsset: (id) => Promise.resolve(healthy(id)),
  });
  open("PAY-231", { ports: p });
  await settle();
  const drawn = (id: string) => section(id).querySelectorAll(".tf-rows a").length;
  expect(["code", "builds", "pages", "monitors", "assets"].map(drawn)).toEqual([3, 3, 1, 2, 2]);
  expect(calls.promote).toEqual([ID]);

  expect(aroundLines(await reachTally(p, ID))).toEqual([
    { section: "Code", count: "3 items", failing: null },
    { section: "Builds", count: "3 builds", failing: "1 failing or down" },
    { section: "Docs", count: "1 page", failing: null },
    { section: "Monitors", count: "2 monitors", failing: "1 failing or down" },
    { section: "Assets", count: "2 assets", failing: null },
  ]);
  // Counting promotes nothing: only the job's own open did.
  expect(calls.promote).toEqual([ID]);
});

test("Open Docs view opens Docs with This job on (R-2.5); the other sections' links leave it", async () => {
  open("PAY-231", { ports: ports({ others: [PAGE] }) });
  await settle();
  expect(sessionFilter("docs").scope).toBeNull();
  section("monitors").querySelector<HTMLAnchorElement>("a.tf-open")!.click();
  expect(sessionFilter("docs").scope).toBeNull();
  sessionFilter("docs").scope = "all";
  section("pages").querySelector<HTMLAnchorElement>("a.tf-open")!.click();
  expect(sessionFilter("docs").scope).toBe("job");
  forgetFilters();
});

test("Open Code view opens Repositories with This job on (R-2.5), and the pick after that is kept (R-2.6)", async () => {
  open("PAY-231", { ports: ports({ rows: [code("gitea:x@1", "commit", "fix", "2026-10-15T10:00:00Z", null, null)] }) });
  await settle();
  expect(sessionFilter("code-repos").scope).toBeNull();
  section("monitors").querySelector<HTMLAnchorElement>("a.tf-open")!.click();
  expect(sessionFilter("code-repos").scope).toBeNull();
  sessionFilter("code-repos").scope = "all";
  section("code").querySelector<HTMLAnchorElement>("a.tf-open")!.click();
  expect(sessionFilter("code-repos").scope).toBe("job");
  sessionFilter("code-repos").scope = "mine";
  expect(sessionFilter("code-repos").scope).toBe("mine");
  forgetFilters();
});

test("Open Monitors opens Monitors with This job on (R-2.5), and the pick after that is kept (R-2.6)", async () => {
  open("PAY-231");
  await settle();
  expect(sessionFilter("monitors").scope).toBeNull();
  section("pages").querySelector<HTMLAnchorElement>("a.tf-open")!.click();
  expect(sessionFilter("monitors").scope).toBeNull();
  sessionFilter("monitors").scope = "all";
  section("monitors").querySelector<HTMLAnchorElement>("a.tf-open")!.click();
  expect(sessionFilter("monitors").scope).toBe("job");
  sessionFilter("monitors").scope = "mine";
  expect(sessionFilter("monitors").scope).toBe("mine");
  forgetFilters();
});

test("a section with nothing in it says so", async () => {
  open();
  await settle();
  expect(roll("monitors")).toBe("Nothing connected.");
  expect(section("monitors").textContent).toContain("Nothing connected yet. Use the connections drawer to link something.");
});

// ---- #818: the blocked banner and Start work ----

test("a blocked ticket shows the red banner naming the blocker, with a button to open it", async () => {
  const blocked = ticket({
    links: [
      {
        link: { id: "l1", from_id: "jira:OPS-77", to_id: ID, relation: "blocks", origin: "source", note: "Staging needs the updated runbook.", created_by: "sync", created_at: "" } as never,
        other: { entity_id: "jira:OPS-77", kind: "ticket", title: "On-call runbook outdated", deleted_at: null },
      },
    ],
  });
  open("PAY-231", { ports: ports({ getEntity: (id) => (id === ID ? Promise.resolve(blocked) : Promise.reject({ code: "not_found" })) }) });
  await settle();
  const banner = target.querySelector(".tf-blocked")!;
  expect(banner.querySelector("strong")?.textContent).toBe("Blocked by OPS-77 On-call runbook outdated");
  expect(banner.textContent).toContain("Staging needs the updated runbook.");
  const button = banner.querySelector("a")!;
  expect(button.textContent).toBe("Open OPS-77");
  expect(button.getAttribute("href")).toBe("#/job/jira%3AOPS-77");
});

test("the banner opens the blocker of its own source when two sources hold the key (#1676)", async () => {
  const blocked = ticket({
    links: [
      {
        link: { id: "l1", from_id: "gitea:OPS-77", to_id: ID, relation: "blocks", origin: "source", note: null, created_by: "sync", created_at: "" } as never,
        other: { entity_id: "gitea:OPS-77", kind: "ticket", title: "Runbook", deleted_at: null },
      },
    ],
  });
  open("PAY-231", { ports: ports({ getEntity: (id) => (id === ID ? Promise.resolve(blocked) : Promise.reject({ code: "not_found" })) }) });
  await settle();
  expect(target.querySelector(".tf-blocked a")?.getAttribute("href")).toBe("#/job/gitea%3AOPS-77");
});

test("an unblocked ticket draws no banner", async () => {
  open();
  await settle();
  expect(target.querySelector(".tf-blocked")).toBeNull();
});

/** A flow as the backend answers it, each step in `outcomes`' state. */
function flow(outcomes: StartWorkStep["outcome"][], repo = REPO.entity_id): StartWorkStep[] {
  const kinds: StartWorkStep["step"][] = ["create_branch", "create_pull_request", "link_pull_request", "transition"];
  const payloads: unknown[] = [
    { CreateBranch: { entity: repo, name: "feature/PAY-231-retry-failed-sepa-payouts", from_ref: "main" } },
    { CreatePullRequest: { entity: repo, title: "WIP: Retry failed SEPA payouts", body: "", head: "feature/PAY-231", base: "main" } },
    { relation: "implements" },
    { Transition: { entity: ID, status: "In Progress" } },
  ];
  return kinds.map((step, index) => ({
    id: index + 1,
    ticket_id: ID,
    step,
    position: index,
    outcome: outcomes[index]!,
    payload: payloads[index],
    write_id: null,
    detail: null,
    updated_at: "",
  }));
}

const TODO_MINE = () => ticket({ status: "To Do", author: "mara" });
const MARA = () => Promise.resolve([sourceFacts("jira", { config: { username: "mara" } }), sourceFacts("gitea")]);

function steps(): [string | undefined, string | undefined][] {
  return [...target.querySelectorAll(".tf-step")].map((row) => [row.querySelector("strong")?.textContent ?? undefined, row.querySelector(".tf-state")?.textContent ?? undefined]);
}

test("a To Do ticket of mine shows Start work, proposed in the suggested repository, in place of Code and Builds", async () => {
  const asked: (string | null)[] = [];
  let stored: StartWorkStep[] = [];
  const OTHER = code("gitea:tidewater/ledger", "repo", "tidewater/ledger", "2026-10-01T08:00:00Z", null, null);
  open("PAY-231", {
    ports: ports({
      rows: [REPO, OTHER],
      listSources: MARA,
      getEntity: (id) => (id === ID ? Promise.resolve(TODO_MINE()) : Promise.reject({ code: "not_found" })),
      jobReach: () => Promise.resolve([own(REPO.entity_id, "repo")]),
      startWork: {
        flow: (_id, repo) => {
          asked.push(repo);
          if (repo !== null && stored.length === 0) stored = flow(["pending", "pending", "pending", "pending"], repo);
          return Promise.resolve(stored);
        },
        run: () => Promise.resolve(stored),
        retry: () => Promise.resolve(stored),
        skip: () => Promise.resolve(stored),
        amend: () => Promise.resolve(stored),
      },
    }),
  });
  await settle();
  expect(asked).toEqual([null, REPO.entity_id]);
  // The picker reads every repository, and asks for the code host role (#1350).
  const picker = calls.list.filter((filter) => filter.reach === null && filter.kinds.join() === "repo");
  expect(picker.map((filter) => filter.role)).toEqual(["code-host"]);
  expect(steps()).toEqual([
    ["Create branch", "ready"],
    ["Open pull request", "ready"],
    ["Link PR to ticket", "ready"],
    ["Move ticket", "ready"],
    ["Start timer", "ready"],
  ]);
  const select = target.querySelector<HTMLSelectElement>(".tf-step select[aria-label='Repository']")!;
  expect([...select.options].map((option) => option.textContent)).toEqual(["tidewater/payout-service (suggested)", "tidewater/ledger"]);
  expect(target.querySelector<HTMLInputElement>("input[aria-label='Create branch']")?.value).toBe("feature/PAY-231-retry-failed-sepa-payouts");
  // A step nothing has sent yet is the reader's to edit (#1707).
  expect(target.querySelector<HTMLInputElement>("input[aria-label='Create branch']")?.disabled).toBe(false);
  expect(select.disabled).toBe(false);
  expect(target.querySelector<HTMLInputElement>("input[aria-label='Move ticket']")?.value).toBe("To Do to In Progress");
  expect(section("code")).toBeNull();
  expect(section("builds")).toBeNull();
  expect(section("assets")).not.toBeNull();
});

test("each step's state follows the flow as its writes move through the queue, and the step it stopped at can be retried or skipped", async () => {
  let answer = flow(["pending", "pending", "pending", "pending"]);
  const acted: string[] = [];
  open("PAY-231", {
    ports: ports({
      listSources: MARA,
      getEntity: (id) => (id === ID ? Promise.resolve(TODO_MINE()) : Promise.reject({ code: "not_found" })),
      startWork: {
        flow: () => Promise.resolve(answer),
        run: () => {
          acted.push("run");
          answer = flow(["succeeded", "queued", "pending", "pending"]);
          return Promise.resolve(answer);
        },
        retry: (id) => {
          acted.push(`retry ${id}`);
          return Promise.resolve(answer);
        },
        skip: (id) => {
          acted.push(`skip ${id}`);
          answer = flow(["succeeded", "skipped", "succeeded", "succeeded"]);
          return Promise.resolve(answer);
        },
        amend: () => Promise.resolve(answer),
      },
    }),
  });
  await settle();
  [...target.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent === "Run all steps")!.click();
  await settle();
  expect(acted).toEqual(["run"]);
  expect(steps().slice(0, 4).map(([, state]) => state)).toEqual(["done", "queued", "ready", "ready"]);
  const queued = target.querySelector(".tf-step[data-step='pr']")!;
  // A queued step's write is still open: it is amended in the write queue,
  // not here (#1707). A step that landed is not edited either.
  expect(queued.querySelector<HTMLInputElement>("input")?.disabled).toBe(true);
  expect(target.querySelector<HTMLInputElement>(".tf-step[data-step='branch'] input")?.disabled).toBe(true);
  [...queued.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent === "Retry")!.click();
  await settle();
  expect(acted).toEqual(["run", "retry 2"]);
  [...queued.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent === "Skip")!.click();
  await settle();
  expect(acted).toEqual(["run", "retry 2", "skip 2"]);
  expect(steps().slice(0, 4).map(([, state]) => state)).toEqual(["done", "skipped", "done", "done"]);
  // A step that already landed offers neither.
  const done = target.querySelector(".tf-step[data-step='branch']")!;
  expect([...done.querySelectorAll("button")].map((b) => b.textContent)).toEqual([]);
});

test("Run all steps starts the timer as step 5, through the shell's start, which opens a running timer's draft first", async () => {
  let ran = false;
  const order: string[] = [];
  const current = { target: { kind: "entity", entity_id: "jira:PAY-228" }, started_at: new Date().toISOString() } as RunningTimer;
  calls.start.length = 0;
  job = mount(TfJob, {
    target,
    props: {
      segment: "PAY-231",
      tab: null,
      timer: { current, elapsed: "1:00" },
      layers: createLayers(),
      ontimer: () => {},
      onstarttimer: (id: string) => order.push(`start ${id} after run: ${ran}`),
      ports: ports({
        listSources: MARA,
        getEntity: (id) => (id === ID ? Promise.resolve(TODO_MINE()) : Promise.reject({ code: "not_found" })),
        startWork: {
          flow: () => Promise.resolve(flow(["pending", "pending", "pending", "pending"])),
          run: () => {
            ran = true;
            return Promise.resolve(flow(["succeeded", "succeeded", "succeeded", "succeeded"]));
          },
          retry: () => Promise.resolve([]),
          skip: () => Promise.resolve([]),
          amend: () => Promise.resolve([]),
        },
      }),
      today: () => new Date(2026, 9, 15, 14, 0),
    },
  });
  flushSync();
  await settle();
  expect(target.querySelector(".tf-step[data-step='timer']")?.textContent).toContain("stops the timer on PAY-228 first");
  [...target.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent === "Run all steps")!.click();
  await settle();
  expect(order).toEqual([`start ${ID} after run: true`]);
});

/** Run all steps on PAY-231 with a flow that ends as `after`; the toasts it raised. */
async function runWith(after: StartWorkStep["outcome"][]) {
  open("PAY-231", {
    ports: ports({
      listSources: MARA,
      getEntity: (id) => (id === ID ? Promise.resolve(TODO_MINE()) : Promise.reject({ code: "not_found" })),
      startWork: {
        flow: () => Promise.resolve(flow(["pending", "pending", "pending", "pending"])),
        run: () => Promise.resolve(flow(after)),
        retry: () => Promise.resolve([]),
        skip: () => Promise.resolve([]),
        amend: () => Promise.resolve([]),
      },
    }),
  });
  await settle();
  toasts.items.length = 0;
  [...target.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent === "Run all steps")!.click();
  await settle();
  return toasts.items;
}

test("Run all steps says how many writes it queued, with Show queue (#1426)", async () => {
  const items = await runWith(["queued", "queued", "queued", "pending"]);
  const started = items.find((toast) => toast.text.startsWith("Started work on"))!;
  expect(started.text).toBe("Started work on PAY-231. 3 writes queued.");
  expect(started.action?.label).toBe("Show queue");
  started.action!.run();
  expect(writeQueuePanel.open).toBe(true);
});

test("Run all steps with one queued step says '1 write queued.', with none no count (#1426)", async () => {
  expect((await runWith(["succeeded", "queued", "pending", "pending"])).map((toast) => toast.text)).toContain(
    "Started work on PAY-231. 1 write queued.",
  );
  unmount(job!);
  job = null;
  const items = await runWith(["succeeded", "succeeded", "skipped", "succeeded"]);
  const started = items.find((toast) => toast.text.startsWith("Started work on"))!;
  expect(started.text).toBe("Started work on PAY-231.");
  expect(started.action).toBeUndefined();
});

test("the stepper draws no Step by step and promises no per-step run (#1426)", async () => {
  const items = await runWith(["queued", "pending", "pending", "pending"]);
  expect([...target.querySelectorAll("button")].map((b) => b.textContent)).not.toContain("Step by step");
  expect(target.querySelector(".tf-steps")!.textContent).toContain("A failed step can be retried, skipped or amended.");
  expect(items.map((toast) => toast.text).join(" ")).not.toContain("Each step can also run on its own");
});

test("a To Do ticket of somebody else's, or mine in progress, shows no Start work", async () => {
  open("PAY-231", {
    ports: ports({ listSources: MARA, getEntity: (id) => (id === ID ? Promise.resolve(ticket({ status: "To Do", author: "jonas" })) : Promise.reject({ code: "not_found" })) }),
  });
  await settle();
  expect(target.querySelector(".tf-steps")).toBeNull();
  unmount(job!);
  job = null;
  open("PAY-231", {
    ports: ports({ listSources: MARA, getEntity: (id) => (id === ID ? Promise.resolve(ticket({ status: "In Progress", author: "mara" })) : Promise.reject({ code: "not_found" })) }),
  });
  await settle();
  expect(target.querySelector(".tf-steps")).toBeNull();
  expect(section("code")).not.toBeNull();
});

// ---- #1230: the sections pick by role, not by kind ----

/** The adapters' claims: TeamCity's runs are `build`, another CI's are `workflow_run`. */
function claimsRegistry() {
  const descriptor = (adapter_kind: string, role: string, run: string | null) =>
    ({
      adapter_kind,
      roles: [{ role, reads: [] }],
      payload_paths: run === null ? [] : [{ kind: run, frame: "run" }, { kind: `${run}_pipeline`, frame: "pipeline" }],
    }) as unknown as SourceDescriptor;
  return createKindRegistry({
    listAdapters: () =>
      Promise.resolve([descriptor("teamcity", "ci", "build"), descriptor("forge", "ci", "workflow_run"), descriptor("gitea", "code-host", null)]),
  });
}

const RUN = { ...build("forge:run#77", "ci.yml #77", "err", "failure", "forge"), kind: "workflow_run" };
const MOCK_REPO = { ...code("mock:demo-repo", "repo", "demo-repo", "2026-10-15T13:00:00Z", null, null), source_id: "mock" };
const MOCK_BUILD = build("mock:build#1", "Mock_Build #1", "err", "failed", "mock");

test("a job's Builds section lists the runs of a CI source whose run kind is not build", async () => {
  const p = ports({ others: [B_OK, RUN], jobReach: () => Promise.resolve([own(B_OK.entity_id, "build"), own(RUN.entity_id, "workflow_run")]) });
  const registry = claimsRegistry();
  open("PAY-231", { ports: p, registry });
  await settle();
  // The section asks for the role and names no kinds; the mirror answers both run kinds (#1295).
  const runs = calls.list.find((filter) => filter.order === "latest_per_parent")!;
  expect(runs).toMatchObject({ role: "ci", kinds: [] });
  expect(hrefsOf("builds")).toHaveLength(2);
  // A run of another kind opens Builds' run page, not the block view (#1256).
  const hrefs = [...section("builds").querySelectorAll(".tf-rows a")].map((row) => decodeURIComponent(row.getAttribute("href") ?? ""));
  expect(hrefs.filter((href) => href === `#/build/${RUN.entity_id}` || href === `#/build/${B_OK.entity_id}`)).toHaveLength(2);

  const tally = await reachTally(p, ID);
  expect(tally.find((line) => line.kind === "workflow_run")).toEqual({ kind: "workflow_run", tone: "err", count: 1, section: "builds" });
  // Counted under Builds because the Builds read answered it, not because the shell knows the run kind.
  expect(aroundLines(tally).find((line) => line.section === "Builds")?.count).toBe("2 items");
});

test("a job's sections draw their rows as read and keep none by source: each read asked for its role (#1295-#1298)", async () => {
  const p = ports({ others: [MOCK_REPO, B_OK, MOCK_BUILD] });
  open("PAY-231", { ports: p, registry: claimsRegistry() });
  await settle();
  expect(hrefsOf("code").filter((href) => href.includes("mock"))).toHaveLength(1);
  expect(hrefsOf("code")).toHaveLength(4);
  expect(hrefsOf("builds").sort()).toEqual([`#/build/${encodeURIComponent(B_OK.entity_id)}`, `#/build/${encodeURIComponent(MOCK_BUILD.entity_id)}`].sort());

  const tally = await reachTally(p, ID);
  const count = (kind: string) => tally.filter((line) => line.kind === kind).reduce((n, line) => n + line.count, 0);
  // The counts are the sections' rows, as read.
  expect([count("repo"), count("build")]).toEqual([2, 2]);
  expect(calls.list.every((filter) => typeof filter.role === "string")).toBe(true);
});

test("with no CI claim known, Builds still asks for the CI role and names no kinds (#1295)", async () => {
  const registry = createKindRegistry({ listAdapters: () => Promise.resolve([]) });
  const p = ports({ others: [B_OK] });
  open("PAY-231", { ports: p, registry });
  await settle();
  expect(calls.list.find((filter) => filter.order === "latest_per_parent")).toMatchObject({ role: "ci", kinds: [] });
  expect(hrefsOf("builds")).toEqual([`#/build/${encodeURIComponent(B_OK.entity_id)}`]);

  calls.list.length = 0;
  await reachTally(p, ID);
  const tallied = calls.list.find((filter) => filter.order === "latest_per_parent");
  expect(tallied).toMatchObject({ role: "ci", kinds: [], reach: ID });
});

// -- A queued move is drawn at once (#1424) ----------------------------------

/** The status the select shows, whether it is drawn as queued, and the mark beside it. */
function statusShown() {
  const select = target.querySelector<HTMLSelectElement>("select[aria-label='Status']")!;
  const mark = target.querySelector(".tf-jobrow .tf-queued-mark");
  return [select.value, select.classList.contains("tf-queued"), mark?.textContent?.trim() ?? null];
}

async function pick(status: string) {
  const select = target.querySelector<HTMLSelectElement>("select[aria-label='Status']")!;
  select.value = status;
  select.dispatchEvent(new Event("change", { bubbles: true }));
  await settle();
}

test("a picked status is drawn muted with queued until the detail's status moves; then the mark goes", async () => {
  let status = "In Progress";
  const moving = () =>
    ports({
      getEntity: (id) => {
        calls.read.push(id);
        return id === ID ? Promise.resolve(ticket({ status })) : Promise.reject({ code: "not_found" });
      },
    });
  open("PAY-231", { ports: moving() });
  await settle();
  expect(statusShown()).toEqual(["In Progress", false, null]);
  await pick("In Review");
  // The move re-read the ticket, which the sync has not moved yet.
  expect(calls.read.filter((id) => id === ID).length).toBeGreaterThan(1);
  expect(statusShown()).toEqual(["In Review", true, "queued"]);

  unmount(job!);
  open("PAY-231", { ports: moving() });
  await settle();
  expect(statusShown()).toEqual(["In Review", true, "queued"]);

  // The status moved, here past the picked one (somebody closed it): the mark goes.
  unmount(job!);
  status = "Done";
  open("PAY-231", { ports: moving() });
  await settle();
  expect(statusShown()).toEqual(["Done", false, null]);

  // Moved back in Jira by somebody else: the old move is not drawn again.
  unmount(job!);
  status = "In Progress";
  open("PAY-231", { ports: moving() });
  await settle();
  expect(statusShown()).toEqual(["In Progress", false, null]);
});

/** A follower the test speaks for, and ports whose writes answer ids from 50 up. */
function following() {
  const hears = new Map<number, Hear>();
  const follower: WriteFollower = {
    follow(id, hear) {
      hears.set(id, hear);
      return () => hears.delete(id);
    },
  };
  let next = 50;
  const numbered = ports({
    submitWrite: (payload) => {
      calls.writes.push(payload);
      return Promise.resolve({ id: next++ } as never);
    },
  });
  return { follower, numbered, hear: (id: number, row: unknown) => hears.get(id)?.(row as never) };
}

test("a move the queue refuses drops the queued status and toasts; Edit and resend opens the status pick again", async () => {
  const { follower, numbered, hear } = following();
  open("PAY-231", { ports: numbered, follower });
  await settle();
  await pick("In Review");
  expect(statusShown()).toEqual(["In Review", true, "queued"]);

  hear(50, { id: 50, state: "refused", detail: "The transition needs a resolution" });
  flushSync();
  expect(statusShown()).toEqual(["In Progress", false, null]);
  const toast = toasts.items.at(-1)!;
  expect(toast.text).toBe("Move of PAY-231 to In Review was refused: The transition needs a resolution");
  const select = target.querySelector<HTMLSelectElement>("select[aria-label='Status']")!;
  expect(document.activeElement).not.toBe(select);
  toast.more!.find((action) => action.label === "Edit and resend")!.run();
  await settle();
  expect(location.hash).toBe("#/job/PAY-231");
  expect(document.activeElement).toBe(select);
  expect(select.value).toBe("In Progress");
});
