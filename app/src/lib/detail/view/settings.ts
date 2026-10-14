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

/** A settings edit as it sits on the queue. */
export interface QueuedSetting {
  entity: string;
  name: string;
  value: string;
}

/**
 * Each op a settings block can edit through, both ways: `payload` builds the
 * write from the block's entity, the value edited and the value it was edited
 * against (`null` where none was drawn); `readBack` reads a queued write of
 * that op back into the setting it changes (#692), or `null` for a payload of
 * another variant. One entry, so the two cannot drift apart.
 */
const OPS: Record<
  string,
  {
    payload: (entity: string, name: string, value: string, base: string | null) => WriteOpPayload;
    readBack: (payload: WriteOpPayload) => QueuedSetting | null;
  }
> = {
  set_parameter: {
    payload: (entity, name, value, base) => ({ SetParameter: { entity, name, value, base } }),
    readBack: (payload) => {
      if (!("SetParameter" in payload)) return null;
      const { entity, name, value } = payload.SetParameter;
      return { entity, name, value };
    },
  },
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
  return OPS[writeOp]?.payload(entity, name, value, base) ?? null;
}

/**
 * The setting `write` changes, or `null` when it is not an edit of `writeOp`,
 * so a detail opened over a write still on the queue can find its row (#692).
 * The payload's variant is the op (`write.op` names the same thing), so
 * reading the variant is the check.
 */
export function queuedSetting(writeOp: string, write: QueuedWrite): QueuedSetting | null {
  return OPS[writeOp]?.readBack(write.payload) ?? null;
}

/**
 * Whether a block's values may be edited here: its op is one the source's
 * descriptor declares (`submit_write` refuses any other) and this shell can
 * build.
 */
export function settingsWritable(writeOp: string, declared: string[]): boolean {
  return declared.includes(writeOp) && Object.hasOwn(OPS, writeOp);
}

/** The queue states a write is still open in. */
type OpenState = Extract<WriteState, "pending" | "held" | "refused">;

/** The state `write` is still open in, or `null` once it has left the queue. */
export function openState(write: QueuedWrite | undefined): OpenState | null {
  const state = write?.state;
  return state === "pending" || state === "held" || state === "refused" ? state : null;
}

/**
 * The mark a still-open write puts on its row (#775): `saving` while it is
 * pending with nothing in its way -- the background send is under way -- and
 * the queue's own state once it waits for a reason, is held or is refused.
 * `null` once it has left the queue.
 */
export function markState(write: QueuedWrite | undefined): SettingMark["state"] | null {
  const state = openState(write);
  if (write && state === "pending" && !write.wait_reason && write.source_enabled !== false) return "saving";
  return state;
}

/**
 * A row's edit on its way through the queue. `saving` while the write is
 * being queued and while its send runs; then the queue's own open state if it
 * waits, is held or refused, until it settles, when the mark goes and the
 * view is read again.
 */
export interface SettingMark {
  state: "saving" | OpenState;
  /** What the queue or the source said, in its own words. Untrusted text. */
  detail: string | null;
  /** The value the write carries. Never drawn for a secret. */
  value: string;
}
