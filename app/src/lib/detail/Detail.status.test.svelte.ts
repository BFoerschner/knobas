/**
 * The ticket detail's status select (#179, #498), and the one rule it lives
 * under: **it never says a status the mirror does not hold.**
 *
 * Picking a status queues a `Transition` through the write queue and stops
 * there. The queue decides send, pend or hold; the source's workflow decides
 * whether the move is legal at all, and refuses by name. So the control goes
 * straight back to the mirrored status after a pick, and the board and this
 * panel both keep saying what the source last said until a sync mirrors the
 * landed write. A select that jumped to the new value would be reporting a
 * hope.
 *
 * Since #498 the **offer** comes from the workflow rather than from the
 * corpus: `reachableTransitions` asks the source what it can reach from where
 * it stands, and the two reads answer different questions — the board says
 * where the ticket *is* and the workflow read says where it can *go*. The
 * corpus offer is still here as the fallback, and a fallback says so on
 * screen.
 *
 * Its own file rather than more of `Detail.test.svelte.ts`: that one is about
 * the three answers a read can have, and this needs a loaded kind registry and
 * three commands of its own.
 */
import { flushSync, mount, unmount } from "svelte";
import { beforeEach, expect, test, vi } from "vitest";

import type { EntityDetail } from "../ipc/entity";
import type { SourceDescriptor } from "../ipc/sources";

/** Plain functions, not `vi.fn` — see the note in `shell/Tile.test.svelte.ts`. */
const queued: unknown[] = [];
/**
 * Every time the panel read a mini board. It must never: since #760 the
 * current status arrives with `get_entity` and the fallback offer is its own
 * read, so a card list is not read to open a ticket.
 */
let boardCalls = 0;
/** Every source the fallback offer was asked about, in order (#760). */
const corpusCalls: string[] = [];
/** Every entity the workflow read was asked about, in order (#498). */
const workflowCalls: string[] = [];
let corpus: () => Promise<string[]> = () => Promise.resolve(CORPUS);
/**
 * What the workflow answers. The default is the seeded Jira shape: every other
 * status, **plus the one the ticket is already in** — Jira's simplified
 * workflow offers a move to the current status, and the select has to be the
 * thing that takes it out.
 */
let workflow: () => Promise<string[]> = () => Promise.resolve(["In Progress", "In Review", "Done"]);
let submitFails = false;

vi.mock("../ipc/entity", () => ({
  listContexts: () => Promise.resolve([]),
  contextMembers: () => Promise.resolve([]),
  createContext: () => Promise.reject(new Error("no context creation in this test")),
  promoteContext: () => Promise.reject(new Error("no promotion in this test")),
  // No source view (#661): the panel is the mirror's, as before.
  entityView: () => Promise.resolve(null),
  getEntity: () => Promise.resolve(entity()),
  unlink: () => Promise.resolve(),
  createLink: () => Promise.resolve({}),
  miniBoard: () => {
    boardCalls += 1;
    return Promise.reject(new Error("a ticket's detail reads no board"));
  },
  sourceStatuses: (sourceId: string) => {
    corpusCalls.push(sourceId);
    return corpus();
  },
  reachableTransitions: (entityId: string) => {
    workflowCalls.push(entityId);
    return workflow();
  },
  submitWrite: (payload: unknown) => {
    queued.push(payload);
    if (submitFails) {
      return Promise.reject({ code: "conflict", message: "the queue is holding a write", source_id: null });
    }
    return Promise.resolve({});
  },
}));

/** The dialog's picker; this file never opens it. */
vi.mock("../ipc/search", () => ({
  search: () => Promise.reject(new Error("the picker is not this file's business")),
  launcherHome: () => Promise.reject(new Error("the dialog never loads the board")),
  noFilters: () => ({ sources: [], kinds: [], updated_within_days: null, mine: false, authors: [] }),
}));

/** What `get_entity` answers with; a test that needs another kind swaps it. */
let entity: () => EntityDetail = () => detail();

/** What the registry answers `list_adapters` with — `write_ops` is the point. */
let writeOps: string[] = ["comment", "transition"];

