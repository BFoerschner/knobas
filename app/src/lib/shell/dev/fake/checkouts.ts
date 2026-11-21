/**
 * `?fake-ipc`: Checkouts (#499): the clones root, a repository's checkout and diff, the
 * *Open in…* applications (#844) and clones (#846). `seed` reads `?fake-clones-root`,
 * `?fake-long-paths`, `?fake-open-fails`, `?fake-no-git` and `?fake-clone-percent`.
 */
import type { CheckoutSettings, CheckoutView, CloneAllPlan, CloneAllState, CloneAnswer, DiffFile, DiffRead, LocalDir, OpenObject, PullRequestWorktree, RangeDiff, RangeFilePage, RangeFiles, RangeNames, ViewBlock, WorkingAt } from "../../../ipc/entity";
import { repoView } from "./views/repos";
import { lineDiff } from "../../../tf/diff/line-diff";
import { fillObject, isDiffEnd, joinInside, objectCommand, worktreeOf } from "../../../tf/openin/openin";
import type { FakeBridge, Handlers } from "../fake-tauri";
import { keepSeed, onFakeReset, seedOf } from "../fake-seed";
import { ASCII_AFTER, ASCII_BEFORE, asciiArt } from "../fake-ascii";
import { CORPUS } from "./corpus";

/**
 * The checkout fixture (#499): a clones root, what the scan "found" under it,
 * and the overrides this session has set.
 *
 * `let` and a mutable map, because the panel's whole point is that setting a
 * path changes what the next read says — a fixture that answered the same
 * thing after a write would make the QA pass prove nothing.
 */
const DEFAULT_CLONES_ROOT = "/Users/mara/src";

let fakeClonesRoot: string | null = DEFAULT_CLONES_ROOT;

/**
 * `?fake-clones-root=<path>` seeds it, and `?fake-clones-root=` (empty) leaves
 * it unset.
 *
 * The `?fake-db=` precedent: a QA pass has to be able to put a screen in a
 * state the default fixture is not in, and *no clones root set* is a different
 * sentence on the panel from *nothing found under one*. A headless run can
 * therefore point the fixture at a temporary directory of its own, which is
 * what #499's third criterion asks for — the fixture has no filesystem, so the
 * root is a label rather than a walk, and the walk is `knobas-core`'s to prove.
 */
function seedClonesRoot(params: URLSearchParams): void {
  const long = params.get("fake-long-paths");
  if (long !== null) {
    fakeScanRoot = long.trim() === "" ? LONG_ROOT : long.trim();
    fakeClonesRoot = fakeScanRoot;
    fakeFolderTail = LONG_TAIL;
    fakeGitPath = LONG_GIT;
  }
  const given = params.get("fake-clones-root");
  if (given === null) return;
  fakeClonesRoot = given.trim() === "" ? null : given.trim();
}

/**
 * `?fake-long-paths` (the clone-path pass, 2026-11-16): every path the fixture draws as a long Windows one, with spaces,
 * so a layout check can hold each place a path is drawn to its box. `?fake-long-paths=<root>` names the root instead.
 * The scan finds its clones under it, each folder ending in a long name, and git is deep under *Program Files*.
 */
const LONG_ROOT = String.raw`C:\Users\someone with a long name\source\repos\very\deep\nested\folders with spaces in them\and still more folders`;
const LONG_TAIL = "-the-repository-name-that-is-long";
const LONG_GIT = String.raw`C:\Program Files\Git for Windows, installed in a folder with a long name\mingw64\libexec\git-core\git.exe`;

/** The root the scan found its clones under: the default clones root, or `?fake-long-paths`' own. */
let fakeScanRoot = DEFAULT_CLONES_ROOT;
/** What ends each scanned folder's name after the repository's: nothing, or `?fake-long-paths`' long tail. */
let fakeFolderTail = "";
let fakeGitPath = "/usr/bin/git";

/** The folder the scan would find, per repo entity, under the scan's root. */
const FAKE_SCANNED: Record<string, string> = {
  "mock:tidewater/payout-service": "payout-service",
  // Code's *Local checkout* column (#837): two more clones, as prototype-6 has several.
  "mock:tidewater/psp-client": "psp-client",
  "mock:tidewater/rust-common": "rust-common",
};

/** Where the scan found `repo`'s clone, if it did: under the root the scan walked, and only while that is the clones root. */
function scannedPath(repo: string): string | undefined {
  const folder = FAKE_SCANNED[repo];
  return folder === undefined || fakeClonesRoot !== fakeScanRoot ? undefined : joinInside(fakeScanRoot, `${folder}${fakeFolderTail}`);
}

const fakeOverrides: Record<string, string> = {};

/** The repo a checkout question is about: itself, or a branch's repository. */
function fakeRepoOf(entityId: string): string | null {
  const entry = CORPUS.find((candidate) => candidate.entity_id === entityId);
  if (!entry) return null;
  if (entry.kind === "repo") return entry.entity_id;
  if (entry.kind !== "branch") return null;
  // The same prefix rule the backend uses: the longest repo id this branch's
  // id starts with (interfaces §4.2's nested key grammar).
  const repos = CORPUS.filter(
    (candidate) => candidate.kind === "repo" && entityId.startsWith(candidate.entity_id),
  ).sort((a, b) => b.entity_id.length - a.entity_id.length);
  return repos[0]?.entity_id ?? null;
}

