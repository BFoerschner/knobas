import { readFileSync } from "node:fs";
import { join } from "node:path";

import { describe, expect, it } from "vitest";

import { budgetFactor } from "./test-budget";

describe("budgetFactor (#2044)", () => {
  it("is one on a Mac with a core for each of `just check`'s eight threads", () => {
    expect(budgetFactor(12, {})).toBe(1);
    expect(budgetFactor(8, {})).toBe(1);
  });

  it("grows by threads over cores on a smaller machine off the runner", () => {
    expect(budgetFactor(4, {})).toBe(2);
    expect(budgetFactor(2, {})).toBe(4);
  });

  it("is four on the GitHub runner whatever its core count says", () => {
    expect(budgetFactor(4, { GITHUB_ACTIONS: "true" })).toBe(4);
    expect(budgetFactor(16, { GITHUB_ACTIONS: "true" })).toBe(4);
  });

  it("keeps the larger figure on a runner with fewer cores still", () => {
    expect(budgetFactor(1, { GITHUB_ACTIONS: "true" })).toBe(8);
  });

  it("reads only the value true as the runner", () => {
    expect(budgetFactor(12, { GITHUB_ACTIONS: "false" })).toBe(1);
  });
});

describe("the store reset's budget (#2359)", () => {
  // The first reset in a file imports every store in `test-resets.ts`, and
  // through the three page components most of the app: 1.7 s alone and idle,
  // all of it transform. A run of one file with load 70-130 beside it took
  // 12.6-13.8 s, past Vitest's 10 s hook budget, and failed a test whose own
  // work took no time. So that one hook takes the whole tree's budget.
  it("is the whole tree's budget, not the hook default", () => {
    const setup = readFileSync(join(process.cwd(), "src/lib/shell/test-setup.ts"), "utf8");
    expect(setup).toMatch(/^afterEach\(resetStores, wholeTreeBudget\(\)\);$/m);
  });
});
