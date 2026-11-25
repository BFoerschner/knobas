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
  OneOffPlace,
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
  OneOffPlace,
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

/** Runs saved command `command` on `target` of `entityId`'s checkout; its lines follow on `channel`. */
export function oneoffRun(entityId: string, command: string, target: OneOffTarget, channel: Channel<OneOffBatch>): Promise<OneOffStarted> {
  return invoke<OneOffStarted>("oneoff_run", { entityId, command, target, channel });
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

/** *Download*: what run `run` printed, as text (with its colour codes where `colours`), saved to Downloads; answers the path. */
export function oneoffSave(run: string, name: string, colours: boolean): Promise<string> {
  return invoke<string>("oneoff_save", { run, name, colours });
}

/** Forgets one run, or every run that ended. */
export function oneoffForget(run: string | null): Promise<void> {
  return invoke<void>("oneoff_forget", { run });
}

/** The login shell's `PATH` this session (read now with `again`), and *Use my shell's PATH* (set with `on`). */
export function oneoffShellPath(again: boolean, on: boolean | null): Promise<OneOffShell> {
  return invoke<OneOffShell>("oneoff_shell_path", { again, on });
}
