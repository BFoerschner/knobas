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
import type { Setting, ViewAction } from "../../ipc/entity";
import type { BuildParameter, MergeStyle, WriteOpPayload } from "../../ipc/sources";

/** Keyed by the union, so a style added to `MergeStyle` and missed here fails to compile. */
const MERGE_STYLES: Record<MergeStyle, true> = { merge: true, rebase: true, squash: true };

function isMergeStyle(option: string | null): option is MergeStyle {
  return option !== null && Object.hasOwn(MERGE_STYLES, option);
}

/**
 * The payload for one op, from the entity the view is of, the option picked
 * or typed, and the values the dialog changed.
 */
const PAYLOADS: Record<
  string,
  (entity: string, option: string | null, parameters: BuildParameter[]) => WriteOpPayload | null
> = {
  merge_pull_request: (entity, option) =>
    isMergeStyle(option) ? { MergePullRequest: { entity, style: option } } : null,
  // A blank branch is the default branch, which is no branch at all: TeamCity
  // reads an explicit default branch's name as another branch (#669).
  run_build: (entity, option, parameters) => ({
    RunBuild: { entity, branch: option?.trim() || null, parameters },
  }),
};

/**
 * How an op's dialog asks for its input. `pick`: one of the options, the
 * first picked. `suggest`: free text, the options offered as suggestions and
 * none picked. `parameters`: the dialog also offers the entity's own settings
 * to change for this one write, and sends only what changed.
 */
export interface ActionForm {
  choice: "pick" | "suggest";
  parameters: boolean;
}

const FORMS: Record<string, ActionForm> = {
  run_build: { choice: "suggest", parameters: true },
};

/** The dialog `action` draws; an op with no entry picks one of its options. */
export function actionForm(action: ViewAction): ActionForm {
  return FORMS[action.write_op] ?? { choice: "pick", parameters: false };
}

/**
 * The write `action` queues against `entity` with `option` picked (or typed)
 * and `parameters` changed, or `null` when this shell cannot build it: an op
 * it has no entry for, or an option the op cannot carry.
 */
export function actionPayload(
  action: ViewAction,
  entity: string,
  option: string | null,
  parameters: BuildParameter[] = [],
): WriteOpPayload | null {
  return PAYLOADS[action.write_op]?.(entity, option, parameters) ?? null;
}

/**
 * The values a dialog sends: those the reader changed from what was drawn,
 * in the order drawn (#669). A secret is drawn empty, so it goes only when
 * the reader typed something; a value the source does not let change
 * (`editable: false`, a URL drawn without its credential) never goes.
 */
export function changedParameters(values: Setting[], drafts: Record<string, string>): BuildParameter[] {
  return values.flatMap((setting) => {
    const draft = drafts[setting.name];
    if (!setting.editable || draft === undefined) return [];
    const drawn = setting.secret ? "" : setting.value;
    return draft === drawn ? [] : [{ name: setting.name, value: draft }];
  });
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
