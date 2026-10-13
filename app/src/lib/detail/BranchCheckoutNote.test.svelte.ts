/**
 * The branch view's note (#704): drawn by the shell from the checkout read,
 * never by the adapter, and only when the checkout is on this branch.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import type { TreeRead } from "../ipc/entity";
import BranchCheckoutNote from "./BranchCheckoutNote.svelte";

let target: HTMLDivElement;
let app: Record<string, unknown> | undefined;

function on(branch: string | null, ahead = 0, changed = 0): TreeRead {
  return {
    status: "read",
    branch,
    head: "0123456789abcdef0123456789abcdef01234567",
    upstream: "origin/x",
    ahead,
    behind: 0,
    changed,
    untracked: 0,
    conflicted: 0,
  };
}

function render(tree: TreeRead | null, branch: string) {
  app = mount(BranchCheckoutNote, { target, props: { tree, branch } });
  flushSync();
}

function text(): string {
  return (target.textContent ?? "").replace(/\s+/g, " ").trim();
}

beforeEach(() => {
  target = document.createElement("div");
  document.body.append(target);
});

afterEach(() => {
  if (app) unmount(app);
  app = undefined;
  target.remove();
});

test("a checkout on this branch says so, with what is waiting to be pushed", () => {
  render(on("feature/PAY-231-sepa-retry", 2), "feature/PAY-231-sepa-retry");
  expect(text()).toBe("Checked out locally · 2 to push");
  expect(target.querySelector(".muted")).not.toBeNull();
});

test("uncommitted changes turn the note to the warn tone", () => {
  render(on("feature/x", 0, 1), "feature/x");
  expect(text()).toBe("Checked out locally · 1 changed");
  expect(target.querySelector(".amber")).not.toBeNull();
});

test("a checkout on another branch, detached, or not read draws nothing", () => {
  render(on("main"), "feature/x");
  expect(text()).toBe("");
  unmount(app!);
  render(on(null), "feature/x");
  expect(text()).toBe("");
  unmount(app!);
  render(null, "feature/x");
  expect(text()).toBe("");
});
