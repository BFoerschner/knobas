/**
 * **The marks a person's runs carry** (PR #2479, ux18; ADR-0063, `docs/decisions/2026-11-17-mark-builds-as-read.md` as
 * amended).
 *
 * Björn: "I want to be able to mark builds as read. Single ones as well as all. Since I might not care about old
 * failed runs." And on #2555: read means opening the run, or a *Mark … as read*.
 *
 * - **Two sets.** {@link RunsRead.read}: the runs a mark reads, opened or marked after they finished. And the runs
 *   marked while they ran, which the mark takes out of the blue only: one that fails later is unread again. A new
 *   failure is a new run, so it is unread.
 * - **Opening a run reads it** ({@link RunsRead.opened}), whichever way the page was reached: drawn at once, written
 *   behind. It writes no activity line, so the window re-reads the Inbox and the top bar itself
 *   ({@link RunsRead.afterOpen}).
 * - **The window's one store**, read once the database answers (`TfShell.svelte`) and replaced whole by every mark, so
 *   a mark of hundreds of runs is one change every list follows at once. It is not in `RunRow`: those answers are
 *   kept, and a flag in them would go stale on a mark.
 * - **One notion with the Inbox.** A mark takes a failed build's Inbox line away (the backend's rule), the Inbox's
 *   *Done* on one marks its run ({@link RunsRead.note}), and *Mark as unread* brings the line back. Each press writes an
 *   activity line, which the Inbox and the top bar re-read on.
 * - Every mark is drawn at once and written behind it; a write that fails puts the marks back and says so. *Mark all as
 *   read* says how many it marked, with *Undo*, which takes back exactly those.
 */
import { ipcErrorMessage } from "../../ipc";
import { markRunOpened, markRunsRead, markRunsUnread, readRuns, type MarkScope, type RunMarks, type RunRow } from "../../ipc/entity";
import { push, undoable } from "../../shell/toasts.svelte";
import { counted } from "../counted";
import { unreadRun } from "./builds";

/** What the marks are read and written by. */
export interface RunsReadPorts {
  read(): Promise<RunMarks>;
  /** Answers the marks it wrote. */
  mark(scope: MarkScope): Promise<RunMarks>;
  /** Answers the ids that had a mark. */
  unmark(runs: string[]): Promise<string[]>;
  /** The run's page was shown; answers whether it wrote. */
  open(run: string): Promise<boolean>;
}

/** The bridge's, each named only when called, so a suite that mocks the bridge without them still loads this. */
const REAL: RunsReadPorts = {
  read: async () => readRuns(),
  mark: async (scope) => markRunsRead(scope),
  unmark: async (runs) => markRunsUnread(runs),
  open: async (run) => markRunOpened(run),
};

/**
 * What *Mark all as read* acts on: the unread runs drawn, every run of the pipelines named, and with `everything` every
 * run of every pipeline, which needs no list from the page.
 */
export interface MarkAll {
  runs: readonly string[];
  pipelines: readonly string[];
  everything?: boolean;
}

/** Every id a mark answered it wrote. */
function idsOf(marks: RunMarks): string[] {
  return [...marks.read, ...marks.while_running];
}

export class RunsRead {
  /** The runs a mark reads, `null` until the database answered. Replaced whole by every change. */
  #read = $state.raw<ReadonlySet<string> | null>(null);
  /** The runs marked while they ran: out of the blue only. Replaced whole by every change. */
  #whileRunning = $state.raw<ReadonlySet<string>>(new Set());
  ports: RunsReadPorts = REAL;
  /**
   * Called when opening a run wrote its mark. The open writes no activity line, so the shell re-reads what counts
   * marks (the top bar, the Inbox) here.
   */
  afterOpen: (() => void) | null = null;
  #loading: Promise<void> | null = null;
  /** The failed runs each open pipeline page holds, by pipeline: *Mark all* turns them at once, older pages too. */
  #held = new Map<string, readonly string[]>();
  /** The marks being written: a read of what they change waits on them ({@link RunsRead.settled}). */
  #writes = new Set<Promise<unknown>>();
  /** The runs opened this session, each written once. */
  #opened = new Set<string>();

