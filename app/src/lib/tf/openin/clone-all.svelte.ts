/**
 * *Clone all* (the smoothness pass, PR #2479), over `clone_all_plan`,
 * `clone_all`, `clone_all_state` and the `clone:all` event: one store for the
 * window, so Code and Settings › Checkouts draw the same run, a page left and
 * come back to draws where it is, and the run's end is one toast wherever the
 * reader is.
 *
 * The run belongs to the backend, which starts one at a time: a second press,
 * by pointer or key, answers the run already going and starts nothing.
 */
import { listen as tauriListen } from "@tauri-apps/api/event";

import { EVENTS, ipcErrorMessage } from "../../ipc";
import { cloneAll, cloneAllPlan, cloneAllState, type CloneAllPlan, type CloneAllState } from "../../ipc/entity";
import { push, type ToastSpec } from "../../shell/toasts.svelte";

export interface CloneAllPorts {
  plan: () => Promise<CloneAllPlan>;
  start: () => Promise<CloneAllState>;
  state: () => Promise<CloneAllState | null>;
  listen: (event: string, handler: (event: { payload: unknown }) => void) => Promise<() => void>;
}

export const realCloneAllPorts: CloneAllPorts = {
  plan: () => cloneAllPlan(),
  start: () => cloneAll(),
  state: () => cloneAllState(),
  listen: (event, handler) => tauriListen(event, handler),
};

/** How many repositories, said as a person says it. */
export function repositories(n: number): string {
  return `${n.toLocaleString("en-US")} ${n === 1 ? "repository" : "repositories"}`;
}

/** The run's one summary toast: what was cloned, what was already there, and every failure with its reason. */
export function summary(run: CloneAllState, root: string | null): ToastSpec {
  const into = root === null ? "" : ` into ${root}`;
  const there = run.adopted === 0 ? "" : `; ${run.adopted} ${run.adopted === 1 ? "was" : "were"} already there`;
  if (run.total === 0) return { text: "Nothing to clone: every repository knobas knows is on this machine." };
  if (run.failed.length === 0) return { text: `Cloned ${repositories(run.cloned)}${into}${there}.` };
  const why = run.failed.map((f) => `${f.name}: ${f.reason}`).join("; ");
  return {
    text: `Cloned ${run.cloned} of ${repositories(run.total)}${into}${there}. ${run.failed.length} failed (${why}).`,
    tone: "err",
    ms: 15_000,
  };
}

export interface CloneAll {
  /** The run going, or the last one; `null` before the first. */
  readonly run: CloneAllState | null;
  /** A run is going. */
  readonly running: boolean;
  /** Read where a run is, and hear it to its end if it is going: once per page that draws it. */
  ensure(): Promise<void>;
  plan(): Promise<CloneAllPlan>;
  /** Start a run, or do nothing while one goes. `root` names where the clones go, for the summary. */
  start(root: string | null): Promise<void>;
  /** Tests only. */
  reset(): void;
}

export function createCloneAll(ports: CloneAllPorts = realCloneAllPorts): CloneAll {
  let run = $state<CloneAllState | null>(null);
  let starting = false;
  let root: string | null = null;
  /** The runs whose summary was shown, or that ended before this window watched them. */
  const told = new Set<number>();
  let listening: Promise<unknown> | null = null;

  /** Hear the run's states: from a start, or a run found going, until the window closes. */
  function listen(): Promise<unknown> {
    listening ??= Promise.resolve()
      .then(() => ports.listen(EVENTS.cloneAll, ({ payload }) => take(payload as CloneAllState)))
      .catch(() => (listening = null));
    return listening;
  }

  function take(next: CloneAllState) {
    if (run !== null && (next.run < run.run || (next.run === run.run && run.finished && !next.finished))) return;
    run = next;
    if (next.finished && !told.has(next.run)) {
      told.add(next.run);
      push(summary(next, root));
    }
  }

  return {
    get run() {
      return run;
    },
    get running() {
      return run !== null && !run.finished;
    },
    async ensure() {
      // Never a rejection: a page that draws the button has nothing to do with a run it could not read.
      const now = await Promise.resolve()
        .then(() => ports.state())
        .catch(() => null);
      if (!now) return;
      // A run that ended before this window watched it was not this window's to sum up.
      if (now.finished && run?.run !== now.run) told.add(now.run);
      // A run going is heard to its end, wherever the reader goes; one that ended needs no ear.
      if (!now.finished) await listen();
      take(now);
    },
    plan: () => ports.plan(),
    async start(at) {
      if (starting || (run !== null && !run.finished)) return;
      starting = true;
      root = at;
      try {
        await listen();
        take(await ports.start());
      } catch (cause) {
        push({ text: `Could not clone all: ${ipcErrorMessage(cause)}`, tone: "err" });
      } finally {
        starting = false;
      }
    },
    reset() {
      run = null;
      starting = false;
      root = null;
      told.clear();
      const old = listening;
      listening = null;
      void old?.then((off) => typeof off === "function" && off());
    },
  };
}

/** The window's one store. Tests build their own. */
export const cloneAllStore = createCloneAll();
