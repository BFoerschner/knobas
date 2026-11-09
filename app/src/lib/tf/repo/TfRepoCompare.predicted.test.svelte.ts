/**
 * *New pull request*'s diff predicted at the checkout and checked behind
 * (ADR-0041, the smoothness pass of PR #2479): the diff the checkout reads
 * between the tips the mirror last synced draws at once, the source's compare
 * is asked behind it, and a branch that moved since is told, on the page and,
 * for a pull request already sent, in a toast.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import type { DiffFile, RangeDiff, SourceView } from "../../ipc/entity";
import { toasts } from "../../shell/toasts.svelte";
import TfRepoCompare from "./TfRepoCompare.svelte";

const REPO = "gitea:tidewater/payout-service";
const BRANCH = "feature/PAY-231-sepa-retry";
const BASE = "ba5e000000000000000000000000000000000000";
const HEAD = "4ead000000000000000000000000000000000000";

const file = (path: string, additions = 1): DiffFile => ({ path, old_path: null, change: "modified", additions, deletions: 0, hunks: [], truncated: false, threads: [] });
const diff = (files: DiffFile[], head = HEAD, predicted?: string): RangeDiff => ({
  files,
  truncated: false,
  range: { repository: REPO, base: BASE, head, commits: [] },
  read_at: predicted === undefined ? "source" : "checkout",
  ...(predicted === undefined ? {} : { predicted_at: predicted }),
});

let target: HTMLElement;
let view: ReturnType<typeof mount> | null = null;
let check: { resolve: (d: RangeDiff) => void } | null;
let sent: unknown[];

beforeEach(() => {
  check = null;
  sent = [];
  toasts.items.splice(0);
  target = document.createElement("div");
  document.body.appendChild(target);
});

afterEach(() => {
  if (view) unmount(view);
  view = null;
  target.remove();
  location.hash = "";
});

function draw(answer: RangeDiff, withCheck = true, standing: () => Promise<SourceView | null> = () => new Promise<SourceView | null>(() => {})) {
  const ports = {
    entityView: standing,
    getEntity: () => new Promise<never>(() => {}),
    listSources: async () => [],
    submitWrite: async (payload: unknown) => {
      sent.push(payload);
      return { id: 7, state: "queued" } as never;
    },
    rangeDiff: async () => answer,
    ...(withCheck ? { sourceRangeDiff: () => new Promise<RangeDiff>((resolve) => (check = { resolve })) } : {}),
    listen: async () => () => {},
  };
  view = mount(TfRepoCompare, {
    target,
    props: {
      ports,
      repoId: REPO,
      branches: [
        { name: "main", entityId: `${REPO}@refs/heads/main`, isDefault: true },
        { name: BRANCH, entityId: `${REPO}@refs/heads/${BRANCH}`, isDefault: false },
      ],
      head: BRANCH,
      defaultBranch: "main",
    },
  });
  flushSync();
}

async function settle() {
  for (let round = 0; round < 4; round += 1) {
    for (let i = 0; i < 10; i += 1) await Promise.resolve();
    await new Promise((resolve) => setTimeout(resolve, 0));
    flushSync();
  }
}

const text = (el: Element | null) => el?.textContent?.replace(/\s+/g, " ").trim() ?? "";
const note = () => [...target.querySelectorAll(".tf-cdiff .tf-cnote")].map(text);

test("the predicted diff draws at once and says it is checked when the source agrees", async () => {
  draw(diff([file("pay.py")], HEAD, "2026-11-09T10:00:00Z"));
  await settle();
  expect(note()).toEqual(["Read from your clone as of the last sync; checking the code host…"]);
  expect(text(target.querySelector(".tf-cdiff"))).toContain("pay.py");

  check!.resolve(diff([file("pay.py")]));
  await settle();
  expect(note()).toEqual(["Read from your clone: the same as on the code host."]);
});

test("a branch that moved since the sync: the source's diff replaces the prediction in place, and the page says so", async () => {
  draw(diff([file("pay.py")], HEAD, "2026-11-09T10:00:00Z"));
  await settle();
  check!.resolve(diff([file("pay.py"), file("retry.py")], "9999000000000000000000000000000000000000"));
  await settle();
  expect(note()).toEqual([`${BRANCH} or main moved on the code host since the last sync: this is the change as it stands there now.`]);
  expect(text(target.querySelector(".tf-cdiff"))).toContain("retry.py");
});

test("a pull request sent before the check answered is told when the check finds the branch moved", async () => {
  draw(diff([file("pay.py")], HEAD, "2026-11-09T10:00:00Z"));
  await settle();
  const title = target.querySelector<HTMLInputElement>("#tf-cmp-title")!;
  title.value = "PAY-231: SEPA retry";
  title.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
  target.querySelector<HTMLFormElement>(".tf-cform")!.requestSubmit();
  await settle();
  expect(sent).toHaveLength(1);
  expect(toasts.items.some((t) => t.text.includes("moved since the last sync"))).toBe(false);

  check!.resolve(diff([file("pay.py"), file("retry.py")], "9999000000000000000000000000000000000000"));
  await settle();
  expect(toasts.items.map((t) => t.text)).toContainEqual(
    `“PAY-231: SEPA retry” was opened from ${BRANCH} as the code host holds it now, which moved since the last sync: its changes differ from the ones this page showed. Review its files.`,
  );
});

test("a pull request sent before a check that agrees is not told anything more", async () => {
  draw(diff([file("pay.py")], HEAD, "2026-11-09T10:00:00Z"));
  await settle();
  const title = target.querySelector<HTMLInputElement>("#tf-cmp-title")!;
  title.value = "PAY-231: SEPA retry";
  title.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
  target.querySelector<HTMLFormElement>(".tf-cform")!.requestSubmit();
  await settle();
  check!.resolve(diff([file("pay.py")]));
  await settle();
  expect(toasts.items.some((t) => t.text.includes("moved since the last sync"))).toBe(false);
});

test("a diff the source answered is not checked again", async () => {
  draw(diff([file("pay.py")]));
  await settle();
  expect(check).toBeNull();
  expect(note()).toEqual([]);
});

test("a branch whose standing the source cannot read leaves the clone's diff drawn, with no error (ADR-0042)", async () => {
  draw(diff([file("pay.py")], HEAD, "2026-11-09T10:00:00Z"), false, () => Promise.reject({ code: "unreachable", message: "connection refused" }));
  await settle();
  expect(text(target.querySelector(".tf-cdiff"))).toContain("pay.py");
  expect(target.querySelector(".tf-cdiff .tf-cfail")).toBeNull();
  expect(text(target.querySelector(".tf-cdiff"))).not.toContain("connection refused");
});
