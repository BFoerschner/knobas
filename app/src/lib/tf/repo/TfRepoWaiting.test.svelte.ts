/**
 * Waiting states on the repository page's tabs (#2246): a first read draws
 * skeleton rows, a read that replaces a tab's content keeps it dimmed, and a
 * failed read draws its error in place of either. Each tab is mounted on its
 * own over a read the test holds back.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import type { SourceView, ViewRow } from "../../ipc/entity";
import TfRepoCommits from "./TfRepoCommits.svelte";

const REPO = "gitea:tidewater/payout-service";
const FEATURE = `${REPO}@refs/heads/feature/x`;

const vrow = (title: string, chip: string): ViewRow => ({ entity_id: null, title, meta: "mara · 1 day ago", tone: null, chip, url: null });
const commits = (rows: ViewRow[]): SourceView => ({
  sections: [{ id: "commits", title: "commits", count: null, deferred: false, blocks: [{ type: "entities", title: null, rows }] }],
  actions: [],
});

/** A read the test answers when it chooses. */
function held<T>() {
  let answer!: (value: T) => void;
  let refuse!: (cause: unknown) => void;
  const promise = new Promise<T>((ok, no) => {
    answer = ok;
    refuse = no;
  });
  return { promise, answer, refuse };
}

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
});

async function settle() {
  for (let i = 0; i < 10; i += 1) await Promise.resolve();
  flushSync();
}

const text = (el: Element | null) => el?.textContent?.replace(/\s+/g, " ").trim() ?? "";

/** Commits over reads keyed by where they are read, each held until answered. */
function commitsTab(reads: Map<string, ReturnType<typeof held<SourceView>>>) {
  const props = $state({
    ports: {
      entityView: (id: string) => reads.get(id)!.promise,
      codeFacts: async () => [],
    },
    repoId: REPO,
    at: REPO,
    branch: "main" as string | null,
    defaultBranch: "main" as string | null,
  });
  view = mount(TfRepoCommits, { target, props });
  return props;
}

test("Commits opened for the first time with the read held back draws skeleton rows, then the commits", async () => {
  const main = held<SourceView>();
  commitsTab(new Map([[REPO, main]]));
  await settle();
  expect(target.querySelectorAll(".tf-skel tbody tr").length).toBeGreaterThan(0);
  expect(target.querySelector(".tf-empty")).toBeNull();

  main.answer(commits([vrow("Bump deps", "a41f2c0000")]));
  await settle();
  expect(target.querySelector(".tf-skel")).toBeNull();
  expect([...target.querySelectorAll(".tf-rcommits tr")].map(text)).toEqual(["a41f2c0000Bump depsmara · 1 day ago"]);
});

test("another branch's commits keep the old ones drawn, dimmed and inert, until they arrive", async () => {
  const main = held<SourceView>();
  const feature = held<SourceView>();
  const props = commitsTab(new Map([[REPO, main], [FEATURE, feature]]));
  main.answer(commits([vrow("Bump deps", "a41f2c0000")]));
  await settle();

  props.at = FEATURE;
  props.branch = "feature/x";
  await settle();
  const dimmed = target.querySelector(".tf-wait-dim");
  expect(dimmed).not.toBeNull();
  expect((dimmed as HTMLElement).inert).toBe(true);
  expect([...dimmed!.querySelectorAll(".tf-rcommits tr")].map(text)).toEqual(["a41f2c0000Bump depsmara · 1 day ago"]);
  expect(target.querySelector(".tf-skel")).toBeNull();

  feature.answer(commits([vrow("Only on the feature", "7be0e40000")]));
  await settle();
  expect(target.querySelector(".tf-wait-dim")).toBeNull();
  expect([...target.querySelectorAll(".tf-rcommits tr")].map(text)).toEqual(["7be0e40000Only on the featuremara · 1 day ago"]);
});

test("a read that fails replaces the dimmed commits with its error, not an empty pane", async () => {
  const main = held<SourceView>();
  const feature = held<SourceView>();
  const props = commitsTab(new Map([[REPO, main], [FEATURE, feature]]));
  main.answer(commits([vrow("Bump deps", "a41f2c0000")]));
  await settle();

  props.at = FEATURE;
  await settle();
  feature.refuse({ code: "unavailable", message: "Gitea did not answer" });
  await settle();
  expect(text(target.querySelector(".tf-cfail"))).toContain("Gitea did not answer");
  expect(target.querySelector(".tf-rcommits")).toBeNull();
  expect(target.querySelector(".tf-wait-dim")).toBeNull();
});
