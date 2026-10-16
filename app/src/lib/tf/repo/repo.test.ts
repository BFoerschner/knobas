/**
 * The repository page's pure half (#838): what an address names, which
 * branches the dropdown offers, which paths the checkout changed, and what the
 * header and the lists say.
 */
import { expect, test } from "vitest";

import type { DiffFile, DiffRead, EntityRow, SourceView, ViewAction, ViewSection } from "../../ipc/entity";
import {
  ancestors,
  branchOptions,
  branchStanding,
  type BranchRow,
  commitPrefill,
  compareHash,
  editHash,
  branchFrom,
  keyIn,
  newBranchName,
  proposal,
  visibleBranches,
  changedPaths,
  checkoutSentence,
  commitHash,
  fileHash,
  fileLines,
  pickDefaultFile,
  readRepoRoute,
  repoHash,
  repoPrs,
  shaOf,
  visibleRepoPrs,
  visibleViewRows,
} from "./repo";

const REPO = "gitea:tidewater/payout-service";

test("an address names the repository, its tab and what the tab has selected", () => {
  expect(readRepoRoute([REPO])).toEqual({ repoId: REPO, tab: "files", path: null, commit: null, branch: null });
  expect(readRepoRoute([REPO, "files", "src", "sepa", "retry.rs"])).toMatchObject({ tab: "files", path: "src/sepa/retry.rs" });
  expect(readRepoRoute([REPO, "commits"])).toMatchObject({ tab: "commits", commit: null });
  expect(readRepoRoute([REPO, "commits", `${REPO}@c90d11`])).toMatchObject({ tab: "commits", commit: `${REPO}@c90d11` });
  expect(readRepoRoute([REPO, "compare", "feature", "x"])).toMatchObject({ tab: "compare", branch: "feature/x" });
  expect(readRepoRoute([REPO, "prs"]).tab).toBe("prs");
  expect(readRepoRoute([REPO, "edit", "README.md"])).toMatchObject({ tab: "edit", path: "README.md" });
  // A tab this page has no place for reads as Files rather than a blank page.
  expect(readRepoRoute([REPO, "wiki"]).tab).toBe("files");
  expect(readRepoRoute([]).repoId).toBeNull();
});

test("addresses encode the entity id as one segment and a path segment by segment", () => {
  expect(repoHash(REPO)).toBe("#/repo/gitea%3Atidewater%2Fpayout-service");
  expect(repoHash(REPO, "commits")).toBe("#/repo/gitea%3Atidewater%2Fpayout-service/commits");
  expect(fileHash(REPO, "src/a b.rs")).toBe("#/repo/gitea%3Atidewater%2Fpayout-service/files/src/a%20b.rs");
  expect(commitHash(REPO, `${REPO}@c90d11`)).toBe(
    "#/repo/gitea%3Atidewater%2Fpayout-service/commits/gitea%3Atidewater%2Fpayout-service%40c90d11",
  );
});

function section(rows: { entity_id: string | null; title: string; chip?: string | null; tone?: "muted" | null }[]): ViewSection {
  return {
    id: "branches",
    title: "Branches",
    count: rows.length,
    deferred: false,
    blocks: [
      {
        type: "entities",
        title: null,
        rows: rows.map((r) => ({ entity_id: r.entity_id, title: r.title, meta: null, tone: r.tone ?? null, chip: r.chip ?? null, url: null })),
      },
    ],
  };
}

test("the dropdown offers the default branch first and every branch the mirror holds", () => {
  const options = branchOptions(
    section([
      { entity_id: `${REPO}@refs/heads/main`, title: "main", chip: "default", tone: "muted" },
      { entity_id: `${REPO}@refs/heads/feature/x`, title: "feature/x" },
      // Not in the mirror: its id was cleared, so nothing can be read at it.
      { entity_id: null, title: "spike/y" },
      // The note a list that stopped paging ends on.
      { entity_id: null, title: "More branches on Gitea", tone: "muted" },
    ]),
  );
  expect(options).toEqual([
    { name: "main", entityId: `${REPO}@refs/heads/main`, isDefault: true },
    { name: "feature/x", entityId: `${REPO}@refs/heads/feature/x`, isDefault: false },
  ]);
  expect(branchOptions(null)).toEqual([]);
  // A default branch the mirror does not hold is still offered: it is read at the repository.
  expect(branchOptions(section([{ entity_id: null, title: "main", chip: "default" }]))).toEqual([
    { name: "main", entityId: null, isDefault: true },
  ]);
});

