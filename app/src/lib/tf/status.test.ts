import { expect, test } from "vitest";

import type { AuthState, CredentialHealth, SourceSyncStatus } from "../ipc/sources";
import { queueLabel, queuedWords, sourceReadings } from "./status";

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
    { id: "jira", text: "Jira synced 4 min ago", fix: null },
    { id: "confluence", text: "Confluence 4 min ago", fix: null },
    { id: "teamcity", text: "TeamCity: sign-in failed 38 min ago (401)", fix: "Re-enter password" },
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
    { id: "gitea", text: "Gitea unreachable, synced 32 min ago", fix: null },
    { id: "kuma", text: "Uptime Kuma: no token stored", fix: "Re-enter token" },
    { id: "confluence", text: "Confluence: no password stored", fix: "Re-enter password" },
  ]);
});

test("a refused token source is told to re-enter its token (#1405)", () => {
  const [reading] = sourceReadings(
    [{ id: "gitea", display_name: "Gitea", auth_kind: "Pat" }],
    [health("gitea", "unauthorized")],
    [],
    NOW,
  );
  expect(reading).toEqual({ id: "gitea", text: "Gitea: sign-in failed 38 min ago (401)", fix: "Re-enter token" });
});

test("before the source list answers, the health store's ids are the names", () => {
  expect(sourceReadings(null, [health("jira", "ok")], [], NOW)).toEqual([
    { id: "jira", text: "jira not synced yet", fix: null },
  ]);
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
