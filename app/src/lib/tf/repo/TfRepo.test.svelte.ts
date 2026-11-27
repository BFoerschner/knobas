/**
 * The repository page (#838, R-8.3 to R-8.5, R-8.7, R-8.10), mounted over
 * stand-in reads: the header and its checkout, the branch dropdown that sets
 * the branch for every tab, the Files tree with the checkout's changes marked,
 * the file with line numbers, Commits and a commit's diff, the repository's
 * pull requests, and the tabs built later. What the source's view holds is
 * `knobas-source-gitea`'s (its view tests and `gitea-live`); here it is a
 * stand-in shaped like it.
 */
import { dropPrMove, heardRefresh, notePrMove } from "../pr/pr-state.svelte";
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

// The filters rank through the app's ranking (#2532): here a letters-in-order stand-in, so a word no substring holds is kept.
vi.mock("../../ipc/search", async (actual) => ({ ...(await actual<object>()), ...(await import("../filter/test-fuzzy")).fuzzyIpc() }));

import type {
  BranchPull,
  CheckoutView,
  DiffRead,
  EntityDetail,
  EntityFilter,
  EntityPage,
  EntityRow,
  SourceView,
  TreeEntry,
  ViewAction,
  ViewBlock,
  ViewRow,
} from "../../ipc/entity";
import TfRepo, { forgetHeaderLooks, forgetHeaderSearches, type RepoPorts } from "./TfRepo.svelte";
import { pressRefreshKey, refreshProbe } from "../refresh.fixture";
import type { EntityRefresh } from "../../ipc/sources";
import { toasts } from "../../shell/toasts.svelte";
import { drawsReading } from "../../shell/test-witness";
import { forgetCounts } from "./last-commit-counts";
import { checkoutPorts } from "./checkout-ports.test-support";

const NOW = new Date(2026, 7, 22, 14, 32);
const REPO = "gitea:tidewater/payout-service";
const MAIN = `${REPO}@refs/heads/main`;
const FEATURE = `${REPO}@refs/heads/feature/PAY-231-sepa-retry`;
const C1 = `${REPO}@c90d11a2b3c4`;

