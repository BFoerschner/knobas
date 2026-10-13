/**
 * Writes go in the background (#752): the dialog closes once the write is
 * queued, the view draws it as on its way, and the queue's settle is what
 * re-reads the view -- or, for a write refused, takes the pending item away
 * and offers the dialog again with what was typed.
 *
 * The queue is played by `pending_writes` and the `activity:new` handler the
 * panel registers: the test decides where the write is, then announces a
 * movement, as the backend does on every transition.
 */
import { flushSync, mount, unmount } from "svelte";
import { beforeEach, expect, test, vi } from "vitest";

import type { EntityDetail, SourceView, ViewSection } from "../ipc/entity";
import type { QueuedWrite, SourceDescriptor, WriteOpPayload } from "../ipc/sources";

const ID = "gitea:tidewater/payout-service#142";

const calls: string[] = [];
const queued: WriteOpPayload[] = [];
/** The views `entity_view` answers, one per read; the last repeats. */
let views: SourceView[] = [];
/** What `pending_writes` answers. */
let open: QueuedWrite[] = [];
const handlers: (() => void)[] = [];

/** The id the queue gives the next write. */
let nextId = 7;

function row(
  payload: WriteOpPayload,
  state: QueuedWrite["state"] = "pending",
  detail: string | null = null,
  id = 7,
): QueuedWrite {
  return {
    id,
    source_id: "gitea",
    entity_id: ID,
    op: "comment",
    payload,
    target_snapshot: null,
    state,
    wait_reason: null,
    detail,
    queued_at: "2026-10-13T10:00:00Z",
    attempted_at: null,
    attempts: 0,
    held_snapshot: null,
    settled_at: null,
    source_enabled: true,
  };
}

vi.mock("../ipc/entity", () => ({
  miniBoard: () => Promise.resolve({ columns: [], sources: [] }),
  submitWrite: (payload: WriteOpPayload) => {
    calls.push("submit_write");
    queued.push(payload);
    // The queue's answer: the row as queued, before any send.
    const answer = row(payload, "pending", null, nextId++);
    open = [...open, answer];
    return Promise.resolve(answer);
  },
  listContexts: () => Promise.resolve([]),
  contextMembers: () => Promise.resolve([]),
  createContext: () => Promise.reject(new Error("no context creation in this test")),
  promoteContext: () => Promise.reject(new Error("no promotion in this test")),
  unlink: () => Promise.resolve(),
  createLink: () => Promise.resolve({}),
  getEntity: () => {
    calls.push("get_entity");
    return Promise.resolve(pull());
  },
  entityView: () => {
    calls.push("entity_view");
    const next = views.length > 1 ? views.shift()! : views[0]!;
    return Promise.resolve(next);
  },
}));

vi.mock("../ipc/sources", () => ({
  listAdapters: (): Promise<SourceDescriptor[]> =>
    Promise.resolve([
      {
        id: "gitea",
        adapter_kind: "gitea",
        name: "Gitea",
        capabilities: [],
        adapter_version: "0",
        auth_methods: [],
        accepts_account: false,
        write_ops: ["comment", "merge_pull_request"],
        entity_kinds: [],
        payload_paths: [],
        config_schema: {},
      },
    ]),
  pendingWrites: () => Promise.resolve(open),
  discardWrite: (id: number) => {
    calls.push(`discard_write ${id}`);
    open = open.filter((w) => w.id !== id);
    return Promise.resolve();
  },
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: (_event: string, handler: () => void) => {
    handlers.push(handler);
    return Promise.resolve(() => {});
  },
}));

vi.mock("../ipc/search", () => ({
  search: () => Promise.reject(new Error("the picker is not this file's business")),
  launcherHome: () => Promise.reject(new Error("the dialog never loads the board")),
  noFilters: () => ({ sources: [], kinds: [], updated_within_days: null, mine: false, authors: [] }),
}));

vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: () => Promise.resolve() }));

const { default: Detail } = await import("./Detail.svelte");
const { toasts } = await import("../shell/toasts.svelte");
const { kindRegistry } = await import("../shell/kind-registry.svelte");
const { writeQueuePanel } = await import("../shell/write-queue.svelte");

function pull(): EntityDetail {
  return {
    row: {
      entity_id: ID,
      kind: "pr",
      source_id: "gitea",
      title: "SEPA retry with exponential backoff",
      updated_at: "2026-08-22T11:42:00Z",
      synced_at: "2026-08-22T14:30:00Z",
      path: null,
      state: null,
    },
    source: { id: "gitea", display_name: "Tidewater Git", adapter_kind: "gitea", enabled: true },
    kind_info: null,
    body_text: "SEPA retry",
    author: "mara",
    payload: { number: 142 },
    web_url: null,
    deleted_at: null,
    links: [],
    activity: [],
    builds: [],
  };
}

