/**
 * The application page (PR #2479, 2026-11-13, in place of #844's dialog): the
 * fields and placeholders, *It will run* filled from a repository picked on
 * this disk, *Try it* (only a saved template runs, ADR-0016, #1643) and its
 * take-back on leaving, *Save*, *Remove* and the way back.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { CheckoutApplicationDraft, CheckoutSettings, EntityRow, OpenObject } from "../../ipc/entity";
import { toasts } from "../../shell/toasts.svelte";
import { pageContext } from "../kept-pages.svelte";
import { PRESS_HOLD_MS } from "../press-hold.svelte";
import { appPagePlace, LAST_TRIED_KEY } from "./app-page";
import { createApps, type Apps } from "./apps.svelte";
import { forgetAppDrafts } from "./app-drafts.svelte";
import TfAppPage from "./TfAppPage.svelte";
import type { TryReads } from "./try-reads";

const PAYOUT = "gitea:tidewater/payout-service";
const LEDGER = "gitea:tidewater/ledger";
const ROOT = "C:\\Users\\Björn Förschner\\src";

function settings(): CheckoutSettings {
  return {
    clones_root: ROOT,
    git: { status: "found", path: "/usr/bin/git" },
    default_application: "vscode",
    applications: [
      { id: "terminal", name: "Terminal", command: "wt -d {path}", file_command: null },
      { id: "vscode", name: "VS Code", command: "code.cmd {path}", file_command: "code.cmd --goto {file}:{line}" },
    ],
  };
}

const row = (entityId: string): EntityRow => ({ entity_id: entityId, title: entityId.slice("gitea:".length), source_id: "gitea" }) as EntityRow;

let target: HTMLElement;
let app: ReturnType<typeof mount> | null = null;
let stored: CheckoutSettings;
let saved: [CheckoutApplicationDraft[], string | null][];
let opened: [string, string, OpenObject][];
let left: number;
let shown: boolean;
let checkouts: Record<string, string | null>;
let holdSave: Promise<void> | null;
let apps: Apps;

const reads: TryReads = {
  repos: () => Promise.resolve([row(PAYOUT), row(LEDGER), row("gitea:tidewater/not-here")]),
  checkouts: (ids) => Promise.resolve(ids.map((entityId) => ({ entityId, path: checkouts[entityId] ?? null, clonesRoot: ROOT }))),
  files: () => Promise.resolve(["Cargo.toml", "README.md", "src/lib.rs"]),
  commits: () =>
    Promise.resolve([
      { sha: "c90d11e4a7b2", title: "Retry the payout once" },
      { sha: "7be0e4f15c60", title: "Read the ledger" },
    ]),
  prs: () => Promise.resolve([{ id: `${PAYOUT}#142`, number: 142, title: "Idempotent retry window" }]),
  prEnds: () => Promise.resolve({ base: "main", head: "feature/retry" }),
  page: () => Promise.resolve(null),
};

/** The window's storage, which this test environment has none of. */
function memoryStorage(): Storage {
  const items = new Map<string, string>();
  return {
    get length() {
      return items.size;
    },
    clear: () => items.clear(),
    getItem: (key) => items.get(key) ?? null,
    key: (index) => [...items.keys()][index] ?? null,
    removeItem: (key) => void items.delete(key),
    setItem: (key, value) => void items.set(key, String(value)),
  };
}

beforeEach(() => {
  vi.stubGlobal("localStorage", memoryStorage());
  stored = settings();
  saved = [];
  opened = [];
  left = 0;
  shown = true;
  holdSave = null;
  checkouts = { [PAYOUT]: `${ROOT}\\payout-service`, [LEDGER]: `${ROOT}\\ledger` };
  toasts.items = [];
  forgetAppDrafts();
  apps = createApps({
    checkoutSettings: () => Promise.resolve(stored),
    setCheckoutApplications: async (drafts, defaultApplication) => {
      saved.push([drafts, defaultApplication]);
      if (holdSave) await holdSave;
      stored = {
        ...stored,
        default_application: defaultApplication,
        applications: drafts.map((draft) => ({
          id: draft.id ?? draft.name.toLowerCase(),
          name: draft.name,
          command: draft.command,
          file_command: draft.file_command ?? null,
          ...(draft.commit_command ? { commit_command: draft.commit_command } : {}),
          ...(draft.pull_request_command ? { pull_request_command: draft.pull_request_command } : {}),
        })),
      };
      return stored;
    },
    openCheckout: () => Promise.resolve([]),
    openObject: (entityId, application, object) => {
      opened.push([entityId, application ?? "", object]);
      return Promise.resolve(["ran"]);
    },
  });
  target = document.createElement("div");
  document.body.appendChild(target);
});

