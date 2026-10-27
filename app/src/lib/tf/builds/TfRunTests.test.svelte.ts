/**
 * The run page's Tests tab (#855) over `run_tests` and `test_history`: failed
 * first, the filter bar's text, result and module, and a failed test's
 * assertion, *Show in log* and history line.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import type { RunTest, TestOutcome } from "../../ipc/entity";
import TfRunTests from "./TfRunTests.svelte";

let target: HTMLElement;
let page: ReturnType<typeof mount> | null = null;

beforeEach(() => {
  target = document.createElement("div");
  document.body.appendChild(target);
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

const FEATURE = "feature/PAY-231-sepa-retry";

const t = (id: string, name: string, status: RunTest["status"], module: string, details: string | null = null): RunTest => ({
  id,
  name,
  module,
  status,
  muted: false,
  new_failure: false,
  duration_ms: 420,
  details,
});

const TESTS: RunTest[] = [
  t("1", "sepa::retry::backs_off_exponentially", "passed", "sepa::retry"),
  t("2", "sepa::retry::gives_up_after_max_attempts", "failed", "sepa::retry", "assertion `left == right` failed\n  left: 6\n right: 5\n  at tests/sepa_retry.rs:14:5"),
  t("3", "ledger::booking::ledger_books_payout", "passed", "ledger::booking"),
];

const o = (status: TestOutcome["status"], branch: string): TestOutcome => ({ run: "r", number: null, branch, status });

function draw(tests: RunTest[] = TESTS): string[] {
  const asked: string[] = [];
  page = mount(TfRunTests, {
    target,
    props: {
      runId: "teamcity:build:1187",
      branch: FEATURE,
      logHref: "#/build/p/r/log",
      ports: {
        tests: () => Promise.resolve({ tests, truncated: false }),
        history: (_run, id) => {
          asked.push(id);
          return Promise.resolve([o("failed", FEATURE), o("passed", "main"), o("failed", FEATURE), ...Array.from({ length: 7 }, () => o("passed", "main"))]);
        },
      },
    },
  });
  return asked;
}

const names = () => [...target.querySelectorAll(".tf-trow .tf-nm")].map((n) => n.textContent);
const chip = (name: string) => [...target.querySelectorAll<HTMLButtonElement>(".tf-fchips button")].find((b) => b.textContent?.trim() === name)!;

test("failed tests come first, open to their assertion, Show in log and how often they failed", async () => {
  const asked = draw();
  await settle();
  expect(names()).toEqual(["sepa::retry::gives_up_after_max_attempts", "sepa::retry::backs_off_exponentially", "ledger::booking::ledger_books_payout"]);
  const detail = target.querySelector(".tf-tdetail")!;
  expect(detail.querySelector("pre")?.textContent).toContain("at tests/sepa_retry.rs:14:5");
  expect(detail.querySelector("a")?.textContent).toBe("Show in log");
  expect(detail.querySelector("a")?.getAttribute("href")).toBe("#/build/p/r/log");
  expect(detail.querySelector(".tf-tacts .tf-dim")?.textContent).toBe("Failed in 2 of the last 10 runs, both on this branch.");
  expect(asked).toEqual(["2"]);
  expect(target.querySelector(".tf-trow .tf-chip.tf-err")?.textContent).toBe("failed");
  expect(target.querySelectorAll(".tf-trow")[1]?.querySelector(".tf-dim")?.textContent).toBe("420 ms");
});

test("a failed test folds closed and open again from its name", async () => {
  draw();
  await settle();
  const name = target.querySelector<HTMLButtonElement>(".tf-trow .tf-nm button")!;
  expect(name.getAttribute("aria-expanded")).toBe("true");
  name.click();
  flushSync();
  expect(target.querySelector(".tf-tdetail")).toBeNull();
  name.click();
  flushSync();
  expect(target.querySelector(".tf-tdetail")).not.toBeNull();
});

test("the filter bar keeps by result, module and words, and counts what it kept", async () => {
  draw();
  await settle();
  chip("Passed").click();
  flushSync();
  expect(names()).toEqual(["sepa::retry::backs_off_exponentially", "ledger::booking::ledger_books_payout"]);
  chip("Passed").click();
  const select = target.querySelector<HTMLSelectElement>("select")!;
  select.value = "ledger::booking";
  select.dispatchEvent(new Event("change", { bubbles: true }));
  flushSync();
  expect(names()).toEqual(["ledger::booking::ledger_books_payout"]);
  select.value = "";
  select.dispatchEvent(new Event("change", { bubbles: true }));
  const input = target.querySelector<HTMLInputElement>("input")!;
  input.value = "backs";
  input.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
  expect(names()).toEqual(["sepa::retry::backs_off_exponentially"]);
  expect(target.textContent).toContain("1 of 3 tests");
});

test("the tests filter draws no / hint: / belongs to the page's pipeline filter (#1453)", async () => {
  draw();
  await settle();
  expect(target.querySelector(".tf-fbar")).not.toBeNull();
  expect(target.querySelector(".tf-fbar kbd")).toBeNull();
});

test("a long run draws the tests near the viewport, open failures with their assertion, and the room of the rest below (#2210)", async () => {
  const failed = Array.from({ length: 3 }, (_, i) => t(`f${i}`, `bulk::failing_${i}`, "failed", "bulk", `assertion ${i} failed`));
  const passed = Array.from({ length: 2000 }, (_, i) => t(`p${i}`, `bulk::case_${i}`, "passed", "bulk"));
  draw([...failed, ...passed]);
  await settle();
  const drawn = names().length;
  expect(drawn).toBeGreaterThan(5);
  expect(drawn).toBeLessThan(100);
  expect(names().slice(0, 4)).toEqual(["bulk::failing_0", "bulk::failing_1", "bulk::failing_2", "bulk::case_0"]);
  expect([...target.querySelectorAll(".tf-tdetail pre")].map((pre) => pre.textContent)).toEqual([
    "assertion 0 failed",
    "assertion 1 failed",
    "assertion 2 failed",
  ]);
  expect(target.textContent).toContain("2003 tests");
  const list = target.querySelector<HTMLElement>(".tf-tres")!;
  expect(list.style.getPropertyValue("padding-top")).toBe("");
  expect(list.style.getPropertyValue("padding-bottom")).not.toBe("");

  // A filter that keeps one test draws it alone, with no room around it.
  const input = target.querySelector<HTMLInputElement>("input")!;
  input.value = "case_1999";
  input.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
  expect(names()).toEqual(["bulk::case_1999"]);
  expect(list.style.getPropertyValue("padding-bottom")).toBe("");
});
