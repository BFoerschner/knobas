/**
 * A kept repository page sees new branches (the third smoothness pass of PR
 * #2479). Found in the app's own window: a page kept open did not read its
 * branch list again on its return, so a branch made after it first drew was
 * unknown to Compare, *New pull request* and the header's list until a
 * refresh. Each test here failed before the fix.
 *
 * - Shown again, on another tab, after the window was away a while, when a
 *   run of the repository's source ends: the list and the pull requests are
 *   read again over what the page drew.
 * - A branch the source lists and the mirror has not synced is offered, read
 *   at the id the source named.
 * - A branch made through knobas is offered at once.
 * - Compare reads a branch the list lacks straight from the source, and says
 *   it is not there only once the source said so and no read of the list is
 *   still under way.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

const followed = vi.hoisted(() => ({ follower: null as import("../../shell/follow-write").WriteFollower | null }));
vi.mock("../../shell/follow-write", async (importOriginal) => {
  const real = await importOriginal<typeof import("../../shell/follow-write")>();
  return { ...real, writeFollower: () => followed.follower ?? real.writeFollower() };
});

import type { EntityDetail, EntityPage, EntityRow, ShownView, SourceView, ViewAction, ViewBlock, ViewRow } from "../../ipc/entity";
import type { QueuedWrite, WriteOpPayload } from "../../ipc/sources";
import { echoFollower } from "../echo-events.fixture";
import { forgetEchoes, type EchoListen } from "../queued.svelte";
import TfRepo, { forgetBranches, type RepoPorts } from "./TfRepo.svelte";

const NOW = new Date(2026, 10, 9, 14, 32);
const REPO = "gitea:tidewater/payout-service";
const at = (name: string) => `${REPO}@refs/heads/${name}`;
const MAIN = at("main");
const FEATURE = at("feature/PAY-231-sepa-retry");
const COMPARE = (branch: string) => [REPO, "compare", ...branch.split("/")];

let target: HTMLElement;
let view: ReturnType<typeof mount> | null = null;
let reads: [string, string | null][];
let writes: WriteOpPayload[];
/** Branches on the forge the mirror has not synced: listed with no id, named in `unsynced`. */
let forge: string[];
/** Pull requests the mirror holds. */
let prs: EntityRow[];
/** Held back, the next read of the branch list waits for it. */
let gate: Promise<void> | null;
/** The bridge's events, raised by the test as the backend would tell them. */
let heard: { event: string; handler: (event: { payload: unknown }) => void }[] = [];
const listen: EchoListen = async (event, handler) => {
  const entry = { event, handler };
  heard.push(entry);
  return () => void heard.splice(heard.indexOf(entry), 1);
};
async function raise(event: string, payload: unknown) {
  for (const entry of [...heard]) if (entry.event === event) entry.handler({ payload });
  await settle();
}
const events = {
  ended: (source: string) => raise("sync:state", { source_id: source, running: false }),
  refreshed: (entity: string, write: number | null) => raise("item:refreshed", { entity_id: entity, write_id: write, ended: "refreshed" }),
};

const live = $state<{ rest: string[]; visible: boolean }>({ rest: [], visible: true });

beforeEach(() => {
  forgetEchoes();
  forgetBranches();
  reads = [];
  writes = [];
  forge = [];
  prs = [pr(1, "SEPA retry")];
  gate = null;
  heard = [];
  location.hash = "";
  target = document.createElement("div");
  document.body.appendChild(target);
});

afterEach(() => {
  followed.follower = null;
  if (view) unmount(view);
  view = null;
  target.remove();
  document.querySelectorAll(".tf-modal").forEach((node) => node.remove());
  vi.restoreAllMocks();
});

function pr(n: number, title: string): EntityRow {
  return {
    entity_id: `${REPO}#${n}`,
    kind: "pr",
    source_id: "gitea",
    title,
    updated_at: "2026-11-09T10:30:00Z",
    synced_at: "2026-11-09T14:30:00Z",
    path: "tidewater/payout-service",
    state: { text: "open", tone: "info" },
  };
}

const vrow = (entity_id: string | null, title: string, chip: string | null = null, meta: string | null = null): ViewRow => ({
  entity_id,
  title,
  meta,
  tone: null,
  chip,
  url: null,
});

const one = (id: string, blocks: ViewBlock[]): SourceView => ({ sections: [{ id, title: id, count: null, deferred: false, blocks }], actions: [] });

const action = (write_op: string, label: string) =>
  ({ write_op, label, fields: [], confirm: null, primary: false, danger: false, disabled: null, with_settings: false, values: {}, entity: null }) as unknown as ViewAction;
const OPEN_PR = action("create_pull_request", "Open pull request…");
const DELETE = action("delete_branch", "Delete branch…");