const file = (path: string, old_path: string | null = null): DiffFile => ({
  path,
  old_path,
  change: "modified",
  additions: 1,
  deletions: 1,
  hunks: [],
  truncated: false,
  threads: [],
});

test("the checkout's changed paths are the unstaged, the staged and the untracked files", () => {
  const read: DiffRead = {
    status: "read",
    unstaged: { files: [file("src/sepa/retry.rs")], truncated: false },
    staged: { files: [file("src/new.rs", "src/old.rs")], truncated: false },
    untracked: ["notes.txt", "scratch/"],
    untracked_truncated: false,
  };
  expect([...changedPaths(read)].sort()).toEqual(["notes.txt", "src/new.rs", "src/sepa/retry.rs"]);
  expect(changedPaths({ status: "no_git" }).size).toBe(0);
});

test("a file's lines drop the empty one after a final newline", () => {
  expect(fileLines("a\nb\n")).toEqual(["a", "b"]);
  expect(fileLines("a\nb")).toEqual(["a", "b"]);
  expect(fileLines("")).toEqual([""]);
});

test("a path's directories, outermost first", () => {
  expect(ancestors("src/sepa/retry.rs")).toEqual(["src", "src/sepa"]);
  expect(ancestors("README.md")).toEqual([]);
});

test("with no file in the address the top directory's README opens, a Markdown one first", () => {
  const entry = (name: string, kind: "file" | "dir" = "file") => ({ name, kind, size: 1, section: `file:${name}` });
  expect(pickDefaultFile([entry("src", "dir"), entry("readme.txt"), entry("README.md")])).toBe("README.md");
  expect(pickDefaultFile([entry("README")])).toBe("README");
  expect(pickDefaultFile([entry("Cargo.toml")])).toBeNull();
});

test("the header says where the checkout is and how it stands, or that there is none", () => {
  expect(checkoutSentence(undefined)).toBeNull();
  expect(
    checkoutSentence({
      repo_entity_id: REPO,
      repo_url: null,
      path: "/Users/mara/src/payout-service",
      found_by: "scan",
      clones_root: null,
      clone_command: null,
      tree: { status: "read", branch: "main", head: null, upstream: null, ahead: 1, behind: 0, changed: 1, untracked: 0, conflicted: 0 },
    }),
  ).toEqual({ path: "/Users/mara/src/payout-service", state: "main, 1 ahead, 1 changed" });
  expect(
    checkoutSentence({
      repo_entity_id: REPO,
      repo_url: null,
      path: null,
      found_by: "nothing",
      clones_root: null,
      clone_command: null,
      tree: null,
    }),
  ).toEqual({ path: null, state: null });
});

test("a commit's sha is its id's tail after @, cut to ten characters", () => {
  expect(shaOf(`${REPO}@c90d11a2b3c4d5e6`)).toBe("c90d11a2b3");
  expect(shaOf("mock:9f2c1ab")).toBeNull();
});

const pr = (id: string, title: string, path: string, state: string, source_id = "gitea"): EntityRow => ({
  entity_id: id,
  kind: "pr",
  source_id,
  title,
  updated_at: null,
  synced_at: "2026-08-22T14:30:00Z",
  path,
  state: { text: state, tone: "info" },
});

test("the repository's pull requests are those whose parent is it, in its source", () => {
  const repo = { source_id: "gitea", title: "tidewater/payout-service" };
  const rows = [
    pr(`${REPO}#142`, "Retry", "tidewater/payout-service", "open"),
    pr("gitea:tidewater/ledger#9", "Ledger", "tidewater/ledger", "open"),
    pr("other:tidewater/payout-service#1", "Elsewhere", "tidewater/payout-service", "open", "other"),
    pr(`${REPO}#139`, "Old fix", "tidewater/payout-service", "merged"),
  ];
  const mine = repoPrs(rows, repo);
  expect(mine.map((r) => r.title)).toEqual(["Retry", "Old fix"]);
  expect(visibleRepoPrs(mine, ["Merged"], "").map((r) => r.title)).toEqual(["Old fix"]);
  expect(visibleRepoPrs(mine, [], "#142").map((r) => r.title)).toEqual(["Retry"]);
});

