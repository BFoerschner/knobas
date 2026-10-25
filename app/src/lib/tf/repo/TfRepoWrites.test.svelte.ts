/**
 * The repository page's writes (#839, #840, #845), mounted over stand-in
 * reads and a recording `submit_write`: the Branches tab and its *New
 * branch…* and *Delete…* dialogs, the compare page and its *Create pull
 * request*, and a file's edit with its commit dialog. What the source's views
 * hold is `knobas-source-gitea`'s (its view tests and `gitea-live`); here it is
 * a stand-in shaped like it. What reaches the queue is asserted as the payload
 * `submit_write` was handed, which is the whole of what the shell decides.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, describe, expect, test } from "vitest";

import type { EntityDetail, EntityRow, SourceView, ViewAction, ViewBlock, ViewRow } from "../../ipc/entity";
import type { QueuedWrite, WriteOpPayload } from "../../ipc/sources";
import { health } from "../../shell/health.svelte";
import { toasts } from "../../shell/toasts.svelte";
import { echoEvents } from "../echo-events.fixture";
import { echoWrite, echoesOn, forgetEchoes } from "../queued.svelte";
import { DELETED, MADE, REPO_BRANCHES, REPO_FILES, commitKind } from "./repo";
import { sourceName } from "./repo";
import TfRepo, { type RepoPorts } from "./TfRepo.svelte";

const NOW = new Date(2026, 7, 22, 14, 32);
const REPO = "gitea:tidewater/payout-service";
const at = (name: string) => `${REPO}@refs/heads/${name}`;
const MAIN = at("main");
const FEATURE = at("feature/PAY-231-sepa-retry");
const SPIKE = at("spike/sepa-instant");
const CHORE = at("chore/bump-tokio");
const LOCKED = at("release/1.9");
const C1 = `${REPO}@c90d11e4a7b2`;

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

const one = (id: string, blocks: ViewBlock[], count: number | null = null): SourceView => ({
  sections: [{ id, title: id, count, deferred: false, blocks }],
  actions: [],
});

const action = (write_op: string, disabled: string | null = null): ViewAction =>
  ({ write_op, label: write_op, fields: [], confirm: null, primary: false, danger: false, disabled, with_settings: false, values: {}, entity: null }) as unknown as ViewAction;

/** A branch's whole view, as `view/branch.rs` draws it. */
function branchView(o: { ahead: number; behind: number; pr?: string; actions: ViewAction[] }): SourceView {
  return {
    sections: [
      {
        id: "overview",
        title: "Overview",
        count: null,
        deferred: false,
        blocks: [
          ...(o.pr
            ? [{ type: "entities" as const, title: "Pull request", rows: [{ ...vrow(o.pr, "SEPA retry", "open", "#142"), tone: "info" as const }] }]
            : []),
          {
            type: "facts",
            facts: [
              { label: "Base", value: "main", tone: null },
              { label: "Ahead", value: String(o.ahead), tone: null },
              { label: "Behind", value: String(o.behind), tone: null },
            ],
          },
        ],
      },
      {
        id: "commits",
        title: "Commits",
        count: 1,
        deferred: false,
        blocks: [{ type: "entities", title: null, rows: [vrow(C1, "PAY-231: jitter in backoff", "c90d11e4a7", "mara · 3 hours ago")] }],
      },
      { id: "diff", title: "Diff", count: 1, deferred: true, blocks: [] },
    ],
    actions: o.actions,
  };
}

/** A commit's whole view carries *New branch…* from it (#707), its whole sha the value. */
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

const BRANCH_VIEWS: Record<string, SourceView> = {
  [FEATURE]: branchView({ ahead: 3, behind: 0, pr: `${REPO}#142`, actions: [action("create_branch"), action("delete_branch")] }),
  [SPIKE]: branchView({ ahead: 2, behind: 9, actions: [action("create_pull_request"), action("create_branch"), action("delete_branch")] }),
  [CHORE]: branchView({ ahead: 0, behind: 6, actions: [action("create_pull_request", "No commits ahead of main"), action("create_branch"), action("delete_branch")] }),
  [LOCKED]: branchView({
    ahead: 1,
    behind: 0,
    actions: [action("create_pull_request"), action("create_branch"), action("delete_branch", "Protected branch: lift its protection on Gitea to delete it")],
  }),
};

const DIFF: ViewBlock = {
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
      hunks: [{ header: "@@", lines: [{ kind: "del", old: 1, new: null, text: "3" }, { kind: "add", old: null, new: 1, text: "5" }] }],
    },
  ],
};

let target: HTMLElement;
let view: ReturnType<typeof mount> | null = null;
let writes: WriteOpPayload[];
/** Each write's `linkTo` (#984), `null` where none was asked for. */
let links: (string | null)[];
let reads: [string, string | null][];

beforeEach(() => {
  forgetEchoes();
  toasts.items = [];
  writes = [];
  links = [];
  reads = [];
  location.hash = "";
  target = document.createElement("div");
  document.body.appendChild(target);
});

afterEach(() => {
  health.replace([]);
  if (view) unmount(view);
  view = null;
  target.remove();
  document.querySelectorAll(".tf-modal").forEach((node) => node.remove());
});