/** A branch's own view: one commit ahead, a pull request offered. */
const branchView = (name: string): SourceView => ({
  sections: [
    { id: "overview", title: "Overview", count: null, deferred: false, blocks: [{ type: "facts", facts: [{ label: "Ahead", value: "1" }, { label: "Behind", value: "0" }] }] as ViewBlock[] },
    { id: "commits", title: "Commits", count: 1, deferred: false, blocks: [{ type: "entities", title: null, rows: [vrow(null, `Work on ${name}`, "abc1234")] }] },
  ],
  actions: [OPEN_PR, DELETE],
});

const diffOf = (name: string): ViewBlock => ({
  type: "diff",
  truncated: false,
  commentable: null,
  gone: [],
  files: [{ path: `${name}.txt`, old_path: null, change: "added", additions: 1, deletions: 0, hunks: [], truncated: false, threads: [] }],
  range: null,
} as unknown as ViewBlock);

function ports(over: Partial<RepoPorts> = {}): RepoPorts {
  return {
    getEntity: async (id) => {
      if (id === REPO)
        return {
          row: { ...pr(0, "tidewater/payout-service"), entity_id: REPO, kind: "repo", path: null, state: null },
          author: null,
          source: { id: "gitea", display_name: "Tidewater Gitea", adapter_kind: "gitea", enabled: true },
          refreshes: false,
          deleted_at: null,
        } as unknown as EntityDetail;
      throw { code: "not_found", message: `${id} is not in the mirror` };
    },
    listEntities: async (): Promise<EntityPage> => ({ rows: prs, total: prs.length, tally: null, groups: null }),
    entityView: async (id, section): Promise<ShownView | SourceView | null> => {
      reads.push([id, section]);
      if (id === REPO && section === "branches") {
        if (gate !== null) await gate;
        return {
          ...one("branches", [
            {
              type: "entities",
              title: null,
              // Newest tip first, as Gitea sorts them: a branch pushed since is above the rest.
              rows: [vrow(MAIN, "main", "default"), ...forge.map((name) => vrow(null, name, null, "1 minute ago")), vrow(FEATURE, "feature/PAY-231-sepa-retry")],
            },
          ]),
          unsynced: forge.map((name) => ({ section: "branches", title: name, entity_id: at(name) })),
        };
      }
      if (section === null && id.startsWith(`${REPO}@refs/heads/`)) return branchView(id.slice(`${REPO}@refs/heads/`.length));
      if (id === REPO && section?.startsWith("compare:")) {
        const head = section.slice(section.indexOf("...") + 3);
        if (head !== "feature/PAY-231-sepa-retry" && !forge.includes(head)) throw { code: "not_found", message: `the source holds no branch ${head}` };
        return one(section, [diffOf(head)]);
      }
      if (section === "files") return one("files", [{ type: "tree", trail: [], entries: [], truncated: false, url: null }]);
      if (section === "commits") return one("commits", [{ type: "entities", title: null, rows: [] }]);
      throw { code: "invalid", message: `no section ${section} of ${id}` };
    },
    entityCheckout: async () => ({ repo_entity_id: REPO, repo_url: null, path: null, found_by: "nothing", clones_root: null, clone_command: null, tree: null }),
    entityCheckoutDiff: async () => {
      throw { code: "not_found", message: "no checkout" };
    },
    setCheckoutOverride: async () => {
      throw { code: "invalid", message: "none" };
    },
    listContexts: async () => [],
    listSources: async () => [],
    jobReach: async () => [],
    submitWrite: async (payload) => {
      writes.push(payload);
      return { id: writes.length } as unknown as QueuedWrite;
    },
    codeFacts: async () => [],
    listen,
    ...over,
  };
}

async function drawLive(rest: string[], stand: RepoPorts = ports()) {
  live.rest = rest;
  live.visible = true;
  view = mount(TfRepo, {
    target,
    props: {
      ready: true,
      get rest() {
        return live.rest;
      },
      get visible() {
        return live.visible;
      },
      job: null,
      ports: stand,
      now: NOW,
    },
  });
  await settle();
}

async function settle() {
  for (let round = 0; round < 5; round += 1) {
    for (let i = 0; i < 10; i += 1) await Promise.resolve();
    await new Promise((resolve) => setTimeout(resolve, 0));
    flushSync();
  }
}

async function go(rest: string[]) {
  live.rest = rest;
  await settle();
}

/** Away and back, as the shell's keep-alive hides and shows the page. */
async function awayAndBack() {
  live.visible = false;
  await settle();
  live.visible = true;
  await settle();
}

