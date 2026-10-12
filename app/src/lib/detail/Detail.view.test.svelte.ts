/**
 * The detail and the source view (#661, ADR-0020): three answers, each drawn
 * differently, and the one rule that binds them -- **the fallback is never
 * silent about an error, and never noisy about an absence** (rule 2).
 *
 * * drawn: the view's sections are tabs, and the generic projection moves
 *   behind a *Raw* disclosure;
 * * `null`: the panel is exactly what it was before views existed;
 * * a failure: the same panel, under one line naming the failure.
 */
import { flushSync, mount, unmount } from "svelte";
import { beforeEach, expect, test, vi } from "vitest";

import type { EntityDetail, SourceView } from "../ipc/entity";

/** Every view read, as `[entity, section]`, in order with `get_entity`'s. */
const calls: string[] = [];
let view: () => Promise<SourceView | null> = () => Promise.resolve(null);

vi.mock("../ipc/entity", () => ({
  miniBoard: () => Promise.resolve({ columns: [], sources: [] }),
  submitWrite: () => Promise.reject(new Error("no write in this test")),
  listContexts: () => Promise.resolve([]),
  contextMembers: () => Promise.resolve([]),
  createContext: () => Promise.reject(new Error("no context creation in this test")),
  promoteContext: () => Promise.reject(new Error("no promotion in this test")),
  unlink: () => Promise.resolve(),
  createLink: () => Promise.resolve({}),
  getEntity: (entityId: string) => {
    calls.push(`get_entity ${entityId}`);
    return Promise.resolve(commit());
  },
  entityView: (entityId: string, section: string | null = null) => {
    calls.push(`entity_view ${entityId} ${section}`);
    return view();
  },
}));

vi.mock("../ipc/search", () => ({
  search: () => Promise.reject(new Error("the picker is not this file's business")),
  launcherHome: () => Promise.reject(new Error("the dialog never loads the board")),
  noFilters: () => ({ sources: [], kinds: [], updated_within_days: null, mine: false, authors: [] }),
}));

vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: () => Promise.resolve() }));

const { default: Detail } = await import("./Detail.svelte");

const ID = "gitea:tidewater/payout-service@c90d11a3f5e2b7c4d9018e6a2b3c4d5e6f708192";

function commit(): EntityDetail {
  return {
    row: {
      entity_id: ID,
      kind: "commit",
      source_id: "gitea",
      title: "PAY-231: jitter in backoff",
      updated_at: "2026-08-22T11:42:00Z",
      synced_at: "2026-08-22T14:30:00Z",
      path: null,
      state: null,
    },
    source: { id: "gitea", display_name: "Tidewater Git", adapter_kind: "gitea", enabled: true },
    kind_info: null,
    body_text: "PAY-231: jitter in backoff\n\nBounded to +/-10 %.",
    author: "mara",
    payload: { sha: "c90d11a3f5e2b7c4d9018e6a2b3c4d5e6f708192", html_url: "https://gitea.example/c" },
    web_url: "https://gitea.example/c",
    deleted_at: null,
    links: [],
    activity: [],
  };
}

const DRAWN: SourceView = {
  sections: [
    {
      id: "overview",
      title: "Overview",
      count: null,
      deferred: false,
      blocks: [{ type: "facts", facts: [{ label: "Commit", value: "c90d11a3f5", tone: null }] }],
    },
    {
      id: "diff",
      title: "Diff",
      count: 1,
      deferred: false,
      blocks: [
        {
          type: "diff",
          truncated: false,
          files: [
            {
              path: "src/retry.rs",
              old_path: null,
              change: "modified",
              additions: 1,
              deletions: 0,
              truncated: false,
              hunks: [{ header: "@@ -1 +1,2 @@", lines: [{ kind: "add", old: null, new: 2, text: "jitter" }] }],
            },
          ],
        },
      ],
    },
  ],
};

function render() {
  const target = document.createElement("div");
  document.body.append(target);
  const app = mount(Detail, {
    target,
    props: { entityId: ID, kind: "commit", contextLabel: "All work", onclose: vi.fn(), onnavigate: vi.fn() },
  });
  flushSync();
  return {
    target,
    text: () => (target.textContent ?? "").replace(/\s+/g, " "),
    tabs: () => [...target.querySelectorAll("[role=tab]")].map((t) => t.textContent!.replace(/\s+/g, " ").trim()),
    raw: () => target.querySelector<HTMLDetailsElement>("details.raw"),
    details: () =>
      [...target.querySelectorAll(".sec-h .lab")].some((label) => label.textContent === "Details"),
    done: () => {
      unmount(app);
      target.remove();
    },
  };
}

beforeEach(() => {
  calls.length = 0;
  view = () => Promise.resolve(null);
});

test("a drawn view is tabs, and the payload moves behind Raw", async () => {
  view = () => Promise.resolve(DRAWN);
  const screen = render();
  await vi.waitFor(() => expect(screen.tabs()).toEqual(["Overview", "Diff 1"]));

  // The view is read after the entity, and once.
  expect(calls).toEqual([`get_entity ${ID}`, `entity_view ${ID} null`]);
  expect(screen.text()).toContain("c90d11a3f5");
  expect(screen.details(), "the generic projection is no longer its own section").toBe(false);
  // The message is the view's to draw: no second copy as a description.
  expect(screen.text()).not.toContain("Bounded to +/-10 %.");
  expect(screen.text()).not.toContain("Showing the mirror");

  const raw = screen.raw()!;
  expect(raw.open).toBe(false);
  expect(raw.querySelector("summary")!.textContent).toContain("Raw");
  // Closed, it draws nothing of the payload; opened, it is the projection.
  raw.open = true;
  raw.dispatchEvent(new Event("toggle"));
  flushSync();
  await vi.waitFor(() => expect(raw.textContent).toContain("c90d11a3f5e2b7c4d9018e6a2b3c4d5e6f708192"));
  screen.done();
});

test("no view is the panel as it always was, with nothing said about it", async () => {
  const screen = render();
  await vi.waitFor(() => expect(calls).toContain(`entity_view ${ID} null`));
  await vi.waitFor(() => expect(screen.text()).toContain("PAY-231: jitter in backoff"));
  // The `null` has landed: a macrotask runs after the answer's microtasks, so
  // what follows is the panel *after* the answer, not before it.
  await new Promise((settle) => setTimeout(settle, 0));
  flushSync();
  expect(screen.tabs()).toEqual([]);
  expect(screen.raw()).toBeNull();
  expect(screen.details()).toBe(true);
  expect(screen.text()).toContain("Bounded to +/-10 %.");
  expect(screen.text()).not.toContain("Showing the mirror");
  screen.done();
});

test("a failed view is the same panel under one line naming the failure", async () => {
  view = () => Promise.reject({ code: "unreachable", message: "Gitea answered 502", source_id: "gitea" });
  const screen = render();
  await vi.waitFor(() => expect(screen.text()).toContain("Showing the mirror: Gitea answered 502"));
  expect(screen.target.querySelectorAll(".mirror-line")).toHaveLength(1);
  expect(screen.tabs()).toEqual([]);
  expect(screen.raw()).toBeNull();
  expect(screen.details()).toBe(true);
  screen.done();
});
