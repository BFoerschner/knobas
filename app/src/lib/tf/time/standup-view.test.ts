/**
 * The Standup tab, as values (#829): where a digest line opens, the digest as
 * text, and the protocol's three sections read out of and written back into
 * the note body the backend keeps.
 */
import { expect, test, vi } from "vitest";

import { kindRegistry } from "../../shell/kind-registry.svelte";
import { page as codeRowPage } from "../code-row/page";
import { pageFor } from "../pages";
import { parseTfHash } from "../route";

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
  expect(lineHash(line({}))).toBe("#/job/jira%3APAY-231");
  expect(lineHash(line({ entity_id: "gitea:payout-service#142", kind: "pr" }))).toBe("#/pr/gitea%3Apayout-service%23142");
  expect(lineHash(line({ entity_id: "confluence:98401", kind: "page" }))).toBe("#/page/confluence%3A98401");
  expect(lineHash(line({ entity_id: "note:0f2c1a", kind: "note" }))).toBe("#/note/note%3A0f2c1a");
  // A protocol note opens the Standup at its date (#1504).
  expect(lineHash(line({ entity_id: "note:7a1", kind: "note", title: "Standup 2026-09-03" }))).toBe("#/time/standup/2026-09-03");
  expect(lineHash(line({ entity_id: "route:9a1b", kind: "route" }))).toBe("#/route/route%3A9a1b");
  expect(lineHash(line({ entity_id: null, kind: null }))).toBeNull();
});

test("a digest line's branch or commit opens its own address, which the repository page's redirect draws (#1440)", () => {
  const branch = lineHash(line({ entity_id: "gitea:x/pay@refs/heads/main", kind: "branch" }))!;
  const commit = lineHash(line({ entity_id: "mock:9f2c1ab", kind: "commit" }))!;
  expect([branch, commit]).toEqual(["#/branch/gitea%3Ax%2Fpay%40refs%2Fheads%2Fmain", "#/commit/mock%3A9f2c1ab"]);
  for (const href of [branch, commit]) expect(pageFor(parseTfHash(href)), href).toBe(codeRowPage);
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
    infrastructure: [
      line({ entity_id: "kuma:payout-api", kind: "monitor", title: "payout-api", reason: "was down 12 min", at: "2026-08-20T03:10:00" }),
    ],
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
      "",
      "Infrastructure",
      "- payout-api was down 12 min, from 03:10",
    ].join("\n"),
  );
});

test("an action item's trailing (Owner) is told apart from its words", () => {
  expect(ownerOf("Decide the manual-review SLA (Priya Nair)")).toEqual({ what: "Decide the manual-review SLA", who: "(Priya Nair)" });
  expect(ownerOf("Ship (v2) today")).toEqual({ what: "Ship (v2) today", who: null });
});

/** A protocol a person wrote by hand: prose and a wrapped tail between bullets, a section knobas does not know, no newline at the end. */
const HANDWRITTEN = [
  "# Standup 2026-08-22",
  "Ran long today.",
  "",
  "## Attendees",
  "- Mara Lindqvist",
  "(Priya joined late)",
  "- Tomasz Wójcik",
  "",
  "## Notes",
  "",
  "- Mara Lindqvist: PAY-231 retries done,",
  "  review open.",
  "Context for the room, not anyone's note.",
  "- Tomasz Wójcik: runbook",
  "",
  "### Parking lot",
  "* rotate the key *later*",
  "",
  "## Action items",
  "",
  "- [ ] Decide the manual-review SLA",
  "",
  "Mara owns the follow-up.",
].join("\n");

test("an edit changes only the bullets it is about; prose, wrapped tails, other sections and a missing last newline stay byte for byte", () => {
  expect(withNote(HANDWRITTEN, "Tomasz Wójcik", "runbook done")).toBe(
    HANDWRITTEN.replace("- Tomasz Wójcik: runbook\n", "- Tomasz Wójcik: runbook done\n"),
  );
  expect(withNote(HANDWRITTEN, "Mara Lindqvist", "merged")).toBe(
    HANDWRITTEN.replace("- Mara Lindqvist: PAY-231 retries done,\n", "- Mara Lindqvist: merged\n"),
  );
  expect(withNote(HANDWRITTEN, "Tomasz Wójcik", "")).toBe(HANDWRITTEN.replace("- Tomasz Wójcik: runbook\n", ""));
  expect(withAttendees(HANDWRITTEN, ["Mara Lindqvist", "Tomasz Wójcik", "Priya Nair"])).toBe(
    HANDWRITTEN.replace("- Tomasz Wójcik\n", "- Tomasz Wójcik\n- Priya Nair\n"),
  );
  expect(withAttendees(HANDWRITTEN, ["Tomasz Wójcik"])).toBe(HANDWRITTEN.replace("- Mara Lindqvist\n", ""));
  expect(withActionItem(HANDWRITTEN, "Ship it")).toBe(
    HANDWRITTEN.replace("- [ ] Decide the manual-review SLA\n", "- [ ] Decide the manual-review SLA\n- [ ] Ship it\n"),
  );
});

test("a new note goes after the last bullet and the lines that continue it", () => {
  const body = "## Attendees\n\n- A\n- B\n\n## Notes\n\n- A: long\n  and wrapped\n\nAfterword.\n";
  expect(withNote(body, "B", "short")).toBe(body.replace("  and wrapped\n", "  and wrapped\n- B: short\n"));
});

test("a CRLF body stays CRLF, on the lines an edit writes as well as the rest", () => {
  const crlf = FILLED.replaceAll("\n", "\r\n");
  expect(withNote(crlf, "Mara Lindqvist", "Merged.")).toBe(
    crlf.replace("PAY-231 retries done, review open.", "Merged."),
  );
  expect(withAttendees(crlf, ["Mara Lindqvist", "Tomasz Wójcik", "Priya Nair"])).toBe(
    crlf.replace("- Tomasz Wójcik\r\n", "- Tomasz Wójcik\r\n- Priya Nair\r\n"),
  );
  expect(withActionItem(crlf, "Ship it")).toBe(crlf.replace("- [x] Rotate the key\r\n", "- [x] Rotate the key\r\n- [ ] Ship it\r\n"));
  expect(withAttendees("# Standup\r\n", ["A"])).toBe("# Standup\r\n\r\n## Attendees\r\n\r\n- A\r\n");
  expect(readProtocol(crlf)).toEqual(readProtocol(FILLED));
});

test("a run of a kind the CI adapters claim opens Builds' run page (#1278)", () => {
  const spy = vi.spyOn(kindRegistry, "frameKinds").mockReturnValue(["build", "workflow_run"]);
  try {
    expect(lineHash(line({ entity_id: "forge:run#77", kind: "workflow_run" }))).toBe("#/build/forge%3Arun%2377");
  } finally {
    spy.mockRestore();
  }
});
