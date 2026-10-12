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
import type { WriteOpPayload, WriteState } from "../../ipc/sources";

/** The payload for one op, from the block's entity and the value edited. */
const PAYLOADS: Record<string, (entity: string, name: string, value: string) => WriteOpPayload> = {
  set_parameter: (entity, name, value) => ({ SetParameter: { entity, name, value } }),
};

/** The write that sets `name` to `value` on `entity`, or `null` for an op this shell cannot queue. */
export function settingPayload(
  writeOp: string,
  entity: string,
  name: string,
  value: string,
): WriteOpPayload | null {
  return PAYLOADS[writeOp]?.(entity, name, value) ?? null;
}

/**
 * Whether a block's values may be edited here: its op is one the source's
 * descriptor declares (`submit_write` refuses any other) and this shell can
 * build.
 */
export function settingsWritable(writeOp: string, declared: string[]): boolean {
  return declared.includes(writeOp) && Object.hasOwn(PAYLOADS, writeOp);
}

/**
 * A row's edit on its way through the queue. `saving` while the write is
 * queued and its first flush runs; then the queue's own open state until it
 * settles, when the mark goes and the view is read again.
 */
export interface SettingMark {
  state: "saving" | Extract<WriteState, "pending" | "held" | "refused">;
  /** What the queue or the source said, in its own words. Untrusted text. */
  detail: string | null;
  /** The value the write carries. Never drawn for a secret. */
  value: string;
}
