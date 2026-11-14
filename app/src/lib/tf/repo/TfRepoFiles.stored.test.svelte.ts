/**
 * The Files tab over a tree the sync stored (the stored-trees pass of PR #2479, 2026-11-14): a top answered from what
 * knobas holds (`held_at`) draws the held line over the tree until the read behind lands, and a branch past what knobas
 * keeps (`tree_unkept`) says so under the tree.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import type { SourceView } from "../../ipc/entity";
import TfRepoFiles from "./TfRepoFiles.svelte";

const REPO = "gitea:tidewater/payout-service";
const TIP = "c0ffee00c0ffee00c0ffee00c0ffee00c0ffee00";

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
  for (let round = 0; round < 4; round += 1) {
    for (let i = 0; i < 10; i += 1) await Promise.resolve();
    await new Promise((resolve) => setTimeout(resolve, 0));
    flushSync();
  }
}

const text = (el: Element | null) =>
  el?.textContent?.replace(/\s+/g, " ").trim() ?? "";
const minutesAgo = (n: number) =>
  new Date(Date.now() - n * 60_000).toISOString();

/** The top at the tip, answered with `extra` (`held_at`, `tree_unkept`) on its first read and plainly after. */
function source(extra: Record<string, unknown>) {
  const reads: string[] = [];
  const entityView = async (_at: string, section: string | null) => {
    const id = section ?? "";
    reads.push(id);
    const first = reads.filter((read) => read === id).length === 1;
    return {
      sections: [
        {
          id,
          title: "Files",
          count: null,
          deferred: false,
          blocks: [
            {
              type: "tree",
              trail: [],
              entries: [
                {
                  name: "README.md",
                  kind: "file",
                  size: 5,
                  section: `file@${TIP}:README.md`,
                },
              ],
              truncated: false,
              url: null,
              commit: TIP,
            },
          ],
        },
      ],
      actions: [],
      ...(first ? extra : {}),
    } as unknown as SourceView;
  };
  return { reads, entityView };
}

function draw(
  entityView: (at: string, section: string | null) => Promise<SourceView>,
) {
  const heard: ((event: { payload: unknown }) => void)[] = [];
  const ports = {
    entityView,
    submitWrite: async () => {
      throw { code: "invalid", message: "no write in this suite" };
    },
    listen: async (
      event: string,
      handler: (event: { payload: unknown }) => void,
    ) => {
      if (event === "view:fresh") heard.push(handler);
      return () => {};
    },
  };
  view = mount(TfRepoFiles, {
    target,
    props: {
      ports,
      repoId: REPO,
      at: REPO,
      branch: "main",
      path: null,
      changed: new Set<string>(),
      source: { id: "gitea", display_name: "Tidewater Git" } as never,
    },
  });
  flushSync();
  return (payload: Record<string, unknown>) => {
    for (const hear of heard)
      hear({ payload: { entity_id: REPO, section: "files", ...payload } });
  };
}

const held = () => target.querySelector(".tf-ftreebox .tf-heldnote");
const notes = () =>
  [...target.querySelectorAll(".tf-ftreebox .tf-gonote")].map(text);

test("a tree drawn from what knobas holds says from when, and the line's words go when the read behind lands", async () => {
  const forge = source({ held_at: minutesAgo(4) });
  const tell = draw(forge.entityView);
  await settle();
  expect(text(held())).toBe(
    "As knobas read it 4 min ago. Checking Tidewater Git…",
  );

  tell({});
  await settle();
  expect(forge.reads.filter((read) => read === "files")).toHaveLength(2);
  expect(text(held())).toBe("");
  expect(
    held(),
    "the line keeps its room, so the rows do not move",
  ).not.toBeNull();
});

test("a read behind that failed says so on the held line", async () => {
  const tell = draw(source({ held_at: minutesAgo(10) }).entityView);
  await settle();
  tell({ failed: "connection refused" });
  await settle();
  expect(text(held())).toBe(
    "As knobas read it 10 min ago. Tidewater Git did not answer: connection refused",
  );
});

test("a tree read now draws no held line", async () => {
  draw(source({}).entityView);
  await settle();
  expect(held()).toBeNull();
});

test("a branch past what knobas keeps says so under the tree", async () => {
  draw(source({ tree_unkept: 123_456 }).entityView);
  await settle();
  expect(notes()).toContain(
    "This branch holds 123,456 entries, more than knobas keeps: its folders are read from Tidewater Git as they open.",
  );
});