/**
 * `entity_range_diff` (#2272): the repository's fake compare, read "at the
 * checkout" when the repository has one and both ends are shas (as the
 * backend's local git), else "from the source". Ignoring whitespace drops a
 * file whose removed and added lines differ only in whitespace.
 */
function fakeRangeDiff(entityId: string, base: string, head: string, ignoreWhitespace: boolean): RangeDiff {
  const shas = isDiffEnd(base, true) && isDiffEnd(head, false);
  return fakeDiffOf(repoView(entityId, `compare:${base}...${head}`), shas && fakeCheckout(entityId).path !== null, ignoreWhitespace);
}

/** The fake compares listed, by repository and the ends their files are read between, as the backend keeps the source's. */
const listedDiffs = new Map<string, RangeDiff>();

/** `entity_range_files` (the compare pass, 2026-11-13): the fake compare's files, without their lines. */
function fakeRangeFiles(entityId: string, base: string, head: string, diff: RangeDiff): RangeFiles {
  listedDiffs.set(`${entityId}\n${diff.range?.base ?? base}\n${diff.range?.head ?? head}`, diff);
  return { ...diff, files: diff.files.map((file) => ({ ...file, hunks: [], truncated: false })) };
}

/** `entity_range_file`: one file of the fake compare, whole in one page (the fake has no file long enough to page). */
function fakeRangeFile(entityId: string, base: string, head: string, path: string, from: number, ignoreWhitespace: boolean): RangeFilePage {
  const diff = listedDiffs.get(`${entityId}\n${base}\n${head}`) ?? (head === WORKING ? fakeWorking(entityId).diff : fakeRangeDiff(entityId, base, head, ignoreWhitespace));
  const file = diff.files.find((one) => one.path === path);
  if (file === undefined) throw { code: "not_found", message: `the diff changes no file ${path}`, source_id: null };
  return { file, from, continues: false, next: null };
}

/** The working tree's token (ux26, `tf/diff/address.ts`). */
const WORKING = "~working-tree";

/**
 * The fixture's working tree against any base (ux26, ADR-0050): what its one uncommitted change, a Markdown file, a
 * drawing and a new untracked note make, each file's text before and after, so a rendered side reads the same words
 * `entity_file_bytes` hands it for `~working-tree` and for the base's sha.
 */
const WORKING_FILES: Record<string, [before: string | null, after: string]> = {
  "docs/retry.md": [
    "# Retrying a payout\n\nA payout that fails with a transient code is sent again.\n\n## Steps\n\n- Validate the mandate\n- Submit the debit\n- Retry once after 24 h\n",
    "# Retrying a payout\n\nA payout that fails with a transient code is sent again with exponential backoff.\n\n## Steps\n\n- Validate the mandate\n- Submit the debit\n- Retry three times, 24 h, 48 h and 96 h apart\n- Alert the payments channel when the last retry fails\n",
  ],
  "docs/flow.mmd": ["flowchart LR\n  validate --> submit\n", "flowchart LR\n  validate --> submit\n  submit -->|transient| retry\n  retry --> submit\n"],
  "notes/new-idea.md": [null, "# A new idea\n\nKeep the retry count in the ledger, not in memory.\n"],
};
/** The working tree's files, with `?fake-ascii`'s drawing among them (`fake-ascii.ts`). */
function workingFiles(): Record<string, [before: string | null, after: string]> {
  return asciiArt() ? { ...WORKING_FILES, "docs/states.txt": [ASCII_BEFORE, ASCII_AFTER] } : WORKING_FILES;
}
const WORKING_BASE = "4f2a91c0d2b9e8a7f6c5d4b3a2918f7e6d5c4b3a";
/** `?fake-touched`: every ask finds the tree touched since the last (a new stamp each time). */
let workingAsks = 0;
const touched = () => new URLSearchParams(globalThis.location?.search).has("fake-touched");

/** The fixture's working tree as `entity_range_files` answers it for `~working-tree`; refused with no checkout. */
function fakeWorking(entityId: string): { diff: RangeDiff; working: WorkingAt } {
  const checkout = fakeCheckout(entityId);
  if (checkout.path === null) throw { code: "not_found", message: "This repository is not cloned on this computer, so it has no working tree here.", source_id: null };
  const local = fakeCheckoutDiff(entityId);
  const tracked = local.status === "read" ? local.unstaged.files.filter((file) => workingFiles()[file.path] === undefined) : [];
  const made = Object.entries(workingFiles()).map(([path, [before, after]]) => {
    const file = lineDiff(path, before ?? "", after);
    return { ...file, change: before === null ? "added" : file.change } as DiffFile;
  });
  const files = [...tracked, ...made].sort((a, b) => (a.path < b.path ? -1 : 1));
  const tree = checkout.tree?.status === "read" ? checkout.tree : null;
  workingAsks += 1;
  return {
    diff: { files, truncated: false, range: { repository: entityId, base: WORKING_BASE, head: WORKING, commits: [] }, read_at: "checkout" },
    working: {
      stamp: touched() ? `touched-${workingAsks}` : "fixed",
      head: tree?.head ?? "9c1d2aa0d2b9e8a7f6c5d4b3a2918f7e6d5c4b3a",
      branch: tree?.branch ?? null,
      untracked: ["notes/new-idea.md"],
      untracked_truncated: false,
    },
  };
}

