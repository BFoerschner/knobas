/**
 * knobas updates itself (ADR-0052) — `crates/knobas-app/src/commands/update.rs`.
 *
 * The types are generated from the Rust (`./generated/update.ts`): a field is explained in its Rust doc comment and
 * nowhere else. The `invoke` functions stay hand-written, pinned to the Rust by a test in that module which
 * `include_str!`s this file. None needs the database.
 */
import { invoke } from "./invoke";

import type { Channel, UpdateNotes, UpdateState } from "./generated/update";

export type {
  ByHandWhy,
  Channel,
  CheckFailed,
  CheckFailedKind,
  InstallKind,
  LastUpdate,
  Offer,
  Phase,
  Seen,
  Stage,
  Standing,
  Step,
  UpdateNotes,
  UpdateState,
} from "./generated/update";

/** What is known, from memory: no request, no database. */
export function updateState(): Promise<UpdateState> {
  return invoke<UpdateState>("update_state");
}

/** *Check now*. */
export function updateCheck(): Promise<UpdateState> {
  return invoke<UpdateState>("update_check");
}

/** *Check for updates* or *Which versions*; a changed channel checks at once. */
export function updateSet(check: boolean | null, channel: Channel | null): Promise<UpdateState> {
  return invoke<UpdateState>("update_set", { check, channel });
}

/** *Download*: the offered version's artifact, checked against knobas's signature as it lands. */
export function updateDownload(): Promise<UpdateState> {
  return invoke<UpdateState>("update_download");
}

/** *Cancel* a download. */
export function updateCancel(): Promise<UpdateState> {
  return invoke<UpdateState>("update_cancel");
}

/** *Restart to update*: back up, put the new knobas in place and restart through the ordinary quit. */
export function updateInstall(): Promise<UpdateState> {
  return invoke<UpdateState>("update_install");
}

/** *Later*: the dot goes until the next start or the next newer version; the download stays. */
export function updateLater(): Promise<UpdateState> {
  return invoke<UpdateState>("update_later");
}

/** A release's notes and its page. */
export function updateNotes(version: string): Promise<UpdateNotes> {
  return invoke<UpdateNotes>("update_notes", { version });
}
