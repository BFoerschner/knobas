/**
 * The run page's Log tab (#854) over `run_log`: the steps' folds, the lines
 * outside them, find with "N of M" and its keys, the toolbar's toggles, and
 * *Download log* through `download_run_file`.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import type { DownloadTarget, LogLine, RunLog } from "../../ipc/entity";
import { toasts } from "../../shell/toasts.svelte";
import TfRunLog from "./TfRunLog.svelte";

let target: HTMLElement;
let page: ReturnType<typeof mount> | null = null;

beforeEach(() => {
  target = document.createElement("div");
  document.body.appendChild(target);
  toasts.items = [];
  Element.prototype.scrollIntoView = () => {};
});

afterEach(() => {
  if (page) unmount(page);
  page = null;
  target.remove();
});

async function settle() {
  for (let turn = 0; turn < 6; turn += 1) await new Promise((resolve) => setTimeout(resolve, 0));
  flushSync();
}

const line = (number: number, step: number | null, text: string, tone: LogLine["tone"] = null): LogLine => ({
  number,
  time: `10:06:${String(number).padStart(2, "0")}`,
  text,
  tone,
  step,
});

const LOG: RunLog = {
  total: 8,
  steps: [
    { name: "Checkout", first: 2, last: 3, seconds: 12, tone: null },
    { name: "cargo test", first: 4, last: 7, seconds: 82, tone: "err" },
  ],
  lines: [
    line(1, null, "TeamCity server version is 2026.1.3"),
    line(2, 0, "Step 1/2: Checkout"),
    line(3, 0, "Updating sources for retry"),
    line(4, 1, "Step 2/2: cargo test"),
    line(5, 1, "warning: unused variable", "warn"),
    line(6, 1, "test sepa::retry::gives_up ... FAILED", "err"),
    line(7, 1, "test sepa::retry::backs_off ... ok"),
    line(8, null, "Build finished"),
  ],
};

function draw(log: RunLog = LOG, failed = true, inFlight = false): DownloadTarget[] {
  const downloads: DownloadTarget[] = [];
  page = mount(TfRunLog, {
    target,
    props: {
      runId: "teamcity:build:1187",
      inFlight,
      failed,
      ports: {
        log: () => Promise.resolve(log),
        download: (_id, what) => {
          downloads.push(what);
          return Promise.resolve("/Users/mara/Downloads/Payout_IntegrationTests-1187.log");
        },
      },
    },
  });
  return downloads;
}

const folds = () => [...target.querySelectorAll<HTMLDetailsElement>(".tf-logv details")];
const shown = () => [...target.querySelectorAll(".tf-ll .tf-no")].map((n) => Number(n.textContent));
const button = (name: string) => [...target.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent?.trim() === name);
const count = () => target.querySelector(".tf-lqn")?.textContent;

function find(text: string) {
  const input = target.querySelector<HTMLInputElement>('input[aria-label="Find in log"]')!;
  input.value = text;
  input.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
}

function enter(shift = false) {
  target.querySelector('input[aria-label="Find in log"]')!.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", shiftKey: shift, bubbles: true }));
  flushSync();
}

test("each step is a fold with its duration and lines; a failed one is open, a passing one holds no lines until opened", async () => {
  draw();
  await settle();
  expect(folds().map((d) => [d.querySelector("summary")?.textContent, d.open])).toEqual([
    ["Checkout12 s, 2 lines", false],
    ["cargo test1 m 22 s, 4 lines", true],
  ]);
  expect(folds()[1]!.querySelector(".tf-dot.tf-err")).not.toBeNull();
  expect(shown()).toEqual([1, 4, 5, 6, 7, 8]);
  expect(target.querySelector('.tf-ll[data-n="6"]')?.classList.contains("tf-e")).toBe(true);
  expect(target.querySelector('.tf-ll[data-n="5"]')?.classList.contains("tf-w")).toBe(true);
  expect(target.querySelector('.tf-ll[data-n="5"] .tf-ts')?.textContent).toBe("10:06:05");
  expect(target.querySelector(".tf-logfoot")?.textContent).toBe("The whole log, 8 lines.");
});

test("a find opens every step with a match and counts N of M; Enter and Shift+Enter move through them", async () => {
  draw();
  await settle();
  find("retry");
  expect(folds().map((d) => d.open)).toEqual([true, true]);
  expect(shown()).toEqual([1, 2, 3, 4, 5, 6, 7, 8]);
  expect(count()).toBe("1 of 3");
  expect(target.querySelector("mark.tf-curm")?.closest(".tf-ll")?.getAttribute("data-n")).toBe("3");
  enter();
  expect(count()).toBe("2 of 3");
  expect(target.querySelector("mark.tf-curm")?.closest(".tf-ll")?.getAttribute("data-n")).toBe("6");
  enter(true);
  enter(true);
  expect(count()).toBe("3 of 3");
  find("nothing here");
  expect(count()).toBe("0 found");
});

test("errors and warnings only, wrap and timestamps are toggles on the toolbar", async () => {
  draw();
  await settle();
  button("Errors and warnings only")!.click();
  flushSync();
  expect(button("Errors and warnings only")!.getAttribute("aria-pressed")).toBe("true");
  expect(shown()).toEqual([5, 6]);
  button("Wrap lines")!.click();
  button("Timestamps")!.click();
  flushSync();
  const viewer = target.querySelector(".tf-logv")!;
  expect([viewer.classList.contains("tf-wrap"), viewer.classList.contains("tf-nots")]).toEqual([true, true]);
});

test("Errors and warnings only draws the log anew instead of dropping its lines one at a time (#2200's class)", async () => {
  // Dropping 15,000 lines of an open step one by one took 3 s on the main
  // thread; drawing the steps anew drops each fold whole.
  draw();
  await settle();
  const before = folds();
  button("Errors and warnings only")!.click();
  flushSync();
  expect(before.some((fold) => fold.isConnected)).toBe(false);
  expect(shown()).toEqual([5, 6]);
  button("Errors and warnings only")!.click();
  flushSync();
  expect(shown()).toEqual([1, 4, 5, 6, 7, 8]);
});

test("Jump to the first error is on a failed run only; Download log saves through knobas and says where", async () => {
  const downloads = draw();
  await settle();
  expect(button("Jump to the first error")).toBeDefined();
  button("Download log")!.click();
  await settle();
  expect(downloads).toEqual([{ kind: "log" }]);
  expect(toasts.items.at(-1)?.text).toBe("Saved Payout_IntegrationTests-1187.log to ~/Downloads");
  unmount(page!);
  page = null;
  draw({ ...LOG, steps: [LOG.steps[0]!, { ...LOG.steps[1]!, tone: null }], lines: LOG.lines.map((l) => ({ ...l, tone: null })) }, false, true);
  await settle();
  expect(button("Jump to the first error")).toBeUndefined();
  expect(folds()[1]!.querySelector("summary")?.textContent).toBe("cargo testrunning, 4 lines");
  expect(folds()[1]!.open).toBe(true);
});

test("past the cap the line under the log says which part it holds", async () => {
  draw({ ...LOG, total: 31_402 });
  await settle();
  expect(target.querySelector(".tf-logfoot")?.textContent).toBe("The last 8 of 31,402 lines. Download log has all of them.");
});

/** One failed step of `n` lines between a loose first and last line: the step opens by itself. */
function longLog(n: number, text: (number: number) => string = (number) => `  Running payout-service check ${number}`): RunLog {
  const lines = [line(1, null, "TeamCity server version is 2026.1.3")];
  for (let number = 2; number <= n + 1; number += 1) lines.push({ ...line(number, 0, text(number)), time: "10:05:58" });
  lines.push(line(n + 2, null, "Build finished"));
  return { total: n + 2, steps: [{ name: "Test", first: 2, last: n + 1, seconds: 30, tone: "err" }], lines };
}

