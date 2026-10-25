/**
 * The repository page (#838, R-8.3 to R-8.5, R-8.7, R-8.10), mounted over
 * stand-in reads: the header and its checkout, the branch dropdown that sets
 * the branch for every tab, the Files tree with the checkout's changes marked,
 * the file with line numbers, Commits and a commit's diff, the repository's
 * pull requests, and the tabs built later. What the source's view holds is
 * `knobas-source-gitea`'s (its view tests and `gitea-live`); here it is a
 * stand-in shaped like it.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import type {
  CheckoutView,
  DiffRead,
  EntityDetail,
  EntityPage,
  EntityRow,
  SourceView,
  TreeEntry,
  ViewAction,
  ViewBlock,
  ViewRow,
} from "../../ipc/entity";
import TfRepo, { type RepoPorts } from "./TfRepo.svelte";
import { createSourceSyncs } from "../role-sync.svelte";
import type { DetailRefreshPorts } from "../refresh.svelte";

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
      if (id === C1 && sectionId === null)
        return withActions(section("diff", [
          {
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
          },
        ]));
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

test("the header names the repository, its last push and its checkout; the tabs follow with their counts", async () => {
  await draw([REPO]);

  expect(text(target.querySelector("h1"))).toBe("tidewater/payout-service");
  expect(text(target.querySelector(".tf-fhead .tf-sub"))).toBe(
    "Payments, Rust, last push today 12:30. Cloned at /Users/mara/src/payout-service, main, 1 ahead, 1 changed.",
  );
  expect([...target.querySelectorAll(".tf-crumbs a.tf-crumb, .tf-crumbs strong")].map(text)).toEqual([
    "Code",
    "Repositories",
    "tidewater/payout-service",
  ]);
  const tabs = [...target.querySelectorAll(".tf-tabs a")];
  expect(tabs.map(text)).toEqual(["Files", "Commits", "Branches 2", "Pull requests 1", "Local changes 1"]);
  expect(tabs[0]!.getAttribute("aria-current")).toBe("page");
  expect(tabs[1]!.getAttribute("href")).toBe("#/repo/gitea%3Atidewater%2Fpayout-service/commits");
  const select = target.querySelector<HTMLSelectElement>(".tf-fhead select")!;
  expect([...select.options].map((o) => o.textContent)).toEqual(["main", "feature/PAY-231-sepa-retry"]);
  expect(select.value).toBe("main");
});

test("Files opens the README, marks the locally changed file M, and a changed file warns", async () => {
  await draw([REPO]);
  expect(tree()).toEqual(["src/", "README.md"]);
  expect(text(target.querySelector(".tf-fileview header"))).toContain("README.md 2 lines, on main");
  expect([...target.querySelectorAll(".tf-code .tf-l")].map(text)).toEqual(["1// README.md", "2fn main() {}"]);
  expect(target.querySelector(".tf-banner")).toBeNull();
  unmount(view!);
  view = null;

  await draw([REPO, "files", "src", "retry.rs"]);
  expect(tree()).toEqual(["src/", "lib.rs", "retry.rs", "README.md"]);
  const changed = [...target.querySelectorAll(".tf-ftree a")].find((a) => text(a) === "retry.rs")!;
  expect(changed.classList.contains("tf-changed")).toBe(true);
  expect(changed.getAttribute("aria-current")).toBe("true");
  expect(changed.getAttribute("href")).toBe("#/repo/gitea%3Atidewater%2Fpayout-service/files/src/retry.rs");
  expect(text(target.querySelector(".tf-banner"))).toBe("Your checkout has an uncommitted change to this file. See local changes");
  expect(target.querySelector(".tf-banner a")!.getAttribute("href")).toBe("#/repo/gitea%3Atidewater%2Fpayout-service/local");
});

test("an uncloned repository reads its files through the source and says it is not on this machine", async () => {
  // With nothing declared the header starts at the push (#973).
  await draw([REPO, "files", "src", "lib.rs"], ports({ codeFacts: async () => [] }, false));
  expect(text(target.querySelector(".tf-fhead .tf-sub"))).toBe("Last push today 12:30. Not on this machine yet.");
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

  expect(tree()).toEqual(["src/", "README.md", "NEW.md"]);
  expect(text(target.querySelector(".tf-fileview header"))).toContain("1 line, on feature/PAY-231-sepa-retry");
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
  expect(rows.map(text)).toEqual(["c90d11a2b3PAY-231: jitter in backoffmara · 3 hours ago", "a41f2c0000Bump depsjonas · 2 days ago"]);
  expect(rows[0]!.querySelector("a")!.getAttribute("href")).toBe(
    "#/repo/gitea%3Atidewater%2Fpayout-service/commits/gitea%3Atidewater%2Fpayout-service%40c90d11a2b3c4",
  );
  // Not in the mirror: nothing to open it at.
  expect(rows[1]!.querySelector("a")).toBeNull();

  const field = target.querySelector<HTMLInputElement>("input[data-filter]")!;
  field.value = "jonas";
  field.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
  expect([...target.querySelectorAll(".tf-cbody tbody tr")].map(text)).toEqual(["a41f2c0000Bump depsjonas · 2 days ago"]);
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
    "c90d11a2b3PAY-231: jitter in backoffmara · 3 hours ago",
  ]);
});

test("a commit draws its sha, message, author and time, its diff per file, and New branch from here", async () => {
  await draw([REPO, "commits", C1]);
  expect(text(target.querySelector(".tf-cgroup-h"))).toContain("c90d11a2b3 PAY-231: jitter in backoff mara, today 11:42");
  const diff = target.querySelector(".tf-diff")!;
  expect(text(diff.querySelector(".tf-fh"))).toBe("src/retry.rs +1 −1");
  expect([...diff.querySelectorAll(".tf-ln")].map(text)).toEqual(["1 fn retry() {", "2− 3", "2+ 5"]);
  expect(diff.querySelector(".tf-ln.tf-del")).not.toBeNull();
  const branch = [...target.querySelectorAll("button")].find((b) => text(b) === "New branch from here…")!;
  expect(branch.getAttribute("aria-disabled")).toBeNull();
  expect(target.querySelector(".tf-tabs a[aria-current='page']")!.textContent).toContain("Commits");
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
  // The pull request's diff block (#842's `TfDiff`), each file marked.
  expect([...target.querySelectorAll(".tf-cbody .tf-diff .tf-fh .tf-mono")].map(text)).toEqual([
    "src/retry.rs (modified, not staged)",
    "src/sepa/mod.rs (modified, staged)",
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
  expect(text(target.querySelector(".tf-cbody"))).toContain("gitea:tidewater/nowhere is not in the mirror");
});

// -- Refresh (#2135) -----------------------------------------------------------

const quietSyncs = () =>
  createSourceSyncs({
    syncRole: () => Promise.reject(new Error("unused")),
    syncNow: () => Promise.reject(new Error("unused")),
    syncStatus: async () => [],
    listen: async () => () => {},
  });

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
const refreshing = (): DetailRefreshPorts => ({
  refreshEntity: async () => ({ outcome: "refreshed" }),
  syncs: quietSyncs(),
});

test("Refresh on a repository reads it from its source, then its row, its branches and the open file again, the page kept meanwhile", async () => {
  const refreshed: string[] = [];
  let rows = 0;
  let answer!: () => void;
  const getEntity = repoDetail();
  await draw(
    [REPO],
    ports({
      getEntity: async (id) => (id === REPO && (rows += 1), getEntity(id)),
      refresh: {
        refreshEntity: (id) => {
          refreshed.push(id);
          return new Promise((resolve) => (answer = () => resolve({ outcome: "refreshed" })));
        },
        syncs: quietSyncs(),
      },
    }),
  );
  expect(refreshButton()?.textContent).toBe("Refresh");
  const before = { rows, reads: viewReads.length };

  refreshButton()!.click();
  await settle();
  expect(refreshButton()?.textContent).toBe("Refreshing…");
  expect(tree()).toEqual(["src/", "README.md"]);

  answer();
  await settle();
  expect(refreshed).toEqual([REPO]);
  expect(rows).toBe(before.rows + 1);
  const again = viewReads.slice(before.reads);
  expect(again).toEqual(expect.arrayContaining([[REPO, "branches"], [REPO, "files"], [REPO, "file:README.md"]]));
  expect(refreshButton()?.textContent).toBe("Refresh");
  expect(text(target.querySelector("h1"))).toBe("tidewater/payout-service");
  expect(tree()).toEqual(["src/", "README.md"]);
  expect([...target.querySelectorAll(".tf-code .tf-l")].map(text)).toEqual(["1// README.md", "2fn main() {}"]);
});

test("a refresh reads Commits, a commit, the compare page and Branches again over what they drew", async () => {
  const stand = (): RepoPorts => ports({ getEntity: repoDetail(), refresh: refreshing() });
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
  expect(await press()).toEqual(expect.arrayContaining([[FEATURE, null], [FEATURE, "diff"]]));
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
      refresh: { refreshEntity: async () => ({ outcome: "failed", reason: "502 Bad Gateway" }), syncs: quietSyncs() },
    }),
  );
  refreshButton()!.click();
  await settle();
  expect(text(target.querySelector(".tf-refresh-failed"))).toBe("Couldn't refresh: 502 Bad Gateway");
  expect(text(target.querySelector("h1"))).toBe("tidewater/payout-service");
  expect(target.querySelectorAll(".tf-cbody tbody tr").length).toBe(2);
});

test("a repository whose adapter does not read one item offers Sync now; a turned-off source neither, under its banner", async () => {
  await draw([REPO], ports({ getEntity: repoDetail({ refreshes: false }), refresh: refreshing() }));
  expect(refreshButton()?.textContent).toBe("Sync now");
  expect(target.querySelector(".tf-turnedoff")).toBeNull();
  unmount(view!);

  await draw([REPO], ports({ getEntity: repoDetail({ enabled: false }), refresh: refreshing() }));
  expect(refreshButton()).toBeNull();
  expect(text(target.querySelector(".tf-turnedoff"))).toContain("Tidewater Gitea is disabled");
});

test("a repository withdrawn upstream says so and keeps Refresh", async () => {
  await draw([REPO], ports({ getEntity: repoDetail({ deleted_at: "2026-08-20T10:00:00Z" }), refresh: refreshing() }));
  expect(text(target.querySelector(".tf-withdrawn"))).toContain("Withdrawn upstream");
  expect(refreshButton()?.textContent).toBe("Refresh");
});

test("the compare page keeps its branch's diff drawn while a refresh reads it again", async () => {
  const base = ports({ getEntity: repoDetail(), refresh: refreshing() });
  let diffs = 0;
  await draw(
    [REPO, "compare", "feature", "PAY-231-sepa-retry"],
    {
      ...base,
      entityView: async (id, sectionId) => {
        if (id === FEATURE && sectionId === null) return { sections: [], actions: [] };
        if (id !== FEATURE || sectionId !== "diff") return base.entityView(id, sectionId);
        // The first read answers; the refresh's stays out.
        if ((diffs += 1) > 1) return new Promise(() => {});
        const diff = await base.entityView(C1, null);
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
  const base = ports({ getEntity: repoDetail(), refresh: refreshing() });
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