test("a view's rows filter on every word typed across chip, title and meta", () => {
  const rows = [
    { entity_id: null, title: "PAY-231: jitter", meta: "mara · 2 hours ago", tone: null, chip: "c90d11", url: null },
    { entity_id: null, title: "Bump deps", meta: "jonas · 3 days ago", tone: null, chip: "a41f2c", url: null },
  ];
  expect(visibleViewRows(rows, "jonas").map((r) => r.chip)).toEqual(["a41f2c"]);
  expect(visibleViewRows(rows, "c90 jitter").map((r) => r.chip)).toEqual(["c90d11"]);
  expect(visibleViewRows(rows, "")).toHaveLength(2);
});

// ---- Branches, compare and edit (#839, #840, #845) ----

const BRANCH = `${REPO}@refs/heads/feature/PAY-231-sepa-retry`;

const action = (write_op: string, disabled: string | null = null): ViewAction => ({
  write_op,
  label: write_op,
  fields: [],
  confirm: null,
  primary: false,
  danger: false,
  disabled,
  with_settings: false,
  values: {},
  entity: null,
  comments: null,
} as unknown as ViewAction);

/** A branch's whole view as `knobas-source-gitea`'s `view/branch.rs` draws it. */
function branchView(over: { pr?: boolean; ahead?: string; behind?: string; actions?: ViewAction[] } = {}): SourceView {
  const facts = [
    { label: "Base", value: "main", tone: null },
    { label: "Ahead", value: over.ahead ?? "3", tone: null },
    { label: "Behind", value: over.behind ?? "1", tone: null },
    ...(over.pr ? [] : [{ label: "Pull request", value: "none", tone: "muted" as const }]),
  ];
  return {
    sections: [
      {
        id: "overview",
        title: "Overview",
        count: null,
        deferred: false,
        blocks: [
          ...(over.pr
            ? [
                {
                  type: "entities" as const,
                  title: "Pull request",
                  rows: [{ entity_id: `${REPO}#142`, title: "SEPA retry", meta: "#142 · feature → main · mara", tone: "info" as const, chip: "open", url: null }],
                },
              ]
            : []),
          { type: "facts" as const, facts },
          {
            type: "entities" as const,
            title: "Last commit",
            rows: [{ entity_id: `${REPO}@c90d11e4`, title: "jitter", meta: "mara · 2 hours ago", tone: null, chip: "c90d11e4", url: null }],
          },
        ],
      },
      {
        id: "commits",
        title: "Commits",
        count: 1,
        deferred: false,
        blocks: [
          {
            type: "entities",
            title: null,
            rows: [{ entity_id: `${REPO}@c90d11e4`, title: "jitter", meta: "mara · 2 hours ago", tone: null, chip: "c90d11e4", url: null }],
          },
        ],
      },
      { id: "diff", title: "Diff", count: 2, deferred: true, blocks: [] },
    ],
    actions: over.actions ?? [action("create_pull_request"), action("create_branch"), action("delete_branch")],
  };
}

test("a branch's standing is read off its view: ahead and behind, its open pull request, its commits and what it offers", () => {
  const standing = branchStanding(branchView({ pr: true, actions: [action("create_branch"), action("delete_branch")] }));
  expect(standing).toMatchObject({ ahead: 3, behind: 1, merged: false, canOpenPr: false, deletable: true, deleteBlocked: null, files: 2 });
  expect(standing.pr).toEqual({ entityId: `${REPO}#142`, number: "#142", state: "open", tone: "info" });
  expect(standing.commits.map((row) => row.title)).toEqual(["jitter"]);

  const none = branchStanding(branchView({ ahead: "0", behind: "4" }));
  expect(none).toMatchObject({ ahead: 0, behind: 4, merged: true, pr: null, canOpenPr: true, deletable: true });

  // Disabled by the source: no PR offered; a protected branch says why it stays.
  const held = branchStanding(
    branchView({ actions: [action("create_pull_request", "No commits ahead of main"), action("delete_branch", "Protected branch: lift its protection on Gitea to delete it")] }),
  );
  expect(held).toMatchObject({ canOpenPr: false, deletable: true, deleteBlocked: "Protected branch: lift its protection on Gitea to delete it" });

  // The default branch: no ahead, nothing to propose or delete.
  const main = branchStanding({
    sections: [{ id: "overview", title: "Overview", count: null, deferred: false, blocks: [{ type: "facts", facts: [{ label: "Branch", value: "default branch", tone: null }] }] }],
    actions: [action("create_branch")],
  });
  expect(main).toMatchObject({ ahead: null, behind: null, merged: false, pr: null, canOpenPr: false, deletable: false, files: null });
});

