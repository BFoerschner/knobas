import { describe, expect, it } from "vitest";
import { markNeedles } from "../filter/filter";
import { needles } from "./needles";

describe("needles", () => {
  it("keeps a quoted phrase whole", () => {
    expect(needles('"sepa retry" batch')).toEqual(["sepa retry", "batch"]);
  });
  it("leaves out -word and -phrase", () => {
    expect(needles('sepa -blocked -"retry batch"')).toEqual(["sepa"]);
  });
  it("leaves out or, in any case, and marks no word for it", () => {
    expect(needles("sepa OR retry or")).toEqual(["sepa", "retry"]);
    expect(markNeedles("error for the ledger", needles("or")).every((run) => !run.hit)).toBe(true);
  });
  it("leaves out the keys, negated or not, and markers", () => {
    expect(needles("project:PAY -status:Done @mara /jira kind:pr -@x sepa")).toEqual(["sepa"]);
  });
  it("keeps an unknown key as a word and #key as its rest", () => {
    expect(needles("jira:PAY-231 #PAY-7")).toEqual(["jira:pay-231", "pay-7"]);
  });
  it("says each once, lowercased, without edge punctuation", () => {
    expect(needles("Sepa, sepa (SEPA)")).toEqual(["sepa"]);
  });
});

describe("markNeedles", () => {
  it("marks a phrase across its space", () => {
    expect(markNeedles("Retry the sepa retry now", ["sepa retry"])).toEqual([
      { text: "Retry the ", hit: false },
      { text: "sepa retry", hit: true },
      { text: " now", hit: false },
    ]);
  });
});
