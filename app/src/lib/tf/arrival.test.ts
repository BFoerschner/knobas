/** Opening at the match (D12): the launcher's hand-over to the page a row opens. */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { ARRIVAL_MS, arrival, arrivalOf, holdsPiece, pieceOf } from "./arrival.svelte";

const what = { words: ["adproxy"], passage: "The retry budget stops behind ADProxy, so the payout waits." };

describe("arrival", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => {
    arrival.clear();
    vi.useRealTimers();
  });

  it("is taken once, by its address", () => {
    arrival.set("#/ticket/jira%3APAY-1", what);
    expect(arrival.waiting("#/ticket/jira%3APAY-1")).toBe(true);
    expect(arrival.take("#/ticket/jira%3APAY-2")).toBeNull();
    expect(arrival.take("#/ticket/jira%3APAY-1")).toEqual(what);
    expect(arrival.take("#/ticket/jira%3APAY-1")).toBeNull();
    expect(arrival.waiting("#/ticket/jira%3APAY-1")).toBe(false);
  });

  it("is taken by an address under its own, never by one that only starts the same", () => {
    arrival.set("#/pr/x%231", what);
    expect(arrival.take("#/pr/x%2310")).toBeNull();
    expect(arrival.take("#/pr/x%231/files")).toEqual(what);
  });

  it("is not taken after five seconds", () => {
    arrival.set("#/page/p", what);
    vi.advanceTimersByTime(ARRIVAL_MS + 1);
    expect(arrival.waiting("#/page/p")).toBe(false);
    expect(arrival.take("#/page/p")).toBeNull();
  });

  it("is still taken just inside five seconds", () => {
    arrival.set("#/page/p", what);
    vi.advanceTimersByTime(ARRIVAL_MS - 10);
    expect(arrival.take("#/page/p")).toEqual(what);
  });
});

describe("arrivalOf", () => {
  it("keeps the marked words distinct and lower-cased, and the text without its ellipses", () => {
    const got = arrivalOf({
      segments: [
        { text: "…times out behind ", hit: false },
        { text: "ADProxy", hit: true },
        { text: ", then ", hit: false },
        { text: "adproxy", hit: true },
        { text: " and ", hit: false },
        { text: "Retry", hit: true },
        { text: " st…", hit: false },
      ],
    });
    expect(got).toEqual({ words: ["adproxy", "retry"], passage: "times out behind ADProxy, then adproxy and Retry st" });
  });
});

describe("pieceOf and holdsPiece", () => {
  it("takes 24 characters around the first word, compared by letters and digits alone", () => {
    const piece = pieceOf(what);
    expect(piece.length).toBeLessThanOrEqual(24);
    expect(piece).toContain("adproxy");
    expect(holdsPiece("<p>The retry budget stops behind **ADProxy**, so the payout waits.</p>", piece)).toBe(true);
    expect(holdsPiece("Another ADProxy mention elsewhere.", piece)).toBe(false);
  });

  it("is the whole passage where it is short", () => {
    expect(pieceOf({ words: ["x"], passage: "Short x." })).toBe("shortx");
  });

  it("holds nothing for an empty piece", () => {
    expect(holdsPiece("anything", "")).toBe(false);
  });
});
