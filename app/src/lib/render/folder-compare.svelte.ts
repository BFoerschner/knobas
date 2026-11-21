/**
 * A folder's generated compare (ux26, `docs/decisions/2026-11-20-a-comparison-is-drawn-rendered-and-may-end-at-the-
 * working-tree.md` rulings 7 and 8; the decision list's sections 9 to 11): the folder renderer run on the folder at each
 * side of a comparison, each side in a lane of its own (`cmp:<end>`), and what the two runs made compared.
 *
 * - **Only a press runs.** Each side is a `FolderRun` asked with `look` always: what is kept for the folder as it is at
 *   that end draws at once, and nothing starts on a look. `press` runs the sides that are not kept (*Run*, *Run again*),
 *   or stops every side that runs (*Stop*).
 * - A side with no folder (`no_folder`: added or deleted with the change, or a root commit's before) is compared as
 *   nothing: every generated file of the other side is added or deleted.
 * - Once neither side runs and each is kept (or has no folder), the two are compared once per pair of tokens:
 *   `folder_render_compare` for a folder of files, the two outputs' bytes for a renderer that draws one. A side made
 *   from an earlier state (`current` false) is compared all the same, and the state says it is stale.
 *
 * Built inside an effect's root: its effects are the caller's.
 */
import { untrack } from "svelte";

import { ipcErrorMessage } from "../ipc";
import { folderRenderCompare, type FolderRenderer, type MadeCompare } from "../ipc/app";
import { FolderRun, ipcFolderRenderPorts, type FolderRenderPorts } from "./folder-run.svelte";

/** The working tree's token (`tf/diff/address.ts`'s `WORKING_TREE`). */
const WORKING = "~working-tree";

/** What a folder's generated compare reaches the backend through: the IPC, a fake in tests. */
export interface FolderComparePorts extends FolderRenderPorts {
  compare(before: string | null, after: string | null, eolAside: boolean): Promise<MadeCompare>;
}

export const ipcFolderComparePorts: FolderComparePorts = { ...ipcFolderRenderPorts, compare: (b, a, eol) => folderRenderCompare(b, a, eol) };

/** Where a folder's generated compare is: the repository, the renderer, the folder and the two ends. */
export interface FolderCompareAt {
  entityId: string;
  renderer: FolderRenderer;
  folder: string;
  /** A whole sha, or `null`: nothing before (a root commit). */
  before: string | null;
  /** A whole sha, or the working tree's token. */
  after: string;
}

/** How one side stands. */
export type SideState = "asking" | "none" | "running" | "kept" | "stale" | "failed" | "absent";

/** How the folder's generated compare stands, as its bar says it. */
export type CompareState = "asking" | "idle" | "running" | "comparing" | "made" | "same" | "failed" | "stale";

/** What the two made, compared: the generated files that differ, or a single output's two drawings. */
export type Compared =
  | { kind: "folder"; old: string | null; new: string | null; compare: MadeCompare }
  | { kind: "single"; old: string | null; new: string | null; same: boolean };

/** The lane of the side read at `end` (ADR-0049): its own slot, so both sides run at once. */
export const laneOf = (end: string) => `cmp:${end}`;

export class FolderCompare {
  readonly before: FolderRun;
  readonly after: FolderRun;
  /** What the two made, compared; `null` until both are drawn and the compare answered. */
  made = $state.raw<Compared | null>(null);
  /** Why the compare could not be made. */
  compareFailed = $state<string | null>(null);
  readonly #at: () => FolderCompareAt | null;
  readonly #ports: FolderComparePorts;
  #asked: string | null = null;