  /** `write`, held in {@link RunsRead.settled} until it answers. */
  #writing<T>(write: Promise<T>): Promise<T> {
    this.#writes.add(write);
    const done = () => void this.#writes.delete(write);
    write.then(done, done);
    return write;
  }

  /**
   * Resolves once every mark written so far has answered, either way: a read of the backend made after it sees them.
   * The Builds rail reads its list again after a mark it doesn't name is taken back (#2543), and a read sent while
   * that write was out could miss it.
   */
  async settled(): Promise<void> {
    await Promise.allSettled([...this.#writes]);
  }

  /**
   * A pipeline page's failed runs, held while it draws them. *Mark all as read* on that pipeline, from the rail or its
   * own header, draws them read before the backend answers. Returns the release.
   */
  hold(pipelineId: string, runs: readonly string[]): () => void {
    this.#held.set(pipelineId, runs);
    return () => {
      if (this.#held.get(pipelineId) === runs) this.#held.delete(pipelineId);
    };
  }

  /** The runs a mark reads, `null` until read: what the pure rules in `builds.ts` take. */
  get read(): ReadonlySet<string> | null {
    return this.#read;
  }

  /** The runs marked while they ran, which a mark took out of the blue only. */
  get whileRunning(): ReadonlySet<string> {
    return this.#whileRunning;
  }

  /** Whether `run` asks for attention now: a failure no mark reads. */
  unread(run: Pick<RunRow, "entity_id" | "finished" | "state">): boolean {
    return unreadRun(run, this.#read);
  }

  /** Whether `run` is in the blue: started, not finished, and no mark of any kind. Queued is not blue. */
  inBlue(run: Pick<RunRow, "entity_id" | "started" | "finished">): boolean {
    return run.started !== null && run.finished === null && !(this.#read?.has(run.entity_id) ?? false) && !this.#whileRunning.has(run.entity_id);
  }

  /** Read the marks once; a second call waits on the first. A read that fails leaves every failure unread. */
  load(): Promise<void> {
    this.#loading ??= this.ports.read().then(
      (marks) => {
        // A mark made while the read was out is kept.
        this.#read = new Set([...marks.read, ...(this.#read ?? [])]);
        this.#whileRunning = new Set([...marks.while_running, ...this.#whileRunning].filter((id) => !this.#read!.has(id)));
      },
      () => {
        this.#loading = null;
      },
    );
    return this.#loading;
  }

  /** Each of `read` read, each of `whileRunning` out of the blue only, each of `drop` with no mark. */
  #with(read: readonly string[], drop: readonly string[] = [], whileRunning: readonly string[] = []): void {
    const next = new Set(this.#read ?? []);
    const running = new Set(this.#whileRunning);
    for (const id of drop) (next.delete(id), running.delete(id));
    for (const id of whileRunning) (next.delete(id), running.add(id));
    for (const id of read) (running.delete(id), next.add(id));
    this.#read = next;
    this.#whileRunning = running;
  }

  /**
   * The marks a write answered, as the backend wrote them. Each of `wasRunning`, drawn read at the press, that the answer
   * does not read keeps the mark it had: the backend writes over a mark taken while a run ran only once the run ended.
   */
  #wrote(marks: RunMarks, wasRunning: readonly string[] = []): void {
    const read = new Set(marks.read);
    this.#with(marks.read, [], [...marks.while_running, ...wasRunning.filter((id) => !read.has(id))]);
  }

  /** The Inbox's *Done* on a failed build read its run (the backend wrote the mark with the answer). */
  note(runId: string): void {
    if (!(this.#read?.has(runId) ?? false)) this.#with([runId]);
  }

  /**
   * The run's page was shown: drawn read at once, written behind, once a session per run. A write that fails puts it
   * back and says nothing, since nobody pressed anything. A write that wrote calls {@link RunsRead.afterOpen}.
   */
  async opened(runId: string): Promise<void> {
    if (runId === "" || this.#opened.has(runId)) return;
    this.#opened.add(runId);
    if (this.#read?.has(runId) ?? false) return;
    const wasRunning = this.#whileRunning.has(runId);
    this.#with([runId]);
    try {
      if (await this.#writing(this.ports.open(runId))) this.afterOpen?.();
    } catch {
      this.#opened.delete(runId);
      this.#with([], [runId], wasRunning ? [runId] : []);
    }
  }

  /** Mark `runs` read, at once; written behind, put back if the write fails. */
  async markRead(runs: readonly string[]): Promise<void> {
    const fresh = runs.filter((id) => !(this.#read?.has(id) ?? false));
    if (fresh.length === 0) return;
    const wasRunning = fresh.filter((id) => this.#whileRunning.has(id));
    this.#with(fresh);
    try {
      this.#wrote(await this.#writing(this.ports.mark({ everything: false, pipelines: [], runs: [...fresh] })), wasRunning);
    } catch (cause) {
      this.#with([], fresh, wasRunning);
      push({ text: `Could not mark ${counted(fresh.length, "run", "runs")} as read: ${ipcErrorMessage(cause)}`, tone: "err" });
    }
  }

  /** Take the marks of `runs` back, at once; written behind, put back if the write fails. */
  async markUnread(runs: readonly string[]): Promise<void> {
    const read = runs.filter((id) => this.#read?.has(id) ?? false);
    const running = runs.filter((id) => this.#whileRunning.has(id));
    const marked = [...read, ...running];
    if (marked.length === 0) return;
    this.#with([], marked);
    for (const id of marked) this.#opened.delete(id);
    try {
      await this.#writing(this.ports.unmark(marked));
    } catch (cause) {
      this.#with(read, [], running);
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
   * *Mark all as read*: the unread runs drawn, and those an open page of one of `scope.pipelines` holds
   * ({@link RunsRead.hold}), or every page held for `everything`, at once; every run of the scope, older pages
   * included, by the backend. Says how many it marked, with *Undo*; a press that marked nothing says so.
   */
  async markAll(scope: MarkAll): Promise<string[]> {
    const everything = scope.everything ?? false;
    const held = everything ? [...this.#held.values()].flat() : scope.pipelines.flatMap((id) => this.#held.get(id) ?? []);
    const drawn = [...new Set([...scope.runs, ...held])].filter((id) => !(this.#read?.has(id) ?? false));
    const wasRunning = drawn.filter((id) => this.#whileRunning.has(id));
    this.#with(drawn);
    let wrote: RunMarks;
    try {
      wrote = await this.#writing(this.ports.mark({ everything, pipelines: everything ? [] : [...scope.pipelines], runs: [...drawn] }));
    } catch (cause) {
      this.#with([], drawn, wasRunning);
      push({ text: `Could not mark the runs as read: ${ipcErrorMessage(cause)}`, tone: "err" });
      return [];
    }
    this.#wrote(wrote, wasRunning);
    const ids = idsOf(wrote);
    if (ids.length === 0) push({ text: "No unread runs", subject: "runs-read" });
    else undoable(`Marked ${counted(ids.length, "run", "runs")} as read`, () => void this.markUnread(ids));
    return ids;
  }

  /** Back to nothing read and the real IPC, for a test. */
  forget(): void {
    this.#read = null;
    this.#whileRunning = new Set();
    this.#loading = null;
    this.#held.clear();
    this.#writes.clear();
    this.#opened.clear();
    this.afterOpen = null;
    this.ports = REAL;
  }
}

/** The window's one. */
export const runsRead = new RunsRead();
