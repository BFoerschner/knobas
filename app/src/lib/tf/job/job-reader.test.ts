/**
 * The job page's reads, with no mount (#1320, spec #1304): opening two jobs in
 * a row keeps the second's answers, a reread after a write touches what it
 * names and nothing else, and a section whose read fails has nothing in it.
 * Every read about a ticket waits while the test holds that ticket.
 */
import { expect, test } from "vitest";

import type { ContextRow, EntityDetail, EntityFilter, EntityRow, JobLinkSets, Reached } from "../../ipc/entity";
import { createJobReader, type JobReaderPorts } from "./job-reader.svelte";
import { sourceFacts } from "./source-facts.fixture";

/** Lets every settled promise's callbacks run. */
async function flush() {
  for (let turn = 0; turn < 10; turn += 1) await new Promise((resolve) => setTimeout(resolve, 0));
}

function ticket(id: string, status = "In Progress"): EntityDetail {
  return {
    row: { entity_id: id, kind: "ticket", source_id: "jira", title: `Ticket ${id}`, updated_at: "", synced_at: "", path: null, state: null },
    source: { id: "jira", display_name: "Jira", adapter_kind: "jira", enabled: true },
    kind_info: null,
    refreshes: true,
    body_text: "",
    author: null,
    payload: {},
    web_url: null,
    deleted_at: null,
    links: [],
    activity: [],
    builds: [],
    status,
    issue_type: "Story",
    priority: null,
    estimate_seconds: null,
    description: null,
    markup: "text",
  };
}

/** One row a section's read answers for `reach`: its role and the ticket in the title. */
function row(role: string | null | undefined, reach: string | null | undefined): EntityRow {
  const kind = { "code-host": "repo", ci: "build", wiki: "page", monitoring: "monitor" }[role ?? ""] ?? "repo";
  return { entity_id: `${role}:${reach}`, kind, source_id: "x", title: `${role} of ${reach}`, updated_at: "", synced_at: "", path: null, state: null };
}

function fakes(over: { failing?: string[] } = {}) {
  /** Tickets whose reads wait, by id, with what lets them go. */
  const held = new Map<string, { gate: Promise<void>; release: () => void }>();
  const asked: string[] = [];
  const status = new Map<string, string>();
  const wait = (id: string | null | undefined) => held.get(id ?? "")?.gate ?? Promise.resolve();
  const ports: JobReaderPorts = {
    listSources: () => {
      asked.push("listSources");
      return Promise.resolve([sourceFacts("jira")]);
    },
    getEntity: async (id) => {
      asked.push(`getEntity ${id}`);
      await wait(id);
      return ticket(id, status.get(id));
    },
    promoteContext: async (id) => {
      asked.push(`promoteContext ${id}`);
      await wait(id);
      return { id: `ctx:${id}`, kind: "ticket", title: id, anchor_id: id, created_at: "", archived_at: null } satisfies ContextRow;
    },
    listEntities: async (filter: EntityFilter) => {
      asked.push(`listEntities ${filter.role} ${filter.reach}`);
      await wait(filter.reach);
      await wait(`role ${filter.role}`);
      if (over.failing?.includes(filter.role ?? "")) throw { code: "unreachable", message: "down", source_id: null };
      return { rows: [row(filter.role, filter.reach)], total: 1, tally: null, groups: null };
    },
    visitEntity: async (id) => {
      asked.push(`visitEntity ${id}`);
      await wait(id);
      return `visited ${id}`;
    },
    entityThread: async (id) => {
      asked.push(`entityThread ${id}`);
      await wait(id);
      return { comments: [], me: [id] };
    },
    roomSuggestions: async (_sources, context) => {
      asked.push(`roomSuggestions ${context}`);
      return { rows: [], total: 0, more: [] };
    },
    jobReach: async (id) => {
      asked.push(`jobReach ${id}`);
      await wait(id);
      return [{ entity_id: `repo of ${id}`, kind: "repo", step: `repo of ${id}`, origin: { kind: "own" } } satisfies Reached];
    },
    jobLinkSets: async (id) => {
      asked.push(`jobLinkSets ${id}`);
      await wait(id);
      return { sets: [], parent: { ticket: `parent of ${id}`, title: null, links: [] } } satisfies JobLinkSets;
    },
    getAsset: (id) => Promise.reject(new Error(`no asset ${id}`)),
  };
  const reader = createJobReader(ports, { registry: { load: () => Promise.resolve() } });
  function hold(id: string) {
    let release = () => {};
    const gate = new Promise<void>((resolve) => (release = resolve));
    held.set(id, { gate, release });
    return () => {
      held.delete(id);
      release();
    };
  }
  return { reader, asked, hold, status, holdRole: (role: string) => hold(`role ${role}`) };
}

/** What the reader holds about which ticket, one line a slot. */
function slots(reader: ReturnType<typeof createJobReader>) {
  return {
    detail: reader.detail?.row.entity_id,
    thread: reader.thread?.me,
    lastVisit: reader.lastVisit,
    inherited: reader.inherited?.parent?.ticket,
    reached: [...reader.reached.keys()],
    code: reader.code?.map((one) => one.title),
    builds: reader.builds?.map((one) => one.title),
    pages: reader.pages?.map((one) => one.title),
    monitors: reader.monitors?.map((one) => one.title),
    proposals: reader.proposals,
    failure: reader.failure,
  };
}

