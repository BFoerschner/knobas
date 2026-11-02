/**
 * The key cell's miss direction (#2421, ADR-0007): a kind with a rule whose
 * path is missing or empty says its kind's word, never a wrong place.
 */
import { describe, expect, it } from "vitest";
import { cellsOf } from "./cells";

const of = (kind: string, path: string | null, title = "Retry payouts") =>
  cellsOf({ kind, entityId: `src:${kind}:1`, title, path, kindWord: "Pull request" });

describe("cellsOf misses toward the kind's word", () => {
  it.each(["pr", "branch", "commit", "pipeline", "build_config", "page", "view", "monitor"])(
    "a %s with no path",
    (kind) => {
      expect(of(kind, null)).toEqual({ cell: "pull request", mono: false, title: "Retry payouts" });
    },
  );

  it("a path of separators alone", () => {
    expect(of("page", " › ")).toEqual({ cell: "pull request", mono: false, title: "Retry payouts" });
  });

  it("a repository whose title has no owner keeps its title", () => {
    expect(of("repo", null, "payout-service")).toEqual({
      cell: "pull request",
      mono: false,
      title: "payout-service",
    });
  });
});
