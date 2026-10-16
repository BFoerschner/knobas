/**
 * Link sets' words and rules (#866, R-7.6, R-7.7), pure: what a rule is
 * called, what a set holds per kind, and which sets the filter keeps.
 *
 * `TfSets.svelte` draws `#/sets` and `TfSetEditor.svelte` `#/sets/<id>` from
 * these; the backend (`knobas_app::link_sets`) is the one that matches rules.
 */
import { keyOf } from "../job/job";
import type { LinkSetMemberKind, LinkSetRow, LinkSetRuleKind, LinkSetRuleRow } from "../../ipc/entity";
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

/** *What is in it*: one line per kind that holds anything, "2 repositories". */
export function contentLines(set: Pick<LinkSetRow, "members">): string[] {
  return MEMBER_KINDS.map((kind) => [kind, set.members.filter((member) => member.kind === kind).length] as const)
    .filter(([, count]) => count > 0)
    .map(([kind, count]) => `${count} ${KIND_LABEL[kind].toLowerCase()}`);
}

/** The sets the filter keeps: every word in the name, description or a member, and any chip's rule kind. */
export function visibleSets(sets: readonly LinkSetRow[], text: string, by: readonly string[]): LinkSetRow[] {
  return sets.filter(
    (set) =>
      (by.length === 0 || set.rules.some((rule) => by.includes(rule.kind))) &&
      matchesAll([set.name, set.description, ...set.members.map((member) => member.title ?? member.entity_id)].join(" "), text),
  );
}

/** How many of a list the editor shows before *and N more* (R-7.7). */
export const SHOWN_KEYS = 40;
