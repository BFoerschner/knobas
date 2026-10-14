/**
 * A build configuration's parameter edited from the detail (#668, ADR-0020
 * rule 4): the edit queues `SetParameter` through the write queue, the row is
 * marked until the queue settles, and then the view is read again.
 *
 * "Settles" is the queue's word, not the button's: a write that went is read
 * back at once; one that waits (the source unreachable), is held or refused
 * keeps its mark with the queue's own state until an `activity:new` finds it
 * gone.
 *
 * And the configuration's *Run…* (#669): the dialog's changed values and
 * branch reach the queue as one `RunBuild`.
 */
import { flushSync, mount, tick, unmount } from "svelte";
import { beforeEach, expect, test, vi } from "vitest";

import type { EntityDetail, SourceView } from "../ipc/entity";
import type { QueuedWrite, SourceDescriptor, WriteOpPayload } from "../ipc/sources";

const ID = "teamcity:buildType:Payout_IntegrationTests";

const calls: string[] = [];
const queued: WriteOpPayload[] = [];
let submitFails = false;
/** When set, the next `submit_write` answers once this resolves. */
let submitGate: Promise<void> | null = null;
/** When set, `pending_writes` fails once, as an IPC call can. */
let pendingFailsOnce = false;
/** What `pending_writes` answers next: the open rows. */
let open: QueuedWrite[] = [];
/** When set, the next `pending_writes` answers what was open when it was asked, once this resolves. */
let pendingGate: Promise<void> | null = null;
let views: SourceView[] = [];
/** When set, the next `entity_view` waits for this before it answers. */
let viewGate: Promise<void> | null = null;
/** The id the next queued write gets. */
let nextId = 41;
/** `activity:new` handlers the panel registered, and how many it dropped. */
const handlers: (() => void)[] = [];
let unlistened = 0;

vi.mock("../ipc/entity", () => ({
  sourceStatuses: () => Promise.resolve([]),
  submitWrite: (payload: WriteOpPayload) => {
    calls.push("submit_write");
    if (submitFails) return Promise.reject({ code: "invalid", message: "no such op", source_id: null });
    queued.push(payload);
    const write = { ...row("pending"), id: nextId++ };
    const gate = submitGate;
    submitGate = null;
    return gate ? gate.then(() => write) : Promise.resolve(write);
  },
  listContexts: () => Promise.resolve([]),
  contextMembers: () => Promise.resolve([]),
  createContext: () => Promise.reject(new Error("no context creation in this test")),
  promoteContext: () => Promise.reject(new Error("no promotion in this test")),
  unlink: () => Promise.resolve(),
  createLink: () => Promise.resolve({}),
  getEntity: (entityId: string) => {
    calls.push("get_entity");
    return Promise.resolve(config(entityId));
  },
  entityView: async () => {
    calls.push("entity_view");
    const next = views.length > 1 ? views.shift()! : views[0]!;
    const gate = viewGate;
    viewGate = null;
    if (gate) await gate;
    return next;
  },
}));