const B = {
  detail: "jira:B",
  thread: ["jira:B"],
  lastVisit: "visited jira:B",
  inherited: "parent of jira:B",
  reached: ["repo of jira:B"],
  code: ["code-host of jira:B"],
  builds: ["ci of jira:B"],
  pages: ["wiki of jira:B"],
  monitors: ["monitoring of jira:B"],
  proposals: [],
  failure: null,
};

test("opening two jobs in a row keeps the second's answers and drops the first's late ones", async () => {
  const { reader, hold } = fakes();
  const releaseA = hold("jira:A");
  reader.open("jira:A");
  await flush();
  // The first job's ticket has not answered: nothing is drawn yet.
  expect(reader.detail).toBeUndefined();

  reader.open("jira:B");
  await flush();
  expect(slots(reader)).toEqual(B);

  // The first job answers now, every read of it: none of it lands.
  releaseA();
  await flush();
  expect(slots(reader)).toEqual(B);
});

test("a job opened while the last one's sections are out keeps none of the last one's rows", async () => {
  const { reader, hold, asked } = fakes();
  reader.open("jira:A");
  await flush();
  expect(reader.detail?.row.entity_id).toBe("jira:A");

  // A's sections are read again and held; B opens before they answer.
  const releaseA = hold("jira:A");
  reader.rereadReach("jira:A");
  reader.open("jira:B");
  // The reset is at once: nothing of A is drawn while B is read.
  expect(reader.detail).toBeUndefined();
  expect(reader.builds).toBeNull();
  expect(reader.reached.size).toBe(0);
  await flush();
  releaseA();
  await flush();
  expect(slots(reader)).toEqual(B);
  // B's context was promoted once, A's once: rereads promote nothing.
  expect(asked.filter((call) => call.startsWith("promoteContext"))).toEqual(["promoteContext jira:A", "promoteContext jira:B"]);
});

test("a reread after a write touches the detail only", async () => {
  const { reader, asked, status } = fakes();
  reader.open("jira:B");
  await flush();
  const before = slots(reader);
  const { builds, code, reached } = reader;
  asked.length = 0;

  status.set("jira:B", "In Review");
  reader.rereadDetail("jira:B");
  await flush();
  expect(asked).toEqual(["getEntity jira:B"]);
  expect(reader.detail?.status).toBe("In Review");
  expect(slots(reader)).toEqual(before);
  // The sections are the same answers, not new ones.
  expect(reader.builds).toBe(builds);
  expect(reader.code).toBe(code);
  expect(reader.reached).toBe(reached);
});

test("a reread for a job left behind reads nothing", async () => {
  const { reader, asked } = fakes();
  reader.open("jira:B");
  await flush();
  asked.length = 0;
  reader.rereadDetail("jira:A");
  reader.rereadReach("jira:A");
  await flush();
  expect(asked).toEqual([]);
});

test("a reread after Hide here reads the link sets, the reach and the sections, not the ticket", async () => {
  const { reader, asked } = fakes();
  reader.open("jira:B");
  await flush();
  const detail = reader.detail;
  asked.length = 0;

  reader.rereadReach("jira:B");
  await flush();
  expect(asked.sort()).toEqual(
    [
      "jobLinkSets jira:B",
      "jobReach jira:B",
      "listEntities ci jira:B",
      "listEntities code-host jira:B",
      "listEntities monitoring jira:B",
      "listEntities wiki jira:B",
    ].sort(),
  );
  expect(reader.detail).toBe(detail);
  expect(slots(reader)).toEqual(B);
});

test("a section read that fails draws nothing for that section, and the others keep theirs", async () => {
  const { reader } = fakes({ failing: ["wiki"] });
  reader.open("jira:B");
  await flush();
  expect(slots(reader)).toEqual({ ...B, pages: [] });
});

test("a job closed drops every answer still out", async () => {
  const { reader, hold } = fakes();
  const release = hold("jira:B");
  reader.open("jira:B");
  await flush();
  reader.close();
  release();
  await flush();
  expect(reader.detail).toBeUndefined();
  expect(reader.builds).toBeNull();
});

test("a refresh's reread reads the ticket, its thread, link sets, reach and sections, and never visits or promotes again (#2134)", async () => {
  const { reader, asked, status } = fakes();
  reader.open("jira:B");
  await flush();
  asked.length = 0;

  status.set("jira:B", "In Review");
  await reader.reread();
  expect(asked.sort()).toEqual(
    [
      "getEntity jira:B",
      "entityThread jira:B",
      "jobLinkSets jira:B",
      "jobReach jira:B",
      "listEntities ci jira:B",
      "listEntities code-host jira:B",
      "listEntities monitoring jira:B",
      "listEntities wiki jira:B",
    ].sort(),
  );
  expect(reader.detail?.status).toBe("In Review");
  expect(slots(reader)).toEqual(B);
});

test("a refresh's reread still out when another job opens lands nothing of the first (#2134)", async () => {
  const { reader, hold } = fakes();
  reader.open("jira:A");
  await flush();
  const releaseA = hold("jira:A");
  const out = reader.reread();
  reader.open("jira:B");
  await flush();
  releaseA();
  await out;
  await flush();
  expect(slots(reader)).toEqual(B);
});

test("a refresh's reread settles only once every section has answered (#2134)", async () => {
  const { reader, holdRole } = fakes();
  reader.open("jira:B");
  await flush();
  const releaseBuilds = holdRole("ci");
  let settled = false;
  void reader.reread().then(() => (settled = true));
  await flush();
  expect(settled).toBe(false);
  releaseBuilds();
  await flush();
  expect(settled).toBe(true);
  expect(slots(reader)).toEqual(B);
});