test("the Branches filter keeps by name, by No PR and by Merged, the default branch only by name", () => {
  const rows: BranchRow[] = [
    { name: "main", entityId: `${REPO}@refs/heads/main`, isDefault: true, meta: null, standing: null },
    { name: "feature/PAY-231-sepa-retry", entityId: BRANCH, isDefault: false, meta: null, standing: branchStanding(branchView({ pr: true })) },
    { name: "spike/sepa-instant", entityId: "x", isDefault: false, meta: null, standing: branchStanding(branchView()) },
    { name: "chore/bump-tokio", entityId: "y", isDefault: false, meta: null, standing: branchStanding(branchView({ ahead: "0" })) },
    { name: "unread", entityId: null, isDefault: false, meta: null, standing: null },
  ];
  const names = (show: string[], text = "") => visibleBranches(rows, show, text).map((row) => row.name);
  expect(names([])).toEqual(["main", "feature/PAY-231-sepa-retry", "spike/sepa-instant", "chore/bump-tokio", "unread"]);
  expect(names([], "sepa")).toEqual(["feature/PAY-231-sepa-retry", "spike/sepa-instant"]);
  expect(names(["No PR"])).toEqual(["spike/sepa-instant", "chore/bump-tokio"]);
  expect(names(["Merged"])).toEqual(["chore/bump-tokio"]);
  expect(names(["No PR", "Merged"])).toEqual(["chore/bump-tokio"]);
});

test("a ticket key is found in a branch name; the pull request is proposed from its ticket", () => {
  expect(keyIn("feature/PAY-231-sepa-retry")).toBe("PAY-231");
  expect(keyIn("fix/OPS2-9")).toBe("OPS2-9");
  expect(keyIn("spike/sepa-instant")).toBeNull();

  expect(proposal("feature/PAY-231-sepa-retry", { key: "PAY-231", summary: "Retry failed SEPA payouts", description: "The batch leaves 14 unsettled." })).toEqual({
    title: "PAY-231: Retry failed SEPA payouts",
    body: "The batch leaves 14 unsettled.\n\nCloses PAY-231.",
  });
  expect(proposal("feature/PAY-231-x", { key: "PAY-231", summary: "Retry", description: "" })).toEqual({ title: "PAY-231: Retry", body: "Closes PAY-231." });
  expect(proposal("spike/sepa-instant", null)).toEqual({ title: "sepa-instant", body: "" });
});

test("a new branch and a commit are prefilled from the job", () => {
  expect(newBranchName("PAY-231")).toBe("feature/PAY-231-");
  expect(newBranchName(null)).toBe("feature/");
  expect(commitPrefill("PAY-231", "src/sepa/retry.rs")).toEqual({ message: "PAY-231: update retry.rs", branch: "fix/PAY-231-retry" });
  expect(commitPrefill(null, "README.md")).toEqual({ message: "update README.md", branch: "fix/README" });
});

test("the edit and compare addresses", () => {
  expect(editHash(REPO, "src/a b.rs")).toBe("#/repo/gitea%3Atidewater%2Fpayout-service/edit/src/a%20b.rs");
  expect(compareHash(REPO, "feature/x")).toBe("#/repo/gitea%3Atidewater%2Fpayout-service/compare/feature%2Fx");
  expect(readRepoRoute([REPO, "edit", "src", "a b.rs"])).toMatchObject({ tab: "edit", path: "src/a b.rs" });
});

test("a branch from a commit starts where the commit's own New branch… starts, disabled where the source says", () => {
  const newBranch = (disabled: string | null): ViewAction =>
    ({
      ...action("create_branch", disabled),
      fields: [
        { name: "name", label: "Name", kind: { type: "text", suggestions: [] }, value: null, required: true, placeholder: null },
        { name: "from_ref", label: "From", kind: { type: "choice", options: [{ value: "c90d11e4a7b2f00d", label: "c90d11e4a7" }] }, value: null, required: true, placeholder: null },
      ],
    }) as ViewAction;
  expect(branchFrom({ sections: [], actions: [newBranch(null)] })).toEqual({ from: { value: "c90d11e4a7b2f00d", label: "c90d11e4a7" }, disabled: null });
  expect(branchFrom({ sections: [], actions: [newBranch("A mirror: its branches come from upstream")] })?.disabled).toBe(
    "A mirror: its branches come from upstream",
  );
  expect(branchFrom({ sections: [], actions: [] })).toBeNull();
  expect(branchFrom(null)).toBeNull();
});
