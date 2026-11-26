/**
 * Commands (ADR-0060), the window's state: the saved list, the run a dialog shows, and the runs of this session that
 * the status bar and the menus read.
 *
 * - **The press paints the dialog.** {@link OneOffs.openRun} builds the dialog's run from what the page knows (the
 *   command's name, the target, its words filled by the TypeScript twin) in the press's own task, and asks the backend
 *   after: the dialog says *Running* at 0:00 before the program has started.
 * - **The window never parses.** A run's lines come read already (colour runs, places, findings) in batches on the
 *   run's channel; {@link Run.take} pushes them onto a plain array and bumps `version`, which is what the rows read.
 * - **Closing leaves a run running.** The dialog lets go of its channel; the run goes on, the status bar counts it,
 *   and a toast says how it ended with *Show*, which opens it again on what it kept.
 */
import type { Channel } from "@tauri-apps/api/core";

import {
  batchChannel,
  oneoffCommands,
  oneoffLines,
  oneoffResult,
  oneoffRun,
  oneoffRuns,
  oneoffSave,
  oneoffStop,
  oneoffWatch,
  oneoffForget,
  oneoffShellPath,
  oneoffRepoCommands,
  oneoffCopyRepoCommands,
  setOneoffCommands,
  type OneOff,
  type OneOffDraft,
  type OneOffShell,
  type OneOffRepoCommands,
  type OneOffBatch,
  type OneOffEnded,
  type OneOffLine,
  type OneOffResult,
  type OneOffRunRow,
  type OneOffStarted,
  type OneOffTarget,
} from "../ipc/oneoff";
import { EVENTS } from "../ipc";
import { listen } from "@tauri-apps/api/event";
import { push } from "../shell/toasts.svelte";
import { endedShort, fill, kindOf, shown, stateOf, targetLabel } from "./oneoff";

/** What the store asks of the backend: the real commands, or a test's. */
export interface OneOffPorts {
  commands(): Promise<OneOff[]>;
  run(entityId: string, command: string, target: OneOffTarget, madeToken: string | null, onbatch: (batch: OneOffBatch) => void): Promise<{ started: OneOffStarted; stop: () => void }>;
  stop(run: string): Promise<void>;
  runs(entityId: string | null): Promise<OneOffRunRow[]>;
  lines(run: string, from: number, count: number): Promise<OneOffBatch | null>;
  watch(run: string, onbatch: (batch: OneOffBatch) => void): Promise<{ known: boolean; stop: () => void }>;
  save(run: string, name: string, colours: boolean, result?: boolean, file?: string | null): Promise<string>;
  result(run: string, file: string | null): Promise<Uint8Array>;
  copy(text: string): Promise<void>;
  setCommands(drafts: OneOffDraft[]): Promise<OneOff[]>;
  forget(run: string | null): Promise<void>;
  shell(again: boolean, on: boolean | null): Promise<OneOffShell>;
  repoCommands(entityId: string): Promise<OneOffRepoCommands>;
  copyRepoCommands(entityId: string, names: string[]): Promise<OneOff[]>;
  /** Hears `oneoff:moved` until the answer is called. */
  moved(handler: (row: OneOffRunRow) => void): () => void;
}

const channelled = <T>(start: (channel: Channel<OneOffBatch>) => Promise<T>, onbatch: (batch: OneOffBatch) => void) => {
  const { channel, stop } = batchChannel(onbatch);
  return start(channel).then((answer) => ({ answer, stop }), (error: unknown) => {
    stop();
    throw error;
  });
};

export const realOneOffPorts: OneOffPorts = {
  commands: () => oneoffCommands(),
  run: (entityId, command, target, madeToken, onbatch) => channelled((ch) => oneoffRun(entityId, command, target, madeToken, ch), onbatch).then(({ answer, stop }) => ({ started: answer, stop })),
  stop: (run) => oneoffStop(run),
  runs: (entityId) => oneoffRuns(entityId),
  lines: (run, from, count) => oneoffLines(run, from, count),
  watch: (run, onbatch) => channelled((ch) => oneoffWatch(run, ch), onbatch).then(({ answer, stop }) => ({ known: answer, stop })),
  save: (run, name, colours, result, file) => oneoffSave(run, name, colours, result ?? false, file ?? null),
  result: (run, file) => oneoffResult(run, file),
  copy: (text) => navigator.clipboard.writeText(text),
  setCommands: (drafts) => setOneoffCommands(drafts),
  forget: (run) => oneoffForget(run),
  shell: (again, on) => oneoffShellPath(again, on),
  repoCommands: (entityId) => oneoffRepoCommands(entityId),
  copyRepoCommands: (entityId, names) => oneoffCopyRepoCommands(entityId, names),
  moved: (handler) => {
    const heard = listen<OneOffRunRow>(EVENTS.oneoffMoved, (event) => handler(event.payload));
    return () => void heard.then((off) => off(), () => {});
  },
};