vi.mock("../ipc/sources", () => ({
  listAdapters: (): Promise<SourceDescriptor[]> =>
    Promise.resolve([
      {
        id: "teamcity",
        adapter_kind: "teamcity",
        name: "TeamCity",
        capabilities: [],
        adapter_version: "0",
        auth_methods: [],
        accepts_account: false,
        write_ops: ["trigger_build", "rerun_build", "set_parameter", "run_build"],
        entity_kinds: [],
        payload_paths: [],
        config_schema: {},
      },
    ]),
  pendingWrites: () => {
    calls.push("pending_writes");
    if (pendingFailsOnce) {
      pendingFailsOnce = false;
      return Promise.reject({ code: "internal", message: "pool timed out", source_id: null });
    }
    const gate = pendingGate;
    pendingGate = null;
    const answer = open;
    return gate ? gate.then(() => answer) : Promise.resolve(answer);
  },
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: (_event: string, handler: () => void) => {
    handlers.push(handler);
    return Promise.resolve(() => {
      unlistened += 1;
    });
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

function row(state: QueuedWrite["state"], detail: string | null = null): QueuedWrite {
  return {
    id: 41,
    source_id: "teamcity",
    entity_id: ID,
    op: "set_parameter",
    payload: { SetParameter: { entity: ID, name: "env.SEPA_RETRIES", value: "7", base: "5" } },
    target_snapshot: null,
    state,
    wait_reason: state === "pending" ? "unreachable" : null,
    detail,
    queued_at: "2026-10-12T20:00:00Z",
    attempted_at: null,
    attempts: 1,
    held_snapshot: null,
  } as QueuedWrite;
}

function config(entityId: string = ID): EntityDetail {
  return {
    row: {
      entity_id: entityId,
      kind: "build_config",
      source_id: "teamcity",
      title: "Payout / Integration Tests",
      updated_at: null,
      synced_at: "2026-10-12T14:30:00Z",
      path: null,
      state: null,
    },
    source: { id: "teamcity", display_name: "Tidewater CI", adapter_kind: "teamcity", enabled: true },
    kind_info: null,
    body_text: "Integration Tests",
    author: null,
    payload: { id: "Payout_IntegrationTests" },
    web_url: null,
    deleted_at: null,
    links: [],
    activity: [],
    builds: [],
    status: null,
  } as EntityDetail;
}

function parameters(retries: string, entity: string = ID): SourceView {
  return {
    actions: [],
    sections: [
      {
        id: "parameters",
        title: "Parameters",
        count: 2,
        deferred: false,
        blocks: [
          {
            type: "settings",
            title: null,
            entity,
            write_op: "set_parameter",
            values: [
              { name: "env.SEPA_RETRIES", value: retries, inherited: false, secret: false, editable: true, description: null },
              { name: "env.SEPA_SANDBOX_KEY", value: "", inherited: false, secret: true, editable: true, description: null },
            ],
          },
        ],
      },
    ],
  };
}

async function settle() {
  for (let i = 0; i < 6; i++) {
    await tick();
    await Promise.resolve();
  }
  flushSync();
}

function render() {
  const target = document.createElement("div");
  document.body.append(target);
  const app = mount(Detail, {
    target,
    props: { entityId: ID, kind: "build_config", contextLabel: "All work", onclose: vi.fn(), onnavigate: vi.fn() },
  });
  flushSync();
  const rowOf = (name: string) =>
    [...target.querySelectorAll<HTMLElement>(".sb-row")].find(
      (r) => r.querySelector(".sb-name")!.textContent!.trim() === name,
    )!;
  return {
    target,
    value: (name: string) => rowOf(name).querySelector<HTMLElement>(".sb-val")!.textContent!.trim(),
    mark: (name: string) => rowOf(name).querySelector(".sb-mark")!.textContent!.trim(),
    /** The queue's sentence on a row's mark, drawn as its tooltip. */
    why: (name: string) => rowOf(name).querySelector(".sb-mark")!.getAttribute("title"),
    async edit(name: string, value: string) {
      rowOf(name).querySelector<HTMLButtonElement>("button.sb-val")!.click();
      flushSync();
      await tick();
      const input = target.querySelector<HTMLInputElement>("input.sb-input")!;
      input.value = value;
      input.dispatchEvent(new Event("input", { bubbles: true }));
      input.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
      await settle();
    },
    input: () => target.querySelector<HTMLInputElement>("input.sb-input"),
    done: () => {
      unmount(app);
      target.remove();
    },
  };
}

beforeEach(async () => {
  calls.length = 0;
  queued.length = 0;
  handlers.length = 0;
  unlistened = 0;
  submitFails = false;
  pendingFailsOnce = false;
  open = [];
  viewGate = null;
  pendingGate = null;
  submitGate = null;
  nextId = 41;
  toasts.items = [];
  views = [parameters("5"), parameters("7")];
  await kindRegistry.load();
});

test("an edit queues SetParameter; a write that went re-reads the view at once", async () => {
  const screen = render();
  await settle();
  expect(screen.value("env.SEPA_RETRIES")).toBe("5");

  await screen.edit("env.SEPA_RETRIES", "7");
  // Edited against the value the view drew, which the source checks.
  expect(queued).toEqual([{ SetParameter: { entity: ID, name: "env.SEPA_RETRIES", value: "7", base: "5" } }]);
  expect(calls.slice(calls.indexOf("submit_write"))).toEqual([
    "submit_write",
    "pending_writes",
    "get_entity",
    "entity_view",
  ]);
  expect(screen.value("env.SEPA_RETRIES")).toBe("7");
  expect(screen.mark("env.SEPA_RETRIES")).toBe("");
  expect(toasts.items.map((t) => t.text)).toContain("env.SEPA_RETRIES set");
  // Nothing left open, so nothing left listening.
  expect(handlers.length).toBe(unlistened);
  screen.done();
});

test("a write that waits keeps its mark until the queue lets it go, then the view is read", async () => {
  open = [row("pending", "TeamCity is unreachable")];
  const screen = render();
  await settle();
  await screen.edit("env.SEPA_RETRIES", "7");

  expect(screen.mark("env.SEPA_RETRIES")).toBe("pending");
  expect(screen.value("env.SEPA_RETRIES")).toBe("7");
  expect(calls.filter((c) => c === "entity_view")).toHaveLength(1);
  expect(handlers).toHaveLength(1);

  // Still open on the next queue movement: nothing changes.
  open = [row("held")];
  handlers[0]!();
  await settle();
  expect(screen.mark("env.SEPA_RETRIES")).toBe("held");

  // Gone from the open rows: it went (or was withdrawn). Read the view again.
  open = [];
  handlers[0]!();
  await settle();
  expect(screen.mark("env.SEPA_RETRIES")).toBe("");
  expect(screen.value("env.SEPA_RETRIES")).toBe("7");
  expect(calls.filter((c) => c === "entity_view")).toHaveLength(2);
  expect(unlistened).toBe(1);
  screen.done();
});

test("a queue read that fails after the save still settles on the next queue movement", async () => {
  const screen = render();
  await settle();
  // Armed after the view drew: the open-time read (#692) is not the one failing.
  pendingFailsOnce = true;
  await screen.edit("env.SEPA_RETRIES", "7");
  // The first read failed: where the write is is unknown, so the row says saving.
  expect(screen.mark("env.SEPA_RETRIES")).toBe("saving…");
  // Listening already, so the next movement asks again and finds it gone.
  expect(handlers.length - unlistened).toBe(1);
  handlers[0]!();
  await settle();
  expect(screen.mark("env.SEPA_RETRIES")).toBe("");
  expect(screen.value("env.SEPA_RETRIES")).toBe("7");
  screen.done();
});

test("a write the queue would not take leaves the editor open and says why", async () => {
  submitFails = true;
  const screen = render();
  await settle();
  await screen.edit("env.SEPA_RETRIES", "7");
  expect(screen.input()?.value).toBe("7");
  expect(toasts.items.map((t) => t.text)).toContain("Could not save env.SEPA_RETRIES: no such op");
  screen.done();
});

test("a secret's toast and mark never carry its value", async () => {
  open = [row("pending")];
  const screen = render();
  await settle();
  await screen.edit("env.SEPA_SANDBOX_KEY", "brand-new-secret");
  // No base: a secret's value was never drawn, so nothing was edited against.
  expect(queued).toEqual([
    { SetParameter: { entity: ID, name: "env.SEPA_SANDBOX_KEY", value: "brand-new-secret", base: null } },
  ]);
  expect(screen.target.textContent).not.toContain("brand-new-secret");
  expect(toasts.items.map((t) => t.text).join(" ")).not.toContain("brand-new-secret");
  screen.done();
});

test("a row saved again while a settled write re-reads the view keeps its newer mark", async () => {
  open = [row("pending")];
  const screen = render();
  await settle();
  await screen.edit("env.SEPA_RETRIES", "7");
  expect(screen.mark("env.SEPA_RETRIES")).toBe("pending");

  // The first write goes; its re-read of the view is held open...
  let release!: () => void;
  viewGate = new Promise((resolve) => (release = resolve));
  open = [];
  handlers[0]!();
  await settle();

  // ...while the reader saves the same row again, and that write waits.
  open = [{ ...row("pending"), id: 42 }];
  await screen.edit("env.SEPA_RETRIES", "8");
  release();
  await settle();
  await settle();

  expect(screen.mark("env.SEPA_RETRIES")).toBe("pending");
  expect(screen.value("env.SEPA_RETRIES")).toBe("8");
  screen.done();
});

// -- a detail reopened while its writes are still queued (#692) -------------

/** A queued `set_parameter` of `name` to `value`, as the queue lists it. */
function setting(id: number, state: QueuedWrite["state"], name: string, value: string, detail: string | null) {
  return { ...row(state, detail), id, payload: { SetParameter: { entity: ID, name, value, base: null } } } as QueuedWrite;
}

test("a view opened with a pending write for one of its parameters marks that row until the queue settles", async () => {
  open = [row("pending", "TeamCity is unreachable")];
  const screen = render();
  await settle();

  // Nothing was saved from this panel; the mark comes from the queue.
  expect(calls).not.toContain("submit_write");
  expect(screen.mark("env.SEPA_RETRIES")).toBe("pending");
  expect(screen.value("env.SEPA_RETRIES")).toBe("7");
  expect(screen.mark("env.SEPA_SANDBOX_KEY")).toBe("");
  expect(handlers.length - unlistened).toBe(1);

  open = [];
  handlers[0]!();
  await settle();
  expect(screen.mark("env.SEPA_RETRIES")).toBe("");
  expect(screen.value("env.SEPA_RETRIES")).toBe("7");
  expect(calls.filter((c) => c === "entity_view")).toHaveLength(2);
  expect(handlers.length).toBe(unlistened);
  screen.done();
});

test("held and refused writes draw the queue's state, with its sentence", async () => {
  open = [
    setting(51, "held", "env.SEPA_RETRIES", "9", 'env.SEPA_RETRIES changed since you edited: now "6"'),
    setting(52, "refused", "env.SEPA_SANDBOX_KEY", "leaked-if-drawn", "403 Forbidden"),
  ];
  views = [parameters("5")];
  const screen = render();
  await settle();

  expect(screen.mark("env.SEPA_RETRIES")).toBe("held");
  expect(screen.why("env.SEPA_RETRIES")).toBe('env.SEPA_RETRIES changed since you edited: now "6"');
  expect(screen.value("env.SEPA_RETRIES")).toBe("9");
  expect(screen.mark("env.SEPA_SANDBOX_KEY")).toBe("refused");
  expect(screen.why("env.SEPA_SANDBOX_KEY")).toBe("403 Forbidden");
  // A secret's queued value is never drawn.
  expect(screen.target.textContent).not.toContain("leaked-if-drawn");
  screen.done();
});

test("only an open write of this block's op, on its entity, for one of its rows marks a row", async () => {
  const other = "teamcity:buildType:Payout_Smoke";
  open = [
    {
      ...setting(61, "pending", "env.SEPA_RETRIES", "1", null),
      entity_id: other,
      payload: { SetParameter: { entity: other, name: "env.SEPA_RETRIES", value: "1", base: null } },
    },
    { ...row("pending"), id: 62, op: "trigger_build", payload: { TriggerBuild: { entity: ID } } },
    setting(63, "pending", "env.NOT_ON_THIS_VIEW", "2", null),
    // A sent row has left the queue in all but name.
    setting(64, "sent", "env.SEPA_RETRIES", "3", null),
  ];
  views = [parameters("5")];
  const screen = render();
  await settle();

  expect(screen.mark("env.SEPA_RETRIES")).toBe("");
  expect(screen.value("env.SEPA_RETRIES")).toBe("5");
  // No row is marked. The pending build on this entity is the one write
  // watched: since #752 a write on its way is drawn as one (its own line),
  // and it is the queue's settling of it that ends the watch.
  expect(screen.target.textContent).toContain("Build");
  expect(handlers.length - unlistened).toBe(1);
  screen.done();
});

test("of two open writes for one row, the newer one marks it", async () => {
  open = [
    // Listed newest first, so the row is chosen by id, not by order.
    setting(72, "pending", "env.SEPA_RETRIES", "8", null),
    setting(71, "refused", "env.SEPA_RETRIES", "6", "first try"),
  ];
  views = [parameters("5")];
  const screen = render();
  await settle();
  expect(screen.mark("env.SEPA_RETRIES")).toBe("pending");
  expect(screen.value("env.SEPA_RETRIES")).toBe("8");
  screen.done();
});

test("a row saved before the queue's first answer keeps its own write's mark", async () => {
  // The queue the panel opens over: an older refused edit of the row.
  open = [setting(31, "refused", "env.SEPA_RETRIES", "6", "first try")];
  let release!: () => void;
  pendingGate = new Promise((resolve) => (release = resolve));
  views = [parameters("5")];
  const screen = render();
  await settle();

  // The reader saves the row before that read answers; the new write waits.
  open = [setting(31, "refused", "env.SEPA_RETRIES", "6", "first try"), setting(41, "pending", "env.SEPA_RETRIES", "8", null)];
  await screen.edit("env.SEPA_RETRIES", "8");
  expect(screen.mark("env.SEPA_RETRIES")).toBe("pending");

  // The open-time read lands late, with only the older write in it.
  release();
  await settle();
  expect(screen.mark("env.SEPA_RETRIES")).toBe("pending");
  expect(screen.value("env.SEPA_RETRIES")).toBe("8");
  screen.done();
});

test("a row whose save is still being queued when the queue's first answer lands keeps its save", async () => {
  // The queue the panel opens over: an older refused edit of the row.
  open = [setting(31, "refused", "env.SEPA_RETRIES", "6", "first try")];
  let answerQueue!: () => void;
  pendingGate = new Promise((resolve) => (answerQueue = resolve));
  views = [parameters("5")];
  const screen = render();
  await settle();

  // The reader saves the row; the write is not on the queue yet.
  let answerSave!: () => void;
  submitGate = new Promise((resolve) => (answerSave = resolve));
  await screen.edit("env.SEPA_RETRIES", "8");
  expect(screen.mark("env.SEPA_RETRIES")).toBe("saving…");

  // The open-time read lands while the save is in flight: the older write does not take the row.
  answerQueue();
  await settle();
  expect(screen.mark("env.SEPA_RETRIES")).toBe("saving…");

  open = [setting(31, "refused", "env.SEPA_RETRIES", "6", "first try"), setting(41, "pending", "env.SEPA_RETRIES", "8", null)];
  answerSave();
  await settle();
  expect(screen.mark("env.SEPA_RETRIES")).toBe("pending");
  expect(screen.value("env.SEPA_RETRIES")).toBe("8");
  screen.done();
});

// --- Run… (#669) ------------------------------------------------------------

/** The configuration's view with *Run…* on its overview beside its parameters. */
function runnable(): SourceView {
  return {
    sections: [
      {
        id: "overview",
        title: "Overview",
        count: null,
        deferred: false,
        blocks: [],
      },
      ...parameters("5").sections,
    ],
    actions: [
      {
        write_op: "run_build",
        label: "Run…",
        fields: [
          {
            name: "branch",
            label: "Branch",
            kind: { type: "text", suggestions: ["feature/PAY-231-sepa-retry"] },
            value: null,
            required: false,
            placeholder: "default branch",
          },
        ],
        confirm: "Queue one build of this configuration.",
        primary: true,
        danger: false,
        disabled: null,
        with_settings: true,
        values: {},
        entity: null,
        line_comments: "none",
      },
    ],
  };
}

test("Run… queues one RunBuild: no branch for the default, and only the value changed", async () => {
  views = [runnable()];
  const screen = render();
  await settle();
  const run = [...screen.target.querySelectorAll<HTMLButtonElement>("button")].find(
    (b) => b.textContent!.trim() === "Run…",
  )!;
  run.click();
  flushSync();
  const dialog = document.querySelector<HTMLElement>("[role=dialog]")!;
  const retries = dialog.querySelector<HTMLInputElement>('input[aria-label="env.SEPA_RETRIES"]')!;
  retries.value = "9";
  retries.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
  [...dialog.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent!.trim() === "Run")!.click();
  await settle();

  expect(queued).toEqual([
    { RunBuild: { entity: ID, branch: null, parameters: [{ name: "env.SEPA_RETRIES", value: "9" }] } },
  ]);
  expect(toasts.items.map((t) => t.text)).toEqual([expect.stringMatching(/^Run queued for /)]);
  expect(document.querySelector("[role=dialog]")).toBeNull();
  screen.done();
});

// --- close on queue (#775) --------------------------------------------------

test("the editor closes on the queue's answer, and the row says saving until the write settles", async () => {
  const screen = render();
  await settle();
  // The queue's answer arrives; the read of where the write is does not.
  let answer!: () => void;
  pendingGate = new Promise((resolve) => (answer = resolve));
  open = [{ ...row("pending"), wait_reason: null }];
  await screen.edit("env.SEPA_RETRIES", "7");

  // Closed, though nothing after the queue's answer has come back.
  expect(screen.input()).toBeNull();
  expect(calls.slice(calls.indexOf("submit_write"))).toEqual(["submit_write", "pending_writes"]);
  expect(screen.mark("env.SEPA_RETRIES")).toBe("saving…");
  expect(screen.value("env.SEPA_RETRIES")).toBe("7");
  expect(toasts.items.map((t) => t.text)).toEqual(["env.SEPA_RETRIES queued"]);

  // On its way with no reason to wait is still saving, not "pending".
  answer();
  await settle();
  expect(screen.mark("env.SEPA_RETRIES")).toBe("saving…");
  // And the row can be edited again meanwhile: nothing is disabled while a send runs.
  const button = screen.target.querySelector<HTMLButtonElement>('button[aria-label="Edit env.SEPA_RETRIES"]')!;
  expect(button.disabled).toBe(false);

  // It went: the view is read again in the background and the mark goes.
  open = [];
  handlers[0]!();
  await settle();
  expect(screen.mark("env.SEPA_RETRIES")).toBe("");
  expect(toasts.items.map((t) => t.text)).toContain("env.SEPA_RETRIES set");
  screen.done();
});

test("a saved write that is held or refused says why once, and Edit and resend reopens the row with the value typed", async () => {
  const screen = render();
  await settle();
  open = [row("pending")];
  await screen.edit("env.SEPA_RETRIES", "7");
  expect(toasts.items.map((t) => t.text)).toEqual(["env.SEPA_RETRIES queued"]);

  open = [row("refused", "403 Forbidden")];
  handlers[0]!();
  await settle();
  const told = toasts.items.filter((t) => t.tone === "err");
  expect(told.map((t) => t.text)).toEqual(["env.SEPA_RETRIES was refused: 403 Forbidden"]);
  expect(told[0]!.action?.label).toBe("Open write queue");
  expect(told[0]!.more?.map((a) => a.label)).toEqual(["Edit and resend"]);
  // The inline mark stays.
  expect(screen.mark("env.SEPA_RETRIES")).toBe("refused");

  // Told once: the next movement with the write still refused is not news.
  handlers[0]!();
  await settle();
  expect(toasts.items.filter((t) => t.tone === "err")).toHaveLength(1);

  told[0]!.more![0]!.run();
  await settle();
  expect(screen.input()?.value).toBe("7");
  screen.done();
});

test("a write found held when the view opens is marked but not toasted", async () => {
  open = [setting(51, "held", "env.SEPA_RETRIES", "9", "changed since")];
  views = [parameters("5")];
  const screen = render();
  await settle();
  expect(screen.mark("env.SEPA_RETRIES")).toBe("held");
  // A queue movement with it still held: it was not saved here, so it is not news.
  expect(handlers.length - unlistened).toBe(1);
  handlers.at(-1)!();
  await settle();
  expect(screen.mark("env.SEPA_RETRIES")).toBe("held");
  expect(toasts.items).toEqual([]);
  screen.done();
});

test("Edit and resend from a toast that outlived its item opens no row of the item now open", async () => {
  const target = document.createElement("div");
  document.body.append(target);
  const OTHER = "teamcity:buildType:Payout_Nightly";
  const props = $state({ entityId: ID, kind: "build_config", contextLabel: "All work", onclose: vi.fn(), onnavigate: vi.fn() });
  views = [parameters("5")];
  const app = mount(Detail, { target, props });
  await settle();
  open = [row("pending")];
  target.querySelector<HTMLButtonElement>(".sb-row button.sb-val")!.click();
  flushSync();
  await tick();
  const input = target.querySelector<HTMLInputElement>("input.sb-input")!;
  input.value = "7";
  input.dispatchEvent(new Event("input", { bubbles: true }));
  input.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
  await settle();
  open = [row("refused", "403 Forbidden")];
  handlers[0]!();
  await settle();
  const told = toasts.items.find((t) => t.tone === "err")!;

  // Another build configuration with a row of the same name is open now.
  views = [parameters("3", OTHER)];
  props.entityId = OTHER;
  await settle();
  told.more![0]!.run();
  await settle();
  expect(target.querySelector("input.sb-input")).toBeNull();
  unmount(app);
  target.remove();
});
