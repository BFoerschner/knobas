/**
 * The ticket detail's status select (#179), and the one rule it lives under:
 * **it never says a status the mirror does not hold.**
 *
 * Picking a status queues a `Transition` through the write queue and stops
 * there. The queue decides send, pend or hold; the source's workflow decides
 * whether the move is legal at all, and refuses by name. So the control goes
 * straight back to the mirrored status after a pick, and the board and this
 * panel both keep saying what the source last said until a sync mirrors the
 * landed write. A select that jumped to the new value would be reporting a
 * hope.
 *
 * Its own file rather than more of `Detail.test.svelte.ts`: that one is about
 * the three answers a read can have, and this needs a loaded kind registry and
 * two commands of its own.
 */
import { flushSync, mount, unmount } from "svelte";
import { beforeEach, expect, test, vi } from "vitest";

import type { EntityDetail, EntityFilter, MiniBoard } from "../ipc/entity";
import type { SourceDescriptor } from "../ipc/sources";

/** Plain functions, not `vi.fn` — see the note in `shell/Tile.test.svelte.ts`. */
const queued: unknown[] = [];
const boardCalls: Pick<EntityFilter, "sources" | "context" | "project">[] = [];
let board: () => Promise<MiniBoard> = () => Promise.resolve(BOARD);
let submitFails = false;

vi.mock("../ipc/entity", () => ({
  listContexts: () => Promise.resolve([]),
  contextMembers: () => Promise.resolve([]),
  createContext: () => Promise.reject(new Error("no context creation in this test")),
  promoteContext: () => Promise.reject(new Error("no promotion in this test")),
  getEntity: () => Promise.resolve(entity()),
  unlink: () => Promise.resolve(),
  createLink: () => Promise.resolve({}),
  miniBoard: (filter: Pick<EntityFilter, "sources" | "context" | "project">) => {
    boardCalls.push(filter);
    return board();
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
        // A getter, because the registry caches the descriptor on its first
        // (and only) `load()`: a plain array would freeze whatever `write_ops`
        // happened to be during the first test in this file, and every later
        // one would silently assert against that.
        get write_ops() {
          return writeOps;
        },
        entity_kinds: [],
        config_schema: {},
      },
    ]),
}));

vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: () => Promise.resolve() }));

const { default: Detail } = await import("./Detail.svelte");
const { toasts } = await import("../shell/toasts.svelte");
const { kindRegistry } = await import("../shell/kind-registry.svelte");

/**
 * `PAY-231` sits *In Progress*, and the board's **columns are narrower than the
 * source's offer** — deliberately. That gap is the whole reason `sources` sits
 * beside `columns` in the granted read (#177): a room where nothing is finished
 * still has to be able to offer *Done*. A fixture where the two agreed would
 * pass just as well against a select that read the offer off the columns, which
 * would then be empty in exactly the room the user needs it in.
 */
const BOARD: MiniBoard = {
  columns: [
    { status: "To Do", cards: [card("PAY-240")] },
    { status: "In Progress", cards: [card("PAY-231")] },
  ],
  sources: [{ source_id: "mock", statuses: ["To Do", "In Progress", "In Review", "Done"] }],
};

function card(key: string) {
  return {
    entity_id: `mock:${key}`,
    source_id: "mock",
    key,
    title: `Title of ${key}`,
    priority: null,
  };
}