/** Where a menu was opened, as what `command` runs on: a folder's command on the repository runs on its top folder. */
export function targetOf(
  at:
    | { kind: "repository" }
    | { kind: "folder"; path: string }
    | { kind: "file"; path: string; line: number | null }
    | { kind: "commit"; sha: string }
    | { kind: "made"; renderer: string; folder: string; path: string; dir: boolean; line: number | null },
  command: OneOff | null = null,
): OneOffTarget {
  switch (at.kind) {
    case "made":
      return { kind: "made", renderer: at.renderer, folder: at.folder, path: at.path, dir: at.dir, lines: at.line === null ? null : [at.line, at.line] };
    case "repository":
      return command !== null && kindOf(command.command, command.stdin) === "folder" ? { kind: "folder", path: "" } : { kind: "repository" };
    case "folder":
      return { kind: "folder", path: at.path };
    case "file":
      return { kind: "file", path: at.path, lines: at.line === null ? null : [at.line, at.line], at: null };
    case "commit":
      return { kind: "commit", sha: at.sha };
  }
}

/**
 * A generated folder a page keeps (ADR-0046; ADR-0060 amended 2026-11-25): the token and the folder on disk a run on it
 * runs in, and how the page reads it back when the backend no longer holds the token (one `look`).
 */
export interface MadeKept {
  token: string;
  dir: string;
  look?: () => Promise<void>;
}

/** Whether `target` is a generated folder's. */
export const isMade = (target: OneOffTarget): target is Extract<OneOffTarget, { kind: "made" | "made_files" }> => target.kind === "made" || target.kind === "made_files";

const madeKey = (entityId: string, renderer: string, folder: string) => `${entityId}\u0000${renderer}\u0000${folder}`;

/** What a run on a generated folder says when none is kept for it (Fable, 2026-11-25). */
export const notKept = (renderer: string, folder: string) => `${folder === "" ? "The repository" : folder} as ${renderer} makes it is not kept now. Open it, then run again.`;

/** The views a run gives: its result first where it made one. */
export type RunView = "output" | "problems" | "result" | "files";

/** One file of a batch (section 12): its own run, once started, and how it ended. */
export interface BatchItem {
  path: string;
  run: Run | null;
  ended: OneOffEnded | null;
}

/** The most files a batch runs, one run a file. */
export const BATCH_MAX = 200;
/** How many of a batch's runs go at once: the app's own limit. */
export const BATCH_AT_ONCE = 4;