function ports(over: Partial<RepoPorts> = {}): RepoPorts {
  return {
    getEntity: async (id) => {
      if (id === REPO)
        return {
          row: row(REPO, "repo", "tidewater/payout-service"),
          source: { id: "gitea", display_name: "Gitea" },
          author: null,
        } as unknown as EntityDetail;
      if (id === "gitea:PAY-231")
        return {
          row: row(id, "ticket", "Retry failed SEPA payouts"),
          body_text: "The nightly batch leaves 14 payouts unsettled.",
        } as unknown as EntityDetail;
      if (id === C1) return { row: row(C1, "commit", "PAY-231: jitter in backoff"), author: "mara" } as unknown as EntityDetail;
      throw { code: "not_found", message: `${id} is not in the mirror` };
    },
    listEntities: async () => ({ rows: [], total: 0, tally: null, groups: null }),
    entityView: async (id, section) => {
      reads.push([id, section]);
      if (id === REPO && section === "branches")
        return one("branches", [
          {
            type: "entities",
            title: null,
            rows: [
              { ...vrow(MAIN, "main", "default", "5d77e1a2c3 · 1 day ago"), tone: "muted" },
              vrow(FEATURE, "feature/PAY-231-sepa-retry", null, "c90d11e4a7 · 3 hours ago"),
              vrow(SPIKE, "spike/sepa-instant", null, "9a1b2c3d4e · 9 days ago"),
              vrow(CHORE, "chore/bump-tokio", null, "77aa00bb11 · 5 days ago"),
              vrow(LOCKED, "release/1.9", null, "1111111111 · 20 days ago"),
              { ...vrow(null, "The first 5 branches; there may be more on Gitea."), tone: "muted" },
            ],
          },
        ]);
      if (section === null && BRANCH_VIEWS[id]) return BRANCH_VIEWS[id]!;
      if (section === "diff" && BRANCH_VIEWS[id]) return one("diff", [DIFF], 1);
      if (section === "files") return one("files", [{ type: "tree", trail: [], entries: [{ name: "README.md", kind: "file", size: 9, section: "file:README.md" }], truncated: false, url: null }]);
      if (section === "file:README.md")
        return one("file:README.md", [
          { type: "file", trail: [], path: "README.md", text: "# payout\nold line\n", language: null, size: 18, truncated: false, binary: false, url: null, sha: "blob-1" },
        ]);
      if (section === "commits") return one("commits", [{ type: "entities", title: null, rows: [] }]);
      if (id === C1 && section === null) return { ...one("diff", [DIFF], 1), actions: [NEW_BRANCH] };
      throw { code: "invalid", message: `no section ${section} of ${id}` };
    },
    entityCheckout: async () => ({
      repo_entity_id: REPO,
      repo_url: null,
      path: "/Users/mara/src/payout-service",
      found_by: "scan",
      clones_root: null,
      clone_command: null,
      tree: { status: "read", branch: "feature/PAY-231-sepa-retry", head: null, upstream: null, ahead: 0, behind: 0, changed: 0, untracked: 0, conflicted: 0 },
    }),
    entityCheckoutDiff: async () => {
      throw { code: "not_found", message: "clean" };
    },
    setCheckoutOverride: async () => {
      throw { code: "invalid", message: "no override in this suite" };
    },
    listContexts: async () => [],
    listSources: async () => [{ id: "gitea" }] as never,
    jobReach: async () => [],
    submitWrite: async (payload, linkTo) => {
      writes.push(payload);
      links.push(linkTo ?? null);
      return { id: writes.length } as unknown as QueuedWrite;
    },
    // Each branch tip's author (#986); release/1.9's is not declared.
    codeFacts: async (ids) =>
      (
        [
          [MAIN, "jonas"],
          [FEATURE, "mara"],
          [SPIKE, "mara"],
          [CHORE, "jonas"],
        ] as const
      )
        .filter(([id]) => ids.includes(id))
        .map(([entity_id, author]) => ({ entity_id, author, mine: author === "mara", language: null, teams: [] })),
    ...over,
  };
}

async function draw(rest: string[], job: string | null = null, stand: RepoPorts = ports()) {
  view = mount(TfRepo, { target, props: { ready: true, rest, job, ports: stand, now: NOW } });
  await settle();
}

async function settle() {
  for (let round = 0; round < 5; round += 1) {
    for (let i = 0; i < 10; i += 1) await Promise.resolve();
    await new Promise((resolve) => setTimeout(resolve, 0));
    flushSync();
  }
}

const text = (el: Element | null | undefined) => el?.textContent?.replace(/\s+/g, " ").trim() ?? "";
const button = (label: string, root: ParentNode = document) =>
  [...root.querySelectorAll<HTMLButtonElement>("button, a")].find((b) => text(b) === label) as HTMLButtonElement | undefined;
const dialog = () => document.querySelector(".tf-dialog");
const rowOf = (name: string) =>
  [...target.querySelectorAll(".tf-rbranches tbody tr")].find((tr) => text(tr.querySelector("td")).startsWith(name))!;

function type(el: HTMLInputElement | HTMLTextAreaElement, value: string) {
  el.value = value;
  el.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
}

// ---------------------------------------------------------------- Branches (#839)

