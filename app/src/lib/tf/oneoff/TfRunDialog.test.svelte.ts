/** The run dialog (ADR-0060, section 13) and the store behind it. */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { OneOff, OneOffBatch, OneOffLine, OneOffRunRow, OneOffStarted, OneOffTarget } from "../../ipc/oneoff";
import { OneOffs, type OneOffPorts } from "../../render/oneoff.svelte";
import { toasts } from "../../shell/toasts.svelte";
import TfRunDialog from "./TfRunDialog.svelte";

const command: OneOff = { id: "eslint", name: "ESLint", command: "eslint {file}", stdin: "none", files: [], folders: [], has: [], repo_holds: [], repos: [], shows: "auto", start_in: "root", path: [], env: [], colours: true, timeout_secs: 120, wsl_paths: false };
const target: OneOffTarget = { kind: "file", path: "src/a.ts", lines: null, at: null };

/** Ports a test drives: each start hands back its batch sink, and `moved` its handler. */
function ports() {
  const sinks: ((b: OneOffBatch) => void)[] = [];
  let moved: (row: OneOffRunRow) => void = () => {};
  let next = 0;
  let answer: ((value: { started: OneOffStarted; stop: () => void }) => void) | null = null;
  const p: OneOffPorts & { sinks: typeof sinks; emit: (row: OneOffRunRow) => void; answer: () => void; kept: Map<string, OneOffLine[]> } = {
    sinks,
    kept: new Map(),
    emit: (row) => moved(row),
    answer: () => answer?.({ started: { run: `r${next}`, argv: ["eslint", "/c/p/src/a.ts"], cwd: "/c/p", branch: "main", head: null, started_at: Date.now(), already: false }, stop: () => {} }),
    commands: () => Promise.resolve([command]),
    run: vi.fn((_e: string, _c: string, _t: OneOffTarget, onbatch: (b: OneOffBatch) => void) => {
      next += 1;
      sinks.push(onbatch);
      return new Promise<{ started: OneOffStarted; stop: () => void }>((resolve) => (answer = resolve));
    }),
    stop: vi.fn(() => Promise.resolve()),
    runs: () => Promise.resolve([]),
    lines: vi.fn((run: string, from: number, count: number) => {
      const lines = (p.kept.get(run) ?? []).slice(from, from + count);
      const kept: OneOffBatch = { run, lines, changed: [], total: p.kept.get(run)?.length ?? 0, dropped: 0, tail_from: 0, bytes: 10, findings: 0, elapsed_ms: 100, ended: { how: "exit", code: 0 } };
      return Promise.resolve(kept);
    }),
    watch: vi.fn(() => Promise.resolve({ known: true, stop: () => {} })),
    save: vi.fn(() => Promise.resolve("/Downloads/eslint-a.ts.txt")),
    result: vi.fn((_run: string, file: string | null) => Promise.resolve(new TextEncoder().encode(file === null ? '{"a": 1}' : `file ${file}`))),
    copy: vi.fn(() => Promise.resolve()),
    setCommands: vi.fn(() => Promise.resolve([command])),
    forget: vi.fn(() => Promise.resolve()),
    shell: vi.fn(() => Promise.resolve({ on: true, offered: true, read: null })),
    repoCommands: vi.fn(() => Promise.resolve({ file: null, offered: [], failed: null })),
    copyRepoCommands: vi.fn(() => Promise.resolve([command])),
    moved: (handler) => {
      moved = handler;
      return () => {};
    },
  };
  return p;
}

const batch = (run: string, lines: OneOffLine[], extra: Partial<OneOffBatch> = {}): OneOffBatch => ({ run, lines, changed: [], total: lines.length, dropped: 0, tail_from: lines.length, bytes: 40, findings: 0, elapsed_ms: 1200, ended: null, ...extra });
const words = (el: Element | null | undefined) => el?.textContent?.replace(/\s+/g, " ").trim() ?? "";
const segs = () => [...document.querySelectorAll(".tf-oo-bar .tf-seg button")].map((b) => words(b));
async function settle() {
  for (let i = 0; i < 4; i += 1) {
    await new Promise((resolve) => setTimeout(resolve, 0));
    flushSync();
  }
}

let view: ReturnType<typeof mount> | null = null;
beforeEach(() => {
  toasts.items = [];
});
afterEach(() => {
  if (view) unmount(view);
  view = null;
  document.body.innerHTML = "";
});

function draw(store: OneOffs, navigate = vi.fn()) {
  view = mount(TfRunDialog, { target: document.body, props: { run: store.open!, store, navigate } });
  flushSync();
  return navigate;
}

