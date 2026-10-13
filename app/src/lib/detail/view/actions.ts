/**
 * What a view's action becomes on the write queue (#667, #702, ADR-0020 rule 4).
 *
 * The source declares an action's dialog -- its fields, sentence and tone --
 * and the shell draws it without knowing which op it is (`ActionDialog`).
 * What the reader answers comes back as text by field name, and this table
 * is the one place it meets a serialized `WriteOp`: one entry per op, as
 * `inbox/actions.ts` is for the inbox's buttons.
 *
 * An op with no entry here is not drawn, whatever the source declares: a
 * button this shell cannot queue would fail on press.
 */
import type { ViewAction, Setting } from "../../ipc/entity";
import type { BuildParameter, MergeStyle, ReviewVerdict, WriteOpPayload } from "../../ipc/sources";

/** What a dialog answers: each field's value, by its `name`. A flag is `"true"` or `"false"`. */
export type ActionValues = Record<string, string>;

/** Keyed by the union, so a style added to `MergeStyle` and missed here fails to compile. */
const MERGE_STYLES: Record<MergeStyle, true> = { merge: true, rebase: true, squash: true };

function isMergeStyle(option: string | undefined): option is MergeStyle {
  return option !== undefined && Object.hasOwn(MERGE_STYLES, option);
}

/** Keyed by the union, as `MERGE_STYLES` is. */
const REVIEW_VERDICTS: Record<ReviewVerdict, true> = { approve: true, request_changes: true, comment: true };

function isReviewVerdict(word: string | undefined): word is ReviewVerdict {
  return word !== undefined && Object.hasOwn(REVIEW_VERDICTS, word);
}

/**
 * The payload for one op, from the entity the view is of, the dialog's
 * values, and the entity's settings the reader changed for this one write
 * (an action with `with_settings`). `null` where the values cannot make one.
 */
const PAYLOADS: Record<
  string,
  (entity: string, values: ActionValues, settings: BuildParameter[]) => WriteOpPayload | null
> = {
  merge_pull_request: (entity, values) =>
    isMergeStyle(values.style) ? { MergePullRequest: { entity, style: values.style } } : null,
  // A blank branch is the default branch, which is no branch at all: TeamCity
  // reads an explicit default branch's name as another branch (#669).
  run_build: (entity, values, settings) => ({
    RunBuild: { entity, branch: values.branch?.trim() || null, parameters: settings },
  }),
  // A reply on a pull request's discussion (#705). The dialog requires the
  // words; a blank body is no reply, whoever builds it.
  comment: (entity, values) => (values.body?.trim() ? { Comment: { entity, body: values.body } } : null),
  // A review (#705). The verdict is the action's fixed value -- *Approve…*
  // and *Request changes…* are one op -- and never read off the label. No
  // line comments yet: #709 adds them from the *Files* tab.
  submit_review: (entity, values) =>
    isReviewVerdict(values.verdict)
      ? { SubmitReview: { entity, verdict: values.verdict, body: values.body ?? "", comments: [] } }
      : null,
};

/**
 * The write `action` queues against `entity` with its fixed values and the
 * dialog's `values`, and the `settings` changed, or `null` when this shell
 * cannot build it: an op it has no entry for, or values the op cannot carry.
 */
export function actionPayload(
  action: ViewAction,
  entity: string,
  values: ActionValues,
  settings: BuildParameter[] = [],
): WriteOpPayload | null {
  // The source's fixed answers lie under the dialog's (#705); a source never
  // fixes a name it also asks for, so the order only matters to a bad one.
  const answered = { ...action.values, ...values };
  return PAYLOADS[action.write_op]?.(entity, answered, settings) ?? null;
}

/**
 * What the dialog's fields start with, as declared: a text its prefill, a
 * choice the option it names (the first when it names none, or one it does
 * not offer), a flag checked only for `"true"`.
 */
export function initialValues(action: ViewAction): ActionValues {
  return Object.fromEntries(
    action.fields.map((field) => {
      const kind = field.kind;
      if (kind.type === "flag") return [field.name, field.value === "true" ? "true" : "false"];
      if (kind.type === "choice") {
        const named = kind.options.find((option) => option.value === field.value);
        return [field.name, (named ?? kind.options[0])?.value ?? ""];
      }
      return [field.name, field.value ?? ""];
    }),
  );
}

/**
 * The names of the required fields `values` leaves unanswered, in the order
 * drawn: a text of blanks, a choice with nothing picked, a flag unchecked.
 */
export function missingFields(action: ViewAction, values: ActionValues): string[] {
  return action.fields
    .filter((field) => {
      if (!field.required) return false;
      const value = values[field.name] ?? "";
      return field.kind.type === "flag" ? value !== "true" : value.trim() === "";
    })
    .map((field) => field.name);
}

/**
 * Whether pressing `action` opens its dialog. Only an action with nothing to
 * ask, no sentence and no danger queues on the button: a danger always asks.
 */
export function opensDialog(action: ViewAction): boolean {
  return action.fields.length > 0 || action.confirm !== null || action.danger || action.with_settings;
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
