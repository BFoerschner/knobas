/**
 * *Everything* with a preview (#2431): with Everything on the dialog grows
 * and splits, results at the left and at the right the selected row's
 * excerpts, the words marked, following the arrow keys after a short pause;
 * a click on an excerpt opens the row. The preview is the hit's own
 * `excerpts`: no port is asked for it. With Titles on the dialog is the one
 * list at its usual size.
 *
 * Driven through props and the keyboard with fake ports, as
 * `Launcher.overlay.test.svelte.ts` drives the box's modes.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { FilesInRepo, LauncherHome, SearchHit, SearchQuery, SearchResponse } from "../ipc";
import { arrival } from "../tf/arrival.svelte";
import Launcher from "./Launcher.svelte";
import { PREVIEW_DEBOUNCE_MS } from "./Preview.svelte";
import { createScope } from "./scope.svelte";
import { DEBOUNCE_MS } from "./session.svelte";

let target: HTMLElement;
let app: ReturnType<typeof mount> | undefined;

function hit(entity_id: string, kind: string, title: string, excerpts: SearchHit["excerpts"]): SearchHit {
  return {
    entity_id,
    kind,
    source_id: entity_id.split(":")[0]!,
    updated_at: null,
    synced_at: "2026-08-25T11:56:00Z",
    path: null,
    state: null,
    title,
    rank: 1,
    snippet: [],
    excerpts,
  };
}

const PAGE = hit("confluence:7", "page", "Retry runbook", [
  {
    place: "Backoff policy",
    segments: [
      { text: "Wait, then ", hit: false },
      { text: "retry", hit: true },
      { text: " again.", hit: false },
    ],
  },
  { place: "Comment by Priya", segments: [{ text: "Cap the ", hit: false }, { text: "retry", hit: true }] },
]);
const PIPELINE = hit("teamcity:Deploy", "pipeline", "Deploy payouts", [
  { place: "Log line 382", segments: [{ text: "connection refused, ", hit: false }, { text: "retry", hit: true }] },
]);
const TITLED = hit("jira:PAY-1", "ticket", "Retry everything", []);
/** A repository its files found (D10, D12): a file's line, and one found by its file's name. */
const GATEWAY = {
  ...hit("gitea:tidewater/ledger-gateway", "repo", "tidewater/ledger-gateway", [
    {
      place: "src/RetryServer.java:7",
      segments: [{ text: "public final class ", hit: false }, { text: "Retry", hit: true }, { text: "Server {", hit: false }],
      file: { path: "src/RetryServer.java", line: 7, before: "", after: "" },
    },
  ]),
  files: { by_text: 4, by_name: 3, branch: "main" },
};
const CHARTS = {
  ...hit("gitea:tidewater/charts", "repo", "tidewater/charts", [
    { place: null, segments: [{ text: "charts/", hit: false }, { text: "retry", hit: true }, { text: "/values.yaml", hit: false }], file: { path: "charts/retry/values.yaml", line: null, before: "", after: "" } },
  ]),
  files: { by_text: 0, by_name: 1, branch: "main" },
};

/** Whether the answer holds the repositories the files found (the tests at the end). */
let withRepos = false;

function response(raw: string): SearchResponse {
  return {
    interpreted: {
      text: raw,
      prefix: null,
      filters: { sources: [], kinds: [], updated_within_days: null, mine: false, authors: [] },
      unknown_tokens: [],
      left_out_tokens: [],
    },
    groups: [
      { kind: "ticket", label: "Ticket", plural: "Tickets", monogram: "TK", total: 1, hits: [TITLED] },
      { kind: "pipeline", label: "Pipeline", plural: "Pipelines", monogram: "PL", total: 1, hits: [PIPELINE] },
      { kind: "page", label: "Page", plural: "Pages", monogram: "PG", total: 1, hits: [PAGE] },
      ...(withRepos ? [{ kind: "repo", label: "Repository", plural: "Repositories", monogram: "RE", total: 2, hits: [GATEWAY, CHARTS] }] : []),
    ],
    total: 3,
    took_ms: 3,
    coverage: [],
  };
}

const HOME = { recent: [], favourites: [], sources: [], pending_writes: 0, notes_shown: true } as unknown as LauncherHome;