/** A working-tree file's bytes at `~working-tree` (after) or any other end (before); `undefined` for a file it does not hold. */
export function fakeWorkingBytes(path: string, commit: string | null): Uint8Array | undefined {
  const sides = workingFiles()[path];
  // Only the working tree's own ends: every other commit reads the fixture's files as before.
  if (sides === undefined || (commit !== WORKING && commit !== "~index" && commit !== WORKING_BASE)) return undefined;
  const text = commit === WORKING ? sides[1] : sides[0];
  if (text === null) throw { code: "not_found", message: `no file ${path} there`, source_id: null };
  return new TextEncoder().encode(text);
}

/** The names `entity_range_names` answers for the working tree: the list less its counts. */
function fakeWorkingNames(entityId: string): RangeNames {
  const { diff, working } = fakeWorking(entityId);
  const letter = { added: "A", deleted: "D", renamed: "R", modified: "M", binary: "M" } as const;
  return {
    paths: diff.files.map((f) => f.path),
    changes: diff.files.map((f) => letter[f.change]).join(""),
    renamed: [],
    modes: [],
    truncated: false,
    range: diff.range!,
    predicted_at: "",
    working,
  };
}

/**
 * `entity_commit_diff` (#2319): the commit's fake `diff` section, read "at the
 * checkout" when its repository has one and `sha` is one, else "from the source".
 */
function fakeCommitDiff(entityId: string, commitId: string, sha: string, ignoreWhitespace: boolean): RangeDiff {
  return fakeDiffOf(repoView(commitId, "diff"), isDiffEnd(sha, false) && fakeCheckout(entityId).path !== null, ignoreWhitespace);
}

/** A fake view's diff block as a `RangeDiff`; at a checkout, whitespace-only files drop when asked. */
function fakeDiffOf(view: ReturnType<typeof repoView>, local: boolean, ignoreWhitespace: boolean): RangeDiff {
  const block = view?.sections[0]?.blocks.find((candidate) => candidate.type === "diff");
  if (block?.type !== "diff") throw { code: "not_found", message: "the source draws no diff here", source_id: null };
  const bare = (file: DiffFile, kind: "add" | "del") =>
    file.hunks.flatMap((hunk) => hunk.lines.filter((line) => line.kind === kind).map((line) => line.text.replace(/\s+/g, ""))).join("\n");
  const files = local && ignoreWhitespace ? block.files.filter((file) => bare(file, "add") !== bare(file, "del")) : block.files;
  return { files, truncated: block.truncated, range: block.range ?? null, read_at: local ? "checkout" : "source" };
}

/** The checkout's `HEAD`, where the fixture has one. */
function fakeHead(entityId: string): string | null {
  const tree = fakeCheckout(entityId).tree;
  return tree?.status === "read" ? tree.head : null;
}

/** `entity_local_dir` at the checkout's `HEAD`: the source's listing of the folder; `null` at any other commit, as before. */
function fakeHeadDir(entityId: string, commit: string | null, dir: string): LocalDir | null {
  if (commit === null || commit !== fakeHead(entityId)) return null;
  const block = repoView(entityId, dir === "" ? "files" : `files:${dir}`)?.sections[0]?.blocks[0];
  if (block?.type !== "tree") return null;
  const entries = block.entries.map((entry) => ({ path: dir === "" ? entry.name : `${dir}/${entry.name}`, kind: entry.kind, size: entry.size }));
  return { commit, as_of: null, dir, entries, complete: true };
}

/** `entity_local_file` at the checkout's `HEAD`: the source's file. */
function fakeHeadFile(entityId: string, commit: string | null, path: string): ViewBlock | null {
  if (commit === null || commit !== fakeHead(entityId)) return null;
  const block = repoView(entityId, `file:${path}`)?.sections[0]?.blocks[0];
  return block?.type === "file" ? { ...block, commit } : null;
}

/** `entity_checkout`, including the refusal a non-repo kind gets. */
function fakeCheckout(entityId: string): CheckoutView {
  const entry = CORPUS.find((candidate) => candidate.entity_id === entityId);
  if (!entry) {
    throw { code: "not_found", message: `${entityId} is not in the mirror`, source_id: null };
  }
  if (entry.kind !== "repo" && entry.kind !== "branch") {
    throw {
      code: "invalid",
      message: `a checkout belongs to a repo or a branch, and ${entityId} is a ${entry.kind}`,
      source_id: null,
    };
  }
  const repo = fakeRepoOf(entityId);
  const repoRow = CORPUS.find((candidate) => candidate.entity_id === repo);
  const url = repoRow?.web_url ?? null;
  const override = repo === null ? undefined : fakeOverrides[repo];
  // Keyed on the *root* as well as the repo, so a run pointed at a temporary
  // directory of its own sees the honest *no checkout* rather than a path
  // under a root nobody set.
  const scanned = repo === null ? undefined : scannedPath(repo);
  const path = override ?? scanned ?? (repo === null ? undefined : fakeCloned[repo]) ?? null;
  return {
    repo_entity_id: repo,
    repo_url: url,
    path,
    found_by: override !== undefined ? "override" : path !== null ? "scan" : "nothing",
    clones_root: fakeClonesRoot,
    clone_command: url === null ? null : `git clone ${url}`,
    // The fixture has no git either (#704): a checkout stands on `main` with
    // one change and one commit to push, so the panel's line has something to
    // say and the warn tone shows.
    tree:
      path === null
        ? null
        : {
            status: "read",
            branch: "main",
            head: "4f1c2a9e0b7d3c5a8e6f1b2d4c7a9e0f3b5d8c1a",
            upstream: "origin/main",
            ahead: 1,
            behind: 0,
            changed: 1,
            untracked: 0,
            conflicted: 0,
          },
  };
}