test("Branches lists main as default, then each branch with ahead and behind or merged, its PR and when", async () => {
  await draw([REPO, "branches"]);
  const rows = [...target.querySelectorAll(".tf-rbranches tbody tr")].map((tr) => [...tr.querySelectorAll("td")].map(text));
  expect(rows).toEqual([
    ["main default", "", "", "jonas", "5d77e1a2c3 · 1 day ago", ""],
    ["feature/PAY-231-sepa-retry", "3 ahead, 0 behind", "#142 open", "mara", "c90d11e4a7 · 3 hours ago", "Delete…"],
    ["spike/sepa-instant", "2 ahead, 9 behind", "none", "mara", "9a1b2c3d4e · 9 days ago", "Open PR Delete…"],
    ["chore/bump-tokio", "merged", "none", "jonas", "77aa00bb11 · 5 days ago", "Delete…"],
    ["release/1.9", "1 ahead, 0 behind", "none", "", "1111111111 · 20 days ago", "Open PR Delete…"],
  ]);
  expect([...target.querySelectorAll(".tf-rbranches th")].map(text)).toEqual([
    "Branch",
    "Against main",
    "Pull request",
    "By",
    "Updated",
    "",
  ]);
  expect(rowOf("feature/").querySelector("a[href^='#/pr/']")!.getAttribute("href")).toBe("#/pr/gitea%3Atidewater%2Fpayout-service%23142");
  expect(button("Open PR", rowOf("spike/"))!.getAttribute("href")).toBe(
    "#/repo/gitea%3Atidewater%2Fpayout-service/compare/spike%2Fsepa-instant",
  );
  // The source's reason, drawn on the button it disables.
  const locked = button("Delete…", rowOf("release/"))!;
  expect(locked.getAttribute("aria-disabled")).toBe("true");
  expect(locked.title).toBe("Protected branch: lift its protection on Gitea to delete it");
  expect(text(target.querySelector(".tf-rbranches-note"))).toBe("The first 5 branches; there may be more on Gitea.");
  // Each branch's standing is its own view's read.
  // Each branch's whole view, and the repository's for its toolbar (#2030).
  expect(reads.filter(([, section]) => section === null).map(([id]) => id).sort()).toEqual([CHORE, FEATURE, LOCKED, REPO, SPIKE].sort());
});

test("the Branches filter narrows by name, No PR and Merged", async () => {
  await draw([REPO, "branches"]);
  const names = () => [...target.querySelectorAll(".tf-rbranches tbody tr")].map((tr) => text(tr.querySelector("td")).split(" ")[0]);
  button("No PR", target)!.click();
  flushSync();
  expect(names()).toEqual(["spike/sepa-instant", "chore/bump-tokio", "release/1.9"]);
  button("Merged", target)!.click();
  flushSync();
  expect(names()).toEqual(["chore/bump-tokio"]);
});

test("Show: Mine keeps the branches whose tip's declared author is mine (#986)", async () => {
  await draw([REPO, "branches"]);
  const names = () => [...target.querySelectorAll(".tf-rbranches tbody tr")].map((tr) => text(tr.querySelector("td")).split(" ")[0]);
  const mine = button("Mine", target)!;
  expect(mine.closest("[role=group]")?.getAttribute("aria-label")).toBe("Show");
  mine.click();
  flushSync();
  expect(names()).toEqual(["feature/PAY-231-sepa-retry", "spike/sepa-instant"]);
});

test("Delete… on a branch with unmerged commits warns what is lost, the open PR, and that the checkout keeps its copy; it queues DeleteBranch", async () => {
  await draw([REPO, "branches"]);
  button("Delete…", rowOf("feature/"))!.click();
  flushSync();
  expect(text(dialog()!.querySelector("h2"))).toBe("Delete feature/PAY-231-sepa-retry?");
  expect([...dialog()!.querySelectorAll(".tf-body > *")].map(text)).toEqual([
    "3 commits on this branch are not in main and will be lost.",
    "PR #142 is open from this branch. Gitea closes it when the branch goes.",
    "This deletes the branch on Gitea. Your local checkout keeps its copy.",
  ]);
  button("Delete branch", dialog()!)!.click();
  await settle();
  expect(writes).toEqual([{ DeleteBranch: { entity: FEATURE } }]);
  expect(dialog()).toBeNull();
  expect(text(rowOf("feature/").querySelector(".tf-chip.tf-warn"))).toBe("delete queued");
  expect(button("Delete…", rowOf("feature/"))).toBeUndefined();
  expect(toasts.items.at(-1)!.text).toBe("Queued: delete branch feature/PAY-231-sepa-retry.");
});

test("Delete… while the repository's source fails its sign-in says the delete waits there", async () => {
  health.patch({ source_id: "gitea", state: "missing_secret", checked_at: null, detail: null, secret_expires_at: null });
  await draw([REPO, "branches"]);
  button("Delete…", rowOf("feature/"))!.click();
  flushSync();
  button("Delete branch", dialog()!)!.click();
  await settle();
  expect(toasts.items.at(-1)!.text).toBe(
    "Queued: delete branch feature/PAY-231-sepa-retry. Gitea sign-in is failing, so it waits there.",
  );
});

test("Delete… on a merged branch says nothing is lost", async () => {
  await draw([REPO, "branches"]);
  button("Delete…", rowOf("chore/"))!.click();
  flushSync();
  expect([...dialog()!.querySelectorAll(".tf-body > *")].map(text)).toEqual([
    "Every commit on this branch is in main. Nothing is lost.",
    "This deletes the branch on Gitea. Your local checkout keeps its copy.",
  ]);
  button("Cancel", dialog()!)!.click();
  flushSync();
  expect(writes).toEqual([]);
});

