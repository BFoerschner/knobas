/**
 * A fake backend for a folder's generated compare (ux26), for tests: each lane's slot holds what is kept for it, asks
 * are counted, a press of *Run* goes until the test lands it (`land`, `fail`), and the compare answers what the test
 * set for a pair of tokens.
 */
import type { FolderAsk, FolderDrawn, FolderFailure, FolderRenderView, FolderRunState, MadeCompare } from "../ipc/app";
import type { FolderComparePorts } from "./folder-compare.svelte";

export interface FakeSlot {
  drawn: FolderDrawn | null;
  current: boolean;
  running: FolderRunState | null;
  failure: FolderFailure | null;
}

export function drawnFolder(token: string, files = 13): FolderDrawn {
  return { token, kind: { kind: "folder", files, bytes: 1000, left_out: 0 }, size: 1000, commit: null, from: "clone", took_ms: 1200, drawn_at: "2026-11-20T10:00:00Z" };
}

export function fakeCompareBackend() {
  const slots = new Map<string, FakeSlot>();
  const asked: FolderAsk[] = [];
  const stopped: string[] = [];
  const compared: [string | null, string | null, boolean][] = [];
  const answers = new Map<string, MadeCompare>();
  const listeners = new Set<(event: { payload: unknown }) => void>();
  let tokens = 0;
  const slotOf = (lane: string, folder = "docs") => {
    const key = `${folder}\u0000${lane}`;
    let slot = slots.get(key);
    if (slot === undefined) {
      slot = { drawn: null, current: false, running: null, failure: null };
      slots.set(key, slot);
    }
    return slot;
  };
  const view = (at: FolderAsk, slot: FakeSlot): FolderRenderView => ({
    renderer: at.renderer,
    folder: at.folder,
    drawn: slot.drawn,
    current: slot.current,
    running: slot.running,
    failure: slot.failure,
    from: "clone",
    commit: at.commit,
  });
  const tell = (folder: string, lane: string, running: FolderRunState | null) => {
    for (const heard of listeners) heard({ payload: { entity_id: "repo", renderer: "site", folder, running, lane } });
  };
  const ports: FolderComparePorts = {
    render: (at) => {
      asked.push(at);
      const slot = slotOf(at.lane ?? "", at.folder);
      if (at.ask === "again" && slot.running === null) {
        slot.running = { state: "queued" };
        slot.failure = null;
      }
      return Promise.resolve(view(at, slot));
    },
    stop: (_id, _renderer, folder, lane) => {
      stopped.push(lane ?? "");
      const slot = slotOf(lane ?? "", folder);
      slot.running = null;
      slot.failure = { why: "stopped" };
      tell(folder, lane ?? "", null);
      return Promise.resolve();
    },
    leave: () => Promise.resolve(),
    output: (token) => Promise.resolve(new TextEncoder().encode(token.startsWith("same") ? "same" : token)),
    frame: () => Promise.resolve("frame"),
    listen: (_event, handler) => {
      listeners.add(handler);
      return Promise.resolve(() => listeners.delete(handler));
    },
    compare: (before, after, eol) => {
      compared.push([before, after, eol]);
      return Promise.resolve(answers.get(`${before}\n${after}`) ?? { files: [], same: 13, eol_only: 0, before_files: 13, after_files: 13 });
    },
  };
  return {
    ports,
    asked,
    stopped,
    compared,
    answers,
    slot: slotOf,
    /** Sets what is kept for `lane` (`token`), as a run that ended or a store read back would. */
    keep(lane: string, token: string, current = true, folder = "docs") {
      Object.assign(slotOf(lane, folder), { drawn: drawnFolder(token), current, running: null, failure: null });
    },
    /** Ends `lane`'s run with what it made, told as the backend tells it. */
    land(lane: string, token = `t${++tokens}`, folder = "docs") {
      Object.assign(slotOf(lane, folder), { drawn: drawnFolder(token), current: true, running: null, failure: null });
      tell(folder, lane, null);
      return token;
    },
    /** Ends `lane`'s run failed. */
    fail(lane: string, failure: FolderFailure = { why: "failed", status: 2, stderr: "lowcode: no project file\n" }, folder = "docs") {
      Object.assign(slotOf(lane, folder), { running: null, failure });
      tell(folder, lane, null);
    },
    /** Tells `lane`'s progress, without an ask. */
    progress(lane: string, running: FolderRunState, folder = "docs") {
      slotOf(lane, folder).running = running;
      tell(folder, lane, running);
    },
  };
}
