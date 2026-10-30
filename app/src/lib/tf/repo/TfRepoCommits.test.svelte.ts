/**
 * *Commits*' range pick (#2271): click one commit's box, shift-click
 * another's, and *Diff selected* opens the diff address from the commit
 * before the oldest picked to the newest.
 */
import { flushSync, mount, tick, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import type { SourceView, ViewRow } from "../../ipc/entity";
import { diffHash } from "../diff/address";
import TfRepoCommits from "./TfRepoCommits.svelte";

const REPO = "gitea:tidewater/payout-service";
const SHAS = [
  "1262a5b610480702cfed4a1f3f76c87bf68fae55",
  "08232cb289a2d05c5813b08948b2063901c33ba7",
  "97cb5a8667f8aac8e6cdaeff83ffec9574f56a4b",
  "907900b245f2941018dfd3ee155b876ef701c102",
];

function commit(sha: string, title: string): ViewRow {
  return { entity_id: `${REPO}@${sha}`, title, meta: "mara · 3 hours ago", tone: null, chip: sha.slice(0, 10), url: null, commit: null } as ViewRow;
}

const VIEW = {
  sections: [
    {
      id: "commits",
      title: "Commits",
      count: 4,
      deferred: false,
      blocks: [{ type: "entities", rows: SHAS.map((sha, n) => commit(sha, `commit ${n}`)) }],
    },
  ],
  actions: [],
} as unknown as SourceView;

let target: HTMLElement;
let view: ReturnType<typeof mount> | null = null;

beforeEach(async () => {
  location.hash = "";
  target = document.createElement("div");
  document.body.append(target);
  view = mount(TfRepoCommits, {
    target,
    props: {
      ports: { entityView: async () => VIEW, codeFacts: async () => [] },
      repoId: REPO,
      at: REPO,
      branch: "main",
      defaultBranch: "main",
    },
  });
  for (let n = 0; n < 5; n++) await tick();
  flushSync();
});
afterEach(() => {
  if (view) unmount(view);
  view = null;
  target.remove();
});

const boxes = () => [...target.querySelectorAll<HTMLInputElement>("tbody input[type=checkbox]")];
const diffSelected = () =>
  [...target.querySelectorAll<HTMLButtonElement>("button")].find((button) => button.textContent!.startsWith("Diff selected"))!;

function click(box: HTMLInputElement, shiftKey = false) {
  box.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true, shiftKey }));
  flushSync();
}

test("nothing picked, Diff selected is offered disabled", () => {
  expect(boxes()).toHaveLength(4);
  expect(diffSelected().disabled).toBe(true);
});

test("a click and a shift-click pick three commits, and Diff selected opens the address for that range", () => {
  click(boxes()[0]!);
  click(boxes()[2]!, true);
  expect(boxes().map((box) => box.checked)).toEqual([true, true, true, false]);
  expect(diffSelected().disabled).toBe(false);
  expect(diffSelected().textContent).toBe("Diff selected (3)");
  diffSelected().click();
  expect(location.hash).toBe(diffHash(REPO, `${SHAS[2]}^`, SHAS[0]!));
  // Picking a commit's box does not open that commit.
  expect(location.hash.startsWith("#/diff/")).toBe(true);
});

test("picked from the bottom up, the range is the same", () => {
  click(boxes()[3]!);
  click(boxes()[1]!, true);
  diffSelected().click();
  expect(location.hash).toBe(diffHash(REPO, `${SHAS[3]}^`, SHAS[1]!));
});
