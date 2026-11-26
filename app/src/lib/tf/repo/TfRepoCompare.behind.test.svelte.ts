/**
 * Compare's branch standing is read behind the compare (the ux16 overs pass, 2026-11-17,
 * `docs/decisions/2026-11-17-a-page-s-waited-read-goes-first.md`): on Björn's PC its eight requests to Gitea went out with
 * the compare's two, and the compare took 450 to 600 ms where it takes 40 to 90 alone. The standing is asked through
 * `behindView`, whose requests wait for the page's own; the compare's list through `rangeFiles` as before.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import type { RangeFiles, ShownView, SourceView } from "../../ipc/entity";
import TfRepoCompare from "./TfRepoCompare.svelte";
import { forgetEarlyCompares } from "./compare-early";

const REPO = "gitea:tidewater/payout-service";
const BRANCH = `${REPO}@refs/heads/feature/a`;

let target: HTMLElement;
let view: ReturnType<typeof mount> | null = null;

beforeEach(() => {
  target = document.createElement("div");
  document.body.appendChild(target);
});

afterEach(() => {
  if (view) unmount(view);
  view = null;
  target.remove();
  location.hash = "";
  forgetEarlyCompares();
});

const standing: SourceView = {
  sections: [
    {
      id: "overview",
      title: "Overview",
      count: null,
      deferred: false,
      blocks: [{ type: "entities", title: null, rows: [{ entity_id: `${REPO}#12`, title: "Retry SEPA payouts", meta: null, chip: "open", tone: null, url: null }] }],
    },
  ],
  actions: [],
};

function draw(withBehind: boolean) {
  const asked: { how: "view" | "behind"; id: string; section: string | null }[] = [];
  const lists: string[] = [];
  const ports = {
    entityView: (id: string, section: string | null): Promise<ShownView | SourceView | null> => {
      asked.push({ how: "view", id, section });
      return Promise.resolve(standing);
    },
    ...(withBehind
      ? {
          behindView: (id: string, section: string | null): Promise<ShownView | SourceView | null> => {
            asked.push({ how: "behind", id, section });
            return Promise.resolve(standing);
          },
        }
      : {}),
    getEntity: () => new Promise<never>(() => {}),
    listSources: async () => [],
    submitWrite: async () => ({ id: 7, state: "queued" }) as never,
    rangeFiles: (_repo: string, _base: string, head: string) => {
      lists.push(head);
      return new Promise<RangeFiles>(() => {});
    },
    rangeFile: () => new Promise<never>(() => {}),
    sourceRangeFiles: () => new Promise<RangeFiles>(() => {}),
    cancelRead: async () => true,
    listen: async () => () => {},
  };
  view = mount(TfRepoCompare, {
    target,
    props: {
      ports,
      repoId: REPO,
      branches: [
        { name: "main", entityId: `${REPO}@refs/heads/main`, isDefault: true },
        { name: "feature/a", entityId: BRANCH, isDefault: false },
      ],
      head: "feature/a",
      defaultBranch: "main",
    },
  });
  flushSync();
  return { asked, lists };
}

async function settle() {
  for (let round = 0; round < 3; round += 1) {
    for (let i = 0; i < 10; i += 1) await Promise.resolve();
    await new Promise((resolve) => setTimeout(resolve, 0));
    flushSync();
  }
}

test("the branch's standing is read behind the compare, the compare's list as a person's read", async () => {
  const { asked, lists } = draw(true);
  await settle();
  expect(lists, "the compare's list goes out at once").toEqual(["feature/a"]);
  expect(asked, "the standing through the behind read, never a person's").toEqual([{ how: "behind", id: BRANCH, section: null }]);
  // What it answers draws as before: the pull request open from the branch.
  expect(target.textContent).toContain("PR #12 is already open from this branch.");
});

test("ports with no behind read read the standing as before", async () => {
  const { asked } = draw(false);
  await settle();
  expect(asked).toEqual([{ how: "view", id: BRANCH, section: null }]);
  expect(target.textContent).toContain("PR #12 is already open from this branch.");
});