/** A run as a dialog shows it. */
export class Run {
  /** The run's id once the backend answered. */
  id = $state<string | null>(null);
  readonly command: OneOff;
  readonly entityId: string;
  readonly repo: string;
  readonly target: OneOffTarget;
  argv = $state<string[]>([]);
  cwd = $state("");
  branch = $state<string | null>(null);
  ended = $state<OneOffEnded | null>(null);
  /** A refusal before anything started: the start was refused by the backend. */
  refused = $state<string | null>(null);
  startedAt = $state(Date.now());
  /** How long it ran, once it ended; the backend's count while it runs. */
  elapsed = $state(0);
  total = $state(0);
  dropped = $state(0);
  tailFrom = $state(0);
  bytes = $state(0);
  findings = $state(0);
  /** Bumped when the lines change: what the rows read instead of a deep proxy of every line. */
  version = $state(0);
  view = $state<RunView>("output");
  wrap = $state(false);
  follow = $state(true);
  stream = $state<"both" | "out" | "err">("both");
  /** What it made that is not its lines, once it ended. */
  result = $state<OneOffResult | null>(null);
  /** A batch's files (one run a file), `null` for one run. */
  batch = $state<BatchItem[] | null>(null);
  /** The batch's picked file. */
  batchAt = $state(0);
  /** *Stop* on a batch: what runs is stopped and nothing more starts. */
  batchStopped = false;
  /** The person picked a view: a result arriving does not take it away. */
  picked = false;
  /** The output line a place press or `N`/`P` stands at. */
  at = $state<number | null>(null);
  /** The generated files a run on a generated folder changed, once it ended; a batch's, all its files'. */
  madeChanged = $state<string[] | null>(null);
  /** The kept folder a run on a generated folder ran in. */
  made: MadeKept | null = null;
  /** The lines, in order, as the window holds them: plain, so a flood costs no proxy. */
  lines: OneOffLine[] = [];
  /** The longest line, in characters: the scroller's width. */
  widest = 0;
  /** Whether each stream wrote: the stream select is drawn only when both did. */
  wrote = { out: false, err: false };
  /** The lines that hold a finding, by number: *Problems* reads these, never every line. */
  found = new Map<number, OneOffLine>();
  private release: (() => void) | null = null;

  constructor(command: OneOff, entityId: string, repo: string, target: OneOffTarget, argv: string[]) {
    this.command = command;
    this.entityId = entityId;
    this.repo = repo;
    this.target = target;
    this.argv = argv;
    if (command.shows === "problems") this.view = "problems";
  }

  get running(): boolean {
    return this.ended === null && this.refused === null;
  }

  /** Takes one batch: new lines onto the end, changed ones in place, the counts and the ending. */
  take(batch: OneOffBatch): void {
    for (const line of batch.lines) this.put(line);
    for (const line of batch.changed) this.put(line);
    this.total = batch.total;
    this.dropped = batch.dropped;
    this.tailFrom = batch.tail_from;
    this.bytes = batch.bytes;
    this.findings = batch.findings;
    this.elapsed = batch.elapsed_ms;
    if (batch.dropped > 0) this.forgetMiddle(batch.tail_from);
    if (batch.result !== null && batch.result !== undefined && this.result === null) {
      this.result = batch.result;
      if (!this.picked && batch.result.from !== "refused") this.view = batch.result.from === "files" ? "files" : "result";
    }
    if (batch.made_changed !== null && batch.made_changed !== undefined) this.changedMade(batch.made_changed);
    if (batch.ended !== null && batch.ended !== undefined) {
      this.ended = batch.ended;
      this.let();
    }
    this.version += 1;
  }

  /** Adds generated files a run changed, each once. */
  changedMade(paths: readonly string[]): void {
    const had = this.madeChanged ?? [];
    const more = paths.filter((p) => !had.includes(p));
    if (this.madeChanged === null || more.length > 0) this.madeChanged = [...had, ...more];
  }

  private put(line: OneOffLine): void {
    const lines = this.lines;
    const last = lines[lines.length - 1];
    if (last === undefined || line.n > last.n) lines.push(line);
    else {
      let lo = 0;
      let hi = lines.length - 1;
      while (lo < hi) {
        const mid = (lo + hi) >> 1;
        if ((lines[mid] as OneOffLine).n < line.n) lo = mid + 1;
        else hi = mid;
      }
      if (lines[lo]?.n === line.n) lines[lo] = line;
      else lines.splice(lo, 0, line);
    }
    if (line.text.length > this.widest) this.widest = line.text.length;
    this.wrote[line.stream] = true;
    if (line.finding) this.found.set(line.n, line);
    else this.found.delete(line.n);
  }

  /** The backend dropped the middle: so does the window, keeping the first lines and the tail. */
  private forgetMiddle(tailFrom: number): void {
    const lines = this.lines;
    let cut = 0;
    while (cut < lines.length && (lines[cut] as OneOffLine).n < 2000) cut++;
    let to = cut;
    while (to < lines.length && (lines[to] as OneOffLine).n < tailFrom) to++;
    for (let i = cut; i < to; i++) this.found.delete((lines[i] as OneOffLine).n);
    if (to > cut) lines.splice(cut, to - cut);
  }

  /** Holds the channel's release, so a close lets go of it. */
  hold(release: () => void): void {
    this.release = release;
  }

