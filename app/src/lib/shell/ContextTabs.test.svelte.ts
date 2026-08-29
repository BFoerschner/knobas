/**
 * The switcher navigates; it does not remember.
 *
 * The one behaviour worth pinning is that choosing a room *changes the
 * address*. A component-local `selected` would look identical on screen and
 * lose the room on every reload, which is precisely the mistake spec §2's
 * "the address is the state" exists to prevent.
 */
import { flushSync, mount, unmount } from "svelte";
import { expect, test, vi } from "vitest";

/** The labels `create_context` was asked to mint. */
const created: string[] = [];

vi.mock("../ipc/entity", () => ({
  listContexts: () => Promise.resolve([]),
  contextMembers: () => Promise.resolve([]),
  promoteContext: () => Promise.reject(new Error("no promotion in this test")),
  createContext: (title: string) => {
    created.push(title);
    return Promise.resolve({
      id: "ctx:fresh",
      kind: "adhoc",
      title,
      anchor_id: null,
      created_at: "2026-08-29T12:00:00Z",
      archived_at: null,
    });
  },
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: () => Promise.resolve(() => {}),
}));

import ContextTabs from "./ContextTabs.svelte";
import { builtinContexts } from "./contexts";
import { createRouter } from "./router.svelte";

const CONTEXTS = builtinContexts([
  { id: "jira", label: "Tidewater Jira" },
  { id: "gitea", label: "Gitea" },
]);

function render(hash: string) {
  location.hash = hash;
  const router = createRouter();
  const stop = router.start();
  const target = document.createElement("div");
  document.body.append(target);
  const app = mount(ContextTabs, { target, props: { router, contexts: CONTEXTS } });
  // Mount effects have not run at the point `mount` returns, so an assertion
  // before this one holds against a component that does nothing at all.
  flushSync();
  return {
    target,
    router,
    tabs: () => [...target.querySelectorAll<HTMLButtonElement>(".tabs .tab")],
    tab: (label: string) =>
      [...target.querySelectorAll<HTMLButtonElement>(".tabs .tab")].find(
        (button) => button.textContent?.trim() === label,
      ),
    done: () => {
      unmount(app);
      stop();
      target.remove();
    },
  };
}

test("exactly one tab is current, and it is the room in the address", () => {
  const screen = render("#/ctx/src:gitea");

  const on = screen.tabs().filter((tab) => tab.classList.contains("on"));
  expect(on.map((tab) => tab.textContent?.trim())).toEqual(["Gitea"]);
  expect(on[0]?.getAttribute("aria-current")).toBe("page");

  screen.done();
});

test("choosing a room navigates rather than setting a local variable", () => {
  const screen = render("#/ctx/all");

  screen.tab("Tidewater Jira")?.click();
  flushSync();

  expect(location.hash).toBe("#/ctx/src:jira");
  expect(screen.router.ctx).toBe("src:jira");
  expect(
    screen.tabs().filter((tab) => tab.classList.contains("on")).map((t) => t.textContent?.trim()),
  ).toEqual(["Tidewater Jira"]);

  screen.done();
});

/**
 * A detail address is *over* a room, so its tab stays lit — the reader has not
 * left the room, they have opened something in it.
 */
test("a detail address keeps its room's tab current", () => {
  const screen = render("#/ctx/src:jira");
  screen.router.go("#/ticket/jira:PAY-231");
  flushSync();

  expect(
    screen.tabs().filter((tab) => tab.classList.contains("on")).map((t) => t.textContent?.trim()),
  ).toEqual(["Tidewater Jira"]);

  screen.done();
});

/** ...and a view that is not a room lights none of them. */
test("the sources view is in no room, so no tab is current", () => {
  const screen = render("#/sources");
  expect(screen.tabs().filter((tab) => tab.classList.contains("on"))).toEqual([]);
  screen.done();
});

/**
 * `+ new` mints an ad-hoc context and **navigates** (#47) — the address is
 * the state here too, so the proof is the hash, not a callback.
 */
test("+ new becomes an input, creates on Enter and lands in the new room", async () => {
  const screen = render("#/ctx/all");
  screen.target.querySelector<HTMLButtonElement>(".tab.new")?.click();
  flushSync();

  const input = screen.target.querySelector<HTMLInputElement>(".tab.new-name");
  expect(input, "the tab becomes the input").not.toBeNull();
  input!.value = "Staging DB configuration";
  input!.dispatchEvent(new Event("input", { bubbles: true }));
  input!.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));

  await vi.waitFor(() => expect(location.hash).toBe("#/ctx/ctx:fresh"));
  expect(created).toEqual(["Staging DB configuration"]);
  screen.done();
});

/** Escape backs out without minting anything. */
test("Escape abandons the label and nothing is created", () => {
  const before = created.length;
  const screen = render("#/ctx/all");
  screen.target.querySelector<HTMLButtonElement>(".tab.new")?.click();
  flushSync();

  const input = screen.target.querySelector<HTMLInputElement>(".tab.new-name");
  input!.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
  flushSync();

  expect(screen.target.querySelector(".tab.new-name")).toBeNull();
  expect(screen.target.querySelector<HTMLButtonElement>(".tab.new")).not.toBeNull();
  expect(created.length).toBe(before);
  screen.done();
});

test("the popover lists every room and closes when one is chosen", () => {
  const screen = render("#/ctx/all");
  expect(screen.target.querySelector(".pop")).toBeNull();

  screen.target.querySelector<HTMLButtonElement>(".ctx-name")?.click();
  flushSync();
  const items = [...screen.target.querySelectorAll<HTMLButtonElement>(".pop .it")];
  expect(items.map((item) => item.querySelector("span")?.textContent)).toEqual([
    "All work",
    "Tidewater Jira",
    "Gitea",
  ]);

  items[2]?.click();
  flushSync();
  expect(location.hash).toBe("#/ctx/src:gitea");
  expect(screen.target.querySelector(".pop")).toBeNull();

  screen.done();
});

/**
 * Rung 1 of the Esc ladder (`keys.ts`): the popover closes and the key goes no
 * further, so one press does not also unwind whatever is open behind it.
 */
test("Escape closes the popover without reaching the shell", () => {
  const screen = render("#/ctx/all");
  screen.target.querySelector<HTMLButtonElement>(".ctx-name")?.click();
  flushSync();

  let reachedWindow = false;
  const spy = () => (reachedWindow = true);
  window.addEventListener("keydown", spy);
  screen.target
    .querySelector(".pop")
    ?.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
  flushSync();
  window.removeEventListener("keydown", spy);

  expect(screen.target.querySelector(".pop")).toBeNull();
  expect(reachedWindow, "the shell's global handler must not see this key").toBe(false);

  screen.done();
});

/** The current room's name is what the switcher shows. */
test("the switcher names the room the reader is in", () => {
  const screen = render("#/ctx/src:gitea");
  expect(screen.target.querySelector(".ctx-name .nm")?.textContent).toBe("Gitea");
  screen.done();
});
