/**
 * `?fake-ipc`: Checkouts (#499): the clones root, a repository's checkout and diff, and
 * the open commands. `seed` reads `?fake-clones-root`, `?fake-open-fails` and
 * `?fake-open-unset`.
 */
import type { Handler } from "../fake-tauri";
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
function fakeCheckout(entityId: string) {
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
  const path = override ?? scanned ?? null;
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
function fakeCheckoutDiff(entityId: string) {
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
 * The open commands (#501): what each button on a repo detail would run.
 *
 * The fixture has no processes, so nothing here starts one — what it stands in
 * for is the *state* the panel and the settings section branch on: a template
 * set, a template that is this platform's default, and none at all. macOS's
 * three defaults are transcribed, because the browser this runs in has no
 * platform to ask.
 */
const FAKE_COMMANDS: { action: string; label: string; default: string | null }[] = [
  { action: "vscode", label: "Open in VS Code", default: 'open -a "Visual Studio Code" {path}' },
  { action: "jetbrains", label: "Open in JetBrains", default: 'open -a "IntelliJ IDEA" {path}' },
  { action: "terminal", label: "Open terminal here", default: "open -a Terminal {path}" },
];

/** What this session has set, per action. */
const fakeTemplates: Record<string, string> = {};

/**
 * The action whose spawn refuses, from `?fake-open-fails=<action>`.
 *
 * The `?fake-clones-root` precedent: a QA pass has to be able to put a screen
 * in a state the default fixture is not in, and *the program would not start*
 * is the one arm of this feature a fixture cannot reach by itself. With it
 * set, pressing that button draws the failure the real backend produces for a
 * template naming a program this machine has not got.
 */
let fakeOpenFails: string | null = null;

/**
 * The action with no template at all, from `?fake-open-unset=<action>`.
 *
 * *Not configured* is the state every action is in off macOS, and macOS is the
 * only platform anyone has run knobas on — so without this the one screen spec
 * #491 describes for Windows and Linux could not be looked at by anybody. It
 * is the same shape as `?fake-clones-root`: a QA pass putting a screen into a
 * state the default fixture is not in.
 */
let fakeOpenUnset: string | null = null;

function seedOpenState(params: URLSearchParams): void {
  const fails = params.get("fake-open-fails");
  if (fails !== null) fakeOpenFails = fails.trim() === "" ? null : fails.trim();
  const unset = params.get("fake-open-unset");
  if (unset !== null) fakeOpenUnset = unset.trim() === "" ? null : unset.trim();
}

/** `checkout_commands` — the stored template, else the platform's default. */
function fakeCommands() {
  return FAKE_COMMANDS.map((command) => {
    const stored = fakeTemplates[command.action];
    const platform = command.action === fakeOpenUnset ? null : command.default;
    return {
      action: command.action,
      label: command.label,
      template: stored ?? platform,
      is_default: stored === undefined,
    };
  });
}

/**
 * `set_checkout_command`, refusals included.
 *
 * A **second** implementation of the template rule, deliberately narrow: the
 * rule is `knobas_core::checkout::expand` and is asserted there over every
 * shape. What this owes is the one refusal a QA pass has to be able to see —
 * a placeholder that is not `{path}`, named — plus the empty-clears-it
 * behaviour the Reset button depends on.
 */
function fakeSetCommand(action: string, template: unknown) {
  const known = FAKE_COMMANDS.find((command) => command.action === action);
  if (!known) {
    throw {
      code: "invalid",
      message: `'${action}' is not something knobas opens a checkout with`,
      source_id: null,
    };
  }
  const value = typeof template === "string" ? template.trim() : "";
  if (value === "") {
    delete fakeTemplates[action];
    return fakeCommands();
  }
  const other = [...value.matchAll(/\{([^}]*)\}/g)].map((match) => match[1]).find((n) => n !== "path");
  if (other !== undefined) {
    throw {
      code: "invalid",
      message:
        `'${value}' cannot be run: {${other}} is not something knobas fills in: ` +
        "the only value a command knobas runs may take is {path}, the checkout on this disk",
      source_id: null,
    };
  }
  if (!value.includes("{path}")) {
    throw {
      code: "invalid",
      message: `'${value}' cannot be run: the command has no {path}, so it would open nothing`,
      source_id: null,
    };
  }
  fakeTemplates[action] = value;
  return fakeCommands();
}

/**
 * `open_checkout`. Starts nothing — there is no process in a browser — and
 * refuses for the action `?fake-open-fails` names, in the shape the real
 * command refuses a program that is not installed.
 */
function fakeOpen(action: string) {
  const known = fakeCommands().find((command) => command.action === action);
  if (!known || known.template === null) {
    throw {
      code: "invalid",
      message: `${known?.label ?? action} is not configured on this platform`,
      source_id: null,
    };
  }
  if (action === fakeOpenFails) {
    throw {
      code: "invalid",
      message: `'${known.template}' could not be run: No such file or directory (os error 2)`,
      source_id: null,
    };
  }
  return null;
}

/** The repos a `clone_repo` started and nobody has stopped. */
const fakeCloning = new Set<string>();

/** This feature's commands. */
export function handlers(): Record<string, Handler> {
  return {
    // Checkouts (#499). The fixture has no filesystem, so what stands in for
    // the scan is `FAKE_CHECKOUTS` -- one repo with a path and one without --
    // and the override is a variable this file keeps. That is enough for the
    // panel, which branches on `found_by` and on whether `path` is there;
    // whether a directory really holds a clone is `knobas-core`'s question and
    // is answered by its own tests over a real temporary tree.
    clones_root: () => fakeClonesRoot,
    set_clones_root: (args) => {
      const path = args["path"];
      fakeClonesRoot = typeof path === "string" && path.trim() !== "" ? path.trim() : null;
      return null;
    },
    entity_checkout: (args) => fakeCheckout(String(args["entityId"] ?? "")),
    checkout_commands: () => fakeCommands(),
    set_checkout_command: (args) =>
      fakeSetCommand(String(args["action"] ?? ""), args["template"]),
    open_checkout: (args) => fakeOpen(String(args["action"] ?? "")),
    entity_checkout_diff: (args) => fakeCheckoutDiff(String(args["entityId"] ?? "")),
    // Clone (#846; `clone_repo`, #823). The answers are real, the progress is
    // not: a handler cannot emit `clone:progress`, so a started clone stays
    // `cloning` until it is stopped, and the screen's bar, its done state and
    // its failure causes are witnessed by `clone_ipc.rs` and the component
    // tests, not by this fixture. A repo the scan already found is `adopted`,
    // as in the backend; a folder is `<clones root>/<repo name>`.
    clone_repo: (args) => {
      const id = String(args["entityId"] ?? "");
      const entry = CORPUS.find((candidate) => candidate.entity_id === id);
      if (!entry || entry.kind !== "repo") {
        throw { code: "invalid", message: `${id} is not a repository in the mirror`, source_id: null };
      }
      if (fakeClonesRoot === null) {
        throw { code: "invalid", message: "set a clones root first", source_id: null };
      }
      const path = `${fakeClonesRoot}/${entry.title.split("/").pop()}`;
      if (args["stop"] === true) {
        const was = fakeCloning.delete(id);
        return was ? { status: "stopped", path, removed: true } : { status: "not_cloning" };
      }
      if (fakeCheckout(id).path !== null) return { status: "adopted", path };
      fakeCloning.add(id);
      return { status: "cloning", path };
    },
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
}
