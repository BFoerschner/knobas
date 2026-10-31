/** Settings › Checkouts and Open in… (#844, R-13a.3) over `checkout_settings`. */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import type { CheckoutApplicationDraft, CheckoutSettings, PullRequestWorktree } from "../../ipc/entity";
import { reset, toasts } from "../../shell/toasts.svelte";
import { createLayers } from "../layers.svelte";
import { createApps } from "../openin/apps.svelte";
import TfCheckoutsSection from "./TfCheckoutsSection.svelte";

let target: HTMLElement;
let view: ReturnType<typeof mount> | null = null;
let stored: CheckoutSettings;
let saved: [CheckoutApplicationDraft[], string | null][];
let roots: (string | null)[];
let worktrees: PullRequestWorktree[];
let refuseRemove: boolean;

beforeEach(() => {
  saved = [];
  roots = [];
  worktrees = [];
  refuseRemove = false;
  stored = {
    clones_root: "/Users/mara/code",
    git: { status: "found", path: "/usr/bin/git" },
    default_application: "vscode",
    applications: [
      { id: "vscode", name: "VS Code", command: "code {path}", file_command: "code --goto {file}:{line}" },
      { id: "fork", name: "Fork", command: "open -a Fork {path}", file_command: null },
    ],
  };
  target = document.createElement("div");
  document.body.appendChild(target);
});

afterEach(() => {
  if (view) unmount(view);
  view = null;
  target.remove();
});

async function settle() {
  for (let round = 0; round < 3; round += 1) {
    for (let i = 0; i < 10; i += 1) await Promise.resolve();
    flushSync();
  }
}

const text = (el: Element | null | undefined) => el?.textContent?.replace(/\s+/g, " ").trim() ?? "";

async function draw(refuse?: (path: string | null) => Promise<void>) {
  const apps = createApps({
    checkoutSettings: () => Promise.resolve(stored),
    setCheckoutApplications: (drafts, defaultApplication) => {
      saved.push([drafts, defaultApplication]);
      stored = { ...stored, default_application: defaultApplication };
      return Promise.resolve(stored);
    },
    openCheckout: () => Promise.resolve([]),
  });
  view = mount(TfCheckoutsSection, {
    target,
    props: {
      apps,
      layers: createLayers(),
      worktreePorts: {
        list: () => Promise.resolve(worktrees),
        remove: (path: string) => {
          if (refuseRemove) {
            return Promise.reject({ code: "conflict", message: `git would not remove ${path}: it contains modified or untracked files` });
          }
          worktrees = worktrees.filter((tree) => tree.path !== path);
          return Promise.resolve(worktrees);
        },
      },
      saveRoot: (path) => {
        if (refuse) return refuse(path);
        roots.push(path);
        stored = { ...stored, clones_root: path };
        return Promise.resolve();
      },
    },
  });
  await settle();
}

test("the section draws the clones root, the git found and the applications, the default marked", async () => {
  await draw();
  expect(text(target.querySelector("h1"))).toBe("Checkouts and Open in…");
  const rows = [...target.querySelectorAll(".tf-stg-row")].map(text);
  expect(rows[0]).toContain("Clones root");
  expect(rows[0]).toContain("/Users/mara/code");
  expect(rows[1]).toContain("git at /usr/bin/git");
  const apps = [...target.querySelectorAll("tbody tr")].map((tr) => [...tr.querySelectorAll("td")].map(text));
  expect(apps).toEqual([
    ["VS Code default", "code {path}", "code --goto {file}:{line}", ""],
    ["Fork", "open -a Fork {path}", "opens the checkout", "Make default"],
  ]);
});

test("Make default saves the list with the new default, and Change… sets the clones root", async () => {
  await draw();
  [...target.querySelectorAll<HTMLButtonElement>("button")].find((b) => text(b) === "Make default")!.click();
  await settle();
  expect(saved.at(-1)![1]).toBe("fork");
  expect(text(target.querySelector("tbody tr:nth-child(2) td"))).toBe("Fork default");

  [...target.querySelectorAll<HTMLButtonElement>("button")].find((b) => text(b) === "Change…")!.click();
  flushSync();
  const field = target.querySelector<HTMLInputElement>("input[aria-label='Clones root']")!;
  field.value = "~/src";
  field.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
  [...target.querySelectorAll<HTMLButtonElement>("button")].find((b) => text(b) === "Save")!.click();
  await settle();
  expect(roots).toEqual(["~/src"]);
  expect(text(target.querySelector(".tf-stg-row"))).toContain("~/src");
});

