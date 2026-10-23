import { describe, expect, it } from "vitest";
import { markNeedles } from "../filter/filter";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { needles, wordAlternatives, type WordAlternative } from "./needles";

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

// `crates/knobas-search/tests/fixtures/needle-parity.json` is read by the Rust
// test `needles_and_alternatives_match_the_shared_parity_fixture` too: a grammar
// change on either side fails the other until the fixture and both copies agree.
describe("parity with knobas_search::query", () => {
  const rows: { typed: string; needles: string[]; alternatives: WordAlternative[] }[] = JSON.parse(
    readFileSync(resolve(process.cwd(), "../crates/knobas-search/tests/fixtures/needle-parity.json"), "utf8",
    ),
  );
  it("has rows", () => expect(rows.length).toBeGreaterThan(10));
  for (const row of rows) {
    it(`reads ${JSON.stringify(row.typed)} as the server does`, () => {
      expect(needles(row.typed)).toEqual(row.needles);
      expect(wordAlternatives(row.typed)).toEqual(row.alternatives);
    });
  }
});
