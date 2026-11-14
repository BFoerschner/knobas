/**
 * A role view's *Sync now* (#2106): the runs `sync_role` answers are in
 * progress until `sync:state` says each ended, and the view's revision moves
 * once they all have.
 */
import { expect, test } from "vitest";

import type { RoleRun, SourceSyncStatus } from "../ipc/sources";
import { EVENTS } from "../ipc";
import { toasts } from "../shell/toasts.svelte";
import { createAllSync, createRoleSyncs, createSourceSyncs, type RoleSyncPorts } from "./role-sync.svelte";
import { sourceRoles } from "./roles.svelte";

function status(source_id: string, run_id: number, running: boolean): SourceSyncStatus {
  return {
    source_id,
    running,
    run_id,
    started_at: null,
    last_finished_at: null,
    last_outcome: null,
    next_run_at: null,
    backoff_until: null,
    last_changed: null,
  };
}

/** Ports whose `sync_role` answers `runs`, and whose `sync:state` the test sends. */
function harness(runs: RoleRun[], statuses: SourceSyncStatus[] = []) {
  const asked: string[] = [];
  const handlers = new Set<(event: { payload: unknown }) => void>();
  const ports: RoleSyncPorts = {
    syncRole: (role) => {
      asked.push(role);
      return Promise.resolve(runs);
    },
    syncNow: (sourceId) => {
      asked.push(`source ${sourceId}`);
      return Promise.resolve(runs.find((run) => run.source_id === sourceId)?.run_id ?? 1);
    },
    syncStatus: () => Promise.resolve(statuses),
    listen: (event, handler) => {
      expect(event).toBe(EVENTS.syncState);
      handlers.add(handler);
      return Promise.resolve(() => handlers.delete(handler));
    },
  };
  const send = (row: SourceSyncStatus) => {
    for (const handler of [...handlers]) handler({ payload: row });
  };
  return { asked, ports, send, handlers };
}

const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

test("the runs are in progress until every one has ended, then the revision moves", async () => {
  const { asked, ports, send, handlers } = harness([
    { source_id: "confluence", run_id: 7 },
    { source_id: "wiki-b", run_id: 9 },
  ]);
  const sync = createRoleSyncs(ports).of("wiki");

  const started = sync.start();
  expect(sync.running).toBe(true);
  await started;
  expect(asked).toEqual(["wiki"]);
  expect(sync.running).toBe(true);

  send(status("confluence", 7, false));
  send(status("wiki-b", 9, true));
  // Another source's ending, and an older run's, end nothing of ours.
  send(status("jira", 9, false));
  send(status("wiki-b", 8, false));
  expect(sync.running).toBe(true);
  expect(sync.revision).toBe(0);

  send(status("wiki-b", 9, false));
  expect(sync.running).toBe(false);
  expect(sync.revision).toBe(1);
  expect(handlers.size).toBe(0);
});

test("a second click while the runs go asks nothing more", async () => {
  const { asked, ports } = harness([{ source_id: "confluence", run_id: 7 }]);
  const sync = createRoleSyncs(ports).of("wiki");
  await sync.start();
  await sync.start();
  expect(asked).toEqual(["wiki"]);
});

test("a run that ended before its answer arrived is read off the status, not waited for", async () => {
  const { ports } = harness([{ source_id: "confluence", run_id: 7 }], [status("confluence", 7, false)]);
  const sync = createRoleSyncs(ports).of("wiki");
  await sync.start();
  await settle();
  expect(sync.running).toBe(false);
  expect(sync.revision).toBe(1);
});

test("no runs to go: back at once, and the view reads nothing again", async () => {
  const { ports } = harness([]);
  const sync = createRoleSyncs(ports).of("wiki");
  await sync.start();
  expect(sync.running).toBe(false);
  expect(sync.revision).toBe(0);
});

test("each role keeps its own runs, and the same role is the same one", async () => {
  const { ports } = harness([{ source_id: "confluence", run_id: 7 }]);
  const syncs = createRoleSyncs(ports);
  await syncs.of("wiki").start();
  expect(syncs.of("wiki").running).toBe(true);
  expect(syncs.of("ci").running).toBe(false);
});

test("a refused ask comes back as not running and says why", async () => {
  const { ports } = harness([]);
  const failing: RoleSyncPorts = { ...ports, syncRole: () => Promise.reject({ code: "not_ready", message: "starting", source_id: null }) };
  const sync = createRoleSyncs(failing).of("wiki");
  await sync.start();
  expect(sync.running).toBe(false);
  expect(sync.failed).toBe("starting");
});

test("a detail's Sync now runs its one source, held by source id, until that run ends (#2107)", async () => {
  const { asked, ports, send } = harness([{ source_id: "kuma", run_id: 12 }]);
  const syncs = createSourceSyncs(ports);
  const sync = syncs.of("kuma");
  expect(syncs.of("kuma")).toBe(sync);

  await sync.start();
  expect(asked).toEqual(["source kuma"]);
  expect(sync.running).toBe(true);
  // Pressed again while it runs: nothing more is asked.
  await sync.start();
  expect(asked).toEqual(["source kuma"]);

  send(status("kuma", 12, false));
  expect(sync.running).toBe(false);
  expect(sync.revision).toBe(1);
});