test("New branch… is prefilled from the job, offers every branch to start from, and queues CreateBranch in the repository", async () => {
  await draw([REPO, "branches"], "PAY-231");
  button("New branch…", target)!.click();
  flushSync();
  expect(text(dialog()!.querySelector("h2"))).toBe("New branch in tidewater/payout-service");
  const name = dialog()!.querySelector<HTMLInputElement>("input[type=text]")!;
  expect(name.value).toBe("feature/PAY-231-");
  const from = dialog()!.querySelector<HTMLSelectElement>("select")!;
  expect([...from.options].map((o) => o.value)).toEqual([
    "main",
    "feature/PAY-231-sepa-retry",
    "spike/sepa-instant",
    "chore/bump-tokio",
    "release/1.9",
  ]);
  type(name, "feature/PAY-231-backoff");
  button("Create branch", dialog()!)!.click();
  await settle();
  expect(writes).toEqual([{ CreateBranch: { entity: REPO, name: "feature/PAY-231-backoff", from_ref: "main" } }]);
  // Drawn queued at once, until the listing holds it.
  expect(text(rowOf("feature/PAY-231-backoff"))).toContain("queued");
  expect(toasts.items.at(-1)!.text).toBe("Queued: create branch feature/PAY-231-backoff in tidewater/payout-service.");
});

test("New branch… offers Link to <job>, ticked, and asks for the branch to be linked to the job's ticket (#984)", async () => {
  await draw([REPO, "branches"], "PAY-231");
  button("New branch…", target)!.click();
  flushSync();
  const box = dialog()!.querySelector<HTMLInputElement>("input[type=checkbox]")!;
  expect(text(box.closest("label"))).toBe("PAY-231 (in knobas)");
  expect(text(box.closest("label")!.previousElementSibling)).toBe("Link to");
  expect(box.checked).toBe(true);
  button("Create branch", dialog()!)!.click();
  await settle();
  expect(writes).toEqual([{ CreateBranch: { entity: REPO, name: "feature/PAY-231-", from_ref: "main" } }]);
  expect(links).toEqual(["gitea:PAY-231"]);
});

test("New branch… unticked links nothing, and with no job offers no link", async () => {
  await draw([REPO, "branches"], "PAY-231");
  button("New branch…", target)!.click();
  flushSync();
  dialog()!.querySelector<HTMLInputElement>("input[type=checkbox]")!.click();
  flushSync();
  type(dialog()!.querySelector<HTMLInputElement>("input[type=text]")!, "feature/PAY-231-unlinked");
  button("Create branch", dialog()!)!.click();
  await settle();
  expect(links).toEqual([null]);
  unmount(view!);
  view = null;

  await draw([REPO, "branches"]);
  button("New branch…", target)!.click();
  flushSync();
  expect(dialog()!.querySelector("input[type=checkbox]")).toBeNull();
  expect(text(dialog())).not.toContain("Link to");
});

test("New branch… with a job the mirror does not hold queues nothing and says why", async () => {
  await draw([REPO, "branches"], "PAY-999");
  button("New branch…", target)!.click();
  flushSync();
  button("Create branch", dialog()!)!.click();
  await settle();
  expect(writes).toEqual([]);
  expect(toasts.items.at(-1)!.text).toBe("PAY-999 is not in the mirror, so the branch cannot be linked to it");
  expect(dialog()).not.toBeNull();
});

test("New branch… with no name sends nothing", async () => {
  await draw([REPO, "branches"]);
  button("New branch…", target)!.click();
  flushSync();
  type(dialog()!.querySelector<HTMLInputElement>("input[type=text]")!, "  ");
  expect(button("Create branch", dialog()!)!.disabled).toBe(true);
});

test("New branch from here… starts the branch at the commit's whole sha", async () => {
  await draw([REPO, "commits", C1]);
  button("New branch from here…", target)!.click();
  flushSync();
  const from = dialog()!.querySelector<HTMLSelectElement>("select")!;
  expect(from.value).toBe("c90d11e4a7b2");
  expect(text(from.options[0])).toBe("c90d11e4a7");
  type(dialog()!.querySelector<HTMLInputElement>("input[type=text]")!, "feature/from-here");
  button("Create branch", dialog()!)!.click();
  await settle();
  expect(writes).toEqual([{ CreateBranch: { entity: REPO, name: "feature/from-here", from_ref: "c90d11e4a7b2" } }]);
});

/** What a token that may only read is told, in its source's words (#2030). */
const READ_ONLY = "This token cannot write: it lacks the write:repository scope";

/** `ports()`, the repository's whole view offering `actions` as its toolbar. */
const repoToolbar = (actions: ViewAction[], over: Partial<RepoPorts> = {}): RepoPorts => {
  const base = ports(over);
  return {
    ...base,
    entityView: async (id, section) =>
      id === REPO && section === null ? { sections: [], actions } : base.entityView(id, section),
  };
};

test("New branch… in the header is the repository's toolbar's: disabled with the source's reason, it opens nothing (#2030)", async () => {
  await draw([REPO, "branches"], null, repoToolbar([action("create_branch", READ_ONLY), action("create_pull_request", READ_ONLY)]));
  const header = target.querySelector(".tf-fhead")!;
  const newBranch = button("New branch…", header)!;
  expect(newBranch.getAttribute("aria-disabled")).toBe("true");
  expect(newBranch.getAttribute("title")).toBe(READ_ONLY);
  newBranch.click();
  flushSync();
  expect(dialog()).toBeNull();
  expect(writes).toEqual([]);
});

