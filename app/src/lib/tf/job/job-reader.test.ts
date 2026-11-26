/**
 * The job page's reads, with no mount (#1320, spec #1304): opening two jobs in
 * a row keeps the second's answers, a reread after a write touches what it
 * names and nothing else, and a section whose read fails has nothing in it.
 * Every read about a ticket waits while the test holds that ticket.
 */
import { expect, test } from "vitest";

import type { CheckoutView, ContextRow, EntityDetail, EntityFilter, EntityRow, JobLinkSets, MiniBoard, Reached } from "../../ipc/entity";
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

function fakes(
  over: {
    failing?: string[];
    rows?: (filter: EntityFilter) => EntityRow[];
    board?: (query: string) => Promise<MiniBoard>;
    checkout?: (id: string) => Promise<CheckoutView>;
    /** Offer `cardReads` (ux19): the reach and the four sections in one read, answered from the reads it stands for. */
    card?: boolean | "failing";
    /** What the promotion answers over the default (a ticket's context made long ago). */
    context?: Partial<ContextRow>;
  } = {},
) {
  /** Tickets whose reads wait, by id, with what lets them go. */
  const held = new Map<string, { gate: Promise<void>; release: () => void }>();
  const asked: string[] = [];
  /** Each card read as it is sent, before it answers. */
  const cardsSent: string[] = [];
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
      return { id: `ctx:${id}`, kind: "ticket", title: id, anchor_id: id, created_at: "", archived_at: null, ...over.context } satisfies ContextRow;
    },
    listEntities: async (filter: EntityFilter) => {
      asked.push(`listEntities ${filter.role} ${filter.reach}`);
      await wait(filter.reach);
      await wait(`role ${filter.role}`);
      if (over.failing?.includes(filter.role ?? "")) throw { code: "unreachable", message: "down", source_id: null };
      const rows = over.rows?.(filter) ?? [row(filter.role, filter.reach)];
      return { rows, total: rows.length, tally: null, groups: null };
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
    ticketBoard: over.board ?? (() => Promise.reject(new Error("no board"))),
    entityCheckout: over.checkout ?? ((id) => Promise.reject(new Error(`no checkout ${id}`))),
  };
  if (over.card !== undefined && over.card !== false) {
    const { jobReach, listEntities } = ports;
    /** Takes back the line a read it stands for logged: the card is asked as one read. */
    const unlog = (line: string) => asked.splice(asked.lastIndexOf(line), 1);
    ports.cardReads = async (id, sections, limit) => {
      cardsSent.push(id);
      if (over.card === "failing") {
        asked.push(`cardReads ${id}`);
        throw { code: "internal", message: "the walk failed", source_id: null };
      }
      // Answered by the reads it stands for, as the fixture's `card_reads` is, and asked as one.
      const reached = await jobReach(id);
      const rows = await Promise.all(
        sections.map((filter) => listEntities({ ...filter, reach: id }, limit, 0).then((page) => page.rows, () => [] as EntityRow[])),
      );
      unlog(`jobReach ${id}`);
      for (const filter of sections) unlog(`listEntities ${filter.role} ${id}`);
      asked.push(`cardReads ${id} ${sections.map((filter) => filter.role).join(",")} ${limit}`);
      return { reached, sections: rows };
    };
  }
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
  return { reader, asked, cardsSent, hold, status, holdRole: (role: string) => hold(`role ${role}`) };
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

// ---- the card's sections table (#2401) ----

/** A board of `columns`, each `[status, total]`, as `mini_board` answers. */
function board(columns: [string | null, number][]): MiniBoard {
  return { columns: columns.map(([status, total]) => ({ status, cards: [], total })), sources: [], lanes: [], more_lanes: 0 };
}

/** A checkout standing on `branch`, `ahead` commits ahead of its upstream. */
function checkout(branch: string, ahead: number): CheckoutView {
  return {
    repo_entity_id: "gitea:repo",
    repo_url: null,
    path: "/clones/repo",
    found_by: "scan",
    clones_root: "/clones",
    clone_command: null,
    tree: { status: "read", branch, head: "abc", upstream: `origin/${branch}`, ahead, behind: 0, changed: 0, untracked: 0, conflicted: 0 },
  };
}

test("the sub-tasks are the board under the ticket's key, counted by its columns' totals, Done not open", async () => {
  const queries: string[] = [];
  const { reader } = fakes({
    board: (query) => {
      queries.push(query);
      return Promise.resolve(board([["To Do", 2], ["In Progress", 1], ["Done", 4]]));
    },
  });
  reader.open("jira:PAY-231");
  await flush();
  expect(queries).toEqual(["under:PAY-231"]);
  expect(reader.subtasks).toEqual({ total: 7, open: 3 });
});

test("a board that cannot be read counts no sub-tasks", async () => {
  const { reader } = fakes();
  reader.open("jira:PAY-231");
  await flush();
  expect(reader.subtasks).toEqual({ total: 0, open: 0 });
});