function detail(over: Partial<EntityDetail> = {}): EntityDetail {
  return {
    row: {
      entity_id: "mock:PAY-231",
      kind: "ticket",
      source_id: "mock",
      title: "Retry failed SEPA payouts",
      updated_at: "2026-08-22T11:48:00Z",
      synced_at: "2026-08-22T14:30:00Z",
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
  boardCalls.length = 0;
  toasts.items = [];
  submitFails = false;
  writeOps = ["comment", "transition"];
  board = () => Promise.resolve(BOARD);
  entity = () => detail();
  await kindRegistry.load();
});

test("offers the statuses that source's corpus shows, with the mirrored one selected", async () => {
  const screen = render();
  await vi.waitFor(() => expect(screen.select()).not.toBeNull());
  flushSync();

  expect(screen.options()).toEqual(["To Do", "In Progress", "In Review", "Done"]);
  expect(screen.select()?.value).toBe("In Progress");
  // The offer is the source's corpus, not the room's columns — so the read is
  // scoped to the source and to no context (#177), and to no project either
  // (#208): a project room with nothing finished still has to offer Done.
  expect(boardCalls).toEqual([{ sources: ["mock"], context: null, project: null }]);

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
  board = () =>
    Promise.resolve({
      columns: [
        { status: "To Do", cards: [card("PAY-240")] },
        { status: null, cards: [card("PAY-231")] },
      ],
      sources: [{ source_id: "mock", statuses: ["To Do"] }],
    });
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
 * The clearing half of the stale-answer guard, asserted **while the new read is
 * still in flight** — the only window in which the bug exists.
 *
 * A select that kept the previous ticket's status until the new board arrived
 * would be offering a move on one ticket while showing another one's state.
 * Dropping the late answer is not enough on its own and a test that only
 * superseded an in-flight read would never notice: this one lets the first
 * board land, then moves the address.
 */
test("opening another ticket never leaves the previous one's status on screen", async () => {
  const screen = render();
  await vi.waitFor(() => expect(screen.select()?.value).toBe("In Progress"));

  // The second ticket's board has not answered yet.
  let land!: (board: MiniBoard) => void;
  board = () => new Promise<MiniBoard>((resolve) => (land = resolve));
  entity = () =>
    detail({
      row: {
        entity_id: "mock:PAY-240",
        kind: "ticket",
        source_id: "mock",
        title: "Payout dashboard latency",
        updated_at: null,
        synced_at: "2026-08-22T14:30:00Z",
      },
    });
  screen.reopen("mock:PAY-240");
  await vi.waitFor(() => expect(boardCalls).toHaveLength(2));
  flushSync();

  expect(screen.select(), "the previous ticket's select is still on screen").toBeNull();

  land({
    columns: [{ status: "To Do", cards: [card("PAY-240")] }],
    sources: [{ source_id: "mock", statuses: ["To Do", "In Progress"] }],
  });
  await vi.waitFor(() => expect(screen.select()).not.toBeNull());
  expect(screen.select()?.value).toBe("To Do");

  screen.done();
});

/**
 * The dropping half of the same guard: two reads in flight, answered
 * **oldest last**. Without the generation check the first ticket's board would
 * land on the second ticket's panel and offer a move against the wrong status.
 */
test("a slow board answer from the previous ticket is discarded, not shown", async () => {
  let first!: (board: MiniBoard) => void;
  board = () => new Promise<MiniBoard>((resolve) => (first = resolve));
  const screen = render();
  await vi.waitFor(() => expect(boardCalls).toHaveLength(1));

  let second!: (board: MiniBoard) => void;
  board = () => new Promise<MiniBoard>((resolve) => (second = resolve));
  entity = () =>
    detail({
      row: {
        entity_id: "mock:PAY-240",
        kind: "ticket",
        source_id: "mock",
        title: "Payout dashboard latency",
        updated_at: null,
        synced_at: "2026-08-22T14:30:00Z",
      },
    });
  screen.reopen("mock:PAY-240");
  await vi.waitFor(() => expect(boardCalls).toHaveLength(2));

  second({
    columns: [{ status: "To Do", cards: [card("PAY-240")] }],
    sources: [{ source_id: "mock", statuses: ["To Do", "In Progress"] }],
  });
  await vi.waitFor(() => expect(screen.select()?.value).toBe("To Do"));

  // The ticket that is no longer on screen answers last, and loses.
  first(BOARD);
  await Promise.resolve();
  flushSync();
  expect(screen.select()?.value, "the previous ticket's board overwrote this one").toBe("To Do");
  expect(screen.options()).toEqual(["To Do", "In Progress"]);

  screen.done();
});

test("no select where the adapter does not offer the transition op", async () => {
  writeOps = ["comment"];
  const screen = render();
  await vi.waitFor(() => expect(boardCalls).toHaveLength(1));
  flushSync();

  expect(screen.select()).toBeNull();
  expect(screen.text()).toContain("Retry failed SEPA payouts");

  screen.done();
});

test("no select where the source's corpus has shown no status at all", async () => {
  board = () => Promise.resolve({ columns: [], sources: [] });
  const screen = render();
  await vi.waitFor(() => expect(boardCalls).toHaveLength(1));
  flushSync();

  expect(screen.select()).toBeNull();

  screen.done();
});

/**
 * Every other kind's detail is unchanged — the select is a ticket's.
 *
 * And unchanged means the read is not made either. The board read brings back
 * the whole of a source's cards; a note or a page can never show a select, so
 * making it there would be that cost paid for nothing, and an absence nobody
 * asserted is one a later refactor restores without noticing.
 */
test("no select on a kind that is not a ticket", async () => {
  entity = () =>
    detail({
      row: {
        entity_id: "mock:ENG-SEPA",
        kind: "page",
        source_id: "mock",
        title: "SEPA retry runbook",
        updated_at: null,
        synced_at: "2026-08-22T14:30:00Z",
      },
    });
  const screen = render("mock:ENG-SEPA", "page");
  await vi.waitFor(() => expect(screen.text()).toContain("SEPA retry runbook"));
  flushSync();

  expect(screen.select()).toBeNull();
  expect(boardCalls, "a kind with no select must not read the board").toEqual([]);

  screen.done();
});