const text = (el: Element | null) => el?.textContent?.replace(/\s+/g, " ").trim() ?? "";
const header = () => target.querySelector<HTMLSelectElement>("select[aria-label='Branch']")!;
const offered = () => [...header().options].map((option) => option.value);
const fromList = () => [...target.querySelector<HTMLSelectElement>("#tf-cmp-from")!.options].map((option) => option.value);
const branchReads = () => reads.filter(([id, section]) => id === REPO && section === "branches").length;
const diffPaths = () => [...target.querySelectorAll(".tf-cdiff .tf-fh .tf-mono")].map(text);

test("a page shown again reads its branch list again: a branch made on the forge meanwhile is in the header and Compare", async () => {
  await drawLive([REPO]);
  expect(offered()).toEqual(["main", "feature/PAY-231-sepa-retry"]);

  forge = ["fix/made-on-the-forge"];
  await awayAndBack();
  expect(offered()).toContain("fix/made-on-the-forge");

  await go(COMPARE("fix/made-on-the-forge"));
  expect(fromList()).toContain("fix/made-on-the-forge");
  // Not in the mirror yet: its standing is read at the id the source named, and its diff by name.
  expect(reads).toContainEqual([at("fix/made-on-the-forge"), null]);
  expect(text(target.querySelector(".tf-compare aside"))).toContain("Work on fix/made-on-the-forge");
  expect(diffPaths()).toEqual(["fix/made-on-the-forge.txt"]);
  expect(target.querySelector(".tf-cdiff .tf-empty")).toBeNull();
});

test("another tab shown reads the list again, and the header's New pull request knows the branch", async () => {
  await drawLive([REPO]);
  const before = branchReads();
  forge = ["fix/from-a-push"];
  await go([REPO, "commits"]);
  expect(branchReads()).toBeGreaterThan(before);
  expect(offered()).toContain("fix/from-a-push");
});

test("the window back after a while reads the lists again; a moment away reads nothing more", async () => {
  let clock = 1_000_000;
  vi.spyOn(Date, "now").mockImplementation(() => clock);
  await drawLive([REPO]);
  const before = branchReads();

  clock += 5_000;
  window.dispatchEvent(new Event("focus"));
  await settle();
  expect(branchReads()).toBe(before);

  forge = ["fix/while-away"];
  clock += 60_000;
  window.dispatchEvent(new Event("focus"));
  await settle();
  expect(branchReads()).toBe(before + 1);
  expect(offered()).toContain("fix/while-away");
});

test("a run of the repository's source ending reads its branches and its pull requests again", async () => {
  await drawLive([REPO, "prs"]);
  expect(text(target.querySelector(".tf-tabs a[href$='/prs']"))).toBe("Pull requests 1");

  forge = ["fix/synced-in"];
  prs = [pr(2, "Instant SEPA"), ...prs];
  await events.ended("another-source");
  expect(offered()).not.toContain("fix/synced-in");

  await events.ended("gitea");
  expect(offered()).toContain("fix/synced-in");
  expect(text(target.querySelector(".tf-tabs a[href$='/prs']"))).toBe("Pull requests 2");
  expect(text(target)).toContain("Instant SEPA");
});

test("a branch made through New branch is offered at once, and Compare says it is being made until a read lists it", async () => {
  const follow = echoFollower();
  followed.follower = follow.follower;
  await drawLive([REPO]);
  [...target.querySelectorAll<HTMLButtonElement>(".tf-fhead button")].find((b) => text(b) === "New branch…")!.click();
  await settle();
  const name = document.querySelector<HTMLInputElement>("#tf-nb-name")!;
  name.value = "feature/made-here";
  name.dispatchEvent(new Event("input", { bubbles: true }));
  await settle();
  document.querySelector<HTMLButtonElement>("button[form='tf-newbranch']")!.click();
  await settle();
  expect(writes).toEqual([{ CreateBranch: { entity: REPO, name: "feature/made-here", from_ref: "main" } }]);

  // In the header at once, not to read at yet.
  const made = [...header().options].find((option) => option.value === "feature/made-here")!;
  expect(made.disabled).toBe(true);
  expect(text(made)).toBe("feature/made-here (queued)");

  await go(COMPARE("feature/made-here"));
  expect(fromList()).toContain("feature/made-here");
  expect(text(target.querySelector(".tf-cdiff .tf-empty"))).toBe(
    "feature/made-here is being made on Tidewater Gitea (queued). What it changes shows here once it is there.",
  );
  expect(text(target)).not.toContain("has no branch");

  // It lands: the refresh after its write reads the list again, which holds it, and its diff is read.
  follow.sent(1);
  forge = ["feature/made-here"];
  await events.refreshed(REPO, 1);
  expect(diffPaths()).toEqual(["feature/made-here.txt"]);
  expect([...header().options].find((option) => option.value === "feature/made-here")!.disabled).toBe(false);
});

