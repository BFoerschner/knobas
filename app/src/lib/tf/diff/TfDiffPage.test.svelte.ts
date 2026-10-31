/**
 * The diff address's page (#2271): it reads the repository's compare between
 * the address's two ends and draws it, and as a kept page of its own it is
 * there again, as it was left, when *Back* or *Forward* returns to it.
 */
import { flushSync, mount, tick, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { DiffFile, EntityDetail, RangeDiff, SourceView } from "../../ipc/entity";
import { createKeptPagesSetting } from "../kept-pages.svelte";
import { definePage, type TfPageContext } from "../page-entry";
import { parseTfHash, type TfRoute } from "../route";
import Fixture, { type PageLog } from "../TfPage.fixture.svelte";
import TfPage from "../TfPage.svelte";
import { diffHash } from "./address";
import { page as diffPage } from "./page";
import TfDiffPage, { type DiffPagePorts } from "./TfDiffPage.svelte";
import { installPipelines, noPipelines } from "./piped";

const bridge = vi.hoisted(() => ({
  entityView: vi.fn<(id: string, section: string | null) => Promise<SourceView | null>>(),
  getEntity: vi.fn<(id: string) => Promise<EntityDetail>>(),
}));
vi.mock("../../ipc/entity", async (original) => ({
  ...(await original<typeof import("../../ipc/entity")>()),
  entityView: bridge.entityView,
  // The range read as the source answers it: the compare section's diff block.
  entityRangeDiff: (id: string, base: string, head: string) =>
    bridge.entityView(id, `compare:${base}...${head}`).then((v) => fromSource(v)),
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

/** A compare section's diff block as `entity_range_diff` answers it from the source. */
function fromSource(v: SourceView | null): RangeDiff {
  const block = v?.sections[0]?.blocks.find((b) => b.type === "diff");
  return block?.type === "diff"
    ? { files: block.files, truncated: block.truncated, range: block.range ?? null, read_at: "source" }
    : { files: [], truncated: false, range: null, read_at: "source" };
}

/** Ports whose range read is the source's compare through `entityView`. */
function viaSource(entityView: DiffPagePorts["entityView"]): DiffPagePorts {
  return {
    entityView,
    getEntity: async () => detail,
    rangeDiff: (id, base, head) => entityView(id, `compare:${base}...${head}`).then(fromSource),
  };
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
  [...root.querySelectorAll(".tf-fh .tf-mono")].filter((h) => !h.closest("[hidden]")).map((h) => h.textContent!.split(" ")[0]);

test("the address reads the repository's compare between its two ends and draws that diff", async () => {
  const ports = viaSource(vi.fn(answer));
  view = mount(TfDiffPage, { target, props: { repoId: REPO, base: `${BASE}^`, head: HEAD, ports } });
  await settle();
  expect(ports.entityView).toHaveBeenCalledWith(REPO, `compare:${BASE}^...${HEAD}`);
  expect(headers(target)).toEqual(["docs/backoff.md", "src/sepa/retry.rs", "tests/sepa_retry.rs"]);
  expect(target.querySelector("[data-testid=diff-range]")!.textContent).toBe("97cb5a8667^ … 1262a5b610");
  expect(target.querySelector("h2 a")!.textContent).toBe("tidewater/payout-service");
});

test("a diff that names no range says its context does not expand, rather than offering nothing silently", async () => {
  const ports = viaSource(vi.fn(answer));
  view = mount(TfDiffPage, { target, props: { repoId: REPO, base: `${BASE}^`, head: HEAD, ports } });
  await settle();
  expect(target.querySelector("[data-testid=no-range]")?.textContent).toContain("Context does not expand here");
  expect(target.querySelector(".tf-exp")).toBeNull();
});

test("a failed read says why instead of drawing no changes", async () => {
  const ports = viaSource(() => Promise.reject(new Error("gitea: 404")));
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

test("a diff that names its range expands context read whole at its two shas (#2310)", async () => {
  const MODIFIED: DiffFile = {
    path: "src/retry.rs",
    old_path: null,
    change: "modified",
    additions: 1,
    deletions: 1,
    hunks: [
      { header: "@@ -1,1 +1,1 @@", lines: [{ kind: "del", old: 1, new: null, text: "a" }, { kind: "add", old: null, new: 1, text: "b" }] },
      { header: "@@ -10,1 +10,1 @@", lines: [{ kind: "context", old: 10, new: 10, text: "line 10" }] },
    ],
    truncated: false,
    threads: [],
  };
  const PARENT = "907900b245f2941018dfd3ee155b876ef701c102";
  const whole = ["b", ...Array.from({ length: 11 }, (_, n) => `line ${n + 2}`)].join("\n");
  const fileView = { sections: [{ id: "f", title: "f", count: null, deferred: false, blocks: [{ type: "file", trail: [], path: "src/retry.rs", text: whole, language: null, size: 1, truncated: false, binary: false, url: null, sha: null }] }], actions: [] } as unknown as SourceView;
  const asked: string[] = [];
  const ports = viaSource(async (id, section) => {
      asked.push(`${id} ${section}`);
      if (section?.startsWith("file@")) return fileView;
      return {
        sections: [{ id: "x", title: "Diff", count: 1, deferred: false, blocks: [{ type: "diff", files: [MODIFIED], truncated: false, commentable: null, gone: [], range: { repository: REPO, base: PARENT, head: HEAD, commits: [] } }] }],
        actions: [],
      } as unknown as SourceView;
  });
  view = mount(TfDiffPage, { target, props: { repoId: REPO, base: `${BASE}^`, head: HEAD, ports } });
  await settle();
  const expand = target.querySelector<HTMLButtonElement>(".tf-exp");
  expect(expand).not.toBeNull();
  expect(target.querySelector("[data-testid=no-range]")).toBeNull();
  expand!.click();
  await settle();
  expect(asked).toContain(`${REPO} file@${HEAD}:src/retry.rs`);
  expect(asked).toContain(`${REPO} file@${PARENT}:src/retry.rs`);
  expect(target.querySelector(".tf-diff")!.textContent!.replace(/\s+/g, " ")).toMatch(/22 line 2 .*99 line 9/);
});

test("a diff read at the checkout offers Ignore whitespace, and asks again with it", async () => {
  const asked: boolean[] = [];
  const ports: DiffPagePorts = {
    ...viaSource(answer),
    rangeDiff: async (_id, _base, _head, ignoreWhitespace) => {
      asked.push(ignoreWhitespace);
      // A reformat: whitespace ignored, nothing is left.
      return { files: ignoreWhitespace ? [] : [file("pay.py")], truncated: false, range: null, read_at: "checkout" };
    },
  };
  view = mount(TfDiffPage, { target, props: { repoId: REPO, base: BASE, head: HEAD, ports } });
  await settle();
  const box = target.querySelector<HTMLInputElement>(".tf-diffws input[type=checkbox]")!;
  expect(box.disabled).toBe(false);
  expect(target.querySelector("[data-testid=ws-why]")).toBeNull();
  expect(headers(target)).toEqual(["pay.py"]);

  box.click();
  await settle();
  expect(asked).toEqual([false, true]);
  expect(headers(target)).toEqual([]);
  expect(target.querySelector(".tf-empty")!.textContent).toContain("Nothing changes");
});

test("when the source answers after whitespace was asked ignored, the box is not left ticked over a diff that shows it", async () => {
  const ports: DiffPagePorts = {
    ...viaSource(answer),
    // The checkout answers first; with whitespace asked ignored, it no longer can (a commit gone, a slow git).
    rangeDiff: async (_id, _base, _head, ignoreWhitespace) => ({ files: [file("pay.py")], truncated: false, range: null, read_at: ignoreWhitespace ? "source" : "checkout" }),
  };
  view = mount(TfDiffPage, { target, props: { repoId: REPO, base: BASE, head: HEAD, ports } });
  await settle();
  target.querySelector<HTMLInputElement>(".tf-diffws input[type=checkbox]")!.click();
  await settle();
  const box = target.querySelector<HTMLInputElement>(".tf-diffws input[type=checkbox]")!;
  expect(box.disabled).toBe(true);
  expect(box.checked).toBe(false);
  expect(target.querySelector("[data-testid=ws-why]")).not.toBeNull();
});

test("on a diff the source served, Ignore whitespace is disabled and says why", async () => {
  const ports = viaSource(answer);
  view = mount(TfDiffPage, { target, props: { repoId: REPO, base: `${BASE}^`, head: HEAD, ports } });
  await settle();
  const box = target.querySelector<HTMLInputElement>(".tf-diffws input[type=checkbox]")!;
  expect(box.disabled).toBe(true);
  const why = document.getElementById(box.getAttribute("aria-describedby")!);
  expect(why?.textContent).toContain("cannot leave whitespace out");
  expect(why?.textContent).toContain("A checkout holding both commits can.");
  expect(why).toBe(target.querySelector("[data-testid=ws-why]"));
});

test("a diff between branch names says a checkout never reads it, rather than that one holding both commits could (Spec 5)", async () => {
  const ports: DiffPagePorts = {
    ...viaSource(answer),
    rangeDiff: async () => ({ files: [file("pay.py")], truncated: false, range: null, read_at: "source" }),
  };
  view = mount(TfDiffPage, { target, props: { repoId: REPO, base: "main", head: "spike/sepa-instant", ports } });
  await settle();
  const why = target.querySelector("[data-testid=ws-why]");
  expect(why?.textContent).toContain("A branch name is read where it lives, never at a checkout");
  expect(why?.textContent).not.toContain("A checkout holding both commits can.");
});

test("a diff that names its range runs a file's default pipeline on both sides at its two shas (#2270)", async () => {
  const B64: DiffFile = {
    path: "fixtures/hook.json.b64",
    old_path: null,
    change: "modified",
    additions: 1,
    deletions: 1,
    hunks: [{ header: "@@ -1 +1 @@", lines: [{ kind: "del", old: 1, new: null, text: "e30=" }, { kind: "add", old: null, new: 1, text: "eyJhIjoxfQ==" }] }],
    truncated: false,
    threads: [],
  };
  const PARENT = "907900b245f2941018dfd3ee155b876ef701c102";
  const asked: string[] = [];
  const ports: DiffPagePorts = {
    ...viaSource(async () =>
      ({
        sections: [{ id: "x", title: "Diff", count: 1, deferred: false, blocks: [{ type: "diff", files: [B64], truncated: false, commentable: null, gone: [], range: { repository: REPO, base: PARENT, head: HEAD, commits: [] } }] }],
        actions: [],
      }) as unknown as SourceView),
    piped: async (id, path, commit) => {
      asked.push(`${id} ${path} ${commit}`);
      return { outcome: "done", name: "hook.json", output: { kind: "text", text: commit === HEAD ? '{\n  "a": 1\n}\n' : "{}\n" } };
    },
  };
  installPipelines({ of: (path) => (path.endsWith(".json.b64") ? { name: "decode", pattern: "*.json.b64", steps: [{ step: "base64_decode" }] } : null) });
  try {
    view = mount(TfDiffPage, { target, props: { repoId: REPO, base: `${BASE}^`, head: HEAD, ports } });
    await settle();
    await settle();
    expect(asked.sort()).toEqual([`${REPO} fixtures/hook.json.b64 ${HEAD}`, `${REPO} fixtures/hook.json.b64 ${PARENT}`].sort());
    expect(target.querySelector(".tf-fh .tf-ran")?.textContent?.trim()).toBe("decode");
    expect(target.querySelector(".tf-diff")!.textContent!.replace(/\s+/g, " ")).toContain('"a": 1');
  } finally {
    installPipelines(noPipelines);
  }
});
