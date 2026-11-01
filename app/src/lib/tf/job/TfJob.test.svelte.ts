/**
 * The job (#816): `#/job/<key>` resolved, promoted once, the card and the
 * reading column (#2399), mounted with stand-ins for every read and write. PAY-231
 * is the chosen job in every test, so its page is the pinned one (spec
 * #2396); `ticket-page.test.svelte.ts` drives both moods through the shell. What the
 * backend does with each call is `crates/knobas-app/tests/it/job_ipc.rs`'s.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import type {
  OpenAt,
  ContextRow,
  EntityDetail,
  EntityFilter,
  EntityRow,
  Reached,
  StartWorkStep,
  StateChip,
} from "../../ipc/entity";
import type { EntityRefresh, SourceDescriptor, WriteOpPayload } from "../../ipc/sources";
import { pressRefreshKey, refreshProbe } from "../refresh.fixture";
import type { RunningTimer } from "../../ipc/time";
import { health } from "../../shell/health.svelte";
import type { WriteFollower } from "../../shell/follow-write";
import { toasts } from "../../shell/toasts.svelte";
import { writeQueuePanel } from "../../shell/write-queue.svelte";
import { echoEvents, echoFollower } from "../echo-events.fixture";
import { forgetEchoes } from "../queued.svelte";
import { createLayers, type Layers } from "../layers.svelte";
import { aroundLines } from "../tickets/tickets";
import type { JobPorts } from "./ports";
import { createKindRegistry } from "../../shell/kind-registry.svelte";
import type { FrameKinds } from "./job";
import { reachTally } from "./sections";
import { chosenJob } from "./chosen.svelte";
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
  // The job: PAY-231 is the chosen job, so its page is the pinned one (spec #2396) unless a test says otherwise.
  chosenJob.choose(ID);
  target = document.createElement("div");
  document.body.appendChild(target);
});

/** What a page queued is held for the session (#1424), and each test is one. */
afterEach(() => {
  forgetEchoes();
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
    refreshes: true,
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
    description: "Payouts to two SEPA banks fail with a 409 on retry.",
    markup: "text",
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
    submitWrite: (payload) => {
      calls.writes.push(payload);
      return Promise.resolve({} as never);
    },
    visitEntity: (id) => {
      calls.visits.push(id);
      return Promise.resolve(null);
    },
    entityThread: () => Promise.resolve({ comments: [], me: ["mara"] }),
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
    registry?: FrameKinds;
    follower?: WriteFollower;
  } = {},
) {
  job = mount(TfJob, {
    target,
    props: {
      segment,
      timer: over.timer ?? { current: null, elapsed: null },
      layers: over.layers ?? createLayers(),
      ontimer: () => (calls.stop += 1),
      onstarttimer: (id: string, title: string) => calls.start.push([id, title]),
      ports: over.ports ?? ports(),
      today: () => new Date(2026, 9, 15, 14, 0),
      ...(over.registry ? { registry: over.registry } : {}),
      ...(over.follower ? { follower: over.follower } : {}),
    },
  });
  flushSync();
}

function text(): string {
  return target.textContent?.replace(/\s+/g, " ") ?? "";
}

