/**
 * A landing whose top listing came first takes the README guess its press asked (the ux16 overs pass, 2026-11-17,
 * `docs/decisions/2026-11-17-a-page-s-waited-read-goes-first.md`): on Björn's PC the press's listing landed with the
 * page's build, so the landing never asked its guess and read the README by the listing's section once the listing was
 * in, a second round behind the build (130 ms more at 6x). `readLandingAhead` asks the guess at the press; the landing
 * takes it wherever the listing names that README, and reads the listing's section only where the press asked nothing.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { SourceView, TreeEntry } from "../../ipc/entity";
import TfRepoFiles from "./TfRepoFiles.svelte";
import { forgetAllDrawnFiles } from "./drawn-files";

const pressed = vi.hoisted(() => ({ asked: false }));
vi.mock("./hover-reads", async (original) => ({
  ...(await original<typeof import("./hover-reads")>()),
  landingGuess: () => (pressed.asked ? "file:README.md" : null),
}));

const REPO = "gitea:tidewater/payout-service";
const SHA = "c0mmit";
const top: TreeEntry[] = [
  { name: "src", kind: "dir", size: null, section: `files@${SHA}:src` },
  { name: "README.md", kind: "file", size: 9, section: `file@${SHA}:README.md` },
];

const reads: string[] = [];
async function entityView(_at: string, section: string | null): Promise<SourceView> {
  const id = section ?? "";
  reads.push(id);
  const one = (blocks: unknown[]) => ({ sections: [{ id, title: "Files", count: null, deferred: false, blocks }], actions: [] }) as unknown as SourceView;
  if (id === "files") return one([{ type: "tree", trail: [], entries: top, truncated: false, url: null, commit: SHA }]);
  return one([{ type: "file", trail: [], path: "README.md", text: "# payout\n", language: null, size: 9, truncated: false, binary: false, url: null, sha: "b10b", commit: SHA }]);
}

let target: HTMLElement;
let view: ReturnType<typeof mount> | null = null;

beforeEach(() => {
  reads.length = 0;
  forgetAllDrawnFiles();
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

/** The landing with its top listing in before the README is asked, as when the press's read of it came back first. */
async function land() {
  const ports = {
    entityView,
    localDir: async () => null,
    submitWrite: async () => {
      throw { code: "invalid", message: "no write here" };
    },
  };
  const props = $state({
    ports,
    repoId: REPO,
    at: REPO,
    branch: "main",
    path: null,
    line: null,
    changed: new Set<string>(),
    treeSections: null as { dir: string; file: string } | null,
  });
  view = mount(TfRepoFiles, { target, props });
  flushSync();
  await settle();
}

test("a landing takes the README guess its press asked, where its listing came first and names that README", async () => {
  pressed.asked = true;
  await land();
  expect(target.querySelector(".tf-fileview .tf-md")?.textContent).toContain("payout");
  expect(reads.filter((id) => id.endsWith("README.md")), "the press's guess, not the listing's section").toEqual(["file:README.md"]);
});

test("with no guess asked at the press, the README is the listing's section, as before", async () => {
  pressed.asked = false;
  await land();
  expect(target.querySelector(".tf-fileview .tf-md")?.textContent).toContain("payout");
  expect(reads.filter((id) => id.endsWith("README.md"))).toEqual([`file@${SHA}:README.md`]);
});