const named = (id: string, display_name: string) => ({ id, display_name, roles: [], role_misses: [] });

test("a press ends in one toast naming the source, when every run has, replaced by the next press's (F-448, F-455, F-068)", async () => {
  toasts.items = [];
  sourceRoles.take([named("confluence", "Tidewater Confluence")]);
  const { ports, send } = harness([{ source_id: "confluence", run_id: 7 }]);
  const sync = createRoleSyncs(ports).of("wiki");
  await sync.start();
  await settle();
  expect(toasts.items).toEqual([]);
  send({ ...status("confluence", 7, false), last_outcome: "ok", last_changed: true });
  expect(toasts.items.map((toast) => toast.text)).toEqual(["Synced Tidewater Confluence"]);
  await sync.start();
  await settle();
  send({ ...status("confluence", 7, false), last_outcome: "ok", last_changed: false });
  expect(toasts.items.map((toast) => toast.text)).toEqual(["Tidewater Confluence is up to date"]);
  await sync.start();
  await settle();
  send({ ...status("confluence", 7, false), last_outcome: "unreachable" });
  expect(toasts.items.map((toast) => [toast.text, toast.tone])).toEqual([["Could not sync Tidewater Confluence: the source can't be reached", "err"]]);
  toasts.items = [];
  sourceRoles.forget();
});

test("several runs read as a count, and a run that failed is named however the others ended", async () => {
  toasts.items = [];
  sourceRoles.take([named("gitea", "Tidewater Gitea"), named("forgejo", "Forgejo"), named("mirror", "Mirror")]);
  const runs = [
    { source_id: "gitea", run_id: 1 },
    { source_id: "forgejo", run_id: 2 },
  ];
  const { ports, send } = harness(runs);
  const sync = createRoleSyncs(ports).of("code-host");
  const end = async (first: Partial<SourceSyncStatus>, second: Partial<SourceSyncStatus>) => {
    await sync.start();
    await settle();
    send({ ...status("gitea", 1, false), ...first });
    send({ ...status("forgejo", 2, false), ...second });
    return toasts.items.map((toast) => toast.text);
  };
  expect(await end({ last_changed: true }, { last_changed: false })).toEqual(["Synced 2 sources"]);
  expect(await end({ last_changed: false }, { last_changed: false })).toEqual(["2 sources are up to date"]);
  expect(await end({ last_outcome: "ok" }, { last_outcome: "unauthorized" })).toEqual(["Could not sync Forgejo: the sign-in failed"]);
  expect(await end({ last_outcome: "unauthorized" }, { last_outcome: "unauthorized" })).toEqual(["Could not sync Tidewater Gitea and Forgejo: the sign-in failed"]);
  expect(await end({ last_outcome: "unauthorized" }, { last_outcome: "error" })).toEqual([
    "Could not sync Tidewater Gitea: the sign-in failed. Could not sync Forgejo: the run failed",
  ]);
  toasts.items = [];
  sourceRoles.forget();
});

test("a role with no runs says so without naming the role", async () => {
  toasts.items = [];
  const { ports } = harness([]);
  await createRoleSyncs(ports).of("wiki").start();
  expect(toasts.items.map((toast) => toast.text)).toEqual(["Nothing to sync"]);
  toasts.items = [];
});

test("Sync all is told only run ids: each ending names its source, and the answer waits for them all (F-652)", async () => {
  toasts.items = [];
  sourceRoles.take([named("gitea", "Tidewater Gitea"), named("teamcity", "Tidewater TeamCity")]);
  const { ports, send } = harness([]);
  const sync = createAllSync(ports, () => Promise.resolve([11, 12]));
  await sync.start();
  await settle();
  expect(sync.running).toBe(true);
  send(status("jira", 9, false));
  send(status("gitea", 11, true));
  expect(sync.going).toBe(2);
  send({ ...status("gitea", 11, false), last_outcome: "ok" });
  expect(toasts.items).toEqual([]);
  send({ ...status("teamcity", 12, false), last_outcome: "unauthorized" });
  expect(sync.running).toBe(false);
  expect(toasts.items.map((toast) => [toast.text, toast.subject])).toEqual([["Could not sync Tidewater TeamCity: the sign-in failed", "sync:all"]]);
  toasts.items = [];
  sourceRoles.forget();
});

test("ran moves when a run of a source filling the role ends with something new, whoever started it (PR #2479)", async () => {
  sourceRoles.take([
    { id: "jira", display_name: "Jira", roles: [{ role: "ticket-system", reads: [] }], role_misses: [] },
    { id: "wiki", display_name: "Wiki", roles: [{ role: "wiki", reads: [] }], role_misses: [] },
  ]);
  const { ports, send } = harness([]);
  const sync = createRoleSyncs(ports).of("ticket-system");
  expect(sync.ran).toBe(0);
  await settle();

  send(status("jira", 4, true));
  expect(sync.ran, "still going").toBe(0);
  send({ ...status("jira", 4, false), last_changed: false });
  expect(sync.ran, "a run that brought nothing").toBe(0);
  send(status("wiki", 5, false));
  expect(sync.ran, "another role's source").toBe(0);
  send(status("jira", 6, false));
  expect(sync.ran).toBe(1);
  expect(sync.revision, "no press of its own").toBe(0);
  sourceRoles.forget();
});
