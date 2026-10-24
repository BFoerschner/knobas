/**
 * Every duration and clock reading the app shows, pinned in one table (#1678).
 *
 * Each row is the string a surface showed before the consolidation, except
 * the rows marked CHANGED, which are the readings the consolidation chose on
 * purpose (listed in the PR).
 */
import { describe, expect, test } from "vitest";

import { ageLabel, clockLabel, keyOf, minutesLabel, secondsLabel, stampLabel } from "./format";
import { minutesOf } from "./week";

describe("minutesLabel: Day, Week, Log all, the launcher board, the draft button", () => {
  test.each([
    [0, "0m"],
    [45, "45m"],
    [60, "1h"],
    [130, "2h 10m"],
    [257, "4h 17m"],
  ])("%d minutes reads %s", (minutes, text) => {
    expect(minutesLabel(minutes)).toBe(text);
  });

  test("the job card's seconds are floored to whole minutes first: 163 s is 2m, 59 s is 0m", () => {
    expect([163, 59, 120].map((seconds) => minutesLabel(minutesOf(seconds)))).toEqual(["2m", "0m", "2m"]);
  });

  // CHANGED: AdHocBlock and the write queue said "2 h 15 min"; they now agree with every other page.
  test("a span of 135 minutes reads 2h 15m everywhere, not 2 h 15 min", () => {
    expect([0, 45, 60, 135].map(minutesLabel)).toEqual(["0m", "45m", "1h", "2h 15m"]);
  });
});

describe("secondsLabel: a run's length", () => {
  test.each([
    [38, "38 s"],
    [252, "4 m 12 s"],
    [65, "1 m 05 s"],
  ])("%d seconds reads %s", (seconds, text) => {
    expect(secondsLabel(seconds)).toBe(text);
  });

  // CHANGED: Settings said "3 min 12 s" for a run Builds called "3 m 12 s".
  test("Builds and Settings agree on a run's length: 192 s is 3 m 12 s on both", () => {
    expect(secondsLabel(192)).toBe("3 m 12 s");
    expect(secondsLabel(180)).toBe("3 m 00 s");
  });
});

describe("clockLabel: the ticket board, the inbox, builds", () => {
  const now = new Date(2026, 7, 22, 15, 0);
  const at = (day: number, hour: number, minute: number) => new Date(2026, 7, day, hour, minute).toISOString();

  test("the ticket board's reading", () => {
    expect(clockLabel(at(22, 11, 48), now)).toBe("today 11:48");
    expect(clockLabel(at(21, 16, 5), now)).toBe("yesterday 16:05");
    expect(clockLabel(at(18, 10, 0), now)).toBe("2026-08-18");
    expect(clockLabel(null, now)).toBe("never");
    expect(clockLabel("not a date", now)).toBe("");
  });

  test("the inbox column drops the word on today", () => {
    expect(clockLabel(at(22, 13, 50), now, true)).toBe("13:50");
    expect(clockLabel(at(18, 9, 5), now, true)).toBe("2026-08-18");
  });

  // CHANGED: the inbox and builds said "yest. 16:30" for the instant the board calls "yesterday 16:30".
  test("the inbox and the ticket board agree on yesterday", () => {
    const instant = at(21, 16, 30);
    expect(clockLabel(instant, now, true)).toBe("yesterday 16:30");
    expect(clockLabel(instant, now, true)).toBe(clockLabel(instant, now));
  });
});

describe("stampLabel: the conversation, the last backup, a source's sync runs (#1986)", () => {
  const now = new Date(2026, 7, 22, 15, 0);
  const at = (day: number, hour: number, minute: number) => new Date(2026, 7, day, hour, minute).toISOString();

  test("an older day keeps its clock, in clockLabel's date", () => {
    expect(stampLabel(at(22, 9, 35), now)).toBe("today 09:35");
    expect(stampLabel(at(21, 17, 41), now)).toBe("yesterday 17:41");
    expect(stampLabel(at(12, 9, 14), now)).toBe("2026-08-12 09:14");
    expect(stampLabel(at(12, 9, 14), now).startsWith(clockLabel(at(12, 9, 14), now))).toBe(true);
    expect(stampLabel("not a date", now)).toBe("");
  });

  test("bare drops the word on today only", () => {
    expect(stampLabel(at(22, 9, 0), now, true)).toBe("09:00");
    expect(stampLabel(at(21, 17, 41), now, true)).toBe("yesterday 17:41");
    expect(stampLabel(at(12, 9, 14), now, true)).toBe("2026-08-12 09:14");
  });

  test("yesterday across a month's first day", () => {
    const first = new Date(2026, 9, 1, 8, 0);
    expect(stampLabel(new Date(2026, 8, 30, 23, 10).toISOString(), first)).toBe("yesterday 23:10");
    expect(stampLabel(new Date(2026, 8, 29, 23, 10).toISOString(), first)).toBe("2026-09-29 23:10");
  });
});

describe("ageLabel: how old a source's data is, the stale line and the status bar (#1986)", () => {
  const now = new Date("2026-08-22T15:00:00Z");
  const before = (minutes: number) => new Date(now.getTime() - minutes * 60_000).toISOString();

  test.each([
    [0, "1 min"],
    [38, "38 min"],
    [59, "59 min"],
    [60, "1 h"],
    [150, "3 h"],
    [47 * 60, "47 h"],
    [48 * 60, "2 days"],
    [80 * 60, "3 days"],
  ])("%i minutes -> %s", (minutes, text) => expect(ageLabel(before(minutes), now)).toBe(text));
});

test("keyOf is the key half of an entity id, and the whole string when there is no source", () => {
  expect(keyOf("jira:PAY-231")).toBe("PAY-231");
  expect(keyOf("confluence:ENG:SEPA payout")).toBe("ENG:SEPA payout");
  expect(keyOf("PAY-231")).toBe("PAY-231");
});