test("the press paints the dialog running at 0:00, before the backend answers", () => {
  const p = ports();
  const store = new OneOffs(p);
  store.openRun(command, "gitea:t/payout", "payout", target, "/c/p");
  draw(store);
  expect(words(document.querySelector(".tf-oo-state"))).toBe("Running");
  expect(words(document.querySelector(".tf-oo-took"))).toBe("0:00");
  expect(words(document.querySelector(".tf-oo-argv"))).toBe("eslint /c/p/src/a.ts");
  expect(words(document.querySelector(".tf-oo-stage"))).toBe("Nothing printed yet.");
  expect(document.querySelector(".tf-oo-line")!.hasAttribute("data-running")).toBe(true);
  expect(words(document.querySelector(".tf-oo-again"))).toBe("Stop");
  // Arrival focus is the stage, not a button: Enter presses nothing.
  expect(document.activeElement?.classList.contains("tf-oo-quiet")).toBe(true);
});

test("lines land, places are links, and the run ends red with its exit code as a figure", async () => {
  const p = ports();
  const store = new OneOffs(p);
  const run = store.openRun(command, "gitea:t/payout", "payout", target);
  const navigate = draw(store);
  p.answer();
  await settle();
  p.sinks[0]!(batch("r1", [
    { n: 0, stream: "out", text: "src/a.ts" , places: [{ from: 0, to: 8, path: "src/a.ts", line: null, col: null }] },
    { n: 1, stream: "out", text: "  3:7  error  no semi  semi", places: [{ from: 2, to: 5, path: "src/a.ts", line: 3, col: 7 }], finding: { severity: "error", said: "no semi", rule: "semi" }, runs: [{ from: 7, to: 12, style: { fg: "r" } }] },
  ], { findings: 1 }));
  p.sinks[0]!(batch("r1", [], { total: 2, findings: 1, ended: { how: "exit", code: 1 }, elapsed_ms: 2400 }));
  flushSync();
  expect(run.ended).toEqual({ how: "exit", code: 1 });
  expect(words(document.querySelector(".tf-oo-state"))).toBe("Exited with 1");
  expect(document.querySelector(".tf-oo-state")!.classList.contains("tf-err")).toBe(true);
  expect(document.querySelector(".tf-oo-code")!.textContent).toBe("1");
  expect(words(document.querySelector(".tf-oo-took"))).toBe("after 2.4s");
  expect(words(document.querySelector(".tf-oo-again"))).toBe("Run again");
  expect(document.querySelector(".tf-oo-r")!.textContent).toBe("error");
  expect(segs()).toEqual(["Output", "Problems 1"]);
  const link = [...document.querySelectorAll<HTMLAnchorElement>(".tf-oo-link")].find((a) => a.textContent === "3:7")!;
  link.click();
  flushSync();
  expect(navigate).toHaveBeenCalledWith("#/repo/gitea%3At%2Fpayout/line/3/src/a.ts");
  expect(store.open).toBeNull();
  expect(toasts.items.at(-1)?.action?.label).toBe("Back to it");
});

test("Problems lists the findings a file a group", async () => {
  const p = ports();
  const store = new OneOffs(p);
  const run = store.openRun(command, "gitea:t/payout", "payout", target);
  draw(store);
  p.sinks[0]!(batch("r1", [{ n: 0, stream: "err", text: "b.ts:9: warning: y", places: [{ from: 0, to: 6, path: "b.ts", line: 9, col: null }], finding: { severity: "warning", said: "y", rule: "W1" } }], { ended: { how: "exit", code: 1 } }));
  run.view = "problems";
  flushSync();
  expect(words(document.querySelector(".tf-oo-pf"))).toBe("b.ts 1 problem");
  expect(words(document.querySelector(".tf-oo-p"))).toBe("9 warning y W1");
});

test("a start the backend refuses says Did not start, with the sentence", async () => {
  const p = ports();
  p.run = vi.fn(() => Promise.reject({ code: "invalid", message: "four commands are running (A, B, C, D): stop one or wait for it to end" }));
  const store = new OneOffs(p);
  store.openRun(command, "gitea:t/payout", "payout", target);
  draw(store);
  await settle();
  expect(words(document.querySelector(".tf-oo-state"))).toBe("Did not start");
  expect(words(document.querySelector(".tf-oo-stage"))).toContain("four commands are running");
});

test("Stop stops it, and Escape closes the dialog and leaves the run going", async () => {
  const p = ports();
  const store = new OneOffs(p);
  const run = store.openRun(command, "gitea:t/payout", "payout", target);
  draw(store);
  p.answer();
  await settle();
  document.querySelector<HTMLButtonElement>(".tf-oo-again")!.click();
  expect(p.stop).toHaveBeenCalledWith("r1");
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
  document.activeElement?.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
  flushSync();
  expect(store.open).toBeNull();
  expect(run.ended).toBeNull();
});

