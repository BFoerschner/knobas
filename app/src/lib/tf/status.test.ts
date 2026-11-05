import { expect, test } from "vitest";

import type { AuthState, CredentialHealth, SourceSyncStatus } from "../ipc/sources";
import { healthyRollup, queueLabel, queuedWords, sourceReadings, type ReadingTone, type SourceReading } from "./status";

const NOW = new Date("2026-08-22T14:32:00Z");

function health(id: string, state: AuthState, checked = "2026-08-22T13:54:00Z"): CredentialHealth {
  return { source_id: id, state, checked_at: checked, detail: null, secret_expires_at: null };
}

function synced(id: string, finished: string | null, outcome: SourceSyncStatus["last_outcome"] = "ok"): SourceSyncStatus {
  return {
    source_id: id,
    running: false,
    run_id: null,
    started_at: null,
    last_finished_at: finished,
    last_outcome: outcome,
    next_run_at: null,
    backoff_until: null,
    last_changed: null,
  };
}

const SOURCES = [
  { id: "jira", display_name: "Jira" },
  { id: "confluence", display_name: "Confluence" },
  { id: "teamcity", display_name: "TeamCity" },
];

test("reads like prototype-6's status bar: the first says synced, a 401 says so", () => {
  const readings = sourceReadings(
    SOURCES,
    [health("jira", "ok"), health("confluence", "ok"), health("teamcity", "unauthorized")],
    [
      synced("jira", "2026-08-22T14:28:00Z"),
      synced("confluence", "2026-08-22T14:28:00Z"),
      synced("teamcity", "2026-08-22T13:54:00Z", "unauthorized"),
    ],
    NOW,
  );
  expect(readings).toEqual([
    { id: "jira", text: "Jira synced 4 min ago", tone: "ok", syncedAt: "2026-08-22T14:28:00Z", fix: null },
    { id: "confluence", text: "Confluence 4 min ago", tone: "ok", syncedAt: "2026-08-22T14:28:00Z", fix: null },
    { id: "teamcity", text: "TeamCity: sign-in failed 38 min ago (401)", tone: "err", syncedAt: "2026-08-22T13:54:00Z", fix: "Re-enter password" },
  ]);
});

test("a missing credential needs the reader too, in the source's noun; an unreachable server does not", () => {
  const readings = sourceReadings(
    [
      { id: "gitea", display_name: "Gitea", auth_kind: "Pat" },
      { id: "kuma", display_name: "Uptime Kuma", auth_kind: "ApiToken" },
      { id: "confluence", display_name: "Confluence", auth_kind: "UserPassword" },
    ],
    [health("gitea", "unreachable"), health("kuma", "missing_secret"), health("confluence", "missing_secret")],
    [synced("gitea", "2026-08-22T14:00:00Z")],
    NOW,
  );
  expect(readings).toEqual([
    { id: "gitea", text: "Gitea unreachable, synced 32 min ago", tone: "warn", syncedAt: "2026-08-22T14:00:00Z", fix: null },
    { id: "kuma", text: "Uptime Kuma: no token stored", tone: "err", syncedAt: null, fix: "Re-enter token" },
    { id: "confluence", text: "Confluence: no password stored", tone: "err", syncedAt: null, fix: "Re-enter password" },
  ]);
});

test("a refused token source is told to re-enter its token (#1405)", () => {
  const [reading] = sourceReadings(
    [{ id: "gitea", display_name: "Gitea", auth_kind: "Pat" }],
    [health("gitea", "unauthorized")],
    [],
    NOW,
  );
  expect(reading).toEqual({ id: "gitea", text: "Gitea: sign-in failed 38 min ago (401)", tone: "err", syncedAt: null, fix: "Re-enter token" });
});

test("before the source list answers, the health store's ids are the names", () => {
  expect(sourceReadings(null, [health("jira", "ok")], [], NOW)).toEqual([
    { id: "jira", text: "jira not synced yet", tone: "muted", syncedAt: null, fix: null },
  ]);
});

function healthy(tone: ReadingTone, syncedAt: string | null = null): SourceReading {
  return { id: syncedAt ?? tone, text: "named", tone, syncedAt, fix: null };
}

test("two healthy sources are named; past that they are one line that keeps the oldest age", () => {
  const late = healthy("ok", "2026-08-22T14:28:00Z");
  const early = healthy("ok", "2026-08-22T13:32:00Z");
  expect(healthyRollup([late, early], NOW)).toBeNull();
  expect(healthyRollup([late, early, healthy("ok", "2026-08-22T14:00:00Z")], NOW)).toEqual({
    text: "3 sources synced",
    rest: "oldest 1 h ago",
    tone: "ok",
  });
});

test("the rollup says what is not synced, and its dot takes the worst of what it counts", () => {
  const synced = healthy("ok", "2026-08-22T14:28:00Z");
  expect(healthyRollup([synced, healthy("info"), healthy("muted")], NOW)).toEqual({
    text: "1 source synced 4 min ago",
    rest: "1 syncing, 1 not synced yet",
    tone: "info",
  });
  expect(healthyRollup([healthy("info"), healthy("warn"), healthy("muted")], NOW)).toEqual({
    text: "1 needs a look",
    rest: "1 syncing, 1 not synced yet",
    tone: "warn",
  });
  expect(healthyRollup([healthy("muted"), healthy("muted"), healthy("muted")], NOW)).toEqual({
    text: "3 not synced yet",
    rest: null,
    tone: "muted",
  });
});

test("what wants the reader stays with the count; only the age may drop out of sight", () => {
  const synced = healthy("ok", "2026-08-22T14:28:00Z");
  expect(healthyRollup([synced, synced, healthy("warn", "2026-08-22T14:28:00Z")], NOW)).toEqual({
    text: "2 sources synced, 1 needs a look",
    rest: "oldest 4 min ago",
    tone: "warn",
  });
});

test("a sync that left part of the source out is amber, as on its Settings row (#2374)", () => {
  const partial: CredentialHealth = { ...health("jira", "ok"), detail: "boards unavailable" };
  const [reading] = sourceReadings([SOURCES[0]!], [partial], [synced("jira", "2026-08-22T14:28:00Z")], NOW);
  expect(reading).toEqual({
    id: "jira",
    text: "Jira synced 4 min ago, boards unavailable",
    tone: "warn",
    syncedAt: "2026-08-22T14:28:00Z",
    fix: null,
  });
});

test("the queue count in words", () => {
  expect(queuedWords(0)).toBe("No queued writes");
  expect(queuedWords(1)).toBe("1 queued write");
  expect(queuedWords(3)).toBe("3 queued writes");
});

/**
 * The bar's queue control (#1409): waiting writes and the ones that need a
 * decision, held plus refused. The decision part is absent at zero.
 */
test("the queue control counts the writes that need a decision too", () => {
  const read = (pending: number, decisions: number) => {
    const label = queueLabel(pending, decisions);
    return [label.queued, label.decide].filter((part) => part !== null).join(" · ");
  };
  expect(read(1, 3)).toBe("1 queued write · 3 need you");
  expect(read(0, 3)).toBe("0 queued writes · 3 need you");
  expect(read(2, 0)).toBe("2 queued writes");
  expect(queueLabel(2, 0).decide).toBeNull();
  expect(read(0, 0)).toBe("No queued writes");
});
