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