test("a run that ends with its dialog closed says so in a toast with Show", async () => {
  const p = ports();
  const store = new OneOffs(p);
  store.hear();
  await store.load();
  store.openRun(command, "gitea:t/payout", "payout", target);
  p.answer();
  await settle();
  store.close();
  const row: OneOffRunRow = { id: "r1", command: "eslint", name: "ESLint", entity_id: "gitea:t/payout", target, argv: [], cwd: "", started_at: 0, lines: 3, findings: 0, kept: true, ended: { how: "exit", code: 0 }, duration_ms: 10 };
  p.emit({ ...row, ended: null, duration_ms: null });
  expect(store.running.length).toBe(1);
  p.emit(row);
  expect(store.running.length).toBe(0);
  const toast = toasts.items.at(-1)!;
  expect(toast.text).toContain("ESLint exited with 0");
  expect(toast.action?.label).toBe("Show");
  p.kept.set("r1", [{ n: 0, stream: "out", text: "one" }, { n: 1, stream: "out", text: "two" }, { n: 2, stream: "out", text: "three" }]);
  toast.action!.run();
  await settle();
  expect(store.open?.id).toBe("r1");
  expect(store.open?.lines.map((l) => l.text)).toEqual(["one", "two", "three"]);
});

test("a flood keeps every line in order, and a changed line is put in its place", () => {
  const p = ports();
  const store = new OneOffs(p);
  const run = store.openRun(command, "gitea:t/payout", "payout", target);
  for (let b = 0; b < 30; b++) run.take(batch("r1", Array.from({ length: 660 }, (_, i) => ({ n: b * 660 + i, stream: "out" as const, text: `line ${b * 660 + i}` }))));
  run.take(batch("r1", [], { changed: [{ n: 5, stream: "out", text: "line five again" }] }));
  expect(run.lines.length).toBe(19_800);
  expect(run.lines[5]!.text).toBe("line five again");
  expect(run.lines.every((l, i) => l.n === i)).toBe(true);
});

test("a result that is not lines opens first, and the bytes overrule a label", async () => {
  const p = ports();
  const store = new OneOffs(p);
  const run = store.openRun(command, "gitea:t/payout", "payout", target);
  draw(store);
  p.answer();
  await settle();
  p.sinks[0]!(batch("r1", [], { ended: { how: "exit", code: 0 }, result: { from: "one", kind: { kind: "json" }, size: 8, name: "stdout", overruled: "image" } }));
  await settle();
  expect(run.view).toBe("result");
  expect(segs()).toEqual(["JSON", "Output"]);
  expect(words(document.querySelector(".tf-oo-counts"))).toBe("Its stdout, 8 B. Said to be image, it is not.");
  expect(p.result).toHaveBeenCalledWith("r1", null);
});

test("Files lists what it wrote with the first picked, and what was left out", async () => {
  const p = ports();
  const store = new OneOffs(p);
  store.openRun(command, "gitea:t/payout", "payout", target);
  draw(store);
  p.answer();
  await settle();
  p.sinks[0]!(batch("r1", [], { ended: { how: "exit", code: 0 }, result: { from: "files", files: [{ path: "a.txt", size: 1 }, { path: "sub/b.json", size: 2 }], bytes: 3, left_out: [{ path: "link", why: "link" }], left_out_total: 1 } }));
  await settle();
  expect(segs()).toEqual(["Files 2", "Output"]);
  expect([...document.querySelectorAll(".tf-oo-flist button")].map((b) => words(b.querySelector(".tf-oo-fname")))).toEqual(["a.txt", "sub/b.json"]);
  expect(words(document.querySelector(".tf-oo-fnote"))).toContain("link (a link)");
  expect(p.result).toHaveBeenCalledWith("r1", "a.txt");
});

test("a batch runs one run a file, four at a time, and says how many failed", async () => {
  const p = ports();
  const store = new OneOffs(p);
  const files = Array.from({ length: 6 }, (_, i) => `src/f${i}.ts`);
  const run = store.openBatch(command, "gitea:t/payout", "payout", files);
  draw(store);
  expect(p.run).toHaveBeenCalledTimes(4);
  p.sinks[0]!(batch("x", [{ n: 0, stream: "out", text: "bad" }], { ended: { how: "exit", code: 1 } }));
  await settle();
  expect(p.run).toHaveBeenCalledTimes(5);
  for (const i of [1, 2, 3, 4]) p.sinks[i]!(batch("x", [], { ended: { how: "exit", code: 0 } }));
  await settle();
  expect(p.run).toHaveBeenCalledTimes(6);
  p.sinks[5]!(batch("x", [], { ended: { how: "exit", code: 0 } }));
  await settle();
  expect(run.ended).toEqual({ how: "exit", code: 1 });
  expect(words(document.querySelector(".tf-oo-state"))).toBe("1 of 6 exited with an error");
  // Failures first.
  expect(words(document.querySelector(".tf-oo-brow .tf-oo-fname"))).toBe("f0.ts");
});

test("Stop on a batch stops what runs and starts no more", async () => {
  const p = ports();
  const store = new OneOffs(p);
  const run = store.openBatch(command, "gitea:t/payout", "payout", Array.from({ length: 9 }, (_, i) => `f${i}.ts`));
  draw(store);
  store.stop(run);
  for (const i of [0, 1, 2, 3]) p.sinks[i]!(batch("x", [], { ended: { how: "stopped" } }));
  await settle();
  expect(p.run).toHaveBeenCalledTimes(4);
  expect(run.ended).toEqual({ how: "stopped" });
});