test("Compare never says a branch is missing while a read of the list that could find it is under way", async () => {
  await drawLive(COMPARE("fix/not-listed-yet"));
  // The source answered 404 and nothing else is reading: only now is it said.
  expect(text(target.querySelector(".tf-cdiff .tf-empty"))).toBe("Tidewater Gitea has no branch fix/not-listed-yet: it was deleted, renamed, or never pushed.");

  // The page is shown again and its list is read, slowly; the branch was pushed meanwhile.
  let open!: () => void;
  gate = new Promise((resolve) => (open = resolve));
  forge = ["fix/not-listed-yet"];
  live.visible = false;
  await settle();
  live.visible = true;
  await settle();
  expect(text(target)).not.toContain("has no branch");
  expect(target.querySelector(".tf-cdiff .tf-wait, .tf-cdiff [aria-busy='true']") ?? target.querySelector(".tf-cdiff .tf-cnote")).not.toBeNull();

  gate = null;
  open();
  await settle();
  expect(diffPaths()).toEqual(["fix/not-listed-yet.txt"]);
  expect(text(target)).not.toContain("has no branch");
});

test("Branches draws a held listing under the held line until the source answers, and says why when it does not", async () => {
  const OLD = "2026-11-09T12:00:00Z";
  let held = true;
  const base = ports();
  await drawLive([REPO, "branches"], {
    ...base,
    entityView: async (id, section) => {
      const answer = await base.entityView(id, section);
      return id === REPO && section === "branches" && held && answer !== null ? { ...answer, held_at: OLD } : answer;
    },
  });
  const line = () => text(target.querySelector(".tf-heldnote"));
  expect(line()).toMatch(/^As knobas read it .+\. Checking Tidewater Gitea…$/);

  await raise("view:fresh", { entity_id: REPO, section: "branches", failed: "connection refused" });
  expect(line()).toMatch(/^As knobas read it .+\. Tidewater Gitea did not answer: connection refused$/);

  // The read behind answered: the page reads the fresh listing, and the line goes.
  held = false;
  forge = ["fix/fresh"];
  await raise("view:fresh", { entity_id: REPO, section: "branches" });
  expect(target.querySelector(".tf-heldnote")).toBeNull();
  expect(text(target.querySelector("table"))).toContain("fix/fresh");
});

test("a branch the mirror lacks is a row with its standing, read at the source's id, and no Delete", async () => {
  forge = ["fix/on-the-forge"];
  await drawLive([REPO, "branches"]);
  const row = [...target.querySelectorAll("tr[data-place]")].find((tr) => text(tr).startsWith("fix/on-the-forge"))!;
  expect(reads).toContainEqual([at("fix/on-the-forge"), null]);
  expect(text(row)).toContain("1 ahead, 0 behind");
  expect([...row.querySelectorAll("a, button")].map(text)).toEqual(["New pull request"]);
  // A branch the mirror holds keeps its Delete.
  const feature = [...target.querySelectorAll("tr[data-place]")].find((tr) => text(tr).startsWith("feature/PAY-231"))!;
  expect([...feature.querySelectorAll("button")].map(text)).toContain("Delete…");
});

test("a list read again keeps the reader's place and focus: a row added above moves nothing they are on", async () => {
  await drawLive([REPO, "branches"]);
  const scroller = document.createElement("div");
  scroller.className = "tf-main";
  target.replaceWith(scroller);
  scroller.appendChild(target);
  const rows = () => [...target.querySelectorAll<HTMLElement>("tr[data-place]")];
  // jsdom lays nothing out: each row is 30 px under the one before it, the scroller's top at 0.
  vi.spyOn(Element.prototype, "getClientRects").mockReturnValue([new DOMRect()] as unknown as DOMRectList);
  vi.spyOn(Element.prototype, "getBoundingClientRect").mockImplementation(function (this: Element) {
    if (this === scroller) return new DOMRect(0, 0, 800, 600);
    const n = rows().indexOf(this as HTMLElement);
    return new DOMRect(0, n * 30 - scroller.scrollTop, 800, 30);
  });
  scroller.scrollTop = 45;
  const link = [...target.querySelectorAll<HTMLAnchorElement>("tr[data-place] a")].find((a) => text(a) === "New pull request")!;
  link.focus();
  const onScreen = rows().find((row) => row.getBoundingClientRect().bottom > 0)!;
  const was = onScreen.getBoundingClientRect().top;

  // A branch pushed meanwhile sorts first, above the reader's place.
  forge = ["fix/newest"];
  await raise("view:fresh", { entity_id: REPO, section: "branches" });
  expect(text(rows()[0]!)).toContain("main");
  expect(text(rows()[1]!)).toContain("fix/newest");
  expect(onScreen.isConnected).toBe(true);
  expect(onScreen.getBoundingClientRect().top).toBe(was);
  expect(document.activeElement).toBe(link);
  scroller.replaceWith(target);
});
