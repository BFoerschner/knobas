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
 * - **The counts by bubble** (#2568, {@link RunsRead.counts}): each pipeline's blue, green, yellow, red and grey, read
 *   whole by one call ({@link RunsRead.readCounts}) on the events the top bar follows and after every mark. A folder
 *   sums its pipelines ({@link RunsRead.countsOf}); the top bar draws {@link RunsRead.topBar}. Green, yellow and grey
 *   count from an instant the backend keeps: when Builds was last left ({@link RunsRead.inFront}), moved by *Mark all as
 *   read* and put back by its *Undo*. While Builds is in front the backend holds the visit's start, so the visit's
 *   green stays drawn and a run that ends while you look adds none.
 */
import { ipcErrorMessage } from "../../ipc";
import {
  buildsInFront,
  markRunOpened,
  markRunsRead,
  markRunsUnread,
  moveSeen,
  putBackSeen,
  readRuns,
  runCounts,
  type BubbleCounts,
  type MarkScope,
  type RunCounts,
  type RunMarks,
  type RunRow,
  type SeenMove,
} from "../../ipc/entity";
import { coalesce } from "../../shell/coalesce";
import { push, undoable } from "../../shell/toasts.svelte";
import { counted } from "../counted";
import { markableRun, noCounts, plusCounts, runBubble, runningRun, unreadRun, type Bubble } from "./builds";

/** What the marks are read and written by. */
export interface RunsReadPorts {
  read(): Promise<RunMarks>;
  /** Answers the marks it wrote. */
  mark(scope: MarkScope): Promise<RunMarks>;
  /** Answers the ids that had a mark. */
  unmark(runs: string[]): Promise<string[]>;
  /** The run's page was shown; answers whether it wrote. */
  open(run: string): Promise<boolean>;
  /** Every pipeline's unread runs by bubble. */
  counts(): Promise<RunCounts>;
  /** A scope's mark on green, yellow and grey: its instant moved; `null` for runs by id alone. */
  moveSeen(scope: MarkScope): Promise<SeenMove | null>;
  /** *Undo* of {@link RunsReadPorts.moveSeen}. */
  putBackSeen(seen: SeenMove): Promise<boolean>;
  /** Builds came in front, or left it. */
  inFront(front: boolean): Promise<boolean>;
}

/** The bridge's, each named only when called, so a suite that mocks the bridge without them still loads this. */
const REAL: RunsReadPorts = {
  read: async () => readRuns(),
  mark: async (scope) => markRunsRead(scope),
  unmark: async (runs) => markRunsUnread(runs),
  open: async (run) => markRunOpened(run),
  counts: async () => runCounts(),
  moveSeen: async (scope) => moveSeen(scope),
  putBackSeen: async (seen) => putBackSeen(seen),
  inFront: async (front) => buildsInFront(front),
};

/** The runs green, yellow and grey count, of `counts`: what moving an instant clears. */
function seenCount(counts: BubbleCounts): number {
  return counts.green + counts.yellow + counts.grey;
}

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
  /** Every pipeline's unread runs by bubble, `null` until read. Replaced whole by every read. */
  #counts = $state.raw<RunCounts | null>(null);
  /** The runs green, yellow and grey count while Builds is in front, for the rows' dots. */
  #fresh = $state.raw<ReadonlySet<string>>(new Set());
  /** Whether Builds is in front, as the shell last said ({@link RunsRead.inFront}). */
  #front = $state(false);
  /** The counts by pipeline, as {@link RunsRead.#counts} holds them. */
  #byPipeline = $derived(new Map((this.#counts?.pipelines ?? []).map((p) => [p.pipeline, p.counts])));

  /**
   * Read every pipeline's counts again. Coalesced, as the top bar's reads are: any number of events while a read is
   * out make one more after it. A read that fails keeps the last answer: a count that dropped to nothing on a failed
   * read would say the trouble was over.
   */
  readCounts: () => Promise<void> = coalesce(async () => {
    try {
      const counts = await this.ports.counts();
      this.#counts = counts;
      this.#fresh = new Set(counts.fresh);
    } catch {
      // Keep the last answer; see above.
    }
  });

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
    return runningRun(run) && !(this.#read?.has(run.entity_id) ?? false) && !this.#whileRunning.has(run.entity_id);
  }

  /** Every pipeline's unread runs by bubble, `null` until read. */
  get counts(): RunCounts | null {
    return this.#counts;
  }

  /** One pipeline's unread runs by bubble, or a folder's (its pipelines summed): zero where none. */
  countsOf(...pipelines: readonly string[]): BubbleCounts {
    return pipelines.reduce((sum, id) => plusCounts(sum, this.#byPipeline.get(id) ?? noCounts()), noCounts());
  }

  /**
   * What the top bar counts, `null` until read: every pipeline's, and with Builds in front no green, yellow or grey,
   * since you are looking at them.
   */
  get topBar(): BubbleCounts | null {
    const total = this.#counts?.total;
    if (total === undefined) return null;
    return this.#front ? { ...total, green: 0, yellow: 0, grey: 0 } : total;
  }

  /**
   * Which bubble `run` counts in now (ADR-0063), or `null`: blue running with no mark, red a failure no mark reads, and
   * green, yellow or grey a run that ended in the visit's window and no mark reads. Those are known only while Builds is
   * in front, which is where a row draws them.
   */
  bubble(run: Pick<RunRow, "entity_id" | "started" | "finished" | "state">): Bubble | null {
    const bubble = runBubble(run);
    if (bubble === null || (this.#read?.has(run.entity_id) ?? false)) return null;
    if (bubble === "blue") return this.#whileRunning.has(run.entity_id) ? null : "blue";
    if (bubble === "red") return "red";
    return this.#fresh.has(run.entity_id) ? bubble : null;
  }

  /**
   * How long after a section change the backend hears it. The page just pressed reads first: told at the press, the
   * write and the counts' read queued its reads behind them (the open budget, Builds' first open at 6x on a 4K screen,
   * drawn at 328 to 387 ms against 180). A run that ends inside it counts as before the change.
   */
  settleMs = 600;
  /** What the backend last heard, `null` before it heard anything this session. */
  #told: boolean | null = null;
  #telling: ReturnType<typeof setTimeout> | null = null;
  /** Who waits on the backend hearing the latest change. */
  #waiting: (() => void)[] = [];

  /**
   * Builds came in front (`front`), or stopped being in front: the shell's section change. The top bar follows at once;
   * the backend hears it {@link RunsRead.settleMs} later, the last change only. Coming in front, it holds the visit's
   * start; leaving, it moves the instant green counts from. The counts are read again after. Resolves once it heard.
   */
  inFront(front: boolean): Promise<void> {
    if (front !== this.#front) {
      this.#front = front;
      // Leaving a visit the backend heard moves the instant to now, so the visit's green, yellow and grey are seen: drawn
      // gone at once, as the read after the leave will say. Kept, the top bar showed them until the backend heard.
      if (!front && this.#told === true) this.#seen();
      this.#tell();
    }
    return this.#heard();
  }

  /** Every pipeline's green, yellow and grey drawn gone: the instant moved past them. */
  #seen(): void {
    const counts = this.#counts;
    if (counts === null) return;
    const left = (c: BubbleCounts): BubbleCounts => ({ ...c, green: 0, yellow: 0, grey: 0 });
    const pipelines = counts.pipelines.map((p) => ({ ...p, counts: left(p.counts) })).filter((p) => p.counts.blue + p.counts.red > 0);
    this.#counts = { ...counts, pipelines, total: left(counts.total), fresh: [] };
    this.#fresh = new Set();
  }

  /**
   * The shell's start, off Builds: a visit a crash left open ends, so its start no longer bounds the counts. On Builds
   * the start is {@link RunsRead.inFront} instead.
   */
  started(): Promise<void> {
    this.#front = false;
    this.#told = null;
    this.#tell();
    return this.#heard();
  }

  /** The window's shell went (unmounted): a change the backend has not heard yet is dropped; the quit leaves Builds itself. */
  untell(): void {
    if (this.#telling !== null) clearTimeout(this.#telling);
    this.#telling = null;
    for (const resolve of this.#waiting.splice(0)) resolve();
  }

  /** The backend hears {@link RunsRead.#front} once the page has had its turn; a later change takes the earlier's place. */
  #tell(): void {
    if (this.#telling !== null) clearTimeout(this.#telling);
    this.#telling = setTimeout(() => void this.#flush(), this.settleMs);
  }

  #heard(): Promise<void> {
    return this.#telling === null ? Promise.resolve() : new Promise((resolve) => this.#waiting.push(resolve));
  }

  async #flush(): Promise<void> {
    this.#telling = null;
    const front = this.#front;
    if (front !== this.#told) {
      this.#told = front;
      try {
        await this.ports.inFront(front);
      } catch {
        // The instant stays where it was: the next change tells it again.
        this.#told = null;
      }
    }
    await this.readCounts();
    for (const resolve of this.#waiting.splice(0)) resolve();
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
      void this.readCounts();
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
      void this.readCounts();
    } catch (cause) {
      this.#with(read, [], running);
      push({ text: `Could not mark ${counted(marked.length, "run", "runs")} as unread: ${ipcErrorMessage(cause)}`, tone: "err" });
    }
  }

  /**
   * *Mark as read* or *Mark as unread* on one run, whichever it is now: a failure read or unread, a running run out of
   * the blue or back in it (#2574). A run with nothing to mark ({@link markableRun}: passed, cancelled, queued) takes
   * nothing, a mark it has from opening it included.
   */
  toggle(run: Pick<RunRow, "entity_id" | "started" | "finished" | "state">): Promise<void> {
    if (!markableRun(run)) return Promise.resolve();
    return this.readable(run) ? this.markRead([run.entity_id]) : this.markUnread([run.entity_id]);
  }

  /** What the row's mark offers on `run` (#2574): *Mark as read* on an unread failure or a run in the blue. */
  readable(run: Pick<RunRow, "entity_id" | "started" | "finished" | "state">): boolean {
    return this.unread(run) || this.inBlue(run);
  }

  /**
   * *Mark all as read*: the unread runs drawn, and those an open page of one of `scope.pipelines` holds
   * ({@link RunsRead.hold}), or every page held for `everything`, at once; every run of the scope, older pages
   * included, by the backend, and the scope's green, yellow and grey by moving its instant (ADR-0063). The scope's
   * counts go to zero at once. Says how many it marked, with *Undo*, which takes back the rows and puts the instant
   * back; a press that marked nothing says so.
   */
  async markAll(scope: MarkAll): Promise<string[]> {
    const everything = scope.everything ?? false;
    const held = everything ? [...this.#held.values()].flat() : scope.pipelines.flatMap((id) => this.#held.get(id) ?? []);
    const drawn = [...new Set([...scope.runs, ...held])].filter((id) => !(this.#read?.has(id) ?? false));
    const wasRunning = drawn.filter((id) => this.#whileRunning.has(id));
    const reached = everything || scope.pipelines.length > 0;
    const cleared = !reached ? 0 : everything ? seenCount(this.#counts?.total ?? noCounts()) : seenCount(this.countsOf(...new Set(scope.pipelines)));
    this.#with(drawn);
    const countsBefore = this.#counts;
    if (reached) this.#cleared(everything ? null : scope.pipelines);
    const asked: MarkScope = { everything, pipelines: everything ? [] : [...scope.pipelines], runs: [...drawn] };
    let wrote: RunMarks;
    let moved: SeenMove | null = null;
    try {
      [wrote, moved] = await this.#writing(
        Promise.all([
          this.ports.mark(asked),
          // A move that fails leaves the green as it was; the rows' mark still stands.
          reached ? this.ports.moveSeen(asked).catch(() => null) : Promise.resolve(null),
        ]),
      );
    } catch (cause) {
      this.#with([], drawn, wasRunning);
      if (this.#counts !== null && countsBefore !== null) this.#counts = countsBefore;
      push({ text: `Could not mark the runs as read: ${ipcErrorMessage(cause)}`, tone: "err" });
      return [];
    }
    this.#wrote(wrote, wasRunning);
    void this.readCounts();
    const ids = idsOf(wrote);
    const marked = ids.length + (moved === null ? 0 : cleared);
    if (marked === 0) push({ text: "No unread runs", subject: "runs-read" });
    else undoable(`Marked ${counted(marked, "run", "runs")} as read`, () => void this.#undoAll(ids, moved));
    return ids;
  }

  /** *Undo* of {@link RunsRead.markAll}: its rows go, and the instant it moved goes back. */
  async #undoAll(ids: readonly string[], moved: SeenMove | null): Promise<void> {
    const rows = ids.length > 0 ? this.markUnread(ids) : Promise.resolve();
    const seen = moved === null ? Promise.resolve(false) : this.ports.putBackSeen(moved).catch(() => false);
    await Promise.all([rows, seen]);
    void this.readCounts();
  }

  /** A scope's counts drawn cleared at once, `null` for every pipeline: the read after the mark says the rest. */
  #cleared(pipelines: readonly string[] | null): void {
    const counts = this.#counts;
    if (counts === null) return;
    if (pipelines === null) {
      this.#counts = { ...counts, pipelines: [], total: noCounts(), fresh: [] };
      this.#fresh = new Set();
      return;
    }
    const gone = new Set(pipelines);
    const kept = counts.pipelines.filter((p) => !gone.has(p.pipeline));
    this.#counts = { ...counts, pipelines: kept, total: kept.reduce((sum, p) => plusCounts(sum, p.counts), noCounts()) };
  }

  /** Back to nothing read and the real IPC, for a test. */
  forget(): void {
    this.#read = null;
    this.#whileRunning = new Set();
    this.#loading = null;
    this.#held.clear();
    this.#writes.clear();
    this.#opened.clear();
    this.#counts = null;
    this.#fresh = new Set();
    this.#front = false;
    this.untell();
    this.#told = null;
    this.settleMs = 600;
    this.afterOpen = null;
    this.ports = REAL;
  }
}

/** The window's one. */
export const runsRead = new RunsRead();