/** The conversation as the source draws it: who and when, then the words. */
function conversation(said: [string, string][]): ViewSection {
  return {
    id: "conversation",
    title: "Conversation",
    count: said.length,
    deferred: false,
    blocks: said.flatMap(([who, words]) => [
      { type: "facts" as const, facts: [{ label: who, value: "today", tone: null }] },
      { type: "text" as const, text: words, markdown: true },
    ]),
  };
}

function view(said: [string, string][], state = "open"): SourceView {
  return {
    sections: [
      {
        id: "overview",
        title: "Overview",
        count: null,
        deferred: false,
        blocks: [{ type: "facts", facts: [{ label: "State", value: state, tone: null }] }],
      },
      conversation(said),
    ],
    actions: [
      {
        write_op: "comment",
        label: "Comment…",
        fields: [{ name: "body", label: "Comment", kind: { type: "long_text" }, value: null, required: true, placeholder: null }],
        confirm: null,
        primary: false,
        danger: false,
        disabled: null,
        with_settings: false,
        values: {},
        entity: null,
        line_comments: "none",
      },
      {
        write_op: "merge_pull_request",
        label: "Merge…",
        fields: [],
        confirm: "Merge feature/x into main?",
        primary: true,
        danger: false,
        disabled: null,
        with_settings: false,
        values: { style: "merge" },
        entity: null,
        line_comments: "none",
      },
    ],
  };
}

function render() {
  const target = document.createElement("div");
  document.body.append(target);
  const app = mount(Detail, {
    target,
    props: { entityId: ID, kind: "pr", contextLabel: "All work", onclose: vi.fn(), onnavigate: vi.fn() },
  });
  flushSync();
  const button = (label: string) =>
    [...target.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent!.trim() === label);
  return {
    target,
    text: () => (target.textContent ?? "").replace(/\s+/g, " "),
    button,
    tab: (title: string) =>
      [...target.querySelectorAll<HTMLButtonElement>("[role=tab]")].find((b) => b.textContent!.trim().startsWith(title))!,
    sending: () => [...target.querySelectorAll("[aria-label=Sending]")].map((e) => (e.textContent ?? "").replace(/\s+/g, " ").trim()),
    done: () => {
      unmount(app);
      target.remove();
    },
  };
}

function dialog(): HTMLElement | null {
  return document.querySelector<HTMLElement>("[role=dialog]");
}

function dialogButton(text: string): HTMLButtonElement {
  return [...dialog()!.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent!.trim() === text)!;
}

async function comment(screen: ReturnType<typeof render>, words: string) {
  await vi.waitFor(() => expect(screen.button("Comment…")).toBeDefined());
  screen.button("Comment…")!.click();
  flushSync();
  const box = dialog()!.querySelector<HTMLTextAreaElement>("textarea")!;
  box.value = words;
  box.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
  dialogButton("Comment").click();
}

/** Announce a queue movement, as `activity:new` does, and let the panel answer. */
async function movement() {
  for (const handler of handlers) handler();
  await new Promise((resolve) => setTimeout(resolve, 0));
  flushSync();
}

beforeEach(async () => {
  calls.length = 0;
  queued.length = 0;
  handlers.length = 0;
  open = [];
  nextId = 7;
  toasts.items = [];
  writeQueuePanel.open = false;
  await kindRegistry.load();
});

test("the comment dialog closes before the write settles, and the comment is drawn as on its way", async () => {
  views = [view([["mara", "Looks right."]])];
  const screen = render();
  await comment(screen, "Bound the jitter.");

  // Closed on the queue's answer; the queue still says pending.
  await vi.waitFor(() => expect(dialog()).toBeNull());
  expect(open.map((w) => w.state)).toEqual(["pending"]);
  expect(calls.filter((c) => c === "entity_view"), "nothing is re-read before the write settles").toHaveLength(1);

  screen.tab("Conversation").click();
  flushSync();
  expect(screen.sending()).toEqual(["You sending… Bound the jitter."]);
  expect(screen.text()).toContain("Looks right.");
  screen.done();
});