  /** Lets go of the run's channel: the dialog closed, or the run ended. */
  let(): void {
    this.release?.();
    this.release = null;
  }
}

/** The store. */
export class OneOffs {
  commands = $state<OneOff[]>([]);
  loaded = $state(false);
  /** The run the dialog shows. */
  open = $state<Run | null>(null);
  /** The runs of this session and the list's, newest first, as `oneoff:moved` keeps them. */
  rows = $state<OneOffRunRow[]>([]);
  private ports: OneOffPorts;
  /** The runs started from this window, whose ending a toast says when their dialog is closed. */
  private mine = new Set<string>();
  /** The generated folders the pages keep now, by repository, renderer and folder. */
  private kept = new Map<string, MadeKept>();

  constructor(ports: OneOffPorts = realOneOffPorts) {
    this.ports = ports;
  }

  /** The runs still going. */
  get running(): OneOffRunRow[] {
    return this.rows.filter((r) => r.ended === null || r.ended === undefined);
  }

  /**
   * A page keeps the generated folder `renderer` made of `folder` (`null`: no longer): what a run on it, *Run again* and
   * the right-click menu's rows take.
   */
  keepMade(entityId: string, renderer: string, folder: string, kept: MadeKept | null): void {
    if (kept === null) this.kept.delete(madeKey(entityId, renderer, folder));
    else this.kept.set(madeKey(entityId, renderer, folder), kept);
  }

  /** The generated folder kept now for `renderer` and `folder`, if a page keeps one. */
  madeOf(entityId: string, renderer: string, folder: string): MadeKept | null {
    return this.kept.get(madeKey(entityId, renderer, folder)) ?? null;
  }

  /** Hears runs start and end until the answer is called: the window's host holds it. */
  hear(): () => void {
    return this.ports.moved((row) => this.moved(row));
  }

  /** Reads the saved commands and the runs. */
  async load(): Promise<void> {
    const [commands, rows] = await Promise.all([this.ports.commands().catch(() => [] as OneOff[]), this.ports.runs(null).catch(() => [] as OneOffRunRow[])]);
    this.commands = commands;
    this.rows = rows;
    this.loaded = true;
  }

  /** Saves the whole list (Settings); the menus read it as stored. Rejects naming the entry that cannot be saved. */
  async save(drafts: OneOffDraft[]): Promise<OneOff[]> {
    const stored = await this.ports.setCommands(drafts);
    this.commands = stored;
    this.loaded = true;
    return stored;
  }

  /** Forgets one run, or every run that ended; the list reads again. */
  async forget(run: string | null): Promise<void> {
    await this.ports.forget(run);
    this.rows = await this.ports.runs(null).catch(() => this.rows);
  }

  /** What a repository's `.knobas/commands.toml` offers, read from its checkout now. */
  repoCommands(entityId: string): Promise<OneOffRepoCommands> {
    return this.ports.repoCommands(entityId);
  }

  /** Copies the named commands of a repository's file into the list, scoped to it. */
  async copyRepoCommands(entityId: string, names: string[]): Promise<OneOff[]> {
    const stored = await this.ports.copyRepoCommands(entityId, names);
    this.commands = stored;
    return stored;
  }

  /** *Where programs are found*: the login shell's `PATH`, read again with `again`, the switch set with `on`. */
  shell(again: boolean, on: boolean | null = null): Promise<OneOffShell> {
    return this.ports.shell(again, on);
  }

  private moved(row: OneOffRunRow): void {
    const at = this.rows.findIndex((r) => r.id === row.id);
    if (at >= 0) this.rows[at] = row;
    else this.rows = [row, ...this.rows];
    const ended = row.ended ?? null;
    if (ended === null || !this.mine.has(row.id)) return;
    this.mine.delete(row.id);
    if (this.open?.id === row.id) return;
    const st = stateOf(ended);
    push({
      text: `${row.name} ${ended.how === "exit" ? `exited with ${ended.code}` : endedShort(ended)} on ${targetLabel(row.target, row.entity_id.split("/").pop() ?? row.entity_id)}`,
      tone: st.tone === "err" ? "err" : "plain",
      action: { label: "Show", run: () => void this.reopen(row.id) },
      subject: `oneoff:${row.id}`,
    });
  }