vi.mock("../ipc/sources", () => ({
  listAdapters: (): Promise<SourceDescriptor[]> =>
    Promise.resolve([
      {
        id: "mock",
        adapter_kind: "mock",
        name: "Tidewater mock",
        capabilities: [],
        adapter_version: "0",
        auth_methods: [],
        accepts_account: false,
        // A getter, because the registry caches the descriptor on its first
        // (and only) `load()`: a plain array would freeze whatever `write_ops`
        // happened to be during the first test in this file, and every later
        // one would silently assert against that.
        get write_ops() {
          return writeOps;
        },
        entity_kinds: [],
        payload_paths: [],
        config_schema: {},
      },
    ]),
}));

vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: () => Promise.resolve() }));

const { default: Detail } = await import("./Detail.svelte");
const { toasts } = await import("../shell/toasts.svelte");
const { kindRegistry } = await import("../shell/kind-registry.svelte");

/**
 * The source's corpus shows more than the ticket's workflow reaches, and a
 * room where nothing is finished still has to be able to offer *Done*: the
 * fallback offer is the corpus, not whatever a board's columns happen to hold.
 */
const CORPUS = ["To Do", "In Progress", "In Review", "Done"];

function detail(over: Partial<EntityDetail> = {}): EntityDetail {
  return {
    row: {
      entity_id: "mock:PAY-231",
      kind: "ticket",
      source_id: "mock",
      title: "Retry failed SEPA payouts",
      updated_at: "2026-08-22T11:48:00Z",
      synced_at: "2026-08-22T14:30:00Z",
      path: null,
      state: null,
    },
    source: { id: "mock", display_name: "Tidewater (mock)", adapter_kind: "mock", enabled: true },
    kind_info: null,
    body_text: "",
    author: "mara",
    payload: { key: "PAY-231", status: "In Progress" },
    web_url: null,
    deleted_at: null,
    links: [],
    activity: [],
    builds: [],
    // `PAY-231` sits *In Progress*, as `get_entity` reads it (#760).
    status: "In Progress",
    issue_type: null,
    priority: null,
    estimate_seconds: null,
    ...over,
  };
}

function render(entityId = "mock:PAY-231", kind: string | null = "ticket") {
  const target = document.createElement("div");
  document.body.append(target);
  const props = $state({
    entityId,
    kind,
    contextLabel: "All work",
    onclose: vi.fn(),
    onnavigate: vi.fn(),
  });
  const app = mount(Detail, { target, props });
  flushSync();
  return {
    target,
    /** Move the address to another entity, as the router does. */
    reopen: (id: string) => {
      props.entityId = id;
      flushSync();
    },
    select: () => target.querySelector<HTMLSelectElement>("select.sel-inline"),
    options: () =>
      [...(target.querySelectorAll<HTMLOptionElement>("select.sel-inline option") ?? [])].map(
        (option) => option.textContent ?? "",
      ),
    text: () => (target.textContent ?? "").replace(/\s+/g, " "),
    done: () => {
      unmount(app);
      target.remove();
    },
  };
}

/** Pick a status the way a person does: set the value, then fire `change`. */
function pick(select: HTMLSelectElement, status: string) {
  select.value = status;
  select.dispatchEvent(new Event("change", { bubbles: true }));
  flushSync();
}

beforeEach(async () => {
  queued.length = 0;
  boardCalls = 0;
  corpusCalls.length = 0;
  workflowCalls.length = 0;
  toasts.items = [];
  submitFails = false;
  writeOps = ["comment", "transition"];
  corpus = () => Promise.resolve(CORPUS);
  workflow = () => Promise.resolve(["In Progress", "In Review", "Done"]);
  entity = () => detail();
  await kindRegistry.load();
});

/**
 * The offer is the **workflow's**, and the corpus is not merged into it (#498).
 *
 * The fixture is built so the two disagree in both directions and one
 * assertion can see it: the corpus shows *To Do*, which this workflow does not
 * reach from *In Progress*, so an offer built from the corpus — or from a
 * union of the two — would still list it. And the workflow answers with *In
 * Progress* itself, so a select that offered the reply verbatim would let a
 * reader queue a move to where the ticket already is.
 */