test("on settle the view is read again and the real comment replaces the pending one", async () => {
  views = [view([["mara", "Looks right."]]), view([["mara", "Looks right."], ["bjoern", "Bound the jitter."]])];
  const screen = render();
  await comment(screen, "Bound the jitter.");
  await vi.waitFor(() => expect(dialog()).toBeNull());
  screen.tab("Conversation").click();
  flushSync();
  await vi.waitFor(() => expect(screen.sending()).toHaveLength(1));

  // Sent: the row has left the queue.
  open = [];
  await movement();
  await vi.waitFor(() => expect(screen.sending()).toEqual([]));
  expect(calls.filter((c) => c === "entity_view")).toHaveLength(2);
  expect(screen.text()).toContain("bjoern");
  expect(screen.text().match(/Bound the jitter\./g)).toHaveLength(1);
  // The reader is still on the tab they opened.
  expect(screen.tab("Conversation").getAttribute("aria-selected")).toBe("true");
  screen.done();
});

test("a refused comment loses its pending item and offers Edit and resend with the words", async () => {
  views = [view([])];
  const screen = render();
  await comment(screen, "Bound the jitter.");
  await vi.waitFor(() => expect(dialog()).toBeNull());
  screen.tab("Conversation").click();
  flushSync();
  await vi.waitFor(() => expect(screen.sending()).toHaveLength(1));

  open = [{ ...open[0]!, state: "refused", detail: "403 Forbidden" }];
  await movement();
  await vi.waitFor(() => expect(screen.sending()).toEqual([]));
  const told = toasts.items.find((t) => t.tone === "err")!;
  expect(told.text).toBe("Comment was refused: 403 Forbidden");
  expect([told.action?.label, ...(told.more ?? []).map((a) => a.label)]).toEqual([
    "Open write queue",
    "Edit and resend",
  ]);
  expect(calls.filter((c) => c === "entity_view"), "a refused write changed nothing to re-read").toHaveLength(1);

  told.more![0]!.run();
  flushSync();
  expect(dialog()!.querySelector<HTMLTextAreaElement>("textarea")!.value).toBe("Bound the jitter.");
  told.action!.run();
  expect(writeQueuePanel.open).toBe(true);
  dialogButton("Cancel").click();
  flushSync();
  screen.done();
});

test("Edit and resend replaces the refused write: one write in the queue, not two", async () => {
  views = [view([])];
  const screen = render();
  await comment(screen, "Bound the jitter.");
  await vi.waitFor(() => expect(dialog()).toBeNull());
  const refused = open[0]!;
  open = [{ ...refused, state: "refused", detail: "403 Forbidden" }];
  await movement();
  const told = await vi.waitFor(() => {
    const toast = toasts.items.find((t) => t.tone === "err");
    expect(toast).toBeDefined();
    return toast!;
  });

  told.more![0]!.run();
  flushSync();
  const box = dialog()!.querySelector<HTMLTextAreaElement>("textarea")!;
  box.value = "Bound the jitter, with the trace.";
  box.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
  dialogButton("Comment").click();
  await vi.waitFor(() => expect(dialog()).toBeNull());

  await vi.waitFor(() => expect(calls).toContain(`discard_write ${refused.id}`));
  expect(calls.indexOf(`discard_write ${refused.id}`), "withdrawn only once its successor is queued").toBeGreaterThan(
    calls.lastIndexOf("submit_write"),
  );
  expect(open.map((w) => [w.state, w.payload])).toEqual([
    ["pending", { Comment: { entity: ID, body: "Bound the jitter, with the trace." } }],
  ]);
  screen.done();
});

test("a merge on its way shows the state it will leave in the header", async () => {
  views = [view([])];
  const screen = render();
  await vi.waitFor(() => expect(screen.button("Merge…")).toBeDefined());
  screen.button("Merge…")!.click();
  flushSync();
  dialogButton("Merge").click();
  await vi.waitFor(() => expect(dialog()).toBeNull());
  await vi.waitFor(() => expect(screen.target.querySelector(".sending-chip")?.textContent).toContain("merged"));
  expect(screen.target.querySelector(".sending-chip")!.textContent).toContain("sending…");

  open = [];
  await movement();
  await vi.waitFor(() => expect(screen.target.querySelector(".sending-chip")).toBeNull());
  screen.done();
});

test("a write still pending when the panel opens is drawn from the queue", async () => {
  open = [row({ Comment: { entity: ID, body: "Written before the restart." } })];
  views = [view([])];
  const screen = render();
  await vi.waitFor(() => expect(screen.tab("Conversation")).toBeDefined());
  screen.tab("Conversation").click();
  flushSync();
  await vi.waitFor(() => expect(screen.sending()).toEqual(["You sending… Written before the restart."]));
  screen.done();
});
