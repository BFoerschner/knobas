/**
 * What a settings block's edit becomes on the write queue (#668, ADR-0020
 * rule 4).
 *
 * A settings block names the `WriteOp` identifier that changes its values and
 * the entity they belong to; the queue takes a serialized `WriteOp`. This
 * table is the one place the two meet, as `actions.ts` is for an actions
 * block. `SettingsBlock` stays generic: it knows names and values, never
 * which op it edits.
 *
 * A block whose op has no entry here draws no control, whatever the source
 * declares: an edit this shell cannot queue would fail on Enter.
 */
import type { ViewBlock } from "../../ipc/entity";
import type { QueuedWrite, WriteOpPayload, WriteState } from "../../ipc/sources";

/** What a settings block's edit is aimed by: its entity and the op that changes it. */
export type SettingsTarget = Pick<Extract<ViewBlock, { type: "settings" }>, "entity" | "write_op">;

/**
 * The payload for one op, from the block's entity, the value edited and the
 * value it was edited against (`null` where none was drawn).
 */
const PAYLOADS: Record<
  string,
  (entity: string, name: string, value: string, base: string | null) => WriteOpPayload
> = {
  set_parameter: (entity, name, value, base) => ({ SetParameter: { entity, name, value, base } }),
};

/**
 * The write that sets `name` to `value` on `entity`, edited against `base`,
 * or `null` for an op this shell cannot queue.
 */
export function settingPayload(
  writeOp: string,
  entity: string,
  name: string,
  value: string,
  base: string | null,
): WriteOpPayload | null {
  return PAYLOADS[writeOp]?.(entity, name, value, base) ?? null;
}

/**
 * What a queued write sets, read back from its payload: the entity, the
 * setting's name and the value it carries. The inverse of {@link PAYLOADS},
 * by op, so a detail opened over a write still on the queue can find the row
 * that write is for (#692).
 */
const SETS: Record<string, (payload: WriteOpPayload) => QueuedSetting | null> = {
  set_parameter: (payload) => ("SetParameter" in payload ? payload.SetParameter : null),
};

/** A settings edit as it sits on the queue. */
export interface QueuedSetting {
  entity: string;
  name: string;
  value: string;
}

/** The setting `write` changes, or `null` when it is not an edit of `writeOp`. */
export function queuedSetting(writeOp: string, write: QueuedWrite): QueuedSetting | null {
  if (write.op !== writeOp) return null;
  const read = SETS[writeOp]?.(write.payload);
  return read ? { entity: read.entity, name: read.name, value: read.value } : null;
}

/**
 * Whether a block's values may be edited here: its op is one the source's
 * descriptor declares (`submit_write` refuses any other) and this shell can
 * build.
 */
export function settingsWritable(writeOp: string, declared: string[]): boolean {
  return declared.includes(writeOp) && Object.hasOwn(PAYLOADS, writeOp);
}

/** The queue states a write is still open in. */
type OpenState = Extract<WriteState, "pending" | "held" | "refused">;

/** The state `write` is still open in, or `null` once it has left the queue. */
export function openState(write: QueuedWrite | undefined): OpenState | null {
  const state = write?.state;
  return state === "pending" || state === "held" || state === "refused" ? state : null;
}

/**
 * A row's edit on its way through the queue. `saving` while the write is
 * queued and its first flush runs; then the queue's own open state until it
 * settles, when the mark goes and the view is read again.
 */
export interface SettingMark {
  state: "saving" | OpenState;
  /** What the queue or the source said, in its own words. Untrusted text. */
  detail: string | null;
  /** The value the write carries. Never drawn for a secret. */
  value: string;
}
