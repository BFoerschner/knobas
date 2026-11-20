/** What the bar and the shell's keys do to a site (ADR-0048, decisions §8, §10). */
import { expect, test } from "vitest";

import type { SiteAsked, SiteStatus } from "../../ipc/site";
import { answerAnyway, answerKeep, closeSite, reloadSite, siteBack, siteForward, type SiteActionPorts } from "./actions";
import { createSiteStore } from "./site.svelte";

const DONE: SiteAsked = { done: true, asks: null };
const ASKS: SiteAsked = { done: false, asks: "unsaved" };

function world(answer: SiteAsked, over: Partial<SiteStatus> = {}) {
  const calls: string[] = [];
  const store = createSiteStore();
  store.update({
    source: "wiki",
    phase: "live",
    url: "https://wiki.example/x",
    title: "x",
    own: true,
    loading: false,
    can_back: false,
    can_forward: false,
    shown: true,
    keyboard: "site",
    asks_origin: null,
    failure: null,
    esc_again_ms: 500,
    ...over,
  });
  const ports: SiteActionPorts = {
    close: (source, force) => (calls.push(`close ${source} ${force}`), Promise.resolve(force ? DONE : answer)),
    act: (source, act, force) => (calls.push(`${act} ${source} ${force}`), Promise.resolve(force || act !== "reload" ? DONE : answer)),
    open: (source, url, force) => (calls.push(`open ${source} ${url} ${force}`), Promise.resolve(DONE)),
  };
  return { calls, store, ports };
}

test("closing a clean page closes it and asks nothing", async () => {
  const w = world(DONE);
  await closeSite("wiki", w.ports, w.store);
  expect(w.calls).toEqual(["close wiki false"]);
  expect(w.store.asking.size).toBe(0);
});

test("closing a page with changes asks in the bar and gives knobas the keyboard; anyway closes it", async () => {
  const w = world(ASKS);
  await closeSite("wiki", w.ports, w.store);
  expect(w.store.asking.get("wiki")).toEqual({ kind: "close" });
  expect(w.calls).toEqual(["close wiki false", "focus-shell wiki false"]);
  await answerAnyway("wiki", w.ports, w.store);
  expect(w.calls.at(-1)).toBe("close wiki true");
  expect(w.store.asking.size).toBe(0);
});

test("keeping the page gives the site the keyboard back and throws nothing away", async () => {
  const w = world(ASKS);
  await reloadSite("wiki", w.ports, w.store);
  expect(w.store.asking.get("wiki")).toEqual({ kind: "reload" });
  answerKeep("wiki", w.ports, w.store);
  expect(w.store.asking.size).toBe(0);
  expect(w.calls).toEqual(["reload wiki false", "focus-shell wiki false", "focus wiki false"]);
});

test("reload while a load runs is stop", async () => {
  const w = world(DONE, { loading: true });
  await reloadSite("wiki", w.ports, w.store);
  expect(w.calls).toEqual(["stop wiki false"]);
});

test("another address, answered anyway, opens it here with force", async () => {
  const w = world(DONE);
  w.store.ask("wiki", { kind: "address", url: "https://wiki.example/y", name: null });
  await answerAnyway("wiki", w.ports, w.store);
  expect(w.calls).toEqual(["open wiki https://wiki.example/y true"]);
});

test("back and forward in the site only where it has somewhere to go; knobas's otherwise", () => {
  const none = world(DONE);
  expect(siteBack("wiki", none.ports, none.store)).toBe(false);
  expect(siteForward("wiki", none.ports, none.store)).toBe(false);
  expect(none.calls).toEqual([]);
  const both = world(DONE, { can_back: true, can_forward: true });
  expect(siteBack("wiki", both.ports, both.store)).toBe(true);
  expect(siteForward("wiki", both.ports, both.store)).toBe(true);
  expect(both.calls).toEqual(["back wiki false", "forward wiki false"]);
});

test("a site that is not open closes nothing", async () => {
  const w = world(DONE);
  await closeSite("jira", w.ports, w.store);
  expect(w.calls).toEqual([]);
});