test("the section says the default is listed first and runs on Enter, and Make default keeps the row order (#1443)", async () => {
  await draw();
  expect(text(target.querySelector(".tf-stg-p"))).toContain("The default is listed first in Open in… and runs on Enter.");
  expect(text(target.querySelector(".tf-stg-p"))).not.toContain("The first one is the default");
  [...target.querySelectorAll<HTMLButtonElement>("button")].find((b) => text(b) === "Make default")!.click();
  await settle();
  expect([...target.querySelectorAll("tbody tr")].map((tr) => text(tr.querySelector("td")))).toEqual(["VS Code", "Fork default"]);
});

const inMono = (el: Element | null | undefined) => el?.closest(".tf-mono") != null;
const mark = (root: ParentNode, needle: string) =>
  [...root.querySelectorAll("*")].filter((el) => text(el) === needle && el.children.length === 0).at(-1) ?? null;

test("a set root and real commands draw in mono; Not set and opens the checkout are sans labels (#1444)", async () => {
  await draw();
  expect(inMono(mark(target, "/Users/mara/code"))).toBe(true);
  expect(inMono(mark(target, "code {path}"))).toBe(true);
  expect(inMono(mark(target, "open -a Fork {path}"))).toBe(true);
  expect(inMono(mark(target, "opens the checkout"))).toBe(false);
});

test("with no clones root, Not set is not in the mono face (#1444)", async () => {
  stored = { ...stored, clones_root: null };
  await draw();
  const notSet = mark(target, "Not set");
  expect(notSet).not.toBeNull();
  expect(inMono(notSet)).toBe(false);
});

test("a clones root the setter refuses is said on the page, and the field stays open with what was typed (#1661)", async () => {
  reset();
  await draw(() =>
    Promise.reject({ code: "invalid", message: 'the clones root must be an absolute path (or start with ~/), not "Code"', source_id: null }),
  );
  [...target.querySelectorAll<HTMLButtonElement>("button")].find((b) => text(b) === "Change…")!.click();
  flushSync();
  const field = target.querySelector<HTMLInputElement>("input[aria-label='Clones root']")!;
  field.value = "Code";
  field.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
  [...target.querySelectorAll<HTMLButtonElement>("button")].find((b) => text(b) === "Save")!.click();
  await settle();
  expect(toasts.items.map((t) => t.text)).toEqual([expect.stringContaining("must be an absolute path")]);
  expect(target.querySelector<HTMLInputElement>("input[aria-label='Clones root']")?.value).toBe("Code");
  expect(roots).toEqual([]);
});

test("the pull request worktrees are listed, a closed one marked, and Remove removes one", async () => {
  const at = (n: number) => `/Users/mara/code/pull-requests/tidewater/payout-service-${n}`;
  worktrees = [
    { repo_entity_id: "gitea:tidewater/payout-service", number: 7, path: at(7), closed: true },
    { repo_entity_id: "gitea:tidewater/payout-service", number: 8, path: at(8), closed: false },
  ];
  await draw();
  const section = [...target.querySelectorAll(".tf-stg-sec")].find((sec) => text(sec.querySelector("h2")) === "Pull request worktrees")!;
  const rows = () => [...section.querySelectorAll("tbody tr")].map((row) => [...row.querySelectorAll("td")].map(text).join(" | "));
  expect(rows()).toEqual([`#7 closed | ${at(7)} | Remove`, `#8 | ${at(8)} | Remove`]);

  section.querySelector<HTMLButtonElement>(`button[title="Remove ${at(7)}"]`)!.click();
  await settle();
  expect(rows()).toEqual([`#8 | ${at(8)} | Remove`]);

  refuseRemove = true;
  section.querySelector<HTMLButtonElement>(`button[title="Remove ${at(8)}"]`)!.click();
  await settle();
  expect(rows()).toEqual([`#8 | ${at(8)} | Remove`]);
  expect(toasts.items.map((toast) => toast.text)).toContain(
    `Could not remove the worktree: git would not remove ${at(8)}: it contains modified or untracked files`,
  );
});