/**
 * `entity_checkout_diff` (#824): the one unstaged change the fixture's working
 * tree counts, as `git diff` would draw it, and nothing staged or untracked.
 * Refused as the backend refuses an entity with no checkout.
 */
function fakeCheckoutDiff(entityId: string): DiffRead {
  const checkout = fakeCheckout(entityId);
  if (checkout.path === null) {
    throw {
      code: "not_found",
      message: `${entityId} has no checkout on this machine, so there are no local changes to read`,
      source_id: null,
    };
  }
  return {
    status: "read",
    unstaged: {
      files: [
        {
          // The file prototype-6's repository page marks `M` (#838).
          path: "src/sepa/retry.rs",
          old_path: null,
          change: "modified",
          additions: 1,
          deletions: 1,
          hunks: [
            {
              header: "@@ -27,4 +27,4 @@ where",
              lines: [
                { kind: "context", old: 27, new: 27, text: "    Fut: std::future::Future<Output = Result<(), PspError>>," },
                { kind: "context", old: 28, new: 28, text: "{" },
                { kind: "del", old: 29, new: null, text: "    let mut attempt = 0;" },
                { kind: "add", old: null, new: 29, text: "    let mut attempt = 1;" },
                { kind: "context", old: 30, new: 30, text: "    while attempt <= MAX_ATTEMPTS {" },
              ],
            },
          ],
          truncated: false,
          threads: [],
        },
        // A Markdown file edited too (ux26): Local changes draws it rendered, the index against the working tree.
        { ...lineDiff("docs/retry.md", WORKING_FILES["docs/retry.md"]![0]!, WORKING_FILES["docs/retry.md"]![1]), change: "modified" } as DiffFile,
      ],
      truncated: false,
    },
    staged: { files: [], truncated: false },
    untracked: [],
    untracked_truncated: false,
  };
}

/**
 * The *Open in…* applications (#844): macOS's shipped four, transcribed (each
 * opening a pull request at its worktree since #2285),
 * because the browser this runs in has no platform to ask. The list a
 * session saves replaces them, as the backend's one settings row does.
 */
interface FakeApp {
  id: string;
  name: string;
  command: string;
  file_command: string | null;
  commit_command?: string;
  range_command?: string;
  pull_request_command?: string;
}

const SHIPPED: FakeApp[] = [
  {
    id: "vscode",
    name: "VS Code",
    command: 'open -a "Visual Studio Code" {path}',
    file_command: 'open -a "Visual Studio Code" {file}',
    pull_request_command: 'open -a "Visual Studio Code" {path}',
  },
  { id: "terminal", name: "Terminal", command: "open -a Terminal {path}", file_command: null, pull_request_command: "open -a Terminal {path}" },
  { id: "fork", name: "Fork", command: "open -a Fork {path}", file_command: null, pull_request_command: "open -a Fork {path}" },
  {
    id: "jetbrains",
    name: "IntelliJ IDEA",
    command: 'open -a "IntelliJ IDEA" {path}',
    file_command: 'open -a "IntelliJ IDEA" {file}',
    pull_request_command: 'open -a "IntelliJ IDEA" {path}',
  },
];

let fakeApps: FakeApp[] = SHIPPED.map((app) => ({ ...app }));
let fakeDefault: string | null = "vscode";

/**
 * The application whose spawn refuses, from `?fake-open-fails=<id>`: the one
 * arm of this feature a fixture cannot reach by itself, a program this
 * machine has not got.
 */
let fakeOpenFails: string | null = null;

/** `?fake-no-git`: Settings › Checkouts as a machine without git draws it. */
let fakeNoGit = false;

function seedOpenState(params: URLSearchParams): void {
  const fails = params.get("fake-open-fails");
  if (fails !== null) fakeOpenFails = fails.trim() === "" ? null : fails.trim();
  if (params.has("fake-no-git")) fakeNoGit = true;
}

/**
 * The template rule, deliberately narrow: `knobas_core::checkout::open` is
 * asserted over every shape. What this owes is the refusal a QA pass has to
 * be able to see, a placeholder knobas does not fill, named.
 */
function refuse(command: string, who: string): void {
  const other = [...command.matchAll(/\{([^}]*)\}/g)].map((match) => match[1]!).find((n) => !["path", "file", "line", "name"].includes(n));
  if (other !== undefined) {
    throw {
      code: "invalid",
      message: `${who} cannot be run: {${other}} is not something knobas fills in: a command may take {path}, {file}, {line} and {name}, all from this disk`,
      source_id: null,
    };
  }
  if (command.trim() === "") {
    throw { code: "invalid", message: `${who} cannot be run: the command is empty, so there is no program to run`, source_id: null };
  }
}

function slug(name: string): string {
  return name.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-+|-+$/g, "") || "app";
}