  /**
   * Opens the dialog on a new run of `command` on `target`, painted from what the page knows, and starts it. `root` is
   * the checkout as the page knows it, for the command line before the backend answers.
   */
  openRun(command: OneOff, entityId: string, repo: string, target: OneOffTarget, root: string | null = null): Run {
    this.close();
    const made = isMade(target) ? this.madeOf(entityId, target.renderer, target.folder) : null;
    const run = new Run(command, entityId, repo, target, filledArgv(command, target, made?.dir ?? root ?? repo));
    run.made = made;
    this.open = run;
    if (isMade(target) && made === null) {
      run.refused = notKept(target.renderer, target.folder);
      return run;
    }
    void this.start(run, entityId, command, target, made)
      .then(({ started, stop }) => {
        run.id = started.run;
        run.argv = started.argv.length > 0 ? started.argv : run.argv;
        run.cwd = started.cwd;
        run.branch = started.branch ?? null;
        run.startedAt = started.started_at;
        this.mine.add(started.run);
        if (run.ended !== null || this.open !== run) stop();
        else run.hold(stop);
      })
      .catch((error: unknown) => {
        run.refused = (error as { message?: string } | null)?.message ?? String(error);
      });
    return run;
  }

  /**
   * Starts `command` on `target`, in the kept folder `made` names for a generated folder's. A token the backend no longer
   * holds is read back by the page's one `look`, then asked once more (Fable, 2026-11-25).
   */
  private start(run: Run, entityId: string, command: OneOff, target: OneOffTarget, made: MadeKept | null) {
    const ask = (kept: MadeKept | null) => this.ports.run(entityId, command.id, target, kept?.token ?? null, (batch) => run.take(batch));
    if (made === null || made.look === undefined) return ask(made);
    const look = made.look;
    return ask(made).catch(async (error: unknown) => {
      if ((error as { code?: string } | null)?.code !== "not_found" || !isMade(target)) throw error;
      await look();
      const again = this.madeOf(entityId, target.renderer, target.folder) ?? made;
      run.made = again;
      return ask(again);
    });
  }

  /** Opens the dialog on a kept run: its newest lines first, and its next batches while it runs. */
  async reopen(id: string): Promise<Run | null> {
    const row = this.rows.find((r) => r.id === id) ?? (await this.ports.runs(null).catch(() => [])).find((r) => r.id === id);
    if (row === undefined) return null;
    const command = this.commands.find((c) => c.id === row.command) ?? ({ ...EMPTY, id: row.command, name: row.name, command: shown(row.argv) } as OneOff);
    this.close();
    const run = new Run(command, row.entity_id, row.entity_id.split("/").pop() ?? row.entity_id, row.target, row.argv);
    run.id = row.id;
    run.cwd = row.cwd;
    run.branch = row.branch ?? null;
    run.startedAt = row.started_at;
    this.open = run;
    const running = row.ended === null || row.ended === undefined;
    if (running) {
      const watched = await this.ports.watch(id, (batch) => run.take(batch)).catch(() => null);
      if (watched !== null) {
        if (this.open === run && run.ended === null) run.hold(watched.stop);
        else watched.stop();
      }
    }
    const newest = Math.max(0, row.lines - 2000);
    const tail = await this.ports.lines(id, newest, 2000).catch(() => null);
    if (tail === null) {
      run.ended = row.ended ?? null;
      run.elapsed = row.duration_ms ?? 0;
      run.version += 1;
      return run;
    }
    run.take({ ...tail, ended: running ? null : (tail.ended ?? null) });
    if (!running) run.ended = row.ended ?? tail.ended ?? null;
    if (newest > 0) {
      const head = await this.ports.lines(id, 0, newest).catch(() => null);
      if (head !== null) run.take({ ...head, ended: run.ended });
    }
    return run;
  }

  /** Closes the dialog; a run still going goes on. */
  close(): void {
    this.open?.let();
    this.open = null;
  }

  /** Stops the dialog's run with everything it started; a batch's runs, and it starts no more. */
  stop(run: Run): void {
    if (run.batch !== null) {
      run.batchStopped = true;
      for (const item of run.batch) {
        if (item.ended === null && item.run === null) item.ended = { how: "stopped" };
        if (item.ended === null && item.run?.id) void this.ports.stop(item.run.id);
      }
      run.version += 1;
      return;
    }
    if (run.id !== null) void this.ports.stop(run.id);
  }

