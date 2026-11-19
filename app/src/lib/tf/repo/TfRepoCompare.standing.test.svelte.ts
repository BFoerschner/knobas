/**
 * Compare shows once its branch's standing has answered (the ux20 Windows pass, 2026-11-19): the form is *New pull
 * request* with Title and Description until the standing says a pull request is open from the branch, and *Compare*
 * without them after, so drawn before it the diff under the form stood 175 px low for a frame and jumped up. The page
 * waits unseen in its room for the standing (`heldHead`, at most `HEAD_HOLD_MS`), then shows whole.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { RangeFiles, ShownView, SourceView } from "../../ipc/entity";
import { HEAD_HOLD_MS } from "../held-head.svelte";
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
  vi.useRealTimers();
  vi.restoreAllMocks();
});

const withPr: SourceView = {
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
const withoutPr: SourceView = { sections: [], actions: [] };

function draw(head: string, answer: () => Promise<ShownView | SourceView | null>) {
  const ports = {
    entityView: () => answer(),
    behindView: () => answer(),
    getEntity: () => new Promise<never>(() => {}),
    listSources: async () => [],
    submitWrite: async () => ({ id: 7, state: "queued" }) as never,
    rangeFiles: () => new Promise<RangeFiles>(() => {}),
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
      head,
      defaultBranch: "main",
      shown: true,
    },
  });
  flushSync();
}

async function settle() {
  for (let round = 0; round < 3; round += 1) {
    for (let i = 0; i < 10; i += 1) await Promise.resolve();
    await new Promise((resolve) => setTimeout(resolve, 0));
    flushSync();
  }
}

const page = () => target.querySelector<HTMLElement>(".tf-compare")!;
const heading = () => target.querySelector(".tf-compare > div > h2")?.textContent?.trim();

test("a branch with a pull request open is held unseen until its standing answers, then shows as Compare", async () => {
  let answer!: (v: SourceView) => void;
  draw("feature/a", () => new Promise((res) => (answer = res)));
  await settle();
  expect(page().classList.contains("tf-heldbelow"), "held while the standing is read").toBe(true);
  expect(heading()).toBe("New pull request");
  answer(withPr);
  await settle();
  expect(page().classList.contains("tf-heldbelow")).toBe(false);
  expect(heading(), "its first face shown is its last").toBe("Compare");
  expect(target.querySelector("#tf-cmp-title")).toBeNull();
});

test("a standing that fails shows the page as it is", async () => {
  let fail!: (e: unknown) => void;
  draw("feature/a", () => new Promise((_res, rej) => (fail = rej)));
  await settle();
  expect(page().classList.contains("tf-heldbelow")).toBe(true);
  fail(new Error("gone"));
  await settle();
  expect(page().classList.contains("tf-heldbelow")).toBe(false);
  expect(heading()).toBe("New pull request");
});

test("a standing slower than the hold does not hide the page longer", async () => {
  vi.useFakeTimers();
  draw("feature/a", () => new Promise(() => {}));
  flushSync();
  expect(page().classList.contains("tf-heldbelow")).toBe(true);
  vi.advanceTimersByTime(HEAD_HOLD_MS + 1);
  flushSync();
  expect(page().classList.contains("tf-heldbelow")).toBe(false);
});

test("the default branch reads no standing and shows at once", async () => {
  draw("main", () => Promise.resolve(withoutPr));
  expect(page().classList.contains("tf-heldbelow")).toBe(false);
});

test("Title takes the focus once the page shows, for a branch with no pull request", async () => {
  // jsdom lays nothing out; the field counts as drawn, so only the hold keeps the focus off it while the page is unseen.
  vi.spyOn(Element.prototype, "getClientRects").mockReturnValue([new DOMRect()] as unknown as DOMRectList);
  let answer!: (v: SourceView) => void;
  draw("feature/a", () => new Promise((res) => (answer = res)));
  await settle();
  expect(document.activeElement?.id).not.toBe("tf-cmp-title");
  answer(withoutPr);
  await settle();
  expect(page().classList.contains("tf-heldbelow")).toBe(false);
  expect(document.activeElement?.id).toBe("tf-cmp-title");
});