/** `checkout_settings`. */
function fakeSettings(): CheckoutSettings {
  return {
    clones_root: fakeClonesRoot,
    git: fakeNoGit ? { status: "no_git" } : { status: "found", path: fakeGitPath },
    default_application: fakeApps.some((app) => app.id === fakeDefault) ? fakeDefault : (fakeApps[0]?.id ?? null),
    applications: fakeApps.map((app) => ({ ...app })),
  };
}

/** `set_checkout_applications`, refusals included. */
function fakeSetApps(drafts: unknown, defaultId: unknown) {
  const list = Array.isArray(drafts) ? (drafts as Partial<FakeApp>[]) : [];
  const out: FakeApp[] = [];
  list.forEach((draft, index) => {
    const name = String(draft.name ?? "").trim();
    if (name === "") throw { code: "invalid", message: `application ${index + 1} has no name`, source_id: null };
    const command = String(draft.command ?? "").trim();
    refuse(command, `${name}'s command`);
    const file = String(draft.file_command ?? "").trim() || null;
    if (file !== null) refuse(file, `${name}'s file command`);
    const given = String(draft.id ?? "").trim();
    let id = given !== "" && !out.some((app) => app.id === given) ? given : slug(name);
    for (let n = 2; out.some((app) => app.id === id); n += 1) id = `${slug(name)}-${n}`;
    const optional = (key: "commit_command" | "range_command" | "pull_request_command") => {
      const given = String(draft[key] ?? "").trim();
      return given === "" ? {} : { [key]: given };
    };
    out.push({
      id,
      name,
      command,
      file_command: file,
      ...optional("commit_command"),
      ...optional("range_command"),
      ...optional("pull_request_command"),
    });
  });
  fakeApps = out;
  fakeDefault = typeof defaultId === "string" ? defaultId : null;
  return fakeSettings();
}

/** A command split into words, quotes grouping only, as the backend splits it. */
function words(command: string): string[] {
  return [...command.matchAll(/"([^"]*)"|'([^']*)'|(\S+)/g)].map((match) => match[1] ?? match[2] ?? match[3] ?? "");
}

/**
 * `open_checkout`. Starts nothing (there is no process in a browser) and
 * answers the argv the backend would start; refuses for the application
 * `?fake-open-fails` names, in the shape the real command refuses a program
 * that is not installed.
 */
function fakeOpen(args: Record<string, unknown>) {
  const entityId = String(args["entityId"] ?? "");
  const checkout = fakeCheckout(entityId);
  if (checkout.path === null) {
    throw { code: "not_found", message: `${entityId} has no checkout on this machine, so there is nothing to open`, source_id: null };
  }
  const file = typeof args["file"] === "string" && args["file"] !== "" ? `${checkout.path}/${args["file"]}` : null;
  const line = typeof args["line"] === "number" ? args["line"] : 1;
  const id = typeof args["application"] === "string" ? args["application"] : fakeSettings().default_application;
  const app = fakeApps.find((one) => one.id === id);
  if (!app) throw { code: "invalid", message: `'${id}' is not one of your applications: see Settings › Checkouts`, source_id: null };
  if (app.id === fakeOpenFails) {
    throw { code: "invalid", message: `'${app.command}' could not be run: No such file or directory (os error 2)`, source_id: null };
  }
  const template = file !== null && app.file_command ? app.file_command : app.command;
  refuse(template, `'${template}'`);
  const name = checkout.path.split("/").pop() ?? "";
  return words(template).map((word) =>
    word
      .replace(/\{path\}/g, checkout.path!)
      .replace(/\{file\}/g, file ?? checkout.path!)
      .replace(/\{line\}/g, String(line))
      .replace(/\{name\}/g, name),
  );
}

/**
 * `open_folder` (2026-11-16): the application's own command with the folder in
 * the checkout's place, as the backend runs it; a folder with `..` in it, or
 * git's own, is refused as there.
 */
function fakeOpenFolder(args: Record<string, unknown>) {
  const folder = String(args["folder"] ?? "").replace(/^\.\/|\/$/g, "");
  const parts = folder.split("/");
  if (folder === "" || parts.some((part) => part === ".." || part === ".git" || part === "") || folder.startsWith("/")) {
    throw { code: "invalid", message: `${folder} is not a folder inside the checkout`, source_id: null };
  }
  const entityId = String(args["entityId"] ?? "");
  const checkout = fakeCheckout(entityId);
  if (checkout.path === null) {
    throw { code: "not_found", message: `${entityId} has no checkout on this machine, so there is nothing to open`, source_id: null };
  }
  const at = `${checkout.path}/${folder}`;
  const id = typeof args["application"] === "string" ? args["application"] : fakeSettings().default_application;
  const app = fakeApps.find((one) => one.id === id);
  if (!app) throw { code: "invalid", message: `'${id}' is not one of your applications: see Settings › Checkouts`, source_id: null };
  refuse(app.command, `'${app.command}'`);
  const name = parts[parts.length - 1] ?? "";
  return words(app.command).map((word) => word.replace(/\{path\}/g, at).replace(/\{file\}/g, at).replace(/\{line\}/g, "1").replace(/\{name\}/g, name));
}

