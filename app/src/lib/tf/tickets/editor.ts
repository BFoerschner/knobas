/**
 * The view editor's rules (#814, R-3.13 to R-3.16), pure: a view's text read
 * into conditions and written back, the search line under them, the board's
 * columns as the editor changes them, and what the preview counts.
 *
 * The stored text is the truth (R-3.3). The editor reads it into conditions
 * when it opens and writes text from them on save. Every key of #811 and the
 * two built keys the editor offers (`under:` as Epic, `updated:`) is a
 * condition; any other token (`is:mine`, `/jira`, a quoted phrase) stays in
 * the words as typed, so nothing a reader typed is lost.
 */
import type { MiniBoardColumn } from "../../ipc/entity";
import type { ViewColumn } from "../../ipc/search";
import { grammarToken } from "../filter/filter";

/** A field a condition can test (R-3.14). */
export type ConditionField =
  | "project"
  | "status"
  | "issuetype"
  | "priority"
  | "assignee"
  | "epic"
  | "label"
  | "sprint"
  | "signal"
  | "updated"
  | "assigned";

/** The fields in prototype-6's order and words, each with the grammar key it writes. */
export const CONDITION_FIELDS: readonly { field: ConditionField; label: string; key: string }[] = [
  { field: "project", label: "Project", key: "project" },
  { field: "status", label: "Status", key: "status" },
  { field: "issuetype", label: "Issue type", key: "issuetype" },
  { field: "priority", label: "Priority", key: "priority" },
  { field: "assignee", label: "Assignee", key: "assignee" },
  { field: "epic", label: "Epic", key: "under" },
  { field: "label", label: "Label", key: "label" },
  { field: "sprint", label: "Sprint", key: "sprint" },
  { field: "signal", label: "Signal", key: "has" },
  { field: "updated", label: "Updated", key: "updated" },
  { field: "assigned", label: "Assigned to me", key: "assigned" },
];

/** One condition: a field, *is* or *is not*, and values any of which it means. */
export interface Condition {
  field: ConditionField;
  negated: boolean;
  values: string[];
}

/** *Which tickets*: every condition has to hold, then the words in the key or summary. */
export interface Conditions {
  conditions: Condition[];
  /** The rest of the text, as typed. */
  words: string;
}

const BY_KEY = new Map(CONDITION_FIELDS.map((entry) => [entry.key, entry]));
const KEY_OF = new Map(CONDITION_FIELDS.map((entry) => [entry.field, entry.key]));

/** The grammar key a field writes. */
export function keyOf(field: ConditionField): string {
  return KEY_OF.get(field)!;
}

/** Split on whitespace, a double-quoted run kept whole with its quotes, as the grammar's tokenizer does. */
function tokenize(text: string): string[] {
  const tokens: string[] = [];
  let current = "";
  let quoted = false;
  for (const ch of text) {
    if (ch === '"') {
      quoted = !quoted;
      current += ch;
    } else if (/\s/.test(ch) && !quoted) {
      if (current !== "") tokens.push(current);
      current = "";
    } else current += ch;
  }
  if (current !== "") tokens.push(current);
  return tokens;
}

function unquote(value: string): string {
  return value.length >= 2 && value.startsWith('"') && value.endsWith('"') ? value.slice(1, -1) : value;
}

/**
 * A view's text as conditions (R-3.3): each `key:value` of a field, `-` for
 * *is not*, values of one key and sign joined as the grammar joins them, in
 * the order first typed; the other tokens as words.
 */
export function parseConditions(text: string): Conditions {
  const conditions: Condition[] = [];
  const words: string[] = [];
  for (const token of tokenize(text)) {
    const negated = token.startsWith("-");
    const body = negated ? token.slice(1) : token;
    const colon = body.indexOf(":");
    const entry = colon > 0 ? BY_KEY.get(body.slice(0, colon).toLowerCase()) : undefined;
    const value = colon > 0 ? unquote(body.slice(colon + 1).trim()).trim() : "";
    if (entry === undefined || value === "") {
      words.push(token);
      continue;
    }
    let held = conditions.find((c) => c.field === entry.field && c.negated === negated);
    if (held === undefined) {
      held = { field: entry.field, negated, values: [] };
      conditions.push(held);
    }
    if (!held.values.includes(value)) held.values.push(value);
  }
  return { conditions, words: words.join(" ") };
}

/** One token of the search line, and whether the grammar reads it. */
export interface SearchPart {
  text: string;
  parses: boolean;
}

const SIGNALS = ["failing-build", "blocker", "open-pr", "review-for-me"];

/** `updated:` and `assigned:` windows as the grammar reads them (`duration_days`). */
function isWindow(value: string): boolean {
  return /^(today|yesterday|week|\d+[dwm])$/i.test(value);
}

/**
 * Whether the grammar reads a condition's token (R-3.14's dotted underline):
 * `under:` and `updated:` take no `-`, `sprint:` reads `current` only, `has:`
 * its four signals, `assigned:` and `updated:` a window.
 */