test("offers the statuses the workflow reaches, with the current one marked and unselectable", async () => {
  const screen = render();
  await vi.waitFor(() => expect(screen.select()).not.toBeNull());
  flushSync();

  expect(screen.options()).toEqual(["In Progress", "In Review", "Done"]);
  expect(screen.options(), "a corpus status the workflow does not reach").not.toContain("To Do");
  expect(screen.select()?.value).toBe("In Progress");
  // Where it stands: shown, so the control says something, and unselectable,
  // because a move to the status a ticket is already in is not a move.
  expect(screen.select()?.querySelector("option")?.disabled).toBe(true);
  expect([...(screen.select()?.querySelectorAll("option:not(:disabled)") ?? [])]).toHaveLength(2);
  // Asked about this ticket, by its entity id and nothing else: the source is
  // the namespace (interfaces §4.1), so there is no second argument to get
  // wrong.
  expect(workflowCalls).toEqual(["mock:PAY-231"]);
  // Opening a ticket reads no card list (#760): where it stands came with
  // `get_entity`, and the corpus offer is the fallback, asked only on failure.
  expect(boardCalls).toBe(0);
  expect(corpusCalls).toEqual([]);

  screen.done();
});

/**
 * The fallback, and the whole reason it is labelled.
 *
 * A workflow read can fail for something the reader cannot act on from this
 * panel — an expired credential, a Jira that is down — and the honest drawing
 * is the offer knobas had before the read existed, plus a word saying nobody
 * checked it. Silently showing the corpus would be the pre-#498 behaviour
 * wearing this ticket's clothes.
 */
test("a failed workflow read falls back to the corpus offer and says it is unverified", async () => {
  workflow = () =>
    Promise.reject({ code: "unauthorized", message: "the PAT expired", source_id: "mock" });
  const screen = render();
  await vi.waitFor(() => expect(screen.text()).toContain("offer unverified"));
  flushSync();

  expect(screen.options()).toEqual(["In Progress", "To Do", "In Review", "Done"]);
  expect(screen.select()?.value).toBe("In Progress");
  // Asked about the ticket's own source, once, and only because the workflow
  // failed (#760).
  expect(corpusCalls).toEqual(["mock"]);
  expect(boardCalls).toBe(0);
  // No toast: one per opened ticket over a source that is down is noise, and
  // the note beside the select is what the reader is owed instead.
  expect(toasts.items).toEqual([]);

  screen.done();
});

/**
 * **Nothing is offered until something has answered.** The corpus is the
 * fallback for a read that *failed*, not for one still in flight: a select that
 * showed it while the workflow was being read would offer statuses nobody has
 * checked, with no note saying so, in the one window where the note is the
 * whole point.
 */
test("a workflow read still in flight offers nothing yet", async () => {
  let land!: (statuses: string[]) => void;
  workflow = () => new Promise<string[]>((resolve) => (land = resolve));
  const screen = render();
  await vi.waitFor(() => expect(workflowCalls).toHaveLength(1));
  flushSync();

  expect(screen.select(), "the ticket has answered and the workflow has not").toBeNull();
  expect(screen.text()).not.toContain("offer unverified");

  land(["In Review"]);
  await vi.waitFor(() => expect(screen.select()).not.toBeNull());
  expect(screen.options()).toEqual(["In Progress", "In Review"]);

  screen.done();
});

/**
 * A workflow that answered and has nowhere to go draws **no select** — and it
 * must not be confused with a read that failed, which is why the two cases sit
 * next to each other. An empty answer is a fact about this ticket; a failed
 * read is a fact about the connection.
 */
test("a workflow with nowhere left to go draws no select and no note", async () => {
  workflow = () => Promise.resolve([]);
  const screen = render();
  await vi.waitFor(() => expect(workflowCalls).toHaveLength(1));
  flushSync();

  expect(screen.select()).toBeNull();
  expect(screen.text()).not.toContain("offer unverified");
  expect(corpusCalls, "an answered workflow needs no fallback").toEqual([]);

  screen.done();
});

/**
 * The read is not made where the source cannot move a ticket at all: such a
 * source has no workflow and refuses the call by name (contract battery clause
 * 8), so making it would be spending a round trip on an answer that is already
 * known — and the select is absent for the reason it always was.
 */
