/**
 * The branch detail says when this machine's checkout is on it (#704): the
 * wire from the checkout panel's read to the note under the title, which
 * `BranchCheckoutNote.test.svelte.ts` and `CheckoutPanel.test.svelte.ts` each
 * test from one end only.
 */
import { flushSync, mount, unmount } from "svelte";
import { expect, test, vi } from "vitest";

import type { CheckoutView, EntityDetail } from "../ipc/entity";

const ID = "gitea:tidewater/payout-service@refs/heads/feature/PAY-231-sepa-retry";
const BRANCH = "feature/PAY-231-sepa-retry";
let checkedOut = BRANCH;

function branch(): EntityDetail {
  return {
    row: {
      entity_id: ID,
      kind: "branch",
      source_id: "gitea",
      title: BRANCH,
      updated_at: "2026-08-22T11:42:00Z",
      synced_at: "2026-08-22T14:30:00Z",
      path: null,
      state: null,
    },
    source: { id: "gitea", display_name: "Tidewater Git", adapter_kind: "gitea", enabled: true },
    kind_info: null,
    body_text: BRANCH,
    author: "mara",
    payload: {},
    web_url: null,
    deleted_at: null,
    links: [],
    activity: [],
    builds: [],
    status: null,
  };
}

function checkout(): CheckoutView {
  return {
    repo_entity_id: "gitea:tidewater/payout-service",
    repo_url: null,
    path: "/Users/mara/src/payout-service",
    found_by: "scan",
    clones_root: "/Users/mara/src",
    clone_command: null,
    tree: {
      status: "read",
      branch: checkedOut,
      head: "0123456789abcdef0123456789abcdef01234567",
      upstream: `origin/${checkedOut}`,
      ahead: 2,
      behind: 0,
      changed: 0,
      untracked: 0,
      conflicted: 0,
    },
  };
}

vi.mock("../ipc/entity", () => ({
  sourceStatuses: () => Promise.resolve([]),
  submitWrite: () => Promise.reject(new Error("no write in this test")),
  listContexts: () => Promise.resolve([]),
  contextMembers: () => Promise.resolve([]),
  unlink: () => Promise.resolve(),
  createLink: () => Promise.resolve({}),
  getEntity: () => Promise.resolve(branch()),
  entityView: () => Promise.resolve(null),
  entityCheckout: () => Promise.resolve(checkout()),
  checkoutCommands: () => Promise.resolve([]),
}));

vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: () => Promise.resolve() }));

const { default: Detail } = await import("./Detail.svelte");

function render() {
  const target = document.createElement("div");
  document.body.append(target);
  const app = mount(Detail, {
    target,
    props: { entityId: ID, kind: "branch", contextLabel: "All work", onclose: vi.fn(), onnavigate: vi.fn() },
  });
  flushSync();
  return {
    note: () => target.querySelector(".d-title .d-local")?.textContent?.trim() ?? null,
    path: () => target.querySelector(".cpath")?.textContent ?? null,
    done: () => {
      unmount(app);
      target.remove();
    },
  };
}

test("a checkout on this branch puts a note under the title", async () => {
  checkedOut = BRANCH;
  const screen = render();
  await vi.waitFor(() => expect(screen.note()).toBe("Checked out locally · 2 to push"));
  screen.done();
});

test("a checkout on another branch puts none there", async () => {
  checkedOut = "main";
  const screen = render();
  // The panel has read (its path is drawn), and the note still is not there.
  await vi.waitFor(() => expect(screen.path()).toBe("/Users/mara/src/payout-service"));
  expect(screen.note()).toBeNull();
  screen.done();
});