  constructor(at: () => FolderCompareAt | null, ports: FolderComparePorts = ipcFolderComparePorts) {
    this.#at = at;
    this.#ports = ports;
    const side = (end: (now: FolderCompareAt) => string | null) => () => {
      const now = this.#at();
      const at = now === null ? null : end(now);
      if (now === null || at === null) return null;
      return {
        entityId: now.entityId,
        renderer: now.renderer.id,
        command: now.renderer.command,
        folder: now.folder,
        commit: at,
        paths: null,
        // Never a start on a look: only *Run* runs (ruling 8).
        start: false,
        lane: laneOf(at),
      };
    };
    this.before = new FolderRun(side((now) => now.before), ports);
    this.after = new FolderRun(side((now) => now.after), ports);

    // Both sides drawn (or one with no folder), and neither running: compare them, once per pair of tokens.
    $effect(() => {
      const old = this.#tokenOf(this.sideOf("old"), this.before);
      const nu = this.#tokenOf(this.sideOf("new"), this.after);
      const now = this.#at();
      if (old === undefined || nu === undefined || (old === null && nu === null) || now === null) return;
      const single = now.renderer.output !== "folder";
      const eol = now.after === WORKING || now.before === WORKING;
      const name = `${single ? "s" : "f"}\n${old}\n${nu}\n${eol}`;
      untrack(() => {
        if (this.#asked === name) return;
        this.#asked = name;
        if (this.made !== null && (this.made.old !== old || this.made.new !== nu)) this.made = null;
        this.compareFailed = null;
        void this.#compare(name, single, old, nu, eol);
      });
    });
  }

  async #compare(name: string, single: boolean, old: string | null, nu: string | null, eol: boolean) {
    try {
      let made: Compared;
      if (single) {
        const [a, b] = await Promise.all([old === null ? null : this.#ports.output(old), nu === null ? null : this.#ports.output(nu)]);
        made = { kind: "single", old, new: nu, same: a !== null && b !== null && sameBytes(a, b) };
      } else {
        made = { kind: "folder", old, new: nu, compare: await this.#ports.compare(old, nu, eol) };
      }
      if (this.#asked === name) this.made = made;
    } catch (cause) {
      if (this.#asked !== name) return;
      this.#asked = null;
      // A token the backend no longer holds: each side is asked again with `look`, which reads it back from its store.
      if (typeof cause === "object" && cause !== null && (cause as { code?: unknown }).code === "not_found") {
        this.before.look();
        this.after.look();
        return;
      }
      this.compareFailed = ipcErrorMessage(cause);
    }
  }

  /** The token a side's compare reads: `null` for no folder, `undefined` while it is not drawn or still runs. */
  #tokenOf(state: SideState, run: FolderRun): string | null | undefined {
    if (state === "absent") return null;
    if (state === "kept" || state === "stale") return run.view?.drawn?.token ?? undefined;
    return undefined;
  }

  /** How side `side` stands. */
  sideOf(side: "old" | "new"): SideState {
    const now = this.#at();
    const end = now === null ? null : side === "old" ? now.before : now.after;
    if (now === null) return "asking";
    if (end === null) return "absent";
    const run = side === "old" ? this.before : this.after;
    if (run.refused !== null) return "failed";
    const view = run.view;
    if (view === null) return "asking";
    if (view.running !== null) return "running";
    if (view.failure?.why === "no_folder") return "absent";
    if (view.drawn !== null && view.current) return "kept";
    if (view.failure !== null && view.failure.why !== "stopped") return "failed";
    if (view.drawn !== null) return "stale";
    return "none";
  }

  /** How the compare stands. */
  get state(): CompareState {
    const sides = [this.sideOf("old"), this.sideOf("new")];
    if (sides.includes("running")) return "running";
    if (sides.includes("failed") || this.compareFailed !== null) return "failed";
    if (sides.includes("asking")) return "asking";
    if (sides.every((s) => s === "absent")) return "idle";
    if (sides.includes("none")) return "idle";
    const made = this.made;
    if (made === null) return "comparing";
    if (sides.includes("stale")) return "stale";
    if (made.kind === "single") return made.same ? "same" : "made";
    return made.compare.files.length === 0 ? "same" : "made";
  }

  /** Whether cards stand under the bar (what the two made differs), not the note box. */
  get lists(): boolean {
    const made = this.made;
    if (made === null) return false;
    return made.kind === "folder" ? made.compare.files.length > 0 : !made.same;
  }

  /** Whether anything is kept for either side, current or not: *Run* is primary only while nothing is. */
  get anyKept(): boolean {
    return [this.before, this.after].some((run) => run.view?.drawn != null);
  }

  /** Whether a press of *Run N folder renders* takes it: nothing kept for a side, or a side made from an earlier state. */
  get runnable(): boolean {
    const now = this.state;
    return now === "idle" || now === "stale";
  }

  /** *Run*, *Run again* or *Stop*: stops every side that runs, else runs every side not kept for the folder as it is. */
  press(): void {
    const runs = [
      { state: this.sideOf("old"), run: this.before },
      { state: this.sideOf("new"), run: this.after },
    ];
    if (runs.some((r) => r.state === "running")) {
      for (const r of runs) if (r.state === "running") r.run.press();
      return;
    }
    for (const r of runs) if (r.state !== "kept" && r.state !== "absent" && r.state !== "asking") r.run.press();
  }

  /** *Stop all*: every side that runs stops. */
  stop(): void {
    if (this.sideOf("old") === "running") this.before.press();
    if (this.sideOf("new") === "running") this.after.press();
  }
}

function sameBytes(a: Uint8Array, b: Uint8Array): boolean {
  if (a.length !== b.length) return false;
  for (let i = 0; i < a.length; i++) if (a[i] !== b[i]) return false;
  return true;
}
