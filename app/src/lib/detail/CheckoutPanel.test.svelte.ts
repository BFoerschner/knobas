/**
 * The checkout panel on a repo and a branch detail (issue #499).
 *
 * The seam is the rendered panel: a `CheckoutView` in, user-visible text and
 * bridge calls out. What the backend does to produce that view is
 * `crates/knobas-app/tests/checkout_ipc.rs`'s, and whether a directory really
 * holds a clone is `knobas-core`'s; what is asserted here is that the three
 * states each say what they are, that *no checkout* offers the clone command,
 * and that setting and clearing a path go through `set_checkout_override`.
 *
 * The panel reads through the module import rather than a `ports` prop -- the
 * detail's own components do -- so the bridge is mocked here.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { CheckoutView } from "../ipc/entity";

const reads: string[] = [];
const writes: { entityId: string; path: string | null }[] = [];
let answer: CheckoutView;
let afterWrite: CheckoutView | null = null;
/** Set by the one test about a refused read; cleared in `beforeEach`. */
let refuseRead = false;

vi.mock("../ipc/entity", () => ({
  entityCheckout: (entityId: string) => {
    reads.push(entityId);
    if (refuseRead) {
      return Promise.reject({
        code: "invalid",
        message: "a checkout belongs to a repo or a branch",
        source_id: null,
      });
    }
    return Promise.resolve(answer);
  },
  setCheckoutOverride: (entityId: string, path: string | null) => {
    writes.push({ entityId, path });
    return Promise.resolve(afterWrite ?? answer);
  },
}));

const CheckoutPanel = (await import("./CheckoutPanel.svelte")).default;

const REPO = "gitea:tidewater/payout-service";
const URL = "https://gitea.example.com/tidewater/payout-service";

function view(over: Partial<CheckoutView> = {}): CheckoutView {
  return {
    repo_entity_id: REPO,
    repo_url: URL,
    path: null,
    found_by: "nothing",
    clones_root: "/Users/mara/src",
    clone_command: `git clone ${URL}`,
    ...over,
  };
}

let target: HTMLDivElement;
let app: Record<string, unknown> | undefined;

function render(entityId = REPO) {
  app = mount(CheckoutPanel, { target, props: { entityId } });
  flushSync();
  return app;
}

/** Let the read's promise land, then re-render. */
async function settle() {
  await Promise.resolve();
  await Promise.resolve();
  flushSync();
}

function text(): string {
  return (target.textContent ?? "").replace(/\s+/g, " ").trim();
}

function button(label: string): HTMLButtonElement {
  const found = [...target.querySelectorAll("button")].find((element) =>
    (element.textContent ?? "").includes(label),
  );
  if (!found) throw new Error(`no button reading ${label}: ${text()}`);
  return found as HTMLButtonElement;
}

beforeEach(() => {
  reads.length = 0;
  writes.length = 0;
  answer = view();
  afterWrite = null;
  refuseRead = false;
  target = document.createElement("div");
  document.body.append(target);
});

afterEach(() => {
  if (app) unmount(app);
  app = undefined;
  target.remove();
});

test("a checkout the scan found is drawn as a path, and says where it came from", async () => {
  answer = view({ path: "/Users/mara/src/payout-service", found_by: "scan" });
  render();
  await settle();

  expect(reads).toEqual([REPO]);
  expect(text()).toContain("/Users/mara/src/payout-service");
  expect(text()).toContain("found under the clones root");
  // Nothing to copy: there is a checkout, so the clone command is not offered.
  expect(text()).not.toContain("git clone");
});

test("a path set by hand says so, and offers to clear it back to the scan", async () => {
  answer = view({ path: "/Users/mara/work/payout-service", found_by: "override" });
  render();
  await settle();

  expect(text()).toContain("set by hand");
  expect(text()).toContain("/Users/mara/work/payout-service");

  afterWrite = view({ path: "/Users/mara/src/payout-service", found_by: "scan" });
  button("Clear").click();
  await settle();

  expect(writes).toEqual([{ entityId: REPO, path: null }]);
  // The panel redraws from the backend's answer, so clearing *shows* the scan
  // taking over rather than blanking the line.
  expect(text()).toContain("found under the clones root");
  expect(text()).toContain("/Users/mara/src/payout-service");
});

test("no checkout says so, names the root it looked in, and offers the clone command", async () => {
  render();
  await settle();

  expect(text()).toContain("No checkout on this machine.");
  expect(text()).toContain("/Users/mara/src");
  expect(text()).toContain(`git clone ${URL}`);
  expect(button("Copy clone command")).toBeTruthy();
});

test("no clones root is a different sentence from nothing found under one", async () => {
  answer = view({ clones_root: null });
  render();
  await settle();

  expect(text()).toContain("No clones root is set.");
  expect(text()).not.toContain("Nothing under");
});

test("a repo with no URL offers no clone command, because there is none to build", async () => {
  answer = view({ repo_url: null, clone_command: null });
  render();
  await settle();

  expect(text()).toContain("No checkout on this machine.");
  expect(text()).not.toContain("git clone");
});

test("setting a path sends it, and the panel draws what came back", async () => {
  render();
  await settle();

  button("Set path…").click();
  flushSync();
  const input = target.querySelector<HTMLInputElement>("input");
  if (!input) throw new Error("no path input");
  input.value = "/Users/mara/work/payout-service";
  input.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();

  afterWrite = view({ path: "/Users/mara/work/payout-service", found_by: "override" });
  button("Save").click();
  await settle();

  expect(writes).toEqual([{ entityId: REPO, path: "/Users/mara/work/payout-service" }]);
  expect(text()).toContain("set by hand");
  expect(text()).toContain("/Users/mara/work/payout-service");
});

test("a branch draws its repository's checkout, and its override is keyed on the repo", async () => {
  const branch = `${REPO}@refs/heads/feature/PAY-231-sepa-retry`;
  answer = view({ path: "/Users/mara/src/payout-service", found_by: "scan" });
  render(branch);
  await settle();

  expect(reads).toEqual([branch]);
  expect(text()).toContain("/Users/mara/src/payout-service");

  // Sent with the *branch's* address; the backend is what keys it on the
  // repository, and `checkout_ipc.rs` is where that is asserted.
  afterWrite = view({ path: "/Users/mara/work/wt", found_by: "override" });
  button("Set path…").click();
  flushSync();
  const input = target.querySelector<HTMLInputElement>("input");
  if (!input) throw new Error("no path input");
  input.value = "/Users/mara/work/wt";
  input.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
  button("Save").click();
  await settle();

  expect(writes).toEqual([{ entityId: branch, path: "/Users/mara/work/wt" }]);
});

test("a read that fails leaves the panel quiet rather than shouting over the item", async () => {
  refuseRead = true;
  render();
  await settle();

  // Still the reading line, and no error banner: the detail is worth drawing
  // without a checkout, and a toast about one nobody asked to see is noise.
  expect(text()).toContain("Reading…");
  expect(text()).not.toContain("No checkout");
});
