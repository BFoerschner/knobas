/**
 * The top strip's sync cluster — spec §2's *"sync monograms with 401
 * highlighted"*.
 *
 * 44 px is not enough room for a list of sources and their states, so the
 * cluster is a row of monograms whose dots carry the state and whose `title`
 * carries the words. What is pinned here is that the dot means what the rest
 * of the app means by it, that a 401 is called out rather than being one red
 * dot among several, and that clicking the cluster lands where the fix is.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import type { AuthState, CredentialHealth } from "../ipc/sources";
import TopStrip from "./TopStrip.svelte";
import { createHealth } from "./health.svelte";
import { createRouter } from "./router.svelte";

function row(source_id: string, state: AuthState): CredentialHealth {
  return {
    source_id,
    state,
    checked_at: "2026-08-25T11:50:00Z",
    detail: null,
    secret_expires_at: null,
  };
}

let target: HTMLDivElement;
let app: Record<string, unknown> | undefined;

function render(states: CredentialHealth[]) {
  const health = createHealth({
    credentialHealth: () => Promise.resolve([]),
    listen: () => Promise.resolve(() => {}),
  });
  for (const entry of states) health.patch(entry);
  const router = createRouter();
  app = mount(TopStrip, { target, props: { router, onsearch: () => {}, health } });
  flushSync();
  return { health, router };
}

function monograms() {
  return [...target.querySelectorAll<HTMLElement>(".sync .mg")];
}

beforeEach(() => {
  location.hash = "#/ctx/all";
  target = document.createElement("div");
  document.body.append(target);
});

afterEach(() => {
  if (app) unmount(app);
  app = undefined;
  target.remove();
});

test("one monogram per configured source, in a stable order", () => {
  render([row("teamcity", "ok"), row("gitea", "ok"), row("jira", "ok")]);
  // By source id, so the cluster does not reshuffle itself every time a
  // health event arrives.
  expect(monograms().map((m) => m.textContent)).toEqual(["GI", "JI", "TE"]);
});

test("an unauthorized source paints its monogram as failed and shows the code", () => {
  render([row("jira", "unauthorized"), row("gitea", "ok")]);
  const jira = monograms().find((m) => m.getAttribute("aria-label")?.includes("jira"))!;
  expect(jira.className).toContain("err");
  expect(monograms().find((m) => m.getAttribute("aria-label")?.includes("gitea"))!.className)
    .not.toContain("err");
  // The mockup's `.err-txt`: a 401 is the one state a person has to act on
  // themselves, and one red dot among five is not a call to action.
  expect(target.querySelector(".sync .err-txt")?.textContent).toContain("401");
});

test("an unreachable source is marked, but is not called a 401", () => {
  render([row("jira", "unreachable")]);
  expect(monograms()[0]!.className).toContain("err");
  // A network fault is not a rejected credential, and telling a reader to go
  // rotate a token that was never the problem wastes their afternoon.
  expect(target.querySelector(".sync .err-txt")).toBeNull();
});

test("unknown is not a fault — it is what every source reads before its first check", () => {
  render([row("jira", "unknown"), row("gitea", "ok")]);
  expect(monograms().every((m) => !m.className.includes("err"))).toBe(true);
  expect(target.querySelector(".sync .err-txt")).toBeNull();
});

test("the cluster's tooltip is where the full list fits", () => {
  render([row("jira", "unauthorized"), row("gitea", "ok")]);
  const title = target.querySelector<HTMLElement>(".sync")!.getAttribute("title") ?? "";
  expect(title).toContain("jira");
  expect(title).toContain("gitea");
  expect(title).toMatch(/rejected|unauthorized/i);
});

test("clicking the cluster goes to the sources view, where the fix is", () => {
  const { router } = render([row("jira", "unauthorized")]);
  target.querySelector<HTMLButtonElement>(".sync")!.click();
  flushSync();
  expect(router.route.view).toBe("sources");
});

test("no sources means no cluster at all, not an empty box", () => {
  render([]);
  expect(target.querySelector(".sync")).toBeNull();
});

test("a health event repaints the cluster without a remount", () => {
  const { health } = render([row("jira", "ok")]);
  expect(monograms()[0]!.className).not.toContain("err");
  health.patch(row("jira", "unauthorized"));
  flushSync();
  expect(monograms()[0]!.className).toContain("err");
  expect(target.querySelector(".sync .err-txt")).toBeTruthy();
});

/** A strip button by its accessible name. */
function tool(label: string) {
  return target.querySelector<HTMLButtonElement>(`button[aria-label="${label}"]`);
}

/**
 * Settings is reached the way sources is: a labelled button on the strip.
 *
 * §14's backup surface (#69) had nowhere to be reached from — the strip's only
 * destination was `#/sources`. This is the whole of the navigation that ticket
 * adds; there is no settings router and no tab strip behind it.
 */
test("the strip has a way into settings, and it goes to #/settings", () => {
  const { router } = render([]);

  const settings = tool("Settings")!;
  expect(settings).toBeTruthy();
  // Two destinations, not one relabelled: sources has not moved.
  expect(tool("Sources")).toBeTruthy();

  settings.click();
  flushSync();
  expect(location.hash).toBe("#/settings");
  expect(router.route).toEqual({ view: "settings" });
});

/** Which of the two is current, so the strip says where the reader is. */
test("the strip marks the surface the reader is actually on", () => {
  location.hash = "#/settings";
  render([]);

  expect(tool("Settings")!.getAttribute("aria-current")).toBe("page");
  expect(tool("Sources")!.getAttribute("aria-current")).toBeNull();
});
