import { expect, test } from "vitest";

import type { CredentialHealth, SourceSyncStatus, SyncRunRow } from "../../ipc/sources";
import {
  bareAddress,
  credentialAlert,
  declaresUser,
  holdings,
  intervalOptions,
  intervalWords,
  runRow,
  settingsPlace,
  sourceLine,
  sourceState,
  userOf,
} from "./settings";

const NOW = new Date("2026-10-15T14:30:00Z");
const MIN_AGO = (n: number) => new Date(NOW.getTime() - n * 60_000).toISOString();

const health = (state: CredentialHealth["state"], at = MIN_AGO(38)): CredentialHealth => ({
  source_id: "teamcity",
  state,
  checked_at: at,
  detail: null,
  secret_expires_at: null,
});

const status = (over: Partial<SourceSyncStatus> = {}): SourceSyncStatus => ({
  source_id: "jira",
  running: false,
  run_id: 1,
  started_at: null,
  last_finished_at: MIN_AGO(4),
  last_outcome: "ok",
  next_run_at: null,
  backoff_until: null,
  last_changed: null,
  ...over,
});

const ticket = { id: "ticket", label: "Ticket", plural: "Tickets", monogram: "TK", full_sync_exhaustive: true };
const pr = { id: "pr", label: "Pull request", plural: "Pull requests", monogram: "PR", full_sync_exhaustive: false };

test("a source's page lights Sources; an unknown section is Sources, as in the prototype", () => {
  expect(settingsPlace([])).toEqual({ section: "sources" });
  expect(settingsPlace(["source", "jira"])).toEqual({ section: "sources", source: "jira" });
  expect(settingsPlace(["standup"])).toEqual({ section: "standup" });
  expect(settingsPlace(["nonsense"])).toEqual({ section: "sources" });
});

test("a row says Synced, Sign-in failed or what else holds it, with a dot in the matching tone", () => {
  const source = { enabled: true, last_run: null };
  expect(sourceState(source, health("ok"), status(), NOW)).toEqual({ tone: "ok", text: "Synced 4 min ago", failed: false });
  expect(sourceState(source, health("unauthorized"), status(), NOW)).toEqual({ tone: "err", text: "Sign-in failed", failed: true });
  expect(sourceState(source, health("missing_secret"), null, NOW).text).toBe("No password stored");
  expect(sourceState(source, health("ok"), status({ running: true }), NOW).tone).toBe("run");
  expect(sourceState({ ...source, enabled: false }, health("ok"), status(), NOW).text).toBe("Paused");
  expect(sourceState(source, health("ok"), null, NOW).text).toBe("Not synced yet");
  expect(sourceState(source, health("unreachable"), status(), NOW)).toEqual({
    tone: "warn",
    text: "Unreachable, synced 4 min ago",
    failed: false,
  });
});

test("holdings name the kind when there is one, and say items when there are several", () => {
  expect(holdings({ item_count: 177, kinds: [ticket] })).toBe("177 tickets");
  expect(holdings({ item_count: 1, kinds: [ticket] })).toBe("1 ticket");
  expect(holdings({ item_count: 2400, kinds: [ticket, pr] })).toBe("2,400 items");
});

test("the address drops its scheme, intervals read as the prototype's", () => {
  expect(bareAddress("https://jira.tidewater.example/")).toBe("jira.tidewater.example");
  expect(intervalWords(300)).toBe("5 min");
  expect(intervalWords(3600)).toBe("1 hour");
  expect(intervalWords(90)).toBe("90 s");
  expect(intervalOptions(900)).toEqual([60, 300, 900, 1800, 3600]);
  expect(intervalOptions(120)).toEqual([60, 120, 300, 900, 1800, 3600]);
});

test("the line under the name says what it is, what it holds and when it syncs", () => {
  const source = { item_count: 177, kinds: [ticket], enabled: true, sync_interval_secs: 300, last_run: null };
  expect(sourceLine(source, "Jira Data Center", status(), NOW)).toBe(
    "Jira Data Center, 177 tickets. Syncs every 5 min, last 4 min ago.",
  );
  expect(sourceLine({ ...source, enabled: false }, "Jira Data Center", null, NOW)).toBe(
    "Jira Data Center, 177 tickets. Paused, not synced yet.",
  );
});

test("a credential failure is said on top with what waits for it; a working one says nothing", () => {
  expect(credentialAlert("TeamCity", "Password", health("unauthorized"), null, NOW)).toBe(
    "Sign-in failed 38 min ago (401). Writes to TeamCity wait in the queue until you re-enter the password.",
  );
  expect(credentialAlert("TeamCity", "Password", health("ok"), null, NOW)).toBeNull();
  expect(credentialAlert("Gitea", "Token", health("missing_secret"), null, NOW)).toBe(
    "No token is stored for Gitea. Writes to Gitea wait in the queue until you enter one.",
  );
  expect(credentialAlert("TeamCity", "Password", health("unreachable"), null, NOW)).toBeNull();
});

test("a sync run reads as started, took and what it did", () => {
  const run: SyncRunRow = {
    id: 7,
    source_id: "jira",
    trigger: "schedule",
    started_at: "2026-10-15T12:28:00Z",
    finished_at: "2026-10-15T12:28:04Z",
    outcome: "ok",
    upserted: 12,
    deleted: 0,
    swept: 0,
    error: null,
    cursor_after: null,
  };
  // The suite runs in Europe/Berlin (vite.config.ts), two hours ahead in October.
  expect(runRow(run, NOW)).toEqual({ id: 7, started: "14:28", took: "4 s", result: "12 updated", tone: "ok" });
  expect(runRow({ ...run, trigger: "backfill", finished_at: "2026-10-15T12:31:12Z" }, NOW)).toMatchObject({
    took: "3 min 12 s",
    result: "12 updated, backfill",
  });
  expect(runRow({ ...run, outcome: "unauthorized", error: "401 Unauthorized" }, NOW)).toMatchObject({
    result: "401 Unauthorized. Re-enter the password.",
    tone: "err",
  });
  expect(runRow({ ...run, finished_at: null, outcome: null }, NOW)).toMatchObject({ result: "Running…", tone: "run" });
  expect(runRow({ ...run, started_at: "2026-10-12T08:00:00Z" }, NOW).started).toBe("12 Oct 10:00");
});

test("the user is the config's username, and only an adapter that declares one gets the row", () => {
  expect(declaresUser({ type: "object", properties: { username: { type: ["string", "null"] } } })).toBe(true);
  expect(declaresUser({ type: "object", properties: {} })).toBe(false);
  expect(declaresUser(null)).toBe(false);
  expect(userOf({ username: "mara" })).toBe("mara");
  expect(userOf({ username: null })).toBe("");
});
