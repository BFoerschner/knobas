/**
 * A pull request's state as a write this session queued leaves it (the pr-state fix, 2026-11-24): drawn over a read
 * that still says what it was until the write's read-back ended, on the window's clock alone, and gone with a write the
 * queue will not send.
 */
import { afterEach, expect, test, vi } from "vitest";

import { echoWrite, forgetEchoes } from "../queued.svelte";
import {
  QUIET_AFTER_MS,
  dropPrMove,
  forgetPrMoves,
  heardRefresh,
  heardRun,
  notePrMove,
  notePrSent,
  prMoveOf,
  prStateOver,
  stateLeft,
  withPrStates,
} from "./pr-state.svelte";

const PR = "gitea:tidewater/payout-service#142";
const OTHER = "gitea:tidewater/payout-service#143";
const OPEN = { text: "open", tone: "info" as const };
const MERGED = { text: "merged", tone: "ok" as const };

afterEach(() => {
  forgetPrMoves();
  forgetEchoes();
  vi.useRealTimers();
});

test("each move leaves the forge's word and tone", () => {
  expect(stateLeft("merge")).toEqual(MERGED);
  expect(stateLeft("close")).toEqual({ text: "closed", tone: "muted" });
  expect(stateLeft("reopen")).toEqual(OPEN);
});

test("rows asked before the read-back are drawn merged; rows asked after it say what they say", () => {
  notePrMove(PR, "merge", "Open", 7, "gitea");
  expect(prStateOver(PR, OPEN, { askedAt: 1_000 })).toEqual(MERGED);
  heardRefresh({ entity_id: PR, write_id: 7, ended: "refreshed" }, 5_000);
  expect(prMoveOf(PR)?.landedAt).toBe(5_000);
  expect(prStateOver(PR, OPEN, { askedAt: 4_999 }), "asked before it ended").toEqual(MERGED);
  expect(prStateOver(PR, OPEN, { askedAt: 5_000 }), "asked after: the mirror's word").toEqual(OPEN);
  expect(prStateOver(OTHER, OPEN, { askedAt: 1_000 }), "another pull request is untouched").toEqual(OPEN);
});

test("the forge's updated_at plays no part: a row it stamped after the queue time still draws the move until the read-back (Fable's veto)", () => {
  notePrMove(PR, "merge", "open", 7, "gitea");
  const rows = [{ entity_id: PR, state: OPEN, updated_at: "2999-01-01T00:00:00Z" }];
  expect(withPrStates(rows, Date.now())[0]!.state).toEqual(MERGED);
});

test("a refresh of another write or a failed one ends nothing; the end of the first run that began after the send does", () => {
  notePrMove(PR, "close", "open", 7, "gitea");
  heardRefresh({ entity_id: PR, write_id: 8, ended: "refreshed" }, 2_000);
  heardRefresh({ entity_id: PR, write_id: 7, ended: "failed" }, 2_000);
  heardRun({ source_id: "gitea", running: false, run_id: 1, started_at: null }, 2_000);
  expect(prMoveOf(PR)?.landedAt).toBeNull();
  notePrSent(PR, 7, 3_000);
  // The run going at the send ends first: the write's read-back waits behind it, so it ends nothing.
  heardRun({ source_id: "gitea", running: true, run_id: 2, started_at: new Date(2_500).toISOString() }, 3_100);
  heardRun({ source_id: "gitea", running: false, run_id: 2, started_at: new Date(2_500).toISOString() }, 3_500);
  heardRun({ source_id: "jira", running: true, run_id: 3, started_at: new Date(3_600).toISOString() }, 3_600);
  heardRun({ source_id: "jira", running: false, run_id: 3, started_at: new Date(3_600).toISOString() }, 3_700);
  expect(prMoveOf(PR)?.landedAt).toBeNull();
  heardRun({ source_id: "gitea", running: true, run_id: 4, started_at: new Date(4_000).toISOString() }, 4_000);
  heardRun({ source_id: "gitea", running: false, run_id: 4, started_at: new Date(4_000).toISOString() }, 6_000);
  expect(prMoveOf(PR)?.landedAt).toBe(6_000);
});

test("a read that already says another state is drawn as it says", () => {
  notePrMove(PR, "close", "open", 7, "gitea");
  expect(prStateOver(PR, MERGED, { askedAt: 0 })).toEqual(MERGED);
});

test("a surface that does not say when it asked draws the move until 10 s after the read-back", () => {
  vi.useFakeTimers();
  notePrMove(PR, "merge", "open", 7, "gitea");
  expect(prStateOver(PR, OPEN)).toEqual(MERGED);
  heardRefresh({ entity_id: PR, write_id: 7, ended: "refreshed" });
  vi.advanceTimersByTime(QUIET_AFTER_MS - 1);
  expect(prStateOver(PR, OPEN)).toEqual(MERGED);
  vi.advanceTimersByTime(1);
  expect(prStateOver(PR, OPEN)).toEqual(OPEN);
});

test("the page's live view is drawn over while the page's echo of the write stands", () => {
  notePrMove(PR, "merge", "open", 7, "gitea");
  expect(prStateOver(PR, OPEN, { live: true }), "no echo: the live view says").toEqual(OPEN);
  echoWrite(PR, "merge", "merge", { write: { id: 7 } });
  expect(prStateOver(PR, OPEN, { live: true })).toEqual(MERGED);
  expect(prStateOver(PR, null, { live: true }), "nothing read yet").toEqual(MERGED);
});

test("a write the queue will not send takes its move with it; another write's drop does not", () => {
  notePrMove(PR, "merge", "open", 7, "gitea");
  dropPrMove(PR, 8);
  expect(prStateOver(PR, OPEN, { askedAt: 0 })).toEqual(MERGED);
  dropPrMove(PR, 7);
  expect(prStateOver(PR, OPEN, { askedAt: 0 })).toEqual(OPEN);
});

test("a second move replaces the first; rows nothing moved come back as the same array", () => {
  notePrMove(PR, "close", "open", 7, "gitea");
  notePrMove(PR, "reopen", "closed", 9, "gitea");
  expect(prStateOver(PR, { text: "closed", tone: "muted" }, { askedAt: 0 })).toEqual(OPEN);
  dropPrMove(PR, 7);
  expect(prStateOver(PR, { text: "closed", tone: "muted" }, { askedAt: 0 }), "the old write's drop is not this move's").toEqual(OPEN);
  const rows = [{ entity_id: OTHER, state: OPEN }];
  expect(withPrStates(rows, 0)).toBe(rows);
});
