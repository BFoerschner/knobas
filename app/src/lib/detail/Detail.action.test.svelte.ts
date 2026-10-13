/**
 * A view's action from the detail (#667, ADR-0020 rule 4): *Merge…* on a pull
 * request queues `MergePullRequest` through the write queue, and the view is
 * read again so the panel stops offering a merge that has happened.
 *
 * The gate is the source's descriptor: an action whose op the source does not
 * declare is not drawn, whatever the view carries.
 */
import { flushSync, mount, unmount } from "svelte";
import { beforeEach, expect, test, vi } from "vitest";

import type { EntityDetail, SourceView } from "../ipc/entity";
import type { SourceDescriptor, WriteOpPayload } from "../ipc/sources";

const ID = "gitea:tidewater/payout-service#142";

/** Every call this panel made, in order. */
const calls: string[] = [];
const queued: WriteOpPayload[] = [];
let submitFails = false;
/** The views `entity_view` answers, one per read; the last repeats. */
let views: SourceView[] = [];

vi.mock("../ipc/entity", () => ({
  miniBoard: () => Promise.resolve({ columns: [], sources: [] }),
  submitWrite: (payload: WriteOpPayload) => {
    calls.push("submit_write");
    if (submitFails) return Promise.reject({ code: "invalid", message: "no such style", source_id: null });
    queued.push(payload);
    return Promise.resolve({ id: 1 });
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
  entityView: (_entityId: string, section: string | null = null) => {
    calls.push(`entity_view ${section}`);
    const next = views.length > 1 ? views.shift()! : views[0]!;
    return Promise.resolve(next);
  },
}));

let writeOps: string[] = [];

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
        // A getter: the registry caches the descriptor on its one `load()`.
        get write_ops() {
          return writeOps;
        },
        entity_kinds: [],
        payload_paths: [],
        config_schema: {},
      },
    ]),
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
  };
}

function overview(state: string, mergeable: boolean): SourceView {
  return {
    sections: [
      {
        id: "overview",
        title: "Overview",
        count: null,
        deferred: false,
        blocks: [{ type: "facts", facts: [{ label: "State", value: state, tone: null }] }],
      },
    ],
    actions: [
      {
        write_op: "merge_pull_request",
        label: "Merge…",
        fields: [
          {
            name: "style",
            label: "Style",
            kind: {
              type: "choice",
              options: [
                { value: "merge", label: "Merge commit" },
                { value: "rebase", label: "Rebase" },
                { value: "squash", label: "Squash" },
              ],
            },
            value: null,
            required: true,
            placeholder: null,
          },
        ],
        confirm: "Merge feature/x into main?",
        primary: true,
        danger: false,
        disabled: mergeable ? null : "Already merged",
        with_settings: false,
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
  return {
    target,
    text: () => (target.textContent ?? "").replace(/\s+/g, " "),
    merge: () => [...target.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent!.trim() === "Merge…"),
    done: () => {
      unmount(app);
      target.remove();
    },
  };
}

function dialogButton(text: string): HTMLButtonElement {
  const dialog = document.querySelector("[role=dialog]")!;
  return [...dialog.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent!.trim() === text)!;
}

beforeEach(async () => {
  calls.length = 0;
  queued.length = 0;
  submitFails = false;
  toasts.items = [];
  writeOps = ["comment", "approve", "merge_pull_request"];
  views = [overview("open", true), overview("merged", false)];
  await kindRegistry.load();
});

test("Merge… queues MergePullRequest with the style picked, then the view is read again", async () => {
  const screen = render();
  await vi.waitFor(() => expect(screen.merge()).toBeDefined());
  screen.merge()!.click();
  flushSync();
  [...document.querySelectorAll<HTMLButtonElement>("[role=radio]")][1]!.click();
  flushSync();
  dialogButton("Merge").click();

  await vi.waitFor(() => expect(screen.text()).toContain("merged"));
  expect(queued).toEqual([{ MergePullRequest: { entity: ID, style: "rebase" } }]);
  expect(calls).toEqual([
    "get_entity",
    "entity_view null",
    "submit_write",
    // The write's own re-read: the mirror and the source's view, both.
    "get_entity",
    "entity_view null",
  ]);
  expect(screen.merge()!.getAttribute("aria-disabled"), "a merged pull request's Merge… says why it is off").toBe(
    "true",
  );
  expect(screen.merge()!.title).toBe("Already merged");
  expect(document.querySelector("[role=dialog]")).toBeNull();
  expect(toasts.items.map((t) => t.text)).toEqual(["Merge queued for tidewater/payout-service#142"]);
  screen.done();
});

test("a source that does not declare the op draws no Merge…, whatever the view says", async () => {
  writeOps = ["comment", "approve"];
  const screen = render();
  await vi.waitFor(() => expect(screen.text()).toContain("open"));
  await new Promise((settle) => setTimeout(settle, 0));
  flushSync();
  expect(screen.merge()).toBeUndefined();
  screen.done();
});

test("a write that could not be queued says why, keeps the dialog, and re-reads nothing", async () => {
  submitFails = true;
  const screen = render();
  await vi.waitFor(() => expect(screen.merge()).toBeDefined());
  screen.merge()!.click();
  flushSync();
  dialogButton("Merge").click();
  await vi.waitFor(() =>
    expect(toasts.items.map((t) => t.text)).toEqual(["Could not queue the merge: no such style"]),
  );
  expect(document.querySelector("[role=dialog]")).not.toBeNull();
  expect(calls).toEqual(["get_entity", "entity_view null", "submit_write"]);
  dialogButton("Cancel").click();
  flushSync();
  screen.done();
});