  /**
   * A file's command on many files (section 12): one run a file, four at a time, at most 200, in a dialog that lists
   * them with how each ended and draws the picked one's output.
   */
  openBatch(command: OneOff, entityId: string, repo: string, paths: readonly string[], root: string | null = null, of: { renderer: string; folder: string } | null = null): Run {
    this.close();
    const files = paths.slice(0, BATCH_MAX);
    const whole: OneOffTarget = of === null ? { kind: "files", paths: [...files] } : { kind: "made_files", renderer: of.renderer, folder: of.folder, paths: [...files] };
    const one = (path: string): OneOffTarget => (of === null ? { kind: "file", path, lines: null, at: null } : { kind: "made", renderer: of.renderer, folder: of.folder, path, dir: false, lines: null });
    const made = of === null ? null : this.madeOf(entityId, of.renderer, of.folder);
    const run = new Run(command, entityId, repo, whole, [command.command]);
    void root;
    run.made = made;
    run.batch = files.map((path) => ({ path, run: null, ended: null }));
    this.open = run;
    if (of !== null && made === null) {
      run.refused = notKept(of.renderer, of.folder);
      return run;
    }
    let next = 0;
    let going = 0;
    const settle = () => {
      const items = run.batch ?? [];
      if (items.every((item) => item.ended !== null)) {
        const failed = items.filter((item) => !(item.ended?.how === "exit" && item.ended.code === 0)).length;
        run.ended = run.batchStopped ? { how: "stopped" } : { how: "exit", code: failed > 0 ? 1 : 0 };
        run.elapsed = Date.now() - run.startedAt;
      }
      run.version += 1;
    };
    const start = () => {
      while (going < BATCH_AT_ONCE && next < files.length && !run.batchStopped) {
        const item = (run.batch as BatchItem[])[next]!;
        next += 1;
        going += 1;
        const sub = new Run(command, entityId, repo, one(item.path), []);
        sub.made = made;
        item.run = sub;
        const done = (ended: OneOffEnded) => {
          if (item.ended !== null) return;
          item.ended = ended;
          going -= 1;
          start();
          settle();
        };
        void this.ports
          .run(entityId, command.id, one(item.path), made?.token ?? null, (batch) => {
            sub.take(batch);
            if (batch.made_changed) run.changedMade(batch.made_changed);
            if (batch.ended !== null && batch.ended !== undefined) done(batch.ended);
            else run.version += 1;
          })
          .then(({ started }) => {
            sub.id = started.run;
            sub.argv = started.argv;
          })
          .catch((error: unknown) => {
            const reason = (error as { message?: string } | null)?.message ?? String(error);
            sub.refused = reason;
            done({ how: "not_started", reason });
          });
      }
      if (run.batchStopped) for (const item of (run.batch as BatchItem[]).slice(next)) if (item.ended === null && item.run === null) item.ended = { how: "stopped" };
      settle();
    };
    start();
    return run;
  }

  /** Runs the dialog's command again on the same thing, as saved now. */
  again(run: Run, root: string | null = null): Run {
    const command = this.commands.find((c) => c.id === run.command.id) ?? run.command;
    const of = run.target.kind === "made_files" ? { renderer: run.target.renderer, folder: run.target.folder } : null;
    if (run.batch !== null) return this.openBatch(command, run.entityId, run.repo, run.batch.map((item) => item.path), root, of);
    return this.openRun(command, run.entityId, run.repo, run.target, root);
  }

  /** The earlier runs of a command on one thing, newest first. */
  history(run: Run): OneOffRunRow[] {
    const key = JSON.stringify(run.target);
    return this.rows.filter((r) => r.command === run.command.id && r.entity_id === run.entityId && JSON.stringify(r.target) === key);
  }

  /** The *Run…* menus standing, each with how specific it is. */
  private menus = new Set<{ priority: number; open: () => void }>();

  /** A *Run…* menu stands until the answer is called. */
  menu(priority: number, open: () => void): () => void {
    const entry = { priority, open };
    this.menus.add(entry);
    return () => this.menus.delete(entry);
  }

