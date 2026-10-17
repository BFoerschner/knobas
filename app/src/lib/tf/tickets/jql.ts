/**
 * *Make an editable copy* (#880, R-3.25), pure: a Jira view's JQL read into
 * the conditions knobas can test, and the parts it leaves out, each with the
 * reason in prototype-6's words ("`component = on-call` (knobas does not sync
 * components)").
 *
 * A part is one clause the query joins with a top-level `AND`; `ORDER BY` is
 * not one. A clause is kept when it is one field, `=`, `!=`, `in` or `not in`
 * (or `updated >=` a recent window) over values the search grammar reads.
 * Anything else stays out whole, never half-translated: an `OR`, a `NOT`,
 * parentheses, a Jira function other than `currentUser()` and
 * `openSprints()`, a field knobas does not sync.
 */
import type { JiraSource } from "../../ipc/search";
import { conditionsText, type Condition, type ConditionField } from "./editor";

/** One part of the query knobas left out, as Jira spells it, and why. */
export interface LeftOut {
  clause: string;
  reason: string;
}

/** What a copy starts from: its conditions as search text, how many parts it kept, and the rest. */
export interface JiraCopy {
  query: string;
  kept: number;
  left: LeftOut[];
}

interface Token {
  kind: "word" | "string" | "op" | "open" | "close" | "comma";
  text: string;
  start: number;
  end: number;
}

interface Clause {
  text: string;
  tokens: Token[];
}

