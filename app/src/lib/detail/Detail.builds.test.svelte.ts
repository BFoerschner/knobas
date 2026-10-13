/**
 * A commit's detail draws the builds the mirror holds for it (#710): the wire
 * from `get_entity`'s `builds` to the panel, which `BuildsPanel.test.svelte.ts`
 * tests from its own end only. Drawn while the source's view is still out,
 * because the rows are the mirror's, not the view's.
 */
import { flushSync, mount, unmount } from "svelte";
import { expect, test, vi } from "vitest";

import type { EntityDetail, EntityRow } from "../ipc/entity";

const SHA = "c90d11a3f5e2b7c4d9018e6a2b3c4d5e6f708192";
const ID = `gitea:tidewater/payout-service@${SHA}`;
let builds: EntityRow[] = [];

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
  };
}

vi.mock("../ipc/entity", () => ({
  miniBoard: () => Promise.resolve({ columns: [], sources: [] }),
  submitWrite: () => Promise.reject(new Error("no write in this test")),
  listContexts: () => Promise.resolve([]),
  contextMembers: () => Promise.resolve([]),
  unlink: () => Promise.resolve(),
  createLink: () => Promise.resolve({}),
  getEntity: () => Promise.resolve(commit()),
  // The source's view never answers: the builds do not wait for it.
  entityView: () => new Promise(() => {}),
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
  const panel = () =>
    [...target.querySelectorAll(".sec")].find((sec) => sec.querySelector(".lab")?.textContent === "Builds") ?? null;
  return {
    onnavigate,
    panel,
    done: () => {
      unmount(app);
      target.remove();
    },
  };
}

test("a commit's builds are drawn and each opens its build", async () => {
  builds = [
    {
      entity_id: "teamcity:1187",
      kind: "build",
      source_id: "teamcity",
      title: "Payout / IntegrationTests #1187",
      updated_at: "2026-08-22T10:14:12Z",
      synced_at: "2026-08-22T14:30:00Z",
      path: "IntegrationTests",
      state: { text: "FAILURE", tone: "err" },
    },
  ];
  const screen = render();
  await vi.waitFor(() => expect(screen.panel()).not.toBeNull());
  expect(screen.panel()?.textContent).toContain("Payout / IntegrationTests #1187");
  expect(screen.panel()?.textContent).toContain("1 failing");
  screen.panel()?.querySelector<HTMLButtonElement>("button.row")?.click();
  expect(screen.onnavigate).toHaveBeenCalledWith(expect.stringContaining("1187"));
  screen.done();
});

test("a commit nothing ran on draws no builds panel", async () => {
  builds = [];
  const screen = render();
  // The detail has drawn (its links panel is there), and no builds panel.
  await vi.waitFor(() => expect(document.body.textContent).toContain("Linked items"));
  expect(screen.panel()).toBeNull();
  screen.done();
});
