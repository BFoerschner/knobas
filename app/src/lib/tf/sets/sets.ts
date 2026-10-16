/**
 * Link sets' words and rules (#866, R-7.6, R-7.7), pure: what a rule is
 * called, what a set holds per kind, and which sets the filter keeps.
 *
 * `TfSets.svelte` draws `#/sets` and `TfSetEditor.svelte` `#/sets/<id>` from
 * these; the backend (`knobas_app::link_sets`) is the one that matches rules.
 */
import { keyOf } from "../job/job";
import type {
  LinkSetMemberKind,
  LinkSetMemberRow,
  LinkSetRow,
  LinkSetRuleKind,
  LinkSetRuleRow,
  StateChip,
} from "../../ipc/entity";
import { matchesAll } from "../filter/filter";

/** The editor's panels, in prototype-6's order (R-7.7). */
export const MEMBER_KINDS: readonly LinkSetMemberKind[] = ["repo", "pipeline", "page", "asset", "monitor"];

/**
 * Each panel's heading. Prototype-6 says *Build configurations*, TeamCity's
 * word; `CONTEXT.md` says **pipeline** and keeps a source's own word to the
 * Builds view's header, so the panel says *Pipelines* (ruled in the PR).
 */
export const KIND_LABEL: Record<LinkSetMemberKind, string> = {
  repo: "Repositories",
  pipeline: "Pipelines",
  page: "Pages",
  asset: "Assets",
  monitor: "Monitors",
};

/** The rule kinds as the *Applies to* dropdown names them (prototype-6's `ruleRow`). */
export const RULE_KINDS: readonly { kind: LinkSetRuleKind; label: string }[] = [
  { kind: "epic", label: "Every ticket under epic" },
  { kind: "parent", label: "Every sub-task of" },
  { kind: "project", label: "Every ticket in project" },
  { kind: "manual", label: "One ticket, by hand" },
];

/** The *Applies by* chips (R-7.6), each a rule kind. */
export const APPLIES_BY: readonly { value: LinkSetRuleKind; label: string }[] = [
  { value: "epic", label: "Epic" },
  { value: "parent", label: "Parent" },
  { value: "project", label: "Project" },
  { value: "manual", label: "By hand" },
];

/**
 * One line per rule, in prototype-6's words (`ruleLabel`): *Every ticket
 * under epic PAY-200 Payout reliability*, *Every ticket in project OPS*,
 * *Every sub-task of PAY-231*, *PAY-231, applied by hand*. The epic's title
 * follows its key only when the source gave it one of its own.
 */
export function ruleLabel(rule: LinkSetRuleRow): string {
  // The value is an entity id or `<source>:<project key>`: either way, its key half.
  const key = keyOf(rule.value);
  switch (rule.kind) {
    case "epic": {
      const title = rule.title && rule.title !== key ? ` ${rule.title}` : "";
      return `Every ticket under epic ${key}${title}`;
    }
    case "project":
      return `Every ticket in project ${key}`;
    case "parent":
      return `Every sub-task of ${key}`;
    case "manual":
      return `${key}, applied by hand`;
  }
}

/**
 * Everything a set holds now: its fixed members, then what each pattern
 * matches (#868), each item once (prototype-6's `setItems`).
 */
export function itemsOf(set: Pick<LinkSetRow, "members" | "patterns">): LinkSetMemberRow[] {
  const seen = new Set<string>();
  return [...set.members, ...set.patterns.flatMap((pattern) => pattern.matches)].filter((item) => {
    if (seen.has(item.entity_id)) return false;
    seen.add(item.entity_id);
    return true;
  });
}

/** *What is in it*: one line per kind that holds anything, "2 repositories", patterns' matches counted. */
export function contentLines(set: Pick<LinkSetRow, "members" | "patterns">): string[] {
  const items = itemsOf(set);
  return MEMBER_KINDS.map((kind) => [kind, items.filter((member) => member.kind === kind).length] as const)
    .filter(([, count]) => count > 0)
    .map(([kind, count]) => `${count} ${KIND_LABEL[kind].toLowerCase()}`);
}

/** The sets the filter keeps: every word in the name, description or an item, and any chip's rule kind. */
export function visibleSets(sets: readonly LinkSetRow[], text: string, by: readonly string[]): LinkSetRow[] {
  return sets.filter(
    (set) =>
      (by.length === 0 || set.rules.some((rule) => by.includes(rule.kind))) &&
      matchesAll([set.name, set.description, ...itemsOf(set).map((member) => member.title ?? member.entity_id)].join(" "), text),
  );
}

/** The member kinds with *Add a pattern…* (R-7.7: builds and pages). */
export const PATTERN_KINDS: readonly LinkSetMemberKind[] = ["pipeline", "page"];

/** A value as the grammar takes it: quoted when it holds a space or a quote-free colon. */
function grammarValue(value: string): string {
  return /[\s:]/.test(value) ? `"${value.replace(/"/g, "")}"` : value;
}

/**
 * The query the pattern dialog's fields stand for (#868), in the search
 * grammar the backend reads it with: a pipeline's `project:` (or none for
 * *Any*) and `name:` glob; a page's `project:` (its space) and `under:` page.
 * The backend parses it (ruling P2); this only writes it.
 */
export function patternQuery(kind: LinkSetMemberKind, project: string, text: string): string {
  const parts: string[] = [];
  const typed = text.trim();
  if (project) parts.push(`project:${grammarValue(project)}`);
  if (kind === "pipeline") parts.push(`name:${grammarValue(typed || "*")}`);
  else if (typed) parts.push(`under:${grammarValue(typed)}`);
  return parts.join(" ");
}

/** A dot's tone class from a member row's state (#1008); none draws the muted dot. */
export function dotClass(state: StateChip | null): string {
  return state ? `tf-${state.tone}` : "";
}

/** How many of a list the editor shows before *and N more* (R-7.7). */
export const SHOWN_KEYS = 40;
