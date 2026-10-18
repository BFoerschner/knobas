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
import type {
  BuildParameter,
  MergeStyle,
  PullRequestState,
  ReviewComment,
  ReviewVerdict,
  UpdateStyle,
  WriteOpPayload,
} from "../../ipc/sources";

/** What a dialog answers: each field's value, by its `name`. A flag is `"true"` or `"false"`. */
export type ActionValues = Record<string, string>;

/** Keyed by the union, so a style added to `MergeStyle` and missed here fails to compile. */
const MERGE_STYLES: Record<MergeStyle, true> = { merge: true, rebase: true, squash: true };

function isMergeStyle(option: string | undefined): option is MergeStyle {
  return option !== undefined && Object.hasOwn(MERGE_STYLES, option);
}

/** Keyed by the union, as `MERGE_STYLES` is. */
const UPDATE_STYLES: Record<UpdateStyle, true> = { merge: true, rebase: true };

function isUpdateStyle(option: string | undefined): option is UpdateStyle {
  return option !== undefined && Object.hasOwn(UPDATE_STYLES, option);
}

/** Keyed by the union, as `MERGE_STYLES` is. */
const REVIEW_VERDICTS: Record<ReviewVerdict, true> = { approve: true, request_changes: true, comment: true };

function isReviewVerdict(word: string | undefined): word is ReviewVerdict {
  return word !== undefined && Object.hasOwn(REVIEW_VERDICTS, word);
}

/** A text value with its blanks trimmed, or `null` when nothing is left. */
function filled(value: string | undefined): string | null {
  const trimmed = value?.trim() ?? "";
  return trimmed === "" ? null : trimmed;
}

// -- edit_pull_request (#706) -------------------------------------------------

/** Keyed by the union, as `MERGE_STYLES` is. */
const PULL_REQUEST_STATES: Record<PullRequestState, true> = { open: true, closed: true };

function isPullRequestState(word: string | undefined): word is PullRequestState {
  return word !== undefined && Object.hasOwn(PULL_REQUEST_STATES, word);
}

/**
 * The answer to `name` if the reader changed it from what was drawn, else
 * `null`. Line ends are compared as `\n`: a text area gives back `\n` for the
 * `\r\n` Gitea may hold, and that is not a change worth overwriting
 * somebody else's edit with (the #706 ruling).
 */
function changedText(name: string, values: ActionValues, drawn: ActionValues): string | null {
  const value = values[name];
  if (value === undefined) return null;
  return sameText(value, drawn[name] ?? "") ? null : value;
}

/** Two answers that differ at most in their line ends (`\r\n` against `\n`). */
function sameText(a: string, b: string): boolean {
  const lines = (text: string) => text.replace(/\r\n/g, "\n");
  return lines(a) === lines(b);
}

/**
 * The ops whose payload sends only what the reader changed, so a dialog sent
 * back as drawn has nothing to write. Every other op's `null` is a payload
 * that cannot be built, and stays an error even when the dialog is untouched
 * (an *Approve…* with an unknown verdict, a *Merge…* with a style this build
 * does not know).
 */
const SENDS_ONLY_CHANGES: ReadonlySet<string> = new Set(["edit_pull_request"]);

/**
 * Whether a dialog with fields came back exactly as it was drawn for an op
 * that sends only changes (#706): an *Edit…* pressed without a change, which
 * builds no write and is no fault.
 */
export function answeredAsDrawn(action: ViewAction, values: ActionValues): boolean {
  if (!SENDS_ONLY_CHANGES.has(action.write_op)) return false;
  const drawn = initialValues(action);
  return (
    action.fields.length > 0 && action.fields.every((f) => sameText(values[f.name] ?? "", drawn[f.name] ?? ""))
  );
}

/**
 * An edit, a close or a reopen (#706): only the fields the reader changed
 * from their declared prefill, so a new title never sends back a description
 * somebody else edited since the view was read; the state is the action's
 * fixed value. `null` for an edit that changes nothing, a blanked title, or
 * a state the SPI cannot carry.
 */
function editPullRequest(entity: string, values: ActionValues, drawn: ActionValues): WriteOpPayload | null {
  const title = changedText("title", values, drawn);
  const body = changedText("body", values, drawn);
  const word = values.state;
  if (word !== undefined && !isPullRequestState(word)) return null;
  const state = word ?? null;
  if (title !== null && title.trim() === "") return null;
  if (title === null && body === null && state === null) return null;
  return { EditPullRequest: { entity, title, body, state } };
}

/**
 * The payload for one op, from the entity the write targets, the dialog's
 * values, the entity's settings the reader changed for this one write (an
 * action with `with_settings`), what each field was drawn with (its
 * declared prefill, `initialValues`) for an op that sends only what changed,
 * and the reader's pending line comments (an action that sends them, #709).
 * `null` where the values cannot make one.
 */
const PAYLOADS: Record<
  string,
  (
    entity: string,
    values: ActionValues,
    settings: BuildParameter[],
    drawn: ActionValues,
    comments: ReviewComment[],
  ) => WriteOpPayload | null