function storage(value: string | null) {
  const kept = new Map<string, string>();
  if (value !== null) kept.set("knobas.launcher.scope", value);
  return {
    getItem: (key: string) => kept.get(key) ?? null,
    setItem: (key: string, v: string) => void kept.set(key, v),
    removeItem: (key: string) => void kept.delete(key),
  };
}

/** Mounts the launcher open under `mode`; answers what each port was asked. */
function open(mode: "everything" | "titles", more: { searchFiles?: (repository: string, raw: string) => Promise<FilesInRepo> } = {}) {
  const kept = storage(mode);
  const searched: SearchQuery[] = [];
  const onnavigate = vi.fn();
  const entityView = vi.fn();
  app = mount(Launcher, {
    target,
    props: {
      open: true,
      onnavigate,
      onclose: () => {},
      ontimer: undefined,
      scope: createScope(() => kept),
      ports: {
        search: async (query: SearchQuery) => (searched.push(query), response(query.raw)),
        launcherHome: async () => HOME,
        entityView,
        searchFiles: more.searchFiles ?? (async () => ({ files: [], by_text: 0, by_name: 0, more: false, branch: "main" })),
      },
    },
  });
  return { searched, onnavigate, entityView };
}

async function settle() {
  await Promise.resolve();
  await Promise.resolve();
  await Promise.resolve();
  flushSync();
}

function input(): HTMLInputElement {
  return target.querySelector("input")!;
}

async function type(text: string) {
  input().value = text;
  input().dispatchEvent(new Event("input", { bubbles: true }));
  vi.advanceTimersByTime(DEBOUNCE_MS);
  await settle();
}

function press(key: string, options: KeyboardEventInit = {}) {
  input().dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true, ...options }));
  flushSync();
}

/** Past the preview's pause. */
async function paused() {
  vi.advanceTimersByTime(PREVIEW_DEBOUNCE_MS);
  await settle();
}

function dialog(): HTMLElement {
  return target.querySelector(".tf-dialog")!;
}

function pane(): HTMLElement | null {
  return target.querySelector('[aria-label="Where it matches"]');
}

/** The pane's excerpts as `place: text`, the marked words in brackets. */
function excerpts(): string[] {
  return [...(pane()?.querySelectorAll(".tf-ln-ex") ?? [])].map((row) => {
    const place = row.querySelector(".tf-ln-ex-at")?.textContent?.trim();
    const text = [...row.querySelector(".tf-ln-ex-t")!.childNodes]
      .map((node) => (node.nodeName === "MARK" ? `[${node.textContent}]` : node.textContent))
      .join("");
    return place ? `${place}: ${text}` : text;
  });
}

beforeEach(() => {
  vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
  target = document.createElement("div");
  document.body.append(target);
});

afterEach(() => {
  withRepos = false;
  if (app) unmount(app);
  app = undefined;
  vi.useRealTimers();
  target.remove();
});

test("with Everything on the dialog grows and shows the pane beside the results", async () => {
  open("everything");
  await settle();
  await type("retry");
  expect(dialog().classList.contains("tf-launch-deep")).toBe(true);
  expect(pane()).not.toBeNull();
  expect(target.querySelector('[aria-label="Results"]')).not.toBeNull();
});

test("Everything's first search finding nothing keeps the usual size; once it has found something it stays grown (F-625)", async () => {
  const kept = storage("everything");
  const nothing = (raw: string): SearchResponse => ({ ...response(raw), groups: [], total: 0 });
  app = mount(Launcher, {
    target,
    props: {
      open: true,
      onnavigate: vi.fn(),
      onclose: () => {},
      ontimer: undefined,
      scope: createScope(() => kept),
      ports: {
        search: async (query: SearchQuery) => (query.raw.startsWith("zz") ? nothing(query.raw) : response(query.raw)),
        launcherHome: async () => HOME,
        entityView: vi.fn(),
      },
    },
  });
  await settle();
  expect(dialog().classList.contains("tf-launch-deep"), "the empty box is grown, so the usual search moves nothing").toBe(true);
  await type("zzzqqq");
  expect(dialog().classList.contains("tf-launch-deep"), "one sentence").toBe(false);
  expect(pane()).toBeNull();
  await type("retry");
  expect(dialog().classList.contains("tf-launch-deep"), "an answer grows it").toBe(true);
  await type("zzzqqq");
  flushSync();
  expect(dialog().classList.contains("tf-launch-deep"), "the box does not move back under the words").toBe(true);
  expect(pane()).toBeNull();
});