function row(entity_id: string, kind: string, title: string, extra: Partial<EntityRow> = {}): EntityRow {
  return {
    entity_id,
    kind,
    source_id: "gitea",
    title,
    updated_at: "2026-08-22T10:30:00Z",
    synced_at: "2026-08-22T14:30:00Z",
    path: null,
    state: null,
    ...extra,
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

const section = (id: string, blocks: ViewBlock[]): SourceView => ({
  sections: [{ id, title: id, count: null, deferred: false, blocks }],
  actions: [],
});

/** A commit's whole view carries *New branch…* from it (#707), its whole sha the value. */
const withActions = (view: SourceView): SourceView => ({ ...view, actions: [NEW_BRANCH] });
const NEW_BRANCH = {
  write_op: "create_branch",
  label: "New branch…",
  fields: [
    { name: "name", label: "Name", kind: { type: "text", suggestions: [] }, value: null, required: true, placeholder: null },
    { name: "from_ref", label: "From", kind: { type: "choice", options: [{ value: "c90d11e4a7b2", label: "c90d11e4a7" }] }, value: null, required: true, placeholder: null },
  ],
  confirm: null,
  primary: false,
  danger: false,
  disabled: null,
  with_settings: false,
  values: {},
  entity: REPO,
} as unknown as ViewAction;

const entry = (name: string, kind: TreeEntry["kind"], at: string): TreeEntry => ({
  name,
  kind,
  size: kind === "dir" ? null : 10,
  section: kind === "dir" ? `files:${at}` : `file:${at}`,
});

/** The tree at each ref: a directory's section → its entries. */
const TREES: Record<string, Record<string, TreeEntry[]>> = {
  [REPO]: {
    files: [entry("src", "dir", "src"), entry("README.md", "file", "README.md")],
    "files:src": [entry("lib.rs", "file", "src/lib.rs"), entry("retry.rs", "file", "src/retry.rs")],
  },
  [FEATURE]: {
    files: [entry("src", "dir", "src"), entry("README.md", "file", "README.md"), entry("NEW.md", "file", "NEW.md")],
    "files:src": [entry("retry.rs", "file", "src/retry.rs")],
  },
};

function checkout(path: string | null): CheckoutView {
  return {
    repo_entity_id: REPO,
    repo_url: null,
    path,
    found_by: path === null ? "nothing" : "scan",
    clones_root: "/Users/mara/src",
    clone_command: null,
    tree:
      path === null
        ? null
        : { status: "read", branch: "main", head: null, upstream: null, ahead: 1, behind: 0, changed: 1, untracked: 0, conflicted: 0 },
  };
}

const LOCAL: DiffRead = {
  status: "read",
  unstaged: {
    files: [{ path: "src/retry.rs", old_path: null, change: "modified", additions: 1, deletions: 1, hunks: [], truncated: false, threads: [] }],
    truncated: false,
  },
  staged: { files: [], truncated: false },
  untracked: [],
  untracked_truncated: false,
};

const PRS = [
  row(`${REPO}#142`, "pr", "SEPA retry with exponential backoff", { path: "tidewater/payout-service", state: { text: "open", tone: "info" } }),
  row(`${REPO}#139`, "pr", "Fix drift", { path: "tidewater/payout-service", state: { text: "merged", tone: "ok" } }),
  row("gitea:tidewater/ledger#9", "pr", "Elsewhere", { path: "tidewater/ledger", state: { text: "open", tone: "info" } }),
];

/** C1's change, as its `diff` section draws it. */
const C1_DIFF: ViewBlock = {
            type: "diff",
            truncated: false,
            commentable: null,
            gone: [],
            files: [
              {
                path: "src/retry.rs",
                old_path: null,
                change: "modified",
                additions: 1,
                deletions: 1,
                truncated: false,
                threads: [],
                hunks: [
                  {
                    header: "@@ -1,2 +1,2 @@",
                    lines: [
                      { kind: "context", old: 1, new: 1, text: "fn retry() {" },
                      { kind: "del", old: 2, new: null, text: "    3" },
                      { kind: "add", old: null, new: 2, text: "    5" },
                    ],
                  },
                ],
              },
            ],
          };

let target: HTMLElement;
let view: ReturnType<typeof mount> | null = null;
let viewReads: [string, string | null][];

beforeEach(() => {
  viewReads = [];
  target = document.createElement("div");
  document.body.appendChild(target);
});

afterEach(() => {
  if (view) unmount(view);
  view = null;
  target.remove();
});

function ports(over: Partial<RepoPorts> = {}, cloned = true): RepoPorts {
  return {
    getEntity: async (id) => {
      if (id === REPO)
        return {
          row: row(REPO, "repo", "tidewater/payout-service"),
          author: null,
          source: { id: "gitea", display_name: "Tidewater Gitea", adapter_kind: "gitea", enabled: true },
          refreshes: true,
          deleted_at: null,
        } as unknown as EntityDetail;
      if (id === C1)
        return {
          row: row(C1, "commit", "PAY-231: jitter in backoff", { updated_at: "2026-08-22T09:42:00Z" }),
          author: "mara",
        } as unknown as EntityDetail;
      throw { code: "not_found", message: `${id} is not in the mirror` };
    },
    listEntities: async (): Promise<EntityPage> => ({ rows: PRS, total: PRS.length, tally: null, groups: null }),
    entityView: async (id, sectionId) => {
      viewReads.push([id, sectionId]);
      if (id === REPO && sectionId === "branches")
        return section("branches", [
          {
            type: "entities",
            title: null,
            rows: [vrow(MAIN, "main", "default"), vrow(FEATURE, "feature/PAY-231-sepa-retry"), vrow(null, "spike/x")],
          },
        ]);
      const at = id === MAIN ? REPO : id;
      if (sectionId === "commits")
        return section("commits", [
          {
            type: "entities",
            title: null,
            rows:
              at === REPO
                ? [vrow(C1, "PAY-231: jitter in backoff", "c90d11a2b3", "mara · 3 hours ago"), vrow(null, "Bump deps", "a41f2c0000", "jonas · 2 days ago")]
                : [vrow(null, "Only on the feature", "7be0e40000", "mara · 1 day ago")],
          },
        ]);
      // A commit's whole view defers its diff, as Gitea's (#2319); the diff is its own read.
      if (id === C1 && sectionId === null)
        return withActions({ sections: [{ id: "diff", title: "Diff", count: null, deferred: true, blocks: [] }], actions: [] });
      if (id === C1 && sectionId === "diff") return section("diff", [C1_DIFF]);
      const tree = TREES[at];
      if (tree && sectionId !== null && tree[sectionId])
        return section(sectionId, [{ type: "tree", trail: [], entries: tree[sectionId]!, truncated: false, url: null }]);
      if (tree && sectionId?.startsWith("file:")) {
        const path = sectionId.slice(5);
        return section(sectionId, [
          {
            type: "file",
            trail: [],
            path,
            text: at === FEATURE ? `// ${path} on the feature\n` : `// ${path}\nfn main() {}\n`,
            language: null,
            size: 20,
            truncated: false,
            binary: false,
            url: null,
            sha: null,
            commit: null,
          },
        ]);
      }
      throw { code: "invalid", message: `no section ${sectionId}` };
    },
    entityCheckout: async () => checkout(cloned ? "/Users/mara/src/payout-service" : null),
    entityCheckoutDiff: async () => {
      if (!cloned) throw { code: "not_found", message: "no checkout" };
      return LOCAL;
    },
    setCheckoutOverride: async () => {
      throw { code: "invalid", message: "no override in this suite" };
    },
    listContexts: async () => [],
    listSources: async () => [],
    jobReach: async () => [],
    submitWrite: async () => {
      throw { code: "invalid", message: "no write in this suite" };
    },
    // The repository's team and language, and C1's author: mara, the identity (#973).
    codeFacts: async (ids) =>
      [
        { entity_id: REPO, author: "tidewater", mine: false, language: "Rust", teams: ["Payments"] },
        { entity_id: C1, author: "mara", mine: true, language: null, teams: [] },
      ].filter((facts) => ids.includes(facts.entity_id)),
    ...over,
  };
}

async function draw(rest: string[], stand: RepoPorts = ports()) {
  view = mount(TfRepo, { target, props: { ready: true, rest, job: null, ports: stand, now: NOW } });
  await settle();
}

async function settle() {
  for (let round = 0; round < 4; round += 1) {
    for (let i = 0; i < 10; i += 1) await Promise.resolve();
    await new Promise((resolve) => setTimeout(resolve, 0));
    flushSync();
  }
}

const text = (el: Element | null) => el?.textContent?.replace(/\s+/g, " ").trim() ?? "";
const tree = () => [...target.querySelectorAll(".tf-ftree a, .tf-ftree button")].map(text);

test("the header and the tabs draw once, whole: unseen in their room until the facts, the checkout and the counts have answered (the nothing-moves pass)", async () => {
  let answer!: () => void;
  const late = new Promise<void>((resolve) => (answer = resolve));
  const stand = ports();
  await draw([REPO], { ...stand, codeFacts: async (ids) => (await late, stand.codeFacts(ids)) });
  const head = target.querySelector(".tf-repohead")!;
  const tabs = target.querySelector(".tf-tabs")!;
  expect(head.classList.contains("tf-unread"), "held while the facts are read").toBe(true);
  expect(tabs.classList.contains("tf-unread"), "the tabs with it").toBe(true);
  answer();
  await settle();
  expect(head.classList.contains("tf-unread")).toBe(false);
  expect(tabs.classList.contains("tf-unread")).toBe(false);
  expect(text(target.querySelector(".tf-fhead .tf-reposub"))).toBe("Payments, Rust, last push today 12:30.");
  // Each tab that can carry a count holds a count's room with none, so one a sync brings pushes no tab.
  const counted = [...tabs.querySelectorAll("a")].filter((a) => /Branches|Pull requests|Builds/.test(a.textContent ?? ""));
  expect(counted.length).toBe(3);
  for (const tab of counted) expect(tab.querySelector(".tf-count"), text(tab)).not.toBeNull();
});

test("the header names the repository, its last push and its checkout; the tabs follow with their counts", async () => {
  await draw([REPO]);

  expect(text(target.querySelector("h1"))).toBe("tidewater/payout-service");
  expect(text(target.querySelector(".tf-fhead .tf-reposub"))).toBe("Payments, Rust, last push today 12:30.");
  // The checkout is a sign beside it, its path the sign's description and a run of its own (the clone-path pass).
  const sign = target.querySelector(".tf-fhead .tf-sub .tf-signbtn")!;
  expect(text(sign)).toBe("Local, main, 1 ahead, 1 changed");
  expect(text(document.getElementById(sign.getAttribute("aria-describedby")!))).toBe("/Users/mara/src/payout-service");
  expect(drawsReading(target, "/Users/mara/src/payout-service")).toBe(true);
  expect([...target.querySelectorAll(".tf-crumbs > a:not(.tf-esc), .tf-crumbs strong")].map(text)).toEqual([
    "Code",
    "tidewater/payout-service",
  ]);
  // A repository's name is a code value in the crumb too.
  expect(target.querySelector(".tf-crumbs strong .tf-mono")).not.toBeNull();
  const tabs = [...target.querySelectorAll(".tf-tabs a")];
  // Two groups (#2257): the repository at a ref, then work in flight, split by a rule.
  expect(tabs.map(text)).toEqual(["Files", "Commits", "Branches 2", "Compare", "Pull requests 1", "Builds", "Local changes 1"]);
  const strip = [...target.querySelector(".tf-tabs")!.children].map((el) => (el.tagName === "A" ? text(el) : "|"));
  expect(strip).toEqual(["Files", "Commits", "Branches 2", "Compare", "|", "Pull requests 1", "Builds", "Local changes 1"]);
  expect(tabs[0]!.getAttribute("aria-current")).toBe("page");
  expect(tabs[1]!.getAttribute("href")).toBe("#/repo/gitea%3Atidewater%2Fpayout-service/commits");
  const select = target.querySelector<HTMLSelectElement>(".tf-fhead select")!;
  // Cloned here, the working tree is the first choice (2026-11-21); this checkout's tree has no commit, so it says why.
  expect([...select.options].map((o) => o.textContent)).toEqual(["Working tree: no commit yet", "main", "feature/PAY-231-sepa-retry"]);
  expect(select.options[0]!.disabled).toBe(true);
  expect(select.value).toBe("main");
  // #2238's order, as the mockup draws it: the primary, the others, Open in, Refresh last.
  const acts = [...target.querySelector(".tf-fhead > .tf-acts")!.children].flatMap((el) => (el.matches(".tf-repohold") ? [...el.children] : [el]));
  expect(acts[0]!.matches("a.tf-btn.tf-primary")).toBe(true);
  expect(text(acts[0]!)).toBe("New pull request");
  expect(acts[1]!.querySelector("select")).toBe(select);
  expect(text(acts[2]!)).toBe("New branch…");
  // *Run…* (ADR-0060) stands before "Open in", a ghost at the same size.
  const runIn = acts[3]!.matches("button") ? acts[3]! : acts[3]!.querySelector("button")!;
  expect(text(runIn)).toBe("Run…");
  expect(runIn.matches(".tf-btn.tf-ghost:not(.tf-sm):not(.tf-primary)")).toBe(true);
  // "Open in" is always a ghost, at the header's default size (#2238, #2296).
  const openIn = acts[4]!.matches("button") ? acts[4]! : acts[4]!.querySelector("button")!;
  expect(text(openIn)).toMatch(/^Open in/);
  expect(openIn.matches(".tf-btn.tf-ghost:not(.tf-sm):not(.tf-primary)")).toBe(true);
  expect(text(acts.at(-1)!)).toBe("Refresh");
});

// F-243: the header's primary never lands on a form saying one is already open.
const WITH_PR = () => section("overview", [{ type: "entities", title: null, rows: [vrow("gitea:tidewater/payout-service#142", "SEPA retry")] }]);

test("New pull request starts on a branch with nothing open, past one that has (F-243)", async () => {
  const base = ports();
  const other = "gitea:tidewater/payout-service@refs/heads/spike/y";
  await draw(
    [REPO],
    ports({
      entityView: async (id, sectionId) => {
        if (id === REPO && sectionId === "branches") return section("branches", [{ type: "entities", title: null, rows: [vrow(MAIN, "main", "default"), vrow(FEATURE, "feature/PAY-231-sepa-retry"), vrow(other, "spike/y")] }]);
        if (id === FEATURE && sectionId === null) return WITH_PR();
        if (id === other && sectionId === null) return { ...section("overview", []), actions: [{ write_op: "create_pull_request", disabled: null } as ViewAction] };
        return base.entityView(id, sectionId);
      },
    }),
  );
  const primary = target.querySelector(".tf-fhead .tf-primary")!;
  expect(text(primary)).toBe("New pull request");
  expect(primary.getAttribute("href")).toBe(`#/repo/${encodeURIComponent(REPO)}/compare/${encodeURIComponent("spike/y")}`);
});

// The inner-6x pass (2026-11-15): the look read up to eight branches' whole views on every visit and every tab shown,
// most of a forge's quota with a person's own reads waiting behind them.
test("the header's look is kept: another tab, a return from Compare and a second visit ask no branch again, and say the same", async () => {
  const base = ports();
  const other = "gitea:tidewater/payout-service@refs/heads/spike/y";
  let asked = 0;
  const stand = ports({
    entityView: async (id, sectionId) => {
      if (id === REPO && sectionId === "branches") return section("branches", [{ type: "entities", title: null, rows: [vrow(MAIN, "main", "default"), vrow(FEATURE, "feature/PAY-231-sepa-retry"), vrow(other, "spike/y")] }]);
      if (sectionId === null && id.includes("@refs/heads/")) asked += 1;
      if (id === FEATURE && sectionId === null) return WITH_PR();
      if (id === other && sectionId === null) return { ...section("overview", []), actions: [{ write_op: "create_pull_request", disabled: null } as ViewAction] };
      return base.entityView(id, sectionId);
    },
  });
  const props = $state({ ready: true, rest: [REPO], job: null, ports: stand, now: NOW });
  view = mount(TfRepo, { target, props });
  await settle();
  const looked = asked;
  expect(looked).toBeGreaterThan(0);
  const proposal = () => target.querySelector(".tf-fhead .tf-primary")?.getAttribute("href");
  expect(proposal()).toBe(`#/repo/${encodeURIComponent(REPO)}/compare/${encodeURIComponent("spike/y")}`);

  // Commits and back: the lists are read again, the look is not.
  props.rest = [REPO, "commits"];
  await settle();
  props.rest = [REPO];
  await settle();
  expect(asked).toBe(looked);
  expect(proposal()).toBe(`#/repo/${encodeURIComponent(REPO)}/compare/${encodeURIComponent("spike/y")}`);

  // Compare from the button (which reads the branch's view for itself), and back: the branch Compare picked is one the
  // look read, so the header asks nothing and holds nothing.
  props.rest = [REPO, "compare", "spike/y"];
  await settle();
  const atCompare = asked;
  props.rest = [REPO];
  flushSync();
  expect(target.querySelector(".tf-fhead .tf-repoheld")).toBeNull();
  await settle();
  expect(asked).toBe(atCompare);

  // A second visit to the repository: drawn at once, nothing asked.
  unmount(view);
  view = mount(TfRepo, { target, props: { ready: true, rest: [REPO], job: null, ports: stand, now: NOW } });
  await settle();
  expect(asked).toBe(atCompare);
  expect(target.querySelector(".tf-fhead .tf-repoheld")).toBeNull();
  expect(proposal()).toBe(`#/repo/${encodeURIComponent(REPO)}/compare/${encodeURIComponent("spike/y")}`);
});

// The ux13 forge-load pass (2026-11-15): after a run, the branch list and the pull requests landing a moment apart each
// started the look again, and each look read every branch's whole view on the forge while the last one's were still out.
test("a look started again while its branches' views are being read asks each branch once", async () => {
  const base = ports();
  const other = "gitea:tidewater/payout-service@refs/heads/spike/y";
  const asked: string[] = [];
  const held: (() => void)[] = [];
  const stand = ports({
    entityView: async (id, sectionId) => {
      if (id === REPO && sectionId === "branches") return section("branches", [{ type: "entities", title: null, rows: [vrow(MAIN, "main", "default"), vrow(FEATURE, "feature/PAY-231-sepa-retry"), vrow(other, "spike/y")] }]);
      if (sectionId === null && id.includes("@refs/heads/")) {
        asked.push(id);
        await new Promise<void>((release) => held.push(release));
      }
      if (id === FEATURE && sectionId === null) return WITH_PR();
      if (id === other && sectionId === null) return { ...section("overview", []), actions: [{ write_op: "create_pull_request", disabled: null } as ViewAction] };
      return base.entityView(id, sectionId);
    },
  });
  const props = $state({ ready: true, rest: [REPO], job: null, ports: stand, now: NOW });
  view = mount(TfRepo, { target, props });
  await settle();
  const first = [...asked];
  expect(first.length).toBeGreaterThan(0);

  // Another tab while they are out: the lists are read again and land, which starts the look again.
  props.rest = [REPO, "commits"];
  await settle();
  props.rest = [REPO];
  await settle();
  expect(asked).toEqual(first);
  while (held.length > 0) {
    for (const release of held.splice(0)) release();
    await settle();
  }
  expect(new Set(asked).size).toBe(asked.length);
  expect(target.querySelector(".tf-fhead .tf-primary")?.getAttribute("href")).toBe(`#/repo/${encodeURIComponent(REPO)}/compare/${encodeURIComponent("spike/y")}`);
});

test("until the branches answer, the primary keeps its room unseen, so it never changes its words under the pointer (the live run, 2026-11-09)", async () => {
  const base = ports();
  const held: (() => void)[] = [];
  await draw(
    [REPO],
    ports({
      entityView: async (id, sectionId) => {
        if (id === FEATURE && sectionId === null) {
          await new Promise<void>((release) => held.push(release));
          return WITH_PR();
        }
        return base.entityView(id, sectionId);
      },
    }),
  );
  expect(held.length).toBeGreaterThan(0);
  expect(target.querySelector(".tf-fhead a.tf-primary"), "nothing to press while it asks").toBeNull();
  expect(target.querySelector(".tf-fhead .tf-repoheld")?.getAttribute("aria-hidden")).toBe("true");
  for (const release of held) release();
  await settle();
  expect(target.querySelector(".tf-fhead .tf-repoheld")).toBeNull();
  expect(target.querySelector(".tf-fhead a.tf-primary")).not.toBeNull();
});

test("a branch picked with a pull request open from it makes the primary Open PR #N, as Code's does (F-243)", async () => {
  const base = ports();
  await draw(
    [REPO],
    ports({
      entityView: async (id, sectionId) => (id === FEATURE && sectionId === null ? WITH_PR() : base.entityView(id, sectionId)),
    }),
  );
  const select = target.querySelector<HTMLSelectElement>(".tf-fhead select")!;
  select.value = "feature/PAY-231-sepa-retry";
  select.dispatchEvent(new Event("change", { bubbles: true }));
  await settle();
  const primary = target.querySelector(".tf-fhead .tf-primary")!;
  expect(text(primary)).toBe("Open PR #142");
  expect(primary.getAttribute("href")).toBe("#/pr/gitea%3Atidewater%2Fpayout-service%23142");
});

// The forge-rounds pass (2026-11-15): the look read each branch's whole view on the forge, about ten requests a branch,
// and held the button for seconds on a first visit. The mirror holds the pull requests and each branch's tip.
const SPIKE_Y = "gitea:tidewater/payout-service@refs/heads/spike/y";
const SPIKE_Z = "gitea:tidewater/payout-service@refs/heads/spike/z";
const MAIN_TIP = "c90d11a3f5e2b7c4d9018e6a2b3c4d5e6f708192";
function pulled(name: string, entity_id: string, tip: string, pull: number | null = null): BranchPull {
  return {
    entity_id,
    name,
    tip,
    is_default: name === "main",
    pull: pull === null ? null : { branch: name, entity_id: `${REPO}#${pull}`, cell: `#${pull}` },
  };
}
function mirrorPorts(branches: string[][], mirror: BranchPull[], asked: string[]): RepoPorts {
  const base = ports();
  return ports({
    entityView: async (id, sectionId) => {
      if (id === REPO && sectionId === "branches")
        return section("branches", [{ type: "entities", title: null, rows: branches.map(([name, id]) => vrow(id!, name!, name === "main" ? "default" : null)) }]);
      if (sectionId === null && id.includes("@refs/heads/")) {
        asked.push(id);
        return { ...section("overview", []), actions: [{ write_op: "create_pull_request", disabled: null } as ViewAction] };
      }
      return base.entityView(id, sectionId);
    },
    repoBranchPulls: async () => mirror,
  });
}

test("New pull request is decided from the mirror: no branch's view is read, and a branch at the default's tip is passed over", async () => {
  forgetHeaderLooks();
  forgetHeaderSearches();
  const asked: string[] = [];
  const branches = [
    ["main", MAIN],
    ["feature/PAY-231-sepa-retry", FEATURE],
    ["spike/y", SPIKE_Y],
    ["spike/z", SPIKE_Z],
  ];
  const mirror = [
    pulled("main", MAIN, MAIN_TIP),
    pulled("feature/PAY-231-sepa-retry", FEATURE, "a".repeat(40), 142),
    pulled("spike/y", SPIKE_Y, MAIN_TIP),
    pulled("spike/z", SPIKE_Z, "b".repeat(40)),
  ];
  await draw([REPO], mirrorPorts(branches, mirror, asked));
  const primary = target.querySelector(".tf-fhead .tf-primary")!;
  expect(text(primary)).toBe("New pull request");
  expect(primary.getAttribute("href")).toBe(`#/repo/${encodeURIComponent(REPO)}/compare/${encodeURIComponent("spike/z")}`);
  expect(asked, "no branch's view read on the forge").toEqual([]);
});

test("where every branch the mirror holds has a pull request open, the primary is Open PR #N, with nothing read", async () => {
  forgetHeaderLooks();
  forgetHeaderSearches();
  const asked: string[] = [];
  const branches = [
    ["main", MAIN],
    ["feature/PAY-231-sepa-retry", FEATURE],
    ["spike/y", SPIKE_Y],
  ];
  const mirror = [pulled("main", MAIN, MAIN_TIP), pulled("feature/PAY-231-sepa-retry", FEATURE, "a".repeat(40), 142), pulled("spike/y", SPIKE_Y, "b".repeat(40), 150)];
  await draw([REPO], mirrorPorts(branches, mirror, asked));
  const primary = target.querySelector(".tf-fhead .tf-primary")!;
  expect(text(primary)).toBe("Open PR #142");
  expect(asked).toEqual([]);
});

test("a branch the mirror does not hold yet is read from its view, as before; the rest are the mirror's", async () => {
  forgetHeaderLooks();
  forgetHeaderSearches();
  const asked: string[] = [];
  const branches = [
    ["main", MAIN],
    ["spike/z", SPIKE_Z],
    ["feature/PAY-231-sepa-retry", FEATURE],
  ];
  const mirror = [pulled("main", MAIN, MAIN_TIP), pulled("feature/PAY-231-sepa-retry", FEATURE, "a".repeat(40), 142)];
  await draw([REPO], mirrorPorts(branches, mirror, asked));
  expect(asked).toEqual([SPIKE_Z]);
  expect(target.querySelector(".tf-fhead .tf-primary")!.getAttribute("href")).toBe(`#/repo/${encodeURIComponent(REPO)}/compare/${encodeURIComponent("spike/z")}`);
});

test("the compare page lights its own tab, Compare, and Pull requests stays dark (#2257)", async () => {
  await draw([REPO, "compare", "feature", "PAY-231-sepa-retry"]);
  const lit = [...target.querySelectorAll(".tf-tabs a[aria-current='page']")].map(text);
  expect(lit).toEqual(["Compare"]);
});

test("Files opens the README, marks the locally changed file M, and a changed file warns", async () => {
  await draw([REPO]);
  expect(tree()).toEqual(["src", "README.md"]);
  expect(text(target.querySelector(".tf-fileview header"))).toContain("README.md 20 B · 2 lines");
  // The README renders as Markdown (#2255).
  expect(text(target.querySelector(".tf-fileview .tf-md"))).toBe("// README.md fn main() {}");
  expect(target.querySelector(".tf-banner")).toBeNull();
  unmount(view!);
  view = null;

  await draw([REPO, "files", "src", "retry.rs"]);
  expect(tree()).toEqual(["src", "lib.rs", "retry.rs", "README.md"]);
  const changed = [...target.querySelectorAll(".tf-ftree a")].find((a) => text(a) === "retry.rs")!;
  expect(changed.classList.contains("tf-changed")).toBe(true);
  expect(changed.getAttribute("aria-current")).toBe("true");
  expect(changed.getAttribute("href")).toBe("#/repo/gitea%3Atidewater%2Fpayout-service/files/src/retry.rs");
  expect(text(target.querySelector(".tf-banner"))).toBe("Your checkout has an uncommitted change to this file. See local changes");
  expect(target.querySelector(".tf-banner a")!.getAttribute("href")).toBe("#/repo/gitea%3Atidewater%2Fpayout-service/local");
});

test("the open file's actions wait unseen in their room for the checkout, so Clone landing pushes no button (the ux21 pass)", async () => {
  let answer!: (view: CheckoutView) => void;
  let fail!: (why: unknown) => void;
  const late = () => new Promise<CheckoutView>((resolve, reject) => ((answer = resolve), (fail = reject)));
  await draw([REPO], ports({ entityCheckout: late }, false));
  const trail = () => target.querySelector(".tf-fileview header .tf-trail")!;
  expect(text(target.querySelector(".tf-fileview .tf-md")), "the README draws meanwhile").toBe("// README.md fn main() {}");
  expect(trail().classList.contains("tf-heldbelow"), "held while the checkout is read").toBe(true);
  expect(trail().querySelector(".tf-openin:not(.tf-oo-runin)")).toBeNull();
  answer(checkout(null));
  await settle();
  expect(trail().classList.contains("tf-heldbelow")).toBe(false);
  expect(text(trail().querySelector(".tf-openin:not(.tf-oo-runin)"))).toBe("Clone");
  unmount(view!);
  view = null;

  // A read that fails shows the rest, with no *Clone* to wait for.
  await draw([REPO, "files", "src", "lib.rs"], ports({ entityCheckout: late }, false));
  expect(trail().classList.contains("tf-heldbelow")).toBe(true);
  fail({ code: "internal", message: "no" });
  await settle();
  expect(trail().classList.contains("tf-heldbelow")).toBe(false);
  expect(trail().querySelector(".tf-openin:not(.tf-oo-runin)")).toBeNull();
});

test("the landing draws the branch's last commit above the README; a file opened does not (#2257)", async () => {
  await draw([REPO]);
  const strip = target.querySelector(".tf-lastc")!;
  expect(text(strip)).toBe("PAY-231: jitter in backoff c90d11a2b3 mara · 3 hours ago History");
  expect(strip.querySelector("a.tf-mono")!.getAttribute("href")).toBe(
    "#/repo/gitea%3Atidewater%2Fpayout-service/commits/gitea%3Atidewater%2Fpayout-service%40c90d11a2b3c4",
  );
  expect([...strip.querySelectorAll("a")].at(-1)!.getAttribute("href")).toBe("#/repo/gitea%3Atidewater%2Fpayout-service/commits");
  // It sits above the README, in the column beside the tree.
  expect(strip.nextElementSibling!.matches(".tf-fileview")).toBe(true);

  // Another branch: its own last commit.
  const select = target.querySelector<HTMLSelectElement>(".tf-fhead select")!;
  select.value = "feature/PAY-231-sepa-retry";
  select.dispatchEvent(new Event("change", { bubbles: true }));
  await settle();
  expect(text(target.querySelector(".tf-lastc"))).toContain("Only on the feature");
  unmount(view!);
  view = null;

  await draw([REPO, "files", "src", "lib.rs"]);
  expect(target.querySelector(".tf-lastc")).toBeNull();
});

test("the mirror's answer, with no count, draws the count read before, and the way to the history keeps its room (the ux21 pass)", async () => {
  forgetCounts();
  const base = ports();
  const commits = (count: number | null, held: boolean) =>
    ports({
      entityView: async (id, sectionId) => {
        const view = await base.entityView(id, sectionId);
        if (sectionId !== "commits" || view === null) return view;
        const counted = { ...view, sections: [{ ...view.sections[0]!, count }] } as SourceView;
        return held ? { ...counted, held_at: "2026-11-19T13:33:26Z" } : counted;
      },
    });
  const way = () => target.querySelector(".tf-lastc a.tf-btn")!;
  // Never counted here: the mirror's answer says *History*, in a room the count will not outgrow.
  await draw([REPO], commits(null, true));
  expect(text(way())).toBe("History");
  expect(way().classList.contains("tf-lastc-all"), "the room as wide as 8,888 commits").toBe(true);
  unmount(view!);
  view = null;

  await draw([REPO], commits(438, false));
  expect(text(way())).toBe("438 commits");
  unmount(view!);
  view = null;

  // Back on the repository, the mirror answers first: the count read before draws at once.
  await draw([REPO], commits(null, true));
  expect(text(way())).toBe("438 commits");
  unmount(view!);
  view = null;

  // A source that answers no count says *History*, whatever was read before.
  await draw([REPO], commits(null, false));
  expect(text(way())).toBe("History");
});

test("a last commit that does not read leaves the landing without its strip (#2257)", async () => {
  const base = ports();
  await draw(
    [REPO],
    ports({
      entityView: (id, sectionId) =>
        sectionId === "commits" ? Promise.reject({ code: "unavailable", message: "down" }) : base.entityView(id, sectionId),
    }),
  );
  expect(target.querySelector(".tf-lastc")).toBeNull();
  expect(text(target.querySelector(".tf-fileview .tf-md"))).toBe("// README.md fn main() {}");
});

test("a line link opens Files on the file with that line marked (#2251)", async () => {
  await draw([REPO, "line", "2", "src", "lib.rs"]);
  expect(text(target.querySelector(".tf-fileview header .tf-p"))).toBe("src/lib.rs");
  expect([...target.querySelectorAll(".tf-code .tf-l.tf-at")].map(text)).toEqual(["2fn main() {}"]);
});

test("an uncloned repository reads its files through the source and says it is not on this machine", async () => {
  // With nothing declared the header starts at the push (#973).
  await draw([REPO, "files", "src", "lib.rs"], ports({ codeFacts: async () => [] }, false));
  expect(text(target.querySelector(".tf-fhead .tf-reposub"))).toBe("Last push today 12:30.");
  expect(text(target.querySelector(".tf-fhead .tf-sub .tf-signbtn"))).toBe("Remote, not cloned");
  expect([...target.querySelectorAll(".tf-code .tf-l")].map(text)).toEqual(["1// src/lib.rs", "2fn main() {}"]);
  expect(target.querySelector(".tf-changed")).toBeNull();
  expect([...target.querySelectorAll(".tf-tabs a")].map(text)).toContain("Local changes");
});

test("the branch dropdown sets the branch for Files and for Commits", async () => {
  await draw([REPO]);
  const select = target.querySelector<HTMLSelectElement>(".tf-fhead select")!;
  select.value = "feature/PAY-231-sepa-retry";
  select.dispatchEvent(new Event("change", { bubbles: true }));
  await settle();

  expect(tree()).toEqual(["src", "README.md", "NEW.md"]);
  expect(text(target.querySelector(".tf-fileview header"))).toContain("README.md 20 B · 1 line");
  expect(viewReads.some(([id, s]) => id === FEATURE && s === "file:README.md")).toBe(true);
  unmount(view!);
  view = null;

  // Held for the session: the Commits tab opens on the same branch.
  await draw([REPO, "commits"]);
  expect(target.querySelector<HTMLSelectElement>(".tf-fhead select")!.value).toBe("feature/PAY-231-sepa-retry");
  expect([...target.querySelectorAll(".tf-cbody tbody tr")].map(text)).toEqual(["7be0e40000Only on the featuremara · 1 day ago"]);
});

test("Commits lists sha, message and who and when; the filter narrows; a mirrored commit opens its diff", async () => {
  await draw([REPO, "commits"]);
  const rows = [...target.querySelectorAll(".tf-cbody tbody tr")];
  // A mirrored commit ends in its *Open in…* (#2274); one the mirror lacks has no sha to open.
  expect(rows.map(text)).toEqual(["c90d11a2b3PAY-231: jitter in backoffmara · 3 hours agoOpen in…", "a41f2c0000Bump depsjonas · 2 days ago"]);
  expect(rows[0]!.querySelector("a")!.getAttribute("href")).toBe(
    "#/repo/gitea%3Atidewater%2Fpayout-service/commits/gitea%3Atidewater%2Fpayout-service%40c90d11a2b3c4",
  );
  // Not in the mirror: nothing to open it at.
  expect(rows[1]!.querySelector("a")).toBeNull();

  const field = target.querySelector<HTMLInputElement>("input[data-filter]")!;
  field.value = "jonas";
  field.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
  await settle();
  expect([...target.querySelectorAll(".tf-cbody tbody tr")].map(text)).toEqual(["a41f2c0000Bump depsjonas · 2 days ago"]);
});

/** Types `words` into the tab's filter and waits for the ranking's answer. */
async function filterBy(words: string) {
  const field = target.querySelector<HTMLInputElement>("input[data-filter]")!;
  field.value = words;
  field.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
  await settle();
}
const marked = (el: Element | null) => [...(el?.querySelectorAll("mark") ?? [])].map(text);

test("Commits ranks its words through the app's ranking, letters in order, marks them, and keeps the branch's order (#2532)", async () => {
  await draw([REPO, "commits"]);
  // No substring holds "bmpdps"; its letters stand in order in "Bump deps".
  await filterBy("bmpdps");
  const rows = [...target.querySelectorAll(".tf-cbody tbody tr")];
  expect(rows.map(text)).toEqual(["a41f2c0000Bump depsjonas · 2 days ago"]);
  expect(marked(rows[0]!).join("").toLowerCase()).toBe("bmpdps");
  // Two kept stay newest first, as the branch lists them, whichever the ranking put first: a range reads their places.
  await filterBy("a");
  expect([...target.querySelectorAll(".tf-cbody tbody tr")].map((tr) => text(tr.querySelector(".tf-mono")))).toEqual(["c90d11a2b3", "a41f2c0000"]);
});

test("Branches and Pull requests rank their words through the app's ranking and mark the matched letters (#2532)", async () => {
  await draw([REPO, "branches"]);
  await filterBy("ftsepa");
  const branches = [...target.querySelectorAll(".tf-rbranches tbody tr[data-place]")];
  expect(branches.map((tr) => text(tr.querySelector("td")))).toEqual(["feature/PAY-231-sepa-retry"]);
  expect(marked(branches[0]!.querySelector("td")).join("").toLowerCase()).toBe("ftsepa");
  unmount(view!);
  view = null;
  target.replaceChildren();

  await draw([REPO, "prs"]);
  await filterBy("sprtry");
  const prs = [...target.querySelectorAll(".tf-cbody tbody tr")];
  expect(prs.map((tr) => text(tr.querySelector("td")))).toEqual(["#142"]);
  expect(marked(prs[0]!).join("").toLowerCase()).toBe("sprtry");
});

test("Commits' Author: Mine keeps the commits whose declared author is mine (#973)", async () => {
  // The unmirrored row's line says mara too: Mine reads the declaration, not the line.
  const stand = ports();
  const read = stand.entityView;
  stand.entityView = async (id, sectionId) => {
    const answer = await read(id, sectionId);
    if (sectionId !== "commits" || answer === null) return answer;
    const block = answer.sections[0]!.blocks[0] as { rows: ViewRow[] };
    block.rows.push(vrow(null, "Unmirrored", "0b1c2d0000", "mara · 4 days ago"));
    return answer;
  };
  await draw([REPO, "commits"], stand);
  const mine = [...target.querySelectorAll("button")].find((button) => text(button) === "Mine")!;
  expect(mine.closest("[role=group]")?.getAttribute("aria-label")).toBe("Author");
  mine.click();
  flushSync();
  expect([...target.querySelectorAll(".tf-cbody tbody tr")].map(text)).toEqual([
    "c90d11a2b3PAY-231: jitter in backoffmara · 3 hours agoOpen in…",
  ]);
});

test("a commit draws its sha, message, author and time, its diff per file, and New branch from here", async () => {
  await draw([REPO, "commits", C1]);
  expect(text(target.querySelector(".tf-cgroup-h"))).toContain("c90d11a2b3 PAY-231: jitter in backoff mara, today 11:42");
  const diff = target.querySelector(".tf-diff")!;
  expect(text(diff.querySelector(".tf-fh"))).toBe("M src/retry.rs +1 −1 Open in…");
  // A context line carries its old and its new number (#2266): `1` `1`.
  expect([...diff.querySelectorAll(".tf-ln")].map(text)).toEqual(["11 fn retry() {", "2− 3", "2+ 5"]);
  expect(diff.querySelector(".tf-ln.tf-del")).not.toBeNull();
  // A commit takes no line comments: only a pull request's review offers them (#2266).
  expect(diff.querySelector(".tf-addc")).toBeNull();
  const branch = [...target.querySelectorAll("button")].find((b) => text(b) === "New branch from here…")!;
  expect(branch.getAttribute("aria-disabled")).toBeNull();
  expect(target.querySelector(".tf-tabs a[aria-current='page']")!.textContent).toContain("Commits");
});

test("a commit the checkout holds is read there, offers Ignore whitespace, and the source reads no diff (#2319)", async () => {
  const asked: [string, string, string, boolean][] = [];
  const only = C1_DIFF.type === "diff" ? C1_DIFF.files : [];
  await draw(
    [REPO, "commits", C1],
    ports({
      commitDiff: async (repo, commit, sha, ignoreWhitespace) => {
        asked.push([repo, commit, sha, ignoreWhitespace]);
        return { files: ignoreWhitespace ? [] : only, truncated: false, range: null, read_at: "checkout" };
      },
    }),
  );
  expect(asked).toEqual([[REPO, C1, "c90d11a2b3c4", false]]);
  expect(text(target.querySelector(".tf-diff .tf-fh"))).toBe("M src/retry.rs +1 −1 Open in…");
  // The whole view is read for its toolbar and parent; its diff is never asked of the source.
  expect(viewReads.filter(([id]) => id === C1)).toEqual([[C1, null]]);
  const ws = target.querySelector<HTMLInputElement>(".tf-dtool .tf-wsbox input")!;
  expect(ws.disabled).toBe(false);
  expect(target.querySelector(".tf-wsbox")!.getAttribute("title")).toBeNull();
  ws.click();
  await settle();
  expect(asked.at(-1)).toEqual([REPO, C1, "c90d11a2b3c4", true]);
  expect(text(target.querySelector(".tf-empty"))).toBe("This commit changes no file.");
  // With nothing drawn the tick can still come off.
  expect(target.querySelector<HTMLInputElement>(".tf-diffws .tf-wsbox input")!.checked).toBe(true);
});

test("a commit no checkout holds is the source's diff, one read, and says why whitespace stays (#2319)", async () => {
  await draw([REPO, "commits", C1]);
  expect(viewReads.filter(([id]) => id === C1)).toEqual(expect.arrayContaining([[C1, null], [C1, "diff"]]));
  expect(viewReads.filter(([id, s]) => id === C1 && s === "diff").length).toBe(1);
  const ws = target.querySelector<HTMLInputElement>(".tf-dtool .tf-wsbox input")!;
  expect(ws.disabled).toBe(true);
  expect(ws.title).toContain("A checkout holding this commit can.");
});

test("an address that names no commit draws the nothing-here block under the tabs, no raw words and no header (F-231)", async () => {
  const bad = "5d77e1a2c3";
  const refusing = (id: string) => id === bad;
  const base = ports();
  await draw(
    [REPO, "commits", bad],
    {
      ...base,
      getEntity: async (id: string) => {
        if (refusing(id)) throw { code: "invalid", message: `entity id ${id} has no ':' separating namespace from key` };
        return base.getEntity(id);
      },
    },
  );
  expect(text(target.querySelector(".tf-nohere p"))).toBe("knobas does not hold this commit. It may not have synced yet.");
  expect(target.querySelector(".tf-nohere a")!.getAttribute("href")).toContain("/commits");
  expect(target.textContent).not.toContain("namespace");
  expect(target.querySelector(".tf-tabs")).not.toBeNull();
  expect(target.querySelector(".tf-cgroup-h")).toBeNull();
});

test("the branch selector holds its room unseen on Compare, whose Into and From already say it (F-230, F-249)", async () => {
  await draw([REPO, "compare"]);
  const held = () => target.querySelector<HTMLElement>("select[aria-label=Branch]")!.closest<HTMLElement>(".tf-repohold")!;
  expect(held().classList.contains("tf-held")).toBe(true);
  expect(held().inert).toBe(true);
  expect(target.querySelector("#tf-cmp-from")).not.toBeNull();
  unmount(view!);
  view = null;
  await draw([REPO, "commits"]);
  expect(held().classList.contains("tf-held")).toBe(false);
  expect(held().inert).toBe(false);
});

test("Pull requests lists this repository's, with a state filter", async () => {
  await draw([REPO, "prs"]);
  const rows = () => [...target.querySelectorAll(".tf-cbody tbody tr")].map((tr) => text(tr.querySelector("td")));
  expect(rows()).toEqual(["#142", "#139"]);
  const merged = [...target.querySelectorAll("button")].find((b) => text(b) === "Merged")!;
  merged.click();
  flushSync();
  expect(rows()).toEqual(["#139"]);
});

test("Local changes draws the checkout line, the uncommitted diff with the PR's diff block, and the untracked files", async () => {
  const read: DiffRead = {
    ...LOCAL,
    staged: {
      files: [{ path: "src/sepa/mod.rs", old_path: null, change: "modified", additions: 2, deletions: 0, hunks: [], truncated: false, threads: [] }],
      truncated: false,
    },
    untracked: ["notes.txt", "scratch/"],
  };
  let reads = 0;
  await draw([REPO, "local"], ports({ entityCheckoutDiff: async () => ((reads += 1), read) }));
  expect(text(target.querySelector(".tf-tabs a[aria-current='page']"))).toContain("Local changes");
  const line = target.querySelector(".tf-cbody .tf-checkout")!;
  expect(text(line)).toContain("/Users/mara/src/payout-service on main 1 ahead, 1 changed");
  expect(line.querySelector("button[aria-haspopup='menu']")).not.toBeNull();
  expect(text(target.querySelector(".tf-cbody"))).toContain("knobas never commits, stashes or pushes here");
  // The pull request's diff block (`TfDiffView`, #2266), each file marked.
  expect([...target.querySelectorAll(".tf-cbody .tf-diff .tf-fh")].map((head) => [text(head.querySelector(".tf-mono")), text(head.querySelector(".tf-state"))])).toEqual([
    ["src/retry.rs", "modified, not staged"],
    ["src/sepa/mod.rs", "modified, staged"],
  ]);
  expect([...target.querySelectorAll(".tf-untracked li")].map(text)).toEqual(["notes.txt", "scratch/"]);

  // And again when the window gets focus.
  const before = reads;
  window.dispatchEvent(new Event("focus"));
  await settle();
  expect(reads).toBeGreaterThan(before);
});

test("Local changes on a clean checkout says so", async () => {
  await draw([REPO, "local"], ports({ entityCheckoutDiff: async () => ({ ...LOCAL, unstaged: { files: [], truncated: false } }) }));
  expect(text(target.querySelector(".tf-cbody .tf-empty"))).toBe("Working tree clean.");
});

test("Local changes for a repository not on this machine offers Clone and setting a path by hand", async () => {
  const overrides: [string, string | null][] = [];
  await draw(
    [REPO, "local"],
    ports(
      {
        setCheckoutOverride: async (id, path) => {
          overrides.push([id, path]);
          return checkout(path);
        },
      },
      false,
    ),
  );
  const empty = target.querySelector(".tf-cbody .tf-empty")!;
  expect(text(empty)).toContain("Not on this machine yet.");
  const clone = [...empty.querySelectorAll("button")].find((b) => text(b) === "Clone")!;
  expect(clone.getAttribute("title")).toBe("Clones into /Users/mara/src/payout-service");
  [...empty.querySelectorAll("button")].find((b) => text(b) === "It is already cloned somewhere else…")!.click();
  flushSync();
  const dialog = document.querySelector("[role='dialog']")!;
  expect(text(dialog.querySelector("h2"))).toBe("Where is payout-service cloned?");
  const input = dialog.querySelector<HTMLInputElement>("#tf-spp")!;
  input.value = "/Users/mara/work/payout-service";
  input.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
  dialog.querySelector<HTMLButtonElement>("button[type='submit']")!.click();
  await settle();
  expect(overrides).toEqual([[REPO, "/Users/mara/work/payout-service"]]);
  // The fresh view has the path: the tab is the cloned one now.
  expect(text(target.querySelector(".tf-cbody .tf-checkout"))).toContain("/Users/mara/work/payout-service");
});

test("a repository the mirror does not hold says so", async () => {
  await draw(["gitea:tidewater/nowhere"]);
  expect(text(target.querySelector(".tf-nohere"))).toContain("repository");
});

// -- Refresh (#2135) -----------------------------------------------------------

/** The repository's detail with `over` on its read: its source and whether its adapter reads one item. */
function repoDetail(over: { enabled?: boolean; refreshes?: boolean; deleted_at?: string | null } = {}): RepoPorts["getEntity"] {
  const base = ports().getEntity;
  return async (id) => {
    const detail = await base(id);
    if (id !== REPO) return detail;
    return {
      ...detail,
      refreshes: over.refreshes ?? true,
      deleted_at: over.deleted_at ?? null,
      source: { ...detail.source, enabled: over.enabled ?? true },
    } as EntityDetail;
  };
}

const refreshButton = () => target.querySelector<HTMLButtonElement>(".tf-fhead .tf-refresh");

test("Refresh on a repository reads it from its source, then its row, its branches and the open file again, the page kept meanwhile", async () => {
  let rows = 0;
  let answer!: () => void;
  const probe = refreshProbe({ answer: () => new Promise((resolve) => (answer = () => resolve({ outcome: "refreshed" }))) });
  const getEntity = repoDetail();
  await draw(
    [REPO],
    ports({
      getEntity: async (id) => (id === REPO && (rows += 1), getEntity(id)),
      refresh: probe.ports,
    }),
  );
  expect(refreshButton()?.textContent).toBe("Refresh");
  const before = { rows, reads: viewReads.length };

  refreshButton()!.click();
  await settle();
  expect(refreshButton()?.textContent).toBe("Refreshing…");
  expect(tree()).toEqual(["src", "README.md"]);

  answer();
  await settle();
  expect(probe.refreshed).toEqual([REPO]);
  expect(rows).toBe(before.rows + 1);
  const again = viewReads.slice(before.reads);
  expect(again).toEqual(expect.arrayContaining([[REPO, "branches"], [REPO, "files"], [REPO, "file:README.md"]]));
  expect(refreshButton()?.textContent).toBe("Refresh");
  expect(text(target.querySelector("h1"))).toBe("tidewater/payout-service");
  expect(tree()).toEqual(["src", "README.md"]);
  // The README renders as Markdown (#2255).
  expect(text(target.querySelector(".tf-fileview .tf-md"))).toBe("// README.md fn main() {}");
});

test("a refresh reads Commits, a commit, the compare page and Branches again over what they drew", async () => {
  const stand = (): RepoPorts => ports({ getEntity: repoDetail(), refresh: refreshProbe().ports });
  const press = async () => {
    const before = viewReads.length;
    refreshButton()!.click();
    await settle();
    return viewReads.slice(before);
  };

  await draw([REPO, "commits"], stand());
  expect(await press()).toContainEqual([REPO, "commits"]);
  expect(target.querySelectorAll(".tf-cbody tbody tr").length).toBe(2);
  unmount(view!);

  await draw([REPO, "commits", C1], stand());
  expect(await press()).toContainEqual([C1, null]);
  expect(text(target.querySelector(".tf-cgroup-h"))).toContain("PAY-231: jitter in backoff");
  unmount(view!);

  await draw([REPO, "compare", "feature", "PAY-231-sepa-retry"], stand());
  expect(await press()).toEqual(expect.arrayContaining([[FEATURE, null], [REPO, "compare:main...feature/PAY-231-sepa-retry"]]));
  unmount(view!);

  await draw([REPO, "branches"], stand());
  const reads = await press();
  expect(reads.filter(([id, s]) => id === REPO && s === "branches").length).toBe(2);
  expect(reads).toContainEqual([FEATURE, null]);
  expect(text(target.querySelector(".tf-cbody"))).toContain("feature/PAY-231-sepa-retry");
});

test("a repository whose refresh failed keeps its page and says why", async () => {
  await draw(
    [REPO, "commits"],
    ports({
      getEntity: repoDetail(),
      refresh: refreshProbe({ answer: async () => ({ outcome: "failed", reason: "502 Bad Gateway" }) }).ports,
    }),
  );
  refreshButton()!.click();
  await settle();
  expect(text(target.querySelector(".tf-refresh-failed"))).toBe("Couldn't refresh: 502 Bad Gateway. Try again");
  expect(text(target.querySelector("h1"))).toBe("tidewater/payout-service");
  expect(target.querySelectorAll(".tf-cbody tbody tr").length).toBe(2);
});

test("a repository whose adapter does not read one item offers Sync now; a turned-off source neither, under its banner", async () => {
  await draw([REPO], ports({ getEntity: repoDetail({ refreshes: false }), refresh: refreshProbe().ports }));
  expect(refreshButton()?.textContent).toBe("Sync now");
  expect(target.querySelector(".tf-turnedoff")).toBeNull();
  unmount(view!);

  await draw([REPO], ports({ getEntity: repoDetail({ enabled: false }), refresh: refreshProbe().ports }));
  expect(refreshButton()).toBeNull();
  expect(text(target.querySelector(".tf-turnedoff"))).toContain("Tidewater Gitea is disabled");
});

test("a repository withdrawn upstream says so and keeps Refresh", async () => {
  await draw([REPO], ports({ getEntity: repoDetail({ deleted_at: "2026-08-20T10:00:00Z" }), refresh: refreshProbe().ports }));
  expect(text(target.querySelector(".tf-withdrawn"))).toContain("Withdrawn upstream");
  expect(refreshButton()?.textContent).toBe("Refresh");
});

test("the compare page keeps its branch's diff drawn while a refresh reads it again", async () => {
  const base = ports({ getEntity: repoDetail(), refresh: refreshProbe().ports });
  let diffs = 0;
  await draw(
    [REPO, "compare", "feature", "PAY-231-sepa-retry"],
    {
      ...base,
      entityView: async (id, sectionId) => {
        if (id === FEATURE && sectionId === null) return { sections: [], actions: [] };
        if (id !== REPO || sectionId !== "compare:main...feature/PAY-231-sepa-retry") return base.entityView(id, sectionId);
        // The first read answers; the refresh's stays out.
        if ((diffs += 1) > 1) return new Promise(() => {});
        const diff = await base.entityView(C1, "diff");
        return section("diff", diff!.sections[0]!.blocks);
      },
    },
  );
  expect(target.querySelector(".tf-cbody .tf-diff")).not.toBeNull();
  refreshButton()!.click();
  await settle();
  expect(diffs).toBe(2);
  expect(target.querySelector(".tf-cbody .tf-diff")).not.toBeNull();
});

test("Refreshing… holds until the open tab's re-read has answered too, as the job page's does", async () => {
  const base = ports({ getEntity: repoDetail(), refresh: refreshProbe().ports });
  let commitsReads = 0;
  let answer!: () => void;
  await draw([REPO, "commits"], {
    ...base,
    entityView: (id, sectionId) => {
      // The refresh's read of the commits stays out until the test answers it.
      if (sectionId === "commits" && (commitsReads += 1) > 1)
        return new Promise((resolve) => (answer = () => resolve(base.entityView(id, sectionId))));
      return base.entityView(id, sectionId);
    },
  });
  refreshButton()!.click();
  await settle();
  expect(commitsReads).toBe(2);
  expect(refreshButton()?.textContent).toBe("Refreshing…");
  expect(target.querySelectorAll(".tf-cbody tbody tr").length).toBe(2);

  answer();
  await settle();
  expect(refreshButton()?.textContent).toBe("Refresh");
});

test("⌘R on a repository runs its Refresh, as a click does (#2156)", async () => {
  const probe = refreshProbe();
  await draw([REPO], ports({ getEntity: repoDetail(), refresh: probe.ports }));
  expect(refreshButton()?.textContent).toBe("Refresh");
  expect(pressRefreshKey().defaultPrevented).toBe(true);
  await settle();
  expect(probe.refreshed).toEqual([REPO]);
});

test("⌘R on a repository says it reads that repository, by its name, and ends in the same toast (ux25)", async () => {
  toasts.items = [];
  let release!: (answer: EntityRefresh) => void;
  const probe = refreshProbe({ answer: () => new Promise<EntityRefresh>((resolve) => (release = resolve)) });
  await draw([REPO], ports({ getEntity: repoDetail(), refresh: probe.ports }));
  const wide = refreshButton()?.getBoundingClientRect().width;
  pressRefreshKey();
  await settle();
  expect(toasts.items.map((toast) => toast.text)).toEqual(["Refreshing payout-service…"]);
  expect(refreshButton()?.textContent).toBe("Refreshing…");
  expect(refreshButton()?.getBoundingClientRect().width).toBe(wide);
  release({ outcome: "refreshed" });
  await settle();
  expect(toasts.items.map((toast) => toast.text)).toEqual([expect.stringMatching(/^Refreshed payout-service from /)]);
  toasts.items = [];
});

// Keep-alive inside the repository (#2248) and a hidden page's pause (#2249).

const LEDGER = "gitea:tidewater/ledger";
/** The address and page visibility a live page is drawn at, moved by `go` and `show`. */
const live = $state<{ rest: string[]; visible: boolean }>({ rest: [], visible: true });

/** Mount the page over `live`, so a test moves its address as the shell does. */
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

async function go(rest: string[]) {
  live.rest = rest;
  await settle();
}

/** The tab bodies the page holds, hidden or shown. */
const shownTab = () => target.querySelector(".tf-cbody > .tf-kept:not([hidden])");
const hiddenTabs = () => target.querySelectorAll(".tf-cbody > .tf-kept[hidden]").length;
const fileView = () => shownTab()?.querySelector(".tf-fileview") ?? null;
const shownTree = () => [...(shownTab()?.querySelectorAll(".tf-ftree a, .tf-ftree button") ?? [])].map(text);
function openDir(name: string) {
  const button = [...target.querySelectorAll<HTMLButtonElement>(".tf-ftree button")].find((one) => text(one) === name)!;
  button.click();
}

/** A second repository, its tree the first one's. */
function twoRepos(): RepoPorts {
  const base = ports();
  return ports({
    getEntity: async (id) => {
      if (id !== LEDGER) return base.getEntity(id);
      const detail = await base.getEntity(REPO);
      return { ...detail, row: row(LEDGER, "repo", "tidewater/ledger") } as EntityDetail;
    },
    entityView: (id, sectionId) => base.entityView(id === LEDGER ? REPO : id, sectionId),
  });
}

test("Files, then Commits, then back to Files keeps the open file and the expanded tree, with no remount (#2248)", async () => {
  await drawLive([REPO, "files", "src", "lib.rs"]);
  expect(shownTree()).toEqual(["src", "lib.rs", "retry.rs", "README.md"]);
  // Folded by hand: a remount would open it again from the address.
  openDir("src");
  await settle();
  expect(shownTree()).toEqual(["src", "README.md"]);
  const drawn = fileView();
  expect(text(drawn?.querySelector("header") ?? null)).toContain("lib.rs");

  await go([REPO, "commits"]);
  expect([...shownTab()!.querySelectorAll("tbody tr")].map(text)[0]).toContain("PAY-231: jitter in backoff");
  expect(hiddenTabs()).toBe(1);
  // The Files tab links back to the file it was left at.
  expect(target.querySelector(".tf-tabs a")!.getAttribute("href")).toBe("#/repo/gitea%3Atidewater%2Fpayout-service/files/src/lib.rs");

  const before = viewReads.length;
  await go([REPO, "files", "src", "lib.rs"]);
  expect(fileView()).toBe(drawn);
  expect(shownTree()).toEqual(["src", "README.md"]);
  // Shown again, it reads the file in the background over what it drew.
  expect(viewReads.slice(before)).toContainEqual([REPO, "file:src/lib.rs"]);
});

test("moving between files keeps the tree, its folds and its scroll: one tree, never drawn again (#2257)", async () => {
  await drawLive([REPO, "files", "src", "lib.rs"]);
  const box = shownTab()!.querySelector(".tf-ftree")!;
  const first = [...box.querySelectorAll("a")].find((a) => text(a) === "README.md")!;
  box.scrollTop = 40;

  await go([REPO, "files", "src", "retry.rs"]);
  expect(shownTab()!.querySelector(".tf-ftree")).toBe(box);
  expect(text(fileView()!.querySelector("header .tf-p"))).toBe("src/retry.rs");
  expect(box.querySelector("a[aria-current='true']")!.getAttribute("href")).toContain("retry.rs");

  // The README at the top opens beside the same tree, src still unfolded, the rows the same nodes.
  await go([REPO, "files", "README.md"]);
  expect(shownTab()!.querySelector(".tf-ftree")).toBe(box);
  expect(shownTree()).toEqual(["src", "lib.rs", "retry.rs", "README.md"]);
  expect([...box.querySelectorAll("a")].find((a) => text(a) === "README.md")).toBe(first);
  expect(box.scrollTop).toBe(40);
});

test("a tab shown again at another place reads that place, and a tab never left reads nothing again (#2248)", async () => {
  await drawLive([REPO, "commits"]);
  const before = viewReads.length;
  await go([REPO, "branches"]);
  await go([REPO, "commits"]);
  expect(viewReads.slice(before).filter(([, s]) => s === "commits")).toHaveLength(1);
  const commits = shownTab();
  await go([REPO, "commits"]);
  expect(shownTab()).toBe(commits);
  expect(viewReads.slice(before).filter(([, s]) => s === "commits")).toHaveLength(1);
});

test("a hidden tab has visible false: Local changes reads its diff on focus only while shown (#2248)", async () => {
  let diffs = 0;
  let checkouts = 0;
  const base = ports();
  await drawLive(
    [REPO, "local"],
    ports({
      entityCheckout: async (id) => ((checkouts += 1), base.entityCheckout(id)),
      entityCheckoutDiff: async (id) => ((diffs += 1), base.entityCheckoutDiff(id)),
    }),
  );
  const shown = { diffs, checkouts };
  window.dispatchEvent(new Event("focus"));
  await settle();
  // The page's own checkout read and its diff, and the tab's diff.
  expect({ diffs: diffs - shown.diffs, checkouts: checkouts - shown.checkouts }).toEqual({ diffs: 2, checkouts: 1 });

  await go([REPO, "files"]);
  const hidden = { diffs, checkouts };
  window.dispatchEvent(new Event("focus"));
  await settle();
  expect({ diffs: diffs - hidden.diffs, checkouts: checkouts - hidden.checkouts }).toEqual({ diffs: 1, checkouts: 1 });
});

test("opening another repository starts its tabs fresh (#2248)", async () => {
  await drawLive([REPO], twoRepos());
  // Opened by hand: nothing in the address opens it.
  openDir("src");
  await settle();
  expect(shownTree()).toEqual(["src", "lib.rs", "retry.rs", "README.md"]);
  await go([REPO, "commits"]);
  expect(hiddenTabs()).toBe(1);
  const left = target.querySelector(".tf-fileview");

  await go([LEDGER]);
  expect(text(target.querySelector("h1"))).toBe("tidewater/ledger");
  expect(hiddenTabs()).toBe(0);
  // Mounted afresh, so no tab carries what the reader did into this repository.
  expect(fileView()).not.toBe(left);
  expect(shownTree()).toEqual(["src", "README.md"]);
  expect(text(fileView()?.querySelector("header") ?? null)).toContain("README.md");
  expect(target.querySelector(".tf-tabs a")!.getAttribute("href")).toBe("#/repo/gitea%3Atidewater%2Fledger/files");

  // Its own tabs are kept for its visit.
  const files = shownTab();
  await go([LEDGER, "commits"]);
  expect(hiddenTabs()).toBe(1);
  await go([LEDGER]);
  expect(shownTab()).toBe(files);
});

test("a hidden repository page does not read its checkout on window focus, and reads it once when shown again (#2249)", async () => {
  let checkouts = 0;
  const base = ports();
  await drawLive([REPO], ports({ entityCheckout: async (id) => ((checkouts += 1), base.entityCheckout(id)) }));
  const mounted = checkouts;

  live.visible = false;
  await settle();
  window.dispatchEvent(new Event("focus"));
  window.dispatchEvent(new Event("focus"));
  await settle();
  expect(checkouts).toBe(mounted);

  live.visible = true;
  await settle();
  expect(checkouts).toBe(mounted + 1);
  window.dispatchEvent(new Event("focus"));
  await settle();
  expect(checkouts).toBe(mounted + 2);
});

// ---- pages that don't wait for the branch list (#2243) and branch lists read behind (#2242) ----

/** A read the test answers when it chooses. */
function heldBack<T>() {
  let answer!: (value: T) => void;
  const promise = new Promise<T>((ok) => {
    answer = ok;
  });
  return { promise, answer };
}

/** `ports()` with the branch list held back, and the repository's detail declaring `fields` (#2307) over a payload naming a default the shell never reads. */
function holdingBranches(fields: Partial<EntityDetail>) {
  const base = ports();
  const branches = heldBack<SourceView | null>();
  const stand: RepoPorts = {
    ...base,
    getEntity: async (id) => ({ ...(await base.getEntity(id)), payload: { default_branch: "payload-main" }, ...fields }) as EntityDetail,
    entityView: (id, sectionId) => {
      if (id === REPO && sectionId === "branches") {
        viewReads.push([id, sectionId]);
        return branches.promise;
      }
      return base.entityView(id, sectionId);
    },
  };
  return { stand, release: async () => branches.answer(await base.entityView(REPO, "branches")) };
}

const branchSelect = () => target.querySelector<HTMLSelectElement>("select[aria-label='Branch']")!;
/** The branches the box offers, the working tree before them left out. */
const branchValues = () => [...branchSelect().options].map((option) => option.value).filter((value) => value !== "~working-tree");

test("with the branch list held back, Files draws on the mirror's default branch, and the list fills the selector without reading the tab again", async () => {
  const { stand, release } = holdingBranches({ default_branch: "main" });
  await draw([REPO], stand);

  expect(tree()).toContain("README.md");
  expect(target.querySelector(".tf-fileview header")).not.toBeNull();
  // Before the list is read, the shown name is the box's one option, so the box is that name's width from its first
  // draw and doesn't widen when the list lands (the branch box, 2026-11-21).
  expect(branchValues()).toEqual(["main"]);
  expect(branchSelect().value).toBe("main");
  expect(branchSelect().disabled).toBe(true);
  const tabReads = () => viewReads.filter(([id, sectionId]) => id === REPO && sectionId !== "branches").length;
  const before = tabReads();
  expect(before).toBeGreaterThan(0);

  await release();
  await settle();
  expect(branchValues()).toEqual(["main", "feature/PAY-231-sepa-retry"]);
  expect(branchSelect().value).toBe("main");
  expect(tabReads()).toBe(before);
  expect(tree()).toContain("README.md");
});

test("a deep file link reads by the tree sections the repository's detail declares, before any listing names them (#2298)", async () => {
  const early = (declared: Partial<EntityDetail>) => {
    const base = ports();
    return { ...base, getEntity: async (id: string) => ({ ...(await base.getEntity(id)), ...declared }) as EntityDetail };
  };
  const fileReads = () => viewReads.filter(([, sectionId]) => sectionId === "blob:src/psp/codes.rs").length;
  await draw([REPO, "files", "src", "psp", "codes.rs"], early({ default_branch: "main", tree: { dir: "dir:", file: "blob:" } }));
  expect(fileReads()).toBe(1);
  unmount(view!);
  view = null;
  viewReads.length = 0;

  await draw([REPO, "files", "src", "psp", "codes.rs"], early({ default_branch: "main" }));
  expect(fileReads()).toBe(0);
});

test("a repository whose detail declares no default branch draws its tree at once and names the branch once the list answers, whatever its payload says", async () => {
  const { stand, release } = holdingBranches({});
  await draw([REPO], stand);
  // Read at the repository, which is its default branch, before anything has named it (the ux12 Windows pass).
  expect(tree()).toContain("README.md");
  expect(branchValues()).toHaveLength(0);

  await release();
  await settle();
  expect(tree()).toContain("README.md");
  expect(branchSelect().value).toBe("main");
});

test("a first open asks for the Files tab's reads with the repository's own, not after them (the ux12 Windows pass)", async () => {
  // The repository's row and its branch list both held: the landing's reads start in the same round anyway, at the
  // repository, where they waited for one of the two to name the branch before (three rounds of reads on Windows).
  const base = ports();
  const detail = heldBack<EntityDetail>();
  const branches = heldBack<SourceView | null>();
  const stand: RepoPorts = {
    ...base,
    getEntity: (id) => (id === REPO ? detail.promise : base.getEntity(id)),
    entityView: (id, sectionId) => {
      if (id !== REPO || sectionId !== "branches") return base.entityView(id, sectionId);
      viewReads.push([id, sectionId]);
      return branches.promise;
    },
  };
  await draw([REPO], stand);
  expect(viewReads.filter(([id, sectionId]) => id === REPO && sectionId === "files").length).toBe(1);
  expect(tree()).toContain("README.md");

  detail.answer({ ...(await base.getEntity(REPO)), default_branch: "main" } as EntityDetail);
  branches.answer(await base.entityView(REPO, "branches"));
  await settle();
  expect(branchSelect().value).toBe("main");
  expect(viewReads.filter(([id, sectionId]) => id === REPO && sectionId === "files").length).toBe(1);
});

test("New branch… is the source's refused_writes answer, asked with the commit's, and the page never reads the whole repository view", async () => {
  const asked: unknown[] = [];
  await draw(
    [REPO],
    ports({
      refusedWrites: async (asks) => {
        asked.push(asks);
        return [{ entity: REPO, op: "create_branch", reason: "A mirror: its branches come from upstream" }];
      },
    }),
  );
  const newBranch = [...target.querySelectorAll<HTMLButtonElement>(".tf-fhead button")].find((b) => text(b) === "New branch…")!;
  expect(newBranch.getAttribute("aria-disabled")).toBe("true");
  expect(newBranch.title).toBe("A mirror: its branches come from upstream");
  expect(asked).toEqual([
    [
      { entity: REPO, op: "create_branch" },
      { entity: REPO, op: "commit_file" },
    ],
  ]);
  expect(viewReads.filter(([id, sectionId]) => id === REPO && sectionId === null)).toEqual([]);
});

test("a toolbar the app could not ask about is asked again on a refresh and when the page is shown again (#2306)", async () => {
  const COULD_NOT = "knobas could not ask whether this credential may make this write (HTTP 502); it asks again when the write is drawn again";
  let down = true;
  let asked = 0;
  await drawLive(
    [REPO],
    ports({
      getEntity: repoDetail(),
      refresh: refreshProbe().ports,
      refusedWrites: async (asks) => {
        asked += 1;
        return down ? asks.map((ask) => ({ ...ask, reason: COULD_NOT })) : [];
      },
    }),
  );
  const newBranch = () => [...target.querySelectorAll<HTMLButtonElement>(".tf-fhead button")].find((b) => text(b) === "New branch…")!;
  expect(newBranch().getAttribute("aria-disabled")).toBe("true");
  expect(newBranch().title).toBe(COULD_NOT);

  down = false;
  refreshButton()!.click();
  await settle();
  expect(asked).toBe(2);
  expect(newBranch().getAttribute("aria-disabled")).toBeNull();

  down = true;
  live.visible = false;
  await settle();
  live.visible = true;
  await settle();
  expect(asked).toBe(3);
  expect(newBranch().title).toBe(COULD_NOT);
});

test("a branch list the app read again behind its answer is read again when it moved (#2242)", async () => {
  let pushed = false;
  const base = ports();
  const heard: ((event: { payload: unknown }) => void)[] = [];
  const stand: RepoPorts = {
    ...base,
    listen: async (event, handler) => {
      if (event === "view:fresh") heard.push(handler);
      return () => {};
    },
    entityView: async (id, sectionId) => {
      const answer = await base.entityView(id, sectionId);
      if (id === REPO && sectionId === "branches" && pushed) {
        const block = answer!.sections[0]!.blocks[0]!;
        if (block.type === "entities") block.rows.push(vrow(`${REPO}@refs/heads/pushed`, "pushed"));
      }
      return answer;
    },
  };
  await draw([REPO], stand);
  expect([...branchSelect().options].map((option) => option.value)).not.toContain("pushed");

  pushed = true;
  for (const hear of heard) hear({ payload: { entity_id: `${REPO}-other`, section: "branches" } });
  await settle();
  expect([...branchSelect().options].map((option) => option.value)).not.toContain("pushed");

  for (const hear of heard) hear({ payload: { entity_id: REPO, section: "branches" } });
  await settle();
  expect([...branchSelect().options].map((option) => option.value)).toContain("pushed");
});

test("the compare page's diff expands context read whole at the shas the source settled (#2310)", async () => {
  const base = ports({ getEntity: repoDetail(), refresh: refreshProbe().ports });
  const HEAD = "1262a5b610480702cfed4a1f3f76c87bf68fae55";
  const FORK = "907900b245f2941018dfd3ee155b876ef701c102";
  const whole = Array.from({ length: 12 }, (_, n) => `line ${n + 1}`).join("\n");
  const asked: string[] = [];
  const diff: ViewBlock = {
    type: "diff",
    files: [
      {
        path: "src/retry.rs",
        old_path: null,
        change: "modified",
        additions: 1,
        deletions: 1,
        hunks: [
          { header: "@@ -1,1 +1,1 @@", lines: [{ kind: "del", old: 1, new: null, text: "a" }, { kind: "add", old: null, new: 1, text: "line 1" }] },
          { header: "@@ -10,1 +10,1 @@", lines: [{ kind: "context", old: 10, new: 10, text: "line 10" }] },
        ],
        truncated: false,
        threads: [],
      },
    ],
    truncated: false,
    commentable: null,
    gone: [],
    range: { repository: REPO, base: FORK, head: HEAD, commits: [] },
  };
  await draw([REPO, "compare", "feature", "PAY-231-sepa-retry"], {
    ...base,
    entityView: async (id, sectionId) => {
      if (id === FEATURE && sectionId === null) return { sections: [], actions: [] };
      if (id === REPO && sectionId?.startsWith("file@")) {
        asked.push(sectionId);
        return section("f", [{ type: "file", trail: [], path: "src/retry.rs", text: whole, language: null, size: 1, truncated: false, binary: false, url: null, sha: null } as unknown as ViewBlock]);
      }
      if (id === REPO && sectionId === "compare:main...feature/PAY-231-sepa-retry") return section("diff", [diff]);
      return base.entityView(id, sectionId);
    },
  });
  const expand = target.querySelector<HTMLButtonElement>(".tf-cbody .tf-exp");
  expect(expand).not.toBeNull();
  expect(target.querySelector("[data-testid=no-range]")).toBeNull();
  expand!.click();
  await settle();
  expect(asked).toEqual(expect.arrayContaining([`file@${HEAD}:src/retry.rs`, `file@${FORK}:src/retry.rs`]));
  expect(text(target.querySelector(".tf-cbody .tf-diff"))).toMatch(/22 line 2/);
});

test("an address with a tab the repository does not have says so under its header and tabs, and lights none", async () => {
  await draw([REPO, "nope"]);
  expect(text(target.querySelector(".tf-fhead h1"))).toBe("tidewater/payout-service");
  expect(text(target.querySelector(".tf-nohere"))).toContain("This repository has no such tab.");
  expect(target.querySelector(".tf-nohere a")!.getAttribute("href")).toBe(`#/repo/${encodeURIComponent(REPO)}`);
  expect(target.querySelector(".tf-tabs a[aria-current]")).toBeNull();
  expect(target.querySelector(".tf-cbody")).toBeNull();
});

test("Compare's header does not offer New pull request again, though it holds its room: the form below is it", async () => {
  await draw([REPO, "compare"]);
  const reachable = [...target.querySelectorAll(".tf-fhead .tf-acts .tf-btn")].filter((node) => node.closest(".tf-held") === null);
  expect(reachable.map(text)).not.toContain("New pull request");
  expect(target.querySelector(".tf-held .tf-btn")?.textContent).toContain("New pull request");
});

test("Pull requests draws Code's row: the title over its author, the state, and when it last moved", async () => {
  await draw([REPO, "prs"]);
  const row = target.querySelector(".tf-rprs tbody tr")!;
  expect(row.closest("table")!.classList.contains("tf-cprs")).toBe(true);
  expect(row.querySelectorAll("td")).toHaveLength(5);
});

test("a merge queued on the pull request's page draws its row merged at once, over a read from before it (the pr-state fix)", async () => {
  await draw([REPO, "prs"]);
  const stateOf = (n: string) => text([...target.querySelectorAll(".tf-rprs tbody tr")].find((tr) => text(tr).startsWith(n))!.querySelector(".tf-chip"));
  expect(stateOf("#142")).toBe("Open");
  // What TfPr does as the merge's queue answer lands; the mirror's row was read before it.
  notePrMove(`${REPO}#142`, "merge", "open", 80);
  flushSync();
  expect(stateOf("#142")).toBe("Merged");
  expect(stateOf("#139"), "another row is as read").toBe("Merged");
  dropPrMove(`${REPO}#142`, 80);
  flushSync();
  expect(stateOf("#142"), "a refused merge: the read again").toBe("Open");
});

test("the tab's count and its State filter read the state a queued merge leaves; rows read after its read-back say what the mirror says", async () => {
  await draw([REPO, "prs"], ports({ getEntity: repoDetail(), refresh: refreshProbe().ports }));
  const tab = () => [...target.querySelectorAll(".tf-tabs a")].find((a) => text(a).startsWith("Pull requests"))!;
  expect(text(tab().querySelector(".tf-count"))).toBe("1");
  notePrMove(`${REPO}#142`, "merge", "open", 80, "gitea");
  flushSync();
  expect(text(tab().querySelector(".tf-count")), "no open pull request left").toBe("");
  heardRefresh({ entity_id: `${REPO}#142`, write_id: 80, ended: "refreshed" });
  // A read asked after the read-back is the mirror's word: here it still says open (the forge took the merge and did not move it).
  refreshButton()!.click();
  await settle();
  expect(text([...target.querySelectorAll(".tf-rprs tbody tr")].find((tr) => text(tr).startsWith("#142"))!.querySelector(".tf-chip"))).toBe("Open");
});

test("the page walks its repository at the forge once it drew, and a walk that brought a merge lands the rows in place (Fable's ruling 2)", async () => {
  const walks: string[] = [];
  let walked = false;
  const merged = { ...PRS[0]!, state: { text: "merged", tone: "ok" as const } };
  const opened = row(`${REPO}#150`, "pr", "Opened at the forge", { path: "tidewater/payout-service", state: { text: "open", tone: "info" } });
  const base = ports();
  await draw(
    [REPO, "prs"],
    ports({
      walk: async (id) => (walks.push(id), (walked = true), { outcome: "refreshed" }),
      // The forge sorts the merged one first now: the drawn list keeps it where it was.
      listEntities: async (...args) => (walked ? { ...(await base.listEntities(...args)), rows: [opened, PRS[1]!, merged, PRS[2]!] } : base.listEntities(...args)),
    }),
  );
  const rows = () => [...target.querySelectorAll(".tf-rprs tbody tr")].map((tr) => [text(tr.querySelector("td")), text(tr.querySelector(".tf-chip"))]);
  expect(rows()).toEqual([["#142", "Open"], ["#139", "Merged"]]);
  await new Promise((resolve) => setTimeout(resolve, 120));
  await settle();
  expect(walks).toEqual([REPO]);
  expect(rows()).toEqual([["#150", "Open"], ["#142", "Merged"], ["#139", "Merged"]]);
});

test("a walk that brought nothing reads nothing again, and the page walks again only after a while", async () => {
  const walks: string[] = [];
  let reads = 0;
  const base = ports();
  await draw([REPO, "prs"], ports({ walk: async (id) => (walks.push(id), { outcome: "run_in_flight" }), listEntities: async (...args) => (reads++, base.listEntities(...args)) }));
  await new Promise((resolve) => setTimeout(resolve, 120));
  await settle();
  expect(walks).toEqual([REPO]);
  const after = reads;
  window.dispatchEvent(new Event("focus"));
  await settle();
  expect(walks, "the window back within 30 s walks nothing").toEqual([REPO]);
  expect(reads).toBe(after);
});

test("Branches names a tip's author by the name the record gives, else the account, and the reader's own reads you (F-107)", async () => {
  await draw(
    [REPO, "branches"],
    ports({
      codeFacts: async (ids) =>
        [
          { entity_id: MAIN, author: "jonas.becker", author_name: "Jonas Becker", mine: false, language: null, teams: [] },
          { entity_id: FEATURE, author: "mara", author_name: "Mara Lindqvist", mine: true, language: null, teams: [] },
        ].filter((facts) => ids.includes(facts.entity_id)),
    }),
  );
  const by = [...target.querySelectorAll(".tf-rbranches tbody tr")].map((tr) => text(tr.querySelectorAll("td")[3]!));
  expect(by).toEqual(expect.arrayContaining(["Jonas Becker", "you"]));
  expect(by).not.toContain("jonas.becker");
});

test("the page asks for its own repository's pull requests by name, and keeps those whose path is it (the query audit, 2026-11-12)", async () => {
  const asked: EntityFilter[] = [];
  await draw(
    [REPO],
    ports({
      listEntities: async (filter) => {
        asked.push(filter);
        // A stored parent answers to a bare name too: the page keeps only its own repository's.
        return { rows: PRS, total: PRS.length, tally: null, groups: null };
      },
    }),
  );
  expect(asked.map((filter) => [filter.kinds, filter.parent])).toEqual([[["pr"], "tidewater/payout-service"]]);
  // The open one of its own; the other repository's open one is not counted.
  expect([...target.querySelectorAll(".tf-tabs a")].map(text)).toContain("Pull requests 1");
});

/** A run of the fixture's one pipeline on `branch`, failed or passed (the repo-builds pass). */
function built(id: string, branch: string, failed: boolean) {
  return {
    entity_id: `tc:build:${id}`,
    number: id,
    state: { text: failed ? "FAILURE" : "SUCCESS", tone: failed ? ("err" as const) : ("ok" as const) },
    branch,
    started: "2026-08-22T10:00:00Z",
    finished: "2026-08-22T10:05:00Z",
    duration_seconds: 300,
    updated_at: "2026-08-22T10:05:00Z",
    web_url: null,
    trigger: null,
    started_by: null,
    started_by_me: false,
    revision: null,
    commit_id: null,
    agent: null,
    current_step: null,
    tests: null,
  };
}

test("Branches says each branch's builds, linking to the Builds tab at it; the tab counts what builds the repository", async () => {
  const builds = {
    pipelines: [
      {
        entity_id: "tc:buildType:Payout_Build",
        source_id: "tc",
        source_name: "TeamCity",
        name: "Payout_Build",
        group: null,
        path: [],
        web_url: null,
        latest: [built("2", "feature/PAY-231-sepa-retry", true), built("1", "main", false)],
      },
    ],
    pull_requests: [],
  };
  await draw([REPO, "branches"], ports({ repoPipelines: () => Promise.resolve(builds) }));
  const cells = [...target.querySelectorAll(".tf-rbranches tbody tr")].map((tr) => text(tr.querySelector(".tf-rbr-builds")));
  expect(cells).toEqual(["All 1 passed", "1 of 1 failing"]);
  const failing = target.querySelector<HTMLAnchorElement>(".tf-rbranches .tf-rbr-builds a.tf-err")!;
  expect(failing.getAttribute("href")).toBe(`#/repo/${encodeURIComponent(REPO)}/builds/${encodeURIComponent("feature/PAY-231-sepa-retry")}`);
  const tab = [...target.querySelectorAll(".tf-tabs a")].find((a) => a.textContent?.startsWith("Builds"))!;
  expect(text(tab)).toBe("Builds 1");
});

// ---- A running build moves on the Builds tab as it does on the Builds page (#2558) ----

/** Run 3 of the fixture's pipeline on `main`, running until `done` says it finished, passed. */
function runningOnMain() {
  const state = { done: false, reads: 0 };
  const run = () => ({
    ...built("3", "main", false),
    state: state.done ? { text: "SUCCESS", tone: "ok" as const } : { text: "running", tone: "info" as const },
    finished: state.done ? "2026-08-22T10:05:00Z" : null,
    duration_seconds: state.done ? 300 : null,
  });
  const repoPipelines = async () => {
    state.reads += 1;
    return {
      pipelines: [
        { entity_id: "tc:buildType:Payout_Build", source_id: "tc", source_name: "TeamCity", name: "Payout_Build", group: null, path: [], web_url: null, latest: [run()] },
        { entity_id: "tc:buildType:Payout_Lint", source_id: "tc", source_name: "TeamCity", name: "Payout_Lint", group: null, path: [], web_url: null, latest: [built("1", "main", false)] },
      ],
      pull_requests: [],
    };
  };
  return { state, repoPipelines };
}

/** The window's events as a test sends them: each handler the page listened with, by event. */
function ears() {
  const heard = new Map<string, ((event: { payload: unknown }) => void)[]>();
  return {
    listen: (async (event: string, handler: (event: { payload: unknown }) => void) => {
      heard.set(event, [...(heard.get(event) ?? []), handler]);
      return () => heard.set(event, (heard.get(event) ?? []).filter((one) => one !== handler));
    }) as NonNullable<RepoPorts["listen"]>,
    send: (event: string, payload: unknown) => {
      for (const hear of heard.get(event) ?? []) hear({ payload });
    },
  };
}

const verdict = () => text(target.querySelector(".tf-rbp-ok > span"));

test("a running build on the Builds tab moves when a read of it at its source moved it (#2558)", async () => {
  const { state, repoPipelines } = runningOnMain();
  const { listen, send } = ears();
  await draw([REPO, "builds", "main"], ports({ repoPipelines, listen, follow: async () => ({ outcome: "refreshed" }) }));
  expect(verdict()).toBe("Every pipeline that ran on main passed, and 1 still running.");

  state.done = true;
  // Another item's refresh moves nothing here.
  send("item:refreshed", { entity_id: "tc:build:99", write_id: null, ended: "refreshed" });
  await settle();
  expect(verdict()).toBe("Every pipeline that ran on main passed, and 1 still running.");

  send("item:refreshed", { entity_id: "tc:build:3", write_id: null, ended: "refreshed" });
  await settle();
  expect(verdict()).toBe("Every pipeline that ran on main passed.");
});

test("a CI source's run that brought news reads the Builds tab again; another source's does not (#2558)", async () => {
  const { state, repoPipelines } = runningOnMain();
  const { listen, send } = ears();
  await draw([REPO, "builds", "main"], ports({ repoPipelines, listen, follow: async () => ({ outcome: "refreshed" }) }));
  state.done = true;
  send("sync:state", { source_id: "jira", running: false, last_changed: true });
  await settle();
  expect(verdict()).toBe("Every pipeline that ran on main passed, and 1 still running.");

  send("sync:state", { source_id: "tc", running: false, last_changed: true });
  await settle();
  expect(verdict()).toBe("Every pipeline that ran on main passed.");
});

test("the Builds tab reads the runs it draws in flight at their source once it has drawn, and no finished one (#2558)", async () => {
  const { repoPipelines } = runningOnMain();
  const followed: string[] = [];
  await draw([REPO, "builds", "main"], ports({ repoPipelines, listen: ears().listen, follow: async (id) => (followed.push(id), { outcome: "refreshed" }) }));
  await new Promise((resolve) => setTimeout(resolve, 150));
  await settle();
  expect(followed).toEqual(["tc:build:3"]);
});

test("⌘R on the Builds tab reads that repository and its runs in flight at their source, and syncs no source (#2558)", async () => {
  const { state, repoPipelines } = runningOnMain();
  const probe = refreshProbe();
  const followed: string[] = [];
  // The run's refresh answers a moment later, having moved it: the tab's read after it is what draws it finished.
  const follow = async (id: string) => {
    followed.push(id);
    await new Promise((resolve) => setTimeout(resolve, 0));
    state.done = true;
    return { outcome: "refreshed" as const };
  };
  await draw([REPO, "builds", "main"], ports({ getEntity: repoDetail(), refresh: probe.ports, repoPipelines, listen: ears().listen, follow }));
  // What the tab's own open asked is not the press's.
  await new Promise((resolve) => setTimeout(resolve, 150));
  await settle();
  followed.length = 0;
  state.done = false;

  pressRefreshKey();
  await settle();
  expect(probe.refreshed).toEqual([REPO]);
  expect(followed).toEqual(["tc:build:3"]);
  expect(probe.synced).toEqual([]);
  expect(verdict()).toBe("Every pipeline that ran on main passed.");
});

// ---- The working tree in the branch box (2026-11-21; Björn: "I would like to be able to select the current work tree in
// the branch selection, not just the branches") ----

const HEAD = "4".repeat(40);
/** A clone at `HEAD` with `src/retry.rs` changed, `README.md` deleted and `notes/todo.md` untracked; the source is never asked for its files. */
function onDiskPorts() {
  const disk = checkoutPorts(() => ({
    commit: HEAD,
    as_of: null,
    truncated: false,
    entries: [
      { path: "src", kind: "dir", size: null },
      { path: "src/retry.rs", kind: "file", size: 10 },
      { path: "README.md", kind: "file", size: 10 },
      { path: "Cargo.toml", kind: "file", size: 10 },
    ],
  }));
  const bytesRead: string[] = [];
  const localFiles: string[] = [];
  const stand = ports({
    entityCheckout: async () => ({
      ...checkout("/Users/mara/src/payout-service"),
      tree: { status: "read", branch: "main", head: HEAD, upstream: null, ahead: 1, behind: 0, changed: 2, untracked: 1, conflicted: 0 },
    }),
    entityCheckoutDiff: async () => ({
      ...LOCAL,
      unstaged: {
        files: [
          { path: "src/retry.rs", old_path: null, change: "modified", additions: 1, deletions: 1, hunks: [], truncated: false, threads: [] },
          { path: "README.md", old_path: null, change: "deleted", additions: 0, deletions: 3, hunks: [], truncated: false, threads: [] },
        ],
        truncated: false,
      },
      untracked: ["notes/todo.md"],
      stamp: "s1",
    }),
    localDir: disk.localDir,
    localPaths: disk.localPaths,
    localFile: async (_at, commit, path) => (
      localFiles.push(`${commit.slice(0, 7)}:${path}`),
      { type: "file", trail: [], path, text: `// ${path} at HEAD\n`, language: null, size: 10, truncated: false, binary: false, url: null, sha: "b".repeat(40), commit }
    ),
    fileBytes: async (_id, path, commit) => (bytesRead.push(`${commit}:${path}`), new TextEncoder().encode(`// ${path} on disk\n`)),
  });
  return { stand, bytesRead, localFiles, dirReads: disk.dirReads };
}

async function chooseWorkingTree() {
  const select = branchSelect();
  select.value = "~working-tree";
  select.dispatchEvent(new Event("change", { bubbles: true }));
  await settle();
}

test("cloned here, the working tree is the box's first choice; chosen, the box says so with the folder glyph", async () => {
  const { stand } = onDiskPorts();
  await draw([REPO], stand);
  const first = branchSelect().options[0]!;
  expect([first.value, first.textContent, first.disabled]).toEqual(["~working-tree", "Working tree", false]);
  expect(branchValues()).toEqual(["main", "feature/PAY-231-sepa-retry"]);
  await chooseWorkingTree();
  expect(branchSelect().value).toBe("~working-tree");
  expect(branchSelect().title).toBe("Working tree at /Users/mara/src/payout-service, on main");
  expect(target.querySelector(".tf-bsel > svg path")!.getAttribute("d")).toBe("M1.75 3.75h4.5l1.5 1.5h6.5v7h-12.5z");
  // Compare opens with the working tree as *From*, ADR-0049's page.
  const compare = [...target.querySelectorAll<HTMLAnchorElement>(".tf-tabs a")].find((a) => text(a) === "Compare")!;
  expect(compare.getAttribute("href")).toBe("#/repo/gitea%3Atidewater%2Fpayout-service/compare/~working-tree");
});

test("the working tree's tree waits for git's list, so it draws once with its changes, and nothing moves when the list lands", async () => {
  const { stand } = onDiskPorts();
  let answer!: () => void;
  const late = new Promise<void>((resolve) => (answer = resolve));
  await draw([REPO], { ...stand, entityCheckoutDiff: async (id) => (await late, stand.entityCheckoutDiff(id)) });
  await chooseWorkingTree();
  expect(tree()).toEqual([]);
  answer();
  await settle();
  expect(tree()).toEqual(["notes", "src", "Cargo.toml", "README.md"]);
});

test("git's list failing does not hold the working tree's tree for ever: it draws HEAD with no marks", async () => {
  const { stand } = onDiskPorts();
  await draw([REPO], { ...stand, entityCheckoutDiff: async () => Promise.reject({ code: "io", message: "git failed" }) });
  await chooseWorkingTree();
  expect(tree()).toEqual(["src", "Cargo.toml", "README.md"]);
});

test("not cloned here, the box offers no working tree", async () => {
  await draw([REPO], ports({}, false));
  expect([...branchSelect().options].map((option) => option.value)).not.toContain("~working-tree");
});

test("Files on the working tree: HEAD's tree from the checkout, the changes laid over it, the source never asked", async () => {
  const { stand, dirReads } = onDiskPorts();
  await draw([REPO], stand);
  const before = viewReads.filter(([, section]) => section?.startsWith("files")).length;
  await chooseWorkingTree();
  expect(tree()).toEqual(["notes", "src", "Cargo.toml", "README.md"]);
  expect(dirReads).toContain(`${HEAD}:`);
  const row = (name: string) => [...target.querySelectorAll<HTMLElement>(".tf-ftree a")].find((a) => text(a) === name)!;
  expect(row("README.md").classList.contains("tf-deleted")).toBe(true);
  expect(row("README.md").title).toBe("Deleted in your working tree");
  expect(viewReads.filter(([, section]) => section?.startsWith("files")).length).toBe(before);
  // The landing's line is the working tree's, in the last commit's room.
  expect(text(target.querySelector(".tf-wtline"))).toBe("Working tree on main at 4444444, 1 ahead, 3 changed Local changes");
});

test("a changed file on the working tree is read from disk and says so; Edit is off; an unchanged one is HEAD's", async () => {
  const { stand, bytesRead, localFiles } = onDiskPorts();
  await draw([REPO, "files", "src", "retry.rs"], stand);
  localFiles.length = 0;
  await chooseWorkingTree();
  expect(bytesRead).toEqual(["~working-tree:src/retry.rs"]);
  expect(target.querySelector(".tf-fileview")!.textContent).toContain("// src/retry.rs on disk");
  expect(text(target.querySelector(".tf-fileview .tf-banner.tf-info"))).toContain("Shown as it is on disk, with your uncommitted change.");
  const tree = [...target.querySelectorAll<HTMLElement>(".tf-ftree a")];
  expect(tree.find((a) => text(a) === "retry.rs")!.classList.contains("tf-changed")).toBe(true);
  expect(localFiles).toEqual([]);
});

test("a held working tree whose checkout stops reading goes back to the default branch and says so once", async () => {
  const { stand } = onDiskPorts();
  let gone = false;
  const going = { ...stand, entityCheckout: async (id: string) => (gone ? { ...checkout("/Users/mara/src/payout-service"), tree: { status: "no_git" as const } } : stand.entityCheckout(id)) };
  view = mount(TfRepo, { target, props: { ready: true, rest: [REPO], job: null, ports: going, now: NOW, visible: true } });
  await settle();
  await chooseWorkingTree();
  expect(branchSelect().value).toBe("~working-tree");
  gone = true;
  window.dispatchEvent(new Event("focus"));
  await settle();
  expect(branchSelect().value).toBe("main");
  expect(toasts.items.some((toast) => toast.text === "The working tree can't be read any more: git was not found. Showing main.")).toBe(true);
});

test("on a working tree whose branch the forge lacks, New pull request is off until the branch is pushed, and Commits says so", async () => {
  const { stand } = onDiskPorts();
  const unpushed = {
    ...stand,
    entityCheckout: async (id: string) => {
      const was = await stand.entityCheckout(id);
      return { ...was, tree: { status: "read" as const, branch: "feat/local-only", head: HEAD, upstream: null, ahead: 0, behind: 0, changed: 2, untracked: 1, conflicted: 0 } };
    },
  };
  await draw([REPO, "commits"], unpushed);
  const primary = () => [...target.querySelectorAll<HTMLElement>(".tf-fhead .tf-repohold .tf-primary")].find((el) => text(el) === "New pull request")!;
  expect(primary().tagName).toBe("A");
  await chooseWorkingTree();
  expect(primary().tagName).toBe("BUTTON");
  expect(primary().getAttribute("aria-disabled")).toBe("true");
  expect(primary().title).toBe("Push feat/local-only to Tidewater Gitea first");
  expect(text(target.querySelector(".tf-wtnote"))).toBe("feat/local-only is not on Tidewater Gitea yet. Push it to open a pull request from it.");
  // Back on a branch, the button proposes one again.
  branchSelect().value = "main";
  branchSelect().dispatchEvent(new Event("change", { bubbles: true }));
  await settle();
  expect(primary().tagName).toBe("A");
});

test("first open keeps the active branch selected and spacing intact when live branches replace the mirror placeholder (#2523)", async () => {
  let releaseBranches!: () => void;
  const branchesGate = new Promise<void>((resolve) => (releaseBranches = resolve));
  const base = ports({
    entityCheckout: async () => ({
      ...checkout("/Users/mara/src/payout-service"),
      tree: { status: "read", branch: "main", head: HEAD, upstream: null, ahead: 1, behind: 0, changed: 1, untracked: 0, conflicted: 0 },
    }),
  });
  const staged = {
    ...base,
    getEntity: async (id: string) => {
      const detail = await base.getEntity(id);
      return id === REPO ? ({ ...detail, default_branch: "main" } as EntityDetail) : detail;
    },
    entityView: async (id: string, sectionId: string | null) => {
      if (id === REPO && sectionId === "branches") await branchesGate;
      return base.entityView(id, sectionId);
    },
  };

  await draw([REPO], staged);
  // Before live branches arrive, the mirror default is selected as the placeholder option after Working tree + <hr>.
  expect(branchSelect().value).toBe("main");
  const activeOptionBefore = branchSelect().selectedOptions[0];
  expect(activeOptionBefore?.value).toBe("main");
  expect(branchSelect().querySelectorAll("hr").length).toBe(1);

  // Live branches arrive and replace the placeholder: the active branch must stay selected without unmounting its <option> or jumping to Working tree.
  releaseBranches();
  await settle();
  expect(branchSelect().value).toBe("main");
  expect(branchSelect().selectedOptions[0]).toBe(activeOptionBefore);
  expect(branchSelect().selectedOptions[0]?.textContent).toBe("main");
  expect(branchValues()).toEqual(["main", "feature/PAY-231-sepa-retry"]);
  expect(branchSelect().querySelectorAll("hr").length).toBe(1);

  // A second open of the same repository matches the first open identically.
  const firstSnapshot = {
    value: branchSelect().value,
    options: [...branchSelect().options].map((o) => [o.value, o.textContent, o.disabled]),
    hrs: branchSelect().querySelectorAll("hr").length,
  };
  unmount(view!);
  view = null;
  await draw([REPO], base);
  expect({
    value: branchSelect().value,
    options: [...branchSelect().options].map((o) => [o.value, o.textContent, o.disabled]),
    hrs: branchSelect().querySelectorAll("hr").length,
  }).toEqual(firstSnapshot);
});