> = {
  // *Delete branch after merge* is a flag (#708); a Merge… drawn without it
  // keeps the branch, as every merge did before. A squash's message (#843)
  // goes only with a squash; a blank one, or any other style, keeps the
  // source's own.
  merge_pull_request: (entity, values) => {
    if (!isMergeStyle(values.style)) return null;
    const message = values.style === "squash" ? filled(values.message) : null;
    return {
      MergePullRequest: {
        entity,
        style: values.style,
        delete_branch: values.delete_branch === "true",
        ...(message === null ? {} : { message }),
      },
    };
  },
  // *Update branch…* (#708): the base merged in, or the head rebased onto it.
  update_pull_request_branch: (entity, values) =>
    isUpdateStyle(values.style) ? { UpdatePullRequestBranch: { entity, style: values.style } } : null,
  // A blank branch is the default branch, which is no branch at all: TeamCity
  // reads an explicit default branch's name as another branch (#669).
  run_build: (entity, values, settings) => ({
    RunBuild: { entity, branch: values.branch?.trim() || null, parameters: settings },
  }),
  // A reply on a pull request's discussion (#705). The dialog requires the
  // words; a blank body is no reply, whoever builds it.
  comment: (entity, values) => (values.body?.trim() ? { Comment: { entity, body: values.body } } : null),
  // A review (#705). The verdict is the action's fixed value -- *Approve…*
  // and *Request changes…* are one op -- and never read off the label. The
  // line comments are the reader's pending ones from the *Files* tab (#709).
  submit_review: (entity, values, _settings, _drawn, comments) =>
    isReviewVerdict(values.verdict)
      ? { SubmitReview: { entity, verdict: values.verdict, body: values.body ?? "", comments } }
      : null,
  // A branch's life (#707). The two creates target the repository, which a
  // branch's or a commit's view names as the action's own `entity`; the
  // refs come from the dialog, a one-option pick where the view fixes them.
  create_branch: (entity, values) => {
    const name = filled(values.name);
    const from = filled(values.from_ref);
    return name && from ? { CreateBranch: { entity, name, from_ref: from } } : null;
  },
  create_pull_request: (entity, values) => {
    const title = filled(values.title);
    const head = filled(values.head);
    const base = filled(values.base);
    return title && head && base
      ? { CreatePullRequest: { entity, title, body: values.body ?? "", head, base } }
      : null;
  },
  delete_branch: (entity) => ({ DeleteBranch: { entity } }),
  // Edit…, Close and Reopen (#706): one op, the state a fixed value.
  edit_pull_request: (entity, values, _settings, drawn) => editPullRequest(entity, values, drawn),
  // A write outside every role's vocabulary (#1194, ADR-0029): the id is the
  // action's fixed value, every other answer goes as a field, and nothing
  // here knows what any of them mean.
  source_action: (entity, values) => {
    const { id, ...fields } = values;
    return id ? { SourceAction: { entity, id, fields } } : null;
  },
};

/**
 * The write `action` queues with its fixed values and the dialog's `values`,
 * and the `settings` changed, or `null` when this shell cannot build it: an
 * op it has no entry for, or values the op cannot carry. It targets the
 * entity the action names where it names one (#707), else `entity`, the one
 * the view is of; the shell never derives one id from another.
 *
 * The `pending` line comments go only with an action that says it sends
 * them (#709); one that is only its line comments (`requires`) with none
 * pending is `null`.
 */
export function actionPayload(
  action: ViewAction,
  entity: string,
  values: ActionValues,
  settings: BuildParameter[] = [],
  pending: ReviewComment[] = [],
): WriteOpPayload | null {
  if (action.line_comments === "requires" && pending.length === 0) return null;
  const comments = sendsComments(action) ? pending : [];
  // The source's fixed answers lie under the dialog's (#705); a source never
  // fixes a name it also asks for, so the order only matters to a bad one.
  const answered = { ...action.values, ...values };
  return (
    PAYLOADS[action.write_op]?.(action.entity ?? entity, answered, settings, initialValues(action), comments) ?? null
  );
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

/** Whether `action` sends the reader's pending line comments with it (#709). */
export function sendsComments(action: ViewAction): boolean {
  return action.line_comments !== "none";
}

/**
 * Whether this shell can queue `op` for a source declaring `declared`: the
 * source declares it (`submit_write` refuses any other) and the payload table
 * has an entry for it.
 */
export function queueable(op: string, declared: string[]): boolean {
  return declared.includes(op) && op in PAYLOADS;
}

/**
 * The actions to draw: those this shell can queue ({@link queueable}), less
 * any that is only the reader's line comments (`requires`, #709) while
 * `pending` is none.
 */
export function offeredActions(actions: ViewAction[], declared: string[], pending = 0): ViewAction[] {
  return actions.filter(
    (action) => queueable(action.write_op, declared) && (action.line_comments !== "requires" || pending > 0),
  );
}

/** The label without the ellipsis that says a dialog follows: the confirm button's words. */
export function actionName(label: string): string {
  return label.replace(/(…|\.\.\.)$/, "").trim();
}