test("with Titles on it is the single list at the usual size", async () => {
  const { searched } = open("titles");
  await settle();
  await type("retry");
  expect(dialog().classList.contains("tf-launch-deep")).toBe(false);
  expect(pane()).toBeNull();
  expect(searched.at(-1)).toMatchObject({ titles: true });
});

test("the pane waits for the selection to settle, then draws its excerpts with their places and marks", async () => {
  open("everything");
  await settle();
  await type("retry");
  // The first row is a hit by its title alone.
  await paused();
  expect(pane()?.textContent).toContain("Retry everything");
  // #2481: the same row may be a hit by a stem or a key, so no word about where.
  expect(pane()?.textContent).toContain("No passage to quote.");
  expect(pane()?.textContent).not.toContain("title holds");

  press("ArrowDown");
  // Still the row before, until the pause is over.
  expect(pane()?.textContent).toContain("Retry everything");
  vi.advanceTimersByTime(PREVIEW_DEBOUNCE_MS - 1);
  flushSync();
  expect(pane()?.textContent).toContain("Retry everything");
  await paused();
  expect(excerpts()).toEqual(["Log line 382: connection refused, [retry]"]);

  press("ArrowDown");
  await paused();
  expect(excerpts()).toEqual(["Backoff policy: Wait, then [retry] again.", "Comment by Priya: Cap the [retry]"]);
});

test("a click on an excerpt opens the row, and the preview made no second read", async () => {
  const { onnavigate, entityView, searched } = open("everything");
  await settle();
  await type("retry");
  press("ArrowDown");
  press("ArrowDown");
  await paused();
  const asked = searched.length;

  (pane()!.querySelectorAll(".tf-ln-ex")[1] as HTMLElement).click();
  flushSync();
  expect(onnavigate).toHaveBeenCalledTimes(1);
  expect(String(onnavigate.mock.calls[0]![0])).toContain("confluence");
  expect(entityView).not.toHaveBeenCalled();
  expect(searched.length).toBe(asked);
});

test("⌘D to Titles takes the pane away and the dialog back to its size", async () => {
  open("everything");
  await settle();
  await type("retry");
  await paused();
  expect(pane()).not.toBeNull();
  press("d", { metaKey: true });
  await settle();
  expect(pane()).toBeNull();
  expect(dialog().classList.contains("tf-launch-deep")).toBe(false);
});

// -- the evidence line and opening at the match (the everything-search decisions, D10 and D12) --

test("a row its text found draws one evidence line, a row with no passage stays one line", async () => {
  open("everything");
  await settle();
  await type("retry");
  const rows = [...target.querySelectorAll<HTMLElement>(".tf-ln-res")];
  const line = (title: string) => rows.find((row) => row.textContent?.includes(title))!.querySelector(".tf-ln-why");
  expect(line("Retry everything")).toBeNull();
  expect(line("Deploy payouts")!.classList.contains("tf-ln-code")).toBe(true);
  // A lead over 18 characters is cut to its last words, so the mark sits in the line's first third.
  expect(line("Deploy payouts")!.querySelector(".tf-ln-why-t")!.textContent).toBe("Log line 382…refused, retry");
  expect(line("Retry runbook")!.querySelector(".tf-ln-why-at")!.textContent).toBe("Backoff policy");
  expect(line("Retry runbook")!.querySelector("mark")!.textContent).toBe("retry");
});

test("a click on an excerpt opens the page at that excerpt", async () => {
  const { onnavigate } = open("everything");
  await settle();
  await type("retry");
  press("ArrowDown");
  press("ArrowDown");
  await paused();
  (pane()!.querySelectorAll(".tf-ln-ex")[1] as HTMLElement).click();
  flushSync();
  const hash = String(onnavigate.mock.calls[0]![0]);
  expect(arrival.take(hash)).toEqual({ words: ["retry"], passage: "Cap the retry" });
});

test("Enter on a row with an evidence line opens it at the line's passage", async () => {
  const { onnavigate } = open("everything");
  await settle();
  await type("retry");
  press("ArrowDown");
  press("ArrowDown");
  press("Enter");
  flushSync();
  const hash = String(onnavigate.mock.calls[0]![0]);
  expect(hash).toContain("confluence");
  expect(arrival.take(hash)).toEqual({ words: ["retry"], passage: "Wait, then retry again." });
});

