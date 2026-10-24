/**
 * The Proposed links view, pure (#835, R-6.3, R-6.4): what a proposal is
 * called, how strong it reads, which proposals the queue keeps, and which one
 * opens after a decision.
 *
 * A proposal is a `room_suggestions` row. The view holds the whole list (the
 * tray answers every waiting proposal under its limit), so the queue filters
 * in the browser (R-2.3's first mode).
 */
import type { LinkEnd, LinkRow, ProposalEvidence, SuggestionEntry, SuggestionPage } from "../../ipc/entity";
import { matchesAll } from "../filter/filter";
import { keyOf } from "../job/job";

/** The tray's read and the open proposal's evidence, injectable so a test needs no bridge. */
export interface ReviewPorts {
  roomSuggestions: (sources: string[], ctx: string | null, limit: number) => Promise<SuggestionPage>;
  proposalEvidence: (linkId: string) => Promise<ProposalEvidence>;
}

/** Prototype-6's three strengths, which its chips and dots draw. */
export type Strength = "strong" | "indirect" | "weak";

/** A decision: *Link them*, *Not related*, *Decide later*. */
export type Verdict = "yes" | "no" | "skip";

/** The decisions taken this session, by link id. */
export type Decided = Record<string, Verdict | undefined>;

/**
 * How strong a proposal reads, from the class of its evidence.
 *
 * An exact key (the two ends name each other) and a relation the source
 * already records are strong; shared wording is weak. *Indirect* is
 * prototype-6's evidence through another link, which no rule writes yet, so
 * nothing reads indirect today (a ruling of #835).
 */
export function strengthOf(link: Pick<LinkRow, "rule_class">): Strength {
  return link.rule_class === "similarity" ? "weak" : "strong";
}

/** Each strength as the view draws it: its word and its dot's tone, as prototype-6 does. */
export const STRENGTHS: Record<Strength, { label: string; tone: "ok" | "info" | "warn" }> = {
  strong: { label: "Strong", tone: "ok" },
  indirect: { label: "Indirect", tone: "info" },
  weak: { label: "Weak", tone: "warn" },
};

/** How many proposals the view asks the tray for: every one a mirror holds in practice. */
export const REVIEW_LIMIT = 500;

/**
 * What an end is called on its card: a ticket by its key and title, a commit
 * by its sha and quoted message (prototype-6's `a41f2c “PAY-231 backoff
 * jitter”`), anything else by its title.
 */
export function endName(end: LinkEnd): string {
  if (end.kind === "ticket") return `${keyOf(end.entity_id)} ${end.title}`;
  if (end.kind === "commit") return `${keyOf(end.entity_id)} “${end.title}”`;
  return end.title;
}

/**
 * The queue row's two halves: the item proposed, and what it would link to.
 * A ticket end is the target, as prototype-6's rows say "to PAY-231"; with no
 * ticket, or two, the link's own direction decides.
 */
export function targetOf(entry: SuggestionEntry): { item: LinkEnd; target: LinkEnd } {
  if (entry.from.kind === "ticket" && entry.to.kind !== "ticket") return { item: entry.to, target: entry.from };
  return { item: entry.from, target: entry.to };
}

/** How the target is named in a row: a ticket by its key, anything else by its name. */
function targetLabel(end: LinkEnd): string {
  return end.kind === "ticket" ? keyOf(end.entity_id) : endName(end);
}

/**
 * The ticket a proposal is about, for *about PAY-231*: its key to read and its
 * entity id to link by, the id naming the source (#1977). `null` when neither
 * end is a ticket.
 */
export function aboutTicket(entry: SuggestionEntry): { key: string; entityId: string } | null {
  const { target } = targetOf(entry);
  return target.kind === "ticket" ? { key: keyOf(target.entity_id), entityId: target.entity_id } : null;
}

const VERDICT_WORD: Record<Verdict, string> = { yes: "linked", no: "not related", skip: "later" };

/** A queue row's second line: "linked, strong, to PAY-231". */
export function rowNote(entry: SuggestionEntry, verdict: Verdict | undefined): string {
  const note = `${strengthOf(entry.link)}, to ${targetLabel(targetOf(entry).target)}`;
  return verdict ? `${VERDICT_WORD[verdict]}, ${note}` : note;
}

/** Whether the job's ticket is at one end. */
function onJob(entry: SuggestionEntry, job: string): boolean {
  return [entry.from, entry.to].some((end) => end.kind === "ticket" && keyOf(end.entity_id) === job);
}

