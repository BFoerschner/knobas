/**
 * What the repository page drew queued this session (#839, #845) goes once the
 * source's own read shows the write landed.
 */
import { beforeEach, expect, test } from "vitest";

import { forgetEchoes, isDeleteQueued, madeNotListed, markCommit, markDeleted, markMade, queuedCommit } from "./queued.svelte";

const REPO = "gitea:tidewater/payout-service";

beforeEach(() => forgetEchoes());

test("a branch queued to be made is drawn until the listing holds it", () => {
  markMade(REPO, "feature/PAY-231-x");
  markMade(REPO, "feature/PAY-231-x");
  expect(madeNotListed(REPO, ["main"])).toEqual(["feature/PAY-231-x"]);
  expect(madeNotListed(REPO, ["main", "feature/PAY-231-x"])).toEqual([]);
  expect(madeNotListed("gitea:tidewater/ledger", [])).toEqual([]);
});

test("a queued commit is drawn while the file is the blob it was made against, and not after", () => {
  markCommit(REPO, "main", "README.md", { message: "PAY-231: update README.md", base: "blob-1" });
  expect(queuedCommit(REPO, "main", "README.md", "blob-1")).toEqual({ message: "PAY-231: update README.md", base: "blob-1" });
  expect(queuedCommit(REPO, "main", "README.md", "blob-2")).toBeNull();
  expect(queuedCommit(REPO, "feature/x", "README.md", "blob-1")).toBeNull();
});

test("a branch queued to be deleted is marked in its repository only", () => {
  markDeleted(REPO, "chore/bump-tokio");
  expect(isDeleteQueued(REPO, "chore/bump-tokio")).toBe(true);
  expect(isDeleteQueued(REPO, "main")).toBe(false);
  forgetEchoes();
  expect(isDeleteQueued(REPO, "chore/bump-tokio")).toBe(false);
});
