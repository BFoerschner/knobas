/**
 * The sentences the checkout panel and the branch view say about a working
 * tree (#704, ADR-0021), from `entity_checkout`'s `tree`.
 */
import { expect, test } from "vitest";

import type { TreeRead, WorkingTreeState } from "../ipc/entity";
import { branchNote, treeLine } from "./working-tree";

function read(over: Partial<WorkingTreeState> = {}): TreeRead {
  return {
    status: "read",
    branch: "main",
    head: "0123456789abcdef0123456789abcdef01234567",
    upstream: "origin/main",
    ahead: 0,
    behind: 0,
    changed: 0,
    untracked: 0,
    conflicted: 0,
    ...over,
  };
}

test("a clean tree names its branch and is quiet", () => {
  expect(treeLine(read())).toEqual({ text: "on main · clean", tone: "muted" });
});

test("changes, untracked paths and commits to push and pull are counted in that order", () => {
  expect(
    treeLine(
      read({
        branch: "knobas-sepa-retry",
        changed: 3,
        untracked: 1,
        ahead: 2,
        behind: 4,
      }),
    ),
  ).toEqual({
    text: "on knobas-sepa-retry · 3 changed · 1 untracked · 2 to push · 4 to pull",
    tone: "amber",
  });
});

test("unpushed commits alone are not a warning", () => {
  expect(treeLine(read({ ahead: 2 }))).toEqual({
    text: "on main · 2 to push",
    tone: "muted",
  });
});

test("a conflict is the error tone and comes first", () => {
  expect(treeLine(read({ conflicted: 1, changed: 2 }))).toEqual({
    text: "on main · 1 conflicted · 2 changed",
    tone: "fail",
  });
});

test("a detached head names its short commit, and a branch with no upstream says so", () => {
  expect(treeLine(read({ branch: null, upstream: null }))).toEqual({
    text: "detached at 0123456 · clean",
    tone: "muted",
  });
  expect(treeLine(read({ upstream: null })).text).toBe("on main · clean · no upstream");
});

test("the states with no reading each say why", () => {
  expect(treeLine({ status: "no_git" })).toEqual({
    text: "git not found on PATH",
    tone: "muted",
  });
  expect(treeLine({ status: "not_a_repo" })).toEqual({
    text: "not a git working tree",
    tone: "amber",
  });
  expect(treeLine({ status: "failed", reason: "git did not answer within 2 s" })).toEqual({
    text: "could not read the working tree: git did not answer within 2 s",
    tone: "fail",
  });
});

test("the branch note appears only when the checkout is on this branch", () => {
  expect(branchNote(read({ branch: "feature/x", ahead: 2, changed: 1 }), "feature/x")).toEqual({
    text: "Checked out locally · 1 changed · 2 to push",
    tone: "amber",
  });
  expect(branchNote(read({ branch: "feature/x" }), "feature/x")).toEqual({
    text: "Checked out locally",
    tone: "muted",
  });
  expect(branchNote(read({ branch: "main" }), "feature/x")).toBeNull();
  expect(branchNote(read({ branch: null }), "main")).toBeNull();
  expect(branchNote({ status: "no_git" }, "main")).toBeNull();
  expect(branchNote(null, "main")).toBeNull();
});
