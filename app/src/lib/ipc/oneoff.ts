/**
 * Commands, run on a press and read in a dialog — `crates/knobas-app/src/commands/oneoff.rs` (ADR-0060). The types are
 * generated from the Rust (`./generated/app.ts`); the `invoke` functions stay hand-written. The window names a saved
 * command by its id and a target, never a program or an argument.
 */
import { Channel } from "@tauri-apps/api/core";

import { invoke } from "./invoke";
import type {
  OneOff,
  OneOffBatch,
  OneOffColour,
  OneOffDraft,
  OneOffEnded,
  OneOffEnv,
  OneOffFinding,
  OneOffKind,
  OneOffLine,
  OneOffOffered,
  OneOffPlace,
  OneOffRepoCommands,
  OneOffResult,
  OneOffResultKind,
  OneOffRun,
  OneOffRunRow,
  OneOffShell,
  OneOffShellPath,
  OneOffShows,
  OneOffStartIn,
  OneOffStarted,
  OneOffStdin,
  OneOffStream,
  OneOffStyle,
  OneOffTarget,
  OneOffUrl,
  TryOn,
} from "./generated/app";

export type {
  OneOff,
  OneOffBatch,
  OneOffColour,
  OneOffDraft,
  OneOffEnded,
  OneOffEnv,
  OneOffFinding,
  OneOffKind,
  OneOffLine,
  OneOffOffered,
  OneOffPlace,
  OneOffRepoCommands,
  OneOffResult,
  OneOffResultKind,
  OneOffRun,
  OneOffRunRow,
  OneOffShell,
  OneOffShellPath,
  OneOffShows,
  OneOffStartIn,
  OneOffStarted,
  OneOffStdin,
  OneOffStream,
  OneOffStyle,
  OneOffTarget,
  OneOffUrl,
};

/** The saved commands, in the person's order. */
export function oneoffCommands(): Promise<OneOff[]> {
  return invoke<OneOff[]>("oneoff_commands");
}

/** Saves the whole list; rejects naming the entry that cannot be saved. */
export function setOneoffCommands(drafts: OneOffDraft[]): Promise<OneOff[]> {
  return invoke<OneOff[]>("set_oneoff_commands", { drafts });
}

/** A channel whose batches go to `onbatch`; `stop` lets go of it. */
export function batchChannel(onbatch: (batch: OneOffBatch) => void): { channel: Channel<OneOffBatch>; stop: () => void } {
  const channel = new Channel<OneOffBatch>();
  channel.onmessage = onbatch;
  // Tauri keeps a channel's callback for the window's life: a dialog that closed lets go of its own.
  return { channel, stop: () => (channel.onmessage = () => {}) };
}

/**
 * Runs saved command `command` on `target` of `entityId`'s checkout; its lines follow on `channel`. A generated folder's
 * target runs in the kept folder `madeToken` names.
 */
export function oneoffRun(entityId: string, command: string, target: OneOffTarget, madeToken: string | null, channel: Channel<OneOffBatch>): Promise<OneOffStarted> {
  return invoke<OneOffStarted>("oneoff_run", { entityId, command, target, madeToken, channel });
}

/**
 * *Try it on* (#2562): saved command `command` on pasted text, a chosen file or a chosen folder, with the chosen folder in
 * the checkout's place; its lines on `channel` as {@link oneoffRun}'s. No row in the list of runs.
 */
export function oneoffTry(command: string, on: TryOn, channel: Channel<OneOffBatch>): Promise<OneOffStarted> {
  return invoke<OneOffStarted>("oneoff_try", { command, on, channel });
}

/** Stops a run with everything it started. */
export function oneoffStop(run: string): Promise<void> {
  return invoke<void>("oneoff_stop", { run });
}

/** Every run, the running ones first; of one repository where `entityId` names it. */
export function oneoffRuns(entityId: string | null): Promise<OneOffRunRow[]> {
  return invoke<OneOffRunRow[]>("oneoff_runs", { entityId });
}

/** A kept run's lines `from` to `from + count`, with its counts; `null` for a run no longer known. */
export function oneoffLines(run: string, from: number, count: number): Promise<OneOffBatch | null> {
  return invoke<OneOffBatch | null>("oneoff_lines", { run, from, count });
}

/** A running run's next batches on `channel` too; `false` for a run no longer known. */
export function oneoffWatch(run: string, channel: Channel<OneOffBatch>): Promise<boolean> {
  return invoke<boolean>("oneoff_watch", { run, channel });
}

/** *Download*: what run `run` printed, as text (with its colour codes where `colours`), or with `result` what it made (`file`, one it wrote to `{outdir}`), saved to Downloads; answers the path. */
export function oneoffSave(run: string, name: string, colours: boolean, result = false, file: string | null = null): Promise<string> {
  return invoke<string>("oneoff_save", { run, name, colours, result, file });
}

/** What `oneoff_result` answers: an `ArrayBuffer` from the bridge, bytes from the dev fake. */
type RawBytes = ArrayBuffer | ArrayLike<number>;

/** The bytes of a run's result, or of `file`, one it wrote to `{outdir}`. */
export async function oneoffResult(run: string, file: string | null): Promise<Uint8Array> {
  const answer = await invoke<RawBytes>("oneoff_result", { run, file });
  return answer instanceof ArrayBuffer ? new Uint8Array(answer) : Uint8Array.from(answer);
}

/** Forgets one run, or every run that ended. */
export function oneoffForget(run: string | null): Promise<void> {
  return invoke<void>("oneoff_forget", { run });
}

/** The login shell's `PATH` this session (read now with `again`), and *Use my shell's PATH* (set with `on`). */
export function oneoffShellPath(again: boolean, on: boolean | null): Promise<OneOffShell> {
  return invoke<OneOffShell>("oneoff_shell_path", { again, on });
}

/** What a repository's `.knobas/commands.toml` offers, read from the checkout now; nothing in it runs. */
export function oneoffRepoCommands(entityId: string): Promise<OneOffRepoCommands> {
  return invoke<OneOffRepoCommands>("oneoff_repo_commands", { entityId });
}

/** Copies the named commands of the repository's file into the person's list, scoped to it; answers the list. */
export function oneoffCopyRepoCommands(entityId: string, names: string[]): Promise<OneOff[]> {
  return invoke<OneOff[]>("oneoff_copy_repo_commands", { entityId, names });
}