/** What the queue's filter bar has set. */
export interface QueueFilter {
  /** *This job* or *All*. */
  scope: "job" | "all";
  /** The job's ticket key; *This job* keeps nothing without one. */
  job: string | null;
  /** The strength chips switched on; none keeps every strength. */
  strengths: Strength[];
  /** The *Decided* chip: decided proposals stay in the queue. */
  decided: boolean;
  text: string;
}

/**
 * The queue: the proposals the filter keeps, newest first. A decided one
 * leaves unless *Decided* is on or it is the one open (`addressed`), so the
 * reader still sees what they just judged.
 */
export function queue(
  entries: readonly SuggestionEntry[],
  decided: Decided,
  filter: QueueFilter,
  addressed: string | null,
): SuggestionEntry[] {
  return entries
    .filter((entry) => {
      const id = entry.link.id;
      if (filter.scope === "job" && (filter.job === null || !onJob(entry, filter.job))) return false;
      if (filter.strengths.length > 0 && !filter.strengths.includes(strengthOf(entry.link))) return false;
      if (!filter.decided && decided[id] !== undefined && id !== addressed) return false;
      return matchesAll(`${endName(entry.from)} ${endName(entry.to)} ${entry.link.reason ?? ""}`, filter.text);
    })
    .sort((a, b) => b.link.created_at.localeCompare(a.link.created_at));
}

/** The proposal open: the addressed one if the queue holds it, else the first undecided. */
export function currentOf(
  list: readonly SuggestionEntry[],
  decided: Decided,
  addressed: string | null,
): SuggestionEntry | null {
  return (
    list.find((entry) => entry.link.id === addressed) ??
    list.find((entry) => decided[entry.link.id] === undefined) ??
    null
  );
}

/** What opens after deciding `id`: the next undecided in the queue, from the top when none follows. */
export function nextAfter(list: readonly SuggestionEntry[], decided: Decided, id: string): SuggestionEntry | null {
  const at = list.findIndex((entry) => entry.link.id === id);
  const order = [...list.slice(at + 1), ...list.slice(0, Math.max(at, 0))];
  return order.find((entry) => entry.link.id !== id && decided[entry.link.id] === undefined) ?? null;
}

/**
 * The reason as one sentence. The detector writes it as a clause ("the
 * commit message mentions PAY-231"); the card reads it as prototype-6's claim.
 */
export function claim(reason: string): string {
  const text = reason.trim();
  if (text === "") return text;
  const capital = text.charAt(0).toUpperCase() + text.slice(1);
  return /[.!?]$/.test(capital) ? capital : `${capital}.`;
}

/** *Link them*'s toast: "Linked: 9f2c1ab implements PAY-231", each end by its first word. */
export function linkedToast(entry: SuggestionEntry, relation: string): string {
  const first = (end: LinkEnd) => endName(end).split(" ")[0];
  return `Linked: ${first(entry.from)} ${relation} ${first(entry.to)}`;
}

/** A relation the dropdown offers: what `create_link` stores, and what is shown. */
export interface RelationOption {
  id: string;
  label: string;
}

/** Prototype-6's relations, in its order, as `create_link` stores them. */
const OFFERED: RelationOption[] = [
  { id: "implements", label: "implements" },
  { id: "deploys", label: "deploys" },
  { id: "documents", label: "documents" },
  { id: "mentions", label: "mentions" },
  { id: "related", label: "related" },
  { id: "blocks", label: "blocks" },
  { id: "depends-on", label: "depends on" },
];

/** The dropdown's options; a relation the proposal carries that the list lacks goes first. */
export function relationOptions(current: string): RelationOption[] {
  return OFFERED.some((option) => option.id === current) ? OFFERED : [{ id: current, label: current }, ...OFFERED];
}

/** How a relation id reads. */
export function relationLabel(id: string): string {
  return OFFERED.find((option) => option.id === id)?.label ?? id;
}

/**
 * How the excerpts sit (#836): side by side when they quote both ends, as
 * prototype-6 draws shared wording; stacked when they quote one.
 */
export function sideBySide(evidence: Pick<ProposalEvidence, "excerpts">): boolean {
  return new Set(evidence.excerpts.map((excerpt) => excerpt.entity_id)).size > 1;
}

/**
 * Whether an excerpt quotes a code value (a branch name), set in mono. A
 * commit message is prose: the design system keeps mono for real code values,
 * and it decides where prototype-6 sets a commit's quote in mono.
 */
export function quotesCode(entry: SuggestionEntry, entityId: string): boolean {
  const end = [entry.from, entry.to].find((candidate) => candidate.entity_id === entityId);
  return end?.kind === "branch";
}