test("New branch… in the header is enabled where the repository's toolbar offers it, or says nothing (#2030)", async () => {
  await draw([REPO, "branches"], null, repoToolbar([action("create_branch")]));
  const newBranch = button("New branch…", target.querySelector(".tf-fhead")!)!;
  expect(newBranch.getAttribute("aria-disabled")).toBeNull();
  newBranch.click();
  flushSync();
  expect(dialog()).not.toBeNull();
});

// ---------------------------------------------------------------- Compare (#840)

const COMPARE = (branch: string) => [REPO, "compare", ...branch.split("/")];

test("the compare page offers Open PR #N where a pull request is already open from the branch", async () => {
  await draw(COMPARE("feature/PAY-231-sepa-retry"), "PAY-231");
  expect(text(target.querySelector(".tf-tabs a[aria-current='page']"))).toContain("Pull requests");
  expect(text(target.querySelector(".tf-compare .tf-chip.tf-info"))).toBe("PR #142 is already open from this branch");
  expect(button("Open PR #142", target)!.getAttribute("href")).toBe("#/pr/gitea%3Atidewater%2Fpayout-service%23142");
  expect(button("Create pull request", target)).toBeUndefined();
});

test("the compare page proposes Into and From, the change and its commits, and queues CreatePullRequest", async () => {
  await draw(COMPARE("spike/sepa-instant"));
  const [into, from] = [...target.querySelectorAll<HTMLSelectElement>(".tf-compare select")];
  expect(into!.value).toBe("main");
  expect([...into!.options].map((o) => o.value)).toEqual(["main", "feature/PAY-231-sepa-retry", "spike/sepa-instant", "chore/bump-tokio", "release/1.9"]);
  expect(from!.value).toBe("spike/sepa-instant");
  expect([...from!.options].map((o) => o.value)).not.toContain("main");
  const title = target.querySelector<HTMLInputElement>(".tf-compare input[type=text]")!;
  expect(title.value).toBe("sepa-instant");
  expect(target.querySelector<HTMLTextAreaElement>(".tf-compare textarea")!.value).toBe("");
  expect(text(target.querySelector(".tf-compare .tf-cfiles"))).toBe("1 file changed against main");
  expect([...target.querySelectorAll(".tf-compare .tf-diff .tf-ln")].map(text)).toEqual(["1− 3", "1+ 5"]);
  expect([...target.querySelectorAll(".tf-compare .tf-side .tf-r")].map((r) => [...r.children].map(text))).toEqual([["c90d11e4a7", "PAY-231: jitter in backoff"]]);

  type(title, "Instant SEPA spike");
  button("Create pull request", target)!.click();
  await settle();
  expect(writes).toEqual([
    { CreatePullRequest: { entity: REPO, title: "Instant SEPA spike", body: "", head: "spike/sepa-instant", base: "main" } },
  ]);
  expect(location.hash).toBe("#/repo/gitea%3Atidewater%2Fpayout-service/prs");
  expect(toasts.items.at(-1)!.text).toBe("Queued: open PR Instant SEPA spike.");
});

test("Create pull request is disabled with the source's reason where the branch's view offers it disabled, and queues nothing (#2030)", async () => {
  const blocked = branchView({ ahead: 2, behind: 9, actions: [action("create_pull_request", READ_ONLY), action("create_branch", READ_ONLY)] });
  const base = ports();
  await draw(COMPARE("spike/sepa-instant"), null, {
    ...base,
    entityView: async (id, section) => (id === SPIKE && section === null ? blocked : base.entityView(id, section)),
  });
  const create = button("Create pull request", target)!;
  expect(create.getAttribute("aria-disabled")).toBe("true");
  expect(create.getAttribute("title")).toBe(READ_ONLY);
  type(target.querySelector<HTMLInputElement>(".tf-compare input[type=text]")!, "Instant SEPA spike");
  create.click();
  await settle();
  expect(writes).toEqual([]);
});

test("From changes the address; a branch holding a key proposes its ticket's title and description", async () => {
  await draw(COMPARE("spike/sepa-instant"), "PAY-231");
  const from = target.querySelectorAll<HTMLSelectElement>(".tf-compare select")[1]!;
  from.value = "chore/bump-tokio";
  from.dispatchEvent(new Event("change", { bubbles: true }));
  expect(location.hash).toBe("#/repo/gitea%3Atidewater%2Fpayout-service/compare/chore%2Fbump-tokio");
  unmount(view!);
  view = null;

  // A branch the listing does not hold yet: queued by this session's edit.
  await draw(COMPARE("fix/PAY-231-README"), "PAY-231");
  expect(target.querySelector<HTMLInputElement>(".tf-compare input[type=text]")!.value).toBe("PAY-231: Retry failed SEPA payouts");
  expect(target.querySelector<HTMLTextAreaElement>(".tf-compare textarea")!.value).toBe(
    "The nightly batch leaves 14 payouts unsettled.\n\nCloses PAY-231.",
  );
  expect(text(target.querySelector(".tf-compare .tf-empty"))).toBe(
    "This branch is not on Gitea yet, or knobas has not synced it. What it changes shows here once it has.",
  );
});

test("a branch holding a key offers Link the PR to <key> in knobas, ticked; it asks for the pull request to be linked (#984)", async () => {
  await draw(COMPARE("fix/PAY-231-README"), "PAY-231");
  const box = target.querySelector<HTMLInputElement>(".tf-compare input[type=checkbox]")!;
  expect(text(box.closest("label"))).toBe("Link the PR to PAY-231 in knobas");
  expect(box.checked).toBe(true);
  button("Create pull request", target)!.click();
  await settle();
  expect(writes).toHaveLength(1);
  expect(links).toEqual(["gitea:PAY-231"]);
  unmount(view!);
  view = null;

  // Unticked, the pull request is not linked.
  await draw(COMPARE("fix/PAY-231-README"), "PAY-231");
  target.querySelector<HTMLInputElement>(".tf-compare input[type=checkbox]")!.click();
  flushSync();
  button("Create pull request", target)!.click();
  await settle();
  expect(links).toEqual(["gitea:PAY-231", null]);
});

