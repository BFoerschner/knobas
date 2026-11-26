/**
 * Compare draws in its final shape at once (the ux20 Windows pass, 2026-11-19; the ux21 overs pass): the form is *New pull
 * request* with Title and Description where no pull request is open from the branch, and *Compare* without them where
 * one is, so drawn before the branch's standing said which, the diff under the form stood 175 px low for a frame and
 * jumped up. The form takes its shape from the mirror (`repo_branch_pulls`, read by the repository's page) until the
 * standing, a forge round trip, answers; the page waits unseen only while the mirror's read is out, at most 100 ms.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { BranchPull, RangeFiles, ShownView, SourceView } from "../../ipc/entity";
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

/** The mirror's branches: `feature/a` with pull request 12 open from it, or with none. */
const mirror = (open: boolean): BranchPull[] => [
  { entity_id: `${REPO}@refs/heads/main`, name: "main", tip: "aaa", is_default: true, pull: null },
  { entity_id: BRANCH, name: "feature/a", tip: "bbb", is_default: false, pull: open ? { branch: "feature/a", entity_id: `${REPO}#12`, cell: null } : null },
  { entity_id: `${REPO}@refs/heads/feature/PAY-7-retry`, name: "feature/PAY-7-retry", tip: "ccc", is_default: false, pull: null },
];
/** The mirror's read as the repository's page holds it: `null` while it is out. */
let pulls = $state<BranchPull[] | "failed" | null>(null);

function draw(head: string, answer: () => Promise<ShownView | SourceView | null>, mirrored: BranchPull[] | "failed" | null = mirror(false)) {
  pulls = mirrored;
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
        { name: "feature/PAY-7-retry", entityId: `${REPO}@refs/heads/feature/PAY-7-retry`, isDefault: false },
      ],
      head,
      defaultBranch: "main",
      shown: true,
      get pulls() {
        return pulls;
      },
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
const held = () => page().classList.contains("tf-heldbelow");

test("a branch the mirror holds a pull request open from shows as Compare at once, before its standing answers", async () => {
  let answer!: (v: SourceView) => void;
  draw("feature/a", () => new Promise((res) => (answer = res)), mirror(true));
  await settle();
  expect(held(), "no wait for the forge's standing").toBe(false);
  expect(heading()).toBe("Compare");
  expect(target.querySelector("#tf-cmp-title")).toBeNull();
  expect(target.querySelector(".tf-cexists")?.textContent).toContain("PR #12 is already open");
  answer(withPr);
  await settle();
  expect(heading(), "its first face shown is its last").toBe("Compare");
});

test("a branch the mirror holds none open from shows as New pull request at once", async () => {
  draw("feature/a", () => new Promise(() => {}), mirror(false));
  await settle();
  expect(held()).toBe(false);
  expect(heading()).toBe("New pull request");
  expect(target.querySelector("#tf-cmp-title")).not.toBeNull();
});

test("the standing, once it answers, says what the form is where the mirror had not caught up", async () => {
  let answer!: (v: SourceView) => void;
  draw("feature/a", () => new Promise((res) => (answer = res)), mirror(false));
  await settle();
  answer(withPr);
  await settle();
  expect(heading()).toBe("Compare");
});

test("opened before the mirror's read answered, the page waits unseen for it, then shows in its shape", async () => {
  draw("feature/a", () => new Promise(() => {}), null);
  await settle();
  expect(held(), "held while the mirror is read").toBe(true);
  pulls = mirror(true);
  await settle();
  expect(held()).toBe(false);
  expect(heading()).toBe("Compare");
});

test("a mirror slower than the hold does not hide the page longer, nor does a failed one", async () => {
  vi.useFakeTimers();
  draw("feature/a", () => new Promise(() => {}), null);
  flushSync();
  expect(held()).toBe(true);
  // The hold is 100 ms.
  vi.advanceTimersByTime(101);
  flushSync();
  expect(held()).toBe(false);
  vi.useRealTimers();
  if (view) unmount(view);
  draw("feature/a", () => new Promise(() => {}), "failed");
  expect(held()).toBe(false);
  expect(heading()).toBe("New pull request");
});

test("the default branch reads no standing and shows at once", async () => {
  draw("main", () => Promise.resolve(withoutPr), null);
  expect(held()).toBe(false);
});

test("a branch that names a ticket keeps the room of the ticket's link while the ticket is looked up", async () => {
  // Drawn when the ticket answered, *Link the PR to …* grew the form a line and moved the diff under it.
  draw("feature/PAY-7-retry", () => new Promise(() => {}));
  await settle();
  const room = target.querySelector<HTMLElement>(".tf-cheld");
  expect(room?.textContent).toContain("PAY-7");
  expect(room?.getAttribute("aria-hidden")).toBe("true");
  expect(room?.querySelector("input")?.tabIndex).toBe(-1);
  // A branch with no key holds none.
  if (view) unmount(view);
  draw("feature/a", () => new Promise(() => {}));
  await settle();
  expect(target.querySelector(".tf-cheld")).toBeNull();
});

test("Title takes the focus once the page shows, for a branch with no pull request", async () => {
  // jsdom lays nothing out; the field counts as drawn, so only the hold keeps the focus off it while the page is unseen.
  vi.spyOn(Element.prototype, "getClientRects").mockReturnValue([new DOMRect()] as unknown as DOMRectList);
  draw("feature/a", () => new Promise(() => {}), null);
  await settle();
  expect(document.activeElement?.id).not.toBe("tf-cmp-title");
  pulls = mirror(false);
  await settle();
  expect(held()).toBe(false);
  expect(document.activeElement?.id).toBe("tf-cmp-title");
});
