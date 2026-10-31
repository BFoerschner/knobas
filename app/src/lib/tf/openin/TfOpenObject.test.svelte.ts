/**
 * *Open in… ▾* on a code object (#2274): the menu offers the applications
 * that accept the object and *Browser*, runs the one picked on the object,
 * and hands *Browser*'s page to the opener. *Diff tool* (#2275) is offered for
 * a range or a pull request whose two shas the checkout holds, and a pull
 * request's applications open its worktree (#2285).
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import type { CheckoutSettings, CheckoutView, OpenObject } from "../../ipc/entity";
import { toasts } from "../../shell/toasts.svelte";
import { createLayers } from "../layers.svelte";
import { createApps, type Apps } from "./apps.svelte";
import TfOpenObject from "./TfOpenObject.svelte";

const REPO = "gitea:tidewater/payout-service";
const WEB = "https://git.tidewater.example/tidewater/payout-service";

function settings(): CheckoutSettings {
  return {
    clones_root: "/Users/mara/code",
    git: { status: "found", path: "/usr/bin/git" },
    default_application: "vscode",
    applications: [
      { id: "terminal", name: "Terminal", command: "open -a Terminal {path}", file_command: null },
      { id: "tig", name: "Tig", command: "tig -C {path}", file_command: null, commit_command: "tig -C {path} show {sha}" },
      {
        id: "vscode",
        name: "VS Code",
        command: "code {path}",
        file_command: "code --goto {file}:{line}",
        range_command: "code --diff {base} {head}",
      },
    ],
  };
}

function view(path: string | null): CheckoutView {
  return {
    repo_entity_id: REPO,
    repo_url: WEB,
    path,
    found_by: path === null ? "nothing" : "scan",
    clones_root: "/Users/mara/code",
    clone_command: null,
    tree: null,
  };
}

let target: HTMLElement;
let app: ReturnType<typeof mount> | null = null;
let apps: Apps;
let checkoutPath: string | null;
let pageOf: (object: OpenObject) => string | null;
const calls = {
  objects: [] as [string, string | null, OpenObject][],
  opened: [] as string[],
  pages: [] as OpenObject[],
  diffs: [] as [string, string][],
};
/** The shas this test's checkout holds; the Diff tool is offered for two of them. */
let local: string[];

beforeEach(() => {
  for (const list of Object.values(calls)) list.length = 0;
  toasts.items = [];
  checkoutPath = "/Users/mara/code/payout-service";
  local = [];
  pageOf = (object) => (object.kind === "commit" ? `${WEB}/commit/${object.sha}` : null);
  apps = createApps({
    checkoutSettings: () => Promise.resolve(settings()),
    setCheckoutApplications: () => Promise.resolve(settings()),
    openCheckout: () => Promise.reject(new Error("a code object never opens as a checkout")),
    openObject: (entityId, application, object) => {
      calls.objects.push([entityId, application, object]);
      return Promise.resolve(["tig", "-C", "/Users/mara/code/payout-service", "show", "c90d11e4a7b2"]);
    },
  });
  target = document.createElement("div");
  document.body.appendChild(target);
});

afterEach(() => {
  if (app) unmount(app);
  app = null;
  target.remove();
});

async function settle() {
  for (let round = 0; round < 4; round += 1) {
    for (let i = 0; i < 10; i += 1) await Promise.resolve();
    await new Promise((resolve) => setTimeout(resolve, 0));
    flushSync();
  }
}

const text = (el: Element | null | undefined) => el?.textContent?.replace(/\s+/g, " ").trim() ?? "";

const argvOf = (base: string, head: string) => ["git", "-C", "/Users/mara/code/payout-service", "difftool", "--dir-diff", "--gui", "--no-prompt", base, head, "--"];

async function openMenu(object: OpenObject, ends?: () => Promise<{ base: string; head: string } | null>) {
  app = mount(TfOpenObject, {
    target,
    props: {
      entityId: REPO,
      object,
      ...(ends ? { ends } : {}),
      apps,
      layers: createLayers(),
      ports: {
        entityCheckout: () => Promise.resolve(view(checkoutPath)),
        objectPage: (_entityId: string, asked: OpenObject) => {
          calls.pages.push(asked);
          return Promise.resolve(pageOf(asked));
        },
        openExternal: (url: string) => {
          calls.opened.push(url);
          return Promise.resolve();
        },
        diffTool: (_entityId: string, base: string, head: string) =>
          Promise.resolve(local.includes(base.replace(/\^$/, "")) && local.includes(head) ? argvOf(base, head) : null),
        openDiffTool: (_entityId: string, base: string, head: string) => {
          calls.diffs.push([base, head]);
          return Promise.resolve(argvOf(base, head));
        },
      },
    },
  });
  await settle();
  const face = target.querySelector<HTMLButtonElement>("button[aria-haspopup=menu]")!;
  expect(face.className).toContain("tf-ghost");
  face.click();
  await settle();
}

const items = () => [...target.querySelectorAll<HTMLButtonElement>("[role=menuitem]")];
const names = () => items().map((item) => text(item.querySelector("b")));

const COMMIT: OpenObject = { kind: "commit", sha: "c90d11e4a7b2" };

test("a commit row's menu lists only the applications that accept commits, then Browser", async () => {
  await openMenu(COMMIT);

  expect(names()).toEqual(["Tig", "Browser"]);
  expect(text(items()[0])).toContain("tig -C /Users/mara/code/payout-service show c90d11e4a7b2");
  expect(text(items()[1])).toContain(`${WEB}/commit/c90d11e4a7b2`);
});