afterEach(() => {
  if (app) unmount(app);
  app = null;
  target?.remove();
  vi.unstubAllGlobals();
});

async function settle() {
  for (let round = 0; round < 6; round += 1) {
    for (let i = 0; i < 10; i += 1) await Promise.resolve();
    await new Promise((resolve) => setTimeout(resolve, 0));
    flushSync();
  }
}

const text = (el: Element | null | undefined) => el?.textContent?.replace(/\s+/g, " ").trim() ?? "";

async function draw(address: string) {
  const at = appPagePlace(address.replace(/^#\/settings\//, "").split("/").map(decodeURIComponent))!;
  app = mount(TfAppPage, { target, props: { at, apps, reads, leave: () => (left += 1) }, context: pageContext(() => shown) });
  await settle();
}

function field(id: string): HTMLInputElement {
  return target.querySelector<HTMLInputElement>(`#tf-app-${id}`)!;
}

function type(id: string, value: string) {
  const input = field(id);
  input.value = value;
  input.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
}

function button(name: string): HTMLButtonElement {
  const found = [...target.querySelectorAll<HTMLButtonElement>("button")].find((b) => text(b) === name);
  if (!found) throw new Error(`no button ${name}`);
  return found;
}

/** The line under a command: what it will run, or what it waits for. */
function line(id: string): string {
  return text(field(id).closest(".tf-appcmd")!.querySelector(".tf-apprun code, .tf-apprun .tf-dim"));
}

function tryButton(id: string): HTMLButtonElement {
  return field(id).closest(".tf-appcmd")!.querySelector<HTMLButtonElement>(".tf-apprun button")!;
}

test("adding: placeholders insert, It will run fills the repository it came from, the warnings speak, and Add saves the whole list and goes back", async () => {
  await draw("#/settings/checkouts/add/gitea%3Atidewater%2Fpayout-service");
  expect(text(target.querySelector("h1"))).toBe("Add an application");
  expect(target.querySelector<HTMLSelectElement>("#tf-app-repo")!.value).toBe(PAYOUT);
  const add = button("Add application");
  expect(add.disabled).toBe(true);
  expect(text(target.querySelector(".tf-dwhy"))).toBe("Name it, and give its command.");

  type("name", "MyGitViewer");
  type("command", "MyGitViewer");
  expect([...target.querySelectorAll(".tf-banner[role='status']")].map(text)).toEqual([
    "The command has no {path}, so the program won’t know which repository to open.",
  ]);
  field("command").focus();
  field("command").dispatchEvent(new FocusEvent("focus"));
  button("{path} the checkout folder").click();
  await settle();
  expect(field("command").value).toBe("MyGitViewer {path}");
  // Filled as the backend runs it: the Windows checkout is one argument, in double quotes.
  expect(line("command")).toBe('MyGitViewer "C:\\Users\\Björn Förschner\\src\\payout-service"');
  expect(target.querySelectorAll(".tf-banner[role='status']")).toHaveLength(0);

  add.click();
  await settle();
  expect(saved).toHaveLength(1);
  expect(saved[0]![0].map((draft) => [draft.id, draft.name, draft.command])).toEqual([
    ["terminal", "Terminal", "wt -d {path}"],
    ["vscode", "VS Code", "code.cmd {path}"],
    [null, "MyGitViewer", "MyGitViewer {path}"],
  ]);
  expect(saved[0]![1]).toBe("vscode");
  expect(toasts.items.map((toast) => toast.text)).toContain("Added MyGitViewer. It is in every Open in… menu.");
  expect(left).toBe(1);
});

test("Try it saves the draft and runs it by the id the save gave it; pressed twice it runs once; a later try and Save write over that entry", async () => {
  await draw("#/settings/checkouts/add/gitea%3Atidewater%2Fpayout-service");
  type("command", "MyGitViewer {path}");
  expect(tryButton("command").disabled).toBe(true);
  type("name", "MyGitViewer");
  expect(tryButton("command").disabled).toBe(false);

  tryButton("command").click();
  await settle();
  // The same press again at once is the hold's, not a second run (the press rule).
  tryButton("command").click();
  await settle();
  expect(opened).toHaveLength(1);
  await new Promise((resolve) => setTimeout(resolve, PRESS_HOLD_MS + 50));
  type("command", "MyGitViewer --new {path}");
  tryButton("command").click();
  await settle();
  expect(saved.map(([drafts]) => drafts.map((draft) => draft.id))).toEqual([
    ["terminal", "vscode", null],
    ["terminal", "vscode", "mygitviewer"],
  ]);
  expect(opened).toEqual([
    [PAYOUT, "mygitviewer", { kind: "repository" }],
    [PAYOUT, "mygitviewer", { kind: "repository" }],
  ]);
  expect(toasts.items.map((toast) => toast.text)).toContain("Started MyGitViewer: ran");
  expect(localStorage.getItem(LAST_TRIED_KEY)).toBe(PAYOUT);

  button("Add application").click();
  await settle();
  expect(saved.at(-1)![0].map((draft) => [draft.id, draft.command])).toEqual([
    ["terminal", "wt -d {path}"],
    ["vscode", "code.cmd {path}"],
    ["mygitviewer", "MyGitViewer --new {path}"],
  ]);
  expect(saved).toHaveLength(3);
});

test("leaving after a try takes back only the tried entry: a default made meanwhile stays (#1643)", async () => {
  await draw("#/settings/checkouts/add/gitea%3Atidewater%2Fpayout-service");
  type("name", "MyGitViewer");
  type("command", "MyGitViewer {path}");
  tryButton("command").click();
  await settle();
  // Settings' *Make default* elsewhere, while the page stood.
  await apps.save(stored.applications, "terminal");
  unmount(app!);
  app = null;
  await settle();
  const [restored, defaultApplication] = saved.at(-1)!;
  expect(restored.map((draft) => draft.id)).toEqual(["terminal", "vscode"]);
  expect(defaultApplication).toBe("terminal");
});

test("the page hidden while a try is saving takes it back after the save, and starts nothing", async () => {
  let release!: () => void;
  holdSave = new Promise((resolve) => (release = resolve));
  await draw("#/settings/checkouts/add/gitea%3Atidewater%2Fpayout-service");
  type("name", "MyGitViewer");
  type("command", "MyGitViewer {path}");
  tryButton("command").click();
  await settle();
  // Leave: another address shows another page, and this one is hidden.
  button("Cancel").click();
  holdSave = null;
  release();
  await settle();
  expect(saved.map(([drafts]) => drafts.map((draft) => draft.id))).toEqual([
    ["terminal", "vscode", null],
    ["terminal", "vscode"],
  ]);
  expect(opened).toEqual([]);
  expect(left).toBe(1);
});

test("editing: Try it saves the edit over the entry, runs it by its id, and leaving puts the entry back as it was", async () => {
  await draw("#/settings/checkouts/app/terminal");
  expect(text(target.querySelector("h1"))).toBe("Terminal");
  expect(field("name").value).toBe("Terminal");
  type("command", "wt -d {path} --title {name}");
  expect(line("command")).toBe('wt -d "C:\\Users\\Björn Förschner\\src\\payout-service" --title payout-service');
  tryButton("command").click();
  await settle();
  expect(saved.at(-1)![0].map((draft) => [draft.id, draft.command])).toEqual([
    ["terminal", "wt -d {path} --title {name}"],
    ["vscode", "code.cmd {path}"],
  ]);
  expect(opened.at(-1)!.slice(0, 2)).toEqual([PAYOUT, "terminal"]);
  shown = false;
  // A kept page hidden by a move elsewhere: the effect reads `visible()` again on the next change it tracks.
  unmount(app!);
  app = null;
  await settle();
  expect(saved.at(-1)![0].map((draft) => [draft.id, draft.command])).toEqual([
    ["terminal", "wt -d {path}"],
    ["vscode", "code.cmd {path}"],
  ]);
});

test("words typed and left stay for the session; Cancel drops them", async () => {
  await draw("#/settings/checkouts/app/vscode");
  type("commit", "code.cmd --commit {sha}");
  expect(text(target.querySelector(".tf-stg-pend"))).toBe("Not saved");
  unmount(app!);
  app = null;
  await draw("#/settings/checkouts/app/vscode");
  expect(field("commit").value).toBe("code.cmd --commit {sha}");
  button("Cancel").click();
  unmount(app!);
  app = null;
  await draw("#/settings/checkouts/app/vscode");
  expect(field("commit").value).toBe("");
  expect(saved).toEqual([]);
});

test("Remove takes it out at once, with Undo on its toast, and goes back", async () => {
  await draw("#/settings/checkouts/app/vscode");
  button("Remove").click();
  await settle();
  expect(saved.at(-1)![0].map((draft) => draft.id)).toEqual(["terminal"]);
  const toast = toasts.items.find((item) => item.text === "Removed VS Code")!;
  expect(toast.action?.label).toBe("Undo");
  expect(left).toBe(1);
  toast.action!.run();
  await settle();
  expect(saved.at(-1)).toEqual([
    [
      { id: "terminal", name: "Terminal", command: "wt -d {path}", file_command: null, commit_command: null, range_command: null, pull_request_command: null },
      { id: "vscode", name: "VS Code", command: "code.cmd {path}", file_command: "code.cmd --goto {file}:{line}", commit_command: null, range_command: null, pull_request_command: null },
    ],
    "vscode",
  ]);
});

test("the repository is the one it came from, else the last one tried, else the first with a checkout; others aren't offered", async () => {
  localStorage.setItem(LAST_TRIED_KEY, LEDGER);
  await draw("#/settings/checkouts/add");
  const pick = target.querySelector<HTMLSelectElement>("#tf-app-repo")!;
  expect(pick.value).toBe(LEDGER);
  expect([...pick.options].map((option) => option.textContent)).toEqual(["tidewater/ledger", "tidewater/payout-service"]);
  unmount(app!);
  localStorage.removeItem(LAST_TRIED_KEY);
  await draw("#/settings/checkouts/add");
  expect(target.querySelector<HTMLSelectElement>("#tf-app-repo")!.value).toBe(PAYOUT);
});

test("with no checkout on this disk it says so, and how to get one", async () => {
  checkouts = {};
  await draw("#/settings/checkouts/add");
  expect(target.querySelector("#tf-app-repo")).toBeNull();
  expect(text(target.querySelector(".tf-stg-sec [role='status']"))).toContain("No repository has a checkout on this disk yet");
  expect(text(target.querySelector(".tf-stg-sec [role='status']"))).toContain("Clone");
  type("name", "Tig");
  type("command", "tig -C {path}");
  expect(line("command")).toBe("Pick a repository below to see it filled in.");
});

test("a commit from Open in… is the one tried; a range is it and the one before; a pull request runs in its worktree", async () => {
  await draw("#/settings/checkouts/add/gitea%3Atidewater%2Fpayout-service/commit/7be0e4f15c60");
  type("name", "Tig");
  type("command", "tig -C {path}");
  type("commit", "tig -C {path} show {sha}");
  await settle();
  expect(target.querySelector<HTMLSelectElement>("#tf-app-trycommit")!.value).toBe("7be0e4f15c60");
  expect(line("commit")).toBe('tig -C "C:\\Users\\Björn Förschner\\src\\payout-service" show 7be0e4f15c60');
  type("range", "git -C {path} difftool -d {base} {head}");
  // 7be0e4f15c60 is the oldest of the two, so it has no commit before it here.
  expect(line("range")).toBe("A range needs two commits; the repository has fewer.");
  const pick = target.querySelector<HTMLSelectElement>("#tf-app-trycommit")!;
  pick.value = "c90d11e4a7b2";
  pick.dispatchEvent(new Event("change", { bubbles: true }));
  flushSync();
  expect(line("range")).toBe('git -C "C:\\Users\\Björn Förschner\\src\\payout-service" difftool -d 7be0e4f15c60 c90d11e4a7b2');

  type("pr", "tig -C {path} log {base}..{head}");
  await settle();
  expect(line("pr")).toBe('tig -C "C:\\Users\\Björn Förschner\\src\\pull-requests\\tidewater\\payout-service-142" log main..feature/retry');
  tryButton("pr").click();
  await settle();
  expect(opened.at(-1)).toEqual([PAYOUT, "tig", { kind: "pull_request", number: 142, base: "main", head: "feature/retry", branch: "feature/retry", item: `${PAYOUT}#142` }]);
});

test("an application the list doesn't hold says so, with the way to the list", async () => {
  await draw("#/settings/checkouts/app/gone");
  expect(text(target)).toContain("Open in… has no such application.");
  expect(target.querySelector("#tf-app-name")).toBeNull();
});
