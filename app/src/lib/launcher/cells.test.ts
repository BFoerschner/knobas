/**
 * The key cell's miss direction (#2421, ADR-0007): a kind with a rule whose
 * path is missing or empty says its kind's word, never a wrong place.
 */
import { describe, expect, it } from "vitest";
import { cellsOf, cutLead, whyOf } from "./cells";

const of = (kind: string, path: string | null, title = "Retry payouts") =>
  cellsOf({ kind, entity_id: `src:${kind}:1`, title, path }, "Pull request");

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
    cellsOf({ kind, entity_id: "src:PAY-7", title: "owner/name", path, cell }, "Thing");

  it.each(["pr", "page", "pipeline", "monitor", "note"])("a %s with a cell draws it", (kind) => {
    expect(withCell(kind, "http", "Handbook › Payments")).toEqual({ cell: "http", mono: false, title: "owner/name" });
  });

  it("a pull request's title starts with its number, read off its id (F-654)", () => {
    const row = { kind: "pr", entity_id: "gitea:tidewater/payout-service#142", title: "Backoff" };
    expect(cellsOf({ ...row, path: "tidewater/payout-service", cell: "payout-service" }, "Pull request")).toEqual({ cell: "payout-service", mono: false, title: "#142 Backoff" });
    expect(cellsOf({ ...row, path: "tidewater/payout-service" }, "Pull request").title).toBe("#142 Backoff");
    expect(cellsOf({ ...row, entity_id: "gitea:tidewater/payout-service", path: null }, "Pull request").title).toBe("Backoff");
  });

  it("a ticket keeps its key and a repository its owner", () => {
    expect(withCell("ticket", "http")).toEqual({ cell: "PAY-7", mono: true, title: "owner/name" });
    expect(withCell("repo", "http")).toEqual({ cell: "owner", mono: false, title: "name" });
  });

  it("an empty cell falls back to the path line", () => {
    expect(withCell("page", "", "Engineering › Payments").cell).toBe("Engineering");
  });
});

describe("whyOf, the evidence line (D10)", () => {
  const seg = (text: string, hit = false) => ({ text, hit });
  const hit = (place: string | null, segments = [seg("The "), seg("payout", true), seg(" retried.")], marks: number[] = []) => ({
    excerpts: [{ place, segments }, { place: "Comment by Priya", segments: [seg("other")] }],
    marks,
  });

  it("quotes the first excerpt under its place", () => {
    for (const place of ["Description", "Comment by Jonas Becker", "Backoff policy"]) {
      const why = whyOf(hit(place))!;
      expect(why.place).toBe(place);
      expect(why.segments).toEqual([seg("The "), seg("payout", true), seg(" retried.")]);
      expect(why.code).toBe(false);
      expect(why.count).toBeNull();
    }
  });

  it("names no place where the passage says it itself", () => {
    expect(whyOf(hit(null))!.place).toBeNull();
    expect(whyOf(hit("Heading"))!.place).toBeNull();
  });

  it("draws a log's line in the mono face under its number", () => {
    const why = whyOf(hit("Log line 212"))!;
    expect(why.place).toBe("Log line 212");
    expect(why.code).toBe(true);
  });

  it("is null for a row with marks, and for one with no excerpt", () => {
    expect(whyOf(hit("Description", undefined, [0, 3]))).toBeNull();
    expect(whyOf({ excerpts: [] })).toBeNull();
    expect(whyOf({ excerpts: [], marks: [] })).toBeNull();
  });

  it("is null where the key cell holds every quoted word: the key is its own reason (D9, row 18)", () => {
    const keyed = { excerpts: [{ place: "Description", segments: [seg("Blocked by "), seg("PAY-231", true), seg(".")] }] };
    expect(whyOf(keyed, "PAY-231")).toBeNull();
    expect(whyOf(keyed, "PAY-232")).not.toBeNull();
    expect(whyOf(keyed)).not.toBeNull();
  });

  it("keeps the excerpt it quotes, for opening at it", () => {
    const h = hit("Description");
    expect(whyOf(h)!.excerpt).toBe(h.excerpts[0]);
  });
});

describe("cutLead", () => {
  const seg = (text: string, hit = false) => ({ text, hit });

  it("leaves a lead of 18 characters and less, and a passage that opens on a mark", () => {
    expect(cutLead([seg("eighteen chars ok "), seg("x", true)])).toEqual([seg("eighteen chars ok "), seg("x", true)]);
    expect(cutLead([seg("x", true), seg(" ".repeat(40))])).toEqual([seg("x", true), seg(" ".repeat(40))]);
  });

  it("cuts a longer lead to an ellipsis and its last 16 characters, from a word's start", () => {
    const cut = cutLead([seg("The certificate on the old directory host "), seg("adproxy", true), seg("-01 expires")]);
    expect(cut[0]).toEqual(seg("…directory host "));
    expect(Array.from(cut[0]!.text).length).toBeLessThanOrEqual(17);
    expect(cut.slice(1)).toEqual([seg("adproxy", true), seg("-01 expires")]);
  });

  it("starts at a word inside the kept 16, never mid-word", () => {
    const cut = cutLead([seg("aaaaaaaaaaaaaaaaaaaaaa bbbbbbbbbbbbb cc "), seg("x", true)]);
    expect(cut[0]!.text).toBe("…cc ");
  });

  it("keeps the last 16 as they are where no word starts in them", () => {
    const cut = cutLead([seg("a".repeat(40)), seg("x", true)]);
    expect(cut[0]!.text).toBe(`…${"a".repeat(16)}`);
  });
});
