import { expect, test } from "vitest";

import { boundedNumber, databaseWords, lastBackupWords, scheduleWords, shareTicks } from "./sections";

const NOW = new Date(2026, 9, 15, 14, 30);

test("the share ticks are the export's parts, in prototype-6's order, and no tick for a part that does not exist", () => {
  expect(shareTicks(true, true, true).map((tick) => [tick.part, tick.label])).toEqual([
    ["links", "Links"],
    ["link_sets", "Link sets"],
    ["ticket_views", "Ticket views"],
    ["contexts", "Contexts"],
    ["assets", "Assets and routes"],
    ["sources", "Sources, without passwords"],
    ["smart_lists", "Smart lists"],
    ["notes", "Notes"],
    ["time", "Time"],
  ]);
  // Smart lists only once one is saved, ticket views and link sets once one
  // exists (CONTEXT.md, *Share export*; #812, #870).
  expect(shareTicks(false, true, true).map((tick) => tick.label)).not.toContain("Smart lists");
  expect(shareTicks(true, true, false).map((tick) => tick.label)).not.toContain("Link sets");
  expect(shareTicks(true, false, true).map((tick) => tick.label)).not.toContain("Ticket views");
});

test("the last backup reads as prototype-6's line", () => {
  const dir = "~/Library/Application Support/knobas/backups";
  const at = new Date(2026, 9, 15, 2, 0).toISOString();
  expect(lastBackupWords({ taken_at: at, file: "a.knobas", bytes: 48 * 1024 * 1024 }, dir, NOW)).toBe(
    `today 02:00, 48 MB, in ${dir}`,
  );
  const before = new Date(2026, 9, 14, 2, 5).toISOString();
  expect(lastBackupWords({ taken_at: before, file: "a.knobas", bytes: 1024 }, dir, NOW)).toBe(
    `yesterday 02:05, 1 KB, in ${dir}`,
  );
  const older = new Date(2026, 9, 2, 23, 0).toISOString();
  // CHANGED (#1986): an older backup read "2 Oct 23:00", the one date in the shell spelled with a month name.
  expect(lastBackupWords({ taken_at: older, file: "a.knobas", bytes: 1024 }, dir, NOW)).toBe(
    `2026-10-02 23:00, 1 KB, in ${dir}`,
  );
  expect(lastBackupWords(null, dir, NOW)).toBe("None yet");
});

test("the schedule offers the stored time of day, or never", () => {
  expect(scheduleWords({ enabled: true, hour: 2, minute: 0, keep: 14 })).toBe("Every day at 02:00");
  expect(scheduleWords({ enabled: false, hour: 21, minute: 30, keep: 14 })).toBe("Every day at 21:30");
});

test("a number field never sends NaN, and stays in its range", () => {
  expect(boundedNumber("", 1, 365, 14)).toBe(14);
  expect(boundedNumber("0", 1, 365, 14)).toBe(1);
  expect(boundedNumber("900", 1, 365, 14)).toBe(365);
  expect(boundedNumber("30", 1, 365, 14)).toBe(30);
});

test("the database line: its size and how many items it holds", () => {
  expect(databaseWords({ db_bytes: 1.2 * 1024 ** 3, item_count: 18400 })).toBe("1.2 GB, 18 400 items");
  expect(databaseWords({ db_bytes: 2048, item_count: 1 })).toBe("2 KB, 1 item");
});
