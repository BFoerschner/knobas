/**
 * A commit's detail draws the builds the mirror holds for it as one line
 * between the view's toolbar and its tabs (#710, #732): the wire from
 * `get_entity`'s `builds` to `BuildsSummary`, which
 * `BuildsSummary.test.svelte.ts` tests from its own end only. Drawn under the
 * header while the source's view is out or where there is none, because the
 * rows are the mirror's, not the view's; moved above the tabs when the view
 * lands, its list still open.
 */
import { flushSync, mount, unmount } from "svelte";
import { beforeEach, expect, test, vi } from "vitest";

import type { EntityDetail, EntityRow, SourceView } from "../ipc/entity";

const SHA = "c90d11a3f5e2b7c4d9018e6a2b3c4d5e6f708192";
const ID = `gitea:tidewater/payout-service@${SHA}`;
let builds: EntityRow[] = [];
/** The source's view: by default it never answers. */
let view: () => Promise<SourceView | null> = () => new Promise(() => {});

function commit(): EntityDetail {
  return {
    row: {
      entity_id: ID,
      kind: "commit",
      source_id: "gitea",
      title: "PAY-231: jitter in backoff",
      updated_at: "2026-08-22T11:42:00Z",
      synced_at: "2026-08-22T14:30:00Z",
      path: "tidewater/payout-service",
      state: null,
    },
    source: { id: "gitea", display_name: "Tidewater Git", adapter_kind: "gitea", enabled: true },
    kind_info: null,
    body_text: "PAY-231: jitter in backoff",
    author: "mara",
    payload: {},
    web_url: null,
    deleted_at: null,
    links: [],
    activity: [],
    builds,
    status: null,
    issue_type: null,
    priority: null,
    estimate_seconds: null,
  };
}

vi.mock("../ipc/entity", () => ({
  sourceStatuses: () => Promise.resolve([]),
  submitWrite: () => Promise.reject(new Error("no write in this test")),
  listContexts: () => Promise.resolve([]),
  contextMembers: () => Promise.resolve([]),
  unlink: () => Promise.resolve(),
  createLink: () => Promise.resolve({}),
  getEntity: () => Promise.resolve(commit()),
  entityView: () => view(),
  entityCheckout: () => Promise.resolve(null),
  checkoutCommands: () => Promise.resolve([]),
}));

vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: () => Promise.resolve() }));

const { default: Detail } = await import("./Detail.svelte");

function render() {
  const target = document.createElement("div");
  document.body.append(target);
  const onnavigate = vi.fn<(hash: string) => void>();
  const app = mount(Detail, {
    target,
    props: { entityId: ID, kind: "commit", contextLabel: "All work", onclose: vi.fn(), onnavigate },
  });
  flushSync();
  const line = () => target.querySelector<HTMLButtonElement>("button.bs-line");
  return {
    target,
    onnavigate,
    line,
    done: () => {
      unmount(app);
      target.remove();
    },
  };
}

const FAILED: EntityRow = {
  entity_id: "teamcity:1187",
  kind: "build",
  source_id: "teamcity",
  title: "Payout / IntegrationTests #1187",
  updated_at: "2026-08-22T10:14:12Z",
  synced_at: "2026-08-22T14:30:00Z",
  path: "IntegrationTests",
  state: { text: "FAILURE", tone: "err" },
};

const PASSED: EntityRow = {
  ...FAILED,
  entity_id: "teamcity:1186",
  title: "Payout / Unit #1186",
  path: "Unit",
  state: { text: "SUCCESS", tone: "ok" },
};

function drawn(): SourceView {
  return {
    actions: [],
    sections: [
      {
        id: "overview",
        title: "Overview",
        count: null,
        deferred: false,
        blocks: [{ type: "facts", facts: [{ label: "Commit", value: "c90d11a3f5", tone: null }] }],
      },
      {
        id: "files",
        title: "Files",
        count: 1,
        deferred: false,
        blocks: [{ type: "text", text: "src/retry.rs", markdown: false }],
      },
    ],
  };
}

/** Whether `a` comes before `b` in the document. */
function before(a: Element | null, b: Element | null): boolean {
  return a !== null && b !== null && (a.compareDocumentPosition(b) & Node.DOCUMENT_POSITION_FOLLOWING) !== 0;
}

beforeEach(() => {
  builds = [FAILED, PASSED];
  view = () => new Promise(() => {});
});

test("while the view is out, the builds line is under the header and opens each build", async () => {
  const screen = render();
  await vi.waitFor(() => expect(screen.line()).not.toBeNull());
  expect(screen.line()?.textContent).toContain("1 failing");
  expect(screen.line()?.closest(".sv")).toBeNull();
  expect(before(screen.target.querySelector(".d-meta"), screen.line())).toBe(true);

  screen.line()?.click();
  flushSync();
  screen.target.querySelector<HTMLButtonElement>(".bs-list button.row")?.click();
  expect(screen.onnavigate).toHaveBeenCalledWith(expect.stringContaining("1187"));
  screen.done();
});

test("with a view, the line sits above the tabs on every tab", async () => {
  view = () => Promise.resolve(drawn());
  const screen = render();
  await vi.waitFor(() => expect(screen.target.querySelector("[role=tablist]")).not.toBeNull());
  const tablist = screen.target.querySelector("[role=tablist]");
  expect(screen.line()?.closest(".sv")).not.toBeNull();
  expect(before(screen.line(), tablist)).toBe(true);
  expect(screen.target.querySelectorAll("button.bs-line")).toHaveLength(1);

  screen.target.querySelectorAll<HTMLButtonElement>("[role=tab]")[1]?.click();
  flushSync();
  expect(before(screen.line(), tablist)).toBe(true);
  expect(screen.line()?.textContent).toContain("1 passed");
  screen.done();
});

test("a list opened while the view was out stays open when it lands", async () => {
  let land: (v: SourceView) => void = () => {};
  view = () => new Promise((resolve) => (land = resolve));
  const screen = render();
  await vi.waitFor(() => expect(screen.line()).not.toBeNull());
  screen.line()?.click();
  flushSync();
  expect(screen.line()?.getAttribute("aria-expanded")).toBe("true");

  land(drawn());
  await vi.waitFor(() => expect(screen.line()?.closest(".sv")).not.toBeNull());
  expect(screen.line()?.getAttribute("aria-expanded")).toBe("true");
  expect(screen.target.querySelectorAll(".bs-list button.row")).toHaveLength(2);
  screen.done();
});

test("a source with no view draws the line under the header", async () => {
  view = () => Promise.resolve(null);
  const screen = render();
  await vi.waitFor(() => expect(document.body.textContent).toContain("Linked items"));
  expect(screen.line()).not.toBeNull();
  expect(screen.line()?.closest(".sv")).toBeNull();
  screen.done();
});

test("a commit nothing ran on draws no builds line", async () => {
  builds = [];
  view = () => Promise.resolve(drawn());
  const screen = render();
  await vi.waitFor(() => expect(screen.target.querySelector("[role=tablist]")).not.toBeNull());
  expect(screen.line()).toBeNull();
  expect(screen.target.textContent).not.toContain("Builds");
  screen.done();
});
