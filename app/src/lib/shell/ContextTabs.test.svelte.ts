/**
 * The switcher navigates; it does not remember.
 *
 * The one behaviour worth pinning is that choosing a room *changes the
 * address*. A component-local `selected` would look identical on screen and
 * lose the room on every reload, which is precisely the mistake spec §2's
 * "the address is the state" exists to prevent.
 */
import { flushSync, mount, unmount } from "svelte";
import { expect, test } from "vitest";

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
 * The one control that cannot work says so. A `+ new` that silently did
 * nothing would be indistinguishable from a bug.
 */
test("+ new is disabled and names the milestone it arrives in", () => {
  const screen = render("#/ctx/all");
  const create = screen.target.querySelector<HTMLButtonElement>(".tab.new");

  expect(create?.disabled).toBe(true);
  expect(create?.title).toMatch(/M2/);

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
