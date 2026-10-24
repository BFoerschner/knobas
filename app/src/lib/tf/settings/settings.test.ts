import { expect, test } from "vitest";

import type { CredentialHealth, SourceSyncStatus, SyncRunRow } from "../../ipc/sources";
import {
  BACKFILL_HINT,
  bareAddress,
  credentialAlert,
  declaresUser,
  holdings,
  intervalOptions,
  intervalWords,
  reenterWords,
  roleMissLines,
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
  // *Sync WEB too* on a Jira view (#879): the page with a value added to a list field.
  expect(settingsPlace(["source", "jira", "add", "projects", "WEB"])).toEqual({
    section: "sources",
    source: "jira",
    add: { field: "projects", value: "WEB" },
  });
  expect(settingsPlace(["source", "jira", "add", "projects"])).toEqual({ section: "sources", source: "jira" });
});

test("a row says Synced, Sign-in failed or what else holds it, with a dot in the matching tone", () => {
  const source = { enabled: true, last_run: null, auth_kind: null, role_misses: [] };
  expect(sourceState(source, health("ok"), status(), NOW)).toEqual({ tone: "ok", text: "Synced 4 min ago", failed: false });
  expect(sourceState(source, health("unauthorized"), status(), NOW)).toEqual({ tone: "err", text: "Sign-in failed", failed: true });
  expect(sourceState(source, health("missing_secret"), null, NOW).text).toBe("No password stored");
  // The noun is the source's (#1405): a token source has no password to store.
  expect(sourceState({ ...source, auth_kind: "UserPassword" }, health("missing_secret"), null, NOW).text).toBe(
    "No password stored",
  );
  expect(sourceState({ ...source, auth_kind: "ApiToken" }, health("missing_secret"), null, NOW).text).toBe("No token stored");
  expect(sourceState({ ...source, auth_kind: "Pat" }, health("unauthorized"), null, NOW).text).toBe("Sign-in failed");
  expect(sourceState(source, health("ok"), status({ running: true }), NOW).tone).toBe("run");
  expect(sourceState({ ...source, enabled: false }, health("ok"), status(), NOW).text).toBe("Paused");
  expect(sourceState(source, health("ok"), null, NOW).text).toBe("Not synced yet");
  expect(sourceState(source, health("unreachable"), status(), NOW)).toEqual({
    tone: "warn",
    text: "Unreachable, synced 4 min ago",
    failed: false,
  });
});

