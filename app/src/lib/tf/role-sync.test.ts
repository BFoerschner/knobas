/**
 * A role view's *Sync now* (#2106): the runs `sync_role` answers are in
 * progress until `sync:state` says each ended, and the view's revision moves
 * once they all have.
 */
import { expect, test } from "vitest";

import type { RoleRun, SourceSyncStatus } from "../ipc/sources";
import { EVENTS } from "../ipc";
import { createRoleSyncs, type RoleSyncPorts } from "./role-sync.svelte";

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