test("Enter on a log's line opens the pipeline as it did, with no arrival", async () => {
  arrival.clear();
  const { onnavigate } = open("everything");
  await settle();
  await type("retry");
  press("ArrowDown");
  press("Enter");
  flushSync();
  const log = String(onnavigate.mock.calls[0]![0]);
  expect(log).toContain("builds");
  expect(arrival.waiting(log)).toBe(false);
});

test("a repository's file line opens the file at its line, its find bar on the words", async () => {
  withRepos = true;
  const { onnavigate } = open("everything");
  await settle();
  await type("retry");
  const rows = [...target.querySelectorAll<HTMLElement>(".tf-ln-res")];
  const gateway = rows.find((row) => row.textContent?.includes("ledger-gateway"))!;
  expect(gateway.querySelector(".tf-ln-why-at")!.textContent).toBe("RetryServer.java:7");
  expect(gateway.querySelector(".tf-ln-why-n")!.textContent).toBe("4 files");
  gateway.click();
  flushSync();
  const hash = String(onnavigate.mock.calls[0]![0]);
  expect(hash).toBe("#/repo/gitea%3Atidewater%2Fledger-gateway/line/7/src/RetryServer.java");
  expect(arrival.take(hash)).toEqual({ words: ["retry"], passage: "public final class RetryServer {" });
});

test("a repository found by a file's name opens that file", async () => {
  withRepos = true;
  const { onnavigate } = open("everything");
  await settle();
  await type("retry");
  const charts = [...target.querySelectorAll<HTMLElement>(".tf-ln-res")].find((row) => row.textContent?.includes("tidewater/charts") || row.textContent?.includes("charts"))!;
  expect(charts.querySelector(".tf-ln-why-n")!.textContent).toBe("1 file name");
  charts.click();
  flushSync();
  expect(String(onnavigate.mock.calls[0]![0])).toBe("#/repo/gitea%3Atidewater%2Fcharts/files/charts/retry/values.yaml");
});

test("the pane lists a repository's files: the answer's first at once, the others under it when they land, a stale answer dropped (D11)", async () => {
  withRepos = true;
  let land: (found: FilesInRepo) => void = () => {};
  const searchFiles = vi.fn(() => new Promise<FilesInRepo>((done) => (land = done)));
  open("everything", { searchFiles });
  await settle();
  await type("retry");
  // Down to the repositories, after the ticket, the pipeline and the page.
  for (let i = 0; i < 3; i++) press("ArrowDown");
  await paused();
  expect(pane()!.querySelector(".tf-ln-pv-n")!.textContent).toBe("In 4 files of main, 3 of them by name.");
  const first = () => [...pane()!.querySelectorAll(".tf-ln-file .tf-ln-file-path")].map((path) => path.textContent);
  expect(first()).toEqual(["src/RetryServer.java"]);
  expect(searchFiles).toHaveBeenCalledWith("gitea:tidewater/ledger-gateway", "retry");
  const file = (path: string, line: number) => ({
    place: `${path}:${line}`,
    segments: [{ text: "class ", hit: false }, { text: "Retry", hit: true }],
    file: { path, line, before: "", after: "" },
  });
  land({ files: [file("src/RetryServer.java", 7), file("src/RetryQueue.java", 3)], by_text: 4, by_name: 3, more: false, branch: "main" });
  await settle();
  expect(first()).toEqual(["src/RetryServer.java", "src/RetryQueue.java"]);
});

test("a files answer for a repository no longer selected draws nothing", async () => {
  withRepos = true;
  let land: (found: FilesInRepo) => void = () => {};
  const searchFiles = vi.fn(() => new Promise<FilesInRepo>((done) => (land = done)));
  open("everything", { searchFiles });
  await settle();
  await type("retry");
  for (let i = 0; i < 3; i++) press("ArrowDown");
  await paused();
  press("ArrowUp");
  await paused();
  land({ files: [{ place: "x:1", segments: [{ text: "Retry", hit: true }], file: { path: "x", line: 1, before: "", after: "" } }], by_text: 4, by_name: 3, more: false, branch: "main" });
  await settle();
  expect(pane()!.querySelectorAll(".tf-ln-file")).toHaveLength(0);
});