test("no read where the adapter does not declare the transition op", async () => {
  writeOps = ["comment"];
  const screen = render();
  await vi.waitFor(() => expect(screen.text()).toContain("Retry failed SEPA payouts"));
  flushSync();

  expect(screen.select()).toBeNull();
  expect(workflowCalls).toEqual([]);

  screen.done();
});

test("picking a status queues a transition through the write queue", async () => {
  const screen = render();
  await vi.waitFor(() => expect(screen.select()).not.toBeNull());

  pick(screen.select()!, "In Review");
  await vi.waitFor(() => expect(queued).toHaveLength(1));

  expect(queued[0]).toEqual({ Transition: { entity: "mock:PAY-231", status: "In Review" } });
  expect(toasts.items.at(-1)?.text).toContain("Move to In Review queued");

  screen.done();
});

/**
 * The rule this file exists for. The write is *queued*, and the queue may yet
 * pend or hold it — and the source's workflow may refuse the move outright. So
 * the control shows the mirror, both immediately after the pick and once the
 * command has answered.
 */
test("the select goes back to the mirrored status and never shows the picked one", async () => {
  const screen = render();
  await vi.waitFor(() => expect(screen.select()).not.toBeNull());

  pick(screen.select()!, "Done");
  expect(screen.select()?.value, "the control must not sit on an unmirrored status").toBe(
    "In Progress",
  );

  await vi.waitFor(() => expect(queued).toHaveLength(1));
  flushSync();
  expect(screen.select()?.value).toBe("In Progress");
  expect(screen.text()).not.toContain("Done queued for");

  screen.done();
});

test("a refused queue submission says so and leaves the select where it was", async () => {
  submitFails = true;
  const screen = render();
  await vi.waitFor(() => expect(screen.select()).not.toBeNull());

  pick(screen.select()!, "In Review");
  await vi.waitFor(() => expect(toasts.items.at(-1)?.tone).toBe("err"));

  expect(toasts.items.at(-1)?.text).toContain("the queue is holding a write");
  expect(screen.select()?.value).toBe("In Progress");

  screen.done();
});

/**
 * A ticket whose record carries no status the read could recognise sits in the
 * terminal group. It can still be moved *out* — that is most of the point —
 * but "no status" is not somewhere anything can be moved *to*, so the option
 * that shows where it stands cannot be chosen.
 */
test("a ticket in the terminal group shows No status, unselectable, and can still move", async () => {
  entity = () => detail({ status: null });
  workflow = () => Promise.resolve(["To Do"]);
  const screen = render();
  await vi.waitFor(() => expect(screen.select()).not.toBeNull());
  flushSync();

  expect(screen.options()).toEqual(["No status", "To Do"]);
  expect(screen.select()?.value).toBe("");
  expect(screen.select()?.querySelector("option")?.disabled).toBe(true);

  pick(screen.select()!, "To Do");
  await vi.waitFor(() => expect(queued).toHaveLength(1));
  expect(queued[0]).toEqual({ Transition: { entity: "mock:PAY-231", status: "To Do" } });

  screen.done();
});

/**
 * Opening another ticket shows that ticket's status: the current one is the
 * open entity's own, so it cannot be the previous ticket's (#760).
 */
test("opening another ticket shows that ticket's status", async () => {
  const screen = render();
  await vi.waitFor(() => expect(screen.select()?.value).toBe("In Progress"));

  entity = () =>
    detail({
      row: {
        entity_id: "mock:PAY-240",
        kind: "ticket",
        source_id: "mock",
        title: "Payout dashboard latency",
        updated_at: null,
        synced_at: "2026-08-22T14:30:00Z",
        path: null,
        state: null,
      },
      status: "To Do",
    });
  screen.reopen("mock:PAY-240");
  await vi.waitFor(() => expect(screen.select()?.value).toBe("To Do"));
  expect(boardCalls).toBe(0);

  screen.done();
});

/**
 * The dropping half of the guard on the fallback read: the previous ticket's
 * corpus answer lands after the next ticket's, and must lose. The source
 * changes with the ticket here, so the two answers differ.
 */
