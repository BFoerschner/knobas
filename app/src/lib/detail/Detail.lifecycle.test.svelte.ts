/**
 * The detail does not jump when its view lands (#693).
 *
 * Before #693 the panel drew the mirror's generic body (Description, Details)
 * the moment `get_entity` answered and swapped it for the source view's tabs
 * a beat later, so the description vanished and the panel jumped, on every
 * navigation. What is pinned here:
 *
 * * while a view read is in flight, the body is one quiet placeholder line,
 *   and the generic body is never drawn on the way to the tabs;
 * * past a short threshold the mirror's body is drawn under *Showing the
 *   mirror*, and a view that lands later replaces it (offline stays usable);
 * * a source that answered `None` for a kind is remembered, so its next item
 *   draws its generic body with no wait;
 * * a recently seen entity is drawn from memory at once and re-read behind.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { EntityDetail, SourceView } from "../ipc/entity";

/** Every IPC read, in order. */
const calls: string[] = [];
/** The view each entity answers with; a pending one is a promise not yet settled. */
let views: Record<string, () => Promise<SourceView | null>> = {};

vi.mock("../ipc/entity", () => ({
  miniBoard: () => Promise.resolve({ columns: [], sources: [] }),
  reachableTransitions: () => Promise.resolve([]),
  submitWrite: () => Promise.reject(new Error("no write in this test")),
  listContexts: () => Promise.resolve([]),
  contextMembers: () => Promise.resolve([]),
  createContext: () => Promise.reject(new Error("no context creation in this test")),
  promoteContext: () => Promise.reject(new Error("no promotion in this test")),
  unlink: () => Promise.resolve(),
  createLink: () => Promise.resolve({}),
  getEntity: (entityId: string) => {
    calls.push(`get_entity ${entityId}`);
    return Promise.resolve(entity(entityId));
  },
  entityView: (entityId: string) => {
    calls.push(`entity_view ${entityId}`);
    return (views[entityId] ?? (() => Promise.resolve(null)))();
  },
}));

vi.mock("../ipc/search", () => ({
  search: () => Promise.reject(new Error("the picker is not this file's business")),
  launcherHome: () => Promise.reject(new Error("the dialog never loads the board")),
  noFilters: () => ({ sources: [], kinds: [], updated_within_days: null, mine: false, authors: [] }),
}));

vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: () => Promise.resolve() }));

const { default: Detail } = await import("./Detail.svelte");

const A = "gitea:tidewater/payout-service@c90d11a3";
const B = "gitea:tidewater/payout-service@77ab0e12";
const JIRA_1 = "jira:PAY-231";
const JIRA_2 = "jira:PAY-232";

function entity(id: string): EntityDetail {
  const jira = id.startsWith("jira:");
  return {
    row: {
      entity_id: id,
      kind: jira ? "ticket" : "commit",
      source_id: jira ? "jira" : "gitea",
      title: `Title of ${id}`,
      updated_at: "2026-08-22T11:42:00Z",
      synced_at: "2026-08-22T14:30:00Z",
      path: null,
      state: null,
    },
    source: jira
      ? { id: "jira", display_name: "Tidewater Jira", adapter_kind: "jira", enabled: true }
      : { id: "gitea", display_name: "Tidewater Git", adapter_kind: "gitea", enabled: true },
    kind_info: null,
    body_text: `The body of ${id}`,
    author: "mara",
    payload: { key: id },
    web_url: null,
    deleted_at: null,
    links: [],
    activity: [],
  };
}

function drawn(label: string): SourceView {
  return {
    sections: [
      {
        id: "overview",
        title: "Overview",
        count: null,
        deferred: false,
        blocks: [{ type: "facts", facts: [{ label: "Commit", value: label, tone: null }] }],
      },
    ],
  };
}

/** A view read the test settles by hand. */
function pending() {
  let settle!: (view: SourceView | null) => void;
  const promise = new Promise<SourceView | null>((resolve) => (settle = resolve));
  return { read: () => promise, settle };
}

/** Let every answered promise land, without touching the (possibly fake) clock. */
async function settle() {
  for (let i = 0; i < 20; i++) await Promise.resolve();
  flushSync();
}

let open: (() => void) | null = null;