/** The fake's Gitea pages (#2274), as `code_pages` declares Gitea's, after its base URL. */
function fakePage(entityId: string, object: OpenObject): string | null {
  const repo = fakeRepoOf(entityId);
  const url = repo === null ? null : fakeCheckout(repo).repo_url;
  if (url === null) return null;
  const enc = (value: string) => value.split("/").map(encodeURIComponent).join("/");
  switch (object.kind) {
    case "repository":
      return url;
    case "file":
      return object.at === null ? null : `${url}/src/${enc(object.at)}/${enc(object.path)}#L${object.line ?? 1}`;
    case "commit":
      return `${url}/commit/${object.sha}`;
    case "range":
      return `${url}/compare/${enc(object.base)}...${enc(object.head)}`;
    case "pull_request":
      return `${url}/pulls/${object.number}`;
  }
}

/** `open_object` (#2274): the argv the backend would start, nothing started. */
function fakeOpenObject(args: Record<string, unknown>) {
  const entityId = String(args["entityId"] ?? "");
  const object = args["object"] as OpenObject;
  const checkout = fakeCheckout(entityId);
  if (checkout.path === null) {
    throw { code: "not_found", message: `${entityId} has no checkout on this machine, so there is nothing to open`, source_id: null };
  }
  const id = typeof args["application"] === "string" ? args["application"] : fakeSettings().default_application;
  const app = fakeApps.find((one) => one.id === id);
  if (!app) throw { code: "invalid", message: `'${id}' is not one of your applications: see Settings › Checkouts`, source_id: null };
  const template = objectCommand({ ...app }, object.kind);
  if (template === null) {
    throw { code: "invalid", message: `${app.name} does not open this: add it in Settings › Checkouts`, source_id: null };
  }
  // A pull request opens in its worktree under the clones root (#2285), which the fake lists.
  let path = checkout.path;
  if (object.kind === "pull_request") {
    path = worktreeOf(fakeClonesRoot, checkout.repo_entity_id, object.number) ?? path;
    if (!fakeWorktrees.some((one) => one.path === path)) {
      fakeWorktrees.push({ repo_entity_id: checkout.repo_entity_id ?? entityId, number: object.number, path, closed: false });
    }
  }
  return words(template).map((word) => fillObject(word, { path, object, url: fakePage(entityId, object) }));
}

/** The pull request worktrees `open_object` made this session (#2285). */
let fakeWorktrees: PullRequestWorktree[] = [];

/** `diff_tool` (#2275): offered when the repository is cloned and both ends are shas. */
function fakeDiffTool(args: Record<string, unknown>): string[] | null {
  const checkout = fakeCheckout(String(args["entityId"] ?? ""));
  const base = String(args["base"] ?? "");
  const head = String(args["head"] ?? "");
  if (checkout.path === null || !isDiffEnd(base, true) || !isDiffEnd(head, false)) return null;
  return [fakeGitPath, "-C", checkout.path, "difftool", "--dir-diff", "--gui", "--no-prompt", base, head, "--"];
}

/**
 * Clones (#846): what `clone_repo` started, ticking `clone:progress` through
 * the bridge as prototype-6's `startClone` does (8 %, 21 %, … 100 %, a step
 * every 350 ms), and done: the repository is then on this machine, at
 * `<clones root>/<name>`.
 *
 * `?fake-clone-percent=<n>` stops every clone at `n` %, for a screenshot of
 * one running.
 */
const fakeCloning = new Map<string, ReturnType<typeof setInterval>>();
const fakeCloned: Record<string, string> = {};
let fakeClonePercent: number | null = null;
let bridge: FakeBridge | null = null;

const STEPS = [8, 21, 37, 52, 68, 81, 93, 100];

function seedClones(params: URLSearchParams): void {
  const percent = params.get("fake-clone-percent");
  if (percent !== null && !Number.isNaN(Number(percent))) fakeClonePercent = Number(percent);
}

function emit(entityId: string, path: string, state: Record<string, unknown>): void {
  bridge?.emit("clone:progress", { entity_id: entityId, path, ...state });
}

function fakeClone(args: Record<string, unknown>): CloneAnswer {
  const id = String(args["entityId"] ?? "");
  const entry = CORPUS.find((candidate) => candidate.entity_id === id);
  if (!entry || entry.kind !== "repo") {
    throw { code: "invalid", message: `${id} is not a repository in the mirror`, source_id: null };
  }
  if (fakeClonesRoot === null) {
    throw { code: "invalid", message: "no clones root is set: choose where clones go in Settings › Checkouts", source_id: null };
  }
  const path = joinInside(fakeClonesRoot, entry.title.split("/").pop() ?? entry.title);
  if (args["stop"] === true) {
    const timer = fakeCloning.get(id);
    if (timer === undefined) return { status: "not_cloning" };
    clearInterval(timer);
    fakeCloning.delete(id);
    return { status: "stopped", path, removed: true };
  }
  if (fakeCheckout(id).path !== null) return { status: "adopted", path };
  if (fakeCloning.has(id)) return { status: "cloning", path };
  let step = 0;
  const timer = setInterval(() => {
    const percent = fakeClonePercent ?? STEPS[step++]!;
    if (percent >= 100) {
      clearInterval(timer);
      fakeCloning.delete(id);
      fakeCloned[id] = path;
      emit(id, path, { state: "done" });
      return;
    }
    emit(id, path, { state: "progress", phase: "Receiving objects", percent });
    if (fakeClonePercent !== null) clearInterval(timer);
  }, 350);
  fakeCloning.set(id, timer);
  return { status: "cloning", path };
}

