/**
 * The Standup tab, as values (#829): where a digest line opens, the digest as
 * text, and the protocol's three sections read out of and written back into
 * the note body the backend keeps.
 */
import { expect, test } from "vitest";

import type { DigestLine } from "../../ipc/entity";
import {
  digestText,
  lineHash,
  ownerOf,
  readProtocol,
  shortDay,
  withActionItem,
  withAttendees,
  withNote,
} from "./standup-view";

/** `knobas_app::protocol::template_of`, as a new protocol starts. */
const TEMPLATE = "# Standup 2026-08-22\n\n## Attendees\n\n- \n\n## Notes\n\n- \n\n## Action items\n\n- [ ] \n";

const FILLED = [
  "# Standup 2026-08-22",
  "",
  "## Attendees",
  "",
  "- Mara Lindqvist",
  "- Tomasz Wójcik",
  "",
  "## Notes",
  "",
  "- Mara Lindqvist: PAY-231 retries done, review open.",
  "- Something said to the room",
  "",
  "## Action items",
  "",
  "- [ ] Decide the manual-review SLA",
  "- [x] Rotate the key",
  "",
].join("\n");

function line(over: Partial<DigestLine>): DigestLine {
  return {
    entity_id: "jira:PAY-231",
    kind: "ticket",
    title: "Retry failed SEPA payouts",
    source: "jira",
    verb: "tracked",
    reason: "4h 30m",
    at: "2026-08-21T15:00:00Z",
    ...over,
  };
}

test("a new protocol has nobody, no notes and no action items", () => {
  expect(readProtocol(TEMPLATE)).toEqual({ attendees: [], notes: {}, items: [] });
});

test("a filled protocol reads its attendees, the notes under their names, and its action items", () => {
  expect(readProtocol(FILLED)).toEqual({
    attendees: ["Mara Lindqvist", "Tomasz Wójcik"],
    notes: { "Mara Lindqvist": "PAY-231 retries done, review open." },
    items: [
      { text: "Decide the manual-review SLA", done: false },
      { text: "Rotate the key", done: true },
    ],
  });
});

test("setting the attendees rewrites that section's bullets and nothing else", () => {
  const next = withAttendees(FILLED, ["Mara Lindqvist", "Priya Nair"]);
  expect(readProtocol(next).attendees).toEqual(["Mara Lindqvist", "Priya Nair"]);
  expect(next.replace("- Priya Nair\n", "").replace("- Mara Lindqvist\n", "- Mara Lindqvist\n- Tomasz Wójcik\n")).toBe(
    FILLED,
  );
});

test("the first attendee fills the template's empty bullet, and nobody leaves it again", () => {
  const one = withAttendees(TEMPLATE, ["Jonas Becker"]);
  expect(one).toContain("## Attendees\n\n- Jonas Becker\n\n## Notes");
  expect(withAttendees(one, [])).toBe(TEMPLATE);
});

test("a note replaces that person's bullet in place and keeps the others", () => {
  const next = withNote(FILLED, "Mara Lindqvist", "Review open, PAY-240 next.");
  expect(next).toBe(FILLED.replace("PAY-231 retries done, review open.", "Review open, PAY-240 next."));
});

test("a first note for someone is added after the last bullet; a note is one line", () => {
  const next = withNote(FILLED, "Tomasz Wójcik", "Runbook update\nthis afternoon.");
  expect(next).toContain("- Something said to the room\n- Tomasz Wójcik: Runbook update this afternoon.\n\n## Action items");
  expect(readProtocol(withNote(withAttendees(TEMPLATE, ["Jonas Becker"]), "Jonas Becker", "Off Monday")).notes).toEqual({ "Jonas Becker": "Off Monday" });
  expect(withNote(TEMPLATE, "Jonas Becker", "Off Monday")).not.toContain("- \n- Jonas");
});

test("emptying a note removes its bullet", () => {
  expect(withNote(FILLED, "Mara Lindqvist", "  ")).not.toContain("Mara Lindqvist:");
});

test("an action item is added as an open task under Action items", () => {
  expect(readProtocol(withActionItem(FILLED, "Ship it")).items.at(-1)).toEqual({ text: "Ship it", done: false });
  expect(withActionItem(TEMPLATE, "Ship it")).toBe(TEMPLATE.replace("- [ ] \n", "- [ ] Ship it\n"));
});

test("a body that lost a heading gets it back at the end", () => {
  const next = withAttendees("# Standup 2026-08-22\n", ["Mara Lindqvist"]);
  expect(next).toBe("# Standup 2026-08-22\n\n## Attendees\n\n- Mara Lindqvist\n");
});

test("a digest line opens its item in the new shell, and a timer label opens nothing", () => {
  expect(lineHash(line({}))).toBe("#/job/PAY-231");
  expect(lineHash(line({ entity_id: "gitea:payout-service#142", kind: "pr" }))).toBe("#/pr/gitea%3Apayout-service%23142");
  expect(lineHash(line({ entity_id: "confluence:98401", kind: "page" }))).toBe("#/page/confluence%3A98401");
  expect(lineHash(line({ entity_id: null, kind: null }))).toBeNull();
});

test("a day reads as its weekday and date", () => {
  expect(shortDay("2026-08-20")).toBe("Thursday 20");
});

test("the digest copies as text, one line each, with the lists' headings", () => {
  const text = digestText({
    yesterday_day: "2026-08-20",
    yesterday: [line({})],
    today: [line({ entity_id: null, kind: null, title: "DB config", reason: "timer running" })],
    blockers: [],
  });
  expect(text).toBe(
    [
      "Yesterday, Thursday 20",
      "- PAY-231 Retry failed SEPA payouts: 4h 30m",
      "",
      "Today",
      "- DB config (timer running)",
      "",
      "Blockers",
      "- None",
    ].join("\n"),
  );
});

test("an action item's trailing (Owner) is told apart from its words", () => {
  expect(ownerOf("Decide the manual-review SLA (Priya Nair)")).toEqual({ what: "Decide the manual-review SLA", who: "(Priya Nair)" });
  expect(ownerOf("Ship (v2) today")).toEqual({ what: "Ship (v2) today", who: null });
});