test("a long open step draws only the lines near the viewport and keeps the room of the rest (#2211)", async () => {
  draw(longLog(5000));
  await settle();
  expect(folds().map((d) => d.open)).toEqual([true]);
  const drawn = shown();
  expect(drawn.length).toBeGreaterThan(5);
  expect(drawn.length).toBeLessThan(200);
  expect(drawn.slice(0, 3)).toEqual([1, 2, 3]);
  expect(target.querySelector<HTMLElement>(".tf-logv")!.style.paddingBottom).not.toBe("");
});

test("find moves to a hit in an undrawn line: it is drawn, marked current and scrolled to (#2211)", async () => {
  const scrolled: string[] = [];
  Element.prototype.scrollIntoView = function (this: Element) {
    scrolled.push(this.closest(".tf-ll")?.getAttribute("data-n") ?? this.tagName);
  };
  draw(longLog(5000, (number) => (number === 10 || number === 4000 ? `needle at ${number}` : `hay ${number}`)));
  await settle();
  find("needle");
  expect(count()).toBe("1 of 2");
  enter();
  expect(count()).toBe("2 of 2");
  expect(target.querySelector("mark.tf-curm")?.closest(".tf-ll")?.getAttribute("data-n")).toBe("4000");
  expect(scrolled.at(-1)).toBe("4000");
  enter();
  expect(target.querySelector("mark.tf-curm")?.closest(".tf-ll")?.getAttribute("data-n")).toBe("10");
  expect(scrolled.at(-1)).toBe("10");
});

test("Jump to the first error reaches an error line far below the drawn ones (#2211)", async () => {
  const scrolled: string[] = [];
  Element.prototype.scrollIntoView = function (this: Element) {
    scrolled.push(this.getAttribute("data-n") ?? this.tagName);
  };
  const log = longLog(5000);
  log.lines[4900] = { ...log.lines[4900]!, tone: "err", text: "error: test failed" };
  draw(log);
  await settle();
  expect(shown()).not.toContain(4901);
  button("Jump to the first error")!.click();
  await settle();
  expect(target.querySelector('.tf-ll[data-n="4901"]')?.classList.contains("tf-e")).toBe(true);
  expect(scrolled.at(-1)).toBe("4901");
});
