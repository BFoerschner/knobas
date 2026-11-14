/**
 * *Open in… ▾* and *Clone* (#844, #846) over the IPC mirror: the menu, its
 * keys and what it runs; the clone's progress, *Stop*, done and failed, as
 * `clone:progress` reports them; *Browser*, and the links to the application
 * page (which `TfAppPage.test.svelte.ts` drives).
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import type {
  CheckoutApplicationDraft,
  CheckoutSettings,
  CheckoutView,
  CloneAnswer,
  CloneEvent,
  OpenAt,
} from "../../ipc/entity";
import { toasts } from "../../shell/toasts.svelte";
import { createLayers } from "../layers.svelte";
import { createApps, type Apps } from "./apps.svelte";
import { createClones, type Clones } from "./clones.svelte";
import TfOpenIn from "./TfOpenIn.svelte";

const REPO = "gitea:tidewater/payout-service";

function settings(): CheckoutSettings {
  return {
    clones_root: "/Users/mara/code",
    git: { status: "found", path: "/usr/bin/git" },
    default_application: "vscode",
    applications: [
      { id: "terminal", name: "Terminal", command: "open -a Terminal {path}", file_command: null },
      { id: "vscode", name: "VS Code", command: "code {path}", file_command: "code --goto {file}:{line}" },
    ],
  };
}

function view(path: string | null, root: string | null = "/Users/mara/code"): CheckoutView {
  return {
    repo_entity_id: REPO,
    repo_url: "https://git.tidewater.example/tidewater/payout-service",
    path,
    found_by: path === null ? "nothing" : "scan",
    clones_root: root,
    clone_command: null,
    tree: path === null ? null : { status: "read", branch: "main", head: null, upstream: null, ahead: 0, behind: 0, changed: 0, untracked: 0, conflicted: 0 },
  };
}

let target: HTMLElement;
let app: ReturnType<typeof mount> | null = null;
let stored: CheckoutSettings;
const calls = {
  open: [] as [string, OpenAt][],
  saved: [] as [CheckoutApplicationDraft[], string | null][],
  clone: [] as [string, boolean | undefined][],
  roots: [] as string[],
};
let emit: (event: CloneEvent) => void = () => {};
let cloneAnswer: CloneAnswer = { status: "cloning", path: "/Users/mara/code/payout-service" };
const PAGE = "https://git.tidewater.example/tidewater/payout-service";
let browsed: string[] = [];
let browse: (url: string) => Promise<void> = (url) => {
  browsed.push(url);
  return Promise.resolve();
};
let apps: Apps;
let clones: Clones;

beforeEach(() => {
  stored = settings();
  for (const list of Object.values(calls)) list.length = 0;
  toasts.items = [];
  browsed = [];
  browse = (url) => {
    browsed.push(url);
    return Promise.resolve();
  };
  cloneAnswer = { status: "cloning", path: "/Users/mara/code/payout-service" };
  apps = createApps({
    checkoutSettings: () => Promise.resolve(stored),
    setCheckoutApplications: (drafts, defaultApplication) => {
      calls.saved.push([drafts, defaultApplication]);
      stored = {
        ...stored,
        default_application: defaultApplication ?? stored.default_application,
        applications: drafts.map((draft) => ({ ...draft, id: draft.id ?? draft.name.toLowerCase() })),
      };
      return Promise.resolve(stored);
    },
    openCheckout: (entityId, at) => {
      calls.open.push([entityId, at]);
      return Promise.resolve(["code", "/Users/mara/code/payout-service"]);
    },
  });
  clones = createClones(
    {
      cloneRepo: (entityId, stop) => {
        calls.clone.push([entityId, stop]);
        return Promise.resolve(stop ? { status: "stopped", path: "/Users/mara/code/payout-service", removed: true } : cloneAnswer);
      },
      entityCheckout: () => Promise.resolve(view("/Users/mara/code/payout-service")),
      listen: (_event, handler) => {
        emit = (payload) => handler({ payload });
        return Promise.resolve(() => {});
      },
    },
    apps,
  );
  target = document.createElement("div");
  document.body.appendChild(target);
});

afterEach(() => {
  if (app) unmount(app);
  app = null;
  target.remove();
  document.querySelectorAll(".tf-modal").forEach((node) => node.remove());
});

async function settle() {
  for (let round = 0; round < 4; round += 1) {
    for (let i = 0; i < 10; i += 1) await Promise.resolve();
    await new Promise((resolve) => setTimeout(resolve, 0));
    flushSync();
  }
}

const text = (el: Element | null | undefined) => el?.textContent?.replace(/\s+/g, " ").trim() ?? "";
const layers = createLayers();

async function draw(checkout: CheckoutView, file: string | null = null, page?: string | null) {
  app = mount(TfOpenIn, {
    target,
    props: {
      entityId: REPO,
      title: "tidewater/payout-service",
      checkout,
      file,
      ...(page !== undefined ? { page } : {}),
      readPage: () => Promise.resolve(PAGE),
      browse: (url: string) => browse(url),
      apps,
      clones,
      layers,
      saveRoot: (path: string) => {
        calls.roots.push(path);
        return Promise.resolve();
      },
    },
  });
  await settle();
}

function button(name: string, root: ParentNode = document): HTMLButtonElement {
  const found = [...root.querySelectorAll<HTMLButtonElement>("button")].find((b) => text(b) === name);
  if (!found) throw new Error(`no button ${name}`);
  return found;
}

const items = () => [...document.querySelectorAll<HTMLElement>("[role='menuitem']")];

// -- Open in… (#844, R-8.22) -------------------------------------------------

test("the whole button opens the menu: the default first, marked and focused, then Browser, then Add and Manage", async () => {
  await draw(view("/Users/mara/code/payout-service"));
  const face = target.querySelector<HTMLButtonElement>("button[aria-haspopup='menu']")!;
  expect(text(face)).toBe("Open in…");
  face.click();
  await settle();
  expect(items().map(text)).toEqual([
    "VS Code (default)code /Users/mara/code/payout-service",
    "Terminalopen -a Terminal /Users/mara/code/payout-service",
    `Browser${PAGE}`,
    "Add an application",
    "Manage applications",
  ]);
  // The two links go to Settings: the application page with this repository to try it on, and the list.
  expect(items().slice(3).map((item) => item.getAttribute("href"))).toEqual([
    "#/settings/checkouts/add/gitea%3Atidewater%2Fpayout-service",
    "#/settings/checkouts",
  ]);
  expect(document.activeElement).toBe(items()[0]);
  expect(layers.open).toHaveLength(1);

  // Arrows move and wrap; Esc closes and gives the focus back.
  const menu = target.querySelector("[role='menu']")!;
  menu.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowDown", bubbles: true }));
  expect(document.activeElement).toBe(items()[1]);
  menu.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowUp", bubbles: true }));
  menu.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowUp", bubbles: true }));
  expect(document.activeElement).toBe(items()[4]);
  menu.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
  flushSync();
  expect(target.querySelector("[role='menu']")).toBeNull();
  expect(document.activeElement).toBe(face);
  expect(layers.open).toHaveLength(0);
});

test("choosing an application runs it at the checkout and says what it started", async () => {
  await draw(view("/Users/mara/code/payout-service"));
  target.querySelector<HTMLButtonElement>("button[aria-haspopup='menu']")!.click();
  await settle();
  items()[0]!.click();
  await settle();
  expect(calls.open).toEqual([[REPO, { application: "vscode", file: null, line: null }]]);
  expect(toasts.items.map((toast) => toast.text)).toContain("Started VS Code: code /Users/mara/code/payout-service");
  expect(target.querySelector("[role='menu']")).toBeNull();
});

test("at a file, the menu shows and runs the file command", async () => {
  await draw(view("/Users/mara/code/payout-service"), "src/sepa/retry.rs");
  target.querySelector<HTMLButtonElement>("button[aria-haspopup='menu']")!.click();
  await settle();
  expect(text(items()[0])).toBe("VS Code (default)code --goto /Users/mara/code/payout-service/src/sepa/retry.rs:1");
  items()[0]!.click();
  await settle();
  expect(calls.open).toEqual([[REPO, { application: "vscode", file: "src/sepa/retry.rs", line: null }]]);
});

test("nothing is drawn before the checkout is read", async () => {
  app = mount(TfOpenIn, { target, props: { entityId: REPO, title: "tidewater/payout-service", checkout: undefined, apps, clones, layers } });
  await settle();
  expect(target.querySelector("button")).toBeNull();
});

// -- Clone (#846, R-8.25) -----------------------------------------------------

test("Clone names the folder, reads Cloning… N % with Stop while it runs, and Stop says nothing was left", async () => {
  await draw(view(null));
  const clone = button("Clone", target);
  expect(clone.getAttribute("title")).toBe("Clones into /Users/mara/code/payout-service");
  clone.click();
  await settle();
  expect(calls.clone).toEqual([[REPO, undefined]]);
  expect(text(target.querySelector("button[disabled]"))).toBe("Cloning… 0 %");

  emit({ entity_id: REPO, path: "/Users/mara/code/payout-service", state: "progress", phase: "Receiving objects", percent: 52 });
  flushSync();
  expect(text(target.querySelector("button[disabled]"))).toBe("Cloning… 52 %");

  target.querySelector<HTMLButtonElement>("button[aria-label='Stop cloning payout-service']")!.click();
  await settle();
  expect(calls.clone.at(-1)).toEqual([REPO, true]);
  expect(toasts.items.map((toast) => toast.text)).toContain("Stopped cloning payout-service. Nothing was left in /Users/mara/code.");
  expect(button("Clone", target)).toBeTruthy();
});

test("a finished clone says where and on which branch, offers the default application, and moves `finished`", async () => {
  await draw(view(null));
  button("Clone", target).click();
  await settle();
  const before = clones.finished;
  emit({ entity_id: REPO, path: "/Users/mara/code/payout-service", state: "done" });
  await settle();
  expect(clones.finished).toBe(before + 1);
  const toast = toasts.items.find((item) => item.text.startsWith("Cloned"))!;
  expect(toast.text).toBe("Cloned payout-service into /Users/mara/code/payout-service, on main");
  expect(toast.action?.label).toBe("Open in VS Code");
  toast.action!.run();
  await settle();
  expect(calls.open).toEqual([[REPO, { application: "vscode" }]]);
});

test("a failed clone is said in place with what to do, and Try again starts it again", async () => {
  await draw(view(null));
  button("Clone", target).click();
  await settle();
  emit({ entity_id: REPO, path: "/Users/mara/code/payout-service", state: "failed", reason: "fatal: Authentication failed", cause: "auth", removed: true });
  flushSync();
  expect(text(target.querySelector("[role='alert']"))).toBe(
    "git could not sign in to git.tidewater.example. knobas uses your own git credentials (ssh agent or credential helper); check them in a terminal",
  );
  button("Try again", target).click();
  await settle();
  expect(calls.clone).toHaveLength(2);
});

test("a folder that exists and is no clone of it is refused in place, in the backend's words", async () => {
  const refusing = createClones(
    {
      cloneRepo: () =>
        Promise.reject({ code: "conflict", message: "/Users/mara/code/payout-service exists and is not a clone of this repository. Rename it, or set a path by hand", source_id: null }),
      entityCheckout: () => Promise.resolve(view(null)),
      listen: () => Promise.resolve(() => {}),
    },
    apps,
  );
  app = mount(TfOpenIn, { target, props: { entityId: REPO, title: "tidewater/payout-service", checkout: view(null), apps, clones: refusing, layers } });
  await settle();
  button("Clone", target).click();
  await settle();
  expect(text(target.querySelector("[role='alert']"))).toBe(
    "/Users/mara/code/payout-service exists and is not a clone of this repository. Rename it, or set a path by hand",
  );
});

test("with no clones root, Clone asks where clones go, prefilled ~/code, saves it and clones", async () => {
  await draw(view(null, null));
  const clone = button("Clone", target);
  expect(clone.getAttribute("title")).toBe("Clones into your clones root/payout-service");
  clone.click();
  await settle();
  const dialog = document.querySelector("[role='dialog']")!;
  expect(text(dialog.querySelector("h2"))).toBe("Where should clones go?");
  expect(dialog.querySelector<HTMLInputElement>("#tf-crt")!.value).toBe("~/code");
  button("Set and clone", dialog).click();
  await settle();
  expect(calls.roots).toEqual(["~/code"]);
  expect(calls.clone).toEqual([[REPO, undefined]]);
  expect(document.querySelector("[role='dialog']")).toBeNull();
});

async function setAndClone(root = "~/code") {
  await draw(view(null, null));
  button("Clone", target).click();
  await settle();
  const dialog = document.querySelector("[role='dialog']")!;
  const field = dialog.querySelector<HTMLInputElement>("#tf-crt")!;
  field.value = root;
  field.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
  button("Set and clone", dialog).click();
  await settle();
}

test("after Set and clone, Clone after Stop starts again without asking for the root (#1442)", async () => {
  await setAndClone("/Users/mara/src");
  expect(calls.clone).toHaveLength(1);
  emit({ entity_id: REPO, path: "/Users/mara/src/payout-service", state: "progress", phase: "receiving", percent: 40 });
  flushSync();
  button("Stop", target).click();
  await settle();
  expect(button("Clone", target).getAttribute("title")).toBe("Clones into /Users/mara/src/payout-service");
  button("Clone", target).click();
  await settle();
  expect(document.querySelector("[role='dialog']")).toBeNull();
  expect(calls.clone.filter(([, stop]) => !stop)).toHaveLength(2);
  expect(calls.roots).toEqual(["/Users/mara/src"]);
});

test("after Set and clone, Try again after a failure starts without asking for the root (#1442)", async () => {
  await setAndClone();
  emit({ entity_id: REPO, path: "/Users/mara/code/payout-service", state: "failed", reason: "boom", cause: "other", removed: true });
  flushSync();
  button("Try again", target).click();
  await settle();
  expect(document.querySelector("[role='dialog']")).toBeNull();
  expect(calls.clone.filter(([, stop]) => !stop)).toHaveLength(2);
});

test("Stop after Set and clone names the root just entered, not the clone folder (#1442)", async () => {
  await setAndClone("/Users/mara/src");
  button("Stop", target).click();
  await settle();
  expect(toasts.items.map((toast) => toast.text)).toContain(
    "Stopped cloning payout-service. Nothing was left in /Users/mara/src.",
  );
});

test("with no root, Cancel in the root dialog starts nothing (#1442)", async () => {
  await draw(view(null, null));
  button("Clone", target).click();
  await settle();
  button("Cancel", document.querySelector("[role='dialog']")!).click();
  await settle();
  expect(document.querySelector("[role='dialog']")).toBeNull();
  expect(calls.clone).toEqual([]);
  expect(calls.roots).toEqual([]);
});

// -- Browser (Björn, 2026-11-13: *Open on Gitea* took the app's window to Gitea) --

test("at a file, Browser opens the file's page in the browser and the window stays where it is", async () => {
  const at = location.href;
  await draw(view("/Users/mara/code/payout-service"), "src/sepa/retry.rs", `${PAGE}/src/branch/main/src/sepa/retry.rs`);
  target.querySelector<HTMLButtonElement>("button[aria-haspopup='menu']")!.click();
  await settle();
  const entry = items().find((item) => text(item).startsWith("Browser"))!;
  expect(entry.tagName).toBe("BUTTON");
  expect(items().map((item) => item.getAttribute("href")).at(-2)).toBe("#/settings/checkouts/add/gitea%3Atidewater%2Fpayout-service/file/src%2Fsepa%2Fretry.rs");
  entry.click();
  await settle();
  expect(browsed).toEqual([`${PAGE}/src/branch/main/src/sepa/retry.rs`]);
  expect(location.href).toBe(at);
  expect(document.querySelector("[role='menu']")).toBeNull();
});

test("a file with no page of its own offers no Browser: the repository's would be the wrong page", async () => {
  await draw(view("/Users/mara/code/payout-service"), "src/sepa/retry.rs", null);
  target.querySelector<HTMLButtonElement>("button[aria-haspopup='menu']")!.click();
  await settle();
  expect(items().some((item) => text(item).startsWith("Browser"))).toBe(false);
});

test("a browser that refuses says so in a toast", async () => {
  browse = () => Promise.reject(new Error("refused: ftp: is not http or https"));
  await draw(view("/Users/mara/code/payout-service"));
  target.querySelector<HTMLButtonElement>("button[aria-haspopup='menu']")!.click();
  await settle();
  items().find((item) => text(item).startsWith("Browser"))!.click();
  await settle();
  expect(toasts.items.map((toast) => [toast.text, toast.tone])).toContainEqual(["Could not open the browser: refused: ftp: is not http or https", "err"]);
});
