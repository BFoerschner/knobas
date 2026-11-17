/**
 * **The failed runs the person marked as read** (PR #2479, ux18, `docs/decisions/2026-11-17-mark-builds-as-read.md`).
 *
 * Björn: "I want to be able to mark builds as read. Single ones as well as all. Since I might not care about old
 * failed runs."
 *
 * - **Unread** is a finished failed run (its tone `err`) with no mark. Every other run is read by its nature. A new
 *   failure is a new run, so it is unread. Opening a run reads nothing: only a mark does.
 * - **The window's one set**, read once the database answers (`TfShell.svelte`) and replaced whole by every mark, so a
 *   mark of hundreds of runs is one change every list follows at once. It is not in `RunRow`: those answers are kept,
 *   and a flag in them would go stale on a mark.
 * - **One notion with the Inbox.** A mark takes a failed build's Inbox line away (the backend's rule), the Inbox's
 *   *Done* on one reads its run ({@link RunsRead.note}), and *Mark as unread* brings the line back. Each mark writes an
 *   activity line, which the Inbox and the top bar re-read on.
 * - Every mark is drawn at once and written behind it; a write that fails puts the marks back and says so. *Mark all as
 *   read* says how many it marked, with *Undo*, which takes back exactly those.
 */
import { ipcErrorMessage } from "../../ipc";
import { markRunsRead, markRunsUnread, readRuns, type RunRow } from "../../ipc/entity";
import { push, undoable } from "../../shell/toasts.svelte";
import { counted } from "../counted";
import { unreadRun } from "./builds";

/** What the marks are read and written by. */
export interface RunsReadPorts {
  read(): Promise<string[]>;
  /** Answers the ids it wrote. */
  mark(runs: string[], pipelines: string[]): Promise<string[]>;
  /** Answers the ids that were read. */
  unmark(runs: string[]): Promise<string[]>;
}

/** The bridge's, each named only when called, so a suite that mocks the bridge without them still loads this. */
const REAL: RunsReadPorts = {
  read: async () => readRuns(),
  mark: async (runs, pipelines) => markRunsRead(runs, pipelines),
  unmark: async (runs) => markRunsUnread(runs),
};

/** What *Mark all as read* acts on: the unread runs drawn, and every failed run of the pipelines named. */
export interface MarkScope {
  runs: readonly string[];
  pipelines: readonly string[];
}

export class RunsRead {
  /** The marks, `null` until the database answered. Replaced whole by every change. */
  #read = $state.raw<ReadonlySet<string> | null>(null);
  ports: RunsReadPorts = REAL;
  #loading: Promise<void> | null = null;

  /** The marks as one set, `null` until read: what the pure rules in `builds.ts` take. */
  get read(): ReadonlySet<string> | null {
    return this.#read;
  }

  /** Whether `run` asks for attention now. */
  unread(run: Pick<RunRow, "entity_id" | "finished" | "state">): boolean {
    return unreadRun(run, this.#read);
  }

  /** Read the marks once; a second call waits on the first. A read that fails leaves every failure unread. */
  load(): Promise<void> {
    this.#loading ??= this.ports.read().then(
      (ids) => {
        // A mark made while the read was out is kept.
        this.#read = new Set([...ids, ...(this.#read ?? [])]);
      },
      () => {
        this.#loading = null;
      },
    );
    return this.#loading;
  }

  #with(add: readonly string[], drop: readonly string[] = []): void {
    const next = new Set(this.#read ?? []);
    for (const id of add) next.add(id);
    for (const id of drop) next.delete(id);
    this.#read = next;
  }

  /** The Inbox's *Done* on a failed build read its run (the backend wrote the mark with the answer). */
  note(runId: string): void {
    if (!(this.#read?.has(runId) ?? false)) this.#with([runId]);
  }

  /** Mark `runs` read, at once; written behind, put back if the write fails. */
  async markRead(runs: readonly string[]): Promise<void> {
    const fresh = runs.filter((id) => !(this.#read?.has(id) ?? false));
    if (fresh.length === 0) return;
    this.#with(fresh);
    try {
      await this.ports.mark([...fresh], []);
    } catch (cause) {
      this.#with([], fresh);
      push({ text: `Could not mark ${counted(fresh.length, "run", "runs")} as read: ${ipcErrorMessage(cause)}`, tone: "err" });
    }
  }

  /** Mark `runs` unread, at once; written behind, put back if the write fails. */
  async markUnread(runs: readonly string[]): Promise<void> {
    const marked = runs.filter((id) => this.#read?.has(id) ?? false);
    if (marked.length === 0) return;
    this.#with([], marked);
    try {
      await this.ports.unmark([...marked]);
    } catch (cause) {
      this.#with(marked);
      push({ text: `Could not mark ${counted(marked.length, "run", "runs")} as unread: ${ipcErrorMessage(cause)}`, tone: "err" });
    }
  }

  /**
   * *Mark as read* or *Mark as unread* on one run, whichever it is now. A run that cannot be unread is never marked,
   * so it takes nothing: it reads as read and has no mark to take back.
   */
  toggle(run: Pick<RunRow, "entity_id" | "finished" | "state">): Promise<void> {
    return this.unread(run) ? this.markRead([run.entity_id]) : this.markUnread([run.entity_id]);
  }

  /**
   * *Mark all as read*: the unread runs drawn at once, and every failed run of `scope.pipelines`, older pages
   * included, by the backend. Says how many it marked, with *Undo*; a press that marked nothing says so.
   */
  async markAll(scope: MarkScope): Promise<string[]> {
    const drawn = scope.runs.filter((id) => !(this.#read?.has(id) ?? false));
    this.#with(drawn);
    let wrote: string[];
    try {
      wrote = await this.ports.mark([...drawn], [...scope.pipelines]);
    } catch (cause) {
      this.#with([], drawn);
      push({ text: `Could not mark the runs as read: ${ipcErrorMessage(cause)}`, tone: "err" });
      return [];
    }
    this.#with(wrote);
    if (wrote.length === 0) push({ text: "No unread runs", subject: "runs-read" });
    else undoable(`Marked ${counted(wrote.length, "run", "runs")} as read`, () => void this.markUnread(wrote));
    return wrote;
  }

  /** Back to nothing read and the real IPC, for a test. */
  forget(): void {
    this.#read = null;
    this.#loading = null;
    this.ports = REAL;
  }
}

/** The window's one. */
export const runsRead = new RunsRead();
