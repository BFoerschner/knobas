/**
 * A run of an **external adapter** on the run page (#1205), over the IPC:
 * the page's own ports (`RUN_BRIDGE`), with the bridge answering what the
 * backend answers for the fixture component's run (`crates/knobas-wasm-
 * fixture/src/builds.rs`). The page reads the log range and streams the
 * downloads as it does for a built-in's run: nothing in it is the adapter's
 * name.
 */
import { createRawSnippet, flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { PipelineRow, RunRow } from "../../ipc/entity";
import { toasts } from "../../shell/toasts.svelte";
import TfRun from "./TfRun.svelte";

const RUN = "surveys:run-1";
const PIPE = "surveys:pipeline-1";

/** The fixture's answers, as the host decodes them from the component. */
const LOG = [
  [0, "checkout quay-telemetry at 4be1c09", null],
  [0, "checked out", null],
  [1, "cargo test --workspace", null],
  [1, "test crane::hoist::brake_wear ... ok", null],
  [1, "test crane::hoist::brake_limit ... FAILED", "err"],
  [1, "error: 1 test failed", "err"],
] as const;

const ANSWERS: Record<string, (args: Record<string, unknown>) => unknown> = {
  run_log: () => ({
    lines: LOG.map(([step, text, tone], i) => ({ number: i + 1, time: null, text, tone, step })),
    steps: [
      { name: "Checkout", first: 1, last: 2, seconds: 3, tone: null },
      { name: "Test", first: 3, last: 6, seconds: 237, tone: "err" },
    ],
    total: 6,
  }),
  run_artifacts: (args) =>
    args.path === null
      ? [
          { path: "reports", name: "reports", size: null, folder: true, children: 1 },
          { path: "crane.txt", name: "crane.txt", size: 21, folder: false, children: null },
        ]
      : [{ path: "reports/summary.txt", name: "summary.txt", size: 18, folder: false, children: null }],
  download_run_file: (args) => {
    const target = args.target as { kind: string; path?: string };
    const name = target.kind === "log" ? "run-42.log" : target.kind === "file" ? target.path!.split("/").at(-1) : "artifacts.zip";
    return `/Users/mara/Downloads/${name}`;
  },
  entity_view: () => ({
    sections: [{ id: "summary", title: "Run 42", count: null, deferred: false, blocks: [] }],
    actions: [],
  }),
  pipeline_settings: () => ({ repositories: [], triggers: [], parameters: [], parameter_write_op: null, steps: ["Checkout", "Test"] }),
  older_pipeline_runs: () => [],
  list_sources: () => [],
  list_adapters: () => [],
  sync_status: () => [],
};

const bridged = vi.hoisted(() => [] as { command: string; args: Record<string, unknown> }[]);
vi.mock("@tauri-apps/api/core", async (original) => ({
  ...(await original<typeof import("@tauri-apps/api/core")>()),
  invoke: (command: string, args: Record<string, unknown> = {}) => {
    bridged.push({ command, args });
    const answer = ANSWERS[command];
    return answer ? Promise.resolve(answer(args)) : Promise.reject(new Error(`no answer for ${command}`));
  },
}));

let target: HTMLElement;
let page: ReturnType<typeof mount> | null = null;

beforeEach(() => {
  bridged.length = 0;
  toasts.items = [];
  Element.prototype.scrollIntoView = () => {};
  target = document.createElement("div");
  document.body.appendChild(target);
});

afterEach(() => {
  if (page) unmount(page);
  page = null;
  target.remove();
});

async function settle() {
  for (let turn = 0; turn < 8; turn += 1) await new Promise((resolve) => setTimeout(resolve, 0));
  flushSync();
}

const THE_RUN: RunRow = {
  entity_id: RUN,
  number: "42",
  state: { text: "failed", tone: "err" },
  branch: "main",
  started: "2026-10-18T09:00:00Z",
  finished: "2026-10-18T09:04:00Z",
  duration_seconds: 240,
  updated_at: "2026-10-18T09:04:00Z",
  web_url: null,
  trigger: null,
  started_by: null,
  started_by_me: false,
  revision: null,
  commit_id: null,
  agent: null,
  current_step: null,
};

const PIPELINE: PipelineRow = {
  entity_id: PIPE,
  source_id: "surveys",
  source_name: "Harbour surveys",
  name: "Crane telemetry",
  group: null,
  path: [],
  pinned: false,
  web_url: null,
  recent: [THE_RUN],
};

function draw(tab: string) {
  const crumbs = createRawSnippet(() => ({ render: () => `<nav class="tf-crumbs"></nav>` }));
  page = mount(TfRun, {
    target,
    props: { pipeline: PIPELINE, rest: [PIPE, RUN, tab], job: null, runs: () => Promise.resolve([THE_RUN]), crumbs },
  });
}

const button = (name: string) => [...target.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent?.trim() === name);
const calls = (command: string) => bridged.filter((c) => c.command === command).map((c) => c.args);

test("an external adapter's run log is read through run_log and drawn in its steps; Download log streams it", async () => {
  draw("log");
  await settle();
  expect(calls("run_log")).toEqual([{ runId: RUN, from: null, count: null }]);
  const lines = [...target.querySelectorAll(".tf-ll")].map((l) => l.getAttribute("data-n"));
  expect(lines).toContain("6");
  expect(target.querySelector('.tf-ll[data-n="6"]')?.classList.contains("tf-e")).toBe(true);
  expect(target.querySelector(".tf-logfoot")?.textContent).toBe("The whole log, 6 lines.");
  button("Download log")!.click();
  await settle();
  expect(calls("download_run_file")).toEqual([{ runId: RUN, target: { kind: "log" } }]);
  expect(toasts.items.at(-1)?.text).toBe("Saved run-42.log to ~/Downloads");
});

test("an external adapter's artifact downloads through download_run_file", async () => {
  draw("artifacts");
  await settle();
  expect(calls("run_artifacts").map((a) => a.path)).toEqual([null, "reports"]);
  const row = [...target.querySelectorAll(".tf-arow")].find((r) => r.textContent?.includes("crane.txt"))!;
  [...row.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent?.trim() === "Download")!.click();
  await settle();
  expect(calls("download_run_file")).toEqual([{ runId: RUN, target: { kind: "file", path: "crane.txt" } }]);
  expect(toasts.items.at(-1)?.text).toBe("Saved crane.txt to ~/Downloads");
});