function render(entityId: string) {
  const target = document.createElement("div");
  document.body.append(target);
  // Every body section the panel has drawn, recorded as it appears: what
  // "no intermediate generic body" is checked against.
  const seen = new Set<string>();
  const observer = new MutationObserver(() => {
    for (const label of target.querySelectorAll(".sec-h .lab")) seen.add(label.textContent ?? "");
  });
  observer.observe(target, { childList: true, subtree: true, characterData: true });
  const app = mount(Detail, {
    target,
    props: { entityId, kind: null, contextLabel: "All work", onclose: vi.fn(), onnavigate: vi.fn() },
  });
  flushSync();
  const done = () => {
    observer.disconnect();
    unmount(app);
    target.remove();
    open = null;
  };
  open = done;
  return {
    target,
    seen,
    text: () => (target.textContent ?? "").replace(/\s+/g, " "),
    tabs: () => [...target.querySelectorAll("[role=tab]")].map((t) => t.textContent!.trim()),
    labels: () => [...target.querySelectorAll(".sec-h .lab")].map((l) => l.textContent),
    placeholder: () => target.querySelector(".view-wait"),
    done,
  };
}

beforeEach(() => {
  calls.length = 0;
  views = {};
});

afterEach(() => {
  open?.();
  vi.useRealTimers();
});

test("while the view is read the body is one placeholder line, and the tabs replace it directly", async () => {
  const view = pending();
  views[A] = view.read;
  const screen = render(A);
  await settle();

  // The header has landed; the body has not been guessed at.
  expect(screen.text()).toContain(`Title of ${A}`);
  expect(screen.placeholder()?.textContent).toContain("Reading from Tidewater Git…");
  expect(screen.labels()).not.toContain("Description");
  expect(screen.labels()).not.toContain("Details");
  expect(screen.text()).not.toContain(`The body of ${A}`);
  // The links stay: they are the mirror's, not the view's.
  expect(screen.labels()).toContain("Linked items");

  view.settle(drawn("c90d11a3"));
  await settle();
  expect(screen.tabs()).toEqual(["Overview"]);
  expect(screen.placeholder()).toBeNull();
  expect([...screen.seen], "no generic body on the way to the tabs").not.toContain("Details");
  expect([...screen.seen]).not.toContain("Description");
});

test("past the threshold the mirror's body is drawn with its line, and a late view replaces it", async () => {
  vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
  const view = pending();
  views[A] = view.read;
  const screen = render(A);
  await settle();
  expect(screen.placeholder()).not.toBeNull();

  vi.advanceTimersByTime(1499);
  await settle();
  expect(screen.placeholder(), "not yet").not.toBeNull();

  vi.advanceTimersByTime(1);
  await settle();
  expect(screen.placeholder()).toBeNull();
  expect(screen.labels()).toContain("Details");
  expect(screen.text()).toContain(`The body of ${A}`);
  expect(screen.text()).toContain("Showing the mirror: Tidewater Git has not answered yet");

  view.settle(drawn("c90d11a3"));
  await settle();
  expect(screen.tabs()).toEqual(["Overview"]);
  expect(screen.text()).not.toContain("Showing the mirror");
  expect(screen.labels()).not.toContain("Details");
});

test("A, then B, then back to A draws A's view at once from memory, and re-reads it behind", async () => {
  views[A] = () => Promise.resolve(drawn("c90d11a3"));
  views[B] = () => Promise.resolve(drawn("77ab0e12"));
  let screen = render(A);
  await settle();
  expect(screen.tabs()).toEqual(["Overview"]);
  screen.done();

  screen = render(B);
  await settle();
  expect(screen.text()).toContain("77ab0e12");
  screen.done();

  const reread = pending();
  views[A] = reread.read;
  calls.length = 0;
  screen = render(A);
  // Synchronously, before any read has answered.
  expect(screen.text()).toContain(`Title of ${A}`);
  expect(screen.tabs()).toEqual(["Overview"]);
  expect(screen.text()).toContain("c90d11a3");
  expect(screen.placeholder()).toBeNull();
  expect(screen.text()).not.toContain("Reading");
  // ...and read again behind it, replacing in place when it changed.
  await settle();
  expect(calls).toEqual([`get_entity ${A}`, `entity_view ${A}`]);
  reread.settle(drawn("c90d11a3, amended"));
  await settle();
  expect(screen.text()).toContain("c90d11a3, amended");
});

test("a source that answered None for a kind draws its next item's generic body with no wait", async () => {
  vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
  let screen = render(JIRA_1);
  await settle();
  expect(screen.labels()).toContain("Details");
  screen.done();

  // The next ticket's view never answers: nothing waits on it.
  views[JIRA_2] = pending().read;
  screen = render(JIRA_2);
  await settle();
  expect(screen.placeholder()).toBeNull();
  expect(screen.labels()).toContain("Details");
  expect(screen.text()).toContain(`The body of ${JIRA_2}`);
  expect(screen.text()).not.toContain("Showing the mirror");
  // Nor is a slow read of a view that is not coming announced: rule 2's
  // "never noisy about an absence".
  vi.advanceTimersByTime(1500);
  await settle();
  expect(screen.text()).not.toContain("Showing the mirror");
});