test("a branch with no key in the mirror offers no link", async () => {
  await draw(COMPARE("spike/sepa-instant"), "PAY-231");
  expect(target.querySelector(".tf-compare input[type=checkbox]")).toBeNull();
  button("Create pull request", target)!.click();
  await settle();
  expect(links).toEqual([null]);
});

// ---------------------------------------------------------------- Edit (#845)

const editor = () => target.querySelector<HTMLTextAreaElement>(".tf-editor textarea")!;

test("Edit turns the file into an editor that says the commit goes to Gitea and the checkout is never written", async () => {
  await draw([REPO, "files", "README.md"]);
  expect(button("Edit", target)!.getAttribute("href")).toBe("#/repo/gitea%3Atidewater%2Fpayout-service/edit/README.md");
  unmount(view!);
  view = null;

  await draw([REPO, "edit", "README.md"]);
  expect(text(target.querySelector(".tf-tabs a[aria-current='page']"))).toBe("Files");
  expect(text(target.querySelector(".tf-fileview header"))).toBe("README.md editing on main Cancel Commit change…");
  expect(button("Cancel", target)!.getAttribute("href")).toBe("#/repo/gitea%3Atidewater%2Fpayout-service/files/README.md");
  expect(text(target.querySelector(".tf-fileview .tf-banner.tf-info"))).toBe(
    "The edit is committed to main on Gitea. Your local checkout is never written to.",
  );
  expect(editor().value).toBe("# payout\nold line\n");
  expect(editor().getAttribute("aria-label")).toBe("Edit README.md");
  expect(text(target.querySelector(".tf-editor .tf-nums"))).toBe("1 2 3");
});

test("Commit change… with nothing changed says so and opens nothing", async () => {
  await draw([REPO, "edit", "README.md"]);
  button("Commit change…", target)!.click();
  flushSync();
  expect(dialog()).toBeNull();
  expect(toasts.items.at(-1)!.text).toBe("Nothing changed yet.");
});

test("the commit dialog shows the diff and a message with the job key, and queues CommitFile onto the branch against the blob read", async () => {
  await draw([REPO, "edit", "README.md"], "PAY-231");
  type(editor(), "# payout\nnew line\n");
  button("Commit change…", target)!.click();
  flushSync();
  expect(text(dialog()!.querySelector("h2"))).toBe("Commit README.md");
  expect([...dialog()!.querySelectorAll(".tf-diff .tf-ln")].map(text)).toEqual(["1 # payout", "2− old line", "2+ new line", "3"]);
  const message = dialog()!.querySelector<HTMLInputElement>("#tf-cf-msg")!;
  expect(message.value).toBe("PAY-231: update README.md");
  const radios = [...dialog()!.querySelectorAll<HTMLInputElement>("input[type=radio]")];
  expect(radios.map((r) => [text(r.closest("label")), r.checked])).toEqual([
    ["Commit to main", true],
    ["Commit to a new branch and open a pull request", false],
  ]);
  expect(dialog()!.querySelector<HTMLInputElement>("#tf-cf-branch")!.value).toBe("fix/PAY-231-README");
  // The checkout is on another branch: no warning.
  expect(dialog()!.querySelector(".tf-banner.tf-warn")).toBeNull();

  button("Commit change", dialog()!)!.click();
  await settle();
  expect(writes).toEqual([
    {
      CommitFile: {
        repo: REPO,
        branch: "main",
        new_branch: null,
        path: "README.md",
        content: "# payout\nnew line\n",
        message: "PAY-231: update README.md",
        base_sha: "blob-1",
      },
    },
  ]);
  expect(location.hash).toBe("#/repo/gitea%3Atidewater%2Fpayout-service/files/README.md");
  expect(toasts.items.at(-1)!.text).toBe("Queued: commit README.md to main.");
  unmount(view!);
  view = null;

  // The file says its commit is queued while Gitea still holds the blob it was made against.
  await draw([REPO, "files", "README.md"]);
  expect(text(target.querySelector(".tf-fileview .tf-banner.tf-warn"))).toBe(
    "A commit of this file is queued: “PAY-231: update README.md”. It shows here once Gitea has it.",
  );
});

test("a commit to a new branch names it and goes on to the compare page", async () => {
  await draw([REPO, "edit", "README.md"], "PAY-231");
  type(editor(), "# payout\n");
  button("Commit change…", target)!.click();
  flushSync();
  const radios = [...dialog()!.querySelectorAll<HTMLInputElement>("input[type=radio]")];
  radios[1]!.click();
  flushSync();
  type(dialog()!.querySelector<HTMLInputElement>("#tf-cf-branch")!, "fix/PAY-231-readme-line");
  button("Commit change", dialog()!)!.click();
  await settle();
  expect(writes).toEqual([
    {
      CommitFile: {
        repo: REPO,
        branch: "main",
        new_branch: "fix/PAY-231-readme-line",
        path: "README.md",
        content: "# payout\n",
        message: "PAY-231: update README.md",
        base_sha: "blob-1",
      },
    },
  ]);
  expect(location.hash).toBe("#/repo/gitea%3Atidewater%2Fpayout-service/compare/fix%2FPAY-231-readme-line");
  expect(toasts.items.at(-1)!.text).toBe("Queued: commit README.md to fix/PAY-231-readme-line, then open a PR.");
});

