/**
 * The diff address's page (#2271): it reads the repository's compare between
 * the address's two ends and draws it, and as a kept page of its own it is
 * there again, as it was left, when *Back* or *Forward* returns to it.
 */
import { flushSync, mount, tick, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { DiffFile, EntityDetail, SourceView } from "../../ipc/entity";
import { createKeptPagesSetting } from "../kept-pages.svelte";
import { definePage, type TfPageContext } from "../page-entry";
import { parseTfHash, type TfRoute } from "../route";
import Fixture, { type PageLog } from "../TfPage.fixture.svelte";
import TfPage from "../TfPage.svelte";
import { diffHash } from "./address";
import { page as diffPage } from "./page";
import TfDiffPage, { type DiffPagePorts } from "./TfDiffPage.svelte";

const bridge = vi.hoisted(() => ({
  entityView: vi.fn<(id: string, section: string | null) => Promise<SourceView | null>>(),
  getEntity: vi.fn<(id: string) => Promise<EntityDetail>>(),
}));
vi.mock("../../ipc/entity", async (original) => ({
  ...(await original<typeof import("../../ipc/entity")>()),
  entityView: bridge.entityView,
  getEntity: bridge.getEntity,
}));

const REPO = "gitea:tidewater/payout-service";
const BASE = "97cb5a8667f8aac8e6cdaeff83ffec9574f56a4b";
const HEAD = "1262a5b610480702cfed4a1f3f76c87bf68fae55";

function file(path: string): DiffFile {
  return {
    path,
    old_path: null,
    change: "added",
    additions: 1,
    deletions: 0,
    hunks: [{ header: "@@ -0,0 +1 @@", lines: [{ kind: "add", old: null, new: 1, text: `in ${path}` }] }],
    truncated: false,
    threads: [],
  };
}

function diffView(paths: string[]): SourceView {
  return {
    sections: [
      {
        id: "x",
        title: "Diff",
        count: paths.length,
        deferred: false,
        blocks: [{ type: "diff", files: paths.map(file), truncated: false, commentable: null, gone: [] }],
      },
    ],
    actions: [],
  } as unknown as SourceView;
}

/** Each compare section answers the files named for it. */
const ANSWERS: Record<string, string[]> = {
  [`compare:${BASE}^...${HEAD}`]: ["docs/backoff.md", "src/sepa/retry.rs", "tests/sepa_retry.rs"],
  [`compare:main...${HEAD}`]: ["docs/backoff.md"],
};

function answer(id: string, section: string | null): Promise<SourceView | null> {
  if (id !== REPO || section === null || !(section in ANSWERS)) return Promise.reject(new Error(`no ${id} ${section}`));
  return Promise.resolve(diffView(ANSWERS[section]!));
}

const detail = { row: { title: "tidewater/payout-service" } } as EntityDetail;

let target: HTMLElement;
let view: ReturnType<typeof mount> | null = null;

beforeEach(() => {
  bridge.entityView.mockReset().mockImplementation(answer);
  bridge.getEntity.mockReset().mockResolvedValue(detail);
  target = document.createElement("div");
  document.body.append(target);
});
afterEach(() => {
  if (view) unmount(view);
  view = null;
  target.remove();
});

async function settle() {
  for (let n = 0; n < 5; n++) await tick();
  flushSync();
}

const headers = (root: ParentNode) =>
  [...root.querySelectorAll(".tf-fh")].filter((h) => !h.closest("[hidden]")).map((h) => h.textContent!.split(" ")[0]);

test("the address reads the repository's compare between its two ends and draws that diff", async () => {
  const ports: DiffPagePorts = { entityView: vi.fn(answer), getEntity: async () => detail };
  view = mount(TfDiffPage, { target, props: { repoId: REPO, base: `${BASE}^`, head: HEAD, ports } });
  await settle();
  expect(ports.entityView).toHaveBeenCalledWith(REPO, `compare:${BASE}^...${HEAD}`);
  expect(headers(target)).toEqual(["docs/backoff.md", "src/sepa/retry.rs", "tests/sepa_retry.rs"]);
  expect(target.querySelector("[data-testid=diff-range]")!.textContent).toBe("97cb5a8667^ … 1262a5b610");
  expect(target.querySelector("h2 a")!.textContent).toBe("tidewater/payout-service");
});

test("a failed read says why instead of drawing no changes", async () => {
  const ports: DiffPagePorts = { entityView: () => Promise.reject(new Error("gitea: 404")), getEntity: async () => detail };
  view = mount(TfDiffPage, { target, props: { repoId: REPO, base: "nope", head: HEAD, ports } });
  await settle();
  expect(target.textContent).toContain("gitea: 404");
  expect(target.querySelector(".tf-empty")).toBeNull();
});

test("Back and Forward return to the diff address: the same page, drawn as it was left", async () => {
  const state = $state<{ route: TfRoute }>({ route: parseTfHash("#/inbox") });
  const log: PageLog = { mounts: [], reads: [] };
  const inbox = definePage({
    matches: (route): route is Extract<TfRoute, { view: "full" }> => route.view === "full" && route.section === "inbox",
    component: Fixture,
    props: ({ visible }) => ({ name: "inbox", visible, log }),
  });
  const ctx = {
    get route() {
      return state.route;
    },
    ready: true,
  } as unknown as Omit<TfPageContext, "visible">;
  view = mount(TfPage, { target, props: { ctx, pages: [diffPage, inbox], kept: createKeptPagesSetting() } });
  const range = diffHash(REPO, `${BASE}^`, HEAD);
  const go = async (hash: string) => {
    state.route = parseTfHash(hash);
    await settle();
  };

  await go(range);
  expect(headers(target)).toEqual(["docs/backoff.md", "src/sepa/retry.rs", "tests/sepa_retry.rs"]);
  const drawn = target.querySelector(".tf-diffpage");

  // Somewhere else, then Back: the diff is the page left, not a new one.
  await go("#/inbox");
  expect(headers(target)).toEqual([]);
  bridge.entityView.mockImplementation(() => new Promise(() => {}));
  await go(range);
  expect(target.querySelector(".tf-diffpage")).toBe(drawn);
  expect(headers(target)).toEqual(["docs/backoff.md", "src/sepa/retry.rs", "tests/sepa_retry.rs"]);
  expect(bridge.getEntity).toHaveBeenCalledTimes(1);

  // Another diff is another page; Back and Forward move between the two.
  bridge.entityView.mockImplementation(answer);
  const other = diffHash(REPO, "main", HEAD);
  await go(other);
  expect(headers(target)).toEqual(["docs/backoff.md"]);
  await go(range);
  expect(target.querySelector(".tf-diffpage:not([hidden] *)")).toBe(drawn);
  expect(headers(target)).toEqual(["docs/backoff.md", "src/sepa/retry.rs", "tests/sepa_retry.rs"]);
  await go(other);
  expect(headers(target)).toEqual(["docs/backoff.md"]);
});