test("a run stopped at a deadline or memory cap names the limit on the health line; other failures do not", () => {
  const run = (error: string | null) =>
    ({ enabled: true, last_run: { outcome: "error", error, finished_at: MIN_AGO(4) } }) as never;
  const failed = status({ last_outcome: "error" });
  const line = (error: string | null) => sourceState(run(error), health("ok"), failed, NOW).text;
  expect(line('the external adapter "wasm-fixture" was stopped at its deadline of 600 s')).toBe(
    "Failed 4 min ago, stopped at its deadline of 600 s",
  );
  expect(line('the external adapter "wasm-fixture" was stopped at its memory cap of 64 MiB')).toBe(
    "Failed 4 min ago, stopped at its memory cap of 64 MiB",
  );
  expect(line("HTTP 500")).toBe("Failed 4 min ago");
  expect(line(null)).toBe("Failed 4 min ago");
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
  expect(intervalWords(20)).toBe("20 s");
  expect(intervalWords(45)).toBe("45 s");
  // The steps at or above the adapter's floor (#1188), plus a stored value off the ladder.
  expect(intervalOptions(900, 60)).toEqual([60, 300, 900, 1800, 3600]);
  expect(intervalOptions(120, 60)).toEqual([60, 120, 300, 900, 1800, 3600]);
  expect(intervalOptions(60, 20)).toEqual([20, 30, 45, 60, 300, 900, 1800, 3600]);
  expect(intervalOptions(300, 45)).toEqual([45, 60, 300, 900, 1800, 3600]);
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

const RUN: SyncRunRow = {
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
const JIRA = { id: "jira", display_name: "Jira", auth_kind: "UserPassword" as const };

test("a sync run reads as started, took and what it did", () => {
  const run = RUN;
  // The suite runs in Europe/Berlin (vite.config.ts), two hours ahead in October.
  expect(runRow(run, JIRA, NOW)).toEqual({ id: 7, started: "14:28", took: "4 s", result: "12 updated", detail: null, tone: "ok" });
  expect(runRow({ ...run, trigger: "backfill", finished_at: "2026-10-15T12:31:12Z" }, JIRA, NOW)).toMatchObject({
    took: "3 m 12 s",
    result: "12 updated, backfill",
  });
  expect(runRow({ ...run, outcome: "unauthorized", error: "401 Unauthorized" }, JIRA, NOW)).toMatchObject({
    result: "401 Unauthorized. Re-enter the password.",
    tone: "err",
  });
  expect(runRow({ ...run, finished_at: null, outcome: null }, JIRA, NOW)).toMatchObject({ result: "Running…", tone: "run" });
  // CHANGED (#1986): an older run read "12 Oct 10:00" and yesterday's "14 Oct 10:00"; they read as the backup line does.
  expect(runRow({ ...run, started_at: "2026-10-12T08:00:00Z" }, JIRA, NOW).started).toBe("2026-10-12 10:00");
  expect(runRow({ ...run, started_at: "2026-10-14T08:00:00Z" }, JIRA, NOW).started).toBe("yesterday 10:00");
  expect(runRow({ ...run, started_at: "not a date" }, JIRA, NOW).started).toBe("—");
});

test("a refused run tells the reader to re-enter the source's own credential (#1405)", () => {
  const refused = { ...RUN, outcome: "unauthorized" as const, error: "401 Unauthorized" };
  expect(runRow(refused, { ...JIRA, auth_kind: "Pat" }, NOW).result).toBe("401 Unauthorized. Re-enter the token.");
  expect(runRow(refused, { ...JIRA, auth_kind: "ApiToken" }, NOW).result).toBe("401 Unauthorized. Re-enter the token.");
  expect(runRow({ ...refused, error: null }, JIRA, NOW).result).toBe("401 Unauthorized. Re-enter the password.");
});

test("the fix for a credential names the source's noun (#1405)", () => {
  expect(reenterWords("UserPassword")).toBe("Re-enter password");
  expect(reenterWords("Pat")).toBe("Re-enter token");
  expect(reenterWords("ApiToken")).toBe("Re-enter token");
  expect(reenterWords(null)).toBe("Re-enter password");
  expect(reenterWords(undefined)).toBe("Re-enter password");
});

test("an unreachable run says the source did not answer, the transport error kept as its detail (#1478)", () => {
  const transport =
    "source: unreachable: error sending request for url (http://127.0.0.1:8111/app/rest/builds?locator=defaultFilter%3Afalse%2Ccount%3A100)";
  const teamcity = { id: "teamcity", display_name: "TeamCity", auth_kind: "UserPassword" as const };
  expect(runRow({ ...RUN, outcome: "unreachable", error: transport }, teamcity, NOW)).toMatchObject({
    result: "TeamCity did not answer.",
    detail: transport,
    tone: "err",
  });
  expect(runRow({ ...RUN, outcome: "unreachable", error: null }, teamcity, NOW)).toMatchObject({
    result: "TeamCity did not answer.",
    detail: null,
  });
  // A source with no display name is called by its id, as everywhere else.
  expect(runRow({ ...RUN, outcome: "unreachable", error: null }, { ...teamcity, display_name: "" }, NOW).result).toBe(
    "teamcity did not answer.",
  );
  // The other outcomes keep the sentence they had, and no detail.
  expect(runRow({ ...RUN, outcome: "unauthorized", error: "401 Unauthorized" }, teamcity, NOW)).toMatchObject({
    result: "401 Unauthorized. Re-enter the password.",
    detail: null,
  });
  expect(runRow({ ...RUN, outcome: "error", error: "watermark refused" }, teamcity, NOW)).toMatchObject({
    result: "watermark refused",
    detail: null,
  });
  expect(runRow({ ...RUN, outcome: "error", error: null }, teamcity, NOW).result).toBe("Failed");
});

test("the user is the config's username, and only an adapter that declares one gets the row", () => {
  expect(declaresUser({ type: "object", properties: { username: { type: ["string", "null"] } } })).toBe(true);
  expect(declaresUser({ type: "object", properties: {} })).toBe(false);
  expect(declaresUser(null)).toBe(false);
  expect(userOf({ username: "mara" })).toBe("mara");
  expect(userOf({ username: null })).toBe("");
});

const priorityMiss = { role: "ticket-system", kind: "ticket", field: "priority", path: "fields.priority.name" };

test("a source whose records missed a role's path says which role and which path (#1199)", () => {
  const source = { enabled: true, last_run: null, auth_kind: null, role_misses: [priorityMiss] };
  expect(sourceState(source, health("ok"), status(), NOW)).toEqual({
    tone: "warn",
    text: "Synced 4 min ago, ticket system not drawn (fields.priority.name)",
    failed: false,
  });
  const two = [...source.role_misses, { role: "code-host", kind: "pr", field: "merged", path: "merged" }];
  expect(sourceState({ ...source, role_misses: two }, health("ok"), status(), NOW).text).toBe(
    "Synced 4 min ago, ticket system not drawn (fields.priority.name) and 1 more",
  );
  // A credential failure still says itself first.
  expect(sourceState(source, health("unauthorized"), status(), NOW).text).toBe("Sign-in failed");

  expect(roleMissLines({ kinds: [ticket, pr], role_misses: [priorityMiss] })).toEqual([
    "Not drawn as a ticket system: no ticket resolves priority at fields.priority.name. Its tickets stay searchable and linkable.",
  ]);
  expect(roleMissLines({ kinds: [ticket], role_misses: [] })).toEqual([]);
});

// #1477: Backfill is also the way out of a sync that stopped on a position the
// server no longer has (TeamCity's deleted newest build), and its refusal sends
// the reader here. The line says it starts over from no position, not only that
// it reads more fields.
test("the Backfill line says it forgets where the last sync stopped, and when to use it", () => {
  expect(BACKFILL_HINT).toBe(
    "Reads everything again and forgets where the last sync stopped: after a knobas update reads more fields, or when a sync stopped and names Backfill. Takes a few minutes; the app stays usable.",
  );
});
