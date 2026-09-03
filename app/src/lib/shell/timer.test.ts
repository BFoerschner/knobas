/**
 * The two timer rules with content in them: what may be a target, and what the
 * elapsed reading says (issue #278).
 *
 * Pure, so they are driven without a bridge, a clock or a mounted component —
 * `timer.svelte.ts` owns the state and `TimerPicker.svelte` owns the drawing,
 * and both read these.
 */
import { expect, test } from "vitest";

import {
  canBeTarget,
  elapsedReading,
  legalCandidates,
  refuseAsTarget,
  targetReading,
  type TargetCandidate,
} from "./timer";

function candidate(entityId: string, kind: string, title = entityId): TargetCandidate {
  return { entityId, kind, title };
}

// -- what may be a target ---------------------------------------------------

/**
 * Story 15, and the direction that makes it a rule rather than an accident: a
 * context is **refused**, with a reason, not merely missing from some list.
 *
 * Both spellings are checked separately, because either one alone has to be
 * enough — a candidate that lost its kind on the way here is still a context,
 * and so is one whose kind says so while its id is something else.
 */
test("a stored context is refused as a timer target, and the refusal says why", () => {
  const byBoth = refuseAsTarget(candidate("ctx:5b1c0f1e", "ctx"));
  expect(byBoth).toContain("a set");

  expect(refuseAsTarget({ entityId: "ctx:5b1c0f1e" })).toBe(byBoth);
  expect(refuseAsTarget({ entityId: "jira:PAY-231", kind: "ctx" })).toBe(byBoth);
  // Case is not a way around it: `NOTE:` and `note:` address the same
  // namespace to every human reading them, and so do `CTX:` and `ctx:`.
  expect(refuseAsTarget({ entityId: "CTX:5b1c0f1e" })).toBe(byBoth);
});

/**
 * ...and the other direction, which is what says the guard is not simply
 * refusing everything. Story 14: a note, a page, a repo and an asset are all
 * legal targets, and so is a kind knobas has never heard of — §3a forbids a
 * table of every kind an adapter will ever emit, so this is a refusal list.
 */
test("every other entity is a legal target, including a kind knobas has never seen", () => {
  for (const [id, kind] of [
    ["jira:PAY-231", "ticket"],
    ["note:5b1c0f1e", "note"],
    ["confluence:ENG:SEPA design", "page"],
    ["gitea:tidewater/payout-service", "repo"],
    ["pagerduty:INC-9", "incident"],
  ] as const) {
    expect(refuseAsTarget(candidate(id, kind)), `${id} should be legal`).toBeNull();
    expect(canBeTarget(candidate(id, kind))).toBe(true);
  }
});

test("something that is not an entity id at all is refused too", () => {
  for (const id of ["DB config for the migration", "", ":PAY-231", "jira:", "jira:   "]) {
    expect(refuseAsTarget({ entityId: id }), `${id} should be refused`).not.toBeNull();
  }
});

/**
 * The picker's list, and the fixture that makes the assertion mean something:
 * the context sits **in the middle**, so an implementation that dropped the
 * first row, the last row, or every row would fail differently from one that
 * removed the right one.
 */
test("the picker's candidate list has the context taken out and nothing else", () => {
  const offered = legalCandidates([
    candidate("jira:PAY-231", "ticket", "Payments retry storm"),
    candidate("ctx:5b1c0f1e", "ctx", "SEPA migration"),
    candidate("note:9f21", "note", "Standup 2026-09-03"),
  ]);
  expect(offered.map((row) => row.entityId)).toEqual(["jira:PAY-231", "note:9f21"]);
});

// -- what the strip reads ---------------------------------------------------

test("an entity target reads as its key, and a label as itself", () => {
  expect(targetReading({ kind: "entity", entity_id: "jira:PAY-231" })).toBe("PAY-231");
  // The first colon only: a key is free to contain more of them.
  expect(targetReading({ kind: "entity", entity_id: "confluence:ENG:SEPA design" })).toBe(
    "ENG:SEPA design",
  );
  expect(targetReading({ kind: "label", label: "DB config for the migration" })).toBe(
    "DB config for the migration",
  );
});

test("the elapsed reading counts up, and grows an hours field only when there is one", () => {
  const started = "2026-09-03T09:00:00Z";
  const at = (iso: string) => elapsedReading(started, new Date(iso));

  expect(at("2026-09-03T09:00:00Z")).toBe("0:00");
  expect(at("2026-09-03T09:00:07Z")).toBe("0:07");
  expect(at("2026-09-03T09:45:12Z")).toBe("45:12");
  // The boundary, from both sides: 59:59 is still minutes, 1:00:00 is hours.
  expect(at("2026-09-03T09:59:59Z")).toBe("59:59");
  expect(at("2026-09-03T10:00:00Z")).toBe("1:00:00");
  // ...and the seconds keep moving past the hour, which is the difference from
  // the sync countdown: this is the value story 16 asks to read as live.
  expect(at("2026-09-03T12:20:07Z")).toBe("3:20:07");
});

/**
 * A start in the future reads `0:00`, never a negative.
 *
 * `started_at` is the database's clock and `now` is the webview's; they
 * disagree by fractions of a second routinely, and `-0:01` is a fact about two
 * clocks that looks like a bug in knobas. The same decision `time.ts`'s `ago`
 * makes.
 */
test("a start in the future reads zero rather than a negative", () => {
  expect(elapsedReading("2026-09-03T09:00:05Z", new Date("2026-09-03T09:00:00Z"))).toBe("0:00");
});

test("an unreadable stamp reads zero rather than NaN", () => {
  expect(elapsedReading("not a date", new Date("2026-09-03T09:00:00Z"))).toBe("0:00");
});