test("a slow corpus answer from the previous ticket is discarded, not shown", async () => {
  workflow = () => Promise.reject({ code: "unreachable", message: "no route", source_id: "mock" });
  let first!: (statuses: string[]) => void;
  corpus = () => new Promise<string[]>((resolve) => (first = resolve));
  const screen = render();
  await vi.waitFor(() => expect(corpusCalls).toHaveLength(1));

  let second!: (statuses: string[]) => void;
  corpus = () => new Promise<string[]>((resolve) => (second = resolve));
  entity = () =>
    detail({
      row: {
        entity_id: "other:PAY-240",
        kind: "ticket",
        source_id: "other",
        title: "Payout dashboard latency",
        updated_at: null,
        synced_at: "2026-08-22T14:30:00Z",
        path: null,
        state: null,
      },
      source: { id: "other", display_name: "Other", adapter_kind: "mock", enabled: true },
      status: "To Do",
    });
  screen.reopen("other:PAY-240");
  await vi.waitFor(() => expect(corpusCalls).toEqual(["mock", "other"]));

  second(["To Do", "Blocked"]);
  await vi.waitFor(() => expect(screen.select()?.value).toBe("To Do"));

  // The ticket that is no longer on screen answers last, and loses.
  first(CORPUS);
  await Promise.resolve();
  flushSync();
  expect(screen.options(), "the previous ticket's corpus overwrote this one").toEqual([
    "To Do",
    "Blocked",
  ]);

  screen.done();
});

/**
 * What #498 buys, stated as the case that used to have no answer: a source
 * whose corpus shows no status at all — a fresh instance, a room where every
 * ticket sits in the terminal group — offered nothing before, because the offer
 * was the corpus. The workflow knows better, and now it is asked.
 */
test("the workflow's offer stands where the corpus shows nothing", async () => {
  corpus = () => Promise.resolve([]);
  entity = () => detail({ status: null });
  const screen = render();
  await vi.waitFor(() => expect(screen.select()).not.toBeNull());
  flushSync();

  expect(screen.options()).toEqual(["No status", "In Progress", "In Review", "Done"]);
  expect(screen.text()).not.toContain("offer unverified");

  screen.done();
});

/**
 * And the other half: no workflow answer *and* no corpus is nothing to offer,
 * which draws no select — the rule the panel has always followed, that an
 * absence is honest and a disabled control claims there is something to do.
 */
test("no select where neither the workflow nor the corpus offers anything", async () => {
  corpus = () => Promise.resolve([]);
  workflow = () => Promise.reject({ code: "unreachable", message: "no route", source_id: "mock" });
  const screen = render();
  await vi.waitFor(() => expect(workflowCalls).toHaveLength(1));
  await vi.waitFor(() => expect(corpusCalls).toHaveLength(1));
  flushSync();

  expect(screen.select()).toBeNull();

  screen.done();
});

/**
 * Every other kind's detail is unchanged — the select is a ticket's.
 *
 * And unchanged means **neither read is made**. The board read brings back the
 * whole of a source's cards and the workflow read is a round trip to the
 * source; a pull request, a build, a page and a monitor can never show a
 * select, so making either there would be that cost paid for nothing — and an
 * absence nobody asserted is one a later refactor restores without noticing.
 *
 * All four kinds and not one of them, because the gate is *is it a ticket*
 * rather than a list of exceptions: a fixture of one kind would pass against a
 * gate that excluded that one kind by name.
 */
test.each([
  ["pr", "mock:tidewater/payout-service#142", "Retry SEPA payouts"],
  ["build", "mock:build:1187", "Nightly payout build"],
  ["page", "mock:ENG-SEPA", "SEPA retry runbook"],
  ["monitor", "mock:8", "payout-api"],
])("no select and no read on a %s", async (kind, entityId, title) => {
  entity = () =>
    detail({
      row: {
        entity_id: entityId,
        kind,
        source_id: "mock",
        title,
        updated_at: null,
        synced_at: "2026-08-22T14:30:00Z",
        path: null,
        state: null,
      },
    });
  const screen = render(entityId, kind);
  await vi.waitFor(() => expect(screen.text()).toContain(title));
  flushSync();

  expect(screen.select()).toBeNull();
  expect(boardCalls, "a kind with no select must not read the board").toBe(0);
  expect(corpusCalls, "a kind with no select must not read the corpus").toEqual([]);
  expect(workflowCalls, "a kind with no select must not read the workflow").toEqual([]);

  screen.done();
});