test("the commit dialog warns when the checkout is on the branch the commit goes to", async () => {
  await draw([REPO, "edit", "README.md"]);
  const select = target.querySelector<HTMLSelectElement>(".tf-fhead select")!;
  select.value = "feature/PAY-231-sepa-retry";
  select.dispatchEvent(new Event("change", { bubbles: true }));
  await settle();
  type(editor(), "changed\n");
  button("Commit change…", target)!.click();
  flushSync();
  expect(text(dialog()!.querySelector(".tf-banner.tf-warn"))).toBe(
    "Your checkout is on this branch. After this commit it is 1 behind; pull in your terminal before you push.",
  );
  expect(text(dialog()!.querySelector("input[type=radio]")!.closest("label"))).toBe("Commit to feature/PAY-231-sepa-retry");
});

test("a file with no blob sha to commit against cannot be edited, and says why", async () => {
  const stand = ports();
  const read = stand.entityView;
  stand.entityView = async (id, section) => {
    const answer = await read(id, section);
    if (section === "file:README.md") (answer!.sections[0]!.blocks[0] as { sha: string | null }).sha = null;
    return answer;
  };
  await draw([REPO, "files", "README.md"], null, stand);
  const edit = button("Edit", target)!;
  expect(edit.getAttribute("aria-disabled")).toBe("true");
  expect(edit.title).toBe("The source did not say which version of the file this is, so a commit could not be checked against it");
});

const PROTECTED =
  "main is a protected branch that this account may not push to: commit to a new branch and open a pull request instead";

/** The file section declaring its own `commit_file`, disabled with `disabled` where given (#985). */
function declaring(disabled: string | null): RepoPorts {
  const stand = ports();
  const read = stand.entityView;
  stand.entityView = async (id, section) => {
    const answer = await read(id, section);
    if (section === "file:README.md") answer!.sections[0]!.blocks.push({ type: "actions", actions: [action("commit_file", disabled)] });
    return answer;
  };
  return stand;
}

test("on a protected branch the commit dialog opens on a new branch, Commit to <branch> disabled with the source's reason (#985)", async () => {
  await draw([REPO, "edit", "README.md"], "PAY-231", declaring(PROTECTED));
  type(editor(), "# payout\nnew line\n");
  button("Commit change…", target)!.click();
  flushSync();
  const [same, fresh] = [...dialog()!.querySelectorAll<HTMLInputElement>("input[type=radio]")];
  expect(same!.disabled).toBe(true);
  expect(same!.checked).toBe(false);
  expect(fresh!.checked).toBe(true);
  expect(text(same!.closest("label")!.querySelector("small"))).toBe(PROTECTED);
  expect(text(document.getElementById(same!.getAttribute("aria-describedby")!))).toBe(PROTECTED);

  button("Commit change", dialog()!)!.click();
  await settle();
  expect(writes).toEqual([
    {
      CommitFile: {
        repo: REPO,
        branch: "main",
        new_branch: "fix/PAY-231-README",
        path: "README.md",
        content: "# payout\nnew line\n",
        message: "PAY-231: update README.md",
        base_sha: "blob-1",
      },
    },
  ]);
});

test("a commit the source declares taken opens on Commit to <branch>, enabled (#985)", async () => {
  await draw([REPO, "edit", "README.md"], null, declaring(null));
  type(editor(), "changed\n");
  button("Commit change…", target)!.click();
  flushSync();
  const [same] = [...dialog()!.querySelectorAll<HTMLInputElement>("input[type=radio]")];
  expect([same!.disabled, same!.checked, same!.hasAttribute("aria-describedby")]).toEqual([false, true, false]);
});

// The repository page's sentences name the source that fills the code-host role (ADR-0029), never Gitea.
describe("a repository whose source is not Gitea", () => {
  const forgejo = (): RepoPorts => {
    const base = ports();
    return {
      ...base,
      getEntity: async (id) => {
        const detail = await base.getEntity(id);
        return id === REPO ? ({ ...detail, source: { id: "forgejo", display_name: "Forgejo" } } as EntityDetail) : detail;
      },
    };
  };
  const seen = () => text(target);

  test("Branches' delete dialog names it", async () => {
    await draw([REPO, "branches"], null, forgejo());
    button("Delete…", rowOf("feature/"))!.click();
    flushSync();
    const body = text(dialog()!);
    expect(body).toContain("Forgejo closes it when the branch goes.");
    expect(body).toContain("This deletes the branch on Forgejo.");
    expect(body).not.toContain("Gitea");
  });

  test("Compare's reviewers line and its not-yet-synced line name it", async () => {
    await draw(COMPARE("spike/sepa-instant"), "PAY-231", forgejo());
    expect(seen()).toContain("Reviewers are asked on Forgejo after it opens.");
    expect(seen()).not.toContain("Gitea");
    unmount(view!);
    view = null;
    await draw(COMPARE("fix/PAY-231-README"), "PAY-231", forgejo());
    expect(text(target.querySelector(".tf-compare .tf-empty"))).toBe(
      "This branch is not on Forgejo yet, or knobas has not synced it. What it changes shows here once it has.",
    );
  });

  test("Files' edit banner and queued-commit line name it", async () => {
    await draw([REPO, "edit", "README.md"], "PAY-231", forgejo());
    expect(text(target.querySelector(".tf-fileview .tf-banner.tf-info"))).toBe(
      "The edit is committed to main on Forgejo. Your local checkout is never written to.",
    );
    type(editor(), "# payout\nnew line\n");
    button("Commit change…", target)!.click();
    flushSync();
    button("Commit change", dialog()!)!.click();
    await settle();
    unmount(view!);
    view = null;
    await draw([REPO, "files", "README.md"], null, forgejo());
    expect(text(target.querySelector(".tf-fileview .tf-banner.tf-warn"))).toBe(
      "A commit of this file is queued: “PAY-231: update README.md”. It shows here once Forgejo has it.",
    );
  });

  test("a source that is not yet known reads as the code host", () => {
    expect([sourceName(null), sourceName({ id: "forgejo", display_name: "" })]).toEqual(["the code host", "forgejo"]);
  });
});

