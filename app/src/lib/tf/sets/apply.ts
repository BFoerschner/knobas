/**
 * *Apply a link set…* and *Save as a link set…* (#869, R-7.8, R-7.9), pure:
 * the drafts the two dialogs save through `save_link_set`, what *Save as a
 * link set…* offers from the ticket's own links, and their toasts.
 *
 * Applying by hand is one more `manual` rule on the set (R-7.8); *Undo* saves
 * the set without it. Saving builds a new set with one rule, the ticket's
 * epic, its sub-tasks, its project or the ticket by hand (R-7.9).
 */
import type { EntityRow, LinkEnd, LinkSetDraft, LinkSetMemberKind, LinkSetRow, LinkSetRule } from "../../ipc/entity";
import { itemsOf, ruleLabel } from "./sets";

/** A set as `save_link_set` takes it back: members, patterns and rules in their order. */
export function draftOf(set: Pick<LinkSetRow, "id" | "name" | "description" | "members" | "patterns" | "rules">): LinkSetDraft {
  return {
    id: set.id,
    name: set.name,
    description: set.description,
    members: set.members.map(({ kind, entity_id }) => ({ kind, entity_id })),
    patterns: set.patterns.map(({ kind, query }) => ({ kind, query })),
    rules: set.rules.map(({ kind, value }) => ({ kind, value })),
  };
}

/** The set with a by-hand rule for `ticketId` after its own (R-7.8). */
export function withHandRule(set: LinkSetRow, ticketId: string): LinkSetDraft {
  const draft = draftOf(set);
  draft.rules.push({ kind: "manual", value: ticketId });
  return draft;
}

/** *Undo*: the set without the by-hand rule for `ticketId`, every other rule kept. */
export function withoutHandRule(set: LinkSetRow, ticketId: string): LinkSetDraft {
  const draft = draftOf(set);
  draft.rules = draft.rules.filter((rule) => !(rule.kind === "manual" && rule.value === ticketId));
  return draft;
}

/**
 * Prototype-6's `applySet` toast. It counts every item the set holds, its
 * patterns' matches too; prototype-6 leaves assets out of the count, which
 * reads as a slip, since the job shows them (ruled in the PR).
 */
export function applyToast(set: Pick<LinkSetRow, "name" | "members" | "patterns">, key: string): string {
  return `${set.name} applied to ${key}. ${itemsOf(set).length} items show on the job.`;
}

/** Prototype-6's `saveSet` toast: the set's name and the rule that now shows it. */
export function savedToast(set: Pick<LinkSetRow, "name" | "rules">): string {
  const rule = set.rules[0];
  return `Saved ${set.name}.${rule ? ` ${ruleLabel(rule)} now shows it.` : ""}`;
}

/** One item *Save as a link set…* offers, ticked. */
export interface Offer {
  kind: LinkSetMemberKind;
  entity_id: string;
  title: string;
}

/** What is never a pipeline: `knobas_core::link_set::NOT_PIPELINE_KINDS`. */
const NOT_PIPELINE = new Set(["repo", "page", "asset", "monitor", "ticket", "pr", "branch", "commit", "build", "ctx", "note", "route"]);

/** The code kinds that belong to one ticket and offer their repository instead. */
const CODE = new Set(["pr", "branch", "commit"]);

/**
 * The repository a pull request, branch or commit sits in, from its id: Gitea
 * keys them `owner/repo#142`, `owner/repo@refs/heads/…` and `owner/repo@sha`
 * under the repository's own `owner/repo` (`knobas_source_gitea::keys`).
 */
function repoOf(entityId: string): Offer | null {
  const colon = entityId.indexOf(":");
  const key = entityId.slice(colon + 1);
  const cut = key.search(/[#@]/);
  if (colon <= 0 || cut <= 0) return null;
  const name = key.slice(0, cut);
  return { kind: "repo", entity_id: `${entityId.slice(0, colon)}:${name}`, title: name };
}

/** The kind an end can be a member as, or `null`. */
function memberKind(kind: string): LinkSetMemberKind | null {
  if (kind === "repo" || kind === "page" || kind === "asset" || kind === "monitor") return kind;
  return NOT_PIPELINE.has(kind) ? null : "pipeline";
}

/** Prototype-6's order: repositories, pipelines, pages, monitors, assets. */
const ORDER: readonly LinkSetMemberKind[] = ["repo", "pipeline", "page", "monitor", "asset"];

/**
 * What *Save as a link set…* offers (R-7.9): the ticket's own repositories,
 * pipelines, pages, monitors and assets, each once. A pull request, branch or
 * commit belongs to this one ticket, so it offers its repository instead; a
 * build run and another ticket are not offered.
 */
export function offered(links: readonly LinkEnd[]): Offer[] {
  const out = new Map<string, Offer>();
  for (const end of links) {
    const kind = memberKind(end.kind);
    if (kind !== null) out.set(end.entity_id, { kind, entity_id: end.entity_id, title: end.title });
  }
  for (const end of links) {
    const repo = CODE.has(end.kind) ? repoOf(end.entity_id) : null;
    if (repo !== null && !out.has(repo.entity_id)) out.set(repo.entity_id, repo);
  }
  return ORDER.flatMap((kind) => [...out.values()].filter((offer) => offer.kind === kind));
}

/**
 * The epic a ticket sits under: the top of its parent chain (its path line,
 * `PAY-200 › PAY-231`), in its own source. `null` with no parent.
 */
export function epicOf(row: Pick<EntityRow, "entity_id" | "source_id" | "path">): string | null {
  const top = row.path?.split(" › ")[0]?.trim();
  return top ? `${row.source_id}:${top}` : null;
}

/** The project rule's value, `<source>:<key prefix>`; `null` for a key with no project part. */
export function projectOf(row: Pick<EntityRow, "entity_id" | "source_id">): string | null {
  const key = row.entity_id.slice(row.entity_id.indexOf(":") + 1);
  const dash = key.lastIndexOf("-");
  return dash > 0 ? `${row.source_id}:${key.slice(0, dash)}` : null;
}

/** The new set: the ticked items in order, one rule, "Saved from KEY." as prototype-6. */
export function savedDraft(name: string, key: string, picked: readonly Offer[], rule: LinkSetRule): LinkSetDraft {
  return {
    id: null,
    name,
    description: `Saved from ${key}.`,
    members: picked.map(({ kind, entity_id }) => ({ kind, entity_id })),
    patterns: [],
    rules: [rule],
  };
}

/** The name prototype-6 suggests: the summary's last two words, then "kit". */
export function savedName(summary: string): string {
  return `${summary.trim().split(/\s+/).slice(-2).join(" ")} kit`;
}