function parses(field: ConditionField, negated: boolean, value: string): boolean {
  switch (field) {
    case "epic":
      return !negated;
    case "updated":
      return !negated && isWindow(value);
    case "sprint":
      return value.toLowerCase() === "current";
    case "signal":
      return SIGNALS.includes(value.toLowerCase());
    case "assigned":
      return isWindow(value);
    default:
      return true;
  }
}

/** The conditions as the search line draws them: each value one token, then the words. */
export function searchParts(read: Conditions): SearchPart[] {
  const parts: SearchPart[] = read.conditions.flatMap((condition) =>
    condition.values.map((value) => ({
      text: `${condition.negated ? "-" : ""}${grammarToken(keyOf(condition.field), value)}`,
      parses: parses(condition.field, condition.negated, value),
    })),
  );
  if (read.words.trim() !== "") parts.push({ text: read.words.trim(), parses: true });
  return parts;
}

/** The conditions as the text a view stores: the conditions in order, then the words. */
export function conditionsText(read: Conditions): string {
  return searchParts(read)
    .map((part) => part.text)
    .join(" ");
}

// ---- the board's columns (R-3.15) ----

/** A column as the editor holds it: a blank *At most* is `null`. */
export type EditColumn = ViewColumn;

/** One column per status, named by it, in the board's order. */
export function onePerStatus(statuses: string[]): EditColumn[] {
  return statuses.map((status) => ({ name: status, statuses: [status], limit: null }));
}

/** The columns a view opens the editor with: its own, or one per status. */
export function editColumns(own: ViewColumn[], statuses: string[]): EditColumn[] {
  if (own.length === 0) return onePerStatus(statuses);
  return own.map((column) => ({ ...column, statuses: [...column.statuses] }));
}

/** Put a status on column `at`, off every other; on the one it is on, off it. */
export function toggleStatus(columns: EditColumn[], at: number, status: string): EditColumn[] {
  const had = columns[at]?.statuses.includes(status) ?? false;
  return columns.map((column, index) => {
    const statuses = column.statuses.filter((s) => s !== status);
    if (index === at && !had) statuses.push(status);
    return { ...column, statuses };
  });
}

/** Move column `at` one place earlier (`-1`) or later (`1`). */
export function moveColumn(columns: EditColumn[], at: number, by: 1 | -1): EditColumn[] {
  const to = at + by;
  if (to < 0 || to >= columns.length) return columns;
  const next = columns.slice();
  const [moved] = next.splice(at, 1);
  next.splice(to, 0, moved!);
  return next;
}

/** The statuses on no column, in the board's order. */
export function looseStatuses(columns: EditColumn[], statuses: string[]): string[] {
  const placed = new Set(columns.flatMap((column) => column.statuses));
  return statuses.filter((status) => !placed.has(status));
}

/**
 * The columns a view stores: none of its own when they are one per status
 * in the board's order (so a status that turns up later gets its column), a
 * blank or zero *At most* as none.
 */
export function storedColumns(columns: EditColumn[], statuses: string[]): ViewColumn[] {
  const plain = onePerStatus(statuses);
  const same =
    columns.length === plain.length &&
    columns.every(
      (column, index) =>
        column.name === plain[index]!.name &&
        column.limit === null &&
        column.statuses.length === 1 &&
        column.statuses[0] === plain[index]!.statuses[0],
    );
  if (same) return [];
  return columns.map((column) => ({
    name: column.name,
    statuses: [...column.statuses],
    limit: column.limit !== null && column.limit > 0 ? column.limit : null,
  }));
}

// ---- the preview (R-3.13) ----

/** One column of the preview's board. */
export interface PreviewColumn {
  name: string;
  total: number;
  limit: number | null;
  /** More tickets than its *At most*: the count in err tone. */
  over: boolean;
  /** Its newest cards, at most `cap`. */
  cards: MiniBoardColumn["cards"];
}

/**
 * The preview's ticket board: how many tickets the view holds now, each
 * column gathering its statuses' tickets, and how many sit in a status on no
 * column (which do not show, as on a Jira board).
 */
export function previewBoard(
  board: MiniBoardColumn[],
  columns: EditColumn[],
  cap = 4,
): { count: number; columns: PreviewColumn[]; hidden: number } {
  const count = board.reduce((all, column) => all + column.total, 0);
  const placed = new Set(columns.flatMap((column) => column.statuses));
  const hidden = board
    .filter((column) => column.status === null || !placed.has(column.status))
    .reduce((all, column) => all + column.total, 0);
  return {
    count,
    hidden,
    columns: columns.map((column) => {
      const held = board.filter((status) => status.status !== null && column.statuses.includes(status.status));
      const total = held.reduce((all, status) => all + status.total, 0);
      const cards = held
        .flatMap((status) => status.cards)
        .sort((a, b) => (b.updated_at ?? "").localeCompare(a.updated_at ?? ""))
        .slice(0, cap);
      return { name: column.name, total, limit: column.limit, over: column.limit !== null && total > column.limit, cards };
    }),
  };
}
