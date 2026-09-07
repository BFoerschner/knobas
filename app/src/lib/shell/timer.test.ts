/**
 * The two timer rules with content in them: what may be a target, and what the
 * elapsed reading says (issue #278).
 *
 * Pure, so they are driven without a bridge, a clock or a mounted component —
 * `timer.svelte.ts` owns the state and `TimerPicker.svelte` owns the drawing,
 * and both read these.
 */
import { readFileSync } from "node:fs";
import { join } from "node:path";

import { expect, test } from "vitest";

import {
  assetTargetId,
  canBeTarget,
  elapsedReading,
  legalCandidates,
  roomForeground,
  targetReading,
  type TargetCandidate,
} from "./timer";

function candidate(entityId: string, kind: string, title = entityId): TargetCandidate {
  return { entityId, kind, title };
}

// -- what is in front of a reader standing in a room ------------------------

/**
 * The ladder itself: the open detail, else the room's anchor, else nothing
 * (#278, spec #272), each rung through `canBeTarget`.
 *
 * The literal words rather than a re-derivation, and each rung driven with the
 * one below it *present*, because the mistake this rule can make is order: an
 * implementation that read the anchor first would pass a test that only ever
 * supplied one of the two.
 */
test("a room's foreground is the open detail, else its anchor, else nothing", () => {
  expect(roomForeground("mock:INC-1", "mock:PAY-231")).toBe("mock:INC-1");
  expect(roomForeground(null, "mock:PAY-231")).toBe("mock:PAY-231");
  expect(roomForeground(undefined, undefined)).toBe(null);
  expect(roomForeground(null, null)).toBe(null);
});

/**
 * Both rungs go through `canBeTarget`, so a context is nothing in front of
 * anybody — and the fall-through is to the **next** rung, not to `null`.
 *
 * A room whose anchor were somehow a context has to answer with the open
 * detail rather than with nothing, which is the direction a guard applied to
 * the wrong half would get backwards.
 */
test("a context is never a room's foreground, from either rung", () => {
  expect(roomForeground("ctx:5b1c0f1e", "mock:PAY-231")).toBe("mock:PAY-231");
  expect(roomForeground("ctx:5b1c0f1e", "ctx:2f1a5d6e")).toBe(null);
  expect(roomForeground("not-an-id", "mock:PAY-231")).toBe("mock:PAY-231");
});

/**
 * **One spelling of the ladder, scanned rather than remembered** (#502, the
 * deputy's ruling of 2026-09-08).
 *
 * Two readers ask what is in front of a reader standing in a room: the
 * heartbeat's foreground (`App.svelte`) and the `captured-from` link a note is
 * born with (`Room.svelte`). They must not be able to disagree, and calling
 * one function is what makes that structural — but only until somebody inlines
 * the two lines back into one of them, which is a change that would look local
 * and would silently make a captured note point at something the day review's
 * passive block for that minute does not.
 *
 * So the scan is the pin: neither file may reach the anchor through
 * `canBeTarget` itself, which is the shape a re-inlined ladder has. `App.svelte`
 * keeps its **assets** branch, which is a different rung of a different view,
 * so the pattern looked for is `anchorId` beside `canBeTarget` rather than
 * `canBeTarget` at all.
 */
test("the room foreground ladder is spelled once, and both readers call it", () => {
  const root = join(process.cwd(), "src");
  for (const file of ["App.svelte", "lib/shell/Room.svelte"]) {
    const source = readFileSync(join(root, file), "utf8");
    expect(source, `${file} has to go through roomForeground`).toContain("roomForeground(");
    const inlined = /anchorId[^\n]*canBeTarget|canBeTarget[^\n]*anchorId/.test(source);
    expect(inlined, `${file} re-spells the anchor rung instead of calling it`).toBe(false);
  }
});

// -- what may be a target ---------------------------------------------------

/**
 * Story 15, and the direction that makes it a rule rather than an accident: a
 * context is **refused** by the predicate, not merely missing from some list.
 *
 * Each spelling is driven on its own, because either alone has to be enough —
 * a candidate that lost its kind on the way here is still a context, and so is
 * one whose kind says so while its id is something else.
 */
test("a stored context is refused as a timer target, by id or by kind alone", () => {
  expect(canBeTarget(candidate("ctx:5b1c0f1e", "ctx"))).toBe(false);
  expect(canBeTarget({ entityId: "ctx:5b1c0f1e" })).toBe(false);
  expect(canBeTarget({ entityId: "jira:PAY-231", kind: "ctx" })).toBe(false);
  // Case is not a way around it: `NOTE:` and `note:` address the same
  // namespace to every human reading them, and so do `CTX:` and `ctx:`.
  expect(canBeTarget({ entityId: "CTX:5b1c0f1e" })).toBe(false);
  expect(canBeTarget({ entityId: "jira:PAY-231", kind: "CTX" })).toBe(false);
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
    expect(canBeTarget(candidate(id, kind)), `${id} should be legal`).toBe(true);
  }
});

test("something that is not an entity id at all is refused too", () => {
  for (const id of ["DB config for the migration", "", ":PAY-231", "jira:", "jira:   "]) {
    expect(canBeTarget({ entityId: id }), `${id} should be refused`).toBe(false);
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

/**
 * An asset is the one target whose key says nothing (#437).
 *
 * The other direction is what makes it a rule: a mirrored item whose *key*
 * begins with the word is not an asset, because the namespace is the half
 * before the first colon and nothing else. Without that, `jira:asset:7` would
 * send the strip looking for an asset that is not there.
 */
test("an asset target is the one the strip has to look up, matched on its namespace", () => {
  expect(assetTargetId({ kind: "entity", entity_id: "asset:9f3c" })).toBe("asset:9f3c");
  // Case-insensitively, the way every other namespace rule here is matched.
  expect(assetTargetId({ kind: "entity", entity_id: "ASSET:9f3c" })).toBe("ASSET:9f3c");
  expect(assetTargetId({ kind: "entity", entity_id: "jira:PAY-231" })).toBeNull();
  expect(assetTargetId({ kind: "entity", entity_id: "jira:asset:7" })).toBeNull();
  expect(assetTargetId({ kind: "label", label: "patching vm-db-01" })).toBeNull();
  expect(assetTargetId(null)).toBeNull();
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