// -- the echo's one lifecycle: settled on the listing's re-read (#2105) ------------

/** Ports whose branch listing and README read are `listing()` and `readme()` as each read is asked, and whose events the test raises. */
function rereading(listing: () => ViewRow[] | null, readme: () => string | null) {
  const events = echoEvents(settle);
  const base = ports();
  const stand = ports({
    listen: events.listen,
    entityView: async (id, section) => {
      const read = await base.entityView(id, section);
      if (id === REPO && section === "branches" && listing() !== null) return one("branches", [{ type: "entities", title: null, rows: listing()! }]);
      if (section === "file:README.md" && readme() !== null) {
        const block = read!.sections[0]!.blocks[0] as Extract<ViewBlock, { type: "file" }>;
        return one("file:README.md", [{ ...block, sha: readme() }]);
      }
      return read;
    },
  });
  return { events, stand };
}

const branchNames = () => [...target.querySelectorAll(".tf-rbranches tbody tr")].map((tr) => text(tr.querySelector("td")));

test("a branch made goes once the listing's re-read the refresh event asks holds it, the tab not opened again (#2105)", async () => {
  echoWrite(REPO, MADE, "fix/PAY-231-readme", { write: { id: 5 }, many: true, surface: REPO_BRANCHES, source: "gitea" });
  let listing: ViewRow[] | null = null;
  const { events, stand } = rereading(() => listing, () => null);
  await draw([REPO, "branches"], null, stand);
  expect(branchNames().at(-1)).toBe("fix/PAY-231-readme queued");
  listing = [vrow(MAIN, "main", "default", "5d77e1a2c3 · 1 day ago"), vrow(`${REPO}@refs/heads/fix/PAY-231-readme`, "fix/PAY-231-readme", null, "aa · now")];
  await events.refreshed(`${REPO}@refs/heads/fix/PAY-231-readme`, 5);
  expect(branchNames()).toEqual(["main default", "fix/PAY-231-readme"]);
});

test("a branch deleted goes once the listing's re-read the refresh event asks no longer holds it (#2105)", async () => {
  let listing: ViewRow[] | null = null;
  const { events, stand } = rereading(() => listing, () => null);
  await draw([REPO, "branches"], null, stand);
  button("Delete…", rowOf("chore/"))!.click();
  flushSync();
  button("Delete branch", dialog()!)!.click();
  await settle();
  expect(text(rowOf("chore/").querySelector(".tf-chip.tf-warn"))).toBe("delete queued");
  // The refresh says it landed before the listing is read again.
  listing = [vrow(MAIN, "main", "default", "5d77e1a2c3 · 1 day ago"), vrow(FEATURE, "feature/PAY-231-sepa-retry", null, "c90d11e4a7 · 3 hours ago"), vrow(CHORE, "chore/bump-tokio", null, "77aa00bb11 · 5 days ago")];
  await events.refreshed(CHORE, 1);
  expect(text(rowOf("chore/").querySelector(".tf-chip.tf-warn"))).toBe("delete sent");
  listing = [vrow(MAIN, "main", "default", "5d77e1a2c3 · 1 day ago"), vrow(FEATURE, "feature/PAY-231-sepa-retry", null, "c90d11e4a7 · 3 hours ago")];
  await events.ended("gitea");
  expect(branchNames()).toEqual(["main default", "feature/PAY-231-sepa-retry"]);
  // Gone, not only out of sight: a branch made again under the name draws no old delete.
  expect(echoesOn(REPO, DELETED)).toEqual([]);
});

test("a file commit goes once the file's re-read the refresh event asks has a newer blob, the file not opened again (#2105)", async () => {
  echoWrite(REPO, commitKind("main", "README.md"), "PAY-231: update README.md", { write: { id: 7 }, base: "blob-1", surface: REPO_FILES, source: "gitea" });
  let readme: string | null = null;
  const { events, stand } = rereading(() => null, () => readme);
  await draw([REPO, "files", "README.md"], null, stand);
  const banner = () => text(target.querySelector(".tf-fileview .tf-banner.tf-warn"));
  expect(banner()).toBe("A commit of this file is queued: “PAY-231: update README.md”. It shows here once Gitea has it.");
  await events.refreshed(REPO, 7);
  expect(banner()).toBe("A commit of this file was sent: “PAY-231: update README.md”. It shows here once Gitea has it.");
  readme = "blob-2";
  await events.ended("gitea");
  expect(banner()).toBe("");
});
