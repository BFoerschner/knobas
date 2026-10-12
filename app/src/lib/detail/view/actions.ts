/**
 * What a view's action becomes on the write queue (#667, ADR-0020 rule 4).
 *
 * An actions block names a `WriteOp` identifier and the choices its dialog
 * offers; the queue takes a serialized `WriteOp`. This table is the one place
 * the two meet, one entry per op, as `inbox/actions.ts` is for the inbox's
 * buttons. `ActionsBlock` stays generic: it knows labels, options and a
 * sentence, and never which op it is drawing.
 *
 * An op with no entry here is not drawn, whatever the source declares: a
 * button this shell cannot queue would fail on press.
 */
import type { ViewAction } from "../../ipc/entity";
import type { MergeStyle, WriteOpPayload } from "../../ipc/sources";

const MERGE_STYLES: readonly string[] = ["merge", "rebase", "squash"] satisfies MergeStyle[];

function isMergeStyle(option: string | null): option is MergeStyle {
  return option !== null && MERGE_STYLES.includes(option);
}

/** The payload for one op, from the entity the view is of and the option picked. */
const PAYLOADS: Record<string, (entity: string, option: string | null) => WriteOpPayload | null> = {
  merge_pull_request: (entity, option) =>
    isMergeStyle(option) ? { MergePullRequest: { entity, style: option } } : null,
};

/**
 * The write `action` queues against `entity` with `option` picked, or `null`
 * when this shell cannot build it: an op it has no entry for, or an option
 * the op cannot carry.
 */
export function actionPayload(
  action: ViewAction,
  entity: string,
  option: string | null,
): WriteOpPayload | null {
  return PAYLOADS[action.write_op]?.(entity, option) ?? null;
}

/**
 * The actions to draw: those whose op the source's descriptor declares
 * (`submit_write` refuses any other) and this shell can queue.
 */
export function offeredActions(actions: ViewAction[], declared: string[]): ViewAction[] {
  return actions.filter((action) => declared.includes(action.write_op) && action.write_op in PAYLOADS);
}

/** The label without the ellipsis that says a dialog follows: the confirm button's words. */
export function actionName(label: string): string {
  return label.replace(/(…|\.\.\.)$/, "").trim();
}