  /** `X`: opens the most specific *Run…* menu standing (a file's over a folder's over the repository's). */
  openMenu(): boolean {
    let best: { priority: number; open: () => void } | null = null;
    for (const m of this.menus) if (best === null || m.priority >= best.priority) best = m;
    best?.open();
    return best !== null;
  }

  /** `Shift X`: runs the last command again on the thing it ran on, with no menu. */
  runLast(): boolean {
    const row = this.rows[0];
    if (row === undefined) return false;
    const command = this.commands.find((c) => c.id === row.command);
    if (command === undefined) return false;
    this.openRun(command, row.entity_id, row.entity_id.split("/").pop() ?? row.entity_id, row.target);
    return true;
  }

  copy(text: string): Promise<void> {
    return this.ports.copy(text);
  }

  /** The bytes of a run's result, or of one file it wrote to `{outdir}`. */
  resultBytes(run: Run, file: string | null = null): Promise<Uint8Array> {
    if (run.id === null) return Promise.reject(new Error("The run has not started."));
    return this.ports.result(run.id, file);
  }

  /** *Download* of the result, or of the file picked in *Files*. */
  saveResult(run: Run, file: string | null): Promise<string> {
    if (run.id === null) return Promise.reject(new Error("The run has not started."));
    const name = file !== null ? (file.split("/").pop() ?? file) : run.result?.from === "one" && run.result.name !== "stdout" ? run.result.name : `${run.command.id}-result${extOf(run.result)}`;
    return this.ports.save(run.id, `${run.command.id}-${name}`, false, true, file);
  }

  saveRun(run: Run, colours: boolean): Promise<string> {
    if (run.id === null) return Promise.reject(new Error("The run has not started."));
    const thing =
      run.target.kind === "file" || (run.target.kind === "made" && !run.target.dir)
        ? (run.target.path.split("/").pop() ?? "file")
        : run.target.kind === "folder" || run.target.kind === "made"
          ? run.target.path.split("/").pop() || (run.target.kind === "made" ? run.target.folder.split("/").pop() : "") || run.repo
          : run.repo;
    return this.ports.save(run.id, `${run.command.id}-${thing}.txt`, colours);
  }
}

/** A result's file extension for its download name. */
function extOf(result: OneOffResult | null): string {
  if (result?.from !== "one") return "";
  const k = result.kind;
  if (k.kind === "image") return k.mime === "image/svg+xml" ? ".svg" : `.${k.mime.split("/")[1] ?? "img"}`;
  return { pdf: ".pdf", page: ".html", json: ".json", markdown: ".md", diff: ".diff", text: ".txt", bytes: ".bin", code: k.kind === "code" ? `.${k.ext}` : "" }[k.kind];
}

const EMPTY: Omit<OneOff, "id" | "name" | "command"> = {
  stdin: "none",
  files: [],
  folders: [],
  has: [],
  repo_holds: [],
  repos: [],
  shows: "auto",
  start_in: "root",
  path: [],
  env: [],
  colours: true,
  timeout_secs: 120,
  wsl_paths: false,
};

/** The window's one store. */
export const oneoffs = new OneOffs();

/** The words `command` becomes on `target`, as the page knows it, `root` the checkout (or the kept folder for a generated folder's). */
export function filledArgv(command: OneOff, target: OneOffTarget, root: string): string[] {
  const file = target.kind === "file" || (target.kind === "made" && !target.dir) ? target.path : null;
  const parent = (path: string) => path.slice(0, Math.max(0, path.lastIndexOf("/")));
  const lines = target.kind === "file" || target.kind === "made" ? (target.lines ?? null) : null;
  const filled = fill(command.command, {
    root,
    file,
    dir: target.kind === "folder" || (target.kind === "made" && target.dir) ? target.path : file !== null ? parent(file) : target.kind === "repository" ? "" : null,
    files: target.kind === "files" || target.kind === "made_files" ? target.paths : [],
    line: lines?.[0] ?? null,
    endline: lines?.[1] ?? null,
    sha: target.kind === "commit" ? target.sha : null,
    base: target.kind === "range" ? target.base : null,
    head: target.kind === "range" ? target.head : null,
  });
  return "argv" in filled ? filled.argv : [command.command];
}
