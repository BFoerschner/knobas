/**
 * A branch's life from its view (#707, spec #701 workflows 2, 4 and 10): its
 * open pull request is the first row of its overview and opens that pull
 * request's detail; *New branch…* and *Open pull request…* queue their
 * creates against the **repository** the action names; *Delete branch…* is a
 * danger that asks, then queues the delete of the branch itself.
 */
import { flushSync, mount, unmount } from "svelte";
import { beforeEach, expect, test, vi } from "vitest";

import type { EntityDetail, SourceView, ViewAction } from "../ipc/entity";
import type { SourceDescriptor, WriteOpPayload } from "../ipc/sources";

const REPO = "gitea:tidewater/payout-service";
const BRANCH = "gitea:tidewater/payout-service@refs/heads/feature/PAY-231-sepa-retry";
const PULL = "gitea:tidewater/payout-service#142";

const queued: WriteOpPayload[] = [];
let view: SourceView;

vi.mock("../ipc/entity", () => ({
  miniBoard: () => Promise.resolve({ columns: [], sources: [] }),
  submitWrite: (payload: WriteOpPayload) => {
    queued.push(payload);
    return Promise.resolve({ id: 1 });
  },
  listContexts: () => Promise.resolve([]),
  contextMembers: () => Promise.resolve([]),
  createContext: () => Promise.reject(new Error("no context creation in this test")),
  promoteContext: () => Promise.reject(new Error("no promotion in this test")),
  unlink: () => Promise.resolve(),
  createLink: () => Promise.resolve({}),
  getEntity: () => Promise.resolve(branch()),
  entityView: () => Promise.resolve(view),
  // A branch draws its checkout panel; none is found here.
  entityCheckout: () => Promise.resolve(null),
  checkoutCommands: () => Promise.resolve([]),
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
        write_ops: ["create_branch", "create_pull_request", "comment", "approve", "merge_pull_request", "delete_branch"],
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

function branch(): EntityDetail {
  return {
    row: {
      entity_id: BRANCH,
      kind: "branch",
      source_id: "gitea",
      title: "feature/PAY-231-sepa-retry",
      updated_at: "2026-08-22T11:42:00Z",
      synced_at: "2026-08-22T14:30:00Z",
      path: "tidewater/payout-service",
      state: null,
    },
    source: { id: "gitea", display_name: "Tidewater Git", adapter_kind: "gitea", enabled: true },
    kind_info: null,
    body_text: "",
    author: null,
    payload: {},
    web_url: null,
    deleted_at: null,
    links: [],
    activity: [],
  };
}

function act(write_op: string, label: string, more: Partial<ViewAction>): ViewAction {
  return {
    write_op,
    label,
    fields: [],
    confirm: null,
    primary: false,
    danger: false,
    disabled: null,
    with_settings: false,
    values: {},
    entity: null,
    ...more,
  };
}

const NEW_BRANCH = act("create_branch", "New branch…", {
  entity: REPO,
  fields: [
    { name: "name", label: "Name", kind: { type: "text", suggestions: [] }, value: null, required: true, placeholder: null },
    {
      name: "from_ref",
      label: "From",
      kind: { type: "choice", options: [{ value: "feature/PAY-231-sepa-retry", label: "feature/PAY-231-sepa-retry" }] },
      value: null,
      required: true,
      placeholder: null,
    },
  ],
});

const OPEN_PULL = act("create_pull_request", "Open pull request…", {
  entity: REPO,
  primary: true,
  fields: [
    { name: "title", label: "Title", kind: { type: "text", suggestions: [] }, value: "PAY-231 sepa retry", required: true, placeholder: null },
    { name: "body", label: "Description", kind: { type: "long_text" }, value: "- one\n- two", required: false, placeholder: null },
    {
      name: "head",
      label: "Head",
      kind: { type: "choice", options: [{ value: "feature/PAY-231-sepa-retry", label: "feature/PAY-231-sepa-retry" }] },
      value: null,
      required: true,
      placeholder: null,
    },
    {
      name: "base",
      label: "Base",
      kind: {
        type: "choice",
        options: [
          { value: "main", label: "main" },
          { value: "release/2026.08", label: "release/2026.08" },
        ],
      },
      value: null,
      required: true,
      placeholder: null,
    },
  ],
});

const DELETE = act("delete_branch", "Delete branch…", {
  danger: true,
  confirm: "Delete feature/PAY-231-sepa-retry? Not merged into main: 2 commits will only be reachable by SHA.",
});

function overview(first: SourceView["sections"][number]["blocks"], actions: ViewAction[]): SourceView {
  return {
    sections: [
      {
        id: "overview",
        title: "Overview",
        count: null,
        deferred: false,
        blocks: [...first, { type: "facts", facts: [{ label: "Base", value: "main", tone: null }] }],
      },
    ],
    actions,
  };
}

function render() {
  const target = document.createElement("div");
  document.body.append(target);
  const onnavigate = vi.fn();
  const app = mount(Detail, {
    target,
    props: { entityId: BRANCH, kind: "branch", contextLabel: "All work", onclose: vi.fn(), onnavigate },
  });
  flushSync();
  return {
    target,
    onnavigate,
    button: (label: string) =>
      [...target.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent!.trim() === label),
    done: () => {
      unmount(app);
      target.remove();
    },
  };
}

function dialog(): HTMLElement {
  return document.querySelector<HTMLElement>("[role=dialog]")!;
}

function dialogButton(text: string): HTMLButtonElement {
  return [...dialog().querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent!.trim() === text)!;
}

beforeEach(async () => {
  queued.length = 0;
  toasts.items = [];
  await kindRegistry.load();
});

test("a branch with an open pull request opens on it, and its row opens that pull request", async () => {
  view = overview(
    [
      {
        type: "entities",
        title: "Pull request",
        rows: [
          {
            entity_id: PULL,
            title: "SEPA retry with exponential backoff",
            meta: "#142 · feature/PAY-231-sepa-retry → main · mara",
            tone: "info",
            chip: "open",
            url: null,
          },
        ],
      },
    ],
    [NEW_BRANCH, DELETE],
  );
  const screen = render();
  await vi.waitFor(() => expect(screen.button("New branch…")).toBeDefined());
  expect(screen.button("Open pull request…"), "a second pull request is not offered").toBeUndefined();
  const panel = screen.target.querySelector(".eb-row button") as HTMLButtonElement;
  expect(panel.textContent!.trim()).toBe("SEPA retry with exponential backoff");
  panel.click();
  expect(screen.onnavigate).toHaveBeenCalledWith(
    "#/entity/gitea:tidewater%2Fpayout-service%23142",
  );
  screen.done();
});

test("New branch… queues CreateBranch in the repository, from this branch", async () => {
  view = overview([], [NEW_BRANCH, DELETE]);
  const screen = render();
  await vi.waitFor(() => expect(screen.button("New branch…")).toBeDefined());
  screen.button("New branch…")!.click();
  flushSync();
  const name = dialog().querySelector<HTMLInputElement>('[data-field="name"]')!;
  name.value = "feature/PAY-231-follow-up";
  name.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
  dialogButton("New branch").click();
  await vi.waitFor(() => expect(queued).toHaveLength(1));
  expect(queued).toEqual([
    { CreateBranch: { entity: REPO, name: "feature/PAY-231-follow-up", from_ref: "feature/PAY-231-sepa-retry" } },
  ]);
  screen.done();
});

test("Open pull request… sends its prefilled title and description into the base picked", async () => {
  view = overview([], [OPEN_PULL, NEW_BRANCH, DELETE]);
  const screen = render();
  await vi.waitFor(() => expect(screen.button("Open pull request…")).toBeDefined());
  expect(screen.button("Open pull request…")!.classList.contains("pri")).toBe(true);
  screen.button("Open pull request…")!.click();
  flushSync();
  expect(dialog().querySelector<HTMLInputElement>('[data-field="title"]')!.value).toBe("PAY-231 sepa retry");
  expect(dialog().querySelector<HTMLTextAreaElement>('[data-field="body"]')!.value).toBe("- one\n- two");
  [...dialog().querySelectorAll<HTMLButtonElement>('[data-field="base"] [role=radio]')][1]!.click();
  flushSync();
  dialogButton("Open pull request").click();
  await vi.waitFor(() => expect(queued).toHaveLength(1));
  expect(queued).toEqual([
    {
      CreatePullRequest: {
        entity: REPO,
        title: "PAY-231 sepa retry",
        body: "- one\n- two",
        head: "feature/PAY-231-sepa-retry",
        base: "release/2026.08",
      },
    },
  ]);
  screen.done();
});

test("Delete branch… asks with what is lost, then deletes the branch itself", async () => {
  view = overview([], [NEW_BRANCH, DELETE]);
  const screen = render();
  await vi.waitFor(() => expect(screen.button("Delete branch…")).toBeDefined());
  screen.button("Delete branch…")!.click();
  flushSync();
  expect(queued, "a danger asks first").toEqual([]);
  expect(dialog().textContent).toContain("2 commits will only be reachable by SHA");
  dialogButton("Delete branch").click();
  await vi.waitFor(() => expect(queued).toHaveLength(1));
  expect(queued).toEqual([{ DeleteBranch: { entity: BRANCH } }]);
  screen.done();
});
