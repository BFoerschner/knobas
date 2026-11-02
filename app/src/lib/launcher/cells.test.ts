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

describe("cellsOf reads the engine's key cell first (#2452)", () => {
  const withCell = (kind: string, cell: string | undefined, path: string | null = null) =>
    cellsOf({ kind, entityId: "src:PAY-7", title: "owner/name", path, kindWord: "Thing", cell });

  it.each(["pr", "page", "pipeline", "monitor", "note"])("a %s with a cell draws it", (kind) => {
    expect(withCell(kind, "http", "Handbook › Payments")).toEqual({ cell: "http", mono: false, title: "owner/name" });
  });

  it("a ticket keeps its key and a repository its owner", () => {
    expect(withCell("ticket", "http")).toEqual({ cell: "PAY-7", mono: true, title: "owner/name" });
    expect(withCell("repo", "http")).toEqual({ cell: "owner", mono: false, title: "name" });
  });

  it("an empty cell falls back to the path line", () => {
    expect(withCell("page", "", "Engineering › Payments").cell).toBe("Engineering");
  });
});
