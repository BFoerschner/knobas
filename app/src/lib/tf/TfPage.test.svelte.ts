/**
 * Keep-alive in the page host (#2247): `TfPage.svelte` keeps the pages you
 * leave mounted and hidden, up to *Pages kept open*, keyed by address, and
 * tells each whether it is the one shown.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import { createKeptPagesSetting, type KeptPagesSetting } from "./kept-pages.svelte";
import { definePage, type TfPageContext, type TfPageEntry } from "./page-entry";
import type { RoleSyncs } from "./role-sync.svelte";
import { parseTfHash, type Section, type TfRoute } from "./route";
import Fixture, { type PageLog } from "./TfPage.fixture.svelte";
import TfPage from "./TfPage.svelte";

type FullRoute = Extract<TfRoute, { view: "full" }>;

let log: PageLog;
let target: HTMLElement;
let host: ReturnType<typeof mount> | null = null;
let setting: KeptPagesSetting;
const state = $state<{ route: TfRoute }>({ route: parseTfHash("#/inbox") });
/** Moves a role's revision, as a run of *Sync now* ending does. */
const roleRevision = $state({ n: 0 });

/** A page per section, each one page wherever it is in that section. */
function sectionPage(section: Section, role?: string): TfPageEntry {
  return definePage({
    matches: (route): route is FullRoute => route.view === "full" && route.section === section,
    component: Fixture,
    ...(role === undefined ? {} : { role }),
    props: ({ visible }) => ({ name: section, visible, log }),
  });
}

/** The repository page: one component, one page per repository. */
const repoPage = definePage({
  matches: (route): route is FullRoute => route.view === "full" && route.head === "repo",
  component: Fixture,
  address: (route) => route.rest[0] ?? "",
  props: ({ route, visible }) => ({ name: `repo ${route.rest[0]}`, visible, log }),
});

const PAGES = [
  repoPage,
  sectionPage("inbox"),
  sectionPage("builds", "ci"),
  sectionPage("docs"),
  sectionPage("deps"),
  sectionPage("monitors"),
  sectionPage("sets"),
  sectionPage("review"),
  sectionPage("time"),
];

const SIX = ["#/inbox", "#/builds", "#/docs", "#/deps", "#/monitors", "#/sets"];

beforeEach(() => {
  log = { mounts: [], reads: [] };
  setting = createKeptPagesSetting();
  state.route = parseTfHash("#/inbox");
  target = document.createElement("div");
  document.body.append(target);
  const ctx = {
    get route() {
      return state.route;
    },
    roleSyncs: {
      of: () => ({
        get revision() {
          return roleRevision.n;
        },
      }),
    } as unknown as RoleSyncs,
  } as unknown as Omit<TfPageContext, "visible">;
  host = mount(TfPage, { target, props: { ctx, pages: PAGES, kept: setting } });
  flushSync();
});

afterEach(() => {
  if (host) unmount(host);
  host = null;
  target.remove();
});

function go(hash: string) {
  state.route = parseTfHash(hash);
  flushSync();
}

/** The pages mounted, by name, in the DOM's order. */
const held = () => [...target.querySelectorAll<HTMLElement>(".fixture-page")].map((page) => page.dataset["page"]);
/** The page whose wrapper is not hidden. */
const shownPages = () =>
  [...target.querySelectorAll<HTMLElement>(".fixture-page")].filter((page) => page.closest("[hidden]") === null).map((page) => page.dataset["page"]);
const pageEl = (name: string) => target.querySelector<HTMLElement>(`[data-page="${name}"]`)!;
const count = (list: string[], name: string) => list.filter((one) => one === name).length;

test("visiting six pages and going back to the second shows it without remounting or reading again", () => {
  for (const hash of SIX) go(hash);
  const reads = log.reads.length;
  go("#/builds");
  expect(count(log.mounts, "builds"), "no new instance").toBe(1);
  expect(log.reads.length, "no initial read: it is the instance it was").toBe(reads + 1 /* the role view's re-read on return */);
  expect(shownPages()).toEqual(["builds"]);
  expect(held()).toHaveLength(6);
});

test("the first page visited is dropped past the cap and remounts when revisited", () => {
  for (const hash of [...SIX, "#/review"]) go(hash);
  expect(held()).not.toContain("inbox");
  go("#/inbox");
  expect(count(log.mounts, "inbox")).toBe(2);
  expect(held(), "the least recently used went in its place").not.toContain("builds");
});

test("with Pages kept open at 0 every page is mounted afresh, as before keep-alive", () => {
  setting.set(0);
  flushSync();
  for (const hash of SIX) go(hash);
  go("#/builds");
  expect(held()).toEqual(["builds"]);
  expect(count(log.mounts, "builds")).toBe(2);
  expect(count(log.mounts, "inbox")).toBe(1);
});

test("lowering Pages kept open drops the hidden pages down to it at once", () => {
  for (const hash of SIX) go(hash);
  expect(held()).toHaveLength(6);
  setting.set(2);
  flushSync();
  expect(held()?.sort()).toEqual(["deps", "monitors", "sets"]);
  expect(shownPages()).toEqual(["sets"]);
});

test("a hidden page has visible false, the shown one true, inside it too", () => {
  go("#/builds");
  expect(pageEl("inbox").dataset["visible"]).toBe("false");
  expect(pageEl("inbox").dataset["shown"]).toBe("false");
  expect(pageEl("builds").dataset["visible"]).toBe("true");
  expect(pageEl("builds").dataset["shown"]).toBe("true");
  go("#/inbox");
  expect(pageEl("inbox").dataset["visible"]).toBe("true");
  expect(pageEl("builds").dataset["visible"]).toBe("false");
});

test("two addresses drawing the same component are two kept pages", () => {
  go("#/repo/one");
  go("#/repo/two");
  expect(held()?.sort()).toEqual(["inbox", "repo one", "repo two"]);
  go("#/repo/one/commits");
  expect(count(log.mounts, "repo one"), "a tab of the same repository is that page").toBe(1);
  expect(shownPages()).toEqual(["repo one"]);
});

test("a role view shown again re-reads, and one never left does not", () => {
  go("#/builds");
  expect(count(log.reads, "builds")).toBe(1);
  go("#/inbox");
  go("#/builds");
  expect(count(log.reads, "builds")).toBe(2);
  go("#/builds/pipeline");
  expect(count(log.reads, "builds"), "another address of the shown page is no return").toBe(2);
});

test("an address with no page hides every kept page", () => {
  go("#/builds");
  go("#/nowhere");
  expect(shownPages()).toEqual([]);
  expect(held()?.sort()).toEqual(["builds", "inbox"]);
  expect(target.textContent).toContain("Nothing lives at");
});