/**
 * *Clone all* (PR #2479): every repository of the corpus not on this machine,
 * cloned one after another in 300 ms each, its run told on `clone:all` and
 * each clone on `clone:progress`.
 */
let fakeCloneAll: CloneAllState | null = null;

function fakeCloneAllPlan(): CloneAllPlan {
  const missing = CORPUS.filter((entry) => entry.kind === "repo" && fakeCheckout(entry.entity_id).path === null);
  const sources = new Map<string, number>();
  for (const entry of missing) {
    const source = entry.entity_id.slice(0, entry.entity_id.indexOf(":"));
    sources.set(source, (sources.get(source) ?? 0) + 1);
  }
  return {
    root: fakeClonesRoot,
    names: fakeClonesRoot === null ? [] : missing.map((entry) => entry.title),
    sources: [...sources].map(([source_id, repositories]) => ({ source_id, name: source_id === "mock" ? "Tidewater Git" : source_id, repositories })),
    already: CORPUS.filter((entry) => entry.kind === "repo").length - missing.length,
    skipped: [],
  };
}

function fakeStartCloneAll(): CloneAllState {
  if (fakeCloneAll !== null && !fakeCloneAll.finished) return fakeCloneAll;
  if (fakeClonesRoot === null) throw { code: "invalid", message: "no clones root is set: choose where clones go in Settings › Checkouts", source_id: null };
  const todo = CORPUS.filter((entry) => entry.kind === "repo" && fakeCheckout(entry.entity_id).path === null);
  const run: CloneAllState = { run: (fakeCloneAll?.run ?? 0) + 1, total: todo.length, cloned: 0, adopted: 0, failed: [], current: [], finished: todo.length === 0 };
  fakeCloneAll = run;
  const tell = () => bridge?.emit("clone:all", { ...run, failed: [...run.failed], current: [...run.current] });
  const next = () => {
    const entry = todo.shift();
    if (entry === undefined) {
      run.finished = true;
      tell();
      return;
    }
    run.current = [entry.title];
    tell();
    // Quicker than one *Clone*'s eight steps, so a whole run fits under a trip's wait.
    const path = joinInside(fakeClonesRoot ?? "", entry.title.split("/").pop() ?? entry.title);
    emit(entry.entity_id, path, { state: "progress", phase: "Receiving objects", percent: 50 });
    setTimeout(() => {
      fakeCloned[entry.entity_id] = path;
      emit(entry.entity_id, path, { state: "done" });
      run.cloned += 1;
      run.current = [];
      next();
    }, 300);
  };
  setTimeout(next, 0);
  return { ...run };
}