test("a key opens its ticket and promotes it once; the card shows the key and the facts, the reading column the title and the description", async () => {
  open();
  await settle();

  expect(calls.read).toContain(ID);
  expect(calls.promote).toEqual([ID]);
  const card = target.querySelector("[aria-label='Job card']")!;
  expect(card.querySelector(".tf-jobkey")?.textContent).toBe("PAY-231");
  expect(card.querySelector("h1"), "the title is the reading column's (#2399)").toBeNull();
  expect(target.querySelector(".tf-tkread h1")?.textContent).toBe("Retry failed SEPA payouts");
  const facts = [...card.querySelectorAll("dl.tf-facts > dt")].map((term) => [term.textContent, term.nextElementSibling?.textContent?.replace(/\s+/g, " ").trim()]);
  // Nobody assigned: a dash for the avatar, and the row says so. The estimate is the Logged row's (#2403), not a facts line's.
  expect(facts).toEqual([
    ["Assignee", "–Unassigned"],
    ["Type", "Story"],
    ["Priority", "High"],
  ]);
  expect(card.querySelector("dl.tf-facts .tf-av.tf-me")).toBeNull();
  expect(target.querySelector(".tf-tkread .tf-tkdesc")?.textContent).toContain("Payouts to two SEPA banks fail with a 409 on retry.");
  expect(card.querySelector(".tf-chip")?.textContent, "the status is the card's chip").toBe("In Progress");
  expect(card.querySelector("select"), "no status select on the card (spec #2396)").toBeNull();
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

const sourceLine = () => target.querySelector(".tf-ticketcard .tf-jobsrc");

test("a key two sources hold: the card names its source and links the other's, and the source's address opens it (#1476)", async () => {
  open("PAY-231", { ports: twoHolders() });
  await settle();
  expect(calls.promote).toEqual(["gitea:PAY-231"]);
  expect(sourceLine()?.textContent?.replace(/\s+/g, " ").trim()).toBe("In Tidewater Gitea · also in Jira");
  expect(sourceLine()?.querySelector("a")?.getAttribute("href")).toBe("#/ticket/jira%3APAY-231");

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
  expect(target.querySelector(".tf-btn kbd")).toBeNull();
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

test("the pinned card's foot offers Start timer, and Stop and log while the timer runs on this ticket, with no clock (spec #2396)", async () => {
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
  expect(text(), "the running time is the top bar chip's alone").not.toContain("12:04");
  expect(text()).not.toContain("since 13:58");
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
  const chips = [...target.querySelectorAll(".tf-jobcard .tf-chip.tf-err")].map((chip) => chip.textContent);
  expect(chips).toEqual(["Blocked by OPS-77"]);
});

// ---- #817: the other four sections ----

function build(id: string, title: string, tone: StateChip["tone"], text: string, source = "teamcity"): EntityRow {
  return { entity_id: id, kind: "build", source_id: source, title, updated_at: "2026-10-15T10:00:00Z", synced_at: "", path: "Payments › Payouts", state: { text, tone } };
}
const B_OK = build("teamcity:lint#904", "Payout_Lint #904", "ok", "success");

function own(entity_id: string, kind: string): Reached {
  return { entity_id, kind, step: entity_id, origin: { kind: "own" } };
}

// #1030: the Tickets preview's *Around this ticket* counts from the same reads.
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
  expect(button.getAttribute("href")).toBe("#/ticket/jira%3AOPS-77");
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
  expect(target.querySelector(".tf-blocked a")?.getAttribute("href")).toBe("#/ticket/gitea%3AOPS-77");
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

test("a To Do ticket of mine shows Start work, proposed in the suggested repository, above the description", async () => {
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
  const stepper = target.querySelector(".tf-tkread .tf-steps")!;
  expect(stepper.compareDocumentPosition(target.querySelector(".tf-tkdesc")!) & Node.DOCUMENT_POSITION_FOLLOWING, "the steps sit above the description").toBeTruthy();
});

test("each step's state follows the flow as its writes move through the queue, and the step it stopped at can be checked again or skipped", async () => {
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
  // A queued step's button re-reads its write and sends nothing, so it says
  // so: *Retry* beside it would read as a second send (#1998).
  expect([...queued.querySelectorAll("button")].map((b) => b.textContent)).toEqual(["Check again", "Skip"]);
  [...queued.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent === "Check again")!.click();
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

test("a failed step keeps Retry, which runs it again (#1998)", async () => {
  const acted: string[] = [];
  const answer = flow(["succeeded", "failed", "pending", "pending"]);
  open("PAY-231", {
    ports: ports({
      listSources: MARA,
      getEntity: (id) => (id === ID ? Promise.resolve(TODO_MINE()) : Promise.reject({ code: "not_found" })),
      startWork: {
        flow: () => Promise.resolve(answer),
        run: () => Promise.resolve(answer),
        retry: (id) => {
          acted.push(`retry ${id}`);
          return Promise.resolve(answer);
        },
        skip: () => Promise.resolve(answer),
        amend: () => Promise.resolve(answer),
      },
    }),
  });
  await settle();
  const failed = target.querySelector(".tf-step[data-step='pr']")!;
  expect(failed.querySelector(".tf-state")?.textContent).toBe("failed");
  expect([...failed.querySelectorAll("button")].map((b) => b.textContent)).toEqual(["Retry", "Skip"]);
  [...failed.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent === "Retry")!.click();
  await settle();
  expect(acted).toEqual(["retry 2"]);
});

test("Check again on a queued step whose write was discarded redraws it failed, with Retry and the discard's sentence (#2015)", async () => {
  let answer = flow(["succeeded", "queued", "pending", "pending"]);
  open("PAY-231", {
    ports: ports({
      listSources: MARA,
      getEntity: (id) => (id === ID ? Promise.resolve(TODO_MINE()) : Promise.reject({ code: "not_found" })),
      startWork: {
        flow: () => Promise.resolve(answer),
        run: () => Promise.resolve(answer),
        retry: () => {
          answer = flow(["succeeded", "failed", "pending", "pending"]);
          answer[1]!.detail = "the write was discarded before it went";
          return Promise.resolve(answer);
        },
        skip: () => Promise.resolve(answer),
        amend: () => Promise.resolve(answer),
      },
    }),
  });
  await settle();
  const row = target.querySelector(".tf-step[data-step='pr']")!;
  [...row.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent === "Check again")!.click();
  await settle();
  expect(row.querySelector(".tf-state")?.textContent).toBe("failed");
  expect([...row.querySelectorAll("button")].map((b) => b.textContent)).toEqual(["Retry", "Skip"]);
  expect(row.querySelector(".tf-swdetail")?.textContent).toBe("the write was discarded before it went");
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

test("Start work's move the source's credential cannot make is drawn saying why, and Run all steps skips it (#2052)", async () => {
  const reason = "This Jira user cannot move tickets in PAY: it lacks the Transition Issues permission";
  const ran: string[] = [];
  let now: StartWorkStep["outcome"][] = ["pending", "pending", "pending", "pending"];
  open("PAY-231", {
    ports: ports({
      listSources: MARA,
      getEntity: (id) => (id === ID ? Promise.resolve(TODO_MINE()) : Promise.reject({ code: "not_found" })),
      refusedWrites: () => Promise.resolve([{ entity: ID, op: "transition", reason }]),
      startWork: {
        flow: () => Promise.resolve(flow(now)),
        run: () => {
          ran.push("run");
          return Promise.resolve(flow(["queued", "queued", "queued", "pending"]));
        },
        retry: () => Promise.resolve([]),
        skip: (stepId) => {
          ran.push(`skip ${stepId}`);
          now = ["queued", "queued", "queued", "skipped"];
          return Promise.resolve(flow(now));
        },
        amend: () => Promise.resolve([]),
      },
    }),
  });
  await settle();
  const move = target.querySelector(".tf-step[data-step='status']")!;
  const field = move.querySelector<HTMLInputElement>("input[aria-label='Move ticket']")!;
  expect(field.getAttribute("aria-disabled")).toBe("true");
  expect(field.title).toBe(reason);
  expect(move.textContent).toContain(reason);
  [...target.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent === "Run all steps")!.click();
  await settle();
  // The move is skipped, which runs the flow on past it; it is never run.
  expect(ran).toEqual(["skip 4"]);
  // Skipped, the row still says why: the reader did not choose the skip.
  const skipped = target.querySelector(".tf-step[data-step='status']")!;
  expect(skipped.querySelector(".tf-state")!.textContent).toBe("skipped");
  expect(skipped.textContent).toContain(reason);
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

test("Start work's footer is the canonical one: its note first, the primary last, on the right (#2294)", async () => {
  await runWith(["pending", "pending", "pending", "pending"]);
  const footer = target.querySelector(".tf-steps > footer")!;
  expect([...footer.children].map((each) => [squash(each), each.className])).toEqual([
    ["A failed step can be retried, skipped or amended.", "tf-dim tf-small tf-swhint"],
    ["Run all steps", "tf-btn tf-primary"],
  ]);
});

test("the stepper leaves once the ticket no longer offers Start work: its move landed (#2399)", async () => {
  let status = "To Do";
  const probe = refreshProbe();
  open("PAY-231", {
    ports: ports({
      listSources: MARA,
      getEntity: (id) => (id === ID ? Promise.resolve(ticket({ status, author: "mara" })) : Promise.reject({ code: "not_found" })),
      refresh: probe.ports,
    }),
  });
  await settle();
  expect(target.querySelector(".tf-tkread .tf-steps"), "the chosen To Do ticket of mine runs the stepper").not.toBeNull();

  status = "In Progress";
  target.querySelector<HTMLButtonElement>(".tf-tkpagehead .tf-refresh")!.click();
  await settle();
  expect(target.querySelector(".tf-steps")).toBeNull();
  expect(target.querySelector(".tf-tkdesc"), "the description stays").not.toBeNull();
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

test("the job's Builds read asks for the CI role, and a run of a kind that is not build counts under Builds", async () => {
  const p = ports({ others: [B_OK, RUN], jobReach: () => Promise.resolve([own(B_OK.entity_id, "build"), own(RUN.entity_id, "workflow_run")]) });
  const registry = claimsRegistry();
  open("PAY-231", { ports: p, registry });
  await settle();
  // The section asks for the role and names no kinds; the mirror answers both run kinds (#1295).
  const runs = calls.list.find((filter) => filter.order === "latest_per_parent")!;
  expect(runs).toMatchObject({ role: "ci", kinds: [] });

  const tally = await reachTally(p, ID);
  expect(tally.find((line) => line.kind === "workflow_run")).toEqual({ kind: "workflow_run", tone: "err", count: 1, section: "builds" });
  // Counted under Builds because the Builds read answered it, not because the shell knows the run kind.
  expect(aroundLines(tally).find((line) => line.section === "Builds")?.count).toBe("2 items");
});

test("the job's section reads keep none by source: each read asked for its role (#1295-#1298)", async () => {
  const p = ports({ others: [MOCK_REPO, B_OK, MOCK_BUILD] });
  open("PAY-231", { ports: p, registry: claimsRegistry() });
  await settle();

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

  calls.list.length = 0;
  await reachTally(p, ID);
  const tallied = calls.list.find((filter) => filter.order === "latest_per_parent");
  expect(tallied).toMatchObject({ role: "ci", kinds: [], reach: ID });
});

// -- A queued comment is drawn at once (#1424) ----------------------------

const squash = (node: Element) => (node.textContent ?? "").replace(/\s+/g, " ").trim();

test("a comment's echo says queued, then sent, and goes on the refresh event's read of the thread, the job not opened again (#2103)", async () => {
  const sync = echoEvents(settle);
  const { follower, sent } = echoFollower();
  let said: { author: string; at: string; body: string; mine: boolean; mentions_me: boolean; markup: "text" }[] = [];
  const io = ports({
    entityThread: () => Promise.resolve({ comments: said, me: ["mara"] }),
    submitWrite: () => Promise.resolve({ id: 72 } as never),
    listen: sync.listen,
  });
  open("PAY-231", { ports: io, follower });
  await settle();
  const box = target.querySelector<HTMLTextAreaElement>("textarea[aria-label='Comment on PAY-231']")!;
  box.value = "Looking at the jitter now";
  box.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
  [...target.querySelectorAll<HTMLButtonElement>(".tf-compose button")].find((b) => squash(b) === "Comment")!.click();
  await settle();
  const echoed = () => [...target.querySelectorAll(".tf-convo .tf-msg.tf-queued .tf-chip")].map((chip) => squash(chip));
  expect(echoed()).toEqual(["queued"]);

  sent(72);
  expect(echoed()).toEqual(["sent"]);

  said = [{ author: "Mara Lindqvist", at: "2026-10-15T13:00:00Z", body: "Looking at the jitter now", mine: true, mentions_me: false, markup: "text" }];
  await sync.refreshed(ID, 72);
  expect(echoed()).toEqual([]);
});

// ---- Refresh on the job page (#2134, #2107's shared pieces) ----

const refreshButton = () => target.querySelector<HTMLButtonElement>(".tf-tkpagehead .tf-refresh");
const jobBanners = () => [...target.querySelectorAll(".tf-tkread > .tf-banner")].map((banner) => squash(banner));

test("Refresh sits beside the title, reads the ticket, says Refreshing… meanwhile, and reads the job again without blanking it", async () => {
  let answer!: (value: EntityRefresh) => void;
  const probe = refreshProbe({ answer: () => new Promise((resolve) => (answer = resolve)) });
  let title = "Retry failed SEPA payouts";
  let threads = 0;
  let reaches = 0;
  /** Set, the thread's next read waits for it. */
  let threadGate: Promise<void> | null = null;
  const io = ports({
    getEntity: (id) => (calls.read.push(id), id === ID ? Promise.resolve(ticket({ row: { ...ticket().row, title } })) : Promise.reject({ code: "not_found" })),
    entityThread: async () => {
      threads++;
      await threadGate;
      return { comments: [], me: ["mara"] };
    },
    jobReach: () => (reaches++, Promise.resolve([])),
    refresh: probe.ports,
  });
  open("PAY-231", { ports: io });
  await settle();
  expect(refreshButton()?.textContent).toBe("Refresh");
  calls.read.length = 0;
  threads = 0;
  reaches = 0;

  refreshButton()!.click();
  await settle();
  expect(probe.refreshed).toEqual([ID]);
  expect(refreshButton()?.textContent).toBe("Refreshing…");
  expect(refreshButton()?.disabled).toBe(true);
  expect(calls.read).toEqual([]);

  title = "Retry failed SEPA payouts once";
  let releaseThread!: () => void;
  threadGate = new Promise((resolve) => (releaseThread = resolve));
  answer({ outcome: "refreshed" });
  flushSync();
  // Drawn as it was until the read answers: the card never blanks.
  expect(target.querySelector(".tf-tkread h1")?.textContent).toBe("Retry failed SEPA payouts");
  await settle();
  expect(target.querySelector(".tf-tkread h1")?.textContent).toBe("Retry failed SEPA payouts once");
  // Refreshing until every read of the job has answered, the thread's too.
  expect(refreshButton()?.textContent).toBe("Refreshing…");
  releaseThread();
  await settle();
  expect(calls.read).toEqual([ID]);
  expect(threads).toBe(1);
  expect(reaches).toBe(1);
  expect(calls.promote).toEqual([ID]);
  expect(target.querySelector(".tf-tkdesc"), "the reading column never blanked").not.toBeNull();
  expect(refreshButton()?.textContent).toBe("Refresh");
  expect(target.querySelector(".tf-refresh-failed")).toBeNull();
});

test("a comment the app could not ask about is asked again on a refresh (#2306)", async () => {
  const reason = "knobas could not ask whether this credential may make this write (HTTP 502); it asks again when the write is drawn again";
  let down = true;
  let asked = 0;
  open("PAY-231", {
    ports: ports({
      refresh: refreshProbe({ answer: () => Promise.resolve({ outcome: "refreshed" }) }).ports,
      refusedWrites: (asks) => {
        asked += 1;
        return Promise.resolve(down ? asks.map((ask) => ({ ...ask, reason })) : []);
      },
    }),
  });
  await settle();
  const send = () => [...target.querySelectorAll<HTMLButtonElement>(".tf-compose button")].find((b) => squash(b) === "Comment")!;
  expect(send().title).toBe(reason);

  down = false;
  refreshButton()!.click();
  await settle();
  expect(asked).toBe(2);
  expect(send().hasAttribute("aria-disabled")).toBe(false);
});

test("a failed refresh on the job says why at the top of the reading column and keeps the job as it was", async () => {
  const probe = refreshProbe({ answer: () => Promise.resolve({ outcome: "failed", reason: "connection refused" }) });
  open("PAY-231", { ports: ports({ refresh: probe.ports }) });
  await settle();
  calls.read.length = 0;
  refreshButton()!.click();
  await settle();
  expect(jobBanners()).toEqual(["Couldn't refresh: connection refused"]);
  expect(target.querySelector(".tf-tkread h1")?.textContent).toBe("Retry failed SEPA payouts");
  expect(calls.read).toEqual([]);
  expect(refreshButton()?.textContent).toBe("Refresh");
});

test("a ticket whose adapter does not read one item offers Sync now on the job, and the job is read again when the run ends", async () => {
  const probe = refreshProbe();
  const io = ports({
    getEntity: (id) => (calls.read.push(id), id === ID ? Promise.resolve(ticket({ refreshes: false })) : Promise.reject({ code: "not_found" })),
    refresh: probe.ports,
  });
  open("PAY-231", { ports: io });
  await settle();
  expect(refreshButton()?.textContent).toBe("Sync now");
  calls.read.length = 0;

  refreshButton()!.click();
  await settle();
  expect(probe.synced).toEqual(["jira"]);
  expect(probe.refreshed).toEqual([]);
  expect(refreshButton()?.textContent).toBe("Syncing…");

  probe.end("jira");
  await settle();
  expect(refreshButton()?.textContent).toBe("Sync now");
  expect(calls.read).toEqual([ID]);
});

test("a turned-off source's ticket draws no Refresh on the job, and the withdrawn and turned-off banners show at the top of the reading column", async () => {
  const probe = refreshProbe();
  const off = ticket({ deleted_at: "2026-10-15T12:00:00Z", source: { ...ticket().source, enabled: false } });
  open("PAY-231", { ports: ports({ getEntity: (id) => (id === ID ? Promise.resolve(off) : Promise.reject({ code: "not_found" })), refresh: probe.ports }) });
  await settle();
  expect(refreshButton()).toBeNull();
  const shown = jobBanners();
  expect(shown).toHaveLength(2);
  expect(shown[0]).toMatch(/^Withdrawn upstream, last seen .+ ago\. Kept because links and notes may point at it\.$/);
  expect(shown[1]).toBe(
    "Its source is turned off: Jira is disabled, so this item is hidden everywhere else. Re-enable the source to bring it back.",
  );
});

test("a comment's echo goes on a Refresh's read of the thread (#2103)", async () => {
  const probe = refreshProbe();
  let status = "In Progress";
  let said: { author: string; at: string; body: string; mine: boolean; mentions_me: boolean; markup: "text" }[] = [];
  let next = 80;
  const io = ports({
    getEntity: (id) => (id === ID ? Promise.resolve(ticket({ status })) : Promise.reject({ code: "not_found" })),
    entityThread: () => Promise.resolve({ comments: said, me: ["mara"] }),
    submitWrite: () => Promise.resolve({ id: next++ } as never),
    refresh: probe.ports,
  });
  open("PAY-231", { ports: io });
  await settle();
  const box = target.querySelector<HTMLTextAreaElement>("textarea[aria-label='Comment on PAY-231']")!;
  box.value = "Looking at the jitter now";
  box.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
  [...target.querySelectorAll<HTMLButtonElement>(".tf-compose button")].find((b) => squash(b) === "Comment")!.click();
  await settle();
  const echoed = () => [...target.querySelectorAll(".tf-convo .tf-msg.tf-queued .tf-chip")].map((chip) => squash(chip));
  expect(echoed()).toEqual(["queued"]);

  status = "In Review";
  said = [{ author: "Mara Lindqvist", at: "2026-10-15T13:00:00Z", body: "Looking at the jitter now", mine: true, mentions_me: false, markup: "text" }];
  refreshButton()!.click();
  await settle();
  expect(echoed()).toEqual([]);
});

test("⌘R on the job runs its Refresh, as a click does, and its Sync now when that is drawn (#2156)", async () => {
  const probe = refreshProbe({ answer: () => new Promise(() => {}) });
  open("PAY-231", { ports: ports({ refresh: probe.ports }) });
  await settle();
  expect(refreshButton()?.textContent).toBe("Refresh");
  expect(pressRefreshKey().defaultPrevented).toBe(true);
  await settle();
  expect(probe.refreshed).toEqual([ID]);
  // Refreshing…: disabled, so a second ⌘R asks nothing.
  pressRefreshKey();
  await settle();
  expect(probe.refreshed).toEqual([ID]);
});

test("⌘R on a job whose ticket's adapter reads no one item runs its Sync now (#2156)", async () => {
  const probe = refreshProbe();
  open("PAY-231", {
    ports: ports({
      getEntity: (id) => (id === ID ? Promise.resolve(ticket({ refreshes: false })) : Promise.reject({ code: "not_found" })),
      refresh: probe.ports,
    }),
  });
  await settle();
  expect(refreshButton()?.textContent).toBe("Sync now");
  pressRefreshKey();
  await settle();
  expect(probe.synced).toEqual(["jira"]);
  expect(refreshButton()?.textContent).toBe("Syncing…");
  pressRefreshKey();
  await settle();
  expect(probe.synced).toEqual(["jira"]);
});

test("the reading column draws a description its source writes in Jira's wiki markup as Markdown, and one in text as text (#2308)", async () => {
  const wiki = ticket({ description: "h2. Why\n\n# first\n# *second*", markup: "jira_wiki" });
  open("PAY-231", { ports: ports({ getEntity: (id) => (id === ID ? Promise.resolve(wiki) : Promise.reject({ code: "not_found" })) }) });
  await settle();
  const asks = target.querySelector(".tf-tkdesc")!;
  expect(asks.querySelector("h2")?.textContent).toBe("Why");
  expect([...asks.querySelectorAll("ol li")].map((li) => li.textContent)).toEqual(["first", "second"]);
  expect(asks.querySelector("ol strong")?.textContent).toBe("second");

  unmount(job!);
  const plain = ticket({ description: "h2. Why\n# first", markup: "text" });
  open("PAY-231", { ports: ports({ getEntity: (id) => (id === ID ? Promise.resolve(plain) : Promise.reject({ code: "not_found" })) }) });
  await settle();
  const text = target.querySelector(".tf-tkdesc .tf-asks")!;
  expect(text.querySelector("h2")).toBeNull();
  expect(text.textContent).toBe("h2. Why\n# first");

  unmount(job!);
  const blank = ticket({ description: "  \n", markup: "markdown" });
  open("PAY-231", { ports: ports({ getEntity: (id) => (id === ID ? Promise.resolve(blank) : Promise.reject({ code: "not_found" })) }) });
  await settle();
  expect(target.querySelector(".tf-tkdesc .tf-asks")).toBeNull();
  expect(target.querySelector(".tf-tkdesc")!.textContent).toContain("The ticket has no description.");
});