test("a branch counts as not pushed when its checkout stands on it with commits ahead, and only then", async () => {
  const branch = (id: string, title: string): EntityRow => ({ entity_id: id, kind: "branch", source_id: "gitea", title, updated_at: "", synced_at: "", path: null, state: null });
  const trees: Record<string, CheckoutView | Error> = {
    "b:ahead": checkout("feat/ahead", 2),
    "b:even": checkout("feat/even", 0),
    "b:elsewhere": checkout("main", 3),
    "b:none": new Error("no checkout"),
  };
  const { reader } = fakes({
    rows: (filter) =>
      filter.role === "code-host"
        ? [branch("b:ahead", "feat/ahead"), branch("b:even", "feat/even"), branch("b:elsewhere", "feat/elsewhere"), branch("b:none", "feat/none")]
        : [row(filter.role, filter.reach)],
    checkout: (id) => (trees[id] instanceof Error ? Promise.reject(trees[id]) : Promise.resolve(trees[id] as CheckoutView)),
  });
  reader.open("jira:PAY-231");
  await flush();
  expect([...reader.unpushed]).toEqual(["b:ahead"]);
});

// ---- the card's sections in one read (ux19) ----

test("with the card's read, opening asks the reach and the four sections once, and draws what the five reads drew", async () => {
  const five = fakes();
  five.reader.open("jira:B");
  await flush();
  const one = fakes({ card: true });
  one.reader.open("jira:B");
  await flush();
  expect(slots(one.reader)).toEqual(slots(five.reader));
  expect(slots(one.reader)).toEqual(B);
  expect(one.asked.filter((call) => /^(jobReach|listEntities|cardReads)/.test(call))).toEqual(["cardReads jira:B code-host,ci,wiki,monitoring 200"]);
});

test("the card's read lands every section at once, so the card's hold waits on one answer", async () => {
  const { reader, holdRole } = fakes({ card: true });
  const releaseWiki = holdRole("wiki");
  reader.open("jira:B");
  await flush();
  // Docs is held, so the one read is: nothing of it has landed, not even the sections that answered.
  expect([reader.code, reader.builds, reader.pages, reader.monitors]).toEqual([null, null, null, null]);
  expect(reader.reached.size).toBe(0);
  releaseWiki();
  await flush();
  expect(slots(reader)).toEqual(B);
});

test("a card read that fails draws every section as having nothing, and the ticket still draws", async () => {
  const { reader } = fakes({ card: "failing" });
  reader.open("jira:B");
  await flush();
  expect(slots(reader)).toEqual({ ...B, reached: [], code: [], builds: [], pages: [], monitors: [] });
  expect(reader.assets).toEqual([]);
});

test("a card read for a job left behind lands nothing", async () => {
  const { reader, hold } = fakes({ card: true });
  const releaseA = hold("jira:A");
  reader.open("jira:A");
  await flush();
  reader.open("jira:B");
  await flush();
  releaseA();
  await flush();
  expect(slots(reader)).toEqual(B);
});

test("with the card's read, a reread after Hide here asks the link sets, the sub-tasks and the card once", async () => {
  const { reader, asked } = fakes({ card: true });
  reader.open("jira:B");
  await flush();
  asked.length = 0;
  reader.rereadReach("jira:B");
  await flush();
  expect(asked.sort()).toEqual(["cardReads jira:B code-host,ci,wiki,monitoring 200", "jobLinkSets jira:B"]);
  expect(slots(reader)).toEqual(B);
});

test("an epic's context the promotion just made reads the card again; an epic's older one, or a ticket's, does not", async () => {
  const cards = (asked: string[]) => asked.filter((call) => call.startsWith("cardReads")).length;
  const made = fakes({ card: true, context: { kind: "epic", created_at: new Date().toISOString() } });
  made.reader.open("jira:B");
  await flush();
  expect(cards(made.asked)).toBe(2);
  expect(slots(made.reader)).toEqual(B);

  const older = fakes({ card: true, context: { kind: "epic", created_at: "2026-01-01T00:00:00Z" } });
  older.reader.open("jira:B");
  await flush();
  expect(cards(older.asked)).toBe(1);

  const story = fakes({ card: true, context: { kind: "ticket", created_at: new Date().toISOString() } });
  story.reader.open("jira:B");
  await flush();
  expect(cards(story.asked)).toBe(1);
});

test("an address naming the ticket's id sends the card's read with the press, before the ticket answers", async () => {
  const { reader, cardsSent, hold, asked } = fakes({ card: true });
  const release = hold("jira:B");
  reader.open("jira:B");
  await flush();
  expect(reader.detail).toBeUndefined();
  expect(cardsSent).toEqual(["jira:B"]);
  release();
  await flush();
  // The one read sent early is the card's: none is sent again once the ticket answers.
  expect(cardsSent).toEqual(["jira:B"]);
  expect(asked.filter((call) => call.startsWith("cardReads"))).toHaveLength(1);
  expect(slots(reader)).toEqual(B);
});

test("an address naming a key alone sends the card's read once the ticket is found", async () => {
  const { reader, cardsSent, hold } = fakes({ card: true });
  const release = hold("jira:B");
  reader.open("B");
  await flush();
  expect(cardsSent).toEqual([]);
  release();
  await flush();
  expect(cardsSent).toEqual(["jira:B"]);
  expect(slots(reader)).toEqual(B);
});

test("the sub-tasks' board is asked with the press, under the key the address names, and once", async () => {
  const queries: string[] = [];
  const { reader, hold } = fakes({
    board: (query) => {
      queries.push(query);
      return Promise.resolve(board([["To Do", 1], ["Done", 2]]));
    },
  });
  const release = hold("jira:PAY-231");
  reader.open("PAY-231");
  await flush();
  expect(reader.detail).toBeUndefined();
  expect(queries).toEqual(["under:PAY-231"]);
  release();
  await flush();
  expect(queries).toEqual(["under:PAY-231"]);
  expect(reader.subtasks).toEqual({ total: 3, open: 1 });
});