test("an application runs on the object, and Browser opens the object's page", async () => {
  await openMenu(COMMIT);
  items()[0]!.click();
  await settle();
  expect(calls.objects).toEqual([[REPO, "tig", COMMIT]]);
  expect(toasts.items.map((toast) => toast.text)).toEqual([
    "Started Tig: tig -C /Users/mara/code/payout-service show c90d11e4a7b2",
  ]);

  target.querySelector<HTMLButtonElement>("button[aria-haspopup=menu]")!.click();
  await settle();
  items().find((item) => text(item.querySelector("b")) === "Browser")!.click();
  await settle();
  expect(calls.opened).toEqual([`${WEB}/commit/c90d11e4a7b2`]);
});

test("a range offers what accepts ranges, the default first, and a file what accepts files", async () => {
  await openMenu({ kind: "range", base: "7be0e4f15c60^", head: "c90d11e4a7b2" });
  expect(names()).toEqual(["VS Code (default)"]);
  expect(text(items()[0])).toContain("code --diff 7be0e4f15c60^ c90d11e4a7b2");
  unmount(app!);
  target.replaceChildren();

  pageOf = (object) => (object.kind === "file" ? `${WEB}/src/c90d11e4a7b2/src/sepa/retry.rs#L42` : null);
  await openMenu({ kind: "file", path: "src/sepa/retry.rs", line: 42, at: "c90d11e4a7b2" });
  // Every application accepts a file: one without a file command opens it with its command.
  expect(names()).toEqual(["VS Code (default)", "Terminal", "Tig", "Browser"]);
  expect(text(items()[0])).toContain("code --goto /Users/mara/code/payout-service/src/sepa/retry.rs:42");
  items()[3]!.click();
  await settle();
  expect(calls.opened).toEqual([`${WEB}/src/c90d11e4a7b2/src/sepa/retry.rs#L42`]);
});

test("a repository not on this machine still opens in the Browser", async () => {
  checkoutPath = null;
  pageOf = (object) => (object.kind === "pull_request" ? `${WEB}/pulls/${object.number}` : null);
  const pr: OpenObject = { kind: "pull_request", number: 12, base: "main", head: "feature/x", branch: "feature/x" };
  await openMenu(pr);
  expect(names()).toEqual(["Browser"]);
  expect(calls.pages).toEqual([pr]);
  items()[0]!.click();
  await settle();
  expect(calls.opened).toEqual([`${WEB}/pulls/12`]);
});

test("with nothing to open it here, the menu says why", async () => {
  checkoutPath = null;
  await openMenu({ kind: "range", base: "main", head: "feature/x" });
  expect(items()).toEqual([]);
  expect(text(target.querySelector(".tf-appmenu"))).toContain("not on this machine");
});

test("a range whose two shas the checkout holds offers the Diff tool, and it runs on them", async () => {
  local = ["7be0e4f15c60", "c90d11e4a7b2"];
  await openMenu({ kind: "range", base: "7be0e4f15c60^", head: "c90d11e4a7b2" });
  expect(names()).toEqual(["VS Code (default)", "Diff tool"]);
  expect(text(items()[1])).toContain("git -C /Users/mara/code/payout-service difftool --dir-diff --gui --no-prompt 7be0e4f15c60^ c90d11e4a7b2");
  items()[1]!.click();
  await settle();
  expect(calls.diffs).toEqual([["7be0e4f15c60^", "c90d11e4a7b2"]]);
  expect(toasts.items.map((toast) => toast.text)[0]).toMatch(/^Started the Diff tool: git -C/);
});

test("a range with a sha the checkout lacks has no Diff tool", async () => {
  local = ["c90d11e4a7b2"];
  await openMenu({ kind: "range", base: "7be0e4f15c60^", head: "c90d11e4a7b2" });
  expect(names()).toEqual(["VS Code (default)"]);
});

test("a pull request's applications open its worktree, and its Diff tool takes the files' two shas", async () => {
  local = ["4b1c0ffee000", "9e2dfeed1111"];
  const withPr = settings();
  withPr.applications[2]!.pull_request_command = "code {path}";
  apps = createApps({
    checkoutSettings: () => Promise.resolve(withPr),
    setCheckoutApplications: () => Promise.resolve(withPr),
    openCheckout: () => Promise.reject(new Error("never")),
    openObject: () => Promise.resolve([]),
  });
  const pr: OpenObject = { kind: "pull_request", number: 12, base: "main", head: "feature/x", branch: "feature/x", item: "gitea:tidewater/payout-service#12" };
  await openMenu(pr, () => Promise.resolve({ base: "4b1c0ffee000", head: "9e2dfeed1111" }));
  expect(names()).toEqual(["VS Code (default)", "Diff tool"]);
  expect(text(items()[0])).toContain("code /Users/mara/code/pull-requests/tidewater/payout-service-12");
  items()[1]!.click();
  await settle();
  expect(calls.diffs).toEqual([["4b1c0ffee000", "9e2dfeed1111"]]);
});

test("a pull request whose files are not read yet has no Diff tool", async () => {
  local = ["4b1c0ffee000", "9e2dfeed1111"];
  await openMenu({ kind: "pull_request", number: 12, base: "main", head: "feature/x", branch: "feature/x" }, () => Promise.resolve(null));
  expect(names()).toEqual([]);
});
