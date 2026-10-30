/**
 * `?fake-ipc`: Checkouts (#499): the clones root, a repository's checkout and diff, the
 * *Open in…* applications (#844) and clones (#846). `seed` reads `?fake-clones-root`,
 * `?fake-open-fails`, `?fake-no-git` and `?fake-clone-percent`.
 */
import type { CheckoutSettings, CheckoutView, CloneAnswer, DiffRead, OpenObject, PullRequestWorktree } from "../../../ipc/entity";
import { fillObject, isDiffEnd, objectCommand, worktreeOf } from "../../../tf/openin/openin";
import type { FakeBridge, Handlers } from "../fake-tauri";
import { keepSeed, onFakeReset, seedOf } from "../fake-seed";
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
  const given = params.get("fake-clones-root");
  if (given === null) return;
  fakeClonesRoot = given.trim() === "" ? null : given.trim();
}

/** What the scan would find, per repo entity. */
const FAKE_SCANNED: Record<string, string> = {
  "mock:tidewater/payout-service": "/Users/mara/src/payout-service",
  // Code's *Local checkout* column (#837): two more clones, as prototype-6 has several.
  "mock:tidewater/psp-client": "/Users/mara/src/psp-client",
  "mock:tidewater/rust-common": "/Users/mara/src/rust-common",
};

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
  const scanned =
    repo === null || fakeClonesRoot !== DEFAULT_CLONES_ROOT ? undefined : FAKE_SCANNED[repo];
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
      ],
      truncated: false,
    },
    staged: { files: [], truncated: false },
    untracked: [],
    untracked_truncated: false,
  };
}

/**
 * The *Open in…* applications (#844): macOS's shipped four, transcribed,
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
  { id: "vscode", name: "VS Code", command: 'open -a "Visual Studio Code" {path}', file_command: 'open -a "Visual Studio Code" {file}' },
  { id: "terminal", name: "Terminal", command: "open -a Terminal {path}", file_command: null },
  { id: "fork", name: "Fork", command: "open -a Fork {path}", file_command: null },
  { id: "jetbrains", name: "IntelliJ IDEA", command: 'open -a "IntelliJ IDEA" {path}', file_command: 'open -a "IntelliJ IDEA" {file}' },
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
    git: fakeNoGit ? { status: "no_git" } : { status: "found", path: "/usr/bin/git" },
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
  return ["/usr/bin/git", "-C", checkout.path, "difftool", "--dir-diff", "--gui", "--no-prompt", base, head, "--"];
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
  const path = `${fakeClonesRoot}/${entry.title.split("/").pop()}`;
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
    clone_repo: (args) => fakeClone(args),
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
  seedOpenState(params);
  seedClones(params);
}

/** The bridge, once installed: what a clone's progress is emitted through. */
export function attach(installed: FakeBridge): void {
  bridge = installed;
}

const atLoad = seedOf({ fakeClonesRoot, fakeApps, fakeDefault, fakeOpenFails, fakeNoGit, fakeClonePercent, bridge });
onFakeReset(() => {
  for (const timer of fakeCloning.values()) clearInterval(timer);
  fakeWorktrees = [];
  ({ fakeClonesRoot, fakeApps, fakeDefault, fakeOpenFails, fakeNoGit, fakeClonePercent, bridge } = atLoad());
});
keepSeed(fakeOverrides, fakeCloning, fakeCloned);
