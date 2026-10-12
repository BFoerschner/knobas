/**
 * A build configuration's parameter edited from the detail (#668, ADR-0020
 * rule 4): the edit queues `SetParameter` through the write queue, the row is
 * marked until the queue settles, and then the view is read again.
 *
 * "Settles" is the queue's word, not the button's: a write that went is read
 * back at once; one that waits (the source unreachable), is held or refused
 * keeps its mark with the queue's own state until an `activity:new` finds it
 * gone.
 */
import { flushSync, mount, tick, unmount } from "svelte";
import { beforeEach, expect, test, vi } from "vitest";

import type { EntityDetail, SourceView } from "../ipc/entity";
import type { QueuedWrite, SourceDescriptor, WriteOpPayload } from "../ipc/sources";

const ID = "teamcity:buildType:Payout_IntegrationTests";

const calls: string[] = [];
const queued: WriteOpPayload[] = [];
let submitFails = false;
/** What `pending_writes` answers next: the open rows. */
let open: QueuedWrite[] = [];
let views: SourceView[] = [];
/** `activity:new` handlers the panel registered, and how many it dropped. */
const handlers: (() => void)[] = [];
let unlistened = 0;

vi.mock("../ipc/entity", () => ({
  miniBoard: () => Promise.resolve({ columns: [], sources: [] }),
  submitWrite: (payload: WriteOpPayload) => {
    calls.push("submit_write");
    if (submitFails) return Promise.reject({ code: "invalid", message: "no such op", source_id: null });
    queued.push(payload);
    return Promise.resolve(row("pending"));
  },
  listContexts: () => Promise.resolve([]),
  contextMembers: () => Promise.resolve([]),
  createContext: () => Promise.reject(new Error("no context creation in this test")),
  promoteContext: () => Promise.reject(new Error("no promotion in this test")),
  unlink: () => Promise.resolve(),
  createLink: () => Promise.resolve({}),
  getEntity: () => {
    calls.push("get_entity");
    return Promise.resolve(config());
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
        id: "teamcity",
        adapter_kind: "teamcity",
        name: "TeamCity",
        capabilities: [],
        adapter_version: "0",
        auth_methods: [],
        accepts_account: false,
        write_ops: ["trigger_build", "rerun_build", "set_parameter"],
        entity_kinds: [],
        payload_paths: [],
        config_schema: {},
      },
    ]),
  pendingWrites: () => {
    calls.push("pending_writes");
    return Promise.resolve(open);
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
    payload: { SetParameter: { entity: ID, name: "env.SEPA_RETRIES", value: "7" } },
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

function config(): EntityDetail {
  return {
    row: {
      entity_id: ID,
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
  } as EntityDetail;
}

function parameters(retries: string): SourceView {
  return {
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
            entity: ID,
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
  open = [];
  toasts.items = [];
  views = [parameters("5"), parameters("7")];
  await kindRegistry.load();
});

test("an edit queues SetParameter; a write that went re-reads the view at once", async () => {
  const screen = render();
  await settle();
  expect(screen.value("env.SEPA_RETRIES")).toBe("5");

  await screen.edit("env.SEPA_RETRIES", "7");
  expect(queued).toEqual([{ SetParameter: { entity: ID, name: "env.SEPA_RETRIES", value: "7" } }]);
  expect(calls.slice(calls.indexOf("submit_write"))).toEqual([
    "submit_write",
    "pending_writes",
    "get_entity",
    "entity_view",
  ]);
  expect(screen.value("env.SEPA_RETRIES")).toBe("7");
  expect(screen.mark("env.SEPA_RETRIES")).toBe("");
  expect(toasts.items.map((t) => t.text)).toContain("env.SEPA_RETRIES set");
  expect(handlers).toHaveLength(0);
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
  expect(queued).toEqual([{ SetParameter: { entity: ID, name: "env.SEPA_SANDBOX_KEY", value: "brand-new-secret" } }]);
  expect(screen.target.textContent).not.toContain("brand-new-secret");
  expect(toasts.items.map((t) => t.text).join(" ")).not.toContain("brand-new-secret");
  screen.done();
});