/** This feature's commands. */
export function handlers(): Handlers {
  return {
    // Checkouts (#499). The fixture has no filesystem, so what stands in for
    // the scan is `FAKE_SCANNED`, what a clone made (`fakeCloned`) and the
    // overrides this file keeps.
    set_clones_root: (args) => {
      const path = args["path"];
      const typed = typeof path === "string" && path.trim() !== "" ? path.trim().replace(/^~(?=\/|$)/, "/Users/mara") : null;
      // The refusal the backend gives (#1661): not absolute once `~` is home.
      if (typed !== null && !typed.startsWith("/")) {
        throw {
          code: "invalid",
          message: `the clones root must be an absolute path (or start with ~/), not ${JSON.stringify(typed)}`,
          source_id: null,
        };
      }
      fakeClonesRoot = typed;
      return null;
    },
    entity_checkout: (args) => fakeCheckout(String(args["entityId"] ?? "")),
    open_checkout: (args) => fakeOpen(args),
    open_object: (args) => fakeOpenObject(args),
    open_folder: (args) => fakeOpenFolder(args),
    object_page: (args) => fakePage(String(args["entityId"] ?? ""), args["object"] as OpenObject),
    diff_tool: (args) => fakeDiffTool(args),
    open_diff_tool: (args) => {
      const argv = fakeDiffTool(args);
      if (argv === null) throw { code: "invalid", message: "the Diff tool opens two commits this repository's checkout holds", source_id: null };
      return argv;
    },
    pull_request_worktrees: () => fakeWorktrees,
    remove_pull_request_worktree: (args) => {
      const path = String(args["path"] ?? "");
      if (!fakeWorktrees.some((one) => one.path === path)) {
        throw { code: "not_found", message: `${path} is not a worktree knobas made`, source_id: null };
      }
      fakeWorktrees = fakeWorktrees.filter((one) => one.path !== path);
      return fakeWorktrees;
    },
    checkout_settings: () => fakeSettings(),
    set_checkout_applications: (args) => fakeSetApps(args["applications"], args["defaultApplication"]),
    entity_checkout_diff: (args) => fakeCheckoutDiff(String(args["entityId"] ?? "")),
    entity_range_diff: (args) =>
      String(args["head"] ?? "") === WORKING
        ? { ...fakeWorking(String(args["entityId"] ?? "")).diff, working: fakeWorking(String(args["entityId"] ?? "")).working }
        : fakeRangeDiff(String(args["entityId"] ?? ""), String(args["base"] ?? ""), String(args["head"] ?? ""), args["ignoreWhitespace"] === true),
    entity_commit_diff: (args) =>
      fakeCommitDiff(String(args["entityId"] ?? ""), String(args["commitId"] ?? ""), String(args["sha"] ?? ""), args["ignoreWhitespace"] === true),
    clone_repo: (args) => fakeClone(args),
    // The demo has no checkout on disk to read a tree at the mirror's tip (ADR-0041): Files reads the fake source, as before.
    // At the checkout's own `HEAD` it answers with the source's tree, so the working tree in the branch box (2026-11-21)
    // draws: `HEAD`'s tree with the fixture's uncommitted changes laid over it.
    entity_local_tree: () => null,
    entity_local_dir: (args) => fakeHeadDir(String(args["entityId"] ?? ""), (args["commit"] as string | null | undefined) ?? null, String(args["dir"] ?? "")),
    entity_local_paths: () => null,
    entity_local_file: (args) => fakeHeadFile(String(args["entityId"] ?? ""), (args["commit"] as string | null | undefined) ?? null, String(args["path"] ?? "")),
    entity_source_range_diff: (args) =>
      fakeRangeDiff(String(args["entityId"] ?? ""), String(args["base"] ?? ""), String(args["head"] ?? ""), false),
    entity_range_files: (args) => {
      const [id, base, head] = [String(args["entityId"] ?? ""), String(args["base"] ?? ""), String(args["head"] ?? "")];
      if (head === WORKING) {
        const { diff, working } = fakeWorking(id);
        return { ...fakeRangeFiles(id, base, head, diff), working };
      }
      return fakeRangeFiles(id, base, head, fakeRangeDiff(id, base, head, args["ignoreWhitespace"] === true));
    },
    // The fake checkouts list their compares whole: Compare reads the list as before. The working tree's names come first.
    entity_range_names: (args) => (String(args["head"] ?? "") === WORKING ? fakeWorkingNames(String(args["entityId"] ?? "")) : null),
    entity_source_range_files: (args) => {
      const [id, base, head] = [String(args["entityId"] ?? ""), String(args["base"] ?? ""), String(args["head"] ?? "")];
      return fakeRangeFiles(id, base, head, fakeRangeDiff(id, base, head, false));
    },
    entity_range_file: (args) =>
      fakeRangeFile(
        String(args["entityId"] ?? ""),
        String(args["base"] ?? ""),
        String(args["head"] ?? ""),
        String(args["path"] ?? ""),
        Number(args["from"] ?? 0),
        args["ignoreWhitespace"] === true,
      ),
    entity_range_file_pages: (args) => {
      const [id, base, head] = [String(args["entityId"] ?? ""), String(args["base"] ?? ""), String(args["head"] ?? "")];
      const asks = (args["files"] as { path: string }[] | undefined) ?? [];
      return asks.map(({ path }) => {
        try {
          return fakeRangeFile(id, base, head, path, 0, args["ignoreWhitespace"] === true);
        } catch {
          return null;
        }
      });
    },
    cancel_read: () => false,
    // *Clone all* (PR #2479): the corpus's repositories not on this machine, one after another.
    clone_all_plan: () => fakeCloneAllPlan(),
    clone_all: () => fakeStartCloneAll(),
    clone_all_state: () => (fakeCloneAll === null ? null : { ...fakeCloneAll }),
    entity_checkouts: (args) =>
      ((args["entityIds"] as string[] | undefined) ?? []).flatMap((entityId) => {
        try {
          return [{ entity_id: entityId, checkout: fakeCheckout(entityId) }];
        } catch {
          return [];
        }
      }),
    set_checkout_override: (args) => {
      const id = String(args["entityId"] ?? "");
      const repo = fakeRepoOf(id);
      const path = args["path"];
      if (repo === null) {
        throw { code: "not_found", message: `${id} has no repository in the mirror`, source_id: null };
      }
      if (typeof path === "string" && path.trim() !== "") {
        fakeOverrides[repo] = path.trim();
      } else {
        delete fakeOverrides[repo];
      }
      return fakeCheckout(id);
    },
  };
}

/** What `?fake-ipc`'s query seeds before the bridge is installed. */
export function seed(params: URLSearchParams): void {
  seedClonesRoot(params);
  // Settings' worktree list, which is empty until a pull request opens in one, holds one under the long root.
  const tree = params.has("fake-long-paths") ? worktreeOf(fakeClonesRoot, "mock:tidewater/payout-service", 142) : null;
  if (tree !== null) fakeWorktrees = [{ repo_entity_id: "mock:tidewater/payout-service", number: 142, path: tree, closed: false }];
  seedOpenState(params);
  seedClones(params);
}

/** The bridge, once installed: what a clone's progress is emitted through. */
export function attach(installed: FakeBridge): void {
  bridge = installed;
}

const atLoad = seedOf({ fakeClonesRoot, fakeScanRoot, fakeFolderTail, fakeGitPath, fakeApps, fakeDefault, fakeOpenFails, fakeNoGit, fakeClonePercent, bridge });
onFakeReset(() => {
  for (const timer of fakeCloning.values()) clearInterval(timer);
  fakeWorktrees = [];
  fakeCloneAll = null;
  ({ fakeClonesRoot, fakeScanRoot, fakeFolderTail, fakeGitPath, fakeApps, fakeDefault, fakeOpenFails, fakeNoGit, fakeClonePercent, bridge } = atLoad());
});
keepSeed(fakeOverrides, fakeCloning, fakeCloned, listedDiffs);