function tokenize(jql: string): Token[] {
  const tokens: Token[] = [];
  let at = 0;
  while (at < jql.length) {
    const ch = jql[at]!;
    if (/\s/.test(ch)) {
      at++;
      continue;
    }
    const start = at;
    if (ch === '"' || ch === "'") {
      at++;
      while (at < jql.length && jql[at] !== ch) at += jql[at] === "\\" ? 2 : 1;
      at = Math.min(at + 1, jql.length);
      tokens.push({ kind: "string", text: jql.slice(start + 1, at - 1), start, end: at });
    } else if (ch === "(" || ch === ")" || ch === ",") {
      at++;
      tokens.push({ kind: ch === "(" ? "open" : ch === ")" ? "close" : "comma", text: ch, start, end: at });
    } else if ("=!~<>".includes(ch)) {
      at += jql[at + 1] === "=" || jql[at + 1] === "~" ? 2 : 1;
      tokens.push({ kind: "op", text: jql.slice(start, at), start, end: at });
    } else {
      while (at < jql.length && !/[\s(),=!~<>"']/.test(jql[at]!)) at++;
      tokens.push({ kind: "word", text: jql.slice(start, at), start, end: at });
    }
  }
  return tokens;
}

const isWord = (token: Token | undefined, word: string) =>
  token?.kind === "word" && token.text.toLowerCase() === word;

/** The query's parts: split on each `AND` outside parentheses, up to `ORDER BY`. */
function clausesOf(jql: string): Clause[] {
  const tokens = tokenize(jql);
  const clauses: Clause[] = [];
  let current: Token[] = [];
  let depth = 0;
  const close = () => {
    if (current.length > 0) {
      clauses.push({ text: jql.slice(current[0]!.start, current.at(-1)!.end), tokens: current });
    }
    current = [];
  };
  for (let i = 0; i < tokens.length; i++) {
    const token = tokens[i]!;
    if (depth === 0 && isWord(token, "order") && isWord(tokens[i + 1], "by")) break;
    if (depth === 0 && isWord(token, "and")) {
      close();
      continue;
    }
    if (token.kind === "open") depth++;
    if (token.kind === "close") depth = Math.max(0, depth - 1);
    current.push(token);
  }
  close();
  return clauses;
}

/** Fields knobas does not sync, by Jira's name in lower case. */
const UNSYNCED: Record<string, string> = {
  component: "knobas does not sync components",
  components: "knobas does not sync components",
  fixversion: "knobas does not sync fix versions",
  affectedversion: "knobas does not sync affected versions",
  resolution: "knobas does not sync resolutions",
  statuscategory: "knobas does not sync status categories",
  issuefunction: "a ScriptRunner function, only Jira can answer it",
};

/** The Jira fields a condition tests, by Jira's name in lower case. */
const FIELDS: Record<string, ConditionField> = {
  project: "project",
  status: "status",
  issuetype: "issuetype",
  type: "issuetype",
  priority: "priority",
  assignee: "assignee",
  labels: "label",
  label: "label",
  sprint: "sprint",
  parent: "epic",
  "epic link": "epic",
  updated: "updated",
};

const CANNOT_READ = "knobas cannot read this part of the query";

/** The first field name in a clause knobas does not sync, as its reason, or `null`. */
function unsyncedIn(tokens: Token[]): string | null {
  for (const token of tokens) {
    const reason = token.kind === "word" || token.kind === "string" ? UNSYNCED[token.text.toLowerCase()] : undefined;
    if (reason !== undefined) return reason;
  }
  return null;
}

/** One value of a clause: a literal, or a function's name. */
interface Value {
  text: string;
  fn: boolean;
}

/** The values after the operator: one, or a parenthesised list; `null` if they are something else. */
function valuesOf(tokens: Token[]): Value[] | null {
  const one = (at: number): [Value, number] | null => {
    const token = tokens[at];
    if (token === undefined || (token.kind !== "word" && token.kind !== "string")) return null;
    if (token.kind === "word" && tokens[at + 1]?.kind === "open") {
      let depth = 0;
      for (let i = at + 1; i < tokens.length; i++) {
        if (tokens[i]!.kind === "open") depth++;
        if (tokens[i]!.kind === "close" && --depth === 0) return [{ text: token.text.toLowerCase(), fn: true }, i + 1];
      }
      return null;
    }
    return [{ text: token.text, fn: false }, at + 1];
  };
  if (tokens[0]?.kind !== "open") {
    const read = one(0);
    return read !== null && read[1] === tokens.length ? [read[0]] : null;
  }
  const values: Value[] = [];
  let at = 1;
  for (;;) {
    const read = one(at);
    if (read === null) return null;
    values.push(read[0]);
    at = read[1];
    if (tokens[at]?.kind === "comma") at++;
    else if (tokens[at]?.kind === "close" && at === tokens.length - 1) return values;
    else return null;
  }
}

/** One clause as a condition, or the reason it is left out. */
function readClause(tokens: Token[]): Condition | string {
  let depth = 0;
  let or = false;
  for (const token of tokens) {
    if (token.kind === "open") depth++;
    else if (token.kind === "close") depth--;
    else if (depth === 0 && isWord(token, "or")) or = true;
  }
  if (or) return unsyncedIn(tokens) ?? "an OR between conditions, which knobas cannot say";
  const head = tokens[0];
  if (head === undefined || (head.kind !== "word" && head.kind !== "string") || isWord(head, "not")) {
    return unsyncedIn(tokens) ?? CANNOT_READ;
  }
  const name = head.text.toLowerCase();
  if (UNSYNCED[name] !== undefined) return UNSYNCED[name];

  // The operator: a symbol, or Jira's words for one.
  let op: string;
  let rest: Token[];
  const next = tokens[1];
  if (next?.kind === "op") [op, rest] = [next.text, tokens.slice(2)];
  else if (isWord(next, "not") && isWord(tokens[2], "in")) [op, rest] = ["not in", tokens.slice(3)];
  else if (isWord(next, "is") && isWord(tokens[2], "not")) [op, rest] = ["is not", tokens.slice(3)];
  else if (next?.kind === "word") [op, rest] = [next.text.toLowerCase(), tokens.slice(2)];
  else return CANNOT_READ;

  if (op === "was" || op === "changed") return "knobas does not keep a ticket's history";
  if (op === "~" || op === "!~") return "a Jira text search, only Jira can answer it";
  const field = FIELDS[name];
  if (field === undefined) return `knobas has no condition for ${head.text}`;
  const values = valuesOf(rest);
  if (values === null) return CANNOT_READ;
  if (op === "is" || op === "is not" || values.some((v) => !v.fn && ["empty", "null"].includes(v.text.toLowerCase()))) {
    return "knobas has no condition for an empty field";
  }

  if (field === "updated") {
    const window = values.length === 1 && !values[0]!.fn ? /^-(\d+)([dw])$/i.exec(values[0]!.text) : null;
    if ((op === ">=" || op === ">") && window !== null) {
      return { field, negated: false, values: [`${window[1]}${window[2]!.toLowerCase()}`] };
    }
    return "knobas tests only how recently a ticket changed";
  }
  if (!["=", "!=", "in", "not in"].includes(op)) return CANNOT_READ;
  const negated = op === "!=" || op === "not in";

  if (field === "sprint") {
    if (!values.every((v) => v.fn && v.text === "opensprints")) return "knobas knows only the active sprint";
    return { field, negated, values: ["current"] };
  }
  if (field === "epic" && negated) return "knobas can only keep the tickets under an epic";
  const read: string[] = [];
  for (const value of values) {
    if (value.fn && field === "assignee" && value.text === "currentuser") read.push("me");
    else if (value.fn) return "a Jira function, only Jira can answer it";
    else read.push(value.text);
  }
  return { field, negated, values: read };
}

function translate(clauses: Clause[]): JiraCopy {
  const conditions: Condition[] = [];
  const left: LeftOut[] = [];
  let kept = 0;
  for (const clause of clauses) {
    let read = readClause(clause.tokens);
    if (typeof read !== "string") {
      const condition = read;
      const held = conditions.find((c) => c.field === condition.field && c.negated === condition.negated);
      if (held !== undefined && !condition.negated) read = "knobas reads two values of one field as either";
      else if (held !== undefined) {
        for (const value of condition.values) if (!held.values.includes(value)) held.values.push(value);
      } else conditions.push(condition);
    }
    if (typeof read === "string") left.push({ clause: clause.text, reason: read });
    else kept++;
  }
  return { query: conditionsText({ conditions, words: "" }), kept, left };
}

/** One JQL query read into conditions. */
export function translateJql(jql: string): JiraCopy {
  return translate(clausesOf(jql));
}

/**
 * A Jira view's query read into conditions: its JQL, the active sprint Jira
 * adds to a Scrum board's, and a Kanban board's sub-filter.
 */
export function translateJira(jira: JiraSource): JiraCopy {
  const sprint = jira.board_type?.toLowerCase() === "scrum" ? clausesOf("sprint in openSprints()") : [];
  return translate([...clausesOf(jira.jql), ...sprint, ...clausesOf(jira.sub_query ?? "")]);
}
