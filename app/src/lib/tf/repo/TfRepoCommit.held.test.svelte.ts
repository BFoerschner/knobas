/**
 * A commit's page when its source does not answer (ADR-0042): the mirror's row
 * and the clone's diff are the page, and the view only adds the parent and
 * *New branch from here…*, so a view that fails leaves the page drawn.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import type { EntityDetail, RangeDiff } from "../../ipc/entity";
import TfRepoCommit from "./TfRepoCommit.svelte";

const REPO = "gitea:tidewater/payout-service";
const SHA = "1262a5b610480702cfed4a1f3f76c87bf68fae55";
const COMMIT = `${REPO}@${SHA}`;

const DETAIL = {
  row: { entity_id: COMMIT, kind: "commit", source_id: "gitea", title: "Bound the SEPA retry jitter", updated_at: "2026-11-09T09:00:00Z" },
  author: "Mara Lindqvist",
} as unknown as EntityDetail;

const LOCAL: RangeDiff = {
  files: [{ path: "src/sepa/retry.rs", old_path: null, change: "modified", additions: 1, deletions: 1, hunks: [], truncated: false, threads: [] }],
  truncated: false,
  range: null,
  read_at: "checkout",
};

let target: HTMLElement;
let view: ReturnType<typeof mount> | null = null;

beforeEach(() => {
  target = document.createElement("div");
  document.body.append(target);
});
afterEach(() => {
  if (view) unmount(view);
  view = null;
  target.remove();
});

async function settle() {
  for (let n = 0; n < 4; n += 1) {
    await Promise.resolve();
    await new Promise((resolve) => setTimeout(resolve, 0));
  }
  flushSync();
}

test("a view the source cannot answer leaves the commit's row and its diff drawn, with no error", async () => {
  view = mount(TfRepoCommit, {
    target,
    props: {
      ports: {
        entityView: () => Promise.reject({ code: "unreachable", message: "connection refused" }),
        getEntity: async () => DETAIL,
        commitDiff: async () => LOCAL,
        fileBytes: async () => new Uint8Array(),
        piped: async () => ({}) as never,
      },
      repoId: REPO,
      commitId: COMMIT,
      now: new Date("2026-11-09T12:00:00Z"),
      onNewBranch: () => {},
    },
  });
  await settle();
  const text = target.textContent?.replace(/\s+/g, " ") ?? "";
  expect(text).toContain("Bound the SEPA retry jitter");
  expect(text).toContain("src/sepa/retry.rs");
  expect(text).not.toContain("could not be read");
  expect(text, "no branch offered without the source's view").not.toContain("New branch from here");
});
